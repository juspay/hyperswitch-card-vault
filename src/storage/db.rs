use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::{AsyncConnection, RunQueryDsl};
use hyperswitch_masking::{ExposeInterface, Secret};

use super::{
    LockerInterface, MerchantInterface, Storage, consts, schema, types,
    types::{StorageDecryption, StorageEncryption},
    utils,
};
use crate::{
    crypto::{
        encryption_manager::managers::aes,
        hash_manager::{hash_interface::Encode, managers::sha::HmacSha512},
    },
    error::{self, ContainerError, ResultContainerExt},
};

impl MerchantInterface for Storage {
    type Algorithm = aes::GcmAes256;
    type Error = error::MerchantDBError;

    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &aes::GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;
        let output: Result<types::MerchantInner, diesel::result::Error> =
            types::MerchantInner::table()
                .filter(schema::merchant::merchant_id.eq(merchant_id))
                .get_result(&mut conn)
                .await;

        let output = match output {
            Err(err) => match err {
                diesel::result::Error::NotFound => {
                    Err(err).change_error(error::StorageError::NotFoundError)
                }
                _ => Err(err).change_error(error::StorageError::FindError),
            },
            Ok(merchant) => Ok(merchant),
        };

        output
            .map_err(From::from)
            .and_then(|inner| Ok(inner.decrypt(key)?))
    }

    async fn find_or_create_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &aes::GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: Result<types::MerchantInner, diesel::result::Error> =
            types::MerchantInner::table()
                .filter(schema::merchant::merchant_id.eq(merchant_id))
                .get_result(&mut conn)
                .await;
        match output {
            Ok(inner) => Ok(inner.decrypt(key)?),
            Err(inner_err) => match inner_err {
                diesel::result::Error::NotFound => {
                    self.insert_merchant(
                        types::MerchantNew {
                            merchant_id,
                            enc_key: aes::generate_aes256_key().to_vec().into(),
                        },
                        key,
                    )
                    .await
                }
                output => Err(output).change_error(error::StorageError::FindError)?,
            },
        }
    }

    async fn insert_merchant(
        &self,
        new: types::MerchantNew<'_>,
        key: &aes::GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;
        let query = diesel::insert_into(types::MerchantInner::table()).values(new.encrypt(key)?);

        query
            .get_result(&mut conn)
            .await
            .change_error(error::StorageError::InsertError)
            .map_err(From::from)
            .and_then(|inner: types::MerchantInner| Ok(inner.decrypt(key)?))
    }

    async fn find_all_keys_excluding_entity_keys(
        &self,
        key: &Self::Algorithm,
        limit: i64,
    ) -> Result<Vec<types::Merchant>, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let result: Result<Vec<types::MerchantInner>, ContainerError<Self::Error>> =
            schema::merchant::table
                .filter(
                    schema::merchant::merchant_id
                        .ne_all(schema::entity::table.select(schema::entity::entity_id)),
                )
                .limit(limit)
                .load::<types::MerchantInner>(&mut conn)
                .await
                .change_error(error::StorageError::FindError)
                .map_err(From::from);

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

impl LockerInterface for Storage {
    type Error = error::VaultDBError;

    async fn find_by_locker_id_merchant_id_customer_id(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: Result<types::LockerInner, diesel::result::Error> = types::LockerInner::table()
            .filter(
                schema::locker::locker_id
                    .eq(locker_id.expose())
                    .and(schema::locker::merchant_id.eq(merchant_id))
                    .and(schema::locker::customer_id.eq(customer_id)),
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
            Ok(locker) => Ok(locker),
        };

        output.map_err(From::from).map(From::from)
    }

    async fn find_by_hash_id_merchant_id_customer_id(
        &self,
        hash_id: &str,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<Option<types::Locker>, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: Result<types::LockerInner, diesel::result::Error> = types::LockerInner::table()
            .filter(
                schema::locker::hash_id
                    .eq(hash_id)
                    .and(schema::locker::merchant_id.eq(merchant_id))
                    .and(schema::locker::customer_id.eq(customer_id)),
            )
            .get_result(&mut conn)
            .await;

        match output {
            Ok(inner) => Ok(Some(inner.into())),
            Err(err) => match err {
                diesel::result::Error::NotFound => Ok(None),
                error => Err(error).change_error(error::StorageError::FindError)?,
            },
        }
    }

