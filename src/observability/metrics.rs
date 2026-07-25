mod middleware;

use std::time::Duration;

use metrics_utils::{
    counter_metric, f64_histogram_buckets, gauge_metric, global_meter, histogram_metric_f64,
    up_down_counter_metric,
};

pub use self::middleware::HttpRequestMetricsLayer;
use super::{MetricsConfig, MetricsHandle};
use crate::error;

pub fn init_metrics(config: &MetricsConfig) -> MetricsHandle {
    match config {
        MetricsConfig::Disabled => MetricsHandle::Disabled,

        MetricsConfig::Otlp {
            endpoint,
            endpoint_timeout_secs,
            metrics_export_interval_secs,
            ..
        } => {
            let metrics_config = metrics_utils::MetricsConfig {
                service_name: String::from(env!("CARGO_PKG_NAME")),
                resource_attributes: Vec::new(),
                otlp_config: Some(metrics_utils::OtlpConfig {
                    endpoint: endpoint.clone(),
                    endpoint_timeout: Some(Duration::from_secs(*endpoint_timeout_secs)),
                    metrics_export_interval: Some(Duration::from_secs(
                        *metrics_export_interval_secs,
                    )),
                    compression: Some(metrics_utils::OtlpCompression::Zstd),
                    temporality: Some(metrics_utils::Temporality::Cumulative),
                }),
                enable_prometheus: false,
            };

            match metrics_utils::init_metrics(&metrics_config) {
                Ok(inner) => {
                    inner.register_as_global();
                    MetricsHandle::Otlp { inner }
                }
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        "Failed to initialize metrics pipeline; metrics disabled"
                    );
                    MetricsHandle::Disabled
                }
            }
        }

        MetricsConfig::Prometheus { host, port, .. } => {
            let metrics_config = metrics_utils::MetricsConfig {
                service_name: String::from(env!("CARGO_PKG_NAME")),
                resource_attributes: Vec::new(),
                otlp_config: None,
                enable_prometheus: true,
            };

            match metrics_utils::init_metrics(&metrics_config) {
                Ok(inner) => {
                    inner.register_as_global();
                    MetricsHandle::Prometheus {
                        inner,
                        host: host.clone(),
                        port: *port,
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        "Failed to initialize metrics pipeline; metrics disabled"
                    );
                    MetricsHandle::Disabled
                }
            }
        }
    }
}

pub fn start_prometheus_metrics_server(
    host: &str,
    port: u16,
    registry: metrics_utils::prometheus::Registry,
) -> Result<(), error::ConfigurationError> {
    use metrics_utils::prometheus::Encoder;

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
                let encoder = metrics_utils::prometheus::TextEncoder::new();
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

            let tenants: Vec<_> = {
                let guard = global_app_state.tenants_app_state.read().await;
                guard
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<Vec<_>>()
            };
            for (tenant_id, tenant_state) in tenants.iter() {
                tenant_state.db.collect_db_pool_state(tenant_id);

                #[cfg(feature = "caching")]
                tenant_state.db.collect_cache_entry_count(tenant_id).await;
            }
        }
    });
}

global_meter!(pub(crate) CARD_VAULT_METER, "card_vault");

// Secret manager
#[cfg(any(feature = "kms-aws", feature = "kms-hashicorp-vault"))]
histogram_metric_f64!(
    pub(crate) SECRET_MANAGER_CALL_DURATION, CARD_VAULT_METER,
    name: "secret_manager.call.duration",
    description: "Duration of completed secret-manager call attempts",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);

// HTTP server
counter_metric!(
    pub(crate) HTTP_SERVER_REQUEST_COUNT, CARD_VAULT_METER,
    name: "http.server.request.count",
    description: "Number of HTTP server requests received",
);
histogram_metric_f64!(
    pub(crate) HTTP_SERVER_REQUEST_DURATION, CARD_VAULT_METER,
    name: "http.server.request.duration",
    description: "Duration of HTTP server requests",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);
up_down_counter_metric!(
    pub(crate) HTTP_SERVER_ACTIVE_REQUESTS, CARD_VAULT_METER,
    name: "http.server.active_requests",
    description: "Number of HTTP server requests currently in flight",
);

// JWE/JWS middleware
#[cfg(feature = "middleware")]
histogram_metric_f64!(
    pub(crate) HTTP_SERVER_JWE_MIDDLEWARE_OPERATION_DURATION, CARD_VAULT_METER,
    name: "http.server.jwe_middleware.operation.duration",
    description: "Duration of JWE/JWS middleware operations",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);

// Rate limiter
#[cfg(feature = "limit")]
counter_metric!(
    pub(crate) HTTP_SERVER_RATE_LIMITED_REQUEST_COUNT, CARD_VAULT_METER,
    name: "http.server.rate_limited_request.count",
    description: "Number of HTTP server requests rejected by rate limiting",
);

// Health check
histogram_metric_f64!(
    pub(crate) HEALTH_CHECK_DURATION, CARD_VAULT_METER,
    name: "health.check.duration",
    description: "Duration of completed health diagnostic checks",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);

// Database
counter_metric!(
    pub(crate) DATABASE_QUERY_COUNT, CARD_VAULT_METER,
    name: "database.query.count",
    description: "Number of database query attempts",
);
histogram_metric_f64!(
    pub(crate) DATABASE_QUERY_DURATION, CARD_VAULT_METER,
    name: "database.query.duration",
    description: "Duration of completed database queries",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);
