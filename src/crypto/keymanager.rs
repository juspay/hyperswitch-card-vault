pub mod types;
use masking::ExposeInterface;
use serde::Deserialize;

use crate::{
    app::TenantAppState,
    crypto::keymanager::types::{
        DataDecryptionRequest, DataDecryptionResponse, DataEncryptionRequest, DataKeyCreateRequest,
        DataKeyCreateResponse, DataKeyTransferRequest, DateEncryptionResponse, EncryptedData,
    },
    error::{
        ApiClientError, ContainerError, DataDecryptionError, DataEncryptionError,
        DataKeyCreationError, DataKeyTransferError, KeyManagerError, NotFoundError,
        ResultContainerExt,
    },
    storage::{types::Entity, EntityInterface},
};

#[derive(Debug, Deserialize, Clone)]
pub struct KeyManagerConfig {
    pub url: String,
    // KMS encrypted
    #[cfg(feature = "keymanager_mtls")]
    pub cert: masking::Secret<String>,
}

pub async fn find_or_create_key_in_key_manager(
    state: &TenantAppState,
    entity_id: &str,
    request_body: DataKeyCreateRequest,
) -> Result<Entity, ContainerError<KeyManagerError>> {
    let entity = state.db.find_by_entity_id(entity_id).await;

    match entity {
        Ok(entity) => Ok(entity),
        Err(inner_err) => match inner_err.is_not_found() {
            true => {
                let url = format!("{}/key/create", state.config.key_manager.url);

                let response = call_encryption_service::<
                    _,
                    DataKeyCreateResponse,
                    DataKeyCreationError,
                >(state, url, request_body)
                .await?;

                Ok(state
                    .db
                    .insert_entity(entity_id, &response.identifier.get_identifier())
                    .await?)
            }
            false => Err(KeyManagerError::DbError.into()),
        },
    }
}

/// Method required to transfer the old dek of merchant to key manager.
/// Can be removed after migration of all keys.
pub async fn transfer_key_to_key_manager(
    state: &TenantAppState,
    entity_id: &str,
    request_body: DataKeyTransferRequest,
) -> Result<Entity, ContainerError<KeyManagerError>> {
    let url = format!("{}/key/transfer", state.config.key_manager.url);

    let response = call_encryption_service::<_, DataKeyCreateResponse, DataKeyTransferError>(
        state,
        url,
        request_body,
    )
    .await?;

    let entity = state
        .db
        .insert_entity(entity_id, &response.identifier.get_identifier())
        .await?;

    Ok(entity)
}

pub async fn encrypt_data_using_key_manager(
    state: &TenantAppState,
    request_body: DataEncryptionRequest,
) -> Result<EncryptedData, ContainerError<KeyManagerError>> {
    let url = format!("{}/data/encrypt", state.config.key_manager.url);

    let response = call_encryption_service::<_, DateEncryptionResponse, DataEncryptionError>(
        state,
        url,
        request_body,
    )
    .await?;

    Ok(response.data)
}

pub async fn decrypt_data_using_key_manager<T>(
    state: &TenantAppState,
    request_body: DataDecryptionRequest,
) -> Result<T, ContainerError<KeyManagerError>>
where
    T: serde::de::DeserializeOwned,
{
    let url = format!("{}/data/decrypt", state.config.key_manager.url);

    let response = call_encryption_service::<_, DataDecryptionResponse, DataDecryptionError>(
        state,
        url,
        request_body,
    )
    .await?;

    serde_json::from_slice::<T>(&response.data.inner().expose())
        .change_error(KeyManagerError::ResponseDecodingFailed)
}

pub async fn call_encryption_service<T, R, E>(
    state: &TenantAppState,
    url: String,
    request_body: T,
) -> Result<R, ContainerError<E>>
where
    T: serde::Serialize + Send + Sync + 'static,
    R: serde::de::DeserializeOwned,
    ContainerError<E>: From<ContainerError<ApiClientError>> + Send + Sync,
{
    let headers = [("Content-type".into(), "application/json".into())]
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let response = state
        .api_client
        .send_request::<_, R>(url, headers, request_body)
        .await?;

    Ok(response)
}
