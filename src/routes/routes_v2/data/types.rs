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
