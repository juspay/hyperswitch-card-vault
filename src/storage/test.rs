use crate::crypto::Encode;
use std::{collections::HashMap, sync::Arc};

use masking::{ExposeInterface, PeekInterface, Secret};
use tokio::sync::RwLock;

use crate::{
    crypto::{
        aes::{generate_aes256_key, GcmAes256},
        sha::HmacSha512,
    },
    error::{self, ContainerError},
};

use super::{
    consts,
    types::{self, StorageDecryption, StorageEncryption},
    utils, MerchantInterface,
};

type DBTable<Key, Value> = Arc<RwLock<HashMap<Key, Value>>>;

#[derive(Clone, Default)]
pub struct TestStorage {
    // key: tenant_id, merchant_id
    merchant_table: DBTable<(String, String), types::MerchantInner>,
    // key: tenant_id, merchant_id, customer_id, locker_id
    locker_table: DBTable<(String, String, String, String), types::LockerInner>,
    // key: tenant_id, merchant_id, customer_id, hash_id
    locker_htable: DBTable<(String, String, String, String), types::LockerInner>,

    hash_table: DBTable<Vec<u8>, types::HashTable>,

    fingerprint_table: DBTable<Vec<u8>, types::Fingerprint>,
}

#[cfg(feature = "caching")]
impl super::Cacheable<types::Merchant> for TestStorage {
    type Key = (String, String);
    type Value = types::Merchant;
}

#[cfg(feature = "caching")]
impl super::Cacheable<types::HashTable> for TestStorage {
    type Key = Vec<u8>;
    type Value = types::HashTable;
}

#[async_trait::async_trait]
impl MerchantInterface for TestStorage {
    type Algorithm = GcmAes256;
    type Error = error::MerchantDBError;

    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        tenant_id: &str,
        key: &GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        self.merchant_table
            .read()
            .await
            .get(&(tenant_id.to_string(), merchant_id.to_string()))
            .ok_or(error::MerchantDBError::NotFoundError)
            .map_err(From::from)
            .and_then(|inner| Ok(inner.clone().decrypt(key)?))
    }

    async fn find_or_create_by_merchant_id(
        &self,
        merchant_id: &str,
        tenant_id: &str,
        key: &GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let output = self
            .insert_merchant(
                types::MerchantNew {
                    tenant_id,
                    merchant_id,
                    enc_key: generate_aes256_key().to_vec().into(),
                },
                key,
            )
            .await;

        match output {
            Ok(output) => Ok(output),
            Err(_) => self.find_by_merchant_id(merchant_id, tenant_id, key).await,
        }
    }
    async fn insert_merchant(
        &self,
        new: types::MerchantNew<'_>,
        key: &GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let mut table = self.merchant_table.write().await;
        let output = table
            .get(&(new.tenant_id.to_string(), new.merchant_id.to_string()))
            .ok_or(error::MerchantDBError::NotFoundError);

        match output {
            Err(error::MerchantDBError::NotFoundError) => {
                let merchant_inner = new.encrypt(key)?.into_test_inner();
                table.insert(merchant_inner.get_test_key(), merchant_inner.clone());
                Ok(merchant_inner.decrypt(key)?)
            }
            _ => Err(ContainerError::from(error::MerchantDBError::DBInsertError)),
        }
    }
}

#[async_trait::async_trait]
impl super::LockerInterface for TestStorage {
    type Algorithm = GcmAes256;
    type Error = error::LockerDBError;

    async fn find_by_locker_id_merchant_id_customer_id(
        &self,
        locker_id: Secret<String>,
        tenant_id: &str,
        merchant_id: &str,
        customer_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        self.locker_table
            .read()
            .await
            .get(&(
                tenant_id.to_string(),
                merchant_id.to_string(),
                customer_id.to_string(),
                locker_id.expose(),
            ))
            .ok_or(error::LockerDBError::DBFilterError)
            .map_err(From::from)
            .and_then(|inner| Ok(inner.clone().decrypt(key)?))
    }

