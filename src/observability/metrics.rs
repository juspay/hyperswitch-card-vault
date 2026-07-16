mod middleware;

use std::time::Duration;

use opentelemetry::global;
use opentelemetry_otlp::{MetricExporter, WithExportConfig};
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

pub use self::middleware::HttpRequestMetricsLayer;
use super::{MetricsConfig, MetricsHandle};
use crate::{
    counter_metric, error, gauge_metric, global_meter, histogram_metric_f64, up_down_counter_metric,
};

pub fn init_metrics(config: &MetricsConfig) -> MetricsHandle {
    match config {
        MetricsConfig::Disabled => MetricsHandle::Disabled,
        MetricsConfig::Otlp {
            endpoint,
            endpoint_timeout_secs,
            metrics_export_interval_secs,
            ..
        } => {
            let exporter = match MetricExporter::builder()
                .with_tonic()
                .with_temporality(Temporality::Cumulative)
                .with_endpoint(endpoint)
                .with_timeout(Duration::from_secs(*endpoint_timeout_secs))
                .build()
            {
                Ok(exporter) => exporter,
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        "Failed to build OTLP metric exporter, metrics disabled"
                    );
                    return MetricsHandle::Disabled;
                }
            };

            let reader = PeriodicReader::builder(exporter)
                .with_interval(Duration::from_secs(*metrics_export_interval_secs))
                .build();

            let provider = SdkMeterProvider::builder()
                .with_reader(reader)
                .with_resource(
                    opentelemetry_sdk::Resource::builder()
                        .with_service_name(env!("CARGO_PKG_NAME"))
                        .build(),
                )
                .build();

            global::set_meter_provider(provider.clone());

            MetricsHandle::Otlp { provider }
        }
        MetricsConfig::Prometheus { host, port, .. } => {
            let registry = prometheus::Registry::new();

            let exporter = match opentelemetry_prometheus::exporter()
                .with_registry(registry.clone())
                .build()
            {
                Ok(exporter) => exporter,
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        "Failed to build Prometheus metric exporter, metrics disabled"
                    );
                    return MetricsHandle::Disabled;
                }
            };

            let provider = SdkMeterProvider::builder()
                .with_reader(exporter)
                .with_resource(
                    opentelemetry_sdk::Resource::builder()
                        .with_service_name(env!("CARGO_PKG_NAME"))
                        .build(),
                )
                .build();

            global::set_meter_provider(provider.clone());

            MetricsHandle::Prometheus {
                provider,
                registry,
                host: host.clone(),
                port: *port,
            }
        }
    }
}

pub fn start_prometheus_metrics_server(
    host: &str,
    port: u16,
    registry: prometheus::Registry,
) -> Result<(), error::ConfigurationError> {
    use prometheus::Encoder;

    let addr = match host.parse::<std::net::IpAddr>() {
        Ok(ip) => std::net::SocketAddr::new(ip, port),
        Err(_) => {
            return Err(error::ConfigurationError::InvalidConfigurationValueError(
                format!(r#"metrics.host "{host}" is not a valid IP address"#),
            ));
        }
    };

    let app = axum::Router::new().route(
        "/metrics",
        axum::routing::get(move || {
            let registry = registry.clone();
            async move {
                let encoder = prometheus::TextEncoder::new();
                let mut buffer = Vec::new();

                if let Err(error) = encoder.encode(&registry.gather(), &mut buffer) {
                    tracing::warn!(?error, "Failed to encode prometheus metrics");
                }

                (
                    axum::http::StatusCode::OK,
                    String::from_utf8(buffer).unwrap_or_default(),
                )
            }
        }),
    );

    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!("Starting Prometheus metrics server at `{addr}`");

                if let Err(error) = axum::serve(listener, app).await {
                    tracing::warn!(?error, "Prometheus metrics server failed");
                }
            }
            Err(error) => {
                tracing::error!(?error, "Failed to bind prometheus metrics server");
            }
        }
    });

    Ok(())
}

