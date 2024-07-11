use diesel::{
    backend::Backend,
    deserialize::{self, FromSql},
    serialize::ToSql,
    sql_types, AsExpression, Identifiable, Insertable, Queryable,
};
use masking::{ExposeInterface, PeekInterface, Secret, StrongSecret};

use crate::{
    crypto::encryption_manager::{encryption_interface::Encryption, managers::aes::GcmAes256},
    error,
    routes::data::types::Validation,
};

use super::schema;

#[derive(Debug, Identifiable, Queryable)]
#[diesel(table_name = schema::merchant)]
pub(super) struct MerchantInner {
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
pub(super) struct MerchantNewInner<'a> {
    merchant_id: &'a str,
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
}

#[derive(Debug)]
pub struct Locker {
    pub locker_id: Secret<String>,
    pub merchant_id: String,
    pub customer_id: String,
    pub enc_data: Secret<Vec<u8>>,
    pub created_at: time::PrimitiveDateTime,
    pub hash_id: String,
    pub ttl: Option<time::PrimitiveDateTime>,
}

#[derive(Debug, Clone)]
pub struct LockerNew<'a> {
    pub locker_id: Secret<String>,
    pub merchant_id: String,
    pub customer_id: String,
    pub enc_data: Secret<Vec<u8>>,
    pub hash_id: &'a str,
    pub ttl: Option<time::PrimitiveDateTime>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::locker)]
pub(super) struct LockerNewInner<'a> {
    locker_id: Secret<String>,
    merchant_id: String,
    customer_id: String,
    enc_data: Encrypted,
    hash_id: &'a str,
    ttl: Option<time::PrimitiveDateTime>,
}

#[derive(Debug, Clone, Identifiable, Queryable)]
#[diesel(table_name = schema::hash_table)]
pub struct HashTable {
    pub id: i32,
    pub hash_id: String,
    pub data_hash: Vec<u8>,
    pub created_at: time::PrimitiveDateTime,
}

#[derive(Debug, Clone, Identifiable, Queryable)]
#[diesel(table_name = schema::fingerprint)]
pub struct Fingerprint {
    pub id: i32,
    pub card_hash: Secret<Vec<u8>>,
    pub card_fingerprint: Secret<String>,
}

#[derive(Debug, Clone, Identifiable, Queryable)]
#[diesel(table_name = schema::entity)]
pub struct Entity {
    pub id: i32,
    pub entity_id: String,
    pub enc_key_id: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
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

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::fingerprint)]
pub(super) struct FingerprintTableNew {
    pub fingerprint_hash: Secret<Vec<u8>>,
    pub fingerprint_id: Secret<String>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::entity)]
pub(super) struct EntityTableNew {
    pub entity_id: String,
    pub enc_key_id: String,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::hash_table)]
pub(super) struct HashTableNew {
    pub hash_id: String,
    pub data_hash: Vec<u8>,
}

///
/// Type representing data stored in ecrypted state in the database
///
#[derive(Debug, AsExpression)]
#[diesel(sql_type = diesel::sql_types::Binary)]
#[repr(transparent)]
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

impl StorageDecryption for LockerInner {
    type Output = Locker;

    type Algorithm = GcmAes256;

    fn decrypt(
        self,
        algo: &Self::Algorithm,
    ) -> <Self::Algorithm as Encryption<Vec<u8>, Vec<u8>>>::ReturnType<'_, Self::Output> {
        Ok(Self::Output {
            locker_id: self.locker_id,
            merchant_id: self.merchant_id,
            customer_id: self.customer_id,
            enc_data: algo.decrypt(self.enc_data.into_inner().expose())?.into(),
            created_at: self.created_at,
            hash_id: self.hash_id,
            ttl: self.ttl,
        })
    }
}

impl<'a> StorageEncryption for LockerNew<'a> {
    type Output = LockerNewInner<'a>;

    type Algorithm = GcmAes256;

    fn encrypt(
        self,
        algo: &Self::Algorithm,
    ) -> <Self::Algorithm as Encryption<Vec<u8>, Vec<u8>>>::ReturnType<'_, Self::Output> {
        Ok(Self::Output {
            locker_id: self.locker_id,
            merchant_id: self.merchant_id,
            customer_id: self.customer_id,
            enc_data: algo.encrypt(self.enc_data.expose())?.into(),
            hash_id: self.hash_id,
            ttl: self.ttl,
        })
    }
}
