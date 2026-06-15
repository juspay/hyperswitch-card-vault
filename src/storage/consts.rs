/// Characters to use for generating NanoID
pub(crate) const ALPHABETS: [char; 62] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i',
    'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B',
    'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U',
    'V', 'W', 'X', 'Y', 'Z',
];

/// Number of characters in a generated ID
pub const ID_LENGTH: usize = 20;

/// Header key for tenant ID
pub const X_TENANT_ID: &str = "x-tenant-id";
/// Header key for request ID
pub const X_REQUEST_ID: &str = "x-request-id";
/// Header key for caller-supplied fingerprint ID (optional)
pub const X_FINGERPRINT_ID: &str = "x-fingerprint-id";
/// Key written by the Redis health-check probe
#[cfg(feature = "redis")]
pub const REDIS_HEALTH_CHECK_KEY: &str = "health_check_redis";
/// Value written by the Redis health-check probe
#[cfg(feature = "redis")]
pub const REDIS_HEALTH_CHECK_VALUE: &str = "1";

/// Header Constants
pub mod headers {
    pub const CONTENT_TYPE: &str = "Content-Type";
    pub const AUTHORIZATION: &str = "Authorization";
}
