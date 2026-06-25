use std::sync::Arc;

use diesel_async::{
    AsyncPgConnection,
    pooled_connection::{
        self,
        deadpool::{Object, Pool},
    },
};
use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};

use crate::{
    config::Database,
    crypto::encryption_manager::encryption_interface::Encryption,
    error::{self, ContainerError},
};

#[cfg(feature = "caching")]
pub mod caching;
pub mod consts;
pub mod db;
#[cfg(feature = "kv")]
pub mod kv;
#[cfg(feature = "redis")]
pub mod redis;
pub mod schema;
pub mod scheme;
pub mod storage_v2;
pub mod types;
pub mod utils;

pub use scheme::StorageScheme;

pub trait State {}

/// Runtime config for read replica routing.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct ReplicaRouting {
    #[serde(default)]
    use_replica: bool,
}

impl crate::runtime_config::RuntimeConfigItem for ReplicaRouting {
    const KEY: &'static str = "locker.use_read_replica";
}

/// Runtime-config payload for the KV master switch (`locker.enable_kv`).
///
/// Per-field `#[serde(default)]` is REQUIRED (not the struct-level `Default`): it
/// lets a partial endpoint payload like `{"enable_kv": true}` deserialize with
/// `soft_kill = false`, instead of erroring on the missing field.
#[cfg(feature = "kv")]
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct KvRuntimeConfig {
    #[serde(default)]
    enable_kv: bool,
    #[serde(default)]
    soft_kill: bool,
}

#[cfg(feature = "kv")]
impl crate::runtime_config::RuntimeConfigItem for KvRuntimeConfig {
    const KEY: &'static str = "locker.enable_kv";
}

/// Pure mapping from the runtime-config payload to [`kv::TableKvSettings`].
///
/// Extracted so the truth table (see `kv_runtime_config_maps_to_settings` test)
/// is unit-testable without a live [`Storage`] or network.  Note: under
/// soft-kill, [`kv::decide_storage_scheme`] ignores `storage_scheme` and routes
/// by op (Insert→Postgres, Find→Redis, Update→Redis-probe).  `soft_kill` is
/// therefore gated on `enable_kv` so an `enable_kv=false, soft_kill=true`
/// payload can never route Finds to Redis.  DO NOT "simplify" the `&&` away.
#[cfg(feature = "kv")]
impl From<KvRuntimeConfig> for kv::TableKvSettings {
    fn from(c: KvRuntimeConfig) -> Self {
        Self {
            storage_scheme: if c.enable_kv {
                kv::StorageScheme::RedisKv
            } else {
                kv::StorageScheme::PostgresOnly
            },
            soft_kill: c.enable_kv && c.soft_kill,
        }
    }
}

/// Storage State that is to be passed though the application
#[derive(Clone)]
pub struct Storage {
    primary_pg_pool: Arc<Pool<AsyncPgConnection>>,
    replica_pg_pool: Option<Arc<Pool<AsyncPgConnection>>>,
    runtime_config_manager: Arc<crate::runtime_config::RuntimeConfigManager>,
    #[cfg(feature = "kv")]
    redis: Option<redis_store::RedisStore>,
    #[cfg(feature = "kv")]
    kv_config: crate::config::KvConfig,
    #[cfg(feature = "kv")]
    request_id: Option<String>,
}

#[cfg(feature = "redis")]
use crate::storage::redis as redis_store;

type DeadPoolConnType = Object<AsyncPgConnection>;

impl Storage {
    fn create_database_connection_pool(
        database_config: &Database,
        schema: &str,
    ) -> error_stack::Result<Pool<AsyncPgConnection>, error::StorageError> {
        let database_url = format!(
            "postgres://{}:{}@{}:{}/{}?application_name={}&options=-c search_path%3D{}",
            database_config.username,
            database_config.password.peek(),
            database_config.host,
            database_config.port,
            database_config.dbname,
            schema,
            schema
        );

        let config =
            pooled_connection::AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
        let pool = Pool::builder(config);

        let pool = match database_config.pool_size {
            Some(value) => pool.max_size(value),
            None => pool,
        };

        pool.build()
            .change_context(error::StorageError::DBPoolError)
    }

