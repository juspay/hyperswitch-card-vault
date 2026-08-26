//! KV trait impls for the fingerprint table.

use async_bb8_diesel::AsyncRunQueryDsl;
use diesel::{ExpressionMethods, QueryDsl, associations::HasTable};
use hyperswitch_masking::{PeekInterface, Secret};

use crate::{
    error::{ContainerError, FingerprintDBError, kv::KvError},
    storage::{
        DbOperation, PgPooledConn, Storage,
        kv::{
            PartitionKey, StorageScheme,
            entity::EntityType,
            partition_key::KvStorePartition,
            resource::{DirectInsert, GetPartitionKey, GetSecondaryKey, KvResource, SecondaryKey},
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
            fingerprint_hash: &self.fingerprint_hash,
        }
    }
}

impl GetSecondaryKey for FingerprintPrimaryKey {
    fn get_secondary_key(&self) -> SecondaryKey {
        SecondaryKey::new(self.get_partition_key().to_string())
    }
}

impl KvResource for Fingerprint {
    type Error = FingerprintDBError;

    type InsertStrategy = DirectInsert;

    type DieselNew = FingerprintTableNew;

    type DieselEntity = Self;

    type PrimaryKeyType = FingerprintPrimaryKey;

    fn get_primary_key_from_new_object(new_object: &Self::DieselNew) -> Self::PrimaryKeyType {
        FingerprintPrimaryKey {
            fingerprint_hash: new_object.fingerprint_hash.clone(),
        }
    }

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = Some(scheme);
    }

    async fn generate_insert_drainer_query(
        conn: &PgPooledConn,
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, KvError> {
        generate_insert_query::<crate::storage::schema::fingerprint::table, _>(
            conn,
            new_object.clone(),
        )
        .await
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self::DieselEntity, ContainerError<FingerprintDBError>> {
        // Writes always go to the primary pool, never a read replica.
        let conn = store.get_conn().await?;

        let query = diesel::insert_into(Self::table()).values(new_object);

        let pool = conn.pool();
        let operation = DbOperation::Insert;
        crate::storage::log_db_query::<<Self as HasTable>::Table, _>(&query, operation, pool);

        let output: Self = crate::storage::record_db_query::<<Self as HasTable>::Table, _, _, _>(
            query.get_result_async(conn.get()),
            operation,
            pool,
        )
        .await?;

        Ok(output)
    }
    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self::DieselEntity, ContainerError<FingerprintDBError>> {
        // Read path: route to the read replica when runtime config enables it.
        let conn = store.route_conn().await?;
        let query = Self::table().filter(
            crate::storage::schema::fingerprint::fingerprint_hash
                .eq(pk.fingerprint_hash.peek().clone()),
        );

        let pool = conn.pool();
        let operation = DbOperation::FindOne;
        crate::storage::log_db_query::<<Self as HasTable>::Table, _>(&query, operation, pool);

        let output: Self = crate::storage::record_db_query::<<Self as HasTable>::Table, _, _, _>(
            query.get_result_async(conn.get()),
            operation,
            pool,
        )
        .await?;

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
