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

    async fn generate_insert_drainer_query(
        new_object: &Self::DieselNew,
        store: &Storage,
    ) -> Result<SerializableQuery, ContainerError<VaultDBError>> {
        let new_object = new_object.clone();

        store
            .with_sync_conn(move |conn| {
                generate_insert_query::<crate::storage::schema::vault::table, _>(conn, new_object)
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
        let output: VaultInner = diesel::insert_into(crate::storage::schema::vault::table)
            .values(new_object)
            .get_result(&mut conn)
            .await?;
        Ok(output.into())
    }

    async fn storage_find_optional(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Option<Self>, ContainerError<VaultDBError>> {
        let PartitionKey::Vault {
            entity_id,
            vault_id,
        } = pk
        else {
            return Ok(None);
        };

        let mut conn = store.route_conn().await?;
        let output = crate::storage::schema::vault::table
            .filter(
                crate::storage::schema::vault::vault_id
                    .eq(*vault_id)
                    .and(crate::storage::schema::vault::entity_id.eq(*entity_id)),
            )
            .get_result::<VaultInner>(&mut conn)
            .await
            .optional()?;

        Ok(output.map(From::from))
    }
}
