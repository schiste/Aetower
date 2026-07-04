use std::{
    borrow::Cow,
    cell::RefCell,
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
    ffi::CString,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    mem::MaybeUninit,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, params, params_from_iter};
use serde::{Deserialize, Serialize};

const MAX_LIMIT: usize = 200;
const MAX_ITEMS_PAGE_LIMIT: usize = 10_000;
const MAX_ITEMS_PAGE_OFFSET: usize = 1_000_000;
const MAX_ROOTS: usize = 64;
const MAX_DIRECTORIES: u64 = 25_000;
const SIZE_WALK_MAX_ENTRIES: u64 = 150_000;
const MIN_ITEM_BYTES: u64 = 1024 * 1024;
const LARGE_FILE_BYTES: u64 = 100 * 1024 * 1024;
const LARGE_DIRECTORY_MIN_BYTES: u64 = 1024 * 1024 * 1024;
const LARGE_DIRECTORY_MAX_DEPTH: usize = 3;
const LARGE_DIRECTORY_MAX_PER_ROOT: usize = 15;
const COLD_AFTER_DAYS: u64 = 365;
const DUPLICATE_PARTIAL_HASH_BYTES: usize = 64 * 1024;
const DUPLICATE_FULL_HASH_MAX_BYTES: u64 = 256 * 1024 * 1024;
const DUPLICATE_GROUP_LIMIT: usize = 8;
const REDUNDANCY_GROUP_LIMIT: usize = 12;
const CLEANUP_ACTIVE_HOLDER_PATH_LIMIT: usize = 64;
const APP_FOOTPRINT_LIMIT: usize = 10;
const SCAN_TIME_BUDGET: Duration = Duration::from_millis(6_500);
const GIT_STATUS_TIME_BUDGET: Duration = Duration::from_millis(650);
const GIT_STATUS_MAX_LINES: usize = 80;
const STALE_AFTER_DAYS: u64 = 7;
const CLAUDE_MD_DELEGATION_MAX_BYTES: u64 = 1024;
const AGENTS_MD_MAX_LINES: usize = 250;
const AGENT_READINESS_MAX_SCORE: u64 = 100;
const REPO_ARTIFACT_BUDGET_BYTES: u64 = 30 * 1024 * 1024 * 1024;
const REPO_GROWTH_BUDGET_BYTES_PER_DAY: u64 = 2 * 1024 * 1024 * 1024;
const TOTAL_ARTIFACT_BUDGET_BYTES: u64 = 30 * 1024 * 1024 * 1024;
const FREE_SPACE_FLOOR_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const VOLUME_PRESSURE_FLOOR_PERCENT: u64 = 10;
const SCHEDULED_SCAN_INTERVAL_HOURS: u64 = 24;
const FAST_SIZE_WALK_MAX_ENTRIES: u64 = 30_000;
const DEEP_SIZE_WALK_MAX_ENTRIES: u64 = SIZE_WALK_MAX_ENTRIES;
const FORENSIC_SIZE_WALK_MAX_ENTRIES: u64 = 300_000;
const STORAGE_INDEX_SCHEMA_VERSION: i64 = 2;
const STORAGE_INDEX_FILE_NAME: &str = "storage-index-v1.sqlite3";
const STORAGE_SCAN_STATE_MAX_AGE_MILLIS: u64 = 24 * 60 * 60 * 1000;
const STORAGE_SCAN_PROGRESS_PERSIST_INTERVAL_MILLIS: u64 = 1_000;
const STORAGE_SCAN_PAUSE_POLL: Duration = Duration::from_millis(80);
const STORAGE_SCAN_QUEUE_POLL: Duration = Duration::from_millis(120);
const STORAGE_GROWTH_BUCKET_MILLIS: u64 = 60 * 60 * 1000;
const STORAGE_INDEX_SNAPSHOT_READ_MULTIPLIER: usize = 24;
const STORAGE_INDEX_FLUSH_CHUNK: usize = 512;
const STORAGE_INDEX_LOOKUP_BIND_CHUNK: usize = 500;
const STORAGE_GROWTH_RETENTION_MILLIS: u64 = 30 * 24 * 60 * 60 * 1000;
const STORAGE_GROWTH_INSIGHTS_WINDOW_DAYS: u64 = 30;
const STORAGE_GROWTH_RATE_SCOPE_LIMIT: usize = 20;
const STORAGE_GROWTH_FORECAST_MIN_DAY_BUCKETS: u64 = 3;
const STORAGE_GROWTH_ANOMALY_LIMIT: usize = 12;
const STORAGE_GROWTH_ANOMALY_CANDIDATE_LIMIT: usize = 100;
const STORAGE_GROWTH_ANOMALY_MIN_BASELINE_BUCKETS: u64 = 3;
const STORAGE_GROWTH_ANOMALY_MIN_DELTA_BYTES: u64 = 16 * MIN_ITEM_BYTES;
const STORAGE_GROWTH_ANOMALY_NEW_PATH_BYTES: u64 = 256 * MIN_ITEM_BYTES;
const STORAGE_SCAN_DIFF_ENTRY_LIMIT: usize = 12;
const COLD_COOLING_AFTER_DAYS: u64 = 90;
const STORAGE_COLD_BAND_TOP_ITEMS: usize = 10;
const DAY_MILLIS: u64 = 24 * 60 * 60 * 1000;
const RECENT_CLEANUP_BLOCK_MILLIS: u64 = 10 * 60 * 1000;
const STORAGE_TREEMAP_MAX_DEPTH: usize = 4;
const STORAGE_TREEMAP_MAX_CHILDREN: usize = 14;
const STORAGE_TREEMAP_MAX_ITEMS: usize = 160;
const STORAGE_WRITER_LEDGER_MAX_BYTES: u64 = 2 * 1024 * 1024;
const STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS: u64 = 10 * 60 * 1000;
const STORAGE_FILESYSTEM_EVENT_LEDGER_MAX_BYTES: u64 = 2 * 1024 * 1024;
const REPOSITORY_INVENTORY_TIME_BUDGET: Duration = Duration::from_millis(30_000);
const REPOSITORY_INVENTORY_MAX_DIRECTORIES: u64 = 200_000;
const STORAGE_SCAN_LATENCY_WARN_MILLIS: u64 = 3_000;
const STORAGE_SCAN_LATENCY_CRITICAL_MILLIS: u64 = 8_000;
const STORAGE_PAYLOAD_WARN_BYTES: u64 = 2 * 1024 * 1024;
const STORAGE_PAYLOAD_CRITICAL_BYTES: u64 = 6 * 1024 * 1024;
const STORAGE_TABLE_PAGE_WARN_MILLIS: u64 = 80;
const STORAGE_TABLE_PAGE_CRITICAL_MILLIS: u64 = 250;
const STORAGE_RENDER_WARN_MILLIS: u64 = 50;
const STORAGE_RENDER_CRITICAL_MILLIS: u64 = 120;
const STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY: &str = "repository_inventory";
const STORAGE_SCAN_PHASE_ARTIFACT_SIZING: &str = "artifact_sizing";
const STORAGE_SCAN_PHASE_SCORECARD_OVERLAY: &str = "scorecard_overlay";
const STORAGE_SCAN_PHASE_FINALIZING: &str = "finalizing";

