use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use rustc_hash::FxHashMap;
use tokio::sync::RwLock;

#[cfg(feature = "key_custodian")]
use crate::routes::key_custodian::CustodianKeyState;
use crate::{app::TenantAppState, config::GlobalConfig, error::ApiError};

pub struct GlobalAppState {
    pub tenants_app_state: RwLock<FxHashMap<String, Arc<TenantAppState>>>,
    #[cfg(feature = "key_custodian")]
    pub tenants_key_state: RwLock<FxHashMap<String, CustodianKeyState>>,
    pub known_tenants: HashSet<String>,
    pub global_config: GlobalConfig,
}

impl GlobalAppState {
    pub fn new(config: &GlobalConfig) -> Arc<Self> {
        let known_tenants = <HashMap<_, _> as Clone>::clone(&config.tenant_secrets)
            .into_keys()
            .collect::<Vec<_>>();

        #[cfg(feature = "key_custodian")]
        let tenants_key_state = {
            let mut tenants_key_state: FxHashMap<String, CustodianKeyState> = FxHashMap::default();
            for tenant in known_tenants.clone() {
                tenants_key_state.insert(tenant, CustodianKeyState::default());
            }
            tenants_key_state
        };

        Arc::new(Self {
            tenants_app_state: RwLock::new(FxHashMap::default()),
            #[cfg(feature = "key_custodian")]
            tenants_key_state: RwLock::new(tenants_key_state),
            known_tenants: HashSet::from_iter(known_tenants),
            global_config: config.clone(),
        })
    }

    pub async fn get_app_state_of_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Arc<TenantAppState>, ApiError> {
        self.tenants_app_state
            .read()
            .await
            .get(tenant_id)
            .cloned()
            .ok_or(ApiError::CustodianLocked)
    }

    pub async fn is_known_tenant(&self, tenant_id: &str) -> Result<(), ApiError> {
        self.known_tenants
            .contains(tenant_id)
            .then_some(())
            .ok_or(ApiError::TenantError("Invalid x-tenant-id"))
    }

    pub async fn set_app_state(&self, state: TenantAppState) {
        let mut write_guard = self.tenants_app_state.write().await;
        write_guard.insert(state.config.tenant_id.clone(), Arc::new(state));
    }

    #[cfg(feature = "key_custodian")]
    // Check if the custodian is already unlocked for a tenant, if so raise an error when calling custodian endpoints
    pub async fn is_custodian_unlocked(&self, tenant_id: &str) -> Result<(), ApiError> {
        self.tenants_app_state
            .read()
            .await
            .contains_key(tenant_id)
            .then_some(Err::<(), ApiError>(ApiError::CustodianUnlocked))
            .transpose()?;
        Ok(())
    }
}
