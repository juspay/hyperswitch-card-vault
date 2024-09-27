use axum::Json;
pub mod types;

use crate::{
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError},
};

pub async fn delete_card(
    TenantStateResolver(_tenant_app_state): TenantStateResolver,
    Json(_request): Json<types::DeleteCardRequest>,
) -> Result<Json<types::DeleteCardResponse>, ContainerError<error::ApiError>> {
    // need handle this once the key manger service is ready
    todo!()
}
