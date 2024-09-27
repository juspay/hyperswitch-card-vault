use axum::Json;
use base64::Engine;
use masking::ExposeInterface;
use serde::{Deserialize, Serialize};

use crate::{
    api_client::Method,
    app::TenantAppState,
    crypto::{
        self,
        consts::BASE64_ENGINE,
        encryption_manager::managers::aes::GcmAes256,
        keymanager::types::{DataKeyCreateResponse, DataKeyTransferRequest, Identifier},
    },
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError, DataKeyTransferError, ResultContainerExt},
    logger,
    storage::{
        consts,
        types::{Entity, Merchant},
        utils, EntityInterface, MerchantInterface,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantKeyTransferRequest {
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferKeyResponse {
    pub total_transferred: usize,
}

pub async fn transfer_keys(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<MerchantKeyTransferRequest>,
) -> Result<Json<TransferKeyResponse>, ContainerError<error::ApiError>> {
    let master_encryption =
        GcmAes256::new(tenant_app_state.config.tenant_secrets.master_key.clone());
    let merchant_keys = tenant_app_state
        .db
        .find_all_keys_excluding_entity_keys(&master_encryption, request.limit)
        .await?;

    logger::debug!("Number of keys to be migrated: {}", merchant_keys.len());

    merchant_keys
        .iter()
        .for_each(|inner| logger::debug!("Migrating merchant: {:?}", inner.merchant_id.clone()));

    let no_of_keys_migrated =
        send_request_to_key_service_for_merchant(&tenant_app_state, merchant_keys).await?;

    Ok(Json(TransferKeyResponse {
        total_transferred: no_of_keys_migrated,
    }))
}

pub async fn send_request_to_key_service_for_merchant(
    state: &TenantAppState,
    keys: Vec<Merchant>,
) -> Result<usize, ContainerError<error::ApiError>> {
    futures::future::try_join_all(keys.into_iter().map(|key| async move {
        let key_encoded = BASE64_ENGINE.encode(key.enc_key.expose());
        let req = DataKeyTransferRequest {
            identifier: Identifier::Entity(utils::generate_id(consts::ID_LENGTH)),
            key: key_encoded,
        };
        migrate_key_to_key_manager(state, &key.merchant_id, req).await
    }))
    .await
    .change_error(error::ApiError::KeyManagerError(
        "Failed while migrating keys",
    ))
    .map(|v| v.len())
}

pub async fn migrate_key_to_key_manager(
    state: &TenantAppState,
    entity_id: &str,
    request_body: DataKeyTransferRequest,
) -> Result<Entity, ContainerError<error::KeyManagerError>> {
    let url = format!("{}/key/transfer", state.config.key_manager.url);

    let response = crypto::keymanager::call_encryption_service::<_, DataKeyTransferError>(
        state,
        url,
        Method::Post,
        Some(request_body),
    )
    .await
    .inspect_err(|err| {
        logger::error!(?err, "Failed to migrate merchant: {}", entity_id);
    })?
    .deserialize_json::<DataKeyCreateResponse, DataKeyTransferError>()
    .await?;

    Ok(state
        .db
        .insert_entity(entity_id, &response.identifier.get_identifier())
        .await
        .inspect_err(|err| {
            logger::error!(?err, "Failed to insert into entity table: {}", entity_id);
        })?)
}
