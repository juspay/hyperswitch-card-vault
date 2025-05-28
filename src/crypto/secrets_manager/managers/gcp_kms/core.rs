//! Core GCP KMS client implementation

use base64::Engine;
use error_stack::ResultExt;
use google_cloud_kms::client::{Client, ClientConfig};

use crate::{crypto::consts::BASE64_ENGINE, error::ConfigurationError, logger};

/// Configuration parameters required for constructing a [`GcpKmsClient`].
#[derive(Clone, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(default)]
pub struct GcpKmsConfig {
    /// The GCP project ID where the KMS key is located.
    pub project_id: String,

    /// The GCP region/location where the key ring is located.
    pub location: String,

    /// The key ring name containing the KMS key.
    pub key_ring: String,

    /// The KMS key name used to encrypt or decrypt data.
    pub key_name: String,

    /// Optional service account key file path for authentication.
    /// If not provided, will use default GCP authentication (ADC).
    pub service_account_key_path: Option<String>,
}

/// Client for GCP KMS operations.
#[derive(Debug, Clone)]
pub struct GcpKmsClient {
    config: GcpKmsConfig,
    key_resource_name: String,
}

impl GcpKmsClient {
    /// Constructs a new GCP KMS client.
    pub async fn new(config: &GcpKmsConfig) -> Self {
        let key_resource_name = format!(
            "projects/{}/locations/{}/keyRings/{}/cryptoKeys/{}",
            config.project_id, config.location, config.key_ring, config.key_name
        );

        Self {
            config: config.clone(),
            key_resource_name,
        }
    }

    /// Decrypts the provided base64-encoded encrypted data using the GCP KMS SDK.
    pub async fn decrypt(
        &self,
        data: impl AsRef<[u8]>,
    ) -> error_stack::Result<String, GcpKmsError> {
        let ciphertext = BASE64_ENGINE
            .decode(data)
            .change_context(GcpKmsError::Base64DecodingFailed)?;

        // Create KMS client
        let client = self.get_kms_client().await?;

        // Create decrypt request using the correct path
        let request = google_cloud_kms::grpc::kms::v1::DecryptRequest {
            name: self.key_resource_name.clone(),
            ciphertext,
            additional_authenticated_data: vec![],
            ciphertext_crc32c: None,
            additional_authenticated_data_crc32c: None,
        };

        // Perform decryption
        let response = client
            .decrypt(request, None)
            .await
            .map_err(|error| {
                logger::error!(gcp_kms_decrypt_error=?error, "Failed to decrypt with GCP KMS");
                error
            })
            .change_context(GcpKmsError::DecryptionFailed)?;

        let plaintext = String::from_utf8(response.plaintext)
            .change_context(GcpKmsError::Utf8DecodingFailed)?;

        Ok(plaintext)
    }

    /// Creates a GCP KMS client with proper authentication
    async fn get_kms_client(&self) -> error_stack::Result<Client, GcpKmsError> {
        let config = if let Some(_key_path) = &self.config.service_account_key_path {
            // For service account key file authentication, we would need to implement
            // custom credential loading. For now, fall back to default auth.
            ClientConfig::default()
                .with_auth()
                .await
                .change_context(GcpKmsError::AuthenticationFailed)?
        } else {
            // Use Application Default Credentials (ADC)
            ClientConfig::default()
                .with_auth()
                .await
                .change_context(GcpKmsError::AuthenticationFailed)?
        };

        Client::new(config)
            .await
            .change_context(GcpKmsError::ClientCreationFailed)
    }
}

/// Errors that could occur during GCP KMS operations.
#[derive(Debug, thiserror::Error)]
pub enum GcpKmsError {
    /// An error occurred when base64 encoding input data.
    #[error("Failed to base64 encode input data")]
    Base64EncodingFailed,

    /// An error occurred when base64 decoding input data.
    #[error("Failed to base64 decode input data")]
    Base64DecodingFailed,

    /// An error occurred when GCP KMS decrypting input data.
    #[error("Failed to GCP KMS decrypt input data")]
    DecryptionFailed,

    /// An error occurred when GCP KMS encrypting input data.
    #[error("Failed to GCP KMS encrypt input data")]
    EncryptionFailed,

    /// The GCP KMS decrypted output does not include a plaintext output.
    #[error("Missing plaintext GCP KMS decryption output")]
    MissingPlaintextDecryptionOutput,

    /// The GCP KMS encrypted output does not include a ciphertext output.
    #[error("Missing ciphertext GCP KMS encryption output")]
    MissingCiphertextEncryptionOutput,

    /// An error occurred UTF-8 decoding GCP KMS decrypted output.
    #[error("Failed to UTF-8 decode decryption output")]
    Utf8DecodingFailed,

    /// The GCP KMS client has not been initialized.
    #[error("The GCP KMS client has not been initialized")]
    GcpKmsClientNotInitialized,

    /// Authentication with GCP failed.
    #[error("Failed to authenticate with GCP")]
    AuthenticationFailed,

    /// Failed to create HTTP client.
    #[error("Failed to create HTTP client")]
    ClientCreationFailed,
}

impl GcpKmsConfig {
    /// Verifies that the [`GcpKmsClient`] configuration is usable.
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if self.project_id.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "GCP KMS project ID must not be empty".into(),
            ));
        }

        if self.location.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "GCP KMS location must not be empty".into(),
            ));
        }

        if self.key_ring.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "GCP KMS key ring must not be empty".into(),
            ));
        }

        if self.key_name.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "GCP KMS key name must not be empty".into(),
            ));
        }

        Ok(())
    }
}
