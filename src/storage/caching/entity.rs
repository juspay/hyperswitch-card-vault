use crate::{
    error::{ContainerError, NotFoundError},
    storage::{self, types},
};

impl<T> storage::EntityInterface for super::Caching<T>
where
    T: storage::EntityInterface
        + storage::Cacheable<types::Entity, Key = String, Value = types::Entity>
        + storage::Cacheable<types::Merchant>
        + storage::Cacheable<types::HashTable>
        + storage::Cacheable<types::Fingerprint>
        + Sync
        + Send,
    ContainerError<<T as storage::EntityInterface>::Error>: NotFoundError,
{
    type Error = T::Error;

    async fn find_by_entity_id(
        &self,
        entity_id: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>> {
        let entity_idd = entity_id.to_string();
        let cached_data = self.lookup::<types::Entity>(entity_idd.clone()).await;
        match cached_data {
            Some(value) => Ok(value),
            None => {
                let output = self.inner.find_by_entity_id(entity_id).await?;
                self.cache_data::<types::Entity>(output.entity_id.to_string(), output.clone())
                    .await;
                Ok(output)
            }
        }
    }

    async fn insert_entity(
        &self,
        entity_id: &str,
        identifier: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>> {
        let output = self.inner.insert_entity(entity_id, identifier).await?;
        self.cache_data::<types::Entity>(entity_id.to_string(), output.clone())
            .await;
        Ok(output)
    }
}
