/// Associates a database type name with an entity type.
///
/// Vendored from `diesel_models/src/kv/entity_type.rs`.
/// Per-table impls are added when a table is integrated into KV.
pub trait EntityType {
    const ENTITY_TYPE: &'static str;
}
