use std::{sync::Arc, time::Duration};

use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};

use crate::{config::RuntimeConfig, error};

const API_KEY_HEADER_NAME: &str = "X-Internal-Api-Key";

/// Binds a runtime-config key string to the type that deserializes its value.
///
/// Fetch with `manager.get::<T>().await`.  Adding a key is one struct + one
/// `impl` line.
pub trait RuntimeConfigItem: serde::de::DeserializeOwned {
    /// The config endpoint key, e.g. `"locker.use_read_replica"`.
    const KEY: &'static str;
}

/// Response format from the runtime config endpoint:
/// ```json
/// {"key": "runtime_config", "value": "{\"use_read_replica\":true}"}
/// ```
#[derive(Debug, serde::Deserialize)]
struct RuntimeConfigResponse {
    value: String,
}

#[derive(Clone)]
enum RuntimeConfigState {
    Disabled,
    Enabled {
        endpoint_url: String,
        api_key: Secret<String>,
        client: reqwest::Client,
        keys: Vec<String>,
        ttl_seconds: u64,
        #[cfg(feature = "caching")]
        cache: moka::future::Cache<String, String>,
    },
}

/// Manages on-demand fetching and caching of runtime configs.
#[derive(Clone)]
pub struct RuntimeConfigManager {
    state: RuntimeConfigState,
}

impl RuntimeConfigManager {
    /// Construct a new runtime config manager.
    pub fn new(
        config: &RuntimeConfig,
        client_idle_timeout: u64,
        pool_max_idle_per_host: usize,
    ) -> error_stack::Result<Self, error::ConfigurationError> {
        Ok(match config {
            RuntimeConfig::Disabled => Self {
                state: RuntimeConfigState::Disabled,
            },
            #[cfg_attr(not(feature = "caching"), allow(unused_variables))]
            RuntimeConfig::Enabled {
                endpoint,
                ttl_seconds,
                cache_max_capacity,
                keys,
            } => {
                let client = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .pool_idle_timeout(Duration::from_secs(client_idle_timeout))
                    .pool_max_idle_per_host(pool_max_idle_per_host)
                    .build()
                    .change_context(error::ConfigurationError::InvalidConfigurationValueError(
                        "Failed to build HTTP client for runtime config endpoint".into(),
                    ))?;

                Self {
                    state: RuntimeConfigState::Enabled {
                        endpoint_url: endpoint.base_url.clone(),
                        api_key: endpoint.api_key.clone(),
                        client,
                        keys: keys.clone(),
                        ttl_seconds: *ttl_seconds,
                        #[cfg(feature = "caching")]
                        cache: moka::future::CacheBuilder::new(*cache_max_capacity)
                            .time_to_live(Duration::from_secs(*ttl_seconds))
                            .build(),
                    },
                }
            }
        })
    }

    #[inline]
    fn deserialize_config<T: serde::de::DeserializeOwned>(key: &str, raw: &str) -> Option<T> {
        match serde_json::from_str::<T>(raw) {
            Ok(val) => Some(val),
            Err(error) => {
                crate::logger::error!(?error, key, raw, "Failed to deserialize runtime config");
                None
            }
        }
    }

    /// Fetch a runtime config value by key, deserialized to the requested type.
    ///
    /// Returns `None` when the runtime config is disabled, the fetch fails, or
    /// deserialization fails.
    pub async fn get_config<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        let (endpoint_url, api_key, client) = match &self.state {
            RuntimeConfigState::Disabled => {
                crate::logger::debug!(key, "Runtime config is disabled, returning None");
                return None;
            }
            RuntimeConfigState::Enabled {
                endpoint_url,
                api_key,
                client,
                ..
            } => (endpoint_url, api_key, client),
        };

        #[cfg(feature = "caching")]
        if let RuntimeConfigState::Enabled { cache, .. } = &self.state {
            if let Some(val) = cache.get(key).await {
                crate::logger::debug!(key, "Runtime config cache hit");
                return Self::deserialize_config(key, &val);
            }
            crate::logger::debug!(key, "Runtime config cache miss, fetching from endpoint");
        }

