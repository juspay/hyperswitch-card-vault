//! KV trait implementations for each table.

use diesel::{
    ExpressionMethods, OptionalExtension, QueryDsl,
    associations::HasTable,
};
use diesel_async::RunQueryDsl;
use hyperswitch_masking::PeekInterface;

use super::{
    StorageScheme,
    constraints::UniqueConstraints,
    entity::EntityType,
    partition_key::{KvStorePartition, PartitionKey},
    resource::{KvFindOptional, KvWriteError, PlainKeyed, StorageResource},
    serializable_query::generate_insert_query,
};
use crate::error::{FingerprintDBError, HashDBError};
use crate::storage::Storage;
use crate::storage::types::{Fingerprint, FingerprintTableNew, HashTable, HashTableNew};

impl EntityType for FingerprintTableNew {
    const ENTITY_TYPE: &'static str = "fingerprint";
}

impl KvStorePartition for Fingerprint {}

impl KvStorePartition for FingerprintTableNew {}

impl UniqueConstraints for Fingerprint {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(self.fingerprint_hash.peek())]
    }

    fn table_name(&self) -> &str {
        "fingerprint"
    }
}

impl UniqueConstraints for FingerprintTableNew {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(self.fingerprint_hash.peek())]
    }

    fn table_name(&self) -> &str {
        "fingerprint"
    }
}

impl PlainKeyed for FingerprintTableNew {}

impl StorageResource for FingerprintTableNew {
    type Domain = Fingerprint;
    type Error = FingerprintDBError;

    fn into_domain(self) -> Self::Domain {
        Self::Domain {
            id: 0,
            fingerprint_hash: self.fingerprint_hash,
            fingerprint_id: self.fingerprint_id,
            updated_by: self.updated_by,
        }
    }

    fn set_storage_scheme(&mut self, scheme: StorageScheme) {
        self.updated_by = scheme;
    }

    fn insert_drainer_query(
        &self,
    ) -> error_stack::Result<super::serializable_query::SerializableQuery, crate::error::StorageError> {
        generate_insert_query::<crate::storage::schema::fingerprint::table, _>(self.clone())
    }

    async fn storage_insert(
        self,
        store: &Storage,
    ) -> Result<Fingerprint, crate::error::ContainerError<FingerprintDBError>> {
        let mut conn = store.get_conn().await?;
        let output: Fingerprint = diesel::insert_into(Fingerprint::table())
            .values(self)
            .get_result(&mut conn)
            .await?;
        Ok(output)
    }
}

impl KvFindOptional for FingerprintTableNew {
    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<Fingerprint>, crate::error::ContainerError<FingerprintDBError>> {
        let PartitionKey::Fingerprint { fingerprint_hash } = pk else {
            let err: crate::error::ContainerError<crate::error::StorageError> =
                crate::error::StorageError::KVError.into();
            return Err(err.into());
        };
        let mut conn = store.get_conn().await?;
        let output = Fingerprint::table()
            .filter(crate::storage::schema::fingerprint::fingerprint_hash.eq(*fingerprint_hash))
            .get_result::<Fingerprint>(&mut conn)
            .await
            .optional()?;
        Ok(output)
    }
}

impl From<KvWriteError> for FingerprintDBError {
    fn from(e: KvWriteError) -> Self {
        match e {
            KvWriteError::Duplicate => Self::Duplicate,
            KvWriteError::Backend(r) => r.current_context().into(),
        }
    }
}

impl EntityType for HashTableNew {
    const ENTITY_TYPE: &'static str = "hash_table";
}

impl KvStorePartition for HashTable {}

impl KvStorePartition for HashTableNew {}

impl UniqueConstraints for HashTable {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(&self.data_hash)]
    }

    fn table_name(&self) -> &str {
        "hash_table"
    }
}

impl UniqueConstraints for HashTableNew {
    fn unique_constraints(&self) -> Vec<String> {
        vec![hex::encode(&self.data_hash)]
    }

    fn table_name(&self) -> &str {
        "hash_table"
    }
}

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
    ) -> error_stack::Result<super::serializable_query::SerializableQuery, crate::error::StorageError> {
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
            let err: crate::error::ContainerError<crate::error::StorageError> =
                crate::error::StorageError::KVError.into();
            return Err(err.into());
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

impl From<KvWriteError> for HashDBError {
    fn from(e: KvWriteError) -> Self {
        match e {
            KvWriteError::Duplicate => Self::Duplicate,
            KvWriteError::Backend(r) => r.current_context().into(),
        }
    }
}
