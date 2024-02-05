use futures_util::TryFutureExt;

use crate::{
    crypto::aes::generate_aes256_key,
    error::{ContainerError, NotFoundError},
    storage::{self, types},
};

#[async_trait::async_trait]
impl<T> storage::MerchantInterface for super::Caching<T, types::Merchant>
where
    T: storage::MerchantInterface
        + storage::Cacheable<types::Merchant, Key = (String, String), Value = types::Merchant>
        + Sync
        + Send,
    ContainerError<<T as storage::MerchantInterface>::Error>: NotFoundError,
{
    type Algorithm = T::Algorithm;
    type Error = T::Error;

    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        tenant_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let cached_data = self
            .lookup((tenant_id.to_string(), merchant_id.to_string()))
            .await;
        match cached_data {
            Some(value) => Ok(value),
            None => {
                let output = self
                    .inner
                    .find_by_merchant_id(merchant_id, tenant_id, key)
                    .await?;
                self.cache_data(
                    (output.tenant_id.to_string(), output.merchant_id.to_string()),
                    output.clone(),
                )
                .await;
                Ok(output)
            }
        }
    }

    async fn find_or_create_by_merchant_id(
        &self,
        merchant_id: &str,
        tenant_id: &str,
        key: &Self::Algorithm,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        self.find_by_merchant_id(merchant_id, tenant_id, key)
            .or_else(|err| async {
                match err.is_not_found() {
                    false => Err(err),
                    true => {
                        self.insert_merchant(
                            types::MerchantNew {
                                merchant_id,
                                tenant_id,
                                enc_key: generate_aes256_key().to_vec().into(),
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
        let tenant_id = new.tenant_id.to_string();
        let output = self.inner.insert_merchant(new, key).await?;
        self.cache_data((tenant_id, merchant_id), output.clone())
            .await;
        Ok(output)
    }
}

#[async_trait::async_trait]
impl<T> storage::HashInterface for super::Caching<T, types::Merchant>
where
    T: storage::HashInterface + storage::Cacheable<types::Merchant> + Sync,
{
    type Error = T::Error;

    async fn find_by_data_hash(
        &self,
        data_hash: &[u8],
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>> {
        self.inner.find_by_data_hash(data_hash).await
    }

    async fn insert_hash(
        &self,
        data_hash: Vec<u8>,
    ) -> Result<types::HashTable, ContainerError<Self::Error>> {
        self.inner.insert_hash(data_hash).await
    }
}
