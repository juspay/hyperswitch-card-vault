use std::{sync::Arc, time::Duration};

#[cfg(feature = "middleware")]
use axum::middleware;
use axum::{extract::Request, routing::post};
use axum_server::tls_rustls::RustlsConfig;
use error_stack::ResultExt;
use tower::ServiceBuilder;
use tower_http::{
    ServiceBuilderExt,
    request_id::{MakeRequestId, RequestId},
    trace as tower_trace,
};

#[cfg(feature = "middleware")]
use crate::middleware as custom_middleware;
#[cfg(feature = "caching")]
use crate::storage::caching::Caching;
use crate::{
    api_client::ApiClient,
    config::{self, GlobalConfig, TenantConfig},
    error, logger, observability,
    routes::{self, routes_v2},
    storage,
    tenant::GlobalAppState,
    utils,
};

#[cfg(feature = "caching")]
type Storage = Caching<storage::Storage>;

#[cfg(not(feature = "caching"))]
type Storage = storage::Storage;

///
/// TenantAppState:
///
///
/// The tenant specific appstate that is passed to main storage endpoints
///
#[derive(Clone)]
pub struct TenantAppState {
    pub db: Storage,
    pub config: config::TenantConfig,
    pub api_client: ApiClient,
    #[cfg(feature = "redis")]
    pub redis: Option<storage::redis::RedisStore>,
}

#[allow(clippy::expect_used)]
impl TenantAppState {
    ///
    /// Construct new app state with configuration
    ///
    pub async fn new(
        global_config: &GlobalConfig,
        tenant_config: TenantConfig,
        api_client: ApiClient,
        #[cfg(feature = "redis")] shared_redis: Option<&storage::redis::RedisStore>,
        runtime_config_manager: Arc<crate::runtime_config::RuntimeConfigManager>,
        #[cfg(feature = "kv")] kv_store: Arc<storage::KvGlobalStore>,
    ) -> error_stack::Result<Self, error::ConfigurationError> {
        #[cfg(feature = "redis")]
        let tenant_redis = shared_redis
            .map(|store| store.clone_with_prefix(tenant_config.redis_key_prefix.trim()));

        #[allow(clippy::map_identity)]
        let db = storage::Storage::new(
            &global_config.database,
            global_config.read_replica.as_ref(),
            &tenant_config.tenant_secrets.schema,
            runtime_config_manager,
            #[cfg(feature = "kv")]
            tenant_redis.clone(),
            #[cfg(feature = "kv")]
            kv_store,
        )
        .await
        .map(
            #[cfg(feature = "caching")]
            Caching::implement_cache(&global_config.cache),
            #[cfg(not(feature = "caching"))]
            std::convert::identity,
        )
        .change_context(error::ConfigurationError::DatabaseError)?;

        Ok(Self {
            db,
            api_client,
            #[cfg(feature = "redis")]
            redis: tenant_redis,
            config: tenant_config,
        })
    }
}

/// Temporary State to store keys
#[cfg(feature = "key_custodian")]
#[derive(Default, Debug)]
pub struct CustodianKeys {
    pub key1: Option<String>,
    pub key2: Option<String>,
}

#[cfg(feature = "vergen")]
fn default_headers() -> tower_http::set_header::SetResponseHeaderLayer<axum::http::HeaderValue> {
    tower_http::set_header::SetResponseHeaderLayer::overriding(
        axum::http::HeaderName::from_static("x-version"),
        axum::http::HeaderValue::from_static(build_info::git_describe!()),
    )
}

#[derive(Clone, Copy)]
struct MakeUuidV7;

impl MakeRequestId for MakeUuidV7 {
    fn make_request_id<B>(&mut self, _request: &axum::http::Request<B>) -> Option<RequestId> {
        let uuid = uuid::Uuid::now_v7();
        axum::http::HeaderValue::from_str(&uuid.to_string())
            .ok()
            .map(RequestId::new)
    }
}

#[allow(clippy::expect_used)]
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Received shutdown signal, starting graceful shutdown");
}

