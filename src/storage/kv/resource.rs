//! Generic KV resource trait, key-shape locators, and CRUD helpers.
//!
//! Stores the Diesel table-mapped entity in Redis and returns the resource model.

use error_stack::Report;
use hyperswitch_redis_interface::{errors::RedisError, types::HsetnxReply};
use tracing::instrument;

use super::{
    StorageScheme,
    entity::EntityType,
    partition_key::{KvStorePartition, PartitionKey},
    scheme::KvState,
    serializable_query::SerializableQuery,
    wrapper::{KvOperation, KvResult, kv_wrapper},
};
use crate::{
    error::{
        ContainerError, StorageErrorExt,
        kv::{KvError, RedisErrorExt},
    },
    observability::metrics,
    storage::{ReverseLookupInterface, Storage, types},
};

/// Secondary-to-primary mapping metadata emitted by a KV resource.
pub(crate) struct ReverseLookupKey {
    pub lookup_id: String,
}

pub(crate) trait GetPartitionKey {
    fn get_partition_key(&self) -> PartitionKey<'_>;
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
    /// secondary-key to primary-key mapping.
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
    type PrimaryKeyType: GetPartitionKey;

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

async fn decide_storage_scheme_for_find_operation(store: &Storage) -> StorageScheme {
    let state = store.kv_settings().await;
    match state {
        KvState::Disabled => StorageScheme::PostgresOnly,
        // in softkill mode as well, always attempt RedisKv and fallback to postgres.
        KvState::Enabled | KvState::SoftKill => StorageScheme::RedisKv,
    }
}

async fn decide_storage_scheme_for_insert_operation(store: &Storage) -> StorageScheme {
    let state = store.kv_settings().await;
    match state {
        // in disabled and softkill mode, always push new inserts to PG
        KvState::Disabled | KvState::SoftKill => StorageScheme::PostgresOnly,
        KvState::Enabled => StorageScheme::RedisKv,
    }
}

/// Call this to decide storage scheme for Update and Delete operations
async fn decide_storage_scheme_for_mutate_operation<M>(
    store: &Storage,
    partition_key: &PartitionKey<'_>,
) -> Result<(StorageScheme, Option<M::DieselEntity>), ContainerError<M::Error>>
where
    M: KvResource,
{
    let state = store.kv_settings().await;

    match state {
        KvState::Disabled => Ok((StorageScheme::PostgresOnly, None)),
        KvState::Enabled => Ok((StorageScheme::RedisKv, None)),
        KvState::SoftKill => {
            // With this implementation, Hot keys may never recover out of KV.
            let partition_key_str = partition_key.to_string();
            let result = kv_wrapper::<M::DieselEntity, M::DieselEntity>(
                store,
                KvOperation::<M::DieselEntity>::HGet(&partition_key_str),
                partition_key.clone(),
            )
            .await;

            match result {
                // return the found redis item so that if the caller is doing update operation, updates can be applied.
                Ok(KvResult::HGet(v)) => Ok((StorageScheme::RedisKv, Some(v))),
                Err(e) if matches!(e.current_context(), RedisError::NotFound) => {
                    crate::observability::metrics::KV_CACHE_MISS_COUNT
                        .add(1, crate::metric_attributes![("resource", M::ENTITY_TYPE)]);
                    Ok((StorageScheme::PostgresOnly, None))
                }
                Err(e) => Err(kv_backend_error::<M::Error>(
                    e.to_redis_failed_response(&partition_key_str),
                )),
                Ok(KvResult::HSetNx(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected HSetNx result for an HGet operation"),
                )),
                Ok(KvResult::Hset(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected Hset result for an HGet operation"),
                )),
                Ok(KvResult::HDel(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected HDel result for an HGet operation"),
                )),
            }
        }
    }
}

async fn insert_resource_inner<M, F>(
    store: &Storage,
    mut diesel_new: M::DieselNew,
    partition_key: PartitionKey<'_>,
    get_reverse_lookup_key: F,
) -> Result<M::DieselEntity, ContainerError<M::Error>>
where
    M: KvResource,
    F: FnOnce(&M::DieselNew, &PartitionKey<'_>) -> Option<ReverseLookupKey>,
{
    let scheme = decide_storage_scheme_for_insert_operation(store).await;
    M::set_storage_scheme(&mut diesel_new, scheme);

    match scheme {
        StorageScheme::PostgresOnly => M::storage_insert(diesel_new, store).await,
        StorageScheme::RedisKv => {
            let drainer_query = M::generate_insert_drainer_query(&diesel_new)
                .map_err(kv_backend_error::<M::Error>)?;

            let partition_key_str = partition_key.to_string();
            if let Some(reverse_lookup_key) = get_reverse_lookup_key(&diesel_new, &partition_key) {
                store
                    .insert_reverse_lookup(types::ReverseLookupNew {
                        lookup_id: reverse_lookup_key.lookup_id.clone(),
                        secondary_key: reverse_lookup_key.lookup_id,
                        partition_key: partition_key_str.clone(),
                        source: M::ENTITY_TYPE.to_string(),
                        updated_by: scheme.to_string(),
                    })
                    .await
                    .map_err(|err| {
                        kv_backend_error::<M::Error>(
                            Report::new(KvError::Backend).attach_printable(format!(
                                "failed to insert reverse lookup record: {err}"
                            )),
                        )
                    })?;
            }

            let diesel_entity = diesel_new.into();
            let reply = kv_wrapper::<(), M::DieselEntity>(
                store,
                KvOperation::HSetNx(&partition_key_str, &diesel_entity, drainer_query),
                partition_key,
            )
            .await
            .map_err(|e| {
                kv_backend_error::<M::Error>(e.to_redis_failed_response(&partition_key_str))
            })?;

            match reply.try_into_hsetnx() {
                Ok(HsetnxReply::KeySet) => Ok(diesel_entity),
                Ok(HsetnxReply::KeyNotSet) => {
                    Err(kv_duplicate_error::<M::Error>(&partition_key_str))
                }
                Err(e) => Err(kv_backend_error::<M::Error>(
                    Report::new(e).change_context(KvError::Backend),
                )),
            }
        }
    }
}

/// Insert via HSetNx. `KeyNotSet` → `Duplicate`. `PostgresOnly` → `storage_insert`.
/// On the RedisKv path the model's serial `id` is unresolved (e.g. `0`); the drainer
/// assigns it on PG replay. Callers only see the business id (`fingerprint_id`).
#[instrument(skip(store, diesel_new, partition_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn insert_resource<M>(
    store: &Storage,
    diesel_new: M::DieselNew,
    partition_key: PartitionKey<'_>,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvResource<InsertStrategy = DirectInsert>,
{
    insert_resource_inner::<M, _>(store, diesel_new, partition_key, |_, _| None)
        .await
        .map(Into::into)
}

#[instrument(skip(store, diesel_new, partition_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn insert_resource_with_reverse_lookup<M>(
    store: &Storage,
    diesel_new: M::DieselNew,
    partition_key: PartitionKey<'_>,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvSecondaryLookupResource,
{
    insert_resource_inner::<M, _>(
        store,
        diesel_new,
        partition_key,
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
    let scheme = decide_storage_scheme_for_find_operation(store).await;

    match scheme {
        StorageScheme::PostgresOnly => M::storage_find(store, &primary_key).await,
        StorageScheme::RedisKv => {
            let key_str = key.to_string();
            let result = kv_wrapper::<M::DieselEntity, M::DieselEntity>(
                store,
                KvOperation::<M::DieselEntity>::HGet(&key_str),
                key.clone(),
            )
            .await;

            match result {
                Ok(KvResult::HGet(v)) => Ok(v),
                Err(e) if matches!(e.current_context(), RedisError::NotFound) => {
                    // Redis miss → fall back to Postgres. In SoftKill this means the key was
                    // never written to Redis, so we read from DB.
                    metrics::KV_CACHE_MISS_COUNT
                        .add(1, crate::metric_attributes![("resource", M::ENTITY_TYPE)]);
                    M::storage_find(store, &primary_key).await
                }
                Err(e) => Err(kv_backend_error::<M::Error>(
                    e.to_redis_failed_response(&key_str),
                )),
                Ok(KvResult::HSetNx(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected HSetNx result for an HGet operation"),
                )),
                Ok(KvResult::Hset(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected Hset result for an HGet operation"),
                )),
                Ok(KvResult::HDel(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected HDel result for an HGet operation"),
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
    let scheme = decide_storage_scheme_for_find_operation(store).await;
    let lookup_id = lookup_key.get_lookup_key();
    match scheme {
        StorageScheme::PostgresOnly => M::storage_find_by_lookup(store, &lookup_key).await,
        StorageScheme::RedisKv => {
            let key_str = match store.find_by_lookup_id(&lookup_id.lookup_id).await {
                Ok(lookup) => lookup.get_partition_key().to_string(),
                Err(err)
                    if matches!(
                        err.get_inner(),
                        crate::error::ReverseLookupDBError::NotFoundError
                    ) =>
                {
                    metrics::KV_CACHE_MISS_COUNT
                        .add(1, crate::metric_attributes![("resource", M::ENTITY_TYPE)]);
                    return M::storage_find_by_lookup(store, &lookup_key).await;
                }
                Err(err) => {
                    return Err(kv_backend_error::<M::Error>(
                        Report::new(KvError::Backend).attach_printable(format!(
                            "failed to find reverse lookup record: {err}"
                        )),
                    ));
                }
            };

            let result = kv_wrapper::<M::DieselEntity, M::DieselEntity>(
                store,
                KvOperation::<M::DieselEntity>::HGet(&key_str),
                PartitionKey::CombinationKey {
                    combination: &key_str,
                },
            )
            .await;

            match result {
                Ok(KvResult::HGet(v)) => Ok(v.into()),
                Err(e) if matches!(e.current_context(), RedisError::NotFound) => {
                    metrics::KV_CACHE_MISS_COUNT
                        .add(1, crate::metric_attributes![("resource", M::ENTITY_TYPE)]);
                    M::storage_find_by_lookup(store, &lookup_key).await
                }
                Err(e) => Err(kv_backend_error::<M::Error>(
                    e.to_redis_failed_response(&key_str),
                )),
                Ok(KvResult::HSetNx(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected HSetNx result for an HGet operation"),
                )),
                Ok(KvResult::Hset(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected Hset result for an HGet operation"),
                )),
                Ok(KvResult::HDel(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected HDel result for an HGet operation"),
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
    let (scheme, cached) = {
        let key = primary_key.get_partition_key();
        decide_storage_scheme_for_mutate_operation::<M>(store, &key).await?
    };
    M::set_update_storage_scheme(&mut update, scheme);

    match scheme {
        StorageScheme::PostgresOnly => M::storage_update(store, update, primary_key).await,
        StorageScheme::RedisKv => {
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
            kv_wrapper::<(), M::DieselEntity>(
                store,
                KvOperation::<M::DieselEntity>::Hset((&key_str, updated_model), update_query),
                key.clone(),
            )
            .await
            .map_err(|e| kv_backend_error::<M::Error>(e.to_redis_failed_response(&key_str)))?
            .try_into_hset()
            .map_err(|e| {
                kv_backend_error::<M::Error>(Report::new(e).change_context(KvError::Backend))
            })?;

            Ok(updated_resource)
        }
    }
}

#[instrument(skip(store, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn delete_resource_by_id<M>(
    store: &Storage,
    primary_key: M::PrimaryKeyType,
) -> Result<usize, ContainerError<M::Error>>
where
    M: KvDeletableResource,
{
    let (scheme, _) = {
        let key = primary_key.get_partition_key();
        decide_storage_scheme_for_mutate_operation::<M>(store, &key).await?
    };

    match scheme {
        StorageScheme::PostgresOnly => M::storage_delete(store, primary_key).await,
        StorageScheme::RedisKv => {
            let key = primary_key.get_partition_key();
            let delete_query = M::generate_delete_drainer_query(&primary_key)
                .map_err(kv_backend_error::<M::Error>)?;

            let key_str = key.to_string();
            let reply = kv_wrapper::<(), M::DieselEntity>(
                store,
                KvOperation::<M::DieselEntity>::HDel(&key_str, delete_query),
                key.clone(),
            )
            .await
            .map_err(|e| kv_backend_error::<M::Error>(e.to_redis_failed_response(&key_str)))?;

            reply.try_into_hdel().map_err(|e| {
                kv_backend_error::<M::Error>(Report::new(e).change_context(KvError::Backend))
            })
        }
    }
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
