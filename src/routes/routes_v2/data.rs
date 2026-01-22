use axum::Json;
use error_stack::ResultExt;
use masking::PeekInterface;
pub mod types;

use crate::{
    crypto::keymanager,
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError, ResultContainerExt},
    logger,
    routes::data::{crypto_operation, types::Validation},
    storage::storage_v2::VaultInterface,
    utils,
};

pub async fn delete_data(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::DeleteDataRequest>,
) -> Result<Json<types::DeleteDataResponse>, ContainerError<error::ApiError>> {
    let _entity = keymanager::get_dek_manager(tenant_app_state.key_manager_mode.is_external())
        .find_by_entity_id(&tenant_app_state, request.entity_id.clone())
        .await?;

    let delete_status = tenant_app_state
        .db
        .delete_from_vault(request.vault_id.clone().into(), &request.entity_id)
        .await?;

    let response = Json(types::DeleteDataResponse {
        entity_id: request.entity_id,
        vault_id: request.vault_id.into(),
    });

    match delete_status {
        0 => logger::info!(delete_data_response = "data not found to delete"),
        _ => logger::info!(delete_data_response=?response, "delete data was successful"),
    }

    Ok(response)
}

pub async fn retrieve_data(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::RetrieveDataRequest>,
) -> Result<Json<types::RetrieveDataResponse>, ContainerError<error::ApiError>> {
    let crypto_manager =
        keymanager::get_dek_manager(tenant_app_state.key_manager_mode.is_external())
            .find_by_entity_id(&tenant_app_state, request.entity_id.clone())
            .await?;

    let vault_data = tenant_app_state
        .db
        .find_by_vault_id_entity_id(request.vault_id.clone().into(), &request.entity_id)
        .await?;

    let decrypted_data =
        crypto_operation::decrypt_data(&tenant_app_state, crypto_manager, vault_data).await?;

    decrypted_data
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
    let decrypted_inner_data = decrypted_data
        .data
        .get_decrypted_inner_value()
        .ok_or(error::ApiError::UnknownError)
        .attach_printable("Failed to decrypt the stored data")?;
    let data_value = serde_json::from_slice(decrypted_inner_data.peek().as_ref())
        .change_error(error::ApiError::DecodingError)?;

    logger::info!(retrieve_data_response = "retrieve data was successful");

    Ok(Json(types::RetrieveDataResponse { data: data_value }))
}

pub async fn add_data(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::StoreDataRequest>,
) -> Result<Json<types::StoreDataResponse>, ContainerError<error::ApiError>> {
    request.validate()?;

    let crypto_manager =
        keymanager::get_dek_manager(tenant_app_state.key_manager_mode.is_external())
            .find_or_create_entity(&tenant_app_state, request.entity_id.clone())
            .await?;

    let insert_data = crypto_operation::encrypt_data_and_insert_into_db_v2(
        &tenant_app_state,
        crypto_manager,
        request,
    )
    .await?;

    let response = Json(types::StoreDataResponse::from(insert_data));
    logger::info!(add_data_response=?response, "add data was successful");

    Ok(response)
}
