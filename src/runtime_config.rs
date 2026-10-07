//! Runtime configuration: a compile-time registry of per-tenant config entries.
//!
//! [`RuntimeConfigKind`] is the registry — one variant per config. Rust checks
//! exhaustiveness where a `match` consumes an enum but not where code enumerates its
//! variants, so enumeration comes from `strum::EnumIter` and the rest is exhaustive
//! matching. Adding a variant is the trigger; this is how the remaining steps surface:
//!
//! | step | if forgotten |
//! |---|---|
//! | [`RuntimeConfigKind::key`] arm | compile error — non-exhaustive match |
//! | `seed_all` arm | compile error — non-exhaustive match |
//! | [`RuntimeConfigEntry`] + [`RuntimeConfigValidate`] impls | compile error — `seed::<T>`'s bound |
//! | seed field on `RuntimeConfig::Enabled` | compile error — `new`'s struct pattern |
//! | [`RuntimeConfigSeeds`] field | compile error — `new`'s struct literal |
//! | [`RuntimeConfigUpdate`] variant | test failure — `every_kind_is_updatable` |
//! | [`RuntimeConfigKind::registered_keys`], `status`, `disabled_status`, cache warming | *nothing — derived* |
//!
//! Only registry machinery lives here. Each config's own halves — key binding and
//! validation — live in a child module: `kv_config`'s in `kv_runtime_config`.

mod kv_runtime_config;

use std::{collections::HashMap, future::Future, time::Instant};

use hyperswitch_masking::PeekInterface;
use strum::IntoEnumIterator;

use crate::{
    error::{self, ContainerError},
    storage::{self, ConfigInterface, KvRuntimeConfigValues, consts},
};

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

/// Validates a requested value against the currently persisted state. No default:
/// [`RuntimeConfigEntry`] requires it as a supertrait, so no config can skip it.
pub trait RuntimeConfigValidate {
    /// Desugared rather than `async fn` so the `Send` bound can be stated — axum
    /// handlers require it.
    fn validate(
        &self,
        store: &storage::Storage,
        manager: &RuntimeConfigManager,
    ) -> impl Future<Output = Result<(), ContainerError<error::RuntimeConfigError>>> + Send;
}

/// One runtime config's `configs` row key, bound to its struct at compile time.
///
/// `KEY` is the source of truth: [`RuntimeConfigKind::key`] defers to it, leaving the
/// `#[serde(rename)]` tag on the [`RuntimeConfigUpdate`] variant as the only other copy —
/// `update_tag_matches_entry_key` asserts they agree.
pub trait RuntimeConfigEntry:
    RuntimeConfigValidate
    + serde::Serialize
    + serde::de::DeserializeOwned
    + Default
    + Clone
    + std::fmt::Debug
    + Send
    + Sync
    + Sized
{
    /// This config's `configs` row key, and its Redis cache key (the per-tenant prefix is
    /// added by `TenantAwareRedisStore`).
    const KEY: &'static str;
}

/// Runtime configuration source.
///
/// When enabled, each config lives in its own row of the per-tenant `configs` table, read
/// through a per-tenant Redis cache. The values here are *seeds* — written at startup only
/// when the row is absent (see [`RuntimeConfigManager::init`]), never a read-path fallback.
/// `admin_api_key` guards `POST /runtime-config`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RuntimeConfig {
    #[default]
    Disabled,
    Enabled {
        admin_api_key: hyperswitch_masking::Secret<String>,
        /// KV master switch and read-replica routing.
        #[serde(default)]
        kv_config: KvRuntimeConfigValues,
    },
}

/// Every runtime config the binary knows about — the registry itself; the module docs
/// table lists what adding a variant obliges you to write.
///
/// No serde derive: it is never read off the wire ([`RuntimeConfigUpdate`] does that), so
/// [`Self::key`] can defer to [`RuntimeConfigEntry::KEY`] instead of repeating the string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumIter)]
pub enum RuntimeConfigKind {
    KvConfig,
}

impl RuntimeConfigKind {
    /// This kind's `configs` row key.
    pub fn key(self) -> &'static str {
        match self {
            Self::KvConfig => KvRuntimeConfigValues::KEY,
        }
    }

    /// Every registered key, for [`RuntimeConfigManager::status`] and cache warming.
    /// Derived from the variants, so a new config needs no edit here.
    pub fn registered_keys() -> impl Iterator<Item = &'static str> {
        Self::iter().map(Self::key)
    }
}

/// Seed values lifted out of `RuntimeConfig::Enabled` so the manager can hold them.
#[derive(Debug, Clone, Default)]
pub struct RuntimeConfigSeeds {
    pub kv_config: KvRuntimeConfigValues,
}

