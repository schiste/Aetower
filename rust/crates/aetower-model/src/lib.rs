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
    /// Network activity (e.g. a process opening a connection to a new remote
    /// host). Targeted by connection-rule automation.
    Network,
    /// A regression detected over persisted history (e.g. memory creep over
    /// days, or idle CPU rising after an app update).
    Regression,
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
    /// Per-logical-core CPU load, with best-effort performance/efficiency
    /// classification. Live-only (not persisted in trend rings).
    #[serde(default)]
    pub per_core_cpu: Vec<CoreLoad>,
}

/// Performance vs efficiency core class (Apple Silicon). `Unknown` on Intel or
/// when the perflevel split could not be determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CoreKind {
    #[default]
    Unknown,
    Performance,
    Efficiency,
}

/// One logical CPU core's current load (0–100) plus its core class.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoreLoad {
    pub index: u32,
    pub percent: f32,
    pub kind: CoreKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostTrend {
    pub machine_friction: Vec<f32>,
    pub cpu_percent: Vec<f32>,
    pub memory_used_bytes: Vec<u64>,
    #[serde(default)]
    pub memory_pressure_score: Vec<f32>,
    pub disk_activity_bps: Vec<u64>,
    #[serde(default)]
    pub network_activity_bps: Vec<u64>,
    #[serde(default)]
    pub wakeups_per_second: Vec<f32>,
    #[serde(default)]
    pub compressed_memory_bytes: Vec<u64>,
    #[serde(default)]
    pub ai_agent_friction: Vec<f32>,
    #[serde(default)]
    pub gpu_percent: Vec<f32>,
    #[serde(default)]
    pub gpu_memory_bytes: Vec<u64>,
    /// Max observed CPU die temperature (°C) per sample — powers the thermal
    /// trend HorizonGraph and the throttle forecast slope.
    #[serde(default)]
    pub max_cpu_temperature: Vec<f32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HostPressureBand {
    #[default]
    Nominal,
    Elevated,
    Severe,
}

pub fn effective_memory_bytes(resident_bytes: u64, physical_footprint_bytes: u64) -> u64 {
    physical_footprint_bytes.max(resident_bytes)
}

pub fn entity_effective_memory_bytes(metrics: &AggregateMetrics) -> u64 {
    effective_memory_bytes(
        metrics.memory_resident_bytes,
        metrics.memory_physical_footprint_bytes,
    )
}

pub fn host_memory_used_ratio(host: &HostSnapshot) -> f32 {
    if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.memory_used_bytes as f32 / host.memory_total_bytes as f32
    }
}

pub fn host_memory_pressure_score(host: &HostSnapshot) -> f32 {
    let used_ratio = host_memory_used_ratio(host).min(1.0);
    let compressed_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.compressed_memory_bytes as f32 / host.memory_total_bytes as f32
    };
    let swap_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.swap_used_bytes as f32 / host.memory_total_bytes as f32
    };

    (used_ratio * 55.0 + compressed_ratio.min(1.0) * 25.0 + swap_ratio.min(1.0) * 20.0).min(100.0)
}

pub fn host_memory_pressure_factor(host: &HostSnapshot) -> f32 {
    let used_ratio = host_memory_used_ratio(host);
    let compressed_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.compressed_memory_bytes as f32 / host.memory_total_bytes as f32
    };
    let swap_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.swap_used_bytes as f32 / host.memory_total_bytes as f32
    };

    (used_ratio * 0.7 + compressed_ratio * 1.5 + swap_ratio * 2.0).min(1.0)
}

pub fn machine_friction_score(host: &HostSnapshot) -> f32 {
    let cpu_score = host.cpu_percent.min(100.0) * 0.5;
    let memory_score = host_memory_used_ratio(host).min(1.0) * 35.0;
    let swap_score = if host.swap_used_bytes == 0 {
        0.0
    } else {
        ((host.swap_used_bytes as f32 / 1_073_741_824.0).min(8.0) / 8.0) * 15.0
    };
    let compressed_score = if host.memory_total_bytes == 0 {
        0.0
    } else {
        ((host.compressed_memory_bytes as f32 / host.memory_total_bytes as f32).min(1.0)) * 12.0
    };
    let network_score = ((host
        .network_receive_bps
        .saturating_add(host.network_send_bps)) as f32
        / 8_388_608.0)
        .min(1.0)
        * 10.0;
    let wakeups_score = (host.wakeups_per_second / 500.0).min(1.0) * 8.0;
    (cpu_score + memory_score + swap_score + compressed_score + network_score + wakeups_score)
        .min(100.0)
}

