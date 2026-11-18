use axum::{
    body::Body,
    http::{Request, request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;

use crate::{
    crypto::encryption_manager::{encryption_interface::Encryption, managers::jw},
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError, ResultContainerExt},
    storage::consts,
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
        metrics_utils::metric_attributes!(("operation", operation), ("outcome", outcome)),
    );

    result
}

/// Whether the caller asked for a signed-only response on the fingerprint route.
fn wants_jws_response(parts: &request::Parts) -> bool {
    parts.uri.path() == consts::V2_FINGERPRINT_PATH
        && parts
            .headers
            .get(consts::X_FP_RESPONSE_ENCODING)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == consts::FP_RESPONSE_ENCODING_JWS)
}

/// Middleware providing implementation to perform JWE + JWS encryption and decryption around the
/// card APIs
pub async fn middleware(
    TenantStateResolver(state): TenantStateResolver,
    parts: request::Parts,
    axum::Json(jwe_body): axum::Json<jw::JweBody>,
    next: Next,
) -> Result<Response, ContainerError<error::ApiError>> {
    let keys = &state.jwe_keys;
    let jws_response = wants_jws_response(&parts);

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

    parts.headers = hyper::HeaderMap::new();
    parts.headers.append(
        hyper::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );

    let response = if jws_response {
        let jws_payload =
            record_jwe_middleware_operation(async { keys.sign(&response_body) }, "response_sign")
                .await?;

        // Echoed so the caller can distinguish a JWS body from a JWE envelope.
        parts.headers.append(
            hyper::header::HeaderName::from_static(consts::X_FP_RESPONSE_ENCODING),
            axum::http::HeaderValue::from_static(consts::FP_RESPONSE_ENCODING_JWS),
        );

        (parts, axum::Json(jws_payload)).into_response()
    } else {
        let jwe_payload = record_jwe_middleware_operation(
            async { keys.encrypt(response_body.to_vec()) },
            "response_encrypt",
        )
        .await?;

        (parts, axum::Json(jwe_payload)).into_response()
    };

    Ok(response)
}
