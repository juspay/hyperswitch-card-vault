use hyperswitch_redis_interface::errors::RedisError;

#[derive(Debug, thiserror::Error)]
pub enum KvError {
    #[error("DuplicateValue: {entity} already exists {key:?}")]
    DuplicateValue {
        entity: &'static str,
        key: Option<String>,
    },
    #[error("KV backend error")]
    Backend,
    #[error("ValueNotFound: {0}")]
    ValueNotFound(String),
    #[error("Serialization failure")]
    SerializationFailed,
}

pub trait RedisErrorExt {
    #[track_caller]
    fn to_redis_failed_response(self, key: &str) -> error_stack::Report<KvError>;
}

impl RedisErrorExt for error_stack::Report<RedisError> {
    fn to_redis_failed_response(self, key: &str) -> error_stack::Report<KvError> {
        match self.current_context() {
            RedisError::NotFound => self.change_context(KvError::ValueNotFound(format!(
                "Data does not exist for key {key}",
            ))),
            RedisError::SetNxFailed | RedisError::SetAddMembersFailed => {
                self.change_context(KvError::DuplicateValue {
                    entity: "redis",
                    key: Some(key.to_string()),
                })
            }
            _ => self.change_context(KvError::Backend),
        }
    }
}
