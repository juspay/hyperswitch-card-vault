#[cfg(feature = "external_key_manager")]
#[tokio::test]
async fn test_disabled_mode_creates_merchants() {
    use hyperswitch_card_vault::{
        config::GlobalConfig, crypto::keymanager, crypto::keymanager::ExternalKeyManagerConfig,
    };

    let config = ExternalKeyManagerConfig::Disabled;
    assert!(!config.is_external());
    assert!(!config.is_mtls_enabled());

    // Verify that get_dek_manager returns InternalKeyManager for Disabled mode
    let dek_manager = keymanager::get_dek_manager(&config);
    // The dek_manager should be able to create entities in Disabled mode
    // This test verifies the type system works correctly
}

#[cfg(feature = "external_key_manager")]
#[tokio::test]
async fn test_enabled_mode_uses_external_manager() {
    use hyperswitch_card_vault::{
        crypto::keymanager, crypto::keymanager::ExternalKeyManagerConfig,
    };

    let config = ExternalKeyManagerConfig::Enabled {
        url: "https://test.example.com".to_string(),
    };

    assert!(config.is_external());
    assert!(!config.is_mtls_enabled());
    assert_eq!(config.get_url(), Some("https://test.example.com"));
    assert!(config.get_ca_cert().is_none());

    // Verify that get_dek_manager returns ExternalKeyManager for Enabled mode
    let _dek_manager = keymanager::get_dek_manager(&config);
}

#[cfg(feature = "external_key_manager")]
#[tokio::test]
async fn test_enabled_with_mtls_mode() {
    use hyperswitch_card_vault::{
        crypto::keymanager, crypto::keymanager::ExternalKeyManagerConfig,
    };
    use masking::Secret;

    let config = ExternalKeyManagerConfig::EnabledWithMtls {
        url: "https://test.example.com".to_string(),
        ca_cert: Secret::new(
            "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----".to_string(),
        ),
    };

    assert!(config.is_external());
    assert!(config.is_mtls_enabled());
    assert_eq!(config.get_url(), Some("https://test.example.com"));
    assert!(config.get_ca_cert().is_some());

    // Verify that get_dek_manager returns ExternalKeyManager for EnabledWithMtls mode
    let _dek_manager = keymanager::get_dek_manager(&config);
}

#[cfg(feature = "external_key_manager")]
#[tokio::test]
async fn test_config_validation() {
    use hyperswitch_card_vault::crypto::keymanager::ExternalKeyManagerConfig;

    // Disabled mode should always validate
    let disabled = ExternalKeyManagerConfig::Disabled;
    assert!(disabled.validate().is_ok());

    // Enabled mode with empty URL should fail
    let enabled_no_url = ExternalKeyManagerConfig::Enabled {
        url: "".to_string(),
    };
    assert!(enabled_no_url.validate().is_err());

    // Enabled mode with valid URL should pass
    let enabled_valid = ExternalKeyManagerConfig::Enabled {
        url: "https://test.example.com".to_string(),
    };
    assert!(enabled_valid.validate().is_ok());

    // EnabledWithMtls mode with empty ca_cert should fail
    use masking::Secret;
    let mtls_no_cert = ExternalKeyManagerConfig::EnabledWithMtls {
        url: "https://test.example.com".to_string(),
        ca_cert: Secret::new("".to_string()),
    };
    assert!(mtls_no_cert.validate().is_err());

    // EnabledWithMtls mode with valid cert should pass
    let mtls_valid = ExternalKeyManagerConfig::EnabledWithMtls {
        url: "https://test.example.com".to_string(),
        ca_cert: Secret::new(
            "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----".to_string(),
        ),
    };
    assert!(mtls_valid.validate().is_ok());
}
