use masking::{ExposeInterface, PeekInterface};

use crate::{
    crypto::Encode,
    error::{self, ContainerError, ResultContainerExt},
    storage,
};

use super::types::{self, DataDuplicationCheck};

impl<'a> TryFrom<(super::types::StoreCardRequest, &'a str, &'a str)>
    for storage::types::LockerNew<'a>
{
    type Error = ContainerError<error::ApiError>;

    fn try_from(
        (value, tenant_id, hash_id): (super::types::StoreCardRequest, &'a str, &'a str),
    ) -> Result<Self, Self::Error> {
        let data = match value.data {
            types::Data::Card { card } => Ok(types::StoredData::CardData(card)),
            types::Data::EncData { enc_card_data } => Ok(types::StoredData::EncData(enc_card_data)),
        }
        .and_then(|inner| {
            serde_json::to_vec(&inner).change_error(error::ApiError::EncodingError)
        })?;

        Ok(Self {
            locker_id: value
                .requestor_card_reference
                .unwrap_or_else(generate_uuid)
                .into(),
            tenant_id,
            merchant_id: value.merchant_id,
            customer_id: value.merchant_customer_id,
            enc_data: data.into(),
            hash_id,
        })
    }
}

impl From<(Option<DataDuplicationCheck>, storage::types::Locker)>
    for super::types::StoreCardResponse
{
    fn from(
        (duplication_check, value): (Option<DataDuplicationCheck>, storage::types::Locker),
    ) -> Self {
        Self {
            status: types::Status::Ok,
            payload: Some(super::types::StoreCardRespPayload {
                card_reference: value.locker_id.expose(),
                duplication_check,
                dedup: None,
            }),
        }
    }
}

impl TryFrom<storage::types::Locker> for super::types::RetrieveCardResponse {
    type Error = ContainerError<error::ApiError>;
    fn try_from(value: storage::types::Locker) -> Result<Self, Self::Error> {
        let (card, enc_card_data) =
            match serde_json::from_slice::<types::StoredData>(&value.enc_data.expose())
                .change_error(error::ApiError::DecodingError)?
            {
                types::StoredData::EncData(data) => (None, Some(data)),
                types::StoredData::CardData(card) => (Some(card), None),
            };

        Ok(Self {
            status: types::Status::Ok,
            payload: Some(super::types::RetrieveCardRespPayload {
                card,
                enc_card_data,
            }),
        })
    }
}

impl From<types::Data> for super::types::StoredData {
    fn from(request_data: types::Data) -> Self {
        match request_data {
            types::Data::EncData { enc_card_data } => Self::EncData(enc_card_data),
            types::Data::Card { card } => Self::CardData(card),
        }
    }
}

/// Generate UUID v4 as strings to be used in storage layer
pub fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn get_hash<T>(
    request: &types::Data,
    hash_algorithm: T,
) -> Result<Vec<u8>, ContainerError<error::ApiError>>
where
    T: Encode<
        Vec<u8>,
        Vec<u8>,
        ReturnType<Vec<u8>> = Result<Vec<u8>, error::ContainerError<error::CryptoError>>,
    >,
{
    let data = match request {
        types::Data::EncData { enc_card_data } => enc_card_data,
        types::Data::Card { card } => card.card_number.peek(),
    };

    let json_data = serde_json::to_vec(data).change_error(error::ApiError::EncodingError)?;

    let hash_data = hash_algorithm.encode(json_data)?;

    Ok(hash_data)
}

pub fn validate_card_metadata(
    stored_payload: &Option<storage::types::Locker>,
    request_data: types::Data,
) -> Result<Option<DataDuplicationCheck>, ContainerError<error::ApiError>> {
    stored_payload
        .as_ref()
        .map(|stored_data| {
            let stored_data =
                serde_json::from_slice::<types::StoredData>(&stored_data.enc_data.clone().expose())
                    .change_error(error::ApiError::DecodingError)?;

            let is_metadata_duplicated = stored_data.eq(&request_data.into());

            Ok(match is_metadata_duplicated {
                true => DataDuplicationCheck::Duplicated,
                false => DataDuplicationCheck::MetaDataChanged,
            })
        })
        .transpose()
}
