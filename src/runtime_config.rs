use std::{sync::Arc, time::Duration};

use error_stack::ResultExt;
#[cfg(feature = "caching")]
use hyperswitch_masking::PeekInterface;
use hyperswitch_masking::Secret;

use crate::{config::RuntimeConfig, error};

#[cfg(feature = "caching")]
const API_KEY_HEADER_NAME: &str = "X-Internal-Api-Key";

/// Binds a runtime-config key string to the type that deserializes its value.
pub trait RuntimeConfigItem: serde::de::DeserializeOwned {
    const KEY: &'static str;
}

/// Response format from the runtime config endpoint:
/// ```json
/// {"key": "runtime_config", "value": "{\"use_read_replica\":true}"}
/// ```
#[cfg(feature = "caching")]
#[derive(Debug, serde::Deserialize)]
struct RuntimeConfigResponse {
    #[expect(dead_code)]
    key: String,
    value: String,
}

#[derive(Clone)]
enum RuntimeConfigState {
    Disabled,
    #[cfg_attr(not(feature = "caching"), allow(dead_code))]
    Enabled {
        endpoint_url: String,
        api_key: Secret<String>,
        client: reqwest::Client,
        keys: Vec<String>,
        refresh_interval_seconds: u64,
        #[cfg(feature = "caching")]
        cache: moka::future::Cache<String, String>,
    },
}

/// Manages fetching and caching of runtime configs.
#[derive(Clone)]
pub struct RuntimeConfigManager {
    state: RuntimeConfigState,
}

impl RuntimeConfigManager {
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
                ttl_seconds: _,
                cache_max_capacity,
                keys,
                refresh_interval_seconds,
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
                        refresh_interval_seconds: *refresh_interval_seconds,
                        #[cfg(feature = "caching")]
                        cache: moka::future::CacheBuilder::new(*cache_max_capacity).build(),
                    },
                }
            }
        })
    }

    #[cfg(feature = "caching")]
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
    /// Cache-only: the prefetch task is the sole fetcher, so the hot path
    /// never blocks on the endpoint. A miss returns `None` and callers fall
    /// back to TOML defaults.
    pub async fn get_config<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        #[cfg(feature = "caching")]
        {
            let RuntimeConfigState::Enabled { cache, .. } = &self.state else {
                return None;
            };

            if let Some(val) = cache.get(key).await {
                return Self::deserialize_config(key, &val);
            }
        }

        #[cfg(not(feature = "caching"))]
        {
            let _ = key;
            let _ = &self.state;
        }

        None
    }

    /// Fetch a runtime config item by its trait-declared key.
    pub async fn get<T: RuntimeConfigItem>(&self) -> Option<T> {
        self.get_config::<T>(T::KEY).await
    }

    /// Spawn a background prefetch task. Returns `None` when disabled.
    #[cfg(feature = "caching")]
    pub fn spawn_prefetch_task(self: &Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        let refresh_interval = match &self.state {
            RuntimeConfigState::Disabled => return None,
            RuntimeConfigState::Enabled {
                refresh_interval_seconds,
                ..
            } => Duration::from_secs(*refresh_interval_seconds),
        };

        let manager = Arc::clone(self);
        crate::logger::info!(
            refresh_interval_secs = refresh_interval.as_secs(),
            "Spawning runtime config prefetch task"
        );

        Some(tokio::spawn(async move {
            manager.prefetch_keys().await;

            let mut ticker = tokio::time::interval(refresh_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;
                manager.prefetch_keys().await;
            }
        }))
    }

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

    #[cfg(feature = "caching")]
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

#[cfg(not(feature = "caching"))]
impl RuntimeConfigManager {
    pub fn spawn_prefetch_task(self: &Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        None
    }
}
