use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
    ffi::CString,
    fs,
    io::{Read, Write},
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
const COLD_AFTER_DAYS: u64 = 365;
const DUPLICATE_FULL_HASH_MAX_BYTES: u64 = 256 * 1024 * 1024;
const DUPLICATE_GROUP_LIMIT: usize = 8;
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
const RECENT_CLEANUP_BLOCK_MILLIS: u64 = 10 * 60 * 1000;
const STORAGE_TREEMAP_MAX_DEPTH: usize = 4;
const STORAGE_TREEMAP_MAX_CHILDREN: usize = 14;
const STORAGE_TREEMAP_MAX_ITEMS: usize = 160;
const STORAGE_WRITER_LEDGER_MAX_BYTES: u64 = 2 * 1024 * 1024;
const STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS: u64 = 10 * 60 * 1000;
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

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * KIB;
    const GIB: f64 = 1024.0 * MIB;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KB", value / KIB)
    } else {
        format!("{bytes} B")
    }
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
    attribute_storage_growth_delta, known_agent_path, load_storage_writer_ledger_records,
    summarize_agent_hygiene,
};
use cleanup::{
    ArtifactRule, apply_cleanup_guardrails, artifact_attribution, artifact_intelligence,
    block_cleanup, build_cleanup_bundles, build_cleanup_recipes, classify_artifact,
    cleanup_item_confidence, cleanup_tier_label, cleanup_tier_rank, evaluate_budget_guardrails,
    git_status_label, is_app_cache_path, is_app_container_path, is_app_preferences_path,
    is_app_receipt_path, is_app_support_path, is_launch_item_path, is_protected_cleanup_path,
    storage_role_for_kind, storage_role_label, summarize_cleanup_tiers,
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
    StorageArtifactAttribution, StorageBudgetGuardrails, StorageBudgetViolation,
    StorageCleanupBundle, StorageCleanupBundleItem, StorageCleanupRecipe,
    StorageCleanupTierSummary, StorageDuplicateGroup, StorageDuplicateItem,
    StorageGrowthAttribution, StorageGrowthDelta, StorageHygieneActionsResponse,
    StorageHygieneItem, StorageHygieneItemsPageResponse, StorageHygieneOptions,
    StorageHygieneOverviewResponse, StorageHygieneRepoDetailResponse, StorageHygieneSummary,
    StorageInvestigationFinding, StorageInvestigationSummary, StorageItemSortKey,
    StoragePerformanceBudgetDiagnostics, StoragePreventionPolicy, StoragePreventionSuggestion,
    StorageRepoArtifactFolder, StorageRepoArtifactMix, StorageRepoFootprint,
    StorageRepositoryInventoryItem, StorageScanDiagnostics, StorageScanMetrics, StorageSkippedRoot,
    StorageSourceCoverage, StorageSystemDataBucket, StorageTreemapNode, StorageVolumeState,
    StorageWriterLedgerRecord,
};
pub use projection::{
    storage_hygiene_actions_json, storage_hygiene_items_page_json, storage_hygiene_overview_json,
    storage_hygiene_repo_detail_json,
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
use report::{
    StorageCandidateCollector, build_storage_hygiene_report_from_index,
    build_storage_hygiene_report_with_options, finalize_storage_report_json, highest_cleanup_tier,
    normalize_dirty_paths, normalize_roots, path_matches_dirty_prefix,
    refresh_storage_performance_budget, skipped_root_permission_state,
    storage_byte_accounting_label, storage_item_evidence, storage_item_next_step,
    storage_performance_budget_diagnostics, storage_source_kind, storage_source_label,
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
