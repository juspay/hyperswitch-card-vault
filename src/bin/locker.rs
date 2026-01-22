use tartarus::{crypto::keymanager::KeyManagerMode, logger, tenant::GlobalAppState};

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

    let key_manager_mode = KeyManagerMode::from_config(&global_config.external_key_manager);

    global_config
        .fetch_raw_secrets(&key_manager_mode)
        .await
        .expect("Failed to fetch raw application secrets");

    let global_app_state = GlobalAppState::new(global_config).await;

    tartarus::app::server_builder(global_app_state)
        .await
        .expect("Failed while building the server");

    Ok(())
}
