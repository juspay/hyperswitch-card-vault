// #[derive(serde::Serialize, serde::Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct Dedup {
//     hash1: Option<String>,
//     hash2: Option<String>,
//     hash1_reference: Option<String>,
//     hash2_reference: Option<String>,
// }

use masking::{PeekInterface, Secret};

use crate::{error, storage, utils};

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
pub struct Card {
    pub card_number: masking::StrongSecret<String>,
    name_on_card: Option<String>,
    card_exp_month: Option<String>,
    card_exp_year: Option<String>,
    card_brand: Option<String>,
    card_isin: Option<String>,
    nick_name: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct StoreCardRespPayload {
    pub card_reference: String,
    pub duplication_check: Option<DataDuplicationCheck>,
    pub dedup: Option<DedupResponsePayload>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DataDuplicationCheck {
    Duplicated,
    MetaDataChanged,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DedupResponsePayload {
    hash1_reference: Option<String>,
    hash2_reference: Option<String>,
}

// Create Card Data Structures

// prioritizing card data over enc_card_data
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct StoreCardRequest {
    pub merchant_id: String,
    pub merchant_customer_id: String,
    pub requestor_card_reference: Option<String>,
    // pub card: Option<Card>,
    // pub enc_card_data: Option<String>,
    #[serde(flatten)]
    pub data: Data,
    #[serde(default, with = "crate::utils::date_time::optional_iso8601")]
    pub ttl: Option<time::PrimitiveDateTime>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum Data {
    EncData { enc_card_data: String },
    Card { card: Card },
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StoreCardResponse {
    pub status: Status,
    pub payload: Option<StoreCardRespPayload>,
}

// Retrieve Card Data Structures

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveCardRequest {
    pub merchant_id: String,
    pub merchant_customer_id: String,
    pub card_reference: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveCardResponse {
    pub status: Status,
    pub payload: Option<RetrieveCardRespPayload>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveCardRespPayload {
    pub card: Option<Card>,
    pub enc_card_data: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeleteCardRequest {
    pub merchant_id: String,
    pub merchant_customer_id: String,
    pub card_reference: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeleteCardResponse {
    pub status: Status,
}

#[derive(serde::Deserialize)]
pub struct FingerprintRequest {
    pub card: FingerprintCardData,
    pub hash_key: Secret<String>,
}

#[derive(serde::Deserialize)]
pub struct FingerprintCardData {
    pub card_number: storage::types::CardNumber,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct FingerprintResponse {
    pub card_fingerprint: Secret<String>,
}

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum StoredData {
    EncData(String),
    CardData(Card),
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Status {
    Ok,
}

pub trait Validation {
    type Error;

    fn validate(&self) -> Result<(), Self::Error>;
}

impl Validation for StoreCardRequest {
    type Error = error::ApiError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.ttl
            .map(|ttl| -> Result<(), Self::Error> {
                if ttl <= utils::date_time::now() {
                    Err(error::ApiError::InvalidTtl)
                } else {
                    Ok(())
                }
            })
            .transpose()?;

        match &self.data {
            Data::EncData { .. } => Ok(()),
            Data::Card { card } => crate::validations::luhn_on_string(card.card_number.peek())
                .then_some(())
                .ok_or(error::ApiError::ValidationError("card number invalid")),
        }
    }
}

impl Validation for FingerprintRequest {
    type Error = error::ApiError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.card.card_number.validate()
    }
}
