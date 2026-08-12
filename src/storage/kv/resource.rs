//! Generic KV resource trait, key-shape locators, and CRUD helpers.
//!
//! Stores the Diesel table-mapped entity in Redis and returns the resource model.

use error_stack::Report;
use tracing::instrument;

use super::{
    StorageScheme,
    entity::EntityType,
    partition_key::{KvStorePartition, PartitionKey},
    scheme::KvState,
    serializable_query::SerializableQuery,
    wrapper::{KvBackend, KvBehaviour, KvFindResult, KvInsertResult},
};
use crate::{
    error::{
        ContainerError, ReverseLookupDBError, StorageErrorExt,
        kv::{KvError, RedisErrorExt},
    },
    observability::metrics,
    storage::{ReverseLookupInterface, Storage, types},
};

/// Secondary-to-primary mapping metadata emitted by a KV resource.
pub(crate) struct ReverseLookupKey {
    pub lookup_id: String,
}

/// Trait for retrieving the partition key of a KV resource.
///
/// This is used to determine which partition a KV resource belongs to.
///
/// For redis kv, this would be used to determine the redis stream partition.
pub(crate) trait GetPartitionKey {
    fn get_partition_key(&self) -> PartitionKey<'_>;
}

#[derive(Clone, Debug)]
pub(crate) struct SecondaryKey(String);

impl SecondaryKey {
    pub(crate) fn new(key: String) -> Self {
        Self(key)
    }
}

impl std::fmt::Display for SecondaryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Trait for retrieving the secondary key of a KV resource.
///
/// This is used to determine which secondary partition a KV resource belongs to.
///
/// For redis kv, this would be the hash field key value.
pub(crate) trait GetSecondaryKey {
    fn get_secondary_key(&self) -> SecondaryKey;
}

pub(crate) trait GetLookupKey {
    fn get_lookup_key(&self) -> ReverseLookupKey;
}

pub(crate) struct DirectInsert;

pub(crate) struct ReverseLookupInsert;

/// Base contract for a table that can be routed through the KV storage layer.
///
/// `KvResource` covers the operations every KV-backed resource must support:
/// inserting a new row and finding an existing row by its primary key. The
/// generic helpers in this module use this trait to decide whether an operation
/// should go to Postgres directly or be served through Redis with a serialized
/// drainer query for eventual Postgres replay.
///
/// Implementors describe both the API-facing resource type (`Self`) and the
/// Diesel table-mapped entity (`DieselEntity`) that is actually serialized into
/// Redis. `PrimaryKeyType` must be able to produce the Redis partition key used
/// for primary-key based lookups.
pub(crate) trait KvResource:
    std::fmt::Debug + KvStorePartition + EntityType + Sync + Send + Sized
{
    /// Storage-layer error type returned by the resource implementation.
    type Error: error_stack::Context
        + Send
        + Sync
        + 'static
        + StorageErrorExt
        + for<'a> From<&'a KvError>;

    /// Insert routing strategy for this resource.
    ///
    /// Use `DirectInsert` when the primary key alone is sufficient for all KV
    /// lookups. Use `ReverseLookupInsert` when inserts must also create a
    /// reverse-lookup-key to primary-key mapping.
    type InsertStrategy;

    /// Diesel insertable/new-record type used for both Postgres inserts and
    /// drainer query generation.
    type DieselNew: Into<Self::DieselEntity>;

    /// Diesel queryable table entity stored as the Redis value.
    ///
    /// This type is converted back into `Self` before returning to callers.
    type DieselEntity: serde::Serialize
        + serde::de::DeserializeOwned
        + std::fmt::Debug
        + KvStorePartition
        + super::entity::EntityType
        + Sync
        + Into<Self>;

    /// Primary key representation for this table.
    ///
    /// This may be a composite key. It must produce the partition key used by
    /// Redis for primary-key based insert, find, update, and delete operations.
    type PrimaryKeyType: GetPartitionKey + GetSecondaryKey;

    /// Reconstruct the primary key from the insert payload for insert-time conflict checks.
    fn get_primary_key_from_new_object(new_object: &Self::DieselNew) -> Self::PrimaryKeyType;

    /// Mark a new record with the storage scheme selected for the insert.
    fn set_storage_scheme(diesel_new: &mut Self::DieselNew, scheme: StorageScheme);

    /// Build the INSERT statement consumed by the drainer when Redis is the
    /// write path.
    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, KvError>;

    /// Insert the new record through the backing storage implementation.
    ///
    /// This is used directly when the selected storage scheme is
    /// `PostgresOnly`, and as the fallback implementation for resources that do
    /// not write through Redis for the current operation.
    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self::DieselEntity, ContainerError<Self::Error>>;

    /// Find a record by primary key through the backing storage implementation.
    ///
    /// This is used for `PostgresOnly` reads and as the fallback when Redis does
    /// not contain the requested primary key.
    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self::DieselEntity, ContainerError<Self::Error>>;
}

