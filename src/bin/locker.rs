#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = tartarus::config::Config::new().expect("failed while parsing config");

    let app = tartarus::app::application_builder(config).await?;

    app.await.unwrap();

    Ok(())
}
