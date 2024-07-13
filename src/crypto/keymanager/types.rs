use std::fmt;

use crate::{
    crypto,
    storage::{consts, utils},
};
use base64::Engine;
use masking::{PeekInterface, Secret};
use serde::{
    de::{self, Unexpected, Visitor},
    ser, Deserialize, Deserializer, Serialize,
};

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct DataKeyCreateRequest {
    #[serde(flatten)]
    pub identifier: Identifier,
}

impl DataKeyCreateRequest {
    pub fn create_request() -> Self {
        Self {
            identifier: Identifier::Entity(utils::generate_id(consts::ID_LENGTH)),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DataKeyCreateResponse {
    #[serde(flatten)]
    pub identifier: Identifier,
    pub key_version: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct DataKeyTransferRequest {
    #[serde(flatten)]
    pub identifier: Identifier,
    pub key: String,
}

impl DataKeyTransferRequest {
    pub fn create_request(key: Vec<u8>) -> Self {
        Self {
            identifier: Identifier::Entity(utils::generate_id(consts::ID_LENGTH)),
            key: crypto::consts::BASE64_ENGINE.encode(key),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DataEncryptionRequest {
    #[serde(flatten)]
    pub identifier: Identifier,
    pub data: DecryptedData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DateEncryptionResponse {
    pub data: EncryptedData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataDecryptionRequest {
    #[serde(flatten)]
    pub identifier: Identifier,
    pub data: EncryptedData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataDecryptionResponse {
    pub data: DecryptedData,
}

#[derive(Debug)]
pub struct EncryptedData {
    pub data: Secret<Vec<u8>>,
}

impl Serialize for EncryptedData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let data = String::from_utf8(self.data.peek().clone()).map_err(ser::Error::custom)?;
        serializer.serialize_str(data.as_str())
    }
}

impl<'de> Deserialize<'de> for EncryptedData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EncryptedDataVisitor;

        impl<'de> Visitor<'de> for EncryptedDataVisitor {
            type Value = EncryptedData;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("string of the format {version}:{base64_encoded_data}'")
            }

            fn visit_str<E>(self, value: &str) -> Result<EncryptedData, E>
            where
                E: de::Error,
            {
                Ok(EncryptedData {
                    data: Secret::new(value.as_bytes().to_vec()),
                })
            }
        }

        deserializer.deserialize_str(EncryptedDataVisitor)
    }
}

#[derive(Clone, Debug)]
pub struct DecryptedData(Secret<Vec<u8>>);

impl DecryptedData {
    pub fn from_data(data: Secret<Vec<u8>>) -> Self {
        Self(data)
    }
    pub fn inner(self) -> Secret<Vec<u8>> {
        self.0
    }
}

impl Serialize for DecryptedData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let data = crypto::consts::BASE64_ENGINE.encode(self.0.peek());
        serializer.serialize_str(&data)
    }
}

impl<'de> Deserialize<'de> for DecryptedData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DecryptedDataVisitor;

        impl<'de> Visitor<'de> for DecryptedDataVisitor {
            type Value = DecryptedData;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("string of the format {version}:{base64_encoded_data}'")
            }

            fn visit_str<E>(self, value: &str) -> Result<DecryptedData, E>
            where
                E: de::Error,
            {
                let dec_data = crypto::consts::BASE64_ENGINE.decode(value).map_err(|err| {
                    let err = err.to_string();
                    E::invalid_value(Unexpected::Str(value), &err.as_str())
                })?;

                Ok(DecryptedData(dec_data.into()))
            }
        }

        deserializer.deserialize_str(DecryptedDataVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "data_identifier", content = "key_identifier")]
pub enum Identifier {
    Entity(String),
}

impl Identifier {
    pub fn get_identifier(&self) -> String {
        match self {
            Self::Entity(identifier) => identifier.clone(),
        }
    }
}
