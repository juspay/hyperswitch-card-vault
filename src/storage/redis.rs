use std::sync::Arc;

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

impl std::fmt::Debug for RedisStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisStore")
            .field("redis_conn", &"RedisConnectionPool doesn't implement Debug")
            .finish()
    }
}

impl RedisStore {
    pub async fn new(conf: &RedisSettings) -> error_stack::Result<Self, RedisError> {
        let pool = RedisConnectionPool::new(conf).await.map_err(into_report)?;
        Ok(Self {
            redis_conn: Arc::new(pool),
        })
    }

    /// A handle onto the same pool that namespaces every key with `key_prefix`.
    pub fn clone_with_prefix(&self, key_prefix: &str) -> Self {
        Self {
            redis_conn: Arc::new(self.redis_conn.as_ref().clone(key_prefix)),
        }
    }

    pub fn spawn_error_watcher(&self) {
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
    pub fn get_redis_conn(&self) -> Arc<RedisConnectionPool> {
        self.redis_conn.clone()
    }

    pub async fn test(&self) -> error_stack::Result<(), RedisError> {
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
}