    /// Create a new storage interface from configuration
    pub async fn new(
        primary_config: &Database,
        replica_config: Option<&Database>,
        schema: &str,
        runtime_config_manager: Arc<crate::runtime_config::RuntimeConfigManager>,
        #[cfg(feature = "kv")] redis: Option<redis_store::RedisStore>,
        #[cfg(feature = "kv")] kv_config: &crate::config::KvConfig,
    ) -> error_stack::Result<Self, error::StorageError> {
        let pg_pool = Arc::new(Self::create_database_connection_pool(
            primary_config,
            schema,
        )?);

        let replica_pool = match replica_config {
            Some(config) => Some(Arc::new(Self::create_database_connection_pool(
                config, schema,
            )?)),
            None => None,
        };

        Ok(Self {
            primary_pg_pool: pg_pool,
            replica_pg_pool: replica_pool,
            runtime_config_manager,
            #[cfg(feature = "kv")]
            redis,
            #[cfg(feature = "kv")]
            kv_config: kv_config.clone(),
            #[cfg(feature = "kv")]
            request_id: None,
        })
    }

    /// Get connection from database pool for accessing data
    pub async fn get_conn(&self) -> Result<DeadPoolConnType, ContainerError<error::StorageError>> {
        Ok(self
            .primary_pg_pool
            .get()
            .await
            .change_context(error::StorageError::PoolClientFailure)?)
    }

    /// Get a connection from the read replica pool, if configured.
    /// Returns `ReplicaPoolNotConfigured` error if no replica pool was initialized.
    pub async fn get_replica_conn(
        &self,
    ) -> Result<DeadPoolConnType, ContainerError<error::StorageError>> {
        Ok(self
            .replica_pg_pool
            .as_ref()
            .ok_or(ContainerError::from(
                error::StorageError::ReplicaPoolNotConfigured,
            ))?
            .get()
            .await
            .change_context(error::StorageError::PoolClientFailure)?)
    }

    /// Returns `true` if a read replica pool was configured and initialized.
    pub fn has_replica(&self) -> bool {
        self.replica_pg_pool.is_some()
    }

    /// Returns `true` when both a replica pool exists and runtime config allows replica reads.
    async fn should_use_replica(&self) -> bool {
        if !self.has_replica() {
            crate::logger::debug!("No replica pool configured");
            return false;
        }

        self.runtime_config_manager
            .get::<ReplicaRouting>()
            .await
            .map(|config| config.use_replica)
            .unwrap_or(false)
    }

    /// Returns a connection from the replica pool when the runtime config enables it,
    /// otherwise returns a primary pool connection.
    pub async fn route_conn(
        &self,
    ) -> Result<DeadPoolConnType, ContainerError<error::StorageError>> {
        if self.should_use_replica().await {
            crate::logger::debug!("Routing to read replica");
            self.get_replica_conn().await
        } else {
            crate::logger::debug!("Routing to primary pool");
            self.get_conn().await
        }
    }

    /// Resolve the global KV settings from runtime config (`locker.enable_kv`,
    /// TTL-cached).
    ///
    /// Performs an async runtime-config lookup — this is NOT a free getter.
    ///
    /// **Resolution order:**
    /// 1. If runtime config is enabled, fetch `locker.enable_kv` from the
    ///    endpoint (TTL-cached). Fail-closed on errors.
    /// 2. If runtime config is disabled, fall back to the file-based
    ///    `[kv] enable_kv` / `soft_kill` values from the TOML config.
    /// 3. If neither source provides a value, return `PostgresOnly` with
    ///    `soft_kill=false`.
    #[cfg(feature = "kv")]
    pub(crate) async fn kv_settings(&self) -> kv::TableKvSettings {
        // Try runtime config endpoint first.
        if let Some(config) = self
            .runtime_config_manager
            .get::<KvRuntimeConfig>()
            .await
        {
            let settings = kv::TableKvSettings::from(config);
            crate::logger::info!(
                source = "runtime_config_endpoint",
                enable_kv = %settings.storage_scheme,
                soft_kill = %settings.soft_kill,
                "KV settings resolved from runtime config endpoint"
            );
            return settings;
        }

        // Runtime config disabled or unreachable — fall back to file-based
        // `[kv] enable_kv` / `soft_kill`.
        let settings = kv::TableKvSettings::from(KvRuntimeConfig {
            enable_kv: self.kv_config.enable_kv,
            soft_kill: self.kv_config.soft_kill,
        });
        crate::logger::info!(
            source = "toml_fallback",
            enable_kv = %self.kv_config.enable_kv,
            soft_kill = %self.kv_config.soft_kill,
            resolved_scheme = %settings.storage_scheme,
            "KV settings resolved from TOML fallback (runtime config disabled or unreachable)"
        );
        settings
    }

