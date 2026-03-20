use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use aetower_model::{
    CapabilityKind, CapabilitySnapshot, CapabilityState, FrontmostAppState, HostSnapshot,
    SystemSnapshot,
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
    frontmost_app_state: Option<FrontmostAppState>,
    history: History,
}

pub struct Engine {
    state: Arc<Mutex<EngineState>>,
    adapters: AdapterManager,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn new() -> Self {
        let adapters = AdapterManager::default();
        let capabilities = adapters.initial_capabilities();
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
                frontmost_app_state: None,
                history: History::new(),
            })),
            adapters,
            running: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }

    pub fn start(&mut self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let state = Arc::clone(&self.state);
        let adapters = self.adapters.clone();
        let running = Arc::clone(&self.running);
        self.worker = Some(thread::spawn(move || {
            let mut collector = Collector::new();

            while running.load(Ordering::SeqCst) {
                let captured_at_millis = time::now_millis();
                let raw = collector.collect();
                let identity = identity::resolve(&raw.processes);
                let (frontmost_app_state, capabilities) = {
                    let guard = state.lock();
                    (
                        guard.frontmost_app_state.clone(),
                        guard.capabilities.clone(),
                    )
                };
                let mut entities =
                    attribution::build_entities(&raw.processes, &identity, frontmost_app_state.as_ref());
                adapters.enrich_entities(&mut entities, &capabilities);

                let host = HostSnapshot {
                    cpu_percent: raw.host.cpu_percent,
                    memory_used_bytes: raw.host.memory_used_bytes,
                    memory_total_bytes: raw.host.memory_total_bytes,
                    swap_used_bytes: raw.host.swap_used_bytes,
                    network_receive_bps: raw.host.network_receive_bps,
                    network_send_bps: raw.host.network_send_bps,
                    thermal_state: "nominal".to_owned(),
                    on_battery: false,
                    frontmost_app_name: frontmost_app_state.as_ref().map(|state| state.app_name.clone()),
                    frontmost_window_title: frontmost_app_state
                        .as_ref()
                        .and_then(|state| state.window_title.clone()),
                };

                friction::apply(&host, &mut entities);
                let mut guard = state.lock();
                let timeline = guard.history.update(captured_at_millis, &entities);
                guard.sequence += 1;
                guard.latest_snapshot = SystemSnapshot {
                    sequence: guard.sequence,
                    captured_at_millis,
                    host,
                    capabilities: capabilities.values().cloned().collect(),
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

    pub fn latest_snapshot(&self) -> SystemSnapshot {
        self.state.lock().latest_snapshot.clone()
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

    pub fn update_frontmost_app_state(&self, state: FrontmostAppState) {
        let mut guard = self.state.lock();
        guard.latest_snapshot.host.frontmost_app_name = Some(state.app_name.clone());
        guard.latest_snapshot.host.frontmost_window_title = state.window_title.clone();
        guard.frontmost_app_state = Some(state);
    }

    pub fn clear_frontmost_app_state(&self) {
        let mut guard = self.state.lock();
        guard.latest_snapshot.host.frontmost_app_name = None;
        guard.latest_snapshot.host.frontmost_window_title = None;
        guard.frontmost_app_state = None;
    }

    pub fn configure_chromium_endpoint(&self, endpoint: Option<String>) {
        self.adapters.configure_chromium_endpoint(endpoint);
        self.refresh_capability(CapabilityKind::ChromiumDebug);
    }

    pub fn configure_docker_socket_path(&self, socket_path: String) {
        self.adapters.configure_docker_socket_path(socket_path);
        self.refresh_capability(CapabilityKind::DockerSocket);
    }

    pub fn configure_privileged_helper(&self, helper_path: Option<String>, enabled: bool) {
        self.adapters
            .configure_privileged_helper(helper_path, enabled);
        self.refresh_capability(CapabilityKind::PrivilegedHelper);
    }

    fn refresh_capability(&self, kind: CapabilityKind) {
        let mut guard = self.state.lock();
        guard
            .capabilities
            .insert(kind.clone(), self.adapters.capability_snapshot(kind, time::now_millis()));
        guard.latest_snapshot.capabilities = guard.capabilities.values().cloned().collect();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}
