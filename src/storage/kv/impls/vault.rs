use async_bb8_diesel::AsyncRunQueryDsl;
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, associations::HasTable};
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
                DirectInsert, GetPartitionKey, GetSecondaryKey, KvDeletableResource,
                KvDeleteWithoutLookup, KvResource, KvUpdatableResource, SecondaryKey,
            },
            serializable_query::{
                SerializableQuery, generate_delete_query, generate_insert_query,
                generate_update_query,
            },
        },
        schema,
        storage_v2::types::{Vault, VaultInner, VaultNewInner, VaultUpdate},
    },
};

impl EntityType for VaultNewInner {
    const ENTITY_TYPE: &'static str = "vault";
}

impl EntityType for VaultInner {
    const ENTITY_TYPE: &'static str = "vault";
}

impl EntityType for Vault {
    const ENTITY_TYPE: &'static str = "vault";
}

impl KvStorePartition for Vault {}

impl KvStorePartition for VaultInner {}

#[derive(Clone)]
pub(crate) struct VaultPrimaryKey {
    pub entity_id: String,
    pub vault_id: String,
}

impl GetPartitionKey for VaultPrimaryKey {
    fn get_partition_key(&self) -> PartitionKey<'_> {
        PartitionKey::Vault {
            entity_id: self.entity_id.as_str(),
            vault_id: self.vault_id.as_str(),
        }
    }
}

impl GetSecondaryKey for VaultPrimaryKey {
    fn get_secondary_key(&self) -> SecondaryKey {
        SecondaryKey::new(self.get_partition_key().to_string())
    }
}

impl KvResource for Vault {
    type Error = VaultDBError;

    type InsertStrategy = DirectInsert;

    type DieselNew = VaultNewInner;

    type DieselEntity = VaultInner;

    type PrimaryKeyType = VaultPrimaryKey;

    fn get_primary_key_from_new_object(new_object: &Self::DieselNew) -> Self::PrimaryKeyType {
        VaultPrimaryKey {
            entity_id: new_object.entity_id().to_string(),
            vault_id: new_object.vault_id().peek().clone(),
        }
    }

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.set_updated_by(scheme);
    }

    async fn generate_insert_drainer_query(
        store: &Storage,
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        generate_insert_query::<crate::storage::schema::vault::table, _>(store, new_object.clone())
            .await
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self::DieselEntity, ContainerError<VaultDBError>> {
        let conn = store.get_conn().await?;
        let query = diesel::insert_into(VaultInner::table()).values(new_object);

        let pool = conn.pool();
        let operation = DbOperation::Insert;
        crate::storage::log_db_query::<<VaultInner as HasTable>::Table, _>(&query, operation, pool);

        let output: VaultInner = crate::storage::record_db_query::<
            <VaultInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result_async(conn.get()), operation, pool)
        .await?;
        Ok(output)
    }
    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self::DieselEntity, ContainerError<VaultDBError>> {
        let conn = store.route_conn().await?;
        // A missing row surfaces (via `?`) as `VaultDBError::NotFoundError`.
        let query = VaultInner::table().filter(
            schema::vault::vault_id
                .eq(pk.vault_id.clone())
                .and(schema::vault::entity_id.eq(pk.entity_id.clone())),
        );

        let pool = conn.pool();
        let operation = DbOperation::FindOne;
        crate::storage::log_db_query::<<VaultInner as HasTable>::Table, _>(&query, operation, pool);

        let output: VaultInner = crate::storage::record_db_query::<
            <VaultInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result_async(conn.get()), operation, pool)
        .await?;
        Ok(output)
    }
}

impl KvDeletableResource for Vault {
    async fn generate_delete_drainer_query(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        let query = diesel::delete(crate::storage::schema::vault::table).filter(
            crate::storage::schema::vault::vault_id
                .eq(pk.vault_id.clone())
                .and(crate::storage::schema::vault::entity_id.eq(pk.entity_id.clone())),
        );

        generate_delete_query::<_, Self::DieselEntity>(store, query).await
    }

    async fn storage_delete(
        store: &Storage,
        pk: Self::PrimaryKeyType,
    ) -> Result<usize, ContainerError<VaultDBError>> {
        let conn = store.get_conn().await?;
        let query = diesel::delete(VaultInner::table()).filter(
            schema::vault::vault_id
                .eq(pk.vault_id)
                .and(schema::vault::entity_id.eq(pk.entity_id)),
        );

        let pool = conn.pool();
        let operation = DbOperation::Delete;
        crate::storage::log_db_query::<<VaultInner as HasTable>::Table, _>(&query, operation, pool);

        let output = crate::storage::record_db_query_rows::<<VaultInner as HasTable>::Table, _, _>(
            query.execute_async(conn.get()),
            operation,
            pool,
        )
        .await?;

        Ok(output)
    }
}
impl KvDeleteWithoutLookup for Vault {}

impl KvUpdatableResource for Vault {
    type DieselUpdate = VaultUpdate;

    fn set_update_storage_scheme(update: &mut Self::DieselUpdate, scheme: StorageScheme) {
        update.updated_by = scheme;
    }

    async fn generate_update_drainer_query(
        store: &Storage,
        update: &Self::DieselUpdate,
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        let query = diesel::update(crate::storage::schema::vault::table)
            .filter(
                crate::storage::schema::vault::vault_id
                    .eq(pk.vault_id.clone())
                    .and(crate::storage::schema::vault::entity_id.eq(pk.entity_id.clone())),
            )
            .set(update.clone());

        generate_update_query::<_, Self::DieselEntity>(store, query).await
    }

    fn apply_update(update: Self::DieselUpdate, current: Self::DieselEntity) -> Self::DieselEntity {
        VaultInner::from_update(update, current)
    }

    async fn storage_update(
        store: &Storage,
        update: Self::DieselUpdate,
        pk: Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let conn = store.get_conn().await?;

        let query = diesel::update(VaultInner::table())
            .filter(
                schema::vault::vault_id
                    .eq(pk.vault_id)
                    .and(schema::vault::entity_id.eq(pk.entity_id)),
            )
            .set(update);

        let pool = conn.pool();
        let operation = DbOperation::Update;
        crate::storage::log_db_query::<<VaultInner as HasTable>::Table, _>(&query, operation, pool);

        let output: VaultInner = crate::storage::record_db_query::<
            <VaultInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result_async(conn.get()), operation, pool)
        .await?;
        Ok(output.into())
    }
}
