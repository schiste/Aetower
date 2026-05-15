use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    io::{Read, Write},
    net::TcpStream,
    os::unix::net::UnixStream,
    path::Path,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsStore, DiagnosticsSubsystem,
};
use aetower_model::{
    AdapterContextKind, AdapterContextSnapshot, AttributionConfidence, CapabilityHealth,
    CapabilityKind, CapabilitySnapshot, CapabilityState, ComponentKind, ComponentSnapshot,
    EntitySnapshot, ProvenanceKind, ProvenanceSnapshot,
};
use aetower_time as time;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};
use tungstenite::{Message, connect};
use url::Url;

const CHROMIUM_TIMEOUT: Duration = Duration::from_millis(300);
const DOCKER_TIMEOUT: Duration = Duration::from_millis(300);
const CHROMIUM_REFRESH_INTERVAL_MILLIS: u64 = 10_000;
const DOCKER_REFRESH_INTERVAL_MILLIS: u64 = 10_000;
const PRIVILEGED_HELPER_REFRESH_INTERVAL_MILLIS: u64 = 10_000;
const CHROMIUM_FETCH_BUDGET: Duration = Duration::from_millis(750);
const DOCKER_FETCH_BUDGET: Duration = Duration::from_millis(750);
// 30 s instead of 10 s: the chau7 adapter fires ~40 serial JSON-RPC calls per
// refresh against Chau7's single-threaded MCP server, and at 10 s tick we left
// no quiet windows for Chau7 to GC, persist async, or unload state. 30 s cuts
// observer-induced pressure by 3x while keeping snapshot freshness reasonable.
const CHAU7_REFRESH_INTERVAL_MILLIS: u64 = 30_000;
// Deep refresh used to run every 4th light tick (every 120 s). Since
// `Chau7Snapshot::inherit_deep_context_from` carries runtime_info, tab_statuses,
// runtime_sessions, repo_stats, repo_events, and recent_runs forward across
// light fetches, deep refreshes are only needed to occasionally re-seed that
// state — not to keep the UI accurate. Stretching to 300 s cuts deep refreshes
// from 30/h to 12/h, eliminating ~36% of the adapter's RPC traffic and the
// CPU spikes that show up in `aetower_runtime_burst_explanation`.
const CHAU7_DEEP_REFRESH_INTERVAL_MILLIS: u64 = 300_000;
const CHAU7_LIGHT_MAX_AI_TABS: usize = 4;
const CHAU7_DEEP_MAX_AI_TABS: usize = 8;
const CHAU7_DEEP_MAX_REPOS: usize = 3;
const ENDPOINT_SECURITY_REFRESH_INTERVAL_MILLIS: u64 = 30_000;
const MAX_CHROMIUM_TARGETS: usize = 5;
const MAX_DOCKER_CONTAINERS: usize = 5;
const MAX_CHAU7_TABS: usize = 30;

#[derive(Clone)]
pub struct AdapterManager {
    state: Arc<Mutex<AdapterState>>,
}

#[derive(Debug, Default)]
struct AdapterState {
    diagnostics: Option<DiagnosticsStore>,
    chromium_endpoint: Option<String>,
    docker_socket_path: String,
    privileged_helper_path: Option<String>,
    privileged_helper_enabled: bool,
    chromium_samples: BTreeMap<String, ChromiumRuntimeSample>,
    last_chromium_fetch_millis: u64,
    last_chromium_success_millis: u64,
    chromium_last_error: Option<String>,
    cached_chromium_targets: Vec<ChromiumPageTarget>,
    last_docker_fetch_millis: u64,
    last_docker_success_millis: u64,
    docker_last_error: Option<String>,
    cached_docker_containers: Vec<DockerContainer>,
    last_privileged_helper_fetch_millis: u64,
    last_privileged_helper_success_millis: u64,
    privileged_helper_last_error: Option<String>,
    cached_privileged_helper_sample: Option<PrivilegedHelperSnapshot>,
    last_endpoint_security_fetch_millis: u64,
    last_endpoint_security_success_millis: u64,
    endpoint_security_last_error: Option<String>,
    cached_endpoint_security_status: Option<EndpointSecurityStatusSnapshot>,
    cached_endpoint_security_sample: Option<EndpointSecuritySample>,
    chau7_socket_path: Option<String>,
    last_chau7_fetch_millis: u64,
    last_chau7_deep_fetch_millis: u64,
    last_chau7_success_millis: u64,
    chau7_last_error: Option<String>,
    cached_chau7_snapshot: Option<crate::chau7::Chau7Snapshot>,
}

struct Chau7FetchPlan {
    socket_path: String,
    options: crate::chau7::Chau7FetchOptions,
    mode: &'static str,
}

#[derive(Debug, Clone)]
struct ChromiumAdapterConfig {
    host: String,
    port: u16,
    path: String,
}

#[derive(Debug, Clone)]
struct DockerAdapterConfig {
    socket_path: String,
}