    async fn insert_or_get_from_locker(
        &self,
        new: types::LockerNew<'_>,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;
        let cloned_new = new.clone();

        let query: Result<_, diesel::result::Error> =
            diesel::insert_into(types::LockerInner::table())
                .values(new)
                .get_result::<types::LockerInner>(&mut conn)
                .await;

        match query {
            Ok(inner) => Ok(inner.into()),
            Err(error) => match error {
                diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::UniqueViolation,
                    _,
                ) => {
                    self.find_by_locker_id_merchant_id_customer_id(
                        cloned_new.locker_id,
                        &cloned_new.merchant_id,
                        &cloned_new.customer_id,
                    )
                    .await
                }
                error => Err(error).change_error(error::StorageError::InsertError)?,
            },
        }
    }

    async fn delete_from_locker(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let query = diesel::delete(types::LockerInner::table()).filter(
            schema::locker::locker_id
                .eq(locker_id.expose())
                .and(schema::locker::merchant_id.eq(merchant_id))
                .and(schema::locker::customer_id.eq(customer_id)),
        );

        Ok(query
            .execute(&mut conn)
            .await
            .change_error(error::StorageError::DeleteError)?)
    }
}

impl super::HashInterface for Storage {
    type Error = error::HashDBError;

    async fn find_by_data_hash(
        &self,
        data_hash: &[u8],
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings_for(super::kv::KvTable::HashTable);
            let scheme = super::kv::decide_storage_scheme(settings, super::kv::Op::Find);
            if matches!(scheme, super::kv::StorageScheme::RedisKv) {
                return self.find_by_data_hash_kv(data_hash).await;
            }
        }

        let mut conn = self.get_conn().await?;

        let output: Result<_, diesel::result::Error> = types::HashTable::table()
            .filter(schema::hash_table::data_hash.eq(data_hash))
            .get_result(&mut conn)
            .await;

        match output {
            Ok(inner) => Ok(Some(inner)),
            Err(inner_err) => match inner_err {
                diesel::result::Error::NotFound => Ok(None),
                error => Err(error).change_error(error::StorageError::FindError)?,
            },
        }
    }
    async fn insert_hash(
        &self,
        data_hash: Vec<u8>,
    ) -> Result<types::HashTable, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings_for(super::kv::KvTable::HashTable);
            let scheme = super::kv::decide_storage_scheme(settings, super::kv::Op::Insert);
            if matches!(scheme, super::kv::StorageScheme::RedisKv) {
                return self.insert_hash_kv(data_hash).await;
            }
        }

        let output = self.find_by_data_hash(&data_hash).await?;
        match output {
            Some(inner) => Ok(inner),
            None => {
                let mut conn = self.get_conn().await?;
                let query =
                    diesel::insert_into(types::HashTable::table()).values(types::HashTableNew {
                        hash_id: utils::generate_uuid(),
                        data_hash,
                        updated_by: "postgres_only".to_string(),
                    });

                Ok(query
                    .get_result(&mut conn)
                    .await
                    .change_error(error::StorageError::InsertError)?)
            }
        }
    }
}

impl super::TestInterface for Storage {
    type Error = error::TestDBError;

    async fn test(&self) -> Result<(), ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let _data = conn
            .test_transaction(|x| {
                Box::pin(async {
                    let query =
                        diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("1 + 1"));
                    let _x: i32 = query
                        .get_result(x)
                        .await
                        .change_error(error::StorageError::FindError)?;

                    diesel::insert_into(types::HashTable::table())
                        .values(types::HashTableNew {
                            hash_id: "test".to_string(),
                            data_hash: b"0".to_vec(),
                            updated_by: "postgres_only".to_string(),
                        })
                        .execute(x)
                        .await
                        .change_error(error::StorageError::InsertError)?;

                    diesel::delete(
                        types::HashTable::table()
                            .filter(schema::hash_table::hash_id.eq("test".to_string())),
                    )
                    .execute(x)
                    .await
                    .change_error(error::StorageError::DeleteError)?;

                    Ok::<_, ContainerError<Self::Error>>(())
                })
            })
            .await;

        Ok(())
    }
}

impl super::FingerprintInterface for Storage {
    type Error = error::FingerprintDBError;

    async fn find_by_fingerprint_hash(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings_for(super::kv::KvTable::Fingerprint);
            let scheme = super::kv::decide_storage_scheme(settings, super::kv::Op::Find);
            if matches!(scheme, super::kv::StorageScheme::RedisKv) {
                return self.find_by_fingerprint_hash_kv(&fingerprint_hash).await;
            }
        }

        let mut conn = self.get_conn().await?;

        let output: Result<_, diesel::result::Error> = types::Fingerprint::table()
            .filter(schema::fingerprint::fingerprint_hash.eq(fingerprint_hash))
            .get_result(&mut conn)
            .await;

