use std::{fmt::Debug, sync::Arc};

use error_stack::ResultExt;
use hyperswitch_redis_interface::{
    RedisConnectionPool,
    errors::RedisError,
    types::{HsetnxReply, RedisEntryId, SetnxReply},
};
use serde::de;
use tracing::debug;

use super::{
    constraints::UniqueConstraints,
    metrics,
    partition_key::{KvStorePartition, PartitionKey},
    serializable_query::SerializableQuery,
};
use crate::logger;

/// Trait providing access to the Redis connection pool.
///
/// Vendored from `storage_impl/src/redis/kv_store.rs`.
/// Implemented by [`crate::storage::Storage`] when the `kv` feature is active.
pub(crate) trait RedisConnInterface {
    fn get_redis_conn(
        &self,
    ) -> error_stack::Result<Arc<RedisConnectionPool>, RedisError>;
}

/// Everything the KV wrapper needs from the store beyond a Redis connection.
///
/// In hyperswitch this is `KVRouterStore<T>`; here we use a trait so that
/// [`crate::storage::Storage`] can supply the values it holds directly.
pub(crate) trait KvStoreContext: RedisConnInterface {
    fn ttl_for_kv(&self) -> u32;
    fn drainer_stream_name(&self, shard_key: &str) -> String;
    fn drainer_num_partitions(&self) -> u8;
    fn request_id(&self) -> &str;
}

// ─── error_stack 0.4 ↔ 0.5 bridge ──────────────────────────────────────────

/// Reconstruct an owned [`RedisError`] from a `&RedisError`.
///
/// `RedisError` does not implement `Clone`, so we rebuild an owned value from
/// the variants our consumers actually discriminate.
fn redis_error_from_ref(err: &RedisError) -> RedisError {
    match err {
        RedisError::NotFound => RedisError::NotFound,
        RedisError::SetNxFailed => RedisError::SetNxFailed,
        RedisError::SetAddMembersFailed => RedisError::SetAddMembersFailed,
        _ => RedisError::UnknownResult,
    }
}

/// Extension trait that bridges `error_stack` 0.4 `Report<RedisError>` (used by
/// `redis_interface`) into the `error_stack` 0.5 `Report<RedisError>` used by
/// card-vault.
trait BridgeRedis<T> {
    fn bridge(self) -> error_stack::Result<T, RedisError>;
}

impl<T> BridgeRedis<T> for Result<T, error_stack_04::Report<RedisError>> {
    fn bridge(self) -> error_stack::Result<T, RedisError> {
        self.map_err(|e| {
            let ctx = e.current_context();
            let redis_err = redis_error_from_ref(ctx);
            error_stack::Report::new(redis_err)
        })
    }
}

// ─── KvOperation / KvResult ─────────────────────────────────────────────────

