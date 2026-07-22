use std::{fmt::Debug, future::Future, sync::Arc};

use error_stack::ResultExt;
use fred::{
    interfaces::{HashesInterface, KeysInterface, TransactionInterface},
    types::RedisValue as FredRedisValue,
};
use hyperswitch_redis_interface::{RedisConnectionPool, errors::RedisError, types::RedisEntryId};
use serde::de;

use super::{
    partition_key::{KvStorePartition, PartitionKey},
    serializable_query::SerializableQuery,
};
use crate::{logger, observability::metrics};

/// Drainer-entry `request_id`: log-only, empty (not threaded), kept for wire-format parity.
const REQUEST_ID: &str = "VAULT_CONSTANT_REQUEST_ID";
const KV_TRANSACTION_MAX_RETRIES: usize = 3;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
enum KvStoredValue<T> {
    Tombstone(KvTombstone),
    Value(T),
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct KvTombstone {
    #[serde(rename = "__hyperswitch_card_vault_kv_tombstone")]
    marker: KvTombstoneMarker,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
enum KvTombstoneMarker {
    #[serde(rename = "v1")]
    V1,
}

impl KvTombstone {
    fn new() -> Self {
        Self {
            marker: KvTombstoneMarker::V1,
        }
    }
}

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

#[derive(Clone, Copy)]
enum KvOperationKind {
    Insert,
    Find,
    Update,
    Delete,
}

impl std::fmt::Display for KvOperationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Insert => f.write_str("insert"),
            Self::Find => f.write_str("find"),
            Self::Update => f.write_str("update"),
            Self::Delete => f.write_str("delete"),
        }
    }
}

pub(crate) trait KvBehaviour {
    type Error: error_stack::Context;

    fn not_found_error() -> Self::Error;

    async fn insert<V>(
        &self,
        partition_key: PartitionKey<'_>,
        value: &V,
        query: SerializableQuery,
    ) -> error_stack::Result<KvInsertResult, Self::Error>
    where
        V: serde::Serialize + Debug + KvStorePartition + Sync;

    async fn find<V>(&self, partition_key: PartitionKey<'_>) -> error_stack::Result<V, Self::Error>
    where
        V: de::DeserializeOwned,
    {
        match self.find_with_status(partition_key).await? {
            KvFindResult::Present(value) => Ok(value),
            KvFindResult::Deleted | KvFindResult::Absent => {
                Err(error_stack::Report::new(Self::not_found_error()))
            }
        }
    }

    async fn find_with_status<V>(
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

pub(crate) struct RedisBackend<'a, C>
where
    C: KvStoreContext,
{
    store: &'a C,
}

impl<'a, C> RedisBackend<'a, C>
where
    C: KvStoreContext,
{
    pub(crate) fn new(store: &'a C) -> Self {
        Self { store }
    }
}

impl<C> KvBehaviour for RedisBackend<'_, C>
where
    C: KvStoreContext + Sync,
{
    type Error = RedisError;

    fn not_found_error() -> Self::Error {
        RedisError::NotFound
    }

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
            let redis_conn = self.store.get_redis_conn()?;
            let key = partition_key.to_string();
            let serialized =
                serde_json::to_string(value).change_context(RedisError::JsonSerializationFailed)?;

            let result = insert_if_absent_or_tombstone(
                redis_conn.as_ref(),
                &key,
                serialized,
                self.store.ttl_for_kv(),
            )
            .await?;

            match result {
                KvInsertResult::Inserted => {
                    // On drainer-push failure the Redis key remains (TTL-bounded) with no
                    // drainer entry — accepted per eventual-consistency model; alert on
                    // KV_FAILED_TO_PUSH_TO_DRAINER.
                    push_to_drainer_stream::<V>(self.store, query, partition_key).await?;
                    Ok(KvInsertResult::Inserted)
                }
                KvInsertResult::AlreadyExists => Ok(KvInsertResult::AlreadyExists),
            }
        })
        .await
    }

