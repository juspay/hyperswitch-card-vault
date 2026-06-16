//! Interactions with the AWS KMS SDK

use std::collections::HashMap;

use aws_config::meta::region::RegionProviderChain;
use aws_sdk_kms::{Client, config::Region, primitives::Blob};
use error_stack::{ResultExt, report};

use crate::{error::ConfigurationError, logger};

/// Configuration parameters required for constructing a [`AwsKmsClient`].
#[derive(Clone, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(default)]
pub struct AwsKmsConfig {
    /// The AWS key identifier of the KMS key used to encrypt or decrypt data.
    pub key_id: String,

    /// The AWS region to send KMS requests to.
    pub region: String,
}

/// Client for AWS KMS operations.
#[derive(Debug, Clone)]
pub struct AwsKmsClient {
    inner_client: Client,
    key_id: String,
}

impl AwsKmsClient {
    /// Constructs a new AWS KMS client.
    pub async fn new(config: &AwsKmsConfig) -> Self {
        let region_provider = RegionProviderChain::first_try(Region::new(config.region.clone()));
        let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region_provider)
            .load()
            .await;

        Self {
            inner_client: Client::new(&sdk_config),
            key_id: config.key_id.clone(),
        }
    }

    /// Encrypts the provided plaintext using the AWS KMS SDK. Optionally accepts an encryption
    /// context that cryptographically binds the ciphertext to additional authenticated data.
    pub async fn encrypt(
        &self,
        plaintext: &[u8],
        encryption_context: Option<HashMap<String, String>>,
    ) -> error_stack::Result<Vec<u8>, AwsKmsError> {
        let mut req = self
            .inner_client
            .encrypt()
            .key_id(&self.key_id)
            .plaintext(Blob::new(plaintext));

        if let Some(ctx) = encryption_context {
            for (k, v) in ctx {
                req = req.encryption_context(k, v);
            }
        }

        let output = req
            .send()
            .await
            .map_err(|error| {
                logger::error!(aws_kms_sdk_error=?error, "Failed to AWS KMS encrypt data");
                error
            })
            .change_context(AwsKmsError::EncryptionFailed)?;

        output
            .ciphertext_blob
            .ok_or(report!(AwsKmsError::MissingCiphertextEncryptionOutput))
            .map(|blob| blob.into_inner())
    }

    /// Decrypts the provided ciphertext using the AWS KMS SDK. The encryption context, if
    /// provided, must match the context used during encryption. We assume that the SDK has
    /// the values required to interact with the AWS KMS APIs (`AWS_ACCESS_KEY_ID` and
    /// `AWS_SECRET_ACCESS_KEY`) either set in environment variables, or that the SDK is
    /// running in a machine that is able to assume an IAM role.
    pub async fn decrypt(
        &self,
        ciphertext: &[u8],
        encryption_context: Option<HashMap<String, String>>,
    ) -> error_stack::Result<Vec<u8>, AwsKmsError> {
        let mut req = self
            .inner_client
            .decrypt()
            .key_id(&self.key_id)
            .ciphertext_blob(Blob::new(ciphertext));

        if let Some(ctx) = encryption_context {
            for (k, v) in ctx {
                req = req.encryption_context(k, v);
            }
        }

        let output = req
            .send()
            .await
            .map_err(|error| {
                logger::error!(aws_kms_sdk_error=?error, "Failed to AWS KMS decrypt data");
                error
            })
            .change_context(AwsKmsError::DecryptionFailed)?;

        output
            .plaintext
            .ok_or(report!(AwsKmsError::MissingPlaintextDecryptionOutput))
            .map(|blob| blob.into_inner())
    }
}

/// Errors that could occur during KMS operations.
#[derive(Debug, thiserror::Error)]
pub enum AwsKmsError {
    /// An error occurred when base64 encoding input data.
    #[error("Failed to base64 encode input data")]
    Base64EncodingFailed,

    /// An error occurred when base64 decoding input data.
    #[error("Failed to base64 decode input data")]
    Base64DecodingFailed,

    /// An error occurred when AWS KMS decrypting input data.
    #[error("Failed to AWS KMS decrypt input data")]
    DecryptionFailed,

    /// An error occurred when AWS KMS encrypting input data.
    #[error("Failed to AWS KMS encrypt input data")]
    EncryptionFailed,

    /// The AWS KMS decrypted output does not include a plaintext output.
    #[error("Missing plaintext AWS KMS decryption output")]
    MissingPlaintextDecryptionOutput,

    /// The AWS KMS encrypted output does not include a ciphertext output.
    #[error("Missing ciphertext AWS KMS encryption output")]
    MissingCiphertextEncryptionOutput,

    /// An error occurred UTF-8 decoding AWS KMS decrypted output.
    #[error("Failed to UTF-8 decode decryption output")]
    Utf8DecodingFailed,

    /// The AWS KMS client has not been initialized.
    #[error("The AWS KMS client has not been initialized")]
    AwsKmsClientNotInitialized,
}

impl AwsKmsConfig {
    /// Verifies that the [`AwsKmsClient`] configuration is usable.
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if self.key_id.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "AWS KMS key ID must not be empty".into(),
            ));
        }

        if self.region.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "AWS KMS region must not be empty".into(),
            ));
        }

        Ok(())
    }
}
