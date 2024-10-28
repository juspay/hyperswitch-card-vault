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
    routes::data::types::{StoreCardRequest, Validation},
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
}

#[derive(Debug, Clone, Identifiable, Queryable)]
#[diesel(table_name = schema::fingerprint)]
pub struct Fingerprint {
    pub id: i32,
    pub fingerprint_hash: Secret<Vec<u8>>,
    pub fingerprint_id: Secret<String>,
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

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::fingerprint)]
pub(super) struct FingerprintTableNew {
    pub fingerprint_hash: Secret<Vec<u8>>,
    pub fingerprint_id: Secret<String>,
}

#[cfg(feature = "external_key_manager")]
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
#[derive(Debug, Clone, AsExpression)]
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
