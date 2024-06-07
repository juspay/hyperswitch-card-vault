use std::sync::Arc;

use axum::{async_trait, extract::FromRequestParts, http::request::Parts};

use crate::{app::TenantAppState, error::ApiError, tenant::GlobalAppState};

#[derive(Clone)]
pub struct TenantStateResolver(pub Arc<TenantAppState>);

#[async_trait]
impl FromRequestParts<Arc<GlobalAppState>> for TenantStateResolver {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<GlobalAppState>,
    ) -> Result<Self, Self::Rejection> {
        let tenant_id = parts
            .headers
            .get("x-tenant-id")
            .and_then(|h| h.to_str().ok())
            .ok_or(ApiError::TenantError("x-tenant-id not found in headers"))?;

        state.is_known_tenant(tenant_id).await?;
        Ok(Self(state.get_app_state_of_tenant(tenant_id).await?))
    }
}

#[cfg(feature = "key_custodian")]
#[derive(Debug)]
pub struct TenantId(pub String);

#[cfg(feature = "key_custodian")]
#[async_trait]
impl FromRequestParts<Arc<GlobalAppState>> for TenantId {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<GlobalAppState>,
    ) -> Result<Self, Self::Rejection> {
        let tenant_id = parts
            .headers
            .get("x-tenant-id")
            .and_then(|h| h.to_str().ok())
            .map(ToString::to_string)
            .ok_or(ApiError::TenantError("x-tenant-id not found in header"))?;

        state.is_known_tenant(&tenant_id).await?;
        state.is_custodian_unlocked(&tenant_id).await?;

        Ok(Self(tenant_id))
    }
}
