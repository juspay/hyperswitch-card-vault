use std::collections::HashSet;

use hyper::header::CONTENT_TYPE;
use hyperswitch_masking::Maskable;

use crate::{app::TenantAppState, storage::consts::X_TENANT_ID};

pub fn get_key_manager_header(
    tenant_app_state: &TenantAppState,
) -> HashSet<(String, Maskable<String>)> {
    [
        (CONTENT_TYPE.to_string(), "application/json".into()),
        (
            X_TENANT_ID.to_string(),
            tenant_app_state.config.tenant_id.clone().into(),
        ),
    ]
    .into_iter()
    .collect::<std::collections::HashSet<_>>()
}
