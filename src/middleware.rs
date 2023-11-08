use crate::app::AppState;
use crate::crypto::jw::JWEncryption;
use crate::crypto::Encryption;
use crate::error::{self, ContainerError, ResultContainerExt};
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
) -> Result<Response<BoxBody>, ContainerError<error::ApiError>> {
    let (parts, body) = request.into_parts();

    let request_body =
        hyper::body::to_bytes(body)
            .await
            .change_error(error::ApiError::RequestMiddlewareError(
                "Failed to read request body for jwe decryption",
            ))?;

    let keys = JWEncryption {
        private_key: state.config.secrets.locker_private_key,
        public_key: state.config.secrets.tenant_public_key,
    };

    let jwe_decrypted = keys.decrypt(request_body.to_vec())?;

    let next_layer_payload = Request::from_parts(parts, Body::from(jwe_decrypted));

    let response = next.run(next_layer_payload).await;

    let (parts, body) = response.into_parts();

    let response_body = hyper::body::to_bytes(body).await.change_error(
        error::ApiError::ResponseMiddlewareError("Failed to read response body for jws signing"),
    )?;

    let jws_signed = keys.encrypt(response_body.to_vec())?;

    let jwt = String::from_utf8(jws_signed).change_error(
        error::ApiError::ResponseMiddlewareError("Could not convert to UTF-8"),
    )?;

    Ok(axum::http::response::Builder::new()
        .status(parts.status)
        .body(jwt.map_err(axum::Error::new).boxed_unsync())
        .change_context(error::ApiError::ResponseMiddlewareError(
            "failed while generating the response",
        ))?)
}
