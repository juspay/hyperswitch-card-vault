use std::sync::Arc;

use axum::{async_trait, extract::FromRequestParts, http::request::Parts};
use hyperswitch_masking::Secret;

use crate::{
    app::TenantAppState,
    error::{ApiError, ContainerError},
    storage::consts,
    tenant::GlobalAppState,
};

#[derive(Clone)]
pub struct TenantStateResolver(pub Arc<TenantAppState>);

#[async_trait]
impl FromRequestParts<Arc<GlobalAppState>> for TenantStateResolver {
    type Rejection = ContainerError<ApiError>;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<GlobalAppState>,
    ) -> Result<Self, Self::Rejection> {
        let tenant_id = parts
            .headers
            .get(consts::X_TENANT_ID)
            .and_then(|h| h.to_str().ok())
            .ok_or(ApiError::TenantError("x-tenant-id not found in headers"))?;

        state.is_known_tenant(tenant_id)?;
        let app_state = state.get_app_state_of_tenant(tenant_id).await?;

        Ok(Self(app_state))
    }
}

#[cfg(feature = "key_custodian")]
#[derive(Debug)]
pub struct TenantId(pub String);

#[cfg(feature = "key_custodian")]
#[async_trait]
impl FromRequestParts<Arc<GlobalAppState>> for TenantId {
    type Rejection = ContainerError<ApiError>;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<GlobalAppState>,
    ) -> Result<Self, Self::Rejection> {
        let tenant_id = parts
            .headers
            .get(consts::X_TENANT_ID)
            .and_then(|h| h.to_str().ok())
            .map(ToString::to_string)
            .ok_or(ApiError::TenantError("x-tenant-id not found in header"))?;

        state.is_known_tenant(&tenant_id)?;
        state.is_custodian_unlocked(&tenant_id).await?;

        Ok(Self(tenant_id))
    }
}

/// Optionally reads `x-fingerprint-id` from request headers.
/// If present, the value must be exactly 20 alphanumeric (0-9 a-z A-Z) characters,
/// matching the format of server-generated fingerprint IDs.
#[derive(Debug)]
pub struct OptionalFingerprintId(pub Option<Secret<String>>);

#[async_trait]
impl FromRequestParts<Arc<GlobalAppState>> for OptionalFingerprintId {
    type Rejection = ContainerError<ApiError>;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &Arc<GlobalAppState>,
    ) -> Result<Self, Self::Rejection> {
        let fingerprint_id = parts
            .headers
            .get(consts::X_FINGERPRINT_ID)
            .and_then(|h| h.to_str().ok())
            .map(|s| -> Result<Secret<String>, ContainerError<ApiError>> {
                if s.len() != consts::ID_LENGTH || !s.chars().all(|c| c.is_ascii_alphanumeric()) {
                    Err(ContainerError::from(ApiError::ValidationError(
                        "x-fingerprint-id must be exactly 20 alphanumeric characters",
                    )))
                } else {
                    Ok(Secret::new(s.to_string()))
                }
            })
            .transpose()?;

        Ok(Self(fingerprint_id))
    }
}
