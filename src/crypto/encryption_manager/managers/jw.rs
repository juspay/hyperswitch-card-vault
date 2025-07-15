use crate::{
    crypto::encryption_manager::encryption_interface::Encryption,
    error::{self, ContainerError},
};
use josekit::{jwe, jws};
use masking::PeekInterface;

pub struct JWEncryption {
    pub(crate) private_key: masking::Secret<String>,
    pub(crate) public_key: masking::Secret<String>,
    pub(crate) encryption_algo: jwe::alg::rsaes::RsaesJweAlgorithm,
    pub(crate) decryption_algo: jwe::alg::rsaes::RsaesJweAlgorithm,
}

impl JWEncryption {
    pub fn new(
        private_key: String,
        public_key: String,
        enc_algo: jwe::alg::rsaes::RsaesJweAlgorithm,
        dec_algo: jwe::alg::rsaes::RsaesJweAlgorithm,
    ) -> Self {
        Self {
            private_key: private_key.into(),
            public_key: public_key.into(),
            encryption_algo: enc_algo,
            decryption_algo: dec_algo,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JwsBody {
    pub header: String,
    pub payload: String,
    pub signature: String,
}

impl JwsBody {
    fn from_dotted_str(input: &str) -> Option<Self> {
        let mut data = input.split('.');
        let header = data.next()?.to_string();
        let payload = data.next()?.to_string();
        let signature = data.next()?.to_string();
        Some(Self {
            header,
            payload,
            signature,
        })
    }

    pub fn get_dotted_jws(self) -> String {
        let header = self.header;
        let payload = self.payload;
        let signature = self.signature;
        format!("{header}.{payload}.{signature}")
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JweBody {
    pub header: String,
    pub iv: String,
    pub encrypted_payload: String,
    pub tag: String,
    pub encrypted_key: String,
}

impl JweBody {
    fn from_str(input: &str) -> Option<Self> {
        let mut data = input.split('.');
        let header = data.next()?.to_string();
        let encrypted_key = data.next()?.to_string();
        let iv = data.next()?.to_string();
        let encrypted_payload = data.next()?.to_string();
        let tag = data.next()?.to_string();
        Some(Self {
            header,
            iv,
            encrypted_payload,
            tag,
            encrypted_key,
        })
    }

    pub fn get_dotted_jwe(self) -> String {
        let header = self.header;
        let encryption_key = self.encrypted_key;
        let iv = self.iv;
        let encryption_payload = self.encrypted_payload;
        let tag = self.tag;
        format!("{header}.{encryption_key}.{iv}.{encryption_payload}.{tag}")
    }
}

impl Encryption<Vec<u8>, JweBody> for JWEncryption {
    type ReturnType<'a, T> = Result<T, ContainerError<error::CryptoError>>;

    fn encrypt(&self, input: Vec<u8>) -> Self::ReturnType<'_, JweBody> {
        let payload = input;
        let jws_encoded = jws_sign_payload(&payload, self.private_key.peek().as_bytes())?;
        let jws_body = JwsBody::from_dotted_str(&jws_encoded).ok_or(
            error::CryptoError::InvalidData("JWS encoded data is incomplete"),
        )?;
        let jws_payload = serde_json::to_vec(&jws_body).map_err(error::CryptoError::from)?;
        let jwe_encrypted = encrypt_jwe(
            &jws_payload,
            self.public_key.peek().as_bytes(),
            self.encryption_algo,
        )?;
        let jwe_body = JweBody::from_str(&jwe_encrypted)
            .ok_or(error::CryptoError::InvalidData("JWE data incomplete"))?;
        Ok(jwe_body)
    }

    fn decrypt(&self, input: JweBody) -> Self::ReturnType<'_, Vec<u8>> {
        let jwe_encoded = input.get_dotted_jwe();
        let jwe_decrypted =
            decrypt_jwe(&jwe_encoded, self.private_key.peek(), self.decryption_algo)?;

        let jws_parsed: JwsBody = serde_json::from_str(&jwe_decrypted)
            .map_err(|_| error::CryptoError::InvalidData("Failed while extracting jws body"))?;

        let jws_encoded = jws_parsed.get_dotted_jws();
        let output = verify_sign(jws_encoded, self.public_key.peek().as_bytes())?;
        Ok(output.as_bytes().to_vec())
    }
}

pub fn jws_sign_payload(
    payload: &[u8],
    private_key: impl AsRef<[u8]>,
) -> Result<String, error::CryptoError> {
    let alg = jws::RS256;
    let src_header = jws::JwsHeader::new();
    let signer = alg.signer_from_pem(private_key)?;
    Ok(jws::serialize_compact(payload, &src_header, &signer)?)
}

pub fn encrypt_jwe(
    payload: &[u8],
    public_key: impl AsRef<[u8]>,
    alg: jwe::alg::rsaes::RsaesJweAlgorithm,
) -> Result<String, error::CryptoError> {
    let enc = "A256GCM";
    let mut src_header = jwe::JweHeader::new();
    src_header.set_content_encryption(enc);
    src_header.set_token_type("JWT");
    let encrypter = alg.encrypter_from_pem(public_key)?;

    Ok(jwe::serialize_compact(payload, &src_header, &encrypter)?)
}

pub fn decrypt_jwe(
    jwt: &str,
    private_key: impl AsRef<[u8]>,
    alg: jwe::alg::rsaes::RsaesJweAlgorithm,
) -> Result<String, error::CryptoError> {
    let decrypter = alg.decrypter_from_pem(private_key)?;

    let (dst_payload, _dst_header) = jwe::deserialize_compact(jwt, &decrypter)?;

    Ok(String::from_utf8(dst_payload)?)
}

pub fn verify_sign(jws_body: String, key: impl AsRef<[u8]>) -> Result<String, error::CryptoError> {
    let alg = jws::RS256;
    let input = jws_body.as_bytes();
    let verifier = alg.verifier_from_pem(key)?;
    let (dst_payload, _dst_header) = jws::deserialize_compact(input, &verifier)?;
    let resp = String::from_utf8(dst_payload)?;
    Ok(resp)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    // Keys used for tests
    // Can be generated using the following commands:
    // `openssl genrsa -out private_self.pem 2048`
    // `openssl rsa -in private_key.pem -pubout -out public_self.pem`
    const ENCRYPTION_KEY: &str = "";
    const DECRYPTION_KEY: &str = "";

    const SIGNATURE_VERIFICATION_KEY: &str = "";
    const SIGNING_KEY: &str = "";

    #[test]
    fn test_jwe() {
        let jwt = encrypt_jwe("request_payload".as_bytes(), ENCRYPTION_KEY, jwe::RSA_OAEP).unwrap();
        let alg = jwe::RSA_OAEP;
        let payload = decrypt_jwe(&jwt, DECRYPTION_KEY, alg).unwrap();
        assert_eq!("request_payload".to_string(), payload)
    }

    #[test]
    fn test_jws() {
        let jwt = jws_sign_payload("jws payload".as_bytes(), SIGNING_KEY).unwrap();
        let payload = verify_sign(jwt, SIGNATURE_VERIFICATION_KEY).unwrap();
        assert_eq!("jws payload".to_string(), payload)
    }
}
