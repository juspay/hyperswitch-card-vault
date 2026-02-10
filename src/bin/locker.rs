use tartarus::{logger, tenant::GlobalAppState};

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut global_config = tartarus::config::GlobalConfig::new()
        .map_err(|e| format!("Failed to parse config: {e}"))?;

    let _guard = logger::setup(
        &global_config.log,
        tartarus::service_name!(),
        [tartarus::service_name!(), "tower_http"],
    );

    global_config.validate()
        .map_err(|e| format!("Failed to validate application configuration: {e}"))?;

    global_config
        .fetch_raw_secrets()
        .await
        .map_err(|e| format!("Failed to fetch raw application secrets: {e}"))?;

    let global_app_state = GlobalAppState::new(global_config).await;

    tartarus::app::server_builder(global_app_state)
        .await
        .map_err(|e| format!("Failed to build server: {e}"))?;

    Ok(())
}

#[tokio::main]
async fn main() {
    if cfg!(feature = "dev") {
        eprintln!("This is a dev build, not for production use");
    }

    if let Err(e) = run().await {
        eprintln!("Application startup error: {e}");
        std::process::exit(1);
    }
}
