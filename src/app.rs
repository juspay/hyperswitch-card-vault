use axum::routing;
use error_stack::ResultExt;
use futures_util::TryFutureExt;
use hyper::server::conn;

use crate::{config, error, routes, storage};

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

pub async fn application_builder(
    config: config::Config,
) -> Result<
    hyper::Server<conn::AddrIncoming, routing::IntoMakeService<axum::Router>>,
    error::ConfigurationError,
>
where
{
    let socket_addr = std::net::SocketAddr::new(config.server.host.parse()?, config.server.port);

    let router = axum::Router::new()
        .nest("/tenant", routes::tenant::serve())
        .nest("/data", routes::data::serve())
        .with_state(
            AppState::new(config)
                .map_err(|_| error::ConfigurationError::DatabaseError)
                .await?,
        )
        .route("/health", routing::get(routes::health::health));

    let server = axum::Server::try_bind(&socket_addr)?.serve(router.into_make_service());
    Ok(server)
}

impl AppState {
    async fn new(config: config::Config) -> error_stack::Result<Self, error::ConfigurationError> {
        #[cfg(feature = "kms")]
        {
            let master_key_kms_input: KmsData<Base64Encoded> = KmsData {
                data: String::from_utf8(config.secrets.master_key.clone())
                    .expect("Failed while converting bytes to String"),
                decode_op: PhantomData,
            };

            #[allow(clippy::expect_used)]
            let kms_decrypted_master_key: KmsData<Raw> = kms::get_kms_client(&config.kms)
                .await
                .decrypt(master_key_kms_input)
                .await
                .expect("Failed while performing KMS decryption");

            let mut config = config.clone();
            config.secrets.master_key = kms_decrypted_master_key.data;
        }

        Ok(Self {
            db: storage::Storage::new(config.database.url.to_owned())
                .await
                .change_context(error::ConfigurationError::DatabaseError)?,

            config,
        })
    }
}