/// Extension of `KvResource` for resources that support deletion by primary key.
///
/// The primary-key insert and find behavior is inherited from `KvResource`.
/// Implementors add the delete-specific Postgres operation and the drainer query
/// needed when deletes are routed through Redis.
pub(crate) trait KvDeletableResource: KvResource {
    /// Build the DELETE statement consumed by the drainer when Redis is the
    /// delete path.
    fn generate_delete_drainer_query(
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, KvError>;

    /// Delete a record by primary key through the backing storage implementation.
    ///
    /// Returns the number of rows deleted from storage.
    async fn storage_delete(
        store: &Storage,
        pk: Self::PrimaryKeyType,
    ) -> Result<usize, ContainerError<Self::Error>>;
}

pub(crate) trait KvDeleteWithoutLookup: KvDeletableResource {}

pub(crate) trait KvDeletableWithLookup: KvDeletableResource {
    fn get_reverse_lookup_key_from_resource(resource: &Self) -> ReverseLookupKey;
}

/// Extension of `KvResource` for resources that support updates by primary key.
///
/// The primary-key insert and find behavior is inherited from `KvResource`.
/// Implementors add the update representation, the Postgres update operation,
/// the Redis-side in-memory merge, and the drainer query needed when updates are
/// routed through Redis.
pub(crate) trait KvUpdatableResource: KvResource {
    /// Diesel changeset/update type for this resource.
    type DieselUpdate;

    /// Mark an update with the storage scheme selected for the operation.
    fn set_update_storage_scheme(diesel_update: &mut Self::DieselUpdate, scheme: StorageScheme);

