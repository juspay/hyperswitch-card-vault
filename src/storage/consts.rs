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
/// Header key a caller uses to request a plain (unencrypted) response; echoed back when honoured
pub const X_RESPONSE_ENCODING: &str = "x-response-encoding";
/// `x-response-encoding` value requesting a plain JSON response
pub const RESPONSE_ENCODING_PLAIN: &str = "plain";
/// Path suffix shared by `/cards/fingerprint` and `/api/v2/vault/fingerprint`
pub const FINGERPRINT_PATH_SUFFIX: &str = "/fingerprint";
/// Key written by the Redis health-check probe
#[cfg(feature = "redis")]
pub const REDIS_HEALTH_CHECK_KEY: &str = "health_check_redis";
/// Value written by the Redis health-check probe
#[cfg(feature = "redis")]
pub const REDIS_HEALTH_CHECK_VALUE: &str = "1";
/// TTL (seconds) on the probe key so Redis drops it even if the delete is skipped
#[cfg(feature = "redis")]
pub const REDIS_HEALTH_CHECK_EXPIRY: i64 = 5;

/// Default maximum lifetime (seconds) of a pooled DB connection
pub const DEFAULT_DB_POOL_MAX_LIFETIME_SECS: u64 = 120;
/// Default minimum number of idle connections maintained in the DB pool
pub const DEFAULT_DB_POOL_MIN_IDLE: u32 = 2;
/// Default idle timeout (seconds) for a pooled DB connection
pub const DEFAULT_DB_POOL_IDLE_TIMEOUT_SECS: u64 = 300;
/// Default timeout (seconds) for acquiring a connection from the DB pool
pub const DEFAULT_DB_POOL_CONNECTION_TIMEOUT_SECS: u64 = 10;

/// Header Constants
pub mod headers {
    pub const CONTENT_TYPE: &str = "Content-Type";
    pub const AUTHORIZATION: &str = "Authorization";
}
