pub async fn health() -> (hyper::StatusCode, &'static str) {
    crate::logger::info!("Health was called");
    (hyper::StatusCode::OK, "health is good")
}
