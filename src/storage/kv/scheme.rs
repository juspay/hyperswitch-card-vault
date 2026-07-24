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

impl KvState {
    pub(crate) fn apply_transition(self, requested: Self, can_enable_kv: bool) -> Self {
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
    fn applies_allowed_kv_state_transitions() {
        assert_eq!(
            KvState::Disabled.apply_transition(KvState::Enabled, true),
            KvState::Enabled
        );
        assert_eq!(
            KvState::Enabled.apply_transition(KvState::SoftKill, false),
            KvState::SoftKill
        );
        assert_eq!(
            KvState::SoftKill.apply_transition(KvState::Disabled, false),
            KvState::Disabled
        );
    }

    #[test]
    fn ignores_disabled_to_enabled_without_redis() {
        assert_eq!(
            KvState::Disabled.apply_transition(KvState::Enabled, false),
            KvState::Disabled
        );
    }

    #[test]
    fn ignores_unsupported_kv_state_transitions() {
        assert_eq!(
            KvState::Disabled.apply_transition(KvState::SoftKill, true),
            KvState::Disabled
        );
        assert_eq!(
            KvState::Enabled.apply_transition(KvState::Disabled, true),
            KvState::Enabled
        );
        assert_eq!(
            KvState::SoftKill.apply_transition(KvState::Enabled, true),
            KvState::SoftKill
        );
    }
}
