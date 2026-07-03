#[cfg(not(feature = "kv"))]
use diesel::{
    BoolExpressionMethods, ExpressionMethods, associations::HasTable, query_dsl::methods::FilterDsl,
};
#[cfg(not(feature = "kv"))]
use diesel_async::RunQueryDsl;
#[cfg(feature = "kv")]
use hyperswitch_masking::PeekInterface;
use hyperswitch_masking::{ExposeInterface, Secret};

use super::{VaultInterface, types};
#[cfg(feature = "kv")]
use crate::storage::kv::KvDeletable;
use crate::{
    error::{self, ContainerError},
    storage::Storage,
};
#[cfg(not(feature = "kv"))]
use crate::{logger, storage::schema};

impl VaultInterface for Storage {
    type Error = error::VaultDBError;

    async fn insert_vault(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let vault_id = new.vault_id.peek().to_string();
            let entity_id = new.entity_id.clone();
            let partition_key = super::super::kv::PartitionKey::Vault {
                vault_id: &vault_id,
                entity_id: &entity_id,
            };

            return super::super::kv::insert_hash_resource::<types::VaultNew>(
                self,
                new,
                partition_key,
            )
            .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let mut conn = self.get_conn().await?;

            logger::info!("performing insert operation on vault data");

            let output: types::VaultInner = diesel::insert_into(types::VaultInner::table())
                .values(new)
                .get_result(&mut conn)
                .await?;

            Ok(output.into())
        }
    }

    async fn find_by_vault_id_entity_id(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let vault_id_str = vault_id.expose().to_string();
            let partition_key = super::super::kv::PartitionKey::Vault {
                vault_id: &vault_id_str,
                entity_id,
            };

            return super::super::kv::find_hash_resource::<types::VaultNew>(self, partition_key)
                .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let mut conn = self.get_conn().await?;

            logger::info!("performing retrieve operation on vault data");

            let output: types::VaultInner = types::VaultInner::table()
                .filter(
                    schema::vault::vault_id
                        .eq(vault_id.expose())
                        .and(schema::vault::entity_id.eq(entity_id)),
                )
                .get_result(&mut conn)
                .await?;

            Ok(output.into())
        }
    }

    async fn update_vault_data(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let vault_id = new.vault_id.peek().to_string();
            let entity_id = new.entity_id.clone();
            let partition_key = super::super::kv::PartitionKey::Vault {
                vault_id: &vault_id,
                entity_id: &entity_id,
            };

            return super::super::kv::update_hash_resource::<types::VaultNew>(
                self,
                new,
                partition_key,
            )
            .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let mut conn = self.get_conn().await?;

            logger::info!("performing update operation on vault data");

            let output: types::VaultInner = diesel::update(types::VaultInner::table())
                .filter(
                    schema::vault::vault_id
                        .eq(new.vault_id.expose())
                        .and(schema::vault::entity_id.eq(&new.entity_id)),
                )
                .set((
                    schema::vault::encrypted_data.eq(new.encrypted_data),
                    schema::vault::expires_at.eq(new.expires_at),
                ))
                .get_result(&mut conn)
                .await?;

            Ok(output.into())
        }
    }

    async fn delete_from_vault(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let vault_id_str = vault_id.expose().to_string();
            let partition_key = super::super::kv::PartitionKey::Vault {
                vault_id: &vault_id_str,
                entity_id,
            };

            // Delete is Postgres-only — direct call, not routed through the KV wrapper.
            return types::VaultNew::storage_delete(self, &partition_key).await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let mut conn = self.get_conn().await?;

            logger::info!("performing delete operation on vault data");

            let output = diesel::delete(types::VaultInner::table())
                .filter(
                    schema::vault::vault_id
                        .eq(vault_id.expose())
                        .and(schema::vault::entity_id.eq(entity_id)),
                )
                .execute(&mut conn)
                .await?;

            Ok(output)
        }
    }
}
