use std::time::Instant;

use hyperswitch_masking::PeekInterface;

use crate::{
    config::RuntimeConfig,
    error::{self, ContainerError},
    storage::{self, ConfigInterface, consts},
};

/// The key identifying the runtime config in both Postgres and Redis.
const CONFIG_KEY: &str = consts::RUNTIME_CONFIG_KEY;

#[derive(Debug, serde::Serialize)]
pub struct RuntimeConfigStatus {
    pub status: RuntimeConfigStatusKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeConfigStatusKind {
    Disabled,
    NotConfigured,
    Available,
    Invalid,
}

/// Manages runtime configuration (`use_replica`, `enable_kv`) backed by the per-tenant
/// `configs` Postgres table with a read-through per-tenant Redis cache.
///
/// No polling: every `get()` fetches the latest value from Redis (or Postgres on a cache
/// miss). `update()` upserts to Postgres and invalidates the Redis cache entry.
pub struct RuntimeConfigManager {
    admin_api_key: hyperswitch_masking::Secret<String>,
}

impl RuntimeConfigManager {
    /// Construct a runtime config manager from the global `RuntimeConfig` settings.
    /// Returns `None` when runtime config is disabled — a manager only exists when the
    /// feature is enabled, so its `admin_api_key` is always present. The owning tenant's
    /// `Storage` is passed to each method — the manager holds no storage handle of its own.
    pub fn new(config: &RuntimeConfig) -> Option<Self> {
        match config {
            RuntimeConfig::Enabled { admin_api_key } => Some(Self {
                admin_api_key: admin_api_key.clone(),
            }),
            RuntimeConfig::Disabled => None,
        }
    }

    /// Bootstrap: ensure a `configs` row exists (seed the safe default if missing) and
    /// warm the Redis cache.
    ///
    /// Fails when the seed read or upsert errors — the tenant must not start serving
    /// with no runtime-config row. The Redis warm is best-effort only: a failed warm
    /// self-heals on the next read (read-through populate, TTL-bounded).
    pub async fn init(
        &self,
        store: &storage::Storage,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        let existing = store.find_config(CONFIG_KEY).await.inspect_err(|err| {
            crate::logger::error!(
                ?err,
                "Failed to read runtime config from Postgres during init"
            );
        })?;

        match existing {
            Some(_) => {
                crate::logger::debug!("Runtime config already present in Postgres");
            }
            None => {
                let default = storage::RuntimeConfigValues::default();
                let value = serde_json::to_value(&default).map_err(|err| {
                    ContainerError::from(error::RuntimeConfigError::InvalidValue(err.to_string()))
                })?;
                store.upsert_config(CONFIG_KEY, value).await.inspect(|_| {
                    crate::logger::info!("Seeded default runtime config into Postgres");
                })?;
            }
        }

        // Warm the Redis cache (read-through populates Redis on a miss).
        let _ = self.get::<serde_json::Value>(store).await;
        Ok(())
    }

    /// Deserialize the latest runtime config into `T`.
    ///
    /// Read-through: Redis GET → on hit, return immediately; on miss/error, fall back to
    /// Postgres SELECT and best-effort populate Redis. Returns `None` when no config row
    /// exists or both stores are unavailable (fail-closed: callers treat `None` as
    /// KV-disabled / replica-off).
    pub async fn get<T: serde::de::DeserializeOwned>(&self, store: &storage::Storage) -> Option<T> {
        let raw = self.get_raw(store).await?;

        match serde_json::from_str::<T>(&raw) {
            Ok(val) => Some(val),
            Err(error) => {
                crate::logger::error!(?error, raw, "Failed to deserialize runtime config");
                None
            }
        }
    }

    /// Fetch the raw JSON string, going through the read-through Redis cache.
    async fn get_raw(&self, store: &storage::Storage) -> Option<String> {
        let start = Instant::now();
        let fetch_from_pg = || async {
            store
                .find_config(CONFIG_KEY)
                .await
                .inspect_err(|err| {
                    crate::logger::error!(?err, "Failed to read runtime config from Postgres");
                })
                .ok()
                .flatten()
                .map(|value| value.to_string())
        };

        #[cfg(feature = "redis")]
        let (source, result) = match store.get_redis_store() {
            Some(redis) => {
                let result = redis
                    .get_or_populate(
                        CONFIG_KEY,
                        consts::RUNTIME_CONFIG_REDIS_TTL_SECS,
                        fetch_from_pg,
                    )
                    .await;
                ("redis", result)
            }
            None => {
                crate::logger::debug!("Redis not configured, reading runtime config from Postgres");
                ("postgres", fetch_from_pg().await)
            }
        };

        #[cfg(not(feature = "redis"))]
        let (source, result) = ("postgres", fetch_from_pg().await);

        crate::observability::metrics::RUNTIME_CONFIG_FETCH_DURATION.record(
            start.elapsed().as_secs_f64(),
            metrics_utils::metric_attributes!(
                ("source", source),
                (
                    "outcome",
                    if result.is_some() { "success" } else { "error" }
                )
            ),
        );

        result
    }

    /// Update the runtime config: validate → check transition → PG upsert → Redis invalidate.
    ///
    /// The requested KV state transition is validated against the currently persisted
    /// state (Postgres/Redis cache) — there is no in-process KV state to consult.
    /// The Redis `DEL` failure is logged as a warning (not propagated) because the
    /// Redis TTL bounds staleness — the next read will eventually repopulate from PG.
    pub async fn update(
        &self,
        store: &storage::Storage,
        value: serde_json::Value,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        // Validate by parsing into RuntimeConfigValues (fails closed on unknown fields).
        let requested = serde_json::from_value::<storage::RuntimeConfigValues>(value.clone())
            .map_err(|err| {
                ContainerError::from(error::RuntimeConfigError::InvalidValue(err.to_string()))
            })?;

        #[cfg(feature = "kv")]
        self.validate_kv_transition(store, requested.enable_kv)
            .await?;

        self.validate_replica_enablement(store, requested.use_replica)
            .await?;

        store.upsert_config(CONFIG_KEY, value).await?;

        #[cfg(feature = "redis")]
        if let Some(redis) = store.get_redis_store() {
            redis.invalidate(CONFIG_KEY).await;
        }

        Ok(())
    }

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
            .get::<storage::RuntimeConfigValues>(store)
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

    /// Returns the current runtime-config status without side effects.
    pub async fn status(&self, store: &storage::Storage) -> RuntimeConfigStatus {
        let raw = self.get_raw(store).await;

        match raw {
            None => RuntimeConfigStatus {
                status: RuntimeConfigStatusKind::NotConfigured,
                config: None,
            },
            Some(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(config) => RuntimeConfigStatus {
                    status: RuntimeConfigStatusKind::Available,
                    config: Some(config),
                },
                Err(error) => {
                    crate::logger::error!(?error, raw, "Runtime config is invalid");
                    RuntimeConfigStatus {
                        status: RuntimeConfigStatusKind::Invalid,
                        config: None,
                    }
                }
            },
        }
    }

    /// Constant-time comparison of a candidate API key against the configured admin key.
    pub fn verify_admin_api_key(&self, candidate: &str) -> bool {
        let expected_bytes = self.admin_api_key.peek().as_bytes();
        let candidate_bytes = candidate.as_bytes();
        constant_time_eq(expected_bytes, candidate_bytes)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
