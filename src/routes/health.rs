use std::sync::Arc;

use crate::tenant::GlobalAppState;
#[cfg(feature = "external_key_manager")]
use crate::{crypto::keymanager, logger};

use axum::{routing::get, Json};

use crate::{custom_extractors::TenantStateResolver, error, storage::TestInterface};

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
}

#[derive(Debug, serde::Serialize, Default)]
pub struct DatabaseHealth {
    database_connection: HealthState,
    database_read: HealthState,
    database_write: HealthState,
    database_delete: HealthState,
}

#[derive(Debug, serde::Serialize, Default)]
pub enum HealthState {
    Working,
    #[default]
    Failing,
}

/// '/health/diagnostics` API handler`
pub async fn diagnostics(TenantStateResolver(state): TenantStateResolver) -> Json<Diagnostics> {
    crate::logger::info!("Health diagnostics was called");

    let db_test_output = state.db.test().await;
    let db_test_output_case_match = db_test_output.as_ref().map_err(|err| err.get_inner());

    let db_health = match db_test_output_case_match {
        Ok(()) => DatabaseHealth {
            database_connection: HealthState::Working,
            database_read: HealthState::Working,
            database_write: HealthState::Working,
            database_delete: HealthState::Working,
        },

        Err(&error::TestDBError::DBReadError) => DatabaseHealth {
            database_connection: HealthState::Working,
            ..Default::default()
        },

        Err(&error::TestDBError::DBWriteError) => DatabaseHealth {
            database_connection: HealthState::Working,
            database_read: HealthState::Working,
            ..Default::default()
        },

        Err(&error::TestDBError::DBDeleteError) => DatabaseHealth {
            database_connection: HealthState::Working,
            database_write: HealthState::Working,
            database_read: HealthState::Working,
            ..Default::default()
        },

        Err(_) => DatabaseHealth {
            ..Default::default()
        },
    };

    #[cfg(feature = "external_key_manager")]
    let keymanager_status = keymanager::external_keymanager::health_check_keymanager(&state)
        .await
        .map_err(|err| logger::error!(keymanager_err=?err))
        .unwrap_or_default();

    axum::Json(Diagnostics {
        key_custodian_locked: false,
        database: db_health,
        #[cfg(feature = "external_key_manager")]
        keymanager_status,
    })
}