        let raw = match Self::fetch(endpoint_url, api_key, client, key).await {
            Ok(val) => val,
            Err(error) => {
                crate::logger::error!(?error, key, "Failed to fetch runtime config from endpoint");
                return None;
            }
        };

        #[cfg(feature = "caching")]
        if let RuntimeConfigState::Enabled { cache, .. } = &self.state {
            crate::logger::debug!(key, "Inserting runtime config into moka cache");
            cache.insert(key.to_string(), raw.clone()).await;
        }

        Self::deserialize_config(key, &raw)
    }

    /// Fetch a runtime config item by its trait-declared key.
    ///
    /// Usage: `manager.get::<ReplicaRouting>().await`
    pub async fn get<T: RuntimeConfigItem>(&self) -> Option<T> {
        self.get_config::<T>(T::KEY).await
    }

    /// Fetch all prefetch keys and populate the moka cache.
    ///
    /// Each key fetch is fault-tolerant: failures are logged and the sweep
    /// continues.  No-op when runtime config is `Disabled`.
    #[cfg(feature = "caching")]
    async fn prefetch_keys(&self) {
        let RuntimeConfigState::Enabled {
            endpoint_url,
            api_key,
            client,
            keys,
            cache,
            ..
        } = &self.state
        else {
            return;
        };

        for key in keys {
            match Self::fetch(endpoint_url, api_key, client, key).await {
                Ok(raw) => {
                    cache.insert(key.clone(), raw).await;
                    crate::logger::debug!(key, "Prefetched runtime config key");
                }
                Err(error) => {
                    crate::logger::warn!(
                        ?error,
                        key,
                        "Failed to prefetch runtime config key, continuing"
                    );
                }
            }
        }
    }

    /// Spawn a background task that prefetches keys at startup and refreshes
    /// them at `ttl * 0.8`.  Returns `None` when runtime config is `Disabled`.
    pub fn spawn_prefetch_task(self: &Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        let ttl_seconds = match &self.state {
            RuntimeConfigState::Disabled => return None,
            RuntimeConfigState::Enabled { ttl_seconds, .. } => *ttl_seconds,
        };

        // Refresh slightly before TTL expiry so the cache never goes cold.
        #[allow(clippy::as_conversions)]
        let refresh_secs = (ttl_seconds as f64 * 0.8).max(1.0) as u64;
        let refresh_interval = Duration::from_secs(refresh_secs);

        let manager = Arc::clone(self);
        crate::logger::info!(
            refresh_interval_secs = refresh_interval.as_secs(),
            "Spawning runtime config prefetch task"
        );

        Some(tokio::spawn(async move {
            #[cfg(feature = "caching")]
            manager.prefetch_keys().await;

            let mut ticker = tokio::time::interval(refresh_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;
                #[cfg(feature = "caching")]
                manager.prefetch_keys().await;
            }
        }))
    }

    async fn fetch(
        endpoint_url: &str,
        api_key: &Secret<String>,
        client: &reqwest::Client,
        key: &str,
    ) -> error_stack::Result<String, error::ConfigurationError> {
        let url = format!("{}/{}", endpoint_url.trim_end_matches('/'), key);

        let response = client
            .get(&url)
            .header(API_KEY_HEADER_NAME, api_key.peek())
            .send()
            .await
            .change_context(error::ConfigurationError::InvalidConfigurationValueError(
                "Failed to send runtime config request".into(),
            ))?;

        if !response.status().is_success() {
            return Err(error_stack::report!(
                error::ConfigurationError::InvalidConfigurationValueError(format!(
                    "Runtime config request returned non-success status ({})",
                    response.status()
                ))
            ));
        }

        let parsed: RuntimeConfigResponse = response.json().await.change_context(
            error::ConfigurationError::InvalidConfigurationValueError(
                "Failed to parse runtime config response".into(),
            ),
        )?;

        crate::logger::info!(%key, config = ?parsed, "Retrieved runtime config");

        Ok(parsed.value)
    }
}