pub fn spawn_bg_metrics_collector(
    global_app_state: &std::sync::Arc<crate::tenant::GlobalAppState>,
    background_metrics_collection_interval_secs: u64,
) {
    let interval = std::time::Duration::from_secs(background_metrics_collection_interval_secs);

    let global_app_state = global_app_state.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(interval);

        // Skip the first tick, which resolves immediately.
        // We want to start metrics collection after the first interval has elapsed.
        interval.tick().await;

        loop {
            interval.tick().await;

            let tenants = global_app_state.tenants_app_state.read().await;
            for (tenant_id, tenant_state) in tenants.iter() {
                tenant_state.db.collect_db_pool_state(tenant_id);

                #[cfg(feature = "caching")]
                tenant_state.db.collect_cache_entry_count(tenant_id).await;
            }
        }
    });
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

// Secret manager
#[cfg(any(feature = "kms-aws", feature = "kms-hashicorp-vault"))]
histogram_metric_f64!(
    pub(crate) SECRET_MANAGER_CALL_DURATION, CARD_VAULT_METER,
    name: "secret_manager.call.duration",
    description: "Duration of completed secret-manager call attempts",
    unit: "s",
    buckets: f64_histogram_buckets(),
);

// HTTP server
counter_metric!(
    pub(crate) HTTP_SERVER_REQUEST_COUNT, CARD_VAULT_METER,
    name: "http.server.request.count",
    description: "Number of HTTP server requests received",
    unit: "1",
);
histogram_metric_f64!(
    pub(crate) HTTP_SERVER_REQUEST_DURATION, CARD_VAULT_METER,
    name: "http.server.request.duration",
    description: "Duration of HTTP server requests",
    unit: "s",
    buckets: f64_histogram_buckets(),
);
up_down_counter_metric!(
    pub(crate) HTTP_SERVER_ACTIVE_REQUESTS, CARD_VAULT_METER,
    name: "http.server.active_requests",
    description: "Number of HTTP server requests currently in flight",
    unit: "1",
);

// JWE/JWS middleware
#[cfg(feature = "middleware")]
histogram_metric_f64!(
    pub(crate) HTTP_SERVER_JWE_MIDDLEWARE_OPERATION_DURATION, CARD_VAULT_METER,
    name: "http.server.jwe_middleware.operation.duration",
    description: "Duration of JWE/JWS middleware operations",
    unit: "s",
    buckets: f64_histogram_buckets(),
);

// Rate limiter
#[cfg(feature = "limit")]
counter_metric!(
    pub(crate) HTTP_SERVER_RATE_LIMITED_REQUEST_COUNT, CARD_VAULT_METER,
    name: "http.server.rate_limited_request.count",
    description: "Number of HTTP server requests rejected by rate limiting",
    unit: "1",
);

// Health check
histogram_metric_f64!(
    pub(crate) HEALTH_CHECK_DURATION, CARD_VAULT_METER,
    name: "health.check.duration",
    description: "Duration of completed health diagnostic checks",
    unit: "s",
    buckets: f64_histogram_buckets(),
);

// Database
counter_metric!(
    pub(crate) DATABASE_QUERY_COUNT, CARD_VAULT_METER,
    name: "database.query.count",
    description: "Number of database query attempts",
    unit: "1",
);
histogram_metric_f64!(
    pub(crate) DATABASE_QUERY_DURATION, CARD_VAULT_METER,
    name: "database.query.duration",
    description: "Duration of completed database queries",
    unit: "s",
    buckets: f64_histogram_buckets(),
);
histogram_metric_f64!(
    pub(crate) DATABASE_CONNECTION_ACQUIRE_DURATION, CARD_VAULT_METER,
    name: "database.connection.acquire.duration",
    description: "Duration of database connection acquisition attempts",
    unit: "s",
    buckets: f64_histogram_buckets(),
);
gauge_metric!(
    pub(crate) DATABASE_POOL_SIZE, CARD_VAULT_METER,
    name: "database.pool.size",
    description: "Total number of connections in the database pool",
    unit: "1",
);
gauge_metric!(
    pub(crate) DATABASE_POOL_AVAILABLE, CARD_VAULT_METER,
    name: "database.pool.available",
    description: "Number of available connections in the database pool",
    unit: "1",
);
gauge_metric!(
    pub(crate) DATABASE_POOL_WAITING, CARD_VAULT_METER,
    name: "database.pool.waiting",
    description: "Number of callers waiting for a database connection",
    unit: "1",
);

