use crate::error;
use josekit::{jwe, jws};

pub struct JWEncryption;

pub struct LockerKey {
    private_key: String,
    public_key: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JwsBody {
    pub header: String,
    pub payload: String,
    pub signature: String,
}

impl JwsBody {
    fn from_str(input: &str) -> Option<Self> {
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

impl super::Encryption<Vec<u8>, Vec<u8>> for JWEncryption {
    type Key = LockerKey;

    type Error = error::CryptoError;

    fn encrypt(input: Vec<u8>, key: Self::Key) -> Result<Vec<u8>, Self::Error> {
        let payload = input;
        let jws_encoded = jws_sign_payload(&payload, key.private_key.as_bytes())?;
        let jws_body = JwsBody::from_str(&jws_encoded).ok_or(error::CryptoError::InvalidData(
            "JWS encoded data is incomplete",
        ))?;
        let jws_payload = serde_json::to_vec(&jws_body)?;
        let jwe_encrypted = encrypt_jwe(&jws_payload, key.public_key)?;
        let jwe_body = JweBody::from_str(&jwe_encrypted)
            .ok_or(error::CryptoError::InvalidData("JWE data incomplete"))?;
        Ok(serde_json::to_vec(&jwe_body)?)
    }

    fn decrypt(input: Vec<u8>, key: Self::Key) -> Result<Vec<u8>, Self::Error> {
        let jwe_body: JweBody = serde_json::from_slice(&input)?;
        let jwe_encoded = jwe_body.get_dotted_jwe();
        let algo = jwe::RSA_OAEP;
        let jwe_decrypted = decrypt_jwe(&jwe_encoded, key.private_key, algo)?;
        let jws_parsed = JwsBody::from_str(&jwe_decrypted).ok_or(
            error::CryptoError::InvalidData("Failed while extracting jws body"),
        )?;
        let jws_encoded = jws_parsed.get_dotted_jws();
        let output = verify_sign(jws_encoded, key.public_key)?;
        Ok(output.as_bytes().to_vec())
    }
}

fn jws_sign_payload(
    payload: &[u8],
    private_key: impl AsRef<[u8]>,
) -> Result<String, error::CryptoError> {
    let alg = jws::RS256;
    let src_header = jws::JwsHeader::new();
    let signer = alg.signer_from_pem(private_key)?;
    Ok(jws::serialize_compact(payload, &src_header, &signer)?)
}

fn encrypt_jwe(payload: &[u8], public_key: impl AsRef<[u8]>) -> Result<String, error::CryptoError> {
    let alg = jwe::RSA_OAEP_256;
    let enc = "A256GCM";
    let mut src_header = jwe::JweHeader::new();
    src_header.set_content_type(enc);
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
