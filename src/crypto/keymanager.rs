pub mod types;

use error_stack::ResultExt;

use masking::ExposeInterface;
use serde::Deserialize;

use crate::{
    app::TenantAppState,
    crypto::keymanager::types::{
        DataKeyCreateRequest, DataKeyCreateResponse, DataKeyTransferRequest, DecryptDataRequest,
        DecryptDataResponse, EncryptDataRequest, EncryptDateResponse, EncryptedData,
    },
    error::{self, ContainerError, NotFoundError, ResultContainerExt},
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
) -> Result<Entity, ContainerError<error::KeyManagerError>> {
    let entity = state.db.find_by_entity_id(entity_id).await;

    match entity {
        Ok(entity) => Ok(entity),
        Err(inner_err) => match inner_err.is_not_found() {
            true => {
                let url = format!("{}/key/create", state.config.key_manager.url);
                let headers = [("Content-type".into(), "application/json".into())]
                    .into_iter()
                    .collect::<std::collections::HashSet<_>>();

                let response = state
                    .api_client
                    .send_request::<_, DataKeyCreateResponse>(url, headers, request_body)
                    .await
                    .change_context(error::KeyManagerError::RequestSendFailed)?;

                Ok(state
                    .db
                    .insert_entity(entity_id, &response.identifier.get_identifier())
                    .await
                    .change_context(error::KeyManagerError::KeyAddFailed)?)
            }
            false => Err(error::KeyManagerError::KeyAddFailed.into()),
        },
    }
}

pub async fn transfer_key_to_key_manager(
    state: &TenantAppState,
    entity_id: &str,
    request_body: DataKeyTransferRequest,
) -> Result<Entity, ContainerError<error::KeyManagerError>> {
    let url = format!("{}/key/transfer", state.config.key_manager.url);
    let headers = [("Content-type".into(), "application/json".into())]
        .into_iter()
        .collect::<std::collections::HashSet<_>>();

    let response = state
        .api_client
        .send_request::<_, DataKeyCreateResponse>(url, headers, request_body)
        .await
        .change_error(error::KeyManagerError::RequestSendFailed)?;

    state
        .db
        .insert_entity(entity_id, &response.identifier.get_identifier())
        .await
        .change_error(error::KeyManagerError::KeyAddFailed)
}

pub async fn encrypt_data_using_key_manager(
    state: &TenantAppState,
    request_body: EncryptDataRequest,
) -> Result<EncryptedData, ContainerError<error::KeyManagerError>> {
    let url = format!("{}/data/encrypt", state.config.key_manager.url);
    let headers = [("Content-type".into(), "application/json".into())]
        .into_iter()
        .collect::<std::collections::HashSet<_>>();

    let response = state
        .api_client
        .send_request::<_, EncryptDateResponse>(url, headers, request_body)
        .await
        .change_context(error::KeyManagerError::RequestSendFailed)?;

    Ok(response.data)
}

pub async fn decrypt_data_using_key_manager<T>(
    state: &TenantAppState,
    request_body: DecryptDataRequest,
) -> Result<T, ContainerError<error::KeyManagerError>>
where
    T: serde::de::DeserializeOwned,
{
    let url = format!("{}/data/decrypt", state.config.key_manager.url);
    let headers = [("Content-type".into(), "application/json".into())]
        .into_iter()
        .collect::<std::collections::HashSet<_>>();

    let response = state
        .api_client
        .send_request::<_, DecryptDataResponse>(url, headers, request_body)
        .await
        .change_context(error::KeyManagerError::RequestSendFailed)?;

    serde_json::from_slice::<T>(&response.data.inner().expose())
        .change_error(error::KeyManagerError::ResponseDecodingFailed)
}
