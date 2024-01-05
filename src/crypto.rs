///
/// Encryption
///
/// A trait to be used internally for maintaining and managing encryption algorithms
///
pub trait Encryption<I, O> {
    type ReturnType<'b, T>
    where
        Self: 'b;
    fn encrypt(&self, input: I) -> Self::ReturnType<'_, O>;
    fn decrypt(&self, input: O) -> Self::ReturnType<'_, I>;
}

pub trait Encode<I, O> {
    type ReturnType<T>;
    fn encode(&self, input: I) -> Self::ReturnType<O>;
}

pub trait Decode<I, O> {
    type ReturnType<'b, T>
    where
        Self: 'b;
    fn decode(&self, input: O) -> Self::ReturnType<'_, I>;
}

pub trait FromEncoded: Sized {
    fn from_encoded(input: String) -> Option<Self>;
}

impl FromEncoded for masking::Secret<String> {
    fn from_encoded(input: String) -> Option<Self> {
        Some(input.into())
    }
}

impl FromEncoded for Vec<u8> {
    fn from_encoded(input: String) -> Option<Self> {
        hex::decode(input).ok()
    }
}

pub mod aes;
#[cfg(feature = "hashicorp-vault")]
pub mod hcvault;
pub mod hollow;
pub mod jw;
#[cfg(feature = "kms")]
pub mod kms;
pub mod multiple;

#[cfg(feature = "kms")]
pub mod consts {
    /// General purpose base64 engine
    pub(crate) const BASE64_ENGINE: base64::engine::GeneralPurpose =
        base64::engine::general_purpose::STANDARD;
}
pub mod sha;
