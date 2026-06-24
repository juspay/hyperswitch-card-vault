use std::time::Duration;

use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};

use crate::{config::RuntimeConfig, error};

const API_KEY_HEADER_NAME: &str = "X-Internal-Api-Key";

/// Response format from the runtime config endpoint:
/// ```json
/// {"key": "runtime_config", "value": "{\"use_read_replica\":true}"}
/// ```
#[derive(Debug, serde::Deserialize)]
struct RuntimeConfigResponse {
    #[expect(dead_code)]
    key: String,
    value: String,
}

#[derive(Clone)]
enum RuntimeConfigState {
    Disabled,
    Enabled {
        endpoint_url: String,
        api_key: Secret<String>,
        client: reqwest::Client,
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
            RuntimeConfigState::Disabled => return None,
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
                return Self::deserialize_config(key, &val);
            }
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
            cache.insert(key.to_string(), raw.clone()).await;
        }

        Self::deserialize_config(key, &raw)
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
