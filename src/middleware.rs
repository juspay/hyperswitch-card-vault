use axum::{
    body::Body,
    http::{Request, request, response},
    middleware::Next,
};
use http_body_util::BodyExt;
use josekit::jwe;

use crate::{
    crypto::encryption_manager::{
        encryption_interface::Encryption,
        managers::jw::{self, JWEncryption},
    },
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError, ResultContainerExt},
};

#[cfg(feature = "middleware")]
async fn record_jwe_middleware_operation<Fut, T, E>(
    future: Fut,
    operation: &'static str,
) -> Result<T, E>
where
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let start = std::time::Instant::now();
    let result = future.await;
    let duration = start.elapsed();
    let outcome = if result.is_ok() { "success" } else { "error" };

    crate::observability::metrics::HTTP_SERVER_JWE_MIDDLEWARE_OPERATION_DURATION.record(
        duration.as_secs_f64(),
        crate::metric_attributes!(("operation", operation), ("outcome", outcome)),
    );

    result
}

/// Middleware providing implementation to perform JWE + JWS encryption and decryption around the
/// card APIs
pub async fn middleware(
    TenantStateResolver(state): TenantStateResolver,
    parts: request::Parts,
    axum::Json(jwe_body): axum::Json<jw::JweBody>,
    next: Next,
) -> Result<(response::Parts, axum::Json<jw::JweBody>), ContainerError<error::ApiError>> {
    let keys = JWEncryption {
        private_key: state.config.locker_secrets.locker_private_key.clone(),
        public_key: state.config.tenant_secrets.public_key.clone(),
        encryption_algo: jwe::RSA_OAEP,
        decryption_algo: jwe::RSA_OAEP_256,
    };

    let jwe_decrypted =
        record_jwe_middleware_operation(async { keys.decrypt(jwe_body) }, "request_decrypt")
            .await?;

    let next_layer_payload = Request::from_parts(parts, Body::from(jwe_decrypted));

    let (mut parts, body) = next.run(next_layer_payload).await.into_parts();

    let response_body = record_jwe_middleware_operation(body.collect(), "response_body_collect")
        .await
        .change_error(error::ApiError::ResponseMiddlewareError(
            "Failed to read response body for jws signing",
        ))?
        .to_bytes();

    let jwe_payload = record_jwe_middleware_operation(
        async { keys.encrypt(response_body.to_vec()) },
        "response_encrypt",
    )
    .await?;

    parts.headers = hyper::HeaderMap::new();
    parts.headers.append(
        hyper::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );

    Ok((parts, axum::Json(jwe_payload)))
}
