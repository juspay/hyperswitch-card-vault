use hyperswitch_masking::{ExposeInterface, Secret};

use crate::{
    app::TenantAppState,
    crypto::hash_manager::{hash_interface::Encode, managers::sha::HmacSha512},
    error::{self, ContainerError, StorageErrorExt},
    storage::{FingerprintInterface, consts, types::Fingerprint, utils},
};

/// Compute the `fingerprint_hash` for `data` (HMAC) and return the stored fingerprint,
/// inserting a new row if none exists. The hash is the canonical dedup key, so an existing
/// row is returned regardless of any caller-supplied `fingerprint_id`.
pub async fn get_or_insert(
    state: &TenantAppState,
    data: Secret<String>,
    key: Secret<String>,
    fingerprint_id: Option<Secret<String>>,
) -> Result<Fingerprint, ContainerError<error::FingerprintDBError>> {
    // The HMAC derivation (the fingerprint-specific envelope) computes the primary key.
    let algo = HmacSha512::<1>::new(key.map(|inner| inner.into_bytes()));
    let fingerprint_hash = algo.encode(data.expose().into_bytes().into())?;

    // Read-first: the hash usually already exists (dedup hot path).
    if let Some(fingerprint) = state
        .db
        .find_optional_by_fingerprint_hash(fingerprint_hash.clone())
        .await?
    {
        return Ok(fingerprint);
    }

    let fingerprint_id =
        fingerprint_id.unwrap_or_else(|| utils::generate_nano_id(consts::ID_LENGTH).into());

    // Cold path: insert, and on a concurrent-insert race re-read the winner row.
    match state
        .db
        .insert_fingerprint(fingerprint_hash.clone(), fingerprint_id)
        .await
    {
        Ok(fingerprint) => Ok(fingerprint),
        Err(err) if err.get_inner().is_duplicate() => state
            .db
            .find_optional_by_fingerprint_hash(fingerprint_hash)
            .await?
            .ok_or_else(|| error::FingerprintDBError::DBInsertError.into()),
        Err(err) => Err(err),
    }
}
