use hyperswitch_masking::{ExposeInterface, Secret};

use crate::{
    error::ContainerError,
    storage::{self, types},
};

impl<T> storage::HashInterface for super::Caching<T>
where
    T: storage::HashInterface
        + storage::Cacheable<types::HashTable, Key = Vec<u8>, Value = types::HashTable>
        + storage::Cacheable<types::Merchant>
        + storage::Cacheable<types::Fingerprint>
        + super::CacheableWithEntity<T>
        + Sync
        + Send,
{
    type Error = T::Error;

    async fn find_optional_by_data_hash(
        &self,
        data_hash: Secret<Vec<u8>>,
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>> {
        let key = data_hash.clone().expose();
        match self.lookup::<types::HashTable>(key.clone()).await {
            value @ Some(_) => Ok(value),
            None => Ok(
                match self.inner.find_optional_by_data_hash(data_hash).await? {
                    None => None,
                    Some(value) => {
                        self.cache_data::<types::HashTable>(key, value.clone())
                            .await;
                        Some(value)
                    }
                },
            ),
        }
    }

    async fn insert_hash(
        &self,
        data_hash: Secret<Vec<u8>>,
    ) -> Result<types::HashTable, ContainerError<Self::Error>> {
        let key = data_hash.clone().expose();
        let output = self.inner.insert_hash(data_hash).await?;
        self.cache_data::<types::HashTable>(key, output.clone())
            .await;
        Ok(output)
    }
}