    /// Set the request ID for KV drainer stream entries.
    ///
    /// Called by [`TenantStateResolver`](crate::custom_extractors::TenantStateResolver)
    /// after cloning the per-tenant state so that each request carries its
    /// own `x-request-id` into the drainer.
    #[cfg(feature = "kv")]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

#[cfg(feature = "kv")]
impl kv::RedisConnInterface for Storage {
    fn get_redis_conn(
        &self,
    ) -> error_stack::Result<std::sync::Arc<hyperswitch_redis_interface::RedisConnectionPool>, hyperswitch_redis_interface::errors::RedisError> {
        self.redis
            .as_ref()
            .map(|r| r.get_redis_conn())
            .ok_or_else(|| error_stack::Report::new(hyperswitch_redis_interface::errors::RedisError::RedisConnectionError))
    }
}

#[cfg(feature = "kv")]
impl kv::KvStoreContext for Storage {
    fn ttl_for_kv(&self) -> u32 {
        self.kv_config.ttl_for_kv
    }

    fn drainer_stream_name(&self, shard_key: &str) -> String {
        self.kv_config.drainer_stream_name(shard_key)
    }

    fn drainer_num_partitions(&self) -> u8 {
        self.kv_config.drainer_num_partitions
    }

    fn request_id(&self) -> &str {
        self.request_id.as_deref().unwrap_or("")
    }
}

#[cfg(feature = "caching")]
pub trait Cacheable<Table> {
    type Key: std::hash::Hash + Eq + PartialEq + Send + Sync + 'static;
    type Value: Clone + Send + Sync + 'static;
}

#[cfg(feature = "caching")]
impl Cacheable<types::Merchant> for Storage {
    type Key = String;
    type Value = types::Merchant;
}

#[cfg(feature = "caching")]
impl Cacheable<types::HashTable> for Storage {
    type Key = Vec<u8>;
    type Value = types::HashTable;
}

#[cfg(feature = "caching")]
impl Cacheable<types::Fingerprint> for Storage {
    type Key = Vec<u8>;
    type Value = types::Fingerprint;
}

#[cfg(all(feature = "caching", feature = "external_key_manager"))]
impl Cacheable<types::Entity> for Storage {
    type Key = String;
    type Value = types::Entity;
}

///
/// MerchantInterface:
///
/// Interface providing functional to interface with the merchant table in database
#[deprecated(
    since = "1.0.0",
    note = "separate encryption service is being used to store DEK"
)]
pub(crate) trait MerchantInterface {
    type Algorithm: Encryption<Vec<u8>, Vec<u8>> + Sync;
    type Error;

    /// find merchant from merchant table with `merchant_id` with key as master key
    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>>;

    /// find merchant from merchant table with `merchant_id` with key as master key
    /// and if not found create a new merchant
    async fn find_or_create_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>>;

    /// Insert a new merchant in the database by encrypting the dek with `master_key`
    async fn insert_merchant(
        &self,
        new: types::MerchantNew<'_>,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>>;

    // This function is under the `dead_code` lint to pass Clippy checks because it utilizes types
    // from both internal and external key_manager.
    #[allow(dead_code)]
    async fn find_all_keys_excluding_entity_keys(
        &self,
        key: &Self::Algorithm,
        limit: i64,
    ) -> Result<Vec<types::Merchant>, ContainerError<Self::Error>>;
}

///
/// LockerInterface:
///
/// Interface for interacting with the locker database table
pub(crate) trait LockerInterface {
    type Error;

