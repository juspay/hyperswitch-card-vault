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

// `EntityType` tags the drainer query built for this table (INSERT today, UPDATE once supported).
impl EntityType for FingerprintTableNew {
    const ENTITY_TYPE: &'static str = "fingerprint";
}

impl EntityType for Fingerprint {
    const ENTITY_TYPE: &'static str = "fingerprint";
}

impl KvStorePartition for Fingerprint {}

impl From<&Fingerprint> for FingerprintTableNew {
    fn from(fingerprint: &Fingerprint) -> Self {
        Self {
            fingerprint_hash: fingerprint.fingerprint_hash.clone(),
            fingerprint_id: fingerprint.fingerprint_id.clone(),
            updated_by: fingerprint.updated_by,
        }
    }
}

impl KvResource for Fingerprint {
    type Error = FingerprintDBError;

    fn set_storage_scheme(&mut self, scheme: StorageScheme) {
        self.updated_by = scheme;
    }

    fn generate_insert_drainer_query(&self) -> error_stack::Result<SerializableQuery, KvError> {
        let new = FingerprintTableNew::from(self);
        generate_insert_query::<crate::storage::schema::fingerprint::table, _>(new)
    }

    async fn storage_insert(
        self,
        store: &Storage,
    ) -> Result<Self, ContainerError<FingerprintDBError>> {
        let new = FingerprintTableNew::from(&self);
        // Writes always go to the primary pool, never a read replica.
        let mut conn = store.route_conn().await?;
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
        // Read path: route to the read replica when runtime config enables it.
        let mut conn = store.route_conn().await?;
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