// External HTTP client
counter_metric!(
    pub(crate) EXTERNAL_HTTP_REQUEST_COUNT, CARD_VAULT_METER,
    name: "external_http.request.count",
    description: "Number of external HTTP request attempts",
    unit: "1",
);
histogram_metric_f64!(
    pub(crate) EXTERNAL_HTTP_REQUEST_DURATION, CARD_VAULT_METER,
    name: "external_http.request.duration",
    description: "Duration of completed external HTTP requests",
    unit: "s",
    buckets: f64_histogram_buckets(),
);

// Cache
#[cfg(feature = "caching")]
counter_metric!(
    pub(crate) CACHE_LOOKUP_COUNT, CARD_VAULT_METER,
    name: "cache.lookup.count",
    description: "Number of cache lookup attempts",
    unit: "1",
);
#[cfg(feature = "caching")]
counter_metric!(
    pub(crate) CACHE_INSERT_COUNT, CARD_VAULT_METER,
    name: "cache.insert.count",
    description: "Number of cache insert attempts",
    unit: "1",
);
#[cfg(feature = "caching")]
counter_metric!(
    pub(crate) CACHE_REMOVAL_COUNT, CARD_VAULT_METER,
    name: "cache.removal.count",
    description: "Number of cache removal events",
    unit: "1",
);
#[cfg(feature = "caching")]
gauge_metric!(
    pub(crate) CACHE_ENTRY_COUNT, CARD_VAULT_METER,
    name: "cache.entry.count",
    description: "Current number of cache entries",
    unit: "1",
);

// TTL-based cleanup
counter_metric!(
    pub(crate) TTL_EXPIRED_DATA_ENCOUNTERED_COUNT, CARD_VAULT_METER,
    name: "ttl.expired_data_encountered.count",
    description: "Number of requests that encountered data with expired TTL",
    unit: "1",
);
counter_metric!(
    pub(crate) TTL_DELETION_COUNT, CARD_VAULT_METER,
    name: "ttl.deletion.count",
    description: "Number of background TTL-based deletions",
    unit: "1",
);

// Domain
counter_metric!(
    pub(crate) DOMAIN_GET_OR_INSERT_COUNT, CARD_VAULT_METER,
    name: "domain.get_or_insert.count",
    description: "Number of domain get-or-insert workflow outcomes",
    unit: "1",
);

// Runtime config
histogram_metric_f64!(
    pub(crate) RUNTIME_CONFIG_FETCH_DURATION, CARD_VAULT_METER,
    name: "runtime_config.fetch.duration",
    description: "Duration of completed runtime config fetch attempts",
    unit: "s",
    buckets: f64_histogram_buckets(),
);

// KV
#[cfg(feature = "kv")]
counter_metric!(
    pub(crate) KV_OPERATION_COUNT, CARD_VAULT_METER,
    name: "kv.operation.count",
    description: "Number of KV operation attempts",
    unit: "1",
);
#[cfg(feature = "kv")]
histogram_metric_f64!(
    pub(crate) KV_OPERATION_DURATION, CARD_VAULT_METER,
    name: "kv.operation.duration",
    description: "Duration of completed KV operations",
    unit: "s",
    buckets: f64_histogram_buckets(),
);
#[cfg(feature = "kv")]
counter_metric!(
    pub(crate) KV_DRAINER_PUSH_COUNT, CARD_VAULT_METER,
    name: "kv.drainer.push.count",
    description: "Number of drainer stream push attempts",
    unit: "1",
);
#[cfg(feature = "kv")]
histogram_metric_f64!(
    pub(crate) KV_DRAINER_PUSH_DURATION, CARD_VAULT_METER,
    name: "kv.drainer.push.duration",
    description: "Duration of completed drainer stream push attempts",
    unit: "s",
    buckets: f64_histogram_buckets(),
);
#[cfg(feature = "kv")]
counter_metric!(
    pub(crate) KV_CACHE_MISS_COUNT, CARD_VAULT_METER,
    name: "kv.cache_miss.count",
    description: "Redis cache misses that fell back to Postgres",
    unit: "1",
);

#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum Resource {
    #[cfg(feature = "external_key_manager")]
    Entity,
    Fingerprint,
    HashTable,
    Locker,
    Merchant,
    Vault,
}

#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum DomainGetOrInsertOutcome {
    FoundExisting,
    FoundExistingAfterDuplicateInsert,
    Created,
    Updated,
    Error,
}

#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum TtlDeletionOutcome {
    Deleted,
    Failed,
}

crate::impl_metric_value_from!(Resource, DomainGetOrInsertOutcome, TtlDeletionOutcome);
