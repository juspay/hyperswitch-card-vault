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
    crypto::encryption_manager::managers::aes::GcmAes256,
    error::{self, ContainerError, ResultContainerExt},
    logger,
    storage::{Storage, schema},
};
#[cfg(feature = "kv")]
use crate::error::RedisErrorExt;

impl VaultInterface for Storage {
    type Algorithm = GcmAes256;
    type Error = error::VaultDBError;

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
                // Redis miss or error — fall through to Postgres below.
            }
        }

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

    async fn insert_or_get_from_vault(
        &self,
        mut new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings().await;
            let scheme = crate::storage::kv::decide_storage_scheme::<types::VaultNew>(
                self,
                settings,
                crate::storage::kv::Op::Insert,
            )
            .await;
            // Stamp the decided scheme so the column can never disagree with
            // the path actually taken.
            new.updated_by = scheme;
            if matches!(scheme, crate::storage::kv::StorageScheme::RedisKv) {
                let vault_id = new.vault_id.peek().clone();
                let entity_id = new.entity_id.clone();

                let partition_key = crate::storage::kv::PartitionKey::Vault {
                    entity_id: &entity_id,
                    vault_id: &vault_id,
                };
                let field = crate::storage::kv::hash_field_key(&partition_key);

                // Try find in Redis first.
                let find_result =
                    crate::storage::kv::kv_wrapper::<types::VaultNew, types::VaultNew>(
                        self,
                        crate::storage::kv::KvOperation::<types::VaultNew>::HGet(&field),
                        partition_key.clone(),
                    )
                    .await;
                if let Ok(kv_result) = find_result {
                    if let Ok(value) = kv_result.try_into_hget() {
                        return Ok(types::Vault::from(value));
                    }
                }

                // Not found in Redis — insert via HSetNx + drainer.
                // updated_by already stamped from the decided scheme above.
                let kv_value = new.clone();

                let drainer_query =
                    crate::storage::kv::serializable_query::generate_insert_query::<
                        schema::vault::table,
                        _,
                    >(kv_value.clone())
                    .change_context(error::VaultDBError::DBInsertError)?;

                let result = crate::storage::kv::kv_wrapper::<(), types::VaultNew>(
                    self,
                    crate::storage::kv::KvOperation::HSetNx(
                        &field,
                        &kv_value,
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

                match result.try_into_hsetnx() {
                    Ok(hyperswitch_redis_interface::types::HsetnxReply::KeySet) => {
                        return Ok(types::Vault::from(kv_value));
                    }
                    Ok(hyperswitch_redis_interface::types::HsetnxReply::KeyNotSet) => {
                        // Key already exists — fall through to PG which will
                        // hit UniqueViolation and re-read.
                    }
                    Err(_) => return Err(ContainerError::from(error::VaultDBError::DBInsertError)),
                }
            }
        }

        let mut conn = self.get_conn().await?;
        let cloned_new = new.clone();

        logger::info!("performing insert operation on vault data");

        let query: Result<_, diesel::result::Error> =
            diesel::insert_into(types::VaultInner::table())
                .values(new)
                .get_result::<types::VaultInner>(&mut conn)
                .await;

        match query {
            Ok(inner) => {
                logger::info!("insert operation completed successfully");
                Ok(inner.into())
            }
            Err(error) => match error {
                diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::UniqueViolation,
                    _,
                ) => {
                    self.find_by_vault_id_entity_id(cloned_new.vault_id, &cloned_new.entity_id)
                        .await
                }
                error => {
                    logger::error!(error = %error, "insert operation failed");
                    Err(error).change_error(error::StorageError::InsertError)?
                }
            },
        }
    }

    async fn upsert_or_get_from_vault(
        &self,
        mut new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings().await;
            let scheme = crate::storage::kv::decide_storage_scheme::<types::VaultNew>(
                self,
                settings,
                crate::storage::kv::Op::Insert,
            )
            .await;
            // Stamp the decided scheme so the column can never disagree with
            // the path actually taken.
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

                // Try HSetNx first (no initial HGet — we want to *write*).
                let drainer_query =
                    crate::storage::kv::serializable_query::generate_insert_query::<
                        schema::vault::table,
                        _,
                    >(kv_value.clone())
                    .change_context(error::VaultDBError::DBInsertError)?;

                let result = crate::storage::kv::kv_wrapper::<(), types::VaultNew>(
                    self,
                    crate::storage::kv::KvOperation::HSetNx(
                        &field,
                        &kv_value,
                        drainer_query,
                    ),
                    partition_key.clone(),
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
                        // Insert succeeded — brand-new row.
                        return Ok(types::Vault::from(kv_value));
                    }
                    Ok(hyperswitch_redis_interface::types::HsetnxReply::KeyNotSet) => {
                        // Key exists — do HSet update inline.
                        let serialized = serde_json::to_string(&kv_value)
                            .change_context(error::VaultDBError::DBInsertError)?;

                        let update_query = diesel::update(types::VaultInner::table())
                            .filter(
                                schema::vault::vault_id
                                    .eq(vault_id.clone())
                                    .and(schema::vault::entity_id.eq(entity_id.clone())),
                            )
                            .set((
                                schema::vault::encrypted_data
                                    .eq(kv_value.encrypted_data.clone()),
                                schema::vault::expires_at.eq(kv_value.expires_at),
                                schema::vault::updated_by.eq(kv_value.updated_by),
                            ));

                        let drainer_update_query =
                            crate::storage::kv::serializable_query::generate_update_query(
                                update_query,
                                "vault".to_string(),
                            )
                            .change_context(error::VaultDBError::DBInsertError)?;

                        let update_result = crate::storage::kv::kv_wrapper::<(), types::VaultNew>(
                            self,
                            crate::storage::kv::KvOperation::Hset(
                                (field.clone(), serialized),
                                drainer_update_query,
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

                        return update_result
                            .try_into_hset()
                            .map(|_| types::Vault::from(kv_value))
                            .map_err(|_| {
                                ContainerError::from(error::VaultDBError::DBInsertError)
                            });
                    }
                    Err(_) => return Err(ContainerError::from(error::VaultDBError::DBInsertError)),
                }
            }
        }

        let mut conn = self.get_conn().await?;
        let cloned_new = new.clone();

        logger::info!("performing upsert operation on vault data");

        let query: Result<_, diesel::result::Error> =
            diesel::insert_into(types::VaultInner::table())
                .values(new)
                .get_result::<types::VaultInner>(&mut conn)
                .await;

        match query {
            Ok(inner) => {
                logger::info!("Insert operation completed successfully");
                Ok(inner.into())
            }
            Err(error) => match error {
                diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::UniqueViolation,
                    _,
                ) => self.update_vault_data(cloned_new).await,
                error => {
                    logger::error!(error = %error, "upsert operation failed");
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
        // Delete always hits Postgres — not KV.
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
        mut new: types::VaultNew,
    ) -> Result<types::Vault, ContainerError<Self::Error>> {
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
                crate::storage::kv::Op::Update(
                    partition_key,
                    Some(new.updated_by),
                ),
            )
            .await;
            // Stamp the decided scheme so the column can never disagree with
            // the path actually taken.
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
