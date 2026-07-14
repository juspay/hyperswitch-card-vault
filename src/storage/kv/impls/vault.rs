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

impl KvResource for Vault {
    type Error = VaultDBError;

    type DieselNew = VaultNew;

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
        pk: &PartitionKey<'_>,
    ) -> Result<Self, ContainerError<VaultDBError>> {
        let PartitionKey::Vault {
            entity_id,
            vault_id,
        } = pk
        else {
            return Err(ContainerError::from(VaultDBError::UnknownError));
        };

        let mut conn = store.route_conn().await?;
        let output: VaultInner = crate::storage::schema::vault::table
            .filter(
                crate::storage::schema::vault::vault_id
                    .eq(*vault_id)
                    .and(crate::storage::schema::vault::entity_id.eq(*entity_id)),
            )
            .get_result::<VaultInner>(&mut conn)
            .await?;

        Ok(output.into())
    }
}