#[derive(Debug, Clone)]
struct ChromiumRuntimeSample {
    captured_at_millis: u64,
    task_duration_seconds: f64,
    network_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct ChromiumTarget {
    #[serde(rename = "type")]
    target_type: Option<String>,
    id: Option<String>,
    title: Option<String>,
    url: Option<String>,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DockerContainerSummary {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Names")]
    names: Vec<String>,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Ports", default)]
    ports: Vec<DockerPortSummary>,
}

#[derive(Debug, Deserialize)]
struct DockerPortSummary {
    #[serde(rename = "IP")]
    ip: Option<String>,
    #[serde(rename = "PrivatePort")]
    private_port: u16,
    #[serde(rename = "PublicPort")]
    public_port: Option<u16>,
    #[serde(rename = "Type")]
    port_type: String,
}

#[derive(Debug, Deserialize)]
struct DockerContainerStats {
    #[serde(rename = "cpu_stats")]
    cpu_stats: DockerCpuStats,
    #[serde(rename = "precpu_stats")]
    pre_cpu_stats: DockerCpuStats,
    #[serde(rename = "memory_stats")]
    memory_stats: DockerMemoryStats,
    #[serde(default)]
    networks: BTreeMap<String, DockerNetworkStats>,
    #[serde(rename = "blkio_stats")]
    blkio_stats: DockerBlkioStats,
    #[serde(rename = "pids_stats")]
    pids_stats: DockerPidsStats,
}

#[derive(Debug, Default, Deserialize)]
struct DockerCpuStats {
    #[serde(rename = "cpu_usage")]
    cpu_usage: DockerCpuUsage,
    system_cpu_usage: Option<u64>,
    online_cpus: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerCpuUsage {
    total_usage: u64,
    #[serde(default)]
    percpu_usage: Vec<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerMemoryStats {
    usage: Option<u64>,
    limit: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerNetworkStats {
    rx_bytes: u64,
    tx_bytes: u64,
}

#[derive(Debug, Default, Deserialize)]
struct DockerBlkioStats {
    #[serde(default, rename = "io_service_bytes_recursive")]
    io_service_bytes_recursive: Vec<DockerBlkioEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerBlkioEntry {
    op: Option<String>,
    value: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerPidsStats {
    current: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct PrivilegedHelperSnapshot {
    processes: Vec<PrivilegedProcessSample>,
}

#[derive(Debug, Clone, Deserialize)]
struct PrivilegedProcessSample {
    pid: u32,
    process_name: String,
    executable_name: Option<String>,
    connections: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct EndpointSecurityStatusSnapshot {
    supported: bool,
    helper_entitled: bool,
    running_as_root: bool,
    can_stream_events: bool,
    eslogger_path: Option<String>,
    detail: String,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct EndpointSecuritySample {
    status: EndpointSecurityStatusSnapshot,
    events: Vec<EndpointSecurityLifecycleEvent>,
}

#[derive(Debug, Clone, Deserialize)]
struct EndpointSecurityLifecycleEvent {
    event_type: String,
    timestamp_millis: u64,
    process_path: Option<String>,
    pid: Option<u32>,
    parent_pid: Option<u32>,
    child_pid: Option<u32>,
    exit_code: Option<i32>,
    signal: Option<i32>,
}

impl Default for AdapterManager {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(AdapterState {
                diagnostics: None,
                chromium_endpoint: env::var("AETOWER_CHROMIUM_ENDPOINT").ok(),
                docker_socket_path: docker_socket_path(),
                privileged_helper_path: env::var("AETOWER_PRIVILEGED_HELPER").ok(),
                privileged_helper_enabled: env::var("AETOWER_PRIVILEGED_HELPER_ENABLED")
                    .ok()
                    .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                    .unwrap_or(false),
                chromium_samples: BTreeMap::new(),
                last_chromium_fetch_millis: 0,
                last_chromium_success_millis: 0,
                chromium_last_error: None,
                cached_chromium_targets: Vec::new(),
                last_docker_fetch_millis: 0,
                last_docker_success_millis: 0,
                docker_last_error: None,
                cached_docker_containers: Vec::new(),
                last_privileged_helper_fetch_millis: 0,
                last_privileged_helper_success_millis: 0,
                privileged_helper_last_error: None,
                cached_privileged_helper_sample: None,
                last_endpoint_security_fetch_millis: 0,
                last_endpoint_security_success_millis: 0,
                endpoint_security_last_error: None,
                cached_endpoint_security_status: None,
                cached_endpoint_security_sample: None,
                chau7_socket_path: chau7_socket_path(),
                last_chau7_fetch_millis: 0,
                last_chau7_deep_fetch_millis: 0,
                last_chau7_success_millis: 0,
                chau7_last_error: None,
                cached_chau7_snapshot: None,
            })),
        }
    }
}

impl AdapterManager {
    pub fn set_diagnostics(&self, diagnostics: DiagnosticsStore) {
        self.state.lock().diagnostics = Some(diagnostics);
    }

    pub fn initial_capabilities(&self) -> BTreeMap<CapabilityKind, CapabilitySnapshot> {
        let now = time::now_millis();
        BTreeMap::from([
            (
                CapabilityKind::Accessibility,
                CapabilitySnapshot {
                    kind: CapabilityKind::Accessibility,
                    state: CapabilityState::Unknown,
                    health: CapabilityHealth::Configured,
                    detail: "Not checked yet. Use Request / Refresh to inspect Accessibility access for richer window and UI context."
                        .to_owned(),
                    last_updated_millis: now,
                },
            ),
            (
                CapabilityKind::FullDiskAccess,
                CapabilitySnapshot {
                    kind: CapabilityKind::FullDiskAccess,
                    state: CapabilityState::Unknown,
                    health: CapabilityHealth::Configured,
                    detail:
                        "Not checked yet. Use Request / Refresh to open Full Disk Access and verify protected-path visibility."
                            .to_owned(),
                    last_updated_millis: now,
                },
            ),
            (
                CapabilityKind::AppleAutomation,
                CapabilitySnapshot {
                    kind: CapabilityKind::AppleAutomation,
                    state: CapabilityState::Unknown,
                    health: CapabilityHealth::Configured,
                    detail: "Not checked yet. Use Request / Refresh to probe Apple Events automation for scriptable app context."
                        .to_owned(),
                    last_updated_millis: now,
                },
            ),
            (
                CapabilityKind::ChromiumDebug,
                self.capability_snapshot(CapabilityKind::ChromiumDebug, now),
            ),
            (
                CapabilityKind::DockerSocket,
                self.capability_snapshot(CapabilityKind::DockerSocket, now),
            ),
            (
                CapabilityKind::PrivilegedHelper,
                self.capability_snapshot(CapabilityKind::PrivilegedHelper, now),
            ),
            (
                CapabilityKind::EndpointSecurity,
                self.capability_snapshot(CapabilityKind::EndpointSecurity, now),
            ),
            (
                CapabilityKind::Chau7,
                self.capability_snapshot(CapabilityKind::Chau7, now),
            ),
        ])
    }

    pub fn capability_snapshot(
        &self,
        kind: CapabilityKind,
        last_updated_millis: u64,
    ) -> CapabilitySnapshot {
        let guard = self.state.lock();
        let (state, detail) = capability_status(&guard, &kind);
        let status_millis =
            capability_status_timestamp(&guard, &kind).unwrap_or(last_updated_millis);
        let health = capability_health(&guard, &kind, time::now_millis());
        CapabilitySnapshot {
            kind,
            state,
            health,
            detail,
            last_updated_millis: status_millis,
        }
    }

    pub fn configure_chromium_endpoint(&self, endpoint: Option<String>) {
        let mut guard = self.state.lock();
        guard.chromium_endpoint = endpoint.filter(|value| !value.trim().is_empty());
        guard.chromium_samples.clear();
        guard.cached_chromium_targets.clear();
        guard.last_chromium_fetch_millis = 0;
        guard.last_chromium_success_millis = 0;
        guard.chromium_last_error = None;
    }

    pub fn configure_docker_socket_path(&self, socket_path: String) {
        let mut guard = self.state.lock();
        guard.docker_socket_path = if socket_path.trim().is_empty() {
            "/var/run/docker.sock".to_owned()
        } else {
            socket_path
        };
        guard.cached_docker_containers.clear();
        guard.last_docker_fetch_millis = 0;
        guard.last_docker_success_millis = 0;
        guard.docker_last_error = None;
    }

    pub fn configure_privileged_helper(&self, helper_path: Option<String>, enabled: bool) {
        let mut guard = self.state.lock();
        guard.privileged_helper_path = helper_path.filter(|value| !value.trim().is_empty());
        guard.privileged_helper_enabled = enabled;
        guard.cached_privileged_helper_sample = None;
        guard.last_privileged_helper_fetch_millis = 0;
        guard.last_privileged_helper_success_millis = 0;
        guard.privileged_helper_last_error = None;
    }

    pub fn configure_chau7_endpoint(&self, socket_path: Option<String>) {
        let mut guard = self.state.lock();
        guard.chau7_socket_path = sanitize_chau7_socket_path(socket_path);
        guard.cached_chau7_snapshot = None;
        guard.last_chau7_fetch_millis = 0;
        guard.last_chau7_deep_fetch_millis = 0;
        guard.last_chau7_success_millis = 0;
        guard.chau7_last_error = None;
    }

    /// Extract per-repository AI cost summaries from the cached Chau7
    /// snapshot. Returns an empty Vec when Chau7 is not connected or no
    /// repos have been tracked yet.
    pub fn ai_repo_summaries(&self) -> Vec<aetower_model::AiRepoSummary> {
        let guard = self.state.lock();
        let Some(snapshot) = guard.cached_chau7_snapshot.as_ref() else {
            return Vec::new();
        };
        snapshot
            .repo_stats
            .iter()
            .map(|(repo_path, stats)| aetower_model::AiRepoSummary {
                repo_path: repo_path.clone(),
                display_name: shorten_home_path(repo_path),
                total_runs: stats.total_runs,
                total_tokens: stats.total_tokens,
                total_cost_usd: stats.total_cost,
                providers: stats.providers.clone(),
            })
            .collect()
    }

    /// Extract the live Chau7 tab/session catalog from the cached adapter
    /// snapshot and attach any currently linked Aetower entity ids.
    pub fn chau7_session_summaries(
        &self,
        entities: &[EntitySnapshot],
    ) -> Vec<aetower_model::Chau7SessionSummary> {
        let snapshot = {
            let guard = self.state.lock();
            guard.cached_chau7_snapshot.clone()
        };
        let Some(snapshot) = snapshot else {
            return Vec::new();
        };

        let mut entity_ids_by_session = BTreeMap::<String, BTreeSet<String>>::new();
        let mut entity_ids_by_repo = BTreeMap::<String, BTreeSet<String>>::new();

        for entity in entities {
            let mut session_ids = BTreeSet::new();
            let mut repo_paths = BTreeSet::new();

            for badge in &entity.badges {
                if let Some(session_id) = badge.strip_prefix("ai-session:")
                    && !session_id.is_empty()
                {
                    session_ids.insert(session_id.to_owned());
                }
            }

            for component in &entity.components {
                if let Some(context) = component.adapter_context.as_ref() {
                    if let Some(session_id) = context.session_id.as_deref()
                        && !session_id.is_empty()
                    {
                        session_ids.insert(session_id.to_owned());
                    }
                    if let Some(repo_root) = context.repo_root.as_deref()
                        && !repo_root.is_empty()
                    {
                        repo_paths.insert(repo_root.to_owned());
                    }
                    if let Some(workspace_path) = context.workspace_path.as_deref()
                        && !workspace_path.is_empty()
                    {
                        repo_paths.insert(workspace_path.to_owned());
                    }
                }
                if let Some(cwd) = component.cwd.as_deref()
                    && !cwd.is_empty()
                {
                    repo_paths.insert(cwd.to_owned());
                }
            }

            for session_id in session_ids {
                entity_ids_by_session
                    .entry(session_id)
                    .or_default()
                    .insert(entity.entity_id.clone());
            }
            for repo_path in repo_paths {
                entity_ids_by_repo
                    .entry(repo_path)
                    .or_default()
                    .insert(entity.entity_id.clone());
            }
        }

        let mut summaries = Vec::new();
        let mut seen_session_ids = BTreeSet::new();

        for tab in &snapshot.tabs {
            let tab_status = snapshot.tab_statuses.get(&tab.tab_id);
            let session_state = tab
                .ai_session_id
                .as_deref()
                .and_then(|session_id| snapshot.runtime_sessions.get(session_id));
            let session = tab.ai_session_id.as_deref().and_then(|session_id| {
                snapshot
                    .sessions
                    .iter()
                    .find(|item| item.session_id == session_id)
            });
            if let Some(session_id) = tab.ai_session_id.as_deref() {
                seen_session_ids.insert(session_id.to_owned());
            }

            let linked_entity_ids = linked_chau7_entity_ids(
                &entity_ids_by_session,
                &entity_ids_by_repo,
                tab.ai_session_id.as_deref(),
                tab.repo_root.as_deref(),
                Some(tab.cwd.as_str()),
            );

            summaries.push(aetower_model::Chau7SessionSummary {
                id: format!("tab:{}", tab.tab_id),
                tab_id: Some(tab.tab_id.clone()),
                session_id: tab.ai_session_id.clone(),
                title: chau7_catalog_title(
                    tab_status.and_then(|status| {
                        (!status.title.is_empty()).then_some(status.title.as_str())
                    }),
                    (!tab.title.is_empty()).then_some(tab.title.as_str()),
                    tab.active_app.as_deref(),
                    tab.ai_session_id.as_deref(),
                ),
                provider: tab
                    .ai_provider
                    .clone()
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        session
                            .map(|item| item.provider.clone())
                            .filter(|value| !value.is_empty())
                    })
                    .unwrap_or_default(),
                status: chau7_catalog_status(tab, tab_status, session_state),
                workspace_path: (!tab.cwd.is_empty()).then(|| tab.cwd.clone()),
                repo_root: tab.repo_root.clone(),
                git_branch: tab
                    .git_branch
                    .clone()
                    .or_else(|| tab_status.and_then(|status| status.git_branch.clone())),
                active_app: tab.active_app.clone(),
                window_id: tab.window_id,
                run_count: session.map(|item| item.run_count).unwrap_or_default(),
                last_active: session
                    .map(|item| item.last_active.clone())
                    .unwrap_or_default(),
                turn_count: session_state
                    .map(|item| item.turn_count)
                    .unwrap_or_default(),
                child_session_count: session_state
                    .map(|item| item.child_session_count)
                    .unwrap_or_default(),
                pending_approval_description: session_state
                    .and_then(|item| item.pending_approval.as_ref())
                    .map(|approval| approval.description.clone())
                    .filter(|value| !value.is_empty()),
                last_exit_reason: session_state
                    .and_then(|item| item.last_exit_reason.clone())
                    .filter(|value| !value.is_empty()),
                active_run_duration_millis: session_state
                    .and_then(|item| item.active_run.as_ref())
                    .map(|run| run.duration_so_far_ms)
                    .unwrap_or_default(),
                is_at_prompt: tab_status.map(|item| item.is_at_prompt).unwrap_or(false),
                shell_loading: tab_status.map(|item| item.shell_loading).unwrap_or(false),
                cto_active: tab_status.map(|item| item.cto_active).unwrap_or(false),
                linked_entity_ids,
            });
        }

        for session in &snapshot.sessions {
            if !seen_session_ids.insert(session.session_id.clone()) {
                continue;
            }
            let session_state = snapshot.runtime_sessions.get(&session.session_id);
            let linked_entity_ids = linked_chau7_entity_ids(
                &entity_ids_by_session,
                &entity_ids_by_repo,
                Some(session.session_id.as_str()),
                Some(session.repo_path.as_str()),
                Some(session.repo_path.as_str()),
            );
            summaries.push(aetower_model::Chau7SessionSummary {
                id: format!("session:{}", session.session_id),
                tab_id: None,
                session_id: Some(session.session_id.clone()),
                title: chau7_catalog_title(None, None, None, Some(session.session_id.as_str())),
                provider: session.provider.clone(),
                status: session_state
                    .map(|item| item.state.clone())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "detached".to_owned()),
                workspace_path: (!session.repo_path.is_empty()).then(|| session.repo_path.clone()),
                repo_root: (!session.repo_path.is_empty()).then(|| session.repo_path.clone()),
                git_branch: None,
                active_app: None,
                window_id: 0,
                run_count: session.run_count,
                last_active: session.last_active.clone(),
                turn_count: session_state
                    .map(|item| item.turn_count)
                    .unwrap_or_default(),
                child_session_count: session_state
                    .map(|item| item.child_session_count)
                    .unwrap_or_default(),
                pending_approval_description: session_state
                    .and_then(|item| item.pending_approval.as_ref())
                    .map(|approval| approval.description.clone())
                    .filter(|value| !value.is_empty()),
                last_exit_reason: session_state
                    .and_then(|item| item.last_exit_reason.clone())
                    .filter(|value| !value.is_empty()),
                active_run_duration_millis: session_state
                    .and_then(|item| item.active_run.as_ref())
                    .map(|run| run.duration_so_far_ms)
                    .unwrap_or_default(),
                is_at_prompt: false,
                shell_loading: false,
                cto_active: false,
                linked_entity_ids,
            });
        }

        summaries.sort_by(|left, right| {
            let left_approval = left.pending_approval_description.is_some();
            let right_approval = right.pending_approval_description.is_some();
            if left_approval != right_approval {
                return right_approval.cmp(&left_approval);
            }
            let left_linked = left.linked_entity_ids.len();
            let right_linked = right.linked_entity_ids.len();
            if left_linked != right_linked {
                return right_linked.cmp(&left_linked);
            }
            left.title.to_lowercase().cmp(&right.title.to_lowercase())
        });
        summaries
    }

    pub fn stop_chau7_session(&self, session_id: &str, force: bool) -> Result<(), String> {
        let guard = self.state.lock();
        let socket_path = guard
            .chau7_socket_path
            .as_deref()
            .ok_or_else(|| "no Chau7 socket configured".to_owned())?
            .to_owned();
        drop(guard);
        crate::chau7::stop_session(&socket_path, session_id, force)
    }

    /// Pin a fan to a minimum RPM by shelling out to the privileged helper.
    ///
    /// Fan control requires SMC writes, which in turn require root. The
    /// sandboxed app cannot write SMC keys directly, so the flow is always
    /// app → `aetower-helper fan-set <id> <rpm>` → SMC. If the helper is not
    /// configured (either no path or the user disabled it), this returns a
    /// clear error rather than silently no-oping.
    pub fn set_fan_min_rpm(&self, fan_id: u8, rpm: f32) -> Result<(), String> {
        let helper_path = self
            .state
            .lock()
            .privileged_helper_path()
            .ok_or_else(|| "privileged helper not configured".to_owned())?;
        invoke_helper_fan_command(&helper_path, "fan-set", fan_id, Some(rpm))
    }

    /// Restore a fan to automatic (OS-controlled) mode.
    ///
    /// Always paired with `set_fan_min_rpm` — the UI should present these
    /// together so users have an obvious way out if a manual override makes
    /// the machine too quiet under load or too loud at idle.
    pub fn reset_fan_auto(&self, fan_id: u8) -> Result<(), String> {
        let helper_path = self
            .state
            .lock()
            .privileged_helper_path()
            .ok_or_else(|| "privileged helper not configured".to_owned())?;
        invoke_helper_fan_command(&helper_path, "fan-reset", fan_id, None)
    }

    pub fn refresh_caches(&self, capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>) {
        let now = time::now_millis();
        let (
            chromium_fetch_plan,
            docker_fetch_plan,
            privileged_helper_fetch_path,
            endpoint_security_fetch_path,
            chau7_fetch_path,
        ) = {
            let guard = self.state.lock();

            let chromium_fetch_plan = capabilities
                .get(&CapabilityKind::ChromiumDebug)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .and_then(|_| {
                    let is_stale = now.saturating_sub(guard.last_chromium_fetch_millis)
                        >= CHROMIUM_REFRESH_INTERVAL_MILLIS;
                    if is_stale {
                        guard
                            .chromium_config()
                            .map(|config| (config, guard.chromium_samples.clone()))
                    } else {
                        None
                    }
                });

            let docker_fetch_plan = capabilities
                .get(&CapabilityKind::DockerSocket)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .and_then(|_| {
                    let is_stale = now.saturating_sub(guard.last_docker_fetch_millis)
                        >= DOCKER_REFRESH_INTERVAL_MILLIS;
                    if is_stale {
                        Some(DockerAdapterConfig {
                            socket_path: guard.docker_socket_path.clone(),
                        })
                    } else {
                        None
                    }
                });

            let privileged_helper_fetch_path = capabilities
                .get(&CapabilityKind::PrivilegedHelper)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .and_then(|_| {
                    let is_stale = now.saturating_sub(guard.last_privileged_helper_fetch_millis)
                        >= PRIVILEGED_HELPER_REFRESH_INTERVAL_MILLIS;
                    if is_stale {
                        guard.privileged_helper_path()
                    } else {
                        None
                    }
                });

            let endpoint_security_fetch_path = guard.privileged_helper_path().filter(|path| {
                now.saturating_sub(guard.last_endpoint_security_fetch_millis)
                    >= ENDPOINT_SECURITY_REFRESH_INTERVAL_MILLIS
                    && Path::new(path).exists()
            });

            let chau7_fetch_path = capabilities
                .get(&CapabilityKind::Chau7)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .and_then(|_| {
                    let is_stale = now.saturating_sub(guard.last_chau7_fetch_millis)
                        >= CHAU7_REFRESH_INTERVAL_MILLIS;
                    if !is_stale {
                        return None;
                    }
                    let socket_path = resolved_chau7_socket_path(&guard)?;
                    let needs_deep_refresh = guard.cached_chau7_snapshot.is_none()
                        || now.saturating_sub(guard.last_chau7_deep_fetch_millis)
                            >= CHAU7_DEEP_REFRESH_INTERVAL_MILLIS;
                    if needs_deep_refresh {
                        Some(Chau7FetchPlan {
                            socket_path,
                            mode: "deep",
                            options: crate::chau7::Chau7FetchOptions {
                                max_ai_tabs: CHAU7_DEEP_MAX_AI_TABS,
                                max_repos: CHAU7_DEEP_MAX_REPOS,
                                include_deep_context: true,
                            },
                        })
                    } else {
                        Some(Chau7FetchPlan {
                            socket_path,
                            mode: "light",
                            options: crate::chau7::Chau7FetchOptions {
                                max_ai_tabs: CHAU7_LIGHT_MAX_AI_TABS,
                                max_repos: 0,
                                include_deep_context: false,
                            },
                        })
                    }
                });

            (
                chromium_fetch_plan,
                docker_fetch_plan,
                privileged_helper_fetch_path,
                endpoint_security_fetch_path,
                chau7_fetch_path,
            )
        };

        // Fetch all adapters in parallel using scoped threads.
        std::thread::scope(|s| {
            let chromium_handle = chromium_fetch_plan.map(|(config, mut samples)| {
                s.spawn(move || {
                    fetch_chromium_targets(&config, &mut samples).map(|targets| (targets, samples))
                })
            });
            let docker_handle =
                docker_fetch_plan.map(|config| s.spawn(move || fetch_docker_containers(&config)));
            let helper_handle = privileged_helper_fetch_path
                .map(|path| s.spawn(move || fetch_privileged_helper_sample(&path)));
            let endpoint_security_handle = endpoint_security_fetch_path
                .map(|path| s.spawn(move || fetch_endpoint_security_sample(&path)));
            let chau7_handle = chau7_fetch_path.map(|plan| {
                s.spawn(move || {
                    crate::chau7::fetch_snapshot_with_options(&plan.socket_path, plan.options)
                        .map(|snapshot| (snapshot, plan.mode))
                })
            });

            if let Some(handle) = chromium_handle {
                emit_adapter_refresh_event(
                    &self.state,
                    DiagnosticsLevel::Debug,
                    DiagnosticsSubsystem::AdapterChromium,
                    "adapter-refresh-started",
                    "Starting Chromium adapter refresh.",
                    now,
                    |builder| builder.adapter("chromium"),
                );
                match handle.join().expect("chromium thread panicked") {
                    Ok((fetched, samples)) => {
                        let mut guard = self.state.lock();
                        guard.chromium_samples = samples;
                        guard.cached_chromium_targets = fetched;
                        guard.last_chromium_fetch_millis = now;
                        guard.last_chromium_success_millis = now;
                        guard.chromium_last_error = None;
                        let item_count = guard.cached_chromium_targets.len();
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Info,
                            DiagnosticsSubsystem::AdapterChromium,
                            "adapter-refresh-succeeded",
                            "Chromium adapter refresh succeeded.",
                            now,
                            |builder| builder.adapter("chromium").field("item_count", item_count),
                        );
                    }
                    Err(error) => {
                        let mut guard = self.state.lock();
                        guard.last_chromium_fetch_millis = now;
                        guard.chromium_last_error = Some(error);
                        let error = guard.chromium_last_error.clone().unwrap_or_default();
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Error,
                            DiagnosticsSubsystem::AdapterChromium,
                            "adapter-refresh-failed",
                            "Chromium adapter refresh failed.",
                            now,
                            |builder| builder.adapter("chromium").field("error", error),
                        );
                    }
                }
            }
            if let Some(handle) = docker_handle {
                emit_adapter_refresh_event(
                    &self.state,
                    DiagnosticsLevel::Debug,
                    DiagnosticsSubsystem::AdapterDocker,
                    "adapter-refresh-started",
                    "Starting Docker adapter refresh.",
                    now,
                    |builder| builder.adapter("docker"),
                );
                match handle.join().expect("docker thread panicked") {
                    Ok(fetched) => {
                        let mut guard = self.state.lock();
                        guard.cached_docker_containers = fetched;
                        guard.last_docker_fetch_millis = now;
                        guard.last_docker_success_millis = now;
                        guard.docker_last_error = None;
                        let item_count = guard.cached_docker_containers.len();
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Info,
                            DiagnosticsSubsystem::AdapterDocker,
                            "adapter-refresh-succeeded",
                            "Docker adapter refresh succeeded.",
                            now,
                            |builder| builder.adapter("docker").field("item_count", item_count),
                        );
                    }
                    Err(error) => {
                        let mut guard = self.state.lock();
                        guard.last_docker_fetch_millis = now;
                        guard.docker_last_error = Some(error);
                        let error = guard.docker_last_error.clone().unwrap_or_default();
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Error,
                            DiagnosticsSubsystem::AdapterDocker,
                            "adapter-refresh-failed",
                            "Docker adapter refresh failed.",
                            now,
                            |builder| builder.adapter("docker").field("error", error),
                        );
                    }
                }
            }
            if let Some(handle) = helper_handle {
                emit_adapter_refresh_event(
                    &self.state,
                    DiagnosticsLevel::Debug,
                    DiagnosticsSubsystem::AdapterHelper,
                    "adapter-refresh-started",
                    "Starting privileged helper refresh.",
                    now,
                    |builder| builder.adapter("helper"),
                );
                match handle.join().expect("helper thread panicked") {
                    Ok(fetched) => {
                        let mut guard = self.state.lock();
                        guard.cached_privileged_helper_sample = Some(fetched);
                        guard.last_privileged_helper_fetch_millis = now;
                        guard.last_privileged_helper_success_millis = now;
                        guard.privileged_helper_last_error = None;
                        let item_count = guard
                            .cached_privileged_helper_sample
                            .as_ref()
                            .map(|sample| sample.processes.len())
                            .unwrap_or(0);
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Info,
                            DiagnosticsSubsystem::AdapterHelper,
                            "adapter-refresh-succeeded",
                            "Privileged helper refresh succeeded.",
                            now,
                            |builder| builder.adapter("helper").field("item_count", item_count),
                        );
                    }
                    Err(error) => {
                        let mut guard = self.state.lock();
                        guard.last_privileged_helper_fetch_millis = now;
                        guard.privileged_helper_last_error = Some(error);
                        let error = guard
                            .privileged_helper_last_error
                            .clone()
                            .unwrap_or_default();
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Error,
                            DiagnosticsSubsystem::AdapterHelper,
                            "adapter-refresh-failed",
                            "Privileged helper refresh failed.",
                            now,
                            |builder| builder.adapter("helper").field("error", error),
                        );
                    }
                }
            }
            if let Some(handle) = endpoint_security_handle {
                emit_adapter_refresh_event(
                    &self.state,
                    DiagnosticsLevel::Debug,
                    DiagnosticsSubsystem::AdapterHelper,
                    "adapter-refresh-started",
                    "Starting Endpoint Security helper refresh.",
                    now,
                    |builder| builder.adapter("endpoint-security"),
                );
                match handle.join().expect("endpoint security thread panicked") {
                    Ok(fetched) => {
                        let mut guard = self.state.lock();
                        guard.cached_endpoint_security_status = Some(fetched.status.clone());
                        guard.cached_endpoint_security_sample = Some(fetched.clone());
                        guard.last_endpoint_security_fetch_millis = now;
                        guard.last_endpoint_security_success_millis = now;
                        guard.endpoint_security_last_error = fetched.status.last_error.clone();
                        let item_count = fetched.events.len();
                        let live = fetched.status.can_stream_events;
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            if live {
                                DiagnosticsLevel::Info
                            } else {
                                DiagnosticsLevel::Warn
                            },
                            DiagnosticsSubsystem::AdapterHelper,
                            if live {
                                "adapter-refresh-succeeded"
                            } else {
                                "adapter-refresh-degraded"
                            },
                            if live {
                                "Endpoint Security helper refresh succeeded."
                            } else {
                                "Endpoint Security helper is configured but not yet able to stream events."
                            },
                            now,
                            |builder| {
                                builder
                                    .adapter("endpoint-security")
                                    .field("item_count", item_count)
                                    .field("live", live)
                            },
                        );
                    }
                    Err(error) => {
                        let mut guard = self.state.lock();
                        guard.last_endpoint_security_fetch_millis = now;
                        guard.endpoint_security_last_error = Some(error);
                        let error = guard
                            .endpoint_security_last_error
                            .clone()
                            .unwrap_or_default();
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Error,
                            DiagnosticsSubsystem::AdapterHelper,
                            "adapter-refresh-failed",
                            "Endpoint Security helper refresh failed.",
                            now,
                            |builder| builder.adapter("endpoint-security").field("error", error),
                        );
                    }
                }
            }
            if let Some(handle) = chau7_handle {
                emit_adapter_refresh_event(
                    &self.state,
                    DiagnosticsLevel::Debug,
                    DiagnosticsSubsystem::AdapterChau7,
                    "adapter-refresh-started",
                    "Starting Chau7 adapter refresh.",
                    now,
                    |builder| builder.adapter("chau7"),
                );
                match handle.join().expect("chau7 thread panicked") {
                    Ok((mut fetched, mode)) => {
                        let mut guard = self.state.lock();
                        if mode == "light"
                            && let Some(previous) = guard.cached_chau7_snapshot.as_ref()
                        {
                            fetched.inherit_deep_context_from(previous);
                        }
                        guard.cached_chau7_snapshot = Some(fetched);
                        guard.last_chau7_fetch_millis = now;
                        if mode == "deep" {
                            guard.last_chau7_deep_fetch_millis = now;
                        }
                        guard.last_chau7_success_millis = now;
                        guard.chau7_last_error = None;
                        let item_count = guard
                            .cached_chau7_snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.tabs.len())
                            .unwrap_or(0);
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Info,
                            DiagnosticsSubsystem::AdapterChau7,
                            "adapter-refresh-succeeded",
                            "Chau7 adapter refresh succeeded.",
                            now,
                            |builder| {
                                builder
                                    .adapter("chau7")
                                    .field("item_count", item_count)
                                    .field("mode", mode)
                            },
                        );
                    }
                    Err(error) => {
                        let mut guard = self.state.lock();
                        guard.last_chau7_fetch_millis = now;
                        guard.chau7_last_error = Some(error);
                        let error = guard.chau7_last_error.clone().unwrap_or_default();
                        drop(guard);
                        emit_adapter_refresh_event(
                            &self.state,
                            DiagnosticsLevel::Error,
                            DiagnosticsSubsystem::AdapterChau7,
                            "adapter-refresh-failed",
                            "Chau7 adapter refresh failed.",
                            now,
                            |builder| builder.adapter("chau7").field("error", error),
                        );
                    }
                }
            }
        });
    }

    #[allow(clippy::collapsible_if)]
    pub fn enrich_entities(
        &self,
        entities: &mut [EntitySnapshot],
        capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>,
    ) {
        let (
            chromium_targets,
            docker_containers,
            privileged_helper_sample,
            endpoint_security_sample,
            chau7_snapshot,
        ) = {
            let guard = self.state.lock();
            let chromium_targets = capabilities
                .get(&CapabilityKind::ChromiumDebug)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .map(|_| guard.cached_chromium_targets.clone());
            let docker_containers = capabilities
                .get(&CapabilityKind::DockerSocket)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .map(|_| guard.cached_docker_containers.clone());
            let privileged_helper_sample = capabilities
                .get(&CapabilityKind::PrivilegedHelper)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .and_then(|_| guard.cached_privileged_helper_sample.clone());
            let endpoint_security_sample = capabilities
                .get(&CapabilityKind::EndpointSecurity)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .and_then(|_| guard.cached_endpoint_security_sample.clone());
            let chau7_snapshot = capabilities
                .get(&CapabilityKind::Chau7)
                .filter(|capability| capability.state == CapabilityState::Granted)
                .and_then(|_| guard.cached_chau7_snapshot.clone());
            (
                chromium_targets,
                docker_containers,
                privileged_helper_sample,
                endpoint_security_sample,
                chau7_snapshot,
            )
        };

        for entity in entities {
            if matches!(
                entity.entity_kind,
                aetower_model::EntityKind::TerminalSession
            ) && !entity
                .badges
                .iter()
                .any(|badge| badge == "command-attributed")
            {
                entity.badges.push("command-attributed".to_owned());
            }

            if matches!(entity.entity_kind, aetower_model::EntityKind::Browser) {
                if let Some(targets) = chromium_targets.as_ref() {
                    let total_network_bps = targets.iter().fold(0u64, |total, target| {
                        total.saturating_add(target.network_bps)
                    });
                    entity.metrics.network_receive_bps = entity
                        .metrics
                        .network_receive_bps
                        .saturating_add(total_network_bps);
                    for target in targets.iter().take(5) {
                        entity.components.push(ComponentSnapshot {
                            kind: ComponentKind::AdapterContext,
                            title: format!("Tab {} · {}", target.id, target.title),
                            detail: chromium_target_detail(target),
                            adapter_context: Some(AdapterContextSnapshot {
                                kind: AdapterContextKind::ChromiumTab,
                                status: None,
                                url: Some(target.url.clone()),
                                workspace_path: None,
                                repo_root: None,
                                image_name: None,
                                session_id: None,
                                app_version: None,
                                build_sha: None,
                                build_timestamp: None,
                                build_channel: None,
                                network_receive_bps: target.network_bps,
                                network_send_bps: 0,
                                disk_read_bps: 0,
                                disk_write_bps: 0,
                                memory_limit_bytes: 0,
                                js_heap_total_bytes: target.js_heap_total_bytes,
                                dom_nodes: target.dom_nodes,
                                documents: target.documents,
                                frames: target.frames,
                                process_count: None,
                                connection_count: None,
                                ports: Vec::new(),
                            }),
                            provenance: Some(ProvenanceSnapshot {
                                kind: ProvenanceKind::BrowserContext,
                                label: "Browser tab context".to_owned(),
                                rule: "chromium remote debugging target".to_owned(),
                                confidence: AttributionConfidence::Medium,
                            }),
                            process_id: None,
                            start_time_millis: 0,
                            executable_path: None,
                            command_line: None,
                            parent_summary: None,
                            launched_by: None,
                            cpu_percent: target.cpu_percent,
                            memory_bytes: target.js_heap_used_bytes,
                            memory_physical_footprint_bytes: 0,
                            cwd: None,
                            user: None,
                        });
                    }
                    if !targets.is_empty()
                        && !entity.badges.iter().any(|badge| badge == "chromium-live")
                    {
                        entity.badges.push("chromium-live".to_owned());
                    }
                }
            }

            if entity.display_name.contains("Docker") || entity.display_name == "com.docker.backend"
            {
                if let Some(containers) = docker_containers.as_ref() {
                    let total_receive_bps = containers.iter().fold(0u64, |total, container| {
                        total.saturating_add(container.network_rx_bytes)
                    });
                    let total_send_bps = containers.iter().fold(0u64, |total, container| {
                        total.saturating_add(container.network_tx_bytes)
                    });
                    entity.metrics.network_receive_bps = entity
                        .metrics
                        .network_receive_bps
                        .saturating_add(total_receive_bps);
                    entity.metrics.network_send_bps = entity
                        .metrics
                        .network_send_bps
                        .saturating_add(total_send_bps);
                    for container in containers.iter().take(5) {
                        entity.components.push(ComponentSnapshot {
                            kind: ComponentKind::AdapterContext,
                            title: container.name.clone(),
                            detail: docker_container_detail(container),
                            adapter_context: Some(AdapterContextSnapshot {
                                kind: AdapterContextKind::DockerContainer,
                                status: Some(container.status.clone()),
                                url: None,
                                workspace_path: None,
                                repo_root: None,
                                image_name: Some(container.image.clone()),
                                session_id: None,
                                app_version: None,
                                build_sha: None,
                                build_timestamp: None,
                                build_channel: None,
                                network_receive_bps: container.network_rx_bytes,
                                network_send_bps: container.network_tx_bytes,
                                disk_read_bps: container.block_read_bytes,
                                disk_write_bps: container.block_write_bytes,
                                memory_limit_bytes: container.memory_limit_bytes,
                                js_heap_total_bytes: 0,
                                dom_nodes: 0,
                                documents: 0,
                                frames: 0,
                                process_count: Some(container.pids.min(u64::from(u32::MAX)) as u32),
                                connection_count: None,
                                ports: container.ports.clone(),
                            }),
                            provenance: Some(ProvenanceSnapshot {
                                kind: ProvenanceKind::ContainerWorkload,
                                label: "Container workload".to_owned(),
                                rule: "docker stats container mapping".to_owned(),
                                confidence: AttributionConfidence::Medium,
                            }),
                            process_id: None,
                            start_time_millis: 0,
                            executable_path: None,
                            command_line: None,
                            parent_summary: None,
                            launched_by: None,
                            cpu_percent: container.cpu_percent,
                            memory_bytes: container.memory_usage_bytes,
                            memory_physical_footprint_bytes: 0,
                            cwd: None,
                            user: None,
                        });
                    }
                    if !containers.is_empty()
                        && !entity.badges.iter().any(|badge| badge == "docker-live")
                    {
                        entity.badges.push("docker-live".to_owned());
                    }
                }
            }

            enrich_vscode_entity(entity);

            if let Some(helper) = privileged_helper_sample.as_ref() {
                if let Some(process) = helper
                    .processes
                    .iter()
                    .find(|process| helper_process_matches(entity, process))
                {
                    entity.components.push(ComponentSnapshot {
                        kind: ComponentKind::AdapterContext,
                        title: format!("Privileged sockets · {}", process.process_name),
                        adapter_context: Some(AdapterContextSnapshot {
                            kind: AdapterContextKind::PrivilegedSocket,
                            status: None,
                            url: None,
                            workspace_path: None,
                            repo_root: None,
                            image_name: None,
                            session_id: None,
                            app_version: None,
                            build_sha: None,
                            build_timestamp: None,
                            build_channel: None,
                            network_receive_bps: 0,
                            network_send_bps: 0,
                            disk_read_bps: 0,
                            disk_write_bps: 0,
                            memory_limit_bytes: 0,
                            js_heap_total_bytes: 0,
                            dom_nodes: 0,
                            documents: 0,
                            frames: 0,
                            process_count: None,
                            connection_count: Some(process.connections.len() as u32),
                            ports: process.connections.clone(),
                        }),
                        provenance: None,
                        detail: format!(
                            "pid {} · {}",
                            process.pid,
                            process.connections.join(" · ")
                        ),
                        process_id: Some(process.pid),
                        start_time_millis: 0,
                        executable_path: None,
                        command_line: None,
                        parent_summary: None,
                        launched_by: None,
                        cpu_percent: 0.0,
                        memory_bytes: 0,
                        memory_physical_footprint_bytes: 0,
                        cwd: None,
                        user: None,
                    });
                    if !entity
                        .badges
                        .iter()
                        .any(|badge| badge == "privileged-helper")
                    {
                        entity.badges.push("privileged-helper".to_owned());
                    }
                }
            }

            // Chau7 enrichment: match tabs to entities, promote AI agents.
            //
            // Matching strategy (in priority order):
            // 1. Entity already classified as AiAgent by identity resolver
            //    → match any tab whose active_app contains the entity name
            // 2. Entity executable path lives under a tab's repo_root/cwd
            // 3. Entity display_name matches a tab's active_app (case-insensitive)
            if let Some(snapshot) = chau7_snapshot.as_ref() {
                let entity_name_lower = entity.display_name.to_lowercase();
                let is_already_ai_agent =
                    matches!(entity.entity_kind, aetower_model::EntityKind::AiAgent);

                // Collect all executable paths from components for cwd matching.
                let entity_exe_paths: Vec<&str> = entity
                    .components
                    .iter()
                    .filter_map(|c| c.executable_path.as_deref())
                    .chain(entity.executable_path.as_deref())
                    .collect();

                for tab in snapshot.tabs.iter().take(MAX_CHAU7_TABS) {
                    // Strategy 1: already an AI agent → match by provider/app name.
                    let matches_by_agent_name = is_already_ai_agent
                        && tab.is_ai_agent()
                        && tab
                            .active_app
                            .as_deref()
                            .map(|app| entity_name_lower.contains(&app.to_lowercase()))
                            .unwrap_or(false);

                    // Strategy 2: any component exe lives under the tab's repo root.
                    // Only use repo_root (not raw cwd) to avoid false positives from
                    // short paths like /Users/dev.  Use Path::starts_with for proper
                    // component-boundary matching.
                    let matches_by_path = tab
                        .repo_root
                        .as_deref()
                        .filter(|p| !p.is_empty())
                        .map(|root| {
                            let root_path = Path::new(root);
                            entity_exe_paths
                                .iter()
                                .any(|p| Path::new(p).starts_with(root_path))
                        })
                        .unwrap_or(false);

                    // Strategy 3: display_name matches active_app.
                    let matches_by_display_name = tab
                        .active_app
                        .as_deref()
                        .map(|app| entity_name_lower == app.to_lowercase())
                        .unwrap_or(false);

                    let tab_matches =
                        matches_by_agent_name || matches_by_path || matches_by_display_name;

                    if !tab_matches {
                        continue;
                    }

                    if tab.is_ai_agent() {
                        entity.entity_kind = aetower_model::EntityKind::AiAgent;
                    }

                    let provider = tab.provider_label().unwrap_or("shell");
                    let tab_status = snapshot.tab_statuses.get(&tab.tab_id);
                    let session_state = tab
                        .ai_session_id
                        .as_deref()
                        .and_then(|session_id| snapshot.runtime_sessions.get(session_id));
                    let repo_events = tab
                        .repo_root
                        .as_deref()
                        .and_then(|repo_root| snapshot.repo_events.get(repo_root));
                    let session_status =
                        chau7_session_status(tab, tab_status, session_state, repo_events);
                    let session_detail =
                        chau7_tab_detail(tab, tab_status, session_state, snapshot, repo_events);
                    entity.components.push(ComponentSnapshot {
                        kind: ComponentKind::AdapterContext,
                        title: format!("{} · {}", provider, tab.title),
                        detail: session_detail,
                        adapter_context: Some(AdapterContextSnapshot {
                            kind: AdapterContextKind::Chau7Session,
                            status: Some(session_status),
                            url: None,
                            workspace_path: (!tab.cwd.is_empty()).then(|| tab.cwd.clone()),
                            repo_root: tab.repo_root.clone(),
                            image_name: None,
                            session_id: tab.ai_session_id.clone(),
                            app_version: snapshot
                                .runtime_info
                                .as_ref()
                                .and_then(|info| info.app_version.clone()),
                            build_sha: snapshot
                                .runtime_info
                                .as_ref()
                                .and_then(|info| info.build_sha.clone()),
                            build_timestamp: snapshot
                                .runtime_info
                                .as_ref()
                                .and_then(|info| info.build_timestamp.clone()),
                            build_channel: snapshot
                                .runtime_info
                                .as_ref()
                                .and_then(|info| info.build_channel.clone()),
                            network_receive_bps: 0,
                            network_send_bps: 0,
                            disk_read_bps: 0,
                            disk_write_bps: 0,
                            memory_limit_bytes: 0,
                            js_heap_total_bytes: 0,
                            dom_nodes: 0,
                            documents: 0,
                            frames: 0,
                            process_count: None,
                            connection_count: None,
                            ports: Vec::new(),
                        }),
                        provenance: None,
                        process_id: None,
                        start_time_millis: 0,
                        executable_path: None,
                        command_line: None,
                        parent_summary: None,
                        launched_by: None,
                        cpu_percent: 0.0,
                        memory_bytes: 0,
                        memory_physical_footprint_bytes: 0,
                        cwd: None,
                        user: None,
                    });

                    if tab.is_ai_agent() {
                        if let Some(label) = tab.provider_label() {
                            push_unique_badge(entity, label);
                        }
                        push_unique_badge(entity, "ai-agent");
                    }

                    push_unique_badge(entity, "chau7-live");
                    if let Some(session_id) = tab.ai_session_id.as_deref() {
                        push_unique_badge(entity, &format!("ai-session:{session_id}"));
                    }

                    // Feature 9: Populate agent cost from repo stats.
                    if let Some(repo) = tab.repo_root.as_deref() {
                        if let Some(stats) = snapshot.repo_stats.get(repo) {
                            entity.agent_cost = Some(aetower_model::AgentCostSummary {
                                total_input_tokens: stats.total_tokens,
                                total_output_tokens: 0,
                                cost_usd: stats.total_cost,
                                total_runs: stats.total_runs,
                                session_energy_nj: 0, // populated by history tracker
                            });
                            if stats.total_turns > 0
                                && !entity
                                    .attribution_notes
                                    .iter()
                                    .any(|note| note.contains("Chau7 repo activity"))
                            {
                                let providers = if stats.providers.is_empty() {
                                    "unknown providers".to_owned()
                                } else {
                                    stats.providers.join(", ")
                                };
                                entity.attribution_notes.push(format!(
                                    "Chau7 repo activity: {} total turn(s) across {}.",
                                    stats.total_turns, providers
                                ));
                            }
                        }
                    }

                    // Feature 12: Populate session markers from recent runs.
                    if let Some(sid) = tab.ai_session_id.as_deref() {
                        for run in &snapshot.recent_runs {
                            if run.session_id.as_deref() != Some(sid) {
                                continue;
                            }
                            if let Ok(ts) = parse_iso_millis(&run.started_at) {
                                entity.session_markers.push(aetower_model::SessionMarker {
                                    timestamp_millis: ts,
                                    kind: aetower_model::SessionMarkerKind::RunStart,
                                    label: format!("{} run", run.provider),
                                });
                            }
                            if let Some(ended) = run.ended_at.as_deref() {
                                if let Ok(ts) = parse_iso_millis(ended) {
                                    entity.session_markers.push(aetower_model::SessionMarker {
                                        timestamp_millis: ts,
                                        kind: aetower_model::SessionMarkerKind::RunEnd,
                                        label: format!("{} done", run.provider),
                                    });
                                }
                            }
                        }
                    }

                    apply_chau7_runtime_state(entity, tab, tab_status, session_state, repo_events);

                    break; // one tab match per entity
                }
            }

            if let Some(sample) = endpoint_security_sample.as_ref() {
                enrich_with_endpoint_security(entity, sample);
            }
        }
    }
}

