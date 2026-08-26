use async_bb8_diesel::{AsyncConnection, AsyncRunQueryDsl};
#[cfg(not(feature = "kv"))]
use diesel::{BoolExpressionMethods, OptionalExtension};
use diesel::{ExpressionMethods, QueryDsl, associations::HasTable};
#[cfg(not(feature = "kv"))]
use hyperswitch_masking::ExposeInterface;
use hyperswitch_masking::Secret;

use super::{
    DbOperation, MerchantInterface, Storage, schema, types,
    types::{StorageDecryption, StorageEncryption},
    utils,
};
use crate::{
    crypto::encryption_manager::managers::aes,
    error::{self, ContainerError, ResultContainerExt},
    logger,
    storage::scheme::StorageScheme,
};

impl MerchantInterface for Storage {
    type Algorithm = aes::GcmAes256;
    type Error = error::MerchantDBError;

    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &aes::GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        // Reads are routed to the read replica when enabled.
        let conn = self.route_conn().await?;

        // Single SELECT; a missing row surfaces (via `?`) as `MerchantDBError::NotFoundError`.
        // Decryption of the stored DEK is the merchant-specific envelope.
        let query = types::MerchantInner::table()
            .filter(schema::merchant::merchant_id.eq(merchant_id.to_owned()));

        let pool = conn.pool();
        let operation = DbOperation::FindOne;
        super::log_db_query::<<types::MerchantInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let inner: types::MerchantInner =
            super::record_db_query::<<types::MerchantInner as HasTable>::Table, _, _, _>(
                query.get_result_async(conn.get()),
                operation,
                pool,
            )
            .await?;

