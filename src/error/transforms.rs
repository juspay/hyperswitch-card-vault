error_transform!(super::CryptoError => super::MerchantDBError);
impl<'a> From<&'a super::CryptoError> for super::MerchantDBError {
    fn from(value: &'a super::CryptoError) -> Self {
        match value {
            super::CryptoError::SerdeJsonError(_)
            | super::CryptoError::JWError(_)
            | super::CryptoError::InvalidData(_)
            | super::CryptoError::EncodingError(_)
            | super::CryptoError::NotImplemented => Self::UnknownError,
            super::CryptoError::EncryptionError => Self::DEKEncryptionError,
            super::CryptoError::DecryptionError => Self::DEKDecryptionError,
        }
    }
}

error_transform!(super::CryptoError => super::LockerDBError);
impl<'a> From<&'a super::CryptoError> for super::LockerDBError {
    fn from(value: &'a super::CryptoError) -> Self {
        match value {
            super::CryptoError::SerdeJsonError(_)
            | super::CryptoError::JWError(_)
            | super::CryptoError::InvalidData(_)
            | super::CryptoError::EncodingError(_)
            | super::CryptoError::NotImplemented => Self::UnknownError,
            super::CryptoError::EncryptionError => Self::DataEncryptionError,
            super::CryptoError::DecryptionError => Self::DataDecryptionError,
        }
    }
}

error_transform!(super::StorageError => super::MerchantDBError);
impl<'a> From<&'a super::StorageError> for super::MerchantDBError {
    fn from(value: &'a super::StorageError) -> Self {
        match value {
            super::StorageError::DBPoolError | super::StorageError::PoolClientFailure => {
                Self::DBError
            }
            super::StorageError::FindError => Self::DBFilterError,
            super::StorageError::DecryptionError
            | super::StorageError::EncryptionError
            | super::StorageError::DeleteError => Self::UnknownError,
            super::StorageError::InsertError => Self::DBInsertError,
            super::StorageError::NotFoundError => Self::NotFoundError,
        }
    }
}

error_transform!(super::StorageError => super::LockerDBError);
impl<'a> From<&'a super::StorageError> for super::LockerDBError {
    fn from(value: &'a super::StorageError) -> Self {
        match value {
            super::StorageError::DBPoolError | super::StorageError::PoolClientFailure => {
                Self::DBError
            }
            super::StorageError::FindError => Self::DBFilterError,
            super::StorageError::DecryptionError | super::StorageError::EncryptionError => {
                Self::UnknownError
            }
            super::StorageError::InsertError => Self::DBInsertError,
            super::StorageError::DeleteError => Self::DBDeleteError,
            super::StorageError::NotFoundError => Self::NotFoundError,
        }
    }
}

error_transform!(super::StorageError => super::HashDBError);
impl<'a> From<&'a super::StorageError> for super::HashDBError {
    fn from(value: &'a super::StorageError) -> Self {
        match value {
            super::StorageError::DBPoolError | super::StorageError::PoolClientFailure => {
                Self::DBError
            }
            super::StorageError::FindError => Self::DBFilterError,
            super::StorageError::DecryptionError
            | super::StorageError::EncryptionError
            | super::StorageError::DeleteError => Self::UnknownError,
            super::StorageError::InsertError => Self::DBInsertError,
            super::StorageError::NotFoundError => Self::DBFilterError,
        }
    }
}

error_transform!(super::StorageError => super::TestDBError);
impl<'a> From<&'a super::StorageError> for super::TestDBError {
    fn from(value: &'a super::StorageError) -> Self {
        match value {
            super::StorageError::DBPoolError | super::StorageError::PoolClientFailure => {
                Self::DBError
            }
            super::StorageError::FindError => Self::DBReadError,
            super::StorageError::InsertError => Self::DBWriteError,
            super::StorageError::DeleteError => Self::DBDeleteError,
            super::StorageError::DecryptionError
            | super::StorageError::EncryptionError
            | super::StorageError::NotFoundError => Self::UnknownError,
        }
    }
}

error_transform!(super::StorageError => super::FingerprintDBError);
impl<'a> From<&'a super::StorageError> for super::FingerprintDBError {
    fn from(value: &'a super::StorageError) -> Self {
        match value {
            super::StorageError::DBPoolError | super::StorageError::PoolClientFailure => {
                Self::DBError
            }
            super::StorageError::FindError | super::StorageError::NotFoundError => {
                Self::DBFilterError
            }
            super::StorageError::DecryptionError
            | super::StorageError::EncryptionError
            | super::StorageError::DeleteError => Self::UnknownError,
            super::StorageError::InsertError => Self::DBInsertError,
        }
    }
}

error_transform!(super::CryptoError => super::FingerprintDBError);
impl<'a> From<&'a super::CryptoError> for super::FingerprintDBError {
    fn from(value: &'a super::CryptoError) -> Self {
        match value {
            super::CryptoError::SerdeJsonError(_)
            | super::CryptoError::JWError(_)
            | super::CryptoError::InvalidData(_)
            | super::CryptoError::NotImplemented
            | super::CryptoError::EncryptionError
            | super::CryptoError::DecryptionError => Self::UnknownError,
            super::CryptoError::EncodingError(_) => Self::EncodingError,
        }
    }
}

