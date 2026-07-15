use diesel::{ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{self, ContainerError, ResultContainerExt, ReverseLookupDBError, kv::KvError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{GetPartitionKey, KvDeleteResource, KvDeleteWithoutLookup, KvResource},
            serializable_query::{SerializableQuery, generate_delete_query, generate_insert_query},
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

#[derive(Clone)]
pub(crate) struct ReverseLookupPrimaryKey {
    pub lookup_id: String,
}

impl GetPartitionKey for ReverseLookupPrimaryKey {
    fn get_partition_key(&self) -> PartitionKey<'_> {
        PartitionKey::ReverseLookup {
            lookup_id: self.lookup_id.as_str(),
        }
    }
}

impl KvResource for ReverseLookup {
    type Error = ReverseLookupDBError;

    type DieselNew = ReverseLookupNew;

    type PrimaryKeyType = ReverseLookupPrimaryKey;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.update_by = scheme.to_string();
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
        diesel::insert_into(Self::table())
            .values(new_object)
            .get_result(&mut conn)
            .await
            .change_error(error::StorageError::InsertError)
            .map_err(From::from)
    }

    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<ReverseLookupDBError>> {
        let mut conn = store.route_conn().await?;
        let output: Result<Self, diesel::result::Error> = Self::table()
            .filter(crate::storage::schema::reverse_lookup::lookup_id.eq(pk.lookup_id.as_str()))
            .get_result::<Self>(&mut conn)
            .await;

        match output {
            Err(err) => match err {
                diesel::result::Error::NotFound => {
                    Err(err).change_error(error::StorageError::NotFoundError)
                }
                _ => Err(err).change_error(error::StorageError::FindError),
            },
            Ok(reverse_lookup) => Ok(reverse_lookup),
        }
        .map_err(From::from)
    }
}

impl KvDeleteResource for ReverseLookup {
    fn generate_delete_drainer_query(
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, KvError> {
        let query = diesel::delete(crate::storage::schema::reverse_lookup::table)
            .filter(crate::storage::schema::reverse_lookup::lookup_id.eq(pk.lookup_id.clone()));

        generate_delete_query(query, Self::ENTITY_TYPE.to_owned())
    }

    async fn storage_delete(
        store: &Storage,
        pk: Self::PrimaryKeyType,
    ) -> Result<usize, ContainerError<ReverseLookupDBError>> {
        let mut conn = store.get_conn().await?;

        diesel::delete(Self::table())
            .filter(crate::storage::schema::reverse_lookup::lookup_id.eq(pk.lookup_id))
            .execute(&mut conn)
            .await
            .change_error(error::StorageError::DeleteError)
            .map_err(From::from)
    }
}

impl KvDeleteWithoutLookup for ReverseLookup {}
