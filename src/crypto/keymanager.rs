pub mod internal_keymanager;

#[cfg(feature = "external_key_manager")]
pub mod external_keymanager;

use hyperswitch_masking::{Secret, StrongSecret};

pub use crate::config::ExternalKeyManagerConfig;
use crate::{
    app::TenantAppState,
    error::{self, ContainerError},
};

/// Metadata about a key-holder record (a `merchant` row under the internal key manager,
/// or an `entity` row under the external key manager) returned by the explicit create flow.
pub struct CreatedEntity {
    pub entity_id: String,
    pub created_at: time::PrimitiveDateTime,
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

    /// Idempotently create the key-holder record for `entity_id` and return its metadata.
    /// Backs the explicit `POST /entity/create` endpoint. Unlike [`Self::find_or_create_entity`]
    /// (the deprecated lazy path used by the add flow), this is the blessed provisioning path.
    async fn create_entity(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<CreatedEntity, ContainerError<error::ApiError>>;
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

pub fn get_dek_manager(config: &ExternalKeyManagerConfig) -> Box<dyn KeyProvider> {
    match config {
        ExternalKeyManagerConfig::Disabled => Box::new(internal_keymanager::InternalKeyManager),
        #[cfg(feature = "external_key_manager")]
        ExternalKeyManagerConfig::Enabled { .. }
        | ExternalKeyManagerConfig::EnabledWithMtls { .. } => {
            Box::new(external_keymanager::ExternalKeyManager)
        }
    }
}