fn parse_iso_millis(iso: &str) -> Result<u64, String> {
    // Parse "2026-04-02T15:59:57.303Z" → millis since epoch.
    // Minimal parser: split on 'T', parse date+time.
    let iso = iso.trim_end_matches('Z');
    let (date_part, time_part) = iso.split_once('T').ok_or("no T separator")?;
    let date_segs: Vec<&str> = date_part.split('-').collect();
    let time_segs: Vec<&str> = time_part.split(':').collect();
    if date_segs.len() != 3 || time_segs.len() != 3 {
        return Err("bad format".to_owned());
    }
    let year: i64 = date_segs[0].parse().map_err(|_| "year")?;
    let month: i64 = date_segs[1].parse().map_err(|_| "month")?;
    let day: i64 = date_segs[2].parse().map_err(|_| "day")?;
    let hour: i64 = time_segs[0].parse().map_err(|_| "hour")?;
    let min: i64 = time_segs[1].parse().map_err(|_| "min")?;
    let sec_frac: f64 = time_segs[2].parse().map_err(|_| "sec")?;

    // Days from epoch (simplified, no leap second handling).
    let days = (year - 1970) * 365 + (year - 1969) / 4 - (year - 1901) / 100
        + (year - 1601) / 400
        + [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][(month - 1) as usize]
        + day
        - 1
        + if month > 2 && (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)) {
            1
        } else {
            0
        };
    let secs = days * 86400 + hour * 3600 + min * 60 + sec_frac as i64;
    Ok((secs as f64 * 1000.0 + (sec_frac.fract() * 1000.0)) as u64)
}

