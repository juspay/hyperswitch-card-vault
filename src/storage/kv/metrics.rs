//! OpenTelemetry counter metrics for KV operations.
//!
//! Counters are defined via the vault's own `observability` macros, mirroring
//! the approach used for HTTP request metrics.  A global meter provider must
//! be stood up at startup (see [`crate::observability::init_metrics`]) for the
//! counters to be exported; without a provider they are no-ops.

use crate::{counter_metric, global_meter};

global_meter!(pub(crate) KV_METER, "card_vault_kv");

counter_metric!(pub(crate) KV_OPERATION_SUCCESSFUL, KV_METER);
counter_metric!(pub(crate) KV_OPERATION_FAILED, KV_METER);
counter_metric!(pub(crate) KV_PUSHED_TO_DRAINER, KV_METER);
counter_metric!(pub(crate) KV_FAILED_TO_PUSH_TO_DRAINER, KV_METER);
counter_metric!(pub(crate) KV_MISS, KV_METER);
