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
#[derive(Debug, Clone, Copy, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum Op {
    Insert,
    Find,
    Update,
    Delete,
}
