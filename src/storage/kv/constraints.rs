use std::sync::Arc;

use hyperswitch_redis_interface::{
    RedisConnectionPool,
    errors::RedisError,
    types::SaddReply,
};

/// Trait for types that have unique constraints which must be enforced in Redis
/// (via SADD) when writing through the KV path.
///
/// Vendored from `storage_impl/src/lib.rs`.
/// Per-table impls are added when a table is integrated into KV.
#[allow(async_fn_in_trait)]
pub trait UniqueConstraints {
    fn unique_constraints(&self) -> Vec<String>;
    fn table_name(&self) -> &str;

    async fn check_for_constraints(
        &self,
        redis_conn: &Arc<RedisConnectionPool>,
    ) -> Result<(), error_stack_04::Report<RedisError>> {
        let constraints = self.unique_constraints();
        let sadd_result = redis_conn
            .sadd(
                &format!("unique_constraint:{}", self.table_name()).into(),
                constraints,
            )
            .await?;

        match sadd_result {
            SaddReply::KeyNotSet => {
                Err(error_stack_04::Report::new(RedisError::SetAddMembersFailed))
            }
            SaddReply::KeySet => Ok(()),
        }
    }
}
