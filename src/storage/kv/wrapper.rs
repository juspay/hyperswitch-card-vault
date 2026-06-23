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
pub trait RedisConnInterface {
    fn get_redis_conn(
        &self,
    ) -> error_stack::Result<Arc<RedisConnectionPool>, RedisError>;
}

/// Everything the KV wrapper needs from the store beyond a Redis connection.
///
/// In hyperswitch this is `KVRouterStore<T>`; here we use a trait so that
/// [`crate::storage::Storage`] can supply the values it holds directly.
pub trait KvStoreContext: RedisConnInterface {
    fn ttl_for_kv(&self) -> u32;
    fn drainer_stream_name(&self, shard_key: &str) -> String;
    fn drainer_num_partitions(&self) -> u8;
    fn request_id(&self) -> &str;
}

// ─── error_stack 0.4 ↔ 0.5 bridge ──────────────────────────────────────────

/// Reconstruct an owned [`RedisError`] from a `&RedisError`.
///
/// `RedisError` does not implement `Clone`, so we compare against the known
/// unit variants using `PartialEq` (which it does derive) and fall back to
/// [`RedisError::UnknownResult`] for parameterised variants.
fn redis_error_from_ref(err: &RedisError) -> RedisError {
    if err == &RedisError::NotFound {
        RedisError::NotFound
    } else if err == &RedisError::SetNxFailed {
        RedisError::SetNxFailed
    } else if err == &RedisError::SetAddMembersFailed {
        RedisError::SetAddMembersFailed
    } else if err == &RedisError::SetHashFailed {
        RedisError::SetHashFailed
    } else if err == &RedisError::SetHashFieldFailed {
        RedisError::SetHashFieldFailed
    } else if err == &RedisError::GetHashFieldFailed {
        RedisError::GetHashFieldFailed
    } else if err == &RedisError::StreamAppendFailed {
        RedisError::StreamAppendFailed
    } else if err == &RedisError::JsonSerializationFailed {
        RedisError::JsonSerializationFailed
    } else if err == &RedisError::JsonDeserializationFailed {
        RedisError::JsonDeserializationFailed
    } else {
        RedisError::UnknownResult
    }
}

/// Extension trait that bridges `error_stack` 0.4 `Report<RedisError>` (used by
/// `redis_interface`) into the `error_stack` 0.5 `Report<RedisError>` used by
/// card-vault.
pub trait BridgeRedis<T> {
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
}// ─── KvOperation / KvResult ─────────────────────────────────────────────────