/// Body of `POST /runtime-config`: `{"key": "<key>", "value": {...}}`.
///
/// serde does the key → struct dispatch, so an unknown key or unknown field in `value` is
/// rejected before any storage call, and the handler cannot pair a key with the wrong
/// struct. The `rename` repeats [`RuntimeConfigEntry::KEY`] because serde cannot read an
/// associated const. This is the one list [`RuntimeConfigKind`] cannot force the compiler
/// to check — `every_kind_is_updatable` covers it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "key", content = "value", deny_unknown_fields)]
pub enum RuntimeConfigUpdate {
    #[serde(rename = "kv_config")]
    KvRuntimeConfigValues(KvRuntimeConfigValues),
}

impl RuntimeConfig {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    /// Enabled mode needs an admin key to guard the update endpoint and a Redis backend,
    /// since every read goes through the per-tenant cache. Disabled mode needs neither, so
    /// both checks hang off the one `Enabled` match.
    pub fn validate(
        &self,
        redis: Option<&hyperswitch_redis_interface::RedisSettings>,
    ) -> Result<(), crate::error::ConfigurationError> {
        let Self::Enabled { admin_api_key, .. } = self else {
            return Ok(());
        };

        if admin_api_key.peek().trim().is_empty() {
            return Err(
                crate::error::ConfigurationError::InvalidConfigurationValueError(
                    r#"runtime_config.admin_api_key is required when mode is "enabled""#.into(),
                ),
            );
        }

        if redis.is_none() {
            return Err(
                crate::error::ConfigurationError::InvalidConfigurationValueError(
                    "runtime_config is enabled but `[redis]` is not configured".into(),
                ),
            );
        }

        Ok(())
    }
}

/// Runtime configuration backed by the per-tenant `configs` table with a read-through
/// Redis cache. No polling: every `get()` fetches the latest value, and `update()` upserts
/// to Postgres then invalidates the cache entry.
pub struct RuntimeConfigManager {
    admin_api_key: hyperswitch_masking::Secret<String>,
    seeds: RuntimeConfigSeeds,
}

impl RuntimeConfigManager {
    /// Capture the configured seeds. `None` when runtime config is disabled, so a
    /// manager's `admin_api_key` is always present. The tenant's `Storage` is passed per
    /// call; the manager holds no handle of its own.
    pub fn new(config: &RuntimeConfig) -> Option<Self> {
        match config {
            RuntimeConfig::Enabled {
                admin_api_key,
                kv_config,
            } => Some(Self {
                admin_api_key: admin_api_key.clone(),
                seeds: RuntimeConfigSeeds {
                    kv_config: kv_config.clone(),
                },
            }),
            RuntimeConfig::Disabled => None,
        }
    }

    /// Bootstrap: ensure every registered config has a `configs` row (seeding when
    /// missing), then warm the Redis cache.
    ///
    /// A failed seed read or upsert is fatal — a tenant must not serve with a missing row.
    /// The warm is best-effort: a failure self-heals on the next read.
    pub async fn init(
        &self,
        store: &storage::Storage,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        self.seed_all(store).await?;

        // Warm the Redis cache (read-through populates Redis on a miss).
        for key in RuntimeConfigKind::registered_keys() {
            let _ = self.get_raw(store, key).await;
        }

        Ok(())
    }

