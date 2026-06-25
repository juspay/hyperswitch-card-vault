use tracing::info;

use crate::storage::scheme::StorageScheme;

use super::partition_key::PartitionKey;
use super::wrapper::KvStoreContext;

/// Per-table KV settings resolved at runtime from the `locker.enable_kv`
/// runtime-config key (see [`crate::storage::KvRuntimeConfig`]).
///
/// Absent/unreachable/disabled config ⇒ `PostgresOnly` (see
/// [`TableKvSettings::default`]).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize,
)]
pub(crate) struct TableKvSettings {
    #[serde(default)]
    pub storage_scheme: StorageScheme,
    #[serde(default)]
    pub soft_kill: bool,
}

/// An enum to represent what operation is being performed, used by
/// [`decide_storage_scheme`] to decide the storage scheme (especially under
/// soft-kill).
#[derive(Debug, Clone)]
pub(crate) enum Op<'a> {
    Insert,
    Find,
    /// Update operation.  Soft-kill routing is resolved in
    /// [`decide_storage_scheme`].
    Update(PartitionKey<'a>, Option<StorageScheme>),
}

impl std::fmt::Display for Op<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Insert => f.write_str("insert"),
            Self::Find => f.write_str("find"),
            Self::Update(p_key, updated_by) => {
                f.write_str(&format!("update_{p_key}_updated_by_{updated_by:?}"))
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
/// - `Insert` → `PostgresOnly` (new records never enter KV)
/// - `Find` → `RedisKv` (reads still check KV — data may still live there)
/// - `Update` with `updated_by = Some(PostgresOnly)` → `PostgresOnly`
///   (record is already PG-only — no probe needed)
/// - `Update` with `updated_by = None` → `PostgresOnly`
///   (no scheme recorded — no probe needed)
/// - `Update` with `updated_by = Some(RedisKv)` → probe Redis with `HGet`;
///   if the key is still present → `RedisKv` (and bump
///   `KV_SOFT_KILL_ACTIVE_UPDATE`); if missing/err → `PostgresOnly`
///
/// Vendored from `storage_impl/src/redis/kv_store.rs::decide_storage_scheme`.
pub(crate) async fn decide_storage_scheme<S>(
    store: &impl KvStoreContext,
    settings: TableKvSettings,
    operation: Op<'_>,
) -> StorageScheme
where
    S: serde::Serialize
        + serde::de::DeserializeOwned
        + std::fmt::Debug
        + super::partition_key::KvStorePartition
        + super::constraints::UniqueConstraints
        + Sync,
{
    if !settings.soft_kill {
        return settings.storage_scheme;
    }

    let updated_scheme = match operation {
        Op::Insert => StorageScheme::PostgresOnly,
        Op::Find => StorageScheme::RedisKv,
        Op::Update(_, Some(StorageScheme::PostgresOnly)) => StorageScheme::PostgresOnly,
        Op::Update(_, None) => StorageScheme::PostgresOnly,
        Op::Update(ref partition_key, Some(StorageScheme::RedisKv)) => {
            use super::wrapper::{KvOperation, kv_wrapper};
            use super::partition_key::hash_field_key;

            let probe_field = hash_field_key(partition_key);

            let result = kv_wrapper::<S, S>(
                store,
                KvOperation::HGet(&probe_field),
                partition_key.clone(),
            )
            .await;

            match result {
                Ok(_) => {
                    super::metrics::KV_SOFT_KILL_ACTIVE_UPDATE.add(
                        1,
                        crate::metric_attributes!(("operation", "update")),
                    );
                    StorageScheme::RedisKv
                }
                Err(_) => StorageScheme::PostgresOnly,
            }
        }
    };

    info!(
        soft_kill_mode = "decide_storage_scheme",
        decided_scheme = %updated_scheme,
        configured_scheme = %settings.storage_scheme,
        operation = %operation,
    );
    updated_scheme
}
