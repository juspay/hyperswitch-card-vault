use diesel::{
    associations::HasTable, query_dsl::methods::FilterDsl, BoolExpressionMethods, ExpressionMethods,
};
use diesel_async::RunQueryDsl;
use masking::{ExposeInterface, Secret};

use crate::{
    crypto::encryption_manager::managers::aes::GcmAes256,
    error::{self, ContainerError, ResultContainerExt},
    storage::{schema, Storage},
};

use super::{types, VaultInterface};

impl VaultInterface for Storage {
    type Algorithm = GcmAes256;
    type Error = error::VaultDBError;

    async fn find_by_vault_id_entity_id(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: Result<types::VaultInner, diesel::result::Error> = types::VaultInner::table()
            .filter(
                schema::vault::vault_id
                    .eq(vault_id.expose())
                    .and(schema::vault::entity_id.eq(entity_id)),
            )
            .get_result(&mut conn)
            .await;

        let output = match output {
            Err(err) => match err {
                diesel::result::Error::NotFound => {
                    Err(err).change_error(error::StorageError::NotFoundError)
                }
                _ => Err(err).change_error(error::StorageError::FindError),
            },
            Ok(vault) => Ok(vault),
        };

        output.map_err(From::from).map(From::from)
    }

    async fn upsert_or_get_from_vault(
        &self,
        new: types::VaultNew,
        mode: Option<types::WriteMode>,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;
        let cloned_new = new.clone();

        let query: Result<_, diesel::result::Error> =
            diesel::insert_into(types::VaultInner::table())
                .values(new)
                .get_result::<types::VaultInner>(&mut conn)
                .await;

        match query {
            Ok(inner) => Ok(inner.into()),
            Err(error) => match error {
                diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::UniqueViolation,
                    _,
                ) => {
                    if let Some(types::WriteMode::Upsert) = mode {
                        self.update_vault_data(cloned_new).await
                    } else {
                        self.find_by_vault_id_entity_id(cloned_new.vault_id, &cloned_new.entity_id)
                            .await
                    }
                }
                error => Err(error).change_error(error::StorageError::InsertError)?,
            },
        }
    }

    async fn delete_from_vault(
        &self,
        vault_id: Secret<String>,
        entity_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let query = diesel::delete(types::VaultInner::table()).filter(
            schema::vault::vault_id
                .eq(vault_id.expose())
                .and(schema::vault::entity_id.eq(entity_id)),
        );

        Ok(query
            .execute(&mut conn)
            .await
            .change_error(error::StorageError::DeleteError)?)
    }

    async fn update_vault_data(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: Result<types::VaultInner, diesel::result::Error> =
            diesel::update(types::VaultInner::table())
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
                .await;

        let output = match output {
            Err(err) => Err(err).change_error(error::StorageError::UpdateError),
            Ok(vault) => Ok(vault),
        };

        output.map_err(From::from).map(From::from)
    }
}
