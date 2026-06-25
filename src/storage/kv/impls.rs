//! KV trait implementations for `fingerprint` and `hash_table`.
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
    Fingerprint, FingerprintTableNew, HashTable, HashTableNew, ReverseLookup, ReverseLookupNew,
};

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

// ─── ReverseLookup ──────────────────────────────────────────────────────────

impl EntityType for ReverseLookupNew {
    const ENTITY_TYPE: &'static str = "reverse_lookup";
}

impl KvStorePartition for ReverseLookup {}

impl KvStorePartition for ReverseLookupNew {}

impl UniqueConstraints for ReverseLookup {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(&self.lookup_id)]
    }

    fn table_name(&self) -> &str {
        "reverse_lookup"
    }
}

impl UniqueConstraints for ReverseLookupNew {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(&self.lookup_id)]
    }

    fn table_name(&self) -> &str {
        "reverse_lookup"
    }
}
