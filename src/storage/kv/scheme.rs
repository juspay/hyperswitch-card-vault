use std::str::FromStr;

use tracing::debug;

use crate::storage::scheme::StorageScheme;

/// Tri-state KV master switch.
///
/// `ttl_for_kv` must exceed max drainer replay lag — otherwise a KV-only
/// fingerprint can expire in Redis before reaching Postgres.
///
/// Deserialization accepts `"disabled"` / `"enabled"` / `"soft_kill"` as a
/// bare string or `{"kv_state": "..."}` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum KvState {
    #[default]
    Disabled,
    /// Write-through Redis; drainer replays to Postgres.
    Enabled,
    /// Insert to Postgres only; reads prefer Redis.
    SoftKill,
}

impl<'de> serde::Deserialize<'de> for KvState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct KvStateVisitor;

        impl<'de> de::Visitor<'de> for KvStateVisitor {
            type Value = KvState;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(r#"a kv_state string ("disabled"/"enabled"/"soft_kill") or {"kv_state": "..."}"#)
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                KvState::from_str(v)
                    .map_err(|_| de::Error::custom(format!("unknown kv_state: {v}")))
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                self.visit_str(&v)
            }

            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut kv_state: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kv_state" => kv_state = Some(map.next_value()?),
                        other => return Err(de::Error::unknown_field(other, &["kv_state"])),
                    }
                }

                let state = kv_state.ok_or_else(|| de::Error::missing_field("kv_state"))?;
                KvState::from_str(&state)
                    .map_err(|_| de::Error::custom(format!("unknown kv_state: {state}")))
            }
        }

        deserializer.deserialize_any(KvStateVisitor)
    }
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
            };
            debug!(%scheme, %operation, "soft-kill routing");
            scheme
        }
    }
}
