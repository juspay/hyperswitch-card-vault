use std::{collections::HashMap, sync::Arc};

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
        Ok(Self(state.get_app_state_of_tenant(tenant_id).await?))
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
///
/// A bare value is the primary id; `label=value` pairs address additional fingerprints by the
/// labels in the request body, so ids resolve by label rather than by position:
///
/// ```text
/// x-fingerprint-id: wyH7yWdtaRhxDkVE1cvs
/// x-fingerprint-id: first=DzqaW1Vvvp2JlY2do68u,second=Kp3xRt9wQm2vLd8nYs5c
/// ```
///
/// Every value must be exactly 20 alphanumeric (0-9 a-z A-Z) characters, matching the format of
/// server-generated fingerprint IDs.
#[derive(Debug, Default)]
pub struct OptionalFingerprintId {
    primary: Option<Secret<String>>,
    named: HashMap<String, Secret<String>>,
}

impl OptionalFingerprintId {
    pub fn primary(&self) -> Option<Secret<String>> {
        self.primary.clone()
    }

    pub fn named(&self, label: &str) -> Option<Secret<String>> {
        self.named.get(label).cloned()
    }
}

fn validated_fingerprint_id(value: &str) -> Result<Secret<String>, ContainerError<ApiError>> {
    match value.len() == consts::ID_LENGTH && value.chars().all(|c| c.is_ascii_alphanumeric()) {
        true => Ok(Secret::new(value.to_string())),
        false => Err(ContainerError::from(ApiError::ValidationError(
            "x-fingerprint-id values must be exactly 20 alphanumeric characters",
        ))),
    }
}

#[async_trait]
impl FromRequestParts<Arc<GlobalAppState>> for OptionalFingerprintId {
    type Rejection = ContainerError<ApiError>;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &Arc<GlobalAppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(consts::X_FINGERPRINT_ID)
            .and_then(|h| h.to_str().ok());

        let mut resolved = Self::default();

        for entry in header.into_iter().flat_map(|value| value.split(',')) {
            let entry = entry.trim();
            match entry.split_once('=') {
                Some((name, value)) => {
                    resolved
                        .named
                        .insert(name.trim().to_string(), validated_fingerprint_id(value.trim())?);
                }
                None => resolved.primary = Some(validated_fingerprint_id(entry)?),
            }
        }

        Ok(resolved)
    }
}