        Ok(inner.decrypt(key)?)
    }

    async fn insert_merchant(
        &self,
        new: types::MerchantNew<'_>,
        key: &aes::GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;

        let query = diesel::insert_into(types::MerchantInner::table()).values(new.encrypt(key)?);

        let pool = conn.pool();
        let operation = DbOperation::Insert;
        super::log_db_query::<<types::MerchantInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let inner: types::MerchantInner =
            super::record_db_query::<<types::MerchantInner as HasTable>::Table, _, _, _>(
                query.get_result_async(conn.get()),
                operation,
                pool,
            )
            .await?;

        Ok(inner.decrypt(key)?)
    }

    #[cfg(feature = "external_key_manager")]
    async fn find_all_keys_excluding_entity_keys(
        &self,
        key: &Self::Algorithm,
        limit: i64,
    ) -> Result<Vec<types::Merchant>, ContainerError<Self::Error>> {
        let conn = self.route_conn().await?;

        let query = schema::merchant::table
            .filter(
                schema::merchant::merchant_id
                    .ne_all(schema::entity::table.select(schema::entity::entity_id)),
            )
            .limit(limit);

        let pool = conn.pool();
        let operation = DbOperation::Filter;
        super::log_db_query::<<types::MerchantInner as HasTable>::Table, _>(
            &query, operation, pool,
        );

        let result: Result<Vec<types::MerchantInner>, ContainerError<Self::Error>> =
            super::record_db_query::<<types::MerchantInner as HasTable>::Table, _, _, _>(
                query.load_async::<types::MerchantInner>(conn.get()),
                operation,
                pool,
            )
            .await
            .change_error(error::StorageError::FindError)
            .map_err(ContainerError::from);

        result?
            .into_iter()
            .map(|inner| {
                inner
                    .decrypt(key)
                    .change_error(error::MerchantDBError::DEKDecryptionError)
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

impl super::LockerInterface for Storage {
    type Error = error::VaultDBError;

    async fn insert_locker(
        &self,
        new: types::LockerNew,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            return super::kv::insert_resource_with_reverse_lookup::<types::Locker>(self, new)
                .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;

            let query = diesel::insert_into(types::LockerInner::table()).values(new);

            let pool = conn.pool();
            let operation = DbOperation::Insert;
            super::log_db_query::<<types::LockerInner as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output: types::LockerInner =
                super::record_db_query::<<types::LockerInner as HasTable>::Table, _, _, _>(
                    query.get_result_async(conn.get()),
                    operation,
                    pool,
                )
                .await?;

            Ok(output.into())
        }
    }

    async fn find_by_locker_id_merchant_id_customer_id(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let pk = super::kv::impls::locker::LockerPrimaryKeyType {
                locker_id: locker_id.clone(),
                merchant_id: merchant_id.to_string(),
                customer_id: customer_id.to_string(),
            };
            return super::kv::find_resource_by_id::<types::Locker>(self, pk).await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;

            // A missing row surfaces (via `?`) as `VaultDBError::NotFoundError`.
            let query = types::LockerInner::table().filter(
                schema::locker::locker_id
                    .eq(locker_id.expose())
                    .and(schema::locker::merchant_id.eq(merchant_id.to_owned()))
                    .and(schema::locker::customer_id.eq(customer_id.to_owned())),
            );

            let pool = conn.pool();
            let operation = DbOperation::FindOne;
            super::log_db_query::<<types::LockerInner as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output: types::LockerInner =
                super::record_db_query::<<types::LockerInner as HasTable>::Table, _, _, _>(
                    query.get_result_async(conn.get()),
                    operation,
                    pool,
                )
                .await?;

            Ok(output.into())
        }
    }

    async fn find_optional_by_hash_id_merchant_id_customer_id(
        &self,
        hash_id: &str,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<Option<types::Locker>, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let lookup_key = super::kv::impls::locker::LockerHashLookupKey {
                hash_id: hash_id.to_string(),
                merchant_id: merchant_id.to_string(),
                customer_id: customer_id.to_string(),
            };

            return super::kv::find_optional_resource_by_lookup_id::<types::Locker>(
                self, lookup_key,
            )
            .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;

            let query = types::LockerInner::table().filter(
                schema::locker::hash_id
                    .eq(hash_id.to_owned())
                    .and(schema::locker::merchant_id.eq(merchant_id.to_owned()))
                    .and(schema::locker::customer_id.eq(customer_id.to_owned())),
            );

            let pool = conn.pool();
            let operation = DbOperation::FindOne;
            super::log_db_query::<<types::LockerInner as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output = super::record_db_query_optional::<
                <types::LockerInner as HasTable>::Table,
                _,
                _,
                _,
            >(
                async {
                    query
                        .get_result_async::<types::LockerInner>(conn.get())
                        .await
                        .optional()
                },
                operation,
                pool,
            )
            .await?;

            Ok(output.map(From::from))
        }
    }

    async fn delete_locker(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let pk = super::kv::impls::locker::LockerPrimaryKeyType {
                locker_id,
                merchant_id: merchant_id.to_string(),
                customer_id: customer_id.to_string(),
            };

            return super::kv::delete_resource_by_id_with_reverse_lookup::<types::Locker>(self, pk)
                .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;
            let query = diesel::delete(types::LockerInner::table()).filter(
                schema::locker::locker_id
                    .eq(locker_id.expose())
                    .and(schema::locker::merchant_id.eq(merchant_id.to_owned()))
                    .and(schema::locker::customer_id.eq(customer_id.to_owned())),
            );

            let pool = conn.pool();
            let operation = DbOperation::Delete;
            super::log_db_query::<<types::LockerInner as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output =
                super::record_db_query_rows::<<types::LockerInner as HasTable>::Table, _, _>(
                    query.execute_async(conn.get()),
                    operation,
                    pool,
                )
                .await?;
            Ok(output)
        }
    }
}

impl super::HashInterface for Storage {
    type Error = error::HashDBError;

