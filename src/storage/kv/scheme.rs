use tracing::info;

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
/// Only the tables wired into the live KV paths are present; `locker` / `vault`
/// are re-added when those tables gain KV support.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize, strum::Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum KvTable {
    Fingerprint,
    HashTable,
    ReverseLookup,
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
#[derive(Debug, Clone, Copy)]
pub enum Op {
    Insert,
    Find,
}

impl std::fmt::Display for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Insert => f.write_str("insert"),
            Self::Find => f.write_str("find"),
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
///
/// Vendored from `storage_impl/src/redis/kv_store.rs::decide_storage_scheme`.
/// The `Update` soft-kill HGet-probe is deferred until the drainer lands; when
/// it returns, re-add the `store`/`async` shape and the `Op::Update` variant
/// together.
pub fn decide_storage_scheme(settings: TableKvSettings, operation: Op) -> StorageScheme {
    if settings.soft_kill {
        let updated_scheme = match operation {
            Op::Insert => StorageScheme::PostgresOnly,
            Op::Find => StorageScheme::RedisKv,
        };
        info!(
            soft_kill_mode = "decide_storage_scheme",
            decided_scheme = %updated_scheme,
            configured_scheme = %settings.storage_scheme,
            operation = %operation,
        );
        updated_scheme
    } else {
        settings.storage_scheme
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn soft_kill_routes_insert_to_postgres_and_find_to_redis() {
        let soft_kill = TableKvSettings {
            storage_scheme: StorageScheme::RedisKv,
            soft_kill: true,
        };
        // Inserts stay on Postgres during soft-kill rollout.
        assert_eq!(
            decide_storage_scheme(soft_kill, Op::Insert),
            StorageScheme::PostgresOnly
        );
        // Reads try Redis first during soft-kill rollout.
        let soft_kill = TableKvSettings {
            storage_scheme: StorageScheme::PostgresOnly,
            soft_kill: true,
        };
        assert_eq!(
            decide_storage_scheme(soft_kill, Op::Find),
            StorageScheme::RedisKv
        );
    }
}
