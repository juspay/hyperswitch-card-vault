use diesel::{Identifiable, Insertable, Queryable};
use hyperswitch_masking::Secret;

use crate::{
    routes::routes_v2::data::types::StoreDataRequest,
    storage::{
        schema,
        scheme::StorageScheme,
        types::{Encryptable, Encrypted},
    },
};

#[derive(Debug)]
pub struct Vault {
    pub vault_id: Secret<String>,
    pub entity_id: String,
    pub data: Encryptable,
    pub created_at: time::PrimitiveDateTime,
    pub expires_at: Option<time::PrimitiveDateTime>,
    pub updated_by: Option<StorageScheme>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = schema::vault)]
pub struct VaultNew {
    pub vault_id: Secret<String>,
    pub entity_id: String,
    pub encrypted_data: Encrypted,
    pub created_at: time::PrimitiveDateTime,
    pub expires_at: Option<time::PrimitiveDateTime>,
    pub updated_by: Option<StorageScheme>,
}

impl VaultNew {
    pub fn new(request: StoreDataRequest, encrypted_data: Encrypted) -> Self {
        Self {
            vault_id: request.vault_id.into(),
            entity_id: request.entity_id,
            encrypted_data,
            created_at: crate::utils::date_time::now(),
            expires_at: *request.ttl,
            updated_by: Some(StorageScheme::PostgresOnly),
        }
    }
}

#[derive(Debug, Identifiable, Queryable)]
#[diesel(table_name = schema::vault)]
pub(super) struct VaultInner {
    id: i32,
    entity_id: String,
    vault_id: Secret<String>,
    encrypted_data: Encrypted,
    created_at: time::PrimitiveDateTime,
    expires_at: Option<time::PrimitiveDateTime>,
    pub updated_by: Option<StorageScheme>,
}

impl From<VaultInner> for Vault {
    fn from(value: VaultInner) -> Self {
        Self {
            vault_id: value.vault_id,
            entity_id: value.entity_id,
            data: value.encrypted_data.into(),
            created_at: value.created_at,
            expires_at: value.expires_at,
            updated_by: value.updated_by,
        }
    }
}

impl From<VaultNew> for Vault {
    fn from(value: VaultNew) -> Self {
        Self {
            vault_id: value.vault_id,
            entity_id: value.entity_id,
            data: value.encrypted_data.into(),
            created_at: value.created_at,
            expires_at: value.expires_at,
            updated_by: value.updated_by,
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::vault)]
pub struct VaultNewInner {
    vault_id: Secret<String>,
    entity_id: String,
    encrypted_data: Encrypted,
    created_at: time::PrimitiveDateTime,
    expires_at: Option<time::PrimitiveDateTime>,
    updated_by: Option<StorageScheme>,
}
