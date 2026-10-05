//! OCI Vault KMS secrets manager, built on the shared `oci_kms` client from the Hyperswitch
//! repository.
//!
//! Authentication (OKE Workload Identity inside Kubernetes, `~/.oci/config` outside it),
//! request signing, timeouts and retries all live in that crate; this module only adapts it to
//! [`SecretManager`].

use error_stack::{Report, ResultExt};
use hyperswitch_masking::{PeekInterface, Secret};

use crate::{
    crypto::secrets_manager::secrets_interface::{SecretManager, SecretsManagementError},
    logger,
};

#[async_trait::async_trait]
impl SecretManager for ::oci_kms::OciKmsClient {
    async fn get_secret(
        &self,
        input: Secret<String>,
    ) -> error_stack::Result<Secret<String>, SecretsManagementError> {
        let plaintext = self.decrypt(input.peek()).await.map_err(|error| {
            logger::error!(oci_kms_error = %error, "Failed to OCI KMS decrypt data");
            Report::new(error).change_context(SecretsManagementError::FetchSecretFailed)
        })?;

        String::from_utf8(plaintext)
            .change_context(SecretsManagementError::FetchSecretFailed)
            .attach_printable("OCI KMS decrypted output is not valid UTF-8")
            .map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic)]

    use super::*;

    fn env(name: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for live tests"))
    }

    /// Decrypts a secret against a real OCI Vault. Skipped by default; run with:
    ///
    /// ```text
    /// OCI_KMS_TEST_CRYPTO_ENDPOINT=https://<vault>-crypto.kms.<region>.oci.oraclecloud.com \
    /// OCI_KMS_TEST_KEY_ID=ocid1.key.oc1... \
    /// OCI_KMS_TEST_CLI_CIPHERTEXT=<output of `oci kms crypto encrypt`> \
    /// OCI_KMS_TEST_CLI_PLAINTEXT=<the plaintext that was encrypted> \
    /// cargo test --features kms-oci oci_kms -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "calls a real OCI Vault"]
    async fn get_secret_decrypts_a_secret() {
        let client = ::oci_kms::OciKmsClient::new(&::oci_kms::OciKmsConfig {
            vault_crypto_endpoint: env("OCI_KMS_TEST_CRYPTO_ENDPOINT"),
            key_id: env("OCI_KMS_TEST_KEY_ID"),
        })
        .expect("client builds");

        let secret = client
            .get_secret(Secret::new(env("OCI_KMS_TEST_CLI_CIPHERTEXT")))
            .await
            .expect("get_secret succeeds");
        assert_eq!(secret.peek(), &env("OCI_KMS_TEST_CLI_PLAINTEXT"));
    }
}
