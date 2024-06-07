use axum::{body::Body, extract::Request};

/// Date-time utilities.
pub mod date_time {
    use time::{OffsetDateTime, PrimitiveDateTime};

    /// Create a new [`PrimitiveDateTime`] with the current date and time in UTC.
    pub fn now() -> PrimitiveDateTime {
        let utc_date_time = OffsetDateTime::now_utc();
        PrimitiveDateTime::new(utc_date_time.date(), utc_date_time.time())
    }
}

/// Record the tenant_id field in request's trace
pub fn record_tenant_id_from_header(request: &Request<Body>) -> tracing::Span {
    macro_rules! inner_trace {
        ($v:expr) => {
            tracing::debug_span!("request", method = %request.method(), uri = %request.uri(), version = ?request.version(), tenant_id = $v)
        };
    }

    match request
        .headers()
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
    {
        Some(value) => inner_trace!(value),
        None => inner_trace!(tracing::field::Empty),
    }
}
