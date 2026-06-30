use std::{fmt::Debug, sync::Arc};

use error_stack::ResultExt;
use hyperswitch_redis_interface::{
    RedisConnectionPool,
    errors::RedisError,
    types::{RedisEntryId, SetnxReply},
};
use serde::de;
use tracing::debug;

use super::{
    metrics,
    partition_key::{KvStorePartition, PartitionKey},
    serializable_query::SerializableQuery,
};
use crate::logger;

/// Provides access to the Redis connection pool.
pub(crate) trait RedisConnInterface {
    fn get_redis_conn(&self) -> error_stack::Result<Arc<RedisConnectionPool>, RedisError>;
}

/// Store context required by the KV wrapper.
pub(crate) trait KvStoreContext: RedisConnInterface {
    fn ttl_for_kv(&self) -> u32;
    fn drainer_stream_name(&self, shard_key: &str) -> String;
    fn drainer_num_partitions(&self) -> u8;
    /// Request-id from the Tokio task-local (set by axum middleware).
    fn request_id(&self) -> String {
        super::request_id::current_request_id()
    }
}

/// Reconstruct an owned `RedisError` from a `&RedisError` (not `Clone`).
fn redis_error_from_ref(err: &RedisError) -> RedisError {
    match err {
        RedisError::NotFound => RedisError::NotFound,
        RedisError::SetNxFailed => RedisError::SetNxFailed,
        RedisError::SetAddMembersFailed => RedisError::SetAddMembersFailed,
        _ => RedisError::UnknownResult,
    }
}

/// Bridges `error_stack` 0.4 `Report<RedisError>` → 0.5.
trait BridgeRedis<T> {
    fn bridge(self) -> error_stack::Result<T, RedisError>;
}

impl<T> BridgeRedis<T> for Result<T, error_stack_04::Report<RedisError>> {
    fn bridge(self) -> error_stack::Result<T, RedisError> {
        self.map_err(|e| {
            // Preserve the 0.4 report's diagnostic chain as a printable attachment.
            error_stack::Report::new(redis_error_from_ref(e.current_context()))
                .attach_printable(format!("{e:?}"))
        })
    }
}

/// Operation to perform on Redis.
pub(crate) enum KvOperation<'a, S: serde::Serialize + Debug> {
    SetNx(&'a S, SerializableQuery),
    Get,
}

/// The result of a KV operation.
#[derive(Debug)]
pub(crate) enum KvResult<T: de::DeserializeOwned> {
    Get(T),
    SetNx(SetnxReply),
}

impl<T: de::DeserializeOwned> KvResult<T> {
    pub(crate) fn try_into_setnx(self) -> Result<SetnxReply, RedisError> {
        match self {
            Self::SetNx(v) => Ok(v),
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
        }
    }
}

pub(crate) async fn kv_wrapper<'a, T, S>(
    store: &impl KvStoreContext,
    op: KvOperation<'a, S>,
    partition_key: PartitionKey<'a>,
) -> error_stack::Result<KvResult<T>, RedisError>
where
    T: de::DeserializeOwned,
    S: serde::Serialize + Debug + KvStorePartition + Sync,
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

                // SETNX is the atomic uniqueness guard — the fingerprint hash
                // *is* the Redis key, so a successful SETNX proves no prior
                // writer claimed it. No SADD set needed.
                let result = redis_conn
                    .serialize_and_set_hash_field_if_not_exist(
                        &key.into(),
                        value,
                        Some(i64::from(ttl)),
                    )
                    .await
                    .bridge()?;

                if matches!(result, SetnxReply::KeySet) {
                    // If the drainer push fails, the Redis key remains (TTL-bounded)
                    // with no drainer entry — it blocks re-insert as a duplicate
                    // for the TTL window and never reaches Postgres. We accept this
                    // (matching hyperswitch's eventual-consistency model); alert on
                    // KV_FAILED_TO_PUSH_TO_DRAINER. A best-effort DEL on failure was
                    // rejected: it adds a race window and a second failure mode.
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
        }
    };

    result
        .await
        .inspect(|_| {
            debug!(kv_operation = %operation, status = "success");
            metrics::KV_OPERATION_SUCCESSFUL.add(
                1,
                crate::metric_attributes!(("operation", operation.clone())),
            );
        })
        .inspect_err(
            |err: &error_stack::Report<RedisError>| match err.current_context() {
                RedisError::NotFound => {
                    debug!(kv_operation = %operation, status = "not_found");
                }
                other => {
                    logger::error!(kv_operation = %operation, status = "error", error = ?other);
                    metrics::KV_OPERATION_FAILED.add(
                        1,
                        crate::metric_attributes!(("operation", operation.clone())),
                    );
                }
            },
        )
}

async fn push_to_drainer_stream<R>(
    store: &impl KvStoreContext,
    serializable_query: SerializableQuery,
    partition_key: PartitionKey<'_>,
) -> error_stack::Result<(), RedisError>
where
    R: KvStorePartition,
{
    let global_id = format!("{partition_key}");
    let request_id = store.request_id();

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
