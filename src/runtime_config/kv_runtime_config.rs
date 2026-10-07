//! The `kv_config` runtime config: everything that pairs [`KvRuntimeConfigValues`] with
//! its `configs` row key and validates a requested value against the persisted one.
//!
//! The parent module owns the registry machinery that must name every config
//! (`RuntimeConfig`, `REGISTERED_KEYS`, `RuntimeConfigSeeds`, `RuntimeConfigUpdate` and
//! the manager's dispatch arms); what lives here is specific to this one config — its
//! [`RuntimeConfigEntry`] key binding, its [`RuntimeConfigValidate`] impl, and the two
//! checks that impl runs.

use super::{RuntimeConfigEntry, RuntimeConfigManager, RuntimeConfigValidate};
use crate::{
    error::{self, ContainerError},
    storage::{self, KvRuntimeConfigValues},
};

impl RuntimeConfigEntry for KvRuntimeConfigValues {
    const KEY: &'static str = "kv_config";
}

/// Required by `RuntimeConfigEntry`'s supertrait bound.
impl RuntimeConfigValidate for KvRuntimeConfigValues {
    async fn validate(
        &self,
        store: &storage::Storage,
        manager: &RuntimeConfigManager,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        #[cfg(feature = "kv")]
        manager
            .validate_kv_transition(store, self.enable_kv)
            .await?;

        manager
            .validate_replica_enablement(store, self.use_replica)
            .await
    }
}

/// The checks behind this config's `validate`. Written against `RuntimeConfigManager`
/// rather than as free functions because both read the persisted state through
/// [`RuntimeConfigManager::get`].
impl RuntimeConfigManager {
    /// Reject illegal KV state transitions against the currently persisted state.
    ///
    /// `Disabled → Enabled` additionally requires a reachable Redis backend, since KV
    /// writes go through Redis.
    #[cfg(feature = "kv")]
    async fn validate_kv_transition(
        &self,
        store: &storage::Storage,
        requested: storage::kv::KvState,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        use storage::kv::KvState;

        let current = self
            .get::<KvRuntimeConfigValues>(store)
            .await
            .map(|values| values.enable_kv)
            .unwrap_or(KvState::Disabled);

        let can_enable_kv = match store.get_redis_store() {
            Some(redis) => redis
                .test()
                .await
                .inspect_err(|err| {
                    crate::logger::error!(
                        ?err,
                        "Redis health check failed while validating KV enablement"
                    );
                })
                .is_ok(),
            None => false,
        };

        if current.is_valid_transition(requested, can_enable_kv) {
            return Ok(());
        }

        crate::logger::warn!(
            current = %current,
            requested = %requested,
            "KV state transition rejected"
        );
        Err(ContainerError::from(
            error::RuntimeConfigError::InvalidStateTransition(format!("{current} -> {requested}")),
        ))
    }

    /// Reject enabling replica reads when no replica pool is configured or the replica
    /// is unreachable. Mirrors the previous global-state refresh behaviour, enforced at
    /// the only write path now (no in-process state to consult).
    async fn validate_replica_enablement(
        &self,
        store: &storage::Storage,
        requested_use_replica: bool,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        if !requested_use_replica {
            return Ok(());
        }

        if !store.has_replica() {
            return Err(ContainerError::from(
                error::RuntimeConfigError::NoReplicaConfigured,
            ));
        }

        store.get_replica_conn().await.map(|_| ()).map_err(|err| {
            crate::logger::error!(
                ?err,
                "Replica health check failed while validating use_replica"
            );
            ContainerError::from(error::RuntimeConfigError::ReplicaUnreachable)
        })
    }
}
