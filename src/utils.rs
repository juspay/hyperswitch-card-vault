use axum::{body::Body, extract::Request};

use crate::storage::consts;

/// Date-time utilities.
pub mod date_time {
    use time::{OffsetDateTime, PrimitiveDateTime};

    /// Create a new [`PrimitiveDateTime`] with the current date and time in UTC.
    pub fn now() -> PrimitiveDateTime {
        let utc_date_time = OffsetDateTime::now_utc();
        PrimitiveDateTime::new(utc_date_time.date(), utc_date_time.time())
    }

    /// Serialize a [`PrimitiveDateTime`] (assumed to be UTC) as an ISO 8601 / RFC 3339 string
    /// with millisecond precision and a `Z` offset, e.g. `2026-06-24T19:27:37.552Z` — matching
    /// the timestamp format used across the Hyperswitch APIs. Intended for use with
    /// `#[serde(serialize_with = "crate::utils::date_time::serialize")]`.
    pub fn serialize<S>(date_time: &PrimitiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let format = time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        let formatted = date_time
            .assume_utc()
            .format(&format)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&formatted)
    }
}

/// Record the header's fields in request's trace
pub fn record_fields_from_header(request: &Request<Body>) -> tracing::Span {
    let span = tracing::debug_span!(
        "request",
        method = %request.method(),
        uri = %request.uri(),
        version = ?request.version(),
        tenant_id = tracing::field::Empty,
        request_id = tracing::field::Empty,
    );
    request
        .headers()
        .get(consts::X_TENANT_ID)
        .and_then(|value| value.to_str().ok())
        .map(|tenant_id| span.record("tenant_id", tenant_id));

    request
        .headers()
        .get(consts::X_REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .map(|request_id| span.record("request_id", request_id));

    span
}
