use diesel::{
    BoolExpressionMethods, ExpressionMethods, associations::HasTable, query_dsl::methods::FilterDsl,
};
use diesel_async::RunQueryDsl;
#[cfg(feature = "kv")]
use error_stack::ResultExt;
use hyperswitch_masking::{ExposeInterface, Secret};
#[cfg(feature = "kv")]
use hyperswitch_masking::PeekInterface;

use super::{VaultInterface, types};
use crate::{
    error::{self, ContainerError},
    logger,
    storage::{Storage, schema},
};
#[cfg(feature = "kv")]
use crate::error::RedisErrorExt;

impl VaultInterface for Storage {
    type Error = error::VaultDBError;

    async fn insert_vault(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        let mut new = new;

        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings().await;
            let scheme = crate::storage::kv::decide_storage_scheme::<types::VaultNew>(
                self,
                settings,
                crate::storage::kv::Op::Insert,
            )
            .await;
            // Stamp the decided scheme on the row.
            new.updated_by = scheme;
            if matches!(scheme, crate::storage::kv::StorageScheme::RedisKv) {
                let vault_id = new.vault_id.peek().clone();
                let entity_id = new.entity_id.clone();

                let partition_key = crate::storage::kv::PartitionKey::Vault {
                    entity_id: &entity_id,
                    vault_id: &vault_id,
                };
                let field = crate::storage::kv::hash_field_key(&partition_key);

                let kv_value = new.clone();

                let drainer_query =
                    crate::storage::kv::serializable_query::generate_insert_query::<
                        schema::vault::table,
                        _,
                    >(kv_value.clone())
                    .change_context(error::VaultDBError::DBInsertError)?;

                let result = crate::storage::kv::kv_wrapper::<(), types::VaultNew>(
                    self,
                    crate::storage::kv::KvOperation::HSetNx(&field, &kv_value, drainer_query),
                    partition_key,
                )
                .await
                .map_err(|err| {
                    ContainerError::from(
                        err.to_redis_failed_response(&field)
                            .change_context(error::VaultDBError::DBInsertError),
                    )
                })?;

                match result.try_into_hsetnx() {
                    Ok(hyperswitch_redis_interface::types::HsetnxReply::KeySet) => {
                        return Ok(types::Vault::from(kv_value));
                    }
                    Ok(hyperswitch_redis_interface::types::HsetnxReply::KeyNotSet) => {
                        // Redis duplicate: don't fall through to PG because
                        // the drainer may not have flushed the original row yet.
                        return Err(ContainerError::from(
                            error::VaultDBError::Duplicate,
                        ));
                    }
                    Err(_) => {
                        return Err(ContainerError::from(
                            error::VaultDBError::DBInsertError,
                        ))
                    }
                }
            }
        }

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
        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings().await;
            let scheme = crate::storage::kv::decide_storage_scheme::<types::VaultNew>(
                self,
                settings,
                crate::storage::kv::Op::Find,
            )
            .await;
            if matches!(scheme, crate::storage::kv::StorageScheme::RedisKv) {
                let partition_key = crate::storage::kv::PartitionKey::Vault {
                    entity_id,
                    vault_id: vault_id.peek(),
                };
                let field = crate::storage::kv::hash_field_key(&partition_key);
                let result =
                    crate::storage::kv::kv_wrapper::<types::VaultNew, types::VaultNew>(
                        self,
                        crate::storage::kv::KvOperation::<types::VaultNew>::HGet(&field),
                        partition_key,
                    )
                    .await;
                if let Ok(kv_result) = result {
                    if let Ok(value) = kv_result.try_into_hget() {
                        return Ok(types::Vault::from(value));
                    }
                }
                // Miss/error: fall through to Postgres.
            }
        }

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

    async fn update_vault_data(
        &self,
        new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        let mut new = new;

        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings().await;
            let partition_key = crate::storage::kv::PartitionKey::Vault {
                entity_id: &new.entity_id,
                vault_id: new.vault_id.peek(),
            };
            let scheme = crate::storage::kv::decide_storage_scheme::<types::VaultNew>(
                self,
                settings,
                crate::storage::kv::Op::Update(partition_key, Some(new.updated_by)),
            )
            .await;
            // Stamp the decided scheme on the row.
            new.updated_by = scheme;
            if matches!(scheme, crate::storage::kv::StorageScheme::RedisKv) {
                let vault_id = new.vault_id.peek().clone();
                let entity_id = new.entity_id.clone();

                let kv_value = new.clone();

                let partition_key = crate::storage::kv::PartitionKey::Vault {
                    entity_id: &entity_id,
                    vault_id: &vault_id,
                };
                let field = crate::storage::kv::hash_field_key(&partition_key);

                let serialized = serde_json::to_string(&kv_value)
                    .change_context(error::VaultDBError::DBInsertError)?;

                let update_query = diesel::update(types::VaultInner::table())
                    .filter(
                        schema::vault::vault_id
                            .eq(vault_id.clone())
                            .and(schema::vault::entity_id.eq(entity_id.clone())),
                    )
                    .set((
                        schema::vault::encrypted_data.eq(kv_value.encrypted_data.clone()),
                        schema::vault::expires_at.eq(kv_value.expires_at),
                        schema::vault::updated_by.eq(kv_value.updated_by),
                    ));

                let drainer_query =
                    crate::storage::kv::serializable_query::generate_update_query(
                        update_query,
                        "vault".to_string(),
                    )
                    .change_context(error::VaultDBError::DBInsertError)?;

                let result = crate::storage::kv::kv_wrapper::<(), types::VaultNew>(
                    self,
                    crate::storage::kv::KvOperation::Hset(
                        (field.clone(), serialized),
                        drainer_query,
                    ),
                    partition_key,
                )
                .await
                .map_err(|err| {
                    ContainerError::from(
                        err.to_redis_failed_response(&field)
                            .change_context(error::VaultDBError::DBInsertError),
                    )
                })?;

                return result
                    .try_into_hset()
                    .map(|_| types::Vault::from(kv_value))
                    .map_err(|_| ContainerError::from(error::VaultDBError::DBInsertError));
            }
        }

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
