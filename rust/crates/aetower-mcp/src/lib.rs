use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{Read, Write},
    net::Shutdown,
    os::fd::AsRawFd,
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsOverview, DiagnosticsQuery, DiagnosticsSubsystem,
};
use aetower_model::{RuntimeLagMetrics, SystemSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "aetower";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const INITIAL_SNAPSHOT_WAIT: Duration = Duration::from_millis(2500);
const INITIAL_SNAPSHOT_POLL: Duration = Duration::from_millis(100);
const SOCKET_DIR_MODE: u32 = 0o700;
const SOCKET_FILE_MODE: u32 = 0o600;
const MCP_READ_TIMEOUT: Duration = Duration::from_millis(250);
const PROXY_POLL_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_INLINE_TEXT_BYTES: usize = 8 * 1024;
const DEFAULT_PROFILE_DURATION_SECONDS: u64 = 3;
const MAX_PROFILE_DURATION_SECONDS: u64 = 15;
const DEFAULT_TOP_STACKS: usize = 5;
const DEFAULT_OPEN_RESOURCE_LIMIT: usize = 80;
const DEFAULT_PROCESS_ACTION_HISTORY_LIMIT: usize = 25;
const DEFAULT_PROCESS_ACTION_HISTORY_WINDOW_MINUTES: u64 = 60;
const DEFAULT_TOP_REGIONS: usize = 10;
const DEFAULT_HOST_ALERT_TOP_ENTITIES: usize = 5;
const DEFAULT_FINDINGS_LIMIT: usize = 10;
const DEFAULT_RECENT_CHANGES_LIMIT: usize = 25;
const DEFAULT_RECENT_CHANGES_WINDOW_MILLIS: u64 = 30 * 60 * 1000;
const DEFAULT_HISTORY_WINDOW_MILLIS: u64 = 72 * 60 * 60 * 1000;
const DEFAULT_INVESTIGATION_WINDOW_MINUTES: u64 = 30;
const DEFAULT_INVESTIGATION_ENTITY_LIMIT: usize = 5;
const DEFAULT_INVESTIGATION_DIAGNOSTICS_LIMIT: usize = 128;
const DEFAULT_DIAGNOSTICS_SUMMARY_LIMIT: usize = 25;
const DEFAULT_DIAGNOSTICS_SUMMARY_QUERY_LIMIT: usize = 5000;
const DEFAULT_INVESTIGATION_HISTORY_LIMIT: usize = 512;
const DEFAULT_HISTORY_EXPECTED_INTERVAL_MILLIS: u64 = 10_000;
const DEFAULT_HISTORY_QUALITY_MAX_SNAPSHOTS: usize = 4096;
const HISTORY_DATA_GAP_MULTIPLIER: u64 = 3;
const DEFAULT_EXPORT_HISTORY_LIMIT: u32 = 120;
const MEMORY_PRESSURE_WARNING_RATIO: f64 = 0.80;
const MEMORY_PRESSURE_CRITICAL_RATIO: f64 = 0.90;
const COMPRESSED_MEMORY_WARNING_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const COMPRESSED_MEMORY_CRITICAL_BYTES: u64 = 6 * 1024 * 1024 * 1024;
const SWAP_WARNING_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const SWAP_CRITICAL_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const WAKEUPS_WARNING: f32 = 12_000.0;
const WAKEUPS_CRITICAL: f32 = 25_000.0;
const HISTORY_STORE_WARNING_BYTES: u64 = 512 * 1024 * 1024;
const HISTORY_STORE_CRITICAL_BYTES: u64 = 1024 * 1024 * 1024;
const HISTORY_WAL_WARNING_BYTES: u64 = 32 * 1024 * 1024;
const HISTORY_WAL_CRITICAL_BYTES: u64 = 128 * 1024 * 1024;
const HISTORY_QUARANTINE_WARNING: u64 = 64;
const HISTORY_QUARANTINE_CRITICAL: u64 = 256;
const DIAGNOSTICS_WARN_WARNING: u32 = 200;
const DIAGNOSTICS_WARN_CRITICAL: u32 = 800;
const DIAGNOSTICS_ERROR_WARNING: u32 = 10;
const DIAGNOSTICS_ERROR_CRITICAL: u32 = 50;
const DIAGNOSTICS_ACTIVE_ERROR_WINDOW_MILLIS: u64 = 10 * 60 * 1000;
const MCP_HELPER_WARNING_COUNT: u32 = 4;
const MCP_HELPER_CRITICAL_COUNT: u32 = 8;
const MCP_HELPER_STALE_MILLIS: u64 = 15 * 60 * 1000;
const MCP_REQUEST_WARNING_RATE: f32 = 10.0;
const MCP_REQUEST_CRITICAL_RATE: f32 = 25.0;
const MCP_REQUEST_BURST_CLIENT_LIMIT: u32 = 3;
const MCP_REQUEST_BURST_HELPER_LIMIT: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageFraming {
    ContentLength,
    JsonLine,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistorySummaryResponse {
    pub store_bytes: u64,
    pub wal_bytes: u64,
    pub snapshot_count: u64,
    pub quarantine_count: u64,
    pub range_count: u64,
    pub oldest_millis: Option<u64>,
    pub newest_millis: Option<u64>,
    pub pending_writes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
enum SeverityBand {
    Info,
    Warning,
    Critical,
}

impl SeverityBand {
    fn score(self) -> u8 {
        match self {
            Self::Info => 1,
            Self::Warning => 2,
            Self::Critical => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExportPrivacyTier {
    Redacted,
    OperatorMode,
    Full,
}

#[derive(Debug, Clone, Serialize)]
struct TopFinding {
    id: String,
    severity: SeverityBand,
    title: String,
    detail: String,
    source: String,
    entity_ids: Vec<String>,
    recommendation: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct HostAlert {
    id: String,
    severity: SeverityBand,
    category: String,
    title: String,
    detail: String,
    metrics: BTreeMap<String, Value>,
    entity_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GroupTreeNode {
    entity_id: String,
    display_name: String,
    relation: String,
    friction: f32,
    cpu_percent: f32,
    memory_bytes: u64,
    process_count: u32,
    badges: Vec<String>,
    recent_change_summary: Option<String>,
    children: Vec<GroupTreeNode>,
}

#[derive(Debug, Clone, Serialize)]
struct AiRuntimeSummary {
    agent_count: usize,
    total_energy_nj_per_s: f64,
    total_cost_usd: f32,
    total_session_energy_nj: u64,
    host_gpu_percent: f32,
    host_gpu_memory_unified_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
struct Chau7BuildIdentityReport {
    app_version: Option<String>,
    build_sha: Option<String>,
    build_timestamp: Option<String>,
    build_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AiRuntimeGroupReport {
    provider: String,
    workspace: Option<String>,
    agent_count: usize,
    total_cpu_percent: f32,
    total_memory_bytes: u64,
    total_energy_nj_per_s: f64,
    total_cost_usd: f32,
    approval_count: usize,
    delegating_count: usize,
    chau7_build: Option<Chau7BuildIdentityReport>,
}

#[derive(Debug, Clone, Serialize)]
struct AiBurdenLeaderReport {
    kind: String,
    entity_id: String,
    display_name: String,
    value_label: String,
}

#[derive(Debug, Clone, Serialize)]
struct AiApprovalReport {
    entity_id: String,
    display_name: String,
    session_id: Option<String>,
    workspace: Option<String>,
    detail: String,
    impact: String,
}

#[derive(Debug, Clone, Serialize)]
struct AiDelegationReport {
    entity_id: String,
    display_name: String,
    session_id: Option<String>,
    workspace: Option<String>,
    child_session_count: usize,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct AiHistoricalTrendReport {
    provider: String,
    workspace: Option<String>,
    cpu_percent: Vec<f64>,
    memory_bytes: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct RecentChangeItem {
    timestamp_millis: u64,
    severity: SeverityBand,
    source: String,
    entity_id: Option<String>,
    title: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilityStatusItem {
    kind: String,
    state: String,
    health: String,
    operator_label: String,
    action_label: String,
    detail: String,
    last_updated_millis: u64,
    severity: SeverityBand,
}

#[derive(Debug, Clone, Serialize)]
struct HistoryStoreHealth {
    severity: SeverityBand,
    summary: String,
    range: HistorySummaryResponse,
    thresholds: BTreeMap<String, Value>,
    data_quality: Option<HistoryDataQualityReport>,
    recent_history_events: Vec<DiagnosticsEvent>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct HistoryDataQualityReport {
    severity: SeverityBand,
    summary: String,
    window_start_millis: u64,
    window_end_millis: u64,
    sampled_snapshots: usize,
    oldest_millis: Option<u64>,
    newest_millis: Option<u64>,
    coverage_millis: u64,
    coverage_ratio: f64,
    expected_interval_millis: u64,
    gap_threshold_millis: u64,
    largest_gap_millis: u64,
    gap_count: usize,
    duplicate_timestamp_count: usize,
    sequence_regression_count: usize,
    sequence_reset_count: usize,
    boot_boundary_count: usize,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SupportBundleSectionManifest {
    name: String,
    estimated_bytes: usize,
    redacted: bool,
    description: String,
}

#[derive(Debug, Clone, Serialize)]
struct RecommendationItem {
    severity: SeverityBand,
    title: String,
    detail: String,
    entity_id: Option<String>,
    source: String,
    expected_benefit: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionHealthCheck {
    key: String,
    severity: SeverityBand,
    summary: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticsSummaryGroup {
    subsystem: String,
    event_type: String,
    level: String,
    count: usize,
    first_millis: u64,
    latest_millis: u64,
    latest_message: String,
    latest_detail: Option<String>,
    sample_fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticsSummaryReport {
    overview: DiagnosticsOverview,
    event_count: usize,
    limit: usize,
    since_millis: Option<u64>,
    include_persisted: bool,
    minimum_level: Option<DiagnosticsLevel>,
    subsystem: Option<aetower_diagnostics::DiagnosticsSubsystem>,
    search: Option<String>,
    groups: Vec<DiagnosticsSummaryGroup>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone)]
struct DiagnosticsSummaryOptions {
    limit: usize,
    since_millis: Option<u64>,
    include_persisted: bool,
    minimum_level: Option<DiagnosticsLevel>,
    subsystem: Option<aetower_diagnostics::DiagnosticsSubsystem>,
    search: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotMetricDelta {
    before: f64,
    after: f64,
    delta: f64,
    percent_change: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotEntityDelta {
    entity_id: String,
    display_name: String,
    before_present: bool,
    after_present: bool,
    friction: SnapshotMetricDelta,
    cpu_percent: SnapshotMetricDelta,
    memory_bytes: SnapshotMetricDelta,
    memory_physical_footprint_bytes: SnapshotMetricDelta,
    wakeups_per_second: SnapshotMetricDelta,
    process_count: SnapshotMetricDelta,
    recent_change_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotDiffSummary {
    host_summary: String,
    entity_summary: String,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotDiffReport {
    before_snapshot_millis: u64,
    after_snapshot_millis: u64,
    crossed_boot_boundary: bool,
    before_boot_id: Option<String>,
    after_boot_id: Option<String>,
    before_boot_time_millis: Option<u64>,
    after_boot_time_millis: Option<u64>,
    before_previous_shutdown: Option<aetower_model::RebootCauseSnapshot>,
    after_previous_shutdown: Option<aetower_model::RebootCauseSnapshot>,
    summary: SnapshotDiffSummary,
    host: BTreeMap<String, SnapshotMetricDelta>,
    entities: Vec<SnapshotEntityDelta>,
}

#[derive(Debug, Clone, Serialize)]
struct RebootSessionReport {
    boot_id: Option<String>,
    boot_time_millis: Option<u64>,
    first_snapshot_millis: u64,
    last_snapshot_millis: u64,
    snapshot_count: usize,
    first_sequence: u64,
    last_sequence: u64,
    previous_shutdown: Option<aetower_model::RebootCauseSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
struct RebootCorrelationMarker {
    timestamp_millis: u64,
    event_type: String,
    level: String,
    message: String,
    detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RebootBoundaryReport {
    before_snapshot_millis: u64,
    after_snapshot_millis: u64,
    reboot_detected_at_millis: Option<u64>,
    gap_millis: u64,
    before_boot_id: Option<String>,
    after_boot_id: Option<String>,
    before_boot_time_millis: Option<u64>,
    after_boot_time_millis: Option<u64>,
    previous_shutdown: Option<aetower_model::RebootCauseSnapshot>,
    before_top_entities: Vec<String>,
    after_top_entities: Vec<String>,
    correlated_markers: Vec<RebootCorrelationMarker>,
    pre_reboot_incidents: Vec<RebootCorrelationMarker>,
}

#[derive(Debug, Clone, Serialize)]
struct RebootReport {
    range_start_millis: u64,
    range_end_millis: u64,
    session_count: usize,
    boundary_count: usize,
    sessions: Vec<RebootSessionReport>,
    boundaries: Vec<RebootBoundaryReport>,
}

#[derive(Debug, Clone, Serialize)]
struct AnomalyDriver {
    metric: String,
    before: f64,
    after: f64,
    delta: f64,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
struct AnomalyExplanation {
    entity_id: String,
    display_name: String,
    severity: SeverityBand,
    summary: String,
    recent_change_summary: Option<String>,
    drivers: Vec<AnomalyDriver>,
    supporting_events: Vec<RecentChangeItem>,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessTreeNodeReport {
    title: String,
    pid: Option<u32>,
    relation: String,
    owner_entity_id: String,
    owner_display_name: String,
    self_cpu_percent: f32,
    subtree_cpu_percent: f32,
    self_memory_bytes: u64,
    subtree_memory_bytes: u64,
    subtree_process_count: u32,
    badges: Vec<String>,
    user: Option<String>,
    cwd: Option<String>,
    provenance: Option<String>,
    launched_by: Option<String>,
    adapter_label: Option<String>,
    status_label: Option<String>,
    children: Vec<ProcessTreeNodeReport>,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessTreeReport {
    captured_at_millis: u64,
    root_entity_id: String,
    root_display_name: String,
    seed_entity_ids: Vec<String>,
    expanded_entity_ids: Vec<String>,
    grouped_process_count: u32,
    expanded_process_count: u32,
    grouping_reasons: Vec<String>,
    roots: Vec<ProcessTreeNodeReport>,
}

#[derive(Debug, Clone, Serialize)]
struct InvestigationBundleReport {
    captured_at_millis: u64,
    window_start_millis: u64,
    window_end_millis: u64,
    window_minutes: u64,
    entity_ids: Vec<String>,
    host_alerts: Vec<HostAlert>,
    top_findings: Vec<TopFinding>,
    recent_changes: Vec<RecentChangeItem>,
    diagnostics: Vec<DiagnosticsEvent>,
    history_diff: Option<SnapshotDiffReport>,
    process_trees: Vec<ProcessTreeReport>,
    caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum DynamicToolRequest {
    MemoryBreakdown {
        entity_id: String,
        top_regions: usize,
    },
    ProfileEntity {
        entity_id: String,
        duration_seconds: u64,
        top_stacks: usize,
    },
    WakeupAttribution {
        entity_id: String,
        duration_seconds: u64,
        top_stacks: usize,
    },
    ProcessInspect {
        pid: u32,
    },
    ProcessOpenResources {
        pid: u32,
        limit: usize,
    },
    ProcessSample {
        pid: u32,
        duration_seconds: u64,
        top_stacks: usize,
    },
    ProcessAction {
        pid: u32,
        action: String,
        dry_run: bool,
        reason: Option<String>,
    },
    ProcessActionHistory {
        window_minutes: u64,
        limit: usize,
    },
}

#[derive(Debug, Clone, Serialize)]
struct MemoryRegionBreakdown {
    region_type: String,
    virtual_bytes: u64,
    resident_bytes: u64,
    dirty_bytes: u64,
    swap_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
struct EntityMemoryBreakdown {
    captured_at_millis: u64,
    entity_id: String,
    display_name: String,
    process_ids: Vec<u32>,
    resident_bytes: u64,
    physical_footprint_bytes: u64,
    memory_metric_note: String,
    regions: Vec<MemoryRegionBreakdown>,
}

#[derive(Debug, Clone, Serialize)]
struct SampledStackReport {
    thread_label: String,
    queue_label: Option<String>,
    sample_count: u32,
    top_frames: Vec<String>,
    classification: String,
}

#[derive(Debug, Clone, Serialize)]
struct EntityProfileReport {
    captured_at_millis: u64,
    entity_id: String,
    display_name: String,
    duration_seconds: u64,
    sampled_process_ids: Vec<u32>,
    thread_count: usize,
    top_stacks: Vec<SampledStackReport>,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
struct WakeupAttributionReport {
    captured_at_millis: u64,
    entity_id: String,
    display_name: String,
    duration_seconds: u64,
    sampled_process_ids: Vec<u32>,
    queue_breakdown: Vec<SampledStackReport>,
    dominant_cause: Option<String>,
    attribution_mode: String,
    caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessPsSummary {
    parent_pid: Option<u32>,
    user: Option<String>,
    status: Option<String>,
    cpu_percent: Option<f32>,
    resident_bytes: Option<u64>,
    command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessInspectionReport {
    captured_at_millis: u64,
    pid: u32,
    alive: bool,
    entity_id: Option<String>,
    display_name: Option<String>,
    component_title: Option<String>,
    component_kind: Option<String>,
    executable_path: Option<String>,
    command_line: Option<String>,
    cwd: Option<String>,
    user: Option<String>,
    parent_pid: Option<u32>,
    parent_summary: Option<String>,
    cpu_percent: Option<f32>,
    memory_bytes: Option<u64>,
    memory_physical_footprint_bytes: Option<u64>,
    start_time_millis: Option<u64>,
    child_pids: Vec<u32>,
    sibling_process_count: u32,
    ps: Option<ProcessPsSummary>,
    safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessOpenResource {
    fd: String,
    resource_type: String,
    name: String,
    detail: Option<String>,
    is_socket: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessOpenResourcesReport {
    captured_at_millis: u64,
    pid: u32,
    resource_count: usize,
    returned: usize,
    file_count: usize,
    socket_count: usize,
    resources: Vec<ProcessOpenResource>,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessSampleReport {
    captured_at_millis: u64,
    pid: u32,
    duration_seconds: u64,
    thread_count: usize,
    top_stacks: Vec<SampledStackReport>,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessActionReport {
    captured_at_millis: u64,
    pid: u32,
    target_pids: Vec<u32>,
    action: String,
    signal: String,
    dry_run: bool,
    executed: bool,
    success: bool,
    command: String,
    reason: Option<String>,
    entity_id: Option<String>,
    display_name: Option<String>,
    message: String,
    safety_notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct ProcessActionPlan {
    normalized_action: String,
    signal: String,
    command: String,
    program: String,
    args: Vec<String>,
    target_pids: Vec<u32>,
    dry_run_message: String,
    success_message: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessActionHistoryItem {
    timestamp_millis: u64,
    pid: Option<u32>,
    target_pids: Vec<u32>,
    action: Option<String>,
    signal: Option<String>,
    success: bool,
    reason: Option<String>,
    entity_id: Option<String>,
    display_name: Option<String>,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessActionHistoryReport {
    window_minutes: u64,
    returned: usize,
    actions: Vec<ProcessActionHistoryItem>,
}

struct ExportQueryOptions<'a> {
    privacy_tier: ExportPrivacyTier,
    entity_ids: &'a [String],
    start_millis: u64,
    end_millis: u64,
    include_snapshot: bool,
    include_history: bool,
    include_diagnostics: bool,
    include_session_health: bool,
    include_ai_runtime_report: bool,
    diagnostics_limit: usize,
    history_limit: u32,
}

pub trait AetowerMcpDataSource: Send + Sync + 'static {
    fn latest_snapshot(&self) -> Result<SystemSnapshot, String>;
    fn latest_snapshot_if_newer(
        &self,
        last_sequence: u64,
    ) -> Result<Option<SystemSnapshot>, String>;
    fn latest_sequence(&self) -> Result<u64, String>;
    fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String>;
    fn history_range_summary(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<HistorySummaryResponse, String>;
    fn load_history_page(
        &self,
        start_millis: u64,
        end_millis: u64,
        before_millis_exclusive: Option<u64>,
        limit: u32,
    ) -> Result<Vec<SystemSnapshot>, String>;
    fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String>;
    fn query_diagnostics(&self, query: DiagnosticsQuery) -> Result<Vec<DiagnosticsEvent>, String>;
    fn record_diagnostics_event(&self, _event: DiagnosticsEvent) {}
    fn record_mcp_runtime_observation(
        &self,
        _total_connections: u64,
        _active_client_count: u64,
        _total_requests: u64,
    ) {
    }
}

pub struct LocalMcpServerHandle {
    running: Arc<AtomicBool>,
    join_handle: Option<thread::JoinHandle<()>>,
    client_threads: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
    socket_path: PathBuf,
    stats: Arc<McpRuntimeStats>,
}

impl LocalMcpServerHandle {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn stats(&self) -> (u64, u64, u64) {
        self.stats.snapshot()
    }
}

#[derive(Default)]
struct McpRuntimeStats {
    total_connections: AtomicU64,
    active_clients: AtomicUsize,
    total_requests: AtomicU64,
}

impl McpRuntimeStats {
    fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.total_connections.load(Ordering::Relaxed),
            self.active_clients.load(Ordering::Relaxed) as u64,
            self.total_requests.load(Ordering::Relaxed),
        )
    }
}

fn publish_mcp_runtime_stats(data_source: &Arc<dyn AetowerMcpDataSource>, stats: &McpRuntimeStats) {
    let (total_connections, active_client_count, total_requests) = stats.snapshot();
    data_source.record_mcp_runtime_observation(
        total_connections,
        active_client_count,
        total_requests,
    );
}

impl Drop for LocalMcpServerHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.socket_path);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
        if let Ok(mut client_threads) = self.client_threads.lock() {
            for join_handle in client_threads.drain(..) {
                let _ = join_handle.join();
            }
        }
        let _ = fs::remove_file(&self.socket_path);
    }
}

#[derive(Clone)]
enum DynamicExecutionMode {
    Local,
}

pub fn default_socket_path() -> PathBuf {
    if let Some(override_path) = env::var_os("AETOWER_MCP_SOCKET_PATH") {
        return PathBuf::from(override_path);
    }
    let base = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    base.join(".aetower").join("mcp.sock")
}

pub fn default_app_support_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Aetower")
}

pub fn start_local_socket_server(
    data_source: Arc<dyn AetowerMcpDataSource>,
    socket_path: impl AsRef<Path>,
) -> Result<LocalMcpServerHandle, String> {
    let socket_path = socket_path.as_ref().to_path_buf();
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("create MCP socket directory {}: {error}", parent.display())
        })?;
        fs::set_permissions(parent, fs::Permissions::from_mode(SOCKET_DIR_MODE)).map_err(
            |error| {
                format!(
                    "set MCP socket directory permissions {}: {error}",
                    parent.display()
                )
            },
        )?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&socket_path) {
        if !metadata.file_type().is_socket() {
            return Err(format!(
                "refusing to replace non-socket MCP path {}",
                socket_path.display()
            ));
        }
        fs::remove_file(&socket_path).map_err(|error| {
            format!("remove stale MCP socket {}: {error}", socket_path.display())
        })?;
    }

    let listener = UnixListener::bind(&socket_path)
        .map_err(|error| format!("bind MCP socket {}: {error}", socket_path.display()))?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(SOCKET_FILE_MODE)).map_err(
        |error| {
            format!(
                "set MCP socket permissions {}: {error}",
                socket_path.display()
            )
        },
    )?;
    let running = Arc::new(AtomicBool::new(true));
    let stats = Arc::new(McpRuntimeStats::default());
    let thread_running = Arc::clone(&running);
    let thread_socket_path = socket_path.clone();
    let client_threads: Arc<Mutex<Vec<thread::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    let thread_client_threads = Arc::clone(&client_threads);
    let thread_stats = Arc::clone(&stats);
    let join_handle = thread::spawn(move || {
        while thread_running.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if !thread_running.load(Ordering::SeqCst) {
                        break;
                    }
                    let source = Arc::clone(&data_source);
                    let connection_running = Arc::clone(&thread_running);
                    thread_stats
                        .total_connections
                        .fetch_add(1, Ordering::Relaxed);
                    thread_stats.active_clients.fetch_add(1, Ordering::Relaxed);
                    publish_mcp_runtime_stats(&source, &thread_stats);
                    let stats = Arc::clone(&thread_stats);
                    let dynamic_mode = DynamicExecutionMode::Local;
                    let join_handle = thread::spawn(move || {
                        if let Err(err) = handle_connection(
                            stream,
                            Arc::clone(&source),
                            dynamic_mode,
                            connection_running,
                            Arc::clone(&stats),
                        ) {
                            // Surface per-connection failures — handle_connection
                            // returns Ok on clean peer close, so reaching here
                            // means a real transport/framing fault the operator
                            // should see instead of the server reporting healthy
                            // while silently dropping every request.
                            eprintln!("aetower-mcp socket: handle_connection: {err}");
                        }
                        stats.active_clients.fetch_sub(1, Ordering::Relaxed);
                        publish_mcp_runtime_stats(&source, &stats);
                    });
                    if let Ok(mut handles) = thread_client_threads.lock() {
                        reap_finished_client_threads(&mut handles);
                        handles.push(join_handle);
                    }
                }
                Err(_) => break,
            }
        }
        if let Ok(mut handles) = thread_client_threads.lock() {
            reap_finished_client_threads(&mut handles);
        }
        let _ = fs::remove_file(thread_socket_path);
    });

    Ok(LocalMcpServerHandle {
        running,
        join_handle: Some(join_handle),
        client_threads,
        socket_path,
        stats,
    })
}

fn reap_finished_client_threads(handles: &mut Vec<thread::JoinHandle<()>>) {
    let mut remaining = Vec::with_capacity(handles.len());
    for handle in handles.drain(..) {
        if handle.is_finished() {
            let _ = handle.join();
        } else {
            remaining.push(handle);
        }
    }
    *handles = remaining;
}

pub fn proxy_stdio_to_socket(socket_path: impl AsRef<Path>) -> Result<(), String> {
    proxy_stdio_to_socket_polling(socket_path)
}

#[cfg(test)]
fn proxy_streams_to_socket<R, W>(
    mut input: R,
    output: W,
    socket_path: impl AsRef<Path>,
) -> Result<(), String>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let socket_path = socket_path.as_ref();
    let stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("connect MCP socket {}: {error}", socket_path.display()))?;
    let mut reader_stream = stream
        .try_clone()
        .map_err(|error| format!("clone socket stream: {error}"))?;
    let mut writer_stream = stream;

    let stdin_thread = thread::spawn(move || -> Result<(), String> {
        match std::io::copy(&mut input, &mut writer_stream) {
            Ok(_) => {}
            Err(ref err) if is_graceful_peer_close(err) => {}
            Err(err) => return Err(format!("copy stdin to MCP socket: {err}")),
        }
        // Signal to the peer that the client is done sending, but keep the
        // read half open so the stdout thread can drain any in-flight response.
        match writer_stream.shutdown(Shutdown::Write) {
            Ok(()) => Ok(()),
            Err(ref err) if is_graceful_peer_close(err) => Ok(()),
            Err(err) => Err(format!("shutdown MCP socket write-half: {err}")),
        }
    });

    let stdout_thread = thread::spawn(move || -> Result<(), String> {
        let mut output = output;
        let mut buffer = [0u8; 8192];
        loop {
            let bytes_read = match reader_stream.read(&mut buffer) {
                Ok(count) => count,
                Err(ref err) if is_graceful_peer_close(err) => 0,
                Err(err) => return Err(format!("read MCP socket: {err}")),
            };
            if bytes_read == 0 {
                break;
            }
            if let Err(err) = output.write_all(&buffer[..bytes_read]) {
                if is_graceful_peer_close(&err) {
                    break;
                }
                return Err(format!("write MCP socket to stdout: {err}"));
            }
            if let Err(err) = output.flush() {
                if is_graceful_peer_close(&err) {
                    break;
                }
                return Err(format!("flush stdout: {err}"));
            }
        }
        Ok(())
    });

    stdin_thread
        .join()
        .map_err(|_| "stdin proxy thread panicked".to_string())??;
    stdout_thread
        .join()
        .map_err(|_| "stdout proxy thread panicked".to_string())??;
    Ok(())
}

fn proxy_stdio_to_socket_polling(socket_path: impl AsRef<Path>) -> Result<(), String> {
    let socket_path = socket_path.as_ref();
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("connect MCP socket {}: {error}", socket_path.display()))?;
    stream
        .set_nonblocking(true)
        .map_err(|error| format!("set MCP socket nonblocking: {error}"))?;

    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();

    let stdin_fd = input.as_raw_fd();
    let socket_fd = stream.as_raw_fd();
    let original_parent_pid = current_parent_pid();
    let mut stdin_open = true;
    let mut write_half_open = true;
    let mut stdin_buffer = [0u8; 8192];
    let mut socket_buffer = [0u8; 8192];

    loop {
        if parent_exited(original_parent_pid) {
            break;
        }

        let mut fds = [
            libc::pollfd {
                fd: stdin_fd,
                events: if stdin_open { libc::POLLIN } else { 0 },
                revents: 0,
            },
            libc::pollfd {
                fd: socket_fd,
                events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
                revents: 0,
            },
        ];

        let poll_result = unsafe {
            libc::poll(
                fds.as_mut_ptr(),
                fds.len() as libc::nfds_t,
                PROXY_POLL_TIMEOUT.as_millis() as i32,
            )
        };
        if poll_result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("poll MCP proxy fds: {error}"));
        }
        if poll_result == 0 {
            continue;
        }

        if stdin_open && (fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0) {
            match input.read(&mut stdin_buffer) {
                Ok(0) => {
                    stdin_open = false;
                    if write_half_open {
                        match stream.shutdown(Shutdown::Write) {
                            Ok(()) => {
                                write_half_open = false;
                            }
                            Err(ref err) if is_graceful_peer_close(err) => {
                                write_half_open = false;
                            }
                            Err(err) => {
                                return Err(format!("shutdown MCP socket write-half: {err}"));
                            }
                        }
                    }
                }
                Ok(bytes_read) => {
                    let mut written = 0;
                    while written < bytes_read {
                        match stream.write(&stdin_buffer[written..bytes_read]) {
                            Ok(0) => break,
                            Ok(count) => written += count,
                            Err(ref err)
                                if err.kind() == std::io::ErrorKind::WouldBlock
                                    || err.kind() == std::io::ErrorKind::Interrupted =>
                            {
                                thread::sleep(Duration::from_millis(10));
                            }
                            Err(ref err) if is_graceful_peer_close(err) => return Ok(()),
                            Err(err) => {
                                return Err(format!("write stdin payload to MCP socket: {err}"));
                            }
                        }
                    }
                }
                Err(ref err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(err) => return Err(format!("read stdin for MCP proxy: {err}")),
            }
        }

        if fds[1].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
            loop {
                match stream.read(&mut socket_buffer) {
                    Ok(0) => return Ok(()),
                    Ok(bytes_read) => {
                        output
                            .write_all(&socket_buffer[..bytes_read])
                            .map_err(|err| format!("write MCP socket to stdout: {err}"))?;
                        output
                            .flush()
                            .map_err(|err| format!("flush stdout: {err}"))?;
                    }
                    Err(ref err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::Interrupted =>
                    {
                        break;
                    }
                    Err(ref err) if is_graceful_peer_close(err) => return Ok(()),
                    Err(err) => return Err(format!("read MCP socket: {err}")),
                }
            }
        }
    }

    Ok(())
}

fn current_parent_pid() -> libc::pid_t {
    unsafe { libc::getppid() }
}

fn parent_exited(original_parent_pid: libc::pid_t) -> bool {
    let current = current_parent_pid();
    current <= 1 || current != original_parent_pid
}

fn is_graceful_peer_close(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::UnexpectedEof
    )
}

fn handle_connection(
    stream: UnixStream,
    data_source: Arc<dyn AetowerMcpDataSource>,
    dynamic_mode: DynamicExecutionMode,
    running: Arc<AtomicBool>,
    stats: Arc<McpRuntimeStats>,
) -> Result<(), String> {
    let mut reader = stream
        .try_clone()
        .map_err(|error| format!("clone MCP socket for read: {error}"))?;
    reader
        .set_read_timeout(Some(MCP_READ_TIMEOUT))
        .map_err(|error| format!("set MCP socket read timeout: {error}"))?;
    let mut writer = stream;
    let server = AetowerMcpServer {
        data_source,
        dynamic_mode,
        mcp_stats: Some(stats),
    };
    let mut framing = None;

    loop {
        match read_message(&mut reader, &mut framing)? {
            ReadMessageOutcome::Message(message) => {
                let response = server.handle_message(message);
                if let Some(response) = response {
                    write_message(
                        &mut writer,
                        &response,
                        framing.unwrap_or(MessageFraming::ContentLength),
                    )?;
                }
            }
            ReadMessageOutcome::EndOfStream => break,
            ReadMessageOutcome::Timeout if running.load(Ordering::SeqCst) => continue,
            ReadMessageOutcome::Timeout => break,
        }
    }
    Ok(())
}

enum ReadMessageOutcome {
    Message(Value),
    EndOfStream,
    Timeout,
}

struct AetowerMcpServer {
    data_source: Arc<dyn AetowerMcpDataSource>,
    dynamic_mode: DynamicExecutionMode,
    mcp_stats: Option<Arc<McpRuntimeStats>>,
}

impl AetowerMcpServer {
    fn handle_message(&self, message: Value) -> Option<Value> {
        if let Some(stats) = self.mcp_stats.as_ref() {
            stats.total_requests.fetch_add(1, Ordering::Relaxed);
            publish_mcp_runtime_stats(&self.data_source, stats);
        }
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": jsonrpc_error(-32600, "Invalid request: missing method"),
            }));
        };
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

        let response = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            })),
            "notifications/initialized" => return None,
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => self.handle_tool_call(params).or_else(Ok),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err(jsonrpc_error(-32601, format!("Unknown method: {method}"))),
        };

        Some(match response {
            Ok(result) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            }),
            Err(error) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": error,
            }),
        })
    }

    fn handle_tool_call(&self, params: Value) -> Result<Value, Value> {
        let tool_name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| jsonrpc_error(-32602, "tools/call missing name"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        match tool_name {
            "aetower_current_snapshot" => self.tool_current_snapshot(arguments),
            "aetower_host_summary" => self.tool_host_summary(arguments),
            "aetower_entity_details" => self.tool_entity_details(arguments),
            "aetower_runtime_lag" => self.tool_runtime_lag(),
            "aetower_diff_snapshots" => self.tool_diff_snapshots(arguments),
            "aetower_reboot_report" => self.tool_reboot_report(arguments),
            "aetower_explain_anomalies" => self.tool_explain_anomalies(arguments),
            "aetower_entity_process_tree" => self.tool_entity_process_tree(arguments),
            "aetower_top_findings" => self.tool_top_findings(arguments),
            "aetower_host_alerts" => self.tool_host_alerts(arguments),
            "aetower_investigation_bundle" => self.tool_investigation_bundle(arguments),
            "aetower_entity_group_tree" => self.tool_entity_group_tree(arguments),
            "aetower_ai_runtime_report" => self.tool_ai_runtime_report(arguments),
            "aetower_recent_changes" => self.tool_recent_changes(arguments),
            "aetower_capability_status" => self.tool_capability_status(),
            "aetower_history_summary" => self.tool_history_summary(arguments),
            "aetower_history_page" => self.tool_history_page(arguments),
            "aetower_history_store_health" => self.tool_history_store_health(arguments),
            "aetower_history_data_quality" => self.tool_history_data_quality(arguments),
            "aetower_memory_breakdown" => self.tool_memory_breakdown(arguments),
            "aetower_profile_entity" => self.tool_profile_entity(arguments),
            "aetower_wakeup_attribution" => self.tool_wakeup_attribution(arguments),
            "aetower_process_inspect" => self.tool_process_inspect(arguments),
            "aetower_process_open_resources" => self.tool_process_open_resources(arguments),
            "aetower_process_sample" => self.tool_process_sample(arguments),
            "aetower_process_action" => self.tool_process_action(arguments),
            "aetower_process_action_history" => self.tool_process_action_history(arguments),
            "aetower_diagnostics_overview" => self.tool_diagnostics_overview(),
            "aetower_diagnostics_summary" => self.tool_diagnostics_summary(arguments),
            "aetower_query_diagnostics" => self.tool_query_diagnostics(arguments),
            "aetower_support_bundle_manifest" => self.tool_support_bundle_manifest(arguments),
            "aetower_recommendations" => self.tool_recommendations(arguments),
            "aetower_session_health" => self.tool_session_health(arguments),
            "aetower_export_query" => self.tool_export_query(arguments),
            _ => Ok(tool_error(format!("Unknown tool: {tool_name}"))),
        }
    }

    fn tool_current_snapshot(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize, Default)]
        struct Args {
            last_sequence: Option<u64>,
            entity_limit: Option<usize>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = if let Some(last_sequence) = args.last_sequence {
            match self
                .data_source
                .latest_snapshot_if_newer(last_sequence)
                .map_err(tool_error)?
            {
                Some(snapshot) => {
                    let summary = SnapshotEnvelope::updated(snapshot, args.entity_limit);
                    return tool_json(summary);
                }
                None => {
                    let warmed = self.wait_for_nonzero_snapshot()?;
                    if warmed.sequence > last_sequence {
                        return tool_json(SnapshotEnvelope::updated(warmed, args.entity_limit));
                    }
                    return tool_json(json!({
                        "updated": false,
                        "sequence": self.data_source.latest_sequence().map_err(tool_error)?,
                    }));
                }
            }
        } else {
            self.wait_for_nonzero_snapshot()?
        };

        tool_json(SnapshotEnvelope::updated(snapshot, args.entity_limit))
    }

    fn tool_host_summary(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize)]
        struct Args {
            #[serde(default = "default_top_entities")]
            top_entities: usize,
        }

        fn default_top_entities() -> usize {
            8
        }

        #[derive(Serialize)]
        struct EntitySummary {
            entity_id: String,
            display_name: String,
            friction: f32,
            cpu_percent: f32,
            memory_bytes: u64,
            badges: Vec<String>,
            recent_change_summary: Option<String>,
        }

        #[derive(Serialize)]
        struct HostSummary {
            sequence: u64,
            captured_at_millis: u64,
            host: aetower_model::HostSnapshot,
            capability_states: BTreeMap<String, String>,
            top_entities: Vec<EntitySummary>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let top_entities = snapshot
            .entities
            .iter()
            .take(args.top_entities.max(1))
            .map(|entity| EntitySummary {
                entity_id: entity.entity_id.clone(),
                display_name: entity.display_name.clone(),
                friction: entity.friction.total_score,
                cpu_percent: entity.metrics.cpu_percent,
                memory_bytes: entity.metrics.memory_resident_bytes,
                badges: entity.badges.clone(),
                recent_change_summary: entity.recent_change_summary.clone(),
            })
            .collect();
        let capability_states = snapshot
            .capabilities
            .iter()
            .map(|capability| {
                (
                    format!("{:?}", capability.kind),
                    format!("{:?}", capability.state),
                )
            })
            .collect();

        tool_json(HostSummary {
            sequence: snapshot.sequence,
            captured_at_millis: snapshot.captured_at_millis,
            host: snapshot.host,
            capability_states,
            top_entities,
        })
    }

    fn tool_entity_details(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize)]
        struct Args {
            entity_id: String,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let entity = snapshot
            .entities
            .into_iter()
            .find(|entity| entity.entity_id == args.entity_id)
            .ok_or_else(|| tool_error(format!("Unknown entity_id: {}", args.entity_id)))?;
        tool_json(entity)
    }

    fn tool_runtime_lag(&self) -> Result<Value, Value> {
        tool_json(
            self.data_source
                .latest_runtime_lag_metrics()
                .map_err(tool_error)?,
        )
    }

    fn tool_diff_snapshots(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            before_millis: u64,
            after_millis: u64,
            #[serde(default)]
            entity_ids: Vec<String>,
            #[serde(default = "default_findings_limit")]
            limit: usize,
        }

        let args: Args = parse_args(arguments)?;
        let before = snapshot_at_or_before(&*self.data_source, args.before_millis)
            .map_err(tool_error)?
            .ok_or_else(|| {
                tool_error(format!(
                    "No persisted snapshot found at or before {}.",
                    args.before_millis
                ))
            })?;
        let after = snapshot_at_or_before(&*self.data_source, args.after_millis)
            .map_err(tool_error)?
            .ok_or_else(|| {
                tool_error(format!(
                    "No persisted snapshot found at or before {}.",
                    args.after_millis
                ))
            })?;
        tool_json(build_snapshot_diff_report(
            &before,
            &after,
            &args.entity_ids,
            args.limit.max(1),
        ))
    }

    fn tool_reboot_report(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            start_millis: Option<u64>,
            #[serde(default)]
            end_millis: Option<u64>,
        }

        let args: Args = parse_args(arguments)?;
        let latest = self.wait_for_nonzero_snapshot()?;
        let end_millis = args.end_millis.unwrap_or(latest.captured_at_millis);
        let start_millis = args
            .start_millis
            .unwrap_or_else(|| end_millis.saturating_sub(DEFAULT_HISTORY_WINDOW_MILLIS));
        tool_json(
            build_reboot_report(
                &*self.data_source,
                start_millis.min(end_millis),
                start_millis.max(end_millis),
            )
            .map_err(tool_error)?,
        )
    }

    fn tool_explain_anomalies(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            entity_ids: Vec<String>,
            #[serde(default = "default_findings_limit")]
            limit: usize,
            #[serde(default = "default_recent_changes_window_minutes")]
            window_minutes: u64,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            explanations: Vec<AnomalyExplanation>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let explanations = build_anomaly_explanations(
            &snapshot,
            &args.entity_ids,
            args.limit.max(1),
            args.window_minutes.saturating_mul(60 * 1000),
        );
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            explanations,
        })
    }

    fn tool_entity_process_tree(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let report = build_process_tree_report(&snapshot, &args.entity_id).map_err(tool_error)?;
        tool_json(report)
    }

    fn tool_top_findings(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_findings_limit")]
            limit: usize,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            findings: Vec<TopFinding>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let diagnostics = self
            .data_source
            .diagnostics_overview()
            .map_err(tool_error)?;
        let history = self
            .data_source
            .history_range_summary(
                snapshot
                    .captured_at_millis
                    .saturating_sub(DEFAULT_HISTORY_WINDOW_MILLIS),
                snapshot.captured_at_millis,
            )
            .map_err(tool_error)?;
        let findings = build_top_findings(&snapshot, &diagnostics, &history, args.limit);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            findings,
        })
    }

    fn tool_host_alerts(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_host_alert_top_entities")]
            top_entities: usize,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            alerts: Vec<HostAlert>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            alerts: build_host_alerts(&snapshot, args.top_entities),
        })
    }

    fn tool_investigation_bundle(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_investigation_window_minutes")]
            window_minutes: u64,
            #[serde(default)]
            start_millis: Option<u64>,
            #[serde(default)]
            end_millis: Option<u64>,
            #[serde(default)]
            entity_ids: Vec<String>,
            #[serde(default = "default_findings_limit")]
            findings_limit: usize,
            #[serde(default = "default_investigation_entity_limit")]
            entity_limit: usize,
            #[serde(default = "default_investigation_diagnostics_limit")]
            diagnostics_limit: usize,
            #[serde(default = "default_include_true")]
            include_process_trees: bool,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let requested_end_millis = args.end_millis.unwrap_or(snapshot.captured_at_millis);
        let requested_start_millis = args.start_millis.unwrap_or_else(|| {
            requested_end_millis
                .saturating_sub(args.window_minutes.max(1).saturating_mul(60 * 1000))
        });
        let window_start_millis = requested_start_millis.min(requested_end_millis);
        let window_end_millis = requested_start_millis.max(requested_end_millis);
        let window_minutes =
            ((window_end_millis.saturating_sub(window_start_millis)) / (60 * 1000)).max(1);
        let mut caveats = Vec::new();

        let diagnostics_overview = match self.data_source.diagnostics_overview() {
            Ok(overview) => overview,
            Err(error) => {
                caveats.push(format!("Diagnostics overview unavailable: {error}"));
                DiagnosticsOverview::default()
            }
        };
        let history_summary = match self
            .data_source
            .history_range_summary(window_start_millis, window_end_millis)
        {
            Ok(summary) => summary,
            Err(error) => {
                caveats.push(format!("History summary unavailable: {error}"));
                empty_history_summary()
            }
        };
        let mut diagnostics = match self.data_source.query_diagnostics(DiagnosticsQuery {
            limit: args.diagnostics_limit.max(1),
            minimum_level: None,
            subsystem: None,
            search: None,
            since_millis: Some(window_start_millis),
            include_persisted: true,
        }) {
            Ok(events) => events,
            Err(error) => {
                caveats.push(format!("Diagnostics events unavailable: {error}"));
                Vec::new()
            }
        };
        diagnostics.retain(|event| event.timestamp_millis <= window_end_millis);
        diagnostics.sort_by(|left, right| {
            right
                .timestamp_millis
                .cmp(&left.timestamp_millis)
                .then(left.id.cmp(&right.id))
        });
        diagnostics.truncate(args.diagnostics_limit.max(1));

        let host_alerts = build_host_alerts(&snapshot, DEFAULT_HOST_ALERT_TOP_ENTITIES);
        let top_findings = build_top_findings(
            &snapshot,
            &diagnostics_overview,
            &history_summary,
            args.findings_limit.max(1),
        );
        let entity_ids = select_investigation_entity_ids(
            &snapshot,
            &args.entity_ids,
            &host_alerts,
            args.entity_limit.max(1),
        );
        let recent_changes = build_recent_changes(
            &snapshot,
            window_end_millis.saturating_sub(window_start_millis),
            DEFAULT_RECENT_CHANGES_LIMIT,
        );

        let history_snapshots = match load_history_snapshots(
            &*self.data_source,
            window_start_millis,
            window_end_millis,
            DEFAULT_INVESTIGATION_HISTORY_LIMIT,
        ) {
            Ok(snapshots) => snapshots,
            Err(error) => {
                caveats.push(format!("Persisted history unavailable: {error}"));
                Vec::new()
            }
        };
        let history_diff = investigation_history_diff(
            &snapshot,
            &history_snapshots,
            &entity_ids,
            args.findings_limit.max(1),
            &diagnostics,
            &mut caveats,
        );

        let mut process_trees = Vec::new();
        if args.include_process_trees {
            for entity_id in &entity_ids {
                match build_process_tree_report(&snapshot, entity_id) {
                    Ok(report) => process_trees.push(report),
                    Err(error) => caveats.push(format!(
                        "Process tree unavailable for entity {entity_id}: {error}"
                    )),
                }
            }
        }

        tool_json(InvestigationBundleReport {
            captured_at_millis: snapshot.captured_at_millis,
            window_start_millis,
            window_end_millis,
            window_minutes,
            entity_ids,
            host_alerts,
            top_findings,
            recent_changes,
            diagnostics,
            history_diff,
            process_trees,
            caveats,
        })
    }

    fn tool_entity_group_tree(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            root_entity_id: String,
            grouped_entity_count: usize,
            grouped_process_count: u32,
            relations: Vec<String>,
            tree: GroupTreeNode,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let root = snapshot
            .entities
            .iter()
            .find(|entity| entity.entity_id == args.entity_id)
            .cloned()
            .ok_or_else(|| tool_error(format!("Unknown entity_id: {}", args.entity_id)))?;
        let (children, relations, grouped_process_count) = related_entity_nodes(&snapshot, &root);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            root_entity_id: root.entity_id.clone(),
            grouped_entity_count: children.len() + 1,
            grouped_process_count,
            relations,
            tree: GroupTreeNode {
                entity_id: root.entity_id,
                display_name: root.display_name,
                relation: "selected-root".to_owned(),
                friction: root.friction.total_score,
                cpu_percent: root.metrics.cpu_percent,
                memory_bytes: root.metrics.memory_resident_bytes,
                process_count: root.metrics.process_count,
                badges: root.badges,
                recent_change_summary: root.recent_change_summary,
                children,
            },
        })
    }

    fn tool_ai_runtime_report(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_history_window_hours")]
            history_window_hours: u64,
            #[serde(default = "default_export_history_limit")]
            history_limit: u32,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            history_status: &'static str,
            history_warning: Option<String>,
            summary: AiRuntimeSummary,
            burden_leaders: Vec<AiBurdenLeaderReport>,
            runtime_groups: Vec<AiRuntimeGroupReport>,
            approvals: Vec<AiApprovalReport>,
            delegations: Vec<AiDelegationReport>,
            recent_changes: Vec<RecentChangeItem>,
            historical_groups: Vec<AiHistoricalTrendReport>,
            recommendations: Vec<RecommendationItem>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let history_start = snapshot
            .captured_at_millis
            .saturating_sub(args.history_window_hours.saturating_mul(60 * 60 * 1000));
        let (history, history_warning) = load_ai_runtime_history(
            self.data_source.as_ref(),
            history_start,
            snapshot.captured_at_millis,
            args.history_limit,
        );
        let report = build_ai_runtime_report(&snapshot, &history);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            history_status: if history_warning.is_some() {
                "degraded"
            } else {
                "ok"
            },
            history_warning,
            summary: report.summary,
            burden_leaders: report.burden_leaders,
            runtime_groups: report.runtime_groups,
            approvals: report.approvals,
            delegations: report.delegations,
            recent_changes: report.recent_changes,
            historical_groups: report.historical_groups,
            recommendations: report.recommendations,
        })
    }

    fn tool_recent_changes(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_recent_changes_window_minutes")]
            window_minutes: u64,
            #[serde(default = "default_recent_changes_limit")]
            limit: usize,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            window_minutes: u64,
            changes: Vec<RecentChangeItem>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let changes = build_recent_changes(
            &snapshot,
            args.window_minutes.saturating_mul(60 * 1000),
            args.limit.max(1),
        );
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            window_minutes: args.window_minutes,
            changes,
        })
    }

    fn tool_capability_status(&self) -> Result<Value, Value> {
        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            counts: BTreeMap<String, usize>,
            capabilities: Vec<CapabilityStatusItem>,
        }

        let snapshot = self.wait_for_nonzero_snapshot()?;
        let capabilities = build_capability_status(&snapshot);
        let mut counts = BTreeMap::new();
        counts.insert(
            "critical".to_owned(),
            capabilities
                .iter()
                .filter(|capability| capability.severity == SeverityBand::Critical)
                .count(),
        );
        counts.insert(
            "warning".to_owned(),
            capabilities
                .iter()
                .filter(|capability| capability.severity == SeverityBand::Warning)
                .count(),
        );
        counts.insert(
            "ok".to_owned(),
            capabilities
                .iter()
                .filter(|capability| capability.severity == SeverityBand::Info)
                .count(),
        );
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            counts,
            capabilities,
        })
    }

    fn tool_history_summary(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize)]
        struct Args {
            start_millis: u64,
            end_millis: u64,
        }

        let args: Args = parse_args(arguments)?;
        let summary = self
            .data_source
            .history_range_summary(args.start_millis, args.end_millis)
            .map_err(tool_error)?;
        tool_json(summary)
    }

    fn tool_history_page(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize)]
        struct Args {
            start_millis: u64,
            end_millis: u64,
            before_millis_exclusive: Option<u64>,
            limit: Option<u32>,
        }

        #[derive(Serialize)]
        struct HistoryPageResponse {
            snapshots: Vec<aetower_model::SystemSnapshot>,
            next_before_millis_exclusive: Option<u64>,
            returned: usize,
        }

        let args: Args = parse_args(arguments)?;
        let snapshots = self
            .data_source
            .load_history_page(
                args.start_millis,
                args.end_millis,
                args.before_millis_exclusive,
                args.limit.unwrap_or(120),
            )
            .map_err(tool_error)?;
        let next_before_millis_exclusive = older_page_cursor(&snapshots);
        tool_json(HistoryPageResponse {
            returned: snapshots.len(),
            snapshots,
            next_before_millis_exclusive,
        })
    }

    fn tool_diagnostics_overview(&self) -> Result<Value, Value> {
        tool_json(
            self.data_source
                .diagnostics_overview()
                .map_err(tool_error)?,
        )
    }

    fn tool_diagnostics_summary(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            limit: Option<usize>,
            query_limit: Option<usize>,
            since_millis: Option<u64>,
            include_persisted: Option<bool>,
            minimum_level: Option<DiagnosticsLevel>,
            subsystem: Option<aetower_diagnostics::DiagnosticsSubsystem>,
            search: Option<String>,
        }

        let args: Args = parse_args(arguments)?;
        let limit = args.limit.unwrap_or(DEFAULT_DIAGNOSTICS_SUMMARY_LIMIT);
        let query_limit = args
            .query_limit
            .unwrap_or(DEFAULT_DIAGNOSTICS_SUMMARY_QUERY_LIMIT);
        let include_persisted = args.include_persisted.unwrap_or(true);
        let query = DiagnosticsQuery {
            limit: query_limit,
            minimum_level: args.minimum_level.clone(),
            subsystem: args.subsystem.clone(),
            search: args.search.clone(),
            since_millis: args.since_millis,
            include_persisted,
        };
        let overview = self
            .data_source
            .diagnostics_overview()
            .map_err(tool_error)?;
        let events = self
            .data_source
            .query_diagnostics(query)
            .map_err(tool_error)?;
        tool_json(build_diagnostics_summary_report(
            overview,
            events,
            DiagnosticsSummaryOptions {
                limit,
                since_millis: args.since_millis,
                include_persisted,
                minimum_level: args.minimum_level,
                subsystem: args.subsystem,
                search: args.search,
            },
        ))
    }

    fn tool_query_diagnostics(&self, arguments: Value) -> Result<Value, Value> {
        let query: DiagnosticsQuery = parse_args(arguments)?;
        tool_json(
            self.data_source
                .query_diagnostics(query)
                .map_err(tool_error)?,
        )
    }

    fn tool_history_store_health(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_history_window_hours")]
            window_hours: u64,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let start_millis = snapshot
            .captured_at_millis
            .saturating_sub(args.window_hours.saturating_mul(60 * 60 * 1000));
        let summary = self
            .data_source
            .history_range_summary(start_millis, snapshot.captured_at_millis)
            .map_err(tool_error)?;
        let recent_history_events = self
            .data_source
            .query_diagnostics(DiagnosticsQuery {
                limit: 32,
                minimum_level: Some(aetower_diagnostics::DiagnosticsLevel::Info),
                subsystem: Some(aetower_diagnostics::DiagnosticsSubsystem::History),
                search: None,
                since_millis: Some(start_millis),
                include_persisted: true,
            })
            .map_err(tool_error)?;
        let data_quality = load_history_snapshots_raw(
            &*self.data_source,
            start_millis,
            snapshot.captured_at_millis,
            DEFAULT_HISTORY_QUALITY_MAX_SNAPSHOTS,
        )
        .ok()
        .map(|snapshots| {
            build_history_data_quality_report(
                &snapshots,
                start_millis,
                snapshot.captured_at_millis,
                DEFAULT_HISTORY_EXPECTED_INTERVAL_MILLIS,
            )
        });
        tool_json(build_history_store_health(
            summary,
            recent_history_events,
            data_quality,
        ))
    }

    fn tool_history_data_quality(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_history_window_hours")]
            window_hours: u64,
            #[serde(default = "default_history_expected_interval_millis")]
            expected_interval_millis: u64,
            #[serde(default = "default_history_quality_max_snapshots")]
            max_snapshots: usize,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let start_millis = snapshot
            .captured_at_millis
            .saturating_sub(args.window_hours.saturating_mul(60 * 60 * 1000));
        let snapshots = load_history_snapshots_raw(
            &*self.data_source,
            start_millis,
            snapshot.captured_at_millis,
            args.max_snapshots.max(1),
        )
        .map_err(tool_error)?;
        tool_json(build_history_data_quality_report(
            &snapshots,
            start_millis,
            snapshot.captured_at_millis,
            args.expected_interval_millis.max(1),
        ))
    }

    fn tool_memory_breakdown(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
            #[serde(default = "default_top_regions")]
            top_regions: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::MemoryBreakdown {
            entity_id: args.entity_id,
            top_regions: args.top_regions.max(1),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    fn tool_profile_entity(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
            #[serde(default = "default_profile_duration_seconds")]
            duration_seconds: u64,
            #[serde(default = "default_top_stacks")]
            top_stacks: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProfileEntity {
            entity_id: args.entity_id,
            duration_seconds: args.duration_seconds.clamp(1, MAX_PROFILE_DURATION_SECONDS),
            top_stacks: args.top_stacks.max(1),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    fn tool_wakeup_attribution(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
            #[serde(default = "default_profile_duration_seconds")]
            duration_seconds: u64,
            #[serde(default = "default_top_stacks")]
            top_stacks: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::WakeupAttribution {
            entity_id: args.entity_id,
            duration_seconds: args.duration_seconds.clamp(1, MAX_PROFILE_DURATION_SECONDS),
            top_stacks: args.top_stacks.max(1),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    fn tool_process_inspect(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            pid: u32,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessInspect { pid: args.pid };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    fn tool_process_open_resources(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            pid: u32,
            #[serde(default = "default_open_resource_limit")]
            limit: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessOpenResources {
            pid: args.pid,
            limit: args.limit.clamp(1, 500),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    fn tool_process_sample(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            pid: u32,
            #[serde(default = "default_profile_duration_seconds")]
            duration_seconds: u64,
            #[serde(default = "default_top_stacks")]
            top_stacks: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessSample {
            pid: args.pid,
            duration_seconds: args.duration_seconds.clamp(1, MAX_PROFILE_DURATION_SECONDS),
            top_stacks: args.top_stacks.max(1),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    fn tool_process_action(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            pid: u32,
            action: String,
            #[serde(default = "default_include_true")]
            dry_run: bool,
            #[serde(default)]
            reason: Option<String>,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessAction {
            pid: args.pid,
            action: args.action,
            dry_run: args.dry_run,
            reason: args.reason,
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    fn tool_process_action_history(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_process_action_history_window_minutes")]
            window_minutes: u64,
            #[serde(default = "default_process_action_history_limit")]
            limit: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessActionHistory {
            window_minutes: args.window_minutes.max(1),
            limit: args.limit.clamp(1, 200),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    fn tool_support_bundle_manifest(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            privacy_tier: Option<ExportPrivacyTier>,
            #[serde(default = "default_support_bundle_diagnostics_limit")]
            diagnostics_limit: usize,
            #[serde(default = "default_history_window_hours")]
            history_window_hours: u64,
        }

        #[derive(Serialize)]
        struct Response {
            privacy_tier: ExportPrivacyTier,
            total_estimated_bytes: usize,
            sections: Vec<SupportBundleSectionManifest>,
            redaction_notes: Vec<String>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let diagnostics = self
            .data_source
            .diagnostics_overview()
            .map_err(tool_error)?;
        let runtime_lag = self
            .data_source
            .latest_runtime_lag_metrics()
            .map_err(tool_error)?;
        let history = self
            .data_source
            .history_range_summary(
                snapshot
                    .captured_at_millis
                    .saturating_sub(args.history_window_hours.saturating_mul(60 * 60 * 1000)),
                snapshot.captured_at_millis,
            )
            .map_err(tool_error)?;
        let diagnostic_events = self
            .data_source
            .query_diagnostics(DiagnosticsQuery {
                limit: args.diagnostics_limit,
                minimum_level: None,
                subsystem: None,
                search: None,
                since_millis: None,
                include_persisted: true,
            })
            .map_err(tool_error)?;
        let tier = args.privacy_tier.unwrap_or(ExportPrivacyTier::Redacted);
        let manifest = build_support_bundle_manifest(
            tier,
            snapshot,
            runtime_lag,
            diagnostics,
            history,
            diagnostic_events,
        )?;
        tool_json(Response {
            privacy_tier: tier,
            total_estimated_bytes: manifest.iter().map(|section| section.estimated_bytes).sum(),
            sections: manifest,
            redaction_notes: privacy_tier_notes(tier),
        })
    }

    fn tool_recommendations(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_findings_limit")]
            limit: usize,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            recommendations: Vec<RecommendationItem>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let diagnostics = self
            .data_source
            .diagnostics_overview()
            .map_err(tool_error)?;
        let runtime = self
            .data_source
            .latest_runtime_lag_metrics()
            .map_err(tool_error)?;
        let history = self
            .data_source
            .history_range_summary(
                snapshot
                    .captured_at_millis
                    .saturating_sub(DEFAULT_HISTORY_WINDOW_MILLIS),
                snapshot.captured_at_millis,
            )
            .map_err(tool_error)?;
        let recommendations =
            build_recommendations(&snapshot, &diagnostics, &runtime, &history, args.limit);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            recommendations,
        })
    }

    fn tool_session_health(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_history_window_hours")]
            history_window_hours: u64,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            overall: SeverityBand,
            checks: Vec<SessionHealthCheck>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let diagnostics = self
            .data_source
            .diagnostics_overview()
            .map_err(tool_error)?;
        let runtime = self
            .data_source
            .latest_runtime_lag_metrics()
            .map_err(tool_error)?;
        let history = self
            .data_source
            .history_range_summary(
                snapshot
                    .captured_at_millis
                    .saturating_sub(args.history_window_hours.saturating_mul(60 * 60 * 1000)),
                snapshot.captured_at_millis,
            )
            .map_err(tool_error)?;
        let checks = build_session_health_checks(&snapshot, &diagnostics, &runtime, &history);
        let overall = checks
            .iter()
            .map(|check| check.severity)
            .max_by_key(|severity| severity.score())
            .unwrap_or(SeverityBand::Info);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            overall,
            checks,
        })
    }

    fn tool_export_query(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            privacy_tier: Option<ExportPrivacyTier>,
            #[serde(default)]
            entity_ids: Vec<String>,
            #[serde(default)]
            start_millis: Option<u64>,
            #[serde(default)]
            end_millis: Option<u64>,
            #[serde(default)]
            include_history: bool,
            #[serde(default = "default_include_true")]
            include_snapshot: bool,
            #[serde(default)]
            include_diagnostics: bool,
            #[serde(default)]
            include_session_health: bool,
            #[serde(default)]
            include_ai_runtime_report: bool,
            #[serde(default = "default_support_bundle_diagnostics_limit")]
            diagnostics_limit: usize,
            #[serde(default = "default_export_history_limit")]
            history_limit: u32,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let end_millis = args.end_millis.unwrap_or(snapshot.captured_at_millis);
        let start_millis = args
            .start_millis
            .unwrap_or(end_millis.saturating_sub(DEFAULT_HISTORY_WINDOW_MILLIS));
        let tier = args.privacy_tier.unwrap_or(ExportPrivacyTier::Redacted);
        let export = build_export_query_response(
            &*self.data_source,
            snapshot,
            ExportQueryOptions {
                privacy_tier: tier,
                entity_ids: &args.entity_ids,
                start_millis,
                end_millis,
                include_snapshot: args.include_snapshot,
                include_history: args.include_history,
                include_diagnostics: args.include_diagnostics,
                include_session_health: args.include_session_health,
                include_ai_runtime_report: args.include_ai_runtime_report,
                diagnostics_limit: args.diagnostics_limit,
                history_limit: args.history_limit,
            },
        )
        .map_err(tool_error)?;
        tool_json(export)
    }

    fn wait_for_nonzero_snapshot(&self) -> Result<aetower_model::SystemSnapshot, Value> {
        let started = Instant::now();
        loop {
            let snapshot = self.data_source.latest_snapshot().map_err(tool_error)?;
            if snapshot.sequence > 0 || started.elapsed() >= INITIAL_SNAPSHOT_WAIT {
                return Ok(snapshot);
            }
            thread::sleep(INITIAL_SNAPSHOT_POLL);
        }
    }

    fn execute_dynamic_request(&self, request: &DynamicToolRequest) -> Result<Value, String> {
        match &self.dynamic_mode {
            DynamicExecutionMode::Local => {
                process_dynamic_tool_request(&*self.data_source, request)
            }
        }
    }
}

#[derive(Serialize)]
struct SnapshotEnvelope {
    updated: bool,
    sequence: u64,
    snapshot: aetower_model::SystemSnapshot,
}

impl SnapshotEnvelope {
    fn updated(mut snapshot: aetower_model::SystemSnapshot, entity_limit: Option<usize>) -> Self {
        if let Some(limit) = entity_limit {
            snapshot.entities.truncate(limit.max(1));
        }
        Self {
            updated: true,
            sequence: snapshot.sequence,
            snapshot,
        }
    }
}

fn tool_json<T: Serialize>(value: T) -> Result<Value, Value> {
    let structured = serde_json::to_value(&value)
        .map_err(|error| tool_error(format!("serialize structured content: {error}")))?;
    let compact = serde_json::to_vec(&structured)
        .map_err(|error| tool_error(format!("serialize tool result: {error}")))?;
    let text = if compact.len() <= MAX_INLINE_TEXT_BYTES {
        serde_json::to_string_pretty(&structured)
            .map_err(|error| tool_error(format!("serialize tool result: {error}")))?
    } else {
        format!("Structured result attached ({} bytes JSON).", compact.len())
    };
    Ok(json!({
        "structuredContent": structured,
        "content": [
            {
                "type": "text",
                "text": text,
            }
        ]
    }))
}

fn default_findings_limit() -> usize {
    DEFAULT_FINDINGS_LIMIT
}

fn default_host_alert_top_entities() -> usize {
    DEFAULT_HOST_ALERT_TOP_ENTITIES
}

fn default_recent_changes_limit() -> usize {
    DEFAULT_RECENT_CHANGES_LIMIT
}

fn default_recent_changes_window_minutes() -> u64 {
    DEFAULT_RECENT_CHANGES_WINDOW_MILLIS / (60 * 1000)
}

fn default_investigation_window_minutes() -> u64 {
    DEFAULT_INVESTIGATION_WINDOW_MINUTES
}

fn default_investigation_entity_limit() -> usize {
    DEFAULT_INVESTIGATION_ENTITY_LIMIT
}

fn default_investigation_diagnostics_limit() -> usize {
    DEFAULT_INVESTIGATION_DIAGNOSTICS_LIMIT
}

fn default_history_expected_interval_millis() -> u64 {
    DEFAULT_HISTORY_EXPECTED_INTERVAL_MILLIS
}

fn default_history_quality_max_snapshots() -> usize {
    DEFAULT_HISTORY_QUALITY_MAX_SNAPSHOTS
}

fn default_history_window_hours() -> u64 {
    DEFAULT_HISTORY_WINDOW_MILLIS / (60 * 60 * 1000)
}

fn default_support_bundle_diagnostics_limit() -> usize {
    1_500
}

fn default_export_history_limit() -> u32 {
    DEFAULT_EXPORT_HISTORY_LIMIT
}

fn default_profile_duration_seconds() -> u64 {
    DEFAULT_PROFILE_DURATION_SECONDS
}

fn default_top_stacks() -> usize {
    DEFAULT_TOP_STACKS
}

fn default_top_regions() -> usize {
    DEFAULT_TOP_REGIONS
}

fn default_open_resource_limit() -> usize {
    DEFAULT_OPEN_RESOURCE_LIMIT
}

fn default_process_action_history_limit() -> usize {
    DEFAULT_PROCESS_ACTION_HISTORY_LIMIT
}

fn default_process_action_history_window_minutes() -> u64 {
    DEFAULT_PROCESS_ACTION_HISTORY_WINDOW_MINUTES
}

fn default_include_true() -> bool {
    true
}

fn empty_history_summary() -> HistorySummaryResponse {
    HistorySummaryResponse {
        store_bytes: 0,
        wal_bytes: 0,
        snapshot_count: 0,
        quarantine_count: 0,
        range_count: 0,
        oldest_millis: None,
        newest_millis: None,
        pending_writes: 0,
    }
}

fn snapshot_at_or_before(
    data_source: &dyn AetowerMcpDataSource,
    target_millis: u64,
) -> Result<Option<SystemSnapshot>, String> {
    let mut snapshots = data_source.load_history_page(0, target_millis, None, 1)?;
    Ok(snapshots.pop())
}

fn metric_delta(before: f64, after: f64) -> SnapshotMetricDelta {
    let delta = after - before;
    let percent_change = if before.abs() < f64::EPSILON {
        None
    } else {
        Some((delta / before.abs()) * 100.0)
    };
    SnapshotMetricDelta {
        before,
        after,
        delta,
        percent_change,
    }
}

fn build_snapshot_diff_report(
    before: &SystemSnapshot,
    after: &SystemSnapshot,
    requested_entity_ids: &[String],
    limit: usize,
) -> SnapshotDiffReport {
    build_snapshot_diff_report_with_diagnostics(before, after, requested_entity_ids, limit, &[])
}

fn build_snapshot_diff_report_with_diagnostics(
    before: &SystemSnapshot,
    after: &SystemSnapshot,
    requested_entity_ids: &[String],
    limit: usize,
    diagnostics: &[DiagnosticsEvent],
) -> SnapshotDiffReport {
    let before_boot = before.host.boot_session.as_ref();
    let after_boot = after.host.boot_session.as_ref();
    let before_boot_id = before_boot.and_then(|boot| boot.boot_id.clone());
    let after_boot_id = after_boot.and_then(|boot| boot.boot_id.clone());
    let before_boot_time_millis = before_boot.and_then(|boot| boot.boot_time_millis);
    let after_boot_time_millis = after_boot.and_then(|boot| boot.boot_time_millis);
    let crossed_boot_boundary =
        before_boot_id != after_boot_id || before_boot_time_millis != after_boot_time_millis;
    let mut host = BTreeMap::new();
    host.insert(
        "cpu_percent".to_owned(),
        metric_delta(
            before.host.cpu_percent as f64,
            after.host.cpu_percent as f64,
        ),
    );
    host.insert(
        "memory_used_bytes".to_owned(),
        metric_delta(
            before.host.memory_used_bytes as f64,
            after.host.memory_used_bytes as f64,
        ),
    );
    host.insert(
        "compressed_memory_bytes".to_owned(),
        metric_delta(
            before.host.compressed_memory_bytes as f64,
            after.host.compressed_memory_bytes as f64,
        ),
    );
    host.insert(
        "swap_used_bytes".to_owned(),
        metric_delta(
            before.host.swap_used_bytes as f64,
            after.host.swap_used_bytes as f64,
        ),
    );
    host.insert(
        "wakeups_per_second".to_owned(),
        metric_delta(
            before.host.wakeups_per_second as f64,
            after.host.wakeups_per_second as f64,
        ),
    );

    let before_entities = before
        .entities
        .iter()
        .map(|entity| (entity.entity_id.clone(), entity))
        .collect::<BTreeMap<_, _>>();
    let after_entities = after
        .entities
        .iter()
        .map(|entity| (entity.entity_id.clone(), entity))
        .collect::<BTreeMap<_, _>>();
    let entity_ids = if requested_entity_ids.is_empty() {
        before_entities
            .keys()
            .chain(after_entities.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        requested_entity_ids.to_vec()
    };
    let mut entities = entity_ids
        .into_iter()
        .filter_map(|entity_id| {
            let before_entity = before_entities.get(&entity_id).copied();
            let after_entity = after_entities.get(&entity_id).copied();
            let display_name = after_entity
                .or(before_entity)
                .map(|entity| entity.display_name.clone())?;
            let before_metrics = before_entity.map(|entity| &entity.metrics);
            let after_metrics = after_entity.map(|entity| &entity.metrics);
            Some(SnapshotEntityDelta {
                entity_id: entity_id.clone(),
                display_name,
                before_present: before_entity.is_some(),
                after_present: after_entity.is_some(),
                friction: metric_delta(
                    before_entity
                        .map(|entity| entity.friction.total_score as f64)
                        .unwrap_or(0.0),
                    after_entity
                        .map(|entity| entity.friction.total_score as f64)
                        .unwrap_or(0.0),
                ),
                cpu_percent: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.cpu_percent as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.cpu_percent as f64)
                        .unwrap_or(0.0),
                ),
                memory_bytes: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.memory_resident_bytes as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.memory_resident_bytes as f64)
                        .unwrap_or(0.0),
                ),
                memory_physical_footprint_bytes: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.memory_physical_footprint_bytes)
                        .map(|value| value as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.memory_physical_footprint_bytes)
                        .map(|value| value as f64)
                        .unwrap_or(0.0),
                ),
                wakeups_per_second: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.wakeups_per_second as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.wakeups_per_second as f64)
                        .unwrap_or(0.0),
                ),
                process_count: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.process_count as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.process_count as f64)
                        .unwrap_or(0.0),
                ),
                recent_change_summary: after_entity
                    .and_then(|entity| entity.recent_change_summary.clone())
                    .or_else(|| {
                        before_entity.and_then(|entity| entity.recent_change_summary.clone())
                    }),
            })
        })
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        right
            .friction
            .delta
            .abs()
            .partial_cmp(&left.friction.delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .wakeups_per_second
                    .delta
                    .abs()
                    .partial_cmp(&left.wakeups_per_second.delta.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    if requested_entity_ids.is_empty() {
        entities.truncate(limit.max(1));
    }

    let cpu_delta = host
        .get("cpu_percent")
        .map(|metric| format_signed_decimal(metric.delta, "%"))
        .unwrap_or_else(|| "n/a".to_owned());
    let memory_delta = host
        .get("memory_used_bytes")
        .map(|metric| format_signed_bytes(metric.delta))
        .unwrap_or_else(|| "n/a".to_owned());
    let wakeup_delta = host
        .get("wakeups_per_second")
        .map(|metric| format_signed_rate(metric.delta))
        .unwrap_or_else(|| "n/a".to_owned());
    let host_summary = if crossed_boot_boundary {
        format!(
            "Crossed a reboot boundary. CPU {}, memory {}, wakeups {} across the selected window.",
            cpu_delta, memory_delta, wakeup_delta
        )
    } else {
        format!(
            "CPU {}, memory {}, wakeups {} across the selected window.",
            cpu_delta, memory_delta, wakeup_delta
        )
    };

    let appeared = entities
        .iter()
        .filter(|entity| !entity.before_present && entity.after_present)
        .count();
    let disappeared = entities
        .iter()
        .filter(|entity| entity.before_present && !entity.after_present)
        .count();
    let changed = entities
        .iter()
        .filter(|entity| entity.before_present && entity.after_present)
        .count();
    let entity_summary = if let Some(top_entity) = entities.first() {
        format!(
            "{} changed, {} appeared, {} disappeared. Largest shift: {} (friction {}).",
            changed,
            appeared,
            disappeared,
            top_entity.display_name,
            format_signed_decimal(top_entity.friction.delta, "")
        )
    } else {
        "No entity deltas were available for the selected window.".to_owned()
    };

    SnapshotDiffReport {
        before_snapshot_millis: before.captured_at_millis,
        after_snapshot_millis: after.captured_at_millis,
        crossed_boot_boundary,
        before_boot_id,
        after_boot_id,
        before_boot_time_millis,
        after_boot_time_millis,
        before_previous_shutdown: before_boot
            .and_then(|boot| boot.previous_shutdown.clone())
            .or_else(|| {
                extract_previous_shutdown_from_events(
                    diagnostics,
                    before.captured_at_millis.saturating_sub(30 * 60 * 1000),
                    before.captured_at_millis.saturating_add(5 * 60 * 1000),
                )
            }),
        after_previous_shutdown: after_boot
            .and_then(|boot| boot.previous_shutdown.clone())
            .or_else(|| {
                extract_previous_shutdown_from_events(
                    diagnostics,
                    after.captured_at_millis.saturating_sub(30 * 60 * 1000),
                    after.captured_at_millis.saturating_add(5 * 60 * 1000),
                )
            }),
        summary: SnapshotDiffSummary {
            host_summary,
            entity_summary,
        },
        host,
        entities,
    }
}

fn format_signed_decimal(value: f64, suffix: &str) -> String {
    let formatted = format!("{:.1}{suffix}", value.abs());
    if value > 0.0 {
        format!("+{formatted}")
    } else if value < 0.0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

fn format_signed_bytes(value: f64) -> String {
    let bytes = value.abs().round() as u64;
    let formatted = format_bytes(bytes);
    if value > 0.0 {
        format!("+{formatted}")
    } else if value < 0.0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

fn format_signed_rate(value: f64) -> String {
    let formatted = format!("{:.0}/s", value.abs());
    if value > 0.0 {
        format!("+{formatted}")
    } else if value < 0.0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

fn build_reboot_report(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
) -> Result<RebootReport, String> {
    let snapshots = load_history_snapshots(data_source, start_millis, end_millis, 2048)?;
    if snapshots.is_empty() {
        return Err(
            "No persisted snapshots were available for that reboot report window.".to_owned(),
        );
    }
    let mut sessions = Vec::<Vec<SystemSnapshot>>::new();
    for snapshot in snapshots {
        let key = snapshot_boot_key(&snapshot.host);
        match sessions.last_mut() {
            Some(current)
                if current
                    .last()
                    .map(|last| snapshot_boot_key(&last.host) == key)
                    .unwrap_or(false) =>
            {
                current.push(snapshot);
            }
            _ => sessions.push(vec![snapshot]),
        }
    }

    let diagnostics = load_reboot_diagnostics(
        data_source,
        start_millis.saturating_sub(60 * 60 * 1000),
        end_millis.saturating_add(30 * 60 * 1000),
    )?;
    let session_reports = sessions
        .iter()
        .filter_map(|session| build_reboot_session_report(session))
        .collect::<Vec<_>>();
    let mut boundaries = Vec::new();
    for pair in sessions.windows(2) {
        let before = pair.first().and_then(|session| session.last());
        let after = pair.get(1).and_then(|session| session.first());
        if let (Some(before), Some(after)) = (before, after) {
            boundaries.push(build_reboot_boundary_report(before, after, &diagnostics));
        }
    }

    Ok(RebootReport {
        range_start_millis: start_millis,
        range_end_millis: end_millis,
        session_count: session_reports.len(),
        boundary_count: boundaries.len(),
        sessions: session_reports,
        boundaries,
    })
}

pub fn diff_snapshots_json(
    data_source: &dyn AetowerMcpDataSource,
    before_millis: u64,
    after_millis: u64,
    entity_ids: &[String],
    limit: usize,
) -> Result<String, String> {
    let start_millis = before_millis.min(after_millis);
    let end_millis = before_millis.max(after_millis);
    let snapshots = data_source.load_history_page(start_millis, end_millis, None, 512)?;
    if snapshots.is_empty() {
        return Err("No persisted snapshots were available for that comparison window.".to_owned());
    }
    let before = snapshots
        .iter()
        .min_by_key(|snapshot| snapshot.captured_at_millis.abs_diff(before_millis))
        .ok_or_else(|| "Could not resolve the before snapshot.".to_owned())?;
    let after = snapshots
        .iter()
        .min_by_key(|snapshot| snapshot.captured_at_millis.abs_diff(after_millis))
        .ok_or_else(|| "Could not resolve the after snapshot.".to_owned())?;
    let diagnostics = load_reboot_diagnostics(
        data_source,
        start_millis.saturating_sub(60 * 60 * 1000),
        end_millis.saturating_add(30 * 60 * 1000),
    )?;
    let report =
        build_snapshot_diff_report_with_diagnostics(before, after, entity_ids, limit, &diagnostics);
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn load_reboot_diagnostics(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
) -> Result<Vec<DiagnosticsEvent>, String> {
    const SEARCHES: &[(&str, usize)] = &[
        ("boot-session-observed", 256),
        ("system-previous-shutdown-cause", 128),
        ("system-panic-marker", 128),
        ("system-sleep-marker", 256),
        ("system-wake-marker", 256),
        ("system-thermal-marker", 256),
        ("system-power-marker", 256),
        ("engine-initialized", 128),
        ("host-incident-snapshot", 512),
        ("tick-over-budget", 2048),
    ];

    let mut events = Vec::new();
    for (search, limit) in SEARCHES {
        let mut chunk = data_source.query_diagnostics(DiagnosticsQuery {
            limit: *limit,
            search: Some((*search).to_owned()),
            since_millis: Some(start_millis),
            include_persisted: true,
            ..DiagnosticsQuery::default()
        })?;
        chunk.retain(|event| event.timestamp_millis <= end_millis);
        events.extend(chunk);
    }
    events.sort_by(|left, right| {
        left.timestamp_millis
            .cmp(&right.timestamp_millis)
            .then(left.id.cmp(&right.id))
    });
    events.dedup_by(|left, right| {
        if !left.id.is_empty() && !right.id.is_empty() {
            left.id == right.id
        } else {
            left.timestamp_millis == right.timestamp_millis
                && left.event_type == right.event_type
                && left.message == right.message
        }
    });
    Ok(events)
}

fn load_history_snapshots(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
    max_snapshots: usize,
) -> Result<Vec<SystemSnapshot>, String> {
    let mut snapshots =
        load_history_snapshots_raw(data_source, start_millis, end_millis, max_snapshots)?;
    snapshots.dedup_by(|left, right| {
        left.captured_at_millis == right.captured_at_millis && left.sequence == right.sequence
    });
    Ok(snapshots)
}

fn load_history_snapshots_raw(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
    max_snapshots: usize,
) -> Result<Vec<SystemSnapshot>, String> {
    let mut before_cursor = None;
    let mut snapshots = Vec::new();
    let page_limit = 256u32;
    while snapshots.len() < max_snapshots {
        let page =
            data_source.load_history_page(start_millis, end_millis, before_cursor, page_limit)?;
        if page.is_empty() {
            break;
        }
        let next_before_cursor = older_page_cursor(&page);
        if before_cursor.is_some() && next_before_cursor == before_cursor {
            break;
        }
        snapshots.extend(page);
        let Some(next_before_cursor) = next_before_cursor else {
            break;
        };
        before_cursor = Some(next_before_cursor);
        if snapshots.len() >= max_snapshots {
            break;
        }
    }
    snapshots.sort_by_key(|snapshot| snapshot.captured_at_millis);
    Ok(snapshots)
}

fn older_page_cursor(snapshots: &[SystemSnapshot]) -> Option<u64> {
    snapshots
        .iter()
        .map(|snapshot| snapshot.captured_at_millis)
        .min()
        .and_then(|captured_at| captured_at.checked_sub(1))
}

fn build_history_data_quality_report(
    snapshots: &[SystemSnapshot],
    window_start_millis: u64,
    window_end_millis: u64,
    expected_interval_millis: u64,
) -> HistoryDataQualityReport {
    let expected_interval_millis = expected_interval_millis.max(1);
    let gap_threshold_millis = expected_interval_millis.saturating_mul(HISTORY_DATA_GAP_MULTIPLIER);
    let sampled_snapshots = snapshots.len();
    let oldest_millis = snapshots
        .first()
        .map(|snapshot| snapshot.captured_at_millis);
    let newest_millis = snapshots.last().map(|snapshot| snapshot.captured_at_millis);
    let coverage_millis = newest_millis
        .zip(oldest_millis)
        .map(|(newest, oldest)| newest.saturating_sub(oldest))
        .unwrap_or(0);
    let requested_window_millis = window_end_millis.saturating_sub(window_start_millis);
    let coverage_ratio = if requested_window_millis == 0 {
        0.0
    } else {
        (coverage_millis as f64 / requested_window_millis as f64).clamp(0.0, 1.0)
    };

    let mut timestamp_counts = BTreeMap::<u64, usize>::new();
    for snapshot in snapshots {
        *timestamp_counts
            .entry(snapshot.captured_at_millis)
            .or_default() += 1;
    }
    let duplicate_timestamp_count = timestamp_counts
        .values()
        .map(|count| count.saturating_sub(1))
        .sum();

    let mut largest_gap_millis = 0u64;
    let mut gap_count = 0usize;
    let mut sequence_regression_count = 0usize;
    let mut sequence_reset_count = 0usize;
    let mut boot_boundary_count = 0usize;
    for pair in snapshots.windows(2) {
        let before = &pair[0];
        let after = &pair[1];
        let gap_millis = after
            .captured_at_millis
            .saturating_sub(before.captured_at_millis);
        largest_gap_millis = largest_gap_millis.max(gap_millis);
        if gap_millis > gap_threshold_millis {
            gap_count += 1;
        }
        if after.sequence < before.sequence {
            if gap_millis > gap_threshold_millis {
                sequence_reset_count += 1;
            } else {
                sequence_regression_count += 1;
            }
        }
        if snapshot_boot_key(&before.host) != snapshot_boot_key(&after.host) {
            boot_boundary_count += 1;
        }
    }

    let severity = if sampled_snapshots == 0
        || sequence_regression_count > 0
        || largest_gap_millis >= 60 * 60 * 1000
        || duplicate_timestamp_count >= 64
    {
        SeverityBand::Critical
    } else if sampled_snapshots < 2 || gap_count > 0 || duplicate_timestamp_count > 0 {
        SeverityBand::Warning
    } else {
        SeverityBand::Info
    };

    let mut recommendations = Vec::new();
    if sampled_snapshots == 0 {
        recommendations.push(
            "History has no readable snapshots in this window; verify writer startup, SQLite path, and quarantine diagnostics."
                .to_owned(),
        );
    }
    if gap_count > 0 {
        recommendations.push(format!(
            "Investigate {} history coverage gap(s) above {}.",
            gap_count,
            format_duration_millis(gap_threshold_millis)
        ));
    }
    if duplicate_timestamp_count > 0 {
        recommendations.push(
            "Audit history writer idempotency; duplicate captured_at timestamps should be rare."
                .to_owned(),
        );
    }
    if sequence_regression_count > 0 {
        recommendations.push(
            "Sequence numbers regressed inside a continuous history stream; audit writer ordering and flush concurrency."
                .to_owned(),
        );
    }
    if sequence_reset_count > 0 {
        recommendations.push(
            "Sequence numbers reset after a coverage gap, which usually indicates an app restart; correlate with engine diagnostics before treating it as corruption."
                .to_owned(),
        );
    }
    if recommendations.is_empty() {
        recommendations
            .push("History ordering and coverage look healthy for this window.".to_owned());
    }

    HistoryDataQualityReport {
        severity,
        summary: match severity {
            SeverityBand::Info => format!(
                "{} snapshots sampled; largest gap {}; no ordering issues detected.",
                sampled_snapshots,
                format_duration_millis(largest_gap_millis)
            ),
            SeverityBand::Warning => format!(
                "{} snapshots sampled; {} gap(s), {} duplicate timestamp(s), {} sequence reset(s), largest gap {}.",
                sampled_snapshots,
                gap_count,
                duplicate_timestamp_count,
                sequence_reset_count,
                format_duration_millis(largest_gap_millis)
            ),
            SeverityBand::Critical => format!(
                "{} snapshots sampled; {} in-stream sequence regression(s), {} gap(s), largest gap {}.",
                sampled_snapshots,
                sequence_regression_count,
                gap_count,
                format_duration_millis(largest_gap_millis)
            ),
        },
        window_start_millis,
        window_end_millis,
        sampled_snapshots,
        oldest_millis,
        newest_millis,
        coverage_millis,
        coverage_ratio,
        expected_interval_millis,
        gap_threshold_millis,
        largest_gap_millis,
        gap_count,
        duplicate_timestamp_count,
        sequence_regression_count,
        sequence_reset_count,
        boot_boundary_count,
        recommendations,
    }
}

fn select_investigation_entity_ids(
    snapshot: &SystemSnapshot,
    requested_entity_ids: &[String],
    host_alerts: &[HostAlert],
    limit: usize,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    let mut push_id = |entity_id: &str| {
        if !entity_id.is_empty() && seen.insert(entity_id.to_owned()) {
            selected.push(entity_id.to_owned());
        }
    };

    if requested_entity_ids.is_empty() {
        for alert in host_alerts {
            for entity_id in &alert.entity_ids {
                push_id(entity_id);
            }
        }
        for entity in top_entities(snapshot, limit) {
            push_id(&entity.entity_id);
        }
    } else {
        for entity_id in requested_entity_ids {
            push_id(entity_id);
        }
    }

    selected.truncate(limit.max(1));
    selected
}

fn investigation_history_diff(
    latest: &SystemSnapshot,
    history_snapshots: &[SystemSnapshot],
    entity_ids: &[String],
    limit: usize,
    diagnostics: &[DiagnosticsEvent],
    caveats: &mut Vec<String>,
) -> Option<SnapshotDiffReport> {
    let Some(before) = history_snapshots.first() else {
        caveats.push(
            "No persisted snapshots were available inside the investigation window.".to_owned(),
        );
        return None;
    };
    let after = history_snapshots
        .last()
        .filter(|after| after.captured_at_millis >= before.captured_at_millis)
        .unwrap_or(latest);
    if before.captured_at_millis == after.captured_at_millis && before.sequence == after.sequence {
        caveats.push(
            "Only one persisted snapshot was available inside the investigation window; history diff omitted."
                .to_owned(),
        );
        return None;
    }
    Some(build_snapshot_diff_report_with_diagnostics(
        before,
        after,
        entity_ids,
        limit,
        diagnostics,
    ))
}

fn build_reboot_session_report(session: &[SystemSnapshot]) -> Option<RebootSessionReport> {
    let first = session.first()?;
    let last = session.last()?;
    Some(RebootSessionReport {
        boot_id: first
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_id.clone()),
        boot_time_millis: first
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_time_millis),
        first_snapshot_millis: first.captured_at_millis,
        last_snapshot_millis: last.captured_at_millis,
        snapshot_count: session.len(),
        first_sequence: first.sequence,
        last_sequence: last.sequence,
        previous_shutdown: first
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.previous_shutdown.clone()),
    })
}

fn build_reboot_boundary_report(
    before: &SystemSnapshot,
    after: &SystemSnapshot,
    diagnostics: &[DiagnosticsEvent],
) -> RebootBoundaryReport {
    let reboot_detected_at_millis = after
        .host
        .boot_session
        .as_ref()
        .and_then(|boot| boot.boot_time_millis)
        .or(Some(after.captured_at_millis));
    let window_start = before.captured_at_millis.saturating_sub(30 * 60 * 1000);
    let window_end = after.captured_at_millis.saturating_add(30 * 60 * 1000);
    let correlated_markers = diagnostics
        .iter()
        .filter(|event| {
            event.timestamp_millis >= window_start
                && event.timestamp_millis <= window_end
                && matches!(
                    event.event_type.as_str(),
                    "boot-session-observed"
                        | "system-previous-shutdown-cause"
                        | "system-sleep-marker"
                        | "system-wake-marker"
                        | "system-panic-marker"
                        | "system-thermal-marker"
                        | "system-power-marker"
                        | "engine-initialized"
                )
        })
        .map(reboot_correlation_marker)
        .collect::<Vec<_>>();
    let pre_reboot_incidents = diagnostics
        .iter()
        .filter(|event| {
            event.timestamp_millis >= window_start
                && event.timestamp_millis <= before.captured_at_millis
                && matches!(
                    event.event_type.as_str(),
                    "host-incident-snapshot" | "tick-over-budget"
                )
        })
        .map(reboot_correlation_marker)
        .collect::<Vec<_>>();
    RebootBoundaryReport {
        before_snapshot_millis: before.captured_at_millis,
        after_snapshot_millis: after.captured_at_millis,
        reboot_detected_at_millis,
        gap_millis: after
            .captured_at_millis
            .saturating_sub(before.captured_at_millis),
        before_boot_id: before
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_id.clone()),
        after_boot_id: after
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_id.clone()),
        before_boot_time_millis: before
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_time_millis),
        after_boot_time_millis: after
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_time_millis),
        previous_shutdown: after
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.previous_shutdown.clone())
            .or_else(|| {
                extract_previous_shutdown_from_events(diagnostics, window_start, window_end)
            }),
        before_top_entities: before
            .entities
            .iter()
            .take(5)
            .map(|entity| entity.display_name.clone())
            .collect(),
        after_top_entities: after
            .entities
            .iter()
            .take(5)
            .map(|entity| entity.display_name.clone())
            .collect(),
        correlated_markers,
        pre_reboot_incidents,
    }
}

