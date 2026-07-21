use hyperswitch_masking::{ExposeInterface, PeekInterface, Secret, StrongSecret};

use crate::{
    app::TenantAppState,
    crypto::{
        encryption_manager::{encryption_interface::Encryption, managers::aes::GcmAes256},
        keymanager::{CreatedEntity, CryptoOperationsManager},
    },
    domain::merchant,
    error::{self, ContainerError, NotFoundError},
    logger,
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
        let master_encryption = GcmAes256::new(
            tenant_app_state
                .config
                .tenant_secrets
                .master_key
                .clone()
                .expose(),
        );

        let merchant = tenant_app_state
            .db
            .find_by_merchant_id(&entity_id, &master_encryption)
            .await?;

        Ok(Box::new(InternalCryptoManager::from_secret_key(
            merchant.enc_key,
        )))
    }

    async fn find_or_create_entity(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>> {
        let master_encryption = GcmAes256::new(
            tenant_app_state
                .config
                .tenant_secrets
                .master_key
                .clone()
                .expose(),
        );

        // DEPRECATED lazy provisioning: read first so the deprecation signal only fires when the
        // add flow actually has to create the merchant. Clients should call `POST /entity`
        // explicitly; once this warning stops appearing the fallback can be removed and the add
        // flow switched to `find_by_entity_id`.
        let merchant = match tenant_app_state
            .db
            .find_by_merchant_id(&entity_id, &master_encryption)
            .await
        {
            Ok(merchant) => merchant,
            Err(err) if err.is_not_found() => {
                logger::warn!(
                    entity_id = %entity_id,
                    deprecation = "add_flow_auto_create",
                    "merchant auto-created during add flow; clients should call POST /entity explicitly"
                );
                crate::observability::metrics::ENTITY_IMPLICIT_CREATE_COUNT.add(
                    1,
                    crate::metric_attributes!((
                        "key_manager",
                        crate::observability::metrics::KeyManagerKind::Internal
                    )),
                );
                merchant::find_or_create(tenant_app_state, &entity_id, &master_encryption).await?
            }
            Err(err) => return Err(err.into()),
        };

        Ok(Box::new(InternalCryptoManager::from_secret_key(
            merchant.enc_key,
        )))
    }

    async fn create_entity(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<CreatedEntity, ContainerError<error::ApiError>> {
        let master_encryption = GcmAes256::new(
            tenant_app_state
                .config
                .tenant_secrets
                .master_key
                .clone()
                .expose(),
        );

        let merchant =
            merchant::find_or_create(tenant_app_state, &entity_id, &master_encryption).await?;

        Ok(CreatedEntity {
            entity_id: merchant.merchant_id,
            created_at: merchant.created_at,
        })
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
        decryted_data: StrongSecret<Vec<u8>>,
    ) -> Result<Secret<Vec<u8>>, ContainerError<error::ApiError>> {
        Ok(self
            .get_inner()
            .encrypt(decryted_data.peek().clone())?
            .into())
    }
    async fn decrypt_data(
        &self,
        _tenant_app_state: &TenantAppState,
        encrypted_data: Secret<Vec<u8>>,
    ) -> Result<StrongSecret<Vec<u8>>, ContainerError<error::ApiError>> {
        Ok(self.get_inner().decrypt(encrypted_data.expose())?.into())
    }
}
