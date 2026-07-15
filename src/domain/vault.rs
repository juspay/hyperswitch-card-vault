use crate::{
    app::TenantAppState,
    error::{self, ContainerError, StorageErrorExt},
    observability::metrics,
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
        Ok(vault) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::Vault,
                metrics::DomainGetOrInsertOutcome::Created,
            );
            Ok(vault)
        }

        Err(err) if err.get_inner().is_duplicate() => match state
            .db
            .find_by_vault_id_entity_id(vault_id, &entity_id)
            .await
        {
            Ok(vault) => {
                super::record_get_or_insert_outcome(
                    metrics::Resource::Vault,
                    metrics::DomainGetOrInsertOutcome::FoundExistingAfterDuplicateInsert,
                );
                Ok(vault)
            }
            Err(err) => {
                super::record_get_or_insert_outcome(
                    metrics::Resource::Vault,
                    metrics::DomainGetOrInsertOutcome::Error,
                );
                Err(err)
            }
        },

        Err(err) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::Vault,
                metrics::DomainGetOrInsertOutcome::Error,
            );
            Err(err)
        }
    }
}

/// Insert the vault row, or overwrite the existing one on a duplicate-key conflict.
pub async fn upsert(
    state: &TenantAppState,
    new: VaultNew,
) -> Result<Vault, ContainerError<error::VaultDBError>> {
    let cloned_new = new.clone();

    match state.db.insert_vault(new).await {
        Ok(vault) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::Vault,
                metrics::DomainGetOrInsertOutcome::Created,
            );
            Ok(vault)
        }

        Err(err) if err.get_inner().is_duplicate() => {
            match state.db.update_vault_data(cloned_new).await {
                Ok(vault) => {
                    super::record_get_or_insert_outcome(
                        metrics::Resource::Vault,
                        metrics::DomainGetOrInsertOutcome::Updated,
                    );
                    Ok(vault)
                }
                Err(err) => {
                    super::record_get_or_insert_outcome(
                        metrics::Resource::Vault,
                        metrics::DomainGetOrInsertOutcome::Error,
                    );
                    Err(err)
                }
            }
        }

        Err(err) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::Vault,
                metrics::DomainGetOrInsertOutcome::Error,
            );
            Err(err)
        }
    }
}
