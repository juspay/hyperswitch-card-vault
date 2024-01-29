use super::logger::config::Log;
#[cfg(feature = "kms-aws")]
use crate::crypto::aws_kms;

#[cfg(feature = "kms-hashicorp-vault")]
use crate::crypto::hcvault;

use std::path::PathBuf;

#[derive(Clone, serde::Deserialize, Debug)]
pub struct Config {
    pub server: Server,
    pub database: Database,
    pub secrets: Secrets,
    #[serde(flatten)]
    pub key_management_service: Option<EncryptionScheme>,
    pub log: Log,
    #[cfg(feature = "limit")]
    pub limit: Limit,
    #[cfg(feature = "caching")]
    pub cache: Cache,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EncryptionScheme {
    #[cfg(feature = "kms-aws")]
    AwsKms(aws_kms::KmsConfig),
    #[cfg(feature = "kms-hashicorp-vault")]
    VaultKv2(hcvault::HashiCorpVaultConfig),
}

#[cfg(feature = "limit")]
#[derive(Clone, serde::Deserialize, Debug)]
pub struct Limit {
    pub request_count: u64,
    pub duration: u64, // in sec
    pub buffer_size: Option<usize>,
}

#[derive(Clone, serde::Deserialize, Debug)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, serde::Deserialize, Debug)]
pub struct Database {
    pub username: String,
    // KMS encrypted
    pub password: masking::Secret<String>,
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub pool_size: Option<usize>,
}

#[cfg(feature = "caching")]
#[derive(Clone, serde::Deserialize, Debug)]
pub struct Cache {
    // time to idle (in secs)
    pub tti: Option<u64>,
    // maximum capacity of the cache
    pub max_capacity: u64,
}

#[derive(Clone, serde::Deserialize, Debug)]
pub struct Secrets {
    pub tenant: String,
    // KMS encrypted
    #[serde(deserialize_with = "deserialize_hex")]
    pub master_key: Vec<u8>,
    // KMS encrypted
    #[cfg(feature = "middleware")]
    pub locker_private_key: masking::Secret<String>,
    // KMS encrypted
    #[cfg(feature = "middleware")]
    pub tenant_public_key: masking::Secret<String>,
}

fn deserialize_hex<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let deserialized_str: String = serde::Deserialize::deserialize(deserializer)?;

    let deserialized_str = deserialized_str.into_bytes();

    Ok(deserialized_str)
}

/// Get the origin directory of the project
pub fn workspace_path() -> PathBuf {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        PathBuf::from(manifest_dir)
    } else {
        PathBuf::from(".")
    }
}

impl Config {
    /// Function to build the configuration by picking it from default locations
    pub fn new() -> Result<Self, config::ConfigError> {
        Self::new_with_config_path(None)
    }

    /// Function to build the configuration by picking it from default locations
    pub fn new_with_config_path(
        explicit_config_path: Option<PathBuf>,
    ) -> Result<Self, config::ConfigError> {
        let env = "dev";
        let config_path = Self::config_path(env, explicit_config_path);

        let config = Self::builder(env)?
            .add_source(config::File::from(config_path).required(false))
            .add_source(config::Environment::with_prefix("LOCKER").separator("__"))
            .build()?;

        serde_path_to_error::deserialize(config).map_err(|error| {
            eprintln!("Unable to deserialize application configuration: {error}");
            error.into_inner()
        })
    }

    pub fn builder(
        environment: &str,
    ) -> Result<config::ConfigBuilder<config::builder::DefaultState>, config::ConfigError> {
        config::Config::builder()
            // Here, it should be `set_override()` not `set_default()`.
            // "env" can't be altered by config field.
            // Should be single source of truth.
            .set_override("env", environment)
    }

    /// Config path.
    pub fn config_path(environment: &str, explicit_config_path: Option<PathBuf>) -> PathBuf {
        let mut config_path = PathBuf::new();
        if let Some(explicit_config_path_val) = explicit_config_path {
            config_path.push(explicit_config_path_val);
        } else {
            let config_directory: String = "config".into();
            let config_file_name = match environment {
                "production" => "production.toml",
                "sandbox" => "sandbox.toml",
                _ => "development.toml",
            };

            config_path.push(workspace_path());
            config_path.push(config_directory);
            config_path.push(config_file_name);
        }
        config_path
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::assertions_on_constants
    )]
    use super::*;

    #[derive(Clone, serde::Deserialize, Debug)]
    struct TestDeser {
        #[serde(flatten)]
        pub key_management_service: Option<EncryptionScheme>,
    }

    #[test]
    fn test_non_case() {
        let data = r#"

        "#;
        let parsed: TestDeser = serde_path_to_error::deserialize(
            config::Config::builder()
                .add_source(config::File::from_str(data, config::FileFormat::Toml))
                .build()
                .unwrap(),
        )
        .unwrap();
        assert!(parsed.key_management_service.is_none())
    }

    #[cfg(feature = "kms-aws")]
    #[test]
    fn test_aws_kms_case() {
        let data = r#"
        [aws_kms]
        key_id = "123"
        region = "abc"
        "#;
        let parsed: TestDeser = serde_path_to_error::deserialize(
            config::Config::builder()
                .add_source(config::File::from_str(data, config::FileFormat::Toml))
                .build()
                .unwrap(),
        )
        .unwrap();

        match parsed.key_management_service {
            Some(EncryptionScheme::AwsKms(value)) => {
                assert!(value.key_id == "123" && value.region == "abc")
            }
            _ => assert!(false),
        }
    }

    #[cfg(feature = "kms-hashicorp-vault")]
    #[test]
    fn test_hashicorp_case() {
        let data = r#"
        [vault_kv2]
        url = "123"
        token = "abc"
        "#;
        let parsed: TestDeser = serde_path_to_error::deserialize(
            config::Config::builder()
                .add_source(config::File::from_str(data, config::FileFormat::Toml))
                .build()
                .unwrap(),
        )
        .unwrap();

        match parsed.key_management_service {
            Some(EncryptionScheme::VaultKv2(value)) => {
                assert!(value.url == "123" && value.token == "abc")
            }
            _ => assert!(false),
        }
    }
}
