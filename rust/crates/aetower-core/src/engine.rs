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
use aetower_time::{self as aet_time, ADAPTER_TICK, FAST_TICK};
use parking_lot::Mutex;
use time::{OffsetDateTime, macros::format_description};

use crate::{
    adapters::AdapterManager,
    collector::{Collector, CollectorConfig},
    history::History,
    run_entity_pipeline,
};

const ADAPTER_IDLE_SLEEP: Duration = Duration::from_secs(5);
const TELEMETRY_DISABLED_SLEEP: Duration = Duration::from_secs(30);
const RUNTIME_HEARTBEAT_INTERVAL_MILLIS: u64 = 10 * 60 * 1000;
const DEFAULT_HISTORY_RETENTION_MILLIS: u64 = 24 * 60 * 60 * 1000;
const EMERGENCY_HISTORY_RETENTION_MILLIS: u64 = 6 * 60 * 60 * 1000;
const HISTORY_SOFT_MAX_BYTES: u64 = 1024 * 1024 * 1024;
const HISTORY_HARD_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const HISTORY_MAX_WAL_BYTES: u64 = 64 * 1024 * 1024;
const HISTORY_SOFT_MAX_SNAPSHOT_COUNT: u64 = 8_000;
const HISTORY_HARD_MAX_SNAPSHOT_COUNT: u64 = 12_000;
const HISTORY_AGGRESSIVE_QUARANTINE_ROWS: u64 = 64;
const HISTORY_HARD_MAX_QUARANTINE_ROWS: u64 = 128;
const SYSTEM_MARKER_LOOKBACK_MILLIS: u64 = 12 * 60 * 60 * 1000;
const SYSTEM_MARKER_PREDICATE: &str = "(eventMessage CONTAINS[c] \"Previous shutdown cause\") OR (eventMessage CONTAINS[c] \"Entering Sleep state\") OR (eventMessage CONTAINS[c] \"Wake reason\") OR (eventMessage CONTAINS[c] \"Wake from\") OR (eventMessage CONTAINS[c] \"panic(cpu\") OR (eventMessage CONTAINS[c] \"userspace watchdog timeout\") OR (eventMessage CONTAINS[c] \"thermal pressure\") OR (eventMessage CONTAINS[c] \"low power mode\")";
const HOST_INCIDENT_PERSIST_INTERVAL_MILLIS: u64 = 15 * 60 * 1000;
const MEMORY_PRESSURE_WARNING_RATIO: f64 = 0.80;
const MEMORY_PRESSURE_CRITICAL_RATIO: f64 = 0.90;
const COMPRESSED_MEMORY_WARNING_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const COMPRESSED_MEMORY_CRITICAL_BYTES: u64 = 6 * 1024 * 1024 * 1024;
const SWAP_WARNING_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const SWAP_CRITICAL_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const WAKEUPS_WARNING: f32 = 12_000.0;
const WAKEUPS_CRITICAL: f32 = 25_000.0;

const UNIFIED_LOG_TIMESTAMP_FORMAT: &[::time::format_description::FormatItem<'static>] = format_description!(
    "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond][offset_hour sign:mandatory][offset_minute]"
);

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

/// Per-capability availability tracker used to emit DiagnosticsEvent
/// on transitions between "collecting" and "not collecting".
///
/// `None` is the seeded pre-first-tick state: no transition event
/// fires on the very first observation, only on subsequent changes.
/// This avoids spurious "disk readings unavailable" events at startup
/// when the collector has not yet run its first slow-cadence refresh.
///
/// The previous implementation of the sensor sampling paths returned
/// an empty `Vec` on failure with no log, no diagnostic event, and no
/// way for an operator to tell "genuinely no devices" (Mac mini, no
/// battery) from "collection tool broke silently" (SMC handle lost,
/// `ioreg` missing, `diskutil` renamed its keys). The tracker closes
/// that gap by surfacing one clear transition event per capability.
#[derive(Default)]
struct SensorAvailabilityState {
    fans_available: SensorCapabilityState,
    cpu_temperatures_available: SensorCapabilityState,
    power_readings_available: SensorCapabilityState,
    disks_available: SensorCapabilityState,
    bluetooth_available: SensorCapabilityState,
}