fn reboot_correlation_marker(event: &DiagnosticsEvent) -> RebootCorrelationMarker {
    RebootCorrelationMarker {
        timestamp_millis: event.timestamp_millis,
        event_type: event.event_type.clone(),
        level: format!("{:?}", event.level).to_ascii_lowercase(),
        message: event.message.clone(),
        detail: event
            .fields
            .iter()
            .find(|field| field.key == "detail")
            .map(|field| field.value.clone()),
    }
}

fn extract_previous_shutdown_from_events(
    events: &[DiagnosticsEvent],
    start_millis: u64,
    end_millis: u64,
) -> Option<aetower_model::RebootCauseSnapshot> {
    events
        .iter()
        .filter(|event| {
            event.timestamp_millis >= start_millis
                && event.timestamp_millis <= end_millis
                && matches!(
                    event.event_type.as_str(),
                    "system-previous-shutdown-cause" | "system-panic-marker"
                )
        })
        .max_by_key(|event| event.timestamp_millis)
        .map(|event| {
            let detail = event
                .fields
                .iter()
                .find(|field| field.key == "detail")
                .map(|field| field.value.clone())
                .unwrap_or_else(|| event.message.clone());
            let code = if event.event_type == "system-previous-shutdown-cause" {
                detail
                    .split(':')
                    .nth(1)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            } else {
                Some("panic".to_owned())
            };
            aetower_model::RebootCauseSnapshot {
                source: "diagnostics".to_owned(),
                code,
                detail,
                observed_at_millis: Some(event.timestamp_millis),
            }
        })
}