    async fn find_optional_by_data_hash(
        &self,
        data_hash: Secret<Vec<u8>>,
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let pk = super::kv::impls::hash_table::HashTablePrimaryKey { data_hash };

            return super::kv::find_optional_resource_by_id::<types::HashTable>(self, pk).await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;

            let query = types::HashTable::table()
                .filter(schema::hash_table::data_hash.eq(data_hash.expose()));

            let pool = conn.pool();
            let operation = DbOperation::FindOne;
            super::log_db_query::<<types::HashTable as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output =
                super::record_db_query_optional::<<types::HashTable as HasTable>::Table, _, _, _>(
                    async {
                        query
                            .get_result_async::<types::HashTable>(conn.get())
                            .await
                            .optional()
                    },
                    operation,
                    pool,
                )
                .await?;

            Ok(output)
        }
    }

    async fn insert_hash(
        &self,
        data_hash: Secret<Vec<u8>>,
    ) -> Result<types::HashTable, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let hash_table_new = types::HashTableNew {
                hash_id: utils::generate_uuid(),
                data_hash,
                created_at: crate::utils::date_time::now(),
                updated_by: Some(StorageScheme::PostgresOnly),
            };
            return super::kv::insert_resource::<types::HashTable>(self, hash_table_new).await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;

            let query =
                diesel::insert_into(types::HashTable::table()).values(types::HashTableNew {
                    hash_id: utils::generate_uuid(),
                    data_hash,
                    created_at: crate::utils::date_time::now(),
                    updated_by: Some(StorageScheme::PostgresOnly),
                });

            let pool = conn.pool();
            let operation = DbOperation::Insert;
            super::log_db_query::<<types::HashTable as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output: types::HashTable =
                super::record_db_query::<<types::HashTable as HasTable>::Table, _, _, _>(
                    query.get_result_async(conn.get()),
                    operation,
                    pool,
                )
                .await?;

            Ok(output)
        }
    }
}

impl super::TestInterface for Storage {
    type Error = error::TestDBError;

    async fn test(&self) -> Result<(), ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;

        // Health probe: insert then delete within one transaction so nothing persists.
        conn.get()
            .transaction_async(|conn| async move {
                let query = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("1 + 1"));
                let _x: i32 = query.get_result_async(&conn).await.map_err(|err| {
                    logger::error!(read_err=?err, "Error while reading element in the database");
                    error::TestDBError::DBReadError
                })?;

                diesel::insert_into(types::HashTable::table())
                    .values(types::HashTableNew {
                        hash_id: "test".to_string(),
                        data_hash: Secret::new(b"0".to_vec()),
                        created_at: crate::utils::date_time::now(),
                        // Test-only — always PostgresOnly.
                        updated_by: Some(StorageScheme::PostgresOnly),
                    })
                    .execute_async(&conn)
                    .await
                    .map_err(|err| {
                        logger::error!(write_err=?err, "Error while writing to database");
                        error::TestDBError::DBWriteError
                    })?;

                diesel::delete(
                    types::HashTable::table()
                        .filter(schema::hash_table::hash_id.eq("test".to_string())),
                )
                .execute_async(&conn)
                .await
                .map_err(|err| {
                    logger::error!(delete_err=?err, "Error while deleting element in the database");
                    error::TestDBError::DBDeleteError
                })?;

                Ok::<(), error::TestDBError>(())
            })
            .await?;

        Ok(())
    }

    async fn test_replica(&self) -> Result<(), ContainerError<Self::Error>> {
        let conn = self.get_replica_conn().await?;

        let query = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("1 + 1"));
        let _x: i32 = query
            .get_result_async(conn.get())
            .await
            .change_error(error::StorageError::FindError)?;

        Ok(())
    }
}

impl super::FingerprintInterface for Storage {
    type Error = error::FingerprintDBError;

    async fn find_optional_by_fingerprint_hash(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            // Return the Queryable model, not the New struct.
            return super::kv::find_optional_resource_by_id::<types::Fingerprint>(
                self,
                super::kv::impls::fingerprint::FingerprintPrimaryKey { fingerprint_hash },
            )
            .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;

            let query = types::Fingerprint::table()
                .filter(schema::fingerprint::fingerprint_hash.eq(fingerprint_hash.expose()));

            let pool = conn.pool();
            let operation = DbOperation::FindOne;
            super::log_db_query::<<types::Fingerprint as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output = super::record_db_query_optional::<
                <types::Fingerprint as HasTable>::Table,
                _,
                _,
                _,
            >(
                async {
                    query
                        .get_result_async::<types::Fingerprint>(conn.get())
                        .await
                        .optional()
                },
                operation,
                pool,
            )
            .await?;

            Ok(output)
        }
    }

    async fn insert_fingerprint(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
        fingerprint_id: Secret<String>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            // `id: 0` — serial unknown at KV-insert time, assigned by drainer on replay.
            // `updated_by: PostgresOnly` is a placeholder — overwritten by `set_storage_scheme`
            // in `insert_resource` before the model is written to Redis or PG.
            let finger_print_new = types::FingerprintTableNew {
                fingerprint_hash: fingerprint_hash.clone(),
                fingerprint_id,
                updated_by: Some(StorageScheme::PostgresOnly),
            };
            return super::kv::insert_resource::<types::Fingerprint>(self, finger_print_new).await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;

            let query = diesel::insert_into(types::Fingerprint::table()).values(
                types::FingerprintTableNew {
                    fingerprint_hash,
                    fingerprint_id,
                    updated_by: Some(StorageScheme::PostgresOnly),
                },
            );

            let pool = conn.pool();
            let operation = DbOperation::Insert;
            super::log_db_query::<<types::Fingerprint as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output: types::Fingerprint =
                super::record_db_query::<<types::Fingerprint as HasTable>::Table, _, _, _>(
                    query.get_result_async(conn.get()),
                    operation,
                    pool,
                )
                .await?;

            Ok(output)
        }
    }
}

