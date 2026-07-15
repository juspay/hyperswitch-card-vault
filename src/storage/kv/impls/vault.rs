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
            resource::{
                GetPartitionKey, KvDeleteResource, KvDeleteWithoutLookup, KvResource,
                KvUpdateResource,
            },
            serializable_query::{
                SerializableQuery, generate_delete_query, generate_insert_query,
                generate_update_query,
            },
        },
        storage_v2::types::{Vault, VaultInner, VaultNew},
    },
};

impl EntityType for VaultNew {
    const ENTITY_TYPE: &'static str = "vault";
}

impl EntityType for Vault {
    const ENTITY_TYPE: &'static str = "vault";
}

impl KvStorePartition for Vault {}

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

    type DieselNew = VaultNew;

    type PrimaryKeyType = VaultPrimaryKey;

    fn set_storage_scheme(new_object: &mut Self::DieselNew, scheme: StorageScheme) {
        new_object.updated_by = scheme;
    }

    fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        generate_insert_query::<crate::storage::schema::vault::table, _>(new_object.clone())
    }

    async fn storage_insert(
        new_object: Self::DieselNew,
        store: &Storage,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let output: VaultInner = diesel::insert_into(crate::storage::schema::vault::table)
            .values(new_object)
            .get_result(&mut conn)
            .await?;
        Ok(output.into())
    }

    async fn storage_find(
        store: &Storage,
        pk: &Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let mut conn = store.route_conn().await?;
        let output: VaultInner = crate::storage::schema::vault::table
            .filter(
                crate::storage::schema::vault::vault_id
                    .eq(pk.vault_id.as_str())
                    .and(crate::storage::schema::vault::entity_id.eq(pk.entity_id.as_str())),
            )
            .get_result::<VaultInner>(&mut conn)
            .await?;

        Ok(output.into())
    }
}

impl KvDeleteResource for Vault {
    fn generate_delete_drainer_query(
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        let query = diesel::delete(crate::storage::schema::vault::table).filter(
            crate::storage::schema::vault::vault_id
                .eq(pk.vault_id.clone())
                .and(crate::storage::schema::vault::entity_id.eq(pk.entity_id.clone())),
        );

        generate_delete_query(query, Self::ENTITY_TYPE.to_owned())
    }

    async fn storage_delete(
        store: &Storage,
        pk: Self::PrimaryKeyType,
    ) -> Result<usize, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let output = diesel::delete(crate::storage::schema::vault::table)
            .filter(
                crate::storage::schema::vault::vault_id
                    .eq(pk.vault_id)
                    .and(crate::storage::schema::vault::entity_id.eq(pk.entity_id)),
            )
            .execute(&mut conn)
            .await?;

        Ok(output)
    }
}

impl KvDeleteWithoutLookup for Vault {}

impl KvUpdateResource for Vault {
    fn generate_update_drainer_query(
        new_object: &Self::DieselNew,
        pk: &Self::PrimaryKeyType,
    ) -> error_stack::Result<SerializableQuery, crate::error::kv::KvError> {
        let query = diesel::update(crate::storage::schema::vault::table)
            .filter(
                crate::storage::schema::vault::vault_id
                    .eq(pk.vault_id.clone())
                    .and(crate::storage::schema::vault::entity_id.eq(pk.entity_id.clone())),
            )
            .set((
                crate::storage::schema::vault::encrypted_data.eq(new_object.encrypted_data.clone()),
                crate::storage::schema::vault::expires_at.eq(new_object.expires_at),
                crate::storage::schema::vault::updated_by.eq(new_object.updated_by),
            ));

        generate_update_query(query, Self::ENTITY_TYPE.to_owned())
    }

    fn apply_update(new_object: Self::DieselNew, current: Self) -> Self {
        Self {
            vault_id: current.vault_id,
            entity_id: current.entity_id,
            data: new_object.encrypted_data.into(),
            created_at: current.created_at,
            expires_at: new_object.expires_at,
            updated_by: new_object.updated_by,
        }
    }

    async fn storage_update(
        store: &Storage,
        new_object: Self::DieselNew,
        pk: Self::PrimaryKeyType,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let output: VaultInner = diesel::update(crate::storage::schema::vault::table)
            .filter(
                crate::storage::schema::vault::vault_id
                    .eq(pk.vault_id)
                    .and(crate::storage::schema::vault::entity_id.eq(pk.entity_id)),
            )
            .set((
                crate::storage::schema::vault::encrypted_data.eq(new_object.encrypted_data),
                crate::storage::schema::vault::expires_at.eq(new_object.expires_at),
                crate::storage::schema::vault::updated_by.eq(new_object.updated_by),
            ))
            .get_result(&mut conn)
            .await?;

        Ok(output.into())
    }
}
