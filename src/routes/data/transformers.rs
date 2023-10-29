use error_stack::ResultExt;
use masking::ExposeInterface;

use crate::{
    error::{self, LogReport},
    storage,
};

use super::types;

impl TryFrom<(super::types::StoreCardRequest, String, String)> for storage::types::LockerNew {
    type Error = error::ApiError;

    fn try_from(
        (value, tenant_id, hash_id): (super::types::StoreCardRequest, String, String),
    ) -> Result<Self, Self::Error> {
        let data = match value.data {
            types::Data::Card { card } => Ok(types::StoredData::CardData(card)),
            types::Data::EncData { enc_card_data } => Ok(types::StoredData::EncData(enc_card_data)),
        }
        .and_then(|inner| {
            serde_json::to_vec(&inner).change_context(error::ApiError::StoreDataFailed)
        })
        .report_unwrap()?;

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

impl From<storage::types::Locker> for super::types::StoreCardResponse {
    fn from(value: storage::types::Locker) -> Self {
        Self {
            status: "Ok".to_string(),
            error_message: None,
            error_code: None,
            payload: Some(super::types::StoreCardRespPayload {
                card_reference: value.locker_id.expose(),
                dedup: None,
            }),
        }
    }
}

impl TryFrom<storage::types::Locker> for super::types::RetrieveCardResponse {
    type Error = error::ApiError;
    fn try_from(value: storage::types::Locker) -> Result<Self, Self::Error> {
        let (card, enc_card_data) =
            match serde_json::from_slice::<types::StoredData>(&value.enc_data.expose())
                .change_context(error::ApiError::RetrieveDataFailed)
                .report_unwrap()?
            {
                types::StoredData::EncData(data) => (None, Some(data)),
                types::StoredData::CardData(card) => (Some(card), None),
            };

        Ok(Self {
            status: "OK".to_string(),
            error_message: None,
            error_code: None,
            payload: Some(super::types::RetrieveCardRespPayload {
                card,
                enc_card_data,
            }),
        })
    }
}

pub fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
