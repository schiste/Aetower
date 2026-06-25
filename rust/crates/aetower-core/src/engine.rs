use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
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
use aetower_policy::{
    COMPRESSED_MEMORY_CRITICAL_BYTES, COMPRESSED_MEMORY_WARNING_BYTES,
    MEMORY_PRESSURE_CRITICAL_RATIO, MEMORY_PRESSURE_WARNING_RATIO, SWAP_CRITICAL_BYTES,
    SWAP_WARNING_BYTES, WAKEUPS_CRITICAL, WAKEUPS_WARNING,
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
const HISTORY_MAINTENANCE_INITIAL_DELAY: Duration = Duration::from_secs(45);
const HISTORY_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(10 * 60);
const HISTORY_MAINTENANCE_BUSY_DEFER_INTERVAL: Duration = Duration::from_secs(2 * 60);
const HISTORY_MAINTENANCE_DEFER_LOG_INTERVAL_MILLIS: u64 = 15 * 60 * 1000;
const HISTORY_MAINTENANCE_RUNTIME_METRICS_STALE_MILLIS: u64 = 30 * 1000;
const HISTORY_MAINTENANCE_ENGINE_OVERRUN_RATIO: f32 = 1.25;
const HISTORY_MAINTENANCE_MIN_ENGINE_OVERRUN_MILLIS: f32 = 1_000.0;
const HISTORY_MAINTENANCE_SELF_CPU_BUSY_PERCENT: f32 = 40.0;
const HISTORY_MAINTENANCE_UI_REFRESH_BUSY_MILLIS: f32 = 250.0;
const HISTORY_MAINTENANCE_BRIDGE_BUSY_MILLIS: f32 = 500.0;
const HISTORY_MAINTENANCE_RENDER_BUSY_MILLIS: f32 = 300.0;
const HISTORY_MAINTENANCE_COMMIT_BUSY_MILLIS: f32 = 100.0;
const HISTORY_MAINTENANCE_INPUT_BUSY_MILLIS: f32 = 200.0;
const HISTORY_MAINTENANCE_HISTORY_QUEUE_BUSY_DEPTH: u32 = 8;
const HISTORY_MAINTENANCE_DIAGNOSTICS_QUEUE_BUSY_DEPTH: u32 = 512;
const COLLECTOR_DEFER_ENGINE_TICK_RATIO: f32 = 0.60;
const COLLECTOR_DEFER_MIN_ENGINE_TICK_MILLIS: f32 = 500.0;
const COLLECTOR_DEFER_COLLECT_MILLIS: f32 = 1_000.0;
const COLLECTOR_DEFER_PROCESS_SAMPLING_MILLIS: f32 = 500.0;
const COLLECTOR_DEFER_SELF_CPU_PERCENT: f32 = 20.0;
const COLLECTOR_DEFER_SELF_WAKEUPS_PER_SECOND: f32 = 300.0;
const COLLECTOR_DEFER_MCP_REQUESTS_PER_SECOND: f32 = 8.0;
// Regression detection runs off-tick over persisted history: a short delay
// after launch, then every few hours. The window is bounded to ~7 days and
// downsampled to keep the load_range read cheap.
const REGRESSION_INITIAL_DELAY: Duration = Duration::from_secs(120);
const REGRESSION_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const REGRESSION_WINDOW_MILLIS: u64 = 7 * 86_400_000;
const REGRESSION_MAX_SNAPSHOTS: usize = 400;
// When a maintenance pass finishes and the store is still above the soft cap,
// shorten the next interval so the pruner converges quickly instead of waiting
// a full 10 minutes. Without this, a write-heavy interval can push the store
// past HISTORY_STORE_WARNING_BYTES between two normal cycles.
const HISTORY_MAINTENANCE_AGGRESSIVE_INTERVAL: Duration = Duration::from_secs(60);
const RUNTIME_HEARTBEAT_INTERVAL_MILLIS: u64 = 10 * 60 * 1000;
const DEFAULT_HISTORY_RETENTION_MILLIS: u64 = 12 * 60 * 60 * 1000;
const EMERGENCY_HISTORY_RETENTION_MILLIS: u64 = 3 * 60 * 60 * 1000;
// Soft cap is 384 MB (25% headroom below HISTORY_STORE_WARNING_BYTES in
// aetower-mcp). The pruner needs to begin reclaiming storage *before* the
// MCP diagnostics warning fires at 512 MB, not at the same threshold —
// otherwise the alarm trips while maintenance is still scheduled, and the
// store keeps growing until the next 10-minute maintenance cycle.
const HISTORY_SOFT_MAX_BYTES: u64 = 384 * 1024 * 1024;
const HISTORY_HARD_MAX_BYTES: u64 = 1024 * 1024 * 1024;
const HISTORY_MAX_WAL_BYTES: u64 = 32 * 1024 * 1024;
const HISTORY_TARGET_SNAPSHOT_COUNT: u64 = 2_000;
const HISTORY_SOFT_MAX_SNAPSHOT_COUNT: u64 = 3_000;
const HISTORY_HARD_MAX_SNAPSHOT_COUNT: u64 = 5_000;
const HISTORY_AGGRESSIVE_QUARANTINE_ROWS: u64 = 64;
const HISTORY_HARD_MAX_QUARANTINE_ROWS: u64 = 128;
const MCP_HELPER_STALE_MILLIS: u64 = 15 * 60 * 1000;
const MCP_HELPER_REAP_RETRY_MILLIS: u64 = 60 * 1000;
const MCP_HELPER_LIFECYCLE_AGE_BUCKET_MILLIS: u64 = 15 * 60 * 1000;
const SYSTEM_MARKER_LOOKBACK_MILLIS: u64 = 90 * 60 * 1000;
const DIAGNOSTIC_REPORT_MARKER_LOOKBACK_MILLIS: u64 = 7 * 24 * 60 * 60 * 1000;
const SYSTEM_MARKER_MIN_LOG_SHOW_LOOKBACK_MILLIS: u64 = 60 * 1000;
const SYSTEM_MARKER_PREDICATE: &str = "(eventMessage CONTAINS[c] \"Previous shutdown cause\") OR (eventMessage CONTAINS[c] \"Entering Sleep state\") OR (eventMessage CONTAINS[c] \"Wake reason\") OR (eventMessage CONTAINS[c] \"panic(cpu\") OR (eventMessage CONTAINS[c] \"userspace watchdog timeout\") OR (eventMessage CONTAINS[c] \"thermal pressure\") OR (eventMessage CONTAINS[c] \"low power mode\")";
const HOST_INCIDENT_PERSIST_INTERVAL_MILLIS: u64 = 60 * 60 * 1000;
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
    fn collector_config(&self, defer_expensive_sampling: bool) -> CollectorConfig {
        CollectorConfig {
            full_collection: self.full_collection,
            defer_expensive_sampling,
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
        if host_pressure_safe_mode_reason(host).is_some() {
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
    latest_snapshot: Arc<SystemSnapshot>,
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
    marker_key: Option<String>,
}

#[derive(Debug, Clone)]
struct PersistedHostIncidentState {
    severity: DiagnosticsLevel,
    last_emitted_millis: u64,
    suppressed_count: u32,
}

#[derive(Debug, Clone, Default)]
struct McpHelperLifecycleState {
    initialized: bool,
    helper_count: u32,
    stale_helper_count: u32,
    oldest_age_bucket: u64,
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
        Arc::make_mut(&mut self.latest_snapshot).sequence = self.sequence;
    }
}

pub struct Engine {
    state: Arc<Mutex<EngineState>>,
    adapters: AdapterManager,
    persistence: Arc<Mutex<Option<aetower_persistence::HistoryStore>>>,
    telemetry: Arc<Mutex<TelemetryExporter>>,
    history_maintenance_cancel: Arc<AtomicBool>,
    system_marker_generation: Arc<AtomicU64>,
    diagnostics: DiagnosticsStore,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    adapter_worker: Option<JoinHandle<()>>,
    telemetry_worker: Option<JoinHandle<()>>,
    history_maintenance_worker: Option<JoinHandle<()>>,
    system_marker_worker: Option<JoinHandle<()>>,
    regression_worker: Option<JoinHandle<()>>,
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
            chau7_sessions: Vec::new(),
            thermal_forecast: None,
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
                latest_snapshot: Arc::new(snapshot),
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
            history_maintenance_cancel: Arc::new(AtomicBool::new(false)),
            system_marker_generation: Arc::new(AtomicU64::new(0)),
            diagnostics,
            running: Arc::new(AtomicBool::new(false)),
            worker: None,
            adapter_worker: None,
            telemetry_worker: None,
            history_maintenance_worker: None,
            system_marker_worker: None,
            regression_worker: None,
        }
    }

    pub fn start(&mut self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        self.history_maintenance_cancel
            .store(false, Ordering::SeqCst);

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
            let mut recently_reaped_mcp_helpers = BTreeMap::<u32, u64>::new();
            let mut mcp_helper_lifecycle = McpHelperLifecycleState::default();
            let mut deferred_history_snapshot: Option<Arc<SystemSnapshot>> = None;
            let mut history_store_busy_since_millis: Option<u64> = None;

            while running.load(Ordering::SeqCst) {
                let tick_started = Instant::now();
                let captured_at_millis = aet_time::now_millis();
                let (runtime_config, previous_runtime_metrics, previous_host) = {
                    let guard = state.lock();
                    (
                        guard.runtime_config.clone(),
                        guard.runtime_lag_metrics.clone(),
                        guard.latest_snapshot.host.clone(),
                    )
                };
                let collector_defer_reason = runtime_config
                    .adaptive_cadence
                    .then(|| {
                        collector_expensive_sampling_defer_reason(
                            &previous_runtime_metrics,
                            &previous_host,
                        )
                    })
                    .flatten();
                collector
                    .configure(runtime_config.collector_config(collector_defer_reason.is_some()));
                if let Some(reason) = collector_defer_reason {
                    diagnostics.emit(
                        DiagnosticsEvent::builder(
                            DiagnosticsLevel::Info,
                            DiagnosticsSubsystem::Collector,
                            "collector-expensive-sampling-deferred",
                            "Deferred expensive process sampling for this tick under runtime pressure.",
                        )
                        .timestamp_millis(captured_at_millis)
                        .field("reason", reason)
                        .field(
                            "previous_engine_tick_millis",
                            format!("{:.3}", previous_runtime_metrics.engine_tick_millis),
                        )
                        .field(
                            "previous_collect_millis",
                            format!("{:.3}", previous_runtime_metrics.collect_millis),
                        )
                        .field(
                            "previous_self_cpu_percent",
                            format!("{:.3}", previous_runtime_metrics.self_cpu_percent),
                        )
                        .field(
                            "previous_self_wakeups_per_second",
                            format!("{:.3}", previous_runtime_metrics.self_wakeups_per_second),
                        )
                        .build(),
                    );
                }
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
                let process_ids = process_id_set(&raw.processes);
                let (mcp_helper_count, stale_mcp_helper_count, oldest_mcp_helper_age_millis) =
                    summarize_mcp_helpers(&raw.processes, captured_at_millis, &process_ids);
                let reaped_mcp_helpers = reap_stale_mcp_helpers(
                    &raw.processes,
                    captured_at_millis,
                    &process_ids,
                    &mut recently_reaped_mcp_helpers,
                );
                if reaped_mcp_helpers > 0 {
                    diagnostics.emit(
                        DiagnosticsEvent::builder(
                            DiagnosticsLevel::Warn,
                            DiagnosticsSubsystem::Engine,
                            "mcp-helper-reaped",
                            "Terminated orphaned local MCP helper process(es).",
                        )
                        .timestamp_millis(captured_at_millis)
                        .field("reaped_count", reaped_mcp_helpers)
                        .field("stale_mcp_helper_count", stale_mcp_helper_count)
                        .build(),
                    );
                }
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
                    per_core_cpu: raw.host.per_core_cpu.clone(),
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

                // history_millis used to be a composite of (a) waiting for
                // `state.lock()` and (b) the actual history.update work, which
                // meant a slow tick could be 5 s of lock wait OR 5 s of CPU
                // work and we couldn't tell which from the diagnostic. Split
                // them so future investigations can attribute correctly —
                // long waits implicate whoever holds `state` (UI thread
                // serializing a snapshot, telemetry exporter, adapter worker);
                // long updates implicate History::update itself (z-score
                // pass, retention bookkeeping, timeline growth).
                let mut runtime_lag_metrics = runtime_lag_metrics;
                runtime_lag_metrics.updated_at_millis = captured_at_millis;
                runtime_lag_metrics.collect_millis = collect_millis as f32;
                runtime_lag_metrics.identity_millis = pipeline_timings.identity_millis as f32;
                runtime_lag_metrics.attribution_millis = pipeline_timings.attribution_millis as f32;
                runtime_lag_metrics.friction_millis = pipeline_timings.friction_millis as f32;
                runtime_lag_metrics.enrich_millis = enrich_millis as f32;
                let history_started = Instant::now();
                let mut guard = state.lock();
                let state_lock_wait_millis = history_started.elapsed().as_secs_f64() * 1000.0;
                let history_update_started = Instant::now();
                let (timeline, host_trend, thermal_forecast) =
                    guard
                        .history
                        .update(captured_at_millis, &host, &mut entities);
                let history_update_millis = history_update_started.elapsed().as_secs_f64() * 1000.0;
                guard.sequence += 1;
                let sequence = guard.sequence;
                drop(guard);
                let history_millis = history_started.elapsed().as_secs_f64() * 1000.0;
                runtime_lag_metrics.history_millis = history_millis as f32;
                // Surface long state-lock waits as a forensic breadcrumb.
                // Threshold of 500 ms is well above normal (lock should be
                // held only for snapshot publication, microseconds at most).
                // Crossing it means someone held `state` for noticeably long
                // — emitting here gives us a timestamped record we can
                // cross-reference against UI / adapter / telemetry events
                // when chasing the next multi-second-tick incident.
                if state_lock_wait_millis >= 500.0 {
                    diagnostics.emit(
                        DiagnosticsEvent::builder(
                            DiagnosticsLevel::Warn,
                            DiagnosticsSubsystem::Engine,
                            "long-state-lock-wait",
                            "Engine tick waited unusually long for the shared state lock.",
                        )
                        .timestamp_millis(captured_at_millis)
                        .field("wait_millis", format!("{state_lock_wait_millis:.3}"))
                        .field(
                            "history_update_millis",
                            format!("{history_update_millis:.3}"),
                        )
                        .field("entities_processed", entities.len())
                        .build(),
                    );
                }
                let chau7_sessions = adapters.chau7_session_summaries(&entities);
                let latest_snapshot = Arc::new(SystemSnapshot {
                    sequence,
                    captured_at_millis,
                    host,
                    host_trend,
                    capabilities: capabilities.values().cloned().collect(),
                    entities,
                    timeline,
                    ai_repo_summaries: adapters.ai_repo_summaries(),
                    chau7_sessions,
                    thermal_forecast,
                });
                let entity_count = latest_snapshot.entities.len();
                let target_tick =
                    runtime_config.target_tick(&latest_snapshot.host, &latest_snapshot.entities);
                emit_operator_safe_cadence_if_needed(
                    &diagnostics,
                    latest_snapshot.as_ref(),
                    target_tick,
                    runtime_config.adaptive_cadence,
                );
                {
                    let mut guard = state.lock();
                    guard.latest_snapshot = Arc::clone(&latest_snapshot);
                }
                emit_boot_session_observed(
                    &diagnostics,
                    latest_snapshot.as_ref(),
                    &mut last_boot_session_key,
                );
                emit_host_incident_snapshots(
                    &diagnostics,
                    latest_snapshot.as_ref(),
                    &mut host_incident_state,
                );
                // Persist snapshot (best-effort, throttled by write_interval).
                // This must stay outside `state.lock()`: persistence clones and
                // enqueues the snapshot, and the writer thread owns the SQLite
                // connection used for the real disk I/O.
                let persist_started = Instant::now();
                let mut history_store_busy = false;
                let mut deferred_history_recovered = false;
                let mut deferred_history_age_millis = 0u64;
                let history_queue_depth =
                    if let Some(mut persistence_guard) = persistence.try_lock() {
                        if let Some(store) = persistence_guard.as_mut() {
                            if let Some(deferred_snapshot) = deferred_history_snapshot.take() {
                                deferred_history_age_millis = captured_at_millis
                                    .saturating_sub(deferred_snapshot.captured_at_millis);
                                store.store_immediately(deferred_snapshot.as_ref());
                                deferred_history_recovered = true;
                                history_store_busy_since_millis = None;
                            }
                            store.maybe_store(latest_snapshot.as_ref());
                            store.pending_writes()
                        } else {
                            0
                        }
                    } else {
                        history_store_busy = true;
                        deferred_history_snapshot = Some(Arc::clone(&latest_snapshot));
                        history_store_busy_since_millis.get_or_insert(captured_at_millis);
                        u64::from(deferred_history_snapshot.is_some())
                    };
                if history_store_busy {
                    let deferred_for_millis = history_store_busy_since_millis
                        .map(|since| captured_at_millis.saturating_sub(since))
                        .unwrap_or(0);
                    diagnostics.emit(
                        DiagnosticsEvent::builder(
                            DiagnosticsLevel::Warn,
                            DiagnosticsSubsystem::Persistence,
                            "history-store-busy",
                            "Deferred the latest history write because maintenance held the store lock.",
                        )
                        .timestamp_millis(captured_at_millis)
                        .sequence(latest_snapshot.sequence)
                        .field("deferred_snapshot", true)
                        .field("deferred_for_millis", deferred_for_millis)
                        .build(),
                    );
                } else if deferred_history_recovered {
                    diagnostics.emit(
                        DiagnosticsEvent::builder(
                            DiagnosticsLevel::Info,
                            DiagnosticsSubsystem::Persistence,
                            "history-store-recovered",
                            "Flushed a deferred history write after store maintenance released the lock.",
                        )
                        .timestamp_millis(captured_at_millis)
                        .sequence(latest_snapshot.sequence)
                        .field("deferred_age_millis", deferred_history_age_millis)
                        .field("pending_writes", history_queue_depth)
                        .build(),
                    );
                }
                let persist_millis = persist_started.elapsed().as_secs_f64() * 1000.0;
                runtime_lag_metrics.persist_millis = persist_millis as f32;
                runtime_lag_metrics.gpu_sample_millis = gpu_sample_millis as f32;
                runtime_lag_metrics.history_queue_depth =
                    history_queue_depth.min(u32::MAX as u64) as u32;
                runtime_lag_metrics.diagnostics_queue_depth =
                    diagnostics.pending_writes().min(u32::MAX as u64) as u32;
                runtime_lag_metrics.mcp_helper_count = mcp_helper_count;
                runtime_lag_metrics.stale_mcp_helper_count = stale_mcp_helper_count;
                runtime_lag_metrics.oldest_mcp_helper_age_millis = oldest_mcp_helper_age_millis;
                if let Some(self_process) = raw.self_process.as_ref() {
                    runtime_lag_metrics.self_cpu_percent = self_process.cpu_percent;
                    runtime_lag_metrics.self_memory_bytes = self_process.memory_bytes;
                    runtime_lag_metrics.self_memory_physical_footprint_bytes =
                        self_process.memory_physical_footprint_bytes;
                    runtime_lag_metrics.self_wakeups_per_second = self_process.wakeups_per_second;
                    runtime_lag_metrics.self_energy_nj_per_s = self_process.energy_nj_per_s;
                }
                runtime_lag_metrics.target_tick_millis = target_tick.as_millis() as f32;
                emit_mcp_helper_lifecycle(
                    &diagnostics,
                    captured_at_millis,
                    sequence,
                    mcp_helper_count,
                    stale_mcp_helper_count,
                    oldest_mcp_helper_age_millis,
                    &mut mcp_helper_lifecycle,
                );
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
                        "collect_host_refresh_millis",
                        format!("{:.3}", raw.timings.host_refresh_millis),
                    )
                    .field(
                        "collect_process_refresh_millis",
                        format!("{:.3}", raw.timings.process_refresh_millis),
                    )
                    .field(
                        "collect_process_sampling_millis",
                        format!("{:.3}", raw.timings.process_sampling_millis),
                    )
                    .field(
                        "collect_storage_refresh_millis",
                        format!("{:.3}", raw.timings.storage_refresh_millis),
                    )
                    .field(
                        "collect_environment_refresh_millis",
                        format!("{:.3}", raw.timings.environment_refresh_millis),
                    )
                    .field(
                        "collect_post_process_millis",
                        format!("{:.3}", raw.timings.post_process_millis),
                    )
                    .field("collect_discovery_scan", raw.timings.discovery_scan)
                    .field("collect_sampled_rusage", raw.timings.sampled_rusage)
                    .field(
                        "collect_deferred_expensive_sampling",
                        raw.timings.deferred_expensive_sampling,
                    )
                    .field(
                        "collect_process_refresh_pid_count",
                        raw.timings.process_refresh_pid_count,
                    )
                    .field(
                        "collect_process_refresh_refreshed_memory",
                        raw.timings.process_refresh_refreshed_memory,
                    )
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
                    .field(
                        "state_lock_wait_millis",
                        format!("{state_lock_wait_millis:.3}"),
                    )
                    .field(
                        "history_update_millis",
                        format!("{history_update_millis:.3}"),
                    )
                    .field("persist_millis", format!("{persist_millis:.3}"))
                    .build(),
                );
                let tick_millis = tick_started.elapsed().as_millis();
                runtime_lag_metrics.engine_tick_millis = tick_millis as f32;
                let should_emit_heartbeat = {
                    let guard = state.lock();
                    should_emit_runtime_heartbeat(
                        guard.last_runtime_heartbeat_millis,
                        captured_at_millis,
                    )
                };
                if should_emit_heartbeat {
                    let top_entity = latest_snapshot.entities.first();
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
                        .field(
                            "collect_host_refresh_millis",
                            format!("{:.3}", raw.timings.host_refresh_millis),
                        )
                        .field(
                            "collect_process_refresh_millis",
                            format!("{:.3}", raw.timings.process_refresh_millis),
                        )
                        .field(
                            "collect_process_sampling_millis",
                            format!("{:.3}", raw.timings.process_sampling_millis),
                        )
                        .field(
                            "collect_storage_refresh_millis",
                            format!("{:.3}", raw.timings.storage_refresh_millis),
                        )
                        .field(
                            "collect_environment_refresh_millis",
                            format!("{:.3}", raw.timings.environment_refresh_millis),
                        )
                        .field(
                            "collect_post_process_millis",
                            format!("{:.3}", raw.timings.post_process_millis),
                        )
                        .field("collect_discovery_scan", raw.timings.discovery_scan)
                        .field("collect_sampled_rusage", raw.timings.sampled_rusage)
                        .field(
                            "collect_deferred_expensive_sampling",
                            raw.timings.deferred_expensive_sampling,
                        )
                        // The two fields below decompose collect_process_refresh_millis
                        // into its real drivers. PID count × per-PID cost dominates
                        // the wall time; `refreshed_memory` tells us whether the
                        // extra `task_info` syscall was on the path. With these,
                        // a slow tick is classifiable instead of being a single
                        // opaque number.
                        .field(
                            "collect_process_refresh_pid_count",
                            raw.timings.process_refresh_pid_count,
                        )
                        .field(
                            "collect_process_refresh_refreshed_memory",
                            raw.timings.process_refresh_refreshed_memory,
                        )
                        .field("history_queue_depth", history_queue_depth)
                        .field(
                            "diagnostics_queue_depth",
                            diagnostics.pending_writes().min(u32::MAX as u64),
                        )
                        // history_millis lets us watch the History::update cost
                        // on healthy ticks, not just on tick-over-budget. Before
                        // this field was added we were blind to slow growth
                        // until it crossed the budget threshold; with it we can
                        // spot a drift from 50ms to seconds visibly.
                        .field("history_millis", format!("{history_millis:.3}"))
                        .field(
                            "state_lock_wait_millis",
                            format!("{state_lock_wait_millis:.3}"),
                        )
                        .field(
                            "history_update_millis",
                            format!("{history_update_millis:.3}"),
                        )
                        // Freelist size on the persisted SQLite store. After
                        // the auto_vacuum=INCREMENTAL migration this should
                        // sit near zero; persistent growth means
                        // `incremental_vacuum` is not keeping up. Use
                        // `try_lock` so the heartbeat never blocks on a
                        // long-running maintenance pass holding the store
                        // mutex — reporting 0 on contention is fine, the
                        // next heartbeat will pick it up.
                        .field(
                            "history_freelist_pages",
                            persistence
                                .try_lock()
                                .as_ref()
                                .and_then(|guard| guard.as_ref())
                                .and_then(|store| store.freelist_pages().ok())
                                .unwrap_or(0),
                        )
                        .field("mcp_helper_count", mcp_helper_count)
                        .field("stale_mcp_helper_count", stale_mcp_helper_count)
                        .build(),
                    );
                }
                {
                    let mut guard = state.lock();
                    guard.runtime_lag_metrics = runtime_lag_metrics;
                    if should_emit_heartbeat {
                        guard.last_runtime_heartbeat_millis = captured_at_millis;
                    }
                }
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
                        "collect_host_refresh_millis",
                        format!("{:.3}", raw.timings.host_refresh_millis),
                    )
                    .field(
                        "collect_process_refresh_millis",
                        format!("{:.3}", raw.timings.process_refresh_millis),
                    )
                    .field(
                        "collect_process_sampling_millis",
                        format!("{:.3}", raw.timings.process_sampling_millis),
                    )
                    .field(
                        "collect_storage_refresh_millis",
                        format!("{:.3}", raw.timings.storage_refresh_millis),
                    )
                    .field(
                        "collect_environment_refresh_millis",
                        format!("{:.3}", raw.timings.environment_refresh_millis),
                    )
                    .field(
                        "collect_post_process_millis",
                        format!("{:.3}", raw.timings.post_process_millis),
                    )
                    .field("collect_discovery_scan", raw.timings.discovery_scan)
                    .field("collect_sampled_rusage", raw.timings.sampled_rusage)
                    .field(
                        "collect_deferred_expensive_sampling",
                        raw.timings.deferred_expensive_sampling,
                    )
                    .field(
                        "collect_process_refresh_pid_count",
                        raw.timings.process_refresh_pid_count,
                    )
                    .field(
                        "collect_process_refresh_refreshed_memory",
                        raw.timings.process_refresh_refreshed_memory,
                    )
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
                    .field(
                        "state_lock_wait_millis",
                        format!("{state_lock_wait_millis:.3}"),
                    )
                    .field(
                        "history_update_millis",
                        format!("{history_update_millis:.3}"),
                    )
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
                            Arc::clone(&guard.latest_snapshot),
                            guard.runtime_lag_metrics.clone(),
                        )
                    };
                    let _ = telemetry.lock().export(snapshot.as_ref(), &lag_metrics);
                }

                let sleep_for = if enabled {
                    Duration::from_secs(interval_secs as u64)
                } else {
                    TELEMETRY_DISABLED_SLEEP
                };
                sleep_with_stop(&running, sleep_for);
            }
        }));

        let state = Arc::clone(&self.state);
        let persistence = Arc::clone(&self.persistence);
        let diagnostics = self.diagnostics.clone();
        let running = Arc::clone(&self.running);
        let maintenance_cancel = Arc::clone(&self.history_maintenance_cancel);
        self.history_maintenance_worker = Some(thread::spawn(move || {
            sleep_with_stop(&running, HISTORY_MAINTENANCE_INITIAL_DELAY);
            let mut last_busy_defer_log_millis = 0;
            while running.load(Ordering::SeqCst) {
                if let Some(reason) = history_maintenance_busy_reason(&state) {
                    let now = aet_time::now_millis();
                    if now.saturating_sub(last_busy_defer_log_millis)
                        >= HISTORY_MAINTENANCE_DEFER_LOG_INTERVAL_MILLIS
                    {
                        diagnostics.emit(
                            DiagnosticsEvent::builder(
                                DiagnosticsLevel::Info,
                                DiagnosticsSubsystem::Persistence,
                                "history-maintenance-deferred",
                                "Deferred persisted history maintenance while runtime load was elevated.",
                            )
                            .field("reason", reason)
                            .field(
                                "defer_millis",
                                HISTORY_MAINTENANCE_BUSY_DEFER_INTERVAL.as_millis(),
                            )
                            .build(),
                        );
                        last_busy_defer_log_millis = now;
                    }
                    sleep_with_stop(&running, HISTORY_MAINTENANCE_BUSY_DEFER_INTERVAL);
                    continue;
                }
                let started = Instant::now();
                let policy = default_history_retention_policy();
                let result = if let Some(guard) = persistence.try_lock() {
                    guard.as_ref().map(|store| {
                        store.maintain_with_policy_cancellable(
                            policy,
                            Arc::clone(&maintenance_cancel),
                        )
                    })
                } else {
                    None
                };
                let elapsed_millis = started.elapsed().as_millis();
                // Default cadence is HISTORY_MAINTENANCE_INTERVAL. Only when
                // the post-maintenance store size still exceeds the soft cap
                // do we accelerate to the aggressive interval so the pruner
                // catches up before the warning threshold is reached.
                let mut next_sleep = HISTORY_MAINTENANCE_INTERVAL;
                match result {
                    Some(Ok(report)) => {
                        if report.store_bytes_after > policy.soft_max_store_bytes {
                            next_sleep = HISTORY_MAINTENANCE_AGGRESSIVE_INTERVAL;
                        }
                        if report.cancelled {
                            diagnostics.emit(
                                DiagnosticsEvent::builder(
                                    DiagnosticsLevel::Info,
                                    DiagnosticsSubsystem::Persistence,
                                    "history-maintenance-cancelled",
                                    "Cancelled persisted history maintenance during shutdown.",
                                )
                                .field("elapsed_millis", elapsed_millis)
                                .field("store_bytes_after", report.store_bytes_after)
                                .field("wal_bytes_after", report.wal_bytes_after)
                                .field("pruned_rows", report.pruned_rows)
                                .build(),
                            );
                        } else if elapsed_millis >= 5_000 {
                            diagnostics.emit(
                                DiagnosticsEvent::builder(
                                    DiagnosticsLevel::Warn,
                                    DiagnosticsSubsystem::Persistence,
                                    "history-maintenance-over-budget",
                                    "Persisted history maintenance took longer than expected.",
                                )
                                .field("elapsed_millis", elapsed_millis)
                                .field("store_bytes_after", report.store_bytes_after)
                                .field("wal_bytes_after", report.wal_bytes_after)
                                .field("pruned_rows", report.pruned_rows)
                                .build(),
                            );
                        }
                    }
                    Some(Err(error)) => {
                        diagnostics.emit(
                            DiagnosticsEvent::builder(
                                DiagnosticsLevel::Error,
                                DiagnosticsSubsystem::Persistence,
                                "history-maintenance-failed",
                                "Persisted history maintenance failed.",
                            )
                            .field("error", error)
                            .build(),
                        );
                    }
                    None => {}
                }
                sleep_with_stop(&running, next_sleep);
            }
        }));

        let diagnostics = self.diagnostics.clone();
        let running = Arc::clone(&self.running);
        let system_marker_generation = Arc::clone(&self.system_marker_generation);
        let worker_generation = system_marker_generation
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        self.system_marker_worker = Some(thread::spawn(move || {
            let since_millis =
                last_persisted_system_marker_millis(&diagnostics).unwrap_or_else(|| {
                    aet_time::now_millis().saturating_sub(SYSTEM_MARKER_LOOKBACK_MILLIS)
                });
            ingest_recent_system_markers(
                &diagnostics,
                &running,
                &system_marker_generation,
                worker_generation,
                since_millis,
            );
        }));

        // Regression detector: off-tick, reads persisted history, emits
        // Regression timeline events. Deduped per (bundle, kind) for the
        // session so a standing regression isn't re-announced every cycle.
        let persistence = Arc::clone(&self.persistence);
        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);
        self.regression_worker = Some(thread::spawn(move || {
            sleep_with_stop(&running, REGRESSION_INITIAL_DELAY);
            let mut emitted: std::collections::HashSet<String> = std::collections::HashSet::new();
            while running.load(Ordering::SeqCst) {
                let now = aet_time::now_millis();
                let window_start = now.saturating_sub(REGRESSION_WINDOW_MILLIS);
                // Load without blocking ticks; skip this cycle if the store is busy.
                let snapshots = match persistence.try_lock() {
                    Some(guard) => guard
                        .as_ref()
                        .and_then(|store| store.load_range(window_start, now).ok())
                        .unwrap_or_default(),
                    None => Vec::new(),
                };
                let downsampled = downsample_hourly(snapshots, REGRESSION_MAX_SNAPSHOTS);
                if !downsampled.is_empty() {
                    let findings = aetower_regression::detect_regressions(&downsampled, now);
                    if !findings.is_empty() {
                        let mut guard = state.lock();
                        for finding in findings {
                            let (kind_key, kind_title) = match finding.kind {
                                aetower_regression::RegressionKind::Memory7dCreep => {
                                    ("memory-creep", "Memory creep")
                                }
                                aetower_regression::RegressionKind::IdleCpuSinceUpdate => {
                                    ("idle-cpu-update", "Idle CPU rose after update")
                                }
                            };
                            let dedup_key = format!("{}:{kind_key}", finding.bundle_id);
                            if emitted.insert(dedup_key) {
                                guard.history.record_regression(
                                    now,
                                    &finding.bundle_id,
                                    kind_key,
                                    finding.severity,
                                    format!("{}: {kind_title}", finding.display_name),
                                    finding.summary,
                                );
                            }
                        }
                    }
                }
                sleep_with_stop(&running, REGRESSION_INTERVAL);
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.history_maintenance_cancel
            .store(true, Ordering::SeqCst);
        self.system_marker_generation.fetch_add(1, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.adapter_worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.telemetry_worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.history_maintenance_worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.regression_worker.take() {
            let _ = worker.join();
        }
        // System-marker ingestion is best-effort and can be slow on machines
        // with large unified-log stores. Never block engine shutdown on it.
        if let Some(worker) = self.system_marker_worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
    }

    pub fn latest_snapshot(&self) -> SystemSnapshot {
        self.state.lock().latest_snapshot.as_ref().clone()
    }

    pub fn latest_snapshot_if_newer(&self, last_sequence: u64) -> Option<SystemSnapshot> {
        let guard = self.state.lock();
        (guard.latest_snapshot.sequence > last_sequence)
            .then(|| guard.latest_snapshot.as_ref().clone())
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

    pub fn record_mcp_runtime_observation(
        &self,
        total_connections: u64,
        active_client_count: u64,
        total_requests: u64,
    ) {
        let now = aet_time::now_millis();
        let mut guard = self.state.lock();
        let previous_total = guard.runtime_lag_metrics.mcp_total_requests;
        let previous_millis = guard.runtime_lag_metrics.mcp_observed_at_millis;
        let elapsed_seconds = now.saturating_sub(previous_millis) as f32 / 1_000.0;
        guard.runtime_lag_metrics.mcp_requests_per_second =
            if previous_millis > 0 && elapsed_seconds > 0.0 && total_requests >= previous_total {
                (total_requests - previous_total) as f32 / elapsed_seconds
            } else {
                0.0
            };
        guard.runtime_lag_metrics.mcp_total_connections = total_connections;
        guard.runtime_lag_metrics.mcp_active_client_count =
            active_client_count.min(u32::MAX as u64) as u32;
        guard.runtime_lag_metrics.mcp_total_requests = total_requests;
        guard.runtime_lag_metrics.mcp_observed_at_millis = now;
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
        let capabilities = guard.capabilities.values().cloned().collect();
        Arc::make_mut(&mut guard.latest_snapshot).capabilities = capabilities;
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

    /// Push the binary-reputation consent + VirusTotal API key into the engine.
    /// The key is held in memory for the session only and never persisted.
    pub fn set_binary_reputation_config(&self, enabled: bool, api_key: String) {
        self.adapters.set_binary_reputation_config(enabled, api_key);
    }

    pub fn configure_telemetry(
        &self,
        endpoint: Option<String>,
        enabled: bool,
        export_interval_secs: u32,
    ) {
        let mut guard = self.telemetry.lock();
        let config = telemetry_config_with_overrides(
            guard.config(),
            endpoint,
            enabled,
            export_interval_secs,
        );
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
                Arc::clone(&guard.latest_snapshot),
                guard.runtime_lag_metrics.clone(),
            )
        };
        self.telemetry
            .lock()
            .verify_export(snapshot.as_ref(), &lag_metrics)
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
        let capabilities = guard.capabilities.values().cloned().collect();
        Arc::make_mut(&mut guard.latest_snapshot).capabilities = capabilities;
        guard.publish_mutation();
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

fn summarize_mcp_helpers(
    processes: &[crate::collector::RawProcessSample],
    captured_at_millis: u64,
    process_ids: &BTreeSet<u32>,
) -> (u32, u32, u64) {
    let mut total = 0u32;
    let mut stale = 0u32;
    let mut oldest_age = 0u64;

    for process in processes {
        if !is_mcp_helper_process(process) {
            continue;
        }
        total = total.saturating_add(1);
        let age = captured_at_millis.saturating_sub(process.start_time_millis);
        oldest_age = oldest_age.max(age);
        if age >= MCP_HELPER_STALE_MILLIS && is_orphaned_process(process, process_ids) {
            stale = stale.saturating_add(1);
        }
    }

    (total, stale, oldest_age)
}

fn emit_mcp_helper_lifecycle(
    diagnostics: &DiagnosticsStore,
    captured_at_millis: u64,
    sequence: u64,
    helper_count: u32,
    stale_helper_count: u32,
    oldest_age_millis: u64,
    state: &mut McpHelperLifecycleState,
) {
    let oldest_age_bucket = oldest_age_millis / MCP_HELPER_LIFECYCLE_AGE_BUCKET_MILLIS.max(1);
    let count_changed = state.helper_count != helper_count;
    let stale_changed = state.stale_helper_count != stale_helper_count;
    let age_bucket_changed = state.oldest_age_bucket != oldest_age_bucket;
    let should_emit = if !state.initialized {
        helper_count > 0
    } else {
        count_changed || stale_changed || (helper_count > 0 && age_bucket_changed)
    };

    state.initialized = true;
    state.helper_count = helper_count;
    state.stale_helper_count = stale_helper_count;
    state.oldest_age_bucket = oldest_age_bucket;

    if !should_emit {
        return;
    }

    let level = if stale_helper_count > 0 {
        DiagnosticsLevel::Warn
    } else {
        DiagnosticsLevel::Info
    };
    let message = if helper_count == 0 {
        "Local MCP helper processes drained."
    } else if stale_helper_count > 0 {
        "Local MCP helper lifecycle reported stale helper process(es)."
    } else {
        "Local MCP helper lifecycle changed."
    };
    diagnostics.emit(
        DiagnosticsEvent::builder(
            level,
            DiagnosticsSubsystem::Engine,
            "mcp-helper-lifecycle",
            message,
        )
        .timestamp_millis(captured_at_millis)
        .sequence(sequence)
        .field("mcp_helper_count", helper_count)
        .field("stale_mcp_helper_count", stale_helper_count)
        .field("oldest_mcp_helper_age_millis", oldest_age_millis)
        .field("age_bucket_minutes", oldest_age_bucket.saturating_mul(15))
        .build(),
    );
}

fn reap_stale_mcp_helpers(
    processes: &[crate::collector::RawProcessSample],
    captured_at_millis: u64,
    process_ids: &BTreeSet<u32>,
    recently_reaped: &mut BTreeMap<u32, u64>,
) -> u32 {
    recently_reaped.retain(|_, last_attempt| {
        captured_at_millis.saturating_sub(*last_attempt) < MCP_HELPER_REAP_RETRY_MILLIS
    });
    let mut reaped = 0u32;

    for process in processes {
        if !is_mcp_helper_process(process) {
            continue;
        }
        let age = captured_at_millis.saturating_sub(process.start_time_millis);
        if age < MCP_HELPER_STALE_MILLIS || !is_orphaned_process(process, process_ids) {
            continue;
        }
        if recently_reaped.contains_key(&process.pid) {
            continue;
        }
        recently_reaped.insert(process.pid, captured_at_millis);
        if terminate_process(process.pid) {
            reaped = reaped.saturating_add(1);
        }
    }

    reaped
}

fn process_id_set(processes: &[crate::collector::RawProcessSample]) -> BTreeSet<u32> {
    processes.iter().map(|process| process.pid).collect()
}

fn is_mcp_helper_process(process: &crate::collector::RawProcessSample) -> bool {
    process.name == "aetower-mcp"
        || process
            .exe
            .as_deref()
            .is_some_and(|exe| exe.ends_with("/aetower-mcp"))
}

fn is_orphaned_process(
    process: &crate::collector::RawProcessSample,
    process_ids: &BTreeSet<u32>,
) -> bool {
    match process.parent_pid {
        None => true,
        Some(0 | 1) => true,
        Some(parent_pid) => !process_ids.contains(&parent_pid),
    }
}

#[cfg(unix)]
fn terminate_process(pid: u32) -> bool {
    const SIGTERM: i32 = 15;
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    if pid <= 1 || pid > i32::MAX as u32 {
        return false;
    }
    unsafe { kill(pid as i32, SIGTERM) == 0 }
}

#[cfg(not(unix))]
fn terminate_process(_pid: u32) -> bool {
    false
}

fn default_history_retention_policy() -> aetower_persistence::HistoryRetentionPolicy {
    aetower_persistence::HistoryRetentionPolicy {
        max_age_millis: DEFAULT_HISTORY_RETENTION_MILLIS,
        emergency_max_age_millis: EMERGENCY_HISTORY_RETENTION_MILLIS,
        soft_max_store_bytes: HISTORY_SOFT_MAX_BYTES,
        hard_max_store_bytes: HISTORY_HARD_MAX_BYTES,
        max_wal_bytes: HISTORY_MAX_WAL_BYTES,
        target_snapshot_count: HISTORY_TARGET_SNAPSHOT_COUNT,
        soft_max_snapshot_count: HISTORY_SOFT_MAX_SNAPSHOT_COUNT,
        hard_max_snapshot_count: HISTORY_HARD_MAX_SNAPSHOT_COUNT,
        aggressive_quarantine_rows: HISTORY_AGGRESSIVE_QUARANTINE_ROWS,
        hard_max_quarantine_rows: HISTORY_HARD_MAX_QUARANTINE_ROWS,
    }
}

fn history_maintenance_busy_reason(state: &Arc<Mutex<EngineState>>) -> Option<&'static str> {
    let Some(guard) = state.try_lock() else {
        return Some("state-lock-busy");
    };
    if let Some(reason) = host_pressure_safe_mode_reason(&guard.latest_snapshot.host) {
        return Some(reason);
    }
    let metrics = &guard.runtime_lag_metrics;
    let now = aet_time::now_millis();
    if metrics.updated_at_millis == 0
        || now.saturating_sub(metrics.updated_at_millis)
            > HISTORY_MAINTENANCE_RUNTIME_METRICS_STALE_MILLIS
    {
        return None;
    }

    if metrics.target_tick_millis > 0.0
        && metrics.engine_tick_millis
            >= (metrics.target_tick_millis * HISTORY_MAINTENANCE_ENGINE_OVERRUN_RATIO)
                .max(HISTORY_MAINTENANCE_MIN_ENGINE_OVERRUN_MILLIS)
    {
        return Some("engine-tick-overrun");
    }
    if metrics.self_cpu_percent >= HISTORY_MAINTENANCE_SELF_CPU_BUSY_PERCENT {
        return Some("self-cpu");
    }
    if metrics.ui_refresh_millis >= HISTORY_MAINTENANCE_UI_REFRESH_BUSY_MILLIS {
        return Some("ui-refresh");
    }
    if metrics.bridge_fetch_millis >= HISTORY_MAINTENANCE_BRIDGE_BUSY_MILLIS {
        return Some("bridge-fetch");
    }
    if metrics.snapshot_to_render_millis >= HISTORY_MAINTENANCE_RENDER_BUSY_MILLIS {
        return Some("snapshot-to-render");
    }
    if metrics.render_commit_millis >= HISTORY_MAINTENANCE_COMMIT_BUSY_MILLIS {
        return Some("render-commit");
    }
    if metrics.input_max_latency_millis >= HISTORY_MAINTENANCE_INPUT_BUSY_MILLIS {
        return Some("input-latency");
    }
    if metrics.history_queue_depth >= HISTORY_MAINTENANCE_HISTORY_QUEUE_BUSY_DEPTH {
        return Some("history-queue");
    }
    if metrics.diagnostics_queue_depth >= HISTORY_MAINTENANCE_DIAGNOSTICS_QUEUE_BUSY_DEPTH {
        return Some("diagnostics-queue");
    }
    None
}

fn collector_expensive_sampling_defer_reason(
    metrics: &RuntimeLagMetrics,
    host: &HostSnapshot,
) -> Option<&'static str> {
    if let Some(reason) = host_pressure_safe_mode_reason(host) {
        return Some(reason);
    }
    if metrics.target_tick_millis > 0.0
        && metrics.engine_tick_millis
            >= (metrics.target_tick_millis * COLLECTOR_DEFER_ENGINE_TICK_RATIO)
                .max(COLLECTOR_DEFER_MIN_ENGINE_TICK_MILLIS)
    {
        return Some("engine-tick-pressure");
    }
    if metrics.collect_millis >= COLLECTOR_DEFER_COLLECT_MILLIS {
        return Some("collector-pressure");
    }
    if metrics.collect_millis >= COLLECTOR_DEFER_PROCESS_SAMPLING_MILLIS
        && metrics.self_cpu_percent >= COLLECTOR_DEFER_SELF_CPU_PERCENT
    {
        return Some("self-cpu-sampling-pressure");
    }
    if metrics.self_wakeups_per_second >= COLLECTOR_DEFER_SELF_WAKEUPS_PER_SECOND {
        return Some("self-wakeup-pressure");
    }
    if metrics.mcp_requests_per_second >= COLLECTOR_DEFER_MCP_REQUESTS_PER_SECOND {
        return Some("mcp-diagnostics-burst");
    }
    None
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

fn telemetry_config_with_overrides(
    current: &OtlpConfig,
    endpoint: Option<String>,
    enabled: bool,
    export_interval_secs: u32,
) -> OtlpConfig {
    let mut config = current.clone();
    config.endpoint = endpoint
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OtlpConfig::default().endpoint);
    config.enabled = enabled;
    config.export_interval_secs = export_interval_secs.max(5);
    config
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

/// Keep at most one snapshot per hour bucket (and cap the total) so the
/// regression detector reads a bounded series regardless of persistence cadence.
fn downsample_hourly(snapshots: Vec<SystemSnapshot>, cap: usize) -> Vec<SystemSnapshot> {
    const HOUR_MILLIS: u64 = 3_600_000;
    let mut seen_hours = std::collections::HashSet::new();
    let mut kept: Vec<SystemSnapshot> = snapshots
        .into_iter()
        .filter(|snapshot| seen_hours.insert(snapshot.captured_at_millis / HOUR_MILLIS))
        .collect();
    // If still over the cap, keep the most recent `cap` hourly samples.
    if kept.len() > cap {
        let drop = kept.len() - cap;
        kept.drain(0..drop);
    }
    kept
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
        let suppressed_count = match state.get_mut(incident.key) {
            Some(previous)
                if previous.severity == incident.severity
                    && captured_at_millis.saturating_sub(previous.last_emitted_millis)
                        < HOST_INCIDENT_PERSIST_INTERVAL_MILLIS =>
            {
                previous.suppressed_count = previous.suppressed_count.saturating_add(1);
                continue;
            }
            Some(previous) => previous.suppressed_count,
            None => 0,
        };

        let mut event = DiagnosticsEvent::builder(
            incident.severity.clone(),
            DiagnosticsSubsystem::History,
            "host-incident-snapshot",
            incident.title,
        )
        .timestamp_millis(captured_at_millis)
        .sequence(snapshot.sequence)
        .field("incident_key", incident.key)
        .field("suppressed_count", suppressed_count)
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
                suppressed_count: 0,
            },
        );
    }
}

fn collect_host_incidents(snapshot: &SystemSnapshot) -> Vec<HostIncidentSnapshot> {
    let mut incidents = Vec::new();
    let host = &snapshot.host;
    let memory_ratio = host_memory_pressure_ratio(host);
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

fn emit_operator_safe_cadence_if_needed(
    diagnostics: &DiagnosticsStore,
    snapshot: &SystemSnapshot,
    target_tick: Duration,
    adaptive_cadence: bool,
) {
    if !adaptive_cadence {
        return;
    }
    let Some(reason) = host_pressure_safe_mode_reason(&snapshot.host) else {
        return;
    };
    diagnostics.emit(
        DiagnosticsEvent::builder(
            DiagnosticsLevel::Warn,
            DiagnosticsSubsystem::Engine,
            "operator-safe-cadence-active",
            "Adaptive cadence widened while host pressure was elevated.",
        )
        .timestamp_millis(snapshot.captured_at_millis)
        .sequence(snapshot.sequence)
        .field("incident_key", "operator-safe-cadence")
        .field("reason", reason)
        .field("target_tick_millis", target_tick.as_millis())
        .field(
            "memory_used_ratio",
            format!("{:.3}", host_memory_pressure_ratio(&snapshot.host)),
        )
        .field(
            "compressed_memory_bytes",
            snapshot.host.compressed_memory_bytes,
        )
        .field("swap_used_bytes", snapshot.host.swap_used_bytes)
        .field(
            "wakeups_per_second",
            format!("{:.1}", snapshot.host.wakeups_per_second),
        )
        .field(
            "thermal_state",
            format!("{:?}", snapshot.host.thermal_state),
        )
        .build(),
    );
}

fn host_pressure_safe_mode_reason(host: &HostSnapshot) -> Option<&'static str> {
    if host_memory_pressure_ratio(host) >= MEMORY_PRESSURE_CRITICAL_RATIO
        || host.swap_used_bytes >= SWAP_CRITICAL_BYTES
        || host.compressed_memory_bytes >= COMPRESSED_MEMORY_CRITICAL_BYTES
    {
        return Some("host-memory-pressure");
    }
    if host.wakeups_per_second >= WAKEUPS_CRITICAL {
        return Some("host-wakeup-storm");
    }
    if host.thermal_state >= ThermalState::Serious {
        return Some("host-thermal-pressure");
    }
    None
}

fn host_memory_pressure_ratio(host: &HostSnapshot) -> f64 {
    if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.memory_used_bytes as f64 / host.memory_total_bytes as f64
    }
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

fn ingest_recent_system_markers(
    diagnostics: &DiagnosticsStore,
    running: &AtomicBool,
    generation: &AtomicU64,
    expected_generation: u64,
    since_millis: u64,
) {
    if !system_marker_worker_is_current(running, generation, expected_generation) {
        return;
    }
    let markers = load_recent_system_markers(since_millis);
    if !system_marker_worker_is_current(running, generation, expected_generation) {
        return;
    }
    for marker in markers {
        if !system_marker_worker_is_current(running, generation, expected_generation) {
            break;
        }
        let mut event = DiagnosticsEvent::builder(
            marker.level,
            DiagnosticsSubsystem::Engine,
            marker.event_type,
            marker.message,
        )
        .timestamp_millis(marker.timestamp_millis)
        .field("category", marker.category)
        .field("detail", marker.detail);
        if let Some(marker_key) = marker.marker_key {
            event = event.field("marker_key", marker_key);
        }
        diagnostics.emit(event.build());
    }

    if !system_marker_worker_is_current(running, generation, expected_generation) {
        return;
    }
    let crash_report_markers = crate::crash_reports::load_recent_crash_report_markers(
        aet_time::now_millis(),
        DIAGNOSTIC_REPORT_MARKER_LOOKBACK_MILLIS,
    );
    for marker in crash_report_markers {
        if !system_marker_worker_is_current(running, generation, expected_generation) {
            break;
        }
        let mut event = DiagnosticsEvent::builder(
            marker.level,
            DiagnosticsSubsystem::Engine,
            marker.event_type,
            marker.message,
        )
        .timestamp_millis(marker.timestamp_millis)
        .field("category", "diagnostic-report");
        for (key, value) in marker.fields {
            event = event.field(key, value);
        }
        diagnostics.emit(event.build());
    }
}

fn system_marker_worker_is_current(
    running: &AtomicBool,
    generation: &AtomicU64,
    expected_generation: u64,
) -> bool {
    running.load(Ordering::SeqCst) && generation.load(Ordering::SeqCst) == expected_generation
}

fn load_recent_system_markers(since_millis: u64) -> Vec<SystemMarker> {
    let last_arg = unified_log_last_arg(since_millis);
    let output = match std::process::Command::new("/usr/bin/log")
        .args([
            "show",
            "--style",
            "json",
            "--last",
            last_arg.as_str(),
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
    if is_system_marker_noise(&normalized) {
        return None;
    }
    if normalized
        .trim_start()
        .starts_with("previous shutdown cause:")
    {
        let marker_key = shutdown_marker_key(&detail);
        return Some(SystemMarker {
            timestamp_millis: timestamp,
            level: DiagnosticsLevel::Info,
            event_type: "system-previous-shutdown-cause",
            message: "Observed a previous shutdown cause marker in the recent system log.",
            detail,
            category: "shutdown",
            marker_key: Some(marker_key),
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
            marker_key: None,
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
            marker_key: None,
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
            marker_key: None,
        });
    }
    if normalized.contains("thermal pressure") {
        return Some(SystemMarker {
            timestamp_millis: timestamp,
            level: thermal_marker_level(&normalized),
            event_type: "system-thermal-marker",
            message: "Observed a recent thermal pressure marker.",
            detail,
            category: "thermal",
            marker_key: None,
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
            marker_key: None,
        });
    }
    None
}

fn is_system_marker_noise(normalized_detail: &str) -> bool {
    normalized_detail.contains("polling for the states of screen")
        || normalized_detail.contains("reachability status")
        || normalized_detail.contains("no wake from sleep")
        || normalized_detail.contains("log run noninteractively")
        || normalized_detail.contains("/usr/bin/log")
        || normalized_detail.contains("current thermal state:")
        || normalized_detail.contains("isnaturalplatform:")
        || normalized_detail.contains("current thermal pressure level=0")
        || normalized_detail.contains("current thermal pressure level = 0")
}

fn unified_log_last_arg(since_millis: u64) -> String {
    let elapsed = aet_time::now_millis().saturating_sub(since_millis);
    unified_log_last_arg_for_elapsed(elapsed)
}

fn unified_log_last_arg_for_elapsed(elapsed_millis: u64) -> String {
    let capped = elapsed_millis.clamp(
        SYSTEM_MARKER_MIN_LOG_SHOW_LOOKBACK_MILLIS,
        SYSTEM_MARKER_LOOKBACK_MILLIS,
    );
    let minutes = capped.div_ceil(60 * 1000).max(1);
    format!("{minutes}m")
}

fn thermal_marker_level(normalized_detail: &str) -> DiagnosticsLevel {
    if normalized_detail.contains("level=0")
        || normalized_detail.contains("level = 0")
        || normalized_detail.contains("nominal")
    {
        DiagnosticsLevel::Info
    } else {
        DiagnosticsLevel::Warn
    }
}

fn shutdown_marker_key(detail: &str) -> String {
    detail
        .split_once(':')
        .map(|(_, code)| code.trim())
        .filter(|code| !code.is_empty())
        .map(|code| format!("previous-shutdown-cause:{code}"))
        .unwrap_or_else(|| "previous-shutdown-cause:unknown".to_owned())
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

    fn engine_state_with_runtime_metrics(
        runtime_lag_metrics: RuntimeLagMetrics,
    ) -> Arc<Mutex<EngineState>> {
        Arc::new(Mutex::new(EngineState {
            sequence: 0,
            latest_snapshot: Arc::new(SystemSnapshot::default()),
            capabilities: std::collections::BTreeMap::new(),
            frontmost_app_state: None,
            history: History::new(),
            runtime_lag_metrics,
            runtime_config: RuntimeCollectionConfig::default(),
            last_runtime_heartbeat_millis: 0,
        }))
    }

    #[test]
    fn history_maintenance_busy_reason_detects_recent_ui_load() {
        let state = engine_state_with_runtime_metrics(RuntimeLagMetrics {
            updated_at_millis: aet_time::now_millis(),
            input_max_latency_millis: HISTORY_MAINTENANCE_INPUT_BUSY_MILLIS,
            ..RuntimeLagMetrics::default()
        });

        assert_eq!(
            history_maintenance_busy_reason(&state),
            Some("input-latency")
        );
    }

    #[test]
    fn history_maintenance_busy_reason_ignores_stale_metrics() {
        let state = engine_state_with_runtime_metrics(RuntimeLagMetrics {
            updated_at_millis: aet_time::now_millis()
                .saturating_sub(HISTORY_MAINTENANCE_RUNTIME_METRICS_STALE_MILLIS + 1),
            input_max_latency_millis: HISTORY_MAINTENANCE_INPUT_BUSY_MILLIS,
            ..RuntimeLagMetrics::default()
        });

        assert_eq!(history_maintenance_busy_reason(&state), None);
    }

    #[test]
    fn history_maintenance_busy_reason_defers_on_host_pressure_even_with_stale_metrics() {
        let state = engine_state_with_runtime_metrics(RuntimeLagMetrics {
            updated_at_millis: aet_time::now_millis()
                .saturating_sub(HISTORY_MAINTENANCE_RUNTIME_METRICS_STALE_MILLIS + 1),
            ..RuntimeLagMetrics::default()
        });
        state.lock().latest_snapshot = Arc::new(SystemSnapshot {
            host: HostSnapshot {
                memory_used_bytes: 15 * 1024 * 1024 * 1024,
                memory_total_bytes: 16 * 1024 * 1024 * 1024,
                ..HostSnapshot::default()
            },
            ..SystemSnapshot::default()
        });

        assert_eq!(
            history_maintenance_busy_reason(&state),
            Some("host-memory-pressure")
        );
    }

    #[test]
    fn adaptive_cadence_uses_low_power_tick_under_host_pressure() {
        let config = RuntimeCollectionConfig::default();
        let host = HostSnapshot {
            wakeups_per_second: WAKEUPS_CRITICAL,
            ..HostSnapshot::default()
        };

        assert_eq!(config.target_tick(&host, &[]), config.low_power_tick);
    }

    /// Pin the relationship between the engine-side retention policy and the
    /// persistence-side VACUUM gate. The original bug fixed in
    /// `perf(persistence): trigger VACUUM by absolute waste, not file size`
    /// was that `should_vacuum` had a 512 MB lower bound while
    /// `HISTORY_SOFT_MAX_BYTES` was 384 MB — the policy treated the DB as
    /// "trouble" at 384 MB but the gate could only fire at 512 MB, so
    /// VACUUM never ran in normal operation. This test re-checks the
    /// invariant: when the file reaches the policy's soft trigger with
    /// realistic fragmentation (we saw 63% in the wild), the gate must
    /// fire. Any future change that reintroduces a similar misalignment
    /// — bumping VACUUM_TRIGGER_FREE_BYTES too high, raising the
    /// fragmentation ratio threshold, lowering HISTORY_SOFT_MAX_BYTES —
    /// will trip this assertion.
    #[test]
    fn vacuum_gate_fires_at_policy_soft_trigger_with_realistic_fragmentation() {
        // ~63% fragmentation observed in the wild (250 MB free of 394 MB).
        // Picking 50% gives the same shape with some safety margin.
        let fragmentation_ratio = 0.50;
        let store_bytes = HISTORY_SOFT_MAX_BYTES;
        let page_size: u64 = 4096;
        let page_count = (store_bytes / page_size) as i64;
        let freelist_count = (page_count as f64 * fragmentation_ratio) as i64;
        let wal_bytes = 1024 * 1024;

        assert!(
            aetower_persistence::should_vacuum_decision(
                store_bytes,
                wal_bytes,
                page_count,
                freelist_count,
            ),
            "VACUUM gate must fire when the DB hits HISTORY_SOFT_MAX_BYTES \
             with significant fragmentation; otherwise aggressive maintenance \
             runs without ever reclaiming pages, leaving a growing freelist \
             (the original 394 MB / 63%-free pathology)."
        );
    }

    /// Companion to the above: an empty/healthy DB must NOT trigger VACUUM
    /// (which would be expensive and pointless). This guards against
    /// over-correcting in the other direction — lowering thresholds so
    /// aggressively that every small-DB maintenance pass rewrites the file.
    #[test]
    fn vacuum_gate_skips_small_or_healthy_databases() {
        let page_size: u64 = 4096;

        // Small DB, even heavily fragmented: not worth a rewrite.
        let store_bytes = 50 * 1024 * 1024;
        let page_count = (store_bytes / page_size) as i64;
        let freelist_count = (page_count as f64 * 0.50) as i64;
        assert!(
            !aetower_persistence::should_vacuum_decision(
                store_bytes,
                0,
                page_count,
                freelist_count
            ),
            "small DB ({store_bytes} bytes) at 50% fragmentation should not VACUUM — \
             absolute waste is below VACUUM_TRIGGER_FREE_BYTES"
        );

        // Large DB, slightly fragmented: also not worth a rewrite.
        let store_bytes = 1024 * 1024 * 1024;
        let page_count = (store_bytes / page_size) as i64;
        let freelist_count = (page_count as f64 * 0.05) as i64;
        assert!(
            !aetower_persistence::should_vacuum_decision(
                store_bytes,
                0,
                page_count,
                freelist_count
            ),
            "large DB at 5% fragmentation should not VACUUM — ratio gate prevents \
             rewriting a mostly-full file for marginal gain"
        );
    }

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
    fn classifies_previous_shutdown_as_low_noise_marker_keyed_by_code() {
        let marker = classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
            event_message: Some("Previous shutdown cause: -128".to_owned()),
        })
        .unwrap_or_else(|| panic!("shutdown marker"));

        assert_eq!(marker.event_type, "system-previous-shutdown-cause");
        assert_eq!(marker.level, DiagnosticsLevel::Info);
        assert_eq!(
            marker.marker_key.as_deref(),
            Some("previous-shutdown-cause:-128")
        );
    }

    #[test]
    fn ignores_marker_polling_noise_and_non_marker_shutdown_mentions() {
        assert!(classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
            event_message: Some(
                "#I Polling for the states of screen, locks, reachability status, ringer state, battery saver mode, thermal pressure level & in metro status".to_owned()
            ),
        })
        .is_none());

        assert!(
            classify_system_marker(UnifiedLogEntry {
                timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
                event_message: Some(
                    "Command line: /usr/bin/log show --predicate previous shutdown cause"
                        .to_owned()
                ),
            })
            .is_none()
        );
    }

    #[test]
    fn ignores_unified_log_self_scan_and_network_wake_noise() {
        assert!(classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
            event_message: Some(
                "log run noninteractively, parent: 80637 (Aetower), args: '/usr/bin/log' 'show' '--style' 'json' '--last' '12h' '--predicate' '(eventMessage CONTAINS[c] \"Entering Sleep state\")'"
                    .to_owned()
            ),
        })
        .is_none());

        assert!(classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
            event_message: Some(
                "[C11831 A45AB72D Hostname#00765b4e:443 tcp, fast-open, no wake from sleep] start"
                    .to_owned()
            ),
        })
        .is_none());

        assert!(classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
            event_message: Some(
                "isNaturalPlatform: true, isNeuralPlatform: true, thermal level: <private>, low power mode: false"
                    .to_owned()
            ),
        })
        .is_none());
    }

    #[test]
    fn caps_unified_log_lookback_for_system_marker_scan() {
        assert_eq!(unified_log_last_arg_for_elapsed(1), "1m");
        assert_eq!(unified_log_last_arg_for_elapsed(10 * 60 * 1000), "10m");
        assert_eq!(unified_log_last_arg_for_elapsed(12 * 60 * 60 * 1000), "90m");
    }

    #[test]
    fn telemetry_overrides_reset_blank_endpoint_to_default() {
        let current = OtlpConfig {
            endpoint: "http://custom.invalid/v1/metrics".to_owned(),
            export_interval_secs: 60,
            enabled: true,
        };

        let configured = telemetry_config_with_overrides(&current, None, false, 1);

        assert_eq!(configured.endpoint, OtlpConfig::default().endpoint);
        assert!(!configured.enabled);
        assert_eq!(configured.export_interval_secs, 5);
    }

    #[test]
    fn telemetry_overrides_trim_explicit_endpoint() {
        let current = OtlpConfig::default();

        let configured = telemetry_config_with_overrides(
            &current,
            Some("  http://collector.local/v1/metrics  ".to_owned()),
            true,
            15,
        );

        assert_eq!(configured.endpoint, "http://collector.local/v1/metrics");
        assert!(configured.enabled);
        assert_eq!(configured.export_interval_secs, 15);
    }

    #[test]
    fn downgrades_nominal_thermal_markers() {
        let marker = classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
            event_message: Some("current thermal pressure level=0".to_owned()),
        });
        assert!(marker.is_none());

        let nominal = classify_system_marker(UnifiedLogEntry {
            timestamp: Some("2026-04-13 19:03:09.753950+0200".to_owned()),
            event_message: Some("Thermal pressure changed to nominal".to_owned()),
        })
        .unwrap_or_else(|| panic!("thermal marker"));
        assert_eq!(nominal.level, DiagnosticsLevel::Info);
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

    #[test]
    fn host_incident_snapshots_aggregate_repeated_pressure() {
        let diagnostics = DiagnosticsStore::new(16);
        let mut state = BTreeMap::<&'static str, PersistedHostIncidentState>::new();
        let mut snapshot = SystemSnapshot {
            sequence: 1,
            captured_at_millis: 1_000,
            host: HostSnapshot {
                memory_used_bytes: 15 * 1024 * 1024 * 1024,
                memory_total_bytes: 16 * 1024 * 1024 * 1024,
                compressed_memory_bytes: 7 * 1024 * 1024 * 1024,
                swap_used_bytes: 20 * 1024 * 1024 * 1024,
                ..HostSnapshot::default()
            },
            ..SystemSnapshot::default()
        };

        emit_host_incident_snapshots(&diagnostics, &snapshot, &mut state);
        snapshot.sequence = 2;
        snapshot.captured_at_millis = 2_000;
        emit_host_incident_snapshots(&diagnostics, &snapshot, &mut state);
        snapshot.sequence = 3;
        snapshot.captured_at_millis = 61 * 60 * 1000;
        emit_host_incident_snapshots(&diagnostics, &snapshot, &mut state);

        let events = diagnostics.recent(10);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence, Some(3));
        assert!(
            events[0]
                .fields
                .iter()
                .any(|field| field.key == "suppressed_count" && field.value == "1")
        );
    }

    #[test]
    fn mcp_helper_lifecycle_emits_on_count_and_age_changes() {
        let diagnostics = DiagnosticsStore::new(16);
        let mut state = McpHelperLifecycleState::default();

        emit_mcp_helper_lifecycle(&diagnostics, 1_000, 1, 0, 0, 0, &mut state);
        emit_mcp_helper_lifecycle(&diagnostics, 2_000, 2, 2, 0, 5 * 60 * 1000, &mut state);
        emit_mcp_helper_lifecycle(&diagnostics, 3_000, 3, 2, 0, 6 * 60 * 1000, &mut state);
        emit_mcp_helper_lifecycle(
            &diagnostics,
            16 * 60 * 1000,
            4,
            2,
            0,
            16 * 60 * 1000,
            &mut state,
        );
        emit_mcp_helper_lifecycle(
            &diagnostics,
            17 * 60 * 1000,
            5,
            2,
            1,
            17 * 60 * 1000,
            &mut state,
        );

        let events = diagnostics.recent(10);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].level, DiagnosticsLevel::Warn);
        assert_eq!(events[0].event_type, "mcp-helper-lifecycle");
        assert_eq!(events[1].sequence, Some(4));
        assert_eq!(events[2].sequence, Some(2));
    }

    #[test]
    fn summarizes_mcp_helpers_and_flags_only_orphaned_stale_ones() {
        fn sample(
            pid: u32,
            parent_pid: Option<u32>,
            start_time_millis: u64,
            name: &str,
        ) -> crate::collector::RawProcessSample {
            crate::collector::RawProcessSample {
                pid,
                parent_pid,
                start_time_millis,
                name: name.to_owned(),
                exe: Some(format!("/tmp/{name}")),
                cmd: Vec::new(),
                cpu_percent: 0.0,
                memory_bytes: 0,
                memory_physical_footprint_bytes: 0,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                energy_nj_per_s: 0.0,
                cwd: None,
                user: None,
                thread_count: 0,
            }
        }

        let processes = vec![
            sample(10, Some(1), 1_000, "parent-client"),
            sample(11, Some(1), 1_000, "aetower-mcp"),
            sample(12, Some(1), 800_000, "aetower-mcp"),
            sample(13, Some(10), 1_000, "aetower-mcp"),
        ];
        let process_ids = process_id_set(&processes);
        let (total, stale, oldest_age) = summarize_mcp_helpers(&processes, 1_000_000, &process_ids);
        assert_eq!(total, 3);
        assert_eq!(stale, 1);
        assert_eq!(oldest_age, 999_000);
    }
}
