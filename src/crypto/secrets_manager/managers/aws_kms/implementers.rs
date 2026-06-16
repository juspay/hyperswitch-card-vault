use base64::Engine;
use error_stack::ResultExt;
use hyperswitch_masking::{PeekInterface, Secret};

use crate::crypto::{
    consts::BASE64_ENGINE,
    secrets_manager::{
        managers::aws_kms::core::{AwsKmsClient, AwsKmsError},
        secrets_interface::{SecretManager, SecretsManagementError},
    },
};

#[async_trait::async_trait]
impl SecretManager for AwsKmsClient {
    async fn get_secret(
        &self,
        input: Secret<String>,
    ) -> error_stack::Result<Secret<String>, SecretsManagementError> {
        let decoded = BASE64_ENGINE
            .decode(input.peek())
            .change_context(AwsKmsError::Base64DecodingFailed)
            .change_context(SecretsManagementError::FetchSecretFailed)?;

        let plaintext = self
            .decrypt(&decoded, None)
            .await
            .change_context(SecretsManagementError::FetchSecretFailed)?;

        String::from_utf8(plaintext)
            .change_context(AwsKmsError::Utf8DecodingFailed)
            .change_context(SecretsManagementError::FetchSecretFailed)
            .map(Into::into)
    }
}