///
/// The server responsible for the custodian APIs and main locker APIs this will perform all storage, retrieval and
/// deletion operation
///
pub async fn server_builder(
    global_app_state: Arc<GlobalAppState>,
    metrics_handle: observability::MetricsHandle,
) -> Result<(), error::ConfigurationError> {
    // Warm + periodically refresh the runtime-config cache. No-op when disabled.
    let runtime_config_manager = global_app_state.runtime_config_manager.clone();
    let state_for_prefetch = global_app_state.clone();
    let _prefetch_handle = runtime_config_manager.spawn_prefetch_task(move || {
        let state_for_prefetch = state_for_prefetch.clone();
        async move {
            state_for_prefetch.apply_runtime_config_updates().await;
        }
    });

    let socket_addr = std::net::SocketAddr::new(
        global_app_state.global_config.server.host.parse()?,
        global_app_state.global_config.server.port,
    );
    let router = axum::Router::new()
        .nest(
            "/data",
            routes::data::serve(
                #[cfg(feature = "limit")]
                global_app_state.clone(),
            ),
        )
        .nest(
            "/cards",
            routes::data::serve(
                #[cfg(feature = "limit")]
                global_app_state.clone(),
            ),
        );

    // v2 routes
    #[cfg_attr(
        all(
            not(feature = "middleware"),
            not(feature = "external_key_manager"),
            not(feature = "key_custodian")
        ),
        allow(unused_mut)
    )]
    let mut router = router
        .nest(
            "/api/v2/vault",
            axum::Router::new()
                .route("/delete", post(routes_v2::data::delete_data))
                .route("/add", post(routes_v2::data::add_data))
                .route("/retrieve", post(routes_v2::data::retrieve_data))
                .route(
                    "/fingerprint",
                    post(routes::data::get_or_insert_fingerprint),
                ),
        )
        // Explicit provisioning endpoint. Config decides the backing table: `merchant` under the
        // internal key manager, `entity` under the external key manager.
        .route("/entity", post(routes::entity::create_entity));

    #[cfg(feature = "middleware")]
    {
        router = router.layer(middleware::from_fn_with_state(
            global_app_state.clone(),
            custom_middleware::middleware,
        ));
    }

    #[cfg(feature = "external_key_manager")]
    {
        if global_app_state
            .global_config
            .external_key_manager
            .is_external()
        {
            router = router.route("/key/transfer", post(routes::key_migration::transfer_keys));
        }
    }

    #[cfg(feature = "key_custodian")]
    {
        router = router.nest("/custodian", routes::key_custodian::serve());
    }

    router = router.nest("/health", routes::health::serve());

    if metrics_handle.provider().is_some() {
        router = router.layer(observability::HttpRequestMetricsLayer);
    }

    if let observability::MetricsHandle::Prometheus {
        registry,
        host,
        port,
        ..
    } = &metrics_handle
    {
        observability::start_prometheus_metrics_server(host, *port, registry.clone())?;
    }

    if metrics_handle.provider().is_some() {
        observability::spawn_bg_metrics_collector(
            &global_app_state,
            global_app_state
                .global_config
                .metrics
                .background_metrics_collection_interval_secs(),
        );
    }

    router = router.layer(
        tower_trace::TraceLayer::new_for_http()
            .make_span_with(|request: &Request<_>| utils::record_fields_from_header(request))
            .on_request(tower_trace::DefaultOnRequest::new().level(tracing::Level::INFO))
            .on_response(
                tower_trace::DefaultOnResponse::new()
                    .level(tracing::Level::INFO)
                    .latency_unit(tower_http::LatencyUnit::Micros),
            )
            .on_failure(
                tower_trace::DefaultOnFailure::new()
                    .latency_unit(tower_http::LatencyUnit::Micros)
                    .level(tracing::Level::ERROR),
            ),
    );

    router = router.layer(
        ServiceBuilder::new()
            .set_x_request_id(MakeUuidV7)
            .propagate_x_request_id(),
    );

    // Register default headers layer last so it wraps all routes, ensuring x-version is present on all responses.
    #[cfg(feature = "vergen")]
    {
        router = router.layer(default_headers());
    }

    let router = router.with_state(global_app_state.clone());

    logger::info!(
        "Locker started [{:?}] [{:?}]",
        global_app_state.global_config.server,
        global_app_state.global_config.log
    );

    logger::debug!(startup_config=?global_app_state.global_config);

    if let Some(tls_config) = &global_app_state.global_config.tls {
        let tcp_listener = std::net::TcpListener::bind(socket_addr)?;
        let rusttls_config =
            RustlsConfig::from_pem_file(&tls_config.certificate, &tls_config.private_key).await?;

        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(30)));
        });

        axum_server::from_tcp_rustls(tcp_listener, rusttls_config)
            .handle(handle)
            .serve(router.into_make_service())
            .await?;
    } else {
        let tcp_listener = tokio::net::TcpListener::bind(socket_addr).await?;

        axum::serve(tcp_listener, router.into_make_service())
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    }

    Ok(())
}
