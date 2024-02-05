pub struct NoEncryption;

impl<I: super::FromEncoded> super::Decode<I, String> for NoEncryption {
    type ReturnType<'b, T> = Option<T>;
    fn decode(&self, input: String) -> Self::ReturnType<'_, I> {
        I::from_encoded(input)
    }
}
