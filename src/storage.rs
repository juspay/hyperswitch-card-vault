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

use std::{fmt::Debug, future::Future, sync::Arc};

use diesel_async::{
    AsyncPgConnection,
    pooled_connection::{
        self,
        deadpool::{Object, Pool},
    },
};
use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};

pub use self::scheme::StorageScheme;
#[cfg(feature = "redis")]
use crate::storage::redis as redis_store;
use crate::{
    config::Database,
    crypto::encryption_manager::encryption_interface::Encryption,
    error::{self, ContainerError},
};

/// All runtime configs, deserialized directly from the config endpoint's JSON body. Field names
/// match the keys the endpoint returns; each `#[serde(default)]` field fails closed when absent.
#[cfg(feature = "redis")]
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct RuntimeConfigValues {
    #[cfg(feature = "kv")]
    #[serde(default)]
    pub enable_kv: kv::KvState,
    #[serde(default)]
    pub use_replica: bool,
}

#[cfg(feature = "redis")]
#[derive(Debug, serde::Serialize)]
pub struct StorageRuntimeConfigStatus {
    pub runtime_config: crate::runtime_config::RuntimeConfigStatus,
    pub storage: StorageRuntimeConfigState,
}

#[cfg(feature = "redis")]
#[derive(Debug, serde::Serialize)]
pub struct StorageRuntimeConfigState {
    pub use_replica: bool,
    #[cfg(feature = "kv")]
    pub kv_state: String,
}

/// Storage State that is to be passed though the application
#[derive(Clone)]
pub struct Storage {
    primary_pg_pool: Arc<Pool<AsyncPgConnection>>,
    replica_pg_pool: Option<Arc<Pool<AsyncPgConnection>>>,
    #[cfg(feature = "kv")]
    kv_backend: Option<kv::KvBackend>,
    /// Per-tenant Redis store paired with the runtime-config manager. The manager is
    /// present only when runtime config is enabled — and it can never exist without
    /// its Redis read-through cache (enforced in `Storage::new`).
    #[cfg(feature = "redis")]
    redis: Option<(
        redis_store::TenantAwareRedisStore,
        Option<Arc<crate::runtime_config::RuntimeConfigManager>>,
    )>,
}

type DeadPoolConnType = Object<AsyncPgConnection>;

#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum DbPool {
    Primary,
    Replica,
}

#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum DbOperation {
    Insert,
    Update,
    Delete,
    FindOne,
    #[cfg(feature = "external_key_manager")]
    Filter,
}

crate::impl_metric_value_from!(DbPool, DbOperation);

pub struct DbConnection {
    conn: DeadPoolConnType,
    pool: DbPool,
}

impl DbConnection {
    fn new(conn: DeadPoolConnType, pool: DbPool) -> Self {
        Self { conn, pool }
    }

    fn pool(&self) -> DbPool {
        self.pool
    }

    fn get_mut(&mut self) -> &mut DeadPoolConnType {
        &mut self.conn
    }
}

impl Storage {
    #[cfg(feature = "redis")]
    pub fn get_redis_store(&self) -> Option<redis_store::TenantAwareRedisStore> {
        self.redis.as_ref().map(|(redis, _)| redis.clone())
    }

    /// The tenant's runtime-config manager — present only when runtime config is
    /// enabled (and thereby Redis-configured; see `Storage::new`).
    #[cfg(feature = "redis")]
    pub fn runtime_config_manager(
        &self,
    ) -> Option<&Arc<crate::runtime_config::RuntimeConfigManager>> {
        self.redis
            .as_ref()
            .and_then(|(_, manager)| manager.as_ref())
    }
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

    /// Create a new storage interface from configuration.
    ///
    /// Fails when runtime config is enabled but Redis is not configured — the runtime
    /// config manager is never operated without its Redis read-through cache.
    pub async fn new(
        primary_config: &Database,
        replica_config: Option<&Database>,
        schema: &str,
        #[cfg(feature = "kv")] kv_config: &crate::config::KvConfig,
        #[cfg(feature = "redis")] redis: Option<redis_store::TenantAwareRedisStore>,
        #[cfg(feature = "redis")] runtime_config: &crate::config::RuntimeConfig,
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

        #[cfg(feature = "redis")]
        let redis = match redis {
            Some(redis) => Some((
                redis,
                crate::runtime_config::RuntimeConfigManager::new(runtime_config).map(Arc::new),
            )),
            None => {
                if runtime_config.is_enabled() {
                    return Err(error::StorageError::InitializationError(
                        "runtime_config is enabled but Redis is not configured",
                    )
                    .into());
                }
                None
            }
        };

        Ok(Self {
            primary_pg_pool: pg_pool,
            replica_pg_pool: replica_pool,
            #[cfg(feature = "kv")]
            kv_backend: redis
                .clone()
                .map(|(redis, _)| kv::KvBackend::redis(redis, kv_config.clone())),
            #[cfg(feature = "redis")]
            redis,
        })
    }

