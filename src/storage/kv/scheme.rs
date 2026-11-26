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
/// Why a KV state change was refused. Kept apart so the caller can report each case
/// with its own message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KvTransitionRejection {
    /// The move is not allowed from the current state.
    IllegalMove,
    /// The move is allowed, but it leaves `Enabled` while replica reads are already on.
    ReplicaStillEnabled,
}

impl KvState {
    /// Validate a state change against the **persisted** state only: `self` and
    /// `current_use_replica` are both what is stored, never what the same request also
    /// asks for, so nothing else in its body can make an invalid move pass.
    ///
    /// `can_enable_kv` is `true` when Redis is confirmed reachable — required to leave
    /// `Disabled`. Leaving `Enabled` is refused while replica reads are on: they depend on
    /// KV and the read path never re-checks that, so they go off in an earlier write.
    pub(crate) fn validate_transition(
        self,
        requested: Self,
        can_enable_kv: bool,
        current_use_replica: bool,
    ) -> Result<(), KvTransitionRejection> {
        if self.apply_candidate(requested, can_enable_kv) != requested {
            return Err(KvTransitionRejection::IllegalMove);
        }

        if self == Self::Enabled && requested != Self::Enabled && current_use_replica {
            return Err(KvTransitionRejection::ReplicaStillEnabled);
        }

        Ok(())
    }

    fn apply_candidate(self, requested: Self, can_enable_kv: bool) -> Self {
        match (self, requested) {
            (current, requested) if current == requested => current,
            (Self::Disabled, Self::Enabled) if can_enable_kv => Self::Enabled,
            (Self::Enabled, Self::SoftKill) => Self::SoftKill,
            (Self::SoftKill, Self::Enabled) if can_enable_kv => Self::Enabled,
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
        assert!(
            KvState::Disabled
                .validate_transition(KvState::Enabled, true, false)
                .is_ok()
        );
        assert!(
            KvState::Enabled
                .validate_transition(KvState::SoftKill, false, false)
                .is_ok()
        );
        assert!(
            KvState::SoftKill
                .validate_transition(KvState::Disabled, false, false)
                .is_ok()
        );
        assert!(
            KvState::Enabled
                .validate_transition(KvState::Enabled, false, false)
                .is_ok()
        );
        assert!(
            KvState::SoftKill
                .validate_transition(KvState::Enabled, true, false)
                .is_ok()
        );
    }

    /// Both ways into `Enabled` route KV writes through Redis, so both need it reachable.
    #[test]
    fn rejects_enabling_kv_without_redis() {
        assert!(
            KvState::Disabled
                .validate_transition(KvState::Enabled, false, false)
                .is_err()
        );
        assert!(
            KvState::SoftKill
                .validate_transition(KvState::Enabled, false, false)
                .is_err()
        );
    }

    #[test]
    fn rejects_unsupported_kv_state_transitions() {
        assert!(
            KvState::Disabled
                .validate_transition(KvState::SoftKill, true, false)
                .is_err()
        );
        assert!(
            KvState::Enabled
                .validate_transition(KvState::Disabled, true, false)
                .is_err()
        );
    }
}
