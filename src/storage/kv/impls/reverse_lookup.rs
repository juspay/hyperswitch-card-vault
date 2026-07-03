//! KV trait impls for the reverse_lookup table.

use diesel::{ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{KvError, ReverseLookupDBError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{KvFind, KvWriteError, PlainKeyed, StorageResource},
            serializable_query::generate_insert_query,
        },
        schema,
        types::{ReverseLookup, ReverseLookupNew},
    },
};

impl EntityType for ReverseLookupNew {
    const ENTITY_TYPE: &'static str = "reverse_lookup";
}

impl KvStorePartition for ReverseLookupNew {}

impl PlainKeyed for ReverseLookupNew {}

impl StorageResource for ReverseLookupNew {
    type Domain = ReverseLookup;
    type Error = ReverseLookupDBError;

    fn into_domain(self) -> Self::Domain {
        self.into()
    }

    fn set_storage_scheme(&mut self, _scheme: StorageScheme) {
        // reverse_lookup has no `updated_by` column; the scheme is not persisted.
    }

    fn insert_drainer_query(
        &self,
    ) -> error_stack::Result<crate::storage::kv::serializable_query::SerializableQuery, KvError>
    {
        generate_insert_query::<schema::reverse_lookup::table, _>(self.clone())
    }

    async fn storage_insert(
        self,
        store: &Storage,
    ) -> Result<ReverseLookup, crate::error::ContainerError<ReverseLookupDBError>> {
        let mut conn = store.get_conn().await?;
        let row: ReverseLookup = diesel::insert_into(ReverseLookup::table())
            .values(self)
            .get_result(&mut conn)
            .await?;
        Ok(row)
    }
}

impl KvFind for ReverseLookupNew {
    async fn storage_find(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<ReverseLookup, crate::error::ContainerError<ReverseLookupDBError>> {
        let PartitionKey::ReverseLookup { lookup_id } = pk else {
            return Err(ReverseLookupDBError::from(&KvError::Backend).into());
        };
        let mut conn = store.get_conn().await?;
        let row: ReverseLookup = ReverseLookup::table()
            .filter(schema::reverse_lookup::lookup_id.eq(*lookup_id))
            .get_result(&mut conn)
            .await?;
        Ok(row)
    }
}

impl From<&KvError> for ReverseLookupDBError {
    fn from(e: &KvError) -> Self {
        match e {
            KvError::DuplicateValue { .. } => Self::DBInsertError,
            KvError::ValueNotFound(_) => Self::NotFoundError,
            KvError::Backend | KvError::SerializationFailed => Self::UnknownError,
        }
    }
}

impl From<KvWriteError> for ReverseLookupDBError {
    fn from(e: KvWriteError) -> Self {
        match e {
            KvWriteError::Duplicate => Self::DBInsertError,
            KvWriteError::Backend(r) => r.current_context().into(),
        }
    }
}
