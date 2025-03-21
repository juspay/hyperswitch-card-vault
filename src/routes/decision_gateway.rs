use std::sync::Arc;

use axum::{routing::post, Json};

#[cfg(feature = "limit")]
use axum::{error_handling::HandleErrorLayer, response::IntoResponse};

use crate::{
    crypto::{
        hash_manager::managers::sha::Sha512,
        keymanager::{self, KeyProvider},
    },
    custom_extractors::TenantStateResolver,
    error::{self, ContainerError, ResultContainerExt},
    logger,
    storage::{FingerprintInterface, HashInterface, LockerInterface},
    tenant::GlobalAppState,
    utils,
};

pub async fn decision_gateway() -> &'static str {
    "hello world!"
}
