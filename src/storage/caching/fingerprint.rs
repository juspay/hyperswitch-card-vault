use masking::{ExposeInterface, PeekInterface, Secret};

use crate::{
    error::ContainerError,
    storage::{self, types},
};

impl<T> storage::FingerprintInterface for super::Caching<T>
where
    T: storage::FingerprintInterface
        + storage::Cacheable<types::Fingerprint, Key = Vec<u8>, Value = types::Fingerprint>
        + storage::Cacheable<types::Merchant>
        + storage::Cacheable<types::HashTable>
        + storage::Cacheable<types::Entity>
        + Sync
        + Send,
{
    type Error = T::Error;

    async fn find_by_fingerprint_hash(
        &self,
        fingerprint_hash: Secret<&[u8]>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>> {
        let key = fingerprint_hash.peek().to_vec();
        let cached_data = self.lookup::<types::Fingerprint>(key.clone()).await;
        match cached_data {
            Some(data) => Ok(Some(data)),
            None => {
                let result = self
                    .inner
                    .find_by_fingerprint_hash(fingerprint_hash)
                    .await?;
                if let Some(ref fingerprint) = result {
                    self.cache_data::<types::Fingerprint>(key, fingerprint.clone())
                        .await;
                }
                Ok(result)
            }
        }
    }

    async fn insert_fingerprint(
        &self,
        data: Secret<String>,
        key: Secret<String>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        let output = self.inner.insert_fingerprint(data, key).await?;
        self.cache_data::<types::Fingerprint>(
            output.fingerprint_hash.clone().expose(),
            output.clone(),
        )
        .await;
        Ok(output)
    }
}
