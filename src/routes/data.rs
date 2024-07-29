use std::sync::Arc;

use axum::{routing::post, Json};

#[cfg(feature = "limit")]
use axum::{error_handling::HandleErrorLayer, response::IntoResponse};

#[cfg(feature = "middleware")]
use axum::middleware;

use masking::ExposeInterface;

use types::StoreCardResponse;

use crate::{
    crypto::{
        encryption_manager::managers::aes::GcmAes256, hash_manager::managers::sha::Sha512,
        keymanager,
    },
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError, NotFoundError, ResultContainerExt},
    storage::{FingerprintInterface, HashInterface, LockerInterface, MerchantInterface},
    tenant::GlobalAppState,
    utils,
};

#[cfg(feature = "middleware")]
use crate::middleware as custom_middleware;

use self::types::Validation;

mod transformers;
pub mod types;

#[cfg(feature = "limit")]
const BUFFER_LIMIT: usize = 1024;
#[cfg(feature = "limit")]
async fn ratelimit_err_handler(_: axum::BoxError) -> impl IntoResponse {
    (hyper::StatusCode::TOO_MANY_REQUESTS, "Rate Limit Applied")
}

///
/// Function for registering routes that is specifically handling the main locker apis
///
#[allow(clippy::let_and_return)]
pub fn serve(
    #[cfg(any(feature = "middleware", feature = "limit"))] global_app_state: Arc<GlobalAppState>,
) -> axum::Router<Arc<GlobalAppState>> {
    #[cfg(feature = "limit")]
    let ratelimit_middleware = tower::ServiceBuilder::new()
        .layer(HandleErrorLayer::new(ratelimit_err_handler))
        .buffer(
            global_app_state
                .global_config
                .limit
                .buffer_size
                .unwrap_or(BUFFER_LIMIT),
        )
        .load_shed()
        .rate_limit(
            global_app_state.global_config.limit.request_count,
            std::time::Duration::from_secs(global_app_state.global_config.limit.duration),
        )
        .into_inner();

    #[cfg(feature = "limit")]
    let delete_route = post(delete_card).layer(ratelimit_middleware);

    #[cfg(not(feature = "limit"))]
    let delete_route = post(delete_card);

    let router = axum::Router::new()
        .route("/delete", delete_route)
        .route("/add", post(add_card))
        .route("/retrieve", post(retrieve_card))
        .route("/fingerprint", post(get_or_insert_fingerprint));

    #[cfg(feature = "middleware")]
    {
        router.layer(middleware::from_fn_with_state(
            global_app_state,
            custom_middleware::middleware,
        ))
    }
    #[cfg(not(feature = "middleware"))]
    router
}

/// `/data/add` handling the requirement of storing data
pub async fn add_card(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::StoreCardRequest>,
) -> Result<Json<types::StoreCardResponse>, ContainerError<error::ApiError>> {
    request.validate()?;

    let master_encryption =
        GcmAes256::new(tenant_app_state.config.tenant_secrets.master_key.clone());

    let is_new_merchant = tenant_app_state
        .db
        .find_by_merchant_id(&request.merchant_id, &master_encryption)
        .await
        .err()
        .map(|err| err.is_not_found())
        .unwrap_or(false);

    let merchant = tenant_app_state
        .db
        .find_or_create_by_merchant_id(&request.merchant_id, &master_encryption)
        .await?;

    if is_new_merchant {
        keymanager::transfer_key_to_key_manager(
            &tenant_app_state,
            &merchant.merchant_id,
            keymanager::types::DataKeyTransferRequest::create_request(
                merchant.enc_key.clone().expose(),
            ),
        )
        .await?;
    }

    let merchant_dek = GcmAes256::new(merchant.enc_key.expose());

    let hash_data = transformers::get_hash(&request.data, Sha512)
        .change_error(error::ApiError::EncodingError)?;

    let optional_hash_table = tenant_app_state.db.find_by_data_hash(&hash_data).await?;

    let (duplication_check, output) = match optional_hash_table {
        Some(hash_table) => {
            let stored_data = tenant_app_state
                .db
                .find_by_hash_id_merchant_id_customer_id(
                    &hash_table.hash_id,
                    &request.merchant_id,
                    &request.merchant_customer_id,
                    &merchant_dek,
                )
                .await?;

            let duplication_check =
                transformers::validate_card_metadata(stored_data.as_ref(), &request.data)?;

            let output = match stored_data {
                Some(data) => data,
                None => {
                    tenant_app_state
                        .db
                        .insert_or_get_from_locker(
                            (request, hash_table.hash_id.as_str()).try_into()?,
                            &merchant_dek,
                        )
                        .await?
                }
            };

            (duplication_check, output)
        }
        None => {
            let hash_table = tenant_app_state.db.insert_hash(hash_data).await?;

            let output = tenant_app_state
                .db
                .insert_or_get_from_locker(
                    (request, hash_table.hash_id.as_str()).try_into()?,
                    &merchant_dek,
                )
                .await?;

            (None, output)
        }
    };

    Ok(Json(StoreCardResponse::from((duplication_check, output))))
}

/// `/data/delete` handling the requirement of deleting data
pub async fn delete_card(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::DeleteCardRequest>,
) -> Result<Json<types::DeleteCardResponse>, ContainerError<error::ApiError>> {
    let master_key = GcmAes256::new(tenant_app_state.config.tenant_secrets.master_key.clone());

    let _merchant = tenant_app_state
        .db
        .find_by_merchant_id(&request.merchant_id, &master_key)
        .await?;

    let _delete_status = tenant_app_state
        .db
        .delete_from_locker(
            request.card_reference.into(),
            &request.merchant_id,
            &request.merchant_customer_id,
        )
        .await?;

    Ok(Json(types::DeleteCardResponse {
        status: types::Status::Ok,
    }))
}

/// `/data/retrieve` handling the requirement of retrieving data
pub async fn retrieve_card(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::RetrieveCardRequest>,
) -> Result<Json<types::RetrieveCardResponse>, ContainerError<error::ApiError>> {
    let master_key = GcmAes256::new(tenant_app_state.config.tenant_secrets.master_key.clone());

    let merchant = tenant_app_state
        .db
        .find_by_merchant_id(&request.merchant_id, &master_key)
        .await?;

    let merchant_dek = GcmAes256::new(merchant.enc_key.expose());

    let card = tenant_app_state
        .db
        .find_by_locker_id_merchant_id_customer_id(
            request.card_reference.clone().into(),
            &request.merchant_id,
            &request.merchant_customer_id,
            &merchant_dek,
        )
        .await?;

    card.ttl
        .map(|ttl| -> Result<(), error::ApiError> {
            if utils::date_time::now() > ttl {
                tokio::spawn(async move {
                    tenant_app_state
                        .db
                        .delete_from_locker(
                            request.card_reference.into(),
                            &request.merchant_id,
                            &request.merchant_customer_id,
                        )
                        .await
                });

                Err(error::ApiError::NotFoundError)
            } else {
                Ok(())
            }
        })
        .transpose()?;

    Ok(Json(card.try_into()?))
}

/// `/cards/fingerprint` handling the creation and retrieval of card fingerprint
pub async fn get_or_insert_fingerprint(
    TenantStateResolver(tenant_app_state): TenantStateResolver,
    Json(request): Json<types::FingerprintRequest>,
) -> Result<Json<types::FingerprintResponse>, ContainerError<error::ApiError>> {
    request.validate()?;

    let fingerprint = tenant_app_state
        .db
        .insert_fingerprint(request.card.card_number, request.hash_key)
        .await?;

    Ok(Json(fingerprint.into()))
}
