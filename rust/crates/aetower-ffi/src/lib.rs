use std::sync::Arc;

use aetower_core::Engine;
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
pub enum TimelineSeverity {
    Info,
    Warning,
    Critical,
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
    pub thermal_state: String,
    pub on_battery: bool,
    pub battery_charge_percent: Option<u8>,
    pub low_power_mode: bool,
    pub frontmost_app_name: Option<String>,
    pub frontmost_window_title: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HostTrend {
    pub machine_friction: Vec<f32>,
    pub cpu_percent: Vec<f32>,
    pub memory_used_bytes: Vec<u64>,
    pub disk_activity_bps: Vec<u64>,
    pub network_activity_bps: Vec<u64>,
    pub wakeups_per_second: Vec<f32>,
    pub compressed_memory_bytes: Vec<u64>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct AggregateMetrics {
    pub cpu_percent: f32,
    pub memory_resident_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    pub wakeups_per_second: f32,
    pub process_count: u32,
    pub is_foreground: bool,
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
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
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

#[derive(Clone, Debug, uniffi::Record)]
pub struct MetricTrend {
    pub friction: Vec<f32>,
    pub cpu_percent: Vec<f32>,
    pub memory_resident_bytes: Vec<u64>,
    pub disk_activity_bps: Vec<u64>,
    pub network_activity_bps: Vec<u64>,
    pub wakeups_per_second: Vec<f32>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Recommendation {
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, uniffi::Record)]
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
    pub recommendations: Vec<Recommendation>,
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
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FrontmostAppState {
    pub app_name: String,
    pub bundle_id: Option<String>,
    pub executable_path: Option<String>,
    pub window_title: Option<String>,
    pub captured_at_millis: u64,
}

#[derive(uniffi::Object)]
pub struct MonitorEngine {
    inner: std::sync::Mutex<Engine>,
}

#[uniffi::export]
impl MonitorEngine {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let mut engine = Engine::new();
        engine.start();
        Arc::new(Self {
            inner: std::sync::Mutex::new(engine),
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

    pub fn latest_sequence(&self) -> u64 {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .latest_sequence()
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

    pub fn configure_chau7_endpoint(&self, socket_path: Option<String>) {
        self.inner
            .lock()
            .expect("engine lock poisoned")
            .configure_chau7_endpoint(socket_path);
    }
}

impl Drop for MonitorEngine {
    fn drop(&mut self) {
        if let Ok(mut engine) = self.inner.lock() {
            engine.stop();
        }
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

impl From<model::TimelineSeverity> for TimelineSeverity {
    fn from(value: model::TimelineSeverity) -> Self {
        match value {
            model::TimelineSeverity::Info => Self::Info,
            model::TimelineSeverity::Warning => Self::Warning,
            model::TimelineSeverity::Critical => Self::Critical,
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
            thermal_state: value.thermal_state,
            on_battery: value.on_battery,
            battery_charge_percent: value.battery_charge_percent,
            low_power_mode: value.low_power_mode,
            frontmost_app_name: value.frontmost_app_name,
            frontmost_window_title: value.frontmost_window_title,
        }
    }
}

impl From<model::HostTrend> for HostTrend {
    fn from(value: model::HostTrend) -> Self {
        Self {
            machine_friction: value.machine_friction,
            cpu_percent: value.cpu_percent,
            memory_used_bytes: value.memory_used_bytes,
            disk_activity_bps: value.disk_activity_bps,
            network_activity_bps: value.network_activity_bps,
            wakeups_per_second: value.wakeups_per_second,
            compressed_memory_bytes: value.compressed_memory_bytes,
        }
    }
}

impl From<model::AggregateMetrics> for AggregateMetrics {
    fn from(value: model::AggregateMetrics) -> Self {
        Self {
            cpu_percent: value.cpu_percent,
            memory_resident_bytes: value.memory_resident_bytes,
            virtual_memory_bytes: value.virtual_memory_bytes,
            disk_read_bps: value.disk_read_bps,
            disk_write_bps: value.disk_write_bps,
            network_receive_bps: value.network_receive_bps,
            network_send_bps: value.network_send_bps,
            wakeups_per_second: value.wakeups_per_second,
            process_count: value.process_count,
            is_foreground: value.is_foreground,
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
            reasons: value.reasons,
        }
    }
}

impl From<model::ProvenanceSnapshot> for ProvenanceSnapshot {
    fn from(value: model::ProvenanceSnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            label: value.label,
        }
    }
}

impl From<model::ComponentSnapshot> for ComponentSnapshot {
    fn from(value: model::ComponentSnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            title: value.title,
            detail: value.detail,
            provenance: value.provenance.map(Into::into),
            process_id: value.process_id,
            executable_path: value.executable_path,
            command_line: value.command_line,
            parent_summary: value.parent_summary,
            launched_by: value.launched_by,
            cpu_percent: value.cpu_percent,
            memory_bytes: value.memory_bytes,
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

impl From<model::Recommendation> for Recommendation {
    fn from(value: model::Recommendation) -> Self {
        Self {
            title: value.title,
            detail: value.detail,
        }
    }
}

impl From<model::EntitySnapshot> for EntitySnapshot {
    fn from(value: model::EntitySnapshot) -> Self {
        Self {
            entity_id: value.entity_id,
            display_name: value.display_name,
            primary_provenance: value.primary_provenance.map(Into::into),
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
            recommendations: value.recommendations.into_iter().map(Into::into).collect(),
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
