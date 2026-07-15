pub mod data;
pub mod health;
#[cfg(feature = "key_custodian")]
pub mod key_custodian;
#[cfg(feature = "external_key_manager")]
pub mod key_migration;
pub mod routes_v2;
pub mod tenant;

fn record_expired_data_encountered(resource: &'static str) {
    crate::observability::metrics::TTL_EXPIRED_DATA_ENCOUNTERED_COUNT
        .add(1, crate::metric_attributes!(("resource", resource)));
}

fn record_ttl_deletion_result(resource: &'static str, outcome: &'static str) {
    crate::observability::metrics::TTL_DELETION_COUNT.add(
        1,
        crate::metric_attributes!(("resource", resource), ("outcome", outcome)),
    );
}