#[derive(Debug, Clone)]
struct ChromiumPageTarget {
    id: String,
    title: String,
    url: String,
    debug_socket: Option<String>,
    js_heap_used_bytes: u64,
    js_heap_total_bytes: u64,
    dom_nodes: u64,
    documents: u64,
    frames: u64,
    cpu_percent: f32,
    network_bps: u64,
}

#[derive(Debug, Clone)]
struct DockerContainer {
    id: String,
    name: String,
    image: String,
    status: String,
    ports: Vec<String>,
    cpu_percent: f32,
    memory_usage_bytes: u64,
    memory_limit_bytes: u64,
    network_rx_bytes: u64,
    network_tx_bytes: u64,
    block_read_bytes: u64,
    block_write_bytes: u64,
    pids: u64,
}

#[derive(Debug, Default)]
struct VsCodeHeuristicSummary {
    workspace: Option<String>,
    extension_hosts: usize,
    watchers: usize,
    pty_hosts: usize,
    shared_processes: usize,
}

#[derive(Debug, Default)]
struct ChromiumMetrics {
    js_heap_used_bytes: u64,
    js_heap_total_bytes: u64,
    dom_nodes: u64,
    documents: u64,
    frames: u64,
    task_duration_seconds: f64,
    network_bytes: u64,
}

impl AdapterState {
    fn chromium_config(&self) -> Option<ChromiumAdapterConfig> {
        self.chromium_endpoint
            .as_deref()
            .and_then(parse_http_endpoint)
            .map(|(host, port, path)| ChromiumAdapterConfig { host, port, path })
    }

    fn privileged_helper_path(&self) -> Option<String> {
        if self.privileged_helper_enabled {
            self.privileged_helper_path.clone()
        } else {
            None
        }
    }
}

fn emit_adapter_refresh_event<F>(
    state: &Arc<Mutex<AdapterState>>,
    level: DiagnosticsLevel,
    subsystem: DiagnosticsSubsystem,
    event_type: &str,
    message: &str,
    timestamp_millis: u64,
    decorate: F,
) where
    F: FnOnce(
        aetower_diagnostics::DiagnosticsEventBuilder,
    ) -> aetower_diagnostics::DiagnosticsEventBuilder,
{
    let diagnostics = state.lock().diagnostics.clone();
    let Some(diagnostics) = diagnostics else {
        return;
    };
    let builder = DiagnosticsEvent::builder(level, subsystem, event_type, message)
        .timestamp_millis(timestamp_millis);
    diagnostics.emit(decorate(builder).build());
}

fn capability_status(state: &AdapterState, kind: &CapabilityKind) -> (CapabilityState, String) {
    let now = time::now_millis();
    match kind {
        CapabilityKind::ChromiumDebug => match state.chromium_endpoint.as_deref() {
            Some(raw) if parse_http_endpoint(raw).is_some() => (
                CapabilityState::Granted,
                format!(
                    "Chromium target discovery configured at {raw}. {}",
                    chromium_runtime_detail(state, now)
                ),
            ),
            Some(raw) => (
                CapabilityState::Denied,
                format!("Chromium endpoint is invalid: {raw}."),
            ),
            None => (
                CapabilityState::Unavailable,
                "Set a local Chromium /json or /json/list endpoint in settings.".to_owned(),
            ),
        },
        CapabilityKind::DockerSocket => {
            if Path::new(&state.docker_socket_path).exists() {
                (
                    CapabilityState::Granted,
                    format!(
                        "Docker socket detected at {}. {}",
                        state.docker_socket_path,
                        docker_runtime_detail(state, now)
                    ),
                )
            } else {
                (
                    CapabilityState::Unavailable,
                    format!("Docker socket not detected at {}.", state.docker_socket_path),
                )
            }
        }
        CapabilityKind::PrivilegedHelper => match (state.privileged_helper_enabled, state.privileged_helper_path.as_deref()) {
            (true, Some(path)) if Path::new(path).exists() => (
                CapabilityState::Granted,
                format!(
                    "Privileged helper configured at {path}. {}",
                    privileged_helper_runtime_detail(state, now)
                ),
            ),
            (true, Some(path)) => (
                CapabilityState::Unavailable,
                format!("Privileged helper is enabled but missing at {path}."),
            ),
            (true, None) => (
                CapabilityState::Unavailable,
                "Privileged helper is enabled but no helper path is configured.".to_owned(),
            ),
            _ => (
                CapabilityState::Unavailable,
                "Privileged helper is disabled. Enable it and provide a helper path for deeper attribution.".to_owned(),
            ),
        },
        CapabilityKind::EndpointSecurity => {
            let Some(path) = state.privileged_helper_path.as_deref() else {
                return (
                    CapabilityState::Unavailable,
                    "Endpoint Security requires the optional enterprise helper path to be configured.".to_owned(),
                );
            };
            if !state.privileged_helper_enabled {
                return (
                    CapabilityState::Unavailable,
                    "Endpoint Security is disabled because the enterprise helper is disabled.".to_owned(),
                );
            }
            if !Path::new(path).exists() {
                return (
                    CapabilityState::Unavailable,
                    format!("Endpoint Security helper path is missing at {path}."),
                );
            }
            if let Some(status) = state.cached_endpoint_security_status.as_ref() {
                let capability_state = if status.can_stream_events {
                    CapabilityState::Granted
                } else if status.supported {
                    CapabilityState::Denied
                } else {
                    CapabilityState::Unavailable
                };
                let detail = endpoint_security_runtime_detail(state, status, now);
                (capability_state, detail)
            } else {
                (
                    CapabilityState::Requested,
                    "Endpoint Security helper is configured. Aetower is waiting for the first readiness probe."
                        .to_owned(),
                )
            }
        }
        CapabilityKind::Chau7 => match resolved_chau7_socket_path(state) {
            Some(path) if Path::new(&path).exists() => (
                CapabilityState::Granted,
                format!(
                    "Chau7 MCP socket detected at {path}. {}",
                    chau7_runtime_detail(state, now)
                ),
            ),
            Some(path) => (
                CapabilityState::Unavailable,
                format!("Chau7 socket not found at {path}. Is Chau7 running?"),
            ),
            None => (
                CapabilityState::Unavailable,
                "No Chau7 socket configured. Set a path or ensure ~/.chau7/mcp.sock exists."
                    .to_owned(),
            ),
        },
        _ => (CapabilityState::Unknown, String::new()),
    }
}

fn capability_status_timestamp(state: &AdapterState, kind: &CapabilityKind) -> Option<u64> {
    match kind {
        CapabilityKind::ChromiumDebug => Some(
            state
                .last_chromium_fetch_millis
                .max(state.last_chromium_success_millis),
        )
        .filter(|value| *value > 0),
        CapabilityKind::DockerSocket => Some(
            state
                .last_docker_fetch_millis
                .max(state.last_docker_success_millis),
        )
        .filter(|value| *value > 0),
        CapabilityKind::PrivilegedHelper => Some(
            state
                .last_privileged_helper_fetch_millis
                .max(state.last_privileged_helper_success_millis),
        )
        .filter(|value| *value > 0),
        CapabilityKind::EndpointSecurity => Some(
            state
                .last_endpoint_security_fetch_millis
                .max(state.last_endpoint_security_success_millis),
        )
        .filter(|value| *value > 0),
        CapabilityKind::Chau7 => Some(
            state
                .last_chau7_fetch_millis
                .max(state.last_chau7_success_millis),
        )
        .filter(|value| *value > 0),
        _ => None,
    }
}

fn capability_health(state: &AdapterState, kind: &CapabilityKind, now: u64) -> CapabilityHealth {
    match kind {
        CapabilityKind::ChromiumDebug => adapter_health_kind(
            state.last_chromium_success_millis,
            state.last_chromium_fetch_millis,
            state.chromium_last_error.as_deref(),
            CHROMIUM_REFRESH_INTERVAL_MILLIS,
            now,
        ),
        CapabilityKind::DockerSocket => {
            if !Path::new(&state.docker_socket_path).exists() {
                CapabilityHealth::Configured
            } else {
                adapter_health_kind(
                    state.last_docker_success_millis,
                    state.last_docker_fetch_millis,
                    state.docker_last_error.as_deref(),
                    DOCKER_REFRESH_INTERVAL_MILLIS,
                    now,
                )
            }
        }
        CapabilityKind::PrivilegedHelper => {
            if !state.privileged_helper_enabled {
                CapabilityHealth::Configured
            } else if state
                .privileged_helper_path
                .as_deref()
                .is_none_or(|path| !Path::new(path).exists())
            {
                CapabilityHealth::Degraded
            } else {
                adapter_health_kind(
                    state.last_privileged_helper_success_millis,
                    state.last_privileged_helper_fetch_millis,
                    state.privileged_helper_last_error.as_deref(),
                    PRIVILEGED_HELPER_REFRESH_INTERVAL_MILLIS,
                    now,
                )
            }
        }
        CapabilityKind::EndpointSecurity => {
            if !state.privileged_helper_enabled {
                CapabilityHealth::Configured
            } else if state
                .privileged_helper_path
                .as_deref()
                .is_none_or(|path| !Path::new(path).exists())
            {
                CapabilityHealth::Degraded
            } else {
                adapter_health_kind(
                    state.last_endpoint_security_success_millis,
                    state.last_endpoint_security_fetch_millis,
                    state.endpoint_security_last_error.as_deref(),
                    ENDPOINT_SECURITY_REFRESH_INTERVAL_MILLIS,
                    now,
                )
            }
        }
        CapabilityKind::Chau7 => {
            if resolved_chau7_socket_path(state)
                .as_deref()
                .is_none_or(|path| !Path::new(path).exists())
            {
                CapabilityHealth::Configured
            } else {
                adapter_health_kind(
                    state.last_chau7_success_millis,
                    state.last_chau7_fetch_millis,
                    state.chau7_last_error.as_deref(),
                    CHAU7_REFRESH_INTERVAL_MILLIS,
                    now,
                )
            }
        }
        _ => CapabilityHealth::Configured,
    }
}

fn chromium_runtime_detail(state: &AdapterState, now: u64) -> String {
    adapter_runtime_detail(
        "tabs",
        state.cached_chromium_targets.len(),
        state.last_chromium_success_millis,
        state.last_chromium_fetch_millis,
        state.chromium_last_error.as_deref(),
        CHROMIUM_REFRESH_INTERVAL_MILLIS,
        now,
    )
}

fn docker_runtime_detail(state: &AdapterState, now: u64) -> String {
    adapter_runtime_detail(
        "containers",
        state.cached_docker_containers.len(),
        state.last_docker_success_millis,
        state.last_docker_fetch_millis,
        state.docker_last_error.as_deref(),
        DOCKER_REFRESH_INTERVAL_MILLIS,
        now,
    )
}

fn privileged_helper_runtime_detail(state: &AdapterState, now: u64) -> String {
    let process_count = state
        .cached_privileged_helper_sample
        .as_ref()
        .map(|sample| sample.processes.len())
        .unwrap_or(0);
    adapter_runtime_detail(
        "processes",
        process_count,
        state.last_privileged_helper_success_millis,
        state.last_privileged_helper_fetch_millis,
        state.privileged_helper_last_error.as_deref(),
        PRIVILEGED_HELPER_REFRESH_INTERVAL_MILLIS,
        now,
    )
}

fn endpoint_security_runtime_detail(
    state: &AdapterState,
    status: &EndpointSecurityStatusSnapshot,
    now: u64,
) -> String {
    let event_count = state
        .cached_endpoint_security_sample
        .as_ref()
        .map(|sample| sample.events.len())
        .unwrap_or(0);
    let mut parts = vec![adapter_health_label(
        state.last_endpoint_security_success_millis,
        state.last_endpoint_security_fetch_millis,
        state.endpoint_security_last_error.as_deref(),
        ENDPOINT_SECURITY_REFRESH_INTERVAL_MILLIS,
        now,
    )];
    parts.push(if status.supported {
        "tooling present".to_owned()
    } else {
        "tooling unavailable".to_owned()
    });
    if status.helper_entitled {
        parts.push("helper entitled".to_owned());
    } else {
        parts.push("helper not entitled".to_owned());
    }
    if status.running_as_root {
        parts.push("running as root".to_owned());
    }
    if let Some(path) = status.eslogger_path.as_deref() {
        parts.push(format!(
            "source {}",
            path.rsplit('/').next().unwrap_or(path)
        ));
    }
    if event_count > 0 {
        parts.push(format!("{event_count} sampled lifecycle events"));
    }
    if state.last_endpoint_security_success_millis > 0 {
        parts.push(format!(
            "last probe {}s ago",
            now.saturating_sub(state.last_endpoint_security_success_millis) / 1000
        ));
    }
    parts.push(status.detail.clone());
    if let Some(error) = status.last_error.as_deref() {
        parts.push(format!("last error: {error}"));
    }
    parts.join(" · ")
}

