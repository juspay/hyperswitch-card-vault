use tracing::debug;

use crate::storage::scheme::StorageScheme;

/// Tri-state KV master switch.
///
/// `ttl_for_kv` must exceed max drainer replay lag — otherwise a KV-only
/// fingerprint can expire in Redis before reaching Postgres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub(crate) enum KvState {
    #[default]
    Disabled,
    /// Write-through Redis; drainer replays to Postgres.
    Enabled,
    /// Insert to Postgres only; reads prefer Redis.
    SoftKill,
}

/// Operation type used by `decide_storage_scheme`.
///
/// `Update` is not yet included — fingerprint only has insert/find. When vault
/// migrates, `Op::Update` will be added with a Redis probe in SoftKill mode.
#[derive(Debug, Clone, Copy, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum Op {
    Insert,
    Find,
    Delete,
}

/// Effective storage scheme for an operation.
///
/// `SoftKill` + `Find` → `RedisKv`: check Redis first; on `NotFound` fall back to Postgres
/// (handled by `find_optional_resource_by_id`). `SoftKill` + `Insert` → `PostgresOnly`:
/// writes bypass Redis, so no new drainer entries are produced during rollout.
pub(crate) fn decide_storage_scheme(state: KvState, operation: Op) -> StorageScheme {
    match state {
        KvState::Disabled => StorageScheme::PostgresOnly,
        KvState::Enabled => StorageScheme::RedisKv,
        KvState::SoftKill => {
            let scheme = match operation {
                Op::Insert => StorageScheme::PostgresOnly,
                Op::Find => StorageScheme::RedisKv,
                Op::Delete => StorageScheme::PostgresOnly,
            };
            debug!(%scheme, %operation, "soft-kill routing");
            scheme
        }
    }
}