        match output {
            Ok(inner) => Ok(Some(inner)),
            Err(inner_err) => match inner_err {
                diesel::result::Error::NotFound => Ok(None),
                error => Err(error).change_error(error::StorageError::FindError)?,
            },
        }
    }
    async fn get_or_insert_fingerprint(
        &self,
        data: Secret<String>,
        key: Secret<String>,
        fingerprint_id: Option<Secret<String>>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        #[cfg(feature = "kv")]
        {
            let settings = self.kv_settings_for(super::kv::KvTable::Fingerprint);
            let scheme = super::kv::decide_storage_scheme(settings, super::kv::Op::Insert);
            if matches!(scheme, super::kv::StorageScheme::RedisKv) {
                return self.get_or_insert_fingerprint_kv(data, key, fingerprint_id).await;
            }
        }

        let algo = HmacSha512::<1>::new(key.map(|inner| inner.into_bytes()));

        let fingerprint_hash = algo.encode(data.expose().into_bytes().into())?;

        let output = self
            .find_by_fingerprint_hash(fingerprint_hash.clone())
            .await?;
        match output {
            // Hash already exists: return the stored fingerprint regardless of any
            // caller-supplied id — the hash is the canonical deduplication key.
            Some(inner) => Ok(inner),
            None => {
                let id = fingerprint_id
                    .unwrap_or_else(|| utils::generate_nano_id(consts::ID_LENGTH).into());
                let cloned_hash = fingerprint_hash.clone();
                let mut conn = self.get_conn().await?;

                let insert_result: Result<types::Fingerprint, diesel::result::Error> =
                    diesel::insert_into(types::Fingerprint::table())
                        .values(types::FingerprintTableNew {
                            fingerprint_hash,
                            fingerprint_id: id,
                            updated_by: "postgres_only".to_string(),
                        })
                        .get_result(&mut conn)
                        .await;

                match insert_result {
                    Ok(inner) => Ok(inner),
                    // Race condition: a concurrent request inserted the same hash first.
                    // Re-read by hash and return the winner row.
                    Err(diesel::result::Error::DatabaseError(
                        diesel::result::DatabaseErrorKind::UniqueViolation,
                        _,
                    )) => self
                        .find_by_fingerprint_hash(cloned_hash)
                        .await?
                        .ok_or_else(|| {
                            ContainerError::from(error::FingerprintDBError::DBInsertError)
                        }),
                    Err(error) => Err(error).change_error(error::StorageError::InsertError)?,
                }
            }
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
        let mut conn = self.get_conn().await?;
        let output: Result<types::Entity, diesel::result::Error> = types::Entity::table()
            .filter(schema::entity::entity_id.eq(entity_id))
            .get_result(&mut conn)
            .await;

        let output = match output {
            Err(err) => match err {
                diesel::result::Error::NotFound => {
                    Err(err).change_error(error::StorageError::NotFoundError)
                }
                _ => Err(err).change_error(error::StorageError::FindError),
            },
            Ok(entity) => Ok(entity),
        };

        output.map_err(From::from)
    }

    async fn insert_entity(
        &self,
        entity_id: &str,
        identifier: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;
        let query = diesel::insert_into(types::Entity::table()).values(types::EntityTableNew {
            entity_id: entity_id.into(),
            enc_key_id: identifier.into(),
        });

        query
            .get_result(&mut conn)
            .await
            .change_error(error::StorageError::InsertError)
            .map_err(From::from)
    }
}

// ─── KV helpers (gated by `kv` feature) ─────────────────────────────────────
//
// The KV read/insert paths reconstruct domain rows (`Fingerprint`, `HashTable`)
// from the value cached in Redis.  Until the drainer replays the write into
// Postgres, the surrogate columns Redis does not store are not authoritative:
//   - `id`        is fabricated as `0`
//   - `created_at` is fabricated as `time::PrimitiveDateTime::MIN`
// Callers must not rely on these fields for rows served from the KV path; they
// become valid only after the row is drained to Postgres and re-read from
// there.

#[cfg(feature = "kv")]
mod kv_helpers {
    use error_stack::ResultExt;
    use hyperswitch_masking::PeekInterface;

    use super::*;
    use crate::error::RedisErrorExt;
    use crate::storage::kv::{
        KvOperation, KvResult, PartitionKey, StorageScheme, kv_wrapper, serializable_query,
        try_redis_get_else_try_database_get,
    };

    impl Storage {
        // ── fingerprint ──

