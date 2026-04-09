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
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsOverview, DiagnosticsQuery, DiagnosticsStore,
    DiagnosticsSubsystem,
};
use aetower_model::{
    CapabilityKind, CapabilitySnapshot, CapabilityState, FrontmostAppState, HostSnapshot,
    HostTrend, RuntimeLagMetrics, SystemSnapshot, ThermalState,
};
use aetower_telemetry::{OtlpConfig, TelemetryExporter};
use aetower_time::{self as time, ADAPTER_TICK, FAST_TICK};
use parking_lot::Mutex;

use crate::{
    adapters::AdapterManager,
    collector::{Collector, CollectorConfig},
    history::History,
    run_entity_pipeline,
};

const ADAPTER_IDLE_SLEEP: Duration = Duration::from_secs(5);
const TELEMETRY_DISABLED_SLEEP: Duration = Duration::from_secs(30);
const RUNTIME_HEARTBEAT_INTERVAL_MILLIS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone)]
struct RuntimeCollectionConfig {
    full_collection: bool,
    adaptive_cadence: bool,
    active_tick: Duration,
    idle_tick: Duration,
    low_power_tick: Duration,
    gpu_sample_interval: Duration,
    gpu_sample_low_power_interval: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeCollectionSettings {
    pub full_collection: bool,
    pub adaptive_cadence: bool,
    pub active_tick_millis: u64,
    pub idle_tick_millis: u64,
    pub low_power_tick_millis: u64,
    pub gpu_sample_interval_millis: u64,
    pub gpu_sample_low_power_interval_millis: u64,
}

impl Default for RuntimeCollectionConfig {
    fn default() -> Self {
        Self {
            full_collection: false,
            adaptive_cadence: true,
            active_tick: FAST_TICK,
            idle_tick: Duration::from_secs(5),
            low_power_tick: Duration::from_secs(8),
            gpu_sample_interval: Duration::from_secs(30),
            gpu_sample_low_power_interval: Duration::from_secs(60),
        }
    }
}

impl RuntimeCollectionConfig {
    fn collector_config(&self) -> CollectorConfig {
        CollectorConfig {
            full_collection: self.full_collection,
        }
    }

    fn target_tick(
        &self,
        host: &HostSnapshot,
        entities: &[aetower_model::EntitySnapshot],
    ) -> Duration {
        if !self.adaptive_cadence {
            return self.active_tick;
        }
        if host.on_battery || host.low_power_mode {
            return self.low_power_tick;
        }
        let has_anomaly = entities.iter().any(|entity| entity.anomaly_detected);
        let hot_entity = entities
            .first()
            .map(|entity| entity.friction.total_score >= 45.0)
            .unwrap_or(false);
        if has_anomaly || hot_entity || host.thermal_state >= ThermalState::Serious {
            self.active_tick
        } else {
            self.idle_tick
        }
    }

    fn gpu_interval(&self, host: &HostSnapshot) -> Duration {
        if host.on_battery || host.low_power_mode {
            self.gpu_sample_low_power_interval
        } else {
            self.gpu_sample_interval
        }
    }
}

struct EngineState {
    sequence: u64,
    latest_snapshot: SystemSnapshot,
    capabilities: BTreeMap<CapabilityKind, CapabilitySnapshot>,
    frontmost_app_state: Option<FrontmostAppState>,
    history: History,
    runtime_lag_metrics: RuntimeLagMetrics,
    runtime_config: RuntimeCollectionConfig,
    last_runtime_heartbeat_millis: u64,
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
                runtime_lag_metrics: RuntimeLagMetrics::default(),
                runtime_config: RuntimeCollectionConfig::default(),
                last_runtime_heartbeat_millis: 0,
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
            let mut gpu_sample = aetower_gpu::GpuSample::default();
            let mut last_gpu_sample_started_at = Instant::now()
                .checked_sub(Duration::from_secs(3600))
                .unwrap_or_else(Instant::now);

