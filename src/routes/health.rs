pub async fn health() -> (hyper::StatusCode, &'static str) {
    (hyper::StatusCode::OK, "health is good")
}
