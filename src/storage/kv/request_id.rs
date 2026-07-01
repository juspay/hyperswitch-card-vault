//! Request-id propagation via a Tokio task-local.
//! Set per-request by the `scope_request_id` middleware in `app.rs`.
//! Task-locals do not cross `tokio::spawn`.

use tokio::task_local;

task_local! {
    pub(crate) static REQUEST_ID: String;
}

/// Read the current request-id, or an empty string when no scope is active.
pub(crate) fn current_request_id() -> String {
    REQUEST_ID.try_with(|v| v.clone()).unwrap_or_default()
}
