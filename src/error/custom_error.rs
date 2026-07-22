#[derive(Debug, thiserror::Error)]
pub enum MerchantDBError {
    #[error("Error while encrypting DEK before adding to DB")]
    DEKEncryptionError,
    #[error("Error while decrypting DEK from DB")]
    DEKDecryptionError,
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding merchant record in the database")]
    DBFilterError,
    #[error("Error while inserting merchant record in the database")]
    DBInsertError,
    #[error("Merchant record not found in database")]
    NotFoundError,
    #[error("Merchant record already exists in database")]
    Duplicate,
    #[error("Unpredictable error occurred")]
    UnknownError,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultDBError {
    #[error("Error while encrypting data before adding to DB")]
    DataEncryptionError,
    #[error("Error while decrypting data from DB")]
    DataDecryptionError,
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding vault record in the database")]
    DBFilterError,
    #[error("Error while inserting vault record in the database")]
    DBInsertError,
    #[error("Error while deleting vault record in the database")]
    DBDeleteError,
    #[error("Vault record not found in database")]
    NotFoundError,
    #[error("Vault record already exists in database")]
    Duplicate,
    #[error("Unpredictable error occurred")]
    UnknownError,
}

#[derive(Debug, thiserror::Error)]
pub enum HashDBError {
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding hash record in the database")]
    DBFilterError,
    #[error("Error while inserting hash record in the database")]
    DBInsertError,
    #[error("Hash record already exists in database")]
    Duplicate,
    #[error("Unpredictable error occurred")]
    UnknownError,
}

#[derive(Debug, thiserror::Error)]
pub enum TestDBError {
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while writing to database")]
    DBWriteError,
    #[error("Error while reading element in the database")]
    DBReadError,
    #[error("Error while deleting element in the database")]
    DBDeleteError,
    #[error("Unpredictable error occurred")]
    UnknownError,
    #[error("Read replica pool is not configured")]
    DBReplicaNotConfigured,
}

#[derive(Debug, thiserror::Error)]
pub enum FingerprintDBError {
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding fingerprint record in the database")]
    DBFilterError,
    #[error("Error while inserting fingerprint record in the database")]
    DBInsertError,
    #[error("Fingerprint record already exists in database")]
    Duplicate,
    #[error("Unpredictable error occurred")]
    UnknownError,
    #[error("Error while encoding data")]
    EncodingError,
}

#[derive(Debug, thiserror::Error)]
pub enum EntityDBError {
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding entity record in the database")]
    DBFilterError,
    #[error("Error while inserting entity record in the database")]
    DBInsertError,
    #[error("Unpredictable error occurred")]
    UnknownError,
    #[error("Entity record not found in database")]
    NotFoundError,
    #[error("Entity record already exists in database")]
    Duplicate,
}

#[derive(Debug, Copy, Clone, thiserror::Error)]
pub enum ReverseLookupDBError {
    #[error("Error while connecting to database")]
    DBError,
    #[error("Error while finding reverse lookup record in the database")]
    DBFilterError,
    #[error("Error while inserting reverse lookup record in the database")]
    DBInsertError,
    #[error("Reverse lookup record not found in database")]
    NotFoundError,
    #[error("Reverse lookup record already exists in database")]
    Duplicate,
    #[error("Unpredictable error occurred")]
    UnknownError,
}

pub trait NotFoundError {
    fn is_not_found(&self) -> bool;
}

impl NotFoundError for super::ContainerError<MerchantDBError> {
    fn is_not_found(&self) -> bool {
        matches!(self.error.current_context(), MerchantDBError::NotFoundError)
    }
}

impl NotFoundError for super::ContainerError<EntityDBError> {
    fn is_not_found(&self) -> bool {
        matches!(self.error.current_context(), EntityDBError::NotFoundError)
    }
}

impl NotFoundError for super::ContainerError<ReverseLookupDBError> {
    fn is_not_found(&self) -> bool {
        matches!(
            self.error.current_context(),
            ReverseLookupDBError::NotFoundError
        )
    }
}
impl NotFoundError for super::ContainerError<VaultDBError> {
    fn is_not_found(&self) -> bool {
        matches!(self.error.current_context(), VaultDBError::NotFoundError)
    }
}

/// Extension implemented by storage error types so the domain composition helpers
/// (e.g. `insert_or_get`) can detect a duplicate-key conflict without knowing the
/// concrete backend or table.
pub trait StorageErrorExt: Sized {
    /// True if this error represents a duplicate-key / already-exists outcome.
    fn is_duplicate(&self) -> bool;

    /// True if this error represents a not-found outcome.
    fn is_not_found(&self) -> bool;
}

/// Implements [`StorageErrorExt`] and the centralised raw-diesel-error classifier
/// for a table's error type, so its storage-layer query functions stay free of
/// conflict-detection logic: they simply `?`, and the unique-violation /
/// not-found cases surface as the named variants.
///
/// All referenced variants must be unit (data-less) variants.
macro_rules! impl_storage_error {
    ($err:ident, duplicate = $dup:ident, not_found = $nf:ident, other = $other:ident) => {
        impl StorageErrorExt for $err {
            fn is_duplicate(&self) -> bool {
                matches!(self, Self::$dup)
            }

            fn is_not_found(&self) -> bool {
                matches!(self, Self::$nf)
            }
        }

        impl From<diesel::result::Error> for super::ContainerError<$err> {
            #[track_caller]
            fn from(err: diesel::result::Error) -> Self {
                let context = match &err {
                    diesel::result::Error::NotFound => $err::$nf,
                    diesel::result::Error::DatabaseError(
                        diesel::result::DatabaseErrorKind::UniqueViolation,
                        _,
                    ) => $err::$dup,
                    _ => $err::$other,
                };
                Self {
                    error: error_stack::Report::from(err).change_context(context),
                }
            }
        }
    };
}

impl_storage_error!(
    VaultDBError,
    duplicate = Duplicate,
    not_found = NotFoundError,
    other = DBError
);
impl_storage_error!(
    MerchantDBError,
    duplicate = Duplicate,
    not_found = NotFoundError,
    other = DBError
);
impl_storage_error!(
    FingerprintDBError,
    duplicate = Duplicate,
    not_found = DBFilterError,
    other = DBError
);
impl_storage_error!(
    HashDBError,
    duplicate = Duplicate,
    not_found = DBFilterError,
    other = DBError
);
impl_storage_error!(
    EntityDBError,
    duplicate = Duplicate,
    not_found = NotFoundError,
    other = DBError
);
impl_storage_error!(
    ReverseLookupDBError,
    duplicate = Duplicate,
    not_found = NotFoundError,
    other = DBError
);
