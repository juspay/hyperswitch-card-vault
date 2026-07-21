use axum::Json;
use serde::{Deserialize, Serialize};

use crate::{
    crypto::keymanager,
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError},
    logger,
    routes::data::types::Validation,
};

/// Request body for `POST /entity`.
#[derive(Debug, Deserialize)]
pub struct CreateEntityRequest {
    pub entity_id: String,
}

impl Validation for CreateEntityRequest {
    type Error = error::ApiError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.entity_id.trim().is_empty() {
            Err(error::ApiError::ValidationError(
                "entity_id must not be empty",
            ))
        } else {
            Ok(())
        }
    }
}

/// Response body for `POST /entity`.
#[derive(Debug, Serialize)]
pub struct CreateEntityResponse {
    pub entity_id: String,
    /// ISO 8601 UTC timestamp, using the shared locker timestamp format.
    #[serde(serialize_with = "crate::utils::primitive_datetime_serde::iso8601::serialize")]
    pub created_at: time::PrimitiveDateTime,
}

/// `POST /entity` — explicitly and idempotently provisions the key-holder record for
/// `entity_id`. The backing table is chosen by configuration: the `merchant` table under the
/// internal key manager, or the `entity` table under the external key manager. Returns the
/// existing record unchanged if it already exists.
pub async fn create_entity(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<CreateEntityRequest>,
) -> Result<Json<CreateEntityResponse>, ContainerError<error::ApiError>> {
    request.validate()?;

    let created = keymanager::get_dek_manager(&tenant_app_state.config.external_key_manager)
        .create_entity(&tenant_app_state, request.entity_id.clone())
        .await?;

    let response = Json(CreateEntityResponse {
        entity_id: created.entity_id,
        created_at: created.created_at,
    });
    logger::info!(create_entity_response = ?response);

    Ok(response)
}
