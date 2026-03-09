use diesel::{
    associations::HasTable, query_dsl::methods::FilterDsl, BoolExpressionMethods, ExpressionMethods,
};
use diesel_async::RunQueryDsl;
use masking::{ExposeInterface, Secret};

use crate::{
    crypto::encryption_manager::managers::aes::GcmAes256,
    error::{self, ContainerError, ResultContainerExt},
    logger,
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

        logger::info!("performing retrieve operation on vault data");

        let output: Result<types::VaultInner, diesel::result::Error> = types::VaultInner::table()
            .filter(
                schema::vault::vault_id
                    .eq(vault_id.expose())
                    .and(schema::vault::entity_id.eq(entity_id)),
            )
            .get_result(&mut conn)
            .await;

        let output = match output {
            Err(err) => {
                logger::error!(error = %err, "retrieve operation failed");
                match err {
                    diesel::result::Error::NotFound => {
                        Err(err).change_error(error::StorageError::NotFoundError)
                    }
                    _ => Err(err).change_error(error::StorageError::FindError),
                }
            }
            Ok(vault) => {
                logger::info!("retrieve operation completed successfully");
                Ok(vault)
            }
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

        logger::info!("performing add operation on vault data");

        let query: Result<_, diesel::result::Error> =
            diesel::insert_into(types::VaultInner::table())
                .values(new)
                .get_result::<types::VaultInner>(&mut conn)
                .await;

        match query {
            Ok(inner) => {
                logger::info!("add operation completed successfully");
                Ok(inner.into())
            }
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
                error => {
                    logger::error!(error = %error, "add operation failed");
                    Err(error).change_error(error::StorageError::InsertError)?
                }
            },
        }
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

        let output = query.execute(&mut conn).await;

        let output = match output {
            Ok(count) => {
                logger::info!("delete operation completed successfully");
                Ok(count)
            }
            Err(err) => {
                logger::error!(error = %err, "delete operation failed");
                Err(err).change_error(error::StorageError::DeleteError)
            }
        };

        output.map_err(From::from)
    }

    async fn update_vault_data(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        logger::info!("performing update operation on vault data");

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
            Err(err) => {
                logger::error!(error = %err, "update operation failed");
                Err(err).change_error(error::StorageError::UpdateError)
            }
            Ok(vault) => {
                logger::info!("update operation completed successfully");
                Ok(vault)
            }
        };

        output.map_err(From::from).map(From::from)
    }
}
