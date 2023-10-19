use axum::routing;
use hyper::server::conn;

use crate::{config, db, error};

#[derive(Clone)]
struct AppState {
    db: db::Pool,
}

pub fn application_builder<I, S, State>(
    config: config::Config,
) -> Result<
    hyper::Server<conn::AddrIncoming, routing::IntoMakeService<axum::Router>>,
    error::ConfigurationError,
>
where
{
    let socket_addr =
        std::net::SocketAddr::new(config.server.host.parse()?, config.server.port);

    let router = axum::Router::new().with_state(AppState::new(config));

    let server = axum::Server::try_bind(&socket_addr)?.serve(router.into_make_service());
    Ok(server)
}

impl AppState {
    fn new(config: config::Config) -> Self {
        todo!()
    }
}
