use diesel::{
    BoolExpressionMethods, ExpressionMethods, associations::HasTable, query_dsl::methods::FilterDsl,
};
use diesel_async::RunQueryDsl;
use hyperswitch_masking::{ExposeInterface, Secret};

use super::{VaultInterface, types};
use crate::{
    error::{self, ContainerError},
    logger,
    storage::{DbOperation, Storage, schema},
};

impl VaultInterface for Storage {
    type Error = error::VaultDBError;

    async fn insert_vault(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        logger::info!("performing insert operation on vault data");

        let query = diesel::insert_into(types::VaultInner::table()).values(new);

        let pool = conn.pool();
        let operation = DbOperation::Insert;
        crate::storage::log_db_query::<<types::VaultInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: types::VaultInner = crate::storage::record_db_query::<
            <types::VaultInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
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

        // A missing row surfaces (via `?`) as `VaultDBError::NotFoundError`.
        let query = types::VaultInner::table().filter(
            schema::vault::vault_id
                .eq(vault_id.expose())
                .and(schema::vault::entity_id.eq(entity_id)),
        );

        let pool = conn.pool();
        let operation = DbOperation::FindOne;
        crate::storage::log_db_query::<<types::VaultInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: types::VaultInner = crate::storage::record_db_query::<
            <types::VaultInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
        .await?;

        Ok(output.into())
    }

    async fn update_vault_data(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        logger::info!("performing update operation on vault data");

        let query = diesel::update(types::VaultInner::table())
            .filter(
                schema::vault::vault_id
                    .eq(new.vault_id.expose())
                    .and(schema::vault::entity_id.eq(&new.entity_id)),
            )
            .set((
                schema::vault::encrypted_data.eq(new.encrypted_data),
                schema::vault::expires_at.eq(new.expires_at),
            ));

        let pool = conn.pool();
        let operation = DbOperation::Update;
        crate::storage::log_db_query::<<types::VaultInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let output: types::VaultInner = crate::storage::record_db_query::<
            <types::VaultInner as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result(conn.get_mut()), operation, pool)
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

        let query = diesel::delete(types::VaultInner::table()).filter(
            schema::vault::vault_id
                .eq(vault_id.expose())
                .and(schema::vault::entity_id.eq(entity_id)),
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
        >(query.execute(conn.get_mut()), operation, pool)
        .await?;

        Ok(output)
    }
}
