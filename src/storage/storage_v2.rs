use hyperswitch_masking::Secret;

use crate::error::ContainerError;

pub mod db;
pub mod types;

///
/// VaultInterface:
///
/// Single-query primitives for the vault table. The `get_or_insert` / `upsert`
/// compositions live in the domain layer (`crate::domain::vault`).
pub(crate) trait VaultInterface {
    type Error;

    /// Insert a vault row. A duplicate primary key surfaces as `Error::is_duplicate()`.
    async fn insert_vault(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>>;

    /// Point read by primary key; a missing row surfaces as `Error::is_not_found()`.
    async fn find_by_vault_id_entity_id(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<types::Vault, ContainerError<Self::Error>>;

    /// Overwrite mutable fields for an existing vault row.
    async fn update_vault_data(
        &self,
        vault_id: Secret<String>,
        entity_id: String,
        update: types::VaultUpdate,
    ) -> Result<types::Vault, ContainerError<Self::Error>>;

    /// Delete a vault row by primary key.
    async fn delete_from_vault(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>>;
}
