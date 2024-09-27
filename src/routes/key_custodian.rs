use std::sync::Arc;

use axum::{extract::State, routing::post, Json};
use error_stack::ResultExt;

use crate::{
    app::TenantAppState,
    config::TenantConfig,
    crypto::encryption_manager::{encryption_interface::Encryption, managers::aes::GcmAes256},
    custom_extractors::TenantId,
    error::{self, ResultContainerExt},
    logger,
    tenant::GlobalAppState,
};

const KEY_LENGTH: usize = 16;

#[derive(Clone, Default, Debug)]
pub struct CustodianKeyState {
    pub key1: Option<String>,
    pub key2: Option<String>,
}

/// Api request model for /custodian/key1 and /custodian/key2 routes
#[derive(serde::Deserialize)]
pub struct CustodianReqPayload {
    #[serde(deserialize_with = "key_validation")]
    pub key: String,
}

fn key_validation<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let deserialized_str: String = serde::Deserialize::deserialize(deserializer)?;

    let hex_data = hex::decode(&deserialized_str)
        .map_err(|_| serde::de::Error::custom("error while parsing hex"))?;

    (hex_data.len() == KEY_LENGTH)
        .then_some(())
        .ok_or(serde::de::Error::custom("Error while validating key"))?;

    Ok(deserialized_str)
}

#[derive(serde::Serialize, Debug)]
pub struct CustodianRespPayload {
    pub message: String,
}

///
/// Function for registering routes that is specifically handling the custodian apis
///
pub fn serve() -> axum::Router<Arc<GlobalAppState>> {
    axum::Router::new()
        .route("/key1", post(key1))
        .route("/key2", post(key2))
        .route("/decrypt", post(decrypt))
}

/// Handler for `/custodian/key1`
pub async fn key1(
    State(global_app_state): State<Arc<GlobalAppState>>,
    TenantId(tenant_id): TenantId,
    Json(payload): Json<CustodianReqPayload>,
) -> Json<CustodianRespPayload> {
    let mut key_state = global_app_state.tenants_key_state.write().await;
    key_state
        .entry(tenant_id.to_string())
        .and_modify(|key_state_data| key_state_data.key1 = Some(payload.key));

    logger::info!("Received key1");
    Json(CustodianRespPayload {
        message: "Received Key1".into(),
    })
}

/// Handler for `/custodian/key2`
pub async fn key2(
    State(global_app_state): State<Arc<GlobalAppState>>,
    TenantId(tenant_id): TenantId,
    Json(payload): Json<CustodianReqPayload>,
) -> Json<CustodianRespPayload> {
    let mut key_state = global_app_state.tenants_key_state.write().await;
    key_state
        .entry(tenant_id.to_string())
        .and_modify(|key_state_data| key_state_data.key2 = Some(payload.key));

    logger::info!("Received key2");
    Json(CustodianRespPayload {
        message: "Received Key2".into(),
    })
}

/// Handler for `/custodian/decrypt`
pub async fn decrypt(
    State(global_app_state): State<Arc<GlobalAppState>>,
    TenantId(tenant_id): TenantId,
) -> Result<Json<CustodianRespPayload>, error::ContainerError<error::ApiError>> {
    let mut key_state_map = global_app_state.tenants_key_state.write().await;
    let key_state_for_tenant = key_state_map
        .get_mut(&tenant_id.to_string())
        .ok_or(error::ApiError::TenantError("Tenant not found"))?;

    let decrypt_output = match key_state_for_tenant {
        CustodianKeyState {
            key1: Some(inner_key1),
            key2: Some(inner_key2),
        } => {
            let tenant_secrets = global_app_state
                .global_config
                .tenant_secrets
                .get(&tenant_id)
                .ok_or(error_stack::report!(error::ApiError::TenantError(
                    "Error while retrieving tenant config"
                )))?;

            let mut tenant_config = TenantConfig::from_global_config(
                &global_app_state.global_config,
                tenant_id.to_owned(),
                tenant_secrets.schema.clone(),
            );
            aes_decrypt_custodian_key(&mut tenant_config, inner_key1, inner_key2).await?;

            let tenant_app_state = TenantAppState::new(
                &global_app_state.global_config,
                tenant_config,
                global_app_state.api_client.clone(),
            )
            .await
            .change_context(error::ApiError::TenantError(
                "Failed while creating AppState for tenant",
            ))?;

            global_app_state.set_app_state(tenant_app_state).await;

            logger::info!("Decryption of Custodian key is successful");
            Ok(Json(CustodianRespPayload {
                message: "Decryption of Custodian key is successful".into(),
            }))
        }
        _ => {
            logger::error!("Both the custodian keys are not present to decrypt");
            Err(error::ApiError::DecryptingKeysFailed(
                "Both the custodain keys are not present to decrypt",
            ))
        }
    };
    match decrypt_output {
        Ok(inner) => Ok(inner),
        Err(inner_err) => {
            key_state_for_tenant.key1 = None;
            key_state_for_tenant.key2 = None;

            Err(inner_err)?
        }
    }
}

async fn aes_decrypt_custodian_key(
    tenant_config: &mut TenantConfig,
    inner_key1: &str,
    inner_key2: &str,
) -> Result<(), error::ContainerError<error::ApiError>> {
    let custodian_key = format!("{}{}", inner_key1, inner_key2);
    // required by the AES algorithm instead of &[u8]
    let aes_decrypted_master_key = GcmAes256::new(
        hex::decode(custodian_key)
            .change_error(error::ApiError::DecryptingKeysFailed("Hex dcoding failed"))?,
    )
    .decrypt(tenant_config.tenant_secrets.master_key.clone())
    .change_error(error::ApiError::DecryptingKeysFailed(
        "AES decryption failed",
    ))?;

    tenant_config.tenant_secrets.master_key = aes_decrypted_master_key;
    Ok(())
}
