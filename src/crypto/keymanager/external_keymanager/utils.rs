use std::collections::HashSet;

use base64::Engine;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE};
use masking::{Mask, Maskable};

use crate::storage::consts::X_TENANT_ID;
use crate::{app::TenantAppState, crypto::consts::BASE64_ENGINE};

pub fn get_key_manager_header(
    tenant_app_state: &TenantAppState,
) -> HashSet<(String, Maskable<String>)> {
    let broken_master_key = {
        let broken_master_key = &tenant_app_state.config.tenant_secrets.master_key;
        let (left_half, right_half) = broken_master_key.split_at(broken_master_key.len() / 2);
        let hex_left = hex::encode(left_half);
        let hex_right = hex::encode(right_half);
        BASE64_ENGINE.encode(format!("{}:{}", hex_left, hex_right))
    };
    [
        (CONTENT_TYPE.to_string(), "application/json".into()),
        (
            AUTHORIZATION.to_string(),
            format!("Basic {}", broken_master_key).into_masked(),
        ),
        (
            X_TENANT_ID.to_string(),
            tenant_app_state.config.tenant_id.clone().into(),
        ),
    ]
    .into_iter()
    .collect::<std::collections::HashSet<_>>()
}
