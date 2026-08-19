//! Admin endpoint for runtime config updates.
//!
//! `POST /runtime-config` validates state transitions, writes the config body into the
//! per-tenant `configs` Postgres table, and invalidates the tenant's Redis cache entry.
//! No in-process state is applied — every consumer (KV routing, replica routing) reads
//! the config per-operation from Postgres via the tenant's Redis cache.

use std::sync::Arc;

use axum::{Json, extract::State, http::HeaderMap};

use crate::{
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError},
    tenant::GlobalAppState,
};

#[derive(serde::Deserialize)]
pub struct UpdateRuntimeConfigRequest {
    /// Raw config body, e.g. `{"use_replica":true,"enable_kv":"enabled"}`.
    /// Unknown fields are rejected by `RuntimeConfigValues::deserialize`.
    pub value: serde_json::Value,
}

#[derive(serde::Serialize)]
pub struct UpdateRuntimeConfigResponse {
    /// The applied config value (echo back).
    pub value: serde_json::Value,
}

/// `POST /runtime-config`
///
/// Auth:
///   - `x-tenant-id`        → tenant whose config table is updated
///   - `x-internal-api-key` → shared secret — must match `runtime_config.admin_api_key`
#[tracing::instrument(skip_all)]
pub async fn update_runtime_config(
    State(_global_app_state): State<Arc<GlobalAppState>>,
    headers: HeaderMap,
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(payload): Json<UpdateRuntimeConfigRequest>,
) -> Result<Json<UpdateRuntimeConfigResponse>, ContainerError<error::ApiError>> {
    let api_key_header = headers
        .get("x-internal-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(error::ApiError::Unauthorized)?;

    let manager =
        tenant_app_state
            .db
            .runtime_config_manager()
            .ok_or(error::ApiError::BadRequest(
                "Runtime config is not enabled for this tenant",
            ))?;

    if !manager.verify_admin_api_key(api_key_header) {
        return Err(error::ApiError::Unauthorized)?;
    }

    manager
        .update(&tenant_app_state.db, payload.value.clone())
        .await
        .map_err(ContainerError::<error::ApiError>::from)?;

    Ok(Json(UpdateRuntimeConfigResponse {
        value: payload.value,
    }))
}
