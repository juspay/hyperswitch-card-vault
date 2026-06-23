//! KV trait implementations for `fingerprint` and `hash_table`.
//!
//! These impls are only compiled with the `kv` feature.  They provide the
//! `EntityType`, `UniqueConstraints`, `KvSupportedEntity`, and
//! `KvStorePartition` impls that the KV framework needs to route operations
//! to Redis and build drainer stream entries.

use hyperswitch_masking::PeekInterface;

use super::{
    constraints::UniqueConstraints,
    entity::{EntityType, KvSupportedEntity},
    partition_key::{KvStorePartition, PartitionKey},
};
use crate::storage::types::{Fingerprint, FingerprintTableNew, HashTable, HashTableNew};

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

impl KvSupportedEntity for Fingerprint {
    fn get_partition_key(&self) -> PartitionKey<'_> {
        PartitionKey::Fingerprint {
            fingerprint_hash: self.fingerprint_hash.peek().as_slice(),
        }
    }

    fn get_hash_field_key(&self) -> String {
        format!("fingerprint_{}", hex::encode(self.fingerprint_hash.peek()))
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
        vec![hex::encode(&self.data_hash)]
    }

    fn table_name(&self) -> &str {
        "hash_table"
    }
}

impl UniqueConstraints for HashTableNew {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(&self.data_hash)]
    }

    fn table_name(&self) -> &str {
        "hash_table"
    }
}

impl KvSupportedEntity for HashTable {
    fn get_partition_key(&self) -> PartitionKey<'_> {
        PartitionKey::Hash {
            data_hash: &self.data_hash,
        }
    }

    fn get_hash_field_key(&self) -> String {
        format!("hash_{}", hex::encode(&self.data_hash))
    }
}
