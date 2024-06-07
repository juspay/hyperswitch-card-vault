use std::sync::Arc;

use crate::tenant::GlobalAppState;

use axum::{response::IntoResponse, routing::get, Json};

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
    crate::logger::info!("Health was called");
    Json(HealthRespPayload {
        message: "Health is good".into(),
    })
}

#[derive(Debug, serde::Serialize, Default)]
pub struct Diagnostics {
    key_custodian_locked: bool,
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
pub async fn diagnostics(
    TenantStateResolver(state): TenantStateResolver,
) -> (hyper::StatusCode, Json<Diagnostics>) {
    crate::logger::info!("Health diagnostics was called");

    let output = state.db.test().await;
    let case_match = output.as_ref().map_err(|err| err.get_inner());
    let diagnostics = match case_match {
        Ok(()) => axum::Json(Diagnostics {
            key_custodian_locked: false,
            database_connection: HealthState::Working,
            database_read: HealthState::Working,
            database_write: HealthState::Working,
            database_delete: HealthState::Working,
        }),

        Err(&error::TestDBError::DBReadError) => axum::Json(Diagnostics {
            key_custodian_locked: false,
            database_connection: HealthState::Working,
            ..Default::default()
        }),

        Err(&error::TestDBError::DBWriteError) => axum::Json(Diagnostics {
            key_custodian_locked: false,
            database_connection: HealthState::Working,
            database_read: HealthState::Working,
            ..Default::default()
        }),

        Err(&error::TestDBError::DBDeleteError) => axum::Json(Diagnostics {
            key_custodian_locked: false,
            database_connection: HealthState::Working,
            database_write: HealthState::Working,
            database_read: HealthState::Working,
            ..Default::default()
        }),

        Err(_) => axum::Json(Diagnostics {
            key_custodian_locked: false,
            ..Default::default()
        }),
    };

    let status_code = output
        .map_err(Into::<error::ContainerError<error::ApiError>>::into)
        .into_response()
        .status();

    (status_code, diagnostics)
}
