mod macros;
pub(crate) mod metrics;

use std::num::NonZeroU64;

pub use self::metrics::{
    HttpRequestMetricsLayer, init_metrics, spawn_bg_metrics_collector,
    start_prometheus_metrics_server,
};

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum MetricsConfig {
    #[default]
    Disabled,

    Otlp {
        endpoint: String,
        #[serde(default = "default_endpoint_timeout")]
        endpoint_timeout_secs: u64,
        #[serde(default = "default_export_interval")]
        metrics_export_interval_secs: u64,
        #[serde(default = "default_bg_metrics_interval")]
        background_metrics_collection_interval_secs: NonZeroU64,
    },

    Prometheus {
        #[serde(default = "default_prometheus_host")]
        host: String,
        #[serde(default = "default_prometheus_port")]
        port: u16,
        #[serde(default = "default_bg_metrics_interval")]
        background_metrics_collection_interval_secs: NonZeroU64,
    },
}

const fn default_endpoint_timeout() -> u64 {
    10
}

const fn default_export_interval() -> u64 {
    15
}

fn default_prometheus_host() -> String {
    "127.0.0.1".to_string()
}

const fn default_prometheus_port() -> u16 {
    9090
}

fn default_bg_metrics_interval() -> NonZeroU64 {
    #[expect(clippy::expect_used)]
    NonZeroU64::new(15).expect("15 is non-zero")
}

impl MetricsConfig {
    pub fn validate(&self) -> Result<(), crate::error::ConfigurationError> {
        match self {
            Self::Disabled => Ok(()),
            Self::Otlp { endpoint, .. } => {
                if endpoint.trim().is_empty() {
                    return Err(
                        crate::error::ConfigurationError::InvalidConfigurationValueError(
                            r#"metrics.endpoint is required when mode is "otlp""#.into(),
                        ),
                    );
                }
                Ok(())
            }
            Self::Prometheus { host, port, .. } => {
                if host.parse::<std::net::IpAddr>().is_err() {
                    return Err(
                        crate::error::ConfigurationError::InvalidConfigurationValueError(
                            r#"metrics.host must be a valid IP address when mode is "prometheus""#
                                .into(),
                        ),
                    );
                }
                if *port == 0 {
                    return Err(
                        crate::error::ConfigurationError::InvalidConfigurationValueError(
                            r#"metrics.port must be a non-zero value when mode is "prometheus""#
                                .into(),
                        ),
                    );
                }
                Ok(())
            }
        }
    }

    pub fn background_metrics_collection_interval_secs(&self) -> u64 {
        match self {
            // We shouldn't be reaching this arm preferably,
            // we shouldn't be launching the metrics collection task if metrics are disabled.
            Self::Disabled => default_bg_metrics_interval().get(),
            Self::Otlp {
                background_metrics_collection_interval_secs,
                ..
            }
            | Self::Prometheus {
                background_metrics_collection_interval_secs,
                ..
            } => background_metrics_collection_interval_secs.get(),
        }
    }
}

pub enum MetricsHandle {
    Disabled,
    Otlp {
        provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    },
    Prometheus {
        provider: opentelemetry_sdk::metrics::SdkMeterProvider,
        registry: prometheus::Registry,
        host: String,
        port: u16,
    },
}

impl MetricsHandle {
    pub fn provider(&self) -> Option<opentelemetry_sdk::metrics::SdkMeterProvider> {
        match self {
            Self::Disabled => None,
            Self::Otlp { provider } | Self::Prometheus { provider, .. } => Some(provider.clone()),
        }
    }
}
