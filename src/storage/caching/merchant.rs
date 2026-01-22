use crate::crypto::encryption_manager::managers::aes;
use crate::{
    error::{ContainerError, NotFoundError},
    storage::{self, types},
};
use futures_util::TryFutureExt;

impl<T> storage::MerchantInterface for super::Caching<T>
where
    T: storage::MerchantInterface
        + storage::Cacheable<types::Merchant, Key = String, Value = types::Merchant>
        + storage::Cacheable<types::HashTable>
        + storage::Cacheable<types::Fingerprint>
        + super::CacheableWithEntity<T>
        + Sync
        + Send,
    ContainerError<<T as storage::MerchantInterface>::Error>: NotFoundError,
{
    type Algorithm = T::Algorithm;
    type Error = T::Error;

    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let cached_data = self
            .lookup::<types::Merchant>(merchant_id.to_string())
            .await;
        match cached_data {
            Some(value) => Ok(value),
            None => {
                let output = self.inner.find_by_merchant_id(merchant_id, key).await?;
                self.cache_data::<types::Merchant>(output.merchant_id.to_string(), output.clone())
                    .await;
                Ok(output)
            }
        }
    }

    async fn find_or_create_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        self.find_by_merchant_id(merchant_id, key)
            .or_else(|err| async {
                match err.is_not_found() {
                    false => Err(err),
                    true => {
                        self.insert_merchant(
                            types::MerchantNew {
                                merchant_id,
                                enc_key: aes::generate_aes256_key().to_vec().into(),
                            },
                            key,
                        )
                        .await
                    }
                }
            })
            .await
    }

    async fn insert_merchant(
        &self,
        new: types::MerchantNew<'_>,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let merchant_id = new.merchant_id.to_string();
        let output = self.inner.insert_merchant(new, key).await?;
        self.cache_data::<types::Merchant>(merchant_id, output.clone())
            .await;
        Ok(output)
    }

    async fn find_all_keys_excluding_entity_keys(
        &self,
        key: &Self::Algorithm,
        limit: i64,
    ) -> Result<Vec<types::Merchant>, ContainerError<Self::Error>> {
        let output = self
            .inner
            .find_all_keys_excluding_entity_keys(key, limit)
            .await?;
        Ok(output)
    }
}
