//!
//! Logger-specific config.
//!

use serde::Deserialize;

/// Log config settings.
#[derive(Debug, Deserialize, Clone)]
pub struct Log {
    /// Logging to a console.
    pub console: LogConsole,
    /// Telemetry / tracing.
    pub telemetry: LogTelemetry,
}

/// Logging to a console.
#[derive(Debug, Deserialize, Clone)]
pub struct LogConsole {
    /// Whether you want to see log in your terminal.
    pub enabled: bool,
    /// What you see in your terminal.
    pub level: Level,
    /// Log format
    pub log_format: LogFormat,
    /// Directive which sets the log level for one or more crates/modules.
    pub filtering_directive: Option<String>,
}

/// Telemetry / tracing.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default)]
pub struct LogTelemetry {
    /// Whether the traces pipeline is enabled.
    pub traces_enabled: bool,
    /// Whether errors in setting up traces or metrics pipelines must be ignored.
    pub ignore_errors: bool,
    /// Sampling rate for traces
    pub sampling_rate: Option<f64>,
    /// Base endpoint URL to send metrics and traces to. Can optionally include the port number.
    pub otel_exporter_otlp_endpoint: Option<String>,
    /// Timeout (in milliseconds) for sending metrics and traces.
    pub otel_exporter_otlp_timeout: Option<u64>,
    /// Whether to use xray ID generator, (enable this if you plan to use AWS-XRAY)
    pub use_xray_generator: bool,
    /// Route Based Tracing
    pub route_to_trace: Option<Vec<String>>,
}

/// Describes the level of verbosity of a span or event.
#[derive(Debug, Clone, Copy)]
pub struct Level(pub(super) tracing::Level);

impl Level {
    /// Returns the most verbose [`tracing::Level`]
    pub fn into_level(&self) -> tracing::Level {
        self.0
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use std::str::FromStr as _;

        let s = String::deserialize(deserializer)?;
        tracing::Level::from_str(&s)
            .map(Level)
            .map_err(serde::de::Error::custom)
    }
}

/// Telemetry / tracing.
#[derive(Default, Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Default pretty log format
    Default,
    /// JSON based structured logging
    #[default]
    Json,
}
