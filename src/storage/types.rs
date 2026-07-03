use diesel::{
    AsExpression, Identifiable, Insertable, Queryable,
    backend::Backend,
    deserialize::{self, FromSql},
    serialize::ToSql,
    sql_types,
};
use hyperswitch_masking::{ExposeInterface, PeekInterface, Secret, StrongSecret};

use super::{schema, scheme::StorageScheme};
use crate::{
    crypto::encryption_manager::{encryption_interface::Encryption, managers::aes::GcmAes256},
    error,
    routes::data::types::{StoreCardRequest, Validation},
};

#[derive(Debug, Identifiable, Queryable)]
#[diesel(table_name = schema::merchant)]
pub(crate) struct MerchantInner {
    id: i32,
    merchant_id: String,
    enc_key: Encrypted,
    created_at: time::PrimitiveDateTime,
}

#[derive(Debug, Clone)]
pub struct Merchant {
    pub merchant_id: String,
    pub enc_key: Secret<Vec<u8>>,
    pub created_at: time::PrimitiveDateTime,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::merchant)]
pub(crate) struct MerchantNewInner<'a> {
    pub(super) merchant_id: &'a str,
    enc_key: Encrypted,
}

#[derive(Debug)]
pub struct MerchantNew<'a> {
    pub merchant_id: &'a str,
    pub enc_key: Secret<Vec<u8>>,
}

#[derive(Debug, Identifiable, Queryable)]
#[diesel(table_name = schema::locker)]
pub(super) struct LockerInner {
    id: i32,
    locker_id: Secret<String>,
    merchant_id: String,
    customer_id: String,
    enc_data: Encrypted,
    created_at: time::PrimitiveDateTime,
    hash_id: String,
    ttl: Option<time::PrimitiveDateTime>,
    pub updated_by: StorageScheme,
}

impl From<LockerInner> for Locker {
    fn from(value: LockerInner) -> Self {
        Self {
            locker_id: value.locker_id,
            merchant_id: value.merchant_id,
            customer_id: value.customer_id,
            data: value.enc_data.into(),
            created_at: value.created_at,
            hash_id: value.hash_id,
            ttl: value.ttl,
            updated_by: value.updated_by,
        }
    }
}

#[derive(Debug)]
pub struct Locker {
    pub locker_id: Secret<String>,
    pub merchant_id: String,
    pub customer_id: String,
    pub data: Encryptable,
    pub created_at: time::PrimitiveDateTime,
    pub hash_id: String,
    pub ttl: Option<time::PrimitiveDateTime>,
    pub updated_by: StorageScheme,
}

#[derive(Debug)]
pub enum Encryptable {
    Encrypted(Secret<Vec<u8>>),
    Decrypted(StrongSecret<Vec<u8>>),
}

impl Encryptable {
    pub fn get_encrypted_inner_value(&self) -> Option<Secret<Vec<u8>>> {
        match self {
            Self::Encrypted(secret) => Some(secret.clone()),
            Self::Decrypted(_) => None,
        }
    }

    pub fn get_decrypted_inner_value(&self) -> Option<StrongSecret<Vec<u8>>> {
        match self {
            Self::Encrypted(_) => None,
            Self::Decrypted(secret) => Some(secret.clone()),
        }
    }

    pub fn from_decrypted_data(decrypted_data: StrongSecret<Vec<u8>>) -> Self {
        Self::Decrypted(decrypted_data)
    }
}

impl From<Encrypted> for Encryptable {
    fn from(value: Encrypted) -> Self {
        Self::Encrypted(value.into_inner())
    }
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = schema::locker)]
pub struct LockerNew<'a> {
    pub locker_id: Secret<String>,
    pub merchant_id: String,
    pub customer_id: String,
    pub enc_data: Encrypted,
    pub hash_id: &'a str,
    pub ttl: Option<time::PrimitiveDateTime>,
    pub updated_by: StorageScheme,
}

impl<'a> LockerNew<'a> {
    pub fn new(request: StoreCardRequest, hash_id: &'a str, enc_data: Encrypted) -> Self {
        Self {
            locker_id: request
                .requestor_card_reference
                .unwrap_or_else(super::utils::generate_uuid)
                .into(),
            merchant_id: request.merchant_id,
            customer_id: request.merchant_customer_id,
            enc_data,
            hash_id,
            ttl: *request.ttl,
            updated_by: StorageScheme::PostgresOnly,
        }
    }
}