    async fn find_by_hash_id_merchant_id_customer_id(
        &self,
        hash_id: &str,
        tenant_id: &str,
        merchant_id: &str,
        customer_id: &str,
        key: &Self::Algorithm,
    ) -> Result<Option<types::Locker>, ContainerError<Self::Error>> {
        self.locker_htable
            .read()
            .await
            .get(&(
                tenant_id.to_string(),
                merchant_id.to_string(),
                customer_id.to_string(),
                hash_id.to_string(),
            ))
            .map(|inner| inner.clone().decrypt(key))
            .transpose()
            .map_err(From::from)
    }

    async fn insert_or_get_from_locker(
        &self,
        new: types::LockerNew<'_>,
        key: &Self::Algorithm,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        let mut table = self.locker_table.write().await;
        let mut htable = self.locker_htable.write().await;

        let output = table
            .get(&(
                new.tenant_id.to_string(),
                new.merchant_id.to_string(),
                new.customer_id.to_string(),
                new.locker_id.peek().to_string(),
            ))
            .ok_or(error::LockerDBError::DBFilterError);

        match output {
            Err(_) => {
                let locker_inner = new.encrypt(key)?.into_test_inner();
                table.insert(locker_inner.get_locker_key(), locker_inner.clone());
                htable.insert(locker_inner.get_hash_key(), locker_inner.clone());
                Ok(locker_inner.decrypt(key)?)
            }
            Ok(table) => Ok(table.clone().decrypt(key)?),
        }
    }

    async fn delete_from_locker(
        &self,
        locker_id: Secret<String>,
        tenant_id: &str,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>> {
        let mut table = self.locker_table.write().await;
        let mut htable = self.locker_htable.write().await;

        let output = table.remove(&(
            tenant_id.to_string(),
            merchant_id.to_string(),
            customer_id.to_string(),
            locker_id.expose(),
        ));

        match output {
            Some(inner) => {
                htable.remove(&(
                    tenant_id.to_string(),
                    merchant_id.to_string(),
                    customer_id.to_string(),
                    inner.get_test_hash(),
                ));
                Ok(1)
            }
            None => Ok(0),
        }
    }
}

#[async_trait::async_trait]
impl super::TestInterface for TestStorage {
    type Error = error::TestDBError;

    async fn test(&self) -> Result<(), ContainerError<Self::Error>> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl super::HashInterface for TestStorage {
    type Error = error::HashDBError;

    async fn find_by_data_hash(
        &self,
        data_hash: &[u8],
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>> {
        Ok(self.hash_table.read().await.get(data_hash).cloned())
    }
    async fn insert_hash(
        &self,
        data_hash: Vec<u8>,
    ) -> Result<types::HashTable, ContainerError<Self::Error>> {
        let output = types::HashTable {
            id: 0,
            hash_id: uuid::Uuid::new_v4().to_string(),
            data_hash: data_hash.clone(),
            created_at: time::PrimitiveDateTime::MIN,
        };
        self.hash_table
            .write()
            .await
            .insert(data_hash, output.clone());
        Ok(output)
    }
}

#[async_trait::async_trait]
impl super::FingerprintInterface for TestStorage {
    type Error = error::FingerprintDBError;

    async fn find_by_card_hash(
        &self,
        card_hash: Secret<&[u8]>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>> {
        Ok(self
            .fingerprint_table
            .read()
            .await
            .get(&card_hash.expose().to_vec())
            .cloned())
    }
    async fn insert_fingerprint(
        &self,
        card: types::CardNumber,
        hash_key: Secret<String>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        let algo = HmacSha512::<1>::new(hash_key.expose().into_bytes().into());

        let card_hash = algo.encode(card.into_bytes())?;

        let output = self.find_by_card_hash(Secret::new(&card_hash)).await?;
        match output {
            Some(inner) => Ok(inner),
            None => {
                let fingerprint = types::Fingerprint {
                    id: 0,
                    card_hash: Secret::new(card_hash.clone()),
                    card_fingerprint: utils::generate_id(consts::ID_LENGTH).into(),
                };
                self.fingerprint_table
                    .write()
                    .await
                    .insert(card_hash, fingerprint.clone());
                Ok(fingerprint)
            }
        }
    }
}
