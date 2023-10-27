use std::path::PathBuf;

#[derive(Clone, serde::Deserialize)]
pub struct Config {
    pub server: Server,
    pub database: Database,
    pub secrets: Secrets,
    pub locker_key: LockerKeys,
}

#[derive(Clone, serde::Deserialize)]
pub struct LockerKeys {
    pub locker_private_key: String,
    pub tenant_public_key: String,
}
#[derive(Clone, serde::Deserialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, serde::Deserialize)]
pub struct Database {
    pub url: String,
}

#[derive(Clone, serde::Deserialize)]
pub struct Secrets {
    pub tenant: String,
    #[serde(deserialize_with = "deserialize_hex")]
    pub master_key: Vec<u8>,
}

/// Function to deserialize hex -> Vec<u8> this is used in case of non KMS decryption
fn deserialize_hex<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let hex_str: String = serde::Deserialize::deserialize(deserializer)?;

    let bytes = match hex::decode(hex_str) {
        Ok(data) => data,
        Err(_) => return Err(serde::de::Error::custom("Base64 decoding error")),
    };

    Ok(bytes)
}

/// Get the origin directory of the project
pub fn workspace_path() -> PathBuf {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(manifest_dir);
        path
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
        let config_path = Self::config_path(&env, explicit_config_path);

        let config = Self::builder(&env)?
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
