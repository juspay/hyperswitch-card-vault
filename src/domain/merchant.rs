#![allow(deprecated)]

use crate::{
    app::TenantAppState,
    crypto::encryption_manager::managers::aes::{self, GcmAes256},
    error::{self, ContainerError, NotFoundError, StorageErrorExt},
    observability::metrics,
    storage::{
        MerchantInterface,
        types::{Merchant, MerchantNew},
    },
};

/// Read the merchant, creating it (with a freshly generated DEK) if absent.
///
/// Read-first to avoid generating a DEK on the hot path; on the cold path the insert is
/// race-safe — a concurrent create surfaces as a duplicate, and we re-read the winner row.
pub async fn find_or_create(
    state: &TenantAppState,
    merchant_id: &str,
    key: &GcmAes256,
) -> Result<Merchant, ContainerError<error::MerchantDBError>> {
    match state.db.find_by_merchant_id(merchant_id, key).await {
        Ok(merchant) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::Merchant,
                metrics::DomainGetOrInsertOutcome::FoundExisting,
            );
            Ok(merchant)
        }

        Err(err) if err.is_not_found() => {
            let new = MerchantNew {
                merchant_id,
                enc_key: aes::generate_aes256_key().to_vec().into(),
            };

            match state.db.insert_merchant(new, key).await {
                Ok(merchant) => {
                    super::record_get_or_insert_outcome(
                        metrics::Resource::Merchant,
                        metrics::DomainGetOrInsertOutcome::Created,
                    );
                    Ok(merchant)
                }

                // Concurrent create won the race — re-read the winner row.
                Err(err) if err.get_inner().is_duplicate() => {
                    match state.db.find_by_merchant_id(merchant_id, key).await {
                        Ok(merchant) => {
                            super::record_get_or_insert_outcome(
                                metrics::Resource::Merchant,
                                metrics::DomainGetOrInsertOutcome::FoundExistingAfterDuplicateInsert,
                            );
                            Ok(merchant)
                        }
                        Err(err) => {
                            super::record_get_or_insert_outcome(
                                metrics::Resource::Merchant,
                                metrics::DomainGetOrInsertOutcome::Error,
                            );
                            Err(err)
                        }
                    }
                }

                Err(err) => {
                    super::record_get_or_insert_outcome(
                        metrics::Resource::Merchant,
                        metrics::DomainGetOrInsertOutcome::Error,
                    );
                    Err(err)
                }
            }
        }

        Err(err) => {
            super::record_get_or_insert_outcome(
                metrics::Resource::Merchant,
                metrics::DomainGetOrInsertOutcome::Error,
            );
            Err(err)
        }
    }
}
