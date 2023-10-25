use axum::{
    extract,
    routing::{get, post},
    Json,
};
use error_stack::ResultExt;
use masking::ExposeInterface;

use crate::{
    app::AppState,
    crypto::aes::GcmAes256,
    error::{self, LogReport},
    storage::{LockerInterface, MerchantInterface},
};

mod transformers;
mod types;

pub fn serve() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/add", post(add_card))
        .route("/delete", post(delete_card))
        .route("/retrieve", get(retrieve_card))
}

pub async fn add_card(
    extract::State(state): extract::State<AppState>,
    Json(request): Json<types::StoreCardRequest>,
) -> Result<Json<types::StoreCardResponse>, error::ApiError> {
    let master_encryption = GcmAes256::new(state.config.secrets.master_key);
    let merchant = state
        .db
        .find_or_create_by_merchant_id(
            request.merchant_id.clone(),
            state.config.secrets.tenant.clone(),
            &master_encryption,
        )
        .await
        .change_context(error::ApiError::StoreDataFailed)
        .report_unwrap()?;
    let merchant_dek = GcmAes256::new(merchant.enc_key.expose());

    let card = state
        .db
        .insert_or_get_from_locker(
            (request, state.config.secrets.tenant).try_into()?,
            &merchant_dek,
        )
        .await
        .change_context(error::ApiError::StoreDataFailed)
        .report_unwrap()?;

    Ok(Json(card.into()))
}

pub async fn delete_card(
    extract::State(state): extract::State<AppState>,
    Json(request): Json<types::DeleteCardRequest>,
) -> Result<Json<types::DeleteCardResponse>, error::ApiError> {
    let master_key = GcmAes256::new(state.config.secrets.master_key.clone());

    let _merchant = state
        .db
        .find_by_merchant_id(
            request.merchant_id.clone(),
            state.config.secrets.tenant.clone(),
            &master_key,
        )
        .await
        .change_context(error::ApiError::DeleteDataFailed)
        .report_unwrap()?;

    let delete_status = state
        .db
        .delete_from_locker(
            request.card_reference.into(),
            state.config.secrets.tenant,
            request.merchant_id,
            request.merchant_customer_id,
        )
        .await
        .change_context(error::ApiError::DeleteDataFailed)
        .report_unwrap()?;

    Ok(Json(types::DeleteCardResponse {
        status: delete_status.to_string(),
        error_message: None,
        error_code: None,
    }))
}

pub async fn retrieve_card(
    extract::State(state): extract::State<AppState>,
    Json(request): Json<types::RetrieveCardRequest>,
) -> Result<Json<types::RetrieveCardResponse>, error::ApiError> {
    let master_key = GcmAes256::new(state.config.secrets.master_key.clone());

    let merchant = state
        .db
        .find_by_merchant_id(
            request.merchant_id.clone(),
            state.config.secrets.tenant.clone(),
            &master_key,
        )
        .await
        .change_context(error::ApiError::DeleteDataFailed)
        .report_unwrap()?;

    let merchant_dek = GcmAes256::new(merchant.enc_key.expose());

    let card = state
        .db
        .find_by_locker_id_merchant_id_customer_id(
            request.card_reference.into(),
            state.config.secrets.tenant.clone(),
            request.merchant_id,
            request.merchant_customer_id,
            &merchant_dek,
        )
        .await
        .change_context(error::ApiError::DeleteDataFailed)
        .report_unwrap()?;

    Ok(Json(card.try_into()?))
}
