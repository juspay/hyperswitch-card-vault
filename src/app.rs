use axum::routing;
use error_stack::ResultExt;
use hyper::server::conn;
use masking::PeekInterface;
#[cfg(feature = "key_custodian")]
use tokio::sync::{mpsc::Sender, RwLock};
use tower_http::trace as tower_trace;

#[cfg(feature = "key_custodian")]
use std::sync::Arc;

use crate::{
    config,
    crypto::{self, Decode},
    error, routes,
    storage::{self},
};

#[cfg(feature = "caching")]
use crate::storage::{caching::Caching, types};

#[cfg(feature = "kms")]
use crate::crypto::{
    kms::{self, Base64Encoded, KmsData, Raw},
    Encryption,
};

#[cfg(feature = "caching")]
type Storage = Caching<Caching<storage::Storage, types::HashTable>, types::Merchant>;

#[cfg(not(feature = "caching"))]
type Storage = storage::Storage;

///
/// AppState:
///
///
/// The state that is passed
///
#[derive(Clone)]
pub struct AppState {
    pub db: Storage,
    pub config: config::Config,
}

/// Temporary State to store keys
#[cfg(feature = "key_custodian")]
#[derive(Default, Debug)]
pub struct Keys {
    pub key1: Option<String>,
    pub key2: Option<String>,
}

#[cfg(feature = "key_custodian")]
pub type SharedState = (
    Arc<RwLock<AppState>>,
    Arc<RwLock<Keys>>,
    tokio::sync::mpsc::Sender<()>,
);

///
/// The server used to fulfil the initialization requirement for the locker. This accepts 2 keys as
/// API input to complete the key custodian stage
///
#[cfg(feature = "key_custodian")]
pub async fn server1_builder(
    state: Arc<RwLock<AppState>>,
    server_tx: Sender<()>,
) -> Result<
    hyper::Server<conn::AddrIncoming, routing::IntoMakeService<axum::Router>>,
    error::ConfigurationError,
>
where
{
    crate::logger::debug!(startup_config=?state.read().await.config);

    let keys = Arc::new(RwLock::new(Keys::default()));
    let socket_addr = std::net::SocketAddr::new(
        state.read().await.config.server.host.parse()?,
        state.read().await.config.server.port,
    );
    let shared_state: SharedState = (state, keys, server_tx);

    let router = axum::Router::new()
        .nest("/custodian", routes::key_custodian::serve())
        .with_state(shared_state)
        .route("/health", routing::get(routes::health::health));

    let server = axum::Server::try_bind(&socket_addr)?.serve(router.into_make_service());
    Ok(server)
}

///
/// The server responsible for the main cards APIs this will perform all storage, retrieval and
/// deletion operation related to locker
///
pub async fn server2_builder(
    state: &AppState,
) -> Result<
    hyper::Server<conn::AddrIncoming, routing::IntoMakeService<axum::Router>>,
    error::ConfigurationError,
>
where
{
    let socket_addr =
        std::net::SocketAddr::new(state.config.server.host.parse()?, state.config.server.port);
    let router = axum::Router::new()
        .nest("/tenant", routes::tenant::serve())
        .nest(
            "/data",
            routes::data::serve(
                #[cfg(any(feature = "middleware", feature = "limit"))]
                state.clone(),
            ),
        )
        .nest(
            "/cards",
            routes::data::serve(
                #[cfg(any(feature = "middleware", feature = "limit"))]
                state.clone(),
            ),
        )
        .with_state(state.clone())
        .route("/health", routing::get(routes::health::health))
        .layer(
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

    let server = axum::Server::try_bind(&socket_addr)?.serve(router.into_make_service());
    Ok(server)
}

#[allow(clippy::expect_used)]
impl AppState {
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
        config: &mut config::Config,
    ) -> error_stack::Result<Self, error::ConfigurationError> {
        let client = Self::kms_client_builder(config).await?;

        config.secrets.master_key = client
            .decode(
                String::from_utf8(config.secrets.master_key.clone())
                    .expect("Failed while converting master key to `String`"),
            )
            .await
            .change_context(error::ConfigurationError::KmsDecryptError("master_key"))?;

        #[cfg(feature = "middleware")]
        {
            config.secrets.tenant_public_key = client
                .decode(config.secrets.tenant_public_key.peek().clone())
                .await
                .change_context(error::ConfigurationError::KmsDecryptError(
                    "tenant_public_key",
                ))?;

            config.secrets.locker_private_key = client
                .decode(config.secrets.locker_private_key.peek().clone())
                .await
                .change_context(error::ConfigurationError::KmsDecryptError(
                    "locker_private_key",
                ))?;
        }

        config.database.password = client
            .decode(config.database.password.peek().clone())
            .await
            .change_context(error::ConfigurationError::KmsDecryptError(
                "database_password",
            ))?;
        Ok(Self {
            db: storage::Storage::new(&config.database)
                .await
                .map(
                    #[cfg(feature = "caching")]
                    storage::caching::implement_cache("hash", &config.cache),
                    #[cfg(not(feature = "caching"))]
                    std::convert::identity,
                )
                .map(
                    #[cfg(feature = "caching")]
                    storage::caching::implement_cache("merchant", &config.cache),
                    #[cfg(not(feature = "caching"))]
                    std::convert::identity,
                )
                .change_context(error::ConfigurationError::DatabaseError)?,

            config: config.clone(),
        })
    }

    pub async fn kms_client_builder(
        config: &config::Config,
    ) -> error_stack::Result<crypto::multiple::Multiple, error::ConfigurationError> {
        crypto::multiple::Multiple::build(config.key_management_service.as_ref()).await
    }
}
