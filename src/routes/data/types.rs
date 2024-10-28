use masking::{Secret, StrongSecret};

use crate::{
    error,
    storage::{
        self,
        types::{Encryptable, Locker},
    },
    utils,
};

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Card {
    pub card_number: storage::types::CardNumber,
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
    pub ttl: Ttl,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum Data {
    EncData { enc_card_data: String },
    Card { card: Card },
}

/// The data expires at the specified date and time.
#[derive(Debug, serde::Serialize, Default)]
pub struct Ttl(pub Option<time::PrimitiveDateTime>);

impl<'de> serde::Deserialize<'de> for Ttl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let duration_in_sec = Option::<usize>::deserialize(deserializer)?
            .map(i64::try_from)
            .transpose()
            .map_err(serde::de::Error::custom)?;

        Ok(Self(duration_in_sec.map(|ttl| {
            let current_time = crate::utils::date_time::now();
            current_time.saturating_add(time::Duration::seconds(ttl))
        })))
    }
}

impl std::ops::Deref for Ttl {
    type Target = Option<time::PrimitiveDateTime>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
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
    pub data: Secret<String>,
    pub key: Secret<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct FingerprintResponse {
    pub fingerprint_id: Secret<String>,
}

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug)]
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
            Data::Card { card } => card.card_number.validate(),
        }
    }
}

pub trait SecretDataManager {
    fn get_encrypted_inner_value(&self) -> Option<Secret<Vec<u8>>>;
    fn set_decrypted_data(&mut self, decrypted_data: StrongSecret<Vec<u8>>);
}

impl SecretDataManager for Locker {
    fn get_encrypted_inner_value(&self) -> Option<Secret<Vec<u8>>> {
        self.data.get_encrypted_inner_value()
    }

    fn set_decrypted_data(&mut self, decrypted_data: StrongSecret<Vec<u8>>) {
        self.data = Encryptable::from_decrypted_data(decrypted_data);
    }
}
