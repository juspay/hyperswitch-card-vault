use tracing::info;

use crate::storage::scheme::StorageScheme;

/// KV settings resolved at runtime from `locker.enable_kv`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize,
)]
pub(crate) struct TableKvSettings {
    #[serde(default)]
    pub storage_scheme: StorageScheme,
    #[serde(default)]
    pub soft_kill: bool,
}

/// Operation type used by `decide_storage_scheme`.
#[derive(Debug, Clone)]
pub(crate) enum Op {
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

/// Effective storage scheme for an operation, accounting for soft-kill.
pub(crate) async fn decide_storage_scheme(
    settings: TableKvSettings,
    operation: Op,
) -> StorageScheme {
    if !settings.soft_kill {
        return settings.storage_scheme;
    }

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
}
