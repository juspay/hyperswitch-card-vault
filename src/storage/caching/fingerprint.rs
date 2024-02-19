use masking::{ExposeInterface, PeekInterface, Secret};

use crate::{
    error::ContainerError,
    storage::{self, types},
};

#[async_trait::async_trait]
impl<T> storage::FingerprintInterface for super::Caching<T>
where
    T: storage::FingerprintInterface
        + storage::Cacheable<types::Fingerprint, Key = Vec<u8>, Value = types::Fingerprint>
        + storage::Cacheable<types::Merchant>
        + storage::Cacheable<types::HashTable>
        + Sync
        + Send,
{
    type Error = T::Error;

    async fn find_by_card_hash(
        &self,
        card_hash: Secret<&[u8]>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>> {
        let key = card_hash.peek().to_vec();
        let cached_data = self.lookup::<types::Fingerprint>(&key).await;
        match cached_data {
            Some(data) => Ok(Some(data)),
            None => {
                let result = self.inner.find_by_card_hash(card_hash).await?;
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
        card: types::CardNumber,
        hash_key: Secret<String>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        let output = self.inner.insert_fingerprint(card, hash_key).await?;
        self.cache_data::<types::Fingerprint>(output.card_hash.clone().expose(), output.clone())
            .await;
        Ok(output)
    }
}
