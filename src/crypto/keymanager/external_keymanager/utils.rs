use std::collections::HashSet;

use base64::Engine;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE};
use masking::Maskable;

use crate::{app::TenantAppState, crypto::consts::BASE64_ENGINE};

pub fn get_key_manager_header(
    tenant_app_state: &TenantAppState,
) -> HashSet<(String, Maskable<String>)> {
    [
        (CONTENT_TYPE.to_string(), "application/json".into()),
        (
            AUTHORIZATION.to_string(),
            get_auth_header(tenant_app_state).into(),
        ),
    ]
    .into_iter()
    .collect::<HashSet<_>>()
}

pub fn get_auth_header(tenant_app_state: &TenantAppState) -> String {
    format!(
        "Basic {}",
        BASE64_ENGINE.encode(format!(
            "{}:{}",
            &tenant_app_state.config.tenant_id,
            hex::encode(&tenant_app_state.config.tenant_secrets.master_key)
        ))
    )
}
