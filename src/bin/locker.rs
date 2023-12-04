use std::sync::Arc;

#[cfg(feature = "profiling")]
use tokio::signal;

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

    #[allow(clippy::expect_used)]
    #[cfg(feature = "profiling")]
    let pprof_guard = pprof::ProfilerGuardBuilder::default()
        .frequency(get_or_default_env("PROFILING__FREQUENCY", 1000))
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .expect("Failed while building pprof guard");

    #[cfg(feature = "key_custodian")]
    {
        let state_lock = state.clone();

        let (server1_tx, mut server1_rx) = tokio::sync::mpsc::channel::<()>(1);

        let server1 = tartarus::app::server1_builder(state_lock, server1_tx.clone())
            .await?
            .with_graceful_shutdown(graceful_shutdown_server1(&mut server1_rx));

        logger::info!(
            "Key Custodian started [{:?}] [{:?}]",
            config.server,
            config.log
        );
        server1.await.expect("Failed while running the server 1");
    }

    let new_state = state.read().await.to_owned();
    let server2 = tartarus::app::server2_builder(&new_state).await?;

    #[cfg(feature = "profiling")]
    let server2 = server2.with_graceful_shutdown(shutdown_signal());

    logger::info!("Locker started [{:?}] [{:?}]", config.server, config.log);
    server2.await.expect("Failed while running the server 2");

    #[allow(clippy::expect_used)]
    #[cfg(feature = "profiling")]
    if let Ok(report) = pprof_guard.report().build() {
        use pprof::protos::Message;
        use std::io::Write;

        let file_path = get_or_default_env("PROFILER__FOLDER", "./".to_owned());
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Failed while getting unix ts")
            .as_secs();
        let mut file = std::fs::File::create(format!("{file_path}/profile_{ts}.pb"))
            .expect("Failed while creating .pb file");
        let profile = report
            .pprof()
            .expect("Failed while getting the profile from report");

        let mut content = Vec::new();
        profile
            .write_to_vec(&mut content)
            .expect("Failed while writing to buffer");
        file.write_all(&content)
            .expect("Failed while writing data to file");
    }

    Ok(())
}

#[cfg(feature = "key_custodian")]
async fn graceful_shutdown_server1(recv: &mut tokio::sync::mpsc::Receiver<()>) {
    recv.recv().await;
    logger::info!("Shutting down the server1 gracefully.");
}

#[cfg(feature = "profiling")]
fn get_or_default_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|string_value| string_value.parse().ok())
        .unwrap_or(default)
}

#[allow(clippy::expect_used)]
#[cfg(feature = "profiling")]
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("signal received, starting graceful shutdown");
}
