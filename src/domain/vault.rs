use crate::{
    app::TenantAppState,
    error::{self, ContainerError, StorageErrorExt},
    storage::storage_v2::{
        VaultInterface,
        types::{Vault, VaultNew},
    },
};

/// Insert the vault row, or return the existing one on a duplicate-key conflict.
pub async fn get_or_insert(
    state: &TenantAppState,
    new: VaultNew,
) -> Result<Vault, ContainerError<error::VaultDBError>> {
    let vault_id = new.vault_id.clone();
    let entity_id = new.entity_id.clone();

    match state.db.insert_vault(new).await {
        Ok(vault) => Ok(vault),
        Err(err) if err.get_inner().is_duplicate() => {
            state
                .db
                .find_by_vault_id_entity_id(vault_id, &entity_id)
                .await
        }
        Err(err) => Err(err),
    }
}

/// Insert the vault row, or overwrite the existing one on a duplicate-key conflict.
pub async fn upsert(
    state: &TenantAppState,
    new: VaultNew,
) -> Result<Vault, ContainerError<error::VaultDBError>> {
    let cloned_new = new.clone();

    match state.db.insert_vault(new).await {
        Ok(vault) => Ok(vault),
        Err(err) if err.get_inner().is_duplicate() => state.db.update_vault_data(cloned_new).await,
        Err(err) => Err(err),
    }
}
