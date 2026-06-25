#![allow(deprecated)]

use crate::{
    app::TenantAppState,
    error::{self, ContainerError, StorageErrorExt},
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
        Ok(hash) => Ok(hash),
        Err(err) if err.get_inner().is_duplicate() => state
            .db
            .find_optional_by_data_hash(&data_hash)
            .await?
            .ok_or_else(|| error::HashDBError::DBInsertError.into()),
        Err(err) => Err(err),
    }
}
