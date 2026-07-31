use std::{fmt::Debug, future::Future, sync::Arc};

use error_stack::ResultExt;
use fred::interfaces::{HashesInterface, KeysInterface, TransactionInterface};
use hyperswitch_redis_interface::{RedisConnectionPool, errors::RedisError, types::RedisEntryId};
use serde::de;

use super::{
    partition_key::{KvStorePartition, PartitionKey},
    serializable_query::SerializableQuery,
};
use crate::{config::KvConfig, logger, observability::metrics, storage::redis as redis_store};

/// Drainer-entry `request_id`: log-only, empty (not threaded), kept for wire-format parity.
const REQUEST_ID: &str = "VAULT_CONSTANT_REQUEST_ID";
const KV_TRANSACTION_MAX_RETRIES: usize = 3;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum KvStoredValue<T> {
    Tombstone(KvTombstone),
    Value(T),
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct KvTombstone {
    #[serde(rename = "__hyperswitch_card_vault_kv_tombstone")]
    marker: KvTombstoneMarker,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
enum KvTombstoneMarker {
    #[serde(rename = "v1")]
    V1,
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

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum KvInsertResult {
    Inserted,
    AlreadyExists,
}

pub(crate) enum KvFindResult<V> {
    Absent,
    Deleted,
    Present(V),
}

#[derive(Clone, Copy, strum::Display)]
#[strum(serialize_all = "snake_case")]
enum KvOperationKind {
    Insert,
    Find,
    Update,
    Delete,
}

pub(crate) trait KvBehaviour {
    type Error: error_stack::Context;
    async fn insert<V>(
        &self,
        partition_key: PartitionKey<'_>,
        value: &V,
        query: SerializableQuery,
    ) -> error_stack::Result<KvInsertResult, Self::Error>
    where
        V: serde::Serialize + Debug + KvStorePartition + Sync;

    async fn find<V>(
        &self,
        partition_key: PartitionKey<'_>,
    ) -> error_stack::Result<KvFindResult<V>, Self::Error>
    where
        V: de::DeserializeOwned;

    async fn update<V>(
        &self,
        partition_key: PartitionKey<'_>,
        value: &V,
        query: SerializableQuery,
    ) -> error_stack::Result<(), Self::Error>
    where
        V: serde::Serialize + Debug + KvStorePartition + Sync;

    async fn delete<V>(
        &self,
        partition_key: PartitionKey<'_>,
        query: SerializableQuery,
    ) -> error_stack::Result<usize, Self::Error>
    where
        V: KvStorePartition;
}

#[derive(Clone)]
pub(crate) enum KvBackend {
    Redis(RedisBackend),
}

impl KvBackend {
    pub(crate) fn redis(redis: redis_store::TenantAwareRedisStore, config: KvConfig) -> Self {
        Self::Redis(RedisBackend::new(redis, config))
    }
}

#[derive(Clone)]
pub(crate) struct RedisBackend {
    redis: redis_store::TenantAwareRedisStore,
    config: KvConfig,
}

impl RedisBackend {
    const TOMBSTONE_VALUE: KvTombstone = KvTombstone {
        marker: KvTombstoneMarker::V1,
    };

    fn new(redis: redis_store::TenantAwareRedisStore, config: KvConfig) -> Self {
        Self { redis, config }
    }

    fn get_redis_conn(&self) -> Arc<RedisConnectionPool> {
        self.redis.get_redis_conn()
    }

    async fn insert_if_absent_or_tombstone(
        &self,
        key: &str,
        resource: &str,
        serialized: String,
    ) -> error_stack::Result<KvInsertResult, RedisError> {
        let redis_conn = self.get_redis_conn();
        let ttl = self.config.ttl_for_kv;
        let redis_key = redis_conn.add_prefix(key);

        for attempt in 1..=KV_TRANSACTION_MAX_RETRIES {
            let client = redis_conn.pool.next();

            logger::debug!(
                kv_operation = "insert_if_absent_or_tombstone",
                redis_key = %redis_key,
                attempt,
                max_retries = KV_TRANSACTION_MAX_RETRIES,
                "Starting Redis conditional insert attempt"
            );

            // The conditional insert needs WATCH before the pre-read and MULTI on the same
            // client. `RedisConnectionPool::get_transaction()` chooses a transaction client
            // directly, but does not expose a watched pre-read on that same client.
            client
                .watch(vec![redis_key.clone()])
                .await
                .change_context(RedisError::SetHashFieldFailed)?;

            let current = client
                .hget::<Option<Vec<u8>>, _, _>(redis_key.clone(), key.to_string())
                .await
                .change_context(RedisError::GetHashFieldFailed)?;

            match current.as_deref() {
                Some(value) if Self::is_tombstone(value) => {
                    logger::debug!(
                        kv_operation = "insert_if_absent_or_tombstone",
                        redis_key = %redis_key,
                        attempt,
                        "Redis conditional insert found tombstone"
                    );
                }
                Some(_) => {
                    client
                        .unwatch()
                        .await
                        .change_context(RedisError::SetHashFieldFailed)?;
                    record_kv_insert_result(resource, "already_exists");
                    logger::debug!(
                        kv_operation = "insert_if_absent_or_tombstone",
                        redis_key = %redis_key,
                        attempt,
                        "Redis conditional insert skipped because key already exists"
                    );
                    return Ok(KvInsertResult::AlreadyExists);
                }
                None => {
                    logger::debug!(
                        kv_operation = "insert_if_absent_or_tombstone",
                        redis_key = %redis_key,
                        attempt,
                        "Redis conditional insert found no existing value"
                    );
                }
            }

            let transaction = client.multi();
            transaction
                .hset::<(), _, _>(
                    redis_key.clone(),
                    vec![(key.to_string(), serialized.clone())],
                )
                .await
                .change_context(RedisError::SetHashFieldFailed)?;
            transaction
                .expire::<(), _>(&redis_key, i64::from(ttl))
                .await
                .change_context(RedisError::SetExpiryFailed)?;

            let txn_result = transaction
                .exec::<Option<(i32, i32)>>(true)
                .await
                .change_context(RedisError::SetHashFieldFailed)?;

            if matches!(txn_result, Some((_, _))) {
                record_kv_insert_result(resource, "inserted");
                logger::debug!(
                    kv_operation = "insert_if_absent_or_tombstone",
                    redis_key = %redis_key,
                    attempt,
                    "Redis conditional insert succeeded"
                );
                return Ok(KvInsertResult::Inserted);
            }

            if attempt < KV_TRANSACTION_MAX_RETRIES {
                metrics::KV_TRANSACTION_RETRY_COUNT.add(
                    1,
                    metrics_utils::metric_attributes!(
                        ("operation", "insert_if_absent_or_tombstone"),
                        ("resource", resource.to_owned()),
                        ("reason", "transaction_conflict"),
                    ),
                );
                logger::warn!(
                    kv_operation = "insert_if_absent_or_tombstone",
                    redis_key = %redis_key,
                    retry_attempt = attempt + 1,
                    max_retries = KV_TRANSACTION_MAX_RETRIES,
                    "Retrying Redis conditional insert after transaction conflict"
                );
            }
        }

        logger::error!(
            kv_operation = "insert_if_absent_or_tombstone",
            redis_key = %redis_key,
            max_retries = KV_TRANSACTION_MAX_RETRIES,
            "Redis conditional insert failed after transaction conflicts"
        );
        record_kv_insert_result(resource, "retry_exhausted");
        Err(RedisError::SetHashFieldFailed.into())
    }

    fn is_tombstone(value: &[u8]) -> bool {
        matches!(
            serde_json::from_slice::<KvStoredValue<de::IgnoredAny>>(value),
            Ok(KvStoredValue::Tombstone(tombstone)) if tombstone == Self::TOMBSTONE_VALUE
        )
    }
}

fn record_kv_insert_result(resource: &str, result: &'static str) {
    metrics::KV_INSERT_RESULT_COUNT.add(
        1,
        metrics_utils::metric_attributes!(
            ("operation", "insert_if_absent_or_tombstone"),
            ("resource", resource.to_owned()),
            ("result", result),
        ),
    );
}

impl KvBehaviour for KvBackend {
    type Error = RedisError;

    async fn insert<V>(
        &self,
        partition_key: PartitionKey<'_>,
        value: &V,
        query: SerializableQuery,
    ) -> error_stack::Result<KvInsertResult, Self::Error>
    where
        V: serde::Serialize + Debug + KvStorePartition + Sync,
    {
        match self {
            Self::Redis(redis) => redis.insert(partition_key, value, query).await,
        }
    }

    async fn find<V>(
        &self,
        partition_key: PartitionKey<'_>,
    ) -> error_stack::Result<KvFindResult<V>, Self::Error>
    where
        V: de::DeserializeOwned,
    {
        match self {
            Self::Redis(redis) => redis.find(partition_key).await,
        }
    }

    async fn update<V>(
        &self,
        partition_key: PartitionKey<'_>,
        value: &V,
        query: SerializableQuery,
    ) -> error_stack::Result<(), Self::Error>
    where
        V: serde::Serialize + Debug + KvStorePartition + Sync,
    {
        match self {
            Self::Redis(redis) => redis.update(partition_key, value, query).await,
        }
    }

    async fn delete<V>(
        &self,
        partition_key: PartitionKey<'_>,
        query: SerializableQuery,
    ) -> error_stack::Result<usize, Self::Error>
    where
        V: KvStorePartition,
    {
        match self {
            Self::Redis(redis) => redis.delete::<V>(partition_key, query).await,
        }
    }
}

impl KvBehaviour for RedisBackend {
    type Error = RedisError;
    async fn insert<V>(
        &self,
        partition_key: PartitionKey<'_>,
        value: &V,
        query: SerializableQuery,
    ) -> error_stack::Result<KvInsertResult, Self::Error>
    where
        V: serde::Serialize + Debug + KvStorePartition + Sync,
    {
        with_kv_metrics(KvOperationKind::Insert, async move {
            let key = partition_key.to_string();
            let resource = query.entity_type();
            let serialized = serde_json::to_string(&KvStoredValue::Value(value))
                .change_context(RedisError::JsonSerializationFailed)?;

            let result = self
                .insert_if_absent_or_tombstone(&key, &resource, serialized)
                .await?;

            match result {
                KvInsertResult::Inserted => {
                    // On drainer-push failure the Redis key remains (TTL-bounded) with no
                    // drainer entry — accepted per eventual-consistency model; alert on
                    // KV_FAILED_TO_PUSH_TO_DRAINER.
                    push_to_drainer_stream::<V>(self, query, partition_key).await?;
                    Ok(KvInsertResult::Inserted)
                }
                KvInsertResult::AlreadyExists => Ok(KvInsertResult::AlreadyExists),
            }
        })
        .await
    }

    async fn find<V>(
        &self,
        partition_key: PartitionKey<'_>,
    ) -> error_stack::Result<KvFindResult<V>, Self::Error>
    where
        V: de::DeserializeOwned,
    {
        with_kv_metrics(KvOperationKind::Find, async move {
            let redis_conn = self.get_redis_conn();
            let key = partition_key.to_string();
            let redis_key = key.clone().into();

            let stored_value = redis_conn
                .get_hash_field_and_deserialize::<Option<KvStoredValue<V>>>(
                    &redis_key,
                    &key,
                    std::any::type_name::<KvStoredValue<V>>(),
                )
                .await
                .bridge();

            match stored_value {
                Ok(Some(KvStoredValue::Tombstone(_))) => Ok(KvFindResult::Deleted),
                Ok(Some(KvStoredValue::Value(value))) => Ok(KvFindResult::Present(value)),
                Ok(None) => Ok(KvFindResult::Absent),
                Err(err) if matches!(err.current_context(), RedisError::NotFound) => {
                    Ok(KvFindResult::Absent)
                }
                Err(err) => Err(err),
            }
        })
        .await
    }

    async fn update<V>(
        &self,
        partition_key: PartitionKey<'_>,
        value: &V,
        query: SerializableQuery,
    ) -> error_stack::Result<(), Self::Error>
    where
        V: serde::Serialize + Debug + KvStorePartition + Sync,
    {
        with_kv_metrics(KvOperationKind::Update, async move {
            let redis_conn = self.get_redis_conn();
            let key = partition_key.to_string();
            let redis_key = key.clone().into();
            let serialized = serde_json::to_string(&KvStoredValue::Value(value))
                .change_context(RedisError::JsonSerializationFailed)?;

            redis_conn
                .set_hash_fields(
                    &redis_key,
                    vec![(key.as_str(), serialized)],
                    Some(self.config.ttl_for_kv.into()),
                )
                .await
                .bridge()?;

            push_to_drainer_stream::<V>(self, query, partition_key).await?;
            Ok(())
        })
        .await
    }

    async fn delete<V>(
        &self,
        partition_key: PartitionKey<'_>,
        query: SerializableQuery,
    ) -> error_stack::Result<usize, Self::Error>
    where
        V: KvStorePartition,
    {
        with_kv_metrics(KvOperationKind::Delete, async move {
            let redis_conn = self.get_redis_conn();
            let key = partition_key.to_string();
            let redis_key = key.clone().into();
            let tombstone =
                serde_json::to_string(&KvStoredValue::<()>::Tombstone(Self::TOMBSTONE_VALUE))
                    .change_context(RedisError::JsonSerializationFailed)?;

            redis_conn
                .set_hash_fields(
                    &redis_key,
                    vec![(key.as_str(), tombstone)],
                    Some(self.config.ttl_for_kv.into()),
                )
                .await
                .bridge()?;

            push_to_drainer_stream::<V>(self, query, partition_key).await?;
            Ok(1)
        })
        .await
    }
}

async fn with_kv_metrics<T, F>(
    operation: KvOperationKind,
    future: F,
) -> error_stack::Result<T, RedisError>
where
    F: Future<Output = error_stack::Result<T, RedisError>>,
{
    let start = std::time::Instant::now();
    let operation = operation.to_string();

    future
        .await
        .inspect(|_| {
            let duration = start.elapsed();
            let attrs = metrics_utils::metric_attributes!(
                ("operation", operation.clone()),
                ("outcome", "success"),
            );
            logger::debug!(kv_operation = %operation, status = "success");
            metrics::KV_OPERATION_COUNT.add(1, attrs);
            metrics::KV_OPERATION_DURATION.record(duration.as_secs_f64(), attrs);
        })
        .inspect_err(|err: &error_stack::Report<RedisError>| {
            let outcome = match err.current_context() {
                RedisError::NotFound => {
                    logger::debug!(kv_operation = %operation, status = "not_found");
                    "not_found"
                }
                other => {
                    logger::error!(kv_operation = %operation, status = "error", error = ?other);
                    "error"
                }
            };
            let duration = start.elapsed();
            let attrs = metrics_utils::metric_attributes!(
                ("operation", operation.clone()),
                ("outcome", outcome)
            );

            metrics::KV_OPERATION_COUNT.add(1, attrs);
            metrics::KV_OPERATION_DURATION.record(duration.as_secs_f64(), attrs);
        })
}

async fn push_to_drainer_stream<R>(
    backend: &RedisBackend,
    serializable_query: SerializableQuery,
    partition_key: PartitionKey<'_>,
) -> error_stack::Result<(), RedisError>
where
    R: KvStorePartition,
{
    let global_id = partition_key.to_string();

    let shard_key = R::shard_key(partition_key, backend.config.drainer_num_partitions);
    let stream_name = backend.config.drainer_stream_name(&shard_key);

    let operation_str = serializable_query.operation().to_string();
    let entity_type_str = serializable_query.entity_type();

    let redis_conn = backend.get_redis_conn();

    let start = std::time::Instant::now();

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
            let duration = start.elapsed();
            let attrs = metrics_utils::metric_attributes!(
                ("operation", operation_str.clone()),
                ("entity_type", entity_type_str.clone()),
                ("outcome", "success"),
            );
            metrics::KV_DRAINER_PUSH_COUNT.add(1, attrs);
            metrics::KV_DRAINER_PUSH_DURATION.record(duration.as_secs_f64(), attrs);
        })
        .inspect_err(|error| {
            let duration = start.elapsed();
            let attrs = metrics_utils::metric_attributes!(
                ("operation", operation_str.clone()),
                ("entity_type", entity_type_str.clone()),
                ("outcome", "error"),
            );
            metrics::KV_DRAINER_PUSH_COUNT.add(1, attrs);
            metrics::KV_DRAINER_PUSH_DURATION.record(duration.as_secs_f64(), attrs);
            logger::error!(?error, "Failed to add entry in drainer stream");
        })
        .change_context(RedisError::StreamAppendFailed)
}
