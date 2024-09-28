use masking::Secret;

use crate::{crypto::encryption_manager::encryption_interface::Encryption, error::ContainerError};

pub mod db;
pub mod types;

///
/// VaultInterface:
///
/// Interface for interacting with the vault database table
#[allow(dead_code)]
pub(crate) trait VaultInterface {
    type Algorithm: Encryption<Vec<u8>, Vec<u8>>;
    type Error;

    /// Fetch data from vault table
    async fn find_by_vault_id_entity_id(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Vault, ContainerError<Self::Error>>;

    /// Insert data from vault table
    async fn insert_or_get_from_vault(
        &self,
        new: types::VaultNew,
        key: &Self::Algorithm,
    ) -> Result<types::Vault, ContainerError<Self::Error>>;

    /// Delete data from the vault
    async fn delete_from_vault(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>>;
}
