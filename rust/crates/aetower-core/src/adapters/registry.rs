use std::{collections::BTreeMap, sync::Arc};

use aetower_model::{CapabilityKind, CapabilitySnapshot, CapabilityState};
use parking_lot::Mutex;

use crate::adapter_trait::Adapter;

use super::{
    AdapterState, chau7_adapter::Chau7Adapter, chromium_adapter::ChromiumAdapter,
    docker_adapter::DockerAdapter, endpoint_security_adapter::EndpointSecurityAdapter,
    helper_adapter::HelperAdapter,
};

pub(super) struct AdapterRegistry {
    adapters: Vec<Arc<dyn Adapter>>,
}

impl AdapterRegistry {
    pub(super) fn new(state: Arc<Mutex<AdapterState>>) -> Self {
        Self {
            adapters: vec![
                Arc::new(ChromiumAdapter::new(Arc::clone(&state))),
                Arc::new(DockerAdapter::new(Arc::clone(&state))),
                Arc::new(HelperAdapter::new(Arc::clone(&state))),
                Arc::new(EndpointSecurityAdapter::new(Arc::clone(&state))),
                Arc::new(Chau7Adapter::new(state)),
            ],
        }
    }

    pub(super) fn adapters(&self) -> &[Arc<dyn Adapter>] {
        &self.adapters
    }

    pub(super) fn adapter_for(&self, kind: &CapabilityKind) -> Option<&dyn Adapter> {
        self.adapters
            .iter()
            .find(|adapter| adapter.capability_kind() == *kind)
            .map(Arc::as_ref)
    }

    pub(super) fn refresh_caches(
        &self,
        capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>,
    ) {
        std::thread::scope(|scope| {
            for adapter in &self.adapters {
                if !adapter_refresh_enabled(adapter.capability_kind(), capabilities) {
                    continue;
                }
                scope.spawn(move || adapter.refresh_cache());
            }
        });
    }
}

fn adapter_refresh_enabled(
    kind: CapabilityKind,
    capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>,
) -> bool {
    match kind {
        CapabilityKind::EndpointSecurity => {
            capability_granted(capabilities, CapabilityKind::EndpointSecurity)
                || capability_granted(capabilities, CapabilityKind::PrivilegedHelper)
        }
        _ => capability_granted(capabilities, kind),
    }
}

fn capability_granted(
    capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>,
    kind: CapabilityKind,
) -> bool {
    capabilities
        .get(&kind)
        .is_some_and(|capability| capability.state == CapabilityState::Granted)
}
