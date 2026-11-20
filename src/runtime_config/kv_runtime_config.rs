//! The `kv_config` runtime config: [`KvRuntimeConfigValues`]'s key binding and its checks.
//! The registry machinery that must name every config stays in the parent module.

use super::{RuntimeConfigEntry, RuntimeConfigValidate};
use crate::{
    error::{self, ContainerError},
    storage::{self, KvRuntimeConfigValues},
};

impl RuntimeConfigEntry for KvRuntimeConfigValues {
    const KEY: &'static str = "kv_config";
}

/// Required by `RuntimeConfigEntry`'s supertrait bound.
impl RuntimeConfigValidate for KvRuntimeConfigValues {
    async fn validate_transition(
        &self,
        previous: &Self,
        store: &storage::Storage,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        // Both checks take their inputs from `previous` — the persisted state — so neither
        // field's verdict depends on what else this same request carries.
        #[cfg(feature = "kv")]
        validate_kv_transition(
            store,
            previous.enable_kv,
            self.enable_kv,
            previous.use_replica,
        )
        .await?;

        // Only a write that actually turns replica reads on is gated, so a no-op leaves an
        // already-enabled replica alone instead of trapping the row.
        if self.use_replica && !previous.use_replica {
            #[cfg(feature = "kv")]
            validate_kv_enabled_for_replica(previous.enable_kv)?;

            validate_replica_reachable(store).await?;
        }

        Ok(())
    }
}

/// Surface [`KvState::validate_transition`]'s verdict as a runtime-config error.
///
/// The rules live on [`KvState`]; this supplies the one input it cannot see — whether Redis
/// is reachable, needed to leave `Disabled` — and names each rejection.
#[cfg(feature = "kv")]
async fn validate_kv_transition(
    store: &storage::Storage,
    current: storage::kv::KvState,
    requested: storage::kv::KvState,
    current_use_replica: bool,
) -> Result<(), ContainerError<error::RuntimeConfigError>> {
    use storage::kv::KvTransitionRejection;

    let is_redis_working = match store.get_redis_store() {
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

    match current.validate_transition(requested, is_redis_working, current_use_replica) {
        Ok(()) => Ok(()),
        Err(KvTransitionRejection::IllegalMove) => {
            crate::logger::warn!(
                current = %current,
                requested = %requested,
                "KV state transition rejected"
            );
            Err(ContainerError::from(
                error::RuntimeConfigError::InvalidStateTransition(format!(
                    "{current} -> {requested}"
                )),
            ))
        }
        Err(KvTransitionRejection::ReplicaStillEnabled) => {
            crate::logger::warn!(
                requested = %requested,
                "enable_kv step-down rejected: use_replica is still true"
            );
            Err(ContainerError::from(
                error::RuntimeConfigError::ReplicaMustBeDisabledFirst,
            ))
        }
    }
}

/// Replica reads depend on KV, so they can only be switched on from a *persisted* `Enabled`
/// — enabling both in one request is refused and takes two calls. `should_use_replica`
/// never re-checks this at read time, which is why it is gated here.
#[cfg(feature = "kv")]
fn validate_kv_enabled_for_replica(
    current: storage::kv::KvState,
) -> Result<(), ContainerError<error::RuntimeConfigError>> {
    if current == storage::kv::KvState::Enabled {
        return Ok(());
    }

    crate::logger::warn!(
        current = %current,
        "use_replica enablement rejected: KV is not enabled"
    );
    Err(ContainerError::from(
        error::RuntimeConfigError::ReplicaRequiresKv,
    ))
}

/// Reject enabling replica reads with no replica pool configured, or an unreachable one.
/// Enforced at the write path, there being no in-process state to consult.
async fn validate_replica_reachable(
    store: &storage::Storage,
) -> Result<(), ContainerError<error::RuntimeConfigError>> {
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
