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
#[cfg(feature = "kms-aws")]
pub mod aws_kms;
#[cfg(feature = "kms-hashicorp-vault")]
pub mod hcvault;
pub mod hollow;
pub mod jw;
pub mod multiple;

#[cfg(feature = "kms-aws")]
pub mod consts {
    /// General purpose base64 engine
    pub(crate) const BASE64_ENGINE: base64::engine::GeneralPurpose =
        base64::engine::general_purpose::STANDARD;
}
pub mod sha;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use masking::ExposeInterface;

    use super::*;
    #[test]
    fn test_from_encoded_string() {
        let value = "123";
        assert_eq!(
            value,
            masking::Secret::<String>::from_encoded(value.to_string())
                .unwrap()
                .expose()
        );
    }

    #[test]
    fn test_from_encoded_bytes() {
        let value = "ff";
        assert_eq!(
            vec![255],
            Vec::<u8>::from_encoded(value.to_string()).unwrap()
        );
    }
}
