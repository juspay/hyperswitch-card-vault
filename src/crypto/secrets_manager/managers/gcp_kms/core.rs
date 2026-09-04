//! Interactions with the GCP Cloud KMS SDK

use base64::Engine;
use error_stack::ResultExt;
use google_cloud_kms::{
    client::{Client, ClientConfig},
    grpc::kms::v1::DecryptRequest,
};

use crate::{crypto::consts::BASE64_ENGINE, error::ConfigurationError, logger};

/// Configuration parameters required for constructing a [`GcpKmsClient`].
#[derive(Clone, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(default)]
pub struct GcpKmsConfig {
    /// The GCP project ID that owns the KMS key ring.
    pub project_id: String,

    /// The location ID (e.g. `"global"`, `"us-east1"`) of the KMS key ring.
    pub location_id: String,

    /// The ID of the KMS key ring.
    pub key_ring_id: String,

    /// The ID of the KMS key used to decrypt data.
    pub key_id: String,
}

/// Client for GCP Cloud KMS operations.
#[derive(Clone)]
pub struct GcpKmsClient {
    inner_client: Client,
    key_name: String,
}

impl std::fmt::Debug for GcpKmsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcpKmsClient")
            .field("key_name", &self.key_name)
            .finish()
    }
}

impl GcpKmsClient {
    /// Constructs a new GCP Cloud KMS client.
    pub async fn new(config: &GcpKmsConfig) -> error_stack::Result<Self, GcpKmsError> {
        let client_config = ClientConfig::default()
            .with_auth()
            .await
            .change_context(GcpKmsError::ClientCreationFailed)?;
        let inner_client = Client::new(client_config)
            .await
            .change_context(GcpKmsError::ClientCreationFailed)?;

        Ok(Self {
            inner_client,
            key_name: format!(
                "projects/{}/locations/{}/keyRings/{}/cryptoKeys/{}",
                config.project_id, config.location_id, config.key_ring_id, config.key_id
            ),
        })
    }

    /// Decrypts the provided base64-encoded encrypted data using the GCP Cloud KMS SDK.
    pub async fn decrypt(
        &self,
        data: impl AsRef<[u8]>,
    ) -> error_stack::Result<String, GcpKmsError> {
        let ciphertext = BASE64_ENGINE
            .decode(data)
            .change_context(GcpKmsError::Base64DecodingFailed)?;

        let request = DecryptRequest {
            name: self.key_name.clone(),
            ciphertext,
            additional_authenticated_data: Vec::new(),
            ciphertext_crc32c: None,
            additional_authenticated_data_crc32c: None,
        };

        let response = self
            .inner_client
            .decrypt(request, None)
            .await
            .map_err(|error| {
                logger::error!(gcp_kms_error=?error, "Failed to GCP KMS decrypt data");
                error
            })
            .change_context(GcpKmsError::DecryptionFailed)?;

        String::from_utf8(response.plaintext).change_context(GcpKmsError::Utf8DecodingFailed)
    }
}

/// Errors that could occur during GCP Cloud KMS operations.
#[derive(Debug, thiserror::Error)]
pub enum GcpKmsError {
    /// An error occurred when base64 decoding input data.
    #[error("Failed to base64 decode input data")]
    Base64DecodingFailed,

    /// An error occurred when GCP KMS decrypting input data.
    #[error("Failed to GCP KMS decrypt input data")]
    DecryptionFailed,

    /// An error occurred UTF-8 decoding GCP KMS decrypted output.
    #[error("Failed to UTF-8 decode decryption output")]
    Utf8DecodingFailed,

    /// Failed while creating the GCP KMS client.
    #[error("Failed while creating a GCP KMS client")]
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

        if self.location_id.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "GCP KMS location ID must not be empty".into(),
            ));
        }

        if self.key_ring_id.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "GCP KMS key ring ID must not be empty".into(),
            ));
        }

        if self.key_id.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "GCP KMS key ID must not be empty".into(),
            ));
        }

        Ok(())
    }
}
