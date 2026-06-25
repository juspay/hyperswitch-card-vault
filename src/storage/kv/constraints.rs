use std::sync::Arc;

use hyperswitch_redis_interface::{
    RedisConnectionPool,
    errors::RedisError,
    types::SaddReply,
};

use crate::storage::scheme::StorageScheme;

/// Trait for types that have unique constraints which must be enforced in Redis
/// (via SADD) when writing through the KV path.
///
/// Vendored from `storage_impl/src/lib.rs`.
/// Per-table impls are added when a table is integrated into KV.
#[allow(async_fn_in_trait)]
pub(crate) trait UniqueConstraints {
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

/// Trait for KV-participating types that expose the `updated_by` field,
/// used by [`super::scheme::decide_storage_scheme`] to probe whether an
/// existing row was last written through Redis (`redis_kv`) or Postgres
/// (`postgres_only`).
///
/// All KV types have `updated_by: StorageScheme`; this trait provides a
/// uniform accessor so the probe can use the typed enum directly instead
/// of parsing a string.
pub(crate) trait KvUpdateProbe {
    fn updated_by(&self) -> StorageScheme;
}
