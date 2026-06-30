//! KV (write-through Redis) framework for card-vault.
//!
//! Live surface: `Get` + `SetNx` on `fingerprint` (content-addressed),
//! gated behind the `kv` feature.  Enablement is a global runtime-config
//! switch (`locker.enable_kv`).

pub(crate) mod constraints;
pub(crate) mod entity;
#[cfg(feature = "kv")]
pub(crate) mod impls;
pub(crate) mod metrics;
pub(crate) mod partition_key;
#[cfg(feature = "kv")]
pub(crate) mod resource;
pub(crate) mod scheme;
pub(crate) mod serializable_query;
pub(crate) mod wrapper;

pub(crate) use partition_key::PartitionKey;
#[cfg(feature = "kv")]
pub(crate) use resource::{find_optional_plain_resource, insert_plain_resource};
pub(crate) use scheme::TableKvSettings;
pub(crate) use wrapper::{KvStoreContext, RedisConnInterface};

pub(crate) use super::scheme::StorageScheme;