fn chau7_runtime_detail(state: &AdapterState, now: u64) -> String {
    let tab_count = state
        .cached_chau7_snapshot
        .as_ref()
        .map(|s| s.tabs.len())
        .unwrap_or(0);
    adapter_runtime_detail(
        "tabs",
        tab_count,
        state.last_chau7_success_millis,
        state.last_chau7_fetch_millis,
        state.chau7_last_error.as_deref(),
        CHAU7_REFRESH_INTERVAL_MILLIS,
        now,
    )
}

fn adapter_runtime_detail(
    unit_label: &str,
    cached_count: usize,
    last_success_millis: u64,
    last_attempt_millis: u64,
    last_error: Option<&str>,
    refresh_interval_millis: u64,
    now: u64,
) -> String {
    let mut parts = Vec::new();
    parts.push(adapter_health_label(
        last_success_millis,
        last_attempt_millis,
        last_error,
        refresh_interval_millis,
        now,
    ));
    if cached_count > 0 {
        parts.push(format!("{cached_count} cached {unit_label}"));
    }
    if last_success_millis > 0 {
        parts.push(format!(
            "last success {}s ago",
            now.saturating_sub(last_success_millis) / 1000
        ));
    } else {
        parts.push("waiting for first successful refresh".to_owned());
    }
    if let Some(error) = last_error {
        parts.push(format!("last error: {error}"));
    }
    parts.join(" · ")
}

fn adapter_health_label(
    last_success_millis: u64,
    last_attempt_millis: u64,
    last_error: Option<&str>,
    refresh_interval_millis: u64,
    now: u64,
) -> String {
    match adapter_health_kind(
        last_success_millis,
        last_attempt_millis,
        last_error,
        refresh_interval_millis,
        now,
    ) {
        CapabilityHealth::Configured => "status configured".to_owned(),
        CapabilityHealth::Live => "status live".to_owned(),
        CapabilityHealth::Cached => "status cached".to_owned(),
        CapabilityHealth::Degraded => "status degraded".to_owned(),
    }
}

fn adapter_health_kind(
    last_success_millis: u64,
    last_attempt_millis: u64,
    last_error: Option<&str>,
    refresh_interval_millis: u64,
    now: u64,
) -> CapabilityHealth {
    if last_success_millis == 0 {
        return if last_error.is_some() {
            CapabilityHealth::Degraded
        } else {
            CapabilityHealth::Configured
        };
    }

    let age = now.saturating_sub(last_success_millis);
    if last_error.is_some() {
        CapabilityHealth::Degraded
    } else if age > refresh_interval_millis.saturating_mul(2) {
        CapabilityHealth::Cached
    } else if last_attempt_millis > 0 {
        CapabilityHealth::Live
    } else {
        CapabilityHealth::Configured
    }
}

fn docker_socket_path() -> String {
    env::var("AETOWER_DOCKER_SOCKET").unwrap_or_else(|_| "/var/run/docker.sock".to_owned())
}

