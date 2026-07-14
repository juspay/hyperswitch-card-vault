//! Per-table KV trait implementations. Add a module per table.

pub(crate) mod fingerprint;
pub(crate) mod hash_table;

use crate::error::{HashDBError, kv::KvError};

impl From<&KvError> for HashDBError {
    fn from(e: &KvError) -> Self {
        match e {
            KvError::DuplicateValue { .. } => Self::Duplicate,
            KvError::ValueNotFound(_) => Self::DBFilterError,
            KvError::Backend | KvError::SerializationFailed => Self::UnknownError,
        }
    }
}
