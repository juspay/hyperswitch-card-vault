use axum::{extract::Request, routing::post};
use axum_server::tls_rustls::RustlsConfig;
use error_stack::ResultExt;
use tower_http::trace as tower_trace;

#[cfg(feature = "middleware")]
use crate::middleware as custom_middleware;

#[cfg(feature = "middleware")]
use axum::middleware;

use std::sync::Arc;

use crate::{
    api_client::ApiClient,
    config::{self, GlobalConfig, TenantConfig},
    error, logger,
    routes::{self, routes_v2},
    storage,
    tenant::GlobalAppState,
    utils,
};

#[cfg(feature = "caching")]
use crate::storage::caching::Caching;

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
    ) -> error_stack::Result<Self, error::ConfigurationError> {
        #[allow(clippy::map_identity)]
        let db = storage::Storage::new(
            &global_config.database,
            &tenant_config.tenant_secrets.schema,
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

///
/// The server responsible for the custodian APIs and main locker APIs this will perform all storage, retrieval and
/// deletion operation
///
pub async fn server_builder(
    global_app_state: Arc<GlobalAppState>,
) -> Result<(), error::ConfigurationError>
where
{
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
    let router = router.nest(
        "/api/v2/vault",
        axum::Router::new()
            .route("/delete", post(routes_v2::data::delete_data))
            .route("/add", post(routes_v2::data::add_data))
            .route("/retrieve", post(routes_v2::data::retrieve_data))
            .route(
                "/fingerprint",
                post(routes::data::get_or_insert_fingerprint),
            ),
    );

    #[cfg(feature = "middleware")]
    let router = router.layer(middleware::from_fn_with_state(
        global_app_state.clone(),
        custom_middleware::middleware,
    ));

    #[cfg(feature = "external_key_manager")]
    let router = router.route("/key/transfer", post(routes::key_migration::transfer_keys));

    #[cfg(feature = "key_custodian")]
    let router = router.nest("/custodian", routes::key_custodian::serve());

    let router = router.layer(
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

    let router = router
        .nest("/health", routes::health::serve())
        .with_state(global_app_state.clone());

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

        axum_server::from_tcp_rustls(tcp_listener, rusttls_config)
            .serve(router.into_make_service())
            .await?;
    } else {
        let tcp_listener = tokio::net::TcpListener::bind(socket_addr).await?;

        axum::serve(tcp_listener, router.into_make_service()).await?;
    }

    Ok(())
}
