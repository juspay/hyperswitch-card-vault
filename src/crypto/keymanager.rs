pub mod types;
use base64::Engine;
use masking::{ExposeInterface, Secret};
use serde::Deserialize;

use crate::{
    api_client::{ApiResponse, Method},
    app::TenantAppState,
    crypto::keymanager::types::{
        DataDecryptionRequest, DataDecryptionResponse, DataEncryptionRequest, DataKeyCreateRequest,
        DataKeyCreateResponse, DataKeyTransferRequest, DateEncryptionResponse, EncryptedData,
    },
    error::{
        ApiClientError, ContainerError, DataDecryptionError, DataEncryptionError,
        DataKeyCreationError, DataKeyTransferError, KeyManagerError, KeyManagerHealthCheckError,
        NotFoundError, ResultContainerExt,
    },
    routes::health,
    storage::{consts::headers, types::Entity, EntityInterface},
};

use super::consts::BASE64_ENGINE;

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

                let response = call_encryption_service::<_, DataKeyCreationError>(
                    state,
                    url,
                    Method::Post,
                    request_body,
                )
                .await?
                .deserialize_json::<DataKeyCreateResponse, DataKeyCreationError>()
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

    let response =
        call_encryption_service::<_, DataKeyTransferError>(state, url, Method::Post, request_body)
            .await?
            .deserialize_json::<DataKeyCreateResponse, DataKeyTransferError>()
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

    let response =
        call_encryption_service::<_, DataEncryptionError>(state, url, Method::Post, request_body)
            .await?
            .deserialize_json::<DateEncryptionResponse, DataEncryptionError>()
            .await?;

    Ok(response.data)
}

pub async fn decrypt_data_using_key_manager<T>(
    state: &TenantAppState,
    request_body: DataDecryptionRequest,
) -> Result<Secret<T>, ContainerError<KeyManagerError>>
where
    T: serde::de::DeserializeOwned,
{
    let url = format!("{}/data/decrypt", state.config.key_manager.url);

    let response =
        call_encryption_service::<_, DataDecryptionError>(state, url, Method::Post, request_body)
            .await?
            .deserialize_json::<DataDecryptionResponse, DataDecryptionError>()
            .await?;

    serde_json::from_slice::<T>(&response.data.inner().expose())
        .map(Secret::from)
        .change_error(KeyManagerError::ResponseDecodingFailed)
}

pub async fn health_check_keymanager(
    state: &TenantAppState,
) -> Result<health::HealthState, ContainerError<KeyManagerHealthCheckError>> {
    let url = format!("{}/health", state.config.key_manager.url);

    call_encryption_service::<_, KeyManagerHealthCheckError>(state, url, Method::Get, ()).await?;

    Ok(health::HealthState::Working)
}

pub async fn call_encryption_service<T, E>(
    state: &TenantAppState,
    url: String,
    method: Method,
    request_body: T,
) -> Result<ApiResponse, ContainerError<E>>
where
    T: serde::Serialize + Send + Sync + 'static,
    ContainerError<E>: From<ContainerError<ApiClientError>> + Send + Sync,
{
    let broken_master_key = {
        let broken_master_key = &state.config.tenant_secrets.master_key;
        let (left_half, right_half) = broken_master_key.split_at(broken_master_key.len() / 2);
        let hex_left = hex::encode(left_half);
        let hex_right = hex::encode(right_half);
        BASE64_ENGINE.encode(format!("{}:{}", hex_left, hex_right))
    };
    let headers = [
        (headers::CONTENT_TYPE.into(), "application/json".into()),
        (
            headers::AUTHORIZATION.into(),
            format!("Basic {}", broken_master_key).into(),
        ),
    ]
    .into_iter()
    .collect::<std::collections::HashSet<_>>();
    let response = state
        .api_client
        .send_request::<_>(url, headers, method, request_body)
        .await?;

    Ok(response)
}
