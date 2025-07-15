use masking::ExposeInterface;

use crate::{
    app::TenantAppState,
    crypto::keymanager::CryptoOperationsManager,
    error::{self, ContainerError, ResultContainerExt},
    routes::data::types,
    storage::{
        storage_v2::{types::VaultNew, VaultInterface},
        types::{Locker, LockerNew},
        LockerInterface,
    },
};

pub async fn encrypt_data_and_insert_into_db<'a>(
    tenant_app_state: &'a TenantAppState,
    crypto_operator: Box<dyn CryptoOperationsManager>,
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

pub async fn decrypt_data<T>(
    tenant_app_state: &TenantAppState,
    crypto_operator: Box<dyn CryptoOperationsManager>,
    mut data: T,
) -> Result<T, ContainerError<error::ApiError>>
where
    T: types::SecretDataManager,
{
    if let Some(encrypted_data) = data.get_encrypted_inner_value() {
        let decrypted_data = crypto_operator
            .decrypt_data(tenant_app_state, encrypted_data)
            .await?;

        data = data.set_decrypted_data(decrypted_data);
    }
    Ok(data)
}

pub async fn encrypt_data_and_insert_into_db_v2(
    tenant_app_state: &TenantAppState,
    crypto_operator: Box<dyn CryptoOperationsManager>,
    request: crate::routes::routes_v2::data::types::StoreDataRequest,
) -> Result<crate::storage::storage_v2::types::Vault, ContainerError<error::ApiError>> {
    let data_to_be_encrypted = serde_json::to_vec(&request.data.clone().expose())
        .change_error(error::ApiError::EncodingError)?;

    let encrypted_data = crypto_operator
        .encrypt_data(tenant_app_state, data_to_be_encrypted.into())
        .await?;

    let vault_new = VaultNew::new(request, encrypted_data.into());

    let vault = tenant_app_state
        .db
        .insert_or_get_from_vault(vault_new)
        .await?;

    Ok(vault)
}
