use error_stack::ResultExt;
use masking::{PeekInterface, Secret};

use crate::crypto::secrets_manager::{
    managers::azure_kms::core::AzureKeyVaultClient,  // Update the path to reflect Azure usage
    secrets_interface::{SecretManager, SecretsManagementError},
};

#[async_trait::async_trait]
impl SecretManager for AzureKeyVaultClient {
    async fn get_secret(
        &self,
        input: Secret<String>,
    ) -> error_stack::Result<Secret<String>, SecretsManagementError> {
        // Decrypt the secret using the AzureKeyVaultClient's decrypt method
        self.decrypt(input.peek())  // `peek()` allows you to access the inner string
            .await
            .change_context(SecretsManagementError::FetchSecretFailed)  // Map errors to secret management failures
            .map(Into::into)  // Convert the decrypted string into `Secret<String>`
    }
}
