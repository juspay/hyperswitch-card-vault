use axum::routing;
use error_stack::ResultExt;
use hyper::server::conn;
#[cfg(feature = "kms")]
use masking::PeekInterface;
#[cfg(feature = "key_custodian")]
use tokio::sync::{mpsc::Sender, RwLock};
use tower_http::trace as tower_trace;

#[cfg(feature = "key_custodian")]
use std::sync::Arc;

use crate::{
    config, error, routes,
    storage::{self},
};

#[cfg(feature = "kms")]
use crate::crypto::{
    kms::{self, Base64Encoded, KmsData, Raw},
    Encryption,
};

///
/// AppState:
///
///
/// The state that is passed
///
#[derive(Clone)]
pub struct AppState {
    pub db: storage::Storage,
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
        .route(
            "/health",
            routing::get(routes::health::health::<routes::health::Custodian, SharedState>),
        )
        .with_state(shared_state);

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
        .route(
            "/health",
            routing::get(routes::health::health::<routes::health::Locker, AppState>),
        )
        .with_state(state.clone())
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
        #[cfg(feature = "kms")]
        {
            let kms_client = kms::get_kms_client(&config.kms).await;

            let master_key_kms_input: KmsData<Base64Encoded> = KmsData::new(
                String::from_utf8(config.secrets.master_key.clone())
                    .expect("Failed while converting bytes to String"),
            );
            let kms_decrypted_master_key: KmsData<Raw> = kms_client
                .decrypt(master_key_kms_input)
                .await
                .change_context(error::ConfigurationError::KmsDecryptError("master_key"))?;
            config.secrets.master_key = hex::decode(
                String::from_utf8(kms_decrypted_master_key.data)
                    .expect("Failed while converting bytes to String"),
            )
            .expect("Failed to hex decode master_key");

            #[cfg(feature = "middleware")]
            {
                let tenant_public_key_kms_input: KmsData<Base64Encoded> =
                    KmsData::new(config.secrets.tenant_public_key.peek().clone());
                let kms_decrypted_tenant_public_key: KmsData<Raw> = kms_client
                    .decrypt(tenant_public_key_kms_input)
                    .await
                    .change_context(error::ConfigurationError::KmsDecryptError(
                        "tenant_public_key",
                    ))?;
                config.secrets.tenant_public_key =
                    String::from_utf8(kms_decrypted_tenant_public_key.data)
                        .expect("Failed while converting bytes to String")
                        .into();
            }

            #[cfg(feature = "middleware")]
            {
                let locker_private_key_kms_input: KmsData<Base64Encoded> =
                    KmsData::new(config.secrets.locker_private_key.peek().clone());
                let kms_decrypted_locker_private_key: KmsData<Raw> = kms_client
                    .decrypt(locker_private_key_kms_input)
                    .await
                    .change_context(error::ConfigurationError::KmsDecryptError(
                        "locker_private_key",
                    ))?;
                config.secrets.locker_private_key =
                    String::from_utf8(kms_decrypted_locker_private_key.data)
                        .expect("Failed while converting bytes to String")
                        .into();
            }

            let db_password_kms_input: KmsData<Base64Encoded> =
                KmsData::new(config.database.password.peek().clone());
            let kms_decrypted_db_password: KmsData<Raw> = kms_client
                .decrypt(db_password_kms_input)
                .await
                .change_context(error::ConfigurationError::KmsDecryptError("db_password"))?;
            config.database.password = String::from_utf8(kms_decrypted_db_password.data)
                .expect("Failed while converting bytes to String")
                .into();
        }
        Ok(Self {
            db: storage::Storage::new(&config.database)
                .await
                .change_context(error::ConfigurationError::DatabaseError)?,

            config: config.clone(),
        })
    }
}
