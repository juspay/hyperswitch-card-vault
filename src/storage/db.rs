use async_bb8_diesel::AsyncRunQueryDsl;
use diesel::BoolExpressionMethods;
use diesel::{associations::HasTable, ExpressionMethods, QueryDsl};
use masking::ExposeInterface;
use masking::Secret;

use crate::crypto::aes::{generate_aes256_key, GcmAes256};
use crate::error::{self, ContainerError, ResultContainerExt};

use super::types::StorageDecryption;
use super::types::StorageEncryption;
use super::{schema, types, LockerInterface, MerchantInterface, Storage};

#[async_trait::async_trait]
impl MerchantInterface for Storage {
    type Algorithm = GcmAes256;
    type Error = error::MerchantDBError;

    async fn find_by_merchant_id(
        &self,
        merchant_id: String,
        tenant_id: String,
        key: &GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;
        let output: Result<types::MerchantInner, diesel::result::Error> =
            types::MerchantInner::table()
                .filter(
                    schema::merchant::merchant_id
                        .eq(merchant_id)
                        .and(schema::merchant::tenant_id.eq(tenant_id)),
                )
                .get_result_async(&*conn)
                .await;
        output
            .change_error(error::StorageError::FindError)
            .map_err(From::from)
            .and_then(|inner| {
                Ok(inner.decrypt(key)?)
                // .change_context(error::StorageError::DecryptionError)
            })
    }

    async fn find_or_create_by_merchant_id(
        &self,
        merchant_id: String,
        tenant_id: String,
        key: &GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;

        let output: Result<types::MerchantInner, diesel::result::Error> =
            types::MerchantInner::table()
                .filter(
                    schema::merchant::merchant_id
                        .eq(merchant_id.to_string())
                        .and(schema::merchant::tenant_id.eq(tenant_id.to_string())),
                )
                .get_result_async(&*conn)
                .await;
        match output {
            Ok(inner) => Ok(inner.decrypt(key)?),
            Err(inner_err) => match inner_err {
                diesel::result::Error::NotFound => {
                    self.insert_merchant(
                        types::MerchantNew {
                            merchant_id,
                            tenant_id,
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
        new: types::MerchantNew,
        key: &GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;
        let query = diesel::insert_into(types::MerchantInner::table()).values(new.encrypt(key)?);

        query
            .get_result_async(&*conn)
            .await
            .change_error(error::StorageError::InsertError)
            .map_err(From::from)
            .and_then(|inner: types::MerchantInner| Ok(inner.decrypt(key)?))
    }
}

#[async_trait::async_trait]
impl LockerInterface for Storage {
    type Algorithm = GcmAes256;
    type Error = error::LockerDBError;

    async fn find_by_locker_id_merchant_id_customer_id(
        &self,
        locker_id: Secret<String>,
        tenant_id: String,
        merchant_id: String,
        customer_id: String,
        key: &Self::Algorithm,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;

        types::LockerInner::table()
            .filter(
                schema::locker::locker_id
                    .eq(locker_id.expose())
                    .and(schema::locker::tenant_id.eq(tenant_id))
                    .and(schema::locker::merchant_id.eq(merchant_id))
                    .and(schema::locker::customer_id.eq(customer_id)),
            )
            .get_result_async(&*conn)
            .await
            .change_error(error::StorageError::FindError)
            .map_err(From::from)
            .and_then(|inner: types::LockerInner| Ok(inner.decrypt(key)?))
    }

    async fn find_by_hash_id_merchant_id_customer_id(
        &self,
        hash_id: String,
        tenant_id: String,
        merchant_id: String,
        customer_id: String,
        key: &Self::Algorithm,
    ) -> Result<Option<types::Locker>, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;

        let output: Result<types::LockerInner, diesel::result::Error> = types::LockerInner::table()
            .filter(
                schema::locker::hash_id
                    .eq(hash_id)
                    .and(schema::locker::tenant_id.eq(tenant_id))
                    .and(schema::locker::merchant_id.eq(merchant_id))
                    .and(schema::locker::customer_id.eq(customer_id)),
            )
            .get_result_async(&*conn)
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
        new: types::LockerNew,
        key: &Self::Algorithm,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;
        let cloned_new = new.clone();

        let query: Result<_, diesel::result::Error> =
            diesel::insert_into(types::LockerInner::table())
                .values(new.encrypt(key)?)
                .get_result_async::<types::LockerInner>(&*conn)
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
                        cloned_new.tenant_id,
                        cloned_new.merchant_id,
                        cloned_new.customer_id,
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
        tenant_id: String,
        merchant_id: String,
        customer_id: String,
    ) -> Result<usize, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;

        let query = diesel::delete(types::LockerInner::table()).filter(
            schema::locker::locker_id
                .eq(locker_id.expose())
                .and(schema::locker::tenant_id.eq(tenant_id))
                .and(schema::locker::merchant_id.eq(merchant_id))
                .and(schema::locker::customer_id.eq(customer_id)),
        );

        Ok(query
            .execute_async(&*conn)
            .await
            .change_error(error::StorageError::DeleteError)?)
    }
}

#[async_trait::async_trait]
impl super::HashInterface for Storage {
    type Error = error::HashDBError;

    async fn find_by_data_hash(
        &self,
        data_hash: Vec<u8>,
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>> {
        let conn = self.get_conn().await?;

        let output: Result<_, diesel::result::Error> = types::HashTable::table()
            .filter(schema::hash_table::data_hash.eq(data_hash))
            .get_result_async(&*conn)
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
        let output = self.find_by_data_hash(data_hash.to_vec()).await?;
        match output {
            Some(inner) => Ok(inner),
            None => {
                let conn = self.get_conn().await?;
                let query =
                    diesel::insert_into(types::HashTable::table()).values(types::HashTableNew {
                        hash_id: uuid::Uuid::new_v4().to_string(),
                        data_hash,
                    });

                Ok(query
                    .get_result_async(&*conn)
                    .await
                    .change_error(error::StorageError::InsertError)?)
            }
        }
    }
}
