use std::{fmt::Debug, sync::Arc};

use error_stack::ResultExt;
use hyperswitch_redis_interface::{
    RedisConnectionPool,
    errors::RedisError,
    types::{HsetnxReply, RedisEntryId},
};
use serde::de;
use tracing::debug;

use super::{
    metrics,
    partition_key::{KvStorePartition, PartitionKey},
    serializable_query::SerializableQuery,
};
use crate::logger;

/// Drainer-entry `request_id`: log-only, empty (not threaded), kept for wire-format parity.
const REQUEST_ID: &str = "VAULT_CONSTANT_REQUEST_ID";

/// Provides access to the Redis connection pool.
pub(crate) trait RedisConnInterface {
    fn get_redis_conn(&self) -> error_stack::Result<Arc<RedisConnectionPool>, RedisError>;
}

/// Store context required by the KV wrapper.
pub(crate) trait KvStoreContext: RedisConnInterface {
    fn ttl_for_kv(&self) -> u32;
    fn drainer_stream_name(&self, shard_key: &str) -> String;
    fn drainer_num_partitions(&self) -> u8;
}

/// Reconstruct an owned `RedisError` from a `&RedisError` (not `Clone`).
fn redis_error_from_ref(err: &RedisError) -> RedisError {
    match err {
        RedisError::NotFound => RedisError::NotFound,
        RedisError::SetNxFailed => RedisError::SetNxFailed,
        RedisError::SetAddMembersFailed => RedisError::SetAddMembersFailed,
        RedisError::InvalidConfiguration(_)
        | RedisError::SetFailed
        | RedisError::SetExFailed
        | RedisError::SetExpiryFailed
        | RedisError::GetFailed
        | RedisError::DeleteFailed
        | RedisError::StreamAppendFailed
        | RedisError::StreamReadFailed
        | RedisError::GetLengthFailed
        | RedisError::StreamDeleteFailed
        | RedisError::StreamTrimFailed
        | RedisError::StreamAcknowledgeFailed
        | RedisError::StreamEmptyOrNotAvailable
        | RedisError::ConsumerGroupCreateFailed
        | RedisError::ConsumerGroupDestroyFailed
        | RedisError::ConsumerGroupRemoveConsumerFailed
        | RedisError::ConsumerGroupSetIdFailed
        | RedisError::ConsumerGroupClaimFailed
        | RedisError::JsonSerializationFailed
        | RedisError::JsonDeserializationFailed
        | RedisError::SetHashFailed
        | RedisError::SetHashFieldFailed
        | RedisError::DeleteHashFieldFailed
        | RedisError::GetHashFieldFailed
        | RedisError::InvalidRedisEntryId
        | RedisError::RedisConnectionError
        | RedisError::SubscribeError
        | RedisError::PublishError
        | RedisError::OnMessageError
        | RedisError::UnknownResult
        | RedisError::AppendElementsToListFailed
        | RedisError::GetListElementsFailed
        | RedisError::GetListLengthFailed
        | RedisError::PopListElementsFailed
        | RedisError::IncrementHashFieldFailed
        | RedisError::ScriptExecutionFailed => RedisError::UnknownResult,
    }
}

/// Bridges `error_stack` 0.4 `Report<RedisError>` → 0.5.
trait BridgeRedis<T> {
    fn bridge(self) -> error_stack::Result<T, RedisError>;
}

impl<T> BridgeRedis<T> for Result<T, error_stack_04::Report<RedisError>> {
    fn bridge(self) -> error_stack::Result<T, RedisError> {
        self.map_err(|e| {
            error_stack::Report::new(redis_error_from_ref(e.current_context()))
                .attach_printable(format!("{e:?}"))
        })
    }
}

/// Operation to perform on Redis.
pub(crate) enum KvOperation<'a, S: serde::Serialize + Debug> {
    HSetNx(&'a str, &'a S, SerializableQuery),
    HGet(&'a str),
    HDel(&'a str, SerializableQuery),
}

/// The result of a KV operation.
#[derive(Debug)]
pub(crate) enum KvResult<T: de::DeserializeOwned> {
    HGet(T),
    HSetNx(HsetnxReply),
    HDel(usize),
}

impl<T: de::DeserializeOwned> KvResult<T> {
    pub(crate) fn try_into_hsetnx(self) -> Result<HsetnxReply, RedisError> {
        match self {
            Self::HSetNx(v) => Ok(v),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub(crate) fn try_into_hdel(self) -> Result<usize, RedisError> {
        match self {
            Self::HDel(v) => Ok(v),
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
            Self::HSetNx(_, _, _) => f.write_str("HSetNx"),
            Self::HGet(_) => f.write_str("HGet"),
            Self::HDel(_, _) => f.write_str("HDel"),
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

    let key = partition_key.to_string();

    let type_name = std::any::type_name::<T>();
    let operation = op.to_string();

    let ttl = store.ttl_for_kv();

    let result = async {
        match op {
            KvOperation::HSetNx(field, value, query) => {
                let result = redis_conn
                    .serialize_and_set_hash_field_if_not_exist(&key.into(), field, value, Some(ttl))
                    .await
                    .bridge()?;

                if matches!(result, HsetnxReply::KeySet) {
                    // On drainer-push failure the Redis key remains (TTL-bounded) with no
                    // drainer entry — accepted per eventual-consistency model; alert on
                    // KV_FAILED_TO_PUSH_TO_DRAINER.
                    push_to_drainer_stream::<S>(store, query, partition_key).await?;
                    Ok(KvResult::HSetNx(result))
                } else {
                    Ok(KvResult::HSetNx(HsetnxReply::KeyNotSet))
                }
            }

            KvOperation::HGet(field) => {
                let result = redis_conn
                    .get_hash_field_and_deserialize(&key.into(), field, type_name)
                    .await
                    .bridge()?;
                Ok(KvResult::HGet(result))
            }

            KvOperation::HDel(field, query) => {
                let result = redis_conn
                    .delete_hash_fields(&key.into(), field)
                    .await
                    .bridge()?;

                push_to_drainer_stream::<S>(store, query, partition_key).await?;
                Ok(KvResult::HDel(result))
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
    let global_id = partition_key.to_string();

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
                .to_field_value_pairs(REQUEST_ID, global_id)
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
