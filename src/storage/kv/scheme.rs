/// Tri-state KV master switch.
///
/// `ttl_for_kv` must exceed max drainer replay lag — otherwise a KV-only
/// fingerprint can expire in Redis before reaching Postgres.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize, strum::Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum KvState {
    #[default]
    Disabled,
    /// Write-through Redis; drainer replays to Postgres.
    Enabled,
    /// Insert to Postgres only; reads prefer Redis.
    SoftKill,
}

/// Transitions are enforced at the runtime-config write path (`POST /runtime-config`)
/// by comparing the persisted (previous) state against the requested state — the KV
/// state itself is never held in-process, only read from Postgres/Redis per operation.
impl KvState {
    /// Validate a requested state transition. `can_enable_kv` is `true` when the KV
    /// backend (Redis) is confirmed reachable — required to leave `Disabled`.
    pub(crate) fn is_valid_transition(self, requested: Self, can_enable_kv: bool) -> bool {
        self.apply_candidate(requested, can_enable_kv) == requested
    }

    fn apply_candidate(self, requested: Self, can_enable_kv: bool) -> Self {
        match (self, requested) {
            (current, requested) if current == requested => current,
            (Self::Disabled, Self::Enabled) if can_enable_kv => Self::Enabled,
            (Self::Enabled, Self::SoftKill) => Self::SoftKill,
            (Self::SoftKill, Self::Disabled) => Self::Disabled,
            _ => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::KvState;

    #[test]
    fn allows_valid_kv_state_transitions() {
        assert!(KvState::Disabled.is_valid_transition(KvState::Enabled, true));
        assert!(KvState::Enabled.is_valid_transition(KvState::SoftKill, false));
        assert!(KvState::SoftKill.is_valid_transition(KvState::Disabled, false));
        assert!(KvState::Enabled.is_valid_transition(KvState::Enabled, false));
    }

    #[test]
    fn rejects_disabled_to_enabled_without_redis() {
        assert!(!KvState::Disabled.is_valid_transition(KvState::Enabled, false));
    }

    #[test]
    fn rejects_unsupported_kv_state_transitions() {
        assert!(!KvState::Disabled.is_valid_transition(KvState::SoftKill, true));
        assert!(!KvState::Enabled.is_valid_transition(KvState::Disabled, true));
        assert!(!KvState::SoftKill.is_valid_transition(KvState::Enabled, true));
    }
}
