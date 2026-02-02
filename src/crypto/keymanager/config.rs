use masking::{ExposeInterface, Secret};

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(tag = "mode")]
#[serde(rename_all = "snake_case")]
pub enum ExternalKeyManagerConfig {
    #[default]
    Disabled,
    Enabled {
        url: String,
    },
    EnabledWithMtls {
        url: String,
        ca_cert: Secret<String>,
    },
}

impl ExternalKeyManagerConfig {
    pub fn is_external(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    pub fn is_mtls_enabled(&self) -> bool {
        matches!(self, Self::EnabledWithMtls { .. })
    }

    pub fn get_url(&self) -> Option<&str> {
        match self {
            Self::Disabled => None,
            Self::Enabled { url } => Some(url),
            Self::EnabledWithMtls { url, .. } => Some(url),
        }
    }

    pub fn get_ca_cert(&self) -> Option<&Secret<String>> {
        match self {
            Self::EnabledWithMtls { ca_cert, .. } => Some(ca_cert),
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<(), crate::error::ConfigurationError> {
        match self {
            Self::Disabled => Ok(()),
            Self::Enabled { url } => {
                if url.trim().is_empty() {
                    return Err(crate::error::ConfigurationError::InvalidConfigurationValueError(
                        "external_key_manager.url is required when external key manager is enabled".into(),
                    ));
                }
                Ok(())
            }
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
