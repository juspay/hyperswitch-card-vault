use diesel::BoolExpressionMethods;
use diesel::{associations::HasTable, ExpressionMethods, QueryDsl};
use diesel_async::{AsyncConnection, RunQueryDsl};
use masking::ExposeInterface;
use masking::Secret;

use crate::{
    crypto::{
        encryption_manager::managers::aes,
        hash_manager::{hash_interface::Encode, managers::sha::HmacSha512},
    },
    error::{self, ContainerError, ResultContainerExt},
};

use super::types::StorageDecryption;
use super::types::StorageEncryption;
use super::{consts, schema, types, utils, LockerInterface, MerchantInterface, Storage};

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
        let output = self.find_by_data_hash(&data_hash).await?;
        match output {
            Some(inner) => Ok(inner),
            None => {
                let mut conn = self.get_conn().await?;
                let query =
                    diesel::insert_into(types::HashTable::table()).values(types::HashTableNew {
                        hash_id: utils::generate_uuid(),
                        data_hash,
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
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        let algo = HmacSha512::<1>::new(key.map(|inner| inner.into_bytes()));

        let fingerprint_hash = algo.encode(data.expose().into_bytes().into())?;

        let output = self
            .find_by_fingerprint_hash(fingerprint_hash.clone())
            .await?;
        match output {
            Some(inner) => Ok(inner),
            None => {
                let mut conn = self.get_conn().await?;
                let query = diesel::insert_into(types::Fingerprint::table()).values(
                    types::FingerprintTableNew {
                        fingerprint_hash,
                        fingerprint_id: utils::generate_nano_id(consts::ID_LENGTH).into(),
                    },
                );

                Ok(query
                    .get_result(&mut conn)
                    .await
                    .change_error(error::StorageError::InsertError)?)
            }
        }
    }
}

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
