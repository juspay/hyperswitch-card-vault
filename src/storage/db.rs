use diesel::BoolExpressionMethods;
use diesel::{associations::HasTable, ExpressionMethods, QueryDsl};
use diesel_async::{AsyncConnection, RunQueryDsl};
use masking::ExposeInterface;
use masking::Secret;

use crate::{
    crypto::{
        encryption_manager::managers::aes::generate_aes256_key,
        encryption_manager::managers::aes::GcmAes256,
        hash_manager::{hash_interface::Encode, managers::sha::HmacSha512},
    },
    error::{self, ContainerError, ResultContainerExt},
};

use super::types::StorageDecryption;
use super::types::StorageEncryption;
use super::{consts, schema, types, utils, LockerInterface, MerchantInterface, Storage};

impl MerchantInterface for Storage {
    type Algorithm = GcmAes256;
    type Error = error::MerchantDBError;

    async fn find_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;
        let output: Result<types::MerchantInner, diesel::result::Error> =
            types::MerchantInner::table()
                .filter(schema::merchant::merchant_id.eq(merchant_id))
                .get_result(&mut conn)
                .await;
        output
            .map_err(|error| match error {
                diesel::result::Error::NotFound => error::StorageError::NotFoundError,
                _ => error::StorageError::FindError,
            })
            .map_err(error::ContainerError::from)
            .map_err(From::from)
            .and_then(|inner| Ok(inner.decrypt(key)?))
    }

    async fn find_or_create_by_merchant_id(
        &self,
        merchant_id: &str,
        key: &GcmAes256,
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
                            enc_key: generate_aes256_key().to_vec().into(),
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
        key: &GcmAes256,
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
}

impl LockerInterface for Storage {
    type Algorithm = GcmAes256;
    type Error = error::LockerDBError;

    async fn find_by_locker_id_merchant_id_customer_id(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
        key: &Self::Algorithm,
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

        output
            .map_err(|error| match error {
                diesel::result::Error::NotFound => error::StorageError::NotFoundError,
                _ => error::StorageError::FindError,
            })
            .map_err(error::ContainerError::from)
            .map_err(From::from)
            .and_then(|inner| Ok(inner.decrypt(key)?))
    }

    async fn find_by_hash_id_merchant_id_customer_id(
        &self,
        hash_id: &str,
        merchant_id: &str,
        customer_id: &str,
        key: &Self::Algorithm,
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
            Ok(inner) => Ok(Some(inner.decrypt(key)?)),
            Err(err) => match err {
                diesel::result::Error::NotFound => Ok(None),
                error => Err(error).change_error(error::StorageError::FindError)?,
            },
        }
    }

    async fn insert_or_get_from_locker(
        &self,
        new: types::LockerNew<'_>,
        key: &Self::Algorithm,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;
        let cloned_new = new.clone();

        let query: Result<_, diesel::result::Error> =
            diesel::insert_into(types::LockerInner::table())
                .values(new.encrypt(key)?)
                .get_result::<types::LockerInner>(&mut conn)
                .await;

        match query {
            Ok(inner) => Ok(inner.decrypt(key)?),
            Err(error) => match error {
                diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::UniqueViolation,
                    _,
                ) => {
                    self.find_by_locker_id_merchant_id_customer_id(
                        cloned_new.locker_id,
                        &cloned_new.merchant_id,
                        &cloned_new.customer_id,
                        key,
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
                        hash_id: uuid::Uuid::new_v4().to_string(),
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

    async fn find_by_card_hash(
        &self,
        card_hash: Secret<&[u8]>,
    ) -> Result<Option<types::Fingerprint>, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: Result<_, diesel::result::Error> = types::Fingerprint::table()
            .filter(schema::fingerprint::card_hash.eq(card_hash))
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
    async fn insert_fingerprint(
        &self,
        card: types::CardNumber,
        hash_key: Secret<String>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        let algo = HmacSha512::<1>::new(hash_key.expose().into_bytes().into());

        let card_hash = algo.encode(card.into_bytes())?;

        let output = self.find_by_card_hash(Secret::new(&card_hash)).await?;
        match output {
            Some(inner) => Ok(inner),
            None => {
                let mut conn = self.get_conn().await?;
                let query = diesel::insert_into(types::Fingerprint::table()).values(
                    types::FingerprintTableNew {
                        card_hash: card_hash.into(),
                        card_fingerprint: utils::generate_id(consts::ID_LENGTH).into(),
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
