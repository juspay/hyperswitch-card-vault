use std::ops::Deref;

use axum::{extract::State, routing::post, Json};
use error_stack::ResultExt;

use crate::{
    app::{Keys, SharedState},
    crypto::{aes::GcmAes256, Encryption},
    error::{self, LogReport},
};

#[derive(serde::Deserialize)]
pub struct KeyPayload {
    pub key: String,
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

pub async fn key1(
    State((_, keys, _)): State<SharedState>,
    Json(payload): Json<KeyPayload>,
) -> (hyper::StatusCode, &'static str) {
    keys.write().await.key1 = Some(payload.key);
    (hyper::StatusCode::OK, "Received Key1")
}

pub async fn key2(
    State((_, keys, _)): State<SharedState>,
    Json(payload): Json<KeyPayload>,
) -> (hyper::StatusCode, &'static str) {
    keys.write().await.key2 = Some(payload.key);
    (hyper::StatusCode::OK, "Received Key2")
}

pub async fn decrypt(
    State((state, keys, tx)): State<SharedState>,
) -> Result<&'static str, error::ApiError> {
    match keys.read().await.deref() {
        Keys {
            key1: Some(inner_key1),
            key2: Some(inner_key2),
        } => {
            let final_key = format!("{}{}", inner_key1, inner_key2);
            let aes_decrypted_key =
                GcmAes256::new(state.read().await.config.secrets.master_key.clone())
                    .decrypt(final_key.into_bytes())
                    .change_context(error::ApiError::DecryptingKeysFailed(
                        "AES decryption failed",
                    ))
                    .report_unwrap()?;

            let master_key = String::from_utf8(aes_decrypted_key)
                .change_context(error::ApiError::DecryptingKeysFailed(
                    "Failed while parsing utf-8",
                ))
                .report_unwrap()?;
            state.write().await.config.secrets.master_key = master_key.into_bytes();

            let _ = tx.send(()).await;
            Ok("Decryption successful")
        }
        _ => Err(error::ApiError::DecryptingKeysFailed(
            "Both the custodain keys are not present",
        )),
    }
}
