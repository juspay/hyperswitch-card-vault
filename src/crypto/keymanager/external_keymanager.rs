pub mod types;
pub mod utils;

use masking::Secret;

pub use crate::crypto::keymanager::ExternalKeyManagerConfig;

use crate::{
    api_client::{ApiResponse, Method},
    app::TenantAppState,
    crypto::keymanager::{
        external_keymanager::{
            self,
            types::{
                DataDecryptionRequest, DataDecryptionResponse, DataEncryptionRequest,
                DataKeyCreateRequest, DataKeyCreateResponse, DataKeyTransferRequest,
                DateEncryptionResponse, DecryptedData, EncryptedData,
            },
        },
        CryptoOperationsManager,
    },
    error::{self, ContainerError, NotFoundError},
    routes::health,
    storage::{types::Entity, EntityInterface},
};
use masking::StrongSecret;

pub async fn create_key_in_key_manager(
    tenant_app_state: &TenantAppState,
    request_body: DataKeyCreateRequest,
) -> Result<DataKeyCreateResponse, ContainerError<error::KeyManagerError>> {
    let url = format!(
        "{}/key/create",
        tenant_app_state
            .config
            .external_key_manager
            .get_url()
            .ok_or_else(|| {
                ContainerError::from(error::KeyManagerError::MissingConfigurationError(
                    "External key manager URL not configured".into(),
                ))
            })?
    );

    let response = call_encryption_service::<_, error::DataKeyCreationError>(
        tenant_app_state,
        url,
        Method::Post,
        request_body,
    )
    .await?
    .deserialize_json::<DataKeyCreateResponse, error::DataKeyCreationError>()
    .await?;

    Ok(response)
}

/// Method required to transfer the old dek of merchant to key manager.
/// Can be removed after migration of alpl keys.
pub async fn transfer_key_to_key_manager(
    tenant_app_state: &TenantAppState,
    request_body: DataKeyTransferRequest,
) -> Result<DataKeyCreateResponse, ContainerError<error::KeyManagerError>> {
    let url = format!(
        "{}/key/transfer",
        tenant_app_state
            .config
            .external_key_manager
            .get_url()
            .ok_or_else(|| {
                ContainerError::from(error::KeyManagerError::MissingConfigurationError(
                    "External key manager URL not configured".into(),
                ))
            })?
    );

    let response = call_encryption_service::<_, error::DataKeyTransferError>(
        tenant_app_state,
        url,
        Method::Post,
        request_body,
    )
    .await?
    .deserialize_json::<DataKeyCreateResponse, error::DataKeyTransferError>()
    .await?;

    Ok(response)
}

pub async fn encrypt_data_using_key_manager(
    tenant_app_state: &TenantAppState,
    request_body: DataEncryptionRequest,
) -> Result<EncryptedData, ContainerError<error::KeyManagerError>> {
    let url = format!(
        "{}/data/encrypt",
        tenant_app_state
            .config
            .external_key_manager
            .get_url()
            .ok_or_else(|| {
                ContainerError::from(error::KeyManagerError::MissingConfigurationError(
                    "External key manager URL not configured".into(),
                ))
            })?
    );

    let response = call_encryption_service::<_, error::DataEncryptionError>(
        tenant_app_state,
        url,
        Method::Post,
        request_body,
    )
    .await?
    .deserialize_json::<DateEncryptionResponse, error::DataEncryptionError>()
    .await?;

    Ok(response.data)
}

pub async fn decrypt_data_using_key_manager(
    tenant_app_state: &TenantAppState,
    request_body: DataDecryptionRequest,
) -> Result<DecryptedData, ContainerError<error::KeyManagerError>> {
    let url = format!(
        "{}/data/decrypt",
        tenant_app_state
            .config
            .external_key_manager
            .get_url()
            .ok_or_else(|| {
                ContainerError::from(error::KeyManagerError::MissingConfigurationError(
                    "External key manager URL not configured".into(),
                ))
            })?
    );

    let response = call_encryption_service::<_, error::DataDecryptionError>(
        tenant_app_state,
        url,
        Method::Post,
        request_body,
    )
    .await?
    .deserialize_json::<DataDecryptionResponse, error::DataDecryptionError>()
    .await?;

    Ok(response.data)
}

