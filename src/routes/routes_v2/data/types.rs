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
    pub status: Status,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Status {
    Ok,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveCardRequest {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveCardResponse {
    pub status: Status,
    pub payload: serde_json::Value,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct StoreCardRequest {
    pub entity_id: String,
    pub vault_id: String,
    pub data: serde_json::Value,
    pub ttl: Ttl,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StoreCardResponse {
    pub entity_id: String,
    pub vault_id: String,
}
