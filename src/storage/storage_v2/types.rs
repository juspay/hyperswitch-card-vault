use diesel::{AsChangeset, Identifiable, Insertable, Queryable};
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
    pub updated_by: StorageScheme,
}

impl VaultNew {
    pub fn new(request: StoreDataRequest, encrypted_data: Encrypted) -> Self {
        Self {
            vault_id: request.vault_id.into(),
            entity_id: request.entity_id,
            encrypted_data,
            created_at: crate::utils::date_time::now(),
            expires_at: *request.ttl,
            updated_by: StorageScheme::PostgresOnly,
        }
    }
}

#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = schema::vault)]
#[diesel(treat_none_as_null = true)]
pub struct VaultUpdate {
    pub encrypted_data: Encrypted,
    pub expires_at: Option<time::PrimitiveDateTime>,
    pub updated_by: StorageScheme,
}

impl From<VaultNew> for VaultUpdate {
    fn from(value: VaultNew) -> Self {
        Self {
            encrypted_data: value.encrypted_data,
            expires_at: value.expires_at,
            updated_by: value.updated_by,
        }
    }
}

#[derive(Debug, Clone, Identifiable, Queryable, serde::Serialize, serde::Deserialize)]
#[diesel(table_name = schema::vault)]
pub(crate) struct VaultInner {
    id: i32,
    entity_id: String,
    vault_id: Secret<String>,
    encrypted_data: Encrypted,
    created_at: time::PrimitiveDateTime,
    expires_at: Option<time::PrimitiveDateTime>,
    updated_by: Option<StorageScheme>,
}

impl VaultInner {
    /// apply the updated fields from VaultUpdate on Vault
    #[cfg(feature = "kv")]
    pub(crate) fn from_update(new: VaultUpdate, current: Self) -> Self {
        let VaultUpdate {
            encrypted_data,
            expires_at,
            updated_by,
        } = new;
        Self {
            id: 0,
            entity_id: current.entity_id,
            vault_id: current.vault_id,
            encrypted_data,
            created_at: current.created_at,
            expires_at,
            updated_by: Some(updated_by),
        }
    }
}

impl From<VaultNew> for VaultNewInner {
    fn from(value: VaultNew) -> Self {
        Self {
            entity_id: value.entity_id,
            vault_id: value.vault_id,
            encrypted_data: value.encrypted_data,
            created_at: value.created_at,
            expires_at: value.expires_at,
            updated_by: Some(value.updated_by),
        }
    }
}

impl From<VaultNewInner> for VaultInner {
    fn from(value: VaultNewInner) -> Self {
        Self {
            id: 0,
            entity_id: value.entity_id,
            vault_id: value.vault_id,
            encrypted_data: value.encrypted_data,
            created_at: value.created_at,
            expires_at: value.expires_at,
            updated_by: value.updated_by,
        }
    }
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

// impl From<VaultNew> for Vault {
//     fn from(value: VaultNew) -> Self {
//         Self {
//             vault_id: value.vault_id,
//             entity_id: value.entity_id,
//             data: value.encrypted_data.into(),
//             created_at: value.created_at,
//             expires_at: value.expires_at,
//             updated_by: Some(value.updated_by),
//         }
//     }
// }

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = schema::vault)]
pub struct VaultNewInner {
    vault_id: Secret<String>,
    entity_id: String,
    encrypted_data: Encrypted,
    created_at: time::PrimitiveDateTime,
    expires_at: Option<time::PrimitiveDateTime>,
    updated_by: Option<StorageScheme>,
}

impl VaultNewInner {
    pub fn set_updated_by(&mut self, updated_by: StorageScheme) {
        self.updated_by = Some(updated_by);
    }
}
