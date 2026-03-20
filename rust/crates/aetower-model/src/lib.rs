use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum EntityKind {
    App,
    Browser,
    Daemon,
    TerminalSession,
    Service,
    Unknown,
}

impl Default for EntityKind {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentKind {
    Process,
    Command,
    AdapterContext,
}

impl Default for ComponentKind {
    fn default() -> Self {
        Self::Process
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityKind {
    Accessibility,
    FullDiskAccess,
    AppleAutomation,
    ChromiumDebug,
    DockerSocket,
}

impl Default for CapabilityKind {
    fn default() -> Self {
        Self::Accessibility
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityState {
    Unknown,
    Granted,
    Denied,
    Requested,
    Unavailable,
}

impl Default for CapabilityState {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum TimelineSeverity {
    Info,
    Warning,
    Critical,
}

impl Default for TimelineSeverity {
    fn default() -> Self {
        Self::Info
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostSnapshot {
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    pub thermal_state: String,
    pub on_battery: bool,
    pub frontmost_app_name: Option<String>,
    pub frontmost_window_title: Option<String>,
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
    pub process_count: u32,
    pub is_foreground: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrictionBreakdown {
    pub total_score: f32,
    pub cpu_score: f32,
    pub memory_score: f32,
    pub disk_score: f32,
    pub foreground_bonus: f32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComponentSnapshot {
    pub kind: ComponentKind,
    pub title: String,
    pub detail: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntitySnapshot {
    pub entity_id: String,
    pub display_name: String,
    pub bundle_id: Option<String>,
    pub executable_path: Option<String>,
    pub entity_kind: EntityKind,
    pub metrics: AggregateMetrics,
    pub friction: FrictionBreakdown,
    pub components: Vec<ComponentSnapshot>,
    pub badges: Vec<String>,
    pub active_window_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilitySnapshot {
    pub kind: CapabilityKind,
    pub state: CapabilityState,
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