    /// Seed every registered config whose row is missing.
    ///
    /// The match is exhaustive over [`RuntimeConfigKind`], and the arm's [`Self::seed`]
    /// call is bound on [`RuntimeConfigEntry`] — so a new config cannot be registered
    /// without both a seed path and its [`RuntimeConfigValidate`] impl.
    async fn seed_all(
        &self,
        store: &storage::Storage,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        for kind in RuntimeConfigKind::iter() {
            match kind {
                RuntimeConfigKind::KvConfig => {
                    self.seed::<KvRuntimeConfigValues>(store, &self.seeds.kv_config)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Write the seed value when the config's row is absent, leaving an existing row
    /// untouched — an upgrade must never reset a tenant's live configuration.
    async fn seed<T: RuntimeConfigEntry>(
        &self,
        store: &storage::Storage,
        seed: &T,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        let existing = store.find_config(T::KEY).await.inspect_err(|err| {
            crate::logger::error!(
                ?err,
                key = T::KEY,
                "Failed to read runtime config from Postgres during init"
            );
        })?;

        match existing {
            Some(_) => {
                crate::logger::debug!(key = T::KEY, "Runtime config already present in Postgres");
            }
            None => {
                let value = serde_json::to_value(seed).map_err(|err| {
                    ContainerError::from(error::RuntimeConfigError::InvalidValue(err.to_string()))
                })?;
                store.upsert_config(T::KEY, value).await.inspect(|_| {
                    crate::logger::info!(
                        key = T::KEY,
                        "Seeded configured runtime config into Postgres"
                    );
                })?;
            }
        }

        Ok(())
    }

    /// Persist a validated config value: PG upsert, then Redis invalidate.
    ///
    /// The Redis `DEL` failure is logged as a warning (not propagated) because the Redis
    /// TTL bounds staleness — the next read eventually repopulates from Postgres.
    async fn put<T: RuntimeConfigEntry>(
        &self,
        store: &storage::Storage,
        value: &T,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        let json = serde_json::to_value(value).map_err(|err| {
            ContainerError::from(error::RuntimeConfigError::InvalidValue(err.to_string()))
        })?;

        store.upsert_config(T::KEY, json).await?;

        if let Some(redis) = store.get_redis_store() {
            redis.invalidate(T::KEY).await;
        }

        Ok(())
    }

    /// Apply an update: validate against the persisted state → PG upsert → Redis
    /// invalidate.
    pub async fn update(
        &self,
        store: &storage::Storage,
        update: RuntimeConfigUpdate,
    ) -> Result<(), ContainerError<error::RuntimeConfigError>> {
        match update {
            RuntimeConfigUpdate::KvRuntimeConfigValues(value) => {
                value.validate(store, self).await?;
                self.put(store, &value).await
            }
        }
    }

    /// Deserialize the latest value of config `T`. The key comes from `T::KEY`, so a
    /// caller cannot pair a key with the wrong struct.
    ///
    /// Read-through: Redis GET → on hit, return immediately; on miss/error, fall back to
    /// Postgres SELECT and best-effort populate Redis. Returns `None` when no config row
    /// exists or both stores are unavailable (fail-closed: callers treat `None` as
    /// KV-disabled / replica-off).
    pub async fn get<T: RuntimeConfigEntry>(&self, store: &storage::Storage) -> Option<T> {
        let raw = self.get_raw(store, T::KEY).await?;

        match serde_json::from_str::<T>(&raw) {
            Ok(val) => Some(val),
            Err(error) => {
                crate::logger::error!(
                    ?error,
                    raw,
                    key = T::KEY,
                    "Failed to deserialize runtime config"
                );
                None
            }
        }
    }

    /// Fetch a config's raw JSON through the read-through Redis cache. The only place a
    /// key is a string rather than derived from a type — [`Self::status`] spans all keys.
    async fn get_raw(&self, store: &storage::Storage, key: &str) -> Option<String> {
        let start = Instant::now();
        let fetch_from_pg = || async {
            store
                .find_config(key)
                .await
                .inspect_err(|err| {
                    crate::logger::error!(?err, key, "Failed to read runtime config from Postgres");
                })
                .ok()
                .flatten()
                .map(|value| value.to_string())
        };

        let (source, result) = match store.get_redis_store() {
            Some(redis) => {
                let result = redis
                    .get_or_populate(key, consts::RUNTIME_CONFIG_REDIS_TTL_SECS, fetch_from_pg)
                    .await;
                ("redis", result)
            }
            None => {
                crate::logger::debug!(
                    key,
                    "Redis not configured, reading runtime config from Postgres"
                );
                ("postgres", fetch_from_pg().await)
            }
        };

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

    /// Current status of every registered config, keyed by its config key. No side effects.
    pub async fn status(
        &self,
        store: &storage::Storage,
    ) -> HashMap<&'static str, RuntimeConfigStatus> {
        let mut statuses = HashMap::with_capacity(RuntimeConfigKind::registered_keys().count());

        for key in RuntimeConfigKind::registered_keys() {
            let status = match self.get_raw(store, key).await {
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
                        crate::logger::error!(?error, raw, key, "Runtime config is invalid");
                        RuntimeConfigStatus {
                            status: RuntimeConfigStatusKind::Invalid,
                            config: None,
                        }
                    }
                },
            };

            statuses.insert(key, status);
        }

        statuses
    }

    /// Constant-time comparison of a candidate API key against the configured admin key.
    pub fn verify_admin_api_key(&self, candidate: &str) -> bool {
        let expected_bytes = self.admin_api_key.peek().as_bytes();
        let candidate_bytes = candidate.as_bytes();
        constant_time_eq(expected_bytes, candidate_bytes)
    }
}

/// Status map for a tenant with runtime config disabled: every registered config reports
/// `disabled`, so the response shape does not depend on the mode.
pub fn disabled_status() -> HashMap<&'static str, RuntimeConfigStatus> {
    RuntimeConfigKind::registered_keys()
        .map(|key| {
            (
                key,
                RuntimeConfigStatus {
                    status: RuntimeConfigStatusKind::Disabled,
                    config: None,
                },
            )
        })
        .collect()
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
