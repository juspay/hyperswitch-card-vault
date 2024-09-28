use axum::Json;
pub mod types;

use crate::{
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError},
};

pub async fn delete_data(
    TenantStateResolver(_tenant_app_state): TenantStateResolver,
    Json(_request): Json<types::DeleteDataRequest>,
) -> Result<Json<types::DeleteDataResponse>, ContainerError<error::ApiError>> {
    // need handle this once the key manger service is ready
    todo!()
}

pub async fn retrieve_data(
    TenantStateResolver(_tenant_app_state): TenantStateResolver,
    Json(_request): Json<types::RetrieveDataRequest>,
) -> Result<Json<types::RetrieveDataResponse>, ContainerError<error::ApiError>> {
    // need handle this once the key manger service is ready
    todo!()
}

pub async fn add_data(
    TenantStateResolver(_tenant_app_state): TenantStateResolver,
    Json(_request): Json<types::StoreDataRequest>,
) -> Result<Json<types::StoreDataResponse>, ContainerError<error::ApiError>> {
    // need handle this once the key manger service is ready
    todo!()
}