#[cfg(feature = "external_key_manager")]
impl super::EntityInterface for Storage {
    type Error = error::EntityDBError;

    async fn find_by_entity_id(
        &self,
        entity_id: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>> {
        // Reads are routed to the read replica when enabled.
        let conn = self.route_conn().await?;

        // A missing row surfaces as `EntityDBError::NotFoundError` (see the `From<diesel>`
        // classifier), which `find_or_create_entity` in the key manager checks via
        // `is_not_found()`.
        let query =
            types::Entity::table().filter(schema::entity::entity_id.eq(entity_id.to_owned()));

        let pool = conn.pool();
        let operation = DbOperation::FindOne;
        super::log_db_query::<<types::Entity as HasTable>::Table, _>(&query, operation, pool);

        let output: types::Entity = super::record_db_query::<
            <types::Entity as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result_async(conn.get()), operation, pool)
        .await?;

        Ok(output)
    }

    async fn insert_entity(
        &self,
        entity_id: &str,
        identifier: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;

        let query = diesel::insert_into(types::Entity::table()).values(types::EntityTableNew {
            entity_id: entity_id.into(),
            enc_key_id: identifier.into(),
        });

        let pool = conn.pool();
        let operation = DbOperation::Insert;
        super::log_db_query::<<types::Entity as HasTable>::Table, _>(&query, operation, pool);

        let output: types::Entity = super::record_db_query::<
            <types::Entity as HasTable>::Table,
            _,
            _,
            _,
        >(query.get_result_async(conn.get()), operation, pool)
        .await?;

        Ok(output)
    }
}

impl super::ReverseLookupInterface for Storage {
    type Error = error::ReverseLookupDBError;

    async fn insert_reverse_lookup(
        &self,
        new: types::ReverseLookupNew,
    ) -> Result<types::ReverseLookup, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            return Box::pin(super::kv::insert_resource::<types::ReverseLookup>(
                self, new,
            ))
            .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;

            let query = diesel::insert_into(types::ReverseLookup::table()).values(new);

            let pool = conn.pool();
            let operation = DbOperation::Insert;
            super::log_db_query::<<types::ReverseLookup as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let reverse_lookup = super::record_db_query::<
                <types::ReverseLookup as HasTable>::Table,
                _,
                _,
                _,
            >(query.get_result_async(conn.get()), operation, pool)
            .await?;
            Ok(reverse_lookup)
        }
    }

    async fn delete_reverse_lookup(
        &self,
        lookup_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            return Box::pin(super::kv::delete_resource_by_id::<types::ReverseLookup>(
                self,
                super::kv::impls::reverse_lookup::ReverseLookupPrimaryKey {
                    lookup_id: lookup_id.to_string(),
                },
            ))
            .await;
        }

        #[cfg(not(feature = "kv"))]
        {
            let conn = self.get_conn().await?;
            let query = diesel::delete(types::ReverseLookup::table())
                .filter(schema::reverse_lookup::lookup_id.eq(lookup_id.to_owned()));

            let pool = conn.pool();
            let operation = DbOperation::Delete;
            super::log_db_query::<<types::ReverseLookup as HasTable>::Table, _>(
                &query, operation, pool,
            );

            let output = super::record_db_query_rows::<
                <types::ReverseLookup as HasTable>::Table,
                _,
                _,
            >(query.execute_async(conn.get()), operation, pool)
            .await?;
            Ok(output)
        }
    }
}
