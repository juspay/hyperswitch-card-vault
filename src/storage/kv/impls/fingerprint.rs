//! KV trait impls for the fingerprint table.

use diesel::{ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;
use hyperswitch_masking::{PeekInterface, Secret};

use crate::{
    error::{ContainerError, FingerprintDBError, kv::KvError},
    storage::{
        Storage,
        kv::{
            PartitionKey, StorageScheme,
            entity::EntityType,
            partition_key::KvStorePartition,
            resource::{GetPartitionKey, KvResource},
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

pub(crate) struct FingerprintPrimaryKey {
    pub fingerprint_hash: Secret<Vec<u8>>,
}

impl GetPartitionKey for FingerprintPrimaryKey {
    fn get_partition_key(&self) -> PartitionKey<'_> {
        PartitionKey::Fingerprint {
            fingerprint_hash: self.fingerprint_hash.peek().as_slice(),
        }
    }
}

impl KvResource for Fingerprint {
    type Error = FingerprintDBError;

    type DieselNew = FingerprintTableNew;

    type DieselEntity = Self;

    type PrimaryKeyType = FingerprintPrimaryKey;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = Some(scheme);
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
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<FingerprintDBError>> {
        // Read path: route to the read replica when runtime config enables it.
        let mut conn = store.route_conn().await?;
        Ok(Self::table()
            .filter(
                crate::storage::schema::fingerprint::fingerprint_hash
                    .eq(pk.fingerprint_hash.peek().as_slice()),
            )
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
