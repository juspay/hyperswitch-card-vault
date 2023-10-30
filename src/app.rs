use axum::routing;
use error_stack::ResultExt;
use futures_util::TryFutureExt;
use hyper::server::conn;

use crate::{config, error, routes, storage};

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
    let state = AppState::new(config)
        .map_err(|_| error::ConfigurationError::DatabaseError)
        .await?;

    let router = axum::Router::new()
        .nest("/tenant", routes::tenant::serve())
        .nest(
            "/data",
            routes::data::serve(
                #[cfg(feature = "middleware")]
                state.clone(),
            ),
        )
        .with_state(state)
        .route("/health", routing::get(routes::health::health));

    let server = axum::Server::try_bind(&socket_addr)?.serve(router.into_make_service());
    Ok(server)
}

impl AppState {
    pub async fn new(
        config: config::Config,
    ) -> error_stack::Result<Self, error::ConfigurationError> {
        Ok(Self {
            db: storage::Storage::new(config.database.url.to_owned())
                .await
                .change_context(error::ConfigurationError::DatabaseError)?,

            config,
        })
    }
}