#[derive(Default)]
struct SensorCapabilityState {
    last_available: Option<bool>,
    consecutive_available: u8,
    consecutive_unavailable: u8,
    has_reported_unavailable: bool,
}

#[derive(Debug, Clone)]
struct SystemMarker {
    timestamp_millis: u64,
    level: DiagnosticsLevel,
    event_type: &'static str,
    message: &'static str,
    detail: String,
    category: &'static str,
}

#[derive(Debug, Clone)]
struct PersistedHostIncidentState {
    severity: DiagnosticsLevel,
    last_emitted_millis: u64,
}

#[derive(Debug, Clone)]
struct HostIncidentSnapshot {
    key: &'static str,
    severity: DiagnosticsLevel,
    title: &'static str,
    detail: String,
    fields: Vec<(&'static str, String)>,
}

#[derive(Debug, serde::Deserialize)]
struct UnifiedLogEntry {
    #[serde(default, rename = "timestamp")]
    timestamp: Option<String>,
    #[serde(default, rename = "eventMessage")]
    event_message: Option<String>,
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
    system_marker_worker: Option<JoinHandle<()>>,
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
            captured_at_millis: aet_time::now_millis(),
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
            ai_repo_summaries: Vec::new(),
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
            let _ = store.maintain_with_policy(default_history_retention_policy());
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
            system_marker_worker: None,
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
            // Availability tracking: one bool per sensor capability so we
            // can emit a DiagnosticsEvent on the transition between
            // "collecting" and "not collecting". `None` is the seeded
            // pre-first-tick state — no event fires on the very first
            // observation, only on subsequent transitions.
            let mut sensor_availability = SensorAvailabilityState::default();
            let mut last_boot_session_key: Option<String> = None;
            let mut host_incident_state =
                BTreeMap::<&'static str, PersistedHostIncidentState>::new();

            while running.load(Ordering::SeqCst) {
                let tick_started = Instant::now();
                let captured_at_millis = aet_time::now_millis();
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
                    battery_health: raw.host.battery_health.clone(),
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
                    boot_session: raw.host.boot_session.clone(),
                    bluetooth_devices: raw.host.bluetooth_devices.clone(),
                    network_interfaces: raw.host.network_interfaces.clone(),
                    disks: raw.host.disks.clone(),
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

                emit_sensor_availability_transitions(
                    &diagnostics,
                    &mut sensor_availability,
                    &host,
                    captured_at_millis,
                );
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

                // Heuristic GPU attribution: macOS has no per-process GPU
                // API, so distribute the host-level `gpu_percent` among AI
                // agent entities proportional to their CPU share. When
                // exactly one agent is running it gets 100% of the host
                // GPU; when several compete, each gets its proportional
                // slice. Non-AI entities keep `estimated_gpu_percent = 0`.
                if host.gpu_percent > 0.0 && ai_count > 0 {
                    let total_ai_cpu: f32 = entities
                        .iter()
                        .filter(|e| matches!(e.entity_kind, aetower_model::EntityKind::AiAgent))
                        .map(|e| e.metrics.cpu_percent)
                        .sum();
                    for entity in entities
                        .iter_mut()
                        .filter(|e| matches!(e.entity_kind, aetower_model::EntityKind::AiAgent))
                    {
                        entity.metrics.estimated_gpu_percent = if total_ai_cpu > 0.0 {
                            // Distribute proportional to CPU share when agents
                            // are doing mixed CPU+GPU work.
                            (entity.metrics.cpu_percent / total_ai_cpu) * host.gpu_percent
                        } else {
                            // Fallback: agents are GPU-active but CPU-idle
                            // (the typical Metal-compute inference pattern).
                            // Distribute equally since we have no better signal.
                            host.gpu_percent / ai_count as f32
                        };
                    }
                }

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
                    ai_repo_summaries: adapters.ai_repo_summaries(),
                };
                emit_boot_session_observed(
                    &diagnostics,
                    &guard.latest_snapshot,
                    &mut last_boot_session_key,
                );
                emit_host_incident_snapshots(
                    &diagnostics,
                    &guard.latest_snapshot,
                    &mut host_incident_state,
                );
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
                    if let Some(store) = persistence.lock().as_ref() {
                        let _ = store.maintain_with_policy(default_history_retention_policy());
                    }
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

