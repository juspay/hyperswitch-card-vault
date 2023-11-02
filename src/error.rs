use std::string::FromUtf8Error;

use error_stack::Report;

#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("error while creating the webserver")]
    ServerError(#[from] hyper::Error),
    #[error("invalid host for socket")]
    AddressError(#[from] std::net::AddrParseError),
    #[error("Error while connecting/creating database pool")]
    DatabaseError,
    #[error("Failed to KMS decrypt: {0}")]
    KmsDecryptError(&'static str),
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("failed while serializing with serde_json")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("error while performing jwe operation")]
    JWError(#[from] josekit::JoseError),
    #[error("invalid data received: {0}")]
    InvalidData(&'static str),
    #[error("error while parsing utf-8")]
    EncodingError(#[from] FromUtf8Error),
    #[error("Failed while encrypting payload")]
    EncryptionError,
    #[error("Failed while decrypting payload")]
    DecryptionError,
    #[error("Not implemented")]
    NotImplemented,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("failed to construct database pool")]
    DBPoolError,
    #[error("failed to construct database pool")]
    PoolClientFailure,
    #[error("Error while finding element in database")]
    FindError,
    #[error("Error while decrypting the payload")]
    DecryptionError,
    #[error("Error while encrypting the payload")]
    EncryptionError,
}

#[derive(Debug, Copy, Clone, thiserror::Error)]
pub enum ApiError {
    #[error("failed while making merchant create")]
    TenentCreateError,
    #[error("failed while calling store data")]
    StoreDataFailed(&'static str),
    #[error("failed while retrieving stored data")]
    RetrieveDataFailed(&'static str),
    #[error("failed to decrypt two custodian keys: {0}")]
    DecryptingKeysFailed(&'static str),

    #[error("failed in request middleware: {0}")]
    RequestMiddlewareError(&'static str),

    #[error("failed in response middleware: {0}")]
    ResponseMiddlewareError(&'static str),

    #[error("Error while encoding data")]
    EncodingError,

    #[error("Failed while decoding data")]
    DecodingError,

    #[error("Failed while retrieving data from \"{0}\"")]
    DatabaseRetrieveFailed(&'static str),

    #[error("Failed while inserting data into \"{0}\"")]
    DatabaseInsertFailed(&'static str),

    #[error("failed while deleting data from {0}")]
    DatabaseDeleteFailed(&'static str),
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::TenentCreateError => (
                hyper::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(ApiErrorResponse::new(
                    "TE_00",
                    "Failed while creating the tenant".to_string(),
                    None,
                )),
            )
                .into_response(),
            Self::DecryptingKeysFailed(err) => (
                hyper::StatusCode::BAD_REQUEST,
                axum::Json(ApiErrorResponse::new(
                    "TE_00",
                    format!("Failed while decrypting two custodian keys: {err}"),
                    None,
                )),
            )
                .into_response(),
            data @ Self::StoreDataFailed(_)
            | data @ Self::RetrieveDataFailed(_)
            | data @ Self::EncodingError
            | data @ Self::ResponseMiddlewareError(_)
            | data @ Self::DatabaseRetrieveFailed(_)
            | data @ Self::DatabaseInsertFailed(_)
            | data @ Self::DatabaseDeleteFailed(_) => (
                hyper::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(ApiErrorResponse::new("TE_01", format!("{}", data), None)),
            )
                .into_response(),
            data @ Self::RequestMiddlewareError(_) | data @ Self::DecodingError => (
                hyper::StatusCode::BAD_REQUEST,
                axum::Json(ApiErrorResponse::new("TE_02", format!("{}", data), None)),
            )
                .into_response(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiErrorResponse {
    code: &'static str,
    message: String,
    data: Option<serde_json::Value>,
}

impl ApiErrorResponse {
    fn new(code: &'static str, message: String, data: Option<serde_json::Value>) -> Self {
        Self {
            code,
            message,
            data,
        }
    }
}

pub trait LogReport<T, E> {
    fn report_unwrap(self) -> Result<T, E>;
}

impl<T, E1, E2> LogReport<T, E1> for Result<T, Report<E2>>
where
    E1: Send + Sync + std::error::Error + Copy + 'static,
    E2: Send + Sync + std::error::Error + Copy + 'static,
    E1: From<E2>,
{
    #[track_caller]
    fn report_unwrap(self) -> Result<T, E1> {
        let output = match self {
            Ok(inner_val) => Ok(inner_val),
            Err(inner_err) => {
                let new_error: E1 = (*inner_err.current_context()).into();
                crate::logger::error!(?inner_err);
                Err(inner_err.change_context(new_error))
            }
        };

        output.map_err(|err| (*err.current_context()))
    }
}
