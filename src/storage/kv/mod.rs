//! KV (write-through Redis) framework for card-vault.
//!
//! This module vendors a lean, card-vault-native version of hyperswitch's
//! KV layer.  It reuses the **concepts and the on-the-wire format** of the
//! hyperswitch drainer (`SerializableQuery`, `StreamData`) so that the future
//! drainer can replay entries without modification, but it does **not** import
//! the hyperswitch domain machinery.
//!
//! The live surface is `Get` + `SetNx` on two content-addressed tables,
//! `fingerprint` and `hash_table`, gated behind the `kv` feature.  Per-tenant
//! scheme config (`[tenant_secrets.<tenant>.kv.<table>]`) selects
//! [`scheme::StorageScheme::RedisKv`] (write-through Redis + drainer stream) or
//! [`scheme::StorageScheme::PostgresOnly`] (the default when a table is absent).
//! `config/development.toml` enables `redis_kv` for both tables, and the
//! `release` feature set includes `kv`.

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
pub use entity::EntityType;
pub use fallback::try_redis_get_else_try_database_get;
pub use partition_key::{KvStorePartition, PartitionKey};
pub use scheme::{KvTable, Op, StorageScheme, TableKvSettings, decide_storage_scheme};
pub use serializable_query::{DatabaseOperation, SerializableQuery};
pub use wrapper::{
    BridgeRedis, KvOperation, KvResult, KvStoreContext, RedisConnInterface, kv_wrapper,
    push_to_drainer_stream,
};
