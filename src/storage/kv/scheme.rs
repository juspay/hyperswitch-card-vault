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
    /// Update operation.  Under soft-kill, the `updated_by` field determines
    /// the behaviour:
    /// - `Some(PostgresOnly)` → `PostgresOnly` (no probe — record is PG-only)
    /// - `None` → `PostgresOnly` (no probe — no scheme recorded)
    /// - `Some(RedisKv)` → probe Redis with `HGet`; if the key is still
    ///   present → `RedisKv` (metric bumped); if missing/err → `PostgresOnly`
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
        // Arm 1 — record last lived in Postgres: straight to Postgres, no probe.
        Op::Update(_, Some(StorageScheme::PostgresOnly)) => StorageScheme::PostgresOnly,
        // Arm 3 — no scheme recorded: straight to Postgres, no probe.
        Op::Update(_, None) => StorageScheme::PostgresOnly,
        // Arm 2 — record claims RedisKv: actively probe Redis to confirm.
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

/// A dummy store for testing `decide_storage_scheme` without a real Redis.
#[cfg(test)]
struct DummyKv;

#[cfg(test)]
impl super::wrapper::RedisConnInterface for DummyKv {
    fn get_redis_conn(
        &self,
    ) -> error_stack::Result<
        std::sync::Arc<hyperswitch_redis_interface::RedisConnectionPool>,
        hyperswitch_redis_interface::errors::RedisError,
    > {
        Err(error_stack::Report::new(
            hyperswitch_redis_interface::errors::RedisError::RedisConnectionError,
        ))
    }
}

#[cfg(test)]
impl super::wrapper::KvStoreContext for DummyKv {
    fn ttl_for_kv(&self) -> u32 {
        900
    }
    fn drainer_stream_name(&self, _shard_key: &str) -> String {
        "{shard_0}_DRAINER_STREAM".to_string()
    }
    fn drainer_num_partitions(&self) -> u8 {
        16
    }
    fn request_id(&self) -> &str {
        ""
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::storage::types::FingerprintTableNew;

    #[test]
    fn no_soft_kill_returns_configured_scheme() {
        let redis_kv = TableKvSettings {
            storage_scheme: StorageScheme::RedisKv,
            soft_kill: false,
        };
        assert_eq!(
            futures::executor::block_on(decide_storage_scheme::<FingerprintTableNew>(
                &DummyKv,
                redis_kv,
                Op::Insert,
            )),
            StorageScheme::RedisKv
        );

        let pg_only = TableKvSettings {
            storage_scheme: StorageScheme::PostgresOnly,
            soft_kill: false,
        };
        assert_eq!(
            futures::executor::block_on(decide_storage_scheme::<FingerprintTableNew>(
                &DummyKv,
                pg_only,
                Op::Find,
            )),
            StorageScheme::PostgresOnly
        );
    }

    #[tokio::test]
    async fn soft_kill_routes_insert_to_postgres_and_find_to_redis() {
        let soft_kill = TableKvSettings {
            storage_scheme: StorageScheme::RedisKv,
            soft_kill: true,
        };
        // Inserts stay on Postgres during soft-kill rollout.
        assert_eq!(
            decide_storage_scheme::<FingerprintTableNew>(&DummyKv, soft_kill, Op::Insert).await,
            StorageScheme::PostgresOnly
        );
        // Reads try Redis first during soft-kill rollout.
        let soft_kill = TableKvSettings {
            storage_scheme: StorageScheme::PostgresOnly,
            soft_kill: true,
        };
        assert_eq!(
            decide_storage_scheme::<FingerprintTableNew>(&DummyKv, soft_kill, Op::Find).await,
            StorageScheme::RedisKv
        );
    }

    #[tokio::test]
    async fn soft_kill_update_short_circuits_for_postgres_only_and_none() {
        let soft_kill = TableKvSettings {
            storage_scheme: StorageScheme::RedisKv,
            soft_kill: true,
        };
        let pkey = PartitionKey::Vault {
            entity_id: "merchant_123",
            vault_id: "vault_123",
        };

        // Arm 1 — updated_by = Some(PostgresOnly) → PostgresOnly, no probe.
        assert_eq!(
            decide_storage_scheme::<FingerprintTableNew>(
                &DummyKv,
                soft_kill,
                Op::Update(pkey.clone(), Some(StorageScheme::PostgresOnly)),
            )
            .await,
            StorageScheme::PostgresOnly
        );

        // Arm 3 — updated_by = None → PostgresOnly, no probe.
        assert_eq!(
            decide_storage_scheme::<FingerprintTableNew>(
                &DummyKv,
                soft_kill,
                Op::Update(pkey, None),
            )
            .await,
            StorageScheme::PostgresOnly
        );
    }

    #[tokio::test]
    async fn soft_kill_update_probes_redis_for_redis_kv() {
        // Arm 2 — updated_by = Some(RedisKv) → probe Redis.
        // DummyKv has no Redis connection, so the probe returns Err → PostgresOnly.
        let soft_kill = TableKvSettings {
            storage_scheme: StorageScheme::RedisKv,
            soft_kill: true,
        };
        let pkey = PartitionKey::Vault {
            entity_id: "merchant_123",
            vault_id: "vault_123",
        };

        assert_eq!(
            decide_storage_scheme::<FingerprintTableNew>(
                &DummyKv,
                soft_kill,
                Op::Update(pkey, Some(StorageScheme::RedisKv)),
            )
            .await,
            StorageScheme::PostgresOnly
        );
    }
}
