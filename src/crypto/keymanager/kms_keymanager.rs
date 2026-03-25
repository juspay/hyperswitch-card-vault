use std::{collections::HashMap, sync::Arc};

use error_stack::ResultExt;
use hyperswitch_masking::{ExposeInterface, PeekInterface, Secret, StrongSecret};

use crate::{
    app::TenantAppState,
    crypto::{
        keymanager::CryptoOperationsManager, secrets_manager::managers::aws_kms::core::AwsKmsClient,
    },
    error::{self, ContainerError},
};

pub struct KmsKeyManager;

#[async_trait::async_trait]
impl super::KeyProvider for KmsKeyManager {
    async fn find_by_entity_id(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>> {
        let kms_client =
            tenant_app_state
                .kms_client
                .as_ref()
                .ok_or(error::ApiError::KeyManagerError(
                    "AWS KMS client not initialized for this tenant",
                ))?;

        Ok(Box::new(KmsCryptoManager {
            client: Arc::clone(kms_client),
            entity_id,
        }))
    }

    async fn find_or_create_entity(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>> {
        // With direct KMS encryption, there is no local key material to create.
        // The KMS key is managed entirely by AWS.
        self.find_by_entity_id(tenant_app_state, entity_id).await
    }
}

struct KmsCryptoManager {
    client: Arc<AwsKmsClient>,
    entity_id: String,
}

impl KmsCryptoManager {
    fn encryption_context(&self) -> HashMap<String, String> {
        HashMap::from([("entity_id".to_string(), self.entity_id.clone())])
    }
}

#[async_trait::async_trait]
impl CryptoOperationsManager for KmsCryptoManager {
    async fn encrypt_data(
        &self,
        _tenant_app_state: &TenantAppState,
        decrypted_data: StrongSecret<Vec<u8>>,
    ) -> Result<Secret<Vec<u8>>, ContainerError<error::ApiError>> {
        self.client
            .encrypt(decrypted_data.peek(), Some(self.encryption_context()))
            .await
            .change_context(error::ApiError::MerchantKeyError)
            .map(Secret::new)
            .map_err(ContainerError::from)
    }

    async fn decrypt_data(
        &self,
        _tenant_app_state: &TenantAppState,
        encrypted_data: Secret<Vec<u8>>,
    ) -> Result<StrongSecret<Vec<u8>>, ContainerError<error::ApiError>> {
        self.client
            .decrypt(&encrypted_data.expose(), Some(self.encryption_context()))
            .await
            .change_context(error::ApiError::MerchantKeyError)
            .map(StrongSecret::new)
            .map_err(ContainerError::from)
    }
}
