use std::{marker::PhantomData, pin::Pin};

use aws_config::meta::region::RegionProviderChain;
use aws_sdk_kms::{config::Region, primitives::Blob, Client};
use base64::Engine;
use error_stack::{report, ResultExt};
use futures::Future;

use crate::crypto::Encryption;

use super::consts;

static KMS_CLIENT: tokio::sync::OnceCell<KmsClient> = tokio::sync::OnceCell::const_new();

/// Returns a shared KMS client, or initializes a new one if not previously initialized.
#[inline]
pub async fn get_kms_client(config: &KmsConfig) -> &'static KmsClient {
    KMS_CLIENT.get_or_init(|| KmsClient::new(config)).await
}

/// Configuration parameters required for constructing a [`KmsClient`].
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct KmsConfig {
    /// The AWS key identifier of the KMS key used to encrypt or decrypt data.
    pub key_id: String,

    /// The AWS region to send KMS requests to.
    pub region: String,
}

/// Client for KMS operations.
#[derive(Debug)]
pub struct KmsClient {
    inner_client: Client,
    key_id: String,
}

impl KmsClient {
    /// Constructs a new KMS client.
    pub async fn new(config: &KmsConfig) -> Self {
        let region_provider = RegionProviderChain::first_try(Region::new(config.region.clone()));
        let sdk_config = aws_config::from_env().region(region_provider).load().await;

        Self {
            inner_client: Client::new(&sdk_config),
            key_id: config.key_id.clone(),
        }
    }
}

/// Errors that could occur during KMS operations.
#[derive(Debug, thiserror::Error)]
pub enum KmsError {
    /// An error occurred when base64 decoding input data.
    #[error("Failed to base64 decode input data")]
    Base64DecodingFailed,

    /// An error occurred when hex decoding input data.
    #[error("Failed to hex decode input data")]
    HexDecodingFailed,

    /// An error occurred when KMS decrypting input data.
    #[error("Failed to KMS decrypt input data")]
    DecryptionFailed,

    /// The KMS decrypted output does not include a plaintext output.
    #[error("Missing plaintext KMS decryption output")]
    MissingPlaintextDecryptionOutput,

    /// An error occurred UTF-8 decoding KMS decrypted output.
    #[error("Failed to UTF-8 decode decryption output")]
    Utf8DecodingFailed,

    /// The KMS client has not been initialized.
    #[error("The KMS client has not been initialized")]
    KmsClientNotInitialized,
}

impl KmsConfig {
    /// Verifies that the [`KmsClient`] configuration is usable.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.key_id.trim().is_empty() {
            return Err("KMS AWS key ID must not be empty");
        };

        if self.region.trim().is_empty() {
            return Err("KMS AWS region must not be empty");
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, Eq, PartialEq)]

pub struct KmsData<T: Decoder> {
    pub data: T::Data,
    pub decode_op: PhantomData<T>,
}

impl<T: Decoder> KmsData<T> {
    pub fn new(data: T::Data) -> Self {
        Self {
            data,
            decode_op: PhantomData,
        }
    }
    pub fn into_decoded(self) -> Result<Vec<u8>, T::Error> {
        T::decode(self.data)
    }
    pub fn encode(data: Vec<u8>) -> Result<Self, T::Error> {
        Ok(Self {
            data: T::encode(data)?,
            decode_op: PhantomData,
        })
    }
}

pub trait Decoder {
    type Data;
    type Error;
    fn encode(input: Vec<u8>) -> Result<Self::Data, Self::Error>;
    fn decode(input: Self::Data) -> Result<Vec<u8>, Self::Error>;
}

pub struct StringEncoded;

impl Decoder for StringEncoded {
    type Data = String;
    type Error = error_stack::Report<KmsError>;

    fn encode(input: Vec<u8>) -> Result<Self::Data, Self::Error> {
        String::from_utf8(input).change_context(KmsError::Utf8DecodingFailed)
    }
    fn decode(input: Self::Data) -> Result<Vec<u8>, Self::Error> {
        Ok(input.into_bytes())
    }
}

pub struct Base64Encoded;

impl Decoder for Base64Encoded {
    type Data = String;
    type Error = error_stack::Report<KmsError>;

    fn encode(input: Vec<u8>) -> Result<Self::Data, Self::Error> {
        Ok(consts::BASE64_ENGINE.encode(input))
    }
    fn decode(input: Self::Data) -> Result<Vec<u8>, Self::Error> {
        consts::BASE64_ENGINE
            .decode(input)
            .change_context(KmsError::Base64DecodingFailed)
    }
}

pub struct HexEncoded;

impl Decoder for HexEncoded {
    type Data = String;
    type Error = error_stack::Report<KmsError>;

    fn encode(input: Vec<u8>) -> Result<Self::Data, Self::Error> {
        Ok(hex::encode(input))
    }
    fn decode(input: Self::Data) -> Result<Vec<u8>, Self::Error> {
        hex::decode(input).change_context(KmsError::HexDecodingFailed)
    }
}

impl<U: Decoder<Error = error_stack::Report<KmsError>>>
    Encryption<KmsData<U>, KmsData<Base64Encoded>> for KmsClient
{
    type ReturnType<'b, T> = Pin<Box<dyn Future<Output = error_stack::Result<T, KmsError>> + 'b>>;

    fn encrypt(&self, _input: KmsData<U>) -> Self::ReturnType<'_, KmsData<Base64Encoded>> {
        todo!()
    }

    fn decrypt<'a>(
        &'a self,
        input: KmsData<Base64Encoded>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<KmsData<U>, KmsError>> + 'a>> {
        Box::pin(async move {
            let mut data = input.into_decoded()?;
            let ciphertext_blob = Blob::new(&mut *data);

            let decrypt_output = self
                .inner_client
                .decrypt()
                .key_id(&self.key_id)
                .ciphertext_blob(ciphertext_blob)
                .send()
                .await
                .change_context(KmsError::DecryptionFailed)?;

            let output = decrypt_output
                .plaintext
                .ok_or(report!(KmsError::MissingPlaintextDecryptionOutput))
                .and_then(|blob| {
                    String::from_utf8(blob.into_inner())
                        .change_context(KmsError::Utf8DecodingFailed)
                })?;

            KmsData::encode(output.into_bytes())
        })
    }
}

pub struct Raw;

impl Decoder for Raw {
    type Data = Vec<u8>;

    type Error = error_stack::Report<KmsError>;

    fn encode(input: Vec<u8>) -> Result<Self::Data, Self::Error> {
        Ok(input)
    }

    fn decode(input: Self::Data) -> Result<Vec<u8>, Self::Error> {
        Ok(input)
    }
}