fn snapshot_boot_key(host: &aetower_model::HostSnapshot) -> Option<String> {
    host.boot_session
        .as_ref()
        .and_then(|boot| boot.boot_id.clone())
        .or_else(|| {
            host.boot_session
                .as_ref()
                .and_then(|boot| boot.boot_time_millis)
                .map(|millis| millis.to_string())
        })
}

fn build_anomaly_explanations(
    snapshot: &SystemSnapshot,
    requested_entity_ids: &[String],
    limit: usize,
    window_millis: u64,
) -> Vec<AnomalyExplanation> {
    let requested = requested_entity_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut entities = snapshot
        .entities
        .iter()
        .filter(|entity| {
            if !requested.is_empty() {
                return requested.contains(&entity.entity_id);
            }
            entity.anomaly_detected || entity.recent_change_summary.is_some()
        })
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        right
            .friction
            .total_score
            .partial_cmp(&left.friction.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entities.truncate(limit.max(1));
    let since = snapshot.captured_at_millis.saturating_sub(window_millis);

    entities
        .into_iter()
        .map(|entity| {
            let mut drivers = entity_metric_drivers(entity);
            drivers.sort_by(|left, right| {
                right
                    .delta
                    .abs()
                    .partial_cmp(&left.delta.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            drivers.truncate(3);
            let supporting_events = snapshot
                .timeline
                .iter()
                .filter(|event| {
                    event.timestamp_millis >= since
                        && event.entity_id.as_deref() == Some(entity.entity_id.as_str())
                })
                .map(|event| RecentChangeItem {
                    timestamp_millis: event.timestamp_millis,
                    severity: match event.severity {
                        aetower_model::TimelineSeverity::Info => SeverityBand::Info,
                        aetower_model::TimelineSeverity::Warning => SeverityBand::Warning,
                        aetower_model::TimelineSeverity::Critical => SeverityBand::Critical,
                    },
                    source: format!("timeline:{:?}", event.category).to_lowercase(),
                    entity_id: event.entity_id.clone(),
                    title: event.title.clone(),
                    detail: event.detail.clone(),
                })
                .take(3)
                .collect::<Vec<_>>();
            let driver_summary = drivers
                .iter()
                .map(|driver| driver.summary.clone())
                .collect::<Vec<_>>()
                .join(" ");
            AnomalyExplanation {
                entity_id: entity.entity_id.clone(),
                display_name: entity.display_name.clone(),
                severity: if entity.friction.total_score >= 20.0 || entity.anomaly_detected {
                    SeverityBand::Warning
                } else {
                    SeverityBand::Info
                },
                summary: if driver_summary.is_empty() {
                    entity.recent_change_summary.clone().unwrap_or_else(|| {
                        "This entity is unusual relative to its recent trend.".to_owned()
                    })
                } else {
                    driver_summary
                },
                recent_change_summary: entity.recent_change_summary.clone(),
                drivers,
                supporting_events,
            }
        })
        .collect()
}

pub fn explain_anomalies_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_ids: &[String],
    limit: usize,
    window_minutes: u64,
) -> Result<String, String> {
    let snapshot = data_source.latest_snapshot()?;
    let explanations = build_anomaly_explanations(
        &snapshot,
        entity_ids,
        limit.max(1),
        window_minutes.max(1) * 60_000,
    );
    serde_json::to_string(&explanations).map_err(|error| error.to_string())
}

fn entity_metric_drivers(entity: &aetower_model::EntitySnapshot) -> Vec<AnomalyDriver> {
    let mut drivers = Vec::new();
    push_driver(
        &mut drivers,
        "friction",
        entity
            .trend
            .friction
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "cpu_percent",
        entity
            .trend
            .cpu_percent
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "memory_bytes",
        entity
            .trend
            .memory_resident_bytes
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "wakeups_per_second",
        entity
            .trend
            .wakeups_per_second
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "disk_activity_bps",
        entity
            .trend
            .disk_activity_bps
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "network_activity_bps",
        entity
            .trend
            .network_activity_bps
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    drivers
}

fn push_driver(drivers: &mut Vec<AnomalyDriver>, metric: &str, values: Vec<f64>) {
    if values.len() < 2 {
        return;
    }
    let after = *values.last().unwrap_or(&0.0);
    let baseline_slice = &values[..values.len() - 1];
    let before = baseline_slice.iter().sum::<f64>() / baseline_slice.len() as f64;
    let delta = after - before;
    if delta.abs() < f64::EPSILON {
        return;
    }
    drivers.push(AnomalyDriver {
        metric: metric.to_owned(),
        before,
        after,
        delta,
        summary: format!(
            "{} shifted from {} to {} (delta {}).",
            metric,
            format_metric_value(metric, before),
            format_metric_value(metric, after),
            format_metric_value(metric, delta)
        ),
    });
}

fn build_process_tree_report(
    snapshot: &SystemSnapshot,
    entity_id: &str,
) -> Result<ProcessTreeReport, String> {
    let root = snapshot
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .ok_or_else(|| format!("Unknown entity_id: {entity_id}"))?;
    let seed_entities = vec![root.clone()];
    let (expanded_entities, grouping_reasons) =
        related_entities_for_process_tree(&seed_entities, &snapshot.entities);
    let expanded_ids = expanded_entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<Vec<_>>();
    let grouped_process_count = seed_entities
        .iter()
        .flat_map(|entity| entity.components.iter())
        .filter(|component| component.kind != aetower_model::ComponentKind::AdapterContext)
        .count() as u32;
    let expanded_process_count = expanded_entities
        .iter()
        .flat_map(|entity| entity.components.iter())
        .filter(|component| component.kind != aetower_model::ComponentKind::AdapterContext)
        .count() as u32;
    let roots = process_tree_roots(root, &expanded_entities);
    Ok(ProcessTreeReport {
        captured_at_millis: snapshot.captured_at_millis,
        root_entity_id: root.entity_id.clone(),
        root_display_name: root.display_name.clone(),
        seed_entity_ids: vec![root.entity_id.clone()],
        expanded_entity_ids: expanded_ids,
        grouped_process_count,
        expanded_process_count,
        grouping_reasons,
        roots,
    })
}

pub fn entity_process_tree_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
) -> Result<String, String> {
    let snapshot = data_source.latest_snapshot()?;
    let report = build_process_tree_report(&snapshot, entity_id)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

#[derive(Clone)]
struct RelatedProcessComponent {
    entity_id: String,
    owner_display_name: String,
    component: aetower_model::ComponentSnapshot,
    badges: Vec<String>,
}

#[derive(Clone, Copy, Default)]
struct ProcessAggregate {
    subtree_cpu_percent: f32,
    subtree_memory_bytes: u64,
    subtree_process_count: u32,
}

fn process_tree_roots(
    root_entity: &aetower_model::EntitySnapshot,
    expanded_entities: &[aetower_model::EntitySnapshot],
) -> Vec<ProcessTreeNodeReport> {
    let related_components = expanded_entities
        .iter()
        .flat_map(|entity| {
            entity
                .components
                .iter()
                .cloned()
                .map(|component| RelatedProcessComponent {
                    entity_id: entity.entity_id.clone(),
                    owner_display_name: entity.display_name.clone(),
                    component,
                    badges: entity.badges.clone(),
                })
        })
        .collect::<Vec<_>>();
    let process_components = related_components
        .iter()
        .filter(|related| related.component.kind != aetower_model::ComponentKind::AdapterContext)
        .cloned()
        .collect::<Vec<_>>();
    let adapter_components = root_entity
        .components
        .iter()
        .filter(|component| component.kind == aetower_model::ComponentKind::AdapterContext)
        .cloned()
        .collect::<Vec<_>>();
    let pid_map = process_components
        .iter()
        .filter_map(|related| {
            related
                .component
                .process_id
                .map(|pid| (pid, related.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    let mut roots = Vec::new();
    let mut children: BTreeMap<u32, Vec<RelatedProcessComponent>> = BTreeMap::new();
    for related in process_components {
        let parent_pid = extract_parent_pid(related.component.parent_summary.as_deref());
        if let Some(parent_pid) = parent_pid
            && pid_map.contains_key(&parent_pid)
        {
            children.entry(parent_pid).or_default().push(related);
        } else {
            roots.push(related);
        }
    }

    let mut reports = Vec::new();
    let total_aggregate = roots
        .iter()
        .fold(ProcessAggregate::default(), |aggregate, root| {
            let next = subtree_aggregate(root, &children);
            ProcessAggregate {
                subtree_cpu_percent: aggregate.subtree_cpu_percent + next.subtree_cpu_percent,
                subtree_memory_bytes: aggregate.subtree_memory_bytes + next.subtree_memory_bytes,
                subtree_process_count: aggregate.subtree_process_count + next.subtree_process_count,
            }
        });

    let mut sorted_roots = roots;
    sorted_roots.sort_by(|left, right| {
        let left_aggregate = subtree_aggregate(left, &children);
        let right_aggregate = subtree_aggregate(right, &children);
        right_aggregate
            .subtree_cpu_percent
            .partial_cmp(&left_aggregate.subtree_cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right_aggregate
                    .subtree_memory_bytes
                    .cmp(&left_aggregate.subtree_memory_bytes)
            })
    });

    let chau7_sessions = adapter_components
        .iter()
        .filter(|component| {
            component.adapter_context.as_ref().is_some_and(|context| {
                context.kind == aetower_model::AdapterContextKind::Chau7Session
            })
        })
        .collect::<Vec<_>>();
    if !chau7_sessions.is_empty() {
        for session in chau7_sessions {
            reports.push(ProcessTreeNodeReport {
                title: session.title.clone(),
                pid: session.process_id,
                relation: "adapter-root".to_owned(),
                owner_entity_id: root_entity.entity_id.clone(),
                owner_display_name: root_entity.display_name.clone(),
                self_cpu_percent: 0.0,
                subtree_cpu_percent: total_aggregate.subtree_cpu_percent,
                self_memory_bytes: 0,
                subtree_memory_bytes: total_aggregate.subtree_memory_bytes,
                subtree_process_count: total_aggregate.subtree_process_count,
                badges: root_entity.badges.clone(),
                user: session.user.clone(),
                cwd: session
                    .adapter_context
                    .as_ref()
                    .and_then(|context| {
                        context.repo_root.clone().or(context.workspace_path.clone())
                    })
                    .or_else(|| session.cwd.clone()),
                provenance: None,
                launched_by: None,
                adapter_label: adapter_label(session),
                status_label: session
                    .adapter_context
                    .as_ref()
                    .and_then(|context| context.status.clone()),
                children: sorted_roots
                    .iter()
                    .map(|root| process_tree_node(root, &children, "session-child"))
                    .collect(),
            });
        }
    } else {
        for root in &sorted_roots {
            reports.push(process_tree_node(root, &children, "process-root"));
        }
    }

    for component in adapter_components.into_iter().filter(|component| {
        component
            .adapter_context
            .as_ref()
            .is_none_or(|context| context.kind != aetower_model::AdapterContextKind::Chau7Session)
    }) {
        reports.push(ProcessTreeNodeReport {
            title: component.title.clone(),
            pid: component.process_id,
            relation: "adapter".to_owned(),
            owner_entity_id: root_entity.entity_id.clone(),
            owner_display_name: root_entity.display_name.clone(),
            self_cpu_percent: 0.0,
            subtree_cpu_percent: 0.0,
            self_memory_bytes: 0,
            subtree_memory_bytes: 0,
            subtree_process_count: 0,
            badges: root_entity.badges.clone(),
            user: component.user.clone(),
            cwd: component
                .adapter_context
                .as_ref()
                .and_then(|context| context.repo_root.clone().or(context.workspace_path.clone()))
                .or_else(|| component.cwd.clone()),
            provenance: None,
            launched_by: None,
            adapter_label: adapter_label(&component),
            status_label: component
                .adapter_context
                .as_ref()
                .and_then(|context| context.status.clone()),
            children: Vec::new(),
        });
    }

    reports
}

fn process_tree_node(
    related: &RelatedProcessComponent,
    children: &BTreeMap<u32, Vec<RelatedProcessComponent>>,
    relation: &str,
) -> ProcessTreeNodeReport {
    let aggregate = subtree_aggregate(related, children);
    let child_nodes = related
        .component
        .process_id
        .and_then(|pid| children.get(&pid))
        .map(|child_components| {
            let mut child_components = child_components.clone();
            child_components.sort_by(|left, right| {
                let left_aggregate = subtree_aggregate(left, children);
                let right_aggregate = subtree_aggregate(right, children);
                right_aggregate
                    .subtree_cpu_percent
                    .partial_cmp(&left_aggregate.subtree_cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            child_components
                .iter()
                .map(|child| process_tree_node(child, children, "child"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    ProcessTreeNodeReport {
        title: related.component.title.clone(),
        pid: related.component.process_id,
        relation: relation.to_owned(),
        owner_entity_id: related.entity_id.clone(),
        owner_display_name: related.owner_display_name.clone(),
        self_cpu_percent: related.component.cpu_percent,
        subtree_cpu_percent: aggregate.subtree_cpu_percent,
        self_memory_bytes: related.component.memory_bytes,
        subtree_memory_bytes: aggregate.subtree_memory_bytes,
        subtree_process_count: aggregate.subtree_process_count,
        badges: related.badges.clone(),
        user: related.component.user.clone(),
        cwd: related.component.cwd.clone(),
        provenance: related
            .component
            .provenance
            .as_ref()
            .map(|provenance| format!("{:?}: {}", provenance.kind, provenance.label)),
        launched_by: related.component.launched_by.clone(),
        adapter_label: None,
        status_label: if aggregate.subtree_process_count > 1 {
            Some("group".to_owned())
        } else {
            None
        },
        children: child_nodes,
    }
}

fn subtree_aggregate(
    related: &RelatedProcessComponent,
    children: &BTreeMap<u32, Vec<RelatedProcessComponent>>,
) -> ProcessAggregate {
    let mut aggregate = ProcessAggregate {
        subtree_cpu_percent: related.component.cpu_percent,
        subtree_memory_bytes: related.component.memory_bytes,
        subtree_process_count: 1,
    };
    if let Some(pid) = related.component.process_id
        && let Some(child_components) = children.get(&pid)
    {
        for child in child_components {
            let child_aggregate = subtree_aggregate(child, children);
            aggregate.subtree_cpu_percent += child_aggregate.subtree_cpu_percent;
            aggregate.subtree_memory_bytes += child_aggregate.subtree_memory_bytes;
            aggregate.subtree_process_count += child_aggregate.subtree_process_count;
        }
    }
    aggregate
}

fn related_entities_for_process_tree(
    seed_entities: &[aetower_model::EntitySnapshot],
    all_entities: &[aetower_model::EntitySnapshot],
) -> (Vec<aetower_model::EntitySnapshot>, Vec<String>) {
    let mut included_ids = seed_entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    let mut included_pids = seed_entities
        .iter()
        .flat_map(|entity| {
            entity
                .components
                .iter()
                .filter_map(|component| component.process_id)
        })
        .collect::<BTreeSet<_>>();
    let selected_session_ids = seed_entities
        .iter()
        .flat_map(entity_session_ids)
        .collect::<BTreeSet<_>>();
    let selected_repo_roots = seed_entities
        .iter()
        .flat_map(entity_repo_roots)
        .collect::<BTreeSet<_>>();
    let mut reasons = BTreeSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        let candidates = all_entities
            .iter()
            .filter(|entity| !included_ids.contains(&entity.entity_id))
            .cloned()
            .collect::<Vec<_>>();
        for candidate in candidates {
            let is_child_by_pid = candidate.components.iter().any(|component| {
                extract_parent_pid(component.parent_summary.as_deref())
                    .is_some_and(|parent_pid| included_pids.contains(&parent_pid))
            });
            let shares_chau7_context = candidate.badges.iter().any(|badge| badge == "chau7-live")
                && (!selected_session_ids.is_disjoint(&entity_session_ids(&candidate))
                    || candidate.components.iter().any(|component| {
                        [
                            component.cwd.as_deref(),
                            component.executable_path.as_deref(),
                            component
                                .adapter_context
                                .as_ref()
                                .and_then(|context| context.workspace_path.as_deref()),
                            component
                                .adapter_context
                                .as_ref()
                                .and_then(|context| context.repo_root.as_deref()),
                        ]
                        .into_iter()
                        .flatten()
                        .any(|path| {
                            selected_repo_roots
                                .iter()
                                .any(|root| path.starts_with(root))
                        })
                    }));
            if is_child_by_pid || shares_chau7_context {
                included_ids.insert(candidate.entity_id.clone());
                included_pids.extend(
                    candidate
                        .components
                        .iter()
                        .filter_map(|component| component.process_id),
                );
                if is_child_by_pid {
                    reasons.insert("expanded through parent/child PID lineage".to_owned());
                }
                if shares_chau7_context {
                    reasons.insert(
                        "expanded through shared Chau7 session or workspace context".to_owned(),
                    );
                }
                changed = true;
            }
        }
    }
    (
        all_entities
            .iter()
            .filter(|entity| included_ids.contains(&entity.entity_id))
            .cloned()
            .collect(),
        reasons.into_iter().collect(),
    )
}

fn extract_parent_pid(summary: Option<&str>) -> Option<u32> {
    let summary = summary?;
    let marker = summary.find("pid ")?;
    let digits = summary[marker + 4..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u32>().ok()
}

fn adapter_label(component: &aetower_model::ComponentSnapshot) -> Option<String> {
    match component
        .adapter_context
        .as_ref()
        .map(|context| &context.kind)
    {
        Some(aetower_model::AdapterContextKind::Chau7Session) => Some("chau7".to_owned()),
        Some(aetower_model::AdapterContextKind::ChromiumTab) => Some("chromium".to_owned()),
        Some(aetower_model::AdapterContextKind::DockerContainer) => Some("docker".to_owned()),
        Some(aetower_model::AdapterContextKind::PrivilegedSocket) => Some("helper".to_owned()),
        Some(aetower_model::AdapterContextKind::VsCodeWorkspace)
        | Some(aetower_model::AdapterContextKind::VsCodeRuntime) => Some("vscode".to_owned()),
        Some(aetower_model::AdapterContextKind::Unknown) | None => None,
    }
}

fn process_dynamic_tool_request(
    data_source: &dyn AetowerMcpDataSource,
    request: &DynamicToolRequest,
) -> Result<Value, String> {
    match request {
        DynamicToolRequest::MemoryBreakdown {
            entity_id,
            top_regions,
        } => tool_json(build_entity_memory_breakdown(
            data_source,
            entity_id,
            *top_regions,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProfileEntity {
            entity_id,
            duration_seconds,
            top_stacks,
        } => tool_json(build_entity_profile(
            data_source,
            entity_id,
            *duration_seconds,
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::WakeupAttribution {
            entity_id,
            duration_seconds,
            top_stacks,
        } => tool_json(build_wakeup_attribution(
            data_source,
            entity_id,
            *duration_seconds,
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProcessInspect { pid } => {
            tool_json(build_process_inspection(data_source, *pid)?)
                .map_err(|error| extract_tool_error_message(&error))
        }
        DynamicToolRequest::ProcessOpenResources { pid, limit } => {
            tool_json(build_process_open_resources(*pid, *limit)?)
                .map_err(|error| extract_tool_error_message(&error))
        }
        DynamicToolRequest::ProcessSample {
            pid,
            duration_seconds,
            top_stacks,
        } => tool_json(build_process_sample(
            *pid,
            (*duration_seconds).clamp(1, MAX_PROFILE_DURATION_SECONDS),
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProcessAction {
            pid,
            action,
            dry_run,
            reason,
        } => tool_json(build_process_action(
            data_source,
            *pid,
            action,
            *dry_run,
            reason.clone(),
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProcessActionHistory {
            window_minutes,
            limit,
        } => tool_json(build_process_action_history(
            data_source,
            *window_minutes,
            *limit,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
    }
}

fn build_entity_memory_breakdown(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    top_regions: usize,
) -> Result<EntityMemoryBreakdown, String> {
    let snapshot = data_source.latest_snapshot()?;
    let entity = snapshot
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .ok_or_else(|| format!("Unknown entity_id: {entity_id}"))?;
    let process_ids = entity_process_ids(entity);
    if process_ids.is_empty() {
        return Err(format!(
            "Entity {} has no attributed process IDs to inspect.",
            entity.display_name
        ));
    }
    let mut regions_by_type = BTreeMap::<String, MemoryRegionBreakdown>::new();
    for pid in &process_ids {
        let output = run_os_command("/usr/bin/vmmap", &[pid.to_string()])?;
        for region in parse_vmmap_regions(&output) {
            let entry = regions_by_type.entry(region.region_type.clone()).or_insert(
                MemoryRegionBreakdown {
                    region_type: region.region_type.clone(),
                    virtual_bytes: 0,
                    resident_bytes: 0,
                    dirty_bytes: 0,
                    swap_bytes: 0,
                },
            );
            entry.virtual_bytes += region.virtual_bytes;
            entry.resident_bytes += region.resident_bytes;
            entry.dirty_bytes += region.dirty_bytes;
            entry.swap_bytes += region.swap_bytes;
        }
    }
    let mut regions = regions_by_type.into_values().collect::<Vec<_>>();
    regions.sort_by(|left, right| {
        right
            .resident_bytes
            .cmp(&left.resident_bytes)
            .then_with(|| right.virtual_bytes.cmp(&left.virtual_bytes))
    });
    regions.truncate(top_regions.max(1));
    Ok(EntityMemoryBreakdown {
        captured_at_millis: snapshot.captured_at_millis,
        entity_id: entity.entity_id.clone(),
        display_name: entity.display_name.clone(),
        process_ids,
        resident_bytes: entity.metrics.memory_resident_bytes,
        physical_footprint_bytes: entity.metrics.memory_physical_footprint_bytes,
        memory_metric_note: if entity.metrics.memory_physical_footprint_bytes > 0 {
            "resident_bytes is the current resident set; physical_footprint_bytes follows macOS task footprint when available and may exceed resident because it includes graphics and other charged memory.".to_owned()
        } else {
            "resident_bytes is the current resident set; physical_footprint_bytes is unavailable for this entity on the current platform or sample.".to_owned()
        },
        regions,
    })
}

pub fn memory_breakdown_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    top_regions: usize,
) -> Result<String, String> {
    let report = build_entity_memory_breakdown(data_source, entity_id, top_regions.max(1))?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn build_entity_profile(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<EntityProfileReport, String> {
    let snapshot = data_source.latest_snapshot()?;
    let entity = snapshot
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .ok_or_else(|| format!("Unknown entity_id: {entity_id}"))?;
    let process_ids = entity_process_ids(entity);
    if process_ids.is_empty() {
        return Err(format!(
            "Entity {} has no attributed process IDs to profile.",
            entity.display_name
        ));
    }
    let mut stack_reports = Vec::new();
    for pid in &process_ids {
        let output = run_os_command(
            "/usr/bin/sample",
            &[
                pid.to_string(),
                duration_seconds.to_string(),
                "1".to_owned(),
            ],
        )?;
        stack_reports.extend(parse_sample_threads(&output));
    }
    stack_reports.sort_by(|left, right| right.sample_count.cmp(&left.sample_count));
    stack_reports.truncate(top_stacks.max(1));
    let summary = if let Some(first) = stack_reports.first() {
        format!(
            "Top sampled thread {} accounted for {} samples and is classified as {}.",
            first.thread_label, first.sample_count, first.classification
        )
    } else {
        "No non-empty sampled stacks were captured.".to_owned()
    };
    Ok(EntityProfileReport {
        captured_at_millis: snapshot.captured_at_millis,
        entity_id: entity.entity_id.clone(),
        display_name: entity.display_name.clone(),
        duration_seconds,
        sampled_process_ids: process_ids,
        thread_count: stack_reports.len(),
        top_stacks: stack_reports,
        summary,
    })
}

pub fn profile_entity_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<String, String> {
    let report = build_entity_profile(
        data_source,
        entity_id,
        duration_seconds.max(1),
        top_stacks.max(1),
    )?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn build_wakeup_attribution(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<WakeupAttributionReport, String> {
    let profile =
        build_entity_profile(data_source, entity_id, duration_seconds, top_stacks.max(3))?;
    let mut grouped = BTreeMap::<(String, String), SampledStackReport>::new();
    for stack in &profile.top_stacks {
        let key = (
            stack
                .queue_label
                .clone()
                .unwrap_or_else(|| stack.thread_label.clone()),
            stack.classification.clone(),
        );
        let entry = grouped.entry(key).or_insert(SampledStackReport {
            thread_label: stack.thread_label.clone(),
            queue_label: stack.queue_label.clone(),
            sample_count: 0,
            top_frames: stack.top_frames.clone(),
            classification: stack.classification.clone(),
        });
        entry.sample_count += stack.sample_count;
        if entry.top_frames.is_empty() {
            entry.top_frames = stack.top_frames.clone();
        }
    }
    let mut queue_breakdown = grouped.into_values().collect::<Vec<_>>();
    queue_breakdown.sort_by(|left, right| right.sample_count.cmp(&left.sample_count));
    queue_breakdown.truncate(top_stacks.max(1));
    let dominant_cause = queue_breakdown
        .iter()
        .find(|entry| entry.classification != "idle")
        .map(|entry| {
            format!(
                "{} dominates sampled wakeups with {} samples on {}.",
                entry.classification,
                entry.sample_count,
                entry
                    .queue_label
                    .clone()
                    .unwrap_or_else(|| entry.thread_label.clone())
            )
        });
    Ok(WakeupAttributionReport {
        captured_at_millis: profile.captured_at_millis,
        entity_id: profile.entity_id,
        display_name: profile.display_name,
        duration_seconds: profile.duration_seconds,
        sampled_process_ids: profile.sampled_process_ids,
        queue_breakdown,
        dominant_cause,
        attribution_mode: "sampled-call-stack-heuristic".to_owned(),
        caveats: vec![
            "This is a sampled heuristic based on `sample`, not exact kernel wakeup accounting."
                .to_owned(),
            "Queue labels are only present when the sampled stack exposed one.".to_owned(),
        ],
    })
}

pub fn wakeup_attribution_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<String, String> {
    let report = build_wakeup_attribution(
        data_source,
        entity_id,
        duration_seconds.max(1),
        top_stacks.max(1),
    )?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

#[derive(Debug, Clone)]
struct ProcessComponentContext {
    entity_id: String,
    display_name: String,
    component_title: String,
    component_kind: String,
    executable_path: Option<String>,
    command_line: Option<String>,
    cwd: Option<String>,
    user: Option<String>,
    parent_summary: Option<String>,
    cpu_percent: f32,
    memory_bytes: u64,
    memory_physical_footprint_bytes: u64,
    start_time_millis: u64,
    sibling_process_count: u32,
}

fn build_process_inspection(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
) -> Result<ProcessInspectionReport, String> {
    validate_pid(pid)?;
    let snapshot = data_source.latest_snapshot()?;
    let context = process_component_context(&snapshot, pid);
    let ps = process_ps_summary(pid).ok();
    let alive = ps.is_some() || process_exists(pid);
    let child_pids = process_child_pids(&snapshot, pid);
    let parent_pid = context
        .as_ref()
        .and_then(|context| extract_parent_pid(context.parent_summary.as_deref()))
        .or_else(|| ps.as_ref().and_then(|summary| summary.parent_pid));
    let mut safety_notes = Vec::new();
    if pid == std::process::id() {
        safety_notes.push(
            "This is the running Aetower process; destructive actions are blocked.".to_owned(),
        );
    }
    if context.is_none() {
        safety_notes.push(
            "This PID is not currently attributed to an Aetower entity; live ps data may still exist."
                .to_owned(),
        );
    }
    if !alive {
        safety_notes.push("The process is not visible to ps right now.".to_owned());
    }

    Ok(ProcessInspectionReport {
        captured_at_millis: snapshot.captured_at_millis,
        pid,
        alive,
        entity_id: context.as_ref().map(|context| context.entity_id.clone()),
        display_name: context.as_ref().map(|context| context.display_name.clone()),
        component_title: context
            .as_ref()
            .map(|context| context.component_title.clone()),
        component_kind: context
            .as_ref()
            .map(|context| context.component_kind.clone()),
        executable_path: context
            .as_ref()
            .and_then(|context| context.executable_path.clone()),
        command_line: context
            .as_ref()
            .and_then(|context| context.command_line.clone())
            .or_else(|| ps.as_ref().and_then(|summary| summary.command.clone())),
        cwd: context.as_ref().and_then(|context| context.cwd.clone()),
        user: context
            .as_ref()
            .and_then(|context| context.user.clone())
            .or_else(|| ps.as_ref().and_then(|summary| summary.user.clone())),
        parent_pid,
        parent_summary: context
            .as_ref()
            .and_then(|context| context.parent_summary.clone()),
        cpu_percent: context
            .as_ref()
            .map(|context| context.cpu_percent)
            .or_else(|| ps.as_ref().and_then(|summary| summary.cpu_percent)),
        memory_bytes: context
            .as_ref()
            .map(|context| context.memory_bytes)
            .or_else(|| ps.as_ref().and_then(|summary| summary.resident_bytes)),
        memory_physical_footprint_bytes: context
            .as_ref()
            .map(|context| context.memory_physical_footprint_bytes),
        start_time_millis: context.as_ref().map(|context| context.start_time_millis),
        child_pids,
        sibling_process_count: context
            .as_ref()
            .map(|context| context.sibling_process_count)
            .unwrap_or(0),
        ps,
        safety_notes,
    })
}

pub fn process_inspect_json(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
) -> Result<String, String> {
    let report = build_process_inspection(data_source, pid)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn build_process_open_resources(
    pid: u32,
    limit: usize,
) -> Result<ProcessOpenResourcesReport, String> {
    validate_pid(pid)?;
    let output = run_os_command(
        "/usr/sbin/lsof",
        &["-nP".to_owned(), "-p".to_owned(), pid.to_string()],
    )?;
    let mut resources = parse_lsof_resources(&output);
    let resource_count = resources.len();
    let socket_count = resources
        .iter()
        .filter(|resource| resource.is_socket)
        .count();
    let file_count = resource_count.saturating_sub(socket_count);
    resources.truncate(limit.max(1));
    Ok(ProcessOpenResourcesReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        pid,
        resource_count,
        returned: resources.len(),
        file_count,
        socket_count,
        resources,
    })
}

pub fn process_open_resources_json(pid: u32, limit: usize) -> Result<String, String> {
    let report = build_process_open_resources(pid, limit)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn build_process_sample(
    pid: u32,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<ProcessSampleReport, String> {
    validate_pid(pid)?;
    if !process_exists(pid) {
        return Err(format!("Process {pid} is not visible to ps right now."));
    }
    let output = run_os_command(
        "/usr/bin/sample",
        &[
            pid.to_string(),
            duration_seconds.to_string(),
            "1".to_owned(),
        ],
    )?;
    let mut stack_reports = parse_sample_threads(&output);
    stack_reports.sort_by(|left, right| right.sample_count.cmp(&left.sample_count));
    stack_reports.truncate(top_stacks.max(1));
    let summary = if let Some(first) = stack_reports.first() {
        format!(
            "Top sampled thread {} accounted for {} samples and is classified as {}.",
            first.thread_label, first.sample_count, first.classification
        )
    } else {
        "No non-empty sampled stacks were captured.".to_owned()
    };
    Ok(ProcessSampleReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        pid,
        duration_seconds,
        thread_count: stack_reports.len(),
        top_stacks: stack_reports,
        summary,
    })
}

pub fn process_sample_json(
    pid: u32,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<String, String> {
    let report = build_process_sample(
        pid,
        duration_seconds.clamp(1, MAX_PROFILE_DURATION_SECONDS),
        top_stacks.max(1),
    )?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn build_process_action(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
    action: &str,
    dry_run: bool,
    reason: Option<String>,
) -> Result<ProcessActionReport, String> {
    validate_pid(pid)?;
    let snapshot = data_source.latest_snapshot().ok();
    let plan = process_action_plan(snapshot.as_ref(), pid, action)?;
    let context = snapshot
        .as_ref()
        .and_then(|snapshot| process_component_context(snapshot, pid));
    let mut safety_notes = Vec::new();
    if plan.target_pids.contains(&std::process::id()) {
        safety_notes.push("Aetower refuses to target its own running process.".to_owned());
        if !dry_run {
            return Err("Refusing to target the running Aetower process.".to_owned());
        }
    }
    if !dry_run {
        let missing_pids = plan
            .target_pids
            .iter()
            .copied()
            .filter(|target_pid| !process_exists(*target_pid))
            .collect::<Vec<_>>();
        if !missing_pids.is_empty() {
            return Err(format!(
                "Target process(es) are not visible to ps right now: {missing_pids:?}."
            ));
        }
    }

    if dry_run {
        return Ok(ProcessActionReport {
            captured_at_millis: current_unix_millis().unwrap_or_default(),
            pid,
            target_pids: plan.target_pids,
            action: plan.normalized_action,
            signal: plan.signal,
            dry_run,
            executed: false,
            success: true,
            command: plan.command,
            reason,
            entity_id: context.as_ref().map(|context| context.entity_id.clone()),
            display_name: context.as_ref().map(|context| context.display_name.clone()),
            message: plan.dry_run_message,
            safety_notes,
        });
    }

    let status = Command::new(&plan.program)
        .args(&plan.args)
        .status()
        .map_err(|error| format!("run {}: {error}", plan.program))?;
    let success = status.success();
    let message = if success {
        plan.success_message.clone()
    } else {
        format!("{} exited with status {status}.", plan.program)
    };
    let report = ProcessActionReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        pid,
        target_pids: plan.target_pids,
        action: plan.normalized_action,
        signal: plan.signal,
        dry_run,
        executed: true,
        success,
        command: plan.command,
        reason: reason.clone(),
        entity_id: context.as_ref().map(|context| context.entity_id.clone()),
        display_name: context.as_ref().map(|context| context.display_name.clone()),
        message,
        safety_notes,
    };
    data_source.record_diagnostics_event(process_action_diagnostics_event(&report));
    Ok(report)
}

pub fn process_action_json(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
    action: &str,
    dry_run: bool,
    reason: Option<String>,
) -> Result<String, String> {
    let report = build_process_action(data_source, pid, action, dry_run, reason)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn build_process_action_history(
    data_source: &dyn AetowerMcpDataSource,
    window_minutes: u64,
    limit: usize,
) -> Result<ProcessActionHistoryReport, String> {
    let now = current_unix_millis().unwrap_or_default();
    let events = data_source.query_diagnostics(DiagnosticsQuery {
        limit: limit.saturating_mul(4).max(limit).max(1),
        search: Some("process-action".to_owned()),
        since_millis: Some(now.saturating_sub(window_minutes.saturating_mul(60 * 1000))),
        include_persisted: true,
        ..DiagnosticsQuery::default()
    })?;
    let actions = events
        .into_iter()
        .filter(|event| event.event_type == "process-action")
        .take(limit.max(1))
        .map(process_action_history_item)
        .collect::<Vec<_>>();
    Ok(ProcessActionHistoryReport {
        window_minutes,
        returned: actions.len(),
        actions,
    })
}

pub fn process_action_history_json(
    data_source: &dyn AetowerMcpDataSource,
    window_minutes: u64,
    limit: usize,
) -> Result<String, String> {
    let report = build_process_action_history(data_source, window_minutes.max(1), limit.max(1))?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn entity_process_ids(entity: &aetower_model::EntitySnapshot) -> Vec<u32> {
    let mut process_ids = entity
        .components
        .iter()
        .filter_map(|component| {
            (component.kind != aetower_model::ComponentKind::AdapterContext)
                .then_some(component.process_id)
                .flatten()
        })
        .collect::<Vec<_>>();
    process_ids.sort_unstable();
    process_ids.dedup();
    process_ids
}

fn process_component_context(
    snapshot: &SystemSnapshot,
    pid: u32,
) -> Option<ProcessComponentContext> {
    for entity in &snapshot.entities {
        for component in &entity.components {
            if component.process_id != Some(pid) {
                continue;
            }
            let sibling_process_count = entity_process_ids(entity).len() as u32;
            return Some(ProcessComponentContext {
                entity_id: entity.entity_id.clone(),
                display_name: entity.display_name.clone(),
                component_title: component.title.clone(),
                component_kind: format!("{:?}", component.kind),
                executable_path: component.executable_path.clone(),
                command_line: component.command_line.clone(),
                cwd: component.cwd.clone(),
                user: component.user.clone(),
                parent_summary: component.parent_summary.clone(),
                cpu_percent: component.cpu_percent,
                memory_bytes: component.memory_bytes,
                memory_physical_footprint_bytes: component.memory_physical_footprint_bytes,
                start_time_millis: component.start_time_millis,
                sibling_process_count,
            });
        }
    }
    None
}

fn process_child_pids(snapshot: &SystemSnapshot, pid: u32) -> Vec<u32> {
    let mut child_pids = snapshot
        .entities
        .iter()
        .flat_map(|entity| &entity.components)
        .filter(|component| extract_parent_pid(component.parent_summary.as_deref()) == Some(pid))
        .filter_map(|component| component.process_id)
        .collect::<Vec<_>>();
    child_pids.sort_unstable();
    child_pids.dedup();
    child_pids
}

fn validate_pid(pid: u32) -> Result<(), String> {
    if pid <= 1 {
        Err(format!(
            "Refusing to inspect or signal protected pid {pid}."
        ))
    } else {
        Ok(())
    }
}

fn process_exists(pid: u32) -> bool {
    Command::new("/bin/ps")
        .arg("-p")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn process_ps_summary(pid: u32) -> Result<ProcessPsSummary, String> {
    let output = run_os_command(
        "/bin/ps",
        &[
            "-p".to_owned(),
            pid.to_string(),
            "-o".to_owned(),
            "ppid=".to_owned(),
            "-o".to_owned(),
            "user=".to_owned(),
            "-o".to_owned(),
            "stat=".to_owned(),
            "-o".to_owned(),
            "pcpu=".to_owned(),
            "-o".to_owned(),
            "rss=".to_owned(),
            "-o".to_owned(),
            "command=".to_owned(),
        ],
    )?;
    parse_ps_summary(&output)
}

fn parse_ps_summary(output: &str) -> Result<ProcessPsSummary, String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "ps returned no process rows".to_owned())?;
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 5 {
        return Err(format!("ps row had too few columns: {line}"));
    }
    Ok(ProcessPsSummary {
        parent_pid: parts[0].parse::<u32>().ok(),
        user: Some(parts[1].to_owned()),
        status: Some(parts[2].to_owned()),
        cpu_percent: parts[3].parse::<f32>().ok(),
        resident_bytes: parts[4]
            .parse::<u64>()
            .ok()
            .map(|rss_kib| rss_kib.saturating_mul(1024)),
        command: (parts.len() > 5).then(|| parts[5..].join(" ")),
    })
}

fn parse_lsof_resources(output: &str) -> Vec<ProcessOpenResource> {
    output
        .lines()
        .skip(1)
        .filter_map(parse_lsof_resource_line)
        .collect()
}

fn parse_lsof_resource_line(line: &str) -> Option<ProcessOpenResource> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 9 {
        return None;
    }
    let resource_type = parts[4].to_owned();
    let name = parts[8..].join(" ");
    let is_socket = matches!(resource_type.as_str(), "IPv4" | "IPv6" | "unix")
        || name.contains("TCP ")
        || name.contains("UDP ")
        || name.contains("->");
    Some(ProcessOpenResource {
        fd: parts[3].to_owned(),
        resource_type,
        name,
        detail: Some(parts[5..8].join(" ")),
        is_socket,
    })
}

fn process_action_plan(
    snapshot: Option<&SystemSnapshot>,
    pid: u32,
    action: &str,
) -> Result<ProcessActionPlan, String> {
    match action.trim().to_ascii_lowercase().as_str() {
        "terminate" | "term" | "sigterm" => Ok(signal_process_action_plan(
            "terminate",
            "TERM",
            vec![pid],
            "Dry run only; no terminate signal was sent.",
            format!("Sent TERM to process {pid}."),
        )),
        "force-kill" | "force_kill" | "kill" | "sigkill" => Ok(signal_process_action_plan(
            "force-kill",
            "KILL",
            vec![pid],
            "Dry run only; no force-kill signal was sent.",
            format!("Sent KILL to process {pid}."),
        )),
        "suspend" | "stop" | "sigstop" => Ok(signal_process_action_plan(
            "suspend",
            "STOP",
            vec![pid],
            "Dry run only; no suspend signal was sent.",
            format!("Sent STOP to process {pid}."),
        )),
        "resume" | "continue" | "cont" | "sigcont" => Ok(signal_process_action_plan(
            "resume",
            "CONT",
            vec![pid],
            "Dry run only; no resume signal was sent.",
            format!("Sent CONT to process {pid}."),
        )),
        "terminate-tree" | "tree-terminate" | "sigterm-tree" => Ok(signal_process_action_plan(
            "terminate-tree",
            "TERM",
            process_tree_target_pids(snapshot, pid),
            "Dry run only; no terminate-tree signal was sent.",
            format!("Sent TERM to process tree rooted at {pid}."),
        )),
        "force-kill-tree" | "tree-kill" | "tree-force-kill" | "sigkill-tree" => {
            Ok(signal_process_action_plan(
                "force-kill-tree",
                "KILL",
                process_tree_target_pids(snapshot, pid),
                "Dry run only; no force-kill-tree signal was sent.",
                format!("Sent KILL to process tree rooted at {pid}."),
            ))
        }
        "lower-priority" | "renice-background" | "background" => Ok(renice_process_action_plan(
            "lower-priority",
            10,
            pid,
            "Dry run only; priority was not changed.",
            format!("Requested lower priority for process {pid} with nice value 10."),
        )),
        "normal-priority" | "restore-priority" | "renice-normal" => Ok(renice_process_action_plan(
            "normal-priority",
            0,
            pid,
            "Dry run only; priority was not changed.",
            format!("Requested normal priority for process {pid} with nice value 0."),
        )),
        _ => Err(format!(
            "Unsupported process action '{action}'. Use terminate, force-kill, suspend, resume, terminate-tree, force-kill-tree, lower-priority, or normal-priority."
        )),
    }
}

fn signal_process_action_plan(
    normalized_action: &str,
    signal: &str,
    target_pids: Vec<u32>,
    dry_run_message: &str,
    success_message: String,
) -> ProcessActionPlan {
    let mut args = vec![format!("-{signal}")];
    args.extend(target_pids.iter().map(u32::to_string));
    ProcessActionPlan {
        normalized_action: normalized_action.to_owned(),
        signal: signal.to_owned(),
        command: format!("/bin/kill {}", args.join(" ")),
        program: "/bin/kill".to_owned(),
        args,
        target_pids,
        dry_run_message: dry_run_message.to_owned(),
        success_message,
    }
}

fn renice_process_action_plan(
    normalized_action: &str,
    nice_value: i32,
    pid: u32,
    dry_run_message: &str,
    success_message: String,
) -> ProcessActionPlan {
    let args = vec![nice_value.to_string(), "-p".to_owned(), pid.to_string()];
    ProcessActionPlan {
        normalized_action: normalized_action.to_owned(),
        signal: format!("renice:{nice_value}"),
        command: format!("/usr/bin/renice {}", args.join(" ")),
        program: "/usr/bin/renice".to_owned(),
        args,
        target_pids: vec![pid],
        dry_run_message: dry_run_message.to_owned(),
        success_message,
    }
}

fn process_tree_target_pids(snapshot: Option<&SystemSnapshot>, pid: u32) -> Vec<u32> {
    let mut target_pids = snapshot
        .map(|snapshot| process_descendant_pids(snapshot, pid))
        .unwrap_or_default();
    target_pids.push(pid);
    target_pids.sort_unstable();
    target_pids.dedup();
    target_pids.retain(|target_pid| *target_pid != 0);
    target_pids
}

fn process_descendant_pids(snapshot: &SystemSnapshot, pid: u32) -> Vec<u32> {
    let mut descendants = Vec::new();
    let mut stack = vec![pid];
    while let Some(parent_pid) = stack.pop() {
        for child_pid in process_child_pids(snapshot, parent_pid) {
            if descendants.contains(&child_pid) {
                continue;
            }
            descendants.push(child_pid);
            stack.push(child_pid);
        }
    }
    descendants
}

fn process_action_diagnostics_event(report: &ProcessActionReport) -> DiagnosticsEvent {
    let mut builder = DiagnosticsEvent::builder(
        if report.success {
            DiagnosticsLevel::Info
        } else {
            DiagnosticsLevel::Warn
        },
        DiagnosticsSubsystem::Engine,
        "process-action",
        report.message.clone(),
    )
    .field("pid", report.pid)
    .field(
        "target_pids",
        report
            .target_pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(","),
    )
    .field("target_count", report.target_pids.len())
    .field("action", report.action.clone())
    .field("signal", report.signal.clone())
    .field("success", report.success)
    .field("command", report.command.clone());
    if let Some(reason) = report.reason.as_ref() {
        builder = builder.field("reason", reason);
    }
    if let Some(entity_id) = report.entity_id.as_ref() {
        builder = builder.entity_id(entity_id.clone());
    }
    if let Some(display_name) = report.display_name.as_ref() {
        builder = builder.field("display_name", display_name);
    }
    builder.build()
}

fn process_action_history_item(event: DiagnosticsEvent) -> ProcessActionHistoryItem {
    let display_name = diagnostics_field(&event, "display_name").map(str::to_owned);
    ProcessActionHistoryItem {
        timestamp_millis: event.timestamp_millis,
        pid: diagnostics_field(&event, "pid").and_then(|value| value.parse::<u32>().ok()),
        target_pids: diagnostics_field(&event, "target_pids")
            .map(parse_pid_list)
            .unwrap_or_default(),
        action: diagnostics_field(&event, "action").map(str::to_owned),
        signal: diagnostics_field(&event, "signal").map(str::to_owned),
        success: diagnostics_field(&event, "success")
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(
                event.level != DiagnosticsLevel::Warn && event.level != DiagnosticsLevel::Error,
            ),
        reason: diagnostics_field(&event, "reason").map(str::to_owned),
        entity_id: event.entity_id,
        display_name,
        message: event.message,
    }
}

fn parse_pid_list(value: &str) -> Vec<u32> {
    value
        .split(',')
        .filter_map(|part| part.trim().parse::<u32>().ok())
        .collect()
}

fn diagnostics_field<'a>(event: &'a DiagnosticsEvent, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.value.as_str())
}

fn run_os_command(program: &str, args: &[String]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("run {}: {error}", program))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if stderr.is_empty() {
            format!("{} exited with status {}", program, output.status)
        } else {
            format!("{} failed: {}", program, stderr)
        });
    }
    String::from_utf8(output.stdout).map_err(|error| format!("decode {} output: {error}", program))
}

fn parse_vmmap_regions(output: &str) -> Vec<MemoryRegionBreakdown> {
    output.lines().filter_map(parse_vmmap_region_line).collect()
}

fn parse_vmmap_region_line(line: &str) -> Option<MemoryRegionBreakdown> {
    let trimmed = line.trim_end();
    let bracket_start = trimmed.find('[')?;
    let bracket_end = trimmed[bracket_start..].find(']')? + bracket_start;
    let prefix = &trimmed[..bracket_start];
    let prefix_tokens = prefix.split_whitespace().collect::<Vec<_>>();
    let address_index = prefix_tokens.iter().position(|token| {
        token.contains('-')
            && token.split('-').all(|part| {
                !part.is_empty() && part.chars().all(|character| character.is_ascii_hexdigit())
            })
    })?;
    let region_type = prefix_tokens[..address_index].join(" ");
    if region_type.is_empty() || region_type == "REGION TYPE" {
        return None;
    }
    let stats = trimmed[bracket_start + 1..bracket_end]
        .split_whitespace()
        .collect::<Vec<_>>();
    if stats.len() < 4 {
        return None;
    }
    Some(MemoryRegionBreakdown {
        region_type,
        virtual_bytes: parse_vmmap_bytes(stats[0]),
        resident_bytes: parse_vmmap_bytes(stats[1]),
        dirty_bytes: parse_vmmap_bytes(stats[2]),
        swap_bytes: parse_vmmap_bytes(stats[3]),
    })
}

fn parse_vmmap_bytes(value: &str) -> u64 {
    let numeric = value.trim_end_matches(|character: char| character.is_ascii_alphabetic());
    let suffix = value[numeric.len()..].to_ascii_uppercase();
    let number = numeric.parse::<f64>().unwrap_or(0.0);
    let multiplier = match suffix.as_str() {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (number * multiplier).round() as u64
}

fn parse_sample_threads(output: &str) -> Vec<SampledStackReport> {
    let mut threads = Vec::new();
    let mut in_call_graph = false;
    let mut current: Option<(u32, String, Option<String>, Vec<String>)> = None;
    for line in output.lines() {
        if line.trim() == "Call graph:" {
            in_call_graph = true;
            continue;
        }
        if !in_call_graph {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        if let Some((sample_count, thread_label, queue_label)) = parse_sample_thread_header(line) {
            if let Some((sample_count, thread_label, queue_label, frames)) = current.take() {
                threads.push(sampled_stack_report(
                    sample_count,
                    thread_label,
                    queue_label,
                    frames,
                ));
            }
            current = Some((sample_count, thread_label, queue_label, Vec::new()));
            continue;
        }
        if let Some((_, _, _, frames)) = current.as_mut()
            && let Some(frame) = parse_sample_frame(line)
        {
            frames.push(frame);
        }
    }
    if let Some((sample_count, thread_label, queue_label, frames)) = current.take() {
        threads.push(sampled_stack_report(
            sample_count,
            thread_label,
            queue_label,
            frames,
        ));
    }
    threads
}

fn parse_sample_thread_header(line: &str) -> Option<(u32, String, Option<String>)> {
    let trimmed = line.trim_start();
    let mut parts = trimmed.split_whitespace();
    let sample_count = parts.next()?.parse::<u32>().ok()?;
    let rest = trimmed[trimmed.find(' ')?..].trim_start();
    if !rest.starts_with("Thread_") {
        return None;
    }
    let thread_label = rest.to_owned();
    let queue_label = rest
        .split_once(": ")
        .map(|(_, queue)| queue.trim_end_matches("  (serial)").trim().to_owned());
    Some((sample_count, thread_label, queue_label))
}

fn parse_sample_frame(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let frame = trimmed
        .trim_start_matches('+')
        .trim_start_matches('!')
        .trim_start();
    let mut parts = frame.split("  (in ");
    let symbol = parts.next()?.trim();
    if symbol.is_empty() {
        return None;
    }
    let symbol = symbol
        .split_once(' ')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(symbol);
    if symbol.is_empty() {
        None
    } else {
        Some(symbol.to_owned())
    }
}

fn sampled_stack_report(
    sample_count: u32,
    thread_label: String,
    queue_label: Option<String>,
    frames: Vec<String>,
) -> SampledStackReport {
    let top_frames = frames
        .into_iter()
        .filter(|frame| {
            !frame.contains("mach_msg")
                && !frame.contains("kevent")
                && !frame.contains("start")
                && !frame.contains("thread_start")
        })
        .take(6)
        .collect::<Vec<_>>();
    let classification = classify_sample_frames(&top_frames);
    SampledStackReport {
        thread_label,
        queue_label,
        sample_count,
        top_frames,
        classification,
    }
}

fn classify_sample_frames(frames: &[String]) -> String {
    let joined = frames.join(" ").to_ascii_lowercase();
    if joined.contains("cvdisplaylink") || joined.contains("displaylink") {
        "display-link".to_owned()
    } else if joined.contains("dispatchsourcetimer")
        || joined.contains("dispatch_source")
        || joined.contains("timer")
    {
        "timer".to_owned()
    } else if joined.contains("nsrunloop") || joined.contains("cfrunlooptimer") {
        "runloop-timer".to_owned()
    } else if joined.contains("recv")
        || joined.contains("send")
        || joined.contains("socket")
        || joined.contains("poll")
    {
        "io".to_owned()
    } else if joined.is_empty() {
        "idle".to_owned()
    } else {
        "cpu-work".to_owned()
    }
}

fn extract_tool_error_message(value: &Value) -> String {
    value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("tool request failed")
        .to_owned()
}

fn format_metric_value(metric: &str, value: f64) -> String {
    match metric {
        "memory_bytes" => format_bytes(value.max(0.0).round() as u64),
        "disk_activity_bps" | "network_activity_bps" => {
            format!("{}/s", format_bytes(value.max(0.0).round() as u64))
        }
        _ => format!("{value:.1}"),
    }
}

fn build_top_findings(
    snapshot: &SystemSnapshot,
    diagnostics: &DiagnosticsOverview,
    history: &HistorySummaryResponse,
    limit: usize,
) -> Vec<TopFinding> {
    let mut findings = Vec::new();
    if let Some(memory) = memory_pressure_finding(snapshot) {
        findings.push(memory);
    }
    if let Some(wakeups) = wakeup_finding(snapshot) {
        findings.push(wakeups);
    }
    if let Some(history_finding) = history_store_finding(history) {
        findings.push(history_finding);
    }
    if let Some(diagnostics_finding) = diagnostics_finding(diagnostics) {
        findings.push(diagnostics_finding);
    }

    for entity in top_entities(snapshot, 4) {
        findings.push(TopFinding {
            id: format!("entity:{}", entity.entity_id),
            severity: if entity.friction.total_score >= 20.0 {
                SeverityBand::Critical
            } else {
                SeverityBand::Warning
            },
            title: format!("{} is a top current friction source", entity.display_name),
            detail: format!(
                "{:.1}% CPU, {} resident, friction {:.1}. {}",
                entity.metrics.cpu_percent,
                format_bytes(entity.metrics.memory_resident_bytes),
                entity.friction.total_score,
                entity
                    .recent_change_summary
                    .clone()
                    .unwrap_or_else(|| "No recent change summary is attached.".to_owned())
            ),
            source: "entity".to_owned(),
            entity_ids: vec![entity.entity_id.clone()],
            recommendation: entity.recommendations.first().map(|recommendation| {
                format!("{}: {}", recommendation.title, recommendation.detail)
            }),
        });
    }

    findings.sort_by(|left, right| {
        right
            .severity
            .score()
            .cmp(&left.severity.score())
            .then_with(|| left.id.cmp(&right.id))
    });
    findings.truncate(limit.max(1));
    findings
}

fn build_host_alerts(snapshot: &SystemSnapshot, top_entity_limit: usize) -> Vec<HostAlert> {
    let mut alerts = Vec::new();
    let used_ratio = if snapshot.host.memory_total_bytes == 0 {
        0.0
    } else {
        snapshot.host.memory_used_bytes as f64 / snapshot.host.memory_total_bytes as f64
    };
    let top_memory_entities = top_memory_entities(snapshot, top_entity_limit);
    let top_external_memory = top_external_memory_entities(snapshot, top_entity_limit);
    let top_memory_entity_labels = if top_external_memory.is_empty() {
        format_entity_burden_labels(&top_memory_entities, |entity| {
            format_bytes(entity.metrics.memory_resident_bytes)
        })
    } else {
        format_entity_burden_labels(&top_external_memory, |entity| {
            format_bytes(entity.metrics.memory_resident_bytes)
        })
    };
    if used_ratio >= MEMORY_PRESSURE_WARNING_RATIO
        || snapshot.host.compressed_memory_bytes >= COMPRESSED_MEMORY_WARNING_BYTES
        || snapshot.host.swap_used_bytes >= SWAP_WARNING_BYTES
    {
        let severity = if used_ratio >= MEMORY_PRESSURE_CRITICAL_RATIO
            || snapshot.host.compressed_memory_bytes >= COMPRESSED_MEMORY_CRITICAL_BYTES
            || snapshot.host.swap_used_bytes >= SWAP_CRITICAL_BYTES
        {
            SeverityBand::Critical
        } else {
            SeverityBand::Warning
        };
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "memory_used_bytes".to_owned(),
            json!(snapshot.host.memory_used_bytes),
        );
        metrics.insert(
            "memory_total_bytes".to_owned(),
            json!(snapshot.host.memory_total_bytes),
        );
        metrics.insert(
            "compressed_memory_bytes".to_owned(),
            json!(snapshot.host.compressed_memory_bytes),
        );
        metrics.insert(
            "swap_used_bytes".to_owned(),
            json!(snapshot.host.swap_used_bytes),
        );
        alerts.push(HostAlert {
            id: "host-memory-pressure".to_owned(),
            severity,
            category: "memory-pressure".to_owned(),
            title: "Host memory pressure is elevated".to_owned(),
            detail: format!(
                "{} used of {}, {} compressed, {} swap. Top current groups: {}.",
                format_bytes(snapshot.host.memory_used_bytes),
                format_bytes(snapshot.host.memory_total_bytes),
                format_bytes(snapshot.host.compressed_memory_bytes),
                format_bytes(snapshot.host.swap_used_bytes),
                if top_memory_entity_labels.is_empty() {
                    "none".to_owned()
                } else if top_external_memory.is_empty() {
                    format!(
                        "no non-Aetower leader visible; current leaders {top_memory_entity_labels}"
                    )
                } else {
                    format!("external leaders {top_memory_entity_labels}")
                }
            ),
            metrics,
            entity_ids: if top_external_memory.is_empty() {
                top_memory_entities
            } else {
                top_external_memory
            }
            .into_iter()
            .map(|entity| entity.entity_id.clone())
            .collect(),
        });
    }

    if snapshot.host.wakeups_per_second >= WAKEUPS_WARNING {
        let severity = if snapshot.host.wakeups_per_second >= WAKEUPS_CRITICAL {
            SeverityBand::Critical
        } else {
            SeverityBand::Warning
        };
        let external_wakeup_leaders = top_external_wakeup_entities(snapshot, top_entity_limit);
        let wakeup_leaders = top_wakeup_entities(snapshot, top_entity_limit);
        let leader = external_wakeup_leaders
            .first()
            .copied()
            .or_else(|| wakeup_leaders.first().copied());
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "host_wakeups_per_second".to_owned(),
            json!(snapshot.host.wakeups_per_second),
        );
        if let Some(leader) = leader {
            metrics.insert(
                "leader_wakeups_per_second".to_owned(),
                json!(leader.metrics.wakeups_per_second),
            );
        }
        alerts.push(HostAlert {
            id: "host-wakeup-storm".to_owned(),
            severity,
            category: "wakeups".to_owned(),
            title: "Wakeup rate is high".to_owned(),
            detail: leader.map_or_else(
                || format!(
                    "Host wakeups are {:.0}/s with no single entity leader identified.",
                    snapshot.host.wakeups_per_second
                ),
                |leader| {
                    if is_aetower_entity(leader) {
                        format!(
                            "Host wakeups are {:.0}/s. No non-Aetower wakeup leader is visible; Aetower leads at {:.0}/s, so check self telemetry and MCP request rate.",
                            snapshot.host.wakeups_per_second,
                            leader.metrics.wakeups_per_second
                        )
                    } else {
                        format!(
                            "Host wakeups are {:.0}/s. External leader {} is at {:.0}/s.",
                            snapshot.host.wakeups_per_second,
                            leader.display_name,
                            leader.metrics.wakeups_per_second
                        )
                    }
                },
            ),
            metrics,
            entity_ids: leader
                .map(|entity| vec![entity.entity_id.clone()])
                .unwrap_or_default(),
        });
    }

    alerts
}

fn build_recent_changes(
    snapshot: &SystemSnapshot,
    window_millis: u64,
    limit: usize,
) -> Vec<RecentChangeItem> {
    let since = snapshot.captured_at_millis.saturating_sub(window_millis);
    let mut changes = snapshot
        .timeline
        .iter()
        .filter(|event| event.timestamp_millis >= since)
        .map(|event| RecentChangeItem {
            timestamp_millis: event.timestamp_millis,
            severity: match event.severity {
                aetower_model::TimelineSeverity::Info => SeverityBand::Info,
                aetower_model::TimelineSeverity::Warning => SeverityBand::Warning,
                aetower_model::TimelineSeverity::Critical => SeverityBand::Critical,
            },
            source: format!("timeline:{:?}", event.category).to_lowercase(),
            entity_id: event.entity_id.clone(),
            title: event.title.clone(),
            detail: event.detail.clone(),
        })
        .collect::<Vec<_>>();

    for entity in &snapshot.entities {
        if let Some(summary) = &entity.recent_change_summary {
            changes.push(RecentChangeItem {
                timestamp_millis: snapshot.captured_at_millis,
                severity: if entity.anomaly_detected {
                    SeverityBand::Warning
                } else {
                    SeverityBand::Info
                },
                source: "entity-summary".to_owned(),
                entity_id: Some(entity.entity_id.clone()),
                title: format!("{} changed recently", entity.display_name),
                detail: summary.clone(),
            });
        }
    }

    changes.sort_by(|left, right| {
        right
            .timestamp_millis
            .cmp(&left.timestamp_millis)
            .then_with(|| right.severity.score().cmp(&left.severity.score()))
    });
    changes.truncate(limit.max(1));
    changes
}

fn build_capability_status(snapshot: &SystemSnapshot) -> Vec<CapabilityStatusItem> {
    let mut capabilities = snapshot
        .capabilities
        .iter()
        .map(|capability| CapabilityStatusItem {
            kind: format!("{:?}", capability.kind),
            state: format!("{:?}", capability.state),
            health: format!("{:?}", capability.health),
            operator_label: capability_operator_label(capability),
            action_label: capability_action_label(capability),
            detail: capability.detail.clone(),
            last_updated_millis: capability.last_updated_millis,
            severity: capability_severity(capability),
        })
        .collect::<Vec<_>>();
    capabilities.sort_by(|left, right| {
        right
            .severity
            .score()
            .cmp(&left.severity.score())
            .then_with(|| left.kind.cmp(&right.kind))
    });
    capabilities
}

fn build_history_store_health(
    summary: HistorySummaryResponse,
    recent_history_events: Vec<DiagnosticsEvent>,
    data_quality: Option<HistoryDataQualityReport>,
) -> HistoryStoreHealth {
    let severity = data_quality
        .as_ref()
        .map(|quality| {
            if quality.severity.score() > history_store_severity(&summary).score() {
                quality.severity
            } else {
                history_store_severity(&summary)
            }
        })
        .unwrap_or_else(|| history_store_severity(&summary));
    let mut thresholds = BTreeMap::new();
    thresholds.insert(
        "warning_store_bytes".to_owned(),
        json!(HISTORY_STORE_WARNING_BYTES),
    );
    thresholds.insert(
        "critical_store_bytes".to_owned(),
        json!(HISTORY_STORE_CRITICAL_BYTES),
    );
    thresholds.insert(
        "warning_wal_bytes".to_owned(),
        json!(HISTORY_WAL_WARNING_BYTES),
    );
    thresholds.insert(
        "critical_wal_bytes".to_owned(),
        json!(HISTORY_WAL_CRITICAL_BYTES),
    );
    thresholds.insert(
        "warning_quarantine_count".to_owned(),
        json!(HISTORY_QUARANTINE_WARNING),
    );
    thresholds.insert(
        "critical_quarantine_count".to_owned(),
        json!(HISTORY_QUARANTINE_CRITICAL),
    );
    let mut recommendations = Vec::new();
    if summary.store_bytes >= HISTORY_STORE_WARNING_BYTES {
        recommendations.push(
            "Trim persisted history more aggressively or shorten the default retention window."
                .to_owned(),
        );
    }
    if summary.wal_bytes >= HISTORY_WAL_WARNING_BYTES {
        recommendations.push(
            "Investigate write pressure and ensure checkpointing is keeping WAL growth under control."
                .to_owned(),
        );
    }
    if summary.quarantine_count >= HISTORY_QUARANTINE_WARNING {
        recommendations.push(
            "Inspect persisted history compatibility issues; quarantined rows should stay near zero."
                .to_owned(),
        );
    }
    if let Some(quality) = data_quality.as_ref()
        && quality.severity != SeverityBand::Info
    {
        recommendations.extend(quality.recommendations.clone());
    }
    HistoryStoreHealth {
        severity,
        summary: format!(
            "{} DB, {} WAL, {} persisted snapshots, {} quarantined rows.",
            format_bytes(summary.store_bytes),
            format_bytes(summary.wal_bytes),
            summary.snapshot_count,
            summary.quarantine_count
        ),
        range: summary,
        thresholds,
        data_quality,
        recent_history_events,
        recommendations,
    }
}

fn build_support_bundle_manifest(
    privacy_tier: ExportPrivacyTier,
    snapshot: SystemSnapshot,
    runtime_lag: RuntimeLagMetrics,
    diagnostics: DiagnosticsOverview,
    history: HistorySummaryResponse,
    diagnostic_events: Vec<DiagnosticsEvent>,
) -> Result<Vec<SupportBundleSectionManifest>, Value> {
    let sections = vec![
        section_manifest(
            "current_snapshot",
            "Live snapshot of host, capabilities, and current entities.",
            &snapshot,
            privacy_tier,
        )?,
        section_manifest(
            "runtime_lag",
            "Self-observability metrics for Aetower itself.",
            &runtime_lag,
            privacy_tier,
        )?,
        section_manifest(
            "diagnostics_overview",
            "Diagnostics ring and persistence overview.",
            &diagnostics,
            privacy_tier,
        )?,
        section_manifest(
            "history_summary",
            "Persisted history store range summary.",
            &history,
            privacy_tier,
        )?,
        section_manifest(
            "diagnostics_events",
            "Recent diagnostics events included in the support bundle payload.",
            &diagnostic_events,
            privacy_tier,
        )?,
    ];
    Ok(sections)
}

fn build_diagnostics_summary_report(
    overview: DiagnosticsOverview,
    events: Vec<DiagnosticsEvent>,
    options: DiagnosticsSummaryOptions,
) -> DiagnosticsSummaryReport {
    let event_count = events.len();
    let mut groups = BTreeMap::<(String, String, String), DiagnosticsSummaryGroup>::new();

    for event in events {
        let subsystem_label = diagnostics_subsystem_label(&event.subsystem);
        let level_label = diagnostics_level_label(&event.level);
        let key = (
            subsystem_label.clone(),
            event.event_type.clone(),
            level_label.clone(),
        );
        let detail = diagnostic_field_value(&event, "detail");
        let sample_fields = diagnostic_sample_fields(&event);
        groups
            .entry(key)
            .and_modify(|group| {
                group.count = group.count.saturating_add(1);
                group.first_millis = group.first_millis.min(event.timestamp_millis);
                if event.timestamp_millis >= group.latest_millis {
                    group.latest_millis = event.timestamp_millis;
                    group.latest_message = event.message.clone();
                    group.latest_detail = detail.clone();
                    group.sample_fields = sample_fields.clone();
                }
            })
            .or_insert_with(|| DiagnosticsSummaryGroup {
                subsystem: subsystem_label,
                event_type: event.event_type,
                level: level_label,
                count: 1,
                first_millis: event.timestamp_millis,
                latest_millis: event.timestamp_millis,
                latest_message: event.message,
                latest_detail: detail,
                sample_fields,
            });
    }

    let mut groups = groups.into_values().collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| right.latest_millis.cmp(&left.latest_millis))
            .then_with(|| left.event_type.cmp(&right.event_type))
    });
    groups.truncate(options.limit.max(1));
    let recommendations = diagnostics_summary_recommendations(&overview, event_count, &groups);

    DiagnosticsSummaryReport {
        overview,
        event_count,
        limit: options.limit.max(1),
        since_millis: options.since_millis,
        include_persisted: options.include_persisted,
        minimum_level: options.minimum_level,
        subsystem: options.subsystem,
        search: options.search,
        groups,
        recommendations,
    }
}

fn diagnostics_summary_recommendations(
    overview: &DiagnosticsOverview,
    event_count: usize,
    groups: &[DiagnosticsSummaryGroup],
) -> Vec<String> {
    let mut recommendations = Vec::new();
    if overview.error_count >= DIAGNOSTICS_ERROR_WARNING {
        recommendations.push(format!(
            "{} retained diagnostics errors are present; start with the top error-level group before paging raw rows.",
            overview.error_count
        ));
    }
    if overview.warn_count >= DIAGNOSTICS_WARN_WARNING {
        recommendations.push(format!(
            "{} retained diagnostics warnings are present; use the grouped summary to separate repeated noise from new failures.",
            overview.warn_count
        ));
    }
    if let Some(group) = groups.first()
        && group.count.saturating_mul(2) >= event_count.max(1)
    {
        recommendations.push(format!(
            "{}:{} dominates the sampled diagnostics with {} of {} event(s).",
            group.subsystem, group.event_type, group.count, event_count
        ));
    }
    if groups
        .iter()
        .any(|group| group.event_type == "host-incident-snapshot")
    {
        recommendations.push(
            "Host incident snapshots are present; correlate them with host alerts and top external burden leaders before blaming Aetower."
                .to_owned(),
        );
    }
    if groups.iter().any(|group| {
        group.event_type == "mcp-helper-reaped" || group.event_type == "mcp-helper-lifecycle"
    }) {
        recommendations.push(
            "MCP helper lifecycle diagnostics appeared; verify clients disconnect cleanly and watch helper count plus oldest-helper age in session health."
                .to_owned(),
        );
    }
    if recommendations.is_empty() {
        recommendations.push(
            "Diagnostics groups are currently low-signal; query raw diagnostics only for the event types that matter."
                .to_owned(),
        );
    }
    recommendations
}

fn diagnostics_level_label(level: &DiagnosticsLevel) -> String {
    serde_json::to_value(level)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{level:?}").to_ascii_lowercase())
}

fn diagnostics_subsystem_label(subsystem: &aetower_diagnostics::DiagnosticsSubsystem) -> String {
    serde_json::to_value(subsystem)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{subsystem:?}").to_ascii_lowercase())
}

fn diagnostic_field_value(event: &DiagnosticsEvent, key: &str) -> Option<String> {
    event
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.value.clone())
}

fn diagnostic_sample_fields(event: &DiagnosticsEvent) -> BTreeMap<String, String> {
    event
        .fields
        .iter()
        .take(8)
        .map(|field| (field.key.clone(), field.value.clone()))
        .collect()
}

fn host_memory_pressure_guidance(snapshot: &SystemSnapshot, runtime: &RuntimeLagMetrics) -> String {
    let external = top_external_memory_entities(snapshot, 4);
    let external_labels = format_entity_burden_labels(&external, |entity| {
        format_bytes(entity.metrics.memory_resident_bytes)
    });
    let observer_context = format!(
        "Aetower self is CPU {:.1}%, memory {}, wakeups {:.0}/s.",
        runtime.self_cpu_percent,
        format_bytes(runtime.self_memory_bytes),
        runtime.self_wakeups_per_second
    );
    if external_labels.is_empty() {
        format!(
            "Compression ({}) and swap ({}) are elevated. No non-Aetower memory leader is visible in the current snapshot; inspect host-level pressure and only then Aetower self. {}",
            format_bytes(snapshot.host.compressed_memory_bytes),
            format_bytes(snapshot.host.swap_used_bytes),
            observer_context
        )
    } else {
        format!(
            "Compression ({}) and swap ({}) are elevated. Start with external memory leaders: {}. {}",
            format_bytes(snapshot.host.compressed_memory_bytes),
            format_bytes(snapshot.host.swap_used_bytes),
            external_labels,
            observer_context
        )
    }
}

fn host_wakeup_guidance(snapshot: &SystemSnapshot, runtime: &RuntimeLagMetrics) -> String {
    let external = top_external_wakeup_entities(snapshot, 4);
    let external_labels = format_entity_burden_labels(&external, |entity| {
        format!("{:.0}/s", entity.metrics.wakeups_per_second)
    });
    let observer_context = format!(
        "Aetower self wakeups are {:.0}/s with MCP {:.1} req/s.",
        runtime.self_wakeups_per_second, runtime.mcp_requests_per_second
    );
    if external_labels.is_empty() {
        format!(
            "Host wakeups are {:.0}/s. No non-Aetower wakeup leader is visible; inspect system services and Aetower self telemetry next. {}",
            snapshot.host.wakeups_per_second, observer_context
        )
    } else {
        format!(
            "Host wakeups are {:.0}/s. Start with external wakeup leaders: {}. {}",
            snapshot.host.wakeups_per_second, external_labels, observer_context
        )
    }
}

fn host_load_detail(snapshot: &SystemSnapshot) -> String {
    let external = top_entities(snapshot, snapshot.entities.len().max(1))
        .into_iter()
        .filter(|entity| !is_aetower_entity(entity))
        .take(3)
        .collect::<Vec<_>>();
    if external.is_empty() {
        let fallback = top_entities(snapshot, 3)
            .into_iter()
            .map(|entity| entity.display_name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        if fallback.is_empty() {
            "No current burden leaders are visible.".to_owned()
        } else {
            format!("No non-Aetower burden leader is visible. Current leaders: {fallback}.")
        }
    } else {
        format!(
            "Top non-Aetower burden leaders: {}.",
            external
                .into_iter()
                .map(|entity| entity.display_name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn build_recommendations(
    snapshot: &SystemSnapshot,
    diagnostics: &DiagnosticsOverview,
    runtime: &RuntimeLagMetrics,
    history: &HistorySummaryResponse,
    limit: usize,
) -> Vec<RecommendationItem> {
    let mut recommendations = Vec::new();
    if memory_pressure_finding(snapshot).is_some() {
        recommendations.push(RecommendationItem {
            severity: SeverityBand::Critical,
            title: "Reduce current memory pressure".to_owned(),
            detail: host_memory_pressure_guidance(snapshot, runtime),
            entity_id: top_external_memory_entities(snapshot, 1)
                .into_iter()
                .next()
                .or_else(|| top_memory_entities(snapshot, 1).into_iter().next())
                .map(|entity| entity.entity_id.clone()),
            source: "host".to_owned(),
            expected_benefit: "Lower swap, better responsiveness, and less battery drain."
                .to_owned(),
        });
    }
    if wakeup_finding(snapshot).is_some() {
        recommendations.push(RecommendationItem {
            severity: SeverityBand::Warning,
            title: "Reduce wakeup-heavy workloads".to_owned(),
            detail: host_wakeup_guidance(snapshot, runtime),
            entity_id: top_external_wakeup_entities(snapshot, 1)
                .into_iter()
                .next()
                .or_else(|| top_wakeup_entities(snapshot, 1).into_iter().next())
                .map(|entity| entity.entity_id.clone()),
            source: "host".to_owned(),
            expected_benefit: "Lower battery drain and smoother foreground interactivity."
                .to_owned(),
        });
    }
    if history_store_finding(history).is_some() {
        recommendations.push(RecommendationItem {
            severity: history_store_severity(history),
            title: "Keep the persisted history store under control".to_owned(),
            detail: format!(
                "The history store is {} with {} WAL and {} quarantined rows.",
                format_bytes(history.store_bytes),
                format_bytes(history.wal_bytes),
                history.quarantine_count
            ),
            entity_id: None,
            source: "history".to_owned(),
            expected_benefit: "Faster History loads and healthier long-run storage behavior."
                .to_owned(),
        });
    }
    if diagnostics_finding(diagnostics).is_some() {
        recommendations.push(RecommendationItem {
            severity: diagnostics_severity(diagnostics),
            title: "Reduce diagnostics churn".to_owned(),
            detail: format!(
                "{} warnings and {} errors are currently retained in the diagnostics ring.",
                diagnostics.warn_count, diagnostics.error_count
            ),
            entity_id: None,
            source: "diagnostics".to_owned(),
            expected_benefit: "Cleaner operator signal and less noisy support output.".to_owned(),
        });
    }
    if mcp_helper_severity(runtime) != SeverityBand::Info {
        recommendations.push(RecommendationItem {
            severity: mcp_helper_severity(runtime),
            title: "Clean up stale MCP helper processes".to_owned(),
            detail: format!(
                "{} helper processes are currently visible, {} of them older than {} minutes.",
                runtime.mcp_helper_count,
                runtime.stale_mcp_helper_count,
                MCP_HELPER_STALE_MILLIS / 60_000
            ),
            entity_id: None,
            source: "mcp".to_owned(),
            expected_benefit: "Lower idle overhead and more trustworthy local-MCP session health."
                .to_owned(),
        });
    }
    if self_runtime_severity(runtime) != SeverityBand::Info {
        recommendations.push(RecommendationItem {
            severity: self_runtime_severity(runtime),
            title: "Reduce Aetower observer overhead".to_owned(),
            detail: self_runtime_recommendation_detail(runtime),
            entity_id: None,
            source: "aetower-self".to_owned(),
            expected_benefit:
                "Lower observer-induced load and more trustworthy performance investigations."
                    .to_owned(),
        });
    }
    for entity in top_entities(snapshot, limit) {
        for recommendation in entity.recommendations.iter().take(2) {
            recommendations.push(RecommendationItem {
                severity: if entity.anomaly_detected {
                    SeverityBand::Warning
                } else {
                    SeverityBand::Info
                },
                title: recommendation.title.clone(),
                detail: recommendation.detail.clone(),
                entity_id: Some(entity.entity_id.clone()),
                source: entity.display_name.clone(),
                expected_benefit:
                    "Higher-confidence attribution and lower friction in the affected group."
                        .to_owned(),
            });
        }
    }
    recommendations.sort_by(|left, right| {
        right
            .severity
            .score()
            .cmp(&left.severity.score())
            .then_with(|| left.title.cmp(&right.title))
    });
    recommendations.truncate(limit.max(1));
    recommendations
}

fn build_session_health_checks(
    snapshot: &SystemSnapshot,
    diagnostics: &DiagnosticsOverview,
    runtime: &RuntimeLagMetrics,
    history: &HistorySummaryResponse,
) -> Vec<SessionHealthCheck> {
    vec![
        SessionHealthCheck {
            key: "runtime".to_owned(),
            severity: runtime_severity(runtime),
            summary: format!(
                "Engine tick {:.1} ms against a {:.0} ms target.",
                runtime.engine_tick_millis, runtime.target_tick_millis
            ),
            detail: format!(
                "Collect {:.1} ms, history {:.1} ms, persist {:.1} ms, history queue {}, diagnostics queue {}, MCP helpers {} ({} stale). Aetower self: CPU {:.1}%, memory {}, wakeups {:.0}/s.",
                runtime.collect_millis,
                runtime.history_millis,
                runtime.persist_millis,
                runtime.history_queue_depth,
                runtime.diagnostics_queue_depth,
                runtime.mcp_helper_count,
                runtime.stale_mcp_helper_count,
                runtime.self_cpu_percent,
                format_bytes(runtime.self_memory_bytes),
                runtime.self_wakeups_per_second
            ),
        },
        SessionHealthCheck {
            key: "diagnostics".to_owned(),
            severity: diagnostics_severity(diagnostics),
            summary: format!(
                "{} warnings, {} errors, {} persisted diagnostics events.",
                diagnostics.warn_count, diagnostics.error_count, diagnostics.persisted_events
            ),
            detail: diagnostics_active_error_message(diagnostics).unwrap_or_else(|| {
                if diagnostics.error_count > 0 {
                    "Retained diagnostics errors are stale or already recovered.".to_owned()
                } else {
                    "No current diagnostics error is attached.".to_owned()
                }
            }),
        },
        SessionHealthCheck {
            key: "history-store".to_owned(),
            severity: history_store_severity(history),
            summary: format!(
                "{} DB, {} WAL, {} persisted snapshots.",
                format_bytes(history.store_bytes),
                format_bytes(history.wal_bytes),
                history.snapshot_count
            ),
            detail: format!(
                "{} quarantined rows across the current store window.",
                history.quarantine_count
            ),
        },
        SessionHealthCheck {
            key: "capabilities".to_owned(),
            severity: snapshot
                .capabilities
                .iter()
                .map(capability_severity)
                .max_by_key(|severity| severity.score())
                .unwrap_or(SeverityBand::Info),
            summary: format!(
                "{} capability checks are currently tracked.",
                snapshot.capabilities.len()
            ),
            detail: snapshot
                .capabilities
                .iter()
                .map(|capability| format!("{:?}: {:?}", capability.kind, capability.state))
                .collect::<Vec<_>>()
                .join(", "),
        },
        SessionHealthCheck {
            key: "host-load".to_owned(),
            severity: host_load_severity(snapshot),
            summary: format!(
                "{} used / {}, {} compressed, {} swap, {:.0} wakeups/s.",
                format_bytes(snapshot.host.memory_used_bytes),
                format_bytes(snapshot.host.memory_total_bytes),
                format_bytes(snapshot.host.compressed_memory_bytes),
                format_bytes(snapshot.host.swap_used_bytes),
                snapshot.host.wakeups_per_second
            ),
            detail: host_load_detail(snapshot),
        },
        SessionHealthCheck {
            key: "mcp".to_owned(),
            severity: mcp_helper_severity(runtime),
            summary: if runtime.mcp_helper_count == 0 {
                format!(
                    "The local in-app MCP server is serving the current app-owned engine state with {} active client(s).",
                    runtime.mcp_active_client_count
                )
            } else {
                format!(
                    "The local in-app MCP server is serving current engine state with {} helper processes ({} stale) and {} active client(s).",
                    runtime.mcp_helper_count,
                    runtime.stale_mcp_helper_count,
                    runtime.mcp_active_client_count
                )
            },
            detail: if runtime.stale_mcp_helper_count > 0 {
                format!(
                    "{} helper processes have been alive for at least {} minutes; oldest helper age is {}. Helpers should exit when clients disconnect.",
                    runtime.stale_mcp_helper_count,
                    MCP_HELPER_STALE_MILLIS / 60_000,
                    format_duration_millis(runtime.oldest_mcp_helper_age_millis)
                )
            } else if runtime.mcp_helper_count > 0 {
                format!(
                    "Oldest helper age is {}; helpers older than {} minutes become lifecycle signals. Total MCP requests: {}, current rate: {:.1}/s.",
                    format_duration_millis(runtime.oldest_mcp_helper_age_millis),
                    MCP_HELPER_STALE_MILLIS / 60_000,
                    runtime.mcp_total_requests,
                    runtime.mcp_requests_per_second
                )
            } else {
                format!(
                    "Agents should consume these tools instead of starting a second collector process. Total MCP requests: {}, current rate: {:.1}/s.",
                    runtime.mcp_total_requests, runtime.mcp_requests_per_second
                )
            },
        },
    ]
}

struct AiRuntimeReportData {
    summary: AiRuntimeSummary,
    burden_leaders: Vec<AiBurdenLeaderReport>,
    runtime_groups: Vec<AiRuntimeGroupReport>,
    approvals: Vec<AiApprovalReport>,
    delegations: Vec<AiDelegationReport>,
    recent_changes: Vec<RecentChangeItem>,
    historical_groups: Vec<AiHistoricalTrendReport>,
    recommendations: Vec<RecommendationItem>,
}

fn build_ai_runtime_report(
    snapshot: &SystemSnapshot,
    history: &[SystemSnapshot],
) -> AiRuntimeReportData {
    let ai_entities = snapshot
        .entities
        .iter()
        .filter(|entity| matches!(entity.entity_kind, aetower_model::EntityKind::AiAgent))
        .collect::<Vec<_>>();

    let runtime_groups = ai_runtime_groups(&ai_entities);
    let group_keys = runtime_groups
        .iter()
        .map(|group| (group.provider.clone(), group.workspace.clone()))
        .collect::<Vec<_>>();

    AiRuntimeReportData {
        summary: AiRuntimeSummary {
            agent_count: ai_entities.len(),
            total_energy_nj_per_s: ai_entities
                .iter()
                .map(|entity| entity.metrics.energy_nj_per_s)
                .sum(),
            total_cost_usd: ai_entities
                .iter()
                .filter_map(|entity| entity.agent_cost.as_ref().map(|cost| cost.cost_usd))
                .sum(),
            total_session_energy_nj: ai_entities
                .iter()
                .filter_map(|entity| {
                    entity
                        .agent_cost
                        .as_ref()
                        .map(|cost| cost.session_energy_nj)
                })
                .sum(),
            host_gpu_percent: snapshot.host.gpu_percent,
            host_gpu_memory_unified_percent: if snapshot.host.memory_total_bytes == 0 {
                0.0
            } else {
                snapshot.host.gpu_memory_bytes as f64 / snapshot.host.memory_total_bytes as f64
                    * 100.0
            },
        },
        burden_leaders: ai_burden_leaders(&ai_entities),
        approvals: ai_approval_queue(&ai_entities),
        delegations: ai_delegations(&ai_entities),
        recent_changes: ai_recent_changes(snapshot, &ai_entities),
        historical_groups: ai_historical_groups(history, &group_keys),
        recommendations: ai_recommendations(&ai_entities),
        runtime_groups,
    }
}

fn load_ai_runtime_history(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
    limit: u32,
) -> (Vec<SystemSnapshot>, Option<String>) {
    match data_source.load_history_page(start_millis, end_millis, None, limit) {
        Ok(history) => (history, None),
        Err(error) => (
            Vec::new(),
            Some(format!(
                "Historical AI runtime trends are temporarily unavailable: {error}"
            )),
        ),
    }
}

fn build_export_query_response(
    data_source: &dyn AetowerMcpDataSource,
    mut snapshot: SystemSnapshot,
    options: ExportQueryOptions<'_>,
) -> Result<Value, String> {
    if !options.entity_ids.is_empty() {
        let ids = options.entity_ids.iter().cloned().collect::<BTreeSet<_>>();
        snapshot
            .entities
            .retain(|entity| ids.contains(&entity.entity_id));
        snapshot
            .timeline
            .retain(|event| match event.entity_id.as_ref() {
                Some(entity_id) => ids.contains(entity_id),
                None => true,
            });
    }

    let mut payload = serde_json::Map::new();
    payload.insert("privacyTier".to_owned(), json!(options.privacy_tier));
    payload.insert("startMillis".to_owned(), json!(options.start_millis));
    payload.insert("endMillis".to_owned(), json!(options.end_millis));

    if options.include_snapshot {
        payload.insert(
            "snapshot".to_owned(),
            export_controlled_json(
                serde_json::to_value(&snapshot).map_err(|error| error.to_string())?,
                options.privacy_tier,
            ),
        );
    }

    if options.include_history {
        let summary =
            data_source.history_range_summary(options.start_millis, options.end_millis)?;
        let page = data_source.load_history_page(
            options.start_millis,
            options.end_millis,
            None,
            options.history_limit,
        )?;
        payload.insert(
            "history".to_owned(),
            export_controlled_json(
                json!({
                    "summary": summary,
                    "snapshots": page,
                }),
                options.privacy_tier,
            ),
        );
    }

    if options.include_diagnostics {
        let events = data_source.query_diagnostics(DiagnosticsQuery {
            limit: options.diagnostics_limit,
            minimum_level: None,
            subsystem: None,
            search: None,
            since_millis: Some(options.start_millis),
            include_persisted: true,
        })?;
        payload.insert(
            "diagnostics".to_owned(),
            export_controlled_json(json!(events), options.privacy_tier),
        );
    }

    if options.include_session_health {
        let diagnostics = data_source.diagnostics_overview()?;
        let runtime = data_source.latest_runtime_lag_metrics()?;
        let history =
            data_source.history_range_summary(options.start_millis, options.end_millis)?;
        let checks = build_session_health_checks(&snapshot, &diagnostics, &runtime, &history);
        payload.insert(
            "sessionHealth".to_owned(),
            export_controlled_json(
                serde_json::to_value(checks).map_err(|error| error.to_string())?,
                options.privacy_tier,
            ),
        );
    }

    if options.include_ai_runtime_report {
        let (history_page, history_warning) = load_ai_runtime_history(
            data_source,
            options.start_millis,
            options.end_millis,
            options.history_limit,
        );
        let report = build_ai_runtime_report(&snapshot, &history_page);
        payload.insert(
            "aiRuntimeReport".to_owned(),
            export_controlled_json(
                serde_json::to_value(json!({
                    "historyStatus": if history_warning.is_some() { "degraded" } else { "ok" },
                    "historyWarning": history_warning,
                    "summary": report.summary,
                    "burdenLeaders": report.burden_leaders,
                    "runtimeGroups": report.runtime_groups,
                    "approvals": report.approvals,
                    "delegations": report.delegations,
                    "recentChanges": report.recent_changes,
                    "historicalGroups": report.historical_groups,
                    "recommendations": report.recommendations,
                }))
                .map_err(|error| error.to_string())?,
                options.privacy_tier,
            ),
        );
    }

    Ok(Value::Object(payload))
}

fn section_manifest<T: Serialize>(
    name: &str,
    description: &str,
    value: &T,
    privacy_tier: ExportPrivacyTier,
) -> Result<SupportBundleSectionManifest, Value> {
    let redacted = privacy_tier != ExportPrivacyTier::Full;
    let json = serde_json::to_value(value)
        .map_err(|error| tool_error(format!("serialize manifest section: {error}")))?;
    let controlled = export_controlled_json(json, privacy_tier);
    let encoded = serde_json::to_vec(&controlled)
        .map_err(|error| tool_error(format!("encode manifest section: {error}")))?;
    Ok(SupportBundleSectionManifest {
        name: name.to_owned(),
        estimated_bytes: encoded.len(),
        redacted,
        description: description.to_owned(),
    })
}

fn top_entities(snapshot: &SystemSnapshot, limit: usize) -> Vec<&aetower_model::EntitySnapshot> {
    let mut entities = snapshot.entities.iter().collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        right
            .friction
            .total_score
            .partial_cmp(&left.friction.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entities.truncate(limit.max(1));
    entities
}

fn top_memory_entities(
    snapshot: &SystemSnapshot,
    limit: usize,
) -> Vec<&aetower_model::EntitySnapshot> {
    let mut entities = snapshot
        .entities
        .iter()
        .filter(|entity| entity.metrics.memory_resident_bytes > 0)
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        right
            .metrics
            .memory_resident_bytes
            .cmp(&left.metrics.memory_resident_bytes)
    });
    entities.truncate(limit.max(1));
    entities
}

fn top_external_memory_entities(
    snapshot: &SystemSnapshot,
    limit: usize,
) -> Vec<&aetower_model::EntitySnapshot> {
    let mut entities = top_memory_entities(snapshot, snapshot.entities.len().max(1))
        .into_iter()
        .filter(|entity| !is_aetower_entity(entity))
        .collect::<Vec<_>>();
    entities.truncate(limit.max(1));
    entities
}

fn top_wakeup_entities(
    snapshot: &SystemSnapshot,
    limit: usize,
) -> Vec<&aetower_model::EntitySnapshot> {
    let mut entities = snapshot
        .entities
        .iter()
        .filter(|entity| entity.metrics.wakeups_per_second > 0.0)
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        right
            .metrics
            .wakeups_per_second
            .partial_cmp(&left.metrics.wakeups_per_second)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entities.truncate(limit.max(1));
    entities
}

fn top_external_wakeup_entities(
    snapshot: &SystemSnapshot,
    limit: usize,
) -> Vec<&aetower_model::EntitySnapshot> {
    let mut entities = top_wakeup_entities(snapshot, snapshot.entities.len().max(1))
        .into_iter()
        .filter(|entity| !is_aetower_entity(entity))
        .collect::<Vec<_>>();
    entities.truncate(limit.max(1));
    entities
}

fn format_entity_burden_labels<F>(entities: &[&aetower_model::EntitySnapshot], metric: F) -> String
where
    F: Fn(&aetower_model::EntitySnapshot) -> String,
{
    entities
        .iter()
        .map(|entity| format!("{} ({})", entity.display_name, metric(entity)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_aetower_entity(entity: &aetower_model::EntitySnapshot) -> bool {
    let display_name = entity.display_name.to_ascii_lowercase();
    let entity_id = entity.entity_id.to_ascii_lowercase();
    display_name.contains("aetower") || entity_id.contains("aetower")
}

fn ai_provider_label(entity: &aetower_model::EntitySnapshot) -> String {
    if let Some(component) = entity.components.iter().find(|component| {
        matches!(
            component
                .adapter_context
                .as_ref()
                .map(|context| &context.kind),
            Some(aetower_model::AdapterContextKind::Chau7Session)
        )
    }) && let Some(prefix) = component.title.split(" · ").next()
    {
        let trimmed = prefix.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }

    entity
        .badges
        .iter()
        .find(|badge| {
            !matches!(
                badge.as_str(),
                "ai-agent"
                    | "chau7-live"
                    | "approval-needed"
                    | "delegating"
                    | "cto-active"
                    | "at-prompt"
                    | "shell-loading"
                    | "agent-error"
            ) && !badge.starts_with("ai-session:")
        })
        .map(|badge| badge.replace('-', " "))
        .unwrap_or_else(|| "Unknown Provider".to_owned())
}

fn ai_workspace(entity: &aetower_model::EntitySnapshot) -> Option<String> {
    entity
        .components
        .iter()
        .filter_map(|component| {
            component
                .adapter_context
                .as_ref()
                .and_then(|context| {
                    context
                        .repo_root
                        .as_ref()
                        .or(context.workspace_path.as_ref())
                        .cloned()
                })
                .or_else(|| component.cwd.clone())
                .map(|path| {
                    let rank = match component
                        .adapter_context
                        .as_ref()
                        .map(|context| &context.kind)
                    {
                        Some(aetower_model::AdapterContextKind::Chau7Session) => 0,
                        Some(aetower_model::AdapterContextKind::VsCodeWorkspace)
                        | Some(aetower_model::AdapterContextKind::VsCodeRuntime) => 1,
                        _ => 2,
                    };
                    (rank, path)
                })
        })
        .min_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.len().cmp(&right.1.len()))
        })
        .map(|(_, path)| path)
}

fn ai_session_component(
    entity: &aetower_model::EntitySnapshot,
) -> Option<&aetower_model::ComponentSnapshot> {
    entity.components.iter().find(|component| {
        matches!(
            component
                .adapter_context
                .as_ref()
                .map(|context| &context.kind),
            Some(aetower_model::AdapterContextKind::Chau7Session)
        )
    })
}

fn ai_runtime_groups(ai_entities: &[&aetower_model::EntitySnapshot]) -> Vec<AiRuntimeGroupReport> {
    let grouped = ai_entities.iter().fold(
        BTreeMap::<(String, Option<String>), Vec<&aetower_model::EntitySnapshot>>::new(),
        |mut acc, entity| {
            let key = (ai_provider_label(entity), ai_workspace(entity));
            acc.entry(key).or_default().push(*entity);
            acc
        },
    );

    let mut groups = grouped
        .into_iter()
        .map(|((provider, workspace), members)| AiRuntimeGroupReport {
            provider,
            workspace,
            agent_count: members.len(),
            total_cpu_percent: members
                .iter()
                .map(|entity| entity.metrics.cpu_percent)
                .sum(),
            total_memory_bytes: members
                .iter()
                .map(|entity| entity.metrics.memory_resident_bytes)
                .sum(),
            total_energy_nj_per_s: members
                .iter()
                .map(|entity| entity.metrics.energy_nj_per_s)
                .sum(),
            total_cost_usd: members
                .iter()
                .filter_map(|entity| entity.agent_cost.as_ref().map(|cost| cost.cost_usd))
                .sum(),
            approval_count: members
                .iter()
                .filter(|entity| entity.badges.iter().any(|badge| badge == "approval-needed"))
                .count(),
            delegating_count: members
                .iter()
                .filter(|entity| entity.badges.iter().any(|badge| badge == "delegating"))
                .count(),
            chau7_build: common_chau7_build_identity(&members),
        })
        .collect::<Vec<_>>();

    groups.sort_by(|left, right| {
        right
            .total_energy_nj_per_s
            .partial_cmp(&left.total_energy_nj_per_s)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .total_cpu_percent
                    .partial_cmp(&left.total_cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    groups
}

fn common_chau7_build_identity(
    members: &[&aetower_model::EntitySnapshot],
) -> Option<Chau7BuildIdentityReport> {
    let identities = members
        .iter()
        .filter_map(|entity| {
            entity.components.iter().find_map(|component| {
                let context = component.adapter_context.as_ref()?;
                (context.kind == aetower_model::AdapterContextKind::Chau7Session).then(|| {
                    Chau7BuildIdentityReport {
                        app_version: context.app_version.clone(),
                        build_sha: context.build_sha.clone(),
                        build_timestamp: context.build_timestamp.clone(),
                        build_channel: context.build_channel.clone(),
                    }
                })
            })
        })
        .filter(|identity| {
            identity.app_version.is_some()
                || identity.build_sha.is_some()
                || identity.build_timestamp.is_some()
                || identity.build_channel.is_some()
        })
        .collect::<Vec<_>>();
    let first = identities.first()?.clone();
    identities
        .iter()
        .all(|identity| {
            identity.app_version == first.app_version
                && identity.build_sha == first.build_sha
                && identity.build_timestamp == first.build_timestamp
                && identity.build_channel == first.build_channel
        })
        .then_some(first)
}

fn ai_burden_leaders(ai_entities: &[&aetower_model::EntitySnapshot]) -> Vec<AiBurdenLeaderReport> {
    let mut leaders = Vec::new();
    if let Some(entity) = ai_entities
        .iter()
        .max_by(|left, right| {
            left.metrics
                .cpu_percent
                .partial_cmp(&right.metrics.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|entity| entity.metrics.cpu_percent > 0.0)
    {
        leaders.push(AiBurdenLeaderReport {
            kind: "cpu".to_owned(),
            entity_id: entity.entity_id.clone(),
            display_name: entity.display_name.clone(),
            value_label: format!("{:.0}%", entity.metrics.cpu_percent),
        });
    }
    if let Some(entity) = ai_entities
        .iter()
        .max_by_key(|entity| entity.metrics.memory_resident_bytes)
        .filter(|entity| entity.metrics.memory_resident_bytes > 0)
    {
        leaders.push(AiBurdenLeaderReport {
            kind: "memory".to_owned(),
            entity_id: entity.entity_id.clone(),
            display_name: entity.display_name.clone(),
            value_label: format_bytes(entity.metrics.memory_resident_bytes),
        });
    }
    if let Some(entity) = ai_entities
        .iter()
        .max_by(|left, right| {
            left.metrics
                .energy_nj_per_s
                .partial_cmp(&right.metrics.energy_nj_per_s)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|entity| entity.metrics.energy_nj_per_s > 0.0)
    {
        leaders.push(AiBurdenLeaderReport {
            kind: "energy".to_owned(),
            entity_id: entity.entity_id.clone(),
            display_name: entity.display_name.clone(),
            value_label: format_energy(entity.metrics.energy_nj_per_s),
        });
    }
    if let Some(entity) = ai_entities
        .iter()
        .max_by(|left, right| {
            left.metrics
                .estimated_gpu_percent
                .partial_cmp(&right.metrics.estimated_gpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|entity| entity.metrics.estimated_gpu_percent > 0.0)
    {
        leaders.push(AiBurdenLeaderReport {
            kind: "gpu".to_owned(),
            entity_id: entity.entity_id.clone(),
            display_name: entity.display_name.clone(),
            value_label: format!("{:.0}%", entity.metrics.estimated_gpu_percent),
        });
    }
    if let Some(entity) = ai_entities
        .iter()
        .max_by(|left, right| {
            left.metrics
                .wakeups_per_second
                .partial_cmp(&right.metrics.wakeups_per_second)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|entity| entity.metrics.wakeups_per_second > 0.0)
    {
        leaders.push(AiBurdenLeaderReport {
            kind: "wakeups".to_owned(),
            entity_id: entity.entity_id.clone(),
            display_name: entity.display_name.clone(),
            value_label: format!("{:.0}/s", entity.metrics.wakeups_per_second),
        });
    }
    leaders
}

fn ai_approval_queue(ai_entities: &[&aetower_model::EntitySnapshot]) -> Vec<AiApprovalReport> {
    ai_entities
        .iter()
        .filter(|entity| entity.badges.iter().any(|badge| badge == "approval-needed"))
        .map(|entity| AiApprovalReport {
            entity_id: entity.entity_id.clone(),
            display_name: entity.display_name.clone(),
            session_id: ai_session_component(entity)
                .and_then(|component| component.adapter_context.as_ref())
                .and_then(|context| context.session_id.clone()),
            workspace: ai_workspace(entity),
            detail: entity
                .recommendations
                .iter()
                .find(|recommendation| recommendation.title == "Resolve pending agent approval")
                .map(|recommendation| recommendation.detail.clone())
                .or_else(|| entity.recent_change_summary.clone())
                .unwrap_or_else(|| {
                    "This runtime is waiting for approval before it can continue.".to_owned()
                }),
            impact: ai_impact_summary(entity),
        })
        .collect()
}

fn ai_delegations(ai_entities: &[&aetower_model::EntitySnapshot]) -> Vec<AiDelegationReport> {
    ai_entities
        .iter()
        .filter(|entity| entity.badges.iter().any(|badge| badge == "delegating"))
        .filter_map(|entity| {
            let detail = ai_session_component(entity)
                .map(|component| component.detail.clone())
                .or_else(|| entity.recent_change_summary.clone())
                .unwrap_or_else(|| "This runtime is delegating work to child sessions.".to_owned());
            let child_session_count = extract_child_session_count(&detail);
            (child_session_count > 0).then(|| AiDelegationReport {
                entity_id: entity.entity_id.clone(),
                display_name: entity.display_name.clone(),
                session_id: ai_session_component(entity)
                    .and_then(|component| component.adapter_context.as_ref())
                    .and_then(|context| context.session_id.clone()),
                workspace: ai_workspace(entity),
                child_session_count,
                detail,
            })
        })
        .collect()
}

fn ai_recent_changes(
    snapshot: &SystemSnapshot,
    ai_entities: &[&aetower_model::EntitySnapshot],
) -> Vec<RecentChangeItem> {
    let ai_entity_ids = ai_entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    let ai_titles = ai_entities
        .iter()
        .map(|entity| format!("{} session ended", entity.display_name).to_lowercase())
        .collect::<BTreeSet<_>>();

    let mut changes = ai_entities
        .iter()
        .filter_map(|entity| {
            entity
                .recent_change_summary
                .as_ref()
                .map(|summary| RecentChangeItem {
                    timestamp_millis: entity
                        .session_markers
                        .iter()
                        .map(|marker| marker.timestamp_millis)
                        .max()
                        .unwrap_or(snapshot.captured_at_millis),
                    severity: if entity.badges.iter().any(|badge| badge == "agent-error") {
                        SeverityBand::Critical
                    } else if entity
                        .badges
                        .iter()
                        .any(|badge| badge == "approval-needed" || badge == "delegating")
                    {
                        SeverityBand::Warning
                    } else {
                        SeverityBand::Info
                    },
                    source: entity.display_name.clone(),
                    entity_id: Some(entity.entity_id.clone()),
                    title: entity.display_name.clone(),
                    detail: summary.clone(),
                })
        })
        .collect::<Vec<_>>();

    changes.extend(snapshot.timeline.iter().filter_map(|event| {
        let entity_match = event
            .entity_id
            .as_ref()
            .map(|entity_id| ai_entity_ids.contains(entity_id))
            .unwrap_or(false);
        let title_match = matches!(event.category, aetower_model::TimelineCategory::Host)
            && event.title.starts_with("GPU memory")
            || matches!(event.category, aetower_model::TimelineCategory::Lifecycle)
                && ai_titles.contains(&event.title.to_lowercase());
        (entity_match || title_match).then(|| RecentChangeItem {
            timestamp_millis: event.timestamp_millis,
            severity: match event.severity {
                aetower_model::TimelineSeverity::Info => SeverityBand::Info,
                aetower_model::TimelineSeverity::Warning => SeverityBand::Warning,
                aetower_model::TimelineSeverity::Critical => SeverityBand::Critical,
            },
            source: "timeline".to_owned(),
            entity_id: event.entity_id.clone(),
            title: event.title.clone(),
            detail: event.detail.clone(),
        })
    }));

    changes.sort_by(|left, right| right.timestamp_millis.cmp(&left.timestamp_millis));
    changes.truncate(10);
    changes
}

fn ai_historical_groups(
    history: &[SystemSnapshot],
    group_keys: &[(String, Option<String>)],
) -> Vec<AiHistoricalTrendReport> {
    group_keys
        .iter()
        .take(4)
        .filter_map(|(provider, workspace)| {
            let mut cpu_percent = Vec::new();
            let mut memory_bytes = Vec::new();
            for snapshot in history.iter().rev().take(24).rev() {
                let matching = snapshot
                    .entities
                    .iter()
                    .filter(|entity| {
                        matches!(entity.entity_kind, aetower_model::EntityKind::AiAgent)
                            && ai_provider_label(entity) == *provider
                            && ai_workspace(entity) == *workspace
                    })
                    .collect::<Vec<_>>();
                cpu_percent.push(
                    matching
                        .iter()
                        .map(|entity| entity.metrics.cpu_percent as f64)
                        .sum(),
                );
                memory_bytes.push(
                    matching
                        .iter()
                        .map(|entity| entity.metrics.memory_resident_bytes)
                        .sum(),
                );
            }
            if cpu_percent.iter().all(|value| *value == 0.0)
                && memory_bytes.iter().all(|value| *value == 0)
            {
                return None;
            }
            Some(AiHistoricalTrendReport {
                provider: provider.clone(),
                workspace: workspace.clone(),
                cpu_percent,
                memory_bytes,
            })
        })
        .collect()
}

fn ai_recommendations(ai_entities: &[&aetower_model::EntitySnapshot]) -> Vec<RecommendationItem> {
    let mut items = Vec::new();
    for entity in ai_entities.iter().take(8) {
        for recommendation in entity.recommendations.iter().take(2) {
            items.push(RecommendationItem {
                severity: if entity.badges.iter().any(|badge| badge == "agent-error") {
                    SeverityBand::Critical
                } else if entity.badges.iter().any(|badge| badge == "approval-needed") {
                    SeverityBand::Warning
                } else {
                    SeverityBand::Info
                },
                title: recommendation.title.clone(),
                detail: recommendation.detail.clone(),
                entity_id: Some(entity.entity_id.clone()),
                source: entity.display_name.clone(),
                expected_benefit: "Lower AI runtime friction and clearer operator action."
                    .to_owned(),
            });
        }
    }
    items
}

fn ai_impact_summary(entity: &aetower_model::EntitySnapshot) -> String {
    let mut parts = Vec::new();
    if entity.metrics.cpu_percent > 0.0 {
        parts.push(format!("{:.0}% CPU", entity.metrics.cpu_percent));
    }
    if entity.metrics.wakeups_per_second > 0.0 {
        parts.push(format!("{:.0} wake/s", entity.metrics.wakeups_per_second));
    }
    if entity.metrics.memory_resident_bytes > 0 {
        parts.push(format_bytes(entity.metrics.memory_resident_bytes));
    }
    if parts.is_empty() {
        parts.push(format!("friction {:.1}", entity.friction.total_score));
    }
    parts.join(" · ")
}

fn extract_child_session_count(detail: &str) -> usize {
    let tokens = detail.split_whitespace().collect::<Vec<_>>();
    for window in tokens.windows(3) {
        if let [count, child, sessions] = window
            && child.eq_ignore_ascii_case("child")
            && sessions.eq_ignore_ascii_case("sessions")
            && let Ok(parsed) = count.parse::<usize>()
        {
            return parsed;
        }
    }
    0
}

fn format_energy(nj_per_s: f64) -> String {
    let mw = nj_per_s / 1_000_000.0;
    if mw >= 1000.0 {
        format!("{:.1} W", mw / 1000.0)
    } else if mw >= 1.0 {
        format!("{:.0} mW", mw)
    } else {
        "0 mW".to_owned()
    }
}

fn memory_pressure_finding(snapshot: &SystemSnapshot) -> Option<TopFinding> {
    let used_ratio = if snapshot.host.memory_total_bytes == 0 {
        0.0
    } else {
        snapshot.host.memory_used_bytes as f64 / snapshot.host.memory_total_bytes as f64
    };
    if used_ratio < MEMORY_PRESSURE_WARNING_RATIO
        && snapshot.host.compressed_memory_bytes < COMPRESSED_MEMORY_WARNING_BYTES
        && snapshot.host.swap_used_bytes < SWAP_WARNING_BYTES
    {
        return None;
    }
    let severity = if used_ratio >= MEMORY_PRESSURE_CRITICAL_RATIO
        || snapshot.host.compressed_memory_bytes >= COMPRESSED_MEMORY_CRITICAL_BYTES
        || snapshot.host.swap_used_bytes >= SWAP_CRITICAL_BYTES
    {
        SeverityBand::Critical
    } else {
        SeverityBand::Warning
    };
    let external = top_external_memory_entities(snapshot, 3);
    let external_labels = format_entity_burden_labels(&external, |entity| {
        format_bytes(entity.metrics.memory_resident_bytes)
    });
    let recommendation = if external_labels.is_empty() {
        "No non-Aetower memory leader is visible; inspect host pressure and Aetower self telemetry only after checking system services.".to_owned()
    } else {
        format!(
            "Start with external memory leaders: {external_labels}. Then verify Aetower self telemetry if pressure remains unexplained."
        )
    };
    Some(TopFinding {
        id: "host-memory-pressure".to_owned(),
        severity,
        title: "Memory pressure is elevated".to_owned(),
        detail: format!(
            "{} used of {}, {} compressed, {} swap.",
            format_bytes(snapshot.host.memory_used_bytes),
            format_bytes(snapshot.host.memory_total_bytes),
            format_bytes(snapshot.host.compressed_memory_bytes),
            format_bytes(snapshot.host.swap_used_bytes)
        ),
        source: "host".to_owned(),
        entity_ids: if external.is_empty() {
            top_memory_entities(snapshot, 3)
        } else {
            external
        }
        .into_iter()
        .map(|entity| entity.entity_id.clone())
        .collect(),
        recommendation: Some(recommendation),
    })
}

fn wakeup_finding(snapshot: &SystemSnapshot) -> Option<TopFinding> {
    if snapshot.host.wakeups_per_second < WAKEUPS_WARNING {
        return None;
    }
    let wakeup_leaders = top_wakeup_entities(snapshot, 1);
    let leader = wakeup_leaders.first().copied();
    let external = top_external_wakeup_entities(snapshot, 3);
    let external_labels = format_entity_burden_labels(&external, |entity| {
        format!("{:.0}/s", entity.metrics.wakeups_per_second)
    });
    let recommendation = if external_labels.is_empty() {
        "No non-Aetower wakeup leader is visible; inspect system services and Aetower self telemetry next.".to_owned()
    } else {
        format!(
            "Start with external wakeup leaders: {external_labels}. Then inspect Aetower self telemetry if wakeups remain elevated."
        )
    };
    Some(TopFinding {
        id: "host-wakeups".to_owned(),
        severity: if snapshot.host.wakeups_per_second >= WAKEUPS_CRITICAL {
            SeverityBand::Critical
        } else {
            SeverityBand::Warning
        },
        title: "Wakeups are high".to_owned(),
        detail: leader.map_or_else(
            || {
                format!(
                    "Host wakeups are {:.0}/s.",
                    snapshot.host.wakeups_per_second
                )
            },
            |leader| {
                format!(
                    "Host wakeups are {:.0}/s. {} leads at {:.0}/s.",
                    snapshot.host.wakeups_per_second,
                    leader.display_name,
                    leader.metrics.wakeups_per_second
                )
            },
        ),
        source: "host".to_owned(),
        entity_ids: if external.is_empty() {
            leader
                .map(|entity| vec![entity.entity_id.clone()])
                .unwrap_or_default()
        } else {
            external
                .into_iter()
                .map(|entity| entity.entity_id.clone())
                .collect()
        },
        recommendation: Some(recommendation),
    })
}

fn history_store_finding(history: &HistorySummaryResponse) -> Option<TopFinding> {
    let severity = history_store_severity(history);
    (severity != SeverityBand::Info).then(|| TopFinding {
        id: "history-store-health".to_owned(),
        severity,
        title: "Persisted history store needs attention".to_owned(),
        detail: format!(
            "{} DB, {} WAL, {} quarantined rows.",
            format_bytes(history.store_bytes),
            format_bytes(history.wal_bytes),
            history.quarantine_count
        ),
        source: "history".to_owned(),
        entity_ids: Vec::new(),
        recommendation: Some(
            "Keep retention bounded and investigate quarantined rows before the store becomes operator-hostile."
                .to_owned(),
        ),
    })
}

fn diagnostics_finding(diagnostics: &DiagnosticsOverview) -> Option<TopFinding> {
    let severity = diagnostics_severity(diagnostics);
    (severity != SeverityBand::Info).then(|| TopFinding {
        id: "diagnostics-health".to_owned(),
        severity,
        title: if diagnostics_active_error_message(diagnostics).is_some()
            || diagnostics.persistence_error.is_some()
        {
            "Diagnostics signal is active".to_owned()
        } else {
            "Diagnostics signal contains retained noise".to_owned()
        },
        detail: format!(
            "{} warnings, {} errors, {} persisted events.",
            diagnostics.warn_count, diagnostics.error_count, diagnostics.persisted_events
        ),
        source: "diagnostics".to_owned(),
        entity_ids: Vec::new(),
        recommendation: diagnostics_active_error_message(diagnostics)
            .or_else(|| diagnostics.persistence_error.clone())
            .or_else(|| {
                Some(
                    "Retained diagnostics are stale; keep them visible for cleanup, but do not treat them as current failure pressure."
                        .to_owned(),
                )
            }),
    })
}

fn related_entity_nodes(
    snapshot: &SystemSnapshot,
    root: &aetower_model::EntitySnapshot,
) -> (Vec<GroupTreeNode>, Vec<String>, u32) {
    let root_session_ids = entity_session_ids(root);
    let root_repo_roots = entity_repo_roots(root);
    let root_workspaces = entity_workspaces(root);
    let root_grouping = root.grouping_suggestion.clone().unwrap_or_default();
    let root_launcher = normalized_group_value(root.launcher_summary.as_deref());
    let mut relations = BTreeSet::new();
    let mut children = Vec::new();
    let mut grouped_process_count = root.metrics.process_count;

    for candidate in &snapshot.entities {
        if candidate.entity_id == root.entity_id {
            continue;
        }
        let mut reason = None;
        let candidate_sessions = entity_session_ids(candidate);
        if !root_session_ids.is_empty()
            && !candidate_sessions.is_empty()
            && !root_session_ids.is_disjoint(&candidate_sessions)
        {
            reason = Some("shared-session".to_owned());
        }
        if reason.is_none() {
            let candidate_repos = entity_repo_roots(candidate);
            if !root_repo_roots.is_empty()
                && !candidate_repos.is_empty()
                && !root_repo_roots.is_disjoint(&candidate_repos)
            {
                reason = Some("shared-repo".to_owned());
            }
        }
        if reason.is_none() {
            let candidate_workspaces = entity_workspaces(candidate);
            if !root_workspaces.is_empty()
                && !candidate_workspaces.is_empty()
                && !root_workspaces.is_disjoint(&candidate_workspaces)
            {
                reason = Some("shared-workspace".to_owned());
            }
        }
        if reason.is_none()
            && !root_grouping.is_empty()
            && candidate.grouping_suggestion.as_deref() == Some(root_grouping.as_str())
        {
            reason = Some("shared-grouping-hint".to_owned());
        }
        if reason.is_none()
            && !root_launcher.is_empty()
            && normalized_group_value(candidate.launcher_summary.as_deref()) == root_launcher
        {
            reason = Some("shared-launcher".to_owned());
        }
        if let Some(reason) = reason {
            grouped_process_count =
                grouped_process_count.saturating_add(candidate.metrics.process_count);
            relations.insert(reason.clone());
            children.push(GroupTreeNode {
                entity_id: candidate.entity_id.clone(),
                display_name: candidate.display_name.clone(),
                relation: reason,
                friction: candidate.friction.total_score,
                cpu_percent: candidate.metrics.cpu_percent,
                memory_bytes: candidate.metrics.memory_resident_bytes,
                process_count: candidate.metrics.process_count,
                badges: candidate.badges.clone(),
                recent_change_summary: candidate.recent_change_summary.clone(),
                children: Vec::new(),
            });
        }
    }

    children.sort_by(|left, right| {
        right
            .friction
            .partial_cmp(&left.friction)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    (
        children,
        relations.into_iter().collect(),
        grouped_process_count,
    )
}

fn entity_session_ids(entity: &aetower_model::EntitySnapshot) -> BTreeSet<String> {
    entity
        .components
        .iter()
        .filter_map(|component| component.adapter_context.as_ref())
        .filter_map(|context| context.session_id.clone())
        .filter(|session_id| !session_id.is_empty())
        .collect()
}

fn entity_repo_roots(entity: &aetower_model::EntitySnapshot) -> BTreeSet<String> {
    entity
        .components
        .iter()
        .filter_map(|component| component.adapter_context.as_ref())
        .filter_map(|context| context.repo_root.clone())
        .filter(|repo_root| !repo_root.is_empty())
        .collect()
}

fn entity_workspaces(entity: &aetower_model::EntitySnapshot) -> BTreeSet<String> {
    entity
        .components
        .iter()
        .filter_map(|component| component.adapter_context.as_ref())
        .filter_map(|context| context.workspace_path.clone())
        .filter(|workspace| !workspace.is_empty())
        .collect()
}

fn normalized_group_value(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| {
            let normalized = value.to_ascii_lowercase();
            !matches!(
                normalized.as_str(),
                "launchd" | "loginwindow" | "xpcproxy" | "kernel_task" | "runningboardd"
            )
        })
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn capability_operator_label(capability: &aetower_model::CapabilitySnapshot) -> String {
    match capability.state {
        aetower_model::CapabilityState::Granted => "Ready".to_owned(),
        aetower_model::CapabilityState::Denied => "Missing access".to_owned(),
        aetower_model::CapabilityState::Requested => "Pending".to_owned(),
        aetower_model::CapabilityState::Unavailable => "Not available".to_owned(),
        aetower_model::CapabilityState::Unknown => "Not checked".to_owned(),
    }
}

fn capability_action_label(capability: &aetower_model::CapabilitySnapshot) -> String {
    match capability.state {
        aetower_model::CapabilityState::Denied => "Grant access".to_owned(),
        aetower_model::CapabilityState::Requested => "Complete setup".to_owned(),
        aetower_model::CapabilityState::Unknown => "Refresh status".to_owned(),
        aetower_model::CapabilityState::Unavailable => "No action".to_owned(),
        aetower_model::CapabilityState::Granted => "Healthy".to_owned(),
    }
}

fn capability_severity(capability: &aetower_model::CapabilitySnapshot) -> SeverityBand {
    match capability.state {
        aetower_model::CapabilityState::Denied => SeverityBand::Critical,
        aetower_model::CapabilityState::Requested => SeverityBand::Warning,
        aetower_model::CapabilityState::Unknown | aetower_model::CapabilityState::Unavailable => {
            SeverityBand::Info
        }
        aetower_model::CapabilityState::Granted => match capability.health {
            aetower_model::CapabilityHealth::Degraded => SeverityBand::Warning,
            _ => SeverityBand::Info,
        },
    }
}

fn history_store_severity(history: &HistorySummaryResponse) -> SeverityBand {
    if history.store_bytes >= HISTORY_STORE_CRITICAL_BYTES
        || history.wal_bytes >= HISTORY_WAL_CRITICAL_BYTES
        || history.quarantine_count >= HISTORY_QUARANTINE_CRITICAL
    {
        SeverityBand::Critical
    } else if history.store_bytes >= HISTORY_STORE_WARNING_BYTES
        || history.wal_bytes >= HISTORY_WAL_WARNING_BYTES
        || history.quarantine_count >= HISTORY_QUARANTINE_WARNING
    {
        SeverityBand::Warning
    } else {
        SeverityBand::Info
    }
}

fn diagnostics_severity(diagnostics: &DiagnosticsOverview) -> SeverityBand {
    if diagnostics.persistence_error.is_some() {
        return SeverityBand::Critical;
    }

    let has_recent_error = diagnostics_recent_error_age_millis(diagnostics).is_some();
    if has_recent_error
        && (diagnostics.error_count >= DIAGNOSTICS_ERROR_CRITICAL
            || diagnostics.warn_count >= DIAGNOSTICS_WARN_CRITICAL)
    {
        SeverityBand::Critical
    } else if diagnostics.error_count >= DIAGNOSTICS_ERROR_WARNING
        || diagnostics.warn_count >= DIAGNOSTICS_WARN_WARNING
        || diagnostics.error_count >= DIAGNOSTICS_ERROR_CRITICAL
        || diagnostics.warn_count >= DIAGNOSTICS_WARN_CRITICAL
    {
        SeverityBand::Warning
    } else {
        SeverityBand::Info
    }
}

fn diagnostics_active_error_message(diagnostics: &DiagnosticsOverview) -> Option<String> {
    let last_error_message = diagnostics.last_error_message.clone()?;
    diagnostics_recent_error_age_millis(diagnostics).map(|_| last_error_message)
}

fn diagnostics_recent_error_age_millis(diagnostics: &DiagnosticsOverview) -> Option<u64> {
    let last_error_millis = diagnostics.last_error_millis?;
    let now_millis = current_unix_millis().unwrap_or(last_error_millis);
    let age_millis = now_millis.saturating_sub(last_error_millis);
    (age_millis <= DIAGNOSTICS_ACTIVE_ERROR_WINDOW_MILLIS).then_some(age_millis)
}

fn current_unix_millis() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .ok()
}

fn runtime_severity(runtime: &RuntimeLagMetrics) -> SeverityBand {
    if runtime.target_tick_millis > 0.0
        && runtime.engine_tick_millis >= runtime.target_tick_millis * 0.75
    {
        SeverityBand::Critical
    } else if runtime.target_tick_millis > 0.0
        && runtime.engine_tick_millis >= runtime.target_tick_millis * 0.35
    {
        SeverityBand::Warning
    } else {
        SeverityBand::Info
    }
}

fn mcp_helper_severity(runtime: &RuntimeLagMetrics) -> SeverityBand {
    if runtime.stale_mcp_helper_count > 0 || runtime.mcp_helper_count >= MCP_HELPER_CRITICAL_COUNT {
        SeverityBand::Critical
    } else if runtime.mcp_helper_count >= MCP_HELPER_WARNING_COUNT {
        SeverityBand::Warning
    } else {
        SeverityBand::Info
    }
}

fn self_runtime_severity(runtime: &RuntimeLagMetrics) -> SeverityBand {
    let self_resource_severity = if runtime.self_cpu_percent >= 25.0
        || runtime.self_memory_bytes >= 768 * 1024 * 1024
        || runtime.self_wakeups_per_second >= 1_000.0
    {
        SeverityBand::Critical
    } else if runtime.self_cpu_percent >= 10.0
        || runtime.self_memory_bytes >= 384 * 1024 * 1024
        || runtime.self_wakeups_per_second >= 300.0
    {
        SeverityBand::Warning
    } else {
        SeverityBand::Info
    };

    self_resource_severity.max(mcp_request_pressure_severity(runtime))
}

fn mcp_request_pressure_severity(runtime: &RuntimeLagMetrics) -> SeverityBand {
    if runtime.mcp_requests_per_second >= MCP_REQUEST_CRITICAL_RATE {
        if mcp_pressure_looks_like_agent_burst(runtime) {
            SeverityBand::Warning
        } else {
            SeverityBand::Critical
        }
    } else if runtime.mcp_requests_per_second >= MCP_REQUEST_WARNING_RATE {
        if mcp_pressure_looks_like_agent_burst(runtime) {
            SeverityBand::Info
        } else {
            SeverityBand::Warning
        }
    } else {
        SeverityBand::Info
    }
}

fn mcp_pressure_looks_like_agent_burst(runtime: &RuntimeLagMetrics) -> bool {
    runtime.mcp_requests_per_second >= MCP_REQUEST_WARNING_RATE
        && runtime.stale_mcp_helper_count == 0
        && runtime.mcp_active_client_count <= MCP_REQUEST_BURST_CLIENT_LIMIT
        && runtime.mcp_helper_count <= MCP_REQUEST_BURST_HELPER_LIMIT
        && runtime.oldest_mcp_helper_age_millis < MCP_HELPER_STALE_MILLIS
}

fn self_runtime_recommendation_detail(runtime: &RuntimeLagMetrics) -> String {
    let mut detail = format!(
        "Aetower self telemetry is CPU {:.1}%, memory {}, wakeups {:.0}/s, MCP {:.1} req/s.",
        runtime.self_cpu_percent,
        format_bytes(runtime.self_memory_bytes),
        runtime.self_wakeups_per_second,
        runtime.mcp_requests_per_second
    );
    if mcp_pressure_looks_like_agent_burst(runtime) {
        detail.push_str(
            " MCP pressure currently looks like a short active-agent audit burst, not sustained helper pressure; watch it if clients/helpers stay elevated.",
        );
    } else if mcp_request_pressure_severity(runtime) != SeverityBand::Info {
        detail.push_str(
            " MCP pressure looks sustained because request rate is elevated without a short-burst helper/client profile.",
        );
    }
    detail
}

fn host_load_severity(snapshot: &SystemSnapshot) -> SeverityBand {
    memory_pressure_finding(snapshot)
        .map(|finding| finding.severity)
        .into_iter()
        .chain(wakeup_finding(snapshot).map(|finding| finding.severity))
        .max_by_key(|severity| severity.score())
        .unwrap_or(SeverityBand::Info)
}

fn privacy_tier_notes(privacy_tier: ExportPrivacyTier) -> Vec<String> {
    match privacy_tier {
        ExportPrivacyTier::Full => vec![
            "Full exports include paths, titles, URLs, commands, and other sensitive fields."
                .to_owned(),
        ],
        ExportPrivacyTier::OperatorMode => vec![
            "Operator mode keeps structure but sanitizes paths, hosts, commands, and sensitive diagnostics."
                .to_owned(),
        ],
        ExportPrivacyTier::Redacted => vec![
            "Redacted exports hide sensitive paths, URLs, titles, commands, and session identifiers."
                .to_owned(),
        ],
    }
}

fn export_controlled_json(value: Value, privacy_tier: ExportPrivacyTier) -> Value {
    match privacy_tier {
        ExportPrivacyTier::Full => value,
        _ => export_controlled_value(value, privacy_tier, None),
    }
}

fn export_controlled_value(
    value: Value,
    privacy_tier: ExportPrivacyTier,
    key: Option<&str>,
) -> Value {
    let normalized_key = key.unwrap_or_default().to_ascii_lowercase();
    if privacy_tier == ExportPrivacyTier::Redacted && is_sensitive_export_key(&normalized_key) {
        return Value::String("<redacted>".to_owned());
    }
    match value {
        Value::Object(map) => {
            let sensitive_event = map
                .get("sensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut redacted = serde_json::Map::with_capacity(map.len());
            for (child_key, child_value) in map {
                if sensitive_event && child_key == "message" {
                    redacted.insert(
                        child_key,
                        Value::String(match privacy_tier {
                            ExportPrivacyTier::OperatorMode => "<sensitive event>".to_owned(),
                            _ => "<redacted sensitive event>".to_owned(),
                        }),
                    );
                    continue;
                }
                if sensitive_event && child_key == "fields" {
                    redacted.insert(
                        child_key,
                        match privacy_tier {
                            ExportPrivacyTier::OperatorMode => {
                                redact_sensitive_fields_operator_mode(child_value)
                            }
                            _ => Value::Array(Vec::new()),
                        },
                    );
                    continue;
                }
                redacted.insert(
                    child_key.clone(),
                    export_controlled_value(child_value, privacy_tier, Some(&child_key)),
                );
            }
            Value::Object(redacted)
        }
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|child| export_controlled_value(child, privacy_tier, key))
                .collect(),
        ),
        Value::String(string) => match privacy_tier {
            ExportPrivacyTier::Full => Value::String(string),
            ExportPrivacyTier::Redacted => {
                if should_redact_string_value(&string, &normalized_key) {
                    Value::String("<redacted>".to_owned())
                } else {
                    Value::String(string)
                }
            }
            ExportPrivacyTier::OperatorMode => {
                if is_sensitive_export_key(&normalized_key)
                    || should_redact_string_value(&string, &normalized_key)
                {
                    Value::String(sanitized_operator_string_value(&string, &normalized_key))
                } else {
                    Value::String(string)
                }
            }
        },
        other => other,
    }
}

fn redact_sensitive_fields_operator_mode(value: Value) -> Value {
    let Value::Array(fields) = value else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        fields
            .into_iter()
            .filter_map(|field| match field {
                Value::Object(mut map) => {
                    let key = map
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("field")
                        .to_owned();
                    let raw_value = map
                        .get("value")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    map.insert(
                        "value".to_owned(),
                        Value::String(sanitized_operator_string_value(
                            &raw_value,
                            &key.to_ascii_lowercase(),
                        )),
                    );
                    Some(Value::Object(map))
                }
                _ => None,
            })
            .collect(),
    )
}

fn is_sensitive_export_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    matches!(
        key,
        "commandline"
            | "cwd"
            | "executablepath"
            | "frontmostwindowtitle"
            | "activewindowtitle"
            | "windowtitle"
            | "workspacepath"
            | "reporoot"
            | "sessionid"
            | "telemetryendpoint"
            | "path"
            | "url"
            | "ports"
            | "entities_preview"
    ) || key.contains("command")
        || key.contains("windowtitle")
        || key.contains("workspace")
        || key.contains("repo")
        || key.contains("endpoint")
        || key.contains("executable")
        || key.contains("path")
}

fn should_redact_string_value(value: &str, key: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    is_sensitive_export_key(key)
        || value.starts_with('/')
        || value.starts_with("file://")
        || value.starts_with("http://")
        || value.starts_with("https://")
        || value.contains("/Users/")
        || value.contains("/var/")
        || value.contains(".sock")
}

fn sanitized_operator_string_value(value: &str, key: &str) -> String {
    if value.is_empty() {
        return value.to_owned();
    }
    if key.contains("windowtitle") || key == "message" {
        return "<redacted>".to_owned();
    }
    if key.contains("command") {
        return value
            .split(' ')
            .next()
            .map(|command| {
                Path::new(command)
                    .file_name()
                    .and_then(|part| part.to_str())
                    .unwrap_or("command")
                    .to_owned()
            })
            .unwrap_or_else(|| "command".to_owned());
    }
    if key.contains("url") || key.contains("endpoint") || value.starts_with("http") {
        return sanitized_host_string(value);
    }
    if key.contains("path")
        || key.contains("cwd")
        || key.contains("workspace")
        || key.contains("repo")
        || value.starts_with('/')
        || value.starts_with("file://")
        || value.contains("/Users/")
    {
        return sanitized_path_string(value);
    }
    if key == "ports" || key == "sessionid" {
        return "<redacted>".to_owned();
    }
    value.to_owned()
}

fn sanitized_host_string(value: &str) -> String {
    let host = value
        .split("://")
        .nth(1)
        .unwrap_or(value)
        .split('/')
        .next()
        .unwrap_or_default();
    if host.is_empty() {
        "<redacted host>".to_owned()
    } else if value.starts_with("http://") {
        format!("http://{host}")
    } else {
        format!("https://{host}")
    }
}

fn sanitized_path_string(value: &str) -> String {
    let cleaned = value.strip_prefix("file://").unwrap_or(value);
    Path::new(cleaned)
        .file_name()
        .and_then(|part| part.to_str())
        .map(|tail| format!("…/{tail}"))
        .unwrap_or_else(|| "<redacted path>".to_owned())
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn format_duration_millis(millis: u64) -> String {
    if millis < 1_000 {
        format!("{millis} ms")
    } else if millis < 60_000 {
        format!("{:.1} s", millis as f64 / 1_000.0)
    } else if millis < 60 * 60_000 {
        format!("{:.1} min", millis as f64 / 60_000.0)
    } else {
        format!("{:.1} h", millis as f64 / (60.0 * 60_000.0))
    }
}

fn tool_error(message: impl Into<String>) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": message.into(),
            }
        ],
        "isError": true,
    })
}

fn parse_args<T: serde::de::DeserializeOwned>(arguments: Value) -> Result<T, Value> {
    serde_json::from_value(arguments)
        .map_err(|error| jsonrpc_error(-32602, format!("invalid tool arguments: {error}")))
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "aetower_current_snapshot",
            "description": "Return the latest live Aetower snapshot. Optionally skip output unless the sequence advanced.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "last_sequence": { "type": "integer", "minimum": 0 },
                    "entity_limit": { "type": "integer", "minimum": 1 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_host_summary",
            "description": "Return a concise host summary plus the top friction entities.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "top_entities": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_entity_details",
            "description": "Return the full entity snapshot for one entity_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string" }
                },
                "required": ["entity_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_runtime_lag",
            "description": "Return the latest self-observability and runtime lag metrics for Aetower itself.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_diff_snapshots",
            "description": "Compare two persisted time points and return host plus per-entity deltas for friction, CPU, memory, wakeups, and process count, including boot-boundary metadata when the snapshots span a reboot.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "before_millis": { "type": "integer", "minimum": 0 },
                    "after_millis": { "type": "integer", "minimum": 0 },
                    "entity_ids": { "type": "array", "items": { "type": "string" }, "maxItems": 100 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "required": ["before_millis", "after_millis"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_reboot_report",
            "description": "Summarize detected boot-session boundaries, pre-reboot pressure, and correlated sleep/wake/panic markers for a time range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "start_millis": { "type": "integer", "minimum": 0 },
                    "end_millis": { "type": "integer", "minimum": 0 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_explain_anomalies",
            "description": "Explain current anomalous entities by highlighting the dominant changed metrics and recent supporting events.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_ids": { "type": "array", "items": { "type": "string" }, "maxItems": 100 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "window_minutes": { "type": "integer", "minimum": 1, "maximum": 1440 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_entity_process_tree",
            "description": "Return a per-process tree for one entity with subtree burden, grouping scope, and expansion reasons.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string" }
                },
                "required": ["entity_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_top_findings",
            "description": "Return the highest-signal current findings across host load, diagnostics, history health, and top friction groups.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_host_alerts",
            "description": "Return current host alerts such as memory pressure and wakeup storms with impacted entity IDs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "top_entities": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_investigation_bundle",
            "description": "Return a focused crash/freeze investigation bundle with current pressure, recent changes, diagnostics, history diff, and optional process trees.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "window_minutes": { "type": "integer", "minimum": 1, "maximum": 1440 },
                    "start_millis": { "type": "integer", "minimum": 0 },
                    "end_millis": { "type": "integer", "minimum": 0 },
                    "entity_ids": { "type": "array", "items": { "type": "string" }, "maxItems": 100 },
                    "findings_limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "entity_limit": { "type": "integer", "minimum": 1, "maximum": 25 },
                    "diagnostics_limit": { "type": "integer", "minimum": 1, "maximum": 5000 },
                    "include_process_trees": { "type": "boolean" }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_entity_group_tree",
            "description": "Return a grouped entity family view for one entity_id using runtime/session/repo relationships.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string" }
                },
                "required": ["entity_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_ai_runtime_report",
            "description": "Return grouped AI runtime insights including burden leaders, approval queue, delegated sessions, recent changes, and recent persisted history trends.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "history_window_hours": { "type": "integer", "minimum": 1, "maximum": 168 },
                    "history_limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_recent_changes",
            "description": "Return a concise feed of recent timeline changes and entity change summaries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "window_minutes": { "type": "integer", "minimum": 1, "maximum": 1440 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_capability_status",
            "description": "Return operator-grade capability state, health, and next-action labels for permissions and adapters.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_history_summary",
            "description": "Return persisted history coverage and store size information for a time range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "start_millis": { "type": "integer", "minimum": 0 },
                    "end_millis": { "type": "integer", "minimum": 0 }
                },
                "required": ["start_millis", "end_millis"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_history_page",
            "description": "Return a bounded page of persisted snapshots for a time range, newest first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "start_millis": { "type": "integer", "minimum": 0 },
                    "end_millis": { "type": "integer", "minimum": 0 },
                    "before_millis_exclusive": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "required": ["start_millis", "end_millis"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_history_store_health",
            "description": "Return persisted history store health, thresholds, and recent history-related diagnostics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "window_hours": { "type": "integer", "minimum": 1, "maximum": 720 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_history_data_quality",
            "description": "Analyze persisted snapshot ordering and coverage for gaps, duplicate timestamps, sequence regressions, and boot boundaries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "window_hours": { "type": "integer", "minimum": 1, "maximum": 720 },
                    "expected_interval_millis": { "type": "integer", "minimum": 1, "maximum": 3600000 },
                    "max_snapshots": { "type": "integer", "minimum": 1, "maximum": 20000 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_memory_breakdown",
            "description": "Ask the running Aetower app to collect a vmmap-style memory region breakdown for one entity.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string" },
                    "top_regions": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "required": ["entity_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_profile_entity",
            "description": "Ask the running Aetower app to run a short sampled profile for one entity and summarize hot threads, queues, and stacks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string" },
                    "duration_seconds": { "type": "integer", "minimum": 1, "maximum": 15 },
                    "top_stacks": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "required": ["entity_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_wakeup_attribution",
            "description": "Ask the running Aetower app to sample one entity and return heuristic wakeup attribution by thread, queue, and dominant sampled cause.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string" },
                    "duration_seconds": { "type": "integer", "minimum": 1, "maximum": 15 },
                    "top_stacks": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "required": ["entity_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_process_inspect",
            "description": "Inspect one running process by PID with current attribution, ps state, children, and safety notes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "minimum": 2 }
                },
                "required": ["pid"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_process_open_resources",
            "description": "List open files and sockets for one process using lsof with a bounded result limit.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "minimum": 2 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "required": ["pid"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_process_sample",
            "description": "Run a short bounded sample for one process and summarize the hottest sampled threads.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "minimum": 2 },
                    "duration_seconds": { "type": "integer", "minimum": 1, "maximum": 15 },
                    "top_stacks": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "required": ["pid"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_process_action",
            "description": "Preview or execute a guarded process action. Defaults to dry_run=true; set dry_run=false only after explicit operator approval.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "minimum": 2 },
                    "action": {
                        "type": "string",
                        "enum": [
                            "terminate",
                            "force-kill",
                            "suspend",
                            "resume",
                            "terminate-tree",
                            "force-kill-tree",
                            "lower-priority",
                            "normal-priority"
                        ]
                    },
                    "dry_run": { "type": "boolean" },
                    "reason": { "type": "string" }
                },
                "required": ["pid", "action"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_process_action_history",
            "description": "Return recent operator process actions recorded by Aetower diagnostics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "window_minutes": { "type": "integer", "minimum": 1, "maximum": 10080 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_diagnostics_overview",
            "description": "Return diagnostics ring and persisted diagnostics health.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_diagnostics_summary",
            "description": "Return diagnostics grouped by subsystem, event type, and level with counts, latest samples, and noise-reduction recommendations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "query_limit": { "type": "integer", "minimum": 1, "maximum": 5000 },
                    "minimum_level": { "type": "string", "enum": ["trace", "debug", "info", "warn", "error"] },
                    "subsystem": {
                        "type": "string",
                        "enum": [
                            "engine",
                            "collector",
                            "identity",
                            "attribution",
                            "friction",
                            "history",
                            "persistence",
                            "telemetry",
                            "gpu",
                            "ffi",
                            "ui",
                            "adapter-chromium",
                            "adapter-docker",
                            "adapter-helper",
                            "adapter-chau7",
                            "adapter-vscode"
                        ]
                    },
                    "search": { "type": "string" },
                    "since_millis": { "type": "integer", "minimum": 0 },
                    "include_persisted": { "type": "boolean" }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_query_diagnostics",
            "description": "Query recent or persisted diagnostics by level, subsystem, text, and time window.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 5000 },
                    "minimum_level": { "type": "string", "enum": ["trace", "debug", "info", "warn", "error"] },
                    "subsystem": {
                        "type": "string",
                        "enum": [
                            "engine",
                            "collector",
                            "identity",
                            "attribution",
                            "friction",
                            "history",
                            "persistence",
                            "telemetry",
                            "gpu",
                            "ffi",
                            "ui",
                            "adapter-chromium",
                            "adapter-docker",
                            "adapter-helper",
                            "adapter-chau7",
                            "adapter-vscode"
                        ]
                    },
                    "search": { "type": "string" },
                    "since_millis": { "type": "integer", "minimum": 0 },
                    "include_persisted": { "type": "boolean" }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_support_bundle_manifest",
            "description": "Return a machine-readable preview of what an Aetower support bundle would contain for a given privacy tier.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "privacy_tier": { "type": "string", "enum": ["redacted", "operator-mode", "full"] },
                    "diagnostics_limit": { "type": "integer", "minimum": 1, "maximum": 5000 },
                    "history_window_hours": { "type": "integer", "minimum": 1, "maximum": 720 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_recommendations",
            "description": "Return structured remediation recommendations derived from host load, history health, diagnostics, and entity recommendations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_session_health",
            "description": "Return a merged health view across runtime lag, diagnostics, history store, capabilities, host load, and MCP state.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "history_window_hours": { "type": "integer", "minimum": 1, "maximum": 720 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_export_query",
            "description": "Return a scoped, privacy-tiered export payload for current snapshot, history, diagnostics, and session health.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "privacy_tier": { "type": "string", "enum": ["redacted", "operator-mode", "full"] },
                    "entity_ids": { "type": "array", "items": { "type": "string" }, "maxItems": 100 },
                    "start_millis": { "type": "integer", "minimum": 0 },
                    "end_millis": { "type": "integer", "minimum": 0 },
                    "include_history": { "type": "boolean" },
                    "include_snapshot": { "type": "boolean" },
                    "include_diagnostics": { "type": "boolean" },
                    "include_session_health": { "type": "boolean" },
                    "include_ai_runtime_report": { "type": "boolean" },
                    "diagnostics_limit": { "type": "integer", "minimum": 1, "maximum": 5000 },
                    "history_limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false
            }
        }),
    ]
}

fn jsonrpc_error(code: i64, message: impl Into<String>) -> Value {
    json!({
        "code": code,
        "message": message.into(),
    })
}

fn read_message(
    reader: &mut impl Read,
    framing: &mut Option<MessageFraming>,
) -> Result<ReadMessageOutcome, String> {
    if matches!(framing, Some(MessageFraming::JsonLine)) {
        return read_json_line_message(reader);
    }

    let mut headers = Vec::new();
    let mut byte = [0u8; 1];
    let mut recent = Vec::new();

    loop {
        match reader.read(&mut byte) {
            Ok(0) => {
                if headers.is_empty() {
                    return Ok(ReadMessageOutcome::EndOfStream);
                }
                let trimmed = trim_ascii_whitespace(&headers);
                if is_json_line_candidate(trimmed) {
                    *framing = Some(MessageFraming::JsonLine);
                    return parse_json_line(trimmed);
                }
                return Err("unexpected EOF while reading headers".into());
            }
            Ok(_) => {
                headers.push(byte[0]);
                let trimmed = trim_ascii_whitespace(&headers);
                if is_json_line_candidate(trimmed) && byte[0] == b'\n' {
                    *framing = Some(MessageFraming::JsonLine);
                    return parse_json_line(trimmed);
                }
                recent.push(byte[0]);
                if recent.len() > 4 {
                    recent.remove(0);
                }
                if recent.ends_with(b"\r\n\r\n") || recent.ends_with(b"\n\n") {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(ReadMessageOutcome::Timeout);
            }
            Err(error) => return Err(format!("read header: {error}")),
        }
    }

    let header_text =
        String::from_utf8(headers).map_err(|error| format!("header utf8: {error}"))?;
    let content_length = header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .ok_or_else(|| "missing Content-Length".to_string())?;

    let mut body = vec![0u8; content_length];
    let mut offset = 0;
    while offset < content_length {
        match reader.read(&mut body[offset..]) {
            Ok(0) => return Err("unexpected EOF while reading body".into()),
            Ok(read) => offset += read,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(format!("read body: {error}")),
        }
    }
    let value = serde_json::from_slice(&body).map_err(|error| format!("parse json: {error}"))?;
    *framing = Some(MessageFraming::ContentLength);
    Ok(ReadMessageOutcome::Message(value))
}

fn read_json_line_message(reader: &mut impl Read) -> Result<ReadMessageOutcome, String> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        match reader.read(&mut byte) {
            Ok(0) => {
                if buffer.is_empty() {
                    return Ok(ReadMessageOutcome::EndOfStream);
                }
                return parse_json_line(trim_ascii_whitespace(&buffer));
            }
            Ok(_) => {
                buffer.push(byte[0]);
                if byte[0] == b'\n' {
                    return parse_json_line(trim_ascii_whitespace(&buffer));
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(ReadMessageOutcome::Timeout);
            }
            Err(error) => return Err(format!("read json line: {error}")),
        }
    }
}

fn is_json_line_candidate(bytes: &[u8]) -> bool {
    matches!(bytes.first(), Some(b'{') | Some(b'['))
}

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn parse_json_line(bytes: &[u8]) -> Result<ReadMessageOutcome, String> {
    if bytes.is_empty() {
        return Ok(ReadMessageOutcome::Timeout);
    }
    let value =
        serde_json::from_slice(bytes).map_err(|error| format!("parse json line: {error}"))?;
    Ok(ReadMessageOutcome::Message(value))
}

fn write_message(
    writer: &mut impl Write,
    message: &Value,
    framing: MessageFraming,
) -> Result<(), String> {
    let body =
        serde_json::to_vec(message).map_err(|error| format!("serialize response: {error}"))?;
    match framing {
        MessageFraming::ContentLength => {
            write!(writer, "Content-Length: {}\r\n\r\n", body.len())
                .map_err(|error| format!("write header: {error}"))?;
            writer
                .write_all(&body)
                .map_err(|error| format!("write body: {error}"))?;
        }
        MessageFraming::JsonLine => {
            writer
                .write_all(&body)
                .map_err(|error| format!("write json line: {error}"))?;
            writer
                .write_all(b"\n")
                .map_err(|error| format!("write json line terminator: {error}"))?;
        }
    }
    writer.flush().map_err(|error| format!("flush: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Write},
        sync::Arc,
    };

    use super::*;
    use aetower_diagnostics::{DiagnosticsLevel, DiagnosticsSubsystem};

    #[derive(Default)]
    struct FakeSource;

    impl AetowerMcpDataSource for FakeSource {
        fn latest_snapshot(&self) -> Result<SystemSnapshot, String> {
            Ok(SystemSnapshot {
                sequence: 1,
                captured_at_millis: 123,
                host: aetower_model::HostSnapshot {
                    memory_used_bytes: 15 * 1024 * 1024 * 1024,
                    memory_total_bytes: 16 * 1024 * 1024 * 1024,
                    compressed_memory_bytes: 7 * 1024 * 1024 * 1024,
                    swap_used_bytes: 18 * 1024 * 1024 * 1024,
                    wakeups_per_second: 31_000.0,
                    ..aetower_model::HostSnapshot::default()
                },
                capabilities: vec![aetower_model::CapabilitySnapshot {
                    kind: aetower_model::CapabilityKind::Accessibility,
                    state: aetower_model::CapabilityState::Denied,
                    health: aetower_model::CapabilityHealth::Degraded,
                    detail: "Accessibility has not been granted.".to_owned(),
                    last_updated_millis: 123,
                }],
                entities: vec![aetower_model::EntitySnapshot {
                    entity_id: "chau7".to_owned(),
                    display_name: "Chau7".to_owned(),
                    entity_kind: aetower_model::EntityKind::AiAgent,
                    metrics: aetower_model::AggregateMetrics {
                        cpu_percent: 41.5,
                        memory_resident_bytes: 750 * 1024 * 1024,
                        wakeups_per_second: 12_000.0,
                        process_count: 3,
                        ..aetower_model::AggregateMetrics::default()
                    },
                    friction: aetower_model::FrictionBreakdown {
                        total_score: 35.5,
                        ..aetower_model::FrictionBreakdown::default()
                    },
                    components: vec![aetower_model::ComponentSnapshot {
                        kind: aetower_model::ComponentKind::AdapterContext,
                        title: "Chau7 session".to_owned(),
                        detail: "repo context".to_owned(),
                        adapter_context: Some(aetower_model::AdapterContextSnapshot {
                            kind: aetower_model::AdapterContextKind::Chau7Session,
                            session_id: Some("rs_1".to_owned()),
                            repo_root: Some("/repo".to_owned()),
                            workspace_path: Some("/repo".to_owned()),
                            app_version: Some("1.4.2".to_owned()),
                            build_sha: Some("abc123def456".to_owned()),
                            build_timestamp: Some("2026-04-14T11:41:49Z".to_owned()),
                            build_channel: Some("dev".to_owned()),
                            ..aetower_model::AdapterContextSnapshot::default()
                        }),
                        ..aetower_model::ComponentSnapshot::default()
                    }],
                    recent_change_summary: Some("Recent Chau7 event: waiting input.".to_owned()),
                    anomaly_detected: true,
                    recommendations: vec![aetower_model::Recommendation {
                        title: "Resolve pending agent approval".to_owned(),
                        detail: "A Chau7 session is currently blocked waiting for approval."
                            .to_owned(),
                    }],
                    ..aetower_model::EntitySnapshot::default()
                }],
                timeline: vec![aetower_model::TimelineEvent {
                    id: "ev-1".to_owned(),
                    timestamp_millis: 120,
                    category: aetower_model::TimelineCategory::Anomaly,
                    severity: aetower_model::TimelineSeverity::Warning,
                    entity_id: Some("chau7".to_owned()),
                    title: "Agent blocked on approval".to_owned(),
                    detail: "A Chau7 session is waiting for approval.".to_owned(),
                }],
                ..SystemSnapshot::default()
            })
        }

        fn latest_snapshot_if_newer(
            &self,
            last_sequence: u64,
        ) -> Result<Option<SystemSnapshot>, String> {
            Ok((last_sequence == 0).then_some(self.latest_snapshot()?))
        }

        fn latest_sequence(&self) -> Result<u64, String> {
            Ok(1)
        }

        fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String> {
            Ok(RuntimeLagMetrics {
                updated_at_millis: 123,
                ..RuntimeLagMetrics::default()
            })
        }

        fn history_range_summary(
            &self,
            _start_millis: u64,
            _end_millis: u64,
        ) -> Result<HistorySummaryResponse, String> {
            Ok(HistorySummaryResponse {
                store_bytes: 1,
                wal_bytes: 2,
                snapshot_count: 3,
                quarantine_count: 0,
                range_count: 2,
                oldest_millis: Some(1),
                newest_millis: Some(2),
                pending_writes: 0,
            })
        }

        fn load_history_page(
            &self,
            _start_millis: u64,
            _end_millis: u64,
            _before_millis_exclusive: Option<u64>,
            _limit: u32,
        ) -> Result<Vec<SystemSnapshot>, String> {
            Ok(vec![self.latest_snapshot()?])
        }

        fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String> {
            Ok(DiagnosticsOverview {
                warn_count: 300,
                error_count: 12,
                persisted_events: 32,
                last_error_message: Some("Failed to start local MCP server".to_owned()),
                ..DiagnosticsOverview::default()
            })
        }

        fn query_diagnostics(
            &self,
            _query: DiagnosticsQuery,
        ) -> Result<Vec<DiagnosticsEvent>, String> {
            Ok(vec![
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Engine,
                    "test",
                    "ok",
                )
                .build(),
            ])
        }
    }

    struct BrokenSource;

    impl AetowerMcpDataSource for BrokenSource {
        fn latest_snapshot(&self) -> Result<SystemSnapshot, String> {
            Err("engine lock poisoned".to_owned())
        }

        fn latest_snapshot_if_newer(
            &self,
            _last_sequence: u64,
        ) -> Result<Option<SystemSnapshot>, String> {
            Err("engine lock poisoned".to_owned())
        }

        fn latest_sequence(&self) -> Result<u64, String> {
            Err("engine lock poisoned".to_owned())
        }

        fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String> {
            Err("engine lock poisoned".to_owned())
        }

        fn history_range_summary(
            &self,
            _start_millis: u64,
            _end_millis: u64,
        ) -> Result<HistorySummaryResponse, String> {
            Err("engine lock poisoned".to_owned())
        }

        fn load_history_page(
            &self,
            _start_millis: u64,
            _end_millis: u64,
            _before_millis_exclusive: Option<u64>,
            _limit: u32,
        ) -> Result<Vec<SystemSnapshot>, String> {
            Err("engine lock poisoned".to_owned())
        }

        fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String> {
            Err("engine lock poisoned".to_owned())
        }

        fn query_diagnostics(
            &self,
            _query: DiagnosticsQuery,
        ) -> Result<Vec<DiagnosticsEvent>, String> {
            Err("engine lock poisoned".to_owned())
        }
    }

    struct HistoryBrokenSource;

    impl AetowerMcpDataSource for HistoryBrokenSource {
        fn latest_snapshot(&self) -> Result<SystemSnapshot, String> {
            FakeSource.latest_snapshot()
        }

        fn latest_snapshot_if_newer(
            &self,
            last_sequence: u64,
        ) -> Result<Option<SystemSnapshot>, String> {
            FakeSource.latest_snapshot_if_newer(last_sequence)
        }

        fn latest_sequence(&self) -> Result<u64, String> {
            FakeSource.latest_sequence()
        }

        fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String> {
            FakeSource.latest_runtime_lag_metrics()
        }

        fn history_range_summary(
            &self,
            start_millis: u64,
            end_millis: u64,
        ) -> Result<HistorySummaryResponse, String> {
            FakeSource.history_range_summary(start_millis, end_millis)
        }

        fn load_history_page(
            &self,
            _start_millis: u64,
            _end_millis: u64,
            _before_millis_exclusive: Option<u64>,
            _limit: u32,
        ) -> Result<Vec<SystemSnapshot>, String> {
            Err("bincode envelope deserialize: tag for enum is not valid, found 23".to_owned())
        }

        fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String> {
            FakeSource.diagnostics_overview()
        }

        fn query_diagnostics(
            &self,
            query: DiagnosticsQuery,
        ) -> Result<Vec<DiagnosticsEvent>, String> {
            FakeSource.query_diagnostics(query)
        }
    }

    struct ObservingSource {
        observations: Arc<Mutex<Vec<(u64, u64, u64)>>>,
    }

    impl AetowerMcpDataSource for ObservingSource {
        fn latest_snapshot(&self) -> Result<SystemSnapshot, String> {
            FakeSource.latest_snapshot()
        }

        fn latest_snapshot_if_newer(
            &self,
            last_sequence: u64,
        ) -> Result<Option<SystemSnapshot>, String> {
            FakeSource.latest_snapshot_if_newer(last_sequence)
        }

        fn latest_sequence(&self) -> Result<u64, String> {
            FakeSource.latest_sequence()
        }

        fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String> {
            FakeSource.latest_runtime_lag_metrics()
        }

        fn history_range_summary(
            &self,
            start_millis: u64,
            end_millis: u64,
        ) -> Result<HistorySummaryResponse, String> {
            FakeSource.history_range_summary(start_millis, end_millis)
        }

        fn load_history_page(
            &self,
            start_millis: u64,
            end_millis: u64,
            before_millis_exclusive: Option<u64>,
            limit: u32,
        ) -> Result<Vec<SystemSnapshot>, String> {
            FakeSource.load_history_page(start_millis, end_millis, before_millis_exclusive, limit)
        }

        fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String> {
            FakeSource.diagnostics_overview()
        }

        fn query_diagnostics(
            &self,
            query: DiagnosticsQuery,
        ) -> Result<Vec<DiagnosticsEvent>, String> {
            FakeSource.query_diagnostics(query)
        }

        fn record_mcp_runtime_observation(
            &self,
            total_connections: u64,
            active_client_count: u64,
            total_requests: u64,
        ) {
            if let Ok(mut observations) = self.observations.lock() {
                observations.push((total_connections, active_client_count, total_requests));
            }
        }
    }

    struct PagingHistorySource {
        snapshots: Vec<SystemSnapshot>,
    }

    fn test_history_snapshot(captured_at_millis: u64, sequence: u64) -> SystemSnapshot {
        SystemSnapshot {
            captured_at_millis,
            sequence,
            ..SystemSnapshot::default()
        }
    }

    impl AetowerMcpDataSource for PagingHistorySource {
        fn latest_snapshot(&self) -> Result<SystemSnapshot, String> {
            self.snapshots
                .last()
                .cloned()
                .ok_or_else(|| "missing snapshot".to_owned())
        }

        fn latest_snapshot_if_newer(
            &self,
            last_sequence: u64,
        ) -> Result<Option<SystemSnapshot>, String> {
            self.latest_snapshot()?
                .sequence
                .gt(&last_sequence)
                .then(|| self.latest_snapshot())
                .transpose()
        }

        fn latest_sequence(&self) -> Result<u64, String> {
            Ok(self
                .snapshots
                .last()
                .map(|snapshot| snapshot.sequence)
                .unwrap_or(0))
        }

        fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String> {
            Ok(RuntimeLagMetrics::default())
        }

        fn history_range_summary(
            &self,
            start_millis: u64,
            end_millis: u64,
        ) -> Result<HistorySummaryResponse, String> {
            let in_range = self
                .snapshots
                .iter()
                .filter(|snapshot| {
                    snapshot.captured_at_millis >= start_millis
                        && snapshot.captured_at_millis <= end_millis
                })
                .collect::<Vec<_>>();
            Ok(HistorySummaryResponse {
                store_bytes: 1,
                wal_bytes: 0,
                snapshot_count: self.snapshots.len() as u64,
                quarantine_count: 0,
                range_count: in_range.len() as u64,
                oldest_millis: in_range
                    .iter()
                    .map(|snapshot| snapshot.captured_at_millis)
                    .min(),
                newest_millis: in_range
                    .iter()
                    .map(|snapshot| snapshot.captured_at_millis)
                    .max(),
                pending_writes: 0,
            })
        }

        fn load_history_page(
            &self,
            start_millis: u64,
            end_millis: u64,
            before_millis_exclusive: Option<u64>,
            limit: u32,
        ) -> Result<Vec<SystemSnapshot>, String> {
            let mut page = self
                .snapshots
                .iter()
                .filter(|snapshot| {
                    snapshot.captured_at_millis >= start_millis
                        && snapshot.captured_at_millis <= end_millis
                        && before_millis_exclusive
                            .map(|before| snapshot.captured_at_millis < before)
                            .unwrap_or(true)
                })
                .cloned()
                .collect::<Vec<_>>();
            page.sort_by(|left, right| right.captured_at_millis.cmp(&left.captured_at_millis));
            page.truncate(limit.max(1) as usize);
            page.reverse();
            Ok(page)
        }

        fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String> {
            Ok(DiagnosticsOverview::default())
        }

        fn query_diagnostics(
            &self,
            _query: DiagnosticsQuery,
        ) -> Result<Vec<DiagnosticsEvent>, String> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self.0.lock() {
                Ok(mut writer) => writer.extend_from_slice(buf),
                Err(_) => panic!("writer lock"),
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn fake_server() -> AetowerMcpServer {
        AetowerMcpServer {
            data_source: Arc::new(FakeSource),
            dynamic_mode: DynamicExecutionMode::Local,
            mcp_stats: None,
        }
    }

    fn history_broken_server() -> AetowerMcpServer {
        AetowerMcpServer {
            data_source: Arc::new(HistoryBrokenSource),
            dynamic_mode: DynamicExecutionMode::Local,
            mcp_stats: None,
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let mut names = tool_definitions()
            .into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_owned))
            .collect::<Vec<_>>();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), original_len);
    }

    #[test]
    fn tools_list_includes_agent_facing_summary_tools() {
        let names = tool_definitions()
            .into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_owned))
            .collect::<Vec<_>>();
        for expected in [
            "aetower_diff_snapshots",
            "aetower_reboot_report",
            "aetower_explain_anomalies",
            "aetower_entity_process_tree",
            "aetower_top_findings",
            "aetower_host_alerts",
            "aetower_investigation_bundle",
            "aetower_entity_group_tree",
            "aetower_ai_runtime_report",
            "aetower_recent_changes",
            "aetower_capability_status",
            "aetower_history_store_health",
            "aetower_history_data_quality",
            "aetower_memory_breakdown",
            "aetower_profile_entity",
            "aetower_wakeup_attribution",
            "aetower_process_inspect",
            "aetower_process_open_resources",
            "aetower_process_sample",
            "aetower_process_action",
            "aetower_process_action_history",
            "aetower_diagnostics_summary",
            "aetower_support_bundle_manifest",
            "aetower_recommendations",
            "aetower_session_health",
            "aetower_export_query",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "missing tool {expected}"
            );
        }
    }

    #[test]
    fn diagnostics_summary_groups_events_by_subsystem_type_and_level() {
        let report = build_diagnostics_summary_report(
            DiagnosticsOverview {
                warn_count: 2,
                persisted_events: 3,
                ..DiagnosticsOverview::default()
            },
            vec![
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Warn,
                    aetower_diagnostics::DiagnosticsSubsystem::History,
                    "host-incident-snapshot",
                    "Host memory pressure incident snapshot recorded.",
                )
                .timestamp_millis(1_000)
                .field("detail", "memory pressure")
                .build(),
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Warn,
                    aetower_diagnostics::DiagnosticsSubsystem::History,
                    "host-incident-snapshot",
                    "Host wakeup storm incident snapshot recorded.",
                )
                .timestamp_millis(2_000)
                .field("detail", "wakeup storm")
                .build(),
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Error,
                    aetower_diagnostics::DiagnosticsSubsystem::AdapterChau7,
                    "adapter-refresh-failed",
                    "Chau7 adapter refresh failed.",
                )
                .timestamp_millis(3_000)
                .field("error", "timeout")
                .build(),
            ],
            DiagnosticsSummaryOptions {
                limit: 10,
                since_millis: Some(500),
                include_persisted: true,
                minimum_level: None,
                subsystem: None,
                search: None,
            },
        );

        assert_eq!(report.event_count, 3);
        assert_eq!(report.groups[0].event_type, "host-incident-snapshot");
        assert_eq!(report.groups[0].count, 2);
        assert_eq!(
            report.groups[0].latest_detail.as_deref(),
            Some("wakeup storm")
        );
        assert!(report.recommendations.iter().any(|recommendation| {
            recommendation.contains("Host incident snapshots are present")
        }));
    }

    #[test]
    fn diagnostics_severity_keeps_recent_error_pressure_critical() {
        let now = current_unix_millis().unwrap_or(1_000_000);
        let diagnostics = DiagnosticsOverview {
            warn_count: DIAGNOSTICS_WARN_CRITICAL,
            error_count: DIAGNOSTICS_ERROR_CRITICAL,
            last_error_millis: Some(now),
            last_error_message: Some("recent persistence error".to_owned()),
            ..DiagnosticsOverview::default()
        };

        assert_eq!(diagnostics_severity(&diagnostics), SeverityBand::Critical);
        assert_eq!(
            diagnostics_active_error_message(&diagnostics).as_deref(),
            Some("recent persistence error")
        );
    }

    #[test]
    fn diagnostics_severity_downgrades_stale_retained_errors() {
        let now = current_unix_millis().unwrap_or(20 * 60 * 1000);
        let diagnostics = DiagnosticsOverview {
            warn_count: DIAGNOSTICS_WARN_CRITICAL,
            error_count: DIAGNOSTICS_ERROR_CRITICAL,
            last_error_millis: Some(now.saturating_sub(DIAGNOSTICS_ACTIVE_ERROR_WINDOW_MILLIS + 1)),
            last_error_message: Some("old retained error".to_owned()),
            ..DiagnosticsOverview::default()
        };

        assert_eq!(diagnostics_severity(&diagnostics), SeverityBand::Warning);
        assert!(diagnostics_active_error_message(&diagnostics).is_none());
    }

    #[test]
    fn self_runtime_classifies_short_mcp_bursts_as_warning() {
        let runtime = RuntimeLagMetrics {
            mcp_requests_per_second: MCP_REQUEST_CRITICAL_RATE * 4.0,
            mcp_active_client_count: MCP_REQUEST_BURST_CLIENT_LIMIT,
            mcp_helper_count: MCP_REQUEST_BURST_HELPER_LIMIT,
            oldest_mcp_helper_age_millis: 60_000,
            ..RuntimeLagMetrics::default()
        };

        assert_eq!(self_runtime_severity(&runtime), SeverityBand::Warning);
        assert!(self_runtime_recommendation_detail(&runtime).contains("active-agent audit burst"));
    }

    #[test]
    fn self_runtime_classifies_sustained_mcp_pressure_as_critical() {
        let runtime = RuntimeLagMetrics {
            mcp_requests_per_second: MCP_REQUEST_CRITICAL_RATE * 4.0,
            mcp_active_client_count: MCP_REQUEST_BURST_CLIENT_LIMIT + 1,
            mcp_helper_count: MCP_REQUEST_BURST_HELPER_LIMIT + 1,
            oldest_mcp_helper_age_millis: MCP_HELPER_STALE_MILLIS + 1,
            ..RuntimeLagMetrics::default()
        };

        assert_eq!(self_runtime_severity(&runtime), SeverityBand::Critical);
        assert!(self_runtime_recommendation_detail(&runtime).contains("looks sustained"));
    }

    #[test]
    fn reads_content_length_framed_message() {
        let payload = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let mut input = format!("Content-Length: {}\r\n\r\n", payload.len()).into_bytes();
        input.extend_from_slice(payload);
        let mut framing = None;
        let outcome = match read_message(&mut input.as_slice(), &mut framing) {
            Ok(outcome) => outcome,
            Err(error) => panic!("read message: {error}"),
        };
        let ReadMessageOutcome::Message(message) = outcome else {
            panic!("expected framed message");
        };
        assert_eq!(framing, Some(MessageFraming::ContentLength));
        assert_eq!(message.get("method").and_then(Value::as_str), Some("ping"));
    }

    #[test]
    fn reads_lf_only_framed_message() {
        let payload = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let mut input = format!("Content-Length: {}\n\n", payload.len()).into_bytes();
        input.extend_from_slice(payload);
        let mut framing = None;
        let outcome = match read_message(&mut input.as_slice(), &mut framing) {
            Ok(outcome) => outcome,
            Err(error) => panic!("read message: {error}"),
        };
        let ReadMessageOutcome::Message(message) = outcome else {
            panic!("expected framed message");
        };
        assert_eq!(framing, Some(MessageFraming::ContentLength));
        assert_eq!(message.get("method").and_then(Value::as_str), Some("ping"));
    }

    #[test]
    fn reads_newline_delimited_json_message() {
        let payload = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let mut input = payload.to_vec();
        input.push(b'\n');
        let mut framing = None;
        let outcome = match read_message(&mut input.as_slice(), &mut framing) {
            Ok(outcome) => outcome,
            Err(error) => panic!("read message: {error}"),
        };
        let ReadMessageOutcome::Message(message) = outcome else {
            panic!("expected framed message");
        };
        assert_eq!(framing, Some(MessageFraming::JsonLine));
        assert_eq!(message.get("method").and_then(Value::as_str), Some("ping"));
    }

    #[test]
    fn invalid_request_returns_error() {
        let response = match fake_server().handle_message(json!({ "jsonrpc": "2.0", "id": 7 })) {
            Some(response) => response,
            None => panic!("response"),
        };
        assert_eq!(
            response
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_i64),
            Some(-32600)
        );
    }

    #[test]
    fn initialize_returns_server_info() {
        let response = match fake_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        assert_eq!(
            response
                .get("result")
                .and_then(|result| result.get("serverInfo"))
                .and_then(|info| info.get("name"))
                .and_then(Value::as_str),
            Some("aetower")
        );
    }

    #[test]
    fn tools_call_returns_structured_content() {
        let response = match fake_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "aetower_host_summary",
                "arguments": {}
            }
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        assert!(
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .is_some()
        );
    }

    #[test]
    fn top_findings_reports_high_signal_issues() {
        let response = match fake_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "aetower_top_findings",
                "arguments": { "limit": 5 }
            }
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        let findings = response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .and_then(|content| content.get("findings"))
            .and_then(Value::as_array);
        let findings = match findings {
            Some(findings) => findings,
            None => panic!("findings array"),
        };
        assert!(!findings.is_empty());
    }

    #[test]
    fn host_pressure_guidance_prefers_external_burden_leaders() {
        fn entity(
            id: &str,
            name: &str,
            memory_resident_bytes: u64,
            wakeups_per_second: f32,
        ) -> aetower_model::EntitySnapshot {
            aetower_model::EntitySnapshot {
                entity_id: id.to_owned(),
                display_name: name.to_owned(),
                metrics: aetower_model::AggregateMetrics {
                    memory_resident_bytes,
                    wakeups_per_second,
                    ..aetower_model::AggregateMetrics::default()
                },
                friction: aetower_model::FrictionBreakdown {
                    total_score: 10.0,
                    ..aetower_model::FrictionBreakdown::default()
                },
                ..aetower_model::EntitySnapshot::default()
            }
        }

        let snapshot = SystemSnapshot {
            host: aetower_model::HostSnapshot {
                memory_used_bytes: 15 * 1024 * 1024 * 1024,
                memory_total_bytes: 16 * 1024 * 1024 * 1024,
                compressed_memory_bytes: 7 * 1024 * 1024 * 1024,
                swap_used_bytes: 18 * 1024 * 1024 * 1024,
                wakeups_per_second: 31_000.0,
                ..aetower_model::HostSnapshot::default()
            },
            entities: vec![
                entity("aetower", "Aetower", 100 * 1024 * 1024, 500.0),
                entity("chau7", "Chau7", 3 * 1024 * 1024 * 1024, 12_000.0),
                entity("chrome", "Google Chrome", 2 * 1024 * 1024 * 1024, 8_000.0),
            ],
            ..SystemSnapshot::default()
        };
        let runtime = RuntimeLagMetrics {
            self_memory_bytes: 100 * 1024 * 1024,
            self_wakeups_per_second: 500.0,
            ..RuntimeLagMetrics::default()
        };
        let memory = memory_pressure_finding(&snapshot).unwrap_or_else(|| panic!("memory finding"));
        let wakeups = wakeup_finding(&snapshot).unwrap_or_else(|| panic!("wakeup finding"));

        assert!(memory.recommendation.as_deref().is_some_and(|value| {
            value.contains("external memory leaders: Chau7") && value.contains("Aetower self")
        }));
        assert!(wakeups.recommendation.as_deref().is_some_and(|value| {
            value.contains("external wakeup leaders: Chau7") && value.contains("Aetower self")
        }));
        assert!(host_memory_pressure_guidance(&snapshot, &runtime).contains("Chau7"));
        assert!(host_wakeup_guidance(&snapshot, &runtime).contains("Chau7"));
    }

    #[test]
    fn investigation_bundle_returns_current_pressure_context() {
        let response = match fake_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 15,
            "method": "tools/call",
            "params": {
                "name": "aetower_investigation_bundle",
                "arguments": {
                    "start_millis": 0,
                    "end_millis": 1800000,
                    "window_minutes": 30
                }
            }
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        let content = response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .cloned()
            .unwrap_or_else(|| panic!("structuredContent"));
        assert_eq!(
            content.get("window_minutes").and_then(Value::as_u64),
            Some(30)
        );
        assert!(
            content
                .get("host_alerts")
                .and_then(Value::as_array)
                .is_some_and(|alerts| !alerts.is_empty())
        );
        assert!(
            content
                .get("top_findings")
                .and_then(Value::as_array)
                .is_some_and(|findings| !findings.is_empty())
        );
        assert!(
            content
                .get("diagnostics")
                .and_then(Value::as_array)
                .is_some_and(|events| !events.is_empty())
        );
        assert!(
            content
                .get("process_trees")
                .and_then(Value::as_array)
                .is_some_and(|trees| !trees.is_empty())
        );
    }

    #[test]
    fn ai_runtime_report_returns_grouped_ai_insights() {
        let response = match fake_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {
                "name": "aetower_ai_runtime_report",
                "arguments": {}
            }
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        let content = response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .cloned()
            .unwrap_or_else(|| panic!("structuredContent"));
        assert!(content.get("summary").is_some());
        assert!(content.get("runtime_groups").is_some());
        assert!(content.get("burden_leaders").is_some());
        let build = content
            .get("runtime_groups")
            .and_then(Value::as_array)
            .and_then(|groups| {
                groups
                    .iter()
                    .find_map(|group| group.get("chau7_build").and_then(Value::as_object))
            })
            .unwrap_or_else(|| panic!("chau7_build"));
        assert_eq!(
            build.get("app_version").and_then(Value::as_str),
            Some("1.4.2")
        );
    }

    #[test]
    fn ai_runtime_report_degrades_gracefully_when_history_decode_fails() {
        let response = match history_broken_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "tools/call",
            "params": {
                "name": "aetower_ai_runtime_report",
                "arguments": {}
            }
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        let content = response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .cloned()
            .unwrap_or_else(|| panic!("structuredContent"));
        assert_eq!(
            content.get("history_status").and_then(Value::as_str),
            Some("degraded")
        );
        assert!(
            content
                .get("history_warning")
                .and_then(Value::as_str)
                .is_some()
        );
        assert!(content.get("summary").is_some());
        assert!(content.get("runtime_groups").is_some());
    }

    #[test]
    fn export_query_redacts_sensitive_paths() {
        let response = match fake_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tools/call",
            "params": {
                "name": "aetower_export_query",
                "arguments": {
                    "privacy_tier": "redacted",
                    "include_snapshot": true,
                    "include_ai_runtime_report": true
                }
            }
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        let privacy_tier = response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .and_then(|content| content.get("privacyTier"))
            .and_then(Value::as_str);
        assert_eq!(privacy_tier, Some("redacted"));
        assert!(
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("aiRuntimeReport"))
                .is_some()
        );
    }

    #[test]
    fn tool_call_reports_data_source_errors() {
        let response = AetowerMcpServer {
            data_source: Arc::new(BrokenSource),
            dynamic_mode: DynamicExecutionMode::Local,
            mcp_stats: None,
        }
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "aetower_host_summary",
                "arguments": {}
            }
        }));
        let response = match response {
            Some(response) => response,
            None => panic!("response"),
        };
        assert_eq!(
            response
                .get("result")
                .and_then(|result| result.get("isError"))
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn start_local_socket_server_sets_owner_only_permissions() {
        let parent = std::env::temp_dir().join(format!("aetower-mcp-test-{}", std::process::id()));
        let socket_path = parent.join("mcp.sock");
        let _ = fs::remove_file(&socket_path);
        let _ = fs::remove_dir_all(&parent);
        let handle = match start_local_socket_server(Arc::new(FakeSource), &socket_path) {
            Ok(handle) => handle,
            Err(error) => panic!("server: {error}"),
        };
        let dir_mode = match fs::metadata(&parent) {
            Ok(metadata) => metadata.permissions().mode() & 0o777,
            Err(error) => panic!("dir metadata: {error}"),
        };
        let file_mode = match fs::metadata(&socket_path) {
            Ok(metadata) => metadata.permissions().mode() & 0o777,
            Err(error) => panic!("socket metadata: {error}"),
        };
        assert_eq!(dir_mode, SOCKET_DIR_MODE);
        assert_eq!(file_mode, SOCKET_FILE_MODE);
        drop(handle);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn proxy_roundtrip_supports_initialize_and_tool_call() {
        let parent =
            std::env::temp_dir().join(format!("aetower-mcp-proxy-test-{}", std::process::id()));
        let socket_path = parent.join("mcp.sock");
        let _ = fs::remove_file(&socket_path);
        let _ = fs::remove_dir_all(&parent);
        let handle = match start_local_socket_server(Arc::new(FakeSource), &socket_path) {
            Ok(handle) => handle,
            Err(error) => panic!("server: {error}"),
        };

        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }
        });
        let tool_call = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "aetower_host_summary",
                "arguments": {}
            }
        });

        let mut framed = Vec::new();
        for message in [initialize, tool_call] {
            let body = match serde_json::to_vec(&message) {
                Ok(body) => body,
                Err(error) => panic!("serialize request: {error}"),
            };
            if let Err(error) = write!(&mut framed, "Content-Length: {}\r\n\r\n", body.len()) {
                panic!("frame header: {error}");
            }
            framed.extend_from_slice(&body);
        }

        let output = Arc::new(Mutex::new(Vec::new()));
        proxy_streams_to_socket(
            Cursor::new(framed),
            SharedWriter(output.clone()),
            &socket_path,
        )
        .map_err(|error| panic!("proxy roundtrip: {error}"))
        .ok();

        let buffer = match output.lock() {
            Ok(output) => output.clone(),
            Err(_) => panic!("output lock"),
        };
        let mut framed_output = buffer.as_slice();
        let mut framing = None;
        let first = match read_message(&mut framed_output, &mut framing) {
            Ok(message) => message,
            Err(error) => panic!("first frame: {error}"),
        };
        let second = match read_message(&mut framed_output, &mut framing) {
            Ok(message) => message,
            Err(error) => panic!("second frame: {error}"),
        };

        let ReadMessageOutcome::Message(initialize_response) = first else {
            panic!("missing initialize response");
        };
        let ReadMessageOutcome::Message(tool_response) = second else {
            panic!("missing tool response");
        };

        assert_eq!(
            initialize_response
                .get("result")
                .and_then(|result| result.get("serverInfo"))
                .and_then(|info| info.get("name"))
                .and_then(Value::as_str),
            Some("aetower")
        );
        assert!(
            tool_response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .is_some()
        );

        drop(handle);
        let _ = fs::remove_dir_all(&parent);
    }

    #[test]
    fn socket_server_reports_connection_and_request_stats() {
        let parent =
            std::env::temp_dir().join(format!("aetower-mcp-stats-test-{}", std::process::id()));
        let socket_path = parent.join("mcp.sock");
        let _ = fs::remove_file(&socket_path);
        let _ = fs::remove_dir_all(&parent);
        let observations = Arc::new(Mutex::new(Vec::new()));
        let handle = match start_local_socket_server(
            Arc::new(ObservingSource {
                observations: Arc::clone(&observations),
            }),
            &socket_path,
        ) {
            Ok(handle) => handle,
            Err(error) => panic!("server: {error}"),
        };

        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }
        });
        let tool_list = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });
        let mut framed = Vec::new();
        for message in [initialize, tool_list] {
            let body = match serde_json::to_vec(&message) {
                Ok(body) => body,
                Err(error) => panic!("serialize request: {error}"),
            };
            if let Err(error) = write!(&mut framed, "Content-Length: {}\r\n\r\n", body.len()) {
                panic!("frame header: {error}");
            }
            framed.extend_from_slice(&body);
        }
        let output = Arc::new(Mutex::new(Vec::new()));
        proxy_streams_to_socket(Cursor::new(framed), SharedWriter(output), &socket_path)
            .map_err(|error| panic!("proxy roundtrip: {error}"))
            .ok();

        let recorded = observations
            .lock()
            .map(|observations| observations.clone())
            .unwrap_or_default();
        assert!(
            recorded
                .iter()
                .any(|(connections, active, requests)| *connections >= 1
                    && *active >= 1
                    && *requests >= 2),
            "missing active request observation: {recorded:?}"
        );
        assert_eq!(handle.stats().0, 1);
        assert!(handle.stats().2 >= 2);
        drop(handle);
        let _ = fs::remove_dir_all(&parent);
    }

    #[test]
    fn snapshot_diff_report_surfaces_entity_delta() {
        let before = SystemSnapshot {
            captured_at_millis: 100,
            host: aetower_model::HostSnapshot {
                wakeups_per_second: 8_000.0,
                boot_session: Some(aetower_model::BootSessionSnapshot {
                    boot_id: Some("boot-a".to_owned()),
                    boot_time_millis: Some(10),
                    host_uptime_millis: Some(90),
                    previous_shutdown: None,
                }),
                ..aetower_model::HostSnapshot::default()
            },
            entities: vec![aetower_model::EntitySnapshot {
                entity_id: "chau7".to_owned(),
                display_name: "Chau7".to_owned(),
                friction: aetower_model::FrictionBreakdown {
                    total_score: 12.0,
                    ..aetower_model::FrictionBreakdown::default()
                },
                metrics: aetower_model::AggregateMetrics {
                    cpu_percent: 12.0,
                    memory_resident_bytes: 512 * 1024 * 1024,
                    wakeups_per_second: 7_450.0,
                    process_count: 2,
                    ..aetower_model::AggregateMetrics::default()
                },
                ..aetower_model::EntitySnapshot::default()
            }],
            ..SystemSnapshot::default()
        };
        let mut after = before.clone();
        after.captured_at_millis = 200;
        after.host.wakeups_per_second = 400.0;
        after.entities[0].friction.total_score = 3.0;
        after.entities[0].metrics.wakeups_per_second = 50.0;

        let report = build_snapshot_diff_report(&before, &after, &[], 10);
        assert_eq!(report.before_snapshot_millis, 100);
        assert_eq!(report.after_snapshot_millis, 200);
        assert!(!report.crossed_boot_boundary);
        assert_eq!(report.entities.len(), 1);
        assert!(report.entities[0].wakeups_per_second.delta < 0.0);
    }

    #[test]
    fn snapshot_diff_report_flags_boot_boundary() {
        let before = SystemSnapshot {
            captured_at_millis: 100,
            host: aetower_model::HostSnapshot {
                boot_session: Some(aetower_model::BootSessionSnapshot {
                    boot_id: Some("boot-a".to_owned()),
                    boot_time_millis: Some(10),
                    host_uptime_millis: Some(90),
                    previous_shutdown: None,
                }),
                ..aetower_model::HostSnapshot::default()
            },
            ..SystemSnapshot::default()
        };
        let after = SystemSnapshot {
            captured_at_millis: 200,
            host: aetower_model::HostSnapshot {
                boot_session: Some(aetower_model::BootSessionSnapshot {
                    boot_id: Some("boot-b".to_owned()),
                    boot_time_millis: Some(180),
                    host_uptime_millis: Some(20),
                    previous_shutdown: Some(aetower_model::RebootCauseSnapshot {
                        source: "unified-log".to_owned(),
                        code: Some("-128".to_owned()),
                        detail: "Previous shutdown cause: -128".to_owned(),
                        observed_at_millis: Some(181),
                    }),
                }),
                ..aetower_model::HostSnapshot::default()
            },
            ..SystemSnapshot::default()
        };

        let report = build_snapshot_diff_report(&before, &after, &[], 10);
        assert!(report.crossed_boot_boundary);
        assert_eq!(report.before_boot_id.as_deref(), Some("boot-a"));
        assert_eq!(report.after_boot_id.as_deref(), Some("boot-b"));
        assert!(report.after_previous_shutdown.is_some());
    }

    #[test]
    fn snapshot_diff_report_uses_diagnostics_shutdown_fallback() {
        let before = SystemSnapshot {
            captured_at_millis: 100,
            host: aetower_model::HostSnapshot {
                boot_session: Some(aetower_model::BootSessionSnapshot {
                    boot_id: Some("boot-a".to_owned()),
                    boot_time_millis: Some(10),
                    host_uptime_millis: Some(90),
                    previous_shutdown: None,
                }),
                ..aetower_model::HostSnapshot::default()
            },
            ..SystemSnapshot::default()
        };
        let after = SystemSnapshot {
            captured_at_millis: 200,
            host: aetower_model::HostSnapshot {
                boot_session: Some(aetower_model::BootSessionSnapshot {
                    boot_id: Some("boot-b".to_owned()),
                    boot_time_millis: Some(180),
                    host_uptime_millis: Some(20),
                    previous_shutdown: None,
                }),
                ..aetower_model::HostSnapshot::default()
            },
            ..SystemSnapshot::default()
        };
        let diagnostics = vec![
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::Engine,
                "system-previous-shutdown-cause",
                "Observed a previous shutdown cause marker in the recent system log.",
            )
            .timestamp_millis(181)
            .field("detail", "Previous shutdown cause: -128")
            .build(),
        ];

        let report =
            build_snapshot_diff_report_with_diagnostics(&before, &after, &[], 10, &diagnostics);
        assert_eq!(
            report
                .after_previous_shutdown
                .as_ref()
                .and_then(|cause| cause.code.as_deref()),
            Some("-128")
        );
    }

    #[test]
    fn history_data_quality_flags_gaps_duplicates_and_regressions() {
        fn snapshot(captured_at_millis: u64, sequence: u64, boot_id: &str) -> SystemSnapshot {
            SystemSnapshot {
                captured_at_millis,
                sequence,
                host: aetower_model::HostSnapshot {
                    boot_session: Some(aetower_model::BootSessionSnapshot {
                        boot_id: Some(boot_id.to_owned()),
                        boot_time_millis: None,
                        host_uptime_millis: None,
                        previous_shutdown: None,
                    }),
                    ..aetower_model::HostSnapshot::default()
                },
                ..SystemSnapshot::default()
            }
        }

        let snapshots = vec![
            snapshot(0, 10, "boot-a"),
            snapshot(10_000, 11, "boot-a"),
            snapshot(10_000, 12, "boot-a"),
            snapshot(20_000, 9, "boot-a"),
            snapshot(130_000, 13, "boot-a"),
            snapshot(140_000, 14, "boot-b"),
        ];

        let report = build_history_data_quality_report(&snapshots, 0, 140_000, 10_000);
        assert_eq!(report.severity, SeverityBand::Critical);
        assert_eq!(report.duplicate_timestamp_count, 1);
        assert_eq!(report.sequence_regression_count, 1);
        assert_eq!(report.sequence_reset_count, 0);
        assert_eq!(report.gap_count, 1);
        assert_eq!(report.largest_gap_millis, 110_000);
        assert_eq!(report.boot_boundary_count, 1);
        assert!(
            report
                .recommendations
                .iter()
                .any(|recommendation| recommendation.contains("Sequence numbers regressed"))
        );
    }

    #[test]
    fn history_data_quality_treats_restart_sequence_reset_as_gap_not_corruption() {
        let snapshots = vec![
            test_history_snapshot(0, 100),
            test_history_snapshot(120_000, 1),
        ];

        let report = build_history_data_quality_report(&snapshots, 0, 120_000, 10_000);

        assert_eq!(report.severity, SeverityBand::Warning);
        assert_eq!(report.sequence_regression_count, 0);
        assert_eq!(report.sequence_reset_count, 1);
        assert_eq!(report.gap_count, 1);
        assert!(
            report
                .recommendations
                .iter()
                .any(|recommendation| recommendation.contains("usually indicates an app restart"))
        );
    }

    #[test]
    fn history_data_quality_tool_returns_structured_report() {
        let response = match fake_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 16,
            "method": "tools/call",
            "params": {
                "name": "aetower_history_data_quality",
                "arguments": { "window_hours": 1 }
            }
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        let content = response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .cloned()
            .unwrap_or_else(|| panic!("structuredContent"));
        assert_eq!(
            content.get("sampled_snapshots").and_then(Value::as_u64),
            Some(1)
        );
        assert!(
            content
                .get("recommendations")
                .and_then(Value::as_array)
                .is_some_and(|recommendations| !recommendations.is_empty())
        );
    }

    #[test]
    fn history_loader_uses_oldest_timestamp_cursor_for_ascending_pages() {
        let source = PagingHistorySource {
            snapshots: (1..=5)
                .map(|index| test_history_snapshot(index * 1_000, index))
                .collect(),
        };

        let snapshots = load_history_snapshots_raw(&source, 0, 5_000, 10)
            .unwrap_or_else(|error| panic!("load history: {error}"));

        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.captured_at_millis)
                .collect::<Vec<_>>(),
            vec![1_000, 2_000, 3_000, 4_000, 5_000]
        );
        let unique_timestamps = snapshots
            .iter()
            .map(|snapshot| snapshot.captured_at_millis)
            .collect::<BTreeSet<_>>();
        assert_eq!(unique_timestamps.len(), snapshots.len());
    }

    #[test]
    fn history_page_next_cursor_points_to_oldest_returned_snapshot() {
        let server = AetowerMcpServer {
            data_source: Arc::new(PagingHistorySource {
                snapshots: (1..=5)
                    .map(|index| test_history_snapshot(index * 1_000, index))
                    .collect(),
            }),
            dynamic_mode: DynamicExecutionMode::Local,
            mcp_stats: None,
        };
        let response = match server.handle_message(json!({
            "jsonrpc": "2.0",
            "id": 17,
            "method": "tools/call",
            "params": {
                "name": "aetower_history_page",
                "arguments": {
                    "start_millis": 0,
                    "end_millis": 5000,
                    "limit": 2
                }
            }
        })) {
            Some(response) => response,
            None => panic!("response"),
        };
        let content = response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .cloned()
            .unwrap_or_else(|| panic!("structuredContent"));

        assert_eq!(content.get("returned").and_then(Value::as_u64), Some(2));
        assert_eq!(
            content
                .get("next_before_millis_exclusive")
                .and_then(Value::as_u64),
            Some(3_999)
        );
        assert_eq!(
            content
                .get("snapshots")
                .and_then(Value::as_array)
                .and_then(|snapshots| snapshots.first())
                .and_then(|snapshot| snapshot.get("captured_at_millis"))
                .and_then(Value::as_u64),
            Some(4_000)
        );
    }

    #[test]
    fn reboot_report_detects_boundary_and_incidents() {
        struct RebootSource {
            snapshots: Vec<SystemSnapshot>,
            diagnostics: Vec<DiagnosticsEvent>,
        }

        impl AetowerMcpDataSource for RebootSource {
            fn latest_snapshot(&self) -> Result<SystemSnapshot, String> {
                self.snapshots
                    .last()
                    .cloned()
                    .ok_or_else(|| "missing snapshot".to_owned())
            }
            fn latest_snapshot_if_newer(
                &self,
                _last_sequence: u64,
            ) -> Result<Option<SystemSnapshot>, String> {
                Ok(None)
            }
            fn latest_sequence(&self) -> Result<u64, String> {
                Ok(self
                    .snapshots
                    .last()
                    .map(|snapshot| snapshot.sequence)
                    .unwrap_or(0))
            }
            fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String> {
                Ok(RuntimeLagMetrics::default())
            }
            fn history_range_summary(
                &self,
                _start_millis: u64,
                _end_millis: u64,
            ) -> Result<HistorySummaryResponse, String> {
                Ok(HistorySummaryResponse {
                    store_bytes: 1,
                    wal_bytes: 0,
                    snapshot_count: self.snapshots.len() as u64,
                    quarantine_count: 0,
                    range_count: self.snapshots.len() as u64,
                    oldest_millis: self
                        .snapshots
                        .first()
                        .map(|snapshot| snapshot.captured_at_millis),
                    newest_millis: self
                        .snapshots
                        .last()
                        .map(|snapshot| snapshot.captured_at_millis),
                    pending_writes: 0,
                })
            }
            fn load_history_page(
                &self,
                start_millis: u64,
                end_millis: u64,
                before_millis_exclusive: Option<u64>,
                limit: u32,
            ) -> Result<Vec<SystemSnapshot>, String> {
                let mut snapshots = self
                    .snapshots
                    .iter()
                    .filter(|snapshot| {
                        snapshot.captured_at_millis >= start_millis
                            && snapshot.captured_at_millis <= end_millis
                            && before_millis_exclusive
                                .map(|before| snapshot.captured_at_millis < before)
                                .unwrap_or(true)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                snapshots
                    .sort_by(|left, right| right.captured_at_millis.cmp(&left.captured_at_millis));
                snapshots.truncate(limit as usize);
                Ok(snapshots)
            }
            fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String> {
                Ok(DiagnosticsOverview::default())
            }
            fn query_diagnostics(
                &self,
                _query: DiagnosticsQuery,
            ) -> Result<Vec<DiagnosticsEvent>, String> {
                Ok(self.diagnostics.clone())
            }
        }

        let before = SystemSnapshot {
            sequence: 10,
            captured_at_millis: 1_000,
            host: aetower_model::HostSnapshot {
                boot_session: Some(aetower_model::BootSessionSnapshot {
                    boot_id: Some("boot-a".to_owned()),
                    boot_time_millis: Some(100),
                    host_uptime_millis: Some(900),
                    previous_shutdown: None,
                }),
                ..aetower_model::HostSnapshot::default()
            },
            entities: vec![aetower_model::EntitySnapshot {
                display_name: "Chau7".to_owned(),
                ..aetower_model::EntitySnapshot::default()
            }],
            ..SystemSnapshot::default()
        };
        let after = SystemSnapshot {
            sequence: 1,
            captured_at_millis: 2_000,
            host: aetower_model::HostSnapshot {
                boot_session: Some(aetower_model::BootSessionSnapshot {
                    boot_id: Some("boot-b".to_owned()),
                    boot_time_millis: Some(1_850),
                    host_uptime_millis: Some(150),
                    previous_shutdown: None,
                }),
                ..aetower_model::HostSnapshot::default()
            },
            entities: vec![aetower_model::EntitySnapshot {
                display_name: "Aetower".to_owned(),
                ..aetower_model::EntitySnapshot::default()
            }],
            ..SystemSnapshot::default()
        };
        let source = RebootSource {
            snapshots: vec![before, after],
            diagnostics: vec![
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Warn,
                    DiagnosticsSubsystem::History,
                    "host-incident-snapshot",
                    "Host wakeup storm incident snapshot recorded.",
                )
                .timestamp_millis(950)
                .field("detail", "Wakeups were elevated before reboot.")
                .build(),
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Warn,
                    DiagnosticsSubsystem::Engine,
                    "system-previous-shutdown-cause",
                    "Observed a previous shutdown cause marker in the recent system log.",
                )
                .timestamp_millis(1_851)
                .field("detail", "Previous shutdown cause: -128")
                .build(),
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Engine,
                    "engine-initialized",
                    "Aetower engine initialized.",
                )
                .timestamp_millis(1_900)
                .build(),
            ],
        };

        let report =
            build_reboot_report(&source, 500, 2_500).unwrap_or_else(|_| panic!("reboot report"));
        assert_eq!(report.boundary_count, 1);
        assert_eq!(report.session_count, 2);
        assert_eq!(
            report.boundaries[0]
                .previous_shutdown
                .as_ref()
                .and_then(|cause| cause.code.as_deref()),
            Some("-128")
        );
        assert!(!report.boundaries[0].pre_reboot_incidents.is_empty());
    }

    #[test]
    fn anomaly_explanations_identify_changed_driver() {
        let snapshot = SystemSnapshot {
            captured_at_millis: 500,
            entities: vec![aetower_model::EntitySnapshot {
                entity_id: "agent".to_owned(),
                display_name: "Claude Code".to_owned(),
                anomaly_detected: true,
                trend: aetower_model::MetricTrend {
                    friction: vec![4.0, 5.0, 6.0, 24.8],
                    wakeups_per_second: vec![65.0, 70.0, 75.0, 522.0],
                    ..aetower_model::MetricTrend::default()
                },
                recent_change_summary: Some("Friction jumped after tool use.".to_owned()),
                ..aetower_model::EntitySnapshot::default()
            }],
            timeline: vec![aetower_model::TimelineEvent {
                id: "ev".to_owned(),
                timestamp_millis: 480,
                category: aetower_model::TimelineCategory::Anomaly,
                severity: aetower_model::TimelineSeverity::Warning,
                entity_id: Some("agent".to_owned()),
                title: "Friction jumped".to_owned(),
                detail: "Wakeups spiked.".to_owned(),
            }],
            ..SystemSnapshot::default()
        };
        let explanations = build_anomaly_explanations(&snapshot, &[], 10, 60_000);
        assert_eq!(explanations.len(), 1);
        assert!(!explanations[0].drivers.is_empty());
    }

    #[test]
    fn process_tree_report_includes_pid_children() {
        let snapshot = SystemSnapshot {
            captured_at_millis: 123,
            entities: vec![aetower_model::EntitySnapshot {
                entity_id: "root".to_owned(),
                display_name: "Root".to_owned(),
                components: vec![
                    aetower_model::ComponentSnapshot {
                        title: "Root proc".to_owned(),
                        process_id: Some(10),
                        cpu_percent: 5.0,
                        memory_bytes: 100,
                        memory_physical_footprint_bytes: 0,
                        ..aetower_model::ComponentSnapshot::default()
                    },
                    aetower_model::ComponentSnapshot {
                        title: "Child proc".to_owned(),
                        process_id: Some(11),
                        parent_summary: Some("Root proc pid 10".to_owned()),
                        cpu_percent: 2.0,
                        memory_bytes: 50,
                        memory_physical_footprint_bytes: 0,
                        ..aetower_model::ComponentSnapshot::default()
                    },
                ],
                ..aetower_model::EntitySnapshot::default()
            }],
            ..SystemSnapshot::default()
        };
        let report =
            build_process_tree_report(&snapshot, "root").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(report.roots.len(), 1);
        assert_eq!(report.roots[0].children.len(), 1);
    }

    #[test]
    fn process_action_dry_run_does_not_signal() {
        let report =
            build_process_action(&FakeSource, 42, "terminate", true, Some("test".to_owned()))
                .unwrap_or_else(|error| panic!("{error}"));

        assert!(!report.executed);
        assert!(report.success);
        assert_eq!(report.target_pids, vec![42]);
        assert_eq!(report.signal, "TERM");
        assert_eq!(report.command, "/bin/kill -TERM 42");
        assert_eq!(report.reason.as_deref(), Some("test"));
    }

    #[test]
    fn process_action_plan_builds_renice_command() {
        let plan = process_action_plan(None, 42, "lower-priority")
            .unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(plan.normalized_action, "lower-priority");
        assert_eq!(plan.signal, "renice:10");
        assert_eq!(plan.target_pids, vec![42]);
        assert_eq!(plan.command, "/usr/bin/renice 10 -p 42");
        assert_eq!(plan.program, "/usr/bin/renice");
        assert_eq!(plan.args, vec!["10", "-p", "42"]);
    }

    #[test]
    fn process_action_plan_expands_process_tree_from_snapshot() {
        let snapshot = SystemSnapshot {
            entities: vec![aetower_model::EntitySnapshot {
                entity_id: "tree".to_owned(),
                display_name: "Tree".to_owned(),
                components: vec![
                    aetower_model::ComponentSnapshot {
                        title: "Root proc".to_owned(),
                        process_id: Some(10),
                        ..aetower_model::ComponentSnapshot::default()
                    },
                    aetower_model::ComponentSnapshot {
                        title: "Child proc".to_owned(),
                        process_id: Some(11),
                        parent_summary: Some("Root proc pid 10".to_owned()),
                        ..aetower_model::ComponentSnapshot::default()
                    },
                    aetower_model::ComponentSnapshot {
                        title: "Grandchild proc".to_owned(),
                        process_id: Some(12),
                        parent_summary: Some("Child proc pid 11".to_owned()),
                        ..aetower_model::ComponentSnapshot::default()
                    },
                ],
                ..aetower_model::EntitySnapshot::default()
            }],
            ..SystemSnapshot::default()
        };
        let plan = process_action_plan(Some(&snapshot), 10, "terminate-tree")
            .unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(plan.normalized_action, "terminate-tree");
        assert_eq!(plan.signal, "TERM");
        assert_eq!(plan.target_pids, vec![10, 11, 12]);
        assert_eq!(plan.command, "/bin/kill -TERM 10 11 12");
    }

    #[test]
    fn parse_lsof_resources_classifies_sockets() {
        let output = r#"COMMAND   PID USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
zsh       42 user  cwd    DIR               1,17      640    2 /Users/user/repo
node      42 user   12u  IPv4 0x123456789abcdef      0t0  TCP 127.0.0.1:3000 (LISTEN)
"#;

        let resources = parse_lsof_resources(output);

        assert_eq!(resources.len(), 2);
        assert!(!resources[0].is_socket);
        assert!(resources[1].is_socket);
        assert!(resources[1].name.contains("127.0.0.1:3000"));
    }

    #[test]
    fn process_action_history_item_extracts_fields() {
        let event = DiagnosticsEvent::builder(
            DiagnosticsLevel::Info,
            DiagnosticsSubsystem::Engine,
            "process-action",
            "Sent TERM to process 42.",
        )
        .timestamp_millis(123)
        .entity_id("entity")
        .field("pid", 42)
        .field("target_pids", "42,43")
        .field("action", "terminate")
        .field("signal", "TERM")
        .field("success", true)
        .field("reason", "test")
        .field("display_name", "Example")
        .build();

        let item = process_action_history_item(event);

        assert_eq!(item.timestamp_millis, 123);
        assert_eq!(item.pid, Some(42));
        assert_eq!(item.target_pids, vec![42, 43]);
        assert_eq!(item.action.as_deref(), Some("terminate"));
        assert_eq!(item.display_name.as_deref(), Some("Example"));
        assert!(item.success);
    }

    #[test]
    fn parse_vmmap_region_line_extracts_region_sizes() {
        let line = "__TEXT                      104a5c000-104c30000    [ 1872K  1248K     0K     0K] r-x/r-x SM=COW";
        let region = parse_vmmap_region_line(line).unwrap_or_else(|| panic!("region"));
        assert_eq!(region.region_type, "__TEXT");
        assert_eq!(region.virtual_bytes, 1_916_928);
        assert_eq!(region.resident_bytes, 1_277_952);
    }

    #[test]
    fn parse_sample_threads_extracts_queue_labels() {
        let sample = r#"
Call graph:
    883 Thread_20416162   DispatchQueue_1: com.apple.main-thread  (serial)
    + 883 start  (in dyld) + 6076
    +   1 CVDisplayLinkCallback  (in Chau7) + 12
    884 Thread_20416314: reqwest-internal-sync-runtime
    + 884 tokio::runtime::runtime::Runtime::block_on  (in libaetower_ffi.dylib) + 584
"#;
        let threads = parse_sample_threads(sample);
        assert_eq!(threads.len(), 2);
        assert_eq!(
            threads[0].queue_label.as_deref(),
            Some("com.apple.main-thread")
        );
    }
}
