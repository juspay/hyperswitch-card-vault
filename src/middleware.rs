use crate::app::AppState;
use crate::crypto::jw::JWEncryption;
use crate::crypto::Encryption;
use crate::error::{self, LogReport};
use axum::{
    body::BoxBody,
    extract,
    http::{Request, Response},
    middleware::Next,
};
use error_stack::ResultExt;
use hyper::body::HttpBody;
use hyper::Body;

/// Middleware providing implementation to perform JWE + JWS encryption and decryption around the
/// card APIs
pub async fn middleware(
    extract::State(state): extract::State<AppState>,
    request: Request<Body>,
    next: Next<Body>,
) -> Result<Response<BoxBody>, error::ApiError> {
    let (parts, body) = request.into_parts();

    let request_body = hyper::body::to_bytes(body)
        .await
        .change_context(error::ApiError::RequestMiddlewareError(
            "Failed to read request body for jwe decryption",
        ))
        .report_unwrap()?;

    let keys = JWEncryption {
        private_key: state.config.secrets.locker_private_key,
        public_key: state.config.secrets.tenant_public_key,
    };

    let jwe_decrypted = keys
        .decrypt(request_body.to_vec())
        .change_context(error::ApiError::RequestMiddlewareError(
            "Jwe decryption failed",
        ))
        .report_unwrap()?;

    let next_layer_payload = Request::from_parts(parts, Body::from(jwe_decrypted));

    let response = next.run(next_layer_payload).await;

    let body = response.into_body();

    let response_body = hyper::body::to_bytes(body)
        .await
        .change_context(error::ApiError::ResponseMiddlewareError(
            "Failed to read response body for jws signing",
        ))
        .report_unwrap()?;

    let jws_signed = keys
        .encrypt(response_body.to_vec())
        .change_context(error::ApiError::ResponseMiddlewareError(
            "Jws signing failed",
        ))
        .report_unwrap()?;

    let jwt = String::from_utf8(jws_signed)
        .change_context(error::ApiError::ResponseMiddlewareError(
            "Could not convert to UTF-8",
        ))
        .report_unwrap()?;

    Ok(Response::new(jwt.map_err(axum::Error::new).boxed_unsync()))
}
