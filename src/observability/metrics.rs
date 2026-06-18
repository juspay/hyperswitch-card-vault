mod middleware;

use std::time::Duration;

use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{MetricExporter, WithExportConfig};
use opentelemetry_sdk::{
    Resource,
    metrics::{PeriodicReader, SdkMeterProvider, Temporality},
    runtime,
};

pub use self::middleware::HttpRequestMetricsLayer;
use super::TelemetryConfig;
use crate::{counter_metric, global_meter, histogram_metric_f64};

pub fn init_metrics_provider(config: &TelemetryConfig) -> Option<SdkMeterProvider> {
    if !config.metrics_enabled {
        return None;
    }

    let exporter = match MetricExporter::builder()
        .with_tonic()
        .with_temporality(Temporality::Cumulative)
        .with_endpoint(&config.endpoint)
        .with_timeout(Duration::from_secs(config.endpoint_timeout_secs))
        .build()
    {
        Ok(exporter) => exporter,
        Err(error) => {
            tracing::warn!(
                ?error,
                "Failed to build OTLP metric exporter, metrics disabled"
            );
            return None;
        }
    };

    let reader = PeriodicReader::builder(exporter, runtime::Tokio)
        .with_interval(Duration::from_secs(config.metrics_export_interval_secs))
        .with_timeout(Duration::from_secs(config.endpoint_timeout_secs))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(Resource::new([KeyValue::new(
            "service.name",
            "hyperswitch_card_vault",
        )]))
        .build();

    global::set_meter_provider(provider.clone());

    Some(provider)
}

pub(crate) fn f64_histogram_buckets() -> Vec<f64> {
    let mut init = 0.000_001;
    let mut buckets: [f64; 30] = [0.0; 30];

    for bucket in &mut buckets {
        *bucket = init;
        init *= 2.0;
    }

    Vec::from(buckets)
}

global_meter!(pub(crate) CARD_VAULT_METER, "card_vault");
counter_metric!(
    pub(crate) REQUEST_COUNT, CARD_VAULT_METER,
    name: "http.server.request.count",
    description: "Number of HTTP server requests received",
    unit: "1",
);
histogram_metric_f64!(
    pub(crate) REQUEST_DURATION, CARD_VAULT_METER,
    name: "http.server.request.duration",
    description: "Duration of HTTP server requests",
    unit: "s",
    buckets: f64_histogram_buckets(),
);
