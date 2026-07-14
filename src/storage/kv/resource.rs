//! Generic KV resource trait, key-shape locators, and CRUD helpers.
//!
//! Stores and returns the Diesel Queryable model (not the `New` projection).

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
        ContainerError,
        kv::{KvError, RedisErrorExt},
    },
    storage::Storage,
};

/// A KV-routed table's Diesel Queryable model: stored in Redis, read back, returned to
/// callers.
pub(crate) trait KvResource:
    serde::Serialize
    + serde::de::DeserializeOwned
    + std::fmt::Debug
    + KvStorePartition
    + EntityType
    + Sync
    + Send
    + Sized
{
    type Error: error_stack::Context + Send + Sync + 'static + for<'a> From<&'a KvError>;

    type DieselNew: Into<Self>;

    fn set_storage_scheme(diesel_new: &mut Self::DieselNew, scheme: StorageScheme);

    /// Drainer INSERT — built from the `Insertable` `New` projection (the model is not
    /// `Insertable`). Implementations rebuild the `New` struct from the model's fields.
    async fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
        store: &Storage,
    ) -> Result<SerializableQuery, ContainerError<Self::Error>>;

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self, ContainerError<Self::Error>>;

    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<Self>, ContainerError<Self::Error>>;
}

/// Locator for a find. `Id` = plain-keyed (single HGet/HSetNx field).
/// Extend with `LookupId(String)` for reverse-lookup tables when their first consumer lands.
pub(crate) enum FindResourceBy<'a> {
    Id(PartitionKey<'a>),
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

/// Insert via HSetNx. `KeyNotSet` → `Duplicate`. `PostgresOnly` → `storage_insert`.
/// On the RedisKv path the model's serial `id` is unresolved (e.g. `0`); the drainer
/// assigns it on PG replay. Callers only see the business id (`fingerprint_id`).
#[instrument(skip(store, diesel_new, partition_key), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn insert_resource<M>(
    store: &Storage,
    mut diesel_new: M::DieselNew,
    partition_key: PartitionKey<'_>,
) -> Result<M, ContainerError<M::Error>>
where
    M: KvResource,
{
    let scheme = decide(store, Op::Insert).await;
    M::set_storage_scheme(&mut diesel_new, scheme);

    match scheme {
        StorageScheme::PostgresOnly => M::storage_insert(diesel_new, store).await,
        StorageScheme::RedisKv => {
            let drainer_query = M::generate_insert_drainer_query(&diesel_new, store).await?;

            let key_str = partition_key.to_string();
            // apply the changes in application
            let diesel_model = diesel_new.into();
            let reply = kv_wrapper::<(), M>(
                store,
                KvOperation::HSetNx(&key_str, &diesel_model, drainer_query),
                partition_key,
            )
            .await
            .map_err(|e| kv_backend_error::<M::Error>(e.to_redis_failed_response(&key_str)))?;

            match reply.try_into_hsetnx() {
                Ok(HsetnxReply::KeySet) => Ok(diesel_model),
                Ok(HsetnxReply::KeyNotSet) => Err(kv_duplicate_error::<M::Error>(&key_str)),
                Err(e) => Err(kv_backend_error::<M::Error>(
                    Report::new(e).change_context(KvError::Backend),
                )),
            }
        }
    }
}

/// Find by plain key. Redis hit → return model. `NotFound` → Postgres fallback.
/// Other Redis errors are surfaced (not masked) to avoid duplicate inserts.
#[instrument(skip(store, find_by), fields(resource = M::ENTITY_TYPE))]
pub(crate) async fn find_optional_resource_by_id<M>(
    store: &Storage,
    find_by: FindResourceBy<'_>,
) -> Result<Option<M>, ContainerError<M::Error>>
where
    M: KvResource,
{
    let scheme = decide(store, Op::Find).await;
    let FindResourceBy::Id(key) = find_by;

    match scheme {
        StorageScheme::PostgresOnly => M::storage_find_optional(store, &key).await,
        StorageScheme::RedisKv => {
            let key_str = key.to_string();
            let result =
                kv_wrapper::<M, M>(store, KvOperation::<M>::HGet(&key_str), key.clone()).await;

            match result {
                Ok(KvResult::HGet(v)) => Ok(Some(v)),
                Err(e) if matches!(e.current_context(), RedisError::NotFound) => {
                    // Redis miss → fall back to Postgres. In SoftKill this means the key was
                    // never written to Redis, so we read from DB.
                    super::metrics::KV_MISS
                        .add(1, crate::metric_attributes![("resource", M::ENTITY_TYPE)]);
                    M::storage_find_optional(store, &key).await
                }
                Err(e) => Err(kv_backend_error::<M::Error>(
                    e.to_redis_failed_response(&key_str),
                )),
                Ok(KvResult::HSetNx(_)) => Err(kv_backend_error::<M::Error>(
                    Report::new(KvError::Backend)
                        .attach_printable("unexpected HSetNx result for an HGet operation"),
                )),
            }
        }
    }
}
