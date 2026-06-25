//! KV (write-through Redis) framework for card-vault.
//!
//! This module vendors a lean, card-vault-native version of hyperswitch's
//! KV layer.  It reuses the **concepts and the on-the-wire format** of the
//! hyperswitch drainer (`SerializableQuery`, `StreamData`) so that the future
//! drainer can replay entries without modification, but it does **not** import
//! the hyperswitch domain machinery.
//!
//! The live surface is `Get` + `SetNx` on two content-addressed tables
//! (`fingerprint`, `hash_table`) and `HGet` + `HSetNx` + `HSet` on two
//! composite-keyed tables (`locker`, `vault`), gated behind the `kv` feature.
//! KV enablement is a **global** runtime-config switch (`locker.enable_kv`,
//! resolved via [`crate::storage::KvRuntimeConfig`]) returning
//! [`scheme::StorageScheme::RedisKv`] (write-through Redis + drainer stream)
//! or [`scheme::StorageScheme::PostgresOnly`] (the fail-closed default).

pub(crate) mod constraints;
pub(crate) mod entity;
pub(crate) mod fallback;
#[cfg(feature = "kv")]
pub(crate) mod impls;
pub(crate) mod metrics;
pub(crate) mod partition_key;
pub(crate) mod scheme;
pub(crate) mod serializable_query;
pub(crate) mod wrapper;

pub(crate) use super::scheme::StorageScheme;
pub(crate) use fallback::try_redis_get_else_try_database_get;
pub(crate) use partition_key::{PartitionKey, hash_field_key};
pub(crate) use scheme::{Op, TableKvSettings, decide_storage_scheme};
pub(crate) use wrapper::{KvOperation, KvResult, KvStoreContext, RedisConnInterface, kv_wrapper};
