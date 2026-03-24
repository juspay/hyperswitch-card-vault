use std::sync::Arc;

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
    error, logger,
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
    #[cfg_attr(
        all(
            not(feature = "middleware"),
            not(feature = "external_key_manager"),
            not(feature = "key_custodian")
        ),
        allow(unused_mut)
    )]
    let mut router = router.nest(
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

        axum_server::from_tcp_rustls(tcp_listener, rusttls_config)
            .serve(router.into_make_service())
            .await?;
    } else {
        let tcp_listener = tokio::net::TcpListener::bind(socket_addr).await?;

        axum::serve(tcp_listener, router.into_make_service()).await?;
    }

    Ok(())
}
