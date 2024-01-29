use std::sync::Arc;

use tokio::sync::RwLock;

use tartarus::{app::AppState, logger};

#[allow(clippy::expect_used)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = tartarus::config::Config::new().expect("failed while parsing config");
    let state = Arc::new(RwLock::new(AppState::new(&mut config).await?));
    let _guard = logger::setup(
        &config.log,
        tartarus::service_name!(),
        [tartarus::service_name!(), "tower_http"],
    );

    #[cfg(feature = "key_custodian")]
    {
        let state_lock = state.clone();

        let (server1_tx, server1_rx) = tokio::sync::mpsc::channel::<()>(1);

        let server1 = tartarus::app::server1_builder(state_lock, server1_tx.clone())
            .await?
            .with_graceful_shutdown(graceful_shutdown_server1(server1_rx));

        logger::info!(
            "Key Custodian started [{:?}] [{:?}]",
            config.server,
            config.log
        );
        server1.await.expect("Failed while running the server 1");
    }

    let new_state = state.read().await.to_owned();
    let server2 = tartarus::app::server2_builder(&new_state).await?;
    logger::info!("Locker started [{:?}] [{:?}]", config.server, config.log);
    server2.await.expect("Failed while running the server 2");

    Ok(())
}

#[cfg(feature = "key_custodian")]
async fn graceful_shutdown_server1(mut recv: tokio::sync::mpsc::Receiver<()>) {
    recv.recv().await;
    logger::info!("Shutting down the server1 gracefully.");
}
