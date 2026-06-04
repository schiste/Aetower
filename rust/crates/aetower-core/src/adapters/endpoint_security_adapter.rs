use std::{collections::BTreeMap, sync::Arc};

use aetower_model::{CapabilityHealth, CapabilityKind, CapabilitySnapshot, EntitySnapshot};
use parking_lot::Mutex;

use crate::adapter_trait::Adapter;

use super::{AdapterState, adapter_capability_health, adapter_capability_snapshot};

pub(super) struct EndpointSecurityAdapter {
    state: Arc<Mutex<AdapterState>>,
}

impl EndpointSecurityAdapter {
    pub(super) fn new(state: Arc<Mutex<AdapterState>>) -> Self {
        Self { state }
    }
}

impl Adapter for EndpointSecurityAdapter {
    fn name(&self) -> &str {
        "Endpoint Security"
    }

    fn capability_kind(&self) -> CapabilityKind {
        CapabilityKind::EndpointSecurity
    }

    fn refresh_cache(&self) {
        super::refresh_endpoint_security_cache(&self.state);
    }

    fn enrich_entities(
        &self,
        _entities: &mut [EntitySnapshot],
        _capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>,
    ) {
    }

    fn capability_snapshot(&self, last_updated_millis: u64) -> CapabilitySnapshot {
        adapter_capability_snapshot(
            &self.state,
            CapabilityKind::EndpointSecurity,
            last_updated_millis,
        )
    }

    fn capability_health(&self) -> CapabilityHealth {
        adapter_capability_health(&self.state, &CapabilityKind::EndpointSecurity)
    }
}
