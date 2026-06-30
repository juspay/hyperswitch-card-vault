use diesel::{
    BoolExpressionMethods, ExpressionMethods, associations::HasTable, query_dsl::methods::FilterDsl,
};
use diesel_async::RunQueryDsl;
use hyperswitch_masking::{ExposeInterface, Secret};

use super::{VaultInterface, types};
use crate::{
    error::{self, ContainerError},
    logger,
    storage::{Storage, schema},
};

impl VaultInterface for Storage {
    type Error = error::VaultDBError;

    async fn insert_vault(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        logger::info!("performing insert operation on vault data");

        let output: types::VaultInner = diesel::insert_into(types::VaultInner::table())
            .values(new)
            .get_result(&mut conn)
            .await?;

        Ok(output.into())
    }

    async fn find_by_vault_id_entity_id(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
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

    async fn update_vault_data(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
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

    async fn delete_from_vault(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>> {
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
