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
    scheme::{Op, decide_storage_scheme},
    serializable_query::SerializableQuery,
    wrapper::{KvOperation, KvResult, kv_wrapper},
};
use crate::{
    error::{
        ContainerError, StorageErrorExt,
        kv::{KvError, RedisErrorExt},
    },
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

/// A KV-routed table's resource model. Its `DieselEntity` is stored in Redis and
/// converted back to the resource returned to callers.
pub(crate) trait KvResource:
    std::fmt::Debug + KvStorePartition + EntityType + Sync + Send + Sized
{
    type Error: error_stack::Context
        + Send
        + Sync
        + 'static
        + StorageErrorExt
        + for<'a> From<&'a KvError>;

    type DieselNew: Into<Self::DieselEntity>;

    type DieselEntity: serde::Serialize
        + serde::de::DeserializeOwned
        + std::fmt::Debug
        + KvStorePartition
        + Sync
        + Into<Self>;

    /// A type that represent the primary key of this table
    /// could be composite key as well.
    type PrimaryKeyType: GetPartitionKey;

    fn set_storage_scheme(diesel_new: &mut Self::DieselNew, scheme: StorageScheme);

    /// Drainer INSERT — built from the `Insertable` `New` projection (the model is not
    /// `Insertable`). Implementations rebuild the `New` struct from the model's fields.
    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, KvError>;

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self, ContainerError<Self::Error>>;

    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<Self::Error>>;
}

pub(crate) trait KvDeleteResource: KvResource {
    fn generate_delete_drainer_query(
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, KvError>;

    async fn storage_delete(
        store: &Storage,
        pk: Self::PrimaryKeyType,
    ) -> Result<usize, ContainerError<Self::Error>>;
}

pub(crate) trait KvUpdateResource: KvResource {
    fn generate_update_drainer_query(
        new_object: &Self::DieselNew,
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, KvError>;

    fn apply_update(new_object: Self::DieselNew, current: Self) -> Self::DieselEntity;

    async fn storage_update(
        store: &Storage,
        new_object: Self::DieselNew,
        pk: Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<Self::Error>>;
}

/// KV reverse lookup trait
pub(crate) trait KvReverseLookupResource: KvResource {
    type LookupKeyType: GetLookupKey;

    fn get_reverse_lookup_key(
        new_object: &Self::DieselNew,
        partition_key: &PartitionKey<'_>,
    ) -> Self::LookupKeyType;

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

async fn decide(store: &Storage, op: Op) -> StorageScheme {
    let state = store.kv_settings().await;
    decide_storage_scheme(state, op)
}

async fn insert_resource_inner<M, F>(
    store: &Storage,
    mut diesel_new: M::DieselNew,
    partition_key: PartitionKey<'_>,
    get_reverse_lookup_key: F,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvResource,
    F: FnOnce(&M::DieselNew, &PartitionKey<'_>) -> Option<ReverseLookupKey>,
{
    let scheme = decide(store, Op::Insert).await;
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
                Ok(HsetnxReply::KeySet) => Ok(diesel_entity.into()),
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
    M: KvResource,
{
    insert_resource_inner::<M, _>(store, diesel_new, partition_key, |_, _| None).await
}

#[instrument(skip(store, diesel_new, partition_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn insert_resource_with_reverse_lookup<M>(
    store: &Storage,
    diesel_new: M::DieselNew,
    partition_key: PartitionKey<'_>,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvReverseLookupResource,
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
    let scheme = decide(store, Op::Find).await;
    let key = primary_key.get_partition_key();

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
                Ok(KvResult::HGet(v)) => Ok(v.into()),
                Err(e) if matches!(e.current_context(), RedisError::NotFound) => {
                    // Redis miss → fall back to Postgres. In SoftKill this means the key was
                    // never written to Redis, so we read from DB.
                    super::metrics::KV_MISS
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

/// Find by reverse lookup id. Reverse-lookup miss and Redis miss both fall back to Postgres.
#[instrument(skip(store, lookup_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn find_resource_by_lookup_id<M>(
    store: &Storage,
    lookup_key: M::LookupKeyType,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvReverseLookupResource,
{
    let scheme = decide(store, Op::Find).await;
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
                    super::metrics::KV_MISS
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
                    super::metrics::KV_MISS
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

#[instrument(skip(store, diesel_new, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn update_resource_by_id<M>(
    store: &Storage,
    mut diesel_new: M::DieselNew,
    primary_key: M::PrimaryKeyType,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvUpdateResource,
    M::PrimaryKeyType: Clone,
{
    let scheme = decide(store, Op::Update).await;
    M::set_storage_scheme(&mut diesel_new, scheme);

    match scheme {
        StorageScheme::PostgresOnly => M::storage_update(store, diesel_new, primary_key).await,
        StorageScheme::RedisKv => {
            let key = primary_key.get_partition_key();
            let current = find_resource_by_id::<M>(store, primary_key.clone()).await?;
            let update_query = M::generate_update_drainer_query(&diesel_new, &primary_key)
                .map_err(kv_backend_error::<M::Error>)?;
            let updated_model = M::apply_update(diesel_new, current);
            let redis_value = serde_json::to_string(&updated_model).map_err(|err| {
                kv_backend_error::<M::Error>(
                    Report::new(KvError::SerializationFailed)
                        .attach_printable(format!("failed to serialize updated resource: {err}")),
                )
            })?;

            let key_str = key.to_string();
            kv_wrapper::<(), M::DieselEntity>(
                store,
                KvOperation::<M::DieselEntity>::Hset((&key_str, redis_value), update_query),
                key.clone(),
            )
            .await
            .map_err(|e| kv_backend_error::<M::Error>(e.to_redis_failed_response(&key_str)))?
            .try_into_hset()
            .map_err(|e| {
                kv_backend_error::<M::Error>(Report::new(e).change_context(KvError::Backend))
            })?;

            Ok(updated_model.into())
        }
    }
}

#[instrument(skip(store, primary_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn delete_resource_by_id<M>(
    store: &Storage,
    primary_key: M::PrimaryKeyType,
) -> Result<usize, ContainerError<M::Error>>
where
    M: KvDeleteResource,
{
    let key = primary_key.get_partition_key();
    let scheme = decide(store, Op::Delete).await;

    match scheme {
        StorageScheme::PostgresOnly => M::storage_delete(store, primary_key).await,
        StorageScheme::RedisKv => {
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
    M: KvReverseLookupResource,
{
    match find_resource_by_lookup_id(store, lookup_key).await {
        Ok(resource) => Ok(Some(resource)),
        Err(err) if err.get_inner().is_not_found() => Ok(None),
        Err(err) => Err(err),
    }
}