error_transform!(super::FingerprintDBError => super::ApiError);
impl<'a> From<&'a super::FingerprintDBError> for super::ApiError {
    fn from(value: &'a super::FingerprintDBError) -> Self {
        match value {
            super::FingerprintDBError::EncodingError => Self::EncodingError,
            super::FingerprintDBError::DBError => Self::DatabaseError,
            super::FingerprintDBError::DBFilterError => Self::RetrieveDataFailed("fingerprint"),
            super::FingerprintDBError::DBInsertError => Self::DatabaseInsertFailed("fingerprint"),
            super::FingerprintDBError::UnknownError => Self::UnknownError,
        }
    }
}

error_transform!(super::CryptoError => super::HashDBError);
impl<'a> From<&'a super::CryptoError> for super::HashDBError {
    fn from(value: &'a super::CryptoError) -> Self {
        match value {
            super::CryptoError::SerdeJsonError(_)
            | super::CryptoError::JWError(_)
            | super::CryptoError::InvalidData(_)
            | super::CryptoError::EncodingError(_)
            | super::CryptoError::NotImplemented
            | super::CryptoError::EncryptionError
            | super::CryptoError::DecryptionError => Self::UnknownError,
        }
    }
}

// -- API Error --

error_transform!(super::MerchantDBError => super::ApiError);
impl<'a> From<&'a super::MerchantDBError> for super::ApiError {
    fn from(value: &'a super::MerchantDBError) -> Self {
        match value {
            super::MerchantDBError::DEKEncryptionError |
            super::MerchantDBError::DEKDecryptionError | // This failure can also
                                                         // occur because of master key failure
            super::MerchantDBError::DBError |
            super::MerchantDBError::DBFilterError |
            super::MerchantDBError::DBInsertError=> Self::MerchantError,
            super::MerchantDBError::NotFoundError=> Self::NotFoundError,
            super::MerchantDBError::UnknownError => Self::UnknownError
        }
    }
}

error_transform!(super::LockerDBError => super::ApiError);
impl<'a> From<&'a super::LockerDBError> for super::ApiError {
    fn from(value: &'a super::LockerDBError) -> Self {
        match value {
            super::LockerDBError::DataEncryptionError
            | super::LockerDBError::DataDecryptionError => Self::MerchantKeyError,
            super::LockerDBError::DBError => Self::DatabaseError,
            super::LockerDBError::DBFilterError => Self::RetrieveDataFailed("locker"),
            super::LockerDBError::DBInsertError => Self::DatabaseInsertFailed("locker"),
            super::LockerDBError::DBDeleteError => Self::DatabaseDeleteFailed("locker"),
            super::LockerDBError::UnknownError => Self::UnknownError,
            super::LockerDBError::NotFoundError => Self::NotFoundError,
        }
    }
}

error_transform!(super::HashDBError => super::ApiError);
impl<'a> From<&'a super::HashDBError> for super::ApiError {
    fn from(value: &'a super::HashDBError) -> Self {
        match value {
            super::HashDBError::DBError => Self::DatabaseError,
            super::HashDBError::DBFilterError => Self::RetrieveDataFailed("hash table"),
            super::HashDBError::DBInsertError => Self::DatabaseInsertFailed("hash table"),
            super::HashDBError::UnknownError => Self::UnknownError,
        }
    }
}

error_transform!(super::TestDBError => super::ApiError);
impl<'a> From<&'a super::TestDBError> for super::ApiError {
    fn from(value: &'a super::TestDBError) -> Self {
        match value {
            super::TestDBError::DBError => Self::DatabaseError,
            super::TestDBError::UnknownError => Self::UnknownError,
            super::TestDBError::DBWriteError => Self::DatabaseInsertFailed("TestFailed"),
            super::TestDBError::DBReadError => Self::RetrieveDataFailed("Test Failed"),
            super::TestDBError::DBDeleteError => Self::DatabaseDeleteFailed("Test Failed"),
        }
    }
}

error_transform!(super::CryptoError => super::ApiError);
impl<'a> From<&'a super::CryptoError> for super::ApiError {
    fn from(value: &'a super::CryptoError) -> Self {
        match value {
            super::CryptoError::SerdeJsonError(_) => Self::DecodingError,
            super::CryptoError::DecryptionError | super::CryptoError::JWError(_) => {
                Self::RequestMiddlewareError("Failed while encrypting/decrypting")
            }
            super::CryptoError::InvalidData(_) => Self::DecodingError,
            super::CryptoError::EncodingError(_) => Self::EncodingError,
            super::CryptoError::EncryptionError => {
                Self::ResponseMiddlewareError("Failed while encrypting response")
            }
            super::CryptoError::NotImplemented => Self::UnknownError,
        }
    }
}
