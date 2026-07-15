use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use error_stack::ResultExt;
#[cfg(feature = "external_key_manager")]
use hyperswitch_masking::Secret;
use hyperswitch_masking::{ExposeInterface, PeekInterface};
#[cfg(feature = "redis")]
use hyperswitch_redis_interface::RedisSettings;

use crate::{
    api_client::ApiClientConfig,
    crypto::secrets_manager::{
        secrets_interface::SecretManager, secrets_management::SecretsManagementConfig,
    },
    error,
    logger::config::Log,
    observability::MetricsConfig,
};

#[derive(Clone, serde::Deserialize, Debug)]
pub struct GlobalConfig {
    pub server: Server,
    pub database: Database,
    pub read_replica: Option<Database>,
    pub secrets: Secrets,
    #[serde(default)]
    pub secrets_management: SecretsManagementConfig,
    pub log: Log,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[cfg(feature = "limit")]
    pub limit: Limit,
    #[cfg(feature = "caching")]
    pub cache: Cache,
    pub tenant_secrets: TenantsSecrets,
    pub tls: Option<ServerTls>,
    #[serde(default)]
    pub api_client: ApiClientConfig,
    #[serde(default)]
    pub external_key_manager: ExternalKeyManagerConfig,
    #[cfg(feature = "redis")]
    pub redis: Option<RedisSettings>,
    #[serde(default)]
    pub runtime_config: RuntimeConfig,
    #[cfg(feature = "kv")]
    #[serde(default)]
    pub kv: KvConfig,
}

#[derive(Clone, Debug)]
pub struct TenantConfig {
    pub tenant_id: String,
    pub locker_secrets: Secrets,
    pub tenant_secrets: TenantSecrets,
    pub external_key_manager: ExternalKeyManagerConfig,
    /// Redis key namespace for this tenant.
    #[cfg(feature = "redis")]
    pub redis_key_prefix: String,
}

impl TenantConfig {
    ///
    /// # Panics
    ///
    /// Never, as tenant_id would already be validated from [`crate::custom_extractors::TenantId`] custom extractor
    ///
    pub fn from_global_config(global_config: &GlobalConfig, tenant_id: String) -> Self {
        #[allow(clippy::unwrap_used)]
        let tenant_secrets = global_config
            .tenant_secrets
            .get(&tenant_id)
            .cloned()
            .unwrap();

        #[cfg(feature = "redis")]
        let redis_key_prefix = tenant_secrets.redis_key_prefix.clone();

        Self {
            tenant_id,
            locker_secrets: global_config.secrets.clone(),
            tenant_secrets,
            external_key_manager: global_config.external_key_manager.clone(),
            #[cfg(feature = "redis")]
            redis_key_prefix,
        }
    }
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
    pub password: hyperswitch_masking::Secret<String>,
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
    pub metrics_collection_interval_secs: Option<u64>,
}

#[derive(Clone, serde::Deserialize, Debug)]
pub struct Secrets {
    // KMS encrypted
    #[cfg(feature = "middleware")]
    pub locker_private_key: hyperswitch_masking::Secret<String>,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct TenantSecrets {
    #[serde(deserialize_with = "deserialize_hex")]
    pub master_key: Vec<u8>,
    #[cfg(feature = "middleware")]
    pub public_key: hyperswitch_masking::Secret<String>,

    /// schema name for the tenant (defaults to tenant_id)
    pub schema: String,

    /// Redis key prefix (deser-only; app reads `TenantConfig.redis_key_prefix`).
    #[cfg(feature = "redis")]
    #[serde(default)]
    pub redis_key_prefix: String,
}

fn deserialize_hex<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let deserialized_str: String = serde::Deserialize::deserialize(deserializer)?;

    let deserialized_str = deserialized_str.into_bytes();

    Ok(deserialized_str)
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct TenantsSecrets(HashMap<String, TenantSecrets>);

impl Deref for TenantsSecrets {
    type Target = HashMap<String, TenantSecrets>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TenantsSecrets {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct ServerTls {
    /// certificate file associated with TLS (path to the certificate file (`pem` format))
    pub certificate: String,
    /// private key file path associated with TLS (path to the private key file (`pem` format))
    pub private_key: String,
}

impl Default for ApiClientConfig {
    fn default() -> Self {
        Self {
            client_idle_timeout: 90,
            pool_max_idle_per_host: 5,
            #[cfg(feature = "external_key_manager")]
            identity: hyperswitch_masking::Secret::default(),
        }
    }
}

/// Get the origin directory of the project
pub fn workspace_path() -> PathBuf {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        PathBuf::from(manifest_dir)
    } else {
        PathBuf::from(".")
    }
}

impl GlobalConfig {
    /// Function to build the configuration by picking it from default locations
    pub fn new() -> Result<Self, config::ConfigError> {
        Self::new_with_config_path(None)
    }

