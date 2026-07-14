use std::sync::Arc;

#[cfg(feature = "kv")]
use diesel::{
    PgConnection,
    r2d2::{ConnectionManager, Pool as SyncPool},
};
use diesel_async::{
    AsyncPgConnection,
    pooled_connection::{
        self,
        deadpool::{Object, Pool},
    },
};
use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};

#[cfg(feature = "kv")]
use crate::error::ErrorTransform;
#[cfg(feature = "kv")]
use crate::storage::redis as redis_store;
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

/// All runtime configs, deserialized directly from the config endpoint's JSON body. Field names
/// match the keys the endpoint returns; each `#[serde(default)]` field fails closed when absent.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct RuntimeConfigValues {
    #[cfg(feature = "kv")]
    #[serde(default)]
    enable_kv: kv::KvState,
    #[serde(default)]
    use_replica: bool,
}

/// Storage State that is to be passed though the application
#[derive(Clone)]
pub struct Storage {
    primary_pg_pool: Arc<Pool<AsyncPgConnection>>,
    replica_pg_pool: Option<Arc<Pool<AsyncPgConnection>>>,
    #[cfg(feature = "kv")]
    sync_pg_pool: Arc<SyncPgPoolType>,
    runtime_config_manager: Arc<crate::runtime_config::RuntimeConfigManager>,
    #[cfg(feature = "kv")]
    redis: Option<redis_store::RedisStore>,
    #[cfg(feature = "kv")]
    kv_config: crate::config::KvConfig,
}

type DeadPoolConnType = Object<AsyncPgConnection>;
#[cfg(feature = "kv")]
type SyncPgPoolType = SyncPool<ConnectionManager<PgConnection>>;

