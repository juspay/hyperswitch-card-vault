use tartarus::{logger, tenant::GlobalAppState};

#[allow(clippy::expect_used)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut global_config =
        tartarus::config::GlobalConfig::new().expect("Failed while parsing config");

    let _guard = logger::setup(
        &global_config.log,
        tartarus::service_name!(),
        [tartarus::service_name!(), "tower_http"],
    );

    #[allow(clippy::expect_used)]
    global_config
        .validate()
        .expect("Failed to validate application configuration");
    global_config
        .fetch_raw_secrets()
        .await
        .expect("Failed to fetch raw application secrets");

    let global_app_state = GlobalAppState::new(&global_config).await;

    let server = tartarus::app::server_builder(global_app_state).await?;
    logger::info!(
        "Locker started [{:?}] [{:?}]",
        global_config.server,
        global_config.log
    );
    server.await.expect("Failed while running the server 2");

    Ok(())
}