            while running.load(Ordering::SeqCst) {
                let tick_started = Instant::now();
                let captured_at_millis = time::now_millis();
                let runtime_config = {
                    let guard = state.lock();
                    guard.runtime_config.clone()
                };
                collector.configure(runtime_config.collector_config());
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
                let collect_started = Instant::now();
                let raw = collector.collect();
                let collect_millis = collect_started.elapsed().as_secs_f64() * 1000.0;
                let (frontmost_app_state, capabilities, runtime_lag_metrics) = {
                    let mut guard = state.lock();
                    refresh_adapter_capabilities(&mut guard, &adapters, captured_at_millis);
                    (
                        guard.frontmost_app_state.clone(),
                        guard.capabilities.clone(),
                        guard.runtime_lag_metrics.clone(),
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
                    gpu_temperature_celsius: None,
                    fans: Vec::new(),
                    cpu_temperatures: Vec::new(),
                    power_readings: Vec::new(),
                };
                let gpu_interval = runtime_config.gpu_interval(&host);
                let mut gpu_sample_millis = 0.0;
                if last_gpu_sample_started_at.elapsed() >= gpu_interval {
                    let gpu_started = Instant::now();
                    if let Some(sample) = aetower_gpu::sample_gpu() {
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
                    gpu_sample_millis = gpu_started.elapsed().as_secs_f64() * 1000.0;
                    last_gpu_sample_started_at = Instant::now();
                    host.gpu_percent = gpu_sample.gpu_percent;
                    host.ane_percent = gpu_sample.ane_percent;
                    host.gpu_memory_bytes = gpu_sample.gpu_memory_bytes;
                    host.gpu_temperature_celsius = gpu_sample.gpu_temperature_celsius;

                    // Sample hardware sensors (fans, temperatures, power) on same interval
                    if let Some(sensor_sample) = aetower_sensors::sample_sensors() {
                        host.fans = sensor_sample.fans;
                        host.cpu_temperatures = sensor_sample.cpu_temperatures;
                        host.power_readings = sensor_sample.power_readings;
                    }
                }
                let pipeline_output =
                    run_entity_pipeline(&raw.processes, &host, frontmost_app_state.as_ref());
                let crate::pipeline::EntityPipelineOutput {
                    identity: _identity,
                    mut entities,
                    timings: pipeline_timings,
                } = pipeline_output;
                let enrich_started = Instant::now();
                adapters.enrich_entities(&mut entities, &capabilities);
                let enrich_millis = enrich_started.elapsed().as_secs_f64() * 1000.0;

                // Aggregate AI agent friction (mutate in place, no second host).
                let (ai_friction, ai_count) = entities
                    .iter()
                    .filter(|e| matches!(e.entity_kind, aetower_model::EntityKind::AiAgent))
                    .fold((0.0f32, 0u32), |(f, c), e| {
                        (f + e.friction.total_score, c + 1)
                    });
                host.ai_agent_friction = ai_friction;
                host.ai_agent_count = ai_count;

                let history_started = Instant::now();
                let mut guard = state.lock();
                let mut runtime_lag_metrics = runtime_lag_metrics;
                runtime_lag_metrics.updated_at_millis = captured_at_millis;
                runtime_lag_metrics.collect_millis = collect_millis as f32;
                runtime_lag_metrics.identity_millis = pipeline_timings.identity_millis as f32;
                runtime_lag_metrics.attribution_millis = pipeline_timings.attribution_millis as f32;
                runtime_lag_metrics.friction_millis = pipeline_timings.friction_millis as f32;
                runtime_lag_metrics.enrich_millis = enrich_millis as f32;
                let (timeline, host_trend) =
                    guard
                        .history
                        .update(captured_at_millis, &host, &mut entities);
                let history_millis = history_started.elapsed().as_secs_f64() * 1000.0;
                runtime_lag_metrics.history_millis = history_millis as f32;
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
                let persist_started = Instant::now();
                let history_queue_depth = persistence
                    .lock()
                    .as_ref()
                    .map(|store| store.pending_writes())
                    .unwrap_or(0);
                if let Some(store) = persistence.lock().as_mut() {
                    store.maybe_store(&guard.latest_snapshot);
                }
                let persist_millis = persist_started.elapsed().as_secs_f64() * 1000.0;
                runtime_lag_metrics.persist_millis = persist_millis as f32;
                runtime_lag_metrics.gpu_sample_millis = gpu_sample_millis as f32;
                runtime_lag_metrics.history_queue_depth =
                    history_queue_depth.min(u32::MAX as u64) as u32;
                runtime_lag_metrics.diagnostics_queue_depth =
                    diagnostics.pending_writes().min(u32::MAX as u64) as u32;
                let sequence = guard.latest_snapshot.sequence;
                let entity_count = guard.latest_snapshot.entities.len();
                let target_tick = runtime_config
                    .target_tick(&guard.latest_snapshot.host, &guard.latest_snapshot.entities);
                runtime_lag_metrics.target_tick_millis = target_tick.as_millis() as f32;
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
                    .field("collect_millis", format!("{collect_millis:.3}"))
                    .field(
                        "identity_millis",
                        format!("{:.3}", pipeline_timings.identity_millis),
                    )
                    .field(
                        "attribution_millis",
                        format!("{:.3}", pipeline_timings.attribution_millis),
                    )
                    .field(
                        "friction_millis",
                        format!("{:.3}", pipeline_timings.friction_millis),
                    )
                    .field("enrich_millis", format!("{enrich_millis:.3}"))
                    .field("history_millis", format!("{history_millis:.3}"))
                    .field("persist_millis", format!("{persist_millis:.3}"))
                    .build(),
                );
                let tick_millis = tick_started.elapsed().as_millis();
                runtime_lag_metrics.engine_tick_millis = tick_millis as f32;
                if should_emit_runtime_heartbeat(
                    guard.last_runtime_heartbeat_millis,
                    captured_at_millis,
                ) {
                    let top_entity = guard.latest_snapshot.entities.first();
                    diagnostics.emit(
                        DiagnosticsEvent::builder(
                            DiagnosticsLevel::Info,
                            DiagnosticsSubsystem::Engine,
                            "runtime-heartbeat",
                            "Recorded a low-frequency runtime heartbeat.",
                        )
                        .timestamp_millis(captured_at_millis)
                        .sequence(sequence)
                        .field("entity_count", entity_count)
                        .field("process_count", raw.processes.len())
                        .field(
                            "top_entity",
                            top_entity
                                .map(|entity| entity.display_name.as_str())
                                .unwrap_or("none"),
                        )
                        .field(
                            "top_friction",
                            format!(
                                "{:.1}",
                                top_entity
                                    .map(|entity| entity.friction.total_score)
                                    .unwrap_or(0.0)
                            ),
                        )
                        .field("target_tick_millis", target_tick.as_millis())
                        .field("tick_millis", tick_millis)
                        .field("collect_millis", format!("{collect_millis:.3}"))
                        .field("history_queue_depth", history_queue_depth)
                        .field(
                            "diagnostics_queue_depth",
                            diagnostics.pending_writes().min(u32::MAX as u64),
                        )
                        .build(),
                    );
                    guard.last_runtime_heartbeat_millis = captured_at_millis;
                }
                guard.runtime_lag_metrics = runtime_lag_metrics;
                drop(guard);
                let is_over_budget = tick_millis > target_tick.as_millis();
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
                    .field("collect_millis", format!("{collect_millis:.3}"))
                    .field(
                        "identity_millis",
                        format!("{:.3}", pipeline_timings.identity_millis),
                    )
                    .field(
                        "attribution_millis",
                        format!("{:.3}", pipeline_timings.attribution_millis),
                    )
                    .field(
                        "friction_millis",
                        format!("{:.3}", pipeline_timings.friction_millis),
                    )
                    .field("enrich_millis", format!("{enrich_millis:.3}"))
                    .field("history_millis", format!("{history_millis:.3}"))
                    .field("persist_millis", format!("{persist_millis:.3}"))
                    .build(),
                );
                let elapsed = tick_started.elapsed();
                if elapsed < target_tick {
                    sleep_with_stop(&running, target_tick - elapsed);
                }
            }
        }));

        let state = Arc::clone(&self.state);
        let adapters = self.adapters.clone();
        let running = Arc::clone(&self.running);
        self.adapter_worker = Some(thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                let capabilities = {
                    let guard = state.lock();
                    guard.capabilities.clone()
                };
                let has_live_adapter = capabilities.values().any(|capability| {
                    capability.state == CapabilityState::Granted
                        && matches!(
                            capability.kind,
                            CapabilityKind::ChromiumDebug
                                | CapabilityKind::DockerSocket
                                | CapabilityKind::PrivilegedHelper
                                | CapabilityKind::EndpointSecurity
                                | CapabilityKind::Chau7
                        )
                });
                if !has_live_adapter {
                    sleep_with_stop(&running, ADAPTER_IDLE_SLEEP);
                    continue;
                }
                adapters.refresh_caches(&capabilities);
                sleep_with_stop(&running, ADAPTER_TICK);
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
                    let (snapshot, lag_metrics) = {
                        let guard = state.lock();
                        (
                            guard.latest_snapshot.clone(),
                            guard.runtime_lag_metrics.clone(),
                        )
                    };
                    let _ = telemetry.lock().export(&snapshot, &lag_metrics);
                }

                let sleep_for = if enabled {
                    Duration::from_secs(interval_secs as u64)
                } else {
                    TELEMETRY_DISABLED_SLEEP
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

    pub fn latest_runtime_lag_metrics(&self) -> RuntimeLagMetrics {
        self.state.lock().runtime_lag_metrics.clone()
    }

    pub fn latest_diagnostics(&self, limit: usize) -> Vec<DiagnosticsEvent> {
        self.diagnostics.recent(limit)
    }

    pub fn query_diagnostics(&self, query: DiagnosticsQuery) -> Vec<DiagnosticsEvent> {
        self.diagnostics.query(&query)
    }

    pub fn diagnostics_overview(&self) -> DiagnosticsOverview {
        self.diagnostics.overview()
    }

    pub fn export_diagnostics_json(&self, limit: usize) -> String {
        self.diagnostics.export_json(limit)
    }

    pub fn export_diagnostics_query_json(&self, query: DiagnosticsQuery) -> String {
        self.diagnostics.query_json(&query)
    }

    pub fn clear_diagnostics(&self) -> Result<(), String> {
        self.diagnostics
            .clear()
            .map_err(|error| format!("clear diagnostics: {error}"))
    }

    pub fn clear_history(&self) -> Result<(), String> {
        match self.persistence.lock().as_ref() {
            Some(store) => store.clear_all(),
            None => Ok(()),
        }
    }

    pub fn record_diagnostics_event(&self, event: DiagnosticsEvent) {
        self.diagnostics.emit(event);
    }

    pub fn update_ui_lag_metrics(&self, metrics: RuntimeLagMetrics) {
        let mut guard = self.state.lock();
        guard.runtime_lag_metrics.updated_at_millis = metrics.updated_at_millis;
        guard.runtime_lag_metrics.bridge_fetch_millis = metrics.bridge_fetch_millis;
        guard.runtime_lag_metrics.ui_refresh_millis = metrics.ui_refresh_millis;
        guard.runtime_lag_metrics.snapshot_to_ui_millis = metrics.snapshot_to_ui_millis;
        guard.runtime_lag_metrics.snapshot_to_render_millis = metrics.snapshot_to_render_millis;
        guard.runtime_lag_metrics.render_commit_millis = metrics.render_commit_millis;
        guard.runtime_lag_metrics.display_frame_interval_millis =
            metrics.display_frame_interval_millis;
        guard.runtime_lag_metrics.display_refresh_hz = metrics.display_refresh_hz;
        guard.runtime_lag_metrics.display_dropped_frames = metrics.display_dropped_frames;
        guard.runtime_lag_metrics.input_avg_latency_millis = metrics.input_avg_latency_millis;
        guard.runtime_lag_metrics.input_max_latency_millis = metrics.input_max_latency_millis;
        guard.runtime_lag_metrics.input_sample_count = metrics.input_sample_count;
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

    pub fn configure_runtime_collection(&self, settings: RuntimeCollectionSettings) {
        let mut guard = self.state.lock();
        guard.runtime_config = RuntimeCollectionConfig {
            full_collection: settings.full_collection,
            adaptive_cadence: settings.adaptive_cadence,
            active_tick: Duration::from_millis(settings.active_tick_millis.max(500)),
            idle_tick: Duration::from_millis(
                settings
                    .idle_tick_millis
                    .max(settings.active_tick_millis.max(500)),
            ),
            low_power_tick: Duration::from_millis(
                settings
                    .low_power_tick_millis
                    .max(settings.active_tick_millis.max(500)),
            ),
            gpu_sample_interval: Duration::from_millis(
                settings.gpu_sample_interval_millis.max(5_000),
            ),
            gpu_sample_low_power_interval: Duration::from_millis(
                settings
                    .gpu_sample_low_power_interval_millis
                    .max(settings.gpu_sample_interval_millis.max(5_000)),
            ),
        };
        drop(guard);
        self.diagnostics.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Collector,
                "runtime-collection-config-updated",
                "Runtime collection policy updated.",
            )
            .field("full_collection", settings.full_collection)
            .field("adaptive_cadence", settings.adaptive_cadence)
            .field("active_tick_millis", settings.active_tick_millis.max(500))
            .field(
                "idle_tick_millis",
                settings
                    .idle_tick_millis
                    .max(settings.active_tick_millis.max(500)),
            )
            .field(
                "low_power_tick_millis",
                settings
                    .low_power_tick_millis
                    .max(settings.active_tick_millis.max(500)),
            )
            .field(
                "gpu_sample_interval_millis",
                settings.gpu_sample_interval_millis.max(5_000),
            )
            .field(
                "gpu_sample_low_power_interval_millis",
                settings
                    .gpu_sample_low_power_interval_millis
                    .max(settings.gpu_sample_interval_millis.max(5_000)),
            )
            .build(),
        );
    }

    pub fn verify_telemetry_export(&self) -> Result<(), String> {
        let (snapshot, lag_metrics) = {
            let guard = self.state.lock();
            (
                guard.latest_snapshot.clone(),
                guard.runtime_lag_metrics.clone(),
            )
        };
        self.telemetry.lock().verify_export(&snapshot, &lag_metrics)
    }

    pub fn load_history_range(&self, start_millis: u64, end_millis: u64) -> Vec<SystemSnapshot> {
        self.persistence
            .lock()
            .as_ref()
            .and_then(|store| store.load_range(start_millis, end_millis).ok())
            .unwrap_or_default()
    }

    pub fn load_history_page(
        &self,
        start_millis: u64,
        end_millis: u64,
        before_millis_exclusive: Option<u64>,
        limit: u32,
    ) -> Vec<SystemSnapshot> {
        self.persistence
            .lock()
            .as_ref()
            .and_then(|store| {
                store
                    .load_range_page(start_millis, end_millis, before_millis_exclusive, limit)
                    .ok()
            })
            .unwrap_or_default()
    }

    pub fn history_range_summary(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Option<aetower_persistence::HistoryRangeSummary> {
        self.persistence
            .lock()
            .as_ref()
            .and_then(|store| store.range_summary(start_millis, end_millis).ok())
    }

    pub fn maintain_history_store(
        &self,
        aggressive: bool,
    ) -> Option<aetower_persistence::HistoryMaintenanceReport> {
        self.persistence
            .lock()
            .as_ref()
            .and_then(|store| store.maintain_storage(aggressive).ok())
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

fn should_emit_runtime_heartbeat(last_heartbeat_millis: u64, captured_at_millis: u64) -> bool {
    last_heartbeat_millis == 0
        || captured_at_millis.saturating_sub(last_heartbeat_millis)
            >= RUNTIME_HEARTBEAT_INTERVAL_MILLIS
}
