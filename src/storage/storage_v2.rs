use masking::Secret;

use crate::{crypto::encryption_manager::encryption_interface::Encryption, error::ContainerError};

pub mod db;
pub mod types;

///
/// VaultInterface:
///
/// Interface for interacting with the vault database table
pub(crate) trait VaultInterface {
    type Algorithm: Encryption<Vec<u8>, Vec<u8>>;
    type Error;

    /// Fetch payment data from vault table by decrypting with `dek`
    async fn find_by_vault_id_entity_id(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Vault, ContainerError<Self::Error>>;

    /// Insert payment data from vault table by decrypting with `dek`
    async fn insert_or_get_from_vault(
        &self,
        new: types::VaultNew,
        key: &Self::Algorithm,
    ) -> Result<types::Vault, ContainerError<Self::Error>>;

    /// Delete card from the vault, without access to the `dek`
    async fn delete_from_vault(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>>;

    // async fn find_by_hash_id_merchant_id_customer_id(
    //     &self,
    //     entity_id: &str,
    //     key: &Self::Algorithm,
    // ) -> Result<Option<types::Vault>, ContainerError<Self::Error>>;
}
