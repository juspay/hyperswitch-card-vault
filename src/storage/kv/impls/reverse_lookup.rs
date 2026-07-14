use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{self, ContainerError, ResultContainerExt, ReverseLookupDBError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::KvResource,
            serializable_query::{SerializableQuery, generate_insert_query},
        },
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

impl KvResource for ReverseLookup {
    type Error = ReverseLookupDBError;

    type DieselNew = ReverseLookupNew;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.update_by = scheme.to_string();
    }

    async fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
        store: &Storage,
    ) -> Result<SerializableQuery, ContainerError<ReverseLookupDBError>> {
        let new_object = new_object.clone();

        store
            .with_sync_conn(move |conn| {
                generate_insert_query::<crate::storage::schema::reverse_lookup::table, _>(
                    conn, new_object,
                )
                .map_err(|report| {
                    let context = ReverseLookupDBError::from(report.current_context());
                    ContainerError::from(report.change_context(context))
                })
            })
            .await
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self, ContainerError<ReverseLookupDBError>> {
        let mut conn = store.get_conn().await?;
        diesel::insert_into(Self::table())
            .values(new_object)
            .get_result(&mut conn)
            .await
            .change_error(error::StorageError::InsertError)
            .map_err(From::from)
    }

    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<Self>, ContainerError<ReverseLookupDBError>> {
        let PartitionKey::ReverseLookup { lookup_id } = pk else {
            return Ok(None);
        };

        let mut conn = store.route_conn().await?;
        let output = Self::table()
            .filter(crate::storage::schema::reverse_lookup::lookup_id.eq(*lookup_id))
            .get_result::<Self>(&mut conn)
            .await
            .optional()
            .change_error(error::StorageError::FindError)
            .map_err(ContainerError::<ReverseLookupDBError>::from)?;

        Ok(output)
    }
}
