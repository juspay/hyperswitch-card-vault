use std::{sync::Arc, {collections::HashMap, time::Duration}};

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

/// Response: `{"key": "...", "value": "[{\"key\":\"...\",\"value\":\"...\"}]"}` —
/// `value` is a JSON-string containing the array of config items.
#[cfg(feature = "caching")]
#[derive(Debug, serde::Deserialize)]
struct RuntimeConfigResponse {
    key: String,
    value: String,
}

#[derive(Clone)]
enum RuntimeConfigState {
    Disabled,
    #[cfg_attr(not(feature = "caching"), allow(dead_code))]
    Enabled {
        endpoint_url: String,
        endpoint_path: String,
        api_key: Secret<String>,
        client: reqwest::Client,
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
    fn build_header_map(
        headers: &HashMap<String, Secret<String>>,
    ) -> error_stack::Result<reqwest::header::HeaderMap, error::ConfigurationError> {
        headers
            .iter()
            .map(|(name, value)| {
                let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                    .change_context(error::ConfigurationError::InvalidConfigurationValueError(
                        format!("invalid runtime_config header name `{name}`"),
                    ))?;
                let mut header_value = reqwest::header::HeaderValue::from_str(value.peek())
                    .change_context(error::ConfigurationError::InvalidConfigurationValueError(
                        format!("invalid runtime_config header value for `{name}`"),
                    ))?;
                header_value.set_sensitive(true);
                Ok((header_name, header_value))
            })
            .collect()
    }

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
                ttl_seconds: _,
                cache_max_capacity,
                refresh_interval_seconds,
            } => {
                let client = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .pool_idle_timeout(Duration::from_secs(client_idle_timeout))
                    .pool_max_idle_per_host(pool_max_idle_per_host)
                    .default_headers(Self::build_header_map(&endpoint.headers)?)
                    .build()
                    .change_context(error::ConfigurationError::InvalidConfigurationValueError(
                        "Failed to build HTTP client for runtime config endpoint".into(),
                    ))?;

                Self {
                    state: RuntimeConfigState::Enabled {
                        endpoint_url: endpoint.base_url.clone(),
                        endpoint_path: endpoint.path.clone(),
                        api_key: endpoint.api_key.clone(),
                        client,
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

    /// Cache-only read. Miss returns `None` (caller falls back to TOML defaults).
    pub async fn get_config<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        #[cfg(feature = "caching")]
        {
            let RuntimeConfigState::Enabled { cache, .. } = &self.state else {
                return None;
            };

            if let Some(val) = cache.get(key).await {
                crate::logger::debug!(key, "Runtime config cache hit");
                return Self::deserialize_config(key, &val);
            }
            crate::logger::debug!(key, "Runtime config cache miss");
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
            manager.prefetch().await;

            let mut ticker = tokio::time::interval(refresh_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;
                manager.prefetch().await;
            }
        }))
    }

    #[cfg(feature = "caching")]
    async fn prefetch(&self) {
        let RuntimeConfigState::Enabled {
            endpoint_url,
            endpoint_path,
            api_key,
            client,
            cache,
            ..
        } = &self.state
        else {
            return;
        };

        match Self::fetch_all(endpoint_url, endpoint_path, api_key, client).await {
            Ok(items) => {
                for item in &items {
                    crate::logger::debug!(
                        key = %item.key,
                        value = %item.value,
                        "Runtime config cache entry upserted"
                    );
                    cache.insert(item.key.clone(), item.value.clone()).await;
                }
                crate::logger::info!(
                    fetched_count = items.len(),
                    cache_entry_count = cache.entry_count(),
                    "Runtime config cache updated"
                );
            }
            Err(e) => {
                crate::logger::warn!(
                    error = ?e,
                    cache_entry_count = cache.entry_count(),
                    "Failed to prefetch runtime config bundle, keeping last-known-good"
                );
            }
        }
    }

    #[cfg(feature = "caching")]
    async fn fetch_all(
        endpoint_url: &str,
        endpoint_path: &str,
        api_key: &Secret<String>,
        client: &reqwest::Client,
    ) -> error_stack::Result<Vec<RuntimeConfigResponse>, error::ConfigurationError> {
        let url = format!(
            "{}/{}",
            endpoint_url.trim_end_matches('/'),
            endpoint_path.trim_start_matches('/')
        );

        crate::logger::debug!(url = %url, "Fetching runtime config bundle");

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

        // Outer: {"key": "...", "value": "[...]"}
        let outer: RuntimeConfigResponse = response.json().await.change_context(
            error::ConfigurationError::InvalidConfigurationValueError(
                "Failed to parse runtime config response".into(),
            ),
        )?;

        // Inner: `value` is a JSON string containing an array of config items.
        serde_json::from_str::<Vec<RuntimeConfigResponse>>(&outer.value).change_context(
            error::ConfigurationError::InvalidConfigurationValueError(
                "Failed to parse runtime config bundle from response value".into(),
            ),
        )
    }
}

#[cfg(not(feature = "caching"))]
impl RuntimeConfigManager {
    pub fn spawn_prefetch_task(self: &Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        None
    }
}
