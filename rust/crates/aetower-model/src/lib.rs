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
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    #[serde(default)]
    pub wakeups_per_second: f32,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrontmostAppState {
    pub app_name: String,
    pub bundle_id: Option<String>,
    pub executable_path: Option<String>,
    pub window_title: Option<String>,
    pub captured_at_millis: u64,
}
