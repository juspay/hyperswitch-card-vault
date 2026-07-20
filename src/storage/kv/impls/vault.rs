use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{ContainerError, VaultDBError},
    storage::{
        DbOperation, Storage, schema,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{
                DirectInsert, GetPartitionKey, KvDeletableResource, KvResource, KvUpdatableResource,
            },
            serializable_query::{
                SerializableQuery, generate_delete_query, generate_insert_query,
                generate_update_query,
            },
        },
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

impl KvResource for Vault {
    type Error = VaultDBError;

    type InsertStrategy = DirectInsert;

    type DieselNew = VaultNewInner;

    type DieselEntity = VaultInner;

    type PrimaryKeyType = VaultPrimaryKey;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.set_updated_by(scheme);
    }

    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        generate_insert_query::<crate::storage::schema::vault::table, _>(new_object.clone())
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self::DieselEntity, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let query = diesel::insert_into(VaultInner::table()).values(new_object);

        let pool = conn.pool();
        let operation = DbOperation::Insert;
        crate::storage::log_db_query::<<VaultInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: VaultInner = crate::storage::record_db_query::<
            <VaultInner as HasTable>::Table,
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
        // A missing row surfaces (via `?`) as `VaultDBError::NotFoundError`.
        let query = VaultInner::table().filter(
            schema::vault::vault_id
                .eq(pk.vault_id.as_str())
                .and(schema::vault::entity_id.eq(pk.entity_id.as_str())),
        );

        let pool = conn.pool();
        let operation = DbOperation::FindOne;
        crate::storage::log_db_query::<<VaultInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: VaultInner = crate::storage::record_db_query::<
            <VaultInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
        .await?;
        Ok(output)
    }
}

impl KvDeletableResource for Vault {
    fn generate_delete_drainer_query(
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        let query = diesel::delete(crate::storage::schema::vault::table).filter(
            crate::storage::schema::vault::vault_id
                .eq(pk.vault_id.clone())
                .and(crate::storage::schema::vault::entity_id.eq(pk.entity_id.clone())),
        );

        generate_delete_query::<_, Self::DieselEntity>(query)
    }

    async fn storage_delete(
        store: &Storage,
        pk: Self::PrimaryKeyType,
    ) -> Result<usize, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let query = diesel::delete(VaultInner::table()).filter(
            schema::vault::vault_id
                .eq(pk.vault_id.as_str())
                .and(schema::vault::entity_id.eq(pk.entity_id.as_str())),
        );

        let pool = conn.pool();
        let operation = DbOperation::Delete;
        crate::storage::log_db_query::<<VaultInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output = crate::storage::record_db_query_rows::<
            <VaultInner as HasTable>::Table,
            _,
            _,
        >(query.execute(conn.get_mut()), operation, pool)
        .await?;

        Ok(output)
    }
}

impl KvUpdatableResource for Vault {
    type DieselUpdate = VaultUpdate;

    fn set_update_storage_scheme(update: &mut Self::DieselUpdate, scheme: StorageScheme) {
        update.updated_by = scheme;
    }

    fn generate_update_drainer_query(
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

        generate_update_query::<_, Self::DieselEntity>(query)
    }

    fn apply_update(update: Self::DieselUpdate, current: Self::DieselEntity) -> Self::DieselEntity {
        VaultInner::from_update(update, current)
    }

    async fn storage_update(
        store: &Storage,
        update: Self::DieselUpdate,
        pk: Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;

        let query = diesel::update(VaultInner::table())
            .filter(
                schema::vault::vault_id
                    .eq(pk.vault_id.as_str())
                    .and(schema::vault::entity_id.eq(pk.entity_id.as_str())),
            )
            .set((
                schema::vault::encrypted_data.eq(update.encrypted_data),
                schema::vault::expires_at.eq(update.expires_at),
            ));

        let pool = conn.pool();
        let operation = DbOperation::Update;
        crate::storage::log_db_query::<<VaultInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: VaultInner = crate::storage::record_db_query::<
            <VaultInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
        .await?;
        Ok(output.into())
    }
}
