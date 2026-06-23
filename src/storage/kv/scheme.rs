use std::fmt::Debug;

use hyperswitch_redis_interface::errors::RedisError;
use serde::de;
use tracing::info;

use super::{
    metrics,
    partition_key::PartitionKey,
    wrapper::{KvOperation, KvStoreContext, kv_wrapper},
};
use crate::storage::kv::{constraints::UniqueConstraints, partition_key::KvStorePartition};

/// Per-tenant storage scheme.
///
/// Vendored concept from `common_enums::enums::MerchantStorageScheme`.
/// When `PostgresOnly`, all reads/writes go directly to Postgres.
/// When `RedisKv`, writes go to Redis (write-through) and a drainer stream
/// replays them to Postgres asynchronously.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize, strum::Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum StorageScheme {
    #[default]
    PostgresOnly,
    RedisKv,
}

/// Identifies which KV-supported table a config entry applies to.
///
/// Used as the key in the per-tenant `kv: HashMap<KvTable, TableKvSettings>`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize, strum::Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum KvTable {
    Fingerprint,
    HashTable,
    Locker,
    Vault,
}

/// Per-table KV settings stored in the per-tenant config.
///
/// Absent table ⇒ `PostgresOnly` (see [`TableKvSettings::default`]).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize,
)]
pub struct TableKvSettings {
    #[serde(default)]
    pub storage_scheme: StorageScheme,
    #[serde(default)]
    pub soft_kill: bool,
}

/// An enum to represent what operation is being performed, used by
/// [`decide_storage_scheme`] to decide the storage scheme (especially under
/// soft-kill).
pub enum Op<'a> {
    Insert,
    Update(PartitionKey<'a>, &'a str, Option<&'a str>),
    Find,
}

impl std::fmt::Display for Op<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Insert => f.write_str("insert"),
            Self::Find => f.write_str("find"),
            Self::Update(p_key, _, updated_by) => {
                f.write_str(&format!("update_{p_key} for updated_by_{updated_by:?}"))
            }
        }
    }
}

/// Decide the effective storage scheme for an operation.
///
/// When soft-kill is **off**, the configured `storage_scheme` is returned
/// unchanged.
///
/// When soft-kill is **on** (the gradual-rollout mode):
/// - `Insert` → `PostgresOnly` (writes still go to Postgres)
/// - `Find` → `RedisKv` (reads try Redis first)
/// - `Update` with `updated_by = "postgres_only"` → `PostgresOnly`
/// - `Update` with a non-empty `updated_by` → HGet-probe Redis; if the key
///   exists use `RedisKv`, otherwise fall back to `PostgresOnly`
/// - `Update` with no `updated_by` → `PostgresOnly`
///
/// Vendored from `storage_impl/src/redis/kv_store.rs::decide_storage_scheme`.
pub async fn decide_storage_scheme<D>(
    store: &impl KvStoreContext,
    settings: TableKvSettings,
    operation: Op<'_>,
) -> StorageScheme
where
    D: de::DeserializeOwned
        + serde::Serialize
        + Debug
        + KvStorePartition
        + UniqueConstraints
        + Sync,
{
    if settings.soft_kill {
        let ops = operation.to_string();
        let updated_scheme = match operation {
            Op::Insert => StorageScheme::PostgresOnly,
            Op::Find => StorageScheme::RedisKv,
            Op::Update(_, _, Some("postgres_only")) => StorageScheme::PostgresOnly,
            Op::Update(partition_key, field, Some(_updated_by)) => {
                match Box::pin(kv_wrapper::<D, _>(
                    store,
                    KvOperation::<D>::HGet(field),
                    partition_key,
                ))
                .await
                {
                    Ok(_) => {
                        metrics::KV_SOFT_KILL_ACTIVE_UPDATE.add(1, &[]);
                        StorageScheme::RedisKv
                    }
                    Err(_) => StorageScheme::PostgresOnly,
                }
            }
            Op::Update(_, _, None) => StorageScheme::PostgresOnly,
        };

        let type_name = std::any::type_name::<D>();
        info!(
            soft_kill_mode = "decide_storage_scheme",
            decided_scheme = %updated_scheme,
            configured_scheme = %settings.storage_scheme,
            entity = %type_name,
            operation = %ops,
        );

        updated_scheme
    } else {
        settings.storage_scheme
    }
}

/// Result type for `decide_storage_scheme` that may fail during the Redis
/// probe in soft-kill mode.  In practice the Redis error is swallowed and
/// converted to `PostgresOnly`, so the function never returns `Err`.
#[allow(dead_code)]
type DecideResult = Result<StorageScheme, RedisError>;
