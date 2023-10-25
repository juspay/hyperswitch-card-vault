///
/// Encryption
///
/// A trait to be used internally for maintaining and managing encryption algorithms
///
pub trait Encryption<I, O> {
    type ReturnType<T>;
    fn encrypt(&self, input: I) -> Self::ReturnType<O>;
    fn decrypt(&self, input: O) -> Self::ReturnType<I>;
}

pub mod aes;
pub mod jw;
