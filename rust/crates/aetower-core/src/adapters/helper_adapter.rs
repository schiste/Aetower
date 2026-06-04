use std::{collections::BTreeMap, sync::Arc};

use aetower_model::{CapabilityHealth, CapabilityKind, CapabilitySnapshot, EntitySnapshot};
use parking_lot::Mutex;

use crate::adapter_trait::Adapter;

use super::{AdapterState, adapter_capability_health, adapter_capability_snapshot};

pub(super) struct HelperAdapter {
    state: Arc<Mutex<AdapterState>>,
}

impl HelperAdapter {
    pub(super) fn new(state: Arc<Mutex<AdapterState>>) -> Self {
        Self { state }
    }
}

impl Adapter for HelperAdapter {
    fn name(&self) -> &str {
        "Privileged Helper"
    }

    fn capability_kind(&self) -> CapabilityKind {
        CapabilityKind::PrivilegedHelper
    }

    fn refresh_cache(&self) {
        super::refresh_privileged_helper_cache(&self.state);
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
            CapabilityKind::PrivilegedHelper,
            last_updated_millis,
        )
    }

    fn capability_health(&self) -> CapabilityHealth {
        adapter_capability_health(&self.state, &CapabilityKind::PrivilegedHelper)
    }
}