pub fn host_pressure_band(host: &HostSnapshot) -> HostPressureBand {
    let used_ratio = host_memory_used_ratio(host);
    let compressed_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.compressed_memory_bytes as f32 / host.memory_total_bytes as f32
    };
    let swap_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.swap_used_bytes as f32 / host.memory_total_bytes as f32
    };

    if used_ratio >= 0.88 || compressed_ratio >= 0.12 || swap_ratio >= 0.08 {
        HostPressureBand::Severe
    } else if used_ratio >= 0.75 || compressed_ratio >= 0.05 || swap_ratio >= 0.02 {
        HostPressureBand::Elevated
    } else {
        HostPressureBand::Nominal
    }
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
    pub self_cpu_percent: f32,
    #[serde(default)]
    pub self_memory_bytes: u64,
    #[serde(default)]
    pub self_memory_physical_footprint_bytes: u64,
    #[serde(default)]
    pub self_wakeups_per_second: f32,
    #[serde(default)]
    pub self_energy_nj_per_s: f64,
    #[serde(default)]
    pub mcp_total_connections: u64,
    #[serde(default)]
    pub mcp_active_client_count: u32,
    #[serde(default)]
    pub mcp_total_requests: u64,
    #[serde(default)]
    pub mcp_requests_per_second: f32,
    #[serde(default)]
    pub mcp_observed_at_millis: u64,
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
    #[serde(default)]
    pub thread_count: u32,
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
    #[serde(default)]
    pub thread_count: u32,
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

/// How urgent a recommendation is — drives ordering and UI tinting.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RecommendationSeverity {
    /// Informational; no action needed.
    Info,
    /// Worth acting on.
    #[default]
    Suggested,
    /// High-friction; act now.
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Recommendation {
    pub title: String,
    pub detail: String,
    #[serde(default)]
    pub severity: RecommendationSeverity,
    /// A suggested one-click process action, as a raw `ProcessActionKind`
    /// value (e.g. `"suspend"`, `"lower-priority"`). `None` for advisory-only
    /// recommendations. Never a destructive action (kill/terminate).
    #[serde(default)]
    pub suggested_action: Option<String>,
    /// The process the suggested action targets (the heaviest member of the
    /// dominant process group).
    #[serde(default)]
    pub target_pid: Option<u32>,
    /// Human-readable label for the target (e.g. `"Slack Helper"`), used in the
    /// action button.
    #[serde(default)]
    pub target_label: Option<String>,
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
    /// Live network connections attributed to this entity's processes
    /// (same-user, sampled best-effort). Deduped, capped.
    #[serde(default)]
    pub network_connections: Vec<NetworkConnection>,
    /// Code-signing classification of the entity's executable
    /// (apple | mac_app_store | developer_id | adhoc | unsigned | other |
    /// unknown). Resolved lazily by the engine and cached — populated only for
    /// entities that have network connections, otherwise `"unknown"`. Used by
    /// the connection-rule automation predicates (e.g. "alert when an unsigned
    /// binary opens a socket").
    #[serde(default = "unknown_signing")]
    pub signing_classification: String,
    /// Whether the signature is ad-hoc (signed in place with no authorities — a
    /// common malware/dev tell). False when not yet resolved.
    #[serde(default)]
    pub is_adhoc: bool,
    /// VirusTotal reputation for the entity's executable, when resolved. Opt-in
    /// and consent-gated: populated by the engine only for "risky" binaries
    /// (unsigned/ad-hoc with active network connections) when the feature is
    /// enabled and an API key is configured. `None` until looked up (or always,
    /// when the feature is off / the binary is signed).
    #[serde(default)]
    pub binary_reputation: Option<BinaryReputation>,
    /// App version (`CFBundleShortVersionString`) for `.app`-bundled entities,
    /// resolved lazily by the engine and cached. Persisted in snapshots so the
    /// regression detector can spot "idle CPU rose after the last update".
    /// `None` for non-bundled processes or before resolution.
    #[serde(default)]
    pub app_version: Option<String>,
}

