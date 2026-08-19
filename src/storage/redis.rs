use std::{future::Future, sync::Arc};

use hyperswitch_redis_interface::{RedisConnectionPool, RedisSettings, errors::RedisError};
use tracing::Instrument;

use crate::storage::consts;

// error_stack 0.4 (redis_interface) vs 0.5 (tartarus): rebuild, `?` can't bridge them.
fn into_report(err: impl std::fmt::Debug) -> error_stack::Report<RedisError> {
    error_stack::Report::new(RedisError::RedisConnectionError).attach_printable(format!("{err:?}"))
}

/// A shared `redis_interface` connection pool handle.
#[derive(Clone)]
pub struct RedisStore {
    redis_conn: Arc<RedisConnectionPool>,
}

#[derive(Clone)]
pub struct TenantAwareRedisStore {
    inner: RedisStore,
}

impl std::ops::Deref for TenantAwareRedisStore {
    type Target = RedisStore;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::fmt::Debug for RedisStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisStore")
            .field("redis_conn", &"RedisConnectionPool doesn't implement Debug")
            .finish()
    }
}

impl RedisStore {
    pub(crate) async fn new(conf: &RedisSettings) -> error_stack::Result<Self, RedisError> {
        let pool = RedisConnectionPool::new(conf).await.map_err(into_report)?;
        Ok(Self {
            redis_conn: Arc::new(pool),
        })
    }

    /// A handle onto the same pool that namespaces every key with `key_prefix`.
    pub(crate) fn clone_with_prefix(&self, key_prefix: &str) -> TenantAwareRedisStore {
        // `.as_ref().clone(..)` calls the pool's inherent `clone`, not `Arc::clone`.
        TenantAwareRedisStore {
            inner: Self {
                redis_conn: Arc::new(self.redis_conn.as_ref().clone(key_prefix)),
            },
        }
    }

    // Logs disconnects via `on_error`. `rx` stays bound (not `_`) so its `tx.send` succeeds.
    pub(crate) fn spawn_error_watcher(&self) {
        let redis_conn = self.redis_conn.clone();
        tokio::spawn(
            async move {
                let (tx, _rx) = tokio::sync::oneshot::channel();
                redis_conn.on_error(tx).await;
            }
            .in_current_span(),
        );
    }

    /// The shared pool. It manages (re)connection internally, so callers run
    /// commands directly and surface per-command errors themselves.
    pub(crate) fn get_redis_conn(&self) -> Arc<RedisConnectionPool> {
        self.redis_conn.clone()
    }

    pub(crate) async fn test(&self) -> error_stack::Result<(), RedisError> {
        let redis_conn = self.get_redis_conn();
        let key = consts::REDIS_HEALTH_CHECK_KEY.into();
        redis_conn
            .set_key_with_expiry(
                &key,
                consts::REDIS_HEALTH_CHECK_VALUE,
                consts::REDIS_HEALTH_CHECK_EXPIRY,
            )
            .await
            .map_err(into_report)?;
        let value: String = redis_conn.get_key(&key).await.map_err(into_report)?;
        if value != consts::REDIS_HEALTH_CHECK_VALUE {
            return Err(error_stack::Report::new(RedisError::UnknownResult)
                .attach_printable("Redis health-check value mismatch"));
        }
        redis_conn.delete_key(&key).await.map_err(into_report)?;
        Ok(())
    }

    /// Read-through cache helper.
    ///
    /// Tries `GET key` first; on a hit returns the cached string immediately.
    /// On a miss (or Redis error — fail-open) it calls `fetch` to produce the
    /// value, then best-effort populates Redis with `SETEX key ttl value`
    /// so subsequent reads hit the cache. Returns `None` when both Redis and
    /// the `fetch` fallback yield nothing.
    pub(crate) async fn get_or_populate<F, Fut>(
        &self,
        key: &str,
        ttl_secs: i64,
        fetch: F,
    ) -> Option<String>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Option<String>> + Send,
    {
        let redis_conn = self.get_redis_conn();
        let redis_key = key.into();

        match redis_conn.get_key::<Option<String>>(&redis_key).await {
            Ok(Some(value)) => {
                crate::logger::debug!(redis_key = %key, "Runtime config cache hit");
                return Some(value);
            }
            Ok(None) => {
                crate::logger::debug!(redis_key = %key, "Runtime config cache miss");
            }
            Err(err) => {
                crate::logger::warn!(
                    ?err,
                    redis_key = %key,
                    "Redis GET failed for runtime config, falling back to fetch"
                );
            }
        }

        let value = fetch().await?;

        if let Err(err) = redis_conn
            .set_key_with_expiry(&redis_key, value.clone(), ttl_secs)
            .await
        {
            crate::logger::warn!(
                ?err,
                redis_key = %key,
                "Failed to populate Redis cache for runtime config"
            );
        }

        Some(value)
    }

    /// Invalidate (DEL) a cached key. The Redis TTL bounds staleness if this
    /// fails, so errors are logged as warnings rather than propagated.
    pub(crate) async fn invalidate(&self, key: &str) {
        let redis_conn = self.get_redis_conn();
        let redis_key = key.into();
        if let Err(err) = redis_conn.delete_key(&redis_key).await {
            crate::logger::warn!(
                ?err,
                redis_key = %key,
                "Failed to invalidate Redis cache for runtime config; TTL bounds staleness"
            );
        }
    }
}
