//! KV trait impls for the fingerprint table.

use diesel::{ExpressionMethods, QueryDsl, associations::HasTable};
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

impl KvResource for Fingerprint {
    type Error = FingerprintDBError;

    type DieselNew = FingerprintTableNew;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = scheme;
    }

    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, KvError> {
        generate_insert_query::<crate::storage::schema::fingerprint::table, _>(new_object.clone())
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self, ContainerError<FingerprintDBError>> {
        // Writes always go to the primary pool, never a read replica.
        let mut conn = store.get_conn().await?;
        Ok(diesel::insert_into(Self::table())
            .values(new_object)
            .get_result(&mut conn)
            .await?)
    }

    async fn storage_find(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Self, ContainerError<FingerprintDBError>> {
        let PartitionKey::Fingerprint { fingerprint_hash } = pk;
        // Read path: route to the read replica when runtime config enables it.
        let mut conn = store.route_conn().await?;
        Ok(Self::table()
            .filter(crate::storage::schema::fingerprint::fingerprint_hash.eq(*fingerprint_hash))
            .get_result::<Self>(&mut conn)
            .await?)
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
