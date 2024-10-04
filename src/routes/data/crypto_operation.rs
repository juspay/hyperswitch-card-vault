use crate::{
    app::TenantAppState,
    crypto::keymanager::CryptoManager,
    error::{self, ContainerError, ResultContainerExt},
    routes::data::types,
    storage::{
        types::{Locker, LockerNew},
        LockerInterface,
    },
};

pub async fn encrypt_data_and_insert_into_db<'a>(
    tenant_app_state: &'a TenantAppState,
    crypto_operator: Box<dyn CryptoManager>,
    request: types::StoreCardRequest,
    hash_id: &'a str,
) -> Result<Locker, ContainerError<error::ApiError>> {
    let data_to_be_encrypted = match request.data.clone() {
        types::Data::Card { card } => Ok(types::StoredData::CardData(card)),
        types::Data::EncData { enc_card_data } => Ok(types::StoredData::EncData(enc_card_data)),
    }
    .and_then(|inner| serde_json::to_vec(&inner).change_error(error::ApiError::EncodingError))?;

    let encrypted_data = crypto_operator
        .encrypt_data(tenant_app_state, data_to_be_encrypted.into())
        .await?;

    let locker_new = LockerNew::new(request, hash_id, encrypted_data.into());

    let locker = tenant_app_state
        .db
        .insert_or_get_from_locker(locker_new)
        .await?;

    Ok(locker)
}

pub async fn decrypt_data(
    tenant_app_state: &TenantAppState,
    crypto_operator: Box<dyn CryptoManager>,
    mut locker: Locker,
) -> Result<Locker, ContainerError<error::ApiError>> {
    let decrypted_data = crypto_operator
        .decrypt_data(tenant_app_state, locker.enc_data)
        .await?;

    locker.enc_data = decrypted_data;

    Ok(locker)
}
