//! KV trait impls for the vault table.

use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;
use hyperswitch_masking::{ExposeInterface, PeekInterface};

use crate::{
    error::{KvError, VaultDBError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{HashKeyed, KvDeletable, KvFind, KvUpdatable, StorageResource},
            serializable_query::{generate_insert_query, generate_update_query},
        },
        schema,
        storage_v2::types::{Vault, VaultInner, VaultNew},
    },
};

impl EntityType for VaultNew {
    const ENTITY_TYPE: &'static str = "vault";
}

impl KvStorePartition for VaultNew {}

impl HashKeyed for VaultNew {}

impl StorageResource for VaultNew {
    type Domain = Vault;
    type Error = VaultDBError;

    fn into_domain(self) -> Self::Domain {
        self.into()
    }

    fn set_storage_scheme(&mut self, scheme: StorageScheme) {
        self.updated_by = scheme;
    }

    fn insert_drainer_query(
        &self,
    ) -> error_stack::Result<crate::storage::kv::serializable_query::SerializableQuery, KvError>
    {
        generate_insert_query::<schema::vault::table, _>(self.clone())
    }

    async fn storage_insert(
        self,
        store: &Storage,
    ) -> Result<Vault, crate::error::ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let row: VaultInner = diesel::insert_into(VaultInner::table())
            .values(self)
            .get_result(&mut conn)
            .await?;
        Ok(row.into())
    }
}

impl KvFind for VaultNew {
    async fn storage_find(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Vault, crate::error::ContainerError<VaultDBError>> {
        let PartitionKey::Vault {
            vault_id,
            entity_id,
        } = pk
        else {
            return Err(VaultDBError::from(&KvError::Backend).into());
        };
        let mut conn = store.get_conn().await?;
        let row: VaultInner = VaultInner::table()
            .filter(
                schema::vault::vault_id
                    .eq(*vault_id)
                    .and(schema::vault::entity_id.eq(*entity_id)),
            )
            .get_result(&mut conn)
            .await?;
        Ok(row.into())
    }
}

impl KvUpdatable for VaultNew {
    fn update_drainer_query(
        &self,
    ) -> error_stack::Result<crate::storage::kv::serializable_query::SerializableQuery, KvError>
    {
        let update_query = diesel::update(schema::vault::table)
            .filter(
                schema::vault::vault_id
                    .eq(self.vault_id.peek().clone())
                    .and(schema::vault::entity_id.eq(self.entity_id.clone())),
            )
            .set((
                schema::vault::encrypted_data.eq(self.encrypted_data.clone()),
                schema::vault::expires_at.eq(self.expires_at),
                schema::vault::updated_by.eq(self.updated_by),
            ));

        generate_update_query(update_query, "vault".to_string())
    }

    async fn storage_update(
        self,
        store: &Storage,
    ) -> Result<Vault, crate::error::ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let row: VaultInner = diesel::update(VaultInner::table())
            .filter(
                schema::vault::vault_id
                    .eq(self.vault_id.expose())
                    .and(schema::vault::entity_id.eq(&self.entity_id)),
            )
            .set((
                schema::vault::encrypted_data.eq(self.encrypted_data),
                schema::vault::expires_at.eq(self.expires_at),
            ))
            .get_result(&mut conn)
            .await?;
        Ok(row.into())
    }
}

impl KvDeletable for VaultNew {
    async fn storage_delete(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<usize, crate::error::ContainerError<VaultDBError>> {
        let PartitionKey::Vault {
            vault_id,
            entity_id,
        } = pk
        else {
            return Err(VaultDBError::from(&KvError::Backend).into());
        };
        let mut conn = store.get_conn().await?;
        let rows = diesel::delete(VaultInner::table())
            .filter(
                schema::vault::vault_id
                    .eq(*vault_id)
                    .and(schema::vault::entity_id.eq(*entity_id)),
            )
            .execute(&mut conn)
            .await?;
        Ok(rows)
    }
}

// From<&KvError> and From<KvWriteError> for VaultDBError are implemented in
// impls/locker.rs (Part B) — vault reuses them.
