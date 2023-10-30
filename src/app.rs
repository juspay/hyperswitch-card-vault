use axum::routing;
use error_stack::ResultExt;
use hyper::server::conn;
#[cfg(feature = "key_custodian")]
use tokio::sync::{mpsc::Sender, RwLock};

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
#[cfg(feature = "kms")]
use std::marker::PhantomData;

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
                #[cfg(feature = "middleware")]
                state.clone(),
            ),
        )
        .with_state(state.clone())
        .route("/health", routing::get(routes::health::health));

    let server = axum::Server::try_bind(&socket_addr)?.serve(router.into_make_service());
    Ok(server)
}

impl AppState {
    pub async fn new(
        config: &mut config::Config,
    ) -> error_stack::Result<Self, error::ConfigurationError> {
        #[cfg(feature = "kms")]
        {
            let kms_client = kms::get_kms_client(&config.kms).await;

            let master_key_kms_input: KmsData<Base64Encoded> = KmsData {
                data: String::from_utf8(config.secrets.master_key.clone())
                    .expect("Failed while converting bytes to String"),
                decode_op: PhantomData,
            };

            #[allow(clippy::expect_used)]
            let kms_decrypted_master_key: KmsData<Raw> = kms_client
                .decrypt(master_key_kms_input)
                .await
                .expect("Failed while performing KMS decryption");

            config.secrets.master_key = kms_decrypted_master_key.data;
        }

        Ok(Self {
            db: storage::Storage::new(config.database.url.to_owned())
                .await
                .change_context(error::ConfigurationError::DatabaseError)?,

            config: config.clone(),
        })
    }
}
