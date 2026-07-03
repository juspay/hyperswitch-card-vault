//! KV trait impls for the hash_table table.

use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{HashDBError, KvError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{KvFindOptional, KvWriteError, PlainKeyed, StorageResource},
            serializable_query::generate_insert_query,
        },
        types::{HashTable, HashTableNew},
    },
};

impl EntityType for HashTableNew {
    const ENTITY_TYPE: &'static str = "hash_table";
}

impl KvStorePartition for HashTableNew {}

impl PlainKeyed for HashTableNew {}

impl StorageResource for HashTableNew {
    type Domain = HashTable;
    type Error = HashDBError;

    fn into_domain(self) -> Self::Domain {
        Self::Domain {
            id: 0,
            hash_id: self.hash_id,
            data_hash: self.data_hash,
            created_at: time::PrimitiveDateTime::MIN,
            updated_by: self.updated_by,
        }
    }

    fn set_storage_scheme(&mut self, scheme: StorageScheme) {
        self.updated_by = scheme;
    }

    fn insert_drainer_query(
        &self,
    ) -> error_stack::Result<crate::storage::kv::serializable_query::SerializableQuery, KvError>
    {
        generate_insert_query::<crate::storage::schema::hash_table::table, _>(self.clone())
    }

    async fn storage_insert(
        self,
        store: &Storage,
    ) -> Result<HashTable, crate::error::ContainerError<HashDBError>> {
        let mut conn = store.get_conn().await?;
        let output: HashTable = diesel::insert_into(HashTable::table())
            .values(self)
            .get_result(&mut conn)
            .await?;
        Ok(output)
    }
}

impl KvFindOptional for HashTableNew {
    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<HashTable>, crate::error::ContainerError<HashDBError>> {
        let PartitionKey::Hash { data_hash } = pk else {
            return Err(HashDBError::from(&KvError::Backend).into());
        };
        let mut conn = store.get_conn().await?;
        let output = HashTable::table()
            .filter(crate::storage::schema::hash_table::data_hash.eq(*data_hash))
            .get_result::<HashTable>(&mut conn)
            .await
            .optional()?;
        Ok(output)
    }
}

impl From<&KvError> for HashDBError {
    fn from(e: &KvError) -> Self {
        match e {
            KvError::DuplicateValue { .. } => Self::Duplicate,
            KvError::ValueNotFound(_) => Self::DBFilterError,
            KvError::Backend | KvError::SerializationFailed => Self::UnknownError,
        }
    }
}

impl From<KvWriteError> for HashDBError {
    fn from(e: KvWriteError) -> Self {
        match e {
            KvWriteError::Duplicate => Self::Duplicate,
            KvWriteError::Backend(r) => r.current_context().into(),
        }
    }
}
