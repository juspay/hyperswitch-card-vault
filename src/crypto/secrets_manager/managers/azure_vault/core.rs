//! Interactions with the Azure Key Vault SDK

use azure_identity::DefaultAzureCredential;
use azure_security_keyvault_keys::{KeyClient, models::KeyOperationResult};
use base64::Engine;
use error_stack::{report, ResultExt};
use azure_core::HttpError;
use std::sync::Arc;

use crate::{crypto::consts::BASE64_ENGINE, error::ConfigurationError, logger};

/// Configuration parameters required for constructing a [`AzureKeyVaultClient`].
#[derive(Clone, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(default)]
pub struct AzureKeyVaultConfig {
    /// The Azure key identifier or name of the Key Vault key used for encryption or decryption.
    pub key_id: String,

    /// The Azure Key Vault URL where the keys are stored.
    pub key_vault_url: String,
}

/// Client for Azure Key Vault operations.
#[derive(Debug, Clone)]
pub struct AzureKeyVaultClient {
    inner_client: Arc<KeyClient>,
    key_id: String,
}

impl AzureKeyVaultClient {
    /// Constructs a new Azure Key Vault client.
    pub async fn new(config: &AzureKeyVaultConfig) -> Self {
        let credential = DefaultAzureCredential::default();
        let client = KeyClient::new(&config.key_vault_url, credential)
            .expect("Failed to create azure vault KeyClient");

        Self {
            inner_client: Arc::new(client),
            key_id: config.key_id.clone(),
        }
    }

    /// Decrypts the provided base64-encoded encrypted data using the Azure Key Vault SDK. We assume
    /// that the Azure SDK has the values required to interact with the Azure APIs (`AZURE_CLIENT_ID`,
    /// `AZURE_TENANT_ID`, `AZURE_CLIENT_SECRET`) set in environment variables, or that the SDK is running
    /// in a machine that can assume a Managed Identity.
    pub async fn decrypt(
        &self,
        data: impl AsRef<[u8]>,
    ) -> error_stack::Result<String, AzureKeyVaultError> {
        let data = BASE64_ENGINE
            .decode(data)
            .change_context(AzureKeyVaultError::Base64DecodingFailed)?;
        
        let decrypt_result = self
            .inner_client
            .decrypt(&self.key_id, data, azure_security_keyvault_keys::models::DecryptParameters::RsaOaep256)
            .await
            .map_err(|error| {
                // Log the error using its `Debug` representation
                logger::error!(azure_kv_sdk_error=?error, "Failed to Azure Key Vault decrypt data");
                error
            })
            .change_context(AzureKeyVaultError::DecryptionFailed)?;

        // Convert the decrypted result to a UTF-8 string.
        let output = String::from_utf8(decrypt_result.result)
            .map_err(|_| report!(AzureKeyVaultError::Utf8DecodingFailed))?;
        
        Ok(output)
    }
}

/// Errors that could occur during Key Vault operations.
#[derive(Debug, thiserror::Error)]
pub enum AzureKeyVaultError {
    /// An error occurred when base64 encoding input data.
    #[error("Failed to base64 encode input data")]
    Base64EncodingFailed,

    /// An error occurred when base64 decoding input data.
    #[error("Failed to base64 decode input data")]
    Base64DecodingFailed,

    /// An error occurred when Azure Key Vault decrypting input data.
    #[error("Failed to Azure Key Vault decrypt input data")]
    DecryptionFailed,

    /// An error occurred when Azure Key Vault encrypting input data.
    #[error("Failed to Azure Key Vault encrypt input data")]
    EncryptionFailed,

    /// The Azure Key Vault decrypted output does not include plaintext output.
    #[error("Missing plaintext Azure Key Vault decryption output")]
    MissingPlaintextDecryptionOutput,

    /// The Azure Key Vault encrypted output does not include ciphertext output.
    #[error("Missing ciphertext Azure Key Vault encryption output")]
    MissingCiphertextEncryptionOutput,

    /// An error occurred UTF-8 decoding Azure Key Vault decrypted output.
    #[error("Failed to UTF-8 decode decryption output")]
    Utf8DecodingFailed,

    /// The Azure Key Vault client has not been initialized.
    #[error("The Azure Key Vault client has not been initialized")]
    AzureKeyVaultClientNotInitialized,
}

impl AzureKeyVaultConfig {
    /// Verifies that the [`AzureKeyVaultClient`] configuration is usable.
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if self.key_id.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "Azure Key Vault key ID must not be empty".into(),
            ));
        }

        if self.key_vault_url.trim().is_empty() {
            return Err(ConfigurationError::InvalidConfigurationValueError(
                "Azure Key Vault URL must not be empty".into(),
            ));
        }

        Ok(())
    }
}
