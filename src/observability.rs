mod macros;
pub(crate) mod metrics;

pub use self::metrics::{HttpRequestMetricsLayer, init_metrics, start_prometheus_metrics_server};

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
    },

    Prometheus {
        #[serde(default = "default_prometheus_host")]
        host: String,
        #[serde(default = "default_prometheus_port")]
        port: u16,
    },
}

fn default_endpoint_timeout() -> u64 {
    10
}

fn default_export_interval() -> u64 {
    15
}

fn default_prometheus_host() -> String {
    "127.0.0.1".to_string()
}

fn default_prometheus_port() -> u16 {
    9090
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
            Self::Prometheus { host, port } => {
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
