//! KV (write-through Redis) framework for card-vault.
//!
//! This module vendors a lean, card-vault-native version of hyperswitch's
//! KV layer.  It reuses the **concepts and the on-the-wire format** of the
//! hyperswitch drainer (`SerializableQuery`, `StreamData`) so that the future
//! drainer can replay entries without modification, but it does **not** import
//! the hyperswitch domain machinery.
//!
//! **Scaffolding only.** No storage table is wired yet.  The per-tenant
//! scheme defaults to [`scheme::StorageScheme::PostgresOnly`], so default
//! builds are unchanged.  Everything here is behind the `kv` feature.

pub mod constraints;
pub mod entity;
pub mod fallback;
#[cfg(feature = "kv")]
pub mod impls;
pub mod metrics;
pub mod partition_key;
pub mod scheme;
pub mod serializable_query;
pub mod wrapper;

pub use constraints::UniqueConstraints;
pub use entity::{EntityType, KvSupportedEntity};
pub use fallback::{find_all_combined_kv_database, try_redis_get_else_try_database_get};
pub use partition_key::{KvStorePartition, PartitionKey};
pub use scheme::{KvTable, Op, StorageScheme, TableKvSettings, decide_storage_scheme};
pub use serializable_query::{DatabaseOperation, SerializableQuery};
pub use wrapper::{
    BridgeRedis, KvOperation, KvResult, KvStoreContext, RedisConnInterface, kv_wrapper,
    push_to_drainer_stream,
};
