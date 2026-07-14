use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{ContainerError, HashDBError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::KvResource,
            serializable_query::{SerializableQuery, generate_insert_query},
        },
        types::{HashTable, HashTableNew},
    },
};

impl EntityType for HashTableNew {
    const ENTITY_TYPE: &'static str = "hash_table";
}

impl EntityType for HashTable {
    const ENTITY_TYPE: &'static str = "hash_table";
}

impl KvStorePartition for HashTable {}

impl KvResource for HashTable {
    type Error = HashDBError;

    type DieselNew = HashTableNew;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = scheme;
    }

    async fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
        store: &Storage,
    ) -> Result<SerializableQuery, ContainerError<HashDBError>> {
        let new_object = new_object.clone();

        store
            .with_sync_conn(move |conn| {
                generate_insert_query::<crate::storage::schema::hash_table::table, _>(
                    conn, new_object,
                )
                .map_err(|report| {
                    let context = HashDBError::from(report.current_context());
                    ContainerError::from(report.change_context(context))
                })
            })
            .await
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self, ContainerError<HashDBError>> {
        let mut conn = store.get_conn().await?;
        Ok(diesel::insert_into(Self::table())
            .values(new_object)
            .get_result(&mut conn)
            .await?)
    }

    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<Self>, ContainerError<HashDBError>> {
        let PartitionKey::HashTable { data_hash } = pk else {
            return Ok(None);
        };

        let mut conn = store.route_conn().await?;
        let output = Self::table()
            .filter(crate::storage::schema::hash_table::data_hash.eq(*data_hash))
            .get_result::<Self>(&mut conn)
            .await
            .optional()?;

        Ok(output)
    }
}
