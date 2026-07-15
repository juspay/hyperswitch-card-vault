#![allow(deprecated)]

use crate::{
    app::TenantAppState,
    error::{self, ContainerError, StorageErrorExt},
    observability::metrics,
    storage::{HashInterface, types::HashTable},
};

/// Insert a hash row for `data_hash`, or return the existing one on a duplicate conflict.
///
/// `add_card` already checks `find_by_data_hash` first; this provides the race-safe insert
/// for the not-found branch (`db.insert` → on duplicate → `db.find`).
pub async fn insert_or_get(
    state: &TenantAppState,
    data_hash: Vec<u8>,
) -> Result<HashTable, ContainerError<error::HashDBError>> {
    match state.db.insert_hash(data_hash.clone()).await {
        Ok(hash) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::HashTable,
                metrics::DomainGetOrInsertOutcome::Created,
            );
            Ok(hash)
        }

        Err(err) if err.get_inner().is_duplicate() => {
            match state.db.find_optional_by_data_hash(&data_hash).await {
                Ok(Some(hash)) => {
                    super::record_get_or_insert_outcome(
                        metrics::Resource::HashTable,
                        metrics::DomainGetOrInsertOutcome::FoundExistingAfterDuplicateInsert,
                    );
                    Ok(hash)
                }
                Ok(None) => {
                    super::record_get_or_insert_outcome(
                        metrics::Resource::HashTable,
                        metrics::DomainGetOrInsertOutcome::Error,
                    );
                    Err(error::HashDBError::DBInsertError.into())
                }
                Err(err) => {
                    super::record_get_or_insert_outcome(
                        metrics::Resource::HashTable,
                        metrics::DomainGetOrInsertOutcome::Error,
                    );
                    Err(err)
                }
            }
        }

        Err(err) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::HashTable,
                metrics::DomainGetOrInsertOutcome::Error,
            );
            Err(err)
        }
    }
}