        pub(crate) async fn find_by_fingerprint_hash_kv(
            &self,
            fingerprint_hash: &Secret<Vec<u8>>,
        ) -> Result<Option<types::Fingerprint>, ContainerError<error::FingerprintDBError>> {
            let partition_key = PartitionKey::Fingerprint {
                fingerprint_hash: fingerprint_hash.peek().as_slice(),
            };

            let result = try_redis_get_else_try_database_get(
                async {
                    let kv_result =
                        kv_wrapper::<types::FingerprintTableNew, types::FingerprintTableNew>(
                            self,
                            KvOperation::<types::FingerprintTableNew>::Get,
                            partition_key,
                        )
                        .await?;
                    match kv_result {
                        KvResult::Get(v) => Ok(types::Fingerprint {
                            id: 0,
                            fingerprint_hash: v.fingerprint_hash,
                            fingerprint_id: v.fingerprint_id,
                            updated_by: v.updated_by,
                        }),
                        _ => Err(error_stack::Report::new(
                            hyperswitch_redis_interface::errors::RedisError::UnknownResult,
                        )),
                    }
                },
                || async {
                    self.find_by_fingerprint_hash_pg(fingerprint_hash.clone())
                        .await?
                        .ok_or_else(|| {
                            error_stack::Report::new(error::StorageError::ValueNotFound(
                                "fingerprint".to_string(),
                            ))
                        })
                },
            )
            .await;

            match result {
                Ok(v) => Ok(Some(v)),
                Err(err) => match err.current_context() {
                    error::StorageError::ValueNotFound(_) => Ok(None),
                    _ => Err(err
                        .change_context(error::FingerprintDBError::DBFilterError)
                        .into()),
                },
            }
        }

        pub(crate) async fn find_by_fingerprint_hash_pg(
            &self,
            fingerprint_hash: Secret<Vec<u8>>,
        ) -> error_stack::Result<Option<types::Fingerprint>, error::StorageError> {
            let mut conn = self
                .get_conn()
                .await
                .change_context(error::StorageError::PoolClientFailure)?;
            let output: Result<_, diesel::result::Error> = types::Fingerprint::table()
                .filter(schema::fingerprint::fingerprint_hash.eq(fingerprint_hash))
                .get_result(&mut conn)
                .await;
            match output {
                Ok(inner) => Ok(Some(inner)),
                Err(diesel::result::Error::NotFound) => Ok(None),
                Err(err) => Err(error_stack::Report::new(err))
                    .change_context(error::StorageError::FindError),
            }
        }

        pub(crate) async fn get_or_insert_fingerprint_kv(
            &self,
            data: Secret<String>,
            key: Secret<String>,
            fingerprint_id: Option<Secret<String>>,
        ) -> Result<types::Fingerprint, ContainerError<error::FingerprintDBError>> {
            let algo = HmacSha512::<1>::new(key.map(|inner| inner.into_bytes()));
            let fingerprint_hash = algo.encode(data.expose().into_bytes().into())?;

            // Try find first
            if let Some(existing) = self.find_by_fingerprint_hash_kv(&fingerprint_hash).await? {
                return Ok(existing);
            }

            // Not found — insert via SetNx + drainer
            let id = fingerprint_id
                .unwrap_or_else(|| utils::generate_nano_id(consts::ID_LENGTH).into());
            let new_fingerprint = types::FingerprintTableNew {
                fingerprint_hash: fingerprint_hash.clone(),
                fingerprint_id: id,
                updated_by: StorageScheme::RedisKv.to_string(),
            };

            let partition_key = PartitionKey::Fingerprint {
                fingerprint_hash: fingerprint_hash.peek().as_slice(),
            };
            let key_str = partition_key.to_string();

            let drainer_query =
                serializable_query::generate_insert_query::<schema::fingerprint::table, _>(
                    new_fingerprint.clone(),
                )
                .change_context(error::FingerprintDBError::DBInsertError)?;

            let result = kv_wrapper::<(), types::FingerprintTableNew>(
                self,
                KvOperation::SetNx(&new_fingerprint, drainer_query),
                partition_key,
            )
            .await
            .map_err(|err| {
                ContainerError::from(
                    err.to_redis_failed_response(&key_str)
                        .change_context(error::FingerprintDBError::DBInsertError),
                )
            })?;

            match result.try_into_setnx() {
                Ok(hyperswitch_redis_interface::types::SetnxReply::KeySet) => {
                    Ok(types::Fingerprint {
                        id: 0,
                        fingerprint_hash,
                        fingerprint_id: new_fingerprint.fingerprint_id,
                        updated_by: new_fingerprint.updated_by,
                    })
                }
                Ok(hyperswitch_redis_interface::types::SetnxReply::KeyNotSet) => {
                    self.find_by_fingerprint_hash_kv(&fingerprint_hash)
                        .await?
                        .ok_or_else(|| {
                            ContainerError::from(error::FingerprintDBError::DBInsertError)
                        })
                }
                Err(_) => Err(ContainerError::from(error::FingerprintDBError::DBInsertError)),
            }
        }

