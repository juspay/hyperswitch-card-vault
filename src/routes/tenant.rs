
//! 
//! Not Implemented
//!

// use crate::error;
// use axum::{extract::State, Json};

// use axum::{routing::{get, post}, extract};
// mod types;
// use types::*;

pub fn serve<S>() -> axum::Router<S>
where
    S: Send + Sync + Clone + 'static,
{
    axum::Router::new()
    // .route("/create", post(todo!()))
    // .route("/get", get(todo!()))
    // .route("/delete", post(todo!()))
}

// async fn create_tenant(
//     Json(request): Json<TenantCreateRequest>,
// ) -> Result<TenantCreateResponse, error::ApiError> {
// }
