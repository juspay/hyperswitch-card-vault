use hyperswitch_redis_interface::errors::RedisError;

use super::StorageError;

pub trait RedisErrorExt {
    #[track_caller]
    fn to_redis_failed_response(self, key: &str) -> error_stack::Report<StorageError>;
}

impl RedisErrorExt for error_stack::Report<RedisError> {
    fn to_redis_failed_response(self, key: &str) -> error_stack::Report<StorageError> {
        match self.current_context() {
            RedisError::NotFound => self.change_context(StorageError::ValueNotFound(format!(
                "Data does not exist for key {key}",
            ))),
            RedisError::SetNxFailed | RedisError::SetAddMembersFailed => {
                self.change_context(StorageError::DuplicateValue {
                    entity: "redis",
                    key: Some(key.to_string()),
                })
            }
            _ => self.change_context(StorageError::KVError),
        }
    }
}
