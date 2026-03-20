use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use aetower_model::{
    CapabilityKind, CapabilitySnapshot, CapabilityState, HostSnapshot, SystemSnapshot,
};
use parking_lot::Mutex;

use crate::{
    adapters::AdapterManager,
    attribution, collector::Collector, friction, history::History, identity, time,
};

struct EngineState {
    sequence: u64,
    latest_snapshot: SystemSnapshot,
    capabilities: BTreeMap<CapabilityKind, CapabilitySnapshot>,
    history: History,
}

pub struct Engine {
    state: Arc<Mutex<EngineState>>,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn new() -> Self {
        let capabilities = AdapterManager::initial_capabilities();
        let snapshot = SystemSnapshot {
            sequence: 0,
            captured_at_millis: time::now_millis(),
            host: HostSnapshot {
                thermal_state: "nominal".to_owned(),
                on_battery: false,
                ..HostSnapshot::default()
            },
            capabilities: capabilities.values().cloned().collect(),
            entities: Vec::new(),
            timeline: Vec::new(),
        };

        Self {
            state: Arc::new(Mutex::new(EngineState {
                sequence: 0,
                latest_snapshot: snapshot,
                capabilities,
                history: History::new(),
            })),
            running: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    pub fn start(&mut self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);
        self.worker = Some(thread::spawn(move || {
            let mut collector = Collector::new();
            let adapters = AdapterManager::default();

            while running.load(Ordering::SeqCst) {
                let captured_at_millis = time::now_millis();
                let raw = collector.collect();
                let identity = identity::resolve(&raw.processes);
                let mut entities = attribution::build_entities(&raw.processes, &identity);

                let mut guard = state.lock();
                adapters.enrich_entities(&mut entities, &guard.capabilities);

                let host = HostSnapshot {
                    cpu_percent: raw.host.cpu_percent,
                    memory_used_bytes: raw.host.memory_used_bytes,
                    memory_total_bytes: raw.host.memory_total_bytes,
                    swap_used_bytes: raw.host.swap_used_bytes,
                    network_receive_bps: raw.host.network_receive_bps,
                    network_send_bps: raw.host.network_send_bps,
                    thermal_state: "nominal".to_owned(),
                    on_battery: false,
                };

                friction::apply(&host, &mut entities);
                let timeline = guard.history.update(captured_at_millis, &entities);
                guard.sequence += 1;
                guard.latest_snapshot = SystemSnapshot {
                    sequence: guard.sequence,
                    captured_at_millis,
                    host,
                    capabilities: guard.capabilities.values().cloned().collect(),
                    entities,
                    timeline,
                };
                drop(guard);

                thread::sleep(time::FAST_TICK);
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    pub fn latest_snapshot_json(&self) -> String {
        serde_json::to_string(&self.state.lock().latest_snapshot).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn set_capability_state(
        &self,
        kind: CapabilityKind,
        state: CapabilityState,
        detail_override: Option<String>,
    ) {
        let mut guard = self.state.lock();
        let now = time::now_millis();
        if let Some(capability) = guard.capabilities.get_mut(&kind) {
            capability.state = state;
            capability.last_updated_millis = now;
            if let Some(detail) = detail_override {
                capability.detail = detail;
            }
        }
        guard.latest_snapshot.capabilities = guard.capabilities.values().cloned().collect();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}
