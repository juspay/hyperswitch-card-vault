use masking::{ExposeInterface, Secret};

use crate::{
    app::TenantAppState,
    crypto::{
        encryption_manager::{encryption_interface::Encryption, managers::aes::GcmAes256},
        keymanager::CryptoOperationsManager,
    },
    error::{self, ContainerError},
    storage::MerchantInterface,
};

pub struct InternalKeyManager;

#[async_trait::async_trait]
impl super::KeyProvider for InternalKeyManager {
    async fn find_by_entity_id(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>> {
        let master_encryption =
            GcmAes256::new(tenant_app_state.config.tenant_secrets.master_key.clone());

        Ok(tenant_app_state
            .db
            .find_by_merchant_id(&entity_id, &master_encryption)
            .await
            .map(|inner| InternalCryptoManager::from_secret_key(inner.enc_key))
            .map(|inner| -> Box<dyn CryptoOperationsManager> { Box::new(inner) })?)
    }

    async fn find_or_create_entity(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>> {
        let master_encryption =
            GcmAes256::new(tenant_app_state.config.tenant_secrets.master_key.clone());

        let entity = tenant_app_state
            .db
            .find_or_create_by_merchant_id(&entity_id, &master_encryption)
            .await;

        let response = entity
            .map(|inner| InternalCryptoManager::from_secret_key(inner.enc_key))
            .map(|inner| -> Box<dyn CryptoOperationsManager> { Box::new(inner) })?;

        Ok(response)
    }
}

pub struct InternalCryptoManager(GcmAes256);

impl InternalCryptoManager {
    fn from_secret_key(key: Secret<Vec<u8>>) -> Self {
        Self(GcmAes256::new(key.expose()))
    }

    fn get_inner(&self) -> &GcmAes256 {
        &self.0
    }
}

#[async_trait::async_trait]
impl CryptoOperationsManager for InternalCryptoManager {
    async fn encrypt_data(
        &self,
        _tenant_app_state: &TenantAppState,
        decryted_data: Secret<Vec<u8>>,
    ) -> Result<Secret<Vec<u8>>, ContainerError<error::ApiError>> {
        Ok(self.get_inner().encrypt(decryted_data.expose())?.into())
    }
    async fn decrypt_data(
        &self,
        _tenant_app_state: &TenantAppState,
        encrypted_data: Secret<Vec<u8>>,
    ) -> Result<Secret<Vec<u8>>, ContainerError<error::ApiError>> {
        Ok(self.get_inner().decrypt(encrypted_data.expose())?.into())
    }
}
