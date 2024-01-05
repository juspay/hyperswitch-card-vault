use std::pin::Pin;

use futures_util::Future;

use crate::error::{self, KmsError};

pub enum Multiple {
    #[cfg(feature = "kms")]
    AwsKms(super::kms::KmsClient),
    #[cfg(feature = "hashicorp-vault")]
    VaultKv2(super::hcvault::HashiCorpVault<super::hcvault::Kv2>),
    None(super::hollow::NoEncryption),
}

impl Multiple {
    pub async fn build(
        config: Option<&crate::config::EncryptionScheme>,
    ) -> error_stack::Result<Self, error::ConfigurationError> {
        #[allow(unreachable_patterns)]
        match config {
            #[cfg(feature = "kms")]
            Some(crate::config::EncryptionScheme::AwsKms(config)) => {
                Ok(Self::AwsKms(super::kms::KmsClient::new(config).await))
            }
            #[cfg(feature = "hashicorp-vault")]
            Some(crate::config::EncryptionScheme::VaultKv2(config)) => {
                Ok(Self::VaultKv2(super::hcvault::HashiCorpVault::new(config)?))
            }
            None => Ok(Self::None(super::hollow::NoEncryption)),
            _ => Err(error::ConfigurationError::KmsDecryptError("unreachable state").into()),
        }
    }
}

impl<I: super::FromEncoded> super::Decode<I, String> for Multiple {
    type ReturnType<'b, T> = Pin<Box<dyn Future<Output = error_stack::Result<T, KmsError>> + 'b>>;

    fn decode(&self, input: String) -> Self::ReturnType<'_, I> {
        match self {
            #[cfg(feature = "kms")]
            Self::AwsKms(client) => client.decode(input),
            #[cfg(feature = "hashicorp-vault")]
            Self::VaultKv2(client) => client.decode(input),
            Self::None(client) => Box::pin(async move {
                client
                    .decode(input)
                    .ok_or(KmsError::HexDecodingFailed)
                    .map_err(From::from)
            }),
        }
    }
}
