//! KV (write-through Redis) framework.

pub(crate) mod entity;
#[cfg(feature = "kv")]
pub(crate) mod impls;
pub(crate) mod metrics;
pub(crate) mod partition_key;
#[cfg(feature = "kv")]
pub(crate) mod request_id;
#[cfg(feature = "kv")]
pub(crate) mod resource;
pub(crate) mod scheme;
pub(crate) mod serializable_query;
pub(crate) mod wrapper;

pub(crate) use partition_key::PartitionKey;
#[cfg(feature = "kv")]
pub(crate) use resource::{
    KvDeletable, find_hash_resource, find_optional_plain_resource, find_plain_resource,
    insert_hash_resource, insert_plain_resource, update_hash_resource,
};
pub(crate) use scheme::KvState;
pub(crate) use wrapper::{KvStoreContext, RedisConnInterface};

pub(crate) use super::scheme::StorageScheme;
