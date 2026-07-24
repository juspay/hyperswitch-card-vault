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

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
use tokio::sync::RwLock;

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
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct RuntimeConfigValues {
    #[cfg(feature = "kv")]
    #[serde(default)]
    enable_kv: kv::KvState,
    #[serde(default)]
    use_replica: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct StorageRuntimeConfigStatus {
    pub runtime_config: crate::runtime_config::RuntimeConfigStatus,
    pub storage: StorageRuntimeConfigState,
}

#[derive(Debug, serde::Serialize)]
pub struct StorageRuntimeConfigState {
    pub use_replica: bool,
    #[cfg(feature = "kv")]
    pub kv_state: String,
}

pub struct GlobalStore {
    use_replica: AtomicBool,
    #[cfg(feature = "kv")]
    config: crate::config::KvConfig,
    #[cfg(feature = "kv")]
    state: RwLock<kv::KvState>,
}

impl GlobalStore {
    pub fn new(#[cfg(feature = "kv")] config: crate::config::KvConfig) -> Self {
        Self {
            use_replica: AtomicBool::new(false),
            #[cfg(feature = "kv")]
            config,
            #[cfg(feature = "kv")]
            state: RwLock::new(kv::KvState::Disabled),
        }
    }

    fn use_replica(&self) -> bool {
        self.use_replica.load(Ordering::Acquire)
    }

    fn enable_replica(&self) {
        self.use_replica.store(true, Ordering::Release);
    }

    fn disable_replica(&self) {
        self.use_replica.store(false, Ordering::Release);
    }

    /// Apply runtime-config replica read transitions after the runtime config cache is refreshed.
    pub(crate) async fn refresh_replica_state_from_runtime_config<F, Fut>(
        &self,
        runtime_config_manager: &crate::runtime_config::RuntimeConfigManager,
        replica_health_check: F,
    ) where
        F: FnOnce() -> Fut,
        Fut: Future<Output = bool>,
    {
        let requested_use_replica = runtime_config_manager
            .get::<RuntimeConfigValues>()
            .await
            .is_some_and(|runtime_conf| runtime_conf.use_replica);

        let current_use_replica = self.use_replica();
        match (current_use_replica, requested_use_replica) {
            (false, true) => {
                if replica_health_check().await {
                    self.enable_replica();
                } else {
                    crate::logger::warn!("Read replica unavailable");
                }
            }
            (true, false) => {
                self.disable_replica();
            }
            _ => {}
        }
    }

    #[cfg(feature = "kv")]
    async fn state(&self) -> kv::KvState {
        *self.state.read().await
    }

    /// Apply runtime-config KV state transitions after the runtime config cache is refreshed.
    #[cfg(feature = "kv")]
    pub(crate) async fn refresh_kv_state_from_runtime_config(
        &self,
        runtime_config_manager: &crate::runtime_config::RuntimeConfigManager,
        redis: Option<&redis_store::RedisStore>,
    ) {
        let requested_state = runtime_config_manager
            .get::<RuntimeConfigValues>()
            .await
            .map(|runtime_config_values| runtime_config_values.enable_kv)
            .unwrap_or(kv::KvState::Disabled);

        let current_state = self.state().await;
        let can_enable_kv = if matches!(
            (current_state, requested_state),
            (kv::KvState::Disabled, kv::KvState::Enabled)
        ) {
            match redis {
                Some(redis) => redis
                    .test()
                    .await
                    .inspect_err(|err| {
                        crate::logger::error!(
                            "error while checking redis connection, Error message: {}",
                            err
                        );
                    })
                    .is_ok(),
                None => {
                    crate::logger::error!("Redis connection unavailable");
                    false
                }
            }
        } else {
            false
        };

        let mut current_state = self.state.write().await;
        let next_state = current_state.apply_transition(requested_state, can_enable_kv);
        if next_state != *current_state {
            crate::logger::info!(from = %*current_state, to = %next_state, "KV mode transition accepted");
            *current_state = next_state;
        } else if requested_state != *current_state {
            crate::logger::warn!(
                current = %*current_state,
                requested = %requested_state,
                "KV mode transition ignored"
            );
        }
    }
}

/// Storage State that is to be passed though the application
#[derive(Clone)]
pub struct Storage {
    primary_pg_pool: Arc<Pool<AsyncPgConnection>>,
    replica_pg_pool: Option<Arc<Pool<AsyncPgConnection>>>,
    runtime_config_manager: Arc<crate::runtime_config::RuntimeConfigManager>,
    global_store: Arc<GlobalStore>,
    #[cfg(feature = "kv")]
    redis: Option<redis_store::TenantAwareRedisStore>,
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
        self.redis.clone()
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

    /// Create a new storage interface from configuration
    pub async fn new(
        primary_config: &Database,
        replica_config: Option<&Database>,
        schema: &str,
        runtime_config_manager: Arc<crate::runtime_config::RuntimeConfigManager>,
        global_store: Arc<GlobalStore>,
        #[cfg(feature = "kv")] redis: Option<redis_store::TenantAwareRedisStore>,
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
            global_store,
            #[cfg(feature = "kv")]
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

    pub async fn runtime_config_status(&self) -> StorageRuntimeConfigStatus {
        StorageRuntimeConfigStatus {
            runtime_config: self.runtime_config_manager.status().await,
            storage: StorageRuntimeConfigState {
                use_replica: self.global_store.use_replica(),
                #[cfg(feature = "kv")]
                kv_state: self.global_store.state().await.to_string(),
            },
        }
    }

    /// Returns `true` when runtime config allows replica reads.
    fn should_use_replica(&self) -> bool {
        self.has_replica() && self.global_store.use_replica()
    }

    /// Returns a connection from the replica pool when the runtime config enables it,
    /// otherwise returns a primary pool connection.
    pub async fn route_conn(&self) -> Result<DbConnection, ContainerError<error::StorageError>> {
        if self.should_use_replica() {
            crate::logger::debug!("Routing to read replica");
            self.get_replica_conn().await
        } else {
            crate::logger::debug!("Routing to primary pool");
            self.get_conn().await
        }
    }

    /// Return the current KV state cached by the runtime-config poller.
    #[cfg(feature = "kv")]
    pub(crate) async fn kv_settings(&self) -> kv::KvState {
        self.global_store.state().await
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
        let attrs = crate::metric_attributes!(("pool", pool), ("tenant_id", tenant_id.to_owned()));

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
            let attrs =
                crate::metric_attributes!(("pool", pool), ("tenant_id", tenant_id.to_owned()));

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
        self.global_store.config.ttl_for_kv
    }

    fn drainer_stream_name(&self, shard_key: &str) -> String {
        self.global_store.config.drainer_stream_name(shard_key)
    }

    fn drainer_num_partitions(&self) -> u8 {
        self.global_store.config.drainer_num_partitions
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

    /// Delete a reverse lookup record by its lookup_id.
    async fn delete_reverse_lookup(
        &self,
        lookup_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>>;
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
        crate::metric_attributes!(("pool", pool), ("outcome", outcome)),
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
{
    let table_name = std::any::type_name::<T>()
        .rsplit("::")
        .nth(1)
        .unwrap_or("UNKNOWN");

    crate::observability::metrics::DATABASE_QUERY_COUNT.add(
        1,
        crate::metric_attributes!(
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
        crate::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool),
            ("outcome", outcome),
        ),
    );

    result
}

#[cfg_attr(feature = "kv", expect(dead_code))]
async fn record_db_query_optional<T, Fut, R, E>(
    future: Fut,
    operation: DbOperation,
    pool: DbPool,
) -> Result<Option<R>, E>
where
    T: diesel::associations::HasTable<Table = T>,
    Fut: Future<Output = Result<Option<R>, E>>,
{
    let table_name = std::any::type_name::<T>()
        .rsplit("::")
        .nth(1)
        .unwrap_or("UNKNOWN");

    crate::observability::metrics::DATABASE_QUERY_COUNT.add(
        1,
        crate::metric_attributes!(
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
        crate::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool),
            ("outcome", outcome),
        ),
    );

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
{
    let table_name = std::any::type_name::<T>()
        .rsplit("::")
        .nth(1)
        .unwrap_or("UNKNOWN");

    crate::observability::metrics::DATABASE_QUERY_COUNT.add(
        1,
        crate::metric_attributes!(
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
        crate::metric_attributes!(
            ("table", table_name),
            ("operation", operation),
            ("pool", pool),
            ("outcome", outcome),
        ),
    );

    result
}
