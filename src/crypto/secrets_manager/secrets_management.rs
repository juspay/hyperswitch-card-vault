#[cfg(any(feature = "kms-gcp", feature = "kms-hashicorp-vault"))]
use error_stack::ResultExt;
use hyperswitch_masking::Secret;

#[cfg(feature = "kms-aws")]
use crate::crypto::secrets_manager::managers::aws_kms::core::{AwsKmsClient, AwsKmsConfig};
#[cfg(feature = "kms-gcp")]
use crate::crypto::secrets_manager::managers::gcp_kms::core::{GcpKmsClient, GcpKmsConfig};
#[cfg(feature = "kms-hashicorp-vault")]
use crate::crypto::secrets_manager::managers::hcvault::core::{
    HashiCorpVault, HashiCorpVaultConfig,
};
use crate::{
    crypto::secrets_manager::{
        managers::hollow::core::NoEncryption,
        secrets_interface::{SecretManager, SecretsManagementError},
    },
    error::ConfigurationError,
};

#[cfg(any(
    feature = "kms-aws",
    feature = "kms-hashicorp-vault",
    feature = "kms-gcp"
))]
async fn record_secret_manager_duration<Fut, T, E>(
    future: Fut,
    backend: &'static str,
    operation: &'static str,
) -> Result<T, E>
where
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let start = std::time::Instant::now();
    let result = future.await;
    let duration = start.elapsed();
    let outcome = if result.is_ok() { "success" } else { "error" };

    crate::observability::metrics::SECRET_MANAGER_CALL_DURATION.record(
        duration.as_secs_f64(),
        metrics_utils::metric_attributes!(
            ("backend", backend),
            ("operation", operation),
            ("outcome", outcome),
        ),
    );

    result
}

/// Enum representing configuration options for secrets management.
#[derive(Debug, Clone, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(tag = "secrets_manager")]
#[serde(rename_all = "snake_case")]
pub enum SecretsManagementConfig {
    /// AWS KMS configuration
    #[cfg(feature = "kms-aws")]
    AwsKms {
        /// AWS KMS config
        aws_kms: AwsKmsConfig,
    },

    /// HashiCorp-Vault configuration
    #[cfg(feature = "kms-hashicorp-vault")]
    HashiCorpVault {
        /// HC-Vault config
        hashi_corp_vault: HashiCorpVaultConfig,
    },

    /// GCP KMS configuration
    #[cfg(feature = "kms-gcp")]
    GcpKms {
        /// GCP KMS config
        gcp_kms: GcpKmsConfig,
    },

    /// Variant representing no encryption
    #[default]
    NoEncryption,
}

enum SecretsManagerClient {
    #[cfg(feature = "kms-aws")]
    AwsKms(AwsKmsClient),
    #[cfg(feature = "kms-hashicorp-vault")]
    HashiCorp(Box<HashiCorpVault>),
    #[cfg(feature = "kms-gcp")]
    GcpKms(Box<GcpKmsClient>),
    NoEncryption(NoEncryption),
}

#[async_trait::async_trait]
impl SecretManager for SecretsManagerClient {
    async fn get_secret(
        &self,
        input: Secret<String>,
    ) -> error_stack::Result<Secret<String>, SecretsManagementError> {
        match self {
            #[cfg(feature = "kms-aws")]
            Self::AwsKms(config) => {
                record_secret_manager_duration(config.get_secret(input), "aws_kms", "decrypt").await
            }
            #[cfg(feature = "kms-hashicorp-vault")]
            Self::HashiCorp(config) => {
                record_secret_manager_duration(config.get_secret(input), "hashicorp_vault", "read")
                    .await
            }
            #[cfg(feature = "kms-gcp")]
            Self::GcpKms(config) => {
                record_secret_manager_duration(config.get_secret(input), "gcp_kms", "decrypt").await
            }
            Self::NoEncryption(config) => config.get_secret(input).await,
        }
    }
}

impl SecretsManagementConfig {
    /// Verifies that the client configuration is usable
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        match self {
            #[cfg(feature = "kms-aws")]
            Self::AwsKms { aws_kms } => aws_kms.validate(),
            #[cfg(feature = "kms-hashicorp-vault")]
            Self::HashiCorpVault { hashi_corp_vault } => hashi_corp_vault.validate(),
            #[cfg(feature = "kms-gcp")]
            Self::GcpKms { gcp_kms } => gcp_kms.validate(),
            Self::NoEncryption => Ok(()),
        }
    }

    /// Retrieves the appropriate secret management client based on the configuration.
    pub async fn get_secret_management_client(
        &self,
    ) -> error_stack::Result<impl SecretManager, SecretsManagementError> {
        match self {
            #[cfg(feature = "kms-aws")]
            Self::AwsKms { aws_kms } => Ok(SecretsManagerClient::AwsKms(
                AwsKmsClient::new(aws_kms).await,
            )),
            #[cfg(feature = "kms-hashicorp-vault")]
            Self::HashiCorpVault { hashi_corp_vault } => HashiCorpVault::new(hashi_corp_vault)
                .change_context(SecretsManagementError::ClientCreationFailed)
                .map(|vault| SecretsManagerClient::HashiCorp(Box::new(vault))),
            #[cfg(feature = "kms-gcp")]
            Self::GcpKms { gcp_kms } => GcpKmsClient::new(gcp_kms)
                .await
                .change_context(SecretsManagementError::ClientCreationFailed)
                .map(|client| SecretsManagerClient::GcpKms(Box::new(client))),
            Self::NoEncryption => Ok(SecretsManagerClient::NoEncryption(NoEncryption)),
        }
    }
}