fn sanitize_chau7_socket_path(socket_path: Option<String>) -> Option<String> {
    socket_path
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn resolved_chau7_socket_path(state: &AdapterState) -> Option<String> {
    sanitize_chau7_socket_path(state.chau7_socket_path.clone()).or_else(chau7_socket_path)
}

fn chau7_socket_path() -> Option<String> {
    if let Ok(value) = env::var("AETOWER_CHAU7_SOCKET")
        && !value.trim().is_empty()
    {
        return Some(value);
    }
    dirs::home_dir()
        .map(|home| home.join(".chau7").join("mcp.sock"))
        .and_then(|path| {
            if path.exists() {
                path.to_str().map(|s| s.to_owned())
            } else {
                None
            }
        })
}

fn linked_chau7_entity_ids(
    entity_ids_by_session: &BTreeMap<String, BTreeSet<String>>,
    entity_ids_by_repo: &BTreeMap<String, BTreeSet<String>>,
    session_id: Option<&str>,
    repo_root: Option<&str>,
    workspace_path: Option<&str>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    if let Some(session_id) = session_id
        && let Some(values) = entity_ids_by_session.get(session_id)
    {
        ids.extend(values.iter().cloned());
    }
    for repo_key in [repo_root, workspace_path].into_iter().flatten() {
        if let Some(values) = entity_ids_by_repo.get(repo_key) {
            ids.extend(values.iter().cloned());
        }
    }
    ids.into_iter().collect()
}

fn chau7_catalog_title(
    status_title: Option<&str>,
    tab_title: Option<&str>,
    active_app: Option<&str>,
    session_id: Option<&str>,
) -> String {
    status_title
        .filter(|value| !value.is_empty())
        .or(tab_title.filter(|value| !value.is_empty()))
        .or(active_app.filter(|value| !value.is_empty()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            session_id
                .map(|value| format!("Session {}", value.chars().take(8).collect::<String>()))
                .unwrap_or_else(|| "Terminal tab".to_owned())
        })
}

fn chau7_catalog_status(
    tab: &crate::chau7::Chau7Tab,
    tab_status: Option<&crate::chau7::Chau7TabStatus>,
    session_state: Option<&crate::chau7::Chau7RuntimeSessionStatus>,
) -> String {
    if let Some(runtime) = session_state {
        if runtime.pending_approval.is_some() {
            return "approval-needed".to_owned();
        }
        if matches!(
            runtime.last_exit_reason.as_deref(),
            Some("error" | "failed")
        ) {
            return "error".to_owned();
        }
        if !runtime.state.is_empty() {
            return runtime.state.clone();
        }
    }
    if let Some(status) = tab_status {
        if status.shell_loading {
            return "shell-loading".to_owned();
        }
        if status.is_at_prompt {
            return "at-prompt".to_owned();
        }
        if !status.status.is_empty() {
            return status.status.clone();
        }
        if let Some(raw_status) = status.raw_status.as_deref()
            && !raw_status.is_empty()
        {
            return raw_status.to_owned();
        }
    }
    if !tab.status.is_empty() {
        return tab.status.clone();
    }
    "unknown".to_owned()
}

fn chau7_tab_detail(
    tab: &crate::chau7::Chau7Tab,
    tab_status: Option<&crate::chau7::Chau7TabStatus>,
    session_state: Option<&crate::chau7::Chau7RuntimeSessionStatus>,
    snapshot: &crate::chau7::Chau7Snapshot,
    repo_events: Option<&Vec<crate::chau7::Chau7RepoEvent>>,
) -> String {
    let mut parts = Vec::new();
    parts.push(format!(
        "status {}",
        chau7_session_status(tab, tab_status, session_state, repo_events)
    ));
    if let Some(repo) = tab.repo_root.as_deref() {
        let short = repo.rsplit('/').next().unwrap_or(repo);
        parts.push(short.to_owned());
    }
    if let Some(branch) = tab.git_branch.as_deref() {
        parts.push(branch.to_owned());
    }
    if let Some(build) = chau7_build_label(snapshot.runtime_info.as_ref()) {
        parts.push(build);
    }
    if let Some(status) = tab_status {
        if status.is_at_prompt {
            parts.push("at prompt".to_owned());
        }
        if status.shell_loading {
            parts.push("shell loading".to_owned());
        }
        if status.cto_active {
            parts.push("cto active".to_owned());
        }
    }
    // Attach run count from matching session.
    if let Some(sid) = tab.ai_session_id.as_deref()
        && let Some(session) = snapshot.sessions.iter().find(|s| s.session_id == sid)
    {
        parts.push(format!("{} runs", session.run_count));
    }
    if let Some(runtime) = session_state {
        if runtime.turn_count > 0 {
            parts.push(format!("{} turns", runtime.turn_count));
        }
        if let Some(active_run) = runtime.active_run.as_ref()
            && active_run.duration_so_far_ms > 0
        {
            parts.push(format!(
                "active {}",
                human_duration_millis(active_run.duration_so_far_ms)
            ));
        }
        if runtime.child_session_count > 0 {
            parts.push(format!("{} child sessions", runtime.child_session_count));
        }
        if let Some(last_exit_reason) = runtime.last_exit_reason.as_deref()
            && !last_exit_reason.is_empty()
        {
            parts.push(format!("last exit {last_exit_reason}"));
        }
        if let Some(approval) = runtime.pending_approval.as_ref()
            && !approval.description.is_empty()
        {
            parts.push(approval.description.clone());
        }
    }
    if let Some(event) = repo_events.and_then(|events| events.first()) {
        let label = chau7_event_label(event);
        if !label.is_empty() {
            parts.push(label);
        }
    }
    parts.join(" · ")
}

fn chau7_build_label(runtime_info: Option<&crate::chau7::Chau7RuntimeInfo>) -> Option<String> {
    let runtime_info = runtime_info?;
    let mut parts = Vec::new();
    if let Some(version) = runtime_info.app_version.as_deref()
        && !version.is_empty()
    {
        parts.push(format!("v{version}"));
    }
    if let Some(channel) = runtime_info.build_channel.as_deref()
        && !channel.is_empty()
    {
        parts.push(channel.to_owned());
    }
    if let Some(sha) = runtime_info.build_sha.as_deref()
        && !sha.is_empty()
    {
        parts.push(format!("#{}", sha.chars().take(7).collect::<String>()));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn chau7_session_status(
    tab: &crate::chau7::Chau7Tab,
    tab_status: Option<&crate::chau7::Chau7TabStatus>,
    session_state: Option<&crate::chau7::Chau7RuntimeSessionStatus>,
    repo_events: Option<&Vec<crate::chau7::Chau7RepoEvent>>,
) -> String {
    if let Some(runtime) = session_state {
        if runtime.pending_approval.is_some() {
            return "approval-needed".to_owned();
        }
        if matches!(
            runtime.last_exit_reason.as_deref(),
            Some("error" | "failed")
        ) {
            return "error".to_owned();
        }
    }
    if let Some(event) = repo_events.and_then(|events| events.first()) {
        match event.event_type.as_str() {
            "waiting_input" => return "waiting-input".to_owned(),
            "permission" => return "approval-needed".to_owned(),
            "idle" => return "idle".to_owned(),
            "finished" => return "finished".to_owned(),
            "process_ended" => return "process-ended".to_owned(),
            _ => {}
        }
    }
    if let Some(status) = tab_status {
        if status.shell_loading {
            return "shell-loading".to_owned();
        }
        if status.is_at_prompt {
            return "at-prompt".to_owned();
        }
        if !status.status.is_empty() {
            return status.status.clone();
        }
        if let Some(raw_status) = status.raw_status.as_deref()
            && !raw_status.is_empty()
        {
            return raw_status.to_owned();
        }
    }
    if let Some(runtime) = session_state
        && !runtime.state.is_empty()
    {
        return runtime.state.clone();
    }
    if !tab.status.is_empty() {
        return tab.status.clone();
    }
    "unknown".to_owned()
}

fn apply_chau7_runtime_state(
    entity: &mut EntitySnapshot,
    tab: &crate::chau7::Chau7Tab,
    tab_status: Option<&crate::chau7::Chau7TabStatus>,
    session_state: Option<&crate::chau7::Chau7RuntimeSessionStatus>,
    repo_events: Option<&Vec<crate::chau7::Chau7RepoEvent>>,
) {
    let Some(session_id) = tab.ai_session_id.as_deref() else {
        return;
    };

    if let Some(runtime) = session_state {
        if runtime.pending_approval.is_some() {
            push_unique_badge(entity, "approval-needed");
            if !entity
                .recommendations
                .iter()
                .any(|recommendation| recommendation.title == "Resolve pending agent approval")
            {
                entity.recommendations.push(aetower_model::Recommendation {
                    title: "Resolve pending agent approval".to_owned(),
                    detail: runtime
                        .pending_approval
                        .as_ref()
                        .map(|approval| approval.description.clone())
                        .filter(|description| !description.is_empty())
                        .unwrap_or_else(|| {
                            "This Chau7 session is waiting for an approval before it can continue."
                                .to_owned()
                        }),
                });
            }
        }

        if matches!(
            runtime.last_exit_reason.as_deref(),
            Some("error" | "failed")
        ) {
            push_unique_badge(entity, "agent-error");
            if !entity
                .attribution_notes
                .iter()
                .any(|note| note.contains("last Chau7 turn exited"))
            {
                entity.attribution_notes.push(format!(
                    "The last Chau7 turn for session {} exited with {}.",
                    session_id,
                    runtime.last_exit_reason.as_deref().unwrap_or("an error")
                ));
            }
            if !entity
                .recommendations
                .iter()
                .any(|recommendation| recommendation.title == "Inspect failed agent turn")
            {
                entity.recommendations.push(aetower_model::Recommendation {
                    title: "Inspect failed agent turn".to_owned(),
                    detail: "Recent Chau7 runtime state indicates the last turn failed. Check the terminal transcript, approvals, or subprocess exits tied to this session.".to_owned(),
                });
            }
        }

        if runtime.child_session_count > 0 {
            push_unique_badge(entity, "delegating");
            if !entity
                .attribution_notes
                .iter()
                .any(|note| note.contains("child Chau7 session"))
            {
                entity.attribution_notes.push(format!(
                    "Chau7 reports {} child session(s) delegated from this runtime.",
                    runtime.child_session_count
                ));
            }
        }

        if let Some(active_run) = runtime.active_run.as_ref()
            && active_run.duration_so_far_ms > 0
            && entity.recent_change_summary.is_none()
        {
            entity.recent_change_summary = Some(format!(
                "{} has been active in Chau7 for {}.",
                tab.provider_label().unwrap_or("Agent"),
                human_duration_millis(active_run.duration_so_far_ms)
            ));
        }
    }

    if let Some(status) = tab_status {
        if status.is_at_prompt {
            push_unique_badge(entity, "at-prompt");
        }
        if status.cto_active {
            push_unique_badge(entity, "cto-active");
        }
        if status.shell_loading {
            push_unique_badge(entity, "shell-loading");
        }
    }

    if let Some(event) = repo_events.and_then(|events| events.first()) {
        let label = chau7_event_label(event);
        if !label.is_empty() {
            entity.recent_change_summary = Some(format!("Recent Chau7 event: {label}."));
        }
        match event.event_type.as_str() {
            "waiting_input" => {
                push_unique_badge(entity, "waiting-input");
                if !entity
                    .recommendations
                    .iter()
                    .any(|recommendation| recommendation.title == "Resume waiting agent")
                {
                    entity.recommendations.push(aetower_model::Recommendation {
                        title: "Resume waiting agent".to_owned(),
                        detail: "This agent session is currently waiting for input. Resume the conversation if you expect more work from it.".to_owned(),
                    });
                }
            }
            "permission" => push_unique_badge(entity, "approval-needed"),
            "process_ended" => {
                push_unique_badge(entity, "recent-process-exit");
                if !entity
                    .attribution_notes
                    .iter()
                    .any(|note| note.contains("recent Chau7-linked process exit"))
                {
                    entity.attribution_notes.push(
                        "A recent Chau7-linked process exit was observed for this repo session."
                            .to_owned(),
                    );
                }
            }
            "idle" => push_unique_badge(entity, "agent-idle"),
            "finished" => push_unique_badge(entity, "agent-finished"),
            _ => {}
        }
    }

    entity.recommendations.truncate(4);
}

fn chau7_event_label(event: &crate::chau7::Chau7RepoEvent) -> String {
    let message = event.message.trim();
    match event.event_type.as_str() {
        "waiting_input" => "waiting for input".to_owned(),
        "permission" => {
            if message.is_empty() {
                "blocked on permission".to_owned()
            } else {
                message.to_owned()
            }
        }
        "finished" => {
            if message.is_empty() {
                "run finished".to_owned()
            } else {
                message.to_owned()
            }
        }
        "process_ended" => "linked subprocess exited".to_owned(),
        "idle" => {
            if message.is_empty() {
                "agent is idle".to_owned()
            } else {
                message.to_owned()
            }
        }
        other => {
            if message.is_empty() {
                other.replace('_', " ")
            } else {
                message.to_owned()
            }
        }
    }
}

fn human_duration_millis(duration_millis: u64) -> String {
    let total_seconds = duration_millis / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn fetch_chromium_targets(
    config: &ChromiumAdapterConfig,
    samples: &mut BTreeMap<String, ChromiumRuntimeSample>,
) -> Result<Vec<ChromiumPageTarget>, String> {
    let response = http_get_tcp(&config.host, config.port, &config.path, CHROMIUM_TIMEOUT)?;
    let parsed: Vec<ChromiumTarget> = serde_json::from_str(&response)
        .map_err(|error| format!("invalid chromium json: {error}"))?;
    let captured_at_millis = time::now_millis();
    let started_at = Instant::now();
    let mut seen_ids = BTreeSet::new();

    let mut targets = parsed
        .into_iter()
        .filter(|target| target.target_type.as_deref() == Some("page"))
        .map(|target| ChromiumPageTarget {
            id: target.id.unwrap_or_else(|| "?".to_owned()),
            title: target
                .title
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Untitled page".to_owned()),
            url: target
                .url
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "about:blank".to_owned()),
            debug_socket: target.web_socket_debugger_url,
            js_heap_used_bytes: 0,
            js_heap_total_bytes: 0,
            dom_nodes: 0,
            documents: 0,
            frames: 0,
            cpu_percent: 0.0,
            network_bps: 0,
        })
        .collect::<Vec<_>>();
    targets.sort_by(|left, right| left.title.cmp(&right.title).then(left.id.cmp(&right.id)));
    targets.truncate(MAX_CHROMIUM_TARGETS);

    for target in &mut targets {
        seen_ids.insert(target.id.clone());
        if started_at.elapsed() >= CHROMIUM_FETCH_BUDGET {
            break;
        }

        if let Some(debug_socket) = target.debug_socket.as_deref()
            && let Ok(metrics) = fetch_chromium_metrics(debug_socket)
        {
            target.js_heap_used_bytes = metrics.js_heap_used_bytes;
            target.js_heap_total_bytes = metrics.js_heap_total_bytes;
            target.dom_nodes = metrics.dom_nodes;
            target.documents = metrics.documents;
            target.frames = metrics.frames;

            if let Some(previous) = samples.insert(
                target.id.clone(),
                ChromiumRuntimeSample {
                    captured_at_millis,
                    task_duration_seconds: metrics.task_duration_seconds,
                    network_bytes: metrics.network_bytes,
                },
            ) {
                let elapsed_seconds =
                    ((captured_at_millis.saturating_sub(previous.captured_at_millis)) as f64
                        / 1000.0)
                        .max(0.001);
                let task_delta = if metrics.task_duration_seconds >= previous.task_duration_seconds
                {
                    metrics.task_duration_seconds - previous.task_duration_seconds
                } else {
                    0.0
                };
                let network_delta = metrics.network_bytes.saturating_sub(previous.network_bytes);
                target.cpu_percent = ((task_delta / elapsed_seconds) * 100.0) as f32;
                target.network_bps = ((network_delta as f64) / elapsed_seconds) as u64;
            }
        }
    }

    samples.retain(|id, _| seen_ids.contains(id));
    targets.sort_by(|left, right| {
        right
            .cpu_percent
            .total_cmp(&left.cpu_percent)
            .then(left.title.cmp(&right.title))
    });
    Ok(targets)
}

fn fetch_docker_containers(config: &DockerAdapterConfig) -> Result<Vec<DockerContainer>, String> {
    let response = http_get_unix(
        &config.socket_path,
        "/containers/json?all=1",
        DOCKER_TIMEOUT,
    )?;
    let parsed: Vec<DockerContainerSummary> =
        serde_json::from_str(&response).map_err(|error| format!("invalid docker json: {error}"))?;
    let started_at = Instant::now();
    let mut containers = parsed
        .into_iter()
        .map(|container| DockerContainer {
            id: container.id.clone(),
            name: container
                .names
                .into_iter()
                .next()
                .unwrap_or_else(|| "/unnamed".to_owned())
                .trim_start_matches('/')
                .to_owned(),
            image: container.image,
            status: if container.status.is_empty() {
                container.state
            } else {
                container.status
            },
            ports: container
                .ports
                .into_iter()
                .map(|port| match (port.ip, port.public_port) {
                    (Some(ip), Some(public_port)) => {
                        format!(
                            "{}:{}->{} {}",
                            ip, public_port, port.private_port, port.port_type
                        )
                    }
                    (None, Some(public_port)) => {
                        format!("{}->{} {}", public_port, port.private_port, port.port_type)
                    }
                    _ => format!("{} {}", port.private_port, port.port_type),
                })
                .collect(),
            cpu_percent: 0.0,
            memory_usage_bytes: 0,
            memory_limit_bytes: 0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            block_read_bytes: 0,
            block_write_bytes: 0,
            pids: 0,
        })
        .collect::<Vec<_>>();
    containers.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    containers.truncate(MAX_DOCKER_CONTAINERS);

    for container in &mut containers {
        if started_at.elapsed() >= DOCKER_FETCH_BUDGET {
            break;
        }

        if let Ok(stats) = fetch_docker_container_stats(config, &container.id) {
            container.cpu_percent = docker_cpu_percent(&stats);
            container.memory_usage_bytes = stats.memory_stats.usage.unwrap_or(0);
            container.memory_limit_bytes = stats.memory_stats.limit.unwrap_or(0);
            (container.network_rx_bytes, container.network_tx_bytes) =
                docker_network_totals(&stats.networks);
            (container.block_read_bytes, container.block_write_bytes) =
                docker_block_io_totals(&stats.blkio_stats.io_service_bytes_recursive);
            container.pids = stats.pids_stats.current.unwrap_or(0);
        }
    }

    Ok(containers)
}

/// Replace the user's home directory prefix with `~` for compact display.
fn shorten_home_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(home_str.as_ref()) {
            return format!("~{rest}");
        }
    }
    path.to_owned()
}

fn fetch_privileged_helper_sample(helper_path: &str) -> Result<PrivilegedHelperSnapshot, String> {
    let output = Command::new(helper_path)
        .args(["sample", "--json"])
        .output()
        .map_err(|error| format!("helper execution failed: {error}"))?;
    if !output.status.success() {
        return Err(format!("helper exited with status {}", output.status));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid privileged helper json: {error}"))
}

fn fetch_endpoint_security_sample(helper_path: &str) -> Result<EndpointSecuritySample, String> {
    let output = Command::new(helper_path)
        .arg("esf-sample")
        .output()
        .map_err(|error| format!("helper ESF execution failed: {error}"))?;
    if !output.status.success() {
        return Err(format!("helper ESF exited with status {}", output.status));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid endpoint security helper json: {error}"))
}

/// Shell out to the helper for a fan control command.
///
/// Returns `Ok(())` on success, or a message containing the helper's stderr
/// on failure. We intentionally propagate the helper's stderr verbatim — it
/// is the clearest signal the UI has for distinguishing "not running as
/// root" from "SMC key not present on this Mac".
fn invoke_helper_fan_command(
    helper_path: &str,
    subcommand: &str,
    fan_id: u8,
    rpm: Option<f32>,
) -> Result<(), String> {
    let mut command = Command::new(helper_path);
    command.arg(subcommand).arg(fan_id.to_string());
    if let Some(rpm) = rpm {
        command.arg(rpm.to_string());
    }
    let output = command
        .output()
        .map_err(|error| format!("helper execution failed: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let suffix = if stderr.is_empty() {
        String::new()
    } else {
        format!(": {stderr}")
    };
    Err(format!(
        "helper {subcommand} exited with status {}{suffix}",
        output.status
    ))
}

fn helper_process_matches(entity: &EntitySnapshot, process: &PrivilegedProcessSample) -> bool {
    let display_name = entity.display_name.to_ascii_lowercase();
    let process_name = process.process_name.to_ascii_lowercase();
    let executable_name = entity
        .executable_path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let helper_executable = process
        .executable_name
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();

    display_name.contains(&process_name)
        || process_name.contains(&display_name)
        || (!helper_executable.is_empty() && executable_name == helper_executable)
}

fn enrich_with_endpoint_security(entity: &mut EntitySnapshot, sample: &EndpointSecuritySample) {
    if !sample.status.can_stream_events {
        if !entity
            .attribution_notes
            .iter()
            .any(|note| note.contains("Endpoint Security helper is configured"))
            && sample.status.supported
        {
            entity.attribution_notes.push(format!(
                "Endpoint Security helper is configured but not live yet: {}.",
                sample.status.detail
            ));
        }
        return;
    }

    let pids = entity
        .components
        .iter()
        .filter_map(|component| component.process_id)
        .collect::<BTreeSet<_>>();
    if pids.is_empty() {
        return;
    }

    let matching = sample
        .events
        .iter()
        .filter(|event| {
            event.pid.is_some_and(|pid| pids.contains(&pid))
                || event.parent_pid.is_some_and(|pid| pids.contains(&pid))
                || event.child_pid.is_some_and(|pid| pids.contains(&pid))
        })
        .cloned()
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return;
    }

    let exec_count = matching
        .iter()
        .filter(|event| event.event_type.eq_ignore_ascii_case("exec"))
        .count();
    let fork_count = matching
        .iter()
        .filter(|event| event.event_type.eq_ignore_ascii_case("fork"))
        .count();
    let exit_count = matching
        .iter()
        .filter(|event| event.event_type.eq_ignore_ascii_case("exit"))
        .count();
    let latest_timestamp = matching
        .iter()
        .map(|event| event.timestamp_millis)
        .max()
        .unwrap_or(0);
    let latest_process_path = matching
        .iter()
        .filter_map(|event| event.process_path.clone())
        .next();
    let exit_signals = matching
        .iter()
        .filter_map(|event| event.signal)
        .collect::<Vec<_>>();
    let exit_codes = matching
        .iter()
        .filter_map(|event| event.exit_code)
        .collect::<Vec<_>>();

    if !entity.badges.iter().any(|badge| badge == "esf-lineage") {
        entity.badges.push("esf-lineage".to_owned());
    }

    if let Some(primary) = entity.primary_provenance.as_mut()
        && primary.confidence != AttributionConfidence::High
    {
        primary.confidence = AttributionConfidence::High;
        if primary.rule.is_empty() {
            primary.rule = "endpoint security lifecycle corroboration".to_owned();
        } else if !primary.rule.contains("endpoint security") {
            primary
                .rule
                .push_str("; endpoint security lifecycle corroboration");
        }
    }

    let mut upgraded_components = 0usize;
    for component in &mut entity.components {
        let Some(pid) = component.process_id else {
            continue;
        };
        let component_matches = matching.iter().any(|event| {
            event.pid == Some(pid) || event.parent_pid == Some(pid) || event.child_pid == Some(pid)
        });
        if !component_matches {
            continue;
        }
        if let Some(provenance) = component.provenance.as_mut()
            && provenance.confidence != AttributionConfidence::High
        {
            provenance.confidence = AttributionConfidence::High;
            if provenance.rule.is_empty() {
                provenance.rule = "endpoint security lifecycle event".to_owned();
            } else if !provenance.rule.contains("endpoint security") {
                provenance
                    .rule
                    .push_str("; endpoint security lifecycle event");
            }
            upgraded_components += 1;
        }
    }

    if upgraded_components > 0 {
        entity.attribution_notes.push(format!(
            "Endpoint Security corroborated {} component lineage match(es) via {} exec, {} fork, and {} exit event(s).",
            upgraded_components, exec_count, fork_count, exit_count
        ));
    } else {
        entity.attribution_notes.push(format!(
            "Endpoint Security observed {} lifecycle event(s) tied to this entity ({} exec, {} fork, {} exit).",
            matching.len(), exec_count, fork_count, exit_count
        ));
    }

    if entity.recent_change_summary.is_none() {
        entity.recent_change_summary = Some(format!(
            "Endpoint Security observed {} recent lifecycle event(s) for this entity{}{}.",
            matching.len(),
            latest_process_path
                .as_deref()
                .map(|path| format!(" around {}", path.rsplit('/').next().unwrap_or(path)))
                .unwrap_or_default(),
            if latest_timestamp > 0 {
                format!(" at {} ms", latest_timestamp)
            } else {
                String::new()
            }
        ));
    }

    if exit_count >= 2
        && !entity
            .recommendations
            .iter()
            .any(|recommendation| recommendation.title == "Inspect ESF exit churn")
    {
        entity.recommendations.push(aetower_model::Recommendation {
            title: "Inspect ESF exit churn".to_owned(),
            detail: format!(
                "Endpoint Security saw {} recent exit event(s) for this entity. Check helpers, launch agents, or child tasks that may be exiting unexpectedly.",
                exit_count
            ),
        });
        entity.recommendations.truncate(4);
    }

    if (!exit_codes.is_empty() || !exit_signals.is_empty())
        && !entity
            .attribution_notes
            .iter()
            .any(|note| note.contains("recent exit status"))
    {
        entity.attribution_notes.push(format!(
            "Endpoint Security recent exit status: codes [{}], signals [{}].",
            exit_codes
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            exit_signals
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

fn parse_http_endpoint(raw: &str) -> Option<(String, u16, String)> {
    let without_scheme = raw.strip_prefix("http://")?;
    let (authority, path) = match without_scheme.split_once('/') {
        Some((authority, rest)) => (authority, format!("/{}", rest)),
        None => (without_scheme, "/json/list".to_owned()),
    };
    let (host, port) = authority.split_once(':')?;
    let port = port.parse().ok()?;
    Some((host.to_owned(), port, path))
}

fn fetch_chromium_metrics(debug_socket: &str) -> Result<ChromiumMetrics, String> {
    let _ = Url::parse(debug_socket).map_err(|error| format!("invalid websocket url: {error}"))?;
    let (mut socket, _) =
        connect(debug_socket).map_err(|error| format!("websocket connect failed: {error}"))?;

    let enable_command = json!({ "id": 1, "method": "Performance.enable" });
    socket
        .send(Message::Text(enable_command.to_string().into()))
        .map_err(|error| format!("websocket enable failed: {error}"))?;
    let _ = read_cdp_response(&mut socket, 1)?;

    let metrics_command = json!({ "id": 2, "method": "Performance.getMetrics" });
    socket
        .send(Message::Text(metrics_command.to_string().into()))
        .map_err(|error| format!("websocket metrics failed: {error}"))?;
    let payload = read_cdp_response(&mut socket, 2)?;
    let metrics = payload
        .get("result")
        .and_then(|value| value.get("metrics"))
        .and_then(Value::as_array)
        .ok_or_else(|| "cdp response missing metrics array".to_owned())?;

    let runtime_command = json!({
        "id": 3,
        "method": "Runtime.evaluate",
        "params": {
            "expression": "(()=>{const kinds=['navigation','resource'];let total=0;for(const kind of kinds){for(const entry of performance.getEntriesByType(kind)){total += entry.transferSize || entry.encodedBodySize || entry.decodedBodySize || 0;}}return Math.round(total);})()",
            "returnByValue": true
        }
    });
    socket
        .send(Message::Text(runtime_command.to_string().into()))
        .map_err(|error| format!("websocket runtime evaluate failed: {error}"))?;
    let runtime_payload = read_cdp_response(&mut socket, 3)?;

    let metric = |name: &str| -> u64 {
        metrics
            .iter()
            .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|value| value.get("value"))
            .and_then(Value::as_f64)
            .map(|value| value.max(0.0) as u64)
            .unwrap_or(0)
    };
    let metric_f64 = |name: &str| -> f64 {
        metrics
            .iter()
            .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|value| value.get("value"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let network_bytes = runtime_payload
        .get("result")
        .and_then(|value| value.get("result"))
        .and_then(|value| value.get("value"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    Ok(ChromiumMetrics {
        js_heap_used_bytes: metric("JSHeapUsedSize"),
        js_heap_total_bytes: metric("JSHeapTotalSize"),
        dom_nodes: metric("Nodes"),
        documents: metric("Documents"),
        frames: metric("Frames"),
        task_duration_seconds: metric_f64("TaskDuration"),
        network_bytes,
    })
}

fn read_cdp_response(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    expected_id: u64,
) -> Result<Value, String> {
    loop {
        let message = socket
            .read()
            .map_err(|error| format!("websocket read failed: {error}"))?;
        match message {
            Message::Text(text) => {
                let payload: Value = serde_json::from_str(&text)
                    .map_err(|error| format!("invalid cdp payload: {error}"))?;
                if payload.get("id").and_then(Value::as_u64) == Some(expected_id) {
                    return Ok(payload);
                }
            }
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
            Message::Close(_) => return Err("websocket closed before expected response".to_owned()),
        }
    }
}

fn fetch_docker_container_stats(
    config: &DockerAdapterConfig,
    container_id: &str,
) -> Result<DockerContainerStats, String> {
    let response = http_get_unix(
        &config.socket_path,
        &format!("/containers/{container_id}/stats?stream=0"),
        DOCKER_TIMEOUT,
    )?;
    serde_json::from_str(&response).map_err(|error| format!("invalid docker stats json: {error}"))
}

fn docker_cpu_percent(stats: &DockerContainerStats) -> f32 {
    let cpu_delta = stats
        .cpu_stats
        .cpu_usage
        .total_usage
        .saturating_sub(stats.pre_cpu_stats.cpu_usage.total_usage);
    let system_delta = stats
        .cpu_stats
        .system_cpu_usage
        .unwrap_or(0)
        .saturating_sub(stats.pre_cpu_stats.system_cpu_usage.unwrap_or(0));
    if cpu_delta == 0 || system_delta == 0 {
        return 0.0;
    }

    let cpu_count = stats
        .cpu_stats
        .online_cpus
        .filter(|count| *count > 0)
        .map(|count| count as f32)
        .unwrap_or_else(|| stats.cpu_stats.cpu_usage.percpu_usage.len().max(1) as f32);

    (cpu_delta as f32 / system_delta as f32) * cpu_count * 100.0
}

fn docker_network_totals(networks: &BTreeMap<String, DockerNetworkStats>) -> (u64, u64) {
    networks.values().fold((0, 0), |(rx, tx), network| {
        (
            rx.saturating_add(network.rx_bytes),
            tx.saturating_add(network.tx_bytes),
        )
    })
}

fn docker_block_io_totals(entries: &[DockerBlkioEntry]) -> (u64, u64) {
    entries.iter().fold((0, 0), |(read, write), entry| {
        let value = entry.value.unwrap_or(0);
        match entry.op.as_deref() {
            Some("Read") => (read.saturating_add(value), write),
            Some("Write") => (read, write.saturating_add(value)),
            _ => (read, write),
        }
    })
}

fn chromium_target_detail(target: &ChromiumPageTarget) -> String {
    let mut parts = vec![target.url.clone()];
    if target.js_heap_used_bytes > 0 || target.js_heap_total_bytes > 0 {
        parts.push(format!(
            "heap {} / {}",
            human_bytes(target.js_heap_used_bytes),
            human_bytes(target.js_heap_total_bytes)
        ));
    }
    if target.network_bps > 0 {
        parts.push(format!("net {}/s", human_bytes(target.network_bps)));
    }
    if target.dom_nodes > 0 || target.documents > 0 || target.frames > 0 {
        parts.push(format!(
            "{} nodes · {} docs · {} frames",
            target.dom_nodes, target.documents, target.frames
        ));
    }
    if let Some(socket) = target.debug_socket.as_deref() {
        parts.push(socket.to_owned());
    }
    parts.join(" · ")
}

fn docker_container_detail(container: &DockerContainer) -> String {
    let mut parts = vec![
        container.image.clone(),
        container.status.clone(),
        format!(
            "cpu {:.1}% · mem {}",
            container.cpu_percent,
            human_bytes(container.memory_usage_bytes)
        ),
    ];
    if container.memory_limit_bytes > 0 {
        parts.push(format!(
            "limit {}",
            human_bytes(container.memory_limit_bytes)
        ));
    }
    if container.network_rx_bytes > 0 || container.network_tx_bytes > 0 {
        parts.push(format!(
            "net {}↓ {}↑",
            human_bytes(container.network_rx_bytes),
            human_bytes(container.network_tx_bytes)
        ));
    }
    if container.block_read_bytes > 0 || container.block_write_bytes > 0 {
        parts.push(format!(
            "io {} read {} write",
            human_bytes(container.block_read_bytes),
            human_bytes(container.block_write_bytes)
        ));
    }
    if container.pids > 0 {
        parts.push(format!("{} pids", container.pids));
    }
    if !container.ports.is_empty() {
        parts.push(container.ports.join(", "));
    }
    parts.join(" · ")
}

fn enrich_vscode_entity(entity: &mut EntitySnapshot) {
    if !is_vscode_entity(entity) {
        return;
    }

    let summary = summarize_vscode_entity(entity);
    if summary.workspace.is_none()
        && summary.extension_hosts == 0
        && summary.watchers == 0
        && summary.pty_hosts == 0
        && summary.shared_processes == 0
    {
        return;
    }

    if let Some(workspace) = summary.workspace.as_deref() {
        entity.components.push(ComponentSnapshot {
            kind: ComponentKind::AdapterContext,
            title: "Workspace".to_owned(),
            detail: workspace.to_owned(),
            adapter_context: Some(AdapterContextSnapshot {
                kind: AdapterContextKind::VsCodeWorkspace,
                status: None,
                url: None,
                workspace_path: Some(workspace.to_owned()),
                repo_root: None,
                image_name: None,
                session_id: None,
                app_version: None,
                build_sha: None,
                build_timestamp: None,
                build_channel: None,
                network_receive_bps: 0,
                network_send_bps: 0,
                disk_read_bps: 0,
                disk_write_bps: 0,
                memory_limit_bytes: 0,
                js_heap_total_bytes: 0,
                dom_nodes: 0,
                documents: 0,
                frames: 0,
                process_count: None,
                connection_count: None,
                ports: Vec::new(),
            }),
            provenance: Some(ProvenanceSnapshot {
                kind: ProvenanceKind::ParentProcess,
                label: "VS Code workspace context".to_owned(),
                rule: "workspace path heuristic".to_owned(),
                confidence: AttributionConfidence::Low,
            }),
            process_id: None,
            start_time_millis: 0,
            executable_path: None,
            command_line: None,
            parent_summary: None,
            launched_by: None,
            cpu_percent: 0.0,
            memory_bytes: 0,
            memory_physical_footprint_bytes: 0,
            cwd: None,
            user: None,
        });
    }

    if summary.extension_hosts > 0 {
        entity.components.push(ComponentSnapshot {
            kind: ComponentKind::AdapterContext,
            title: "Extension Host".to_owned(),
            detail: format!("{} extension host process(es)", summary.extension_hosts),
            adapter_context: Some(AdapterContextSnapshot {
                kind: AdapterContextKind::VsCodeRuntime,
                status: Some("extension-host".to_owned()),
                url: None,
                workspace_path: summary.workspace.clone(),
                repo_root: None,
                image_name: None,
                session_id: None,
                app_version: None,
                build_sha: None,
                build_timestamp: None,
                build_channel: None,
                network_receive_bps: 0,
                network_send_bps: 0,
                disk_read_bps: 0,
                disk_write_bps: 0,
                memory_limit_bytes: 0,
                js_heap_total_bytes: 0,
                dom_nodes: 0,
                documents: 0,
                frames: 0,
                process_count: Some(summary.extension_hosts.min(u32::MAX as usize) as u32),
                connection_count: None,
                ports: Vec::new(),
            }),
            provenance: None,
            process_id: None,
            start_time_millis: 0,
            executable_path: None,
            command_line: None,
            parent_summary: None,
            launched_by: None,
            cpu_percent: 0.0,
            memory_bytes: 0,
            memory_physical_footprint_bytes: 0,
            cwd: None,
            user: None,
        });
        push_unique_badge(entity, "vscode-extension-host");
    }

    if summary.watchers > 0 || summary.pty_hosts > 0 || summary.shared_processes > 0 {
        let mut parts = Vec::new();
        if summary.watchers > 0 {
            parts.push(format!("{} watcher(s)", summary.watchers));
        }
        if summary.pty_hosts > 0 {
            parts.push(format!("{} pty host(s)", summary.pty_hosts));
        }
        if summary.shared_processes > 0 {
            parts.push(format!("{} shared process(es)", summary.shared_processes));
        }
        entity.components.push(ComponentSnapshot {
            kind: ComponentKind::AdapterContext,
            title: "Editor runtime".to_owned(),
            detail: parts.join(" · "),
            adapter_context: Some(AdapterContextSnapshot {
                kind: AdapterContextKind::VsCodeRuntime,
                status: Some("editor-runtime".to_owned()),
                url: None,
                workspace_path: summary.workspace.clone(),
                repo_root: None,
                image_name: None,
                session_id: None,
                app_version: None,
                build_sha: None,
                build_timestamp: None,
                build_channel: None,
                network_receive_bps: 0,
                network_send_bps: 0,
                disk_read_bps: 0,
                disk_write_bps: 0,
                memory_limit_bytes: 0,
                js_heap_total_bytes: 0,
                dom_nodes: 0,
                documents: 0,
                frames: 0,
                process_count: Some(
                    (summary.watchers + summary.pty_hosts + summary.shared_processes)
                        .min(u32::MAX as usize) as u32,
                ),
                connection_count: None,
                ports: Vec::new(),
            }),
            provenance: None,
            process_id: None,
            start_time_millis: 0,
            executable_path: None,
            command_line: None,
            parent_summary: None,
            launched_by: None,
            cpu_percent: 0.0,
            memory_bytes: 0,
            memory_physical_footprint_bytes: 0,
            cwd: None,
            user: None,
        });
    }

    push_unique_badge(entity, "vscode-live");
}

fn is_vscode_entity(entity: &EntitySnapshot) -> bool {
    let display_name = entity.display_name.to_lowercase();
    let bundle_id = entity
        .bundle_id
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    matches!(
        display_name.as_str(),
        "visual studio code" | "code" | "code - insiders" | "cursor" | "vscodium"
    ) || bundle_id.contains("code")
        || bundle_id.contains("cursor")
}

fn summarize_vscode_entity(entity: &EntitySnapshot) -> VsCodeHeuristicSummary {
    let mut summary = VsCodeHeuristicSummary::default();

    for component in &entity.components {
        let command_line = component.command_line.as_deref().unwrap_or_default();
        if command_line.contains("--type=extensionHost") {
            summary.extension_hosts += 1;
        }
        if command_line.contains("--type=fileWatcher") || command_line.contains("watcherService") {
            summary.watchers += 1;
        }
        if command_line.contains("--type=ptyHost") {
            summary.pty_hosts += 1;
        }
        if command_line.contains("--type=shared-process") {
            summary.shared_processes += 1;
        }
        if summary.workspace.is_none() {
            summary.workspace = workspace_hint_from_command_line(command_line);
        }
    }

    summary
}

fn workspace_hint_from_command_line(command_line: &str) -> Option<String> {
    for segment in command_line.split_whitespace() {
        if let Some(uri) = segment.strip_prefix("--folder-uri=") {
            return Some(trim_file_uri(uri));
        }
        if let Some(uri) = segment.strip_prefix("--file-uri=") {
            return Some(trim_file_uri(uri));
        }
        if segment.starts_with('/') && !segment.starts_with("/Applications/") {
            return Some(segment.trim_matches('"').to_owned());
        }
    }
    None
}

fn trim_file_uri(uri: &str) -> String {
    uri.trim_matches('"')
        .strip_prefix("file://")
        .unwrap_or(uri.trim_matches('"'))
        .to_owned()
}

fn push_unique_badge(entity: &mut EntitySnapshot, badge: &str) {
    if !entity.badges.iter().any(|existing| existing == badge) {
        entity.badges.push(badge.to_owned());
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GiB", value / GB)
    } else if value >= MB {
        format!("{:.1} MiB", value / MB)
    } else if value >= KB {
        format!("{:.1} KiB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

fn http_get_tcp(host: &str, port: u16, path: &str, timeout: Duration) -> Result<String, String> {
    let mut stream =
        TcpStream::connect((host, port)).map_err(|error| format!("tcp connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("tcp read timeout failed: {error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("tcp write timeout failed: {error}"))?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        path, host, port
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("tcp write failed: {error}"))?;

    read_http_body(&mut stream)
}

fn http_get_unix(socket_path: &str, path: &str, timeout: Duration) -> Result<String, String> {
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("unix connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("unix read timeout failed: {error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("unix write timeout failed: {error}"))?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n",
        path
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("unix write failed: {error}"))?;

    read_http_body(&mut stream)
}

fn read_http_body(stream: &mut impl Read) -> Result<String, String> {
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read failed: {error}"))?;

    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "http response missing header terminator".to_owned())?;
    Ok(body.to_owned())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use aetower_model::{
        AggregateMetrics, AttributionConfidence, CapabilityHealth, CapabilitySnapshot,
        ComponentKind, ComponentSnapshot, EntityKind, EntitySnapshot, FrictionBreakdown,
        MetricTrend, ProvenanceKind, ProvenanceSnapshot,
    };
    use parking_lot::Mutex;

    use super::{
        AdapterManager, AdapterState, CapabilityKind, CapabilityState, ChromiumTarget,
        DockerBlkioEntry, DockerContainerStats, DockerContainerSummary, DockerNetworkStats,
        EndpointSecurityLifecycleEvent, EndpointSecuritySample, EndpointSecurityStatusSnapshot,
        adapter_runtime_detail, capability_status, docker_block_io_totals, docker_cpu_percent,
        docker_network_totals, endpoint_security_runtime_detail, enrich_vscode_entity,
        enrich_with_endpoint_security, parse_http_endpoint, resolved_chau7_socket_path,
        sanitize_chau7_socket_path, workspace_hint_from_command_line,
    };

    #[test]
    fn parses_http_endpoint_with_explicit_path() {
        let result = parse_http_endpoint("http://127.0.0.1:9222/json/list");
        assert_eq!(
            result,
            Some(("127.0.0.1".to_owned(), 9222, "/json/list".to_owned()))
        );
    }

    #[test]
    fn parses_http_endpoint_with_default_path() {
        let result = parse_http_endpoint("http://localhost:9223");
        assert_eq!(
            result,
            Some(("localhost".to_owned(), 9223, "/json/list".to_owned()))
        );
    }

    #[test]
    fn chromium_target_deserializes() {
        let raw = r#"{"type":"page","id":"1","title":"Docs","url":"https://example.com","webSocketDebuggerUrl":"ws://localhost/devtools/page/1"}"#;
        let parsed: ChromiumTarget = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.target_type.as_deref(), Some("page"));
        assert_eq!(parsed.id.as_deref(), Some("1"));
        assert_eq!(parsed.title.as_deref(), Some("Docs"));
    }

    #[test]
    fn docker_container_deserializes() {
        let raw = r#"{"Id":"abc123","Names":["/web"],"Image":"nginx:latest","State":"running","Status":"Up 3 minutes","Ports":[{"IP":"0.0.0.0","PrivatePort":80,"PublicPort":8080,"Type":"tcp"}]}"#;
        let parsed: DockerContainerSummary = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.names[0], "/web");
        assert_eq!(parsed.image, "nginx:latest");
        assert_eq!(parsed.status, "Up 3 minutes");
        assert_eq!(parsed.ports[0].public_port, Some(8080));
    }

    #[test]
    fn docker_cpu_percent_uses_usage_deltas() {
        let raw = r#"{
            "cpu_stats":{"cpu_usage":{"total_usage":300,"percpu_usage":[100,200]},"system_cpu_usage":2000,"online_cpus":2},
            "precpu_stats":{"cpu_usage":{"total_usage":100,"percpu_usage":[50,50]},"system_cpu_usage":1000,"online_cpus":2},
            "memory_stats":{"usage":0,"limit":0},
            "networks":{},
            "blkio_stats":{"io_service_bytes_recursive":[]},
            "pids_stats":{"current":1}
        }"#;
        let parsed: DockerContainerStats = serde_json::from_str(raw).unwrap();
        assert_eq!(docker_cpu_percent(&parsed), 40.0);
    }

    #[test]
    fn docker_network_totals_accumulate_interfaces() {
        let mut networks = BTreeMap::new();
        networks.insert(
            "eth0".to_owned(),
            DockerNetworkStats {
                rx_bytes: 1024,
                tx_bytes: 2048,
            },
        );
        networks.insert(
            "eth1".to_owned(),
            DockerNetworkStats {
                rx_bytes: 512,
                tx_bytes: 256,
            },
        );
        assert_eq!(docker_network_totals(&networks), (1536, 2304));
    }

    #[test]
    fn docker_block_io_totals_split_read_write() {
        let entries = vec![
            DockerBlkioEntry {
                op: Some("Read".to_owned()),
                value: Some(2048),
            },
            DockerBlkioEntry {
                op: Some("Write".to_owned()),
                value: Some(4096),
            },
            DockerBlkioEntry {
                op: Some("Read".to_owned()),
                value: Some(1024),
            },
        ];
        assert_eq!(docker_block_io_totals(&entries), (3072, 4096));
    }

    #[test]
    fn privileged_helper_capability_reflects_configuration() {
        let state = AdapterState {
            privileged_helper_enabled: true,
            privileged_helper_path: Some("/tmp/missing-helper".to_owned()),
            ..AdapterState::default()
        };
        let (capability_state, detail) =
            capability_status(&state, &CapabilityKind::PrivilegedHelper);
        assert_eq!(capability_state, CapabilityState::Unavailable);
        assert!(detail.contains("missing"));
    }

    #[test]
    fn adapter_runtime_detail_reports_live_cached_and_errors() {
        let live = adapter_runtime_detail("tabs", 3, 20_000, 20_000, None, 10_000, 25_000);
        assert!(live.contains("status live"));
        assert!(live.contains("3 cached tabs"));

        let cached = adapter_runtime_detail("tabs", 1, 1_000, 1_000, None, 5_000, 20_000);
        assert!(cached.contains("status cached"));

        let degraded = adapter_runtime_detail(
            "tabs",
            0,
            15_000,
            20_000,
            Some("tcp connect failed"),
            10_000,
            21_000,
        );
        assert!(degraded.contains("status degraded"));
        assert!(degraded.contains("last error: tcp connect failed"));
    }

    #[test]
    fn endpoint_security_runtime_detail_reports_entitlement_state() {
        let state = AdapterState {
            last_endpoint_security_fetch_millis: 20_000,
            last_endpoint_security_success_millis: 20_000,
            cached_endpoint_security_sample: Some(EndpointSecuritySample {
                status: EndpointSecurityStatusSnapshot {
                    supported: true,
                    helper_entitled: true,
                    running_as_root: true,
                    can_stream_events: true,
                    eslogger_path: Some("/usr/bin/eslogger".to_owned()),
                    detail: "Endpoint Security event streaming is available.".to_owned(),
                    last_error: None,
                },
                events: vec![EndpointSecurityLifecycleEvent {
                    event_type: "exec".to_owned(),
                    timestamp_millis: 1,
                    process_path: Some("/bin/zsh".to_owned()),
                    pid: Some(41),
                    parent_pid: Some(1),
                    child_pid: None,
                    exit_code: None,
                    signal: None,
                }],
            }),
            ..AdapterState::default()
        };
        let detail = endpoint_security_runtime_detail(
            &state,
            state
                .cached_endpoint_security_sample
                .as_ref()
                .map(|sample| &sample.status)
                .expect("status"),
            25_000,
        );
        assert!(detail.contains("helper entitled"));
        assert!(detail.contains("sampled lifecycle events"));
    }

    #[test]
    fn workspace_hint_extracts_folder_uri() {
        let command = r#"code --folder-uri=file:///Users/test/src/project --reuse-window"#;
        assert_eq!(
            workspace_hint_from_command_line(command).as_deref(),
            Some("/Users/test/src/project")
        );
    }

    #[test]
    fn sanitize_chau7_socket_path_treats_blank_as_unset() {
        assert_eq!(sanitize_chau7_socket_path(None), None);
        assert_eq!(sanitize_chau7_socket_path(Some("  ".to_owned())), None);
        assert_eq!(
            sanitize_chau7_socket_path(Some(" /tmp/chau7.sock ".to_owned())),
            Some("/tmp/chau7.sock".to_owned())
        );
    }

    #[test]
    fn resolved_chau7_socket_path_prefers_explicit_path() {
        let state = AdapterState {
            chau7_socket_path: Some("/tmp/custom.sock".to_owned()),
            ..AdapterState::default()
        };
        assert_eq!(
            resolved_chau7_socket_path(&state),
            Some("/tmp/custom.sock".to_owned())
        );
    }

    #[test]
    fn vscode_enrichment_adds_workspace_and_extension_context() {
        let mut entity = EntitySnapshot {
            entity_id: "bundle-path:/Applications/Visual Studio Code.app".to_owned(),
            display_name: "Visual Studio Code".to_owned(),
            primary_provenance: None,
            launcher_summary: None,
            attribution_notes: Vec::new(),
            bundle_id: Some("com.microsoft.VSCode".to_owned()),
            executable_path: Some(
                "/Applications/Visual Studio Code.app/Contents/MacOS/Electron".to_owned(),
            ),
            oldest_process_start_millis: 0,
            newest_process_start_millis: 0,
            entity_kind: EntityKind::App,
            metrics: AggregateMetrics::default(),
            friction: FrictionBreakdown::default(),
            components: vec![
                ComponentSnapshot {
                    kind: ComponentKind::Process,
                    title: "Code".to_owned(),
                    detail: String::new(),
                    adapter_context: None,
                    provenance: None,
                    process_id: Some(1),
                    start_time_millis: 0,
                    executable_path: None,
                    command_line: Some(
                        "code --folder-uri=file:///Users/test/src/project".to_owned(),
                    ),
                    parent_summary: None,
                    launched_by: None,
                    cpu_percent: 0.0,
                    memory_bytes: 0,
                    memory_physical_footprint_bytes: 0,
                    cwd: None,
                    user: None,
                },
                ComponentSnapshot {
                    kind: ComponentKind::Process,
                    title: "Code Helper".to_owned(),
                    detail: String::new(),
                    adapter_context: None,
                    provenance: None,
                    process_id: Some(2),
                    start_time_millis: 0,
                    executable_path: None,
                    command_line: Some(
                        "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper --type=extensionHost".to_owned(),
                    ),
                    parent_summary: None,
                    launched_by: None,
                    cpu_percent: 0.0,
                    memory_bytes: 0,
                    memory_physical_footprint_bytes: 0,
                    cwd: None,
                    user: None,
                },
            ],
            trend: MetricTrend::default(),
            badges: Vec::new(),
            active_window_title: None,
            recent_change_summary: None,
            anomaly_detected: false,
            thermal_contribution: None,
            grouping_suggestion: None,
            agent_cost: None,
            session_markers: Vec::new(),
            recommendations: Vec::new(),
        };

        enrich_vscode_entity(&mut entity);

        assert!(entity.badges.iter().any(|badge| badge == "vscode-live"));
        assert!(
            entity
                .badges
                .iter()
                .any(|badge| badge == "vscode-extension-host")
        );
        assert!(
            entity
                .components
                .iter()
                .any(|component| component.title == "Workspace")
        );
        assert!(
            entity
                .components
                .iter()
                .any(|component| component.title == "Extension Host")
        );
    }

    #[test]
    fn endpoint_security_enrichment_upgrades_attribution_confidence() {
        let mut entity = EntitySnapshot {
            entity_id: "entity".to_owned(),
            display_name: "Code".to_owned(),
            primary_provenance: Some(ProvenanceSnapshot {
                kind: ProvenanceKind::AppBundle,
                label: "Application bundle".to_owned(),
                rule: "bundle".to_owned(),
                confidence: AttributionConfidence::Medium,
            }),
            launcher_summary: None,
            attribution_notes: Vec::new(),
            bundle_id: None,
            executable_path: None,
            oldest_process_start_millis: 0,
            newest_process_start_millis: 0,
            entity_kind: EntityKind::App,
            metrics: AggregateMetrics::default(),
            friction: FrictionBreakdown::default(),
            components: vec![ComponentSnapshot {
                kind: ComponentKind::Process,
                title: "Code Helper".to_owned(),
                detail: String::new(),
                adapter_context: None,
                provenance: Some(ProvenanceSnapshot {
                    kind: ProvenanceKind::ParentProcess,
                    label: "Parent process".to_owned(),
                    rule: "heuristic".to_owned(),
                    confidence: AttributionConfidence::Low,
                }),
                process_id: Some(42),
                start_time_millis: 1,
                executable_path: None,
                command_line: None,
                parent_summary: None,
                launched_by: None,
                cpu_percent: 0.0,
                memory_bytes: 0,
                memory_physical_footprint_bytes: 0,
                cwd: None,
                user: None,
            }],
            trend: MetricTrend::default(),
            badges: Vec::new(),
            active_window_title: None,
            recent_change_summary: None,
            anomaly_detected: false,
            thermal_contribution: None,
            grouping_suggestion: None,
            agent_cost: None,
            session_markers: Vec::new(),
            recommendations: Vec::new(),
        };

        enrich_with_endpoint_security(
            &mut entity,
            &EndpointSecuritySample {
                status: EndpointSecurityStatusSnapshot {
                    supported: true,
                    helper_entitled: true,
                    running_as_root: true,
                    can_stream_events: true,
                    eslogger_path: Some("/usr/bin/eslogger".to_owned()),
                    detail: "live".to_owned(),
                    last_error: None,
                },
                events: vec![
                    EndpointSecurityLifecycleEvent {
                        event_type: "exec".to_owned(),
                        timestamp_millis: 1,
                        process_path: Some("/bin/zsh".to_owned()),
                        pid: Some(42),
                        parent_pid: Some(1),
                        child_pid: None,
                        exit_code: None,
                        signal: None,
                    },
                    EndpointSecurityLifecycleEvent {
                        event_type: "exit".to_owned(),
                        timestamp_millis: 2,
                        process_path: Some("/bin/zsh".to_owned()),
                        pid: Some(42),
                        parent_pid: Some(1),
                        child_pid: None,
                        exit_code: Some(1),
                        signal: None,
                    },
                    EndpointSecurityLifecycleEvent {
                        event_type: "exit".to_owned(),
                        timestamp_millis: 3,
                        process_path: Some("/bin/zsh".to_owned()),
                        pid: Some(42),
                        parent_pid: Some(1),
                        child_pid: None,
                        exit_code: Some(9),
                        signal: Some(9),
                    },
                ],
            },
        );

        assert!(entity.badges.iter().any(|badge| badge == "esf-lineage"));
        assert_eq!(
            entity
                .primary_provenance
                .as_ref()
                .map(|value| value.confidence.clone()),
            Some(AttributionConfidence::High)
        );
        assert!(
            entity
                .components
                .first()
                .and_then(|component| component.provenance.as_ref())
                .map(|value| value.confidence == AttributionConfidence::High)
                .unwrap_or(false)
        );
        assert!(!entity.attribution_notes.is_empty());
        assert!(!entity.recommendations.is_empty());
    }

    #[test]
    fn chau7_enrichment_surfaces_runtime_state_and_repo_events() {
        let manager = AdapterManager {
            state: Arc::new(Mutex::new(AdapterState {
                cached_chau7_snapshot: Some(crate::chau7::Chau7Snapshot {
                    tabs: vec![
                        crate::chau7::Chau7Tab {
                            tab_id: "tab-1".to_owned(),
                            title: "Aetower coding".to_owned(),
                            cwd: "/Users/test/Aetower".to_owned(),
                            repo_root: Some("/Users/test/Aetower".to_owned()),
                            git_branch: Some("master".to_owned()),
                            ai_provider: Some("claude".to_owned()),
                            ai_session_id: Some("session-1".to_owned()),
                            status: "running".to_owned(),
                            active_app: Some("Claude".to_owned()),
                            window_id: 7,
                        },
                        crate::chau7::Chau7Tab {
                            tab_id: "tab-2".to_owned(),
                            title: "Detached scratchpad".to_owned(),
                            cwd: "/Users/test/Scratch".to_owned(),
                            repo_root: Some("/Users/test/Scratch".to_owned()),
                            git_branch: Some("feature/demo".to_owned()),
                            ai_provider: Some("codex".to_owned()),
                            ai_session_id: Some("session-2".to_owned()),
                            status: "idle".to_owned(),
                            active_app: Some("Codex".to_owned()),
                            window_id: 8,
                        },
                    ],
                    sessions: vec![
                        crate::chau7::Chau7Session {
                            session_id: "session-1".to_owned(),
                            provider: "claude".to_owned(),
                            repo_path: "/Users/test/Aetower".to_owned(),
                            run_count: 6,
                            last_active: "2026-04-08T14:22:26.696Z".to_owned(),
                        },
                        crate::chau7::Chau7Session {
                            session_id: "session-2".to_owned(),
                            provider: "codex".to_owned(),
                            repo_path: "/Users/test/Scratch".to_owned(),
                            run_count: 2,
                            last_active: "2026-04-08T14:25:00.000Z".to_owned(),
                        },
                    ],
                    runtime_info: Some(crate::chau7::Chau7RuntimeInfo {
                        app_version: Some("1.4.2".to_owned()),
                        build_sha: Some("abc123def456".to_owned()),
                        build_timestamp: Some("2026-04-08T14:20:00Z".to_owned()),
                        build_channel: Some("dev".to_owned()),
                    }),
                    repo_stats: BTreeMap::from([(
                        "/Users/test/Aetower".to_owned(),
                        crate::chau7::Chau7RepoStats {
                            total_runs: 6,
                            total_tokens: 82_167_695,
                            total_cost: 12.5,
                            total_turns: 503,
                            providers: vec!["claude".to_owned(), "codex".to_owned()],
                        },
                    )]),
                    recent_runs: vec![],
                    tab_statuses: BTreeMap::from([(
                        "tab-1".to_owned(),
                        crate::chau7::Chau7TabStatus {
                            title: "Claude".to_owned(),
                            git_branch: Some("master".to_owned()),
                            status: "running".to_owned(),
                            is_at_prompt: true,
                            shell_loading: false,
                            cto_active: true,
                            raw_status: Some("running".to_owned()),
                        },
                    )]),
                    runtime_sessions: BTreeMap::from([(
                        "session-1".to_owned(),
                        crate::chau7::Chau7RuntimeSessionStatus {
                            state: "ready".to_owned(),
                            turn_count: 3,
                            last_completed_turn_id: Some("t_3".to_owned()),
                            last_exit_reason: Some("error".to_owned()),
                            pending_approval: Some(crate::chau7::Chau7PendingApproval {
                                description: "Claude needs your permission to use Bash".to_owned(),
                            }),
                            active_run: Some(crate::chau7::Chau7ActiveRun {
                                duration_so_far_ms: 438_172,
                            }),
                            child_session_count: 2,
                        },
                    )]),
                    repo_events: BTreeMap::from([(
                        "/Users/test/Aetower".to_owned(),
                        vec![crate::chau7::Chau7RepoEvent {
                            event_type: "permission".to_owned(),
                            message: "Claude needs your permission to use Write".to_owned(),
                        }],
                    )]),
                }),
                ..AdapterState::default()
            })),
        };

        let capabilities = BTreeMap::from([(
            CapabilityKind::Chau7,
            CapabilitySnapshot {
                kind: CapabilityKind::Chau7,
                state: CapabilityState::Granted,
                health: CapabilityHealth::Live,
                detail: "live".to_owned(),
                last_updated_millis: 0,
            },
        )]);

        let mut entities = vec![EntitySnapshot {
            entity_id: "agent".to_owned(),
            display_name: "Claude".to_owned(),
            primary_provenance: None,
            launcher_summary: None,
            attribution_notes: Vec::new(),
            bundle_id: None,
            executable_path: Some("/Users/test/Aetower/.claude/agent-binary".to_owned()),
            oldest_process_start_millis: 0,
            newest_process_start_millis: 0,
            entity_kind: EntityKind::AiAgent,
            metrics: AggregateMetrics::default(),
            friction: FrictionBreakdown::default(),
            components: vec![ComponentSnapshot {
                kind: ComponentKind::Process,
                title: "Claude".to_owned(),
                detail: String::new(),
                adapter_context: None,
                provenance: None,
                process_id: Some(1),
                start_time_millis: 0,
                executable_path: Some("/Users/test/Aetower/bin/claude".to_owned()),
                command_line: None,
                parent_summary: None,
                launched_by: None,
                cpu_percent: 0.0,
                memory_bytes: 0,
                memory_physical_footprint_bytes: 0,
                cwd: None,
                user: None,
            }],
            trend: MetricTrend::default(),
            badges: Vec::new(),
            active_window_title: None,
            recent_change_summary: None,
            anomaly_detected: false,
            thermal_contribution: None,
            grouping_suggestion: None,
            agent_cost: None,
            session_markers: Vec::new(),
            recommendations: Vec::new(),
        }];

        manager.enrich_entities(&mut entities, &capabilities);
        let entity = &entities[0];
        let session_summaries = manager.chau7_session_summaries(&entities);

        assert!(entity.badges.iter().any(|badge| badge == "chau7-live"));
        assert!(entity.badges.iter().any(|badge| badge == "approval-needed"));
        assert!(entity.badges.iter().any(|badge| badge == "delegating"));
        assert!(entity.badges.iter().any(|badge| badge == "at-prompt"));
        assert!(entity.badges.iter().any(|badge| badge == "cto-active"));
        assert!(
            entity
                .badges
                .iter()
                .any(|badge| badge == "ai-session:session-1")
        );
        assert!(
            entity
                .attribution_notes
                .iter()
                .any(|note| note.contains("503 total turn(s)"))
        );
        assert!(
            entity
                .attribution_notes
                .iter()
                .any(|note| note.contains("child session(s)"))
        );
        assert_eq!(
            entity.recent_change_summary.as_deref(),
            Some("Recent Chau7 event: Claude needs your permission to use Write.")
        );
        assert!(
            entity
                .recommendations
                .iter()
                .any(|recommendation| recommendation.title == "Resolve pending agent approval")
        );
        assert!(
            entity
                .recommendations
                .iter()
                .any(|recommendation| recommendation.title == "Inspect failed agent turn")
        );
        let adapter_component = entity
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::AdapterContext)
            .expect("adapter component");
        assert_eq!(
            adapter_component
                .adapter_context
                .as_ref()
                .and_then(|value| value.status.as_deref()),
            Some("approval-needed")
        );
        assert_eq!(
            adapter_component
                .adapter_context
                .as_ref()
                .and_then(|value| value.app_version.as_deref()),
            Some("1.4.2")
        );
        assert_eq!(
            adapter_component
                .adapter_context
                .as_ref()
                .and_then(|value| value.build_sha.as_deref()),
            Some("abc123def456")
        );
        assert!(adapter_component.detail.contains("3 turns"));
        assert!(adapter_component.detail.contains("2 child sessions"));
        assert_eq!(session_summaries.len(), 2);
        let matched = session_summaries
            .iter()
            .find(|summary| summary.session_id.as_deref() == Some("session-1"))
            .unwrap_or_else(|| panic!("matched session"));
        assert_eq!(matched.title, "Claude");
        assert_eq!(matched.git_branch.as_deref(), Some("master"));
        assert_eq!(matched.window_id, 7);
        assert_eq!(matched.linked_entity_ids, vec!["agent".to_owned()]);
        let detached = session_summaries
            .iter()
            .find(|summary| summary.session_id.as_deref() == Some("session-2"))
            .unwrap_or_else(|| panic!("detached session"));
        assert_eq!(detached.title, "Detached scratchpad");
        assert_eq!(detached.git_branch.as_deref(), Some("feature/demo"));
        assert!(detached.linked_entity_ids.is_empty());
    }
}
