use diesel::{Identifiable, Insertable, Queryable};
use masking::{ExposeInterface, Secret};

use crate::{
    crypto::encryption_manager::{encryption_interface::Encryption, managers::aes::GcmAes256},
    storage::{
        schema,
        types::{Encrypted, StorageDecryption, StorageEncryption},
    },
};

#[derive(Debug)]
pub struct Vault {
    pub vault_id: Secret<String>,
    pub entity_id: String,
    pub encrypted_data: Secret<Vec<u8>>,
    pub created_at: time::PrimitiveDateTime,
    pub expires_at: Option<time::PrimitiveDateTime>,
}

#[derive(Debug, Clone)]
pub struct VaultNew {
    pub vault_id: Secret<String>,
    pub entity_id: String,
    pub encrypted_data: Secret<Vec<u8>>,
    pub expires_at: Option<time::PrimitiveDateTime>,
}

#[derive(Debug, Identifiable, Queryable)]
#[diesel(table_name = schema::vault)]
pub(super) struct VaultInner {
    id: i32,
    vault_id: Secret<String>,
    entity_id: String,
    encrypted_data: Encrypted,
    created_at: time::PrimitiveDateTime,
    expires_at: Option<time::PrimitiveDateTime>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::vault)]
pub struct VaultNewInner {
    vault_id: Secret<String>,
    entity_id: String,
    encrypted_data: Encrypted,
    expires_at: Option<time::PrimitiveDateTime>,
}

impl StorageDecryption for VaultInner {
    type Output = Vault;

    type Algorithm = GcmAes256;

    fn decrypt(
        self,
        algo: &Self::Algorithm,
    ) -> <Self::Algorithm as Encryption<Vec<u8>, Vec<u8>>>::ReturnType<'_, Self::Output> {
        Ok(Self::Output {
            vault_id: self.vault_id,
            entity_id: self.entity_id,
            encrypted_data: algo
                .decrypt(self.encrypted_data.into_inner().expose())?
                .into(),
            created_at: self.created_at,
            expires_at: self.expires_at,
        })
    }
}

impl StorageEncryption for VaultNew {
    type Output = VaultNewInner;

    type Algorithm = GcmAes256;

    fn encrypt(
        self,
        algo: &Self::Algorithm,
    ) -> <Self::Algorithm as Encryption<Vec<u8>, Vec<u8>>>::ReturnType<'_, Self::Output> {
        Ok(Self::Output {
            vault_id: self.vault_id,
            entity_id: self.entity_id,
            encrypted_data: algo.encrypt(self.encrypted_data.expose())?.into(),
            expires_at: self.expires_at,
        })
    }
}
