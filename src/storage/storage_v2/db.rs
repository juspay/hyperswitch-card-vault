#[cfg(not(feature = "kv"))]
use async_bb8_diesel::AsyncRunQueryDsl;
#[cfg(not(feature = "kv"))]
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, associations::HasTable};
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
use crate::{
    logger,
    storage::{DbOperation, schema},
};

impl VaultInterface for Storage {
    type Error = error::VaultDBError;

    async fn insert_vault(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let new_inner = types::VaultNewInner::from(new);
            return crate::storage::kv::insert_resource::<types::Vault>(self, new_inner).await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;
            logger::info!("performing insert operation on vault data");
            let query = diesel::insert_into(types::VaultInner::table()).values(new);

            let pool = conn.pool();
            let operation = DbOperation::Insert;
            crate::storage::log_db_query::<<types::VaultInner as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output: types::VaultInner =
                crate::storage::record_db_query::<<types::VaultInner as HasTable>::Table, _, _, _>(
                    query.get_result_async(conn.get()),
                    operation,
                    pool,
                )
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
            let pk = crate::storage::kv::impls::vault::VaultPrimaryKey {
                entity_id: entity_id.to_string(),
                vault_id,
            };

            return crate::storage::kv::find_optional_resource_by_id::<types::Vault>(self, pk)
                .await?
                .ok_or_else(|| ContainerError::from(error::VaultDBError::NotFoundError));
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;
            logger::info!("performing retrieve operation on vault data");
            // A missing row surfaces (via `?`) as `VaultDBError::NotFoundError`.
            let query = types::VaultInner::table().filter(
                schema::vault::vault_id
                    .eq(vault_id.expose())
                    .and(schema::vault::entity_id.eq(entity_id.to_owned())),
            );

            let pool = conn.pool();
            let operation = DbOperation::FindOne;
            crate::storage::log_db_query::<<types::VaultInner as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output: types::VaultInner =
                crate::storage::record_db_query::<<types::VaultInner as HasTable>::Table, _, _, _>(
                    query.get_result_async(conn.get()),
                    operation,
                    pool,
                )
                .await?;
            Ok(output.into())
        }
    }

    async fn update_vault_data(
        &self,
        vault_id: Secret<String>,
        entity_id: String,
        update: types::VaultUpdate,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let vault_id = vault_id.peek().clone();

            return crate::storage::kv::update_resource_by_id::<types::Vault>(
                self,
                update,
                crate::storage::kv::impls::vault::VaultPrimaryKey {
                    entity_id,
                    vault_id,
                },
            )
            .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;
            logger::info!("performing update operation on vault data");

            let query = diesel::update(types::VaultInner::table())
                .filter(
                    schema::vault::vault_id
                        .eq(vault_id.expose())
                        .and(schema::vault::entity_id.eq(entity_id)),
                )
                .set(update);

            let pool = conn.pool();
            let operation = DbOperation::Update;
            crate::storage::log_db_query::<<types::VaultInner as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output: types::VaultInner =
                crate::storage::record_db_query::<<types::VaultInner as HasTable>::Table, _, _, _>(
                    query.get_result_async(conn.get()),
                    operation,
                    pool,
                )
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
            let conn = self.get_conn().await?;
            logger::info!("performing delete operation on vault data");
            let query = diesel::delete(types::VaultInner::table()).filter(
                schema::vault::vault_id
                    .eq(vault_id.expose())
                    .and(schema::vault::entity_id.eq(entity_id.to_owned())),
            );

            let pool = conn.pool();
            let operation = DbOperation::Delete;
            crate::storage::log_db_query::<<types::VaultInner as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output = crate::storage::record_db_query_rows::<
                <types::VaultInner as HasTable>::Table,
                _,
                _,
            >(query.execute_async(conn.get()), operation, pool)
            .await?;

            Ok(output)
        }
    }
}
