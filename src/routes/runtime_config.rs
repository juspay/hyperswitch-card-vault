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
    runtime_config::RuntimeConfigUpdate,
    tenant::GlobalAppState,
};

/// `POST /runtime-config`
///
/// Body: `{"key": "<config key>", "value": {...}}`. The key selects which runtime config
/// is written and, with it, the struct `value` must match — both are fixed at compile
/// time by the `runtime_configs!` registry, so an unknown key or an unknown field inside
/// `value` is rejected during deserialization, before any storage call.
///
/// Auth:
///   - `x-tenant-id`        → tenant whose config table is updated
///   - `x-internal-api-key` → shared secret — must match `runtime_config.admin_api_key`
#[tracing::instrument(skip_all)]
pub async fn update_runtime_config(
    State(_global_app_state): State<Arc<GlobalAppState>>,
    headers: HeaderMap,
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(payload): Json<RuntimeConfigUpdate>,
) -> Result<Json<RuntimeConfigUpdate>, ContainerError<error::ApiError>> {
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
        Err(error::ApiError::Unauthorized)?;
    }

    manager
        .update(&tenant_app_state.db, payload.clone())
        .await
        .map_err(ContainerError::<error::ApiError>::from)?;

    Ok(Json(payload))
}
