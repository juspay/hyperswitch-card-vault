//! Google Cloud Platform Key Management Service integration for secrets management

pub mod core;
pub mod implementers;

pub use self::core::GcpKmsClient;