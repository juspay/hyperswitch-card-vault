use masking::Secret;

use crate::routes::data::types::Ttl;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeleteCardRequest {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeleteCardResponse {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveCardRequest {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveCardResponse {
    pub payload: Secret<serde_json::Value>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct StoreCardRequest {
    pub entity_id: String,
    pub vault_id: String,
    pub data: Secret<serde_json::Value>,
    pub ttl: Ttl,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StoreCardResponse {
    pub entity_id: String,
    pub vault_id: String,
}