pub async fn health_check_keymanager(
    tenant_app_state: &TenantAppState,
) -> Result<health::HealthState, ContainerError<error::KeyManagerHealthCheckError>> {
    let url = format!(
        "{}/health",
        tenant_app_state
            .config
            .external_key_manager
            .get_url()
            .ok_or_else(|| {
                ContainerError::from(
                    error::KeyManagerHealthCheckError::MissingConfigurationError(
                        "External key manager URL not configured".into(),
                    ),
                )
            })?
    );

    call_encryption_service::<_, error::KeyManagerHealthCheckError>(
        tenant_app_state,
        url,
        Method::Get,
        (),
    )
    .await?;

    Ok(health::HealthState::Working)
}

pub async fn call_encryption_service<T, E>(
    tenant_app_state: &TenantAppState,
    url: String,
    method: Method,
    request_body: T,
) -> Result<ApiResponse, ContainerError<E>>
where
    T: serde::Serialize + Send + Sync + 'static,
    ContainerError<E>: From<ContainerError<error::ApiClientError>> + Send + Sync,
{
    let headers = utils::get_key_manager_header(tenant_app_state);

    let response = tenant_app_state
        .api_client
        .send_request::<_>(url, headers, method, request_body)
        .await?;

    Ok(response)
}

#[derive(Clone)]
pub struct ExternalKeyManager;

#[async_trait::async_trait]
impl super::KeyProvider for ExternalKeyManager {
    async fn find_by_entity_id(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>> {
        Ok(tenant_app_state
            .db
            .find_by_entity_id(&entity_id)
            .await
            .map(ExternalCryptoManager::from_entity)
            .map(Box::new)?)
    }

    async fn find_or_create_entity(
        &self,
        tenant_app_state: &TenantAppState,
        entity_id: String,
    ) -> Result<Box<dyn CryptoOperationsManager>, ContainerError<error::ApiError>> {
        let entity = tenant_app_state.db.find_by_entity_id(&entity_id).await;

        let entity = match entity {
            Ok(entity) => Ok(entity),
            Err(inner_err) => match inner_err.is_not_found() {
                true => {
                    let external_keymanager_resp = external_keymanager::create_key_in_key_manager(
                        tenant_app_state,
                        DataKeyCreateRequest::create_request(),
                    )
                    .await?;

                    Ok(tenant_app_state
                        .db
                        .insert_entity(
                            &entity_id,
                            &external_keymanager_resp.identifier.get_identifier(),
                        )
                        .await?)
                }
                false => Err::<_, ContainerError<error::ApiError>>(inner_err.into()),
            },
        };
        Ok(entity
            .map(ExternalCryptoManager::from_entity)
            .map(Box::new)?)
    }
}

pub struct ExternalCryptoManager(Entity);

impl ExternalCryptoManager {
    fn from_entity(entity: Entity) -> Self {
        Self(entity)
    }

    fn get_inner(&self) -> &Entity {
        &self.0
    }
}

#[async_trait::async_trait]
impl super::CryptoOperationsManager for ExternalCryptoManager {
    async fn encrypt_data(
        &self,
        tenant_app_state: &TenantAppState,
        decryted_data: StrongSecret<Vec<u8>>,
    ) -> Result<Secret<Vec<u8>>, ContainerError<error::ApiError>> {
        let encryption_req = DataEncryptionRequest::create_request(
            self.get_inner().enc_key_id.clone(),
            decryted_data,
        )?;
        let encrypted_data = encrypt_data_using_key_manager(tenant_app_state, encryption_req)
            .await?
            .inner();

        Ok(encrypted_data)
    }
    async fn decrypt_data(
        &self,
        tenant_app_state: &TenantAppState,
        encrypted_data: Secret<Vec<u8>>,
    ) -> Result<StrongSecret<Vec<u8>>, ContainerError<error::ApiError>> {
        let decryption_req = DataDecryptionRequest::create_request(
            self.get_inner().enc_key_id.clone(),
            encrypted_data,
        );
        let decrypted_data = decrypt_data_using_key_manager(tenant_app_state, decryption_req)
            .await?
            .inner();

        Ok(decrypted_data)
    }
}
