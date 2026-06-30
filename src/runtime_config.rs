use std::{collections::HashMap, sync::Arc, time::Duration};

use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};

use crate::{config::RuntimeConfig, error};

const API_KEY_HEADER_NAME: &str = "X-Internal-Api-Key";

/// Cache key for the single cached config-bundle entry.
#[cfg(feature = "caching")]
const BUNDLE_CACHE_KEY: &str = "__runtime_config_bundle__";

/// Binds a runtime-config key string to its deserialized type.
pub trait RuntimeConfigItem: serde::de::DeserializeOwned {
    /// The config endpoint key, e.g. `"locker.use_read_replica"`.
    const KEY: &'static str;
}

/// "All configs" endpoint response: config key → JSON-string value.
type ConfigBundle = HashMap<String, String>;

#[derive(Clone)]
enum RuntimeConfigState {
    Disabled,
    Enabled {
        endpoint_url: String,
        api_key: Secret<String>,
        client: reqwest::Client,
        refresh_interval_seconds: u64,
        #[cfg(feature = "caching")]
        cache: moka::future::Cache<String, Arc<ConfigBundle>>,
    },
}

/// Manages fetching and caching of runtime configs. The entire bundle is
/// fetched in one endpoint call and cached as a single moka entry.
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
                        refresh_interval_seconds: *refresh_interval_seconds,
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

    /// Fetch a config value by key. On cache miss, fetches the entire bundle.
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

        // Try the cached bundle first.
        #[cfg(feature = "caching")]
        if let RuntimeConfigState::Enabled { cache, .. } = &self.state {
            if let Some(bundle) = cache.get(BUNDLE_CACHE_KEY).await {
                crate::logger::debug!(key, "Runtime config bundle cache hit");
                if let Some(raw) = bundle.get(key) {
                    return Self::deserialize_config(key, raw);
                }
                crate::logger::warn!(key, "Key not present in runtime config bundle");
                return None;
            }
            crate::logger::debug!("Runtime config bundle cache miss, fetching from endpoint");
        }

        // Cold miss: fetch the bundle.
        let bundle = match Self::fetch_bundle(endpoint_url, api_key, client).await {
            Ok(b) => Arc::new(b),
            Err(error) => {
                crate::logger::error!(
                    ?error, key,
                    "Failed to fetch runtime config bundle from endpoint"
                );
                return None;
            }
        };

        #[cfg(feature = "caching")]
        if let RuntimeConfigState::Enabled { cache, .. } = &self.state {
            cache.insert(BUNDLE_CACHE_KEY.to_string(), Arc::clone(&bundle)).await;
        }

        bundle
            .get(key)
            .map(|raw| Self::deserialize_config(key, raw))
            .unwrap_or(None)
    }

    /// Fetch a config item by its trait-declared key.
    pub async fn get<T: RuntimeConfigItem>(&self) -> Option<T> {
        self.get_config::<T>(T::KEY).await
    }

    /// Warm the cache. No-op when runtime config is `Disabled`.
    async fn prefetch_bundle(&self) {
        let RuntimeConfigState::Enabled {
            endpoint_url,
            api_key,
            client,
            ..
        } = &self.state
        else {
            return;
        };

        match Self::fetch_bundle(endpoint_url, api_key, client).await {
            Ok(bundle) => {
                crate::logger::debug!(
                    num_configs = bundle.len(),
                    "Prefetched runtime config bundle"
                );
                #[cfg(feature = "caching")]
                if let RuntimeConfigState::Enabled { cache, .. } = &self.state {
                    cache
                        .insert(BUNDLE_CACHE_KEY.to_string(), Arc::new(bundle))
                        .await;
                }
            }
            Err(error) => {
                crate::logger::warn!(
                    ?error,
                    "Failed to prefetch runtime config bundle, continuing"
                );
            }
        }
    }

    /// Spawn a background prefetch task. Returns `None` when disabled.
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
            "Spawning runtime config bundle prefetch task"
        );

        Some(tokio::spawn(async move {
            // Warm the cache immediately at startup.
            manager.prefetch_bundle().await;

            let mut ticker = tokio::time::interval(refresh_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;
                manager.prefetch_bundle().await;
            }
        }))
    }

    /// Fetch the entire config bundle from the endpoint.
    async fn fetch_bundle(
        endpoint_url: &str,
        api_key: &Secret<String>,
        client: &reqwest::Client,
    ) -> error_stack::Result<ConfigBundle, error::ConfigurationError> {
        let url = format!("{}/all", endpoint_url.trim_end_matches('/'));

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

        let bundle: ConfigBundle = response.json().await.change_context(
            error::ConfigurationError::InvalidConfigurationValueError(
                "Failed to parse runtime config bundle response".into(),
            ),
        )?;

        crate::logger::info!(
            num_configs = bundle.len(),
            "Retrieved runtime config bundle"
        );

        Ok(bundle)
    }
}
