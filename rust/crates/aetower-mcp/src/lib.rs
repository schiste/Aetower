use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsOverview, DiagnosticsQuery, DiagnosticsSubsystem,
};
use aetower_model::{RuntimeLagMetrics, SystemSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod transport;

pub use transport::{
    LocalMcpServerHandle, default_socket_path, is_socket_listener_reachable, proxy_stdio_to_socket,
    start_local_socket_server,
};

mod reports;
mod tools;

pub use reports::history::diff_snapshots_json;
pub use reports::process::{
    entity_process_tree_json, memory_breakdown_json, process_action_history_json,
    process_action_json, process_inspect_json, process_open_resources_json, process_sample_json,
    profile_entity_json, self_memory_attribution_json, wakeup_attribution_json,
};

pub use reports::diagnostics::explain_anomalies_json;
#[cfg(test)]
use reports::history::build_snapshot_diff_report_with_diagnostics;
use reports::history::{
    build_history_data_quality_report, build_history_efficiency_diagnostics,
    build_history_store_health, build_reboot_report, build_snapshot_diff_report,
    investigation_history_diff, load_history_snapshots, load_history_snapshots_raw,
    older_page_cursor, select_investigation_entity_ids,
};
#[cfg(test)]
use reports::process::{
    build_process_action, parse_lsof_resources, parse_sample_threads, parse_vmmap_region_line,
    process_action_history_item, process_action_plan,
};
use reports::process::{
    build_process_tree_report, entity_process_ids, format_metric_value,
    process_dynamic_tool_request, vmmap_regions_for_processes,
};

use reports::diagnostics::{
    build_anomaly_explanations, build_diagnostics_summary_report, build_recommendations,
    build_support_bundle_manifest, host_load_detail,
};
#[cfg(test)]
use reports::diagnostics::{host_memory_pressure_guidance, host_wakeup_guidance};
use reports::export::build_export_query_response;
use reports::runtime::{
    build_ai_runtime_report, build_runtime_burst_explanation,
    build_self_runtime_memory_attribution, build_self_runtime_watch_report,
    build_session_health_checks, load_ai_runtime_history, self_runtime_watch_sample,
};
#[cfg(test)]
use reports::runtime::{self_runtime_watch_averages, self_runtime_watch_summary};
use reports::snapshot::{
    build_capability_status, build_host_alerts, build_recent_changes, build_top_findings,
};
use transport::{McpRuntimeStats, jsonrpc_error, publish_mcp_runtime_stats};

#[cfg(test)]
use transport::{
    MessageFraming, ReadMessageOutcome, SOCKET_DIR_MODE, SOCKET_FILE_MODE, proxy_streams_to_socket,
    read_message,
};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "aetower";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const INITIAL_SNAPSHOT_WAIT: Duration = Duration::from_millis(2500);
const INITIAL_SNAPSHOT_POLL: Duration = Duration::from_millis(100);
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
const DEFAULT_SELF_WATCH_DURATION_SECONDS: u64 = 30;
const MAX_SELF_WATCH_DURATION_SECONDS: u64 = 300;
const DEFAULT_SELF_WATCH_INTERVAL_MILLIS: u64 = 1_000;
const MIN_SELF_WATCH_INTERVAL_MILLIS: u64 = 250;
const MAX_SELF_WATCH_INTERVAL_MILLIS: u64 = 60_000;
const DEFAULT_RUNTIME_BURST_WINDOW_MINUTES: u64 = 10;
const DEFAULT_RUNTIME_BURST_EVENT_LIMIT: usize = 64;
pub(crate) const MEMORY_PRESSURE_WARNING_RATIO: f64 = 0.80;
pub(crate) const MEMORY_PRESSURE_CRITICAL_RATIO: f64 = 0.90;
pub(crate) const COMPRESSED_MEMORY_WARNING_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub(crate) const COMPRESSED_MEMORY_CRITICAL_BYTES: u64 = 6 * 1024 * 1024 * 1024;
pub(crate) const SWAP_WARNING_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub(crate) const SWAP_CRITICAL_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub(crate) const WAKEUPS_WARNING: f32 = 12_000.0;
pub(crate) const WAKEUPS_CRITICAL: f32 = 25_000.0;
const HISTORY_STORE_WARNING_BYTES: u64 = 512 * 1024 * 1024;
const HISTORY_STORE_CRITICAL_BYTES: u64 = 1024 * 1024 * 1024;
const HISTORY_WAL_WARNING_BYTES: u64 = 32 * 1024 * 1024;
const HISTORY_WAL_CRITICAL_BYTES: u64 = 128 * 1024 * 1024;
const HISTORY_SNAPSHOT_WARNING_COUNT: u64 = 3_000;
const HISTORY_SNAPSHOT_CRITICAL_COUNT: u64 = 5_000;
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
    pub store_modified_millis: Option<u64>,
    pub wal_modified_millis: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SeverityBand {
    Info,
    Warning,
    Critical,
}

impl SeverityBand {
    pub(crate) fn score(self) -> u8 {
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
pub(crate) struct TopFinding {
    id: String,
    severity: SeverityBand,
    title: String,
    detail: String,
    source: String,
    entity_ids: Vec<String>,
    recommendation: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HostAlert {
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
pub(crate) struct RecentChangeItem {
    timestamp_millis: u64,
    severity: SeverityBand,
    source: String,
    entity_id: Option<String>,
    title: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilityStatusItem {
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
    efficiency: HistoryEfficiencyDiagnostics,
    thresholds: BTreeMap<String, Value>,
    data_quality: Option<HistoryDataQualityReport>,
    recent_history_events: Vec<DiagnosticsEvent>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct HistoryEfficiencyDiagnostics {
    severity: SeverityBand,
    summary: String,
    window_millis: u64,
    write_rate_per_minute: f64,
    estimated_bytes_per_minute: f64,
    average_snapshot_bytes: u64,
    store_pressure_percent: f64,
    wal_pressure_percent: f64,
    snapshot_count_pressure_percent: f64,
    wal_age_millis: Option<u64>,
    checkpoint_status: String,
    estimated_minutes_to_store_warning: Option<f64>,
    estimated_minutes_to_wal_warning: Option<f64>,
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
struct SelfRuntimeWatchSample {
    observed_at_millis: u64,
    engine_tick_millis: f32,
    target_tick_millis: f32,
    collect_millis: f32,
    history_millis: f32,
    persist_millis: f32,
    bridge_fetch_millis: f32,
    ui_refresh_millis: f32,
    snapshot_to_render_millis: f32,
    render_commit_millis: f32,
    self_cpu_percent: f32,
    self_memory_bytes: u64,
    self_memory_physical_footprint_bytes: u64,
    self_wakeups_per_second: f32,
    mcp_requests_per_second: f32,
    mcp_active_client_count: u32,
    mcp_helper_count: u32,
    stale_mcp_helper_count: u32,
}

#[derive(Debug, Clone, Serialize)]
struct SelfRuntimeWatchAverages {
    engine_tick_millis: f32,
    collect_millis: f32,
    history_millis: f32,
    persist_millis: f32,
    self_cpu_percent: f32,
    self_wakeups_per_second: f32,
    mcp_requests_per_second: f32,
}

#[derive(Debug, Clone, Serialize)]
struct SelfRuntimeMemoryAttribution {
    current_resident_bytes: u64,
    current_physical_footprint_bytes: u64,
    peak_resident_bytes: u64,
    peak_physical_footprint_bytes: u64,
    peak_at_millis: Option<u64>,
    peak_delta_from_current_bytes: i64,
    self_process_id: u32,
    self_entity_id: Option<String>,
    self_display_name: Option<String>,
    process_ids: Vec<u32>,
    regions: Vec<MemoryRegionBreakdown>,
    note: String,
    caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SelfRuntimeWatchReport {
    started_at_millis: u64,
    ended_at_millis: u64,
    duration_seconds: u64,
    interval_millis: u64,
    sample_count: usize,
    summary: String,
    averages: SelfRuntimeWatchAverages,
    memory: SelfRuntimeMemoryAttribution,
    samples: Vec<SelfRuntimeWatchSample>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeBurstDriver {
    key: String,
    severity: SeverityBand,
    metric: String,
    value: String,
    detail: String,
    recommendation: String,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeBurstCorrelatedEvent {
    timestamp_millis: u64,
    level: String,
    subsystem: String,
    event_type: String,
    message: String,
    detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeBurstExplanation {
    captured_at_millis: u64,
    window_minutes: u64,
    severity: SeverityBand,
    summary: String,
    drivers: Vec<RuntimeBurstDriver>,
    correlated_events: Vec<RuntimeBurstCorrelatedEvent>,
    recommendations: Vec<String>,
}

struct RuntimeBurstThreshold<'a> {
    key: &'a str,
    metric: &'a str,
    warning_threshold: f32,
    critical_threshold: f32,
    detail: &'a str,
    recommendation: &'a str,
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

#[derive(Clone)]
pub(crate) enum DynamicExecutionMode {
    Local,
}

pub fn default_app_support_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Aetower")
}

pub(crate) struct AetowerMcpServer {
    data_source: Arc<dyn AetowerMcpDataSource>,
    dynamic_mode: DynamicExecutionMode,
    mcp_stats: Option<Arc<McpRuntimeStats>>,
}

impl AetowerMcpServer {
    pub(crate) fn new_with_stats(
        data_source: Arc<dyn AetowerMcpDataSource>,
        dynamic_mode: DynamicExecutionMode,
        mcp_stats: Option<Arc<McpRuntimeStats>>,
    ) -> Self {
        Self {
            data_source,
            dynamic_mode,
            mcp_stats,
        }
    }

    pub(crate) fn handle_message(&self, message: Value) -> Option<Value> {
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
            "aetower_watch_self" => self.tool_watch_self(arguments),
            "aetower_runtime_burst_explanation" => self.tool_runtime_burst_explanation(arguments),
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

fn default_self_watch_duration_seconds() -> u64 {
    DEFAULT_SELF_WATCH_DURATION_SECONDS
}

fn default_self_watch_interval_millis() -> u64 {
    DEFAULT_SELF_WATCH_INTERVAL_MILLIS
}

fn default_runtime_burst_window_minutes() -> u64 {
    DEFAULT_RUNTIME_BURST_WINDOW_MINUTES
}

fn default_runtime_burst_event_limit() -> usize {
    DEFAULT_RUNTIME_BURST_EVENT_LIMIT
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
        store_modified_millis: None,
        wal_modified_millis: None,
    }
}

fn history_diagnostics_query(start_millis: u64, limit: usize) -> DiagnosticsQuery {
    DiagnosticsQuery {
        limit,
        minimum_level: Some(aetower_diagnostics::DiagnosticsLevel::Info),
        subsystem: None,
        search: Some("history".to_owned()),
        since_millis: Some(start_millis),
        include_persisted: true,
    }
}

fn runtime_diagnostics_query(start_millis: u64, limit: usize) -> DiagnosticsQuery {
    DiagnosticsQuery {
        limit,
        minimum_level: Some(aetower_diagnostics::DiagnosticsLevel::Info),
        subsystem: None,
        search: None,
        since_millis: Some(start_millis),
        include_persisted: true,
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

pub(crate) fn top_entities(
    snapshot: &SystemSnapshot,
    limit: usize,
) -> Vec<&aetower_model::EntitySnapshot> {
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

pub(crate) fn top_memory_entities(
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

pub(crate) fn top_external_memory_entities(
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

pub(crate) fn top_wakeup_entities(
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

pub(crate) fn top_external_wakeup_entities(
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

pub(crate) fn format_entity_burden_labels<F>(
    entities: &[&aetower_model::EntitySnapshot],
    metric: F,
) -> String
where
    F: Fn(&aetower_model::EntitySnapshot) -> String,
{
    entities
        .iter()
        .map(|entity| format!("{} ({})", entity.display_name, metric(entity)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn is_aetower_entity(entity: &aetower_model::EntitySnapshot) -> bool {
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

pub(crate) fn memory_pressure_finding(snapshot: &SystemSnapshot) -> Option<TopFinding> {
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

pub(crate) fn wakeup_finding(snapshot: &SystemSnapshot) -> Option<TopFinding> {
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

pub(crate) fn history_store_finding(history: &HistorySummaryResponse) -> Option<TopFinding> {
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

pub(crate) fn diagnostics_finding(diagnostics: &DiagnosticsOverview) -> Option<TopFinding> {
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

pub(crate) fn capability_operator_label(capability: &aetower_model::CapabilitySnapshot) -> String {
    match capability.state {
        aetower_model::CapabilityState::Granted => "Ready".to_owned(),
        aetower_model::CapabilityState::Denied => "Missing access".to_owned(),
        aetower_model::CapabilityState::Requested => "Pending".to_owned(),
        aetower_model::CapabilityState::Unavailable => "Not available".to_owned(),
        aetower_model::CapabilityState::Unknown => "Not checked".to_owned(),
    }
}

pub(crate) fn capability_action_label(capability: &aetower_model::CapabilitySnapshot) -> String {
    match capability.state {
        aetower_model::CapabilityState::Denied => "Grant access".to_owned(),
        aetower_model::CapabilityState::Requested => "Complete setup".to_owned(),
        aetower_model::CapabilityState::Unknown => "Refresh status".to_owned(),
        aetower_model::CapabilityState::Unavailable => "No action".to_owned(),
        aetower_model::CapabilityState::Granted => "Healthy".to_owned(),
    }
}

pub(crate) fn capability_severity(capability: &aetower_model::CapabilitySnapshot) -> SeverityBand {
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

pub(crate) fn format_bytes(bytes: u64) -> String {
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
            "name": "aetower_watch_self",
            "description": "Run a bounded live watch of Aetower's own runtime overhead, memory peak, MCP pressure, UI latency, and optional self vmmap attribution.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "duration_seconds": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 300,
                        "default": DEFAULT_SELF_WATCH_DURATION_SECONDS
                    },
                    "interval_millis": {
                        "type": "integer",
                        "minimum": 250,
                        "maximum": 60000,
                        "default": DEFAULT_SELF_WATCH_INTERVAL_MILLIS
                    },
                    "include_memory_breakdown": { "type": "boolean", "default": true },
                    "top_regions": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "aetower_runtime_burst_explanation",
            "description": "Explain current Aetower observer overhead by correlating runtime lag, self CPU/wakeups, UI render latency, MCP request pressure, history work, and recent adapter diagnostics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "window_minutes": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 120,
                        "default": DEFAULT_RUNTIME_BURST_WINDOW_MINUTES
                    },
                    "event_limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 256,
                        "default": DEFAULT_RUNTIME_BURST_EVENT_LIMIT
                    }
                },
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Cursor, Write},
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
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
                target_tick_millis: 2_000.0,
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
                store_modified_millis: Some(2),
                wal_modified_millis: Some(2),
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
                store_modified_millis: in_range
                    .iter()
                    .map(|snapshot| snapshot.captured_at_millis)
                    .max(),
                wal_modified_millis: None,
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
            "aetower_watch_self",
            "aetower_runtime_burst_explanation",
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
    fn self_runtime_watch_reports_peak_memory_without_vmmap() {
        let samples = vec![
            SelfRuntimeWatchSample {
                observed_at_millis: 1,
                engine_tick_millis: 40.0,
                target_tick_millis: 2_000.0,
                collect_millis: 20.0,
                history_millis: 10.0,
                persist_millis: 1.0,
                bridge_fetch_millis: 0.0,
                ui_refresh_millis: 0.0,
                snapshot_to_render_millis: 0.0,
                render_commit_millis: 0.0,
                self_cpu_percent: 5.0,
                self_memory_bytes: 100,
                self_memory_physical_footprint_bytes: 200,
                self_wakeups_per_second: 20.0,
                mcp_requests_per_second: 1.0,
                mcp_active_client_count: 1,
                mcp_helper_count: 1,
                stale_mcp_helper_count: 0,
            },
            SelfRuntimeWatchSample {
                observed_at_millis: 2,
                engine_tick_millis: 80.0,
                target_tick_millis: 2_000.0,
                collect_millis: 40.0,
                history_millis: 20.0,
                persist_millis: 3.0,
                bridge_fetch_millis: 0.0,
                ui_refresh_millis: 0.0,
                snapshot_to_render_millis: 0.0,
                render_commit_millis: 0.0,
                self_cpu_percent: 15.0,
                self_memory_bytes: 300,
                self_memory_physical_footprint_bytes: 700,
                self_wakeups_per_second: 40.0,
                mcp_requests_per_second: 3.0,
                mcp_active_client_count: 1,
                mcp_helper_count: 1,
                stale_mcp_helper_count: 0,
            },
        ];
        let averages = self_runtime_watch_averages(&samples);
        let memory = build_self_runtime_memory_attribution(&FakeSource, &samples, false, 3);
        let summary = self_runtime_watch_summary(&samples, &averages, &memory);

        assert_eq!(averages.engine_tick_millis, 60.0);
        assert_eq!(memory.current_physical_footprint_bytes, 700);
        assert_eq!(memory.peak_physical_footprint_bytes, 700);
        assert!(
            memory
                .caveats
                .iter()
                .any(|caveat| caveat.contains("skipped"))
        );
        assert!(summary.contains("peak footprint"));
    }

    #[test]
    fn runtime_burst_explanation_correlates_runtime_and_adapter_events() {
        let runtime = RuntimeLagMetrics {
            engine_tick_millis: 900.0,
            target_tick_millis: 1_000.0,
            collect_millis: 180.0,
            history_millis: 120.0,
            snapshot_to_render_millis: 150.0,
            self_cpu_percent: 32.0,
            self_wakeups_per_second: 1_200.0,
            mcp_requests_per_second: MCP_REQUEST_CRITICAL_RATE + 1.0,
            mcp_active_client_count: 5,
            mcp_helper_count: 5,
            oldest_mcp_helper_age_millis: MCP_HELPER_STALE_MILLIS + 1,
            ..RuntimeLagMetrics::default()
        };
        let events = vec![
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::AdapterChau7,
                "adapter-refresh-failed",
                "Chau7 adapter refresh failed.",
            )
            .field("error", "timeout")
            .build(),
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Persistence,
                "history-retention-maintained",
                "Applied persisted history retention and maintenance policy.",
            )
            .field("checkpointed", true)
            .build(),
        ];

        let explanation = build_runtime_burst_explanation(123, 10, &runtime, &events);

        assert_eq!(explanation.severity, SeverityBand::Critical);
        assert!(
            explanation
                .drivers
                .iter()
                .any(|driver| driver.key == "engine-tick")
        );
        assert!(
            explanation
                .drivers
                .iter()
                .any(|driver| driver.key == "adapter-refresh")
        );
        assert_eq!(explanation.correlated_events.len(), 2);
        assert!(explanation.summary.contains("strongest signal"));
    }

    #[test]
    fn session_health_returns_exact_runtime_lag_sample_used_for_checks() {
        let response = match fake_server().handle_message(json!({
            "jsonrpc": "2.0",
            "id": 25,
            "method": "tools/call",
            "params": {
                "name": "aetower_session_health",
                "arguments": { "history_window_hours": 1 }
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
            content
                .pointer("/runtime_lag/updated_at_millis")
                .and_then(Value::as_u64),
            Some(123)
        );
        assert_eq!(
            content
                .pointer("/runtime_lag/target_tick_millis")
                .and_then(Value::as_f64),
            Some(2_000.0)
        );
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
    fn history_efficiency_reports_growth_and_checkpoint_status() {
        let summary = HistorySummaryResponse {
            store_bytes: HISTORY_STORE_WARNING_BYTES * 9 / 10,
            wal_bytes: HISTORY_WAL_WARNING_BYTES / 2,
            snapshot_count: 900,
            quarantine_count: 0,
            range_count: 60,
            oldest_millis: Some(0),
            newest_millis: Some(60 * 60 * 1000),
            pending_writes: 1,
            store_modified_millis: Some(60 * 60 * 1000),
            wal_modified_millis: Some(60 * 60 * 1000),
        };
        let events = vec![
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Persistence,
                "history-retention-maintained",
                "Applied persisted history retention and maintenance policy.",
            )
            .field("checkpointed", true)
            .field("wal_bytes_after", summary.wal_bytes)
            .build(),
        ];

        let efficiency = build_history_efficiency_diagnostics(&summary, &events);

        assert_eq!(efficiency.severity, SeverityBand::Warning);
        assert!(efficiency.write_rate_per_minute > 0.0);
        assert!(efficiency.store_pressure_percent >= 89.0);
        assert!(efficiency.checkpoint_status.contains("Checkpointed"));
        assert!(efficiency.estimated_minutes_to_store_warning.is_some());
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
                    store_modified_millis: self
                        .snapshots
                        .last()
                        .map(|snapshot| snapshot.captured_at_millis),
                    wal_modified_millis: None,
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
