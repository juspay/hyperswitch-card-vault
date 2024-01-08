use crate::app::AppState;
use crate::crypto::jw::{self, JWEncryption};
use crate::crypto::Encryption;
use crate::error::{self, ContainerError, ResultContainerExt};
use axum::body::Body;
use axum::http::{request, response};
use axum::{extract, http::Request, middleware::Next};

use http_body_util::BodyExt;
use josekit::jwe;

/// Middleware providing implementation to perform JWE + JWS encryption and decryption around the
/// card APIs
pub async fn middleware(
    extract::State(state): extract::State<AppState>,
    parts: request::Parts,
    axum::Json(jwe_body): axum::Json<jw::JweBody>,
    next: Next,
) -> Result<(response::Parts, axum::Json<jw::JweBody>), ContainerError<error::ApiError>> {
    let keys = JWEncryption {
        private_key: state.config.secrets.locker_private_key,
        public_key: state.config.secrets.tenant_public_key,
        encryption_algo: jwe::RSA_OAEP,
        decryption_algo: jwe::RSA_OAEP_256,
    };

    let jwe_decrypted = keys.decrypt(jwe_body)?;

    let next_layer_payload = Request::from_parts(parts, Body::from(jwe_decrypted));

    let (mut parts, body) = next.run(next_layer_payload).await.into_parts();

    let response_body = body
        .collect()
        .await
        .change_error(error::ApiError::ResponseMiddlewareError(
            "Failed to read response body for jws signing",
        ))?
        .to_bytes();

    let jwe_payload = keys.encrypt(response_body.to_vec())?;

    parts.headers = hyper::HeaderMap::new();
    parts.headers.append(
        hyper::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );

    Ok((parts, axum::Json(jwe_payload)))
}
