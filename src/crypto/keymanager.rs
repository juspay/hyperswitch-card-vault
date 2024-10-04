#[cfg(feature = "external_key_manager")]
pub mod external_keymanager;
#[cfg(not(feature = "external_key_manager"))]
pub mod internal_keymanager;

use crate::{
    app::TenantAppState,
    error::{self, ContainerError},
};
use masking::Secret;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait KeyProvider: Send + Sync {
    async fn find_by_entity_id(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoManager>, ContainerError<error::ApiError>>;

    async fn find_or_create_entity(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoManager>, ContainerError<error::ApiError>>;
}

#[async_trait::async_trait]
pub trait CryptoManager: Send + Sync {
    async fn encrypt_data(
        &self,
        tenant_app_state: &TenantAppState,
        decryted_data: Secret<Vec<u8>>,
    ) -> Result<Secret<Vec<u8>>, ContainerError<error::ApiError>>;
    async fn decrypt_data(
        &self,
        tenant_app_state: &TenantAppState,
        encrypted_data: Secret<Vec<u8>>,
    ) -> Result<Secret<Vec<u8>>, ContainerError<error::ApiError>>;
}

pub fn get_dek_manager() -> Arc<dyn KeyProvider> {
    #[cfg(feature = "external_key_manager")]
    {
        Arc::new(external_keymanager::ExternalKeyManager)
    }

    #[cfg(not(feature = "external_key_manager"))]
    {
        Arc::new(internal_keymanager::InternalKeyManager)
    }
}