/// An enum to represent what operation to do on Redis.
///
/// - `Get` / `SetNx` are used by `fingerprint` and `hash_table`.
/// - `HGet` / `HSetNx` / `Hset` are used by composite-keyed tables (`locker`, `vault`).
pub(crate) enum KvOperation<'a, S: serde::Serialize + Debug> {
    SetNx(&'a S, SerializableQuery),
    Get,
    HGet(&'a str),
    HSetNx(&'a str, &'a S, SerializableQuery),
    Hset((String, String), SerializableQuery),
}

/// The result of a KV operation.
#[derive(Debug)]
pub(crate) enum KvResult<T: de::DeserializeOwned> {
    Get(T),
    SetNx(SetnxReply),
    HGet(T),
    HSetNx(HsetnxReply),
    Hset(()),
}

impl<T: de::DeserializeOwned> KvResult<T> {
    pub(crate) fn try_into_setnx(self) -> Result<SetnxReply, RedisError> {
        match self {
            Self::SetNx(v) => Ok(v),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub(crate) fn try_into_hget(self) -> Result<T, RedisError> {
        match self {
            Self::HGet(v) => Ok(v),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub(crate) fn try_into_hsetnx(self) -> Result<HsetnxReply, RedisError> {
        match self {
            Self::HSetNx(v) => Ok(v),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub(crate) fn try_into_hset(self) -> Result<(), RedisError> {
        match self {
            Self::Hset(()) => Ok(()),
            _ => Err(RedisError::UnknownResult),
        }
    }
}

impl<T> std::fmt::Display for KvOperation<'_, T>
where
    T: serde::Serialize + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SetNx(_, _) => f.write_str("Setnx"),
            Self::Get => f.write_str("Get"),
            Self::HGet(_) => f.write_str("Hget"),
            Self::HSetNx(_, _, _) => f.write_str("HSetNx"),
            Self::Hset(_, _) => f.write_str("Hset"),
        }
    }
}

// ─── kv_wrapper ─────────────────────────────────────────────────────────────

pub(crate) async fn kv_wrapper<'a, T, S>(
    store: &impl KvStoreContext,
    op: KvOperation<'a, S>,
    partition_key: PartitionKey<'a>,
) -> error_stack::Result<KvResult<T>, RedisError>
where
    T: de::DeserializeOwned,
    S: serde::Serialize + Debug + KvStorePartition + UniqueConstraints + Sync,
{
    let redis_conn = store.get_redis_conn()?;

    let key = format!("{partition_key}");

    let type_name = std::any::type_name::<T>();
    let operation = op.to_string();

    let ttl = store.ttl_for_kv();

    let result = async {
        match op {
            KvOperation::SetNx(value, query) => {
                debug!(kv_operation = %operation, ?value);

                if let Err(err) = value.check_for_constraints(&redis_conn).await.bridge() {
                    if matches!(err.current_context(), RedisError::SetAddMembersFailed) {
                        return Ok(KvResult::SetNx(SetnxReply::KeyNotSet));
                    }
                    return Err(err);
                }

                let result = redis_conn
                    .serialize_and_set_key_if_not_exist(&key.into(), value, Some(i64::from(ttl)))
                    .await
                    .bridge()?;

                if matches!(result, SetnxReply::KeySet) {
                    push_to_drainer_stream::<S>(store, query, partition_key).await?;
                    Ok(KvResult::SetNx(result))
                } else {
                    Ok(KvResult::SetNx(SetnxReply::KeyNotSet))
                }
            }

            KvOperation::Get => {
                let result = redis_conn
                    .get_and_deserialize_key(&key.into(), type_name)
                    .await
                    .bridge()?;
                Ok(KvResult::Get(result))
            }

            KvOperation::HGet(field) => {
                let result = redis_conn
                    .get_hash_field_and_deserialize(&key.into(), field, type_name)
                    .await
                    .bridge()?;
                Ok(KvResult::HGet(result))
            }

            KvOperation::HSetNx(field, value, query) => {
                debug!(kv_operation = %operation, ?value);

                // SADD for unique constraints may fail if the key already
                // exists (e.g. upsert on an existing row).  In that case we
                // return `KeyNotSet` so the caller can fall through to the
                // update path — mirroring how `SetNx` handles it.
                if let Err(err) = value.check_for_constraints(&redis_conn).await.bridge() {
                    if matches!(err.current_context(), RedisError::SetAddMembersFailed) {
                        return Ok(KvResult::HSetNx(HsetnxReply::KeyNotSet));
                    }
                    return Err(err);
                }

                let result = redis_conn
                    .serialize_and_set_hash_field_if_not_exist(
                        &key.into(),
                        field,
                        value,
                        Some(ttl),
                    )
                    .await
                    .bridge()?;

                if matches!(result, HsetnxReply::KeySet) {
                    push_to_drainer_stream::<S>(store, query, partition_key).await?;
                    Ok(KvResult::HSetNx(result))
                } else {
                    Err(error_stack::Report::new(RedisError::SetNxFailed))
                }
            }

            KvOperation::Hset((field, serialized), query) => {
                debug!(kv_operation = %operation, field = %field);

                redis_conn
                    .set_hash_fields(&key.into(), vec![(field, serialized)], Some(i64::from(ttl)))
                    .await
                    .bridge()?;

                push_to_drainer_stream::<S>(store, query, partition_key).await?;

                Ok(KvResult::Hset(()))
            }
        }
    };

    result
        .await
        .inspect(|_| {
            debug!(kv_operation = %operation, status = "success");
            metrics::KV_OPERATION_SUCCESSFUL
                .add(1, crate::metric_attributes!(("operation", operation.clone())));
        })
        .inspect_err(|err| {
            match err.current_context() {
                RedisError::NotFound => {
                    debug!(kv_operation = %operation, status = "not_found");
                }
                other => {
                    logger::error!(kv_operation = %operation, status = "error", error = ?other);
                    metrics::KV_OPERATION_FAILED
                        .add(1, crate::metric_attributes!(("operation", operation.clone())));
                }
            }
        })
}

// ─── push_to_drainer_stream ─────────────────────────────────────────────────

async fn push_to_drainer_stream<R>(
    store: &impl KvStoreContext,
    serializable_query: SerializableQuery,
    partition_key: PartitionKey<'_>,
) -> error_stack::Result<(), RedisError>
where
    R: KvStorePartition,
{
    let global_id = format!("{partition_key}");
    let request_id = store.request_id().to_string();

    let shard_key = R::shard_key(partition_key, store.drainer_num_partitions());
    let stream_name = store.drainer_stream_name(&shard_key);

    let operation_str = serializable_query.operation().to_string();
    let entity_type_str = serializable_query.entity_type();

    let redis_conn = store.get_redis_conn()?;

    redis_conn
        .stream_append_entry(
            &stream_name.into(),
            &RedisEntryId::AutoGeneratedID,
            serializable_query
                .to_field_value_pairs(request_id, global_id)
                .change_context(RedisError::JsonSerializationFailed)?,
        )
        .await
        .bridge()
        .map(|_| {
            metrics::KV_PUSHED_TO_DRAINER.add(
                1,
                crate::metric_attributes!(
                    ("operation", operation_str.clone()),
                    ("entity_type", entity_type_str.clone()),
                ),
            );
        })
        .inspect_err(|error| {
            metrics::KV_FAILED_TO_PUSH_TO_DRAINER.add(
                1,
                crate::metric_attributes!(
                    ("operation", operation_str.clone()),
                    ("entity_type", entity_type_str.clone()),
                ),
            );
            logger::error!(?error, "Failed to add entry in drainer stream");
        })
        .change_context(RedisError::StreamAppendFailed)
}
