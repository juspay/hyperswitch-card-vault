use hyperswitch_redis_interface::errors::RedisError;

use super::metrics;
use crate::error::{RedisErrorExt, StorageError};

/// Try Redis first; on `RedisError::NotFound` fall back to the database closure
/// (emitting a `KV_MISS` metric).  Any other Redis error is converted to a
/// [`StorageError`] via [`RedisErrorExt::to_redis_failed_response`].
///
/// Vendored from `storage_impl/src/utils.rs`.
pub async fn try_redis_get_else_try_database_get<F, RFut, DFut, T>(
    redis_fut: RFut,
    database_call_closure: F,
) -> error_stack::Result<T, StorageError>
where
    F: FnOnce() -> DFut,
    RFut: futures::Future<Output = error_stack::Result<T, RedisError>>,
    DFut: futures::Future<Output = error_stack::Result<T, StorageError>>,
{
    let redis_output = redis_fut.await;
    match redis_output {
        Ok(output) => Ok(output),
        Err(redis_error) => match redis_error.current_context() {
            RedisError::NotFound => {
                metrics::KV_MISS.add(1, &[]);
                database_call_closure().await
            }
            _ => Err(redis_error.to_redis_failed_response("")),
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use error_stack::Report;
    use hyperswitch_redis_interface::errors::RedisError;

    use super::*;

    #[tokio::test]
    async fn redis_not_found_falls_back_to_database() {
        let redis_fut = async { Err::<u32, _>(Report::new(RedisError::NotFound)) };
        let db = || async { Ok::<u32, Report<StorageError>>(42) };
        let result = try_redis_get_else_try_database_get(redis_fut, db).await;
        assert_eq!(result.unwrap(), 42);
    }
}
