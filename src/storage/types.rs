use base64::Engine;
use diesel::{
    AsExpression, Identifiable, Insertable, Queryable,
    backend::Backend,
    deserialize::{self, FromSql},
    serialize::ToSql,
    sql_types,
};
use hyperswitch_masking::{ExposeInterface, PeekInterface, Secret, StrongSecret};

use super::{schema, scheme::StorageScheme};
#[cfg(feature = "kv")]
use crate::storage::kv;
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

#[derive(Debug, Identifiable, Queryable, serde::Serialize, serde::Deserialize)]
#[diesel(table_name = schema::locker)]
pub(crate) struct LockerInner {
    id: i32,
    locker_id: Secret<String>,
    merchant_id: String,
    customer_id: String,
    enc_data: Encrypted,
    #[serde(with = "crate::utils::primitive_datetime_serde::iso8601")]
    created_at: time::PrimitiveDateTime,
    hash_id: String,
    #[serde(with = "crate::utils::primitive_datetime_serde::iso8601::option")]
    ttl: Option<time::PrimitiveDateTime>,
    pub updated_by: Option<StorageScheme>,
}

impl From<LockerNew> for LockerInner {
    fn from(value: LockerNew) -> Self {
        Self {
            id: 0,
            locker_id: value.locker_id,
            merchant_id: value.merchant_id,
            customer_id: value.customer_id,
            enc_data: value.enc_data,
            created_at: value.created_at,
            hash_id: value.hash_id,
            ttl: value.ttl,
            updated_by: value.updated_by,
        }
    }
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
    pub updated_by: Option<StorageScheme>,
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
pub struct LockerNew {
    pub locker_id: Secret<String>,
    pub merchant_id: String,
    pub customer_id: String,
    pub enc_data: Encrypted,
    pub created_at: time::PrimitiveDateTime,
    pub hash_id: String,
    pub ttl: Option<time::PrimitiveDateTime>,
    pub updated_by: Option<StorageScheme>,
}

impl LockerNew {
    pub fn new(request: StoreCardRequest, hash_id: &str, enc_data: Encrypted) -> Self {
        Self {
            locker_id: request
                .requestor_card_reference
                .unwrap_or_else(super::utils::generate_uuid)
                .into(),
            merchant_id: request.merchant_id,
            customer_id: request.merchant_customer_id,
            enc_data,
            created_at: crate::utils::date_time::now(),
            hash_id: hash_id.to_string(),
            ttl: *request.ttl,
            // Placeholder — overwritten by `set_storage_scheme` when locker joins KV.
            updated_by: Some(StorageScheme::PostgresOnly),
        }
    }
}

impl From<LockerNew> for Locker {
    fn from(value: LockerNew) -> Self {
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

#[derive(Debug, Clone, Identifiable, Queryable)]
#[diesel(table_name = schema::reverse_lookup, primary_key(lookup_id))]
#[expect(dead_code)]
pub(crate) struct ReverseLookup {
    pub lookup_id: String,
    pub secondary_key: String,
    pub partition_key: String,
    pub source: String,
    pub updated_by: String,
}

#[cfg(feature = "kv")]
impl ReverseLookup {
    pub(crate) fn get_partition_key(&self) -> kv::PartitionKey<'_> {
        kv::PartitionKey::CombinationKey {
            combination: &self.partition_key,
        }
    }
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = schema::reverse_lookup)]
pub(crate) struct ReverseLookupNew {
    pub lookup_id: String,
    pub secondary_key: String,
    pub partition_key: String,
    pub source: String,
    pub updated_by: String,
}

impl From<ReverseLookupNew> for ReverseLookup {
    fn from(value: ReverseLookupNew) -> Self {
        Self {
            lookup_id: value.lookup_id,
            secondary_key: value.secondary_key,
            partition_key: value.partition_key,
            source: value.source,
            updated_by: value.updated_by,
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
    pub updated_by: Option<StorageScheme>,
}

#[derive(Debug, Clone, Identifiable, Queryable, serde::Serialize, serde::Deserialize)]
#[diesel(table_name = schema::fingerprint)]
pub struct Fingerprint {
    pub id: i32,
    pub fingerprint_hash: Secret<Vec<u8>>,
    pub fingerprint_id: Secret<String>,
    pub updated_by: Option<StorageScheme>,
}

impl From<FingerprintTableNew> for Fingerprint {
    fn from(value: FingerprintTableNew) -> Self {
        Self {
            id: 0,
            fingerprint_hash: value.fingerprint_hash,
            fingerprint_id: value.fingerprint_id,
            updated_by: value.updated_by,
        }
    }
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

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = schema::fingerprint)]
pub(crate) struct FingerprintTableNew {
    pub fingerprint_hash: Secret<Vec<u8>>,
    pub fingerprint_id: Secret<String>,
    pub updated_by: Option<StorageScheme>,
}

#[cfg(feature = "external_key_manager")]
#[derive(Debug, Insertable)]
#[diesel(table_name = schema::entity)]
pub(super) struct EntityTableNew {
    pub entity_id: String,
    pub enc_key_id: String,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = schema::hash_table)]
pub(super) struct HashTableNew {
    pub hash_id: String,
    pub data_hash: Vec<u8>,
    pub updated_by: Option<StorageScheme>,
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

impl serde::Serialize for Encrypted {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer
            .serialize_str(&base64::engine::general_purpose::STANDARD.encode(self.inner.peek()))
    }
}

impl<'de> serde::Deserialize<'de> for Encrypted {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(value)
            .map(Secret::new)
            .map(Self::new)
            .map_err(serde::de::Error::custom)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn primitive_datetime(second: u8, nanosecond: u32) -> Option<time::PrimitiveDateTime> {
        Some(time::PrimitiveDateTime::new(
            time::Date::from_calendar_date(1970, time::Month::January, 1).ok()?,
            time::Time::from_hms_nano(0, 0, second, nanosecond).ok()?,
        ))
    }

