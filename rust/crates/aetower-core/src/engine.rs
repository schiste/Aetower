use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsOverview, DiagnosticsStore, DiagnosticsSubsystem,
};
use aetower_model::{
    CapabilityKind, CapabilitySnapshot, CapabilityState, FrontmostAppState, HostSnapshot,
    HostTrend, SystemSnapshot, ThermalState,
};
use aetower_telemetry::{OtlpConfig, TelemetryExporter};
use aetower_time::{self as time, ADAPTER_TICK, FAST_TICK};
use parking_lot::Mutex;

use crate::{
    adapters::AdapterManager, collector::Collector, history::History, run_entity_pipeline,
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
    persistence: Arc<Mutex<Option<aetower_persistence::HistoryStore>>>,
    telemetry: Arc<Mutex<TelemetryExporter>>,
    diagnostics: DiagnosticsStore,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    adapter_worker: Option<JoinHandle<()>>,
    telemetry_worker: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn new() -> Self {
        let app_support_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("Aetower");
        let diagnostics_path = app_support_dir.join("diagnostics.ndjson");
        let diagnostics = DiagnosticsStore::with_persistence(2_000, &diagnostics_path, 10_000)
            .unwrap_or_default();
        let adapters = AdapterManager::default();
        adapters.set_diagnostics(diagnostics.clone());
        let capabilities = adapters.initial_capabilities();
        let snapshot = SystemSnapshot {
            sequence: 0,
            captured_at_millis: time::now_millis(),
            host: HostSnapshot {
                thermal_state: ThermalState::Nominal,
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

        // Open persistence database (best-effort — app works without it).
        let db_path = app_support_dir.join("history.db");
        let persistence = std::fs::create_dir_all(db_path.parent().unwrap())
            .ok()
            .and_then(|_| aetower_persistence::HistoryStore::open(&db_path, 5).ok())
            .map(|mut store| {
                store.set_diagnostics(diagnostics.clone());
                store
            });
        if let Some(store) = persistence.as_ref() {
            // Prune entries older than 7 days on startup.
            let seven_days_ago = time::now_millis().saturating_sub(7 * 24 * 60 * 60 * 1000);
            let _ = store.prune(seven_days_ago);
        }
        let mut telemetry_exporter = TelemetryExporter::new(telemetry_config_from_env());
        telemetry_exporter.set_diagnostics(diagnostics.clone());
        diagnostics.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "engine-initialized",
                "Aetower engine initialized.",
            )
            .field("history_path", db_path.display())
            .build(),
        );

        Self {
            state: Arc::new(Mutex::new(EngineState {
                sequence: 0,
                latest_snapshot: snapshot,
                capabilities,
                frontmost_app_state: None,
                history: History::new(),
            })),
            adapters,
            persistence: Arc::new(Mutex::new(persistence)),
            telemetry: Arc::new(Mutex::new(telemetry_exporter)),
            diagnostics,
            running: Arc::new(AtomicBool::new(false)),
            worker: None,
            adapter_worker: None,
            telemetry_worker: None,
        }
    }

    pub fn start(&mut self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let state = Arc::clone(&self.state);
        let adapters = self.adapters.clone();
        let persistence = Arc::clone(&self.persistence);
        let running = Arc::clone(&self.running);
        let diagnostics = self.diagnostics.clone();
        self.worker = Some(thread::spawn(move || {
            let mut collector = Collector::new();
            let mut next_tick = Instant::now();
            let mut gpu_sample = aetower_gpu::GpuSample::default();
            let mut gpu_sample_tick: u8 = 0;

            while running.load(Ordering::SeqCst) {
                let tick_started = Instant::now();
                let captured_at_millis = time::now_millis();
                diagnostics.emit(
                    DiagnosticsEvent::builder(
                        DiagnosticsLevel::Debug,
                        DiagnosticsSubsystem::Engine,
                        "tick-started",
                        "Engine tick started.",
                    )
                    .timestamp_millis(captured_at_millis)
                    .build(),
                );
                let raw = collector.collect();
                if gpu_sample_tick.is_multiple_of(3)
                    && let Some(sample) = aetower_gpu::sample_gpu()
                {
                    gpu_sample = sample.clone();
                    diagnostics.emit(
                        DiagnosticsEvent::builder(
                            DiagnosticsLevel::Debug,
                            DiagnosticsSubsystem::Gpu,
                            "gpu-sample-read",
                            "Read a GPU sample.",
                        )
                        .timestamp_millis(captured_at_millis)
                        .field("gpu_percent", sample.gpu_percent)
                        .field("ane_percent", sample.ane_percent)
                        .field("gpu_memory_bytes", sample.gpu_memory_bytes)
                        .build(),
                    );
                }
                gpu_sample_tick = gpu_sample_tick.wrapping_add(1);
                let (frontmost_app_state, capabilities) = {
                    let mut guard = state.lock();
                    refresh_adapter_capabilities(&mut guard, &adapters, captured_at_millis);
                    (
                        guard.frontmost_app_state.clone(),
                        guard.capabilities.clone(),
                    )
                };
                // Single HostSnapshot construction — passed to pipeline
                // (which runs identity + attribution + friction internally).
                let mut host = HostSnapshot {
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
                    ai_agent_friction: 0.0,
                    ai_agent_count: 0,
                    gpu_percent: gpu_sample.gpu_percent,
                    ane_percent: gpu_sample.ane_percent,
                    gpu_memory_bytes: gpu_sample.gpu_memory_bytes,
                };
                let pipeline_output =
                    run_entity_pipeline(&raw.processes, &host, frontmost_app_state.as_ref());
                let mut entities = pipeline_output.entities;
                adapters.enrich_entities(&mut entities, &capabilities);

                // Aggregate AI agent friction (mutate in place, no second host).
                let (ai_friction, ai_count) = entities
                    .iter()
                    .filter(|e| matches!(e.entity_kind, aetower_model::EntityKind::AiAgent))
                    .fold((0.0f32, 0u32), |(f, c), e| {
                        (f + e.friction.total_score, c + 1)
                    });
                host.ai_agent_friction = ai_friction;
                host.ai_agent_count = ai_count;

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
                // Persist snapshot (best-effort, throttled by write_interval).
                if let Some(store) = persistence.lock().as_mut() {
                    store.maybe_store(&guard.latest_snapshot);
                }
                let sequence = guard.latest_snapshot.sequence;
                let entity_count = guard.latest_snapshot.entities.len();
                diagnostics.emit(
                    DiagnosticsEvent::builder(
                        DiagnosticsLevel::Info,
                        DiagnosticsSubsystem::Engine,
                        "snapshot-published",
                        "Published a new system snapshot.",
                    )
                    .timestamp_millis(captured_at_millis)
                    .sequence(sequence)
                    .field("entity_count", entity_count)
                    .field("process_count", raw.processes.len())
                    .build(),
                );
                drop(guard);
                let tick_millis = tick_started.elapsed().as_millis();
                let is_over_budget = tick_millis > FAST_TICK.as_millis();
                let level = if is_over_budget {
                    DiagnosticsLevel::Warn
                } else {
                    DiagnosticsLevel::Debug
                };
                diagnostics.emit(
                    DiagnosticsEvent::builder(
                        level,
                        DiagnosticsSubsystem::Engine,
                        if is_over_budget {
                            "tick-over-budget"
                        } else {
                            "tick-completed"
                        },
                        if is_over_budget {
                            "Engine tick exceeded the fast-path budget."
                        } else {
                            "Engine tick completed."
                        },
                    )
                    .timestamp_millis(captured_at_millis)
                    .sequence(sequence)
                    .field("tick_millis", tick_millis)
                    .field("entity_count", entity_count)
                    .field("process_count", raw.processes.len())
                    .build(),
                );
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

        let state = Arc::clone(&self.state);
        let telemetry = Arc::clone(&self.telemetry);
        let running = Arc::clone(&self.running);
        self.telemetry_worker = Some(thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                let (enabled, interval_secs) = {
                    let exporter = telemetry.lock();
                    (
                        exporter.is_enabled(),
                        exporter.config().export_interval_secs.max(5),
                    )
                };

                if enabled {
                    let snapshot = state.lock().latest_snapshot.clone();
                    let _ = telemetry.lock().export(&snapshot);
                }

                let sleep_for = if enabled {
                    Duration::from_secs(interval_secs as u64)
                } else {
                    Duration::from_secs(2)
                };
                sleep_with_stop(&running, sleep_for);
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
        if let Some(worker) = self.telemetry_worker.take() {
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

    pub fn latest_diagnostics(&self, limit: usize) -> Vec<DiagnosticsEvent> {
        self.diagnostics.recent(limit)
    }

    pub fn diagnostics_overview(&self) -> DiagnosticsOverview {
        self.diagnostics.overview()
    }

    pub fn export_diagnostics_json(&self, limit: usize) -> String {
        self.diagnostics.export_json(limit)
    }

    pub fn record_diagnostics_event(&self, event: DiagnosticsEvent) {
        self.diagnostics.emit(event);
    }

    pub fn set_capability_state(
        &self,
        kind: CapabilityKind,
        state: CapabilityState,
        detail_override: Option<String>,
    ) {
        self.diagnostics.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "capability-state-changed",
                "Capability state updated.",
            )
            .capability(format!("{kind:?}"))
            .field("state", format!("{state:?}"))
            .build(),
        );
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

    pub fn configure_telemetry(
        &self,
        endpoint: Option<String>,
        enabled: bool,
        export_interval_secs: u32,
    ) {
        let mut guard = self.telemetry.lock();
        let mut config = guard.config().clone();
        if let Some(endpoint) = endpoint {
            config.endpoint = endpoint;
        }
        config.enabled = enabled;
        config.export_interval_secs = export_interval_secs.max(5);
        guard.update_config(config);
        self.diagnostics.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Telemetry,
                "telemetry-config-updated",
                "Telemetry configuration updated.",
            )
            .field("enabled", enabled)
            .field("export_interval_secs", export_interval_secs.max(5))
            .field("endpoint", guard.config().endpoint.clone())
            .build(),
        );
    }

    pub fn load_history_range(&self, start_millis: u64, end_millis: u64) -> Vec<SystemSnapshot> {
        self.persistence
            .lock()
            .as_ref()
            .and_then(|store| store.load_range(start_millis, end_millis).ok())
            .unwrap_or_default()
    }

    pub fn stop_agent_session(&self, session_id: String, force: bool) -> Result<(), String> {
        self.adapters.stop_chau7_session(&session_id, force)
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

fn telemetry_config_from_env() -> OtlpConfig {
    let mut config = OtlpConfig::default();
    if let Ok(endpoint) = std::env::var("AETOWER_OTLP_ENDPOINT") {
        config.endpoint = endpoint;
    }
    if let Ok(interval) = std::env::var("AETOWER_OTLP_INTERVAL_SECS")
        && let Ok(interval_secs) = interval.parse::<u32>()
    {
        config.export_interval_secs = interval_secs.max(5);
    }
    if let Ok(enabled) = std::env::var("AETOWER_OTLP_ENABLED") {
        config.enabled = matches!(enabled.as_str(), "1" | "true" | "TRUE" | "yes" | "YES");
    }
    config
}

fn sleep_with_stop(running: &AtomicBool, duration: Duration) {
    let started_at = Instant::now();
    while running.load(Ordering::SeqCst) && started_at.elapsed() < duration {
        let remaining = duration.saturating_sub(started_at.elapsed());
        thread::sleep(remaining.min(Duration::from_millis(250)));
    }
}
