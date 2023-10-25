#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dedup {
    hash1: Option<String>,
    hash2: Option<String>,
    hash1_reference: Option<String>,
    hash2_reference: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    card_number: masking::Secret<String>,
    name_on_card: Option<String>,
    card_exp_month: Option<String>,
    card_exp_year: Option<String>,
    card_brand: Option<String>,
    card_isin: Option<String>,
    nick_name: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreCardRespPayload {
    pub card_reference: String,
    pub dedup: Option<DedupResponsePayload>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DedupResponsePayload {
    hash1_reference: Option<String>,
    hash2_reference: Option<String>,
}

// Create Card Data Structures

// prioritizing card data over enc_card_data
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreCardRequest {
    pub merchant_id: String,
    pub merchant_customer_id: String,
    pub requestor_card_reference: Option<String>,
    pub card: Option<Card>,
    pub enc_card_data: Option<String>,
    pub dedup: Option<Dedup>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreCardResponse {
    pub status: String,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub payload: Option<StoreCardRespPayload>,
}

// Retrieve Card Data Structures

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrieveCardRequest {
    pub merchant_id: String,
    pub merchant_customer_id: String,
    pub card_reference: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrieveCardResponse {
    pub status: String,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub payload: Option<RetrieveCardRespPayload>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrieveCardRespPayload {
    pub card: Option<Card>,
    pub enc_card_data: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCardRequest {
    pub merchant_id: String,
    pub merchant_customer_id: String,
    pub card_reference: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCardResponse {
    pub status: String,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum StoredData {
    EncData(String),
    CardData(Card),
}
