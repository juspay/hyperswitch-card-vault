use masking::Secret;

use crate::routes::data::types::Ttl;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeleteDataRequest {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeleteDataResponse {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveDataRequest {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveDataResponse {
    pub payload: Secret<serde_json::Value>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct StoreDataRequest {
    pub entity_id: String,
    pub vault_id: String,
    pub data: Secret<serde_json::Value>,
    pub ttl: Ttl,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StoreDataResponse {
    pub entity_id: String,
    pub vault_id: String,
}
