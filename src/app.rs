use axum::routing;
use error_stack::ResultExt;
use tower_http::trace as tower_trace;

use std::sync::Arc;

use crate::{
    config::{self, GlobalConfig, TenantConfig},
    error, routes, storage,
    tenant::GlobalAppState,
};

#[cfg(feature = "caching")]
use crate::storage::caching::Caching;

#[cfg(feature = "kms")]
use crate::crypto::{
    kms::{self, Base64Encoded, KmsData, Raw},
    Encryption,
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
}

#[allow(clippy::expect_used)]
impl TenantAppState {
    ///
    /// Construct new app state with configuration
    ///
    /// # Panics
    ///
    /// - If the master key cannot be parsed as a string
    /// - If the public/private key cannot be parsed as a string after kms decrypt
    /// - If the database password cannot be parsed as a string after kms decrypt
    ///
    pub async fn new(
        global_config: GlobalConfig,
        tenant_config: TenantConfig,
    ) -> error_stack::Result<Self, error::ConfigurationError> {
        let db = storage::Storage::new(&global_config.database, &tenant_config.tenant_id)
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
) -> Result<
    axum::serve::Serve<routing::IntoMakeService<axum::Router>, axum::Router>,
    error::ConfigurationError,
>
where
{
    let socket_addr = std::net::SocketAddr::new(
        global_app_state.global_config.server.host.parse()?,
        global_app_state.global_config.server.port,
    );
    let router = axum::Router::new()
        .nest("/tenant", routes::tenant::serve())
        .nest(
            "/data",
            routes::data::serve(
                #[cfg(any(feature = "middleware", feature = "limit"))]
                global_app_state.clone(),
            ),
        )
        .nest(
            "/cards",
            routes::data::serve(
                #[cfg(any(feature = "middleware", feature = "limit"))]
                global_app_state.clone(),
            ),
        )
        .nest("/health", routes::health::serve());

    #[cfg(feature = "key_custodian")]
    let router = router.nest("/custodian", routes::key_custodian::serve());

    let router = router.with_state(global_app_state.clone()).layer(
        tower_trace::TraceLayer::new_for_http()
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
    let tcp_listener = tokio::net::TcpListener::bind(&socket_addr).await?;
    let server = axum::serve(tcp_listener, router.into_make_service());
    Ok(server)
}
