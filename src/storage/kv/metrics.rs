//! KV operation counter metrics.

use crate::{counter_metric, global_meter};

global_meter!(pub(crate) KV_METER, "card_vault_kv");

counter_metric!(
    pub(crate) KV_OPERATION_SUCCESSFUL, KV_METER,
    name: "kv.operations.successful",
    description: "KV operations that succeeded",
    unit: "1",
);
counter_metric!(
    pub(crate) KV_OPERATION_FAILED, KV_METER,
    name: "kv.operations.failed",
    description: "KV operations that failed",
    unit: "1",
);
counter_metric!(
    pub(crate) KV_PUSHED_TO_DRAINER, KV_METER,
    name: "kv.drainer.pushed",
    description: "Entries pushed to the drainer stream",
    unit: "1",
);
counter_metric!(
    pub(crate) KV_FAILED_TO_PUSH_TO_DRAINER, KV_METER,
    name: "kv.drainer.push_failed",
    description: "Failed pushes to the drainer stream",
    unit: "1",
);
counter_metric!(
    pub(crate) KV_MISS, KV_METER,
    name: "kv.redis_miss",
    description: "Redis cache misses that fell back to Postgres",
    unit: "1",
);