    /// Get connection from database pool for accessing data
    pub async fn get_conn(&self) -> Result<DbConnection, ContainerError<error::StorageError>> {
        let pool = DbPool::Primary;
        let conn = record_db_connection_acquire_duration(self.primary_pg_pool.get(), pool)
            .await
            .change_context(error::StorageError::PoolClientFailure)?;

        Ok(DbConnection::new(conn, pool))
    }

    /// Get a connection from the read replica pool, if configured.
    /// Returns `ReplicaPoolNotConfigured` error if no replica pool was initialized.
    pub async fn get_replica_conn(
        &self,
    ) -> Result<DbConnection, ContainerError<error::StorageError>> {
        match self.replica_pg_pool.as_ref() {
            Some(pg_pool) => {
                let pool = DbPool::Replica;
                let conn = record_db_connection_acquire_duration(pg_pool.get(), pool)
                    .await
                    .change_context(error::StorageError::PoolClientFailure)?;

                Ok(DbConnection::new(conn, pool))
            }
            None => Err(ContainerError::from(
                error::StorageError::ReplicaPoolNotConfigured,
            )),
        }
    }

    /// Returns `true` if a read replica pool was configured and initialized.
    pub fn has_replica(&self) -> bool {
        self.replica_pg_pool.is_some()
    }

    #[cfg(feature = "redis")]
    pub async fn runtime_config_status(&self) -> StorageRuntimeConfigStatus {
        let runtime_config = match self.runtime_config_manager() {
            Some(manager) => manager.status(self).await,
            None => crate::runtime_config::RuntimeConfigStatus {
                status: crate::runtime_config::RuntimeConfigStatusKind::Disabled,
                config: None,
            },
        };

        StorageRuntimeConfigStatus {
            runtime_config,
            storage: StorageRuntimeConfigState {
                use_replica: self.should_use_replica().await,
                #[cfg(feature = "kv")]
                kv_state: self.kv_settings().await.to_string(),
            },
        }
    }

    /// Returns `true` when the tenant's runtime config enables replica reads and a
    /// replica pool is configured. Read per-operation from Postgres (`configs` row,
    /// Redis-cached) — never held in-process. Fails closed (`false`) when the config
    /// cannot be read.
    #[cfg(feature = "redis")]
    async fn should_use_replica(&self) -> bool {
        self.has_replica()
            && self
                .runtime_config_values()
                .await
                .is_some_and(|values| values.use_replica)
    }

    /// Without Redis there is no runtime config at all — replica reads stay off.
    #[cfg(not(feature = "redis"))]
    async fn should_use_replica(&self) -> bool {
        false
    }

    /// Returns a connection from the replica pool when the runtime config enables it,
    /// otherwise returns a primary pool connection.
    pub async fn route_conn(&self) -> Result<DbConnection, ContainerError<error::StorageError>> {
        if self.should_use_replica().await {
            crate::logger::debug!("Routing to read replica");
            self.get_replica_conn().await
        } else {
            crate::logger::debug!("Routing to primary pool");
            self.get_conn().await
        }
    }

    /// Read the KV state from the tenant's runtime config (`configs` Postgres row,
    /// read-through the tenant's Redis cache). Fails closed to `Disabled` when the
    /// config cannot be read.
    #[cfg(feature = "kv")]
    pub(crate) async fn kv_settings(&self) -> kv::KvState {
        self.runtime_config_values()
            .await
            .map(|values| values.enable_kv)
            .unwrap_or(kv::KvState::Disabled)
    }

    /// Fetch the tenant's runtime-config values via the runtime-config manager
    /// (`configs` Postgres row, read-through the tenant's Redis cache).
    ///
    /// **No fetch happens when runtime config is disabled** — there is no manager, so
    /// `None` is returned and callers fail closed (`use_replica: false`, KV `Disabled`)
    /// without touching Postgres or Redis.
    #[cfg(feature = "redis")]
    pub(crate) async fn runtime_config_values(&self) -> Option<RuntimeConfigValues> {
        match self.runtime_config_manager() {
            Some(manager) => manager.get::<RuntimeConfigValues>(self).await,
            None => None,
        }
    }

