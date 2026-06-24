use std::{collections::BTreeMap, sync::Arc};

use aetower_model::{CapabilityKind, CapabilitySnapshot, CapabilityState};
use parking_lot::Mutex;

use crate::adapter_trait::Adapter;

use super::{AdapterState, adapter_capability_snapshot};

type RefreshCacheFn = fn(&Arc<Mutex<AdapterState>>);

struct AdapterDefinition {
    name: &'static str,
    capability_kind: CapabilityKind,
    refresh_cache: RefreshCacheFn,
}

const ADAPTER_DEFINITIONS: &[AdapterDefinition] = &[
    AdapterDefinition {
        name: "Chromium",
        capability_kind: CapabilityKind::ChromiumDebug,
        refresh_cache: super::refresh_chromium_cache,
    },
    AdapterDefinition {
        name: "Docker",
        capability_kind: CapabilityKind::DockerSocket,
        refresh_cache: super::refresh_docker_cache,
    },
    AdapterDefinition {
        name: "Privileged Helper",
        capability_kind: CapabilityKind::PrivilegedHelper,
        refresh_cache: super::refresh_privileged_helper_cache,
    },
    AdapterDefinition {
        name: "Endpoint Security",
        capability_kind: CapabilityKind::EndpointSecurity,
        refresh_cache: super::refresh_endpoint_security_cache,
    },
    AdapterDefinition {
        name: "Chau7",
        capability_kind: CapabilityKind::Chau7,
        refresh_cache: super::refresh_chau7_cache,
    },
];

struct GenericAdapter {
    state: Arc<Mutex<AdapterState>>,
    definition: &'static AdapterDefinition,
}

impl GenericAdapter {
    fn new(state: Arc<Mutex<AdapterState>>, definition: &'static AdapterDefinition) -> Self {
        debug_assert!(!definition.name.is_empty());
        Self { state, definition }
    }
}

impl Adapter for GenericAdapter {
    fn capability_kind(&self) -> CapabilityKind {
        self.definition.capability_kind.clone()
    }

    fn refresh_cache(&self) {
        (self.definition.refresh_cache)(&self.state);
    }

    fn capability_snapshot(&self, last_updated_millis: u64) -> CapabilitySnapshot {
        adapter_capability_snapshot(
            &self.state,
            self.definition.capability_kind.clone(),
            last_updated_millis,
        )
    }
}

pub(super) struct AdapterRegistry {
    adapters: Vec<Arc<dyn Adapter>>,
}

impl AdapterRegistry {
    pub(super) fn new(state: Arc<Mutex<AdapterState>>) -> Self {
        Self {
            adapters: ADAPTER_DEFINITIONS
                .iter()
                .map(|definition| {
                    Arc::new(GenericAdapter::new(Arc::clone(&state), definition))
                        as Arc<dyn Adapter>
                })
                .collect(),
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