fn unknown_signing() -> String {
    "unknown".to_owned()
}

/// Derived safety verdict for a binary from its VirusTotal analysis stats.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReputationVerdict {
    /// No engine flagged the binary.
    Clean,
    /// At least one engine flagged it suspicious (but none malicious).
    Suspicious,
    /// At least one engine flagged it malicious.
    Malicious,
    /// Not present in the VirusTotal corpus, or not yet analysed.
    #[default]
    Unknown,
}

/// VirusTotal reputation for a binary, keyed by its SHA-256. Only the hash is
/// ever sent to VirusTotal — never the file, its path, or any host context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BinaryReputation {
    pub sha256: String,
    pub malicious: u32,
    pub suspicious: u32,
    pub harmless: u32,
    pub undetected: u32,
    pub total_engines: u32,
    pub verdict: ReputationVerdict,
    /// Permalink to the VirusTotal report for this hash (may be empty when the
    /// hash is unknown to VirusTotal).
    pub permalink: String,
    pub checked_at_millis: u64,
}

/// Classify a code signature from its `codesign` authority chain. Returns the
/// classification string and whether the signature is ad-hoc (signed in place
/// with no signing authorities). Developer ID and Mac App Store are checked
/// before the generic Apple leaf because their chains also anchor to Apple
/// roots. Shared by the on-demand inspector (`aetower-mcp`) and the continuous
/// engine resolver (`aetower-core`) so both agree on one taxonomy.
pub fn classify_signature(signed: bool, authority: &[String]) -> (String, bool) {
    if !signed {
        return ("unsigned".to_owned(), false);
    }
    if authority.is_empty() {
        return ("adhoc".to_owned(), true);
    }
    let chain = authority.join(" | ");
    let classification = if chain.contains("Developer ID Application") {
        "developer_id"
    } else if chain.contains("Apple Mac OS Application Signing") {
        "mac_app_store"
    } else if chain.contains("Software Signing") {
        "apple"
    } else {
        "other"
    };
    (classification.to_owned(), false)
}

/// One network socket attributed to a process: protocol, local endpoint, and
/// (for connected sockets) the remote endpoint + state. Listening sockets have
/// `remote == None` and `state == "LISTEN"`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConnection {
    pub protocol: String,
    pub local: String,
    pub remote: Option<String>,
    pub state: String,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceCostScope {
    Entity,
    Session,
    Repository,
    #[default]
    Machine,
}

impl ResourceCostScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Session => "session",
            Self::Repository => "repository",
            Self::Machine => "machine",
        }
    }
}

/// One normalized cost row for an entity, session, repository, or whole machine.
///
/// Monetary and carbon values include only the sources Aetower can currently
/// attribute. For example, Chau7 can provide cumulative AI spend, while kernel
/// energy provides watts and session energy for observed processes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceCostRollup {
    pub scope: ResourceCostScope,
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub repository_path: Option<String>,
    #[serde(default)]
    pub watts: f64,
    #[serde(default)]
    pub energy_watt_hours: f64,
    #[serde(default)]
    pub battery_minutes: Option<f64>,
    #[serde(default)]
    pub dollars: f64,
    #[serde(default)]
    pub carbon_grams: f64,
    #[serde(default)]
    pub disk_growth_bytes: i64,
    #[serde(default)]
    pub thermal_contribution: Option<String>,
    pub source: String,
    pub confidence: f32,
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
    /// Raw Chau7 tab/session summaries, preserved even when process
    /// attribution is incomplete so the UI can show every active tab/session.
    #[serde(default)]
    pub chau7_sessions: Vec<Chau7SessionSummary>,
    /// Normalized resource cost rows for machine, entity, repository, and
    /// session scopes. This is the shared UI/MCP surface for "what did it cost?"
    /// questions.
    #[serde(default)]
    pub resource_cost_rollups: Vec<ResourceCostRollup>,
    /// Heuristic throttle forecast, present only while the Mac is warming and a
    /// throttle is plausibly imminent. `None` when nominal/cooling.
    #[serde(default)]
    pub thermal_forecast: Option<ThermalForecast>,
}

