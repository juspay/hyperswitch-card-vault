use tartarus::{logger, tenant::GlobalAppState};

#[allow(clippy::expect_used)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize rustls crypto provider for GCP KMS
    #[cfg(feature = "kms-gcp")]
    {
        use rustls::crypto::{aws_lc_rs, CryptoProvider};
        let _ = CryptoProvider::install_default(aws_lc_rs::default_provider());
    }

    if cfg!(feature = "dev") {
        eprintln!("This is a dev build, not for production use");
    }

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

    let global_app_state = GlobalAppState::new(global_config).await;

    tartarus::app::server_builder(global_app_state)
        .await
        .expect("Failed while building the server");

    Ok(())
}