/// Owned version of `LockerNew` for KV (Redis) operations.
///
/// `LockerNew<'a>` borrows `hash_id: &'a str`, so it cannot satisfy
/// `DeserializeOwned`. This struct owns all fields and is serde-able for
/// write-through Redis.
#[cfg(feature = "kv")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Insertable)]
#[diesel(table_name = schema::locker)]
pub struct LockerKvValue {
    pub locker_id: Secret<String>,
    pub merchant_id: String,
    pub customer_id: String,
    pub enc_data: Encrypted,
    pub hash_id: String,
    pub ttl: Option<time::PrimitiveDateTime>,
    pub updated_by: StorageScheme,
}

#[cfg(feature = "kv")]
impl<'a> From<&LockerNew<'a>> for LockerKvValue {
    fn from(new: &LockerNew<'a>) -> Self {
        Self {
            locker_id: new.locker_id.clone(),
            merchant_id: new.merchant_id.clone(),
            customer_id: new.customer_id.clone(),
            enc_data: new.enc_data.clone(),
            hash_id: new.hash_id.to_string(),
            ttl: new.ttl,
            updated_by: new.updated_by,
        }
    }
}

#[cfg(feature = "kv")]
impl From<LockerKvValue> for Locker {
    fn from(v: LockerKvValue) -> Self {
        Self {
            locker_id: v.locker_id,
            merchant_id: v.merchant_id,
            customer_id: v.customer_id,
            data: v.enc_data.into(),
            created_at: time::PrimitiveDateTime::MIN,
            hash_id: v.hash_id,
            ttl: v.ttl,
            updated_by: v.updated_by,
        }
    }
}

#[derive(Debug, Clone, Identifiable, Queryable)]
#[diesel(table_name = schema::reverse_lookup, primary_key(lookup_id))]
#[expect(dead_code)]
pub(crate) struct ReverseLookup {
    pub lookup_id: String,
    pub secondary_key: String,
    pub partition_key: String,
    pub source: String,
    pub update_by: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Insertable)]
#[diesel(table_name = schema::reverse_lookup)]
pub(crate) struct ReverseLookupNew {
    pub lookup_id: String,
    pub secondary_key: String,
    pub partition_key: String,
    pub source: String,
    pub update_by: String,
}

impl From<ReverseLookupNew> for ReverseLookup {
    fn from(value: ReverseLookupNew) -> Self {
        Self {
            lookup_id: value.lookup_id,
            secondary_key: value.secondary_key,
            partition_key: value.partition_key,
            source: value.source,
            update_by: value.update_by,
        }
    }
}

#[derive(Debug, Clone, Identifiable, Queryable)]
#[diesel(table_name = schema::hash_table)]
pub struct HashTable {
    pub id: i32,
    pub hash_id: String,
    pub data_hash: Vec<u8>,
    pub created_at: time::PrimitiveDateTime,
    pub updated_by: StorageScheme,
}

#[derive(Debug, Clone, Identifiable, Queryable, serde::Serialize, serde::Deserialize)]
#[diesel(table_name = schema::fingerprint)]
pub struct Fingerprint {
    pub id: i32,
    pub fingerprint_hash: Secret<Vec<u8>>,
    pub fingerprint_id: Secret<String>,
    pub updated_by: StorageScheme,
}

#[cfg(feature = "external_key_manager")]
#[derive(Debug, Clone, Identifiable, Queryable)]
#[diesel(table_name = schema::entity)]
pub struct Entity {
    pub id: i32,
    pub entity_id: String,
    pub enc_key_id: String,
    pub created_at: time::PrimitiveDateTime,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq, Clone)]
pub struct CardNumber(StrongSecret<String>);

impl Validation for CardNumber {
    type Error = error::ApiError;

    fn validate(&self) -> Result<(), Self::Error> {
        crate::validations::sanitize_card_number(self.0.peek())?
            .then_some(())
            .ok_or(error::ApiError::ValidationError("card number invalid"))
    }
}

impl CardNumber {
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.peek().clone().into_bytes()
    }
}

impl std::ops::Deref for CardNumber {
    type Target = StrongSecret<String>;

    fn deref(&self) -> &StrongSecret<String> {
        &self.0
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Insertable)]
#[diesel(table_name = schema::fingerprint)]
pub(super) struct FingerprintTableNew {
    pub fingerprint_hash: Secret<Vec<u8>>,
    pub fingerprint_id: Secret<String>,
    pub updated_by: StorageScheme,
}

