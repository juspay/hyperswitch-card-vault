use std::str::FromStr;

use tracing::debug;

use crate::storage::scheme::StorageScheme;

/// Tri-state KV master switch, replacing `enable_kv`/`soft_kill` so the
/// invalid `enable_kv=false, soft_kill=true` is unrepresentable.
///
/// # Durability invariant
///
/// `ttl_for_kv` must exceed max drainer replay lag — otherwise a KV-only
/// fingerprint can expire in Redis before the drainer replays to Postgres,
/// yielding a duplicate logical fingerprint.
///
/// Backward-compatible deserialization accepts `"disabled"` / `"enabled"` /
/// `"soft_kill"` (bare string or `{"kv_state": "..."}`) **or** the legacy
/// `{"enable_kv": bool, "soft_kill": bool}` object.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, strum::Display, strum::EnumString,
)]
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
                f.write_str(
                    "a kv_state string (\"disabled\"/\"enabled\"/\"soft_kill\") \
                     or a legacy {enable_kv, soft_kill} object",
                )
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                KvState::from_str(v)
                    .map_err(|_| de::Error::custom(format!("unknown kv_state: {v}")))
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                self.visit_str(&v)
            }

            /// Accepts bare string, `{"kv_state": "..."}`, or legacy `{"enable_kv", "soft_kill"}`.
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut enable_kv: Option<bool> = None;
                let mut soft_kill = false;
                let mut kv_state: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "enable_kv" => enable_kv = Some(map.next_value()?),
                        "soft_kill" => soft_kill = map.next_value()?,
                        "kv_state" => kv_state = Some(map.next_value()?),
                        other => {
                            return Err(de::Error::unknown_field(
                                other,
                                &["enable_kv", "soft_kill", "kv_state"],
                            ))
                        }
                    }
                }

                // New form takes precedence.
                if let Some(state_str) = kv_state {
                    return KvState::from_str(&state_str).map_err(|_| {
                        de::Error::custom(format!("unknown kv_state: {state_str}"))
                    });
                }

                // Legacy form.
                let enable_kv = enable_kv.ok_or_else(|| de::Error::missing_field("enable_kv"))?;
                Ok(match (enable_kv, soft_kill) {
                    (false, _) => KvState::Disabled,
                    (true, false) => KvState::Enabled,
                    (true, true) => KvState::SoftKill,
                })
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
pub(crate) fn decide_storage_scheme(state: KvState, operation: Op) -> StorageScheme {
    match state {
        KvState::Disabled => StorageScheme::PostgresOnly,
        KvState::Enabled => StorageScheme::RedisKv,
        KvState::SoftKill => {
            let scheme = match operation {
                Op::Insert => StorageScheme::PostgresOnly,
                Op::Find => StorageScheme::RedisKv,
            };
            debug!(
                kv_state = "soft_kill",
                decided_scheme = %scheme,
                operation = %operation,
                "soft-kill routing: inserts bypass Redis, reads prefer Redis"
            );
            scheme
        }
    }
}
