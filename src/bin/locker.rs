use hyperswitch_card_vault::{logger, observability, tenant::GlobalAppState};

#[allow(clippy::expect_used)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut global_config =
        hyperswitch_card_vault::config::GlobalConfig::new().expect("Failed while parsing config");

    let _guard = logger::setup(
        &global_config.log,
        hyperswitch_card_vault::service_name!(),
        [hyperswitch_card_vault::service_name!(), "tower_http"],
    )
    .expect("Failed to initialize logging");

    global_config
        .validate()
        .expect("Failed to validate application configuration");

    // Initialize metrics here so that the global meter provider is already set
    // when the secret manager is called.
    let metrics_handle = observability::init_metrics(&global_config.metrics);

    global_config
        .fetch_raw_secrets()
        .await
        .expect("Failed to fetch raw application secrets");

    let global_app_state = GlobalAppState::new(global_config).await;

    hyperswitch_card_vault::app::server_builder(global_app_state, metrics_handle)
        .await
        .expect("Failed while building the server");

    Ok(())
}
