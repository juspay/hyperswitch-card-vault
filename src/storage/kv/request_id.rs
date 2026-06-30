//! Request-id propagation via a Tokio task-local.

use tokio::task_local;

task_local! {
    pub(crate) static REQUEST_ID: String;
}

/// Read the current request-id, or an empty string when no scope is active.
pub(crate) fn current_request_id() -> String {
    REQUEST_ID
        .try_with(|v| v.clone())
        .unwrap_or_default()
}