        // ── hash_table ──

        pub(crate) async fn find_by_data_hash_kv(
            &self,
            data_hash: &[u8],
        ) -> Result<Option<types::HashTable>, ContainerError<error::HashDBError>> {
            let partition_key = PartitionKey::Hash { data_hash };

            let result = try_redis_get_else_try_database_get(
                async {
                    let kv_result =
                        kv_wrapper::<types::HashTableNew, types::HashTableNew>(
                            self,
                            KvOperation::<types::HashTableNew>::Get,
                            partition_key,
                        )
                        .await?;
                    match kv_result {
                        KvResult::Get(v) => Ok(types::HashTable {
                            id: 0,
                            hash_id: v.hash_id,
                            data_hash: v.data_hash,
                            created_at: time::PrimitiveDateTime::MIN,
                            updated_by: v.updated_by,
                        }),
                        _ => Err(error_stack::Report::new(
                            hyperswitch_redis_interface::errors::RedisError::UnknownResult,
                        )),
                    }
                },
                || async {
                    self.find_by_data_hash_pg(data_hash.to_vec())
                        .await?
                        .ok_or_else(|| {
                            error_stack::Report::new(error::StorageError::ValueNotFound(
                                "hash_table".to_string(),
                            ))
                        })
                },
            )
            .await;

            match result {
                Ok(v) => Ok(Some(v)),
                Err(err) => match err.current_context() {
                    error::StorageError::ValueNotFound(_) => Ok(None),
                    _ => Err(err.change_context(error::HashDBError::DBFilterError).into()),
                },
            }
        }

        pub(crate) async fn find_by_data_hash_pg(
            &self,
            data_hash: Vec<u8>,
        ) -> error_stack::Result<Option<types::HashTable>, error::StorageError> {
            let mut conn = self
                .get_conn()
                .await
                .change_context(error::StorageError::PoolClientFailure)?;
            let output: Result<_, diesel::result::Error> = types::HashTable::table()
                .filter(schema::hash_table::data_hash.eq(data_hash))
                .get_result(&mut conn)
                .await;
            match output {
                Ok(inner) => Ok(Some(inner)),
                Err(diesel::result::Error::NotFound) => Ok(None),
                Err(err) => Err(error_stack::Report::new(err))
                    .change_context(error::StorageError::FindError),
            }
        }

        pub(crate) async fn insert_hash_kv(
            &self,
            data_hash: Vec<u8>,
        ) -> Result<types::HashTable, ContainerError<error::HashDBError>> {
            // Try find first
            if let Some(existing) = self.find_by_data_hash_kv(&data_hash).await? {
                return Ok(existing);
            }

            // Not found — insert via SetNx + drainer
            let new_hash = types::HashTableNew {
                hash_id: utils::generate_uuid(),
                data_hash: data_hash.clone(),
                updated_by: StorageScheme::RedisKv.to_string(),
            };

            let partition_key = PartitionKey::Hash {
                data_hash: &data_hash,
            };
            let key_str = partition_key.to_string();

            let drainer_query =
                serializable_query::generate_insert_query::<schema::hash_table::table, _>(
                    new_hash.clone(),
                )
                .change_context(error::HashDBError::DBInsertError)?;

            let result = kv_wrapper::<(), types::HashTableNew>(
                self,
                KvOperation::SetNx(&new_hash, drainer_query),
                partition_key,
            )
            .await
            .map_err(|err| {
                ContainerError::from(
                    err.to_redis_failed_response(&key_str)
                        .change_context(error::HashDBError::DBInsertError),
                )
            })?;

            match result.try_into_setnx() {
                Ok(hyperswitch_redis_interface::types::SetnxReply::KeySet) => {
                    Ok(types::HashTable {
                        id: 0,
                        hash_id: new_hash.hash_id,
                        data_hash,
                        created_at: time::PrimitiveDateTime::MIN,
                        updated_by: new_hash.updated_by,
                    })
                }
                Ok(hyperswitch_redis_interface::types::SetnxReply::KeyNotSet) => {
                    self.find_by_data_hash_kv(&data_hash)
                        .await?
                        .ok_or_else(|| ContainerError::from(error::HashDBError::DBInsertError))
                }
                Err(_) => Err(ContainerError::from(error::HashDBError::DBInsertError)),
            }
        }
    }
}