impl Storage {
    fn database_url(database_config: &Database, schema: &str) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}?application_name={}&options=-c search_path%3D{}",
            database_config.username,
            database_config.password.peek(),
            database_config.host,
            database_config.port,
            database_config.dbname,
            schema,
            schema
        )
    }

    fn create_database_connection_pool(
        database_config: &Database,
        schema: &str,
    ) -> error_stack::Result<Pool<AsyncPgConnection>, error::StorageError> {
        let database_url = Self::database_url(database_config, schema);

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

    #[cfg(feature = "kv")]
    fn create_sync_database_connection_pool(
        database_config: &Database,
        schema: &str,
    ) -> error_stack::Result<SyncPgPoolType, error::StorageError> {
        let database_url = Self::database_url(database_config, schema);
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        let pool = SyncPool::builder();

        let pool = match database_config.pool_size {
            Some(value) => pool.max_size(u32::try_from(value).map_err(|_| {
                error_stack::Report::new(error::StorageError::DBPoolError)
                    .attach_printable("database.pool_size exceeds u32::MAX")
            })?),
            None => pool,
        };

        pool.build(manager)
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
        #[cfg(feature = "kv")]
        let sync_pg_pool = Arc::new(Self::create_sync_database_connection_pool(
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
            #[cfg(feature = "kv")]
            sync_pg_pool,
            runtime_config_manager,
            #[cfg(feature = "kv")]
            redis,
            #[cfg(feature = "kv")]
            kv_config: kv_config.clone(),
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

    #[cfg(feature = "kv")]
    pub(crate) async fn with_sync_conn<T, E, F>(&self, f: F) -> Result<T, ContainerError<E>>
    where
        T: Send + 'static,
        E: error_stack::Context + Send + Sync + 'static + for<'a> From<&'a error::StorageError>,
        F: FnOnce(&mut PgConnection) -> Result<T, ContainerError<E>> + Send + 'static,
        ContainerError<E>: From<ContainerError<error::StorageError>>
            + ErrorTransform<ContainerError<error::StorageError>>,
    {
        let pool = self.sync_pg_pool.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = pool
                .get()
                .change_context(error::StorageError::PoolClientFailure)
                .map_err(ContainerError::<error::StorageError>::from)
                .map_err(ContainerError::<E>::from)?;

            f(&mut conn)
        })
        .await
        .map_err(|error| {
            let storage_error = ContainerError::<error::StorageError>::from(
                error_stack::Report::new(error::StorageError::PoolClientFailure)
                    .attach_printable(format!("Failed to join sync database task: {error}")),
            );
            ContainerError::<E>::from(storage_error)
        })?
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
            .get::<RuntimeConfigValues>()
            .await
            .is_some_and(|runtime_conf| runtime_conf.use_replica)
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

    /// Resolve `KvState` from runtime config; fail-closed to `Disabled` when absent.
    #[cfg(feature = "kv")]
    pub(crate) async fn kv_settings(&self) -> kv::KvState {
        self.runtime_config_manager
            .get::<RuntimeConfigValues>()
            .await
            .map(|runtime_config_values| runtime_config_values.enable_kv)
            .unwrap_or(kv::KvState::Disabled)
    }
}

#[cfg(feature = "kv")]
impl kv::RedisConnInterface for Storage {
    fn get_redis_conn(
        &self,
    ) -> error_stack::Result<
        std::sync::Arc<hyperswitch_redis_interface::RedisConnectionPool>,
        hyperswitch_redis_interface::errors::RedisError,
    > {
        self.redis
            .as_ref()
            .map(|r| r.get_redis_conn())
            .ok_or_else(|| {
                error_stack::Report::new(
                    hyperswitch_redis_interface::errors::RedisError::RedisConnectionError,
                )
            })
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

    /// Read a merchant by `merchant_id`, decrypting the stored DEK with `key`. A missing row
    /// surfaces as `Error::is_not_found()` (matching the KV `null` → not-found mapping). The
    /// `find_or_create` composition lives in `crate::domain::merchant`.
    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>>;

    /// Insert a new merchant, encrypting the dek with `master_key`. A duplicate primary key
    /// surfaces as `Error::is_duplicate()`.
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
/// Single-query primitives for the locker table. The `get_or_insert` composition lives
/// in the domain layer (`crate::domain::locker`), which sequences these primitives.
pub(crate) trait LockerInterface {
    type Error;

    /// Insert a locker row. A duplicate primary key surfaces as `Error::is_duplicate()`.
    async fn insert_locker(
        &self,
        new: types::LockerNew<'_>,
    ) -> Result<types::Locker, ContainerError<Self::Error>>;

    /// Point read by primary key; a missing row surfaces as `Error::is_not_found()`.
    async fn find_by_locker_id_merchant_id_customer_id(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<types::Locker, ContainerError<Self::Error>>;

    /// Read by the `hash_id` secondary lookup; `None` if absent.
    async fn find_optional_by_hash_id_merchant_id_customer_id(
        &self,
        hash_id: &str,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<Option<types::Locker>, ContainerError<Self::Error>>;

    /// Delete a locker row by primary key.
    async fn delete_locker(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>>;
}

/// Trait defining behaviour of the application with the hash table, providing APIs to interact
/// with it
#[deprecated(
    since = "1.0.0",
    note = "duplication of data should now be handled on the client side"
)]
pub(crate) trait HashInterface {
    type Error;

    /// Read by `data_hash` (secondary lookup); `None` if absent.
    async fn find_optional_by_data_hash(
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

    /// Read by `fingerprint_hash` (secondary dedup lookup); `None` if absent.
    async fn find_optional_by_fingerprint_hash(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>>;

    /// Insert a fingerprint row. A duplicate hash surfaces as `Error::is_duplicate()`.
    async fn insert_fingerprint(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
        fingerprint_id: Secret<String>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>>;
}

#[expect(dead_code)]
///
/// ReverseLookupInterface:
///
/// Interface for interacting with the reverse_lookup database table.
/// The table maps an external lookup_id to the partition key and
/// secondary key along with the source of insertion.
pub(crate) trait ReverseLookupInterface {
    type Error;

    /// Fetch a reverse lookup record by its lookup_id.
    async fn find_by_lookup_id(
        &self,
        lookup_id: &str,
    ) -> Result<types::ReverseLookup, ContainerError<Self::Error>>;

    /// Insert a new reverse lookup record into the database.
    async fn insert_reverse_lookup(
        &self,
        new: types::ReverseLookupNew,
    ) -> Result<types::ReverseLookup, ContainerError<Self::Error>>;
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