/// A heuristic forecast of imminent thermal throttling, extrapolated from the
/// recent CPU-temperature slope and thermal-state trajectory. An estimate, not
/// a hardware speed-limit reading.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThermalForecast {
    /// Estimated minutes until throttling at the current warming rate. `None`
    /// when already throttling (state == Critical) or not estimable.
    pub minutes_to_throttle: Option<f32>,
    /// Recent warming rate in °C per minute (negative = cooling).
    pub trend_celsius_per_min: f32,
    /// Current thermal state at the time of the forecast.
    pub state: ThermalState,
    /// The top thermal contributor the user could act on.
    pub top_contributor_entity_id: Option<String>,
    pub top_contributor_pid: Option<u32>,
    pub top_contributor_label: Option<String>,
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

/// Live Chau7 tab/session summary sourced from the adapter's cached snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Chau7SessionSummary {
    pub id: String,
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub workspace_path: Option<String>,
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub active_app: Option<String>,
    #[serde(default)]
    pub window_id: u32,
    #[serde(default)]
    pub run_count: u32,
    #[serde(default)]
    pub last_active: String,
    #[serde(default)]
    pub turn_count: u32,
    #[serde(default)]
    pub child_session_count: u32,
    #[serde(default)]
    pub pending_approval_description: Option<String>,
    #[serde(default)]
    pub last_exit_reason: Option<String>,
    #[serde(default)]
    pub active_run_duration_millis: u64,
    #[serde(default)]
    pub is_at_prompt: bool,
    #[serde(default)]
    pub shell_loading: bool,
    #[serde(default)]
    pub cto_active: bool,
    #[serde(default)]
    pub linked_entity_ids: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_signature_taxonomy() {
        // Unsigned and ad-hoc are the two firewall-relevant verdicts; ad-hoc is
        // the "signed but no authority" tell.
        assert_eq!(
            classify_signature(false, &[]),
            ("unsigned".to_owned(), false)
        );
        assert_eq!(classify_signature(true, &[]), ("adhoc".to_owned(), true));

        let developer_id = vec!["Developer ID Application: Acme (TEAMID)".to_owned()];
        assert_eq!(
            classify_signature(true, &developer_id),
            ("developer_id".to_owned(), false)
        );

        let apple = vec!["Software Signing".to_owned()];
        assert_eq!(
            classify_signature(true, &apple),
            ("apple".to_owned(), false)
        );

        let unknown_chain = vec!["Some Other Authority".to_owned()];
        assert_eq!(
            classify_signature(true, &unknown_chain),
            ("other".to_owned(), false)
        );
    }

    #[test]
    fn signing_classification_defaults_to_unknown_on_deserialize() {
        // Snapshots persisted before this field existed must read back as
        // "unknown" (never "" or a false "unsigned"), so the firewall predicate
        // can't misfire on legacy data. Simulate a legacy payload by serializing
        // a current snapshot and stripping the two new keys.
        let Ok(mut value) = serde_json::to_value(EntitySnapshot::default()) else {
            panic!("snapshot serializes");
        };
        let Some(object) = value.as_object_mut() else {
            panic!("snapshot is a JSON object");
        };
        object.remove("signing_classification");
        object.remove("is_adhoc");
        object.remove("binary_reputation");

        let Ok(entity) = serde_json::from_value::<EntitySnapshot>(value) else {
            panic!("legacy snapshot deserializes");
        };
        assert_eq!(entity.signing_classification, "unknown");
        assert!(!entity.is_adhoc);
        assert!(entity.binary_reputation.is_none());
    }

    #[test]
    fn reputation_verdict_defaults_to_unknown() {
        assert_eq!(ReputationVerdict::default(), ReputationVerdict::Unknown);
    }
}