histogram_metric_f64!(
    pub(crate) DATABASE_CONNECTION_ACQUIRE_DURATION, CARD_VAULT_METER,
    name: "database.connection.acquire.duration",
    description: "Duration of database connection acquisition attempts",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);
gauge_metric!(
    pub(crate) DATABASE_POOL_SIZE, CARD_VAULT_METER,
    name: "database.pool.size",
    description: "Total number of connections in the database pool",
);
gauge_metric!(
    pub(crate) DATABASE_POOL_AVAILABLE, CARD_VAULT_METER,
    name: "database.pool.available",
    description: "Number of available connections in the database pool",
);
gauge_metric!(
    pub(crate) DATABASE_POOL_WAITING, CARD_VAULT_METER,
    name: "database.pool.waiting",
    description: "Number of callers waiting for a database connection",
);

// External HTTP client
counter_metric!(
    pub(crate) EXTERNAL_HTTP_REQUEST_COUNT, CARD_VAULT_METER,
    name: "external_http.request.count",
    description: "Number of external HTTP request attempts",
);
histogram_metric_f64!(
    pub(crate) EXTERNAL_HTTP_REQUEST_DURATION, CARD_VAULT_METER,
    name: "external_http.request.duration",
    description: "Duration of completed external HTTP requests",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);

// Cache
#[cfg(feature = "caching")]
counter_metric!(
    pub(crate) CACHE_LOOKUP_COUNT, CARD_VAULT_METER,
    name: "cache.lookup.count",
    description: "Number of cache lookup attempts",
);
#[cfg(feature = "caching")]
counter_metric!(
    pub(crate) CACHE_INSERT_COUNT, CARD_VAULT_METER,
    name: "cache.insert.count",
    description: "Number of cache insert attempts",
);
#[cfg(feature = "caching")]
counter_metric!(
    pub(crate) CACHE_REMOVAL_COUNT, CARD_VAULT_METER,
    name: "cache.removal.count",
    description: "Number of cache removal events",
);
#[cfg(feature = "caching")]
gauge_metric!(
    pub(crate) CACHE_ENTRY_COUNT, CARD_VAULT_METER,
    name: "cache.entry.count",
    description: "Current number of cache entries",
);

// TTL-based cleanup
counter_metric!(
    pub(crate) TTL_EXPIRED_DATA_ENCOUNTERED_COUNT, CARD_VAULT_METER,
    name: "ttl.expired_data_encountered.count",
    description: "Number of requests that encountered data with expired TTL",
);
counter_metric!(
    pub(crate) TTL_DELETION_COUNT, CARD_VAULT_METER,
    name: "ttl.deletion.count",
    description: "Number of background TTL-based deletions",
);

// Domain
counter_metric!(
    pub(crate) DOMAIN_GET_OR_INSERT_COUNT, CARD_VAULT_METER,
    name: "domain.get_or_insert.count",
    description: "Number of domain get-or-insert workflow outcomes",
);

// Entity provisioning
counter_metric!(
    pub(crate) ENTITY_IMPLICIT_CREATE_COUNT, CARD_VAULT_METER,
    name: "entity.implicit_create.count",
    description: "Number of key-holder records auto-created during the add flow (deprecated lazy provisioning)",
);

// Runtime config
histogram_metric_f64!(
    pub(crate) RUNTIME_CONFIG_FETCH_DURATION, CARD_VAULT_METER,
    name: "runtime_config.fetch.duration",
    description: "Duration of completed runtime config fetch attempts",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);

// KV
#[cfg(feature = "kv")]
counter_metric!(
    pub(crate) KV_OPERATION_COUNT, CARD_VAULT_METER,
    name: "kv.operation.count",
    description: "Number of KV operation attempts",
);
#[cfg(feature = "kv")]
histogram_metric_f64!(
    pub(crate) KV_OPERATION_DURATION, CARD_VAULT_METER,
    name: "kv.operation.duration",
    description: "Duration of completed KV operations",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);
#[cfg(feature = "kv")]
counter_metric!(
    pub(crate) KV_DRAINER_PUSH_COUNT, CARD_VAULT_METER,
    name: "kv.drainer.push.count",
    description: "Number of drainer stream push attempts",
);
#[cfg(feature = "kv")]
histogram_metric_f64!(
    pub(crate) KV_DRAINER_PUSH_DURATION, CARD_VAULT_METER,
    name: "kv.drainer.push.duration",
    description: "Duration of completed drainer stream push attempts",
    unit: "s",
    buckets: f64_histogram_buckets().to_vec(),
);
#[cfg(feature = "kv")]
counter_metric!(
    pub(crate) KV_CACHE_MISS_COUNT, CARD_VAULT_METER,
    name: "kv.cache_miss.count",
    description: "Redis cache misses that fell back to Postgres",
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

/// Which key manager backed an operation, used as a metric attribute.
#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum KeyManagerKind {
    Internal,
    #[cfg(feature = "external_key_manager")]
    External,
}

#[macro_export]
macro_rules! impl_metric_value_from {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl From<$ty> for metrics_utils::opentelemetry::Value {
                fn from(v: $ty) -> Self {
                    Self::from(<&'static str>::from(v))
                }
            }
        )+
    };
}

impl_metric_value_from!(
    Resource,
    DomainGetOrInsertOutcome,
    TtlDeletionOutcome,
    KeyManagerKind
);