    /// Fetch payment data from locker table by decrypting with `dek`
    async fn find_by_locker_id_merchant_id_customer_id(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<types::Locker, ContainerError<Self::Error>>;

    /// Insert payment data from locker table by decrypting with `dek`
    async fn insert_or_get_from_locker(
        &self,
        new: types::LockerNew<'_>,
    ) -> Result<types::Locker, ContainerError<Self::Error>>;

    /// Delete card from the locker, without access to the `dek`
    async fn delete_from_locker(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>>;

    async fn find_by_hash_id_merchant_id_customer_id(
        &self,
        hash_id: &str,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<Option<types::Locker>, ContainerError<Self::Error>>;
}

/// Trait defining behaviour of the application with the hash table, providing APIs to interact
/// with it
#[deprecated(
    since = "1.0.0",
    note = "duplication of data should now be handled on the client side"
)]
pub(crate) trait HashInterface {
    type Error;

    async fn find_by_data_hash(
        &self,
        data_hash: &[u8],
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>>;
    async fn insert_hash(
        &self,
        data_hash: Vec<u8>,
    ) -> Result<types::HashTable, ContainerError<Self::Error>>;
}

pub(crate) trait TestInterface {
    type Error;
    async fn test(&self) -> Result<(), ContainerError<Self::Error>>;
    async fn test_replica(&self) -> Result<(), ContainerError<Self::Error>>;
}

///
/// Fingerprint:
///
/// Interface providing functions to interface with the fingerprint table in database
pub(crate) trait FingerprintInterface {
    type Error;

    async fn find_by_fingerprint_hash(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>>;

    async fn get_or_insert_fingerprint(
        &self,
        data: Secret<String>,
        key: Secret<String>,
        fingerprint_id: Option<Secret<String>>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>>;
}

///
/// EntityInterface:
///
/// Interface providing functionality to interface with the entity table in database
#[cfg(feature = "external_key_manager")]
pub(crate) trait EntityInterface {
    type Error;

    /// find merchant from merchant table with `merchant_id` with key as master key
    async fn find_by_entity_id(
        &self,
        entity_id: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>>;

    /// Insert a new merchant in the database by encrypting the dek with `master_key`
    async fn insert_entity(
        &self,
        entity_id: &str,
        identifier: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>>;
}

#[cfg(all(test, feature = "kv"))]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Truth table for `From<KvRuntimeConfig> for kv::TableKvSettings`.
    ///
    /// `enable_kv=false` must always yield `PostgresOnly` with an *effective*
    /// `soft_kill=false`, even when the payload requests `soft_kill=true`
    /// (KV-off can never leak reads to Redis).
    #[test]
    fn kv_runtime_config_maps_to_settings() {
        use kv::{StorageScheme::*, TableKvSettings};
        let cases = [
            // (enable_kv, soft_kill) -> (scheme, eff_soft_kill)
            ((false, false), (PostgresOnly, false)),
            ((false, true), (PostgresOnly, false)), // gated off
            ((true, false), (RedisKv, false)),
            ((true, true), (RedisKv, true)),
        ];
        for ((enable_kv, soft_kill), (scheme, eff)) in cases {
            let got = TableKvSettings::from(KvRuntimeConfig { enable_kv, soft_kill });
            assert_eq!(
                got,
                TableKvSettings {
                    storage_scheme: scheme,
                    soft_kill: eff,
                },
                "enable_kv={enable_kv}, soft_kill={soft_kill}"
            );
        }
    }

    /// Wire-contract test: the inner JSON value string returned by the runtime
    /// config endpoint must deserialize into `KvRuntimeConfig`, including the
    /// partial-payload (missing `soft_kill`) and empty-object cases.
    #[test]
    fn kv_runtime_config_deserializes() {
        let full: KvRuntimeConfig =
            serde_json::from_str(r#"{"enable_kv":true,"soft_kill":false}"#).unwrap();
        assert!(full.enable_kv && !full.soft_kill);

        let partial: KvRuntimeConfig = serde_json::from_str(r#"{"enable_kv":true}"#).unwrap();
        assert!(partial.enable_kv && !partial.soft_kill); // missing field defaults false

        let empty: KvRuntimeConfig = serde_json::from_str("{}").unwrap();
        assert!(!empty.enable_kv && !empty.soft_kill); // fail-closed defaults
    }
}
