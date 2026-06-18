mod macros;
mod metrics;

pub use self::metrics::{HttpRequestMetricsLayer, init_metrics_provider};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub metrics_enabled: bool,
    pub endpoint: String,
    pub endpoint_timeout_secs: u64,
    pub metrics_export_interval_secs: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: false,
            endpoint: "http://localhost:4317".to_string(),
            endpoint_timeout_secs: 10,
            metrics_export_interval_secs: 15,
        }
    }
}
