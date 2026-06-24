use diesel::{
    BoolExpressionMethods, ExpressionMethods, OptionalExtension, QueryDsl, associations::HasTable,
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use hyperswitch_masking::{ExposeInterface, Secret};

use super::{
    MerchantInterface, Storage, schema, types,
    types::{StorageDecryption, StorageEncryption},
    utils,
};
use crate::{
    crypto::encryption_manager::managers::aes,
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
        // Reads are routed to the read replica when enabled (#171).
        let mut conn = self.route_conn().await?;

        // Single SELECT; a missing row surfaces (via `?`) as `MerchantDBError::NotFoundError`.
        // Decryption of the stored DEK is the merchant-specific envelope.
        let inner: types::MerchantInner = types::MerchantInner::table()
            .filter(schema::merchant::merchant_id.eq(merchant_id))
            .get_result(&mut conn)
            .await?;

        Ok(inner.decrypt(key)?)
    }

    async fn insert_merchant(
        &self,
        new: types::MerchantNew<'_>,
        key: &aes::GcmAes256,
    ) -> Result<types::Merchant, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let inner: types::MerchantInner = diesel::insert_into(types::MerchantInner::table())
            .values(new.encrypt(key)?)
            .get_result(&mut conn)
            .await?;

        Ok(inner.decrypt(key)?)
    }

    async fn find_all_keys_excluding_entity_keys(
        &self,
        key: &Self::Algorithm,
        limit: i64,
    ) -> Result<Vec<types::Merchant>, ContainerError<Self::Error>> {
        let mut conn = self.route_conn().await?;

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

impl super::LockerInterface for Storage {
    type Error = error::VaultDBError;

    async fn insert_locker(
        &self,
        new: types::LockerNew<'_>,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: types::LockerInner = diesel::insert_into(types::LockerInner::table())
            .values(new)
            .get_result(&mut conn)
            .await?;

        Ok(output.into())
    }

    async fn find_by_locker_id_merchant_id_customer_id(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<types::Locker, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        // A missing row surfaces (via `?`) as `VaultDBError::NotFoundError`.
        let output: types::LockerInner = types::LockerInner::table()
            .filter(
                schema::locker::locker_id
                    .eq(locker_id.expose())
                    .and(schema::locker::merchant_id.eq(merchant_id))
                    .and(schema::locker::customer_id.eq(customer_id)),
            )
            .get_result(&mut conn)
            .await?;

        Ok(output.into())
    }

    async fn find_optional_by_hash_id_merchant_id_customer_id(
        &self,
        hash_id: &str,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<Option<types::Locker>, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output = types::LockerInner::table()
            .filter(
                schema::locker::hash_id
                    .eq(hash_id)
                    .and(schema::locker::merchant_id.eq(merchant_id))
                    .and(schema::locker::customer_id.eq(customer_id)),
            )
            .get_result::<types::LockerInner>(&mut conn)
            .await
            .optional()?;

        Ok(output.map(From::from))
    }

    async fn delete_locker(
        &self,
        locker_id: Secret<String>,
        merchant_id: &str,
        customer_id: &str,
    ) -> Result<usize, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output = diesel::delete(types::LockerInner::table())
            .filter(
                schema::locker::locker_id
                    .eq(locker_id.expose())
                    .and(schema::locker::merchant_id.eq(merchant_id))
                    .and(schema::locker::customer_id.eq(customer_id)),
            )
            .execute(&mut conn)
            .await?;

        Ok(output)
    }
}

impl super::HashInterface for Storage {
    type Error = error::HashDBError;

    async fn find_optional_by_data_hash(
        &self,
        data_hash: &[u8],
    ) -> Result<Option<types::HashTable>, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output = types::HashTable::table()
            .filter(schema::hash_table::data_hash.eq(data_hash))
            .get_result::<types::HashTable>(&mut conn)
            .await
            .optional()?;

        Ok(output)
    }

    async fn insert_hash(
        &self,
        data_hash: Vec<u8>,
    ) -> Result<types::HashTable, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: types::HashTable = diesel::insert_into(types::HashTable::table())
            .values(types::HashTableNew {
                hash_id: utils::generate_uuid(),
                data_hash,
            })
            .get_result(&mut conn)
            .await?;

        Ok(output)
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

    async fn test_replica(&self) -> Result<(), ContainerError<Self::Error>> {
        let mut conn = self.get_replica_conn().await?;

        let query = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("1 + 1"));
        let _x: i32 = query
            .get_result(&mut conn)
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
        let mut conn = self.get_conn().await?;

        let output = types::Fingerprint::table()
            .filter(schema::fingerprint::fingerprint_hash.eq(fingerprint_hash))
            .get_result::<types::Fingerprint>(&mut conn)
            .await
            .optional()?;

        Ok(output)
    }

    async fn insert_fingerprint(
        &self,
        fingerprint_hash: Secret<Vec<u8>>,
        fingerprint_id: Secret<String>,
    ) -> Result<types::Fingerprint, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: types::Fingerprint = diesel::insert_into(types::Fingerprint::table())
            .values(types::FingerprintTableNew {
                fingerprint_hash,
                fingerprint_id,
            })
            .get_result(&mut conn)
            .await?;

        Ok(output)
    }
}

#[cfg(feature = "external_key_manager")]
impl super::EntityInterface for Storage {
    type Error = error::EntityDBError;

    async fn find_by_entity_id(
        &self,
        entity_id: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>> {
        // Reads are routed to the read replica when enabled (#171).
        let mut conn = self.route_conn().await?;

        // A missing row surfaces as `EntityDBError::NotFoundError` (see the `From<diesel>`
        // classifier), which `find_or_create_entity` in the key manager checks via
        // `is_not_found()`.
        let output: types::Entity = types::Entity::table()
            .filter(schema::entity::entity_id.eq(entity_id))
            .get_result(&mut conn)
            .await?;

        Ok(output)
    }

    async fn insert_entity(
        &self,
        entity_id: &str,
        identifier: &str,
    ) -> Result<types::Entity, ContainerError<Self::Error>> {
        let mut conn = self.get_conn().await?;

        let output: types::Entity = diesel::insert_into(types::Entity::table())
            .values(types::EntityTableNew {
                entity_id: entity_id.into(),
                enc_key_id: identifier.into(),
            })
            .get_result(&mut conn)
            .await?;

        Ok(output)
    }
}
