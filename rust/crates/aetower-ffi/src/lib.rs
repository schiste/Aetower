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
pub enum TimelineSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Debug, uniffi::Record)]
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

#[derive(Clone, Debug, uniffi::Record)]
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

#[derive(Clone, Debug, uniffi::Record)]
pub struct FrictionBreakdown {
    pub total_score: f32,
    pub cpu_score: f32,
    pub memory_score: f32,
    pub disk_score: f32,
    pub foreground_bonus: f32,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ComponentSnapshot {
    pub kind: ComponentKind,
    pub title: String,
    pub detail: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
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

#[derive(Clone, Debug, uniffi::Record)]
pub struct CapabilitySnapshot {
    pub kind: CapabilityKind,
    pub state: CapabilityState,
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
            network_receive_bps: value.network_receive_bps,
            network_send_bps: value.network_send_bps,
            thermal_state: value.thermal_state,
            on_battery: value.on_battery,
            frontmost_app_name: value.frontmost_app_name,
            frontmost_window_title: value.frontmost_window_title,
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
            foreground_bonus: value.foreground_bonus,
            reasons: value.reasons,
        }
    }
}

impl From<model::ComponentSnapshot> for ComponentSnapshot {
    fn from(value: model::ComponentSnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            title: value.title,
            detail: value.detail,
            cpu_percent: value.cpu_percent,
            memory_bytes: value.memory_bytes,
        }
    }
}

impl From<model::EntitySnapshot> for EntitySnapshot {
    fn from(value: model::EntitySnapshot) -> Self {
        Self {
            entity_id: value.entity_id,
            display_name: value.display_name,
            bundle_id: value.bundle_id,
            executable_path: value.executable_path,
            entity_kind: value.entity_kind.into(),
            metrics: value.metrics.into(),
            friction: value.friction.into(),
            components: value.components.into_iter().map(Into::into).collect(),
            badges: value.badges,
            active_window_title: value.active_window_title,
        }
    }
}

impl From<model::CapabilitySnapshot> for CapabilitySnapshot {
    fn from(value: model::CapabilitySnapshot) -> Self {
        Self {
            kind: value.kind.into(),
            state: value.state.into(),
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
