//! Generic KV resource trait, key-shape markers, and CRUD helpers.

use error_stack::Report;
use hyperswitch_redis_interface::types::HsetnxReply;
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
    error::{ContainerError, KvError, RedisErrorExt},
    storage::Storage,
};

/// A table's KV value type — serde-able, written to Redis, replayed to PG by the drainer.
pub(crate) trait StorageResource:
    serde::Serialize
    + serde::de::DeserializeOwned
    + std::fmt::Debug
    + KvStorePartition
    + EntityType
    + Sync
    + Send
{
    type Domain;
    type Error: error_stack::Context + Send + Sync + 'static + From<KvWriteError>;

    fn into_domain(self) -> Self::Domain;
    fn set_storage_scheme(&mut self, scheme: StorageScheme);
    fn insert_drainer_query(&self) -> error_stack::Result<SerializableQuery, KvError>;
    async fn storage_insert(
        self,
        store: &Storage,
    ) -> Result<Self::Domain, ContainerError<Self::Error>>;
}

/// Find-optional DB op for KV-routed tables.
///
/// `StorageResource` is a super-trait bound: implementing `KvFindOptional`
/// implies `StorageResource`, so callers need only name `KvFindOptional`.
pub(crate) trait KvFindOptional: StorageResource {
    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<Self::Domain>, ContainerError<Self::Error>>;
}

/// Marker for plain-keyed tables (Redis Get/SetNx).
pub(crate) trait PlainKeyed {}

/// Errors from the generic KV insert helper.
#[derive(Debug)]
pub(crate) enum KvWriteError {
    Duplicate,
    Backend(Report<KvError>),
}

fn kv_write_error<E: From<KvWriteError> + error_stack::Context>(
    e: KvWriteError,
) -> ContainerError<E> {
    ContainerError::from(E::from(e))
}

async fn decide(store: &Storage, op: Op) -> StorageScheme {
    let state = store.kv_settings().await;
    decide_storage_scheme(state, op)
}

#[instrument(skip(store, partition_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn find_optional_plain_resource<M>(
    store: &Storage,
    partition_key: PartitionKey<'_>,
) -> Result<Option<M::Domain>, ContainerError<M::Error>>
where
    M: PlainKeyed + KvFindOptional,
{
    let scheme = decide(store, Op::Find).await;

    if matches!(scheme, StorageScheme::RedisKv) {
        let result = kv_wrapper::<M, M>(
            store,
            KvOperation::<M>::HGet(M::ENTITY_TYPE),
            partition_key.clone(),
        )
        .await;

        if let Ok(KvResult::HGet(v)) = result {
            Ok(Some(M::into_domain(v)))
        } else {
            // Redis miss or error — fall through to Postgres.
            M::storage_find_optional(store, &partition_key).await
        }
    } else {
        M::storage_find_optional(store, &partition_key).await
    }
}

/// Insert via SetNx. KeyNotSet → Duplicate (no PG fallback). PostgresOnly → storage_insert.
///
/// In the `RedisKv` path the returned domain object is built from the *model*
/// (not a DB row), so its serial PK (`id`) is unpopulated (`0`). The PK is
/// assigned by the drainer on replay. Callers must not read `id` — use
/// `fingerprint_id` (the caller-supplied nanoid) instead.
#[instrument(skip(store, model, partition_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn insert_plain_resource<M>(
    store: &Storage,
    mut model: M,
    partition_key: PartitionKey<'_>,
) -> Result<M::Domain, ContainerError<M::Error>>
where
    M: StorageResource + PlainKeyed,
{
    let scheme = decide(store, Op::Insert).await;

    model.set_storage_scheme(scheme);

    if matches!(scheme, StorageScheme::RedisKv) {
        let key_str = partition_key.to_string();

        let drainer_query = model
            .insert_drainer_query()
            .map_err(|e| kv_write_error::<M::Error>(KvWriteError::Backend(e)))?;

        let reply = kv_wrapper::<(), M>(
            store,
            KvOperation::HSetNx(M::ENTITY_TYPE, &model, drainer_query),
            partition_key,
        )
        .await
        .map_err(|e| {
            kv_write_error::<M::Error>(KvWriteError::Backend(e.to_redis_failed_response(&key_str)))
        })?;

        return match reply.try_into_hsetnx() {
            Ok(HsetnxReply::KeySet) => Ok(M::into_domain(model)),
            Ok(HsetnxReply::KeyNotSet) => Err(kv_write_error::<M::Error>(KvWriteError::Duplicate)),
            Err(e) => Err(kv_write_error::<M::Error>(KvWriteError::Backend(
                Report::new(e).change_context(KvError::Backend),
            ))),
        };
    }

    model.storage_insert(store).await
}
