//! KV (write-through Redis) framework.

pub(crate) mod entity;
#[cfg(feature = "kv")]
pub(crate) mod impls;
pub(crate) mod metrics;
#[cfg(feature = "kv")]
pub(crate) mod partition_key;
#[cfg(feature = "kv")]
pub(crate) mod resource;
pub(crate) mod scheme;
pub(crate) mod serializable_query;
pub(crate) mod wrapper;

#[cfg(feature = "kv")]
pub(crate) use self::{
    partition_key::PartitionKey,
    resource::{
        delete_resource_by_id, find_optional_resource_by_id, find_optional_resource_by_lookup_id,
        find_resource_by_id, insert_resource, insert_resource_with_reverse_lookup,
    },
};
pub(crate) use self::{
    scheme::KvState,
    wrapper::{KvStoreContext, RedisConnInterface},
};
pub(crate) use super::scheme::StorageScheme;