    /// Build the UPDATE statement consumed by the drainer when Redis is the
    /// update path.
    fn generate_update_drainer_query(
        update: &Self::DieselUpdate,
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, KvError>;

    /// Apply an update to the current Diesel entity stored in Redis.
    ///
    /// The returned entity is written back to Redis and converted to `Self` for
    /// the caller.
    fn apply_update(update: Self::DieselUpdate, current: Self::DieselEntity) -> Self::DieselEntity;

    /// Update a record by primary key through the backing storage implementation.
    async fn storage_update(
        store: &Storage,
        update: Self::DieselUpdate,
        pk: Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<Self::Error>>;
}

/// Extension of `KvResource` for resources that support secondary-key lookups.
///
/// `KvSecondaryLookupResource` is for resources whose Redis value is still
/// stored by the primary partition key, but which also need a secondary key
/// lookup path. Inserts create a reverse lookup record that maps the secondary
/// lookup id to the primary partition key. Finds by secondary key first resolve
/// that mapping, then read the resource by the primary key from Redis, with
/// Postgres fallback on lookup or Redis misses.
pub(crate) trait KvSecondaryLookupResource:
    KvResource<InsertStrategy = ReverseLookupInsert>
{
    /// Secondary-key representation used to build and query reverse lookup ids.
    type LookupKeyType: GetLookupKey;

    /// Derive the secondary lookup key for a newly inserted record.
    ///
    /// The returned key is persisted as a reverse lookup record during Redis KV
    /// inserts, allowing later reads by secondary key to resolve the primary
    /// partition key.
    fn get_reverse_lookup_key(
        new_object: &Self::DieselNew,
        partition_key: &PartitionKey<'_>,
    ) -> Self::LookupKeyType;

    /// Find a record by secondary key through the backing storage implementation.
    ///
    /// This is used for `PostgresOnly` reads and as the fallback when the
    /// reverse lookup record or Redis value is missing.
    async fn storage_find_by_lookup(
        store: &Storage,
        lookup_key: &Self::LookupKeyType,
    ) -> Result<Self, ContainerError<Self::Error>>;
}

pub(crate) enum InsertConflictKey {
    PartitionKey(String),
    LookupKey(String),
}

impl InsertConflictKey {
    fn key(&self) -> &str {
        match self {
            Self::PartitionKey(key) | Self::LookupKey(key) => key,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::PartitionKey(_) => "partition_key",
            Self::LookupKey(_) => "lookup_key",
        }
    }
}

pub(crate) trait KvInsertConflictStrategy<M>
where
    M: KvResource,
{
    async fn storage_get_insert_conflict(
        store: &Storage,
        diesel_new: &M::DieselNew,
        partition_key: &PartitionKey<'_>,
    ) -> Result<Option<InsertConflictKey>, ContainerError<M::Error>>;
}

impl<M> KvInsertConflictStrategy<M> for DirectInsert
where
    M: KvResource<InsertStrategy = Self>,
{
    async fn storage_get_insert_conflict(
        store: &Storage,
        diesel_new: &M::DieselNew,
        partition_key: &PartitionKey<'_>,
    ) -> Result<Option<InsertConflictKey>, ContainerError<M::Error>> {
        let primary_key = M::get_primary_key_from_new_object(diesel_new);

        match M::storage_find(store, &primary_key).await {
            Ok(_) => Ok(Some(InsertConflictKey::PartitionKey(
                partition_key.to_string(),
            ))),
            Err(err) if err.get_inner().is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl<M> KvInsertConflictStrategy<M> for ReverseLookupInsert
where
    M: KvSecondaryLookupResource,
{
    async fn storage_get_insert_conflict(
        store: &Storage,
        diesel_new: &M::DieselNew,
        partition_key: &PartitionKey<'_>,
    ) -> Result<Option<InsertConflictKey>, ContainerError<M::Error>> {
        let primary_key = M::get_primary_key_from_new_object(diesel_new);

        match M::storage_find(store, &primary_key).await {
            Ok(_) => {
                return Ok(Some(InsertConflictKey::PartitionKey(
                    partition_key.to_string(),
                )));
            }
            Err(err) if err.get_inner().is_not_found() => {}
            Err(err) => return Err(err),
        }

        let lookup_key = M::get_reverse_lookup_key(diesel_new, partition_key);
        let reverse_lookup_key = lookup_key.get_lookup_key();

        match M::storage_find_by_lookup(store, &lookup_key).await {
            Ok(_) => Ok(Some(InsertConflictKey::LookupKey(
                reverse_lookup_key.lookup_id,
            ))),
            Err(err) if err.get_inner().is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }
}

fn kv_backend_error<E>(report: Report<KvError>) -> ContainerError<E>
where
    E: for<'a> From<&'a KvError> + error_stack::Context,
{
    let ctx: E = report.current_context().into();
    ContainerError::from(report.change_context(ctx))
}

fn kv_duplicate_error<E>(key: &str) -> ContainerError<E>
where
    E: for<'a> From<&'a KvError> + error_stack::Context,
{
    kv_backend_error::<E>(Report::new(KvError::DuplicateValue {
        key: key.to_string(),
    }))
}

fn reverse_lookup_insert_error_to_resource_error<E>(
    lookup_id: &str,
    err: ContainerError<ReverseLookupDBError>,
) -> ContainerError<E>
where
    E: for<'a> From<&'a KvError> + error_stack::Context,
{
    let kv_error = if err.get_inner().is_duplicate() {
        KvError::DuplicateValue {
            key: lookup_id.to_string(),
        }
    } else {
        KvError::Backend
    };

    kv_backend_error::<E>(err.error.change_context(kv_error))
}

#[derive(Clone, Default)]
pub(crate) enum DecidedStorageScheme {
    #[default]
    PostgresOnly,
    Kv(KvBackend),
}

impl DecidedStorageScheme {
    fn storage_scheme(&self) -> StorageScheme {
        match self {
            Self::PostgresOnly => StorageScheme::PostgresOnly,
            Self::Kv(_) => StorageScheme::RedisKv,
        }
    }
}

fn log_storage_scheme_decision(
    resource: &'static str,
    operation: &'static str,
    decided_scheme: &DecidedStorageScheme,
) {
    let storage_scheme = decided_scheme.storage_scheme();
    crate::logger::info!(
        resource = %resource,
        operation = %operation,
        storage_scheme = %storage_scheme,
        "Storage scheme decision"
    );
}

async fn decide_storage_scheme_for_find_operation(store: &Storage) -> DecidedStorageScheme {
    let state = store.kv_settings().await;
    match state {
        KvState::Disabled => DecidedStorageScheme::PostgresOnly,
        // in softkill mode as well, always attempt RedisKv and fallback to postgres.
        KvState::Enabled | KvState::SoftKill => store
            .kv_backend()
            .map_or(DecidedStorageScheme::PostgresOnly, DecidedStorageScheme::Kv),
    }
}

/// Decide where an insert should be written for the current KV runtime state.
///
/// The insert path is more conservative than reads because accepting a Redis write for a
/// row that already exists only in PostgreSQL would bypass PostgreSQL's unique constraints
/// until drainer replay. This is especially important during KV enablement, when older
/// records may not have Redis entries yet.
///
/// Decision summary:
/// - `Disabled`: write directly to PostgreSQL.
/// - `Enabled`: check Redis first. If Redis has no knowledge of the key, check PostgreSQL
///   with the resource's insert strategy; existing PostgreSQL rows are returned as
///   duplicates, otherwise the insert can proceed through KV.
/// - `SoftKill`: check Redis first. Tombstoned keys stay on KV to preserve drainer-delay
///   ordering; completely absent keys write to PostgreSQL.
async fn decide_storage_scheme_for_insert_operation<M>(
    store: &Storage,
    diesel_new: &M::DieselNew,
    partition_key: &PartitionKey<'_>,
    reverse_lookup_key: Option<&ReverseLookupKey>,
) -> Result<DecidedStorageScheme, ContainerError<M::Error>>
where
    M: KvResource,
    M::InsertStrategy: KvInsertConflictStrategy<M>,
{
    let state = store.kv_settings().await;
    crate::logger::debug!(
        resource = %M::ENTITY_TYPE,
        operation = "insert",
        kv_state = %state,
        has_reverse_lookup_key = reverse_lookup_key.is_some(),
        "Deciding insert storage scheme"
    );

    match state {
        // KV is fully disabled, so PostgreSQL remains the source of truth for inserts.
        KvState::Disabled => {
            crate::logger::debug!(
                resource = %M::ENTITY_TYPE,
                operation = "insert",
                kv_state = %state,
                storage_scheme = %StorageScheme::PostgresOnly,
                "KV disabled; routing insert to PostgreSQL"
            );
            Ok(DecidedStorageScheme::PostgresOnly)
        }
        KvState::Enabled => {
            let Some(kv_backend) = store.kv_backend() else {
                crate::logger::debug!(
                    resource = %M::ENTITY_TYPE,
                    operation = "insert",
                    kv_state = %state,
                    storage_scheme = %StorageScheme::PostgresOnly,
                    "KV enabled but backend is unavailable; routing insert to PostgreSQL"
                );
                return Ok(DecidedStorageScheme::PostgresOnly);
            };

            // First honor any existing Redis state: present keys are duplicates and
            // tombstoned keys must remain on KV so pending deletes can drain safely.
            match decide_insert_scheme_from_kv_state::<M>(
                kv_backend.clone(),
                partition_key,
                reverse_lookup_key,
            )
            .await?
            {
                Some(decided_scheme) => {
                    crate::logger::debug!(
                        resource = %M::ENTITY_TYPE,
                        operation = "insert",
                        kv_state = %state,
                        storage_scheme = %decided_scheme.storage_scheme(),
                        "Redis state determined insert storage scheme"
                    );
                    Ok(decided_scheme)
                }
                None => {
                    // Redis is absent. Check PostgreSQL before writing to KV so records
                    // created before KV enablement still enforce their DB uniqueness.
                    crate::logger::debug!(
                        resource = %M::ENTITY_TYPE,
                        operation = "insert",
                        kv_state = %state,
                        "Redis has no insert state; checking PostgreSQL for conflicts"
                    );
                    let conflict_key = match <M::InsertStrategy as KvInsertConflictStrategy<
                        M,
                    >>::storage_get_insert_conflict(
                        store,
                        diesel_new,
                        partition_key,
                    )
                    .await
                    {
                        Ok(conflict_key) => conflict_key,
                        Err(err) => {
                            crate::logger::warn!(
                                resource = %M::ENTITY_TYPE,
                                operation = "insert",
                                kv_state = %state,
                                error = ?err,
                                "PostgreSQL conflict check failed for KV-enabled insert"
                            );
                            return Err(err);
                        }
                    };

                    match conflict_key {
                        Some(conflict_key) => {
                            crate::logger::debug!(
                                resource = %M::ENTITY_TYPE,
                                operation = "insert",
                                kv_state = %state,
                                conflict_key_type = %conflict_key.kind(),
                                "PostgreSQL conflict found for KV-enabled insert"
                            );
                            Err(kv_duplicate_error::<M::Error>(conflict_key.key()))
                        }
                        None => {
                            crate::logger::debug!(
                                resource = %M::ENTITY_TYPE,
                                operation = "insert",
                                kv_state = %state,
                                storage_scheme = %StorageScheme::RedisKv,
                                "No PostgreSQL conflict found; routing insert to KV"
                            );
                            Ok(DecidedStorageScheme::Kv(kv_backend))
                        }
                    }
                }
            }
        }
        KvState::SoftKill => {
            let Some(kv_backend) = store.kv_backend() else {
                crate::logger::debug!(
                    resource = %M::ENTITY_TYPE,
                    operation = "insert",
                    kv_state = %state,
                    storage_scheme = %StorageScheme::PostgresOnly,
                    "KV soft-kill but backend is unavailable; routing insert to PostgreSQL"
                );
                return Ok(DecidedStorageScheme::PostgresOnly);
            };

            // In soft-kill, only keys already tracked by Redis stay on KV. Fully absent
            // keys move to PostgreSQL as part of draining traffic away from KV.
            let redis_decision = decide_insert_scheme_from_kv_state::<M>(
                kv_backend,
                partition_key,
                reverse_lookup_key,
            )
            .await?;

            match redis_decision {
                Some(decided_scheme) => {
                    crate::logger::debug!(
                        resource = %M::ENTITY_TYPE,
                        operation = "insert",
                        kv_state = %state,
                        storage_scheme = %decided_scheme.storage_scheme(),
                        "Redis state determined soft-kill insert storage scheme"
                    );
                    Ok(decided_scheme)
                }
                None => {
                    crate::logger::debug!(
                        resource = %M::ENTITY_TYPE,
                        operation = "insert",
                        kv_state = %state,
                        storage_scheme = %StorageScheme::PostgresOnly,
                        "Redis has no insert state in soft-kill; routing insert to PostgreSQL"
                    );
                    Ok(DecidedStorageScheme::PostgresOnly)
                }
            }
        }
    }
}

/// Inspect Redis state for an insert key and return a forced decision when Redis is
/// authoritative for the key.
///
/// Return values:
/// - `Ok(Some(Kv(_)))`: Redis contains a tombstone, so re-insert through KV and let the
///   drainer serialize the pending delete/insert effects.
/// - `Err(Duplicate)`: Redis contains a live primary or reverse-lookup record, so a new
///   insert would violate uniqueness.
/// - `Ok(None)`: both the primary key and optional reverse lookup key are absent from
///   Redis; the caller must decide whether to check PostgreSQL or write there directly.
///
/// Backend errors are propagated because treating Redis failures as misses could route
/// duplicate or tombstoned records to PostgreSQL incorrectly.
async fn decide_insert_scheme_from_kv_state<M>(
    kv_backend: KvBackend,
    partition_key: &PartitionKey<'_>,
    reverse_lookup_key: Option<&ReverseLookupKey>,
) -> Result<Option<DecidedStorageScheme>, ContainerError<M::Error>>
where
    M: KvResource,
{
    // Step 1: check the primary Redis key.
    let partition_key_str = partition_key.to_string();
    crate::logger::debug!(
        resource = %M::ENTITY_TYPE,
        operation = "insert",
        redis_lookup = "primary",
        has_reverse_lookup_key = reverse_lookup_key.is_some(),
        "Checking Redis state for insert"
    );

    match kv_backend
        .find::<M::DieselEntity>(partition_key.clone())
        .await
    {
        // A live primary key means the insert is a duplicate.
        Ok(KvFindResult::Present(_)) => {
            crate::logger::debug!(
                resource = %M::ENTITY_TYPE,
                operation = "insert",
                redis_lookup = "primary",
                redis_state = "present",
                "Primary Redis key already exists; treating insert as duplicate"
            );
            Err(kv_duplicate_error::<M::Error>(&partition_key_str))
        }
        // A tombstone means a delete may still be draining; re-insert through KV.
        Ok(KvFindResult::Deleted) => {
            crate::logger::debug!(
                resource = %M::ENTITY_TYPE,
                operation = "insert",
                redis_lookup = "primary",
                redis_state = "deleted",
                storage_scheme = %StorageScheme::RedisKv,
                "Primary Redis key is tombstoned; routing insert through KV"
            );
            Ok(Some(DecidedStorageScheme::Kv(kv_backend)))
        }
        Ok(KvFindResult::Absent) => {
            crate::logger::debug!(
                resource = %M::ENTITY_TYPE,
                operation = "insert",
                redis_lookup = "primary",
                redis_state = "absent",
                "Primary Redis key absent for insert"
            );
            metrics::KV_CACHE_MISS_COUNT.add(
                1,
                metrics_utils::metric_attributes![("resource", M::ENTITY_TYPE)],
            );

            // Step 2: resources without a secondary uniqueness key have no more Redis
            // state to consult.
            let Some(reverse_lookup_key) = reverse_lookup_key else {
                crate::logger::debug!(
                    resource = %M::ENTITY_TYPE,
                    operation = "insert",
                    "No reverse lookup key for insert; Redis has no authoritative state"
                );
                return Ok(None);
            };

            // Step 3: check the reverse lookup key for resources whose logical insert
            // uniqueness is represented by a secondary Redis key.
            let reverse_lookup_partition_key = PartitionKey::ReverseLookup {
                lookup_id: &reverse_lookup_key.lookup_id,
            };
            let reverse_lookup_key_str = reverse_lookup_partition_key.to_string();

            crate::logger::debug!(
                resource = %M::ENTITY_TYPE,
                operation = "insert",
                redis_lookup = "reverse_lookup",
                "Checking reverse lookup Redis state for insert"
            );

            match kv_backend
                .find::<types::ReverseLookup>(reverse_lookup_partition_key)
                .await
            {
                // A live reverse lookup means another live row owns the logical key.
                Ok(KvFindResult::Present(_)) => {
                    crate::logger::debug!(
                        resource = %M::ENTITY_TYPE,
                        operation = "insert",
                        redis_lookup = "reverse_lookup",
                        redis_state = "present",
                        "Reverse lookup Redis key already exists; treating insert as duplicate"
                    );
                    Err(kv_duplicate_error::<M::Error>(
                        &reverse_lookup_key.lookup_id,
                    ))
                }
                // A reverse-lookup tombstone is still KV-tracked and should be replaced
                // through KV rather than bypassed via PostgreSQL.
                Ok(KvFindResult::Deleted) => {
                    crate::logger::debug!(
                        resource = %M::ENTITY_TYPE,
                        operation = "insert",
                        redis_lookup = "reverse_lookup",
                        redis_state = "deleted",
                        storage_scheme = %StorageScheme::RedisKv,
                        "Reverse lookup Redis key is tombstoned; routing insert through KV"
                    );
                    Ok(Some(DecidedStorageScheme::Kv(kv_backend)))
                }
                Ok(KvFindResult::Absent) => {
                    crate::logger::debug!(
                        resource = %M::ENTITY_TYPE,
                        operation = "insert",
                        redis_lookup = "reverse_lookup",
                        redis_state = "absent",
                        "Reverse lookup Redis key absent for insert"
                    );
                    metrics::KV_CACHE_MISS_COUNT.add(
                        1,
                        metrics_utils::metric_attributes![("resource", M::ENTITY_TYPE)],
                    );
                    Ok(None)
                }
                Err(e) => {
                    crate::logger::warn!(
                        resource = %M::ENTITY_TYPE,
                        operation = "insert",
                        redis_lookup = "reverse_lookup",
                        error = ?e,
                        "Reverse lookup Redis state check failed for insert"
                    );
                    Err(kv_backend_error::<M::Error>(
                        e.to_redis_failed_response(&reverse_lookup_key_str),
                    ))
                }
            }
        }
        Err(e) => {
            crate::logger::warn!(
                resource = %M::ENTITY_TYPE,
                operation = "insert",
                redis_lookup = "primary",
                error = ?e,
                "Primary Redis state check failed for insert"
            );
            Err(kv_backend_error::<M::Error>(
                e.to_redis_failed_response(&partition_key_str),
            ))
        }
    }
}

/// Call this to decide storage scheme for Update and Delete operations
async fn decide_storage_scheme_for_mutate_operation<M>(
    store: &Storage,
    partition_key: &PartitionKey<'_>,
) -> Result<(DecidedStorageScheme, Option<M::DieselEntity>), ContainerError<M::Error>>
where
    M: KvResource,
{
    let state = store.kv_settings().await;

    match state {
        KvState::Disabled => Ok((DecidedStorageScheme::PostgresOnly, None)),
        KvState::Enabled => Ok((
            store
                .kv_backend()
                .map_or(DecidedStorageScheme::PostgresOnly, DecidedStorageScheme::Kv),
            None,
        )),
        KvState::SoftKill => {
            let Some(kv_backend) = store.kv_backend() else {
                return Ok((DecidedStorageScheme::PostgresOnly, None));
            };
            // With this implementation, Hot keys may never recover out of KV.
            let partition_key_str = partition_key.to_string();
            let result = kv_backend
                .find::<M::DieselEntity>(partition_key.clone())
                .await;

            match result {
                // in case of value Present and value Deleted response, stick to KV mode
                // in order to cover for drainer delay.
                Ok(KvFindResult::Present(v)) => Ok((DecidedStorageScheme::Kv(kv_backend), Some(v))),
                Ok(KvFindResult::Deleted) => Ok((DecidedStorageScheme::Kv(kv_backend), None)),
                Ok(KvFindResult::Absent) => {
                    metrics::KV_CACHE_MISS_COUNT.add(
                        1,
                        metrics_utils::metric_attributes![("resource", M::ENTITY_TYPE)],
                    );
                    Ok((DecidedStorageScheme::PostgresOnly, None))
                }
                Err(e) => Err(kv_backend_error::<M::Error>(
                    e.to_redis_failed_response(&partition_key_str),
                )),
            }
        }
    }
}

async fn insert_resource_inner<M, F>(
    store: &Storage,
    mut diesel_new: M::DieselNew,
    get_reverse_lookup_key: F,
) -> Result<M::DieselEntity, ContainerError<M::Error>>
where
    M: KvResource,
    M::InsertStrategy: KvInsertConflictStrategy<M>,
    F: FnOnce(&M::DieselNew, &PartitionKey<'_>) -> Option<ReverseLookupKey>,
{
    let primary_key = M::get_primary_key_from_new_object(&diesel_new);
    let partition_key = primary_key.get_partition_key();
    let reverse_lookup_key = get_reverse_lookup_key(&diesel_new, &partition_key);
    let decided_scheme = decide_storage_scheme_for_insert_operation::<M>(
        store,
        &diesel_new,
        &partition_key,
        reverse_lookup_key.as_ref(),
    )
    .await?;
    log_storage_scheme_decision(M::ENTITY_TYPE, "insert", &decided_scheme);
    let scheme = decided_scheme.storage_scheme();
    M::set_storage_scheme(&mut diesel_new, scheme);

    match decided_scheme {
        DecidedStorageScheme::PostgresOnly => M::storage_insert(diesel_new, store).await,
        DecidedStorageScheme::Kv(kv_backend) => {
            let drainer_query = M::generate_insert_drainer_query(&diesel_new)
                .map_err(kv_backend_error::<M::Error>)?;

            let partition_key_str = partition_key.to_string();
            if let Some(reverse_lookup_key) = reverse_lookup_key {
                let lookup_id = reverse_lookup_key.lookup_id.clone();
                let secondary_key_str = primary_key.get_secondary_key().to_string();
                store
                    .insert_reverse_lookup(types::ReverseLookupNew {
                        lookup_id: lookup_id.clone(),
                        secondary_key: secondary_key_str,
                        partition_key: partition_key_str.clone(),
                        source: M::ENTITY_TYPE.to_string(),
                        updated_by: scheme.to_string(),
                    })
                    .await
                    .map_err(|err| {
                        reverse_lookup_insert_error_to_resource_error::<M::Error>(&lookup_id, err)
                    })?;
            }

            let diesel_entity = diesel_new.into();
            let reply = kv_backend
                .insert(partition_key, &diesel_entity, drainer_query)
                .await
                .map_err(|e| {
                    kv_backend_error::<M::Error>(e.to_redis_failed_response(&partition_key_str))
                })?;

            match reply {
                KvInsertResult::Inserted => Ok(diesel_entity),
                KvInsertResult::AlreadyExists => {
                    Err(kv_duplicate_error::<M::Error>(&partition_key_str))
                }
            }
        }
    }
}

/// Insert via KV backend. `AlreadyExists` → `Duplicate`. `PostgresOnly` → `storage_insert`.
/// On the RedisKv path the model's serial `id` is unresolved (e.g. `0`); the drainer
/// assigns it on PG replay. Callers only see the business id (`fingerprint_id`).
#[instrument(skip(store, diesel_new), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn insert_resource<M>(
    store: &Storage,
    diesel_new: M::DieselNew,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvResource<InsertStrategy = DirectInsert>,
{
    insert_resource_inner::<M, _>(store, diesel_new, |_, _| None)
        .await
        .map(Into::into)
}

#[instrument(skip(store, diesel_new), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn insert_resource_with_reverse_lookup<M>(
    store: &Storage,
    diesel_new: M::DieselNew,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvSecondaryLookupResource,
{
    insert_resource_inner::<M, _>(
        store,
        diesel_new,
        |new_object, partition_key| {
            Some(M::get_reverse_lookup_key(new_object, partition_key).get_lookup_key())
        },
    )
    .await
    .map(Into::into)
}

/// Find by plain key. Redis hit → return model. `NotFound` → Postgres fallback.
/// Other Redis errors are surfaced (not masked) to avoid duplicate inserts.
#[instrument(skip(store, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn find_resource_by_id_inner<M>(
    store: &Storage,
    primary_key: M::PrimaryKeyType,
) -> Result<M::DieselEntity, ContainerError<M::Error>>
where
    M: KvResource,
{
    let key = primary_key.get_partition_key();
    let decided_scheme = decide_storage_scheme_for_find_operation(store).await;
    log_storage_scheme_decision(M::ENTITY_TYPE, "find", &decided_scheme);

    match decided_scheme {
        DecidedStorageScheme::PostgresOnly => M::storage_find(store, &primary_key).await,
        DecidedStorageScheme::Kv(kv_backend) => {
            let key_str = key.to_string();
            let result = kv_backend.find::<M::DieselEntity>(key.clone()).await;

            match result {
                Ok(KvFindResult::Present(v)) => Ok(v),
                Ok(KvFindResult::Absent) => {
                    // Redis miss → fall back to Postgres. In SoftKill this means the key was
                    // never written to Redis, so we read from DB.
                    metrics::KV_CACHE_MISS_COUNT.add(
                        1,
                        metrics_utils::metric_attributes![("resource", M::ENTITY_TYPE)],
                    );
                    M::storage_find(store, &primary_key).await
                }
                Ok(KvFindResult::Deleted) => Err(kv_backend_error::<M::Error>(Report::new(
                    KvError::ValueNotFound(format!("Data was deleted for key {key_str}")),
                ))),
                Err(e) => Err(kv_backend_error::<M::Error>(
                    e.to_redis_failed_response(&key_str),
                )),
            }
        }
    }
}

/// Find by plain key. Redis hit → return model. `NotFound` → Postgres fallback.
/// Other Redis errors are surfaced (not masked) to avoid duplicate inserts.
#[instrument(skip(store, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn find_resource_by_id<M>(
    store: &Storage,
    primary_key: M::PrimaryKeyType,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvResource,
{
    find_resource_by_id_inner::<M>(store, primary_key)
        .await
        .map(Into::into)
}

/// Find by reverse lookup id. Reverse-lookup miss and Redis miss both fall back to Postgres.
#[instrument(skip(store, lookup_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn find_resource_by_lookup_id<M>(
    store: &Storage,
    lookup_key: M::LookupKeyType,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvSecondaryLookupResource,
{
    let decided_scheme = decide_storage_scheme_for_find_operation(store).await;
    log_storage_scheme_decision(M::ENTITY_TYPE, "find_by_lookup", &decided_scheme);
    let lookup_id = lookup_key.get_lookup_key();
    match decided_scheme {
        DecidedStorageScheme::PostgresOnly => M::storage_find_by_lookup(store, &lookup_key).await,
        DecidedStorageScheme::Kv(kv_backend) => {
            let reverse_lookup_partition_key = PartitionKey::ReverseLookup {
                lookup_id: &lookup_id.lookup_id,
            };
            let reverse_lookup_key_str = reverse_lookup_partition_key.to_string();
            let key_str = match kv_backend
                .find::<types::ReverseLookup>(reverse_lookup_partition_key)
                .await
            {
                Ok(KvFindResult::Present(lookup)) => lookup.get_partition_key().to_string(),
                Ok(KvFindResult::Absent) => {
                    metrics::KV_CACHE_MISS_COUNT.add(
                        1,
                        metrics_utils::metric_attributes![("resource", M::ENTITY_TYPE)],
                    );
                    return M::storage_find_by_lookup(store, &lookup_key).await;
                }
                Ok(KvFindResult::Deleted) => {
                    return Err(kv_backend_error::<M::Error>(Report::new(
                        KvError::ValueNotFound(format!(
                            "Data was deleted for reverse lookup key {}",
                            lookup_id.lookup_id
                        )),
                    )));
                }
                Err(err) => {
                    return Err(kv_backend_error::<M::Error>(
                        err.to_redis_failed_response(&reverse_lookup_key_str),
                    ));
                }
            };

            let result = kv_backend
                .find::<M::DieselEntity>(PartitionKey::CombinationKey {
                    combination: &key_str,
                })
                .await;

            match result {
                Ok(KvFindResult::Present(v)) => Ok(v.into()),
                Ok(KvFindResult::Absent) => {
                    // Redis miss → fall back to Postgres. In SoftKill this means the key was
                    // never written to Redis, so we read from DB.
                    metrics::KV_CACHE_MISS_COUNT.add(
                        1,
                        metrics_utils::metric_attributes![("resource", M::ENTITY_TYPE)],
                    );
                    M::storage_find_by_lookup(store, &lookup_key).await
                }
                Ok(KvFindResult::Deleted) => Err(kv_backend_error::<M::Error>(Report::new(
                    KvError::ValueNotFound(format!("Data was deleted for key {key_str}")),
                ))),
                Err(e) => Err(kv_backend_error::<M::Error>(
                    e.to_redis_failed_response(&key_str),
                )),
            }
        }
    }
}

#[instrument(skip(store, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn find_optional_resource_by_id<M>(
    store: &Storage,
    primary_key: M::PrimaryKeyType,
) -> Result<Option<M>, ContainerError<M::Error>>
where
    M: KvResource,
{
    match find_resource_by_id(store, primary_key).await {
        Ok(resource) => Ok(Some(resource)),
        Err(err) if err.get_inner().is_not_found() => Ok(None),
        Err(err) => Err(err),
    }
}

#[instrument(skip(store, update, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn update_resource_by_id<M>(
    store: &Storage,
    mut update: M::DieselUpdate,
    primary_key: M::PrimaryKeyType,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvUpdatableResource,
    M::PrimaryKeyType: Clone,
    M::DieselEntity: Clone,
{
    let (decided_scheme, cached) = {
        let key = primary_key.get_partition_key();
        decide_storage_scheme_for_mutate_operation::<M>(store, &key).await?
    };
    log_storage_scheme_decision(M::ENTITY_TYPE, "update", &decided_scheme);
    let scheme = decided_scheme.storage_scheme();
    M::set_update_storage_scheme(&mut update, scheme);

    match decided_scheme {
        DecidedStorageScheme::PostgresOnly => M::storage_update(store, update, primary_key).await,
        DecidedStorageScheme::Kv(kv_backend) => {
            let key = primary_key.get_partition_key();
            let current = match cached {
                Some(resource) => resource,
                None => find_resource_by_id_inner::<M>(store, primary_key.clone()).await?,
            };
            let update_query = M::generate_update_drainer_query(&update, &primary_key)
                .map_err(kv_backend_error::<M::Error>)?;
            let updated_model = M::apply_update(update, current);
            let updated_resource = updated_model.clone().into();

            let key_str = key.to_string();
            kv_backend
                .update(key.clone(), &updated_model, update_query)
                .await
                .map_err(|e| kv_backend_error::<M::Error>(e.to_redis_failed_response(&key_str)))?;

            Ok(updated_resource)
        }
    }
}

#[instrument(skip(store, primary_key), fields(resource = M::ENTITY_TYPE))]
async fn delete_resource_by_id_inner<M>(
    store: &Storage,
    primary_key: M::PrimaryKeyType,
) -> Result<usize, ContainerError<M::Error>>
where
    M: KvDeletableResource,
{
    let (decided_scheme, _) = {
        let key = primary_key.get_partition_key();
        decide_storage_scheme_for_mutate_operation::<M>(store, &key).await?
    };
    log_storage_scheme_decision(M::ENTITY_TYPE, "delete", &decided_scheme);

    match decided_scheme {
        DecidedStorageScheme::PostgresOnly => M::storage_delete(store, primary_key).await,
        DecidedStorageScheme::Kv(kv_backend) => {
            let key = primary_key.get_partition_key();
            let delete_query = M::generate_delete_drainer_query(&primary_key)
                .map_err(kv_backend_error::<M::Error>)?;

            let key_str = key.to_string();
            kv_backend
                .delete::<M::DieselEntity>(key.clone(), delete_query)
                .await
                .map_err(|e| kv_backend_error::<M::Error>(e.to_redis_failed_response(&key_str)))
        }
    }
}

#[instrument(skip(store, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn delete_resource_by_id<M>(
    store: &Storage,
    primary_key: M::PrimaryKeyType,
) -> Result<usize, ContainerError<M::Error>>
where
    M: KvDeleteWithoutLookup,
{
    delete_resource_by_id_inner::<M>(store, primary_key).await
}

#[instrument(skip(store, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn delete_resource_by_id_with_reverse_lookup<M>(
    store: &Storage,
    primary_key: M::PrimaryKeyType,
) -> Result<usize, ContainerError<M::Error>>
where
    M: KvDeletableWithLookup,
    M::PrimaryKeyType: Clone,
{
    let reverse_lookup_key = find_optional_resource_by_id::<M>(store, primary_key.clone())
        .await?
        .map(|resource| M::get_reverse_lookup_key_from_resource(&resource));

    let deleted_rows = delete_resource_by_id_inner::<M>(store, primary_key).await?;

    if let Some(reverse_lookup_key) = reverse_lookup_key {
        store
            .delete_reverse_lookup(&reverse_lookup_key.lookup_id)
            .await
            .map_err(|err| {
                kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable(format!("failed to delete reverse lookup record: {err}")),
                )
            })?;
    }

    Ok(deleted_rows)
}

#[instrument(skip(store, lookup_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn find_optional_resource_by_lookup_id<M>(
    store: &Storage,
    lookup_key: M::LookupKeyType,
) -> Result<Option<M>, ContainerError<M::Error>>
where
    M: KvSecondaryLookupResource,
{
    match find_resource_by_lookup_id(store, lookup_key).await {
        Ok(resource) => Ok(Some(resource)),
        Err(err) if err.get_inner().is_not_found() => Ok(None),
        Err(err) => Err(err),
    }
}