    #[cfg(feature = "kv")]
    pub(crate) fn kv_backend(&self) -> Option<kv::KvBackend> {
        self.kv_backend.clone()
    }

    pub fn collect_db_pool_state(&self, tenant_id: &str) {
        use crate::observability::metrics::{
            DATABASE_POOL_AVAILABLE, DATABASE_POOL_SIZE, DATABASE_POOL_WAITING,
        };

        fn to_u64(value: usize, field: &'static str, pool: DbPool, tenant_id: &str) -> Option<u64> {
            match u64::try_from(value) {
                Ok(v) => Some(v),
                Err(_) => {
                    tracing::warn!(
                        field,
                        pool = %<&'static str>::from(pool),
                        tenant_id,
                        value,
                        "Database pool metric value overflows u64, skipping"
                    );
                    None
                }
            }
        }

        let primary = self.primary_pg_pool.status();
        let pool = DbPool::Primary;
        let attrs =
            metrics_utils::metric_attributes!(("pool", pool), ("tenant_id", tenant_id.to_owned()));

        if let Some(size) = to_u64(primary.size, "size", pool, tenant_id) {
            DATABASE_POOL_SIZE.record(size, attrs);
        }
        if let Some(available) = to_u64(primary.available, "available", pool, tenant_id) {
            DATABASE_POOL_AVAILABLE.record(available, attrs);
        }
        if let Some(waiting) = to_u64(primary.waiting, "waiting", pool, tenant_id) {
            DATABASE_POOL_WAITING.record(waiting, attrs);
        }

        if let Some(replica) = &self.replica_pg_pool {
            let replica = replica.status();
            let pool = DbPool::Replica;
            let attrs = metrics_utils::metric_attributes!(
                ("pool", pool),
                ("tenant_id", tenant_id.to_owned())
            );

            if let Some(size) = to_u64(replica.size, "size", pool, tenant_id) {
                DATABASE_POOL_SIZE.record(size, attrs);
            }
            if let Some(available) = to_u64(replica.available, "available", pool, tenant_id) {
                DATABASE_POOL_AVAILABLE.record(available, attrs);
            }
            if let Some(waiting) = to_u64(replica.waiting, "waiting", pool, tenant_id) {
                DATABASE_POOL_WAITING.record(waiting, attrs);
            }
        }
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
    type Key = Secret<Vec<u8>>;
    type Value = types::HashTable;
}

#[cfg(feature = "caching")]
impl Cacheable<types::Fingerprint> for Storage {
    type Key = Secret<Vec<u8>>;
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

    #[cfg(feature = "external_key_manager")]
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
        new: types::LockerNew,
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
        data_hash: Secret<Vec<u8>>,
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>>;
    async fn insert_hash(
        &self,
        data_hash: Secret<Vec<u8>>,
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

#[cfg_attr(not(feature = "kv"), expect(dead_code))]
///
/// ReverseLookupInterface:
///
/// Interface for interacting with the reverse_lookup database table.
/// The table maps an external lookup_id to the partition key and
/// secondary key along with the source of insertion.
pub(crate) trait ReverseLookupInterface {
    type Error;

    /// Insert a new reverse lookup record into the database.
    async fn insert_reverse_lookup(
        &self,
        new: types::ReverseLookupNew,
    ) -> Result<types::ReverseLookup, ContainerError<Self::Error>>;

    /// Delete a reverse lookup record by its lookup_id.
    async fn delete_reverse_lookup(
        &self,
        lookup_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>>;
}

///
/// ConfigInterface:
///
/// Interface for interacting with the `configs` database table — the source of
/// truth for runtime configuration (`use_replica`, `enable_kv`).
#[cfg(feature = "redis")]
pub(crate) trait ConfigInterface {
    type Error;

    /// Read a config row by its primary key. Returns `None` when the row is absent.
    async fn find_config(
        &self,
        key: &str,
    ) -> Result<Option<serde_json::Value>, ContainerError<Self::Error>>;

    /// Upsert a config row (`INSERT … ON CONFLICT (key) DO UPDATE`).
    async fn upsert_config(
        &self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), ContainerError<Self::Error>>;
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

async fn record_db_connection_acquire_duration<Fut, T, E>(future: Fut, pool: DbPool) -> Result<T, E>
where
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let start = std::time::Instant::now();
    let result = future.await;
    let duration = start.elapsed();
    let outcome = if result.is_ok() { "success" } else { "error" };

    crate::observability::metrics::DATABASE_CONNECTION_ACQUIRE_DURATION.record(
        duration.as_secs_f64(),
        metrics_utils::metric_attributes!(("pool", pool), ("outcome", outcome)),
    );

    result
}

#[track_caller]
fn log_db_query<T, Q>(query: &Q, operation: DbOperation, pool: DbPool)
where
    T: diesel::associations::HasTable<Table = T>,
    Q: diesel::query_builder::QueryFragment<diesel::pg::Pg>,
{
    let table_name = std::any::type_name::<T>()
        .rsplit("::")
        .nth(1)
        .unwrap_or("UNKNOWN");

    crate::logger::debug!(
        query = %diesel::debug_query(query),
        table = %table_name,
        operation = %<&'static str>::from(operation),
        pool = %<&'static str>::from(pool),
        "Executing database query",
    );
}

async fn record_db_query<T, Fut, R, E>(
    future: Fut,
    operation: DbOperation,
    pool: DbPool,
) -> Result<R, E>
where
    T: diesel::associations::HasTable<Table = T>,
    Fut: Future<Output = Result<R, E>>,
    E: Debug,
{
    let table_name = std::any::type_name::<T>()
        .rsplit("::")
        .nth(1)
        .unwrap_or("UNKNOWN");

    crate::observability::metrics::DATABASE_QUERY_COUNT.add(
        1,
        metrics_utils::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool)
        ),
    );

    let start = std::time::Instant::now();
    let result = future.await;
    let duration = start.elapsed();
    let outcome = if result.is_ok() { "success" } else { "error" };

    crate::observability::metrics::DATABASE_QUERY_DURATION.record(
        duration.as_secs_f64(),
        metrics_utils::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool),
            ("outcome", outcome),
        ),
    );

