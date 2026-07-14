use diesel::{BoolExpressionMethods, ExpressionMethods, OptionalExtension, QueryDsl};
use diesel_async::RunQueryDsl;

use crate::{
    error::{ContainerError, VaultDBError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::KvResource,
            serializable_query::{SerializableQuery, generate_insert_query},
        },
        types::{Locker, LockerInner, LockerNew},
    },
};

impl EntityType for LockerNew {
    const ENTITY_TYPE: &'static str = "locker";
}

impl EntityType for Locker {
    const ENTITY_TYPE: &'static str = "locker";
}

impl KvStorePartition for Locker {}

impl KvResource for Locker {
    type Error = VaultDBError;

    type DieselNew = LockerNew;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = scheme;
    }

    async fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
        store: &Storage,
    ) -> Result<SerializableQuery, ContainerError<VaultDBError>> {
        let new_object = new_object.clone();

        store
            .with_sync_conn(move |conn| {
                generate_insert_query::<crate::storage::schema::locker::table, _>(conn, new_object)
                    .map_err(|report| {
                        let context = VaultDBError::from(report.current_context());
                        ContainerError::from(report.change_context(context))
                    })
            })
            .await
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let output: LockerInner = diesel::insert_into(crate::storage::schema::locker::table)
            .values(new_object)
            .get_result(&mut conn)
            .await?;
        Ok(output.into())
    }

    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<Self>, ContainerError<VaultDBError>> {
        let mut conn = store.route_conn().await?;

        let PartitionKey::Locker {
            merchant_id,
            customer_id,
            locker_id,
        } = pk
        else {
            return Ok(None);
        };

        let output = crate::storage::schema::locker::table
            .filter(
                crate::storage::schema::locker::locker_id
                    .eq(*locker_id)
                    .and(crate::storage::schema::locker::merchant_id.eq(*merchant_id))
                    .and(crate::storage::schema::locker::customer_id.eq(*customer_id)),
            )
            .get_result::<LockerInner>(&mut conn)
            .await
            .optional()?;

        Ok(output.map(From::from))
    }
}
