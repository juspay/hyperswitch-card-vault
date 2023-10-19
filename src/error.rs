use std::string::FromUtf8Error;

#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("error while creating the webserver")]
    ServerError(#[from] hyper::Error),
    #[error("invalid host for socket")]
    AddressError(#[from] std::net::AddrParseError),
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
    EncodingError(#[from] FromUtf8Error)
}
