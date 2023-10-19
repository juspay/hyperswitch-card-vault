use axum::{response::IntoResponse, body::BoxBody};

pub trait Encryption<I, O: serde::Serialize + serde::de::DeserializeOwned> {
    type Key;
    type Error;
    fn encrypt(input: I, key: Self::Key) -> Result<O, Self::Error>;
    fn decrypt(input: O, key: Self::Key) -> Result<I, Self::Error>;
}

pub mod jw;

