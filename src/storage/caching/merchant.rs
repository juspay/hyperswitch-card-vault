use crate::{
    error::ContainerError,
    storage::{self, types},
};

impl<T> storage::MerchantInterface for super::Caching<T>
where
    T: storage::MerchantInterface
        + storage::Cacheable<types::Merchant, Key = String, Value = types::Merchant>
        + storage::Cacheable<types::HashTable>
        + storage::Cacheable<types::Fingerprint>
        + super::CacheableWithEntity<T>
        + Sync
        + Send,
{
    type Algorithm = T::Algorithm;
    type Error = T::Error;

    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        match self
            .lookup::<types::Merchant>(merchant_id.to_string())
            .await
        {
            Some(value) => Ok(value),
            None => {
                // A not-found error propagates without being cached.
                let merchant = self.inner.find_by_merchant_id(merchant_id, key).await?;
                self.cache_data::<types::Merchant>(merchant_id.to_string(), merchant.clone())
                    .await;
                Ok(merchant)
            }
        }
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
        self.inner
            .find_all_keys_excluding_entity_keys(key, limit)
            .await
    }
}
