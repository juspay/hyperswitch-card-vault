use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;
use hyperswitch_masking::PeekInterface;

use crate::{
    error::{ContainerError, VaultDBError},
    storage::{
        DbOperation, Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{
                GetLookupKey, GetPartitionKey, KvDeletableResource, KvDeletableWithLookup, KvResource,
                KvSecondaryLookupResource, ReverseLookupInsert, ReverseLookupKey,
            },
            serializable_query::{SerializableQuery, generate_delete_query, generate_insert_query},
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

impl EntityType for LockerInner {
    const ENTITY_TYPE: &'static str = "locker";
}

impl KvStorePartition for Locker {}

impl KvStorePartition for LockerInner {}

#[derive(Clone)]
pub struct LockerPrimaryKeyType {
    pub locker_id: hyperswitch_masking::Secret<String>,
    pub merchant_id: String,
    pub customer_id: String,
}

impl GetPartitionKey for LockerPrimaryKeyType {
    fn get_partition_key(&self) -> PartitionKey<'_> {
        PartitionKey::Locker {
            merchant_id: &self.merchant_id,
            customer_id: &self.customer_id,
            locker_id: self.locker_id.peek(),
        }
    }
}

pub(crate) struct LockerHashLookupKey {
    pub hash_id: String,
    pub merchant_id: String,
    pub customer_id: String,
}

impl GetLookupKey for LockerHashLookupKey {
    fn get_lookup_key(&self) -> ReverseLookupKey {
        let Self {
            hash_id,
            merchant_id,
            customer_id,
        } = self;
        ReverseLookupKey {
            lookup_id: format!("locker_{merchant_id}_{customer_id}_{hash_id}"),
        }
    }
}

impl KvSecondaryLookupResource for Locker {
    type LookupKeyType = LockerHashLookupKey;

    fn get_reverse_lookup_key(
        new_object: &Self::DieselNew,
        _partition_key: &PartitionKey<'_>,
    ) -> Self::LookupKeyType {
        LockerHashLookupKey {
            hash_id: new_object.hash_id.clone(),
            customer_id: new_object.customer_id.clone(),
            merchant_id: new_object.merchant_id.clone(),
        }
    }

    async fn storage_find_by_lookup(
        store: &Storage,
        lookup_key: &Self::LookupKeyType,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let mut conn = store.route_conn().await?;

        let query = crate::storage::schema::locker::table.filter(
            crate::storage::schema::locker::hash_id
                .eq(lookup_key.hash_id.as_str())
                .and(
                    crate::storage::schema::locker::merchant_id.eq(lookup_key.merchant_id.as_str()),
                )
                .and(
                    crate::storage::schema::locker::customer_id.eq(lookup_key.customer_id.as_str()),
                ),
        );

        let pool = conn.pool();
        let operation = DbOperation::FindOne;
        crate::storage::log_db_query::<<LockerInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: LockerInner = crate::storage::record_db_query::<
            <LockerInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
        .await?;

        Ok(output.into())
    }
}

impl KvResource for Locker {
    type Error = VaultDBError;

    type InsertStrategy = ReverseLookupInsert;

    type DieselNew = LockerNew;

    type DieselEntity = LockerInner;

    type PrimaryKeyType = LockerPrimaryKeyType;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = Some(scheme);
    }

    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        generate_insert_query::<crate::storage::schema::locker::table, _>(new_object.clone())
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self::DieselEntity, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;

        let query = diesel::insert_into(crate::storage::schema::locker::table).values(new_object);

        let pool = conn.pool();
        let operation = DbOperation::Insert;
        crate::storage::log_db_query::<<LockerInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: LockerInner = crate::storage::record_db_query::<
            <LockerInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
        .await?;

        Ok(output)
    }

    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self::DieselEntity, ContainerError<VaultDBError>> {
        let mut conn = store.route_conn().await?;

        let query = crate::storage::schema::locker::table.filter(
            crate::storage::schema::locker::locker_id
                .eq(pk.locker_id.peek().as_str())
                .and(crate::storage::schema::locker::merchant_id.eq(pk.merchant_id.as_str()))
                .and(crate::storage::schema::locker::customer_id.eq(pk.customer_id.as_str())),
        );

        let pool = conn.pool();
        let operation = DbOperation::FindOne;
        crate::storage::log_db_query::<<LockerInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: LockerInner = crate::storage::record_db_query::<
            <LockerInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
        .await?;

        Ok(output)
    }
}

impl KvDeletableResource for Locker {
    fn generate_delete_drainer_query(
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        let query = diesel::delete(crate::storage::schema::locker::table).filter(
            crate::storage::schema::locker::locker_id
                .eq(pk.locker_id.peek().clone())
                .and(crate::storage::schema::locker::merchant_id.eq(pk.merchant_id.clone()))
                .and(crate::storage::schema::locker::customer_id.eq(pk.customer_id.clone())),
        );

        generate_delete_query::<_, Self>(query)
    }

    async fn storage_delete(
        store: &Storage,
        pk: Self::PrimaryKeyType,
    ) -> Result<usize, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;

        let query = diesel::delete(LockerInner::table()).filter(
            crate::storage::schema::locker::locker_id
                .eq(pk.locker_id.peek().as_str())
                .and(crate::storage::schema::locker::merchant_id.eq(pk.merchant_id.as_str()))
                .and(crate::storage::schema::locker::customer_id.eq(pk.customer_id.as_str())),
        );

        let pool = conn.pool();
        let operation = DbOperation::Delete;
        crate::storage::log_db_query::<<LockerInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output =
            crate::storage::record_db_query_rows::<<LockerInner as HasTable>::Table, _, _>(
                query.execute(conn.get_mut()),
                operation,
                pool,
            )
            .await?;
        Ok(output)
    }
}

impl KvDeletableWithLookup for Locker {
    fn get_reverse_lookup_key_from_resource(resource: &Self) -> ReverseLookupKey {
        ReverseLookupKey {
            lookup_id: format!(
                "locker_{}_{}_{}",
                resource.merchant_id, resource.customer_id, resource.hash_id
            ),
        }
    }
}