    async fn find_with_status<V>(
        &self,
        partition_key: PartitionKey<'_>,
    ) -> error_stack::Result<KvFindResult<V>, Self::Error>
    where
        V: de::DeserializeOwned,
    {
        with_kv_metrics(KvOperationKind::Find, async move {
            let redis_conn = self.store.get_redis_conn()?;
            let key = partition_key.to_string();
            let redis_key = key.clone().into();

            let stored_value = redis_conn
                .get_hash_field_and_deserialize::<Option<KvStoredValue<V>>>(
                    &redis_key,
                    &key,
                    std::any::type_name::<KvStoredValue<V>>(),
                )
                .await
                .bridge()?;

            match stored_value {
                Some(KvStoredValue::Tombstone(_)) => Ok(KvFindResult::Deleted),
                Some(KvStoredValue::Value(value)) => Ok(KvFindResult::Present(value)),
                None => Ok(KvFindResult::Absent),
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
            let redis_conn = self.store.get_redis_conn()?;
            let key = partition_key.to_string();
            let redis_key = key.clone().into();
            let serialized =
                serde_json::to_string(value).change_context(RedisError::JsonSerializationFailed)?;

            redis_conn
                .set_hash_fields(
                    &redis_key,
                    vec![(key.as_str(), serialized)],
                    Some(self.store.ttl_for_kv().into()),
                )
                .await
                .bridge()?;

            push_to_drainer_stream::<V>(self.store, query, partition_key).await?;
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
            let redis_conn = self.store.get_redis_conn()?;
            let key = partition_key.to_string();
            let redis_key = key.clone().into();
            let tombstone = serialized_tombstone()?;

            redis_conn
                .set_hash_fields(
                    &redis_key,
                    vec![(key.as_str(), tombstone)],
                    Some(self.store.ttl_for_kv().into()),
                )
                .await
                .bridge()?;

            push_to_drainer_stream::<V>(self.store, query, partition_key).await?;
            Ok(1)
        })
        .await
    }
}

async fn insert_if_absent_or_tombstone(
    redis_conn: &RedisConnectionPool,
    key: &str,
    serialized: String,
    ttl: u32,
) -> error_stack::Result<KvInsertResult, RedisError> {
    for _ in 0..KV_TRANSACTION_MAX_RETRIES {
        let redis_key = redis_conn.add_prefix(key);
        let client = redis_conn.pool.next();

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

        if current.as_deref().is_some_and(|value| !is_tombstone(value)) {
            client
                .unwatch()
                .await
                .change_context(RedisError::SetHashFieldFailed)?;
            return Ok(KvInsertResult::AlreadyExists);
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
            .expire::<(), _>(redis_key, i64::from(ttl))
            .await
            .change_context(RedisError::SetExpiryFailed)?;

        if transaction_committed(
            transaction
                .exec::<FredRedisValue>(true)
                .await
                .change_context(RedisError::SetHashFieldFailed)?,
        ) {
            return Ok(KvInsertResult::Inserted);
        }
    }

    Err(RedisError::SetHashFieldFailed.into())
}

fn transaction_committed(result: FredRedisValue) -> bool {
    !matches!(result, FredRedisValue::Null)
}

fn serialized_tombstone() -> error_stack::Result<String, RedisError> {
    serde_json::to_string(&KvStoredValue::<serde_json::Value>::Tombstone(
        KvTombstone::new(),
    ))
    .change_context(RedisError::JsonSerializationFailed)
}

fn is_tombstone(value: &[u8]) -> bool {
    matches!(
        serde_json::from_slice::<KvStoredValue<de::IgnoredAny>>(value),
        Ok(KvStoredValue::Tombstone(_))
    )
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
            let attrs = crate::metric_attributes!(
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
            let attrs =
                crate::metric_attributes!(("operation", operation.clone()), ("outcome", outcome));

            metrics::KV_OPERATION_COUNT.add(1, attrs);
            metrics::KV_OPERATION_DURATION.record(duration.as_secs_f64(), attrs);
        })
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
            let attrs = crate::metric_attributes!(
                ("operation", operation_str.clone()),
                ("entity_type", entity_type_str.clone()),
                ("outcome", "success"),
            );
            metrics::KV_DRAINER_PUSH_COUNT.add(1, attrs);
            metrics::KV_DRAINER_PUSH_DURATION.record(duration.as_secs_f64(), attrs);
        })
        .inspect_err(|error| {
            let duration = start.elapsed();
            let attrs = crate::metric_attributes!(
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
