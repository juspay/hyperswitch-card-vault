use diesel::{ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{ContainerError, ReverseLookupDBError, kv::KvError},
    storage::{
        self, Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{self as kv_resouce, KvResource},
            serializable_query::{SerializableQuery, generate_insert_query},
        },
        schema,
        types::{ReverseLookup, ReverseLookupNew},
    },
};
impl EntityType for ReverseLookupNew {
    const ENTITY_TYPE: &'static str = "reverse_lookup";
}

impl EntityType for ReverseLookup {
    const ENTITY_TYPE: &'static str = "reverse_lookup";
}

impl KvStorePartition for ReverseLookup {}

pub(crate) struct ReverseLookupPrimaryKey {
    pub lookup_id: String,
}

impl crate::storage::kv::resource::GetPartitionKey for ReverseLookupPrimaryKey {
    fn get_partition_key(&self) -> PartitionKey<'_> {
        PartitionKey::ReverseLookup {
            lookup_id: &self.lookup_id,
        }
    }
}

impl KvResource for ReverseLookup {
    type Error = ReverseLookupDBError;

    type InsertStrategy = kv_resouce::DirectInsert;

    type DieselNew = ReverseLookupNew;

    type DieselEntity = Self;

    type PrimaryKeyType = ReverseLookupPrimaryKey;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = scheme.to_string();
    }

    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, KvError> {
        generate_insert_query::<crate::storage::schema::reverse_lookup::table, _>(
            new_object.clone(),
        )
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self, ContainerError<ReverseLookupDBError>> {
        let mut conn = store.get_conn().await?;

        let query = diesel::insert_into(ReverseLookup::table()).values(new_object);

        let pool = conn.pool();
        let operation = storage::DbOperation::Insert;
        storage::log_db_query::<<ReverseLookup as HasTable>::Table, _>(&query, operation, pool);

        let reverse_lookup =
            storage::record_db_query::<<ReverseLookup as HasTable>::Table, _, _, _>(
                query.get_result(conn.get_mut()),
                operation,
                pool,
            )
            .await?;
        Ok(reverse_lookup)
    }

    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<ReverseLookupDBError>> {
        let mut conn = store.get_conn().await?;
        let query = ReverseLookup::table()
            .filter(schema::reverse_lookup::lookup_id.eq(pk.lookup_id.as_str()));

        let pool = conn.pool();
        let operation = storage::DbOperation::FindOne;
        storage::log_db_query::<<ReverseLookup as HasTable>::Table, _>(&query, operation, pool);

        let output: ReverseLookup = storage::record_db_query::<
            <ReverseLookup as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
        .await?;
        Ok(output)
    }
}