#[cfg(feature = "external_key_manager")]
#[derive(Debug, Insertable)]
#[diesel(table_name = schema::entity)]
pub(super) struct EntityTableNew {
    pub entity_id: String,
    pub enc_key_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Insertable)]
#[diesel(table_name = schema::hash_table)]
pub(super) struct HashTableNew {
    pub hash_id: String,
    pub data_hash: Vec<u8>,
    pub updated_by: StorageScheme,
}

///
/// Type representing data stored in ecrypted state in the database
///
#[derive(Debug, Clone, AsExpression, serde::Serialize, serde::Deserialize)]
#[diesel(sql_type = diesel::sql_types::Binary)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Encrypted {
    inner: Secret<Vec<u8>>,
}

impl Encrypted {
    pub fn new(item: Secret<Vec<u8>>) -> Self {
        Self { inner: item }
    }

    #[inline]
    pub fn into_inner(self) -> Secret<Vec<u8>> {
        self.inner
    }

    #[inline]
    pub fn get_inner(&self) -> &Secret<Vec<u8>> {
        &self.inner
    }
}

impl From<Vec<u8>> for Encrypted {
    fn from(value: Vec<u8>) -> Self {
        Self {
            inner: value.into(),
        }
    }
}

impl From<Secret<Vec<u8>>> for Encrypted {
    fn from(value: Secret<Vec<u8>>) -> Self {
        Self::new(value)
    }
}

impl<DB> FromSql<sql_types::Binary, DB> for Encrypted
where
    DB: Backend,
    Secret<Vec<u8>>: FromSql<sql_types::Binary, DB>,
{
    fn from_sql(bytes: DB::RawValue<'_>) -> diesel::deserialize::Result<Self> {
        <Secret<Vec<u8>>>::from_sql(bytes).map(Self::new)
    }
}

impl<DB> ToSql<sql_types::Binary, DB> for Encrypted
where
    DB: Backend,
    Secret<Vec<u8>>: ToSql<sql_types::Binary, DB>,
{
    fn to_sql<'b>(
        &'b self,
        out: &mut diesel::serialize::Output<'b, '_, DB>,
    ) -> diesel::serialize::Result {
        self.get_inner().to_sql(out)
    }
}

impl<DB> Queryable<sql_types::Binary, DB> for Encrypted
where
    DB: Backend,
    Secret<Vec<u8>>: FromSql<sql_types::Binary, DB>,
{
    type Row = Secret<Vec<u8>>;
    fn build(row: Self::Row) -> deserialize::Result<Self> {
        Ok(Self { inner: row })
    }
}

pub(super) trait StorageDecryption: Sized {
    type Output;
    type Algorithm: Encryption<Vec<u8>, Vec<u8>>;
    fn decrypt(
        self,
        algo: &Self::Algorithm,
    ) -> <Self::Algorithm as Encryption<Vec<u8>, Vec<u8>>>::ReturnType<'_, Self::Output>;
}

pub(super) trait StorageEncryption: Sized {
    type Output;
    type Algorithm: Encryption<Vec<u8>, Vec<u8>>;
    fn encrypt(
        self,
        algo: &Self::Algorithm,
    ) -> <Self::Algorithm as Encryption<Vec<u8>, Vec<u8>>>::ReturnType<'_, Self::Output>;
}

impl StorageDecryption for MerchantInner {
    type Output = Merchant;

    type Algorithm = GcmAes256;

    fn decrypt(
        self,
        algo: &Self::Algorithm,
    ) -> <Self::Algorithm as Encryption<Vec<u8>, Vec<u8>>>::ReturnType<'_, Self::Output> {
        Ok(Self::Output {
            merchant_id: self.merchant_id,
            enc_key: algo.decrypt(self.enc_key.into_inner().expose())?.into(),
            created_at: self.created_at,
        })
    }
}

impl<'a> StorageEncryption for MerchantNew<'a> {
    type Output = MerchantNewInner<'a>;

    type Algorithm = GcmAes256;

    fn encrypt(
        self,
        algo: &Self::Algorithm,
    ) -> <Self::Algorithm as Encryption<Vec<u8>, Vec<u8>>>::ReturnType<'_, Self::Output> {
        Ok(Self::Output {
            merchant_id: self.merchant_id,
            enc_key: algo.encrypt(self.enc_key.expose())?.into(),
        })
    }
}
