use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceSnapshot {
    pub kind: ProvenanceKind,
    pub label: String,
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
    pub thermal_state: String,
    pub on_battery: bool,
    pub battery_charge_percent: Option<u8>,
    pub low_power_mode: bool,
    pub frontmost_app_name: Option<String>,
    pub frontmost_window_title: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregateMetrics {
    pub cpu_percent: f32,
    pub memory_resident_bytes: u64,
    pub virtual_memory_bytes: u64,
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
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComponentSnapshot {
    pub kind: ComponentKind,
    pub title: String,
    pub detail: String,
    pub provenance: Option<ProvenanceSnapshot>,
    pub process_id: Option<u32>,
    pub executable_path: Option<String>,
    pub command_line: Option<String>,
    pub parent_summary: Option<String>,
    pub launched_by: Option<String>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
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
pub struct EntitySnapshot {
    pub entity_id: String,
    pub display_name: String,
    pub primary_provenance: Option<ProvenanceSnapshot>,
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
