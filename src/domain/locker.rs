use crate::{
    app::TenantAppState,
    error::{self, ContainerError, StorageErrorExt},
    observability::metrics,
    storage::{
        LockerInterface,
        types::{Locker, LockerNew},
    },
};

/// Insert the locker row, or return the existing one on a duplicate-key conflict.
///
/// This is the composition that used to live inside the storage layer as
/// `insert_or_get_from_locker`: `db.insert` → on duplicate → `db.get`.
pub async fn get_or_insert(
    state: &TenantAppState,
    new: LockerNew,
) -> Result<Locker, ContainerError<error::VaultDBError>> {
    let locker_id = new.locker_id.clone();
    let merchant_id = new.merchant_id.clone();
    let customer_id = new.customer_id.clone();

    match state.db.insert_locker(new).await {
        Ok(locker) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::Locker,
                metrics::DomainGetOrInsertOutcome::Created,
            );
            Ok(locker)
        }

        Err(err) if err.get_inner().is_duplicate() => match state
            .db
            .find_by_locker_id_merchant_id_customer_id(locker_id, &merchant_id, &customer_id)
            .await
        {
            Ok(locker) => {
                super::record_get_or_insert_outcome(
                    metrics::Resource::Locker,
                    metrics::DomainGetOrInsertOutcome::FoundExistingAfterDuplicateInsert,
                );
                Ok(locker)
            }
            Err(err) => {
                super::record_get_or_insert_outcome(
                    metrics::Resource::Locker,
                    metrics::DomainGetOrInsertOutcome::Error,
                );
                Err(err)
            }
        },

        Err(err) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::Locker,
                metrics::DomainGetOrInsertOutcome::Error,
            );
            Err(err)
        }
    }
}
