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
}

/// Serde helpers for `time::PrimitiveDateTime`.
pub mod primitive_datetime_serde {
    pub mod iso8601 {
        use std::num::NonZeroU8;

        use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::Error as _};
        use time::{
            OffsetDateTime, PrimitiveDateTime, UtcOffset,
            format_description::well_known::{
                Iso8601,
                iso8601::{Config, EncodedConfig, TimePrecision},
            },
        };

        const FORMAT_CONFIG: EncodedConfig = Config::DEFAULT
            .set_time_precision(TimePrecision::Second {
                decimal_digits: NonZeroU8::new(9),
            })
            .encode();

        pub fn serialize<S>(date_time: &PrimitiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            date_time
                .assume_utc()
                .format(&Iso8601::<FORMAT_CONFIG>)
                .map_err(S::Error::custom)?
                .serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<PrimitiveDateTime, D::Error>
        where
            D: Deserializer<'de>,
        {
            let date_time = String::deserialize(deserializer)?;
            OffsetDateTime::parse(&date_time, &Iso8601::<FORMAT_CONFIG>)
                .map(|offset_date_time| {
                    let utc_date_time = offset_date_time.to_offset(UtcOffset::UTC);
                    PrimitiveDateTime::new(utc_date_time.date(), utc_date_time.time())
                })
                .map_err(serde::de::Error::custom)
        }

        pub mod option {
            use serde::Serialize;

            use super::*;

            pub fn serialize<S>(
                date_time: &Option<PrimitiveDateTime>,
                serializer: S,
            ) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                date_time
                    .map(|date_time| date_time.assume_utc().format(&Iso8601::<FORMAT_CONFIG>))
                    .transpose()
                    .map_err(S::Error::custom)?
                    .serialize(serializer)
            }

            pub fn deserialize<'de, D>(
                deserializer: D,
            ) -> Result<Option<PrimitiveDateTime>, D::Error>
            where
                D: Deserializer<'de>,
            {
                Option::<String>::deserialize(deserializer)?
                    .map(|date_time| {
                        OffsetDateTime::parse(&date_time, &Iso8601::<FORMAT_CONFIG>).map(
                            |offset_date_time| {
                                let utc_date_time = offset_date_time.to_offset(UtcOffset::UTC);
                                PrimitiveDateTime::new(utc_date_time.date(), utc_date_time.time())
                            },
                        )
                    })
                    .transpose()
                    .map_err(serde::de::Error::custom)
            }
        }
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
