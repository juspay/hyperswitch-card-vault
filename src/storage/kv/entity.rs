use super::{constraints::UniqueConstraints, partition_key::PartitionKey};

/// Associates a database type name with an entity type.
///
/// Vendored from `diesel_models/src/kv/entity_type.rs`.
/// Per-table impls are added when a table is integrated into KV.
pub trait EntityType {
    const ENTITY_TYPE: &'static str;
}

/// This trait defines behaviour that must be followed by any table that has
/// support for KV.
///
/// Vendored from `storage_impl/src/lib.rs` (`KvSupportedEntity`).
/// Per-table impls are added when a table is integrated into KV.
pub trait KvSupportedEntity: UniqueConstraints {
    fn get_partition_key(&self) -> PartitionKey<'_>;
    fn get_hash_field_key(&self) -> String;
}
