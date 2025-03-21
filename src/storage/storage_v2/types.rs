use diesel::{Identifiable, Insertable, Queryable};
use masking::Secret;

use crate::{
    // routes::routes_v2::data::types::StoreDataRequest,
    storage::{
        schema,
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
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = schema::vault)]
pub struct VaultNew {
    pub vault_id: Secret<String>,
    pub entity_id: String,
    pub encrypted_data: Encrypted,
    pub expires_at: Option<time::PrimitiveDateTime>,
}

// impl VaultNew {
//     pub fn new(request: StoreDataRequest, encrypted_data: Encrypted) -> Self {
//         Self {
//             vault_id: request.vault_id.into(),
//             entity_id: request.entity_id,
//             encrypted_data,
//             expires_at: *request.ttl,
//         }
//     }
// }

#[derive(Debug, Identifiable, Queryable)]
#[diesel(table_name = schema::vault)]
pub(super) struct VaultInner {
    id: i32,
    entity_id: String,
    vault_id: Secret<String>,
    encrypted_data: Encrypted,
    created_at: time::PrimitiveDateTime,
    expires_at: Option<time::PrimitiveDateTime>,
}

impl From<VaultInner> for Vault {
    fn from(value: VaultInner) -> Self {
        Self {
            vault_id: value.vault_id,
            entity_id: value.entity_id,
            data: value.encrypted_data.into(),
            created_at: value.created_at,
            expires_at: value.expires_at,
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::vault)]
pub struct VaultNewInner {
    vault_id: Secret<String>,
    entity_id: String,
    encrypted_data: Encrypted,
    expires_at: Option<time::PrimitiveDateTime>,
}
