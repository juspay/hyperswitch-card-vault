#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = tartarus::config::Config::new().expect("failed while parsing config");

    let app = tartarus::app::application_builder(&mut config).await?;

    app.await.unwrap();

    Ok(())
}
