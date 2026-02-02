pub mod config;
pub mod internal_keymanager;

#[cfg(feature = "external_key_manager")]
pub mod external_keymanager;

pub use config::ExternalKeyManagerConfig;

use crate::{
    app::TenantAppState,
    error::{self, ContainerError},
};
use masking::{Secret, StrongSecret};

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

#[cfg(feature = "external_key_manager")]
pub fn get_dek_manager(config: &ExternalKeyManagerConfig) -> Box<dyn KeyProvider> {
    match config {
        ExternalKeyManagerConfig::Disabled => Box::new(internal_keymanager::InternalKeyManager),
        ExternalKeyManagerConfig::Enabled { .. }
        | ExternalKeyManagerConfig::EnabledWithMtls { .. } => {
            Box::new(external_keymanager::ExternalKeyManager)
        }
    }
}

#[cfg(not(feature = "external_key_manager"))]
pub fn get_dek_manager() -> Box<dyn KeyProvider> {
    Box::new(internal_keymanager::InternalKeyManager)
}
