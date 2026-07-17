use diesel::{ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{ContainerError, HashDBError, kv::KvError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{GetPartitionKey, KvResource},
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

pub(crate) struct HashTablePrimaryKey {
    pub data_hash: Vec<u8>,
}

impl GetPartitionKey for HashTablePrimaryKey {
    fn get_partition_key(&self) -> PartitionKey<'_> {
        PartitionKey::HashTable {
            data_hash: &self.data_hash,
        }
    }
}

impl KvResource for HashTable {
    type Error = HashDBError;

    type DieselNew = HashTableNew;

    type DieselEntity = Self;

    type PrimaryKeyType = HashTablePrimaryKey;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = Some(scheme);
    }

    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, KvError> {
        generate_insert_query::<crate::storage::schema::hash_table::table, _>(new_object.clone())
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

    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<HashDBError>> {
        let mut conn = store.route_conn().await?;
        Ok(Self::table()
            .filter(crate::storage::schema::hash_table::data_hash.eq(&pk.data_hash))
            .get_result::<Self>(&mut conn)
            .await?)
    }
}
