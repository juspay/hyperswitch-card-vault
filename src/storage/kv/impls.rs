//! Per-table KV trait implementations. Add a module per table.

pub(crate) mod fingerprint;
pub(crate) mod reverse_lookup;

use crate::error::{ReverseLookupDBError, kv::KvError};

impl From<&KvError> for ReverseLookupDBError {
    fn from(e: &KvError) -> Self {
        match e {
            KvError::DuplicateValue { .. } => Self::Duplicate,
            KvError::ValueNotFound(_) => Self::NotFoundError,
            KvError::Backend | KvError::SerializationFailed => Self::UnknownError,
        }
    }
}
