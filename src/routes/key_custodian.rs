use std::{ops::Deref, sync::Arc};

use axum::{extract::State, routing::post, Json};
use error_stack::ResultExt;
use tokio::sync::RwLock;

use crate::{
    app::{AppState, Keys, SharedState},
    crypto::{aes::GcmAes256, Encryption},
    error::{self, LogReport},
    logger,
};

const KEY_LENGTH: usize = 16;

/// Api request model for /custodian/key1 and /custodian/key2 routes
#[derive(serde::Deserialize)]
pub struct KeyPayload {
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

///
/// Function for creating the server that is specifically handling the custodian apis
///
pub fn serve() -> axum::Router<SharedState> {
    axum::Router::new()
        .route("/key1", post(key1))
        .route("/key2", post(key2))
        .route("/decrypt", post(decrypt))
}

/// Handler for `/custodian/key1`
pub async fn key1(
    State((_, keys, _)): State<SharedState>,
    Json(payload): Json<KeyPayload>,
) -> (hyper::StatusCode, &'static str) {
    keys.write().await.key1 = Some(payload.key);
    logger::info!("Received Key1");
    (hyper::StatusCode::OK, "Received Key1")
}

/// Handler for `/custodian/key2`
pub async fn key2(
    State((_, keys, _)): State<SharedState>,
    Json(payload): Json<KeyPayload>,
) -> (hyper::StatusCode, &'static str) {
    keys.write().await.key2 = Some(payload.key);
    logger::info!("Received Key2");
    (hyper::StatusCode::OK, "Received Key2")
}

/// Handler for `/custodian/decrypt`
pub async fn decrypt(
    State((state, keys, tx)): State<SharedState>,
) -> Result<&'static str, error::ApiError> {
    let decrypt_output = match keys.read().await.deref() {
        Keys {
            key1: Some(inner_key1),
            key2: Some(inner_key2),
        } => {
            aes_decrypt_custodian_key(state, inner_key1, inner_key2).await?;

            let _ = tx.send(()).await;
            logger::info!("Decryption of Custodian key is successful");
            Ok("Decryption successful")
        }
        _ => {
            logger::error!("Both the custodian keys are not present");
            Err(error::ApiError::DecryptingKeysFailed(
                "Both the custodain keys are not present",
            ))
        }
    };
    match decrypt_output {
        value @ Ok(_) => value,
        error @ Err(_) => {
            keys.write().await.key1 = None;
            keys.write().await.key2 = None;
            error
        }
    }
}

async fn aes_decrypt_custodian_key(
    state: Arc<RwLock<AppState>>,
    inner_key1: &str,
    inner_key2: &str,
) -> Result<(), error::ApiError> {
    let final_key = format!("{}{}", inner_key1, inner_key2);
    // required by the AES algorithm instead of &[u8]
    let aes_decrypted_key = GcmAes256::new(
        hex::decode(final_key)
            .change_context(error::ApiError::DecryptingKeysFailed("Hex dcoding failed"))
            .report_unwrap()?,
    )
    .decrypt(state.read().await.config.secrets.master_key.clone())
    .change_context(error::ApiError::DecryptingKeysFailed(
        "AES decryption failed",
    ))
    .report_unwrap()?;

    state.write().await.config.secrets.master_key = aes_decrypted_key;
    Ok(())
}
