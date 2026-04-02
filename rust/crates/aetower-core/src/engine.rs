use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use aetower_model::{
    CapabilityKind, CapabilitySnapshot, CapabilityState, FrontmostAppState, HostSnapshot,
    HostTrend, SystemSnapshot,
};
use aetower_time::{self as time, ADAPTER_TICK, FAST_TICK};
use parking_lot::Mutex;

use crate::{
    adapters::AdapterManager, collector::Collector, friction, history::History, run_entity_pipeline,
};

struct EngineState {
    sequence: u64,
    latest_snapshot: SystemSnapshot,
    capabilities: BTreeMap<CapabilityKind, CapabilitySnapshot>,
    frontmost_app_state: Option<FrontmostAppState>,
    history: History,
}

impl EngineState {
    fn publish_mutation(&mut self) {
        self.sequence = self.sequence.saturating_add(1);
        self.latest_snapshot.sequence = self.sequence;
    }
}

pub struct Engine {
    state: Arc<Mutex<EngineState>>,
    adapters: AdapterManager,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    adapter_worker: Option<JoinHandle<()>>,
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
                battery_charge_percent: None,
                low_power_mode: false,
                ..HostSnapshot::default()
            },
            capabilities: capabilities.values().cloned().collect(),
            entities: Vec::new(),
            host_trend: HostTrend::default(),
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
            adapter_worker: None,
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
            let mut next_tick = Instant::now();

            while running.load(Ordering::SeqCst) {
                let captured_at_millis = time::now_millis();
                let raw = collector.collect();
                let (frontmost_app_state, capabilities) = {
                    let mut guard = state.lock();
                    refresh_adapter_capabilities(&mut guard, &adapters, captured_at_millis);
                    (
                        guard.frontmost_app_state.clone(),
                        guard.capabilities.clone(),
                    )
                };
                let pipeline_output = run_entity_pipeline(
                    &raw.processes,
                    &HostSnapshot {
                        cpu_percent: raw.host.cpu_percent,
                        memory_used_bytes: raw.host.memory_used_bytes,
                        memory_total_bytes: raw.host.memory_total_bytes,
                        swap_used_bytes: raw.host.swap_used_bytes,
                        compressed_memory_bytes: raw.host.compressed_memory_bytes,
                        disk_read_bps: raw.host.disk_read_bps,
                        disk_write_bps: raw.host.disk_write_bps,
                        network_receive_bps: raw.host.network_receive_bps,
                        network_send_bps: raw.host.network_send_bps,
                        wakeups_per_second: raw.host.wakeups_per_second,
                        thermal_state: raw.host.thermal_state.clone(),
                        on_battery: raw.host.on_battery,
                        battery_charge_percent: raw.host.battery_charge_percent,
                        low_power_mode: raw.host.low_power_mode,
                        frontmost_app_name: frontmost_app_state
                            .as_ref()
                            .map(|state| state.app_name.clone()),
                        frontmost_window_title: frontmost_app_state
                            .as_ref()
                            .and_then(|state| state.window_title.clone()),
                    },
                    frontmost_app_state.as_ref(),
                );
                let mut entities = pipeline_output.entities;
                adapters.enrich_entities(&mut entities, &capabilities);

                let host = HostSnapshot {
                    cpu_percent: raw.host.cpu_percent,
                    memory_used_bytes: raw.host.memory_used_bytes,
                    memory_total_bytes: raw.host.memory_total_bytes,
                    swap_used_bytes: raw.host.swap_used_bytes,
                    compressed_memory_bytes: raw.host.compressed_memory_bytes,
                    disk_read_bps: raw.host.disk_read_bps,
                    disk_write_bps: raw.host.disk_write_bps,
                    network_receive_bps: raw.host.network_receive_bps,
                    network_send_bps: raw.host.network_send_bps,
                    wakeups_per_second: raw.host.wakeups_per_second,
                    thermal_state: raw.host.thermal_state,
                    on_battery: raw.host.on_battery,
                    battery_charge_percent: raw.host.battery_charge_percent,
                    low_power_mode: raw.host.low_power_mode,
                    frontmost_app_name: frontmost_app_state
                        .as_ref()
                        .map(|state| state.app_name.clone()),
                    frontmost_window_title: frontmost_app_state
                        .as_ref()
                        .and_then(|state| state.window_title.clone()),
                };
                friction::apply(&host, &mut entities);

                let mut guard = state.lock();
                let (timeline, host_trend) =
                    guard
                        .history
                        .update(captured_at_millis, &host, &mut entities);
                guard.sequence += 1;
                guard.latest_snapshot = SystemSnapshot {
                    sequence: guard.sequence,
                    captured_at_millis,
                    host,
                    host_trend,
                    capabilities: capabilities.values().cloned().collect(),
                    entities,
                    timeline,
                };
                drop(guard);

                next_tick += FAST_TICK;
                let now = Instant::now();
                if now < next_tick {
                    thread::sleep(next_tick - now);
                } else {
                    next_tick = now;
                }
            }
        }));

        let state = Arc::clone(&self.state);
        let adapters = self.adapters.clone();
        let running = Arc::clone(&self.running);
        self.adapter_worker = Some(thread::spawn(move || {
            let mut next_tick = Instant::now();

            while running.load(Ordering::SeqCst) {
                let capabilities = {
                    let guard = state.lock();
                    guard.capabilities.clone()
                };
                adapters.refresh_caches(&capabilities);

                next_tick += ADAPTER_TICK;
                let now = Instant::now();
                if now < next_tick {
                    thread::sleep(next_tick - now);
                } else {
                    next_tick = now;
                }
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.adapter_worker.take() {
            let _ = worker.join();
        }
    }

    pub fn latest_snapshot(&self) -> SystemSnapshot {
        self.state.lock().latest_snapshot.clone()
    }

    pub fn latest_snapshot_if_newer(&self, last_sequence: u64) -> Option<SystemSnapshot> {
        let guard = self.state.lock();
        (guard.latest_snapshot.sequence > last_sequence).then(|| guard.latest_snapshot.clone())
    }

    pub fn latest_sequence(&self) -> u64 {
        self.state.lock().latest_snapshot.sequence
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
        guard.publish_mutation();
    }

    pub fn update_frontmost_app_state(&self, state: FrontmostAppState) {
        let mut guard = self.state.lock();
        guard.frontmost_app_state = Some(state);
    }

    pub fn clear_frontmost_app_state(&self) {
        let mut guard = self.state.lock();
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

    pub fn configure_chau7_endpoint(&self, socket_path: Option<String>) {
        self.adapters.configure_chau7_endpoint(socket_path);
        self.refresh_capability(CapabilityKind::Chau7);
    }

    fn refresh_capability(&self, kind: CapabilityKind) {
        let mut guard = self.state.lock();
        guard.capabilities.insert(
            kind.clone(),
            self.adapters.capability_snapshot(kind, time::now_millis()),
        );
        guard.latest_snapshot.capabilities = guard.capabilities.values().cloned().collect();
        guard.publish_mutation();
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

fn refresh_adapter_capabilities(
    state: &mut EngineState,
    adapters: &AdapterManager,
    now_millis: u64,
) {
    for kind in [
        CapabilityKind::ChromiumDebug,
        CapabilityKind::DockerSocket,
        CapabilityKind::PrivilegedHelper,
        CapabilityKind::Chau7,
    ] {
        state
            .capabilities
            .insert(kind.clone(), adapters.capability_snapshot(kind, now_millis));
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}
