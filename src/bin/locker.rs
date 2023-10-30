#[cfg(feature = "key_custodian")]
use std::sync::Arc;

#[cfg(feature = "key_custodian")]
use tokio::sync::RwLock;

use futures_util::TryFutureExt;
use tartarus::{app::AppState, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = tartarus::config::Config::new().expect("failed while parsing config");
    let state = AppState::new(&mut config)
        .map_err(|_| error::ConfigurationError::DatabaseError)
        .await?;

    #[cfg(feature = "key_custodian")]
    {
        let state_lock = Arc::new(RwLock::new(state.clone()));

        let (server1_tx, mut server1_rx) = tokio::sync::mpsc::channel::<()>(1);

        let server1 = tartarus::app::server1_builder(state_lock, server1_tx.clone())
            .await?
            .with_graceful_shutdown(graceful_shutdown_server1(&mut server1_rx));
        server1.await.unwrap();
    }

    let server2 = tartarus::app::server2_builder(&state).await?;
    server2.await.unwrap();

    Ok(())
}

#[cfg(feature = "key_custodian")]
async fn graceful_shutdown_server1(recv: &mut tokio::sync::mpsc::Receiver<()>) {
    recv.recv().await;
    println!("Shutting down the server1 gracefully.");
}
