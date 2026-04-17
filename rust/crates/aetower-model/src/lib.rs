use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum EntityKind {
    App,
    Browser,
    Daemon,
    TerminalSession,
    Service,
    AiAgent,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentKind {
    #[default]
    Process,
    Command,
    AdapterContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityKind {
    #[default]
    Accessibility,
    FullDiskAccess,
    AppleAutomation,
    ChromiumDebug,
    DockerSocket,
    PrivilegedHelper,
    Chau7,
    EndpointSecurity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityState {
    #[default]
    Unknown,
    Granted,
    Denied,
    Requested,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityHealth {
    #[default]
    Configured,
    Live,
    Cached,
    Degraded,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThermalState {
    #[default]
    Nominal,
    Fair,
    Serious,
    Critical,
}

impl std::fmt::Display for ThermalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Nominal => "nominal",
            Self::Fair => "fair",
            Self::Serious => "serious",
            Self::Critical => "critical",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TimelineSeverity {
    #[default]
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TimelineCategory {
    #[default]
    Lifecycle,
    Friction,
    Host,
    Thermal,
    Anomaly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
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
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AttributionConfidence {
    High,
    Medium,
    #[default]
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceSnapshot {
    pub kind: ProvenanceKind,
    pub label: String,
    #[serde(default)]
    pub rule: String,
    #[serde(default)]
    pub confidence: AttributionConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterContextKind {
    ChromiumTab,
    DockerContainer,
    PrivilegedSocket,
    Chau7Session,
    VsCodeWorkspace,
    VsCodeRuntime,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterContextSnapshot {
    pub kind: AdapterContextKind,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub workspace_path: Option<String>,
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub image_name: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub build_sha: Option<String>,
    #[serde(default)]
    pub build_timestamp: Option<String>,
    #[serde(default)]
    pub build_channel: Option<String>,
    #[serde(default)]
    pub network_receive_bps: u64,
    #[serde(default)]
    pub network_send_bps: u64,
    #[serde(default)]
    pub disk_read_bps: u64,
    #[serde(default)]
    pub disk_write_bps: u64,
    #[serde(default)]
    pub memory_limit_bytes: u64,
    #[serde(default)]
    pub js_heap_total_bytes: u64,
    #[serde(default)]
    pub dom_nodes: u64,
    #[serde(default)]
    pub documents: u64,
    #[serde(default)]
    pub frames: u64,
    #[serde(default)]
    pub process_count: Option<u32>,
    #[serde(default)]
    pub connection_count: Option<u32>,
    #[serde(default)]
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostSnapshot {
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub swap_used_bytes: u64,
    #[serde(default)]
    pub compressed_memory_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    #[serde(default)]
    pub wakeups_per_second: f32,
    pub thermal_state: ThermalState,
    pub on_battery: bool,
    pub battery_charge_percent: Option<u8>,
    pub low_power_mode: bool,
    pub frontmost_app_name: Option<String>,
    pub frontmost_window_title: Option<String>,
    #[serde(default)]
    pub ai_agent_friction: f32,
    #[serde(default)]
    pub ai_agent_count: u32,
    #[serde(default)]
    pub gpu_percent: f32,
    #[serde(default)]
    pub ane_percent: f32,
    #[serde(default)]
    pub gpu_memory_bytes: u64,
    #[serde(default)]
    pub gpu_temperature_celsius: Option<f32>,
    #[serde(default)]
    pub fans: Vec<FanReading>,
    #[serde(default)]
    pub cpu_temperatures: Vec<TemperatureReading>,
    #[serde(default)]
    pub power_readings: Vec<PowerReading>,
    #[serde(default)]
    pub battery_health: Option<BatteryHealthSnapshot>,
    #[serde(default)]
    pub boot_session: Option<BootSessionSnapshot>,
    #[serde(default)]
    pub network_interfaces: Vec<NetworkInterfaceSnapshot>,
    #[serde(default)]
    pub disks: Vec<DiskHealthSnapshot>,
    #[serde(default)]
    pub bluetooth_devices: Vec<BluetoothDeviceBattery>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostTrend {
    pub machine_friction: Vec<f32>,
    pub cpu_percent: Vec<f32>,
    pub memory_used_bytes: Vec<u64>,
    pub disk_activity_bps: Vec<u64>,
    #[serde(default)]
    pub network_activity_bps: Vec<u64>,
    #[serde(default)]
    pub wakeups_per_second: Vec<f32>,
    #[serde(default)]
    pub compressed_memory_bytes: Vec<u64>,
    #[serde(default)]
    pub ai_agent_friction: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BootSessionSnapshot {
    #[serde(default)]
    pub boot_id: Option<String>,
    #[serde(default)]
    pub boot_time_millis: Option<u64>,
    #[serde(default)]
    pub host_uptime_millis: Option<u64>,
    #[serde(default)]
    pub previous_shutdown: Option<RebootCauseSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RebootCauseSnapshot {
    pub source: String,
    #[serde(default)]
    pub code: Option<String>,
    pub detail: String,
    #[serde(default)]
    pub observed_at_millis: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeLagMetrics {
    #[serde(default)]
    pub updated_at_millis: u64,
    #[serde(default)]
    pub engine_tick_millis: f32,
    #[serde(default)]
    pub collect_millis: f32,
    #[serde(default)]
    pub identity_millis: f32,
    #[serde(default)]
    pub attribution_millis: f32,
    #[serde(default)]
    pub friction_millis: f32,
    #[serde(default)]
    pub enrich_millis: f32,
    #[serde(default)]
    pub history_millis: f32,
    #[serde(default)]
    pub persist_millis: f32,
    #[serde(default)]
    pub gpu_sample_millis: f32,
    #[serde(default)]
    pub target_tick_millis: f32,
    #[serde(default)]
    pub history_queue_depth: u32,
    #[serde(default)]
    pub diagnostics_queue_depth: u32,
    #[serde(default)]
    pub mcp_helper_count: u32,
    #[serde(default)]
    pub stale_mcp_helper_count: u32,
    #[serde(default)]
    pub oldest_mcp_helper_age_millis: u64,
    #[serde(default)]
    pub bridge_fetch_millis: f32,
    #[serde(default)]
    pub ui_refresh_millis: f32,
    #[serde(default)]
    pub snapshot_to_ui_millis: f32,
    #[serde(default)]
    pub snapshot_to_render_millis: f32,
    #[serde(default)]
    pub render_commit_millis: f32,
    #[serde(default)]
    pub display_frame_interval_millis: f32,
    #[serde(default)]
    pub display_refresh_hz: f32,
    #[serde(default)]
    pub display_dropped_frames: u64,
    #[serde(default)]
    pub input_avg_latency_millis: f32,
    #[serde(default)]
    pub input_max_latency_millis: f32,
    #[serde(default)]
    pub input_sample_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregateMetrics {
    pub cpu_percent: f32,
    pub memory_resident_bytes: u64,
    #[serde(default)]
    pub memory_physical_footprint_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    #[serde(default)]
    pub wakeups_per_second: f32,
    /// Per-second energy draw in nanojoules (= nanowatts), aggregated
    /// across all processes that belong to this entity. Derived from
    /// macOS `ri_billed_energy` via `proc_pid_rusage(RUSAGE_INFO_V4)`.
    /// Zero when the kernel does not report energy (older macOS, non-
    /// Apple-Silicon, process just spawned) or when the platform is not
    /// macOS.
    #[serde(default)]
    pub energy_nj_per_s: f64,
    /// Heuristic estimate of the percentage of host GPU utilisation
    /// attributable to this entity. macOS does not expose per-process
    /// GPU usage, so the engine distributes `host.gpu_percent` among
    /// AI agent entities proportional to their CPU share. Non-AI
    /// entities always read 0.0.
    #[serde(default)]
    pub estimated_gpu_percent: f32,
    pub process_count: u32,
    pub is_foreground: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrictionContributor {
    pub key: String,
    pub label: String,
    pub score: f32,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrictionBreakdown {
    pub total_score: f32,
    pub cpu_score: f32,
    pub memory_score: f32,
    pub disk_score: f32,
    #[serde(default)]
    pub network_score: f32,
    #[serde(default)]
    pub wakeups_score: f32,
    #[serde(default)]
    pub pressure_score: f32,
    pub foreground_bonus: f32,
    #[serde(default)]
    pub energy_impact_score: f32,
    pub reasons: SmallVec<[String; 3]>,
    #[serde(default)]
    pub contributors: Vec<FrictionContributor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComponentSnapshot {
    pub kind: ComponentKind,
    pub title: String,
    pub detail: String,
    #[serde(default)]
    pub adapter_context: Option<AdapterContextSnapshot>,
    pub provenance: Option<ProvenanceSnapshot>,
    pub process_id: Option<u32>,
    #[serde(default)]
    pub start_time_millis: u64,
    pub executable_path: Option<String>,
    pub command_line: Option<String>,
    pub parent_summary: Option<String>,
    pub launched_by: Option<String>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    #[serde(default)]
    pub memory_physical_footprint_bytes: u64,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricTrend {
    pub friction: Vec<f32>,
    pub cpu_percent: Vec<f32>,
    pub memory_resident_bytes: Vec<u64>,
    pub disk_activity_bps: Vec<u64>,
    #[serde(default)]
    pub network_activity_bps: Vec<u64>,
    #[serde(default)]
    pub wakeups_per_second: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Recommendation {
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCostSummary {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub cost_usd: f32,
    pub total_runs: u32,
    /// Cumulative energy drawn by this AI agent's processes since the
    /// session started, in nanojoules. Accumulated by the history
    /// tracker from per-tick `energy_nj_per_s` rates. Zero when the
    /// kernel does not report energy or on non-macOS platforms.
    ///
    /// Conversion helpers:
    /// - nJ → Wh: divide by 3.6e12
    /// - Wh → mAh at 3.7V nominal: Wh / 3.7 * 1000
    #[serde(default)]
    pub session_energy_nj: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SessionMarkerKind {
    #[default]
    RunStart,
    RunEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionMarker {
    pub timestamp_millis: u64,
    pub kind: SessionMarkerKind,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntitySnapshot {
    pub entity_id: String,
    pub display_name: String,
    pub primary_provenance: Option<ProvenanceSnapshot>,
    #[serde(default)]
    pub launcher_summary: Option<String>,
    #[serde(default)]
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
    #[serde(default)]
    pub recent_change_summary: Option<String>,
    #[serde(default)]
    pub anomaly_detected: bool,
    #[serde(default)]
    pub thermal_contribution: Option<String>,
    #[serde(default)]
    pub grouping_suggestion: Option<String>,
    #[serde(default)]
    pub agent_cost: Option<AgentCostSummary>,
    #[serde(default)]
    pub session_markers: Vec<SessionMarker>,
    #[serde(default)]
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilitySnapshot {
    pub kind: CapabilityKind,
    pub state: CapabilityState,
    pub health: CapabilityHealth,
    pub detail: String,
    pub last_updated_millis: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimelineEvent {
    pub id: String,
    pub timestamp_millis: u64,
    #[serde(default)]
    pub category: TimelineCategory,
    pub severity: TimelineSeverity,
    pub entity_id: Option<String>,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemSnapshot {
    pub sequence: u64,
    pub captured_at_millis: u64,
    pub host: HostSnapshot,
    pub host_trend: HostTrend,
    pub capabilities: Vec<CapabilitySnapshot>,
    pub entities: Vec<EntitySnapshot>,
    pub timeline: Vec<TimelineEvent>,
    /// Per-repository AI cost summary sourced from the Chau7 adapter.
    /// Empty when Chau7 is not connected or no repos have been tracked.
    #[serde(default)]
    pub ai_repo_summaries: Vec<AiRepoSummary>,
}

/// Aggregated cost/usage data for one repository tracked by Chau7.
///
/// Populated from `Chau7RepoStats` during adapter enrichment. The
/// display_name is a shortened version of the full path for UI
/// rendering (e.g. `~/Projects/Aetower` instead of the full absolute
/// path).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiRepoSummary {
    pub repo_path: String,
    pub display_name: String,
    pub total_runs: u32,
    pub total_tokens: u64,
    pub total_cost_usd: f32,
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FanReading {
    pub id: u8,
    pub name: String,
    pub current_rpm: f32,
    pub min_rpm: f32,
    pub max_rpm: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemperatureReading {
    pub label: String,
    pub celsius: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PowerUnit {
    #[default]
    Watts,
    Volts,
    Amps,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PowerReading {
    pub label: String,
    pub value: f32,
    pub unit: PowerUnit,
}

/// macOS battery condition string as reported by the SMC/IOPS layer.
///
/// Apple's System Information displays these exact categories. `Good` and
/// `Fair` are healthy states; `Poor` and `ServiceBattery` indicate the cell
/// has degraded enough that Apple recommends replacement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BatteryCondition {
    #[default]
    Unknown,
    Good,
    Fair,
    Poor,
    ServiceBattery,
}

/// Long-lived battery health metrics sampled from the macOS power management layer.
///
/// These values change slowly (cycle count ticks by 1 per full discharge,
/// design capacity is a static per-device constant) so the collector refreshes
/// them on the low-frequency `HostEnvironment` cadence rather than every tick.
///
/// `health_percent` is derived as `max_capacity_mah / design_capacity_mah` —
/// macOS exposes both values and Apple's own battery report uses the same
/// ratio. An M-series MacBook is rated for ~1000 cycles before reaching 80%.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BatteryHealthSnapshot {
    /// Full discharge cycles logged by the SMC. `None` when the IOPS
    /// dictionary did not expose the key on this tick, distinct from
    /// `Some(0)` which would mean a brand-new battery.
    pub cycle_count: Option<u32>,
    /// Original factory capacity in mAh. `None` when the IOPS
    /// dictionary did not expose the key.
    pub design_capacity_mah: Option<u32>,
    /// Current full-charge capacity in mAh. `None` when the IOPS
    /// dictionary did not expose the key.
    pub max_capacity_mah: Option<u32>,
    /// Ratio of `max_capacity_mah / design_capacity_mah` as a 0-100
    /// percentage. `None` when either input is missing — the ratio
    /// cannot be derived from partial data, and fabricating a `0.0`
    /// would be indistinguishable from a dead battery.
    pub health_percent: Option<f32>,
    pub condition: BatteryCondition,
    pub temperature_celsius: Option<f32>,
}

/// Overall disk health as reported by `diskutil info -plist`'s `SMARTStatus`
/// field.
///
/// Apple only exposes a coarse three-way distinction (`Verified` / `Failing`
/// / `Not Supported`). We add `Warning` so the UI can surface "healthy but
/// nearing end-of-life" when `percentage_used >= 80` even if macOS still
/// calls the drive healthy — that's the earliest actionable signal we have
/// for "back up now, start shopping for a replacement".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DiskHealthStatus {
    #[default]
    Unknown,
    Healthy,
    Warning,
    Failing,
    NotSupported,
}

/// SMART-ish summary for one physical disk.
///
/// Sampled from `diskutil info -plist` on macOS. All fields are optional
/// because non-NVMe drives, external USB enclosures, and older macOS
/// versions may not expose every key — the UI renders "—" for missing
/// values instead of zeroing them out, so the distinction between "not
/// reported" and "genuinely zero" is preserved.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Direction of a threshold check against a sensor value.
///
/// `Above` fires when the value exceeds the threshold — used for
/// temperatures, wakeups, and anything else where "too high" is bad.
/// `Below` fires when the value drops under the threshold — used for
/// battery health percentages and available spare, where "too low" is bad.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThresholdDirection {
    #[default]
    Above,
    Below,
}

/// One configurable threshold for a named sensor reading.
///
/// The threshold has two tiers (`warning_value` and `critical_value`). For
/// `Above` thresholds, `critical_value` must be strictly greater than
/// `warning_value`; for `Below`, strictly less. Tiered alerts let the UI
/// show a yellow chip at first sign of trouble and escalate to red when
/// the situation is actually dangerous.
///
/// `sensor_key` identifies the metric being watched. It is a stable string
/// like `"cpu_temperature"`, `"battery_health"`, or `"fan_0_rpm"` that the
/// history crate uses to look up both the current value and the previous
/// alert state without string comparisons to human-readable labels.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlertThreshold {
    pub sensor_key: String,
    pub warning_value: f32,
    pub critical_value: f32,
    pub direction: ThresholdDirection,
}

/// Current classification of a sensor reading against its threshold.
///
/// Used both as the "previous state" cached inside the history tracker and
/// as the "new state" computed each tick — transitions between the two are
/// what drive `TimelineEvent` emission. `Nominal` is the default state
/// when no tracker has been initialised yet.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AlertLevel {
    #[default]
    Nominal,
    Warning,
    Critical,
}

/// Broad category of a Bluetooth peripheral used to pick the right UI
/// icon.
///
/// macOS's `ClassOfDevice` bitfield technically encodes this, but parsing
/// it reliably is painful — the peripherals users actually care about
/// (keyboards, mice, trackpads, headphones, game controllers) are trivial
/// to disambiguate by name. Anything the heuristic can't classify falls
/// through to `Other` so the UI still renders a row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BluetoothDeviceType {
    #[default]
    Other,
    Keyboard,
    Mouse,
    Trackpad,
    Headphones,
    GameController,
}

/// One wireless peripheral with a reported battery level.
///
/// Sampled from `ioreg` output on macOS. `address` is the stable device
/// identifier (Bluetooth MAC); `battery_percent` is `None` when the
/// peripheral is paired but currently disconnected, so the UI can still
/// show the device in a list without inventing a level.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BluetoothDeviceBattery {
    pub name: String,
    pub address: String,
    pub battery_percent: Option<u8>,
    pub device_type: BluetoothDeviceType,
}

/// One physical or virtual network interface on the host.
///
/// Aetower already surfaces a single `network_receive_bps/send_bps` pair for
/// the entire machine; this record adds the per-interface breakdown without
/// replacing the aggregate. `mac_address` is the stable identifier that
/// survives renaming (e.g. `en0` flipping between Wi-Fi and Ethernet when a
/// USB adapter is plugged in).
///
/// `is_up` is derived from whether the interface holds any assigned IP
/// address at sample time — a good proxy for "configured and reachable"
/// without reaching into platform-specific link-state APIs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkInterfaceSnapshot {
    pub name: String,
    pub mac_address: String,
    pub receive_bps: u64,
    pub send_bps: u64,
    pub is_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrontmostAppState {
    pub app_name: String,
    pub bundle_id: Option<String>,
    pub executable_path: Option<String>,
    pub window_title: Option<String>,
    pub captured_at_millis: u64,
}
