use masking::{Secret, StrongSecret};

use crate::{
    error,
    routes::data::types::{SecretDataManager, Ttl, Validation},
    storage::{storage_v2::types::Vault, types::Encryptable},
};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeleteDataRequest {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DeleteDataResponse {
    pub entity_id: String,
    pub vault_id: Secret<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveDataRequest {
    pub entity_id: String,
    pub vault_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RetrieveDataResponse {
    pub data: Secret<serde_json::Value>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct StoreDataRequest {
    pub entity_id: String,
    pub vault_id: String,
    pub data: Secret<serde_json::Value>,
    pub ttl: Ttl,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct StoreDataResponse {
    pub entity_id: String,
    pub vault_id: Secret<String>,
}

impl SecretDataManager for Vault {
    fn get_encrypted_inner_value(&self) -> Option<Secret<Vec<u8>>> {
        self.data.get_encrypted_inner_value()
    }

    fn set_decrypted_data(mut self, decrypted_data: StrongSecret<Vec<u8>>) -> Self {
        self.data = Encryptable::from_decrypted_data(decrypted_data);
        self
    }
}

impl Validation for StoreDataRequest {
    type Error = error::ApiError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.ttl.validate()
    }
}