        let diagnostics = self.diagnostics.clone();
        self.system_marker_worker = Some(thread::spawn(move || {
            let since_millis =
                last_persisted_system_marker_millis(&diagnostics).unwrap_or_else(|| {
                    aet_time::now_millis().saturating_sub(SYSTEM_MARKER_LOOKBACK_MILLIS)
                });
            ingest_recent_system_markers(&diagnostics, since_millis);
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
        // System-marker ingestion is best-effort and can be slow on machines
        // with large unified-log stores. Never block engine shutdown on it.
        let _ = self.system_marker_worker.take();
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
        let now = aet_time::now_millis();
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

    /// Pin a fan to a manual minimum RPM. Delegates to the privileged helper,
    /// which must already be configured via `configure_privileged_helper`.
    ///
    /// The UI layer is expected to validate `rpm` against the fan's reported
    /// `min_rpm`/`max_rpm` *before* calling this — the engine trusts the
    /// caller for bounds checking and only surfaces helper-level failures.
    pub fn set_fan_min_rpm(&self, fan_id: u8, rpm: f32) -> Result<(), String> {
        self.adapters.set_fan_min_rpm(fan_id, rpm)
    }

    /// Restore a fan to automatic (OS-controlled) mode.
    pub fn reset_fan_auto(&self, fan_id: u8) -> Result<(), String> {
        self.adapters.reset_fan_auto(fan_id)
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

    pub fn try_load_history_page(
        &self,
        start_millis: u64,
        end_millis: u64,
        before_millis_exclusive: Option<u64>,
        limit: u32,
    ) -> Result<Vec<SystemSnapshot>, String> {
        let persistence = self.persistence.lock();
        let store = persistence
            .as_ref()
            .ok_or_else(|| "persisted history store is not open".to_string())?;
        store.load_range_page(start_millis, end_millis, before_millis_exclusive, limit)
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

    pub fn try_history_range_summary(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<aetower_persistence::HistoryRangeSummary, String> {
        let persistence = self.persistence.lock();
        let store = persistence
            .as_ref()
            .ok_or_else(|| "persisted history store is not open".to_string())?;
        store.range_summary(start_millis, end_millis)
    }

    pub fn maintain_history_store(
        &self,
        aggressive: bool,
    ) -> Option<aetower_persistence::HistoryMaintenanceReport> {
        self.persistence.lock().as_ref().and_then(|store| {
            if aggressive {
                store.maintain_storage(true).ok()
            } else {
                store
                    .maintain_with_policy(default_history_retention_policy())
                    .ok()
            }
        })
    }

    pub fn stop_agent_session(&self, session_id: String, force: bool) -> Result<(), String> {
        self.adapters.stop_chau7_session(&session_id, force)
    }

    fn refresh_capability(&self, kind: CapabilityKind) {
        let mut guard = self.state.lock();
        guard.capabilities.insert(
            kind.clone(),
            self.adapters
                .capability_snapshot(kind, aet_time::now_millis()),
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

fn default_history_retention_policy() -> aetower_persistence::HistoryRetentionPolicy {
    aetower_persistence::HistoryRetentionPolicy {
        max_age_millis: DEFAULT_HISTORY_RETENTION_MILLIS,
        emergency_max_age_millis: EMERGENCY_HISTORY_RETENTION_MILLIS,
        soft_max_store_bytes: HISTORY_SOFT_MAX_BYTES,
        hard_max_store_bytes: HISTORY_HARD_MAX_BYTES,
        max_wal_bytes: HISTORY_MAX_WAL_BYTES,
        soft_max_snapshot_count: HISTORY_SOFT_MAX_SNAPSHOT_COUNT,
        hard_max_snapshot_count: HISTORY_HARD_MAX_SNAPSHOT_COUNT,
        aggressive_quarantine_rows: HISTORY_AGGRESSIVE_QUARANTINE_ROWS,
        hard_max_quarantine_rows: HISTORY_HARD_MAX_QUARANTINE_ROWS,
    }
}

/// Emit a DiagnosticsEvent whenever a sensor capability transitions
/// between "collecting readings" and "not collecting".
///
/// Each capability is tracked independently so that a Mac mini with no
/// battery and no fans still surfaces useful events for its disks
/// (available) and Bluetooth peripherals (available when paired).
///
/// The transition detector deliberately does NOT emit on the very first
/// observation: at startup the caches inside the collector have not yet
/// run their slow-cadence refresh cycles, so the first tick always sees
/// empty `disks` and `bluetooth_devices` vectors. Emitting "disks
/// unavailable" immediately would be a false positive every time the app
/// launches.
fn emit_sensor_availability_transitions(
    diagnostics: &DiagnosticsStore,
    state: &mut SensorAvailabilityState,
    host: &HostSnapshot,
    captured_at_millis: u64,
) {
    check_capability_transition(
        diagnostics,
        &mut state.fans_available,
        !host.fans.is_empty(),
        "sensor-fans",
        "Fan readings",
        captured_at_millis,
    );
    check_capability_transition(
        diagnostics,
        &mut state.cpu_temperatures_available,
        !host.cpu_temperatures.is_empty(),
        "sensor-cpu-temperatures",
        "CPU temperature readings",
        captured_at_millis,
    );
    check_capability_transition(
        diagnostics,
        &mut state.power_readings_available,
        !host.power_readings.is_empty(),
        "sensor-power-readings",
        "Power readings",
        captured_at_millis,
    );
    check_capability_transition(
        diagnostics,
        &mut state.disks_available,
        !host.disks.is_empty(),
        "sensor-disks",
        "Disk SMART readings",
        captured_at_millis,
    );
    check_capability_transition(
        diagnostics,
        &mut state.bluetooth_available,
        !host.bluetooth_devices.is_empty(),
        "sensor-bluetooth",
        "Bluetooth peripheral readings",
        captured_at_millis,
    );
}

fn check_capability_transition(
    diagnostics: &DiagnosticsStore,
    previous: &mut SensorCapabilityState,
    currently_available: bool,
    event_stem: &str,
    capability_label: &str,
    captured_at_millis: u64,
) {
    if currently_available {
        previous.consecutive_available = previous.consecutive_available.saturating_add(1);
        previous.consecutive_unavailable = 0;
        let was_unavailable = previous.last_available == Some(false);
        previous.last_available = Some(true);
        if was_unavailable
            && previous.has_reported_unavailable
            && previous.consecutive_available >= 2
        {
            previous.has_reported_unavailable = false;
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Engine,
                    format!("{event_stem}-recovered"),
                    format!("{capability_label} are available again."),
                )
                .timestamp_millis(captured_at_millis)
                .build(),
            );
        }
        return;
    }

    previous.consecutive_unavailable = previous.consecutive_unavailable.saturating_add(1);
    previous.consecutive_available = 0;
    let was_available = previous.last_available;
    previous.last_available = Some(false);
    if previous.consecutive_unavailable < 3 || previous.has_reported_unavailable {
        return;
    }

    previous.has_reported_unavailable = true;
    if was_available == Some(true) {
        diagnostics.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::Engine,
                format!("{event_stem}-unavailable"),
                format!(
                    "{capability_label} disappeared after previously being collected. The underlying sensor path likely regressed."
                ),
            )
            .timestamp_millis(captured_at_millis)
            .build(),
        );
    } else {
        diagnostics.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                format!("{event_stem}-unavailable"),
                format!(
                    "{capability_label} are unavailable on this system. Aetower is treating this as a stable unsupported capability instead of an active fault."
                ),
            )
            .timestamp_millis(captured_at_millis)
            .build(),
        );
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

fn emit_boot_session_observed(
    diagnostics: &DiagnosticsStore,
    snapshot: &SystemSnapshot,
    last_boot_session_key: &mut Option<String>,
) {
    let Some(boot_session) = snapshot.host.boot_session.as_ref() else {
        return;
    };
    let current_key = boot_session_key(boot_session);
    if current_key.is_none() || *last_boot_session_key == current_key {
        return;
    }
    let mut event = DiagnosticsEvent::builder(
        DiagnosticsLevel::Info,
        DiagnosticsSubsystem::Engine,
        "boot-session-observed",
        "Observed the active boot session in the latest host snapshot.",
    )
    .timestamp_millis(snapshot.captured_at_millis)
    .sequence(snapshot.sequence);
    if let Some(boot_id) = boot_session.boot_id.as_deref() {
        event = event.field("boot_id", boot_id);
    }
    if let Some(boot_time_millis) = boot_session.boot_time_millis {
        event = event.field("boot_time_millis", boot_time_millis);
    }
    if let Some(host_uptime_millis) = boot_session.host_uptime_millis {
        event = event.field("host_uptime_millis", host_uptime_millis);
    }
    if let Some(previous_shutdown) = boot_session.previous_shutdown.as_ref() {
        event = event
            .field("previous_shutdown_source", &previous_shutdown.source)
            .field(
                "previous_shutdown_code",
                previous_shutdown.code.as_deref().unwrap_or("unknown"),
            )
            .field("previous_shutdown_detail", &previous_shutdown.detail);
    }
    diagnostics.emit(event.build());
    *last_boot_session_key = current_key;
}

fn emit_host_incident_snapshots(
    diagnostics: &DiagnosticsStore,
    snapshot: &SystemSnapshot,
    state: &mut BTreeMap<&'static str, PersistedHostIncidentState>,
) {
    let incidents = collect_host_incidents(snapshot);
    let captured_at_millis = snapshot.captured_at_millis;
    let active_keys = incidents
        .iter()
        .map(|incident| incident.key)
        .collect::<Vec<_>>();
    state.retain(|key, _| active_keys.iter().any(|active| active == key));

    for incident in incidents {
        let should_emit = match state.get(incident.key) {
            Some(previous) => {
                previous.severity != incident.severity
                    || captured_at_millis.saturating_sub(previous.last_emitted_millis)
                        >= HOST_INCIDENT_PERSIST_INTERVAL_MILLIS
            }
            None => true,
        };
        if !should_emit {
            continue;
        }

        let mut event = DiagnosticsEvent::builder(
            incident.severity.clone(),
            DiagnosticsSubsystem::History,
            "host-incident-snapshot",
            incident.title,
        )
        .timestamp_millis(captured_at_millis)
        .sequence(snapshot.sequence)
        .field("incident_key", incident.key)
        .field(
            "severity",
            match incident.severity {
                DiagnosticsLevel::Warn => "warning",
                DiagnosticsLevel::Error => "critical",
                _ => "info",
            },
        )
        .field("detail", &incident.detail);
        if let Some(boot_session) = snapshot.host.boot_session.as_ref()
            && let Some(boot_id) = boot_session.boot_id.as_deref()
        {
            event = event.field("boot_id", boot_id);
        }
        let top_entity_ids = snapshot
            .entities
            .iter()
            .take(3)
            .map(|entity| entity.entity_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let top_entity_names = snapshot
            .entities
            .iter()
            .take(3)
            .map(|entity| entity.display_name.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        if !top_entity_ids.is_empty() {
            event = event.field("top_entity_ids", top_entity_ids);
        }
        if !top_entity_names.is_empty() {
            event = event.field("top_entity_names", top_entity_names);
        }
        for (key, value) in incident.fields {
            event = event.field(key, value);
        }
        diagnostics.emit(event.build());
        state.insert(
            incident.key,
            PersistedHostIncidentState {
                severity: incident.severity,
                last_emitted_millis: captured_at_millis,
            },
        );
    }
}

fn collect_host_incidents(snapshot: &SystemSnapshot) -> Vec<HostIncidentSnapshot> {
    let mut incidents = Vec::new();
    let host = &snapshot.host;
    let memory_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.memory_used_bytes as f64 / host.memory_total_bytes as f64
    };
    let memory_severity = if memory_ratio >= MEMORY_PRESSURE_CRITICAL_RATIO
        || host.swap_used_bytes >= SWAP_CRITICAL_BYTES
        || host.compressed_memory_bytes >= COMPRESSED_MEMORY_CRITICAL_BYTES
    {
        Some(DiagnosticsLevel::Error)
    } else if memory_ratio >= MEMORY_PRESSURE_WARNING_RATIO
        || host.swap_used_bytes >= SWAP_WARNING_BYTES
        || host.compressed_memory_bytes >= COMPRESSED_MEMORY_WARNING_BYTES
    {
        Some(DiagnosticsLevel::Warn)
    } else {
        None
    };
    if let Some(severity) = memory_severity {
        incidents.push(HostIncidentSnapshot {
            key: "memory-pressure",
            severity: severity.clone(),
            title: if severity == DiagnosticsLevel::Error {
                "Critical host memory pressure incident snapshot recorded."
            } else {
                "Host memory pressure incident snapshot recorded."
            },
            detail: format!(
                "Memory pressure reached {:.0}% used with {} compressed and {} swap in use.",
                memory_ratio * 100.0,
                format_bytes(host.compressed_memory_bytes),
                format_bytes(host.swap_used_bytes)
            ),
            fields: vec![
                ("memory_used_bytes", host.memory_used_bytes.to_string()),
                ("memory_total_bytes", host.memory_total_bytes.to_string()),
                (
                    "compressed_memory_bytes",
                    host.compressed_memory_bytes.to_string(),
                ),
                ("swap_used_bytes", host.swap_used_bytes.to_string()),
            ],
        });
    }

    let wakeup_severity = if host.wakeups_per_second >= WAKEUPS_CRITICAL {
        Some(DiagnosticsLevel::Error)
    } else if host.wakeups_per_second >= WAKEUPS_WARNING {
        Some(DiagnosticsLevel::Warn)
    } else {
        None
    };
    if let Some(severity) = wakeup_severity {
        let leader = snapshot.entities.first();
        incidents.push(HostIncidentSnapshot {
            key: "host-wakeup-storm",
            severity: severity.clone(),
            title: if severity == DiagnosticsLevel::Error {
                "Critical host wakeup storm incident snapshot recorded."
            } else {
                "Host wakeup storm incident snapshot recorded."
            },
            detail: leader
                .map(|entity| {
                    format!(
                        "{} is currently the top wakeup leader at {:.0}/s while the host is at {:.0}/s.",
                        entity.display_name,
                        entity.metrics.wakeups_per_second,
                        host.wakeups_per_second
                    )
                })
                .unwrap_or_else(|| {
                    format!(
                        "Host wakeups are elevated at {:.0}/s with no clear entity leader.",
                        host.wakeups_per_second
                    )
                }),
            fields: vec![("wakeups_per_second", format!("{:.1}", host.wakeups_per_second))],
        });
    }

    if host.thermal_state >= ThermalState::Serious {
        let severity = if host.thermal_state >= ThermalState::Critical {
            DiagnosticsLevel::Error
        } else {
            DiagnosticsLevel::Warn
        };
        incidents.push(HostIncidentSnapshot {
            key: "thermal-pressure",
            severity: severity.clone(),
            title: if severity == DiagnosticsLevel::Error {
                "Critical host thermal pressure incident snapshot recorded."
            } else {
                "Host thermal pressure incident snapshot recorded."
            },
            detail: format!(
                "Host thermal state is {:?} with CPU {:.1}% and wakeups {:.0}/s.",
                host.thermal_state, host.cpu_percent, host.wakeups_per_second
            ),
            fields: vec![
                ("thermal_state", format!("{:?}", host.thermal_state)),
                ("cpu_percent", format!("{:.1}", host.cpu_percent)),
                (
                    "wakeups_per_second",
                    format!("{:.1}", host.wakeups_per_second),
                ),
            ],
        });
    }

    incidents
}

fn last_persisted_system_marker_millis(diagnostics: &DiagnosticsStore) -> Option<u64> {
    diagnostics
        .query(&DiagnosticsQuery {
            limit: 256,
            include_persisted: true,
            ..DiagnosticsQuery::default()
        })
        .into_iter()
        .filter(|event| {
            matches!(
                event.event_type.as_str(),
                "system-sleep-marker"
                    | "system-wake-marker"
                    | "system-previous-shutdown-cause"
                    | "system-panic-marker"
                    | "system-thermal-marker"
                    | "system-power-marker"
            )
        })
        .map(|event| event.timestamp_millis)
        .max()
}

fn ingest_recent_system_markers(diagnostics: &DiagnosticsStore, since_millis: u64) {
    let markers = load_recent_system_markers(since_millis);
    for marker in markers {
        diagnostics.emit(
            DiagnosticsEvent::builder(
                marker.level,
                DiagnosticsSubsystem::Engine,
                marker.event_type,
                marker.message,
            )
            .timestamp_millis(marker.timestamp_millis)
            .field("category", marker.category)
            .field("detail", marker.detail)
            .build(),
        );
    }
}

fn load_recent_system_markers(since_millis: u64) -> Vec<SystemMarker> {
    let output = match std::process::Command::new("/usr/bin/log")
        .args([
            "show",
            "--style",
            "json",
            "--last",
            "12h",
            "--predicate",
            SYSTEM_MARKER_PREDICATE,
        ])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    let stdout = match String::from_utf8(output.stdout) {
        Ok(stdout) => stdout,
        Err(_) => return Vec::new(),
    };
    let entries = match serde_json::from_str::<Vec<UnifiedLogEntry>>(&stdout) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut markers = entries
        .into_iter()
        .filter_map(classify_system_marker)
        .filter(|marker| marker.timestamp_millis > since_millis)
        .collect::<Vec<_>>();
    markers.sort_by_key(|marker| marker.timestamp_millis);
    markers.dedup_by(|left, right| {
        left.timestamp_millis == right.timestamp_millis
            && left.event_type == right.event_type
            && left.detail == right.detail
    });
    markers
}

fn classify_system_marker(entry: UnifiedLogEntry) -> Option<SystemMarker> {
    let timestamp = parse_unified_log_timestamp(entry.timestamp.as_deref()?)?;
    let detail = entry.event_message?;
    let normalized = detail.to_ascii_lowercase();
    if normalized.contains("previous shutdown cause") {
        return Some(SystemMarker {
            timestamp_millis: timestamp,
            level: DiagnosticsLevel::Warn,
            event_type: "system-previous-shutdown-cause",
            message: "Observed a previous shutdown cause marker in the recent system log.",
            detail,
            category: "shutdown",
        });
    }
    if normalized.contains("panic(cpu") || normalized.contains("userspace watchdog timeout") {
        return Some(SystemMarker {
            timestamp_millis: timestamp,
            level: DiagnosticsLevel::Warn,
            event_type: "system-panic-marker",
            message: "Observed a panic or watchdog marker in the recent system log.",
            detail,
            category: "panic",
        });
    }
    if normalized.contains("wake reason")
        || normalized.contains("wake from")
        || normalized.contains("darkwake")
    {
        return Some(SystemMarker {
            timestamp_millis: timestamp,
            level: DiagnosticsLevel::Info,
            event_type: "system-wake-marker",
            message: "Observed a recent system wake marker.",
            detail,
            category: "wake",
        });
    }
    if normalized.contains("entering sleep state") || normalized.contains("previous sleep cause") {
        return Some(SystemMarker {
            timestamp_millis: timestamp,
            level: DiagnosticsLevel::Info,
            event_type: "system-sleep-marker",
            message: "Observed a recent system sleep marker.",
            detail,
            category: "sleep",
        });
    }
    if normalized.contains("thermal pressure") {
        return Some(SystemMarker {
            timestamp_millis: timestamp,
            level: DiagnosticsLevel::Warn,
            event_type: "system-thermal-marker",
            message: "Observed a recent thermal pressure marker.",
            detail,
            category: "thermal",
        });
    }
    if normalized.contains("low power mode") {
        return Some(SystemMarker {
            timestamp_millis: timestamp,
            level: DiagnosticsLevel::Info,
            event_type: "system-power-marker",
            message: "Observed a recent power-state marker.",
            detail,
            category: "power",
        });
    }
    None
}

fn parse_unified_log_timestamp(value: &str) -> Option<u64> {
    let parsed = OffsetDateTime::parse(value, UNIFIED_LOG_TIMESTAMP_FORMAT).ok()?;
    let unix = parsed.unix_timestamp_nanos() / 1_000_000;
    u64::try_from(unix).ok()
}

fn boot_session_key(boot_session: &aetower_model::BootSessionSnapshot) -> Option<String> {
    boot_session.boot_id.clone().or_else(|| {
        boot_session
            .boot_time_millis
            .map(|millis| millis.to_string())
    })
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / GIB)
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unified_log_timestamp() {
        let millis = parse_unified_log_timestamp("2026-04-13 19:03:09.753950+0200")
            .unwrap_or_else(|| panic!("timestamp"));
        assert!(millis > 0);
    }

    #[test]
    fn classifies_sleep_and_wake_markers() {
        let sleep = classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
            event_message: Some("Entering Sleep state due to Clamshell Sleep".to_owned()),
        })
        .unwrap_or_else(|| panic!("sleep marker"));
        assert_eq!(sleep.event_type, "system-sleep-marker");

        let wake = classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:05:09.753950+0200".to_owned()),
            event_message: Some("Wake reason: EC.RTC".to_owned()),
        })
        .unwrap_or_else(|| panic!("wake marker"));
        assert_eq!(wake.event_type, "system-wake-marker");
    }

    #[test]
    fn collects_memory_pressure_incident() {
        let snapshot = SystemSnapshot {
            captured_at_millis: 100,
            host: HostSnapshot {
                memory_used_bytes: 15 * 1024 * 1024 * 1024,
                memory_total_bytes: 16 * 1024 * 1024 * 1024,
                compressed_memory_bytes: 7 * 1024 * 1024 * 1024,
                swap_used_bytes: 20 * 1024 * 1024 * 1024,
                ..HostSnapshot::default()
            },
            ..SystemSnapshot::default()
        };
        let incidents = collect_host_incidents(&snapshot);
        assert!(
            incidents
                .iter()
                .any(|incident| incident.key == "memory-pressure")
        );
    }
}
