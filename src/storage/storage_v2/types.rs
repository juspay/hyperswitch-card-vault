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

// #[derive(Debug, Clone, Insertable)]
// #[diesel(table_name = schema::vault)]
// pub struct VaultNew {
//     pub vault_id: Secret<String>,
//     pub entity_id: String,
//     pub encrypted_data: Encrypted,
//     pub expires_at: Option<time::PrimitiveDateTime>,
// }

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
