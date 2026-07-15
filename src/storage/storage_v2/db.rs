#[cfg(not(feature = "kv"))]
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, associations::HasTable};
#[cfg(not(feature = "kv"))]
use diesel_async::RunQueryDsl;
#[cfg(not(feature = "kv"))]
use hyperswitch_masking::ExposeInterface;
#[cfg(feature = "kv")]
use hyperswitch_masking::PeekInterface;
use hyperswitch_masking::Secret;

use super::{VaultInterface, types};
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
            let vault_id = new.vault_id.peek().clone();
            let entity_id = new.entity_id.clone();
            let partition_key = crate::storage::kv::PartitionKey::Vault {
                entity_id: &entity_id,
                vault_id: &vault_id,
            };

            return crate::storage::kv::insert_resource::<types::Vault>(self, new, partition_key)
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
            let vault_id = vault_id.peek().clone();

            return crate::storage::kv::find_optional_resource_by_id::<types::Vault>(
                self,
                crate::storage::kv::impls::vault::VaultPrimaryKey {
                    entity_id: entity_id.to_string(),
                    vault_id,
                },
            )
            .await?
            .ok_or_else(|| ContainerError::from(error::VaultDBError::NotFoundError));
        }

        #[cfg(not(feature = "kv"))]
        {
            let mut conn = self.get_conn().await?;

            logger::info!("performing retrieve operation on vault data");

            // A missing row surfaces (via `?`) as `VaultDBError::NotFoundError`.
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
            let vault_id = new.vault_id.peek().clone();
            let entity_id = new.entity_id.clone();

            return crate::storage::kv::update_resource_by_id::<types::Vault>(
                self,
                new,
                crate::storage::kv::impls::vault::VaultPrimaryKey {
                    entity_id,
                    vault_id,
                },
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
            let vault_id = vault_id.peek().clone();
            let pk = crate::storage::kv::impls::vault::VaultPrimaryKey {
                entity_id: entity_id.to_string(),
                vault_id,
            };

            return crate::storage::kv::delete_resource_by_id::<types::Vault>(self, pk).await;
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
