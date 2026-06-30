//! KV operation counter metrics.

use crate::{counter_metric, global_meter};

global_meter!(pub(crate) KV_METER, "card_vault_kv");

counter_metric!(pub(crate) KV_OPERATION_SUCCESSFUL, KV_METER);
counter_metric!(pub(crate) KV_OPERATION_FAILED, KV_METER);
counter_metric!(pub(crate) KV_PUSHED_TO_DRAINER, KV_METER);
counter_metric!(pub(crate) KV_FAILED_TO_PUSH_TO_DRAINER, KV_METER);
