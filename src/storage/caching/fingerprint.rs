use hyperswitch_masking::Secret;

use crate::{
    error::ContainerError,
    storage::{self, types},
};

impl<T> storage::FingerprintInterface for super::Caching<T>
where
    T: storage::FingerprintInterface
        + storage::Cacheable<types::Fingerprint, Key = Secret<Vec<u8>>, Value = types::Fingerprint>
        + storage::Cacheable<types::Merchant>
        + storage::Cacheable<types::HashTable>
        + super::CacheableWithEntity<T>
        + Sync
        + Send,
{
    type Error = T::Error;

    async fn find_optional_by_fingerprint_hash(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>> {
        match self
            .lookup::<types::Fingerprint>(fingerprint_hash.clone())
            .await
        {
            Some(data) => Ok(Some(data)),
            None => {
                let result = self
                    .inner
                    .find_optional_by_fingerprint_hash(fingerprint_hash.clone())
                    .await?;
                if let Some(ref fingerprint) = result {
                    self.cache_data::<types::Fingerprint>(fingerprint_hash, fingerprint.clone())
                        .await;
                }
                Ok(result)
            }
        }
    }

    async fn insert_fingerprint(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
        fingerprint_id: Secret<String>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        let output = self
            .inner
            .insert_fingerprint(fingerprint_hash, fingerprint_id)
            .await?;
        self.cache_data::<types::Fingerprint>(output.fingerprint_hash.clone(), output.clone())
            .await;
        Ok(output)
    }
}
