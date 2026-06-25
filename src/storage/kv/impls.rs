//! KV trait implementations for `fingerprint`, `hash_table`, `locker`, and `vault`.
//!
//! These impls are only compiled with the `kv` feature.  They provide the
//! `EntityType`, `UniqueConstraints`, and `KvStorePartition` impls that the
//! KV framework needs to route operations to Redis and build drainer stream
//! entries.

use hyperswitch_masking::PeekInterface;

use super::{
    constraints::UniqueConstraints,
    entity::EntityType,
    partition_key::KvStorePartition,
};
use crate::storage::types::{
    Fingerprint, FingerprintTableNew, HashTable, HashTableNew, LockerKvValue,
};
use crate::storage::storage_v2::types::VaultNew;

// ─── Fingerprint ────────────────────────────────────────────────────────────

impl EntityType for FingerprintTableNew {
    const ENTITY_TYPE: &'static str = "fingerprint";
}

impl KvStorePartition for Fingerprint {}

impl KvStorePartition for FingerprintTableNew {}

impl UniqueConstraints for Fingerprint {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(self.fingerprint_hash.peek())]
    }

    fn table_name(&self) -> &str {
        "fingerprint"
    }
}

impl UniqueConstraints for FingerprintTableNew {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(self.fingerprint_hash.peek())]
    }

    fn table_name(&self) -> &str {
        "fingerprint"
    }
}

// ─── HashTable ──────────────────────────────────────────────────────────────

impl EntityType for HashTableNew {
    const ENTITY_TYPE: &'static str = "hash_table";
}

impl KvStorePartition for HashTable {}

impl KvStorePartition for HashTableNew {}

impl UniqueConstraints for HashTable {
    fn unique_constraints(&self) -> Vec<String> {
        vec![self.hash_id.clone()]
    }

    fn table_name(&self) -> &str {
        "hash_table"
    }
}

impl UniqueConstraints for HashTableNew {
    fn unique_constraints(&self) -> Vec<String> {
        vec![self.hash_id.clone()]
    }

    fn table_name(&self) -> &str {
        "hash_table"
    }
}

// ─── Locker ─────────────────────────────────────────────────────────────────

impl EntityType for LockerKvValue {
    const ENTITY_TYPE: &'static str = "locker";
}

impl KvStorePartition for LockerKvValue {}

impl UniqueConstraints for LockerKvValue {
    fn unique_constraints(&self) -> Vec<String> {
        vec![
            format!(
                "locker_id_{}_merchant_id_{}_customer_id_{}",
                self.locker_id.peek(),
                self.merchant_id,
                self.customer_id
            ),
        ]
    }

    fn table_name(&self) -> &str {
        "locker"
    }
}

// ─── Vault ──────────────────────────────────────────────────────────────────

impl EntityType for VaultNew {
    const ENTITY_TYPE: &'static str = "vault";
}

impl KvStorePartition for VaultNew {}

impl UniqueConstraints for VaultNew {
    fn unique_constraints(&self) -> Vec<String> {
        vec![
            format!(
                "vault_id_{}_entity_id_{}",
                self.vault_id.peek(),
                self.entity_id
            ),
        ]
    }

    fn table_name(&self) -> &str {
        "vault"
    }
}
