use axum::{extract, response::IntoResponse};

use crate::{app, error, storage::TestInterface};

/// '/health` API handler`
pub(crate) async fn health<Base: HealthCheck<State = State>, State>(
    extract::State(state): extract::State<Base::State>,
) -> (hyper::StatusCode, &'static str) {
    crate::logger::info!("Health was called");
    Base::health(state).await
}

pub(crate) trait HealthCheck {
    type State;
    async fn health(state: Self::State) -> (hyper::StatusCode, &'static str);
    async fn diagnostics(state: Self::State) -> (hyper::StatusCode, axum::Json<Diagnostics>);
}

#[derive(Debug, serde::Serialize, Default)]
pub struct Diagnostics {
    key_custodian_locked: bool,
    database_connection: State,
    database_read: State,
    database_write: State,
    database_delete: State,
}

#[derive(Debug, serde::Serialize, Default)]
pub enum State {
    Working,
    #[default]
    Failing,
}

#[cfg(feature = "key_custodian")]
pub struct Custodian;

pub struct Locker;

#[cfg(feature = "key_custodian")]
impl HealthCheck for Custodian {
    type State = app::SharedState;

    async fn health(_state: Self::State) -> (hyper::StatusCode, &'static str) {
        crate::logger::info!("Health was called");
        (hyper::StatusCode::FORBIDDEN, "key custodian locked")
    }
    async fn diagnostics(_state: Self::State) -> (hyper::StatusCode, axum::Json<Diagnostics>) {
        (hyper::StatusCode::FORBIDDEN, axum::Json(Default::default()))
    }
}

impl HealthCheck for Locker {
    type State = app::AppState;

    async fn health(state: Self::State) -> (hyper::StatusCode, &'static str) {
        let x = state.db.test().await;

        let output = x
            .map_err(Into::<error::ContainerError<error::ApiError>>::into)
            .into_response();

        (
            output.status(),
            if output.status() == hyper::StatusCode::OK {
                "Health is good"
            } else {
                "Health Check Failed"
            },
        )
    }
    async fn diagnostics(state: Self::State) -> (hyper::StatusCode, axum::Json<Diagnostics>) {
        let output = state.db.test().await;
        let case_match = output.as_ref().map_err(|err| err.get_inner());
        let diagnostics = match case_match {
            Ok(()) => axum::Json(Diagnostics {
                key_custodian_locked: false,
                database_connection: State::Working,
                database_read: State::Working,
                database_write: State::Working,
                database_delete: State::Working,
            }),

            Err(&error::TestDBError::DBReadError) => axum::Json(Diagnostics {
                key_custodian_locked: false,
                database_connection: State::Working,
                ..Default::default()
            }),

            Err(&error::TestDBError::DBWriteError) => axum::Json(Diagnostics {
                key_custodian_locked: false,
                database_connection: State::Working,
                database_read: State::Working,
                ..Default::default()
            }),

            Err(&error::TestDBError::DBDeleteError) => axum::Json(Diagnostics {
                key_custodian_locked: false,
                database_connection: State::Working,
                database_write: State::Working,
                database_read: State::Working,
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
}