    if let Err(error) = &result {
        crate::logger::error!(
            table = table_name,
            operation = ?operation,
            pool = ?pool,
            duration_ms = duration.as_millis(),
            error_message = ?error,
            "Database query failed"
        );
    }

    result
}

async fn record_db_query_optional<T, Fut, R, E>(
    future: Fut,
    operation: DbOperation,
    pool: DbPool,
) -> Result<Option<R>, E>
where
    T: diesel::associations::HasTable<Table = T>,
    Fut: Future<Output = Result<Option<R>, E>>,
    E: Debug,
{
    let table_name = std::any::type_name::<T>()
        .rsplit("::")
        .nth(1)
        .unwrap_or("UNKNOWN");

    crate::observability::metrics::DATABASE_QUERY_COUNT.add(
        1,
        metrics_utils::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool)
        ),
    );

    let start = std::time::Instant::now();
    let result = future.await;
    let duration = start.elapsed();
    let outcome = match &result {
        Ok(Some(_)) => "success",
        Ok(None) => "not_found",
        Err(_) => "error",
    };

    crate::observability::metrics::DATABASE_QUERY_DURATION.record(
        duration.as_secs_f64(),
        metrics_utils::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool),
            ("outcome", outcome),
        ),
    );

    if let Err(error) = &result {
        crate::logger::error!(
            table = table_name,
            operation = ?operation,
            pool = ?pool,
            duration_ms = duration.as_millis(),
            error_message = ?error,
            "Database optional query failed"
        );
    }

    result
}

async fn record_db_query_rows<T, Fut, E>(
    future: Fut,
    operation: DbOperation,
    pool: DbPool,
) -> Result<usize, E>
where
    T: diesel::associations::HasTable<Table = T>,
    Fut: Future<Output = Result<usize, E>>,
    E: Debug,
{
    let table_name = std::any::type_name::<T>()
        .rsplit("::")
        .nth(1)
        .unwrap_or("UNKNOWN");

    crate::observability::metrics::DATABASE_QUERY_COUNT.add(
        1,
        metrics_utils::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool)
        ),
    );

    let start = std::time::Instant::now();
    let result = future.await;
    let duration = start.elapsed();
    let outcome = match &result {
        Ok(rows) if *rows == 0 => "zero_rows",
        Ok(_) => "success",
        Err(_) => "error",
    };

    crate::observability::metrics::DATABASE_QUERY_DURATION.record(
        duration.as_secs_f64(),
        metrics_utils::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool),
            ("outcome", outcome),
        ),
    );

    if let Err(error) = &result {
        crate::logger::error!(
            table = table_name,
            operation = ?operation,
            pool = ?pool,
            duration_ms = duration.as_millis(),
            error_message = ?error,
            "Database rows query failed"
        );
    }

    result
}
