use crate::app::AppState;
use crate::crypto::jw::{decrypt_jwe, encrypt_jwe, jws_sign_payload, verify_sign};
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
use josekit::jwe;

pub async fn middleware(
    extract::State(state): extract::State<AppState>,
    request: Request<Body>,
    next: Next<Body>,
) -> Result<Response<BoxBody>, error::ApiError> {
    let (parts, body) = request.into_parts();

    let request_body = hyper::body::to_bytes(body)
        .await
        .change_context(error::ApiError::MiddlewareError(
            "Failed to read request body for jwe decryption",
        ))
        .report_unwrap()?;

    let jwt = String::from_utf8(request_body.to_vec())
        .change_context(error::ApiError::MiddlewareError(
            "Could not convert to UTF-8",
        ))
        .report_unwrap()?;

    let jwe_decrypted = decrypt_jwe(
        &jwt,
        &state.config.secrets.locker_private_key,
        jwe::RSA_OAEP_256,
    )
    .change_context(error::ApiError::MiddlewareError("Jwe decryption failed"))
    .report_unwrap()?;

    let jws_verified = verify_sign(jwe_decrypted, &state.config.secrets.tenant_public_key)
        .change_context(error::ApiError::MiddlewareError("Jws verification failed"))
        .report_unwrap()?;

    let next_layer_payload = Request::from_parts(parts, Body::from(jws_verified));

    let response = next.run(next_layer_payload).await;

    let body = response.into_body();

    let response_body = hyper::body::to_bytes(body)
        .await
        .change_context(error::ApiError::MiddlewareError(
            "Failed to read response body for jws signing",
        ))
        .report_unwrap()?;

    let jws_signed = jws_sign_payload(&response_body, &state.config.secrets.locker_private_key)
        .change_context(error::ApiError::MiddlewareError("Jws signing failed"))
        .report_unwrap()?;

    let jwe_encrypted = encrypt_jwe(
        jws_signed.as_bytes(),
        &state.config.secrets.tenant_public_key,
    )
    .change_context(error::ApiError::MiddlewareError("Jwe encryption failed"))
    .report_unwrap()?;

    Ok(Response::new(
        jwe_encrypted.map_err(axum::Error::new).boxed_unsync(),
    ))
}
