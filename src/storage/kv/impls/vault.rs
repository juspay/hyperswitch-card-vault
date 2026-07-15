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
            resource::{GetPartitionKey, KvResource},
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
