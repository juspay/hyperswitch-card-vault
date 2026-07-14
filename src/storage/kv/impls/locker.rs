use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl};
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

    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        generate_insert_query::<crate::storage::schema::locker::table, _>(new_object.clone())
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

    async fn storage_find(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let mut conn = store.route_conn().await?;

        let PartitionKey::Locker {
            merchant_id,
            customer_id,
            locker_id,
        } = pk
        else {
            return Err(ContainerError::from(VaultDBError::UnknownError));
        };

        let output = crate::storage::schema::locker::table
            .filter(
                crate::storage::schema::locker::locker_id
                    .eq(*locker_id)
                    .and(crate::storage::schema::locker::merchant_id.eq(*merchant_id))
                    .and(crate::storage::schema::locker::customer_id.eq(*customer_id)),
            )
            .get_result::<LockerInner>(&mut conn)
            .await?;

        Ok(output.into())
    }
}