    #[test]
    fn encrypted_serializes_as_base64_string() {
        let encrypted = Encrypted::new(Secret::new(vec![1, 2, 3, 254, 255]));

        let serialized = serde_json::to_string(&encrypted);
        assert!(serialized.is_ok(), "failed to serialize encrypted data");
        let Ok(serialized) = serialized else { return };

        assert_eq!(serialized, "\"AQID/v8=\"");
    }

    #[test]
    fn encrypted_deserializes_from_base64_string() {
        let deserialized = serde_json::from_str::<Encrypted>("\"AQID/v8=\"");
        assert!(deserialized.is_ok(), "failed to deserialize encrypted data");
        let Ok(deserialized) = deserialized else {
            return;
        };

        assert_eq!(deserialized.get_inner().peek(), &vec![1, 2, 3, 254, 255]);
    }

    #[test]
    fn locker_inner_serializes_timestamps_as_iso8601_strings() {
        let created_at = primitive_datetime(1, 123_456_789);
        assert!(created_at.is_some(), "failed to build created_at");
        let Some(created_at) = created_at else { return };

        let ttl = primitive_datetime(2, 654_321_987);
        assert!(ttl.is_some(), "failed to build ttl");
        let Some(ttl) = ttl else { return };

        let locker = LockerInner {
            id: 1,
            locker_id: Secret::new("locker_id".to_string()),
            merchant_id: "merchant_id".to_string(),
            customer_id: "customer_id".to_string(),
            enc_data: Encrypted::new(Secret::new(vec![1, 2, 3])),
            created_at,
            hash_id: "hash_id".to_string(),
            ttl: Some(ttl),
            updated_by: StorageScheme::RedisKv,
        };

        let serialized = serde_json::to_value(&locker);
        assert!(serialized.is_ok(), "failed to serialize locker");
        let Ok(serialized) = serialized else { return };

        assert_eq!(
            serialized["created_at"],
            serde_json::json!("1970-01-01T00:00:01.123456789Z")
        );
        assert_eq!(
            serialized["ttl"],
            serde_json::json!("1970-01-01T00:00:02.654321987Z")
        );
    }

    #[test]
    fn locker_inner_deserializes_timestamps_from_iso8601_strings() {
        let serialized = serde_json::json!({
            "id": 1,
            "locker_id": "locker_id",
            "merchant_id": "merchant_id",
            "customer_id": "customer_id",
            "enc_data": "AQID",
            "created_at": "1970-01-01T00:00:01.123456789Z",
            "hash_id": "hash_id",
            "ttl": "1970-01-01T00:00:02.654321987Z",
            "updated_by": "redis_kv"
        });

        let deserialized = serde_json::from_value::<LockerInner>(serialized);
        assert!(deserialized.is_ok(), "failed to deserialize locker");
        let Ok(deserialized) = deserialized else {
            return;
        };

        let created_at = primitive_datetime(1, 123_456_789);
        assert!(created_at.is_some(), "failed to build created_at");
        let Some(created_at) = created_at else { return };

        let ttl = primitive_datetime(2, 654_321_987);
        assert!(ttl.is_some(), "failed to build ttl");
        let Some(ttl) = ttl else { return };

        assert_eq!(deserialized.created_at, created_at);
        assert_eq!(deserialized.ttl, Some(ttl));
    }

    #[test]
    fn locker_inner_round_trips_timestamps_without_precision_loss() {
        let created_at = primitive_datetime(1, 123_456_789);
        assert!(created_at.is_some(), "failed to build created_at");
        let Some(created_at) = created_at else { return };

        let ttl = primitive_datetime(2, 654_321_987);
        assert!(ttl.is_some(), "failed to build ttl");
        let Some(ttl) = ttl else { return };

        let locker = LockerInner {
            id: 1,
            locker_id: Secret::new("locker_id".to_string()),
            merchant_id: "merchant_id".to_string(),
            customer_id: "customer_id".to_string(),
            enc_data: Encrypted::new(Secret::new(vec![1, 2, 3])),
            created_at,
            hash_id: "hash_id".to_string(),
            ttl: Some(ttl),
            updated_by: StorageScheme::RedisKv,
        };

        let serialized = serde_json::to_string(&locker);
        assert!(serialized.is_ok(), "failed to serialize locker");
        let Ok(serialized) = serialized else { return };

        let deserialized = serde_json::from_str::<LockerInner>(&serialized);
        assert!(deserialized.is_ok(), "failed to deserialize locker");
        let Ok(deserialized) = deserialized else {
            return;
        };

        assert_eq!(deserialized.created_at, locker.created_at);
        assert_eq!(deserialized.ttl, locker.ttl);
    }
}
