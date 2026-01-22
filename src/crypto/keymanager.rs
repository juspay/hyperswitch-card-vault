pub mod external_keymanager;
pub mod internal_keymanager;

use crate::{
    app::TenantAppState,
    crypto::keymanager::external_keymanager::ExternalKeyManagerConfig,
    error::{self, ContainerError},
};
use masking::{Secret, StrongSecret};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum KeyManagerMode {
    #[default]
    Internal,
    ExternalPlain,
    ExternalMtls,
}

impl KeyManagerMode {
    pub fn from_config(config: &ExternalKeyManagerConfig) -> Self {
        match (config.enabled, config.mtls_enabled) {
            (false, _) => Self::Internal,
            (true, false) => Self::ExternalPlain,
            (true, true) => Self::ExternalMtls,
        }
    }

    pub fn is_external(&self) -> bool {
        matches!(self, Self::ExternalPlain | Self::ExternalMtls)
    }

    pub fn is_mtls_enabled(&self) -> bool {
        matches!(self, Self::ExternalMtls)
    }
}

#[async_trait::async_trait]
pub trait KeyProvider: Send + Sync {
    async fn find_by_entity_id(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>>;

    async fn find_or_create_entity(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>>;
}

#[async_trait::async_trait]
pub trait CryptoOperationsManager: Send + Sync {
    async fn encrypt_data(
        &self,
        tenant_app_state: &TenantAppState,
        decryted_data: StrongSecret<Vec<u8>>,
    ) -> Result<Secret<Vec<u8>>, ContainerError<error::ApiError>>;
    async fn decrypt_data(
        &self,
        tenant_app_state: &TenantAppState,
        encrypted_data: Secret<Vec<u8>>,
    ) -> Result<StrongSecret<Vec<u8>>, ContainerError<error::ApiError>>;
}

pub fn get_dek_manager(is_external: bool) -> Box<dyn KeyProvider> {
    match is_external {
        true => Box::new(external_keymanager::ExternalKeyManager),
        false => Box::new(internal_keymanager::InternalKeyManager),
    }
}
