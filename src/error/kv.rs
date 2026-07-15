use hyperswitch_redis_interface::errors::RedisError;

/// KV-layer errors.
///
/// `SerializationFailed` covers both directions (serialize/deserialize) and both sides
/// (host-side drainer query building + Redis payload (de)serialization). `Backend` means
/// transport/driver-level failures only.
#[derive(Debug, thiserror::Error)]
pub enum KvError {
    #[error("Duplicate value already exists for key `{key}`")]
    DuplicateValue { key: String },
    #[error("KV backend error")]
    Backend,
    #[error("Value not found: {0}")]
    ValueNotFound(String),
    #[error("(De)serialization failed")]
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
                    key: key.to_string(),
                })
            }
            RedisError::JsonSerializationFailed | RedisError::JsonDeserializationFailed => {
                self.change_context(KvError::SerializationFailed)
            }
            RedisError::InvalidConfiguration(_)
            | RedisError::SetFailed
            | RedisError::SetExFailed
            | RedisError::SetExpiryFailed
            | RedisError::GetFailed
            | RedisError::DeleteFailed
            | RedisError::StreamAppendFailed
            | RedisError::StreamReadFailed
            | RedisError::GetLengthFailed
            | RedisError::StreamDeleteFailed
            | RedisError::StreamTrimFailed
            | RedisError::StreamAcknowledgeFailed
            | RedisError::StreamEmptyOrNotAvailable
            | RedisError::ConsumerGroupCreateFailed
            | RedisError::ConsumerGroupDestroyFailed
            | RedisError::ConsumerGroupRemoveConsumerFailed
            | RedisError::ConsumerGroupSetIdFailed
            | RedisError::ConsumerGroupClaimFailed
            | RedisError::SetHashFailed
            | RedisError::SetHashFieldFailed
            | RedisError::DeleteHashFieldFailed
            | RedisError::GetHashFieldFailed
            | RedisError::InvalidRedisEntryId
            | RedisError::RedisConnectionError
            | RedisError::SubscribeError
            | RedisError::PublishError
            | RedisError::OnMessageError
            | RedisError::UnknownResult
            | RedisError::AppendElementsToListFailed
            | RedisError::GetListElementsFailed
            | RedisError::GetListLengthFailed
            | RedisError::PopListElementsFailed
            | RedisError::IncrementHashFieldFailed
            | RedisError::ScriptExecutionFailed => self.change_context(KvError::Backend),
        }
    }
}
