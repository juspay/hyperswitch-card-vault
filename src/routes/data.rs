use axum::{extract, routing::post, Json};

#[cfg(feature = "limit")]
use axum::{error_handling::HandleErrorLayer, response::IntoResponse};

#[cfg(feature = "middleware")]
use axum::middleware;

use masking::ExposeInterface;

use types::StoreCardResponse;

use crate::{
    app::AppState,
    crypto::{aes::GcmAes256, sha::Sha512},
    error::{self, ContainerError, ResultContainerExt},
    storage::{HashInterface, LockerInterface, MerchantInterface},
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
/// Function for creating the server that is specifically handling the cards api
///
#[allow(clippy::let_and_return)]
pub fn serve(
    #[cfg(any(feature = "middleware", feature = "limit"))] state: AppState,
) -> axum::Router<AppState> {
    #[cfg(feature = "limit")]
    let ratelimit_middleware = tower::ServiceBuilder::new()
        .layer(HandleErrorLayer::new(ratelimit_err_handler))
        .buffer(state.config.limit.buffer_size.unwrap_or(BUFFER_LIMIT))
        .load_shed()
        .rate_limit(
            state.config.limit.request_count,
            std::time::Duration::from_secs(state.config.limit.duration),
        )
        .into_inner();

    #[cfg(feature = "limit")]
    let delete_route = post(delete_card).layer(ratelimit_middleware);

    #[cfg(not(feature = "limit"))]
    let delete_route = post(delete_card);

    let router = axum::Router::new()
        .route("/delete", delete_route)
        .route("/add", post(add_card))
        .route("/retrieve", post(retrieve_card));

    #[cfg(feature = "middleware")]
    {
        router.layer(middleware::from_fn_with_state(
            state,
            custom_middleware::middleware,
        ))
    }
    #[cfg(not(feature = "middleware"))]
    router
}

/// `/data/add` handling the requirement of storing cards
pub async fn add_card(
    extract::State(state): extract::State<AppState>,
    Json(request): Json<types::StoreCardRequest>,
) -> Result<Json<types::StoreCardResponse>, ContainerError<error::ApiError>> {
    request.validate()?;

    let master_encryption = GcmAes256::new(state.config.secrets.master_key);
    let merchant = state
        .db
        .find_or_create_by_merchant_id(
            &request.merchant_id,
            &state.config.secrets.tenant,
            &master_encryption,
        )
        .await?;

    let merchant_dek = GcmAes256::new(merchant.enc_key.expose());

    let hash_data = transformers::get_hash(&request.data, Sha512)
        .change_error(error::ApiError::EncodingError)?;

    let optional_hash_table = state.db.find_by_data_hash(&hash_data).await?;

    let (duplication_check, output) = match optional_hash_table {
        Some(hash_table) => {
            let stored_data = state
                .db
                .find_by_hash_id_merchant_id_customer_id(
                    &hash_table.hash_id,
                    &state.config.secrets.tenant,
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
                    state
                        .db
                        .insert_or_get_from_locker(
                            (
                                request,
                                state.config.secrets.tenant.as_str(),
                                hash_table.hash_id.as_str(),
                            )
                                .try_into()?,
                            &merchant_dek,
                        )
                        .await?
                }
            };

            (duplication_check, output)
        }
        None => {
            let hash_table = state.db.insert_hash(hash_data).await?;

            let output = state
                .db
                .insert_or_get_from_locker(
                    (
                        request,
                        state.config.secrets.tenant.as_str(),
                        hash_table.hash_id.as_str(),
                    )
                        .try_into()?,
                    &merchant_dek,
                )
                .await?;

            (None, output)
        }
    };

    Ok(Json(StoreCardResponse::from((duplication_check, output))))
}

/// `/data/delete` handling the requirement of deleting cards
pub async fn delete_card(
    extract::State(state): extract::State<AppState>,
    Json(request): Json<types::DeleteCardRequest>,
) -> Result<Json<types::DeleteCardResponse>, ContainerError<error::ApiError>> {
    let master_key = GcmAes256::new(state.config.secrets.master_key.clone());

    let _merchant = state
        .db
        .find_by_merchant_id(
            &request.merchant_id,
            &state.config.secrets.tenant,
            &master_key,
        )
        .await?;

    let _delete_status = state
        .db
        .delete_from_locker(
            request.card_reference.into(),
            &state.config.secrets.tenant,
            &request.merchant_id,
            &request.merchant_customer_id,
        )
        .await?;

    Ok(Json(types::DeleteCardResponse {
        status: types::Status::Ok,
    }))
}

/// `/data/retrieve` handling the requirement of retrieving cards
pub async fn retrieve_card(
    extract::State(state): extract::State<AppState>,
    Json(request): Json<types::RetrieveCardRequest>,
) -> Result<Json<types::RetrieveCardResponse>, ContainerError<error::ApiError>> {
    let master_key = GcmAes256::new(state.config.secrets.master_key.clone());

    let merchant = state
        .db
        .find_by_merchant_id(
            &request.merchant_id,
            &state.config.secrets.tenant,
            &master_key,
        )
        .await?;

    let merchant_dek = GcmAes256::new(merchant.enc_key.expose());

    let card = state
        .db
        .find_by_locker_id_merchant_id_customer_id(
            request.card_reference.into(),
            &state.config.secrets.tenant,
            &request.merchant_id,
            &request.merchant_customer_id,
            &merchant_dek,
        )
        .await?;

    Ok(Json(card.try_into()?))
}