fn storage_now_millis() -> u64 {
    crate::current_unix_millis().unwrap_or_default()
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn path_is_under_root(path: &str, root: &Path) -> bool {
    let root = root.display().to_string();
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn first_non_empty(primary: Option<&str>, fallback: Option<&str>) -> Option<String> {
    primary
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fallback.filter(|value| !value.trim().is_empty()))
        .map(str::to_owned)
}

fn add_bytes(map: &mut BTreeMap<String, u64>, key: &str, bytes: u64) {
    let entry = map.entry(key.to_owned()).or_insert(0);
    *entry = entry.saturating_add(bytes);
}

fn merge_bytes(target: &mut BTreeMap<String, u64>, source: &BTreeMap<String, u64>) {
    for (key, bytes) in source {
        add_bytes(target, key, *bytes);
    }
}

fn dominant_key(map: &BTreeMap<String, u64>) -> Option<String> {
    map.iter()
        .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(key, _)| key.clone())
}

fn unique_limited(values: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            unique.push(value);
        }
        if unique.len() >= limit {
            break;
        }
    }
    unique
}

fn percent_u64(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        0
    } else {
        numerator.saturating_mul(100) / denominator
    }
}

fn percent(part: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    ((part as f64 / total as f64) * 1000.0).round() as f32 / 10.0
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn existing_parent(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(parent) = current {
        if parent.exists() {
            return Some(parent.to_path_buf());
        }
        current = parent.parent();
    }
    None
}

fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

// Decimal (1000-based) units to match how the macOS UI reports storage
// (ByteCountFormatter .file, Finder, Disk Utility). Dividing by 1024 while
// labelling the result "GB" made server prose disagree with the Swift
// rendering of the same byte count (e.g. "7.0 GB" vs "7,5 GB").
fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1000.0;
    const MB: f64 = 1000.0 * KB;
    const GB: f64 = 1000.0 * MB;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Composite reclaim-recommendation score, computed from row-local factors
/// only so it can be persisted at index flush time and recomputed identically
/// at hydration time:
///
/// ```text
/// score = log2(physical_bytes / 1 MiB)   // size, log-scaled; < 1 MiB => 0
///       * tier_weight(cleanup_tier)      // safe 1.0, rebuildable 0.8,
///                                        // expensive 0.3, risky 0.1, else 0
///       * (1 + min(age_days / 180, 1))   // staleness: untouched rows up to 2x
/// ```
///
/// `age_days` is derived from max(modified, accessed) relative to
/// `reference_millis` (the row's `last_scan_millis` at flush time). Rows with
/// no timestamps get no staleness boost rather than a guessed one.
fn storage_recommendation_score(
    physical_bytes: u64,
    cleanup_tier: &str,
    modified_millis: Option<u64>,
    accessed_millis: Option<u64>,
    reference_millis: u64,
) -> f64 {
    let tier_weight = match cleanup_tier {
        "safe" => 1.0,
        "rebuildable" => 0.8,
        "expensive" => 0.3,
        "risky" => 0.1,
        _ => 0.0,
    };
    if tier_weight == 0.0 || physical_bytes == 0 {
        return 0.0;
    }
    let size_factor = (physical_bytes as f64 / MIN_ITEM_BYTES as f64)
        .log2()
        .max(0.0);
    let touched_millis = modified_millis
        .unwrap_or(0)
        .max(accessed_millis.unwrap_or(0));
    let staleness = if touched_millis == 0 {
        1.0
    } else {
        let age_days = reference_millis.saturating_sub(touched_millis) / DAY_MILLIS;
        1.0 + (age_days as f64 / 180.0).min(1.0)
    };
    size_factor * tier_weight * staleness
}

fn rebuild_seconds_label(seconds: u64) -> String {
    if seconds >= 3_600 {
        let hours = seconds / 3_600;
        let minutes = (seconds % 3_600) / 60;
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes}m")
        }
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

mod agent_guidance;
mod attribution;
mod cleanup;
mod jobs;
mod models;
mod projection;
mod repo;
mod report;
mod state_store;
#[cfg(test)]
mod tests;
mod treemap;
mod walk;
use crate::reports::process::build_resource_holders_by_files;
use agent_guidance::{
    agent_contract_candidate_paths, agent_contract_coverage_audit, agent_guidance_audit,
    guidance_status,
};
#[cfg(test)]
use agent_guidance::{
    agent_contract_definitions, audit_reference_paths, line_explicitly_prohibits_command,
    local_markdown_paths, markdown_headings,
};
use attribution::{
    attribute_storage_growth_delta, known_agent_path, load_storage_filesystem_event_records,
    load_storage_writer_ledger_records, summarize_agent_hygiene,
};
use cleanup::{
    ArtifactRule, apply_artifact_rule_intelligence, apply_cleanup_guardrails,
    apply_measured_rebuild_costs, artifact_attribution, artifact_intelligence, block_cleanup,
    build_cleanup_bundles, build_cleanup_recipes, classify_artifact, cleanup_item_confidence,
    cleanup_tier_label, cleanup_tier_rank, evaluate_budget_guardrails, git_status_label,
    is_app_cache_path, is_app_container_path, is_app_preferences_path, is_app_receipt_path,
    is_app_support_path, is_launch_item_path, is_protected_cleanup_path, large_directory_rule,
    semantic_artifact_intelligence, storage_role_for_kind, storage_role_label,
    summarize_cleanup_tiers,
};
pub(crate) use jobs::StorageScanJobProgress;
#[cfg(test)]
use jobs::{StorageScanControl, StorageScanJobRequest, StorageScanThrottle};
use jobs::{StorageScanMode, StorageScanRuntimeContext};
pub use jobs::{
    storage_scan_cancel_json, storage_scan_pause_json, storage_scan_result_json,
    storage_scan_resume_json, storage_scan_start_json, storage_scan_status_json,
};
pub(crate) use models::StorageHygieneReport;
use models::{
    RepositoryInventoryCompleteness, RepositoryInventoryDiagnostics, RepositoryInventoryReport,
    RepositoryInventoryRootCoverage, StorageAgentArtifactSummary, StorageAgentContractCoverage,
    StorageAgentGuidanceIssue, StorageAgentHygieneSummary, StorageAgentItemSummary,
    StorageAgentRepoSummary, StorageAppFootprint, StorageAppFootprintComponent,
    StorageAppOwnershipSignal, StorageArtifactAttribution, StorageBudgetGuardrails,
    StorageBudgetViolation, StorageCleanupBundle, StorageCleanupBundleItem, StorageCleanupRecipe,
    StorageCleanupTierSummary, StorageColdData, StorageColdDataBand, StorageDuplicateGroup,
    StorageDuplicateItem, StorageFilesystemEventRecord, StorageGrowthAnomaly,
    StorageGrowthAttribution, StorageGrowthDelta, StorageGrowthForecast, StorageGrowthInsights,
    StorageGrowthInsightsResponse, StorageGrowthRate, StorageHygieneActionsResponse,
    StorageHygieneItem, StorageHygieneItemsPageResponse, StorageHygieneOptions,
    StorageHygieneOverviewResponse, StorageHygieneRepoDetailResponse, StorageHygieneSummary,
    StorageInvestigationFinding, StorageInvestigationSummary, StorageItemSortKey,
    StoragePerformanceBudgetDiagnostics, StoragePreventionPolicy, StoragePreventionSuggestion,
    StorageRedundancyGroup, StorageRedundancyItem, StorageRepoArtifactFolder,
    StorageRepoArtifactMix, StorageRepoFootprint, StorageRepositoryInventoryItem,
    StorageScanDiagnostics, StorageScanDiff, StorageScanDiffEntry, StorageScanMetrics,
    StorageSkippedRoot, StorageSourceCoverage, StorageSystemDataBucket, StorageTreemapNode,
    StorageVolumeState, StorageWriterLedgerRecord,
};
pub use projection::{
    storage_growth_insights_json, storage_hygiene_actions_json, storage_hygiene_items_page_json,
    storage_hygiene_overview_json, storage_hygiene_repo_detail_json,
};
pub use repo::repository_inventory_json;
use repo::{
    RepositoryQuality, apply_clone_groups_to_repo_footprints,
    apply_growth_deltas_to_repo_footprints, cached_repository_inventory_coverage, find_git_root,
    git_ignored_path_set, git_repository_has_active_changes, git_status_path_map,
    git_tracked_path_set, is_git_repository_root, merge_repository_inventory_cache, read_git_head,
    repository_git_file_fingerprint, repository_inventory_cache_roots,
    repository_inventory_cache_states, repository_inventory_completeness,
    repository_inventory_fingerprint, scan_repository_inventory_roots_with_budget,
    summarize_repo_footprints, summarize_repository_inventory,
};
pub(crate) use report::build_storage_hygiene_report_with_mode;
#[cfg(test)]
use report::{
    CleanupPathHolder, apply_active_cleanup_holders, build_storage_cold_data, per_root_walk_slice,
};
use report::{
    StorageCandidateCollector, build_storage_hygiene_report_from_index,
    build_storage_hygiene_report_with_options, finalize_storage_report_json, highest_cleanup_tier,
    normalize_dirty_paths, normalize_roots, path_matches_dirty_prefix,
    refresh_storage_performance_budget, skipped_root_permission_state,
    storage_byte_accounting_label, storage_item_evidence, storage_item_next_step,
    storage_local_reclaimable_bytes, storage_performance_budget_diagnostics, storage_source_kind,
    storage_source_label, summarize_volume_states,
};
#[cfg(test)]
pub(crate) use report::{
    build_storage_hygiene_report_for_roots, build_storage_hygiene_report_for_roots_mode,
};
pub use report::{
    storage_hygiene_deep_scan_json, storage_hygiene_indexed_json, storage_hygiene_json,
    storage_hygiene_mode_json,
};
use state_store::{
    RepositoryInventoryCacheEntry, RepositoryInventoryCacheState, StorageIndexedFileRow,
    StorageScanPersistedRecord, StorageScanPersistedState, StorageScanStateStore, StorageSizeIndex,
};
use treemap::build_storage_treemap_roots;
use walk::{
    SizeWalkResult, file_access_age_days, is_cloud_storage_path, is_network_storage_path,
    scan_root, storage_item_for_indexed_row, unix_metadata_millis,
};