    /// Function to build the configuration by picking it from default locations
    pub fn new_with_config_path(
        explicit_config_path: Option<PathBuf>,
    ) -> Result<Self, config::ConfigError> {
        let env = Env::current_env();
        let config_path = Self::config_path(&env, explicit_config_path);

        let config = Self::builder(&env)?
            .add_source(config::File::from(config_path).required(false))
            .add_source(
                config::Environment::with_prefix("LOCKER")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        serde_path_to_error::deserialize(config).map_err(|error| {
            eprintln!("Unable to deserialize application configuration: {error}");
            error.into_inner()
        })
    }

    pub fn builder(
        environment: &Env,
    ) -> Result<config::ConfigBuilder<config::builder::DefaultState>, config::ConfigError> {
        config::Config::builder()
            // Here, it should be `set_override()` not `set_default()`.
            // "env" can't be altered by config field.
            // Should be single source of truth.
            .set_override("env", environment.to_string())
    }

    /// Config path.
    pub fn config_path(environment: &Env, explicit_config_path: Option<PathBuf>) -> PathBuf {
        let mut config_path = PathBuf::new();
        if let Some(explicit_config_path_val) = explicit_config_path {
            config_path.push(explicit_config_path_val);
        } else {
            let config_directory: String = "config".into();
            let config_file_name = environment.config_path();

            config_path.push(workspace_path());
            config_path.push(config_directory);
            config_path.push(config_file_name);
        }
        config_path
    }

    /// # Panics
    ///
    /// - If secret management client cannot be constructed
    /// - If master key cannot be utf8 decoded to String
    /// - If master key cannot be hex decoded
    ///
    #[allow(clippy::expect_used)]
    pub async fn fetch_raw_secrets(
        &mut self,
    ) -> error_stack::Result<(), error::ConfigurationError> {
        let secret_management_client = self
            .secrets_management
            .get_secret_management_client()
            .await
            .expect("Failed to create secret management client");

        self.database.password = secret_management_client
            .get_secret(self.database.password.clone())
            .await
            .change_context(error::ConfigurationError::KmsDecryptError(
                "database_password",
            ))?;

        if let Some(ref mut read_replica) = self.read_replica {
            read_replica.password = secret_management_client
                .get_secret(read_replica.password.clone())
                .await
                .change_context(error::ConfigurationError::KmsDecryptError(
                    "read_replica_password",
                ))?;
        }

        for tenant_secrets in self.tenant_secrets.values_mut() {
            tenant_secrets.master_key = hex::decode(
                secret_management_client
                    .get_secret(
                        String::from_utf8(tenant_secrets.master_key.clone())
                            .expect("Failed while converting master key to `String`")
                            .into(),
                    )
                    .await
                    .change_context(error::ConfigurationError::KmsDecryptError("master_key"))?
                    .expose(),
            )
            .expect("Failed to hex decode master key")
        }

        #[cfg(feature = "middleware")]
        {
            for tenant_secrets in self.tenant_secrets.values_mut() {
                tenant_secrets.public_key = secret_management_client
                    .get_secret(tenant_secrets.public_key.clone())
                    .await
                    .change_context(error::ConfigurationError::KmsDecryptError("public_key"))?;
            }

            self.secrets.locker_private_key = secret_management_client
                .get_secret(self.secrets.locker_private_key.clone())
                .await
                .change_context(error::ConfigurationError::KmsDecryptError(
                    "locker_private_key",
                ))?;
        }

        if let RuntimeConfig::Enabled {
            ref mut endpoint, ..
        } = self.runtime_config
        {
            endpoint.api_key = secret_management_client
                .get_secret(endpoint.api_key.clone())
                .await
                .change_context(error::ConfigurationError::KmsDecryptError(
                    "runtime_config api_key",
                ))?;
        }

        #[cfg(feature = "external_key_manager")]
        {
            // Decrypt api_client.identity only when mTLS is enabled, as it's required for client certificate authentication
            if self.external_key_manager.is_mtls_enabled() {
                let decrypted_identity = secret_management_client
                    .get_secret(self.api_client.identity.clone())
                    .await
                    .change_context(error::ConfigurationError::KmsDecryptError(
                        "api_client-identity",
                    ))?;

                self.api_client.identity = decrypted_identity;
            }

            self.external_key_manager = match &self.external_key_manager {
                ExternalKeyManagerConfig::EnabledWithMtls { url, ca_cert } => {
                    let decrypted_ca_cert = secret_management_client
                        .get_secret(ca_cert.clone())
                        .await
                        .change_context(error::ConfigurationError::KmsDecryptError("ca_cert"))?;

                    ExternalKeyManagerConfig::EnabledWithMtls {
                        url: url.clone(),
                        ca_cert: decrypted_ca_cert,
                    }
                }
                ExternalKeyManagerConfig::Enabled { .. } | ExternalKeyManagerConfig::Disabled => {
                    self.external_key_manager.clone()
                }
            };
        }

        Ok(())
    }

    pub fn validate(&self) -> error_stack::Result<(), error::ConfigurationError> {
        self.secrets_management.validate()?;
        self.runtime_config.validate()?;
        #[cfg(feature = "kv")]
        {
            self.kv.validate()?;
            self.validate_kv_tenant_prefixes()?;
        }
        #[cfg(feature = "external_key_manager")]
        {
            self.external_key_manager.validate()?;
            self.api_client
                .validate_for_mtls(&self.external_key_manager)?;
        }
        self.metrics.validate()?;

        Ok(())
    }

    /// Require non-empty, unique `redis_key_prefix` per tenant when kv + redis + multi-tenant.
    #[cfg(feature = "kv")]
    fn validate_kv_tenant_prefixes(&self) -> Result<(), error::ConfigurationError> {
        #[cfg(feature = "redis")]
        if self.redis.is_none() || self.tenant_secrets.len() <= 1 {
            return Ok(());
        }

        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (tenant_id, secrets) in self.tenant_secrets.iter() {
            let prefix = secrets.redis_key_prefix.trim();
            if prefix.is_empty() {
                return Err(error::ConfigurationError::InvalidConfigurationValueError(
                    format!(
                        "tenant `{tenant_id}`: redis_key_prefix required with kv + multi-tenant"
                    ),
                ));
            }
            if !seen.insert(prefix) {
                return Err(error::ConfigurationError::InvalidConfigurationValueError(
                    format!("duplicate redis_key_prefix `{prefix}`"),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(feature = "kv")]
#[derive(Clone, Debug, serde::Deserialize)]
pub struct KvConfig {
    /// Drainer stream suffix: `{shard_N}_{suffix}`.
    #[serde(default = "default_drainer_stream_suffix")]
    pub drainer_stream_suffix: String,
    /// Drainer shard count. Must be `> 0` (validated in [`KvConfig::validate`]).
    #[serde(default = "default_drainer_num_partitions")]
    pub drainer_num_partitions: u8,
    /// TTL (seconds) for KV keys in Redis. Must exceed max drainer replay lag.
    #[serde(default = "default_ttl_for_kv")]
    pub ttl_for_kv: u32,
}

#[cfg(feature = "kv")]
fn default_drainer_stream_suffix() -> String {
    "DRAINER_STREAM".to_string()
}

#[cfg(feature = "kv")]
fn default_drainer_num_partitions() -> u8 {
    16
}

#[cfg(feature = "kv")]
fn default_ttl_for_kv() -> u32 {
    900
}

#[cfg(feature = "kv")]
impl Default for KvConfig {
    fn default() -> Self {
        Self {
            drainer_stream_suffix: default_drainer_stream_suffix(),
            drainer_num_partitions: default_drainer_num_partitions(),
            ttl_for_kv: default_ttl_for_kv(),
        }
    }
}

#[cfg(feature = "kv")]
impl KvConfig {
    /// Format: `{shard_key}_{suffix}`.
    pub fn drainer_stream_name(&self, shard_key: &str) -> String {
        format!("{{{}}}_{}", shard_key, self.drainer_stream_suffix)
    }

    /// Reject `drainer_num_partitions == 0` (crc32 % 0 panics).
    pub fn validate(&self) -> Result<(), error::ConfigurationError> {
        if self.drainer_num_partitions == 0 {
            return Err(error::ConfigurationError::InvalidConfigurationValueError(
                "kv.drainer_num_partitions must be greater than 0".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Env {
    Development,
    Release,
}

impl Env {
    pub const fn current_env() -> Self {
        if cfg!(debug_assertions) {
            Self::Development
        } else {
            Self::Release
        }
    }

    pub const fn config_path(self) -> &'static str {
        match self {
            Self::Development => "development.toml",
            Self::Release => "production.toml",
        }
    }
}

impl std::fmt::Display for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Development => write!(f, "development"),
            Self::Release => write!(f, "release"),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RuntimeConfigEndpoint {
    pub base_url: String,
    pub api_key: hyperswitch_masking::Secret<String>,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, hyperswitch_masking::Secret<String>>,
    #[serde(default)]
    pub path: String,
}

/// Runtime configuration source.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RuntimeConfig {
    #[default]
    Disabled,
    Enabled {
        endpoint: RuntimeConfigEndpoint,
        #[serde(default = "default_runtime_config_refresh_interval_seconds")]
        refresh_interval_seconds: u64,
    },
}

fn default_runtime_config_refresh_interval_seconds() -> u64 {
    30
}

impl RuntimeConfig {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    pub fn validate(&self) -> Result<(), crate::error::ConfigurationError> {
        if let Self::Enabled { endpoint, .. } = self {
            if endpoint.base_url.trim().is_empty() {
                return Err(
                    crate::error::ConfigurationError::InvalidConfigurationValueError(
                        r#"runtime_config.endpoint.base_url is required when mode is "enabled""#
                            .into(),
                    ),
                );
            }

            if endpoint.api_key.peek().trim().is_empty() {
                return Err(
                    crate::error::ConfigurationError::InvalidConfigurationValueError(
                        r#"runtime_config.endpoint.api_key is required when mode is "enabled""#
                            .into(),
                    ),
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(tag = "mode")]
#[serde(rename_all = "snake_case")]
pub enum ExternalKeyManagerConfig {
    #[default]
    Disabled,
    #[cfg(feature = "external_key_manager")]
    Enabled { url: String },
    #[cfg(feature = "external_key_manager")]
    EnabledWithMtls {
        url: String,
        ca_cert: Secret<String>,
    },
}

impl ExternalKeyManagerConfig {
    pub fn is_external(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    #[cfg(feature = "external_key_manager")]
    pub fn is_mtls_enabled(&self) -> bool {
        matches!(self, Self::EnabledWithMtls { .. })
    }

    pub fn get_url(&self) -> Option<&str> {
        match self {
            Self::Disabled => None,
            #[cfg(feature = "external_key_manager")]
            Self::Enabled { url } | Self::EnabledWithMtls { url, .. } => Some(url),
        }
    }

    #[cfg(feature = "external_key_manager")]
    pub fn get_url_required(&self) -> Result<&str, crate::error::KeyManagerError> {
        self.get_url().ok_or_else(|| {
            crate::error::KeyManagerError::MissingConfigurationError(
                "external_key_manager.url is required when external key manager is enabled".into(),
            )
        })
    }

    #[cfg(feature = "external_key_manager")]
    pub fn get_ca_cert(&self) -> Option<&Secret<String>> {
        match self {
            Self::EnabledWithMtls { ca_cert, .. } => Some(ca_cert),
            Self::Disabled | Self::Enabled { .. } => None,
        }
    }

    pub fn validate(&self) -> Result<(), crate::error::ConfigurationError> {
        match self {
            Self::Disabled => Ok(()),
            #[cfg(feature = "external_key_manager")]
            Self::Enabled { url } => {
                if url.trim().is_empty() {
                    return Err(crate::error::ConfigurationError::InvalidConfigurationValueError(
                        "external_key_manager.url is required when external key manager is enabled".into(),
                    ));
                }
                Ok(())
            }
            #[cfg(feature = "external_key_manager")]
            Self::EnabledWithMtls { url, ca_cert } => {
                if url.trim().is_empty() {
                    return Err(crate::error::ConfigurationError::InvalidConfigurationValueError(
                        "external_key_manager.url is required when external key manager is enabled".into(),
                    ));
                }
                if ca_cert.clone().expose().trim().is_empty() {
                    return Err(
                        crate::error::ConfigurationError::InvalidConfigurationValueError(
                            "external_key_manager.ca_cert is required when mTLS is enabled".into(),
                        ),
                    );
                }
                Ok(())
            }
        }
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
        #[serde(default)]
        pub secrets_management: SecretsManagementConfig,
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
        assert_eq!(
            parsed.secrets_management,
            SecretsManagementConfig::NoEncryption
        )
    }

    #[cfg(feature = "kms-aws")]
    #[test]
    fn test_aws_kms_case() {
        let data = r#"
        [secrets_management]
        secrets_manager = "aws_kms"

        [secrets_management.aws_kms]
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

        match parsed.secrets_management {
            SecretsManagementConfig::AwsKms { aws_kms } => {
                assert!(aws_kms.key_id == "123" && aws_kms.region == "abc")
            }
            _ => assert!(false),
        }
    }

    #[cfg(feature = "kms-hashicorp-vault")]
    #[test]
    fn test_hashicorp_case() {
        let data = r#"
        [secrets_management]
        secrets_manager = "hashi_corp_vault"

        [secrets_management.hashi_corp_vault]
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

        match parsed.secrets_management {
            SecretsManagementConfig::HashiCorpVault { hashi_corp_vault } => {
                assert!(hashi_corp_vault.url == "123" && hashi_corp_vault.token.expose() == "abc")
            }
            _ => assert!(false),
        }
    }
}
