use crate::error;

///
/// Type providing encoding functional to perform hashing
///
pub struct Sha512;

impl super::Encode<Vec<u8>, Vec<u8>> for Sha512 {
    type ReturnType<T> = Result<T, error::ContainerError<error::CryptoError>>;

    #[tracing::instrument(skip_all)]
    fn encode(&self, input: Vec<u8>) -> Self::ReturnType<Vec<u8>> {
        let digest = ring::digest::digest(&ring::digest::SHA512, &input);
        Ok(digest.as_ref().to_vec())
    }
}
