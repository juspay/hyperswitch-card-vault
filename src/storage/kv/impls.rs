//! Per-table KV trait implementations. Add a module per table.

pub(crate) mod fingerprint;
pub(crate) mod locker;
pub(crate) mod reverse_lookup;

use crate::error::{ReverseLookupDBError, VaultDBError, kv::KvError};

impl From<&KvError> for ReverseLookupDBError {
    fn from(e: &KvError) -> Self {
        match e {
            KvError::DuplicateValue { .. } => Self::Duplicate,
            KvError::ValueNotFound(_) => Self::NotFoundError,
            KvError::Backend | KvError::SerializationFailed => Self::UnknownError,
        }
    }
}

impl From<&KvError> for VaultDBError {
    fn from(e: &KvError) -> Self {
        match e {
            KvError::DuplicateValue { .. } => Self::Duplicate,
            KvError::ValueNotFound(_) => Self::NotFoundError,
            KvError::Backend | KvError::SerializationFailed => Self::UnknownError,
        }
    }
}
