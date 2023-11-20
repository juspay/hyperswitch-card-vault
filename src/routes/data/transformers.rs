use masking::ExposeInterface;

use crate::{
    error::{self, ContainerError, ResultContainerExt},
    storage,
};

use super::types;

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
                .requester_card_reference
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

impl From<storage::types::Locker> for super::types::StoreCardResponse {
    fn from(value: storage::types::Locker) -> Self {
        Self {
            status: types::Status::Ok,
            payload: Some(super::types::StoreCardRespPayload {
                card_reference: value.locker_id.expose(),
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

/// Generate UUID v4 as strings to be used in storage layer
pub fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
