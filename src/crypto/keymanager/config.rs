#[cfg(feature = "external_key_manager")]
use masking::{ExposeInterface, Secret};

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
            Self::Enabled { url } => Some(url),
            #[cfg(feature = "external_key_manager")]
            Self::EnabledWithMtls { url, .. } => Some(url),
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
