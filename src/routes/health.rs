use std::sync::Arc;

use axum::{Json, routing::get};

#[cfg(feature = "external_key_manager")]
use crate::{crypto::keymanager, logger};
use crate::{
    custom_extractors::TenantStateResolver, error, storage::TestInterface, tenant::GlobalAppState,
};
async fn record_health_check<Fut, T, E>(future: Fut, check: &'static str) -> Result<T, E>
where
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let start = std::time::Instant::now();
    let result = future.await;
    let duration = start.elapsed();
    let outcome = if result.is_ok() { "success" } else { "error" };

    crate::observability::metrics::HEALTH_CHECK_DURATION.record(
        duration.as_secs_f64(),
        crate::metric_attributes!(("check", check), ("outcome", outcome)),
    );

    result
}

///
/// Function for registering routes that is specifically handling the health apis
///
pub fn serve() -> axum::Router<Arc<GlobalAppState>> {
    axum::Router::new()
        .route("/", get(health))
        .route("/diagnostics", get(diagnostics))
}

#[derive(serde::Serialize, Debug)]
pub struct HealthRespPayload {
    pub message: String,
}

/// '/health` API handler`
#[tracing::instrument(skip_all)]
pub async fn health() -> Json<HealthRespPayload> {
    crate::logger::debug!("Health was called");
    Json(HealthRespPayload {
        message: "Health is good".into(),
    })
}

#[derive(Debug, serde::Serialize, Default)]
pub struct Diagnostics {
    key_custodian_locked: bool,
    database: DatabaseHealth,
    #[cfg(feature = "external_key_manager")]
    keymanager_status: HealthState,
    #[cfg(feature = "redis")]
    redis_status: HealthState,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct DatabaseHealth {
    database_connection: HealthState,
    database_read: HealthState,
    database_write: HealthState,
    database_delete: HealthState,
    database_replica: HealthState,
}

#[derive(Debug, serde::Serialize, Default)]
pub enum HealthState {
    Working,
    #[default]
    Failing,
    Disabled,
}

/// '/health/diagnostics` API handler`
#[tracing::instrument(skip_all)]
pub async fn diagnostics(TenantStateResolver(state): TenantStateResolver) -> Json<Diagnostics> {
    crate::logger::info!("Health diagnostics was called");

    let db_test_output = record_health_check(state.db.test(), "database").await;
    let db_test_output_case_match = db_test_output.as_ref().map_err(|err| err.get_inner());

    let replica_database_health = if state.db.has_replica() {
        match record_health_check(state.db.test_replica(), "database_replica").await {
            Ok(()) => HealthState::Working,
            Err(_) => HealthState::Failing,
        }
    } else {
        HealthState::Disabled
    };

    let db_health = match db_test_output_case_match {
        Ok(()) => DatabaseHealth {
            database_connection: HealthState::Working,
            database_read: HealthState::Working,
            database_write: HealthState::Working,
            database_delete: HealthState::Working,
            database_replica: replica_database_health,
        },

        Err(&error::TestDBError::DBReadError) => DatabaseHealth {
            database_connection: HealthState::Working,
            database_replica: replica_database_health,
            ..Default::default()
        },

        Err(&error::TestDBError::DBWriteError) => DatabaseHealth {
            database_connection: HealthState::Working,
            database_read: HealthState::Working,
            database_replica: replica_database_health,
            ..Default::default()
        },

        Err(&error::TestDBError::DBDeleteError) => DatabaseHealth {
            database_connection: HealthState::Working,
            database_write: HealthState::Working,
            database_read: HealthState::Working,
            database_replica: replica_database_health,
            ..Default::default()
        },

        Err(_) => DatabaseHealth {
            database_replica: replica_database_health,
            ..Default::default()
        },
    };

    #[cfg(feature = "external_key_manager")]
    let keymanager_status = {
        use crate::config::ExternalKeyManagerConfig;

        match &state.config.external_key_manager {
            ExternalKeyManagerConfig::Disabled => HealthState::Disabled,
            ExternalKeyManagerConfig::Enabled { .. }
            | ExternalKeyManagerConfig::EnabledWithMtls { .. } => record_health_check(
                keymanager::external_keymanager::health_check_keymanager(&state),
                "keymanager",
            )
            .await
            .map_err(|err| logger::error!(keymanager_err=?err))
            .unwrap_or_default(),
        }
    };

    #[cfg(feature = "redis")]
    let redis_status = match &state.db.get_redis_store() {
        None => HealthState::Disabled,
        Some(redis) => match record_health_check(redis.test(), "redis").await {
            Ok(()) => HealthState::Working,
            Err(err) => {
                crate::logger::error!(redis_err=?err);
                HealthState::Failing
            }
        },
    };

    axum::Json(Diagnostics {
        key_custodian_locked: false,
        database: db_health,
        #[cfg(feature = "external_key_manager")]
        keymanager_status,
        #[cfg(feature = "redis")]
        redis_status,
    })
}
