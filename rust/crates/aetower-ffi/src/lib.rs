use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use aetower_core::{Engine, RuntimeCollectionSettings};
use aetower_diagnostics as diagnostics;
use aetower_mcp::{
    AetowerMcpDataSource, HistorySummaryResponse, LocalMcpServerHandle, default_socket_path,
    diff_snapshots_json, entity_process_tree_json, explain_anomalies_json, filter_entities_json,
    is_socket_listener_reachable, memory_breakdown_json, persistence_deep_scan_json,
    persistence_scan_json, process_action_history_json, process_action_json, process_inspect_json,
    process_open_resources_json, process_sample_json, profile_entity_json,
    resource_holders_by_file_json, resource_holders_by_port_json, self_memory_attribution_json,
    start_local_socket_server, wakeup_attribution_json,
};
use aetower_model as model;

uniffi::setup_scaffolding!();

#[derive(Clone, Debug, uniffi::Enum)]
pub enum EntityKind {
    App,
    Browser,
    Daemon,
    TerminalSession,
    Service,
    AiAgent,
    Unknown,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ComponentKind {
    Process,
    Command,
    AdapterContext,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum CapabilityKind {
    Accessibility,
    FullDiskAccess,
    AppleAutomation,
    ChromiumDebug,
    DockerSocket,
    PrivilegedHelper,
    Chau7,
    EndpointSecurity,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum CapabilityState {
    Unknown,
    Granted,
    Denied,
    Requested,
    Unavailable,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum CapabilityHealth {
    Configured,
    Live,
    Cached,
    Degraded,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum DiagnosticsLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum DiagnosticsSubsystem {
    Engine,
    Collector,
    Identity,
    Attribution,
    Friction,
    History,
    Persistence,
    Telemetry,
    Gpu,
    Ffi,
    Ui,
    AdapterChromium,
    AdapterDocker,
    AdapterHelper,
    AdapterChau7,
    AdapterVsCode,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ThermalState {
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum AttributionConfidence {
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum TimelineSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum TimelineCategory {
    Lifecycle,
    Friction,
    Host,
    Thermal,
    Anomaly,
    Network,
    Regression,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ProvenanceKind {
    UserLaunch,
    AppBundle,
    HelperTree,
    ShellSession,
    LoginItem,
    ServiceManager,
    XpcService,
    BrowserContext,
    ContainerWorkload,
    ParentProcess,
    Unknown,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ProvenanceSnapshot {
    pub kind: ProvenanceKind,
    pub label: String,
    pub rule: String,
    pub confidence: AttributionConfidence,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum AdapterContextKind {
    ChromiumTab,
    DockerContainer,
    PrivilegedSocket,
    Chau7Session,
    VsCodeWorkspace,
    VsCodeRuntime,
    Unknown,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct AdapterContextSnapshot {
    pub kind: AdapterContextKind,
    pub status: Option<String>,
    pub url: Option<String>,
    pub workspace_path: Option<String>,
    pub repo_root: Option<String>,
    pub image_name: Option<String>,
    pub session_id: Option<String>,
    pub app_version: Option<String>,
    pub build_sha: Option<String>,
    pub build_timestamp: Option<String>,
    pub build_channel: Option<String>,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub memory_limit_bytes: u64,
    pub js_heap_total_bytes: u64,
    pub dom_nodes: u64,
    pub documents: u64,
    pub frames: u64,
    pub process_count: Option<u32>,
    pub connection_count: Option<u32>,
    pub ports: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HostSnapshot {
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub compressed_memory_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    pub wakeups_per_second: f32,
    pub thermal_state: ThermalState,
    pub on_battery: bool,
    pub battery_charge_percent: Option<u8>,
    pub low_power_mode: bool,
    pub frontmost_app_name: Option<String>,
    pub frontmost_window_title: Option<String>,
    pub ai_agent_friction: f32,
    pub ai_agent_count: u32,
    pub gpu_percent: f32,
    pub ane_percent: f32,
    pub gpu_memory_bytes: u64,
    pub gpu_temperature_celsius: Option<f32>,
    pub fans: Vec<FanReading>,
    pub cpu_temperatures: Vec<TemperatureReading>,
    pub power_readings: Vec<PowerReading>,
    pub battery_health: Option<BatteryHealthSnapshot>,
    pub boot_session: Option<BootSessionSnapshot>,
    pub network_interfaces: Vec<NetworkInterfaceSnapshot>,
    pub disks: Vec<DiskHealthSnapshot>,
    pub bluetooth_devices: Vec<BluetoothDeviceBattery>,
    pub per_core_cpu: Vec<CoreLoad>,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum CoreKind {
    Unknown,
    Performance,
    Efficiency,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct CoreLoad {
    pub index: u32,
    pub percent: f32,
    pub kind: CoreKind,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FanReading {
    pub id: u8,
    pub name: String,
    pub current_rpm: f32,
    pub min_rpm: f32,
    pub max_rpm: f32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TemperatureReading {
    pub label: String,
    pub celsius: f32,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum PowerUnit {
    Watts,
    Volts,
    Amps,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PowerReading {
    pub label: String,
    pub value: f32,
    pub unit: PowerUnit,
}

/// macOS battery condition as reported by IOPS/System Information.
///
/// Mirrors `aetower_model::BatteryCondition` for the UniFFI bridge.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum BatteryCondition {
    Unknown,
    Good,
    Fair,
    Poor,
    ServiceBattery,
}

/// Long-lived battery health metrics exposed to Swift.
///
/// All values are derived in a single IOPSCopyPowerSourcesInfo pass. Every
/// numeric field is `Option<…>` so the Swift UI can tell "the IOKit key
/// was not reported this tick" (`nil`) apart from a genuinely-zero
/// reading (`0`). Before this change the Rust side silently conflated
/// "missing" with "zero", which made desktop Macs indistinguishable from
/// a brand-new laptop battery.
#[derive(Clone, Debug, uniffi::Record)]
pub struct BatteryHealthSnapshot {
    pub cycle_count: Option<u32>,
    pub design_capacity_mah: Option<u32>,
    pub max_capacity_mah: Option<u32>,
    pub health_percent: Option<f32>,
    pub condition: BatteryCondition,
    pub temperature_celsius: Option<f32>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct BootSessionSnapshot {
    pub boot_id: Option<String>,
    pub boot_time_millis: Option<u64>,
    pub host_uptime_millis: Option<u64>,
    pub previous_shutdown: Option<RebootCauseSnapshot>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RebootCauseSnapshot {
    pub source: String,
    pub code: Option<String>,
    pub detail: String,
    pub observed_at_millis: Option<u64>,
}

/// Per-interface network snapshot exposed to Swift.
///
/// `mac_address` is an empty string when the kernel does not expose one
/// (loopback, tunnels, some virtual adapters). `is_up` is `true` when the
/// interface holds at least one assigned IP at sample time.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NetworkInterfaceSnapshot {
    pub name: String,
    pub mac_address: String,
    pub receive_bps: u64,
    pub send_bps: u64,
    pub is_up: bool,
}

/// Overall disk health classification exposed to Swift.
///
/// Mirrors `aetower_model::DiskHealthStatus`. `Warning` is an Aetower-only
/// escalation from the macOS `Verified` state when wear indicators suggest
/// the drive is nearing end-of-life.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum DiskHealthStatus {
    Unknown,
    Healthy,
    Warning,
    Failing,
    NotSupported,
}

/// SMART snapshot for one physical disk.
///
/// Optional counters stay `None` when `diskutil` does not expose the key —
/// the UI should render "—" rather than inventing a zero. `total_size_bytes`
/// is included so the UI can present "512 GB SSD" alongside the health state.
#[derive(Clone, Debug, uniffi::Record)]
pub struct DiskHealthSnapshot {
    pub device_identifier: String,
    pub model: String,
    pub total_size_bytes: u64,
    pub status: DiskHealthStatus,
    pub temperature_celsius: Option<f32>,
    pub percentage_used: Option<u8>,
    pub available_spare_percent: Option<u8>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub media_errors: Option<u64>,
}

/// Broad category used by the UI to pick the right icon for a Bluetooth
/// peripheral.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum BluetoothDeviceType {
    Other,
    Keyboard,
    Mouse,
    Trackpad,
    Headphones,
    GameController,
}

/// Battery level for one wireless peripheral.
///
/// `battery_percent` is `Option` because a paired-but-disconnected device
/// still shows up in the ioreg tree without a current value. The UI should
/// display "—" for those rows rather than hiding them.
#[derive(Clone, Debug, uniffi::Record)]
pub struct BluetoothDeviceBattery {
    pub name: String,
    pub address: String,
    pub battery_percent: Option<u8>,
    pub device_type: BluetoothDeviceType,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HostTrend {
    pub machine_friction: Vec<f32>,
    pub cpu_percent: Vec<f32>,
    pub memory_used_bytes: Vec<u64>,
    pub memory_pressure_score: Vec<f32>,
    pub disk_activity_bps: Vec<u64>,
    pub network_activity_bps: Vec<u64>,
    pub wakeups_per_second: Vec<f32>,
    pub compressed_memory_bytes: Vec<u64>,
    pub ai_agent_friction: Vec<f32>,
    pub gpu_percent: Vec<f32>,
    pub gpu_memory_bytes: Vec<u64>,
    pub max_cpu_temperature: Vec<f32>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiLagMetrics {
    pub updated_at_millis: u64,
    pub bridge_fetch_millis: f32,
    pub ui_refresh_millis: f32,
    pub snapshot_to_ui_millis: f32,
    pub snapshot_to_render_millis: f32,
    pub render_commit_millis: f32,
    pub display_frame_interval_millis: f32,
    pub display_refresh_hz: f32,
    pub display_dropped_frames: u64,
    pub input_avg_latency_millis: f32,
    pub input_max_latency_millis: f32,
    pub input_sample_count: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RuntimeLagMetrics {
    pub updated_at_millis: u64,
    pub engine_tick_millis: f32,
    pub collect_millis: f32,
    pub identity_millis: f32,
    pub attribution_millis: f32,
    pub friction_millis: f32,
    pub enrich_millis: f32,
    pub history_millis: f32,
    pub persist_millis: f32,
    pub gpu_sample_millis: f32,
    pub target_tick_millis: f32,
    pub history_queue_depth: u32,
    pub diagnostics_queue_depth: u32,
    pub mcp_helper_count: u32,
    pub stale_mcp_helper_count: u32,
    pub oldest_mcp_helper_age_millis: u64,
    pub self_cpu_percent: f32,
    pub self_memory_bytes: u64,
    pub self_memory_physical_footprint_bytes: u64,
    pub self_wakeups_per_second: f32,
    pub self_energy_nj_per_s: f64,
    pub mcp_total_connections: u64,
    pub mcp_active_client_count: u32,
    pub mcp_total_requests: u64,
    pub mcp_requests_per_second: f32,
    pub mcp_observed_at_millis: u64,
    pub bridge_fetch_millis: f32,
    pub ui_refresh_millis: f32,
    pub snapshot_to_ui_millis: f32,
    pub snapshot_to_render_millis: f32,
    pub render_commit_millis: f32,
    pub display_frame_interval_millis: f32,
    pub display_refresh_hz: f32,
    pub display_dropped_frames: u64,
    pub input_avg_latency_millis: f32,
    pub input_max_latency_millis: f32,
    pub input_sample_count: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RuntimeCollectionSettingsInput {
    pub full_collection: bool,
    pub adaptive_cadence: bool,
    pub active_tick_millis: u64,
    pub idle_tick_millis: u64,
    pub low_power_tick_millis: u64,
    pub gpu_sample_interval_millis: u64,
    pub gpu_sample_low_power_interval_millis: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct AggregateMetrics {
    pub cpu_percent: f32,
    pub memory_resident_bytes: u64,
    pub memory_physical_footprint_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    pub wakeups_per_second: f32,
    /// Per-second energy draw in nanojoules (= nanowatts), summed across
    /// all processes belonging to this entity. Zero when the kernel does
    /// not report energy. The UI can format as milliwatts (`/ 1e6`) or
    /// watts (`/ 1e9`) depending on magnitude.
    pub energy_nj_per_s: f64,
    /// Heuristic GPU share attributed to this entity (0-100). Only non-zero
    /// for AI agent entities; zero for everything else.
    pub estimated_gpu_percent: f32,
    pub process_count: u32,
    pub thread_count: u32,
    pub is_foreground: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FrictionContributor {
    pub key: String,
    pub label: String,
    pub score: f32,
    pub detail: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FrictionBreakdown {
    pub total_score: f32,
    pub cpu_score: f32,
    pub memory_score: f32,
    pub disk_score: f32,
    pub network_score: f32,
    pub wakeups_score: f32,
    pub pressure_score: f32,
    pub foreground_bonus: f32,
    pub energy_impact_score: f32,
    pub reasons: Vec<String>,
    pub contributors: Vec<FrictionContributor>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ComponentSnapshot {
    pub kind: ComponentKind,
    pub title: String,
    pub detail: String,
    pub adapter_context: Option<AdapterContextSnapshot>,
    pub provenance: Option<ProvenanceSnapshot>,
    pub process_id: Option<u32>,
    pub start_time_millis: u64,
    pub executable_path: Option<String>,
    pub command_line: Option<String>,
    pub parent_summary: Option<String>,
    pub launched_by: Option<String>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub memory_physical_footprint_bytes: u64,
    pub cwd: Option<String>,
    pub user: Option<String>,
    pub thread_count: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MetricTrend {
    pub friction: Vec<f32>,
    pub cpu_percent: Vec<f32>,
    pub memory_resident_bytes: Vec<u64>,
    pub disk_activity_bps: Vec<u64>,
    pub network_activity_bps: Vec<u64>,
    pub wakeups_per_second: Vec<f32>,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum RecommendationSeverity {
    Info,
    Suggested,
    Urgent,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Recommendation {
    pub title: String,
    pub detail: String,
    pub severity: RecommendationSeverity,
    pub suggested_action: Option<String>,
    pub target_pid: Option<u32>,
    pub target_label: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct AgentCostSummary {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub cost_usd: f32,
    pub total_runs: u32,
    /// Cumulative energy in nanojoules for this AI session. The UI can
    /// convert: nJ → Wh (÷ 3.6e12), Wh → mAh at 3.7V (Wh / 3.7 * 1000).
    pub session_energy_nj: u64,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum SessionMarkerKind {
    RunStart,
    RunEnd,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SessionMarker {
    pub timestamp_millis: u64,
    pub kind: SessionMarkerKind,
    pub label: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct EntitySnapshot {
    pub entity_id: String,
    pub display_name: String,
    pub primary_provenance: Option<ProvenanceSnapshot>,
    pub launcher_summary: Option<String>,
    pub attribution_notes: Vec<String>,
    pub bundle_id: Option<String>,
    pub executable_path: Option<String>,
    pub oldest_process_start_millis: u64,
    pub newest_process_start_millis: u64,
    pub entity_kind: EntityKind,
    pub metrics: AggregateMetrics,
    pub friction: FrictionBreakdown,
    pub components: Vec<ComponentSnapshot>,
    pub trend: MetricTrend,
    pub badges: Vec<String>,
    pub active_window_title: Option<String>,
    pub recent_change_summary: Option<String>,
    pub anomaly_detected: bool,
    pub thermal_contribution: Option<String>,
    pub grouping_suggestion: Option<String>,
    pub agent_cost: Option<AgentCostSummary>,
    pub session_markers: Vec<SessionMarker>,
    pub recommendations: Vec<Recommendation>,
    pub network_connections: Vec<NetworkConnection>,
    pub signing_classification: String,
    pub is_adhoc: bool,
    pub binary_reputation: Option<BinaryReputation>,
    pub app_version: Option<String>,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ReputationVerdict {
    Clean,
    Suspicious,
    Malicious,
    Unknown,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct BinaryReputation {
    pub sha256: String,
    pub malicious: u32,
    pub suspicious: u32,
    pub harmless: u32,
    pub undetected: u32,
    pub total_engines: u32,
    pub verdict: ReputationVerdict,
    pub permalink: String,
    pub checked_at_millis: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NetworkConnection {
    pub protocol: String,
    pub local: String,
    pub remote: Option<String>,
    pub state: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct CapabilitySnapshot {
    pub kind: CapabilityKind,
    pub state: CapabilityState,
    pub health: CapabilityHealth,
    pub detail: String,
    pub last_updated_millis: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TimelineEvent {
    pub id: String,
    pub timestamp_millis: u64,
    pub category: TimelineCategory,
    pub severity: TimelineSeverity,
    pub entity_id: Option<String>,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SystemSnapshot {
    pub sequence: u64,
    pub captured_at_millis: u64,
    pub host: HostSnapshot,
    pub host_trend: HostTrend,
    pub capabilities: Vec<CapabilitySnapshot>,
    pub entities: Vec<EntitySnapshot>,
    pub timeline: Vec<TimelineEvent>,
    pub ai_repo_summaries: Vec<AiRepoSummary>,
    pub chau7_sessions: Vec<Chau7SessionSummary>,
    pub thermal_forecast: Option<ThermalForecast>,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum UiMetricSeverity {
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiMetricCard {
    pub id: String,
    pub title: String,
    pub value: f64,
    pub unit: String,
    pub display_value: String,
    pub detail: String,
    pub severity: UiMetricSeverity,
    pub samples: Vec<f64>,
    pub fixed_ceiling: Option<f64>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiHostSummary {
    pub machine_friction: f32,
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub compressed_memory_bytes: u64,
    pub swap_used_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    pub wakeups_per_second: f32,
    pub gpu_percent: f32,
    pub gpu_memory_bytes: u64,
    pub thermal_state: ThermalState,
    pub on_battery: bool,
    pub low_power_mode: bool,
    pub frontmost_app_name: Option<String>,
    pub frontmost_window_title: Option<String>,
    pub ai_agent_count: u32,
    pub entity_count: u32,
    pub process_count: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiHostTrend {
    pub machine_friction: Vec<f64>,
    pub cpu_percent: Vec<f64>,
    pub memory_used_bytes: Vec<f64>,
    pub memory_pressure_score: Vec<f64>,
    pub disk_activity_bps: Vec<f64>,
    pub network_activity_bps: Vec<f64>,
    pub wakeups_per_second: Vec<f64>,
    pub gpu_percent: Vec<f64>,
    pub gpu_memory_bytes: Vec<f64>,
    pub max_cpu_temperature: Vec<f64>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiProcessRow {
    pub entity_id: String,
    pub display_name: String,
    pub entity_kind: EntityKind,
    pub bundle_id: Option<String>,
    pub executable_path: Option<String>,
    pub primary_pid: Option<u32>,
    pub process_count: u32,
    pub thread_count: u32,
    pub cpu_percent: f32,
    pub memory_resident_bytes: u64,
    pub memory_physical_footprint_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    pub wakeups_per_second: f32,
    pub energy_nj_per_s: f64,
    pub estimated_gpu_percent: f32,
    pub friction_score: f32,
    pub is_foreground: bool,
    pub anomaly_detected: bool,
    pub active_window_title: Option<String>,
    pub recent_change_summary: Option<String>,
    pub signing_classification: String,
    pub is_adhoc: bool,
    pub app_version: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiProcessComponent {
    pub title: String,
    pub detail: String,
    pub process_id: Option<u32>,
    pub start_time_millis: u64,
    pub executable_path: Option<String>,
    pub command_line: Option<String>,
    pub parent_summary: Option<String>,
    pub launched_by: Option<String>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub memory_physical_footprint_bytes: u64,
    pub cwd: Option<String>,
    pub user: Option<String>,
    pub thread_count: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiSelectedEntity {
    pub row: UiProcessRow,
    pub components: Vec<UiProcessComponent>,
    pub trend: MetricTrend,
    pub recommendations: Vec<Recommendation>,
    pub network_connections: Vec<NetworkConnection>,
    pub badges: Vec<String>,
    pub attribution_notes: Vec<String>,
    pub thermal_contribution: Option<String>,
    pub grouping_suggestion: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiSnapshot {
    pub sequence: u64,
    pub captured_at_millis: u64,
    pub host: UiHostSummary,
    pub host_trend: UiHostTrend,
    pub metric_cards: Vec<UiMetricCard>,
    pub process_rows: Vec<UiProcessRow>,
    pub selected_entity: Option<UiSelectedEntity>,
    pub total_entity_count: u32,
    pub returned_entity_count: u32,
    pub total_process_count: u32,
    pub timeline_warning_count: u32,
    pub timeline_critical_count: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UiSnapshotDelta {
    pub updated: bool,
    pub sequence: u64,
    pub base_sequence: u64,
    pub captured_at_millis: u64,
    pub process_limit: u32,
    pub trend_points: u32,
    pub base_available: bool,
    pub host: Option<UiHostSummary>,
    pub host_trend: Option<UiHostTrend>,
    pub changed_metric_cards: Vec<UiMetricCard>,
    pub changed_process_rows: Vec<UiProcessRow>,
    pub removed_entity_ids: Vec<String>,
    pub selected_entity: Option<UiSelectedEntity>,
    pub selected_entity_changed: bool,
    pub selected_entity_removed: bool,
    pub total_entity_count: u32,
    pub returned_entity_count: u32,
    pub total_process_count: u32,
    pub timeline_warning_count: u32,
    pub timeline_critical_count: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ThermalForecast {
    pub minutes_to_throttle: Option<f32>,
    pub trend_celsius_per_min: f32,
    pub state: ThermalState,
    pub top_contributor_entity_id: Option<String>,
    pub top_contributor_pid: Option<u32>,
    pub top_contributor_label: Option<String>,
}

/// Per-repository AI cost summary from the Chau7 adapter.
#[derive(Clone, Debug, uniffi::Record)]
pub struct AiRepoSummary {
    pub repo_path: String,
    pub display_name: String,
    pub total_runs: u32,
    pub total_tokens: u64,
    pub total_cost_usd: f32,
    pub providers: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Chau7SessionSummary {
    pub id: String,
    pub tab_id: Option<String>,
    pub session_id: Option<String>,
    pub title: String,
    pub provider: String,
    pub status: String,
    pub workspace_path: Option<String>,
    pub repo_root: Option<String>,
    pub git_branch: Option<String>,
    pub active_app: Option<String>,
    pub window_id: u32,
    pub run_count: u32,
    pub last_active: String,
    pub turn_count: u32,
    pub child_session_count: u32,
    pub pending_approval_description: Option<String>,
    pub last_exit_reason: Option<String>,
    pub active_run_duration_millis: u64,
    pub is_at_prompt: bool,
    pub shell_loading: bool,
    pub cto_active: bool,
    pub linked_entity_ids: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FrontmostAppState {
    pub app_name: String,
    pub bundle_id: Option<String>,
    pub executable_path: Option<String>,
    pub window_title: Option<String>,
    pub captured_at_millis: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DiagnosticsField {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DiagnosticsEvent {
    pub id: String,
    pub timestamp_millis: u64,
    pub level: DiagnosticsLevel,
    pub subsystem: DiagnosticsSubsystem,
    pub event_type: String,
    pub sequence: Option<u64>,
    pub entity_id: Option<String>,
    pub adapter: Option<String>,
    pub capability: Option<String>,
    pub message: String,
    pub fields: Vec<DiagnosticsField>,
    pub sensitive: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DiagnosticsOverview {
    pub ring_capacity: u32,
    pub current_size: u32,
    pub dropped_events: u64,
    pub error_count: u32,
    pub warn_count: u32,
    pub last_event_millis: Option<u64>,
    pub last_error_millis: Option<u64>,
    pub last_error_message: Option<String>,
    pub persisted_events: u64,
    pub persisted_path: Option<String>,
    pub persisted_bytes: u64,
    pub persistence_error: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DiagnosticsQuery {
    pub limit: u32,
    pub minimum_level: Option<DiagnosticsLevel>,
    pub subsystem: Option<DiagnosticsSubsystem>,
    pub search: Option<String>,
    pub since_millis: Option<u64>,
    pub include_persisted: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HistoryRangeSummary {
    pub store_bytes: u64,
    pub wal_bytes: u64,
    pub snapshot_count: u64,
    pub quarantine_count: u64,
    pub range_count: u64,
    pub oldest_millis: Option<u64>,
    pub newest_millis: Option<u64>,
    pub pending_writes: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HistoryRangeSummaryResult {
    pub summary: Option<HistoryRangeSummary>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HistoryPageLoadResult {
    pub snapshots: Vec<SystemSnapshot>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct JsonQueryResult {
    pub json: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HistoryMaintenanceReport {
    pub store_bytes_before: u64,
    pub wal_bytes_before: u64,
    pub store_bytes_after: u64,
    pub wal_bytes_after: u64,
    pub checkpointed: bool,
    pub vacuumed: bool,
    pub pruned_rows: u64,
    pub aggressive_reason: Option<String>,
    pub cancelled: bool,
    pub elapsed_millis: u64,
}

#[derive(uniffi::Object)]
pub struct MonitorEngine {
    inner: Arc<std::sync::Mutex<Engine>>,
    mcp_server: std::sync::Mutex<Option<LocalMcpServerHandle>>,
    ui_snapshot_cache: std::sync::Mutex<Vec<UiSnapshotCacheEntry>>,
}

#[uniffi::export]
impl MonitorEngine {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let mut engine = Engine::new();
        engine.start();
        Arc::new(Self {
            inner: Arc::new(std::sync::Mutex::new(engine)),
            mcp_server: std::sync::Mutex::new(None),
            ui_snapshot_cache: std::sync::Mutex::new(Vec::new()),
        })
    }

    pub fn latest_snapshot(&self) -> SystemSnapshot {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .latest_snapshot()
            .into()
    }

    pub fn latest_snapshot_if_newer(&self, last_sequence: u64) -> Option<SystemSnapshot> {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .latest_snapshot_if_newer(last_sequence)
            .map(Into::into)
    }

    pub fn latest_ui_snapshot(
        &self,
        process_limit: u32,
        trend_points: u32,
        selected_entity_id: Option<String>,
    ) -> UiSnapshot {
        let Ok(engine) = self.inner.lock() else {
            return ui_snapshot_from(
                &model::SystemSnapshot::default(),
                process_limit,
                trend_points,
                selected_entity_id.as_deref(),
            );
        };
        let snapshot = engine.latest_snapshot_arc();
        let ui_snapshot = ui_snapshot_from(
            snapshot.as_ref(),
            process_limit,
            trend_points,
            selected_entity_id.as_deref(),
        );
        self.remember_ui_snapshot(
            ui_snapshot_cache_key(process_limit, trend_points, selected_entity_id.as_deref()),
            &ui_snapshot,
        );
        ui_snapshot
    }

    pub fn latest_ui_snapshot_if_newer(
        &self,
        last_sequence: u64,
        process_limit: u32,
        trend_points: u32,
        selected_entity_id: Option<String>,
    ) -> Option<UiSnapshot> {
        let Ok(engine) = self.inner.lock() else {
            return None;
        };
        let snapshot = engine.latest_snapshot_arc_if_newer(last_sequence)?;
        let ui_snapshot = ui_snapshot_from(
            snapshot.as_ref(),
            process_limit,
            trend_points,
            selected_entity_id.as_deref(),
        );
        self.remember_ui_snapshot(
            ui_snapshot_cache_key(process_limit, trend_points, selected_entity_id.as_deref()),
            &ui_snapshot,
        );
        Some(ui_snapshot)
    }

    pub fn latest_ui_snapshot_delta_since(
        &self,
        last_sequence: u64,
        process_limit: u32,
        trend_points: u32,
        selected_entity_id: Option<String>,
    ) -> UiSnapshotDelta {
        let key = ui_snapshot_cache_key(process_limit, trend_points, selected_entity_id.as_deref());
        let Ok(engine) = self.inner.lock() else {
            return ui_snapshot_delta_no_update(
                &model::SystemSnapshot::default(),
                last_sequence,
                &key,
            );
        };
        let snapshot = engine.latest_snapshot_arc();
        if snapshot.sequence <= last_sequence {
            return ui_snapshot_delta_no_update(snapshot.as_ref(), last_sequence, &key);
        }

        let ui_snapshot = ui_snapshot_from(
            snapshot.as_ref(),
            key.process_limit,
            key.trend_points,
            key.selected_entity_id.as_deref(),
        );
        let base_snapshot = self.cached_ui_snapshot(last_sequence, &key);
        let delta = ui_snapshot_delta_from_current(
            last_sequence,
            &key,
            &ui_snapshot,
            base_snapshot.as_ref(),
        );
        self.remember_ui_snapshot(key, &ui_snapshot);
        delta
    }

    pub fn latest_sequence(&self) -> u64 {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .latest_sequence()
    }

    pub fn latest_runtime_lag_metrics(&self) -> RuntimeLagMetrics {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .latest_runtime_lag_metrics()
            .into()
    }

    pub fn set_capability_state(
        &self,
        kind: CapabilityKind,
        state: CapabilityState,
        detail_override: Option<String>,
    ) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .set_capability_state(kind.into(), state.into(), detail_override);
    }

    pub fn update_frontmost_app_state(&self, state: FrontmostAppState) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .update_frontmost_app_state(state.into());
    }

    pub fn clear_frontmost_app_state(&self) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .clear_frontmost_app_state();
    }

    pub fn configure_chromium_endpoint(&self, endpoint: Option<String>) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .configure_chromium_endpoint(endpoint);
    }

    pub fn configure_docker_socket_path(&self, socket_path: String) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .configure_docker_socket_path(socket_path);
    }

    pub fn configure_privileged_helper(&self, helper_path: Option<String>, enabled: bool) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .configure_privileged_helper(helper_path, enabled);
    }

    /// Pin a fan to a manual minimum RPM via the privileged helper.
    ///
    /// Returns an empty string on success or a human-readable error message
    /// on failure (privileged helper not configured, helper exit non-zero,
    /// SMC write failed). The UI surfaces the message directly to the user.
    pub fn set_fan_min_rpm(&self, fan_id: u8, rpm: f32) -> String {
        let Ok(engine) = self.inner.lock() else {
            return "engine lock poisoned".to_owned();
        };
        match engine.set_fan_min_rpm(fan_id, rpm) {
            Ok(()) => String::new(),
            Err(error) => error,
        }
    }

    /// Restore a fan to automatic (OS-controlled) mode.
    pub fn reset_fan_auto(&self, fan_id: u8) -> String {
        let Ok(engine) = self.inner.lock() else {
            return "engine lock poisoned".to_owned();
        };
        match engine.reset_fan_auto(fan_id) {
            Ok(()) => String::new(),
            Err(error) => error,
        }
    }

    pub fn configure_chau7_endpoint(&self, socket_path: Option<String>) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .configure_chau7_endpoint(socket_path);
    }

    /// Push binary-reputation consent + the VirusTotal API key into the engine.
    /// Pass an empty key to clear it. The key is session-only (never persisted
    /// by the engine).
    pub fn set_binary_reputation_config(&self, enabled: bool, api_key: String) {
        let Ok(engine) = self.inner.lock() else {
            return;
        };
        engine.set_binary_reputation_config(enabled, api_key);
    }

    pub fn configure_telemetry(
        &self,
        endpoint: Option<String>,
        enabled: bool,
        export_interval_secs: u32,
    ) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .configure_telemetry(endpoint, enabled, export_interval_secs);
    }

    pub fn verify_telemetry_export(&self) -> String {
        match self
            .inner
            .lock()
            .expect("engine lock poisoned")
            .verify_telemetry_export()
        {
            Ok(()) => String::new(),
            Err(error) => error,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn configure_runtime_collection(&self, settings: RuntimeCollectionSettingsInput) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .configure_runtime_collection(RuntimeCollectionSettings {
                full_collection: settings.full_collection,
                adaptive_cadence: settings.adaptive_cadence,
                active_tick_millis: settings.active_tick_millis,
                idle_tick_millis: settings.idle_tick_millis,
                low_power_tick_millis: settings.low_power_tick_millis,
                gpu_sample_interval_millis: settings.gpu_sample_interval_millis,
                gpu_sample_low_power_interval_millis: settings.gpu_sample_low_power_interval_millis,
            });
    }

    pub fn stop_agent_session(&self, session_id: String, force: bool) -> String {
        match self
            .inner
            .lock()
            .expect("engine lock poisoned")
            .stop_agent_session(session_id, force)
        {
            Ok(()) => String::new(),
            Err(error) => error,
        }
    }

    pub fn load_history_range(&self, start_millis: u64, end_millis: u64) -> Vec<SystemSnapshot> {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .load_history_range(start_millis, end_millis)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn load_history_page(
        &self,
        start_millis: u64,
        end_millis: u64,
        before_millis_exclusive: Option<u64>,
        limit: u32,
    ) -> Vec<SystemSnapshot> {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .load_history_page(start_millis, end_millis, before_millis_exclusive, limit)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn history_range_summary(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Option<HistoryRangeSummary> {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .history_range_summary(start_millis, end_millis)
            .map(Into::into)
    }

    pub fn history_range_summary_result(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> HistoryRangeSummaryResult {
        let engine = match self.inner.lock() {
            Ok(engine) => engine,
            Err(_) => {
                return HistoryRangeSummaryResult {
                    summary: None,
                    error_message: Some("engine lock poisoned".to_owned()),
                };
            }
        };
        match engine.try_history_range_summary(start_millis, end_millis) {
            Ok(summary) => HistoryRangeSummaryResult {
                summary: Some(summary.into()),
                error_message: None,
            },
            Err(error) => HistoryRangeSummaryResult {
                summary: None,
                error_message: Some(error),
            },
        }
    }

    pub fn load_history_page_result(
        &self,
        start_millis: u64,
        end_millis: u64,
        before_millis_exclusive: Option<u64>,
        limit: u32,
    ) -> HistoryPageLoadResult {
        let engine = match self.inner.lock() {
            Ok(engine) => engine,
            Err(_) => {
                return HistoryPageLoadResult {
                    snapshots: Vec::new(),
                    error_message: Some("engine lock poisoned".to_owned()),
                };
            }
        };
        match engine.try_load_history_page(start_millis, end_millis, before_millis_exclusive, limit)
        {
            Ok(snapshots) => HistoryPageLoadResult {
                snapshots: snapshots.into_iter().map(Into::into).collect(),
                error_message: None,
            },
            Err(error) => HistoryPageLoadResult {
                snapshots: Vec::new(),
                error_message: Some(error),
            },
        }
    }

    pub fn maintain_history_store(&self, aggressive: bool) -> Option<HistoryMaintenanceReport> {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .maintain_history_store(aggressive)
            .map(Into::into)
    }

    pub fn latest_diagnostics(&self, limit: u32) -> Vec<DiagnosticsEvent> {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .latest_diagnostics(limit as usize)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn query_diagnostics(&self, query: DiagnosticsQuery) -> Vec<DiagnosticsEvent> {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .query_diagnostics(query.into())
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn diagnostics_overview(&self) -> DiagnosticsOverview {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .diagnostics_overview()
            .into()
    }

    pub fn clear_diagnostics(&self) -> String {
        match self
            .inner
            .lock()
            .expect("engine lock poisoned")
            .clear_diagnostics()
        {
            Ok(()) => String::new(),
            Err(error) => error,
        }
    }

    pub fn clear_history(&self) -> String {
        match self
            .inner
            .lock()
            .expect("engine lock poisoned")
            .clear_history()
        {
            Ok(()) => String::new(),
            Err(error) => error,
        }
    }

    pub fn export_diagnostics_json(&self, limit: u32) -> String {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .export_diagnostics_json(limit as usize)
    }

    pub fn export_diagnostics_query_json(&self, query: DiagnosticsQuery) -> String {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .export_diagnostics_query_json(query.into())
    }

    pub fn record_diagnostics_event(&self, event: DiagnosticsEvent) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .record_diagnostics_event(event.into());
    }

    pub fn update_ui_lag_metrics(&self, metrics: UiLagMetrics) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .update_ui_lag_metrics(metrics.into());
    }

    pub fn export_snapshot_json(&self) -> String {
        let snapshot = self
            .inner
            .lock()
            .expect("engine lock poisoned")
            .latest_snapshot();
        serde_json::to_string_pretty(&snapshot).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    /// Explicit teardown for the local MCP server. Dropping the handle
    /// signals the accept loop, joins client threads, and unlinks the
    /// socket file. Callers should invoke this from their app-lifecycle
    /// shutdown hook (e.g. applicationWillTerminate) because SwiftUI
    /// @State drop is not guaranteed to run under NSApp.terminate().
    pub fn stop_local_mcp_server(&self) {
        if let Ok(mut slot) = self.mcp_server.lock() {
            let _ = slot.take();
        }
    }

    pub fn start_local_mcp_server(&self, socket_path: Option<String>) -> String {
        let resolved_path = socket_path
            .map(std::path::PathBuf::from)
            .unwrap_or_else(default_socket_path);
        if let Ok(mut slot) = self.mcp_server.lock() {
            if let Some(existing) = slot.as_ref()
                && existing.socket_path() == resolved_path.as_path()
                && is_socket_listener_reachable(&resolved_path)
            {
                return String::new();
            }
            let _ = slot.take();
        }
        let data_source = Arc::new(MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        });
        match start_local_socket_server(data_source, &resolved_path) {
            Ok(handle) => {
                let path_display = resolved_path.display().to_string();
                if let Ok(mut slot) = self.mcp_server.lock() {
                    *slot = Some(handle);
                }
                if let Ok(engine) = self.inner.lock() {
                    engine.record_diagnostics_event(
                        diagnostics::DiagnosticsEvent::builder(
                            diagnostics::DiagnosticsLevel::Info,
                            diagnostics::DiagnosticsSubsystem::Ffi,
                            "mcp-server-started",
                            "Started local MCP server",
                        )
                        .field("path", path_display)
                        .build(),
                    );
                }
                String::new()
            }
            Err(error) => {
                let path_display = resolved_path.display().to_string();
                if let Ok(engine) = self.inner.lock() {
                    engine.record_diagnostics_event(
                        diagnostics::DiagnosticsEvent::builder(
                            diagnostics::DiagnosticsLevel::Error,
                            diagnostics::DiagnosticsSubsystem::Ffi,
                            "mcp-server-start-failed",
                            "Failed to start local MCP server",
                        )
                        .field("path", path_display)
                        .field("error", &error)
                        .build(),
                    );
                }
                error
            }
        }
    }

    pub fn diff_snapshots_json(
        &self,
        before_millis: u64,
        after_millis: u64,
        entity_ids: Vec<String>,
        limit: u32,
    ) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(diff_snapshots_json(
            &data_source,
            before_millis,
            after_millis,
            &entity_ids,
            limit as usize,
        ))
    }

    pub fn explain_anomalies_json(
        &self,
        entity_ids: Vec<String>,
        limit: u32,
        window_minutes: u32,
    ) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(explain_anomalies_json(
            &data_source,
            &entity_ids,
            limit as usize,
            window_minutes as u64,
        ))
    }

    pub fn entity_process_tree_json(&self, entity_id: String) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(entity_process_tree_json(&data_source, &entity_id))
    }

    pub fn memory_breakdown_json(&self, entity_id: String, top_regions: u32) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(memory_breakdown_json(
            &data_source,
            &entity_id,
            top_regions as usize,
        ))
    }

    pub fn self_memory_attribution_json(&self, top_regions: u32) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(self_memory_attribution_json(
            &data_source,
            top_regions as usize,
        ))
    }

    pub fn profile_entity_json(
        &self,
        entity_id: String,
        duration_seconds: u32,
        top_stacks: u32,
    ) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(profile_entity_json(
            &data_source,
            &entity_id,
            duration_seconds as u64,
            top_stacks as usize,
        ))
    }

    pub fn wakeup_attribution_json(
        &self,
        entity_id: String,
        duration_seconds: u32,
        top_stacks: u32,
    ) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(wakeup_attribution_json(
            &data_source,
            &entity_id,
            duration_seconds as u64,
            top_stacks as usize,
        ))
    }

    pub fn process_inspect_json(&self, pid: u32) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(process_inspect_json(&data_source, pid))
    }

    /// Enumerate the machine's startup/persistence surface (launchd, login
    /// items, cron) with cheap file metadata. On-demand; no data source needed.
    pub fn persistence_scan_json(&self) -> JsonQueryResult {
        json_query_result(persistence_scan_json())
    }

    /// Deep startup/persistence audit with code-signing enrichment. Explicit
    /// because it may fork one codesign check per unique executable.
    pub fn persistence_deep_scan_json(&self) -> JsonQueryResult {
        json_query_result(persistence_deep_scan_json())
    }

    /// Evaluate a sandboxed Rhai filter expression against the latest snapshot,
    /// returning matched entity ids / pids as JSON.
    pub fn filter_entities_json(&self, expression: String) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(filter_entities_json(&data_source, &expression))
    }

    pub fn process_open_resources_json(&self, pid: u32, limit: u32) -> JsonQueryResult {
        json_query_result(process_open_resources_json(pid, limit as usize))
    }

    /// Reverse pivot: list every process holding the given TCP/UDP port.
    pub fn resource_holders_by_port_json(&self, port: u32) -> JsonQueryResult {
        if port == 0 || port > u32::from(u16::MAX) {
            return json_query_result(Err("port must be between 1 and 65535".to_owned()));
        }
        json_query_result(resource_holders_by_port_json(port as u16))
    }

    /// Reverse pivot: list every process holding the given file path.
    pub fn resource_holders_by_file_json(&self, path: String) -> JsonQueryResult {
        json_query_result(resource_holders_by_file_json(&path))
    }

    pub fn process_sample_json(
        &self,
        pid: u32,
        duration_seconds: u32,
        top_stacks: u32,
    ) -> JsonQueryResult {
        json_query_result(process_sample_json(
            pid,
            duration_seconds as u64,
            top_stacks as usize,
        ))
    }

    pub fn process_action_json(
        &self,
        pid: u32,
        action: String,
        dry_run: bool,
        reason: Option<String>,
    ) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(process_action_json(
            &data_source,
            pid,
            &action,
            dry_run,
            reason,
        ))
    }

    pub fn process_action_history_json(&self, window_minutes: u32, limit: u32) -> JsonQueryResult {
        let data_source = MonitorEngineDataSource {
            engine: Arc::clone(&self.inner),
        };
        json_query_result(process_action_history_json(
            &data_source,
            window_minutes as u64,
            limit as usize,
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UiSnapshotCacheKey {
    process_limit: u32,
    trend_points: u32,
    selected_entity_id: Option<String>,
}

#[derive(Clone, Debug)]
struct UiSnapshotCacheEntry {
    key: UiSnapshotCacheKey,
    snapshot: UiSnapshot,
}

const UI_SNAPSHOT_CACHE_LIMIT: usize = 12;

impl MonitorEngine {
    fn remember_ui_snapshot(&self, key: UiSnapshotCacheKey, snapshot: &UiSnapshot) {
        let Ok(mut cache) = self.ui_snapshot_cache.lock() else {
            return;
        };
        cache.retain(|entry| entry.snapshot.sequence != snapshot.sequence || entry.key != key);
        cache.push(UiSnapshotCacheEntry {
            key,
            snapshot: snapshot.clone(),
        });
        let overflow = cache.len().saturating_sub(UI_SNAPSHOT_CACHE_LIMIT);
        if overflow > 0 {
            cache.drain(0..overflow);
        }
    }

    fn cached_ui_snapshot(&self, sequence: u64, key: &UiSnapshotCacheKey) -> Option<UiSnapshot> {
        let Ok(cache) = self.ui_snapshot_cache.lock() else {
            return None;
        };
        cache
            .iter()
            .rev()
            .find(|entry| entry.snapshot.sequence == sequence && &entry.key == key)
            .map(|entry| entry.snapshot.clone())
    }
}

impl Drop for MonitorEngine {
    fn drop(&mut self) {
        if let Ok(mut server) = self.mcp_server.lock() {
            let _ = server.take();
        }
        if let Ok(mut engine) = self.inner.lock() {
            engine.stop();
        }
    }
}

struct MonitorEngineDataSource {
    engine: Arc<std::sync::Mutex<Engine>>,
}

impl AetowerMcpDataSource for MonitorEngineDataSource {
    fn latest_snapshot(&self) -> Result<model::SystemSnapshot, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.latest_snapshot())
    }

    fn latest_snapshot_if_newer(
        &self,
        last_sequence: u64,
    ) -> Result<Option<model::SystemSnapshot>, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.latest_snapshot_if_newer(last_sequence))
    }

    fn latest_sequence(&self) -> Result<u64, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.latest_sequence())
    }

    fn latest_runtime_lag_metrics(&self) -> Result<model::RuntimeLagMetrics, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.latest_runtime_lag_metrics())
    }

    fn history_range_summary(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<HistorySummaryResponse, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        engine
            .try_history_range_summary(start_millis, end_millis)
            .map(|summary| HistorySummaryResponse {
                store_bytes: summary.store_bytes,
                wal_bytes: summary.wal_bytes,
                snapshot_count: summary.snapshot_count,
                quarantine_count: summary.quarantine_count,
                range_count: summary.range_count,
                oldest_millis: summary.oldest_millis,
                newest_millis: summary.newest_millis,
                pending_writes: summary.pending_writes,
                store_modified_millis: summary.store_modified_millis,
                wal_modified_millis: summary.wal_modified_millis,
            })
    }

    fn load_history_page(
        &self,
        start_millis: u64,
        end_millis: u64,
        before_millis_exclusive: Option<u64>,
        limit: u32,
    ) -> Result<Vec<model::SystemSnapshot>, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        engine.try_load_history_page(start_millis, end_millis, before_millis_exclusive, limit)
    }

    fn diagnostics_overview(&self) -> Result<diagnostics::DiagnosticsOverview, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.diagnostics_overview())
    }

    fn query_diagnostics(
        &self,
        query: diagnostics::DiagnosticsQuery,
    ) -> Result<Vec<diagnostics::DiagnosticsEvent>, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.query_diagnostics(query))
    }

    fn record_diagnostics_event(&self, event: diagnostics::DiagnosticsEvent) {
        if let Ok(engine) = self.engine.lock() {
            engine.record_diagnostics_event(event);
        }
    }

    fn record_mcp_runtime_observation(
        &self,
        total_connections: u64,
        active_client_count: u64,
        total_requests: u64,
    ) {
        if let Ok(engine) = self.engine.lock() {
            engine.record_mcp_runtime_observation(
                total_connections,
                active_client_count,
                total_requests,
            );
        }
    }
}

fn json_query_result(result: Result<String, String>) -> JsonQueryResult {
    match result {
        Ok(json) => JsonQueryResult {
            json: Some(json),
            error_message: None,
        },
        Err(error) => JsonQueryResult {
            json: None,
            error_message: Some(error),
        },
    }
}

const DEFAULT_UI_PROCESS_LIMIT: usize = 160;
const MAX_UI_PROCESS_LIMIT: usize = 500;
const DEFAULT_UI_TREND_POINTS: usize = 120;
const MAX_UI_TREND_POINTS: usize = 300;
const SELECTED_ENTITY_COMPONENT_LIMIT: usize = 48;
const SELECTED_ENTITY_RECOMMENDATION_LIMIT: usize = 8;
const SELECTED_ENTITY_NETWORK_LIMIT: usize = 16;

fn ui_snapshot_cache_key(
    process_limit: u32,
    trend_points: u32,
    selected_entity_id: Option<&str>,
) -> UiSnapshotCacheKey {
    UiSnapshotCacheKey {
        process_limit: saturating_u32(normalized_ui_limit(
            process_limit,
            DEFAULT_UI_PROCESS_LIMIT,
            1,
            MAX_UI_PROCESS_LIMIT,
        )),
        trend_points: saturating_u32(normalized_ui_limit(
            trend_points,
            DEFAULT_UI_TREND_POINTS,
            8,
            MAX_UI_TREND_POINTS,
        )),
        selected_entity_id: selected_entity_id
            .map(str::trim)
            .filter(|entity_id| !entity_id.is_empty())
            .map(ToOwned::to_owned),
    }
}

fn ui_snapshot_from(
    snapshot: &model::SystemSnapshot,
    process_limit: u32,
    trend_points: u32,
    selected_entity_id: Option<&str>,
) -> UiSnapshot {
    let process_limit = normalized_ui_limit(
        process_limit,
        DEFAULT_UI_PROCESS_LIMIT,
        1,
        MAX_UI_PROCESS_LIMIT,
    );
    let trend_points = normalized_ui_limit(
        trend_points,
        DEFAULT_UI_TREND_POINTS,
        8,
        MAX_UI_TREND_POINTS,
    );
    let total_process_count = snapshot.entities.iter().fold(0u32, |count, entity| {
        count.saturating_add(entity.metrics.process_count)
    });
    let machine_friction = ui_machine_friction(snapshot);
    let process_rows: Vec<UiProcessRow> = snapshot
        .entities
        .iter()
        .take(process_limit)
        .map(ui_process_row_from_entity)
        .collect();
    let selected_entity = selected_entity_id
        .filter(|entity_id| !entity_id.is_empty())
        .and_then(|entity_id| {
            snapshot
                .entities
                .iter()
                .find(|entity| entity.entity_id == entity_id)
        })
        .map(|entity| ui_selected_entity_from_entity(entity, trend_points));
    let timeline_warning_count = snapshot
        .timeline
        .iter()
        .filter(|event| event.severity == model::TimelineSeverity::Warning)
        .count();
    let timeline_critical_count = snapshot
        .timeline
        .iter()
        .filter(|event| event.severity == model::TimelineSeverity::Critical)
        .count();

    UiSnapshot {
        sequence: snapshot.sequence,
        captured_at_millis: snapshot.captured_at_millis,
        host: ui_host_summary_from_snapshot(snapshot, machine_friction, total_process_count),
        host_trend: ui_host_trend_from_host_trend(&snapshot.host_trend, trend_points),
        metric_cards: ui_metric_cards_from_snapshot(snapshot, machine_friction, trend_points),
        returned_entity_count: saturating_u32(process_rows.len()),
        process_rows,
        selected_entity,
        total_entity_count: saturating_u32(snapshot.entities.len()),
        total_process_count,
        timeline_warning_count: saturating_u32(timeline_warning_count),
        timeline_critical_count: saturating_u32(timeline_critical_count),
    }
}

fn ui_snapshot_delta_no_update(
    snapshot: &model::SystemSnapshot,
    base_sequence: u64,
    key: &UiSnapshotCacheKey,
) -> UiSnapshotDelta {
    UiSnapshotDelta {
        updated: false,
        sequence: snapshot.sequence,
        base_sequence,
        captured_at_millis: snapshot.captured_at_millis,
        process_limit: key.process_limit,
        trend_points: key.trend_points,
        base_available: true,
        host: None,
        host_trend: None,
        changed_metric_cards: Vec::new(),
        changed_process_rows: Vec::new(),
        removed_entity_ids: Vec::new(),
        selected_entity: None,
        selected_entity_changed: false,
        selected_entity_removed: false,
        total_entity_count: saturating_u32(snapshot.entities.len()),
        returned_entity_count: 0,
        total_process_count: snapshot.entities.iter().fold(0u32, |count, entity| {
            count.saturating_add(entity.metrics.process_count)
        }),
        timeline_warning_count: saturating_u32(
            snapshot
                .timeline
                .iter()
                .filter(|event| event.severity == model::TimelineSeverity::Warning)
                .count(),
        ),
        timeline_critical_count: saturating_u32(
            snapshot
                .timeline
                .iter()
                .filter(|event| event.severity == model::TimelineSeverity::Critical)
                .count(),
        ),
    }
}

fn ui_snapshot_delta_from_current(
    base_sequence: u64,
    key: &UiSnapshotCacheKey,
    current: &UiSnapshot,
    base: Option<&UiSnapshot>,
) -> UiSnapshotDelta {
    let base_available = base.is_some();
    let host = match base {
        Some(base)
            if ui_host_summary_signature(&base.host)
                == ui_host_summary_signature(&current.host) =>
        {
            None
        }
        _ => Some(current.host.clone()),
    };
    let host_trend = match base {
        Some(base)
            if ui_host_trend_signature(&base.host_trend)
                == ui_host_trend_signature(&current.host_trend) =>
        {
            None
        }
        _ => Some(current.host_trend.clone()),
    };
    let changed_metric_cards = changed_metric_cards(current, base);
    let (changed_process_rows, removed_entity_ids) = changed_process_rows(current, base);
    let (selected_entity, selected_entity_changed, selected_entity_removed) =
        changed_selected_entity(current, base);

    UiSnapshotDelta {
        updated: true,
        sequence: current.sequence,
        base_sequence,
        captured_at_millis: current.captured_at_millis,
        process_limit: key.process_limit,
        trend_points: key.trend_points,
        base_available,
        host,
        host_trend,
        changed_metric_cards,
        changed_process_rows,
        removed_entity_ids,
        selected_entity,
        selected_entity_changed,
        selected_entity_removed,
        total_entity_count: current.total_entity_count,
        returned_entity_count: current.returned_entity_count,
        total_process_count: current.total_process_count,
        timeline_warning_count: current.timeline_warning_count,
        timeline_critical_count: current.timeline_critical_count,
    }
}

fn changed_metric_cards(current: &UiSnapshot, base: Option<&UiSnapshot>) -> Vec<UiMetricCard> {
    let Some(base) = base else {
        return current.metric_cards.clone();
    };
    let base_by_id: BTreeMap<&str, String> = base
        .metric_cards
        .iter()
        .map(|card| (card.id.as_str(), ui_metric_card_signature(card)))
        .collect();
    current
        .metric_cards
        .iter()
        .filter(|card| {
            base_by_id
                .get(card.id.as_str())
                .map(|signature| signature != &ui_metric_card_signature(card))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

fn changed_process_rows(
    current: &UiSnapshot,
    base: Option<&UiSnapshot>,
) -> (Vec<UiProcessRow>, Vec<String>) {
    let Some(base) = base else {
        return (current.process_rows.clone(), Vec::new());
    };
    let base_by_id: BTreeMap<&str, String> = base
        .process_rows
        .iter()
        .map(|row| (row.entity_id.as_str(), ui_process_row_signature(row)))
        .collect();
    let current_ids: BTreeSet<&str> = current
        .process_rows
        .iter()
        .map(|row| row.entity_id.as_str())
        .collect();
    let changed_rows = current
        .process_rows
        .iter()
        .filter(|row| {
            base_by_id
                .get(row.entity_id.as_str())
                .map(|signature| signature != &ui_process_row_signature(row))
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    let removed_ids = base
        .process_rows
        .iter()
        .filter(|row| !current_ids.contains(row.entity_id.as_str()))
        .map(|row| row.entity_id.clone())
        .collect();
    (changed_rows, removed_ids)
}

fn changed_selected_entity(
    current: &UiSnapshot,
    base: Option<&UiSnapshot>,
) -> (Option<UiSelectedEntity>, bool, bool) {
    let Some(base) = base else {
        let changed = current.selected_entity.is_some();
        return (current.selected_entity.clone(), changed, false);
    };
    match (&current.selected_entity, &base.selected_entity) {
        (None, None) => (None, false, false),
        (None, Some(_)) => (None, true, true),
        (Some(current), None) => (Some(current.clone()), true, false),
        (Some(current), Some(base)) => {
            let changed =
                ui_selected_entity_signature(current) != ui_selected_entity_signature(base);
            (changed.then(|| current.clone()), changed, false)
        }
    }
}

fn ui_host_summary_signature(value: &UiHostSummary) -> String {
    format!("{value:?}")
}

fn ui_host_trend_signature(value: &UiHostTrend) -> String {
    format!("{value:?}")
}

fn ui_metric_card_signature(value: &UiMetricCard) -> String {
    format!("{value:?}")
}

fn ui_process_row_signature(value: &UiProcessRow) -> String {
    format!("{value:?}")
}

fn ui_selected_entity_signature(value: &UiSelectedEntity) -> String {
    format!("{value:?}")
}

fn normalized_ui_limit(
    value: u32,
    default_value: usize,
    min_value: usize,
    max_value: usize,
) -> usize {
    if value == 0 {
        return default_value;
    }
    usize::try_from(value)
        .map(|limit| limit.clamp(min_value, max_value))
        .unwrap_or(max_value)
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn ui_machine_friction(snapshot: &model::SystemSnapshot) -> f32 {
    snapshot
        .host_trend
        .machine_friction
        .last()
        .copied()
        .or_else(|| {
            snapshot
                .entities
                .first()
                .map(|entity| entity.friction.total_score)
        })
        .unwrap_or_default()
}

fn ui_host_summary_from_snapshot(
    snapshot: &model::SystemSnapshot,
    machine_friction: f32,
    total_process_count: u32,
) -> UiHostSummary {
    UiHostSummary {
        machine_friction,
        cpu_percent: snapshot.host.cpu_percent,
        memory_used_bytes: snapshot.host.memory_used_bytes,
        memory_total_bytes: snapshot.host.memory_total_bytes,
        compressed_memory_bytes: snapshot.host.compressed_memory_bytes,
        swap_used_bytes: snapshot.host.swap_used_bytes,
        disk_read_bps: snapshot.host.disk_read_bps,
        disk_write_bps: snapshot.host.disk_write_bps,
        network_receive_bps: snapshot.host.network_receive_bps,
        network_send_bps: snapshot.host.network_send_bps,
        wakeups_per_second: snapshot.host.wakeups_per_second,
        gpu_percent: snapshot.host.gpu_percent,
        gpu_memory_bytes: snapshot.host.gpu_memory_bytes,
        thermal_state: snapshot.host.thermal_state.into(),
        on_battery: snapshot.host.on_battery,
        low_power_mode: snapshot.host.low_power_mode,
        frontmost_app_name: snapshot.host.frontmost_app_name.clone(),
        frontmost_window_title: snapshot.host.frontmost_window_title.clone(),
        ai_agent_count: snapshot.host.ai_agent_count,
        entity_count: saturating_u32(snapshot.entities.len()),
        process_count: total_process_count,
    }
}

fn ui_host_trend_from_host_trend(value: &model::HostTrend, max_points: usize) -> UiHostTrend {
    UiHostTrend {
        machine_friction: downsample_f32_to_f64(&value.machine_friction, max_points),
        cpu_percent: downsample_f32_to_f64(&value.cpu_percent, max_points),
        memory_used_bytes: downsample_u64_to_f64(&value.memory_used_bytes, max_points),
        memory_pressure_score: downsample_f32_to_f64(&value.memory_pressure_score, max_points),
        disk_activity_bps: downsample_u64_to_f64(&value.disk_activity_bps, max_points),
        network_activity_bps: downsample_u64_to_f64(&value.network_activity_bps, max_points),
        wakeups_per_second: downsample_f32_to_f64(&value.wakeups_per_second, max_points),
        gpu_percent: downsample_f32_to_f64(&value.gpu_percent, max_points),
        gpu_memory_bytes: downsample_u64_to_f64(&value.gpu_memory_bytes, max_points),
        max_cpu_temperature: downsample_f32_to_f64(&value.max_cpu_temperature, max_points),
    }
}

fn ui_metric_cards_from_snapshot(
    snapshot: &model::SystemSnapshot,
    machine_friction: f32,
    trend_points: usize,
) -> Vec<UiMetricCard> {
    let disk_bps = snapshot
        .host
        .disk_read_bps
        .saturating_add(snapshot.host.disk_write_bps);
    let network_bps = snapshot
        .host
        .network_receive_bps
        .saturating_add(snapshot.host.network_send_bps);
    let memory_ratio = if snapshot.host.memory_total_bytes == 0 {
        0.0
    } else {
        snapshot.host.memory_used_bytes as f64 / snapshot.host.memory_total_bytes as f64
    };
    vec![
        UiMetricCard {
            id: "friction".to_owned(),
            title: "Friction".to_owned(),
            value: f64::from(machine_friction),
            unit: "score".to_owned(),
            display_value: format!("{machine_friction:.0}"),
            detail: "Host pressure and top process burden".to_owned(),
            severity: severity_for_percent(f64::from(machine_friction), 45.0, 70.0),
            samples: downsample_f32_to_f64(&snapshot.host_trend.machine_friction, trend_points),
            fixed_ceiling: Some(100.0),
        },
        UiMetricCard {
            id: "cpu".to_owned(),
            title: "CPU".to_owned(),
            value: f64::from(snapshot.host.cpu_percent),
            unit: "percent".to_owned(),
            display_value: format!("{:.0}%", snapshot.host.cpu_percent),
            detail: "Total host CPU load".to_owned(),
            severity: severity_for_percent(f64::from(snapshot.host.cpu_percent), 70.0, 90.0),
            samples: downsample_f32_to_f64(&snapshot.host_trend.cpu_percent, trend_points),
            fixed_ceiling: Some(100.0),
        },
        UiMetricCard {
            id: "memory".to_owned(),
            title: "Memory".to_owned(),
            value: snapshot.host.memory_used_bytes as f64,
            unit: "bytes".to_owned(),
            display_value: format_bytes(snapshot.host.memory_used_bytes),
            detail: format!(
                "{:.0}% of {}",
                memory_ratio * 100.0,
                format_bytes(snapshot.host.memory_total_bytes)
            ),
            severity: severity_for_percent(memory_ratio * 100.0, 75.0, 90.0),
            samples: downsample_u64_to_f64(&snapshot.host_trend.memory_used_bytes, trend_points),
            fixed_ceiling: (snapshot.host.memory_total_bytes > 0)
                .then_some(snapshot.host.memory_total_bytes as f64),
        },
        UiMetricCard {
            id: "disk".to_owned(),
            title: "Disk".to_owned(),
            value: disk_bps as f64,
            unit: "bytes_per_second".to_owned(),
            display_value: format_bps(disk_bps),
            detail: "Read and write throughput".to_owned(),
            severity: severity_for_bps(disk_bps, 100 * 1024 * 1024, 500 * 1024 * 1024),
            samples: downsample_u64_to_f64(&snapshot.host_trend.disk_activity_bps, trend_points),
            fixed_ceiling: None,
        },
        UiMetricCard {
            id: "network".to_owned(),
            title: "Network".to_owned(),
            value: network_bps as f64,
            unit: "bytes_per_second".to_owned(),
            display_value: format_bps(network_bps),
            detail: "Receive and send throughput".to_owned(),
            severity: severity_for_bps(network_bps, 25 * 1024 * 1024, 100 * 1024 * 1024),
            samples: downsample_u64_to_f64(&snapshot.host_trend.network_activity_bps, trend_points),
            fixed_ceiling: None,
        },
        UiMetricCard {
            id: "wakeups".to_owned(),
            title: "Wakeups".to_owned(),
            value: f64::from(snapshot.host.wakeups_per_second),
            unit: "wakeups_per_second".to_owned(),
            display_value: format!("{:.0}/s", snapshot.host.wakeups_per_second),
            detail: "Host wakeups per second".to_owned(),
            severity: severity_for_percent(
                f64::from(snapshot.host.wakeups_per_second),
                1_000.0,
                5_000.0,
            ),
            samples: downsample_f32_to_f64(&snapshot.host_trend.wakeups_per_second, trend_points),
            fixed_ceiling: None,
        },
        UiMetricCard {
            id: "gpu".to_owned(),
            title: "GPU".to_owned(),
            value: f64::from(snapshot.host.gpu_percent),
            unit: "percent".to_owned(),
            display_value: format!("{:.0}%", snapshot.host.gpu_percent),
            detail: format!(
                "{} GPU memory",
                format_bytes(snapshot.host.gpu_memory_bytes)
            ),
            severity: severity_for_percent(f64::from(snapshot.host.gpu_percent), 70.0, 90.0),
            samples: downsample_f32_to_f64(&snapshot.host_trend.gpu_percent, trend_points),
            fixed_ceiling: Some(100.0),
        },
    ]
}

fn ui_process_row_from_entity(entity: &model::EntitySnapshot) -> UiProcessRow {
    UiProcessRow {
        entity_id: entity.entity_id.clone(),
        display_name: entity.display_name.clone(),
        entity_kind: entity.entity_kind.clone().into(),
        bundle_id: entity.bundle_id.clone(),
        executable_path: entity.executable_path.clone(),
        primary_pid: entity
            .components
            .iter()
            .find_map(|component| component.process_id),
        process_count: entity.metrics.process_count,
        thread_count: entity.metrics.thread_count,
        cpu_percent: entity.metrics.cpu_percent,
        memory_resident_bytes: entity.metrics.memory_resident_bytes,
        memory_physical_footprint_bytes: entity.metrics.memory_physical_footprint_bytes,
        disk_read_bps: entity.metrics.disk_read_bps,
        disk_write_bps: entity.metrics.disk_write_bps,
        network_receive_bps: entity.metrics.network_receive_bps,
        network_send_bps: entity.metrics.network_send_bps,
        wakeups_per_second: entity.metrics.wakeups_per_second,
        energy_nj_per_s: entity.metrics.energy_nj_per_s,
        estimated_gpu_percent: entity.metrics.estimated_gpu_percent,
        friction_score: entity.friction.total_score,
        is_foreground: entity.metrics.is_foreground,
        anomaly_detected: entity.anomaly_detected,
        active_window_title: entity.active_window_title.clone(),
        recent_change_summary: entity.recent_change_summary.clone(),
        signing_classification: entity.signing_classification.clone(),
        is_adhoc: entity.is_adhoc,
        app_version: entity.app_version.clone(),
    }
}

fn ui_selected_entity_from_entity(
    entity: &model::EntitySnapshot,
    trend_points: usize,
) -> UiSelectedEntity {
    UiSelectedEntity {
        row: ui_process_row_from_entity(entity),
        components: entity
            .components
            .iter()
            .take(SELECTED_ENTITY_COMPONENT_LIMIT)
            .map(ui_process_component_from_component)
            .collect(),
        trend: ui_metric_trend_from_metric_trend(&entity.trend, trend_points),
        recommendations: entity
            .recommendations
            .iter()
            .take(SELECTED_ENTITY_RECOMMENDATION_LIMIT)
            .cloned()
            .map(Into::into)
            .collect(),
        network_connections: entity
            .network_connections
            .iter()
            .take(SELECTED_ENTITY_NETWORK_LIMIT)
            .cloned()
            .map(Into::into)
            .collect(),
        badges: entity.badges.clone(),
        attribution_notes: entity.attribution_notes.clone(),
        thermal_contribution: entity.thermal_contribution.clone(),
        grouping_suggestion: entity.grouping_suggestion.clone(),
    }
}

fn ui_process_component_from_component(component: &model::ComponentSnapshot) -> UiProcessComponent {
    UiProcessComponent {
        title: component.title.clone(),
        detail: component.detail.clone(),
        process_id: component.process_id,
        start_time_millis: component.start_time_millis,
        executable_path: component.executable_path.clone(),
        command_line: component.command_line.clone(),
        parent_summary: component.parent_summary.clone(),
        launched_by: component.launched_by.clone(),
        cpu_percent: component.cpu_percent,
        memory_bytes: component.memory_bytes,
        memory_physical_footprint_bytes: component.memory_physical_footprint_bytes,
        cwd: component.cwd.clone(),
        user: component.user.clone(),
        thread_count: component.thread_count,
    }
}

fn ui_metric_trend_from_metric_trend(value: &model::MetricTrend, max_points: usize) -> MetricTrend {
    MetricTrend {
        friction: downsample_f32_to_f32(&value.friction, max_points),
        cpu_percent: downsample_f32_to_f32(&value.cpu_percent, max_points),
        memory_resident_bytes: downsample_u64_to_u64(&value.memory_resident_bytes, max_points),
        disk_activity_bps: downsample_u64_to_u64(&value.disk_activity_bps, max_points),
        network_activity_bps: downsample_u64_to_u64(&value.network_activity_bps, max_points),
        wakeups_per_second: downsample_f32_to_f32(&value.wakeups_per_second, max_points),
    }
}

fn severity_for_percent(value: f64, warning: f64, critical: f64) -> UiMetricSeverity {
    if value >= critical {
        UiMetricSeverity::Critical
    } else if value >= warning {
        UiMetricSeverity::Warning
    } else {
        UiMetricSeverity::Normal
    }
}

fn severity_for_bps(value: u64, warning: u64, critical: u64) -> UiMetricSeverity {
    if value >= critical {
        UiMetricSeverity::Critical
    } else if value >= warning {
        UiMetricSeverity::Warning
    } else {
        UiMetricSeverity::Normal
    }
}

fn downsample_f32_to_f64(values: &[f32], max_points: usize) -> Vec<f64> {
    downsample_by_index(values, max_points, |value| f64::from(*value))
}

fn downsample_u64_to_f64(values: &[u64], max_points: usize) -> Vec<f64> {
    downsample_by_index(values, max_points, |value| *value as f64)
}

fn downsample_f32_to_f32(values: &[f32], max_points: usize) -> Vec<f32> {
    downsample_by_index(values, max_points, |value| *value)
}

fn downsample_u64_to_u64(values: &[u64], max_points: usize) -> Vec<u64> {
    downsample_by_index(values, max_points, |value| *value)
}

fn downsample_by_index<T, U, F>(values: &[T], max_points: usize, map: F) -> Vec<U>
where
    F: Fn(&T) -> U,
{
    if max_points == 0 || values.is_empty() {
        return Vec::new();
    }
    if values.len() <= max_points {
        return values.iter().map(map).collect();
    }
    let last_index = values.len() - 1;
    let denominator = max_points - 1;
    (0..max_points)
        .map(|index| {
            let source_index = index * last_index / denominator;
            map(&values[source_index])
        })
        .collect()
}

fn format_bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut scaled = value as f64;
    let mut unit_index = 0;
    while scaled >= 1024.0 && unit_index < UNITS.len() - 1 {
        scaled /= 1024.0;
        unit_index += 1;
    }
    if unit_index == 0 {
        format!("{value} {}", UNITS[unit_index])
    } else if scaled >= 10.0 {
        format!("{scaled:.0} {}", UNITS[unit_index])
    } else {
        format!("{scaled:.1} {}", UNITS[unit_index])
    }
}

fn format_bps(value: u64) -> String {
    format!("{}/s", format_bytes(value))
}

#[cfg(test)]
mod ui_snapshot_tests {
    use super::*;

    fn test_entity(entity_id: &str, cpu_percent: f32) -> model::EntitySnapshot {
        model::EntitySnapshot {
            entity_id: entity_id.to_owned(),
            display_name: entity_id.to_owned(),
            metrics: model::AggregateMetrics {
                process_count: 1,
                cpu_percent,
                ..model::AggregateMetrics::default()
            },
            ..model::EntitySnapshot::default()
        }
    }

    #[test]
    fn ui_snapshot_caps_process_rows_without_losing_counts() {
        let snapshot = model::SystemSnapshot {
            sequence: 7,
            entities: (0..3)
                .map(|index| model::EntitySnapshot {
                    entity_id: format!("entity-{index}"),
                    display_name: format!("Entity {index}"),
                    metrics: model::AggregateMetrics {
                        process_count: 2,
                        cpu_percent: index as f32,
                        ..model::AggregateMetrics::default()
                    },
                    ..model::EntitySnapshot::default()
                })
                .collect(),
            ..model::SystemSnapshot::default()
        };

        let ui_snapshot = ui_snapshot_from(&snapshot, 2, 120, None);

        assert_eq!(ui_snapshot.sequence, 7);
        assert_eq!(ui_snapshot.process_rows.len(), 2);
        assert_eq!(ui_snapshot.total_entity_count, 3);
        assert_eq!(ui_snapshot.returned_entity_count, 2);
        assert_eq!(ui_snapshot.total_process_count, 6);
    }

    #[test]
    fn ui_snapshot_returns_capped_selected_entity_details() {
        let selected = model::EntitySnapshot {
            entity_id: "selected".to_owned(),
            display_name: "Selected".to_owned(),
            components: (0..80)
                .map(|index| model::ComponentSnapshot {
                    title: format!("component-{index}"),
                    process_id: Some(index),
                    ..model::ComponentSnapshot::default()
                })
                .collect(),
            network_connections: (0..40)
                .map(|index| model::NetworkConnection {
                    protocol: "tcp".to_owned(),
                    local: format!("127.0.0.1:{index}"),
                    remote: None,
                    state: "LISTEN".to_owned(),
                })
                .collect(),
            ..model::EntitySnapshot::default()
        };
        let snapshot = model::SystemSnapshot {
            entities: vec![selected],
            ..model::SystemSnapshot::default()
        };

        let ui_snapshot = ui_snapshot_from(&snapshot, 1, 16, Some("selected"));
        let Some(selected_entity) = ui_snapshot.selected_entity else {
            panic!("selected entity should be present");
        };

        assert_eq!(
            selected_entity.components.len(),
            SELECTED_ENTITY_COMPONENT_LIMIT
        );
        assert_eq!(
            selected_entity.network_connections.len(),
            SELECTED_ENTITY_NETWORK_LIMIT
        );
        assert_eq!(selected_entity.row.primary_pid, Some(0));
    }

    #[test]
    fn downsample_keeps_first_and_last_points() {
        let values: Vec<f32> = (0..100).map(|value| value as f32).collect();

        let downsampled = downsample_f32_to_f64(&values, 10);

        assert_eq!(downsampled.len(), 10);
        assert_eq!(downsampled.first().copied(), Some(0.0));
        assert_eq!(downsampled.last().copied(), Some(99.0));
    }

    #[test]
    fn ui_snapshot_delta_no_update_is_tiny() {
        let snapshot = model::SystemSnapshot {
            sequence: 9,
            entities: vec![test_entity("stable", 1.0)],
            ..model::SystemSnapshot::default()
        };
        let key = ui_snapshot_cache_key(2, 16, None);

        let delta = ui_snapshot_delta_no_update(&snapshot, 9, &key);

        assert!(!delta.updated);
        assert_eq!(delta.sequence, 9);
        assert!(delta.host.is_none());
        assert!(delta.host_trend.is_none());
        assert!(delta.changed_metric_cards.is_empty());
        assert!(delta.changed_process_rows.is_empty());
        assert!(delta.removed_entity_ids.is_empty());
        assert_eq!(delta.total_entity_count, 1);
        assert_eq!(delta.total_process_count, 1);
    }

    #[test]
    fn ui_snapshot_delta_without_cached_base_returns_full_lean_payload() {
        let snapshot = model::SystemSnapshot {
            sequence: 2,
            entities: vec![test_entity("one", 1.0), test_entity("two", 2.0)],
            ..model::SystemSnapshot::default()
        };
        let key = ui_snapshot_cache_key(10, 16, None);
        let current = ui_snapshot_from(&snapshot, key.process_limit, key.trend_points, None);

        let delta = ui_snapshot_delta_from_current(1, &key, &current, None);

        assert!(delta.updated);
        assert!(!delta.base_available);
        assert!(delta.host.is_some());
        assert!(delta.host_trend.is_some());
        assert_eq!(delta.changed_metric_cards.len(), current.metric_cards.len());
        assert_eq!(delta.changed_process_rows.len(), 2);
        assert!(delta.removed_entity_ids.is_empty());
    }

    #[test]
    fn ui_snapshot_delta_reports_changed_and_removed_rows() {
        let base_snapshot = model::SystemSnapshot {
            sequence: 1,
            entities: vec![test_entity("one", 1.0), test_entity("removed", 2.0)],
            ..model::SystemSnapshot::default()
        };
        let current_snapshot = model::SystemSnapshot {
            sequence: 2,
            entities: vec![test_entity("one", 9.0), test_entity("added", 3.0)],
            ..model::SystemSnapshot::default()
        };
        let key = ui_snapshot_cache_key(10, 16, None);
        let base = ui_snapshot_from(&base_snapshot, key.process_limit, key.trend_points, None);
        let current =
            ui_snapshot_from(&current_snapshot, key.process_limit, key.trend_points, None);

        let delta = ui_snapshot_delta_from_current(1, &key, &current, Some(&base));
        let changed_ids: BTreeSet<&str> = delta
            .changed_process_rows
            .iter()
            .map(|row| row.entity_id.as_str())
            .collect();

        assert!(delta.updated);
        assert!(delta.base_available);
        assert_eq!(changed_ids, BTreeSet::from(["added", "one"]));
        assert_eq!(delta.removed_entity_ids, vec!["removed".to_owned()]);
    }

    #[test]
    fn ui_snapshot_delta_reports_selected_entity_changes() {
        let mut base_entity = test_entity("selected", 1.0);
        base_entity.components = vec![model::ComponentSnapshot {
            title: "before".to_owned(),
            process_id: Some(10),
            ..model::ComponentSnapshot::default()
        }];
        let mut current_entity = test_entity("selected", 1.0);
        current_entity.components = vec![model::ComponentSnapshot {
            title: "after".to_owned(),
            process_id: Some(10),
            ..model::ComponentSnapshot::default()
        }];
        let key = ui_snapshot_cache_key(10, 16, Some("selected"));
        let base = ui_snapshot_from(
            &model::SystemSnapshot {
                sequence: 1,
                entities: vec![base_entity],
                ..model::SystemSnapshot::default()
            },
            key.process_limit,
            key.trend_points,
            key.selected_entity_id.as_deref(),
        );
        let current = ui_snapshot_from(
            &model::SystemSnapshot {
                sequence: 2,
                entities: vec![current_entity],
                ..model::SystemSnapshot::default()
            },
            key.process_limit,
            key.trend_points,
            key.selected_entity_id.as_deref(),
        );

        let delta = ui_snapshot_delta_from_current(1, &key, &current, Some(&base));

        assert!(delta.selected_entity_changed);
        assert!(!delta.selected_entity_removed);
        assert!(delta.selected_entity.is_some());
    }
}

impl From<model::EntityKind> for EntityKind {
    fn from(value: model::EntityKind) -> Self {
        match value {
            model::EntityKind::App => Self::App,
            model::EntityKind::Browser => Self::Browser,
            model::EntityKind::Daemon => Self::Daemon,
            model::EntityKind::TerminalSession => Self::TerminalSession,
            model::EntityKind::Service => Self::Service,
            model::EntityKind::AiAgent => Self::AiAgent,
            model::EntityKind::Unknown => Self::Unknown,
        }
    }
}

impl From<model::ComponentKind> for ComponentKind {
    fn from(value: model::ComponentKind) -> Self {
        match value {
            model::ComponentKind::Process => Self::Process,
            model::ComponentKind::Command => Self::Command,
            model::ComponentKind::AdapterContext => Self::AdapterContext,
        }
    }
}

impl From<CapabilityKind> for model::CapabilityKind {
    fn from(value: CapabilityKind) -> Self {
        match value {
            CapabilityKind::Accessibility => Self::Accessibility,
            CapabilityKind::FullDiskAccess => Self::FullDiskAccess,
            CapabilityKind::AppleAutomation => Self::AppleAutomation,
            CapabilityKind::ChromiumDebug => Self::ChromiumDebug,
            CapabilityKind::DockerSocket => Self::DockerSocket,
            CapabilityKind::PrivilegedHelper => Self::PrivilegedHelper,
            CapabilityKind::Chau7 => Self::Chau7,
            CapabilityKind::EndpointSecurity => Self::EndpointSecurity,
        }
    }
}

impl From<model::CapabilityKind> for CapabilityKind {
    fn from(value: model::CapabilityKind) -> Self {
        match value {
            model::CapabilityKind::Accessibility => Self::Accessibility,
            model::CapabilityKind::FullDiskAccess => Self::FullDiskAccess,
            model::CapabilityKind::AppleAutomation => Self::AppleAutomation,
            model::CapabilityKind::ChromiumDebug => Self::ChromiumDebug,
            model::CapabilityKind::DockerSocket => Self::DockerSocket,
            model::CapabilityKind::PrivilegedHelper => Self::PrivilegedHelper,
            model::CapabilityKind::Chau7 => Self::Chau7,
            model::CapabilityKind::EndpointSecurity => Self::EndpointSecurity,
        }
    }
}

impl From<CapabilityState> for model::CapabilityState {
    fn from(value: CapabilityState) -> Self {
        match value {
            CapabilityState::Unknown => Self::Unknown,
            CapabilityState::Granted => Self::Granted,
            CapabilityState::Denied => Self::Denied,
            CapabilityState::Requested => Self::Requested,
            CapabilityState::Unavailable => Self::Unavailable,
        }
    }
}

impl From<model::CapabilityState> for CapabilityState {
    fn from(value: model::CapabilityState) -> Self {
        match value {
            model::CapabilityState::Unknown => Self::Unknown,
            model::CapabilityState::Granted => Self::Granted,
            model::CapabilityState::Denied => Self::Denied,
            model::CapabilityState::Requested => Self::Requested,
            model::CapabilityState::Unavailable => Self::Unavailable,
        }
    }
}

impl From<model::CapabilityHealth> for CapabilityHealth {
    fn from(value: model::CapabilityHealth) -> Self {
        match value {
            model::CapabilityHealth::Configured => Self::Configured,
            model::CapabilityHealth::Live => Self::Live,
            model::CapabilityHealth::Cached => Self::Cached,
            model::CapabilityHealth::Degraded => Self::Degraded,
        }
    }
}

impl From<diagnostics::DiagnosticsLevel> for DiagnosticsLevel {
    fn from(value: diagnostics::DiagnosticsLevel) -> Self {
        match value {
            diagnostics::DiagnosticsLevel::Trace => Self::Trace,
            diagnostics::DiagnosticsLevel::Debug => Self::Debug,
            diagnostics::DiagnosticsLevel::Info => Self::Info,
            diagnostics::DiagnosticsLevel::Warn => Self::Warn,
            diagnostics::DiagnosticsLevel::Error => Self::Error,
        }
    }
}

impl From<diagnostics::DiagnosticsSubsystem> for DiagnosticsSubsystem {
    fn from(value: diagnostics::DiagnosticsSubsystem) -> Self {
        match value {
            diagnostics::DiagnosticsSubsystem::Engine => Self::Engine,
            diagnostics::DiagnosticsSubsystem::Collector => Self::Collector,
            diagnostics::DiagnosticsSubsystem::Identity => Self::Identity,
            diagnostics::DiagnosticsSubsystem::Attribution => Self::Attribution,
            diagnostics::DiagnosticsSubsystem::Friction => Self::Friction,
            diagnostics::DiagnosticsSubsystem::History => Self::History,
            diagnostics::DiagnosticsSubsystem::Persistence => Self::Persistence,
            diagnostics::DiagnosticsSubsystem::Telemetry => Self::Telemetry,
            diagnostics::DiagnosticsSubsystem::Gpu => Self::Gpu,
            diagnostics::DiagnosticsSubsystem::Ffi => Self::Ffi,
            diagnostics::DiagnosticsSubsystem::Ui => Self::Ui,
            diagnostics::DiagnosticsSubsystem::AdapterChromium => Self::AdapterChromium,
            diagnostics::DiagnosticsSubsystem::AdapterDocker => Self::AdapterDocker,
            diagnostics::DiagnosticsSubsystem::AdapterHelper => Self::AdapterHelper,
            diagnostics::DiagnosticsSubsystem::AdapterChau7 => Self::AdapterChau7,
            diagnostics::DiagnosticsSubsystem::AdapterVsCode => Self::AdapterVsCode,
        }
    }
}

impl From<model::ThermalState> for ThermalState {
    fn from(value: model::ThermalState) -> Self {
        match value {
            model::ThermalState::Nominal => Self::Nominal,
            model::ThermalState::Fair => Self::Fair,
            model::ThermalState::Serious => Self::Serious,
            model::ThermalState::Critical => Self::Critical,
        }
    }
}

impl From<model::AttributionConfidence> for AttributionConfidence {
    fn from(value: model::AttributionConfidence) -> Self {
        match value {
            model::AttributionConfidence::High => Self::High,
            model::AttributionConfidence::Medium => Self::Medium,
            model::AttributionConfidence::Low => Self::Low,
        }
    }
}

impl From<model::ProvenanceKind> for ProvenanceKind {
    fn from(value: model::ProvenanceKind) -> Self {
        match value {
            model::ProvenanceKind::UserLaunch => Self::UserLaunch,
            model::ProvenanceKind::AppBundle => Self::AppBundle,
            model::ProvenanceKind::HelperTree => Self::HelperTree,
            model::ProvenanceKind::ShellSession => Self::ShellSession,
            model::ProvenanceKind::LoginItem => Self::LoginItem,
            model::ProvenanceKind::ServiceManager => Self::ServiceManager,
            model::ProvenanceKind::XpcService => Self::XpcService,
            model::ProvenanceKind::BrowserContext => Self::BrowserContext,
            model::ProvenanceKind::ContainerWorkload => Self::ContainerWorkload,
            model::ProvenanceKind::ParentProcess => Self::ParentProcess,
            model::ProvenanceKind::Unknown => Self::Unknown,
        }
    }
}

impl From<model::AdapterContextKind> for AdapterContextKind {
    fn from(value: model::AdapterContextKind) -> Self {
        match value {
            model::AdapterContextKind::ChromiumTab => Self::ChromiumTab,
            model::AdapterContextKind::DockerContainer => Self::DockerContainer,
            model::AdapterContextKind::PrivilegedSocket => Self::PrivilegedSocket,
            model::AdapterContextKind::Chau7Session => Self::Chau7Session,
            model::AdapterContextKind::VsCodeWorkspace => Self::VsCodeWorkspace,
            model::AdapterContextKind::VsCodeRuntime => Self::VsCodeRuntime,
            model::AdapterContextKind::Unknown => Self::Unknown,
        }
    }
}

impl From<model::TimelineSeverity> for TimelineSeverity {
    fn from(value: model::TimelineSeverity) -> Self {
        match value {
            model::TimelineSeverity::Info => Self::Info,
            model::TimelineSeverity::Warning => Self::Warning,
            model::TimelineSeverity::Critical => Self::Critical,
        }
    }
}

impl From<model::TimelineCategory> for TimelineCategory {
    fn from(value: model::TimelineCategory) -> Self {
        match value {
            model::TimelineCategory::Lifecycle => Self::Lifecycle,
            model::TimelineCategory::Friction => Self::Friction,
            model::TimelineCategory::Host => Self::Host,
            model::TimelineCategory::Thermal => Self::Thermal,
            model::TimelineCategory::Anomaly => Self::Anomaly,
            model::TimelineCategory::Network => Self::Network,
            model::TimelineCategory::Regression => Self::Regression,
        }
    }
}

impl From<diagnostics::DiagnosticsField> for DiagnosticsField {
    fn from(value: diagnostics::DiagnosticsField) -> Self {
        Self {
            key: value.key,
            value: value.value,
        }
    }
}

impl From<diagnostics::DiagnosticsEvent> for DiagnosticsEvent {
    fn from(value: diagnostics::DiagnosticsEvent) -> Self {
        Self {
            id: value.id,
            timestamp_millis: value.timestamp_millis,
            level: value.level.into(),
            subsystem: value.subsystem.into(),
            event_type: value.event_type,
            sequence: value.sequence,
            entity_id: value.entity_id,
            adapter: value.adapter,
            capability: value.capability,
            message: value.message,
            fields: value.fields.into_iter().map(Into::into).collect(),
            sensitive: value.sensitive,
        }
    }
}

impl From<diagnostics::DiagnosticsOverview> for DiagnosticsOverview {
    fn from(value: diagnostics::DiagnosticsOverview) -> Self {
        Self {
            ring_capacity: value.ring_capacity,
            current_size: value.current_size,
            dropped_events: value.dropped_events,
            error_count: value.error_count,
            warn_count: value.warn_count,
            last_event_millis: value.last_event_millis,
            last_error_millis: value.last_error_millis,
            last_error_message: value.last_error_message,
            persisted_events: value.persisted_events,
            persisted_path: value.persisted_path,
            persisted_bytes: value.persisted_bytes,
            persistence_error: value.persistence_error,
        }
    }
}

impl From<aetower_persistence::HistoryRangeSummary> for HistoryRangeSummary {
    fn from(value: aetower_persistence::HistoryRangeSummary) -> Self {
        Self {
            store_bytes: value.store_bytes,
            wal_bytes: value.wal_bytes,
            snapshot_count: value.snapshot_count,
            quarantine_count: value.quarantine_count,
            range_count: value.range_count,
            oldest_millis: value.oldest_millis,
            newest_millis: value.newest_millis,
            pending_writes: value.pending_writes,
        }
    }
}

impl From<aetower_persistence::HistoryMaintenanceReport> for HistoryMaintenanceReport {
    fn from(value: aetower_persistence::HistoryMaintenanceReport) -> Self {
        Self {
            store_bytes_before: value.store_bytes_before,
            wal_bytes_before: value.wal_bytes_before,
            store_bytes_after: value.store_bytes_after,
            wal_bytes_after: value.wal_bytes_after,
            checkpointed: value.checkpointed,
            vacuumed: value.vacuumed,
            pruned_rows: value.pruned_rows,
            aggressive_reason: value.aggressive_reason,
            cancelled: value.cancelled,
            elapsed_millis: value.elapsed_millis,
        }
    }
}

impl From<UiLagMetrics> for model::RuntimeLagMetrics {
    fn from(value: UiLagMetrics) -> Self {
        Self {
            updated_at_millis: value.updated_at_millis,
            bridge_fetch_millis: value.bridge_fetch_millis,
            ui_refresh_millis: value.ui_refresh_millis,
            snapshot_to_ui_millis: value.snapshot_to_ui_millis,
            snapshot_to_render_millis: value.snapshot_to_render_millis,
            render_commit_millis: value.render_commit_millis,
            display_frame_interval_millis: value.display_frame_interval_millis,
            display_refresh_hz: value.display_refresh_hz,
            display_dropped_frames: value.display_dropped_frames,
            input_avg_latency_millis: value.input_avg_latency_millis,
            input_max_latency_millis: value.input_max_latency_millis,
            input_sample_count: value.input_sample_count,
            ..model::RuntimeLagMetrics::default()
        }
    }
}

impl From<DiagnosticsLevel> for diagnostics::DiagnosticsLevel {
    fn from(value: DiagnosticsLevel) -> Self {
        match value {
            DiagnosticsLevel::Trace => Self::Trace,
            DiagnosticsLevel::Debug => Self::Debug,
            DiagnosticsLevel::Info => Self::Info,
            DiagnosticsLevel::Warn => Self::Warn,
            DiagnosticsLevel::Error => Self::Error,
        }
    }
}

impl From<DiagnosticsSubsystem> for diagnostics::DiagnosticsSubsystem {
    fn from(value: DiagnosticsSubsystem) -> Self {
        match value {
            DiagnosticsSubsystem::Engine => Self::Engine,
            DiagnosticsSubsystem::Collector => Self::Collector,
            DiagnosticsSubsystem::Identity => Self::Identity,
            DiagnosticsSubsystem::Attribution => Self::Attribution,
            DiagnosticsSubsystem::Friction => Self::Friction,
            DiagnosticsSubsystem::History => Self::History,
            DiagnosticsSubsystem::Persistence => Self::Persistence,
            DiagnosticsSubsystem::Telemetry => Self::Telemetry,
            DiagnosticsSubsystem::Gpu => Self::Gpu,
            DiagnosticsSubsystem::Ffi => Self::Ffi,
            DiagnosticsSubsystem::Ui => Self::Ui,
            DiagnosticsSubsystem::AdapterChromium => Self::AdapterChromium,
            DiagnosticsSubsystem::AdapterDocker => Self::AdapterDocker,
            DiagnosticsSubsystem::AdapterHelper => Self::AdapterHelper,
            DiagnosticsSubsystem::AdapterChau7 => Self::AdapterChau7,
            DiagnosticsSubsystem::AdapterVsCode => Self::AdapterVsCode,
        }
    }
}

impl From<DiagnosticsField> for diagnostics::DiagnosticsField {
    fn from(value: DiagnosticsField) -> Self {
        Self {
            key: value.key,
            value: value.value,
        }
    }
}

impl From<DiagnosticsEvent> for diagnostics::DiagnosticsEvent {
    fn from(value: DiagnosticsEvent) -> Self {
        Self {
            id: value.id,
            timestamp_millis: value.timestamp_millis,
            level: value.level.into(),
            subsystem: value.subsystem.into(),
            event_type: value.event_type,
            sequence: value.sequence,
            entity_id: value.entity_id,
            adapter: value.adapter,
            capability: value.capability,
            message: value.message,
            fields: value.fields.into_iter().map(Into::into).collect(),
            sensitive: value.sensitive,
        }
    }
}

impl From<model::HostSnapshot> for HostSnapshot {
    fn from(value: model::HostSnapshot) -> Self {
        Self {
            cpu_percent: value.cpu_percent,
            memory_used_bytes: value.memory_used_bytes,
            memory_total_bytes: value.memory_total_bytes,
            swap_used_bytes: value.swap_used_bytes,
            compressed_memory_bytes: value.compressed_memory_bytes,
            disk_read_bps: value.disk_read_bps,
            disk_write_bps: value.disk_write_bps,
            network_receive_bps: value.network_receive_bps,
            network_send_bps: value.network_send_bps,
            wakeups_per_second: value.wakeups_per_second,
            thermal_state: value.thermal_state.into(),
            on_battery: value.on_battery,
            battery_charge_percent: value.battery_charge_percent,
            low_power_mode: value.low_power_mode,
            frontmost_app_name: value.frontmost_app_name,
            frontmost_window_title: value.frontmost_window_title,
            ai_agent_friction: value.ai_agent_friction,
            ai_agent_count: value.ai_agent_count,
            gpu_percent: value.gpu_percent,
            ane_percent: value.ane_percent,
            gpu_memory_bytes: value.gpu_memory_bytes,
            gpu_temperature_celsius: value.gpu_temperature_celsius,
            fans: value.fans.into_iter().map(Into::into).collect(),
            cpu_temperatures: value.cpu_temperatures.into_iter().map(Into::into).collect(),
            power_readings: value.power_readings.into_iter().map(Into::into).collect(),
            battery_health: value.battery_health.map(Into::into),
            boot_session: value.boot_session.map(Into::into),
            network_interfaces: value
                .network_interfaces
                .into_iter()
                .map(Into::into)
                .collect(),
            disks: value.disks.into_iter().map(Into::into).collect(),
            bluetooth_devices: value
                .bluetooth_devices
                .into_iter()
                .map(Into::into)
                .collect(),
            per_core_cpu: value.per_core_cpu.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<model::CoreKind> for CoreKind {
    fn from(value: model::CoreKind) -> Self {
        match value {
            model::CoreKind::Performance => CoreKind::Performance,
            model::CoreKind::Efficiency => CoreKind::Efficiency,
            model::CoreKind::Unknown => CoreKind::Unknown,
        }
    }
}

impl From<model::CoreLoad> for CoreLoad {
    fn from(value: model::CoreLoad) -> Self {
        Self {
            index: value.index,
            percent: value.percent,
            kind: value.kind.into(),
        }
    }
}

impl From<model::FanReading> for FanReading {
    fn from(value: model::FanReading) -> Self {
        Self {
            id: value.id,
            name: value.name,
            current_rpm: value.current_rpm,
            min_rpm: value.min_rpm,
            max_rpm: value.max_rpm,
        }
    }
}

impl From<model::TemperatureReading> for TemperatureReading {
    fn from(value: model::TemperatureReading) -> Self {
        Self {
            label: value.label,
            celsius: value.celsius,
        }
    }
}

impl From<model::PowerUnit> for PowerUnit {
    fn from(value: model::PowerUnit) -> Self {
        match value {
            model::PowerUnit::Watts => Self::Watts,
            model::PowerUnit::Volts => Self::Volts,
            model::PowerUnit::Amps => Self::Amps,
        }
    }
}

impl From<model::PowerReading> for PowerReading {
    fn from(value: model::PowerReading) -> Self {
        Self {
            label: value.label,
            value: value.value,
            unit: value.unit.into(),
        }
    }
}

impl From<model::BatteryCondition> for BatteryCondition {
    fn from(value: model::BatteryCondition) -> Self {
        match value {
            model::BatteryCondition::Unknown => Self::Unknown,
            model::BatteryCondition::Good => Self::Good,
            model::BatteryCondition::Fair => Self::Fair,
            model::BatteryCondition::Poor => Self::Poor,
            model::BatteryCondition::ServiceBattery => Self::ServiceBattery,
        }
    }
}

impl From<model::BatteryHealthSnapshot> for BatteryHealthSnapshot {
    fn from(value: model::BatteryHealthSnapshot) -> Self {
        Self {
            cycle_count: value.cycle_count,
            design_capacity_mah: value.design_capacity_mah,
            max_capacity_mah: value.max_capacity_mah,
            health_percent: value.health_percent,
            condition: value.condition.into(),
            temperature_celsius: value.temperature_celsius,
        }
    }
}

impl From<model::RebootCauseSnapshot> for RebootCauseSnapshot {
    fn from(value: model::RebootCauseSnapshot) -> Self {
        Self {
            source: value.source,
            code: value.code,
            detail: value.detail,
            observed_at_millis: value.observed_at_millis,
        }
    }
}

impl From<model::BootSessionSnapshot> for BootSessionSnapshot {
    fn from(value: model::BootSessionSnapshot) -> Self {
        Self {
            boot_id: value.boot_id,
            boot_time_millis: value.boot_time_millis,
            host_uptime_millis: value.host_uptime_millis,
            previous_shutdown: value.previous_shutdown.map(Into::into),
        }
    }
}

impl From<model::NetworkInterfaceSnapshot> for NetworkInterfaceSnapshot {
    fn from(value: model::NetworkInterfaceSnapshot) -> Self {
        Self {
            name: value.name,
            mac_address: value.mac_address,
            receive_bps: value.receive_bps,
            send_bps: value.send_bps,
            is_up: value.is_up,
        }
    }
}

impl From<model::DiskHealthStatus> for DiskHealthStatus {
    fn from(value: model::DiskHealthStatus) -> Self {
        match value {
            model::DiskHealthStatus::Unknown => Self::Unknown,
            model::DiskHealthStatus::Healthy => Self::Healthy,
            model::DiskHealthStatus::Warning => Self::Warning,
            model::DiskHealthStatus::Failing => Self::Failing,
            model::DiskHealthStatus::NotSupported => Self::NotSupported,
        }
    }
}

impl From<model::DiskHealthSnapshot> for DiskHealthSnapshot {
    fn from(value: model::DiskHealthSnapshot) -> Self {
        Self {
            device_identifier: value.device_identifier,
            model: value.model,
            total_size_bytes: value.total_size_bytes,
            status: value.status.into(),
            temperature_celsius: value.temperature_celsius,
            percentage_used: value.percentage_used,
            available_spare_percent: value.available_spare_percent,
            power_on_hours: value.power_on_hours,
            power_cycles: value.power_cycles,
            media_errors: value.media_errors,
        }
    }
}

impl From<model::BluetoothDeviceType> for BluetoothDeviceType {
    fn from(value: model::BluetoothDeviceType) -> Self {
        match value {
            model::BluetoothDeviceType::Other => Self::Other,
            model::BluetoothDeviceType::Keyboard => Self::Keyboard,
            model::BluetoothDeviceType::Mouse => Self::Mouse,
            model::BluetoothDeviceType::Trackpad => Self::Trackpad,
            model::BluetoothDeviceType::Headphones => Self::Headphones,
            model::BluetoothDeviceType::GameController => Self::GameController,
        }
    }
}

impl From<model::BluetoothDeviceBattery> for BluetoothDeviceBattery {
    fn from(value: model::BluetoothDeviceBattery) -> Self {
        Self {
            name: value.name,
            address: value.address,
            battery_percent: value.battery_percent,
            device_type: value.device_type.into(),
        }
    }
}

impl From<model::HostTrend> for HostTrend {
    fn from(value: model::HostTrend) -> Self {
        Self {
            machine_friction: value.machine_friction,
            cpu_percent: value.cpu_percent,
            memory_used_bytes: value.memory_used_bytes,
            memory_pressure_score: value.memory_pressure_score,
            disk_activity_bps: value.disk_activity_bps,
            network_activity_bps: value.network_activity_bps,
            wakeups_per_second: value.wakeups_per_second,
            compressed_memory_bytes: value.compressed_memory_bytes,
            ai_agent_friction: value.ai_agent_friction,
            gpu_percent: value.gpu_percent,
            gpu_memory_bytes: value.gpu_memory_bytes,
            max_cpu_temperature: value.max_cpu_temperature,
        }
    }
}

impl From<model::AggregateMetrics> for AggregateMetrics {
    fn from(value: model::AggregateMetrics) -> Self {
        Self {
            cpu_percent: value.cpu_percent,
            memory_resident_bytes: value.memory_resident_bytes,
            memory_physical_footprint_bytes: value.memory_physical_footprint_bytes,
            disk_read_bps: value.disk_read_bps,
            disk_write_bps: value.disk_write_bps,
            network_receive_bps: value.network_receive_bps,
            network_send_bps: value.network_send_bps,
            wakeups_per_second: value.wakeups_per_second,
            energy_nj_per_s: value.energy_nj_per_s,
            estimated_gpu_percent: value.estimated_gpu_percent,
            process_count: value.process_count,
            thread_count: value.thread_count,
            is_foreground: value.is_foreground,
        }
    }
}

impl From<model::FrictionContributor> for FrictionContributor {
    fn from(value: model::FrictionContributor) -> Self {
        Self {
            key: value.key,
            label: value.label,
            score: value.score,
            detail: value.detail,
        }
    }
}

impl From<model::FrictionBreakdown> for FrictionBreakdown {
    fn from(value: model::FrictionBreakdown) -> Self {
        Self {
            total_score: value.total_score,
            cpu_score: value.cpu_score,
            memory_score: value.memory_score,
            disk_score: value.disk_score,
            network_score: value.network_score,
            wakeups_score: value.wakeups_score,
            pressure_score: value.pressure_score,
            foreground_bonus: value.foreground_bonus,
            energy_impact_score: value.energy_impact_score,
            reasons: value.reasons.into_vec(),
            contributors: value.contributors.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<model::ProvenanceSnapshot> for ProvenanceSnapshot {
    fn from(value: model::ProvenanceSnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            label: value.label,
            rule: value.rule,
            confidence: value.confidence.into(),
        }
    }
}

impl From<model::RuntimeLagMetrics> for RuntimeLagMetrics {
    fn from(value: model::RuntimeLagMetrics) -> Self {
        Self {
            updated_at_millis: value.updated_at_millis,
            engine_tick_millis: value.engine_tick_millis,
            collect_millis: value.collect_millis,
            identity_millis: value.identity_millis,
            attribution_millis: value.attribution_millis,
            friction_millis: value.friction_millis,
            enrich_millis: value.enrich_millis,
            history_millis: value.history_millis,
            persist_millis: value.persist_millis,
            gpu_sample_millis: value.gpu_sample_millis,
            target_tick_millis: value.target_tick_millis,
            history_queue_depth: value.history_queue_depth,
            diagnostics_queue_depth: value.diagnostics_queue_depth,
            mcp_helper_count: value.mcp_helper_count,
            stale_mcp_helper_count: value.stale_mcp_helper_count,
            oldest_mcp_helper_age_millis: value.oldest_mcp_helper_age_millis,
            self_cpu_percent: value.self_cpu_percent,
            self_memory_bytes: value.self_memory_bytes,
            self_memory_physical_footprint_bytes: value.self_memory_physical_footprint_bytes,
            self_wakeups_per_second: value.self_wakeups_per_second,
            self_energy_nj_per_s: value.self_energy_nj_per_s,
            mcp_total_connections: value.mcp_total_connections,
            mcp_active_client_count: value.mcp_active_client_count,
            mcp_total_requests: value.mcp_total_requests,
            mcp_requests_per_second: value.mcp_requests_per_second,
            mcp_observed_at_millis: value.mcp_observed_at_millis,
            bridge_fetch_millis: value.bridge_fetch_millis,
            ui_refresh_millis: value.ui_refresh_millis,
            snapshot_to_ui_millis: value.snapshot_to_ui_millis,
            snapshot_to_render_millis: value.snapshot_to_render_millis,
            render_commit_millis: value.render_commit_millis,
            display_frame_interval_millis: value.display_frame_interval_millis,
            display_refresh_hz: value.display_refresh_hz,
            display_dropped_frames: value.display_dropped_frames,
            input_avg_latency_millis: value.input_avg_latency_millis,
            input_max_latency_millis: value.input_max_latency_millis,
            input_sample_count: value.input_sample_count,
        }
    }
}

impl From<model::AdapterContextSnapshot> for AdapterContextSnapshot {
    fn from(value: model::AdapterContextSnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            status: value.status,
            url: value.url,
            workspace_path: value.workspace_path,
            repo_root: value.repo_root,
            image_name: value.image_name,
            session_id: value.session_id,
            app_version: value.app_version,
            build_sha: value.build_sha,
            build_timestamp: value.build_timestamp,
            build_channel: value.build_channel,
            network_receive_bps: value.network_receive_bps,
            network_send_bps: value.network_send_bps,
            disk_read_bps: value.disk_read_bps,
            disk_write_bps: value.disk_write_bps,
            memory_limit_bytes: value.memory_limit_bytes,
            js_heap_total_bytes: value.js_heap_total_bytes,
            dom_nodes: value.dom_nodes,
            documents: value.documents,
            frames: value.frames,
            process_count: value.process_count,
            connection_count: value.connection_count,
            ports: value.ports,
        }
    }
}

impl From<model::ComponentSnapshot> for ComponentSnapshot {
    fn from(value: model::ComponentSnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            title: value.title,
            detail: value.detail,
            adapter_context: value.adapter_context.map(Into::into),
            provenance: value.provenance.map(Into::into),
            process_id: value.process_id,
            start_time_millis: value.start_time_millis,
            executable_path: value.executable_path,
            command_line: value.command_line,
            parent_summary: value.parent_summary,
            launched_by: value.launched_by,
            cpu_percent: value.cpu_percent,
            memory_bytes: value.memory_bytes,
            memory_physical_footprint_bytes: value.memory_physical_footprint_bytes,
            cwd: value.cwd,
            user: value.user,
            thread_count: value.thread_count,
        }
    }
}

impl From<model::MetricTrend> for MetricTrend {
    fn from(value: model::MetricTrend) -> Self {
        Self {
            friction: value.friction,
            cpu_percent: value.cpu_percent,
            memory_resident_bytes: value.memory_resident_bytes,
            disk_activity_bps: value.disk_activity_bps,
            network_activity_bps: value.network_activity_bps,
            wakeups_per_second: value.wakeups_per_second,
        }
    }
}

impl From<model::RecommendationSeverity> for RecommendationSeverity {
    fn from(value: model::RecommendationSeverity) -> Self {
        match value {
            model::RecommendationSeverity::Info => Self::Info,
            model::RecommendationSeverity::Suggested => Self::Suggested,
            model::RecommendationSeverity::Urgent => Self::Urgent,
        }
    }
}

impl From<model::Recommendation> for Recommendation {
    fn from(value: model::Recommendation) -> Self {
        Self {
            title: value.title,
            detail: value.detail,
            severity: value.severity.into(),
            suggested_action: value.suggested_action,
            target_pid: value.target_pid,
            target_label: value.target_label,
        }
    }
}

impl From<model::EntitySnapshot> for EntitySnapshot {
    fn from(value: model::EntitySnapshot) -> Self {
        Self {
            entity_id: value.entity_id,
            display_name: value.display_name,
            primary_provenance: value.primary_provenance.map(Into::into),
            launcher_summary: value.launcher_summary,
            attribution_notes: value.attribution_notes,
            bundle_id: value.bundle_id,
            executable_path: value.executable_path,
            oldest_process_start_millis: value.oldest_process_start_millis,
            newest_process_start_millis: value.newest_process_start_millis,
            entity_kind: value.entity_kind.into(),
            metrics: value.metrics.into(),
            friction: value.friction.into(),
            components: value.components.into_iter().map(Into::into).collect(),
            trend: value.trend.into(),
            badges: value.badges,
            active_window_title: value.active_window_title,
            recent_change_summary: value.recent_change_summary,
            anomaly_detected: value.anomaly_detected,
            thermal_contribution: value.thermal_contribution,
            grouping_suggestion: value.grouping_suggestion,
            agent_cost: value.agent_cost.map(Into::into),
            session_markers: value.session_markers.into_iter().map(Into::into).collect(),
            recommendations: value.recommendations.into_iter().map(Into::into).collect(),
            network_connections: value
                .network_connections
                .into_iter()
                .map(Into::into)
                .collect(),
            signing_classification: value.signing_classification,
            is_adhoc: value.is_adhoc,
            binary_reputation: value.binary_reputation.map(Into::into),
            app_version: value.app_version,
        }
    }
}

impl From<model::ReputationVerdict> for ReputationVerdict {
    fn from(value: model::ReputationVerdict) -> Self {
        match value {
            model::ReputationVerdict::Clean => Self::Clean,
            model::ReputationVerdict::Suspicious => Self::Suspicious,
            model::ReputationVerdict::Malicious => Self::Malicious,
            model::ReputationVerdict::Unknown => Self::Unknown,
        }
    }
}

impl From<model::BinaryReputation> for BinaryReputation {
    fn from(value: model::BinaryReputation) -> Self {
        Self {
            sha256: value.sha256,
            malicious: value.malicious,
            suspicious: value.suspicious,
            harmless: value.harmless,
            undetected: value.undetected,
            total_engines: value.total_engines,
            verdict: value.verdict.into(),
            permalink: value.permalink,
            checked_at_millis: value.checked_at_millis,
        }
    }
}

impl From<model::NetworkConnection> for NetworkConnection {
    fn from(value: model::NetworkConnection) -> Self {
        Self {
            protocol: value.protocol,
            local: value.local,
            remote: value.remote,
            state: value.state,
        }
    }
}

impl From<model::AgentCostSummary> for AgentCostSummary {
    fn from(value: model::AgentCostSummary) -> Self {
        Self {
            total_input_tokens: value.total_input_tokens,
            total_output_tokens: value.total_output_tokens,
            cost_usd: value.cost_usd,
            total_runs: value.total_runs,
            session_energy_nj: value.session_energy_nj,
        }
    }
}

impl From<model::SessionMarkerKind> for SessionMarkerKind {
    fn from(value: model::SessionMarkerKind) -> Self {
        match value {
            model::SessionMarkerKind::RunStart => Self::RunStart,
            model::SessionMarkerKind::RunEnd => Self::RunEnd,
        }
    }
}

impl From<model::SessionMarker> for SessionMarker {
    fn from(value: model::SessionMarker) -> Self {
        Self {
            timestamp_millis: value.timestamp_millis,
            kind: value.kind.into(),
            label: value.label,
        }
    }
}

impl From<model::CapabilitySnapshot> for CapabilitySnapshot {
    fn from(value: model::CapabilitySnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            state: value.state.into(),
            health: value.health.into(),
            detail: value.detail,
            last_updated_millis: value.last_updated_millis,
        }
    }
}

impl From<model::TimelineEvent> for TimelineEvent {
    fn from(value: model::TimelineEvent) -> Self {
        Self {
            id: value.id,
            timestamp_millis: value.timestamp_millis,
            category: value.category.into(),
            severity: value.severity.into(),
            entity_id: value.entity_id,
            title: value.title,
            detail: value.detail,
        }
    }
}

impl From<model::SystemSnapshot> for SystemSnapshot {
    fn from(value: model::SystemSnapshot) -> Self {
        Self {
            sequence: value.sequence,
            captured_at_millis: value.captured_at_millis,
            host: value.host.into(),
            host_trend: value.host_trend.into(),
            capabilities: value.capabilities.into_iter().map(Into::into).collect(),
            entities: value.entities.into_iter().map(Into::into).collect(),
            timeline: value.timeline.into_iter().map(Into::into).collect(),
            ai_repo_summaries: value
                .ai_repo_summaries
                .into_iter()
                .map(Into::into)
                .collect(),
            chau7_sessions: value.chau7_sessions.into_iter().map(Into::into).collect(),
            thermal_forecast: value.thermal_forecast.map(Into::into),
        }
    }
}

impl From<model::ThermalForecast> for ThermalForecast {
    fn from(value: model::ThermalForecast) -> Self {
        Self {
            minutes_to_throttle: value.minutes_to_throttle,
            trend_celsius_per_min: value.trend_celsius_per_min,
            state: value.state.into(),
            top_contributor_entity_id: value.top_contributor_entity_id,
            top_contributor_pid: value.top_contributor_pid,
            top_contributor_label: value.top_contributor_label,
        }
    }
}

impl From<model::AiRepoSummary> for AiRepoSummary {
    fn from(value: model::AiRepoSummary) -> Self {
        Self {
            repo_path: value.repo_path,
            display_name: value.display_name,
            total_runs: value.total_runs,
            total_tokens: value.total_tokens,
            total_cost_usd: value.total_cost_usd,
            providers: value.providers,
        }
    }
}

impl From<model::Chau7SessionSummary> for Chau7SessionSummary {
    fn from(value: model::Chau7SessionSummary) -> Self {
        Self {
            id: value.id,
            tab_id: value.tab_id,
            session_id: value.session_id,
            title: value.title,
            provider: value.provider,
            status: value.status,
            workspace_path: value.workspace_path,
            repo_root: value.repo_root,
            git_branch: value.git_branch,
            active_app: value.active_app,
            window_id: value.window_id,
            run_count: value.run_count,
            last_active: value.last_active,
            turn_count: value.turn_count,
            child_session_count: value.child_session_count,
            pending_approval_description: value.pending_approval_description,
            last_exit_reason: value.last_exit_reason,
            active_run_duration_millis: value.active_run_duration_millis,
            is_at_prompt: value.is_at_prompt,
            shell_loading: value.shell_loading,
            cto_active: value.cto_active,
            linked_entity_ids: value.linked_entity_ids,
        }
    }
}

impl From<DiagnosticsQuery> for diagnostics::DiagnosticsQuery {
    fn from(value: DiagnosticsQuery) -> Self {
        Self {
            limit: value.limit as usize,
            minimum_level: value.minimum_level.map(Into::into),
            subsystem: value.subsystem.map(Into::into),
            search: value.search,
            since_millis: value.since_millis,
            include_persisted: value.include_persisted,
        }
    }
}

impl From<FrontmostAppState> for model::FrontmostAppState {
    fn from(value: FrontmostAppState) -> Self {
        Self {
            app_name: value.app_name,
            bundle_id: value.bundle_id,
            executable_path: value.executable_path,
            window_title: value.window_title,
            captured_at_millis: value.captured_at_millis,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use super::*;

    #[test]
    fn monitor_engine_starts_local_mcp_server() {
        // Use a per-process sub-directory under $TMPDIR instead of the
        // top-level temp dir. `start_local_socket_server` chmods the
        // parent of the socket path, and macOS refuses chmod on the
        // system tmpfs (`/var/folders/...`) that `env::temp_dir()`
        // returns — the only way the test can run on macOS is to own
        // its own directory.
        let dir = std::env::temp_dir().join(format!("aetower-mcp-ffi-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let socket_path = dir.join("mcp.sock");
        let socket_string = socket_path.display().to_string();
        let engine = MonitorEngine::new();
        let result = engine.start_local_mcp_server(Some(socket_string.clone()));
        assert!(result.is_empty(), "{result}");

        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(socket_path.exists(), "expected socket at {}", socket_string);
        drop(engine);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn monitor_engine_reuses_live_local_mcp_server() {
        let dir = std::env::temp_dir().join(format!("aetower-mcp-reuse-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let socket_path = dir.join("mcp.sock");
        let socket_string = socket_path.display().to_string();
        let engine = MonitorEngine::new();

        let first = engine.start_local_mcp_server(Some(socket_string.clone()));
        assert!(first.is_empty(), "{first}");
        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(socket_path.exists(), "expected socket at {}", socket_string);

        let second = engine.start_local_mcp_server(Some(socket_string.clone()));
        assert!(second.is_empty(), "{second}");
        assert!(
            socket_path.exists(),
            "expected socket to remain live at {}",
            socket_string
        );

        drop(engine);
        let _ = fs::remove_dir_all(&dir);
    }
}