/// An enum to represent what operation to do on Redis.
pub enum KvOperation<'a, S: serde::Serialize + Debug> {
    Hset((&'a str, String), SerializableQuery),
    SetNx(&'a S, SerializableQuery),
    HSetNx(&'a str, &'a S, SerializableQuery),
    HGet(&'a str),
    Get,
    Scan(&'a str),
}

/// The result of a KV operation.
#[derive(Debug)]
pub enum KvResult<T: de::DeserializeOwned> {
    HGet(T),
    Get(T),
    Hset(()),
    SetNx(SetnxReply),
    HSetNx(HsetnxReply),
    Scan(Vec<T>),
}

impl<T: de::DeserializeOwned> KvResult<T> {
    pub fn try_into_hget(self) -> Result<T, RedisError> {
        match self {
            Self::HGet(v) => Ok(v),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub fn try_into_hset(self) -> Result<(), RedisError> {
        match self {
            Self::Hset(()) => Ok(()),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub fn try_into_hsetnx(self) -> Result<HsetnxReply, RedisError> {
        match self {
            Self::HSetNx(v) => Ok(v),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub fn try_into_setnx(self) -> Result<SetnxReply, RedisError> {
        match self {
            Self::SetNx(v) => Ok(v),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub fn try_into_scan(self) -> Result<Vec<T>, RedisError> {
        match self {
            Self::Scan(v) => Ok(v),
            _ => Err(RedisError::UnknownResult),
        }
    }

    pub fn try_into_get(self) -> Result<T, RedisError> {
        match self {
            Self::Get(v) => Ok(v),
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
            Self::Hset(_, _) => f.write_str("Hset"),
            Self::SetNx(_, _) => f.write_str("Setnx"),
            Self::HSetNx(_, _, _) => f.write_str("HSetNx"),
            Self::HGet(_) => f.write_str("Hget"),
            Self::Get => f.write_str("Get"),
            Self::Scan(_) => f.write_str("Scan"),
        }
    }
}

// ─── kv_wrapper ─────────────────────────────────────────────────────────────

pub async fn kv_wrapper<'a, T, S>(
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
            KvOperation::Hset(value, query) => {
                debug!(kv_operation = %operation, ?value);

                redis_conn
                    .set_hash_fields(&key.into(), vec![value], Some(i64::from(ttl)))
                    .await
                    .bridge()?;

                push_to_drainer_stream::<S>(store, query, partition_key).await?;

                Ok(KvResult::Hset(()))
            }

            KvOperation::HGet(field) => {
                let result = redis_conn
                    .get_hash_field_and_deserialize(&key.into(), field, type_name)
                    .await
                    .bridge()?;
                Ok(KvResult::HGet(result))
            }

            KvOperation::Scan(pattern) => {
                let result: Vec<T> = redis_conn
                    .hscan_and_deserialize(&key.into(), pattern, None)
                    .await
                    .bridge()
                    .and_then(|result| {
                        if result.is_empty() {
                            Err(error_stack::Report::new(RedisError::NotFound))
                        } else {
                            Ok(result)
                        }
                    })?;
                Ok(KvResult::Scan(result))
            }

            KvOperation::HSetNx(field, value, query) => {
                debug!(kv_operation = %operation, ?value);

                value.check_for_constraints(&redis_conn).await.bridge()?;

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

            KvOperation::SetNx(value, query) => {
                debug!(kv_operation = %operation, ?value);

                let result = redis_conn
                    .serialize_and_set_key_if_not_exist(&key.into(), value, Some(i64::from(ttl)))
                    .await
                    .bridge()?;

                value.check_for_constraints(&redis_conn).await.bridge()?;

                if matches!(result, SetnxReply::KeySet) {
                    push_to_drainer_stream::<S>(store, query, partition_key).await?;
                    Ok(KvResult::SetNx(result))
                } else {
                    Err(error_stack::Report::new(RedisError::SetNxFailed))
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

    let attributes: Vec<(&str, String)> = vec![("operation", operation.clone())];
    let attr_refs: Vec<(&str, &str)> =
        attributes.iter().map(|(k, v)| (*k, v.as_str())).collect();
    result
        .await
        .inspect(|_| {
            debug!(kv_operation = %operation, status = "success");
            metrics::KV_OPERATION_SUCCESSFUL.add(1, &attr_refs);
        })
        .inspect_err(|err| {
            logger::error!(kv_operation = %operation, status = "error", error = ?err);
            metrics::KV_OPERATION_FAILED.add(1, &attr_refs);
        })
}

// ─── push_to_drainer_stream ─────────────────────────────────────────────────

pub async fn push_to_drainer_stream<R>(
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

    let metric_attributes: Vec<(&str, String)> = vec![
        ("operation", serializable_query.operation().to_string()),
        ("entity_type", serializable_query.entity_type()),
    ];
    let metric_refs: Vec<(&str, &str)> = metric_attributes
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

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
        .map(|_| metrics::KV_PUSHED_TO_DRAINER.add(1, &metric_refs))
        .inspect_err(|error| {
            metrics::KV_FAILED_TO_PUSH_TO_DRAINER.add(1, &metric_refs);
            logger::error!(?error, "Failed to add entry in drainer stream");
        })
        .change_context(RedisError::StreamAppendFailed)
}
