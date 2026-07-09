//! KV trait impls for the fingerprint table.

use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{ContainerError, FingerprintDBError, kv::KvError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::KvResource,
            serializable_query::{SerializableQuery, generate_insert_query},
        },
        types::{Fingerprint, FingerprintTableNew},
    },
};

// Required by `generate_insert_query<N: Insertable + EntityType>` — the drainer builder.
impl EntityType for FingerprintTableNew {
    const ENTITY_TYPE: &'static str = "fingerprint";
}

impl EntityType for Fingerprint {
    const ENTITY_TYPE: &'static str = "fingerprint";
}

impl KvStorePartition for Fingerprint {}

impl KvResource for Fingerprint {
    type Error = FingerprintDBError;

    fn set_storage_scheme(&mut self, scheme: StorageScheme) {
        self.updated_by = scheme;
    }

    fn generate_insert_drainer_query(&self) -> error_stack::Result<SerializableQuery, KvError> {
        let new = FingerprintTableNew {
            fingerprint_hash: self.fingerprint_hash.clone(),
            fingerprint_id: self.fingerprint_id.clone(),
            updated_by: self.updated_by,
        };
        generate_insert_query::<crate::storage::schema::fingerprint::table, _>(new)
    }

    async fn storage_insert(
        self,
        store: &Storage,
    ) -> Result<Self, ContainerError<FingerprintDBError>> {
        let new = FingerprintTableNew {
            fingerprint_hash: self.fingerprint_hash,
            fingerprint_id: self.fingerprint_id,
            updated_by: self.updated_by,
        };
        let mut conn = store.get_conn().await?;
        Ok(diesel::insert_into(Self::table())
            .values(new)
            .get_result(&mut conn)
            .await?)
    }

    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<Self>, ContainerError<FingerprintDBError>> {
        let PartitionKey::Fingerprint { fingerprint_hash } = pk;
        // Use the primary conn — the drainer writes to the primary, and this PG
        // fallback fires on a Redis miss where the row may have just been replayed.
        let mut conn = store.get_conn().await?;
        let output = Self::table()
            .filter(crate::storage::schema::fingerprint::fingerprint_hash.eq(*fingerprint_hash))
            .get_result::<Self>(&mut conn)
            .await
            .optional()?;
        Ok(output)
    }
}

impl From<&KvError> for FingerprintDBError {
    fn from(e: &KvError) -> Self {
        match e {
            KvError::DuplicateValue { .. } => Self::Duplicate,
            KvError::ValueNotFound(_) => Self::DBFilterError,
            KvError::Backend | KvError::SerializationFailed => Self::UnknownError,
        }
    }
}
