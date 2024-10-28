use axum::Json;
use error_stack::ResultExt;
use masking::PeekInterface;
pub mod types;

use crate::{
    crypto::keymanager::{self, KeyProvider},
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError, ResultContainerExt},
    routes::data::crypto_operation,
    storage::storage_v2::VaultInterface,
    utils,
};

pub async fn delete_data(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::DeleteDataRequest>,
) -> Result<Json<types::DeleteDataResponse>, ContainerError<error::ApiError>> {
    let _entity = keymanager::get_dek_manager()
        .find_by_entity_id(&tenant_app_state, request.entity_id.clone())
        .await?;

    let _delete_status = tenant_app_state
        .db
        .delete_from_vault(request.vault_id.clone().into(), &request.entity_id)
        .await?;
    Ok(Json(types::DeleteDataResponse {
        entity_id: request.entity_id,
        vault_id: request.vault_id,
    }))
}

pub async fn retrieve_data(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::RetrieveDataRequest>,
) -> Result<Json<types::RetrieveDataResponse>, ContainerError<error::ApiError>> {
    let crypto_manager = keymanager::get_dek_manager()
        .find_by_entity_id(&tenant_app_state, request.entity_id.clone())
        .await?;

    let mut vault = tenant_app_state
        .db
        .find_by_vault_id_entity_id(request.vault_id.clone().into(), &request.entity_id)
        .await?;

    crypto_operation::decrypt_data(&tenant_app_state, crypto_manager, &mut vault).await?;

    vault
        .expires_at
        .map(|ttl| -> Result<(), error::ApiError> {
            if utils::date_time::now() > ttl {
                tokio::spawn(async move {
                    tenant_app_state
                        .db
                        .delete_from_vault(request.vault_id.into(), &request.entity_id)
                        .await
                });

                Err(error::ApiError::NotFoundError)
            } else {
                Ok(())
            }
        })
        .transpose()?;
    let decrypted_data = vault
        .data
        .get_decrypted_inner_value()
        .ok_or(error::ApiError::UnknownError)
        .attach_printable("Failed to decrypt the stored data")?;
    let decrypted_data_value = serde_json::from_slice(decrypted_data.peek().as_ref())
        .change_error(error::ApiError::DecodingError)?;

    Ok(Json(types::RetrieveDataResponse {
        data: decrypted_data_value,
    }))
}

pub async fn add_data(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::StoreDataRequest>,
) -> Result<Json<types::StoreDataResponse>, ContainerError<error::ApiError>> {
    let crypto_manager = keymanager::get_dek_manager()
        .find_or_create_entity(&tenant_app_state, request.entity_id.clone())
        .await?;

    let insert_data = crypto_operation::encrypt_data_and_insert_into_db_v2(
        &tenant_app_state,
        crypto_manager,
        request,
    )
    .await?;

    Ok(Json(types::StoreDataResponse::from(insert_data)))
}
