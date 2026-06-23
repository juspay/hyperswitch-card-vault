use std::collections::HashSet;

use hyperswitch_redis_interface::errors::RedisError;

use super::{constraints::UniqueConstraints, metrics};
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

/// Deduplicate KV rows and SQL rows by their unique-constraint keys.
fn union_vec<T>(mut kv_rows: Vec<T>, sql_rows: Vec<T>) -> Vec<T>
where
    T: UniqueConstraints,
{
    let mut kv_unique_keys = HashSet::new();

    kv_rows.iter().for_each(|v| {
        kv_unique_keys.insert(v.unique_constraints().concat());
    });

    sql_rows.into_iter().for_each(|v| {
        let unique_key = v.unique_constraints().concat();
        if !kv_unique_keys.contains(&unique_key) {
            kv_rows.push(v);
        }
    });

    kv_rows
}

/// Find all rows in Redis KV; if the count is insufficient (or Redis misses
/// entirely), supplement with database rows.  Results are deduplicated by
/// [`UniqueConstraints::unique_constraints`].
///
/// Vendored from `storage_impl/src/utils.rs`.
pub async fn find_all_combined_kv_database<F, RFut, DFut, T>(
    redis_fut: RFut,
    database_call: F,
    limit: Option<i64>,
) -> error_stack::Result<Vec<T>, StorageError>
where
    T: UniqueConstraints,
    F: FnOnce() -> DFut,
    RFut: futures::Future<Output = error_stack::Result<Vec<T>, RedisError>>,
    DFut: futures::Future<Output = error_stack::Result<Vec<T>, StorageError>>,
{
    let trunc = |v: &mut Vec<_>| {
        if let Some(l) = limit.and_then(|v| TryInto::try_into(v).ok()) {
            v.truncate(l);
        }
    };

    let limit_satisfies = |len: usize, limit: i64| {
        TryInto::try_into(limit)
            .ok()
            .is_none_or(|val: usize| len >= val)
    };

    let redis_output = redis_fut.await;
    match (redis_output, limit) {
        (Ok(mut kv_rows), Some(lim)) if limit_satisfies(kv_rows.len(), lim) => {
            trunc(&mut kv_rows);
            Ok(kv_rows)
        }
        (Ok(kv_rows), _) => database_call().await.map(|db_rows| {
            let mut res = union_vec(kv_rows, db_rows);
            trunc(&mut res);
            res
        }),
        (Err(redis_error), _) => match redis_error.current_context() {
            RedisError::NotFound => {
                metrics::KV_MISS.add(1, &[]);
                database_call().await
            }
            _ => Err(redis_error.to_redis_failed_response("")),
        },
    }
}

/// Macro for the reverse-lookup fallback pattern.
///
/// If the reverse-lookup query returns a not-found error (`ValueNotFound` or
/// `NotFoundError`), fall back to the database call `$b`.  Otherwise propagate
/// the error.
///
/// Vendored from `common_utils/src/macros.rs::fallback_reverse_lookup_not_found!`.
#[macro_export]
macro_rules! fallback_reverse_lookup_not_found {
    ($a:expr, $b:expr) => {
        match $a {
            Ok(res) => res,
            Err(err) => {
                $crate::logger::error!(reverse_lookup_fallback = ?err);
                match err.current_context() {
                    $crate::error::StorageError::ValueNotFound(_) => return $b,
                    $crate::error::StorageError::NotFoundError => return $b,
                    _ => return Err(err),
                }
            }
        };
    };
}

pub use fallback_reverse_lookup_not_found;
