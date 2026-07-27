use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};
use tokio::sync::RwLock;
use tracing::Instrument;

use crate::{config::RuntimeConfig, error};

const API_KEY_HEADER_NAME: &str = "X-Internal-Api-Key";

/// Endpoint envelope: `{"key": "...", "value": "<config json string>"}`. Only `value` is used —
/// it's a JSON string holding the flat config object (`{"enable_kv": "...", ...}`).
#[derive(serde::Deserialize)]
struct RuntimeConfigResponse {
    value: String,
}

enum RuntimeConfigState {
    Disabled,
    Enabled {
        endpoint_url: String,
        endpoint_path: String,
        api_key: Secret<String>,
        client: reqwest::Client,
        refresh_interval: Duration,
        /// Last-known-good config body; `None` until the first successful fetch.
        cache: RwLock<Option<String>>,
    },
}

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
    NotFetched,
    Available,
    Invalid,
}

/// Fetches the runtime-config endpoint on a schedule and serves the last-known-good body.
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
            RuntimeConfig::Enabled {
                endpoint,
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
                        refresh_interval: Duration::from_secs(*refresh_interval_seconds),
                        cache: RwLock::new(None),
                    },
                }
            }
        })
    }

    /// Deserialize the last-known-good config body into `T`.
    ///
    /// Returns `None` when the manager is disabled, no config has been fetched yet, or the
    /// cached payload cannot be deserialized into `T`.
    pub async fn get<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        let RuntimeConfigState::Enabled { cache, .. } = &self.state else {
            crate::logger::debug!("Runtime config disabled");
            return None;
        };

        let guard = cache.read().await;
        let Some(raw) = guard.as_deref() else {
            crate::logger::debug!("Runtime config not fetched yet");
            return None;
        };

        match serde_json::from_str::<T>(raw) {
            Ok(val) => Some(val),
            Err(error) => {
                crate::logger::error!(?error, raw, "Failed to deserialize runtime config");
                None
            }
        }
    }

    /// Returns the current cached runtime-config status without fetching from the endpoint.
    pub async fn status(&self) -> RuntimeConfigStatus {
        let RuntimeConfigState::Enabled { cache, .. } = &self.state else {
            return RuntimeConfigStatus {
                status: RuntimeConfigStatusKind::Disabled,
                config: None,
            };
        };

        let guard = cache.read().await;
        let Some(raw) = guard.as_deref() else {
            return RuntimeConfigStatus {
                status: RuntimeConfigStatusKind::NotFetched,
                config: None,
            };
        };

        match serde_json::from_str(raw) {
            Ok(config) => RuntimeConfigStatus {
                status: RuntimeConfigStatusKind::Available,
                config: Some(config),
            },
            Err(error) => {
                crate::logger::error!(?error, raw, "Cached runtime config is invalid");
                RuntimeConfigStatus {
                    status: RuntimeConfigStatusKind::Invalid,
                    config: None,
                }
            }
        }
    }

    /// Spawn a background task that refreshes the config on an interval. Returns `None` when disabled.
    pub fn spawn_prefetch_task<F, Fut>(
        self: &Arc<Self>,
        on_successful_fetch: F,
    ) -> Option<tokio::task::JoinHandle<()>>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let refresh_interval = match &self.state {
            RuntimeConfigState::Disabled => return None,
            RuntimeConfigState::Enabled {
                refresh_interval, ..
            } => *refresh_interval,
        };

        let manager = Arc::clone(self);
        crate::logger::info!(
            refresh_interval_secs = refresh_interval.as_secs(),
            "Spawning runtime config prefetch task"
        );

        Some(tokio::spawn(
            async move {
                if manager.prefetch().await {
                    on_successful_fetch().await;
                }

                let mut ticker = tokio::time::interval(refresh_interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

                loop {
                    ticker.tick().await;
                    if manager.prefetch().await {
                        on_successful_fetch().await;
                    }
                }
            }
            .in_current_span(),
        ))
    }

    async fn prefetch(&self) -> bool {
        let RuntimeConfigState::Enabled {
            endpoint_url,
            endpoint_path,
            api_key,
            client,
            cache,
            ..
        } = &self.state
        else {
            return false;
        };

        match Self::fetch_config(endpoint_url, endpoint_path, api_key, client).await {
            Ok(body) => {
                crate::logger::info!(config = %body, "Runtime config fetched");
                *cache.write().await = Some(body);
                true
            }
            Err(e) => {
                crate::logger::warn!(
                    error = ?e,
                    "Failed to prefetch runtime config, keeping last-known-good"
                );
                false
            }
        }
    }

    /// Fetch the config endpoint and return the inner config JSON string (the envelope's `value`).
    /// Validated as JSON so a malformed 2xx response can't overwrite the last-known-good entry.
    async fn fetch_config(
        endpoint_url: &str,
        endpoint_path: &str,
        api_key: &Secret<String>,
        client: &reqwest::Client,
    ) -> error_stack::Result<String, error::ConfigurationError> {
        let url = format!(
            "{}/{}",
            endpoint_url.trim_end_matches('/'),
            endpoint_path.trim_start_matches('/')
        );

        crate::logger::debug!(url = %url, "Fetching runtime config");

        let request = client.get(&url).header(API_KEY_HEADER_NAME, api_key.peek());
        let response = record_runtime_config_fetch_duration(request)
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

        let RuntimeConfigResponse { value } = response.json().await.change_context(
            error::ConfigurationError::InvalidConfigurationValueError(
                "Failed to parse runtime config response envelope".into(),
            ),
        )?;

        serde_json::from_str::<serde_json::Value>(&value).change_context(
            error::ConfigurationError::InvalidConfigurationValueError(
                "Runtime config value is not valid JSON".into(),
            ),
        )?;

        Ok(value)
    }
}

async fn record_runtime_config_fetch_duration(
    request: reqwest::RequestBuilder,
) -> Result<reqwest::Response, reqwest::Error> {
    let start = std::time::Instant::now();
    let result = request.send().await;
    let duration = start.elapsed();

    let (outcome, status_code) = match result.as_ref() {
        Ok(resp) => {
            let status = resp.status();
            let outcome = match status.as_u16() {
                200..=299 => "success",
                300..=399 => "redirect",
                400..=499 => "client_error",
                500..=599 => "server_error",
                _ => "unknown",
            };
            (outcome, Some(status.as_u16()))
        }
        Err(_) => ("transport_error", None),
    };

    let status_code = status_code
        .map(|s| s.to_string())
        .unwrap_or_else(|| "UNKNOWN".to_string());

    crate::observability::metrics::RUNTIME_CONFIG_FETCH_DURATION.record(
        duration.as_secs_f64(),
        crate::metric_attributes!(("outcome", outcome), ("status_code", status_code)),
    );

    result
}
