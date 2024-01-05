use std::{marker::PhantomData, pin::Pin};

use error_stack::ResultExt;
use futures_util::Future;
use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};

use crate::error::{ConfigurationError, KmsError};

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    pub url: String,

    pub token: String,
}

impl<Interface> HashiCorpVault<Interface> {
    pub fn new(
        config: &VaultConfig,
    ) -> error_stack::Result<Self, crate::error::ConfigurationError> {
        VaultClient::new(
            VaultClientSettingsBuilder::default()
                .address(&config.url)
                .token(&config.token)
                .build()
                .change_context(ConfigurationError::VaultClientError)
                .attach_printable("Failed while building vault settings")?,
        )
        .change_context(ConfigurationError::VaultClientError)
        .map(|client| Self {
            client,
            _inner: PhantomData,
        })
    }
}

pub struct HashiCorpVault<Interface> {
    _inner: PhantomData<Interface>,
    client: vaultrs::client::VaultClient,
}

pub struct Kv2;

trait Engine: Sized {
    type ReturnType<'b, T>
    where
        T: 'b,
        Self: 'b;
    fn read(client: &HashiCorpVault<Self>, location: String) -> Self::ReturnType<'_, String>;
}

impl Engine for Kv2 {
    type ReturnType<'b, T: 'b> =
        Pin<Box<dyn Future<Output = error_stack::Result<T, KmsError>> + 'b>>;
    fn read(client: &HashiCorpVault<Self>, location: String) -> Self::ReturnType<'_, String> {
        Box::pin(async move {
            let mut split = location.split(':');
            let mount = split.next().ok_or(KmsError::IncompleteData)?;
            let path = split.next().ok_or(KmsError::IncompleteData)?;

            vaultrs::kv2::read(&client.client, mount, path)
                .await
                .change_context(KmsError::FetchFailed)
        })
    }
}

impl<I: super::FromEncoded, Interface> super::Decode<I, String> for HashiCorpVault<Interface>
where
    for<'a> Interface: Engine<
            ReturnType<'a, String> = Pin<
                Box<dyn Future<Output = error_stack::Result<String, KmsError>> + 'a>,
            >,
        > + 'a,
{
    type ReturnType<'b, T> = Pin<Box<dyn Future<Output = error_stack::Result<T, KmsError>> + 'b>> where Interface: 'b;

    fn decode(&self, input: String) -> Self::ReturnType<'_, I> {
        Box::pin(async move {
            let output = Interface::read(self, input).await?;
            I::from_encoded(output)
                .ok_or(KmsError::Utf8DecodingFailed)
                .map_err(From::from)
        })
    }
}
