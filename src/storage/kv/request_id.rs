//! Request-id propagation via a Tokio task-local.
//!
//! Set once per request by the `scope_request_id` middleware in `app.rs` and
//! read by the KV drainer push. Task-locals do **not** cross `tokio::spawn`, so
//! any KV write moved onto a spawned task will read an empty request-id; keep KV
//! writes on the request task (as they are today).

use tokio::task_local;

task_local! {
    pub(crate) static REQUEST_ID: String;
}

/// Read the current request-id, or an empty string when no scope is active.
pub(crate) fn current_request_id() -> String {
    REQUEST_ID.try_with(|v| v.clone()).unwrap_or_default()
}
