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

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const MAX_LIMIT: usize = 200;
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

#[derive(Clone, Debug)]
struct RankedStorageItem {
    size_bytes: u64,
    path: String,
    item: StorageHygieneItem,
}

impl PartialEq for RankedStorageItem {
    fn eq(&self, other: &Self) -> bool {
        self.size_bytes == other.size_bytes && self.path == other.path
    }
}

impl Eq for RankedStorageItem {}

impl PartialOrd for RankedStorageItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedStorageItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.size_bytes
            .cmp(&other.size_bytes)
            .then_with(|| self.path.cmp(&other.path))
    }
}

#[derive(Debug)]
struct StorageCandidateCollector {
    limit: usize,
    heap: BinaryHeap<Reverse<RankedStorageItem>>,
    seen: u64,
}

impl StorageCandidateCollector {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            heap: BinaryHeap::new(),
            seen: 0,
        }
    }

    fn push(&mut self, item: StorageHygieneItem) {
        self.seen = self.seen.saturating_add(1);
        if self.limit == 0 {
            return;
        }
        let ranked = RankedStorageItem {
            size_bytes: item.size_bytes,
            path: item.path.clone(),
            item,
        };
        if self.heap.len() < self.limit {
            self.heap.push(Reverse(ranked));
            return;
        }
        if self.heap.peek().is_some_and(|current| ranked > current.0) {
            let _ = self.heap.pop();
            self.heap.push(Reverse(ranked));
        }
    }

    fn into_sorted_items(self) -> Vec<StorageHygieneItem> {
        let mut items: Vec<_> = self.heap.into_iter().map(|ranked| ranked.0.item).collect();
        items.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        items
    }
}

fn path_is_under_root(path: &str, root: &Path) -> bool {
    let root = root.display().to_string();
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

pub fn storage_hygiene_json(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
) -> Result<String, String> {
    storage_hygiene_mode_json(
        roots,
        max_depth,
        limit,
        StorageScanMode::FastChangedOnly.as_str(),
    )
}

pub fn repository_inventory_json(roots: Vec<String>, max_depth: usize) -> Result<String, String> {
    let started = Instant::now();
    let captured_at_millis = crate::current_unix_millis().unwrap_or_default();
    let requested_roots = normalize_roots(roots);
    let root_labels = requested_roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    let repository_cache = StorageSizeIndex::open();
    let cached_repository_entries =
        repository_cache.load_repository_inventory_cache(&requested_roots);
    let scan = scan_repository_inventory_roots(&requested_roots, max_depth);
    let repository_walk_millis = started.elapsed().as_millis() as u64;
    let discovered_repository_count = scan.repositories_by_root.len().min(u64::MAX as usize) as u64;
    let scanned_directory_count = scan
        .coverage
        .iter()
        .map(|coverage| coverage.scanned_directory_count)
        .fold(0u64, u64::saturating_add);
    let skipped_directory_count = scan
        .coverage
        .iter()
        .map(|coverage| coverage.skipped_directory_count)
        .fold(0u64, u64::saturating_add);

    let mut metrics = StorageScanMetrics {
        storage_index_status: "repository_inventory_only".to_owned(),
        ..StorageScanMetrics::default()
    };
    repository_cache.store_repository_inventory_cache(
        &scan.repositories_by_root,
        captured_at_millis,
        &mut metrics,
    );
    let cached_repository_roots = repository_inventory_cache_roots(&cached_repository_entries);
    let (repository_roots, not_seen_repository_roots) = merge_repository_inventory_cache(
        cached_repository_roots,
        scan.repositories_by_root.clone(),
    );
    let repository_cache_states = repository_inventory_cache_states(
        &repository_roots,
        &scan.repositories_by_root,
        &cached_repository_entries,
    );
    let repository_inventory = summarize_repository_inventory(
        repository_roots,
        &not_seen_repository_roots,
        &repository_cache_states,
        started,
        StorageScanMode::FastChangedOnly,
        &mut metrics,
    );
    let mut repository_inventory_completeness =
        repository_inventory_completeness(&scan.coverage, scan.truncated);
    if !not_seen_repository_roots.is_empty() {
        repository_inventory_completeness.complete = false;
    }
    let report = RepositoryInventoryReport {
        captured_at_millis,
        scan_duration_millis: started.elapsed().as_millis() as u64,
        roots: root_labels,
        repository_inventory,
        repository_inventory_complete: repository_inventory_completeness.complete,
        repository_inventory_truncated: repository_inventory_completeness.truncated,
        repository_inventory_roots: repository_inventory_completeness.roots,
        repository_inventory_partial_roots: repository_inventory_completeness.partial_roots,
        repository_inventory_coverage: scan.coverage,
        truncated: scan.truncated,
        diagnostics: RepositoryInventoryDiagnostics {
            repository_walk_millis,
            git_millis: metrics.git_millis,
            discovered_repository_count,
            scanned_directory_count,
            skipped_directory_count,
        },
    };
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub fn storage_hygiene_mode_json(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: &str,
) -> Result<String, String> {
    finalize_storage_report_json(build_storage_hygiene_report_with_mode(
        roots, max_depth, limit, mode,
    ))
}

pub fn storage_hygiene_indexed_json(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
) -> Result<String, String> {
    finalize_storage_report_json(build_storage_hygiene_report_from_index(
        roots, max_depth, limit,
    )?)
}

fn finalize_storage_report_json(mut report: StorageHygieneReport) -> Result<String, String> {
    let serialize_started = Instant::now();
    let json = serde_json::to_string(&report).map_err(|error| error.to_string())?;
    report.diagnostics.serialize_millis = serialize_started.elapsed().as_millis() as u64;
    report.diagnostics.payload_bytes = json.len().min(u64::MAX as usize) as u64;
    refresh_storage_performance_budget(&mut report, 0, 0);
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn refresh_storage_performance_budget(
    report: &mut StorageHygieneReport,
    table_page_millis: u64,
    render_publish_millis: u64,
) {
    let mut notes = Vec::new();
    let mut severity = 0u8;
    if report.scan_duration_millis >= STORAGE_SCAN_LATENCY_CRITICAL_MILLIS {
        severity = severity.max(2);
        notes.push(format!(
            "scan latency exceeded critical budget: {}ms >= {}ms",
            report.scan_duration_millis, STORAGE_SCAN_LATENCY_CRITICAL_MILLIS
        ));
    } else if report.scan_duration_millis >= STORAGE_SCAN_LATENCY_WARN_MILLIS {
        severity = severity.max(1);
        notes.push(format!(
            "scan latency exceeded warning budget: {}ms >= {}ms",
            report.scan_duration_millis, STORAGE_SCAN_LATENCY_WARN_MILLIS
        ));
    }
    if report.diagnostics.payload_bytes >= STORAGE_PAYLOAD_CRITICAL_BYTES {
        severity = severity.max(2);
        notes.push(format!(
            "payload exceeded critical budget: {} bytes >= {} bytes",
            report.diagnostics.payload_bytes, STORAGE_PAYLOAD_CRITICAL_BYTES
        ));
    } else if report.diagnostics.payload_bytes >= STORAGE_PAYLOAD_WARN_BYTES {
        severity = severity.max(1);
        notes.push(format!(
            "payload exceeded warning budget: {} bytes >= {} bytes",
            report.diagnostics.payload_bytes, STORAGE_PAYLOAD_WARN_BYTES
        ));
    }
    if table_page_millis >= STORAGE_TABLE_PAGE_CRITICAL_MILLIS {
        severity = severity.max(2);
        notes.push(format!(
            "table page exceeded critical budget: {table_page_millis}ms >= {STORAGE_TABLE_PAGE_CRITICAL_MILLIS}ms"
        ));
    } else if table_page_millis >= STORAGE_TABLE_PAGE_WARN_MILLIS {
        severity = severity.max(1);
        notes.push(format!(
            "table page exceeded warning budget: {table_page_millis}ms >= {STORAGE_TABLE_PAGE_WARN_MILLIS}ms"
        ));
    }
    if render_publish_millis >= STORAGE_RENDER_CRITICAL_MILLIS {
        severity = severity.max(2);
        notes.push(format!(
            "render publish exceeded critical budget: {render_publish_millis}ms >= {STORAGE_RENDER_CRITICAL_MILLIS}ms"
        ));
    } else if render_publish_millis >= STORAGE_RENDER_WARN_MILLIS {
        severity = severity.max(1);
        notes.push(format!(
            "render publish exceeded warning budget: {render_publish_millis}ms >= {STORAGE_RENDER_WARN_MILLIS}ms"
        ));
    }
    if report.diagnostics.candidate_seen_count >= 1_000_000 {
        notes.push(format!(
            "stress fixture scale: {} candidates seen",
            report.diagnostics.candidate_seen_count
        ));
    }
    if notes.is_empty() {
        notes.push("storage scan and UI payload stayed within current budgets".to_owned());
    }
    report.diagnostics.performance_budget = StoragePerformanceBudgetDiagnostics {
        status: match severity {
            0 => "ok",
            1 => "warn",
            _ => "critical",
        }
        .to_owned(),
        scan_job_latency_millis: report.scan_duration_millis,
        payload_bytes: report.diagnostics.payload_bytes,
        payload_budget_bytes: STORAGE_PAYLOAD_WARN_BYTES,
        table_page_millis,
        table_page_budget_millis: STORAGE_TABLE_PAGE_WARN_MILLIS,
        render_publish_millis,
        render_budget_millis: STORAGE_RENDER_WARN_MILLIS,
        notes,
    };
}

pub fn storage_hygiene_overview_json(
    roots: Vec<String>,
    max_depth: usize,
    mode: &str,
) -> Result<String, String> {
    let report = build_storage_hygiene_projection_report(roots, max_depth, 40, mode);
    serde_json::to_string(&StorageHygieneOverviewResponse {
        captured_at_millis: report.captured_at_millis,
        scan_duration_millis: report.scan_duration_millis,
        scan_mode: report.scan_mode,
        diagnostics: report.diagnostics,
        summary: report.summary,
        investigation: report.investigation,
        cleanup_tiers: report.cleanup_tiers,
        cleanup_recipes: report.cleanup_recipes.into_iter().take(8).collect(),
        cleanup_bundles: report.cleanup_bundles.into_iter().take(4).collect(),
        budget_guardrails: report.budget_guardrails,
        agent_hygiene: report.agent_hygiene,
        repository_inventory_complete: report.repository_inventory_complete,
        repository_inventory_truncated: report.repository_inventory_truncated,
        repository_inventory_roots: report.repository_inventory_roots,
        repository_inventory_partial_roots: report.repository_inventory_partial_roots,
        repository_inventory_coverage: report.repository_inventory_coverage,
        repo_footprints: report.repo_footprints.into_iter().take(8).collect(),
        duplicate_groups: report.duplicate_groups.into_iter().take(6).collect(),
        app_footprints: report.app_footprints.into_iter().take(6).collect(),
        system_data_buckets: report.system_data_buckets,
        treemap_roots: report.treemap_roots,
        items: report.items.into_iter().take(12).collect(),
        roots: report.roots,
        skipped_roots: report.skipped_roots,
        source_coverage: report.source_coverage,
        volume_states: report.volume_states,
        growth_deltas: report.growth_deltas,
        truncated: report.truncated,
        caveats: report.caveats,
    })
    .map_err(|error| error.to_string())
}

pub fn storage_hygiene_actions_json(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: &str,
) -> Result<String, String> {
    let report = build_storage_hygiene_projection_report(roots, max_depth, limit, mode);
    serde_json::to_string(&StorageHygieneActionsResponse {
        captured_at_millis: report.captured_at_millis,
        scan_mode: report.scan_mode,
        diagnostics: report.diagnostics,
        cleanup_tiers: report.cleanup_tiers,
        cleanup_recipes: report.cleanup_recipes,
        cleanup_bundles: report.cleanup_bundles,
        budget_guardrails: report.budget_guardrails,
    })
    .map_err(|error| error.to_string())
}

fn sort_storage_items(
    items: &mut [StorageHygieneItem],
    sort_key: StorageItemSortKey,
    descending: bool,
) {
    items.sort_by(|left, right| {
        let ordering = match sort_key {
            StorageItemSortKey::Size => left.size_bytes.cmp(&right.size_bytes),
            StorageItemSortKey::Path => left.path.cmp(&right.path),
            StorageItemSortKey::Modified => left
                .modified_millis
                .unwrap_or_default()
                .cmp(&right.modified_millis.unwrap_or_default()),
            StorageItemSortKey::Accessed => left
                .accessed_millis
                .unwrap_or_default()
                .cmp(&right.accessed_millis.unwrap_or_default()),
            StorageItemSortKey::Tier => left
                .cleanup_tier
                .cmp(&right.cleanup_tier)
                .then_with(|| left.safety.cmp(&right.safety)),
            StorageItemSortKey::Kind => left.kind.cmp(&right.kind),
        };

        if ordering == Ordering::Equal {
            left.path.cmp(&right.path)
        } else if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

pub fn storage_hygiene_items_page_json(
    roots: Vec<String>,
    max_depth: usize,
    offset: usize,
    limit: usize,
    mode: &str,
    sort_key: &str,
    sort_descending: bool,
) -> Result<String, String> {
    let requested_limit = offset.saturating_add(limit).clamp(1, MAX_LIMIT);
    let mut report =
        build_storage_hygiene_projection_report(roots, max_depth, requested_limit, mode);
    let table_started = Instant::now();
    let sort_key = StorageItemSortKey::parse(sort_key);
    let mut report_items = std::mem::take(&mut report.items);
    sort_storage_items(&mut report_items, sort_key, sort_descending);
    let total_available = report_items.len();
    let items = report_items
        .into_iter()
        .skip(offset)
        .take(limit.min(MAX_LIMIT))
        .collect::<Vec<_>>();
    refresh_storage_performance_budget(&mut report, table_started.elapsed().as_millis() as u64, 0);
    serde_json::to_string(&StorageHygieneItemsPageResponse {
        captured_at_millis: report.captured_at_millis,
        scan_mode: report.scan_mode,
        diagnostics: report.diagnostics,
        offset,
        limit,
        sort_key: sort_key.as_str().to_owned(),
        sort_descending,
        returned_count: items.len(),
        total_available,
        has_more: offset.saturating_add(items.len()) < total_available,
        items,
    })
    .map_err(|error| error.to_string())
}

pub fn storage_hygiene_repo_detail_json(repo_root: String, mode: &str) -> Result<String, String> {
    let report = build_storage_hygiene_projection_report(vec![repo_root.clone()], 8, 120, mode);
    let repository = report
        .repository_inventory
        .iter()
        .find(|repository| repository.repo_root == repo_root)
        .cloned()
        .or_else(|| report.repository_inventory.first().cloned());
    serde_json::to_string(&StorageHygieneRepoDetailResponse {
        captured_at_millis: report.captured_at_millis,
        scan_mode: report.scan_mode,
        diagnostics: report.diagnostics,
        repository,
        repo_footprints: report.repo_footprints,
        items: report.items,
        cleanup_recipes: report.cleanup_recipes,
        cleanup_bundles: report.cleanup_bundles,
        caveats: report.caveats,
    })
    .map_err(|error| error.to_string())
}

pub fn storage_hygiene_deep_scan_json(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
) -> Result<String, String> {
    storage_hygiene_mode_json(
        roots,
        max_depth,
        limit,
        StorageScanMode::DeepNative.as_str(),
    )
}

pub(crate) fn build_storage_hygiene_report_with_mode(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: &str,
) -> StorageHygieneReport {
    build_storage_hygiene_report_with_options(
        roots,
        StorageHygieneOptions {
            max_depth: max_depth.clamp(1, 12),
            limit: limit.clamp(1, MAX_LIMIT),
            mode: StorageScanMode::parse(mode),
            runtime: None,
            dirty_paths: Vec::new(),
        },
    )
}

fn build_storage_hygiene_projection_report(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: &str,
) -> StorageHygieneReport {
    if StorageScanMode::parse(mode) == StorageScanMode::InstantCached
        && let Ok(report) = build_storage_hygiene_report_from_index(roots.clone(), max_depth, limit)
    {
        return report;
    }
    build_storage_hygiene_report_with_mode(roots, max_depth, limit, mode)
}

fn build_storage_hygiene_report_with_options(
    roots: Vec<String>,
    options: StorageHygieneOptions,
) -> StorageHygieneReport {
    let started = Instant::now();
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let roots = normalize_roots(roots);
    let requested_roots = roots.clone();
    let mut metrics = StorageScanMetrics {
        storage_index_status: "not_opened".to_owned(),
        ..StorageScanMetrics::default()
    };
    let storage_index = if options.mode.use_storage_index() {
        StorageSizeIndex::open()
    } else {
        StorageSizeIndex::disabled("mode_requires_fresh_walk")
    };
    metrics.storage_index_status = storage_index.status.clone();
    let repository_cache = StorageSizeIndex::open();
    let cached_repository_entries =
        repository_cache.load_repository_inventory_cache(&requested_roots);
    if let Some(runtime) = options.runtime.as_ref() {
        let _ = runtime.set_phase(STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY, None);
    }
    let repository_inventory_scan = scan_repository_inventory_roots_with_budget(
        &requested_roots,
        options.max_depth,
        REPOSITORY_INVENTORY_TIME_BUDGET,
        options.runtime.as_ref(),
    );
    if let Some(runtime) = options.runtime.as_ref() {
        let _ = runtime.set_phase(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, None);
    }
    let mut collector = StorageCandidateCollector::new(options.limit);
    repository_cache.store_repository_inventory_cache(
        &repository_inventory_scan.repositories_by_root,
        now_millis,
        &mut metrics,
    );
    let cached_repository_roots = repository_inventory_cache_roots(&cached_repository_entries);
    let (repository_roots, not_seen_repository_roots) = merge_repository_inventory_cache(
        cached_repository_roots,
        repository_inventory_scan.repositories_by_root.clone(),
    );
    let repository_cache_states = repository_inventory_cache_states(
        &repository_roots,
        &repository_inventory_scan.repositories_by_root,
        &cached_repository_entries,
    );
    let mut scanned_roots = Vec::new();
    let mut skipped_roots = Vec::new();
    let mut scanned_directory_count = 0;
    let mut truncated = repository_inventory_scan.truncated;

    for root in roots {
        if started.elapsed() >= SCAN_TIME_BUDGET {
            truncated = true;
            break;
        }
        if let Some(runtime) = options.runtime.as_ref()
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&root), 0, 0, 0)
        {
            truncated = true;
            break;
        }

        let root_display = root.display().to_string();
        if !root.exists() {
            skipped_roots.push(StorageSkippedRoot {
                path: root_display,
                reason: "missing".to_owned(),
            });
            continue;
        }

        let metadata = match fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) => {
                skipped_roots.push(StorageSkippedRoot {
                    path: root_display,
                    reason: error.to_string(),
                });
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            skipped_roots.push(StorageSkippedRoot {
                path: root_display,
                reason: "symlink root skipped".to_owned(),
            });
            continue;
        }

        scanned_roots.push(root.display().to_string());
        let root_started = Instant::now();
        let (_root_repositories, root_dirs, root_truncated) = scan_root(
            &root,
            &options,
            started,
            now_millis,
            &storage_index,
            &mut collector,
            &mut metrics,
        );
        metrics.root_walk_millis = metrics
            .root_walk_millis
            .saturating_add(root_started.elapsed().as_millis() as u64);
        scanned_directory_count += root_dirs;
        truncated |= root_truncated;
    }

    metrics.candidate_seen_count = collector.seen;
    metrics.scanned_directory_count = scanned_directory_count;
    metrics.discovered_repository_count = repository_roots.len().min(u64::MAX as usize) as u64;
    let mut items = collector.into_sorted_items();
    if options.mode.verify_source_control() {
        let git_started = Instant::now();
        annotate_items_source_control(&mut items);
        metrics.git_millis = metrics
            .git_millis
            .saturating_add(git_started.elapsed().as_millis() as u64);
    }
    apply_cleanup_guardrails(&mut items, now_millis);
    for item in &mut items {
        item.evidence = storage_item_evidence(item);
        item.next_step = storage_item_next_step(item);
    }

    if let Some(runtime) = options.runtime.as_ref() {
        let _ = runtime.set_phase(STORAGE_SCAN_PHASE_SCORECARD_OVERLAY, None);
    }
    let mut repository_inventory_completeness = repository_inventory_completeness(
        &repository_inventory_scan.coverage,
        repository_inventory_scan.truncated,
    );
    if !not_seen_repository_roots.is_empty() {
        repository_inventory_completeness.complete = false;
    }
    let repository_inventory = summarize_repository_inventory(
        repository_roots,
        &not_seen_repository_roots,
        &repository_cache_states,
        started,
        options.mode,
        &mut metrics,
    );
    if let Some(runtime) = options.runtime.as_ref() {
        let _ = runtime.set_phase(STORAGE_SCAN_PHASE_FINALIZING, None);
    }

    let summary = summarize_storage_items(&items, scanned_directory_count);
    let investigation = summarize_storage_investigation(&items, truncated);
    let cleanup_tiers = summarize_cleanup_tiers(&items);
    let cleanup_recipes = build_cleanup_recipes(&items);
    let cleanup_bundles = build_cleanup_bundles(&items);
    let mut repo_footprints = summarize_repo_footprints(&items);
    let duplicate_groups = summarize_duplicate_groups(&items);
    let app_footprints = summarize_app_footprints(&items);
    let system_data_buckets = summarize_system_data_buckets(&items);
    let treemap_roots = build_storage_treemap_roots(&items, &scanned_roots);
    let growth_deltas = storage_index.load_growth_deltas(40);
    apply_growth_deltas_to_repo_footprints(&mut repo_footprints, &growth_deltas);
    apply_clone_groups_to_repo_footprints(&mut repo_footprints, &repository_inventory);
    let agent_hygiene = summarize_agent_hygiene(&items);
    let source_coverage =
        summarize_source_coverage(&requested_roots, &scanned_roots, &skipped_roots, &items);
    let volume_states = summarize_volume_states(&requested_roots);
    let budget_guardrails = evaluate_budget_guardrails(
        &summary,
        &repo_footprints,
        &volume_states,
        &items,
        &growth_deltas,
    );
    let diagnostics = StorageScanDiagnostics {
        mode: options.mode.as_str().to_owned(),
        root_walk_millis: metrics.root_walk_millis,
        size_walk_millis: metrics.size_walk_millis,
        git_millis: metrics.git_millis,
        serialize_millis: 0,
        payload_bytes: 0,
        decode_millis: 0,
        scanned_directory_count: metrics.scanned_directory_count,
        discovered_repository_count: metrics.discovered_repository_count,
        sized_entry_count: metrics.sized_entry_count,
        candidate_seen_count: metrics.candidate_seen_count,
        candidate_retained_count: items.len().min(u64::MAX as usize) as u64,
        storage_index_status: metrics.storage_index_status,
        storage_index_hits: metrics.storage_index_hits,
        storage_index_misses: metrics.storage_index_misses,
        storage_index_writes: metrics.storage_index_writes,
        native_metadata_strategy: options.mode.native_metadata_strategy().to_owned(),
        fsevents_status: "swift_cache_invalidation".to_owned(),
        lazy_git_status: !options.mode.collect_git_status(),
        top_k_retained: true,
        performance_budget: StoragePerformanceBudgetDiagnostics::default(),
    };
    StorageHygieneReport {
        captured_at_millis: now_millis,
        scan_duration_millis: started.elapsed().as_millis() as u64,
        scan_mode: options.mode.as_str().to_owned(),
        diagnostics,
        summary,
        investigation,
        cleanup_tiers,
        cleanup_recipes,
        cleanup_bundles,
        budget_guardrails,
        agent_hygiene,
        repository_inventory,
        repository_inventory_complete: repository_inventory_completeness.complete,
        repository_inventory_truncated: repository_inventory_completeness.truncated,
        repository_inventory_roots: repository_inventory_completeness.roots,
        repository_inventory_partial_roots: repository_inventory_completeness.partial_roots,
        repository_inventory_coverage: repository_inventory_scan.coverage,
        repo_footprints,
        duplicate_groups,
        app_footprints,
        system_data_buckets,
        treemap_roots,
        items,
        roots: scanned_roots,
        skipped_roots,
        source_coverage,
        volume_states,
        growth_deltas,
        truncated,
        caveats: vec![
            "Cleanup cockpit: Aetower prepares evidence, reveal targets, verification commands, and Trash-first cleanup manifests."
                .to_owned(),
            "Sizes are bounded estimates and may omit paths that require additional permissions."
                .to_owned(),
            "Review candidates may be rebuildable but can still contain release artifacts or local environments."
                .to_owned(),
            "Last-access timestamps can be unavailable, coarse, or lazily updated depending on the macOS volume."
                .to_owned(),
            "Command and process-tree attribution uses indexed deltas plus optional writer ledgers from Aetower/Chau7; unmatched writers are reported as low-confidence instead of guessed."
                .to_owned(),
            "Repository inventory runs before artifact sizing so repository coverage remains available even when artifact sizing truncates."
                .to_owned(),
        ],
    }
}

fn build_storage_hygiene_report_from_index(
    roots: Vec<String>,
    _max_depth: usize,
    limit: usize,
) -> Result<StorageHygieneReport, String> {
    let started = Instant::now();
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let roots = normalize_roots(roots);
    let requested_roots = roots.clone();
    let mut metrics = StorageScanMetrics {
        storage_index_status: "not_opened".to_owned(),
        ..StorageScanMetrics::default()
    };
    let storage_index = StorageSizeIndex::open();
    metrics.storage_index_status = storage_index.status.clone();
    let cached_repository_entries = storage_index.load_repository_inventory_cache(&requested_roots);
    let repository_roots = repository_inventory_cache_roots(&cached_repository_entries);
    let limit = limit.clamp(1, MAX_LIMIT);
    let rows = storage_index.load_candidate_rows(&roots, limit, &mut metrics)?;
    if rows.is_empty() && repository_roots.is_empty() {
        return Err("storage index has no candidate rows yet".to_owned());
    }

    let mut items = rows
        .into_iter()
        .map(|row| storage_item_for_indexed_row(row, now_millis))
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    let git_started = Instant::now();
    annotate_items_source_control(&mut items);
    metrics.git_millis = metrics
        .git_millis
        .saturating_add(git_started.elapsed().as_millis() as u64);
    apply_cleanup_guardrails(&mut items, now_millis);
    for item in &mut items {
        item.evidence = storage_item_evidence(item);
        item.next_step = storage_item_next_step(item);
    }
    metrics.discovered_repository_count = repository_roots.len().min(u64::MAX as usize) as u64;
    let repository_inventory_coverage =
        cached_repository_inventory_coverage(&requested_roots, &repository_roots);
    let repository_inventory_completeness =
        repository_inventory_completeness(&repository_inventory_coverage, false);
    let not_seen_repository_roots = BTreeSet::new();
    let latest_repository_roots = BTreeMap::new();
    let repository_cache_states = repository_inventory_cache_states(
        &repository_roots,
        &latest_repository_roots,
        &cached_repository_entries,
    );
    let repository_inventory = summarize_repository_inventory(
        repository_roots,
        &not_seen_repository_roots,
        &repository_cache_states,
        started,
        StorageScanMode::InstantCached,
        &mut metrics,
    );

    let scanned_roots = roots
        .iter()
        .filter(|root| root.exists())
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    let skipped_roots = requested_roots
        .iter()
        .filter(|root| !root.exists())
        .map(|root| StorageSkippedRoot {
            path: root.display().to_string(),
            reason: "missing".to_owned(),
        })
        .collect::<Vec<_>>();
    let summary = summarize_storage_items(&items, 0);
    let investigation = summarize_storage_investigation(&items, false);
    let cleanup_tiers = summarize_cleanup_tiers(&items);
    let cleanup_recipes = build_cleanup_recipes(&items);
    let cleanup_bundles = build_cleanup_bundles(&items);
    let mut repo_footprints = summarize_repo_footprints(&items);
    let duplicate_groups = summarize_duplicate_groups(&items);
    let app_footprints = summarize_app_footprints(&items);
    let system_data_buckets = summarize_system_data_buckets(&items);
    let treemap_roots = build_storage_treemap_roots(&items, &scanned_roots);
    metrics.sized_entry_count = items.len().min(u64::MAX as usize) as u64;
    metrics.candidate_seen_count = metrics.sized_entry_count;
    let growth_deltas = storage_index.load_growth_deltas(40);
    apply_growth_deltas_to_repo_footprints(&mut repo_footprints, &growth_deltas);
    apply_clone_groups_to_repo_footprints(&mut repo_footprints, &repository_inventory);
    let agent_hygiene = summarize_agent_hygiene(&items);
    let source_coverage =
        summarize_source_coverage(&requested_roots, &scanned_roots, &skipped_roots, &items);
    let volume_states = summarize_volume_states(&requested_roots);
    let budget_guardrails = evaluate_budget_guardrails(
        &summary,
        &repo_footprints,
        &volume_states,
        &items,
        &growth_deltas,
    );
    let diagnostics = StorageScanDiagnostics {
        mode: StorageScanMode::InstantCached.as_str().to_owned(),
        root_walk_millis: 0,
        size_walk_millis: 0,
        git_millis: metrics.git_millis,
        serialize_millis: 0,
        payload_bytes: 0,
        decode_millis: 0,
        scanned_directory_count: 0,
        discovered_repository_count: metrics.discovered_repository_count,
        sized_entry_count: metrics.sized_entry_count,
        candidate_seen_count: metrics.candidate_seen_count,
        candidate_retained_count: items.len().min(u64::MAX as usize) as u64,
        storage_index_status: format!("snapshot:{}", metrics.storage_index_status),
        storage_index_hits: metrics.storage_index_hits,
        storage_index_misses: metrics.storage_index_misses,
        storage_index_writes: 0,
        native_metadata_strategy: "persistent_index".to_owned(),
        fsevents_status: "dirty_paths_refresh_full_scan".to_owned(),
        lazy_git_status: true,
        top_k_retained: true,
        performance_budget: StoragePerformanceBudgetDiagnostics::default(),
    };
    Ok(StorageHygieneReport {
        captured_at_millis: now_millis,
        scan_duration_millis: started.elapsed().as_millis() as u64,
        scan_mode: StorageScanMode::InstantCached.as_str().to_owned(),
        diagnostics,
        summary,
        investigation,
        cleanup_tiers,
        cleanup_recipes,
        cleanup_bundles,
        budget_guardrails,
        agent_hygiene,
        repository_inventory,
        repository_inventory_complete: repository_inventory_completeness.complete,
        repository_inventory_truncated: repository_inventory_completeness.truncated,
        repository_inventory_roots: repository_inventory_completeness.roots,
        repository_inventory_partial_roots: repository_inventory_completeness.partial_roots,
        repository_inventory_coverage,
        repo_footprints,
        duplicate_groups,
        app_footprints,
        system_data_buckets,
        treemap_roots,
        items,
        roots: scanned_roots,
        skipped_roots,
        source_coverage,
        volume_states,
        growth_deltas,
        truncated: false,
        caveats: vec![
            "Loaded from Aetower's persistent storage index for instant display.".to_owned(),
            "Run a refresh before destructive cleanup when the displayed path changed recently."
                .to_owned(),
            "Growth attribution is based on indexed size deltas and optional Aetower/Chau7 writer ledger records."
                .to_owned(),
        ],
    })
}

fn annotate_items_source_control(items: &mut [StorageHygieneItem]) {
    let mut grouped = BTreeMap::<String, Vec<(usize, String)>>::new();
    for (index, item) in items.iter().enumerate() {
        let Some(repo_root) = item.attribution.repo_root.as_deref() else {
            continue;
        };
        let Ok(relative_path) = Path::new(&item.path).strip_prefix(repo_root) else {
            continue;
        };
        let relative = relative_path.display().to_string();
        if relative.is_empty() {
            continue;
        }
        grouped
            .entry(repo_root.to_owned())
            .or_default()
            .push((index, relative));
    }

    for (repo_root, indexed_paths) in grouped {
        let relative_paths: Vec<String> =
            indexed_paths.iter().map(|(_, path)| path.clone()).collect();
        let tracked = git_tracked_path_set(Path::new(&repo_root), &relative_paths);
        let ignored = git_ignored_path_set(Path::new(&repo_root), &relative_paths);
        let status_map = git_status_path_map(Path::new(&repo_root), &relative_paths);
        let repo_dirty = git_repository_has_active_changes(Path::new(&repo_root));
        for (index, relative_path) in indexed_paths {
            let status = if let Some(status) = status_map.get(&relative_path) {
                status.as_str()
            } else if tracked.contains(&relative_path) {
                "tracked"
            } else if ignored.contains(&relative_path) {
                "ignored"
            } else if matches!(
                items[index].storage_role.as_str(),
                "build-artifact" | "cache" | "temporary" | "dependency-tree" | "environment"
            ) {
                "generated-or-cache"
            } else {
                "untracked"
            };
            items[index].git_status = status.to_owned();
            if repo_dirty {
                block_cleanup(
                    &mut items[index],
                    "Owning Git repository has active worktree changes.",
                );
            }
        }
    }
}

fn storage_item_evidence(item: &StorageHygieneItem) -> Vec<String> {
    let mut evidence = Vec::new();
    evidence.push(format!(
        "Matched {} rule; storage role is {}.",
        item.kind,
        storage_role_label(&item.storage_role)
    ));
    evidence.push(format!(
        "Cleanup tier is {}; confidence score is {}%.",
        cleanup_tier_label(&item.cleanup_tier),
        cleanup_item_confidence(item)
    ));
    evidence.push(format!(
        "Size estimate is {}.",
        human_bytes(item.size_bytes)
    ));
    if item.logical_bytes != item.physical_bytes {
        evidence.push(format!(
            "Logical size is {}; physical reclaim estimate is {} using {}.",
            human_bytes(item.logical_bytes),
            human_bytes(item.physical_bytes),
            item.byte_accounting
        ));
    } else {
        evidence.push(format!("Byte accounting uses {}.", item.byte_accounting));
    }
    if item.sparse_or_shared {
        evidence.push(
            "Physical bytes are lower than logical bytes; APFS clone, compression, sparse, or cloud materialization may be involved."
                .to_owned(),
        );
    }
    if item.has_hardlinks {
        evidence.push(format!(
            "Hardlink count reached {}; reclaim may be lower while another link remains.",
            item.hardlink_count
        ));
    }
    if item.cloud_placeholder {
        evidence.push(
            "This path looks cloud-backed or dehydrated: logical bytes may not be present locally."
                .to_owned(),
        );
    }
    if item.protected_path {
        evidence.push("Path is under a protected system/application root.".to_owned());
    }
    evidence.push(format!(
        "Estimated rebuild cost is {}{}.",
        item.estimated_rebuild_cost,
        item.estimated_rebuild_seconds
            .map(|seconds| format!(" (~{})", rebuild_seconds_label(seconds)))
            .unwrap_or_default()
    ));
    if item.size_truncated {
        evidence.push(
            "Size walk was truncated by the scan budget; actual size may be larger.".to_owned(),
        );
    }
    if let Some(age_days) = item.age_days {
        evidence.push(if age_days == 0 {
            "Modified today.".to_owned()
        } else {
            format!("Last modified about {age_days} day(s) ago.")
        });
    }
    if let Some(access_age_days) = item.access_age_days {
        evidence.push(if access_age_days == 0 {
            "Accessed today.".to_owned()
        } else {
            format!("Last accessed about {access_age_days} day(s) ago.")
        });
    }
    if item.cold {
        evidence.push(format!(
            "Access age is past the {COLD_AFTER_DAYS}-day cold-file threshold."
        ));
    }
    if item.stale {
        evidence.push(format!(
            "Older than the {STALE_AFTER_DAYS}-day stale threshold."
        ));
    }
    if let Some(repo_name) = item.attribution.repo_name.as_deref() {
        let branch = item
            .attribution
            .git_branch
            .as_deref()
            .or(item.attribution.git_head.as_deref())
            .unwrap_or("unknown ref");
        evidence.push(format!(
            "Attributed to Git repository {repo_name} at {branch}."
        ));
    } else {
        evidence.push("No enclosing Git repository was found.".to_owned());
    }
    evidence.push(format!(
        "Source-control status: {}.",
        git_status_label(&item.git_status)
    ));
    if !item.cleanup_allowed {
        evidence.push(format!(
            "Cleanup blocked: {}.",
            item.cleanup_blockers.join("; ")
        ));
    }
    if let Some(session) = item.attribution.ai_agent_session.as_deref() {
        evidence.push(format!("AI-agent directory evidence: {session}."));
    }
    evidence.extend(item.attribution.notes.iter().take(2).cloned());
    unique_limited(evidence, 12)
}

fn storage_byte_accounting_label(logical_bytes: u64, physical_bytes: u64) -> String {
    if logical_bytes == 0 && physical_bytes == 0 {
        "empty path".to_owned()
    } else if physical_bytes == 0 {
        "logical fallback or cloud placeholder".to_owned()
    } else if physical_bytes < logical_bytes {
        "APFS physical blocks".to_owned()
    } else if physical_bytes > logical_bytes {
        "allocated blocks including filesystem overhead".to_owned()
    } else {
        "logical and physical bytes match".to_owned()
    }
}

fn storage_item_next_step(item: &StorageHygieneItem) -> String {
    if item.size_truncated {
        return "Reveal the path and run the copied `du` command when the machine is idle to confirm true size.".to_owned();
    }
    if !item.cleanup_consequence.is_empty() {
        return format!(
            "{} {}",
            item.cleanup_consequence,
            match item.cleanup_tier.as_str() {
                "safe" => "Stage it only after confirming the owner is idle.",
                "rebuildable" => "Stage it after stopping active builds/tests.",
                "expensive" => "Prefer a manual review because refetch/rebuild can be costly.",
                "risky" =>
                    "Manual review required; do not automate cleanup, and confirm supersession or backup first.",
                _ => "Reveal and inspect the path before taking action.",
            }
        );
    }
    match item.cleanup_tier.as_str() {
        "safe" => "Reveal the item, confirm the current debugging/support session no longer needs it, then copy the cleanup plan if appropriate.".to_owned(),
        "rebuildable" => "Stop active builds/tests first; this should be recoverable by rerunning the owning toolchain.".to_owned(),
        "expensive" => "Confirm dependency manifests or environment setup can recreate it before reclaiming space.".to_owned(),
        "risky" => "Manual review required: confirm this is generated or superseded before deleting anything.".to_owned(),
        _ => "Reveal and inspect the path before taking action.".to_owned(),
    }
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

fn summarize_storage_items(
    items: &[StorageHygieneItem],
    scanned_directory_count: u64,
) -> StorageHygieneSummary {
    let mut summary = StorageHygieneSummary {
        item_count: items.len(),
        scanned_directory_count,
        ..StorageHygieneSummary::default()
    };

    for item in items {
        summary.total_reclaimable_bytes = summary
            .total_reclaimable_bytes
            .saturating_add(item.size_bytes);
        if item.safety == "safe" {
            summary.safe_candidate_count += 1;
        } else {
            summary.review_candidate_count += 1;
        }
        if item.stale {
            summary.stale_candidate_count += 1;
        }
        if item.attribution.repo_root.is_some() {
            summary.attributed_repo_count += 1;
        }
        if item.size_bytes > summary.largest_item_bytes {
            summary.largest_item_bytes = item.size_bytes;
            summary.largest_item_path = Some(item.path.clone());
        }
    }

    summary
}

fn summarize_storage_investigation(
    items: &[StorageHygieneItem],
    truncated: bool,
) -> StorageInvestigationSummary {
    let mut summary = StorageInvestigationSummary {
        open_conflict_status:
            "not_checked: open-file conflict detection requires a file-event/open-FD journal."
                .to_owned(),
        ..StorageInvestigationSummary::default()
    };

    for item in items {
        match item.cleanup_tier.as_str() {
            "rebuildable" => {
                summary.rebuildable_bytes =
                    summary.rebuildable_bytes.saturating_add(item.size_bytes)
            }
            "expensive" => {
                summary.expensive_bytes = summary.expensive_bytes.saturating_add(item.size_bytes)
            }
            "risky" => summary.risky_bytes = summary.risky_bytes.saturating_add(item.size_bytes),
            _ => {}
        }
        if matches!(item.storage_role.as_str(), "cache" | "build-artifact") {
            summary.known_cache_bytes = summary.known_cache_bytes.saturating_add(item.size_bytes);
        }
        if matches!(item.kind.as_str(), "large-file" | "release-artifact") {
            summary.large_file_count += 1;
        }
        if item.cold || item.kind == "cold-file" {
            summary.cold_file_count += 1;
            summary.cold_file_bytes = summary.cold_file_bytes.saturating_add(item.size_bytes);
        }
        if item.safety != "safe" || matches!(item.cleanup_tier.as_str(), "expensive" | "risky") {
            summary.review_item_count += 1;
        }
    }

    let mut seen_paths = BTreeSet::new();
    push_investigation_finding(
        &mut summary.top_findings,
        &mut seen_paths,
        "largest",
        "Largest candidate",
        "Biggest item found by the bounded storage scan.",
        largest_item_by(items, |_| true),
    );
    push_investigation_finding(
        &mut summary.top_findings,
        &mut seen_paths,
        "rebuildable",
        "Largest rebuildable",
        "Best first target if active builds/tests are idle.",
        largest_item_by(items, |item| item.cleanup_tier == "rebuildable"),
    );
    push_investigation_finding(
        &mut summary.top_findings,
        &mut seen_paths,
        "review",
        "Needs manual review",
        "Largest non-safe candidate; do not automate cleanup.",
        largest_item_by(items, |item| {
            item.safety != "safe" || item.cleanup_tier == "risky"
        }),
    );
    push_investigation_finding(
        &mut summary.top_findings,
        &mut seen_paths,
        "large-file",
        "Large file",
        "Standalone large file or package surfaced for inspection.",
        largest_item_by(items, |item| {
            item.kind == "large-file" || item.kind == "release-artifact"
        }),
    );
    push_investigation_finding(
        &mut summary.top_findings,
        &mut seen_paths,
        "cold-file",
        "Cold file",
        "File that has not been accessed for more than a year.",
        largest_item_by(items, |item| item.cold || item.kind == "cold-file"),
    );
    push_investigation_finding(
        &mut summary.top_findings,
        &mut seen_paths,
        "agent",
        "Agent-attributed",
        "Largest artifact tied to known local AI-agent evidence.",
        largest_item_by(items, |item| item.attribution.ai_agent_session.is_some()),
    );
    push_investigation_finding(
        &mut summary.top_findings,
        &mut seen_paths,
        "stale",
        "Stale artifact",
        "Oldest-size pressure that crossed the stale threshold.",
        largest_item_by(items, |item| item.stale),
    );
    summary.top_findings.truncate(6);

    if items.is_empty() {
        summary.recommended_next_steps.push(
            "No large local development artifacts were found in the scanned roots.".to_owned(),
        );
    } else {
        if summary.rebuildable_bytes > 0 {
            summary.recommended_next_steps.push(format!(
                "Start with rebuildable/cache candidates: {} can usually be recreated by toolchains.",
                human_bytes(summary.rebuildable_bytes)
            ));
        }
        if summary.review_item_count > 0 {
            summary.recommended_next_steps.push(format!(
                "Manually inspect {} review item(s), especially risky large files and dependency trees.",
                summary.review_item_count
            ));
        }
        if summary.cold_file_count > 0 {
            summary.recommended_next_steps.push(format!(
                "Review {} cold file(s) last accessed over {COLD_AFTER_DAYS} days ago before reclaiming any personal data.",
                summary.cold_file_count
            ));
        }
        summary
            .recommended_next_steps
            .push("Use Reveal and copied `du` commands first, then stage approved targets for Finder Trash cleanup.".to_owned());
    }
    if truncated {
        summary.recommended_next_steps.push(
            "Scan was partial; narrow the root or rerun when the machine is idle before making cleanup decisions."
                .to_owned(),
        );
    }

    summary
}

fn largest_item_by(
    items: &[StorageHygieneItem],
    predicate: impl Fn(&StorageHygieneItem) -> bool,
) -> Option<&StorageHygieneItem> {
    items
        .iter()
        .filter(|item| predicate(item))
        .max_by(|left, right| {
            left.size_bytes
                .cmp(&right.size_bytes)
                .then_with(|| right.path.cmp(&left.path))
        })
}

fn push_investigation_finding(
    findings: &mut Vec<StorageInvestigationFinding>,
    seen_paths: &mut BTreeSet<String>,
    id_prefix: &str,
    title: &str,
    detail: &str,
    item: Option<&StorageHygieneItem>,
) {
    let Some(item) = item else {
        return;
    };
    if !seen_paths.insert(item.path.clone()) {
        return;
    }
    findings.push(StorageInvestigationFinding {
        id: format!("{id_prefix}|{}", item.id),
        title: title.to_owned(),
        detail: detail.to_owned(),
        path: item.path.clone(),
        storage_role: item.storage_role.clone(),
        cleanup_tier: item.cleanup_tier.clone(),
        safety: item.safety.clone(),
        size_bytes: item.size_bytes,
        confidence_score: cleanup_item_confidence(item),
        evidence: item.evidence.iter().take(5).cloned().collect(),
        recommended_action: item.next_step.clone(),
    });
}

fn summarize_duplicate_groups(items: &[StorageHygieneItem]) -> Vec<StorageDuplicateGroup> {
    let mut cheap_groups = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        let path = Path::new(&item.path);
        if !path.is_file() || item.size_bytes < MIN_ITEM_BYTES {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("no-extension")
            .to_ascii_lowercase();
        cheap_groups
            .entry(format!("{}|{extension}", item.size_bytes))
            .or_default()
            .push(item);
    }

    let mut groups = Vec::new();
    for (cheap_key, candidates) in cheap_groups {
        if candidates.len() < 2 {
            continue;
        }

        let can_hash = candidates
            .iter()
            .all(|item| item.size_bytes <= DUPLICATE_FULL_HASH_MAX_BYTES);
        if can_hash {
            let mut by_hash = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
            for item in candidates {
                if let Some(hash) = file_content_hash(Path::new(&item.path)) {
                    by_hash.entry(hash).or_default().push(item);
                }
            }
            for (hash, hashed_items) in by_hash {
                if hashed_items.len() >= 2 {
                    groups.push(duplicate_group_from_items(
                        format!("hash|{hash}"),
                        hash,
                        true,
                        96,
                        hashed_items,
                    ));
                }
            }
        } else {
            groups.push(duplicate_group_from_items(
                format!("candidate|{cheap_key}"),
                cheap_key,
                false,
                48,
                candidates,
            ));
        }
    }

    groups.sort_by(|left, right| {
        right
            .reclaimable_bytes
            .cmp(&left.reclaimable_bytes)
            .then_with(|| right.total_bytes.cmp(&left.total_bytes))
            .then_with(|| left.candidate_key.cmp(&right.candidate_key))
    });
    groups.truncate(DUPLICATE_GROUP_LIMIT);
    groups
}

fn duplicate_group_from_items(
    id: String,
    candidate_key: String,
    confirmed: bool,
    confidence_score: u8,
    mut items: Vec<&StorageHygieneItem>,
) -> StorageDuplicateGroup {
    items.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    let total_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    let keep_bytes = items.iter().map(|item| item.size_bytes).max().unwrap_or(0);
    let paths = items
        .iter()
        .take(8)
        .map(|item| StorageDuplicateItem {
            path: item.path.clone(),
            display_name: item.display_name.clone(),
            size_bytes: item.size_bytes,
            modified_millis: item.modified_millis,
            cleanup_tier: item.cleanup_tier.clone(),
            safety: item.safety.clone(),
        })
        .collect::<Vec<_>>();

    StorageDuplicateGroup {
        id,
        candidate_key,
        confirmed,
        confidence_score,
        file_count: items.len(),
        total_bytes,
        reclaimable_bytes: total_bytes.saturating_sub(keep_bytes),
        paths,
        recommendation: if confirmed {
            "Confirmed byte-identical files. Keep the canonical copy, Quick Look samples, then stage only deliberate duplicates.".to_owned()
        } else {
            "Potential duplicate group by size/type only. Quick Look and hash externally before deleting.".to_owned()
        },
        caveat: if confirmed {
            "Full content hashing was performed only after cheap size/type grouping identified candidates.".to_owned()
        } else {
            format!(
                "Files exceed the {} per-file hash cap or could not be hashed in this bounded scan.",
                human_bytes(DUPLICATE_FULL_HASH_MAX_BYTES)
            )
        },
    }
}

fn file_content_hash(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hash = 0xcbf29ce484222325u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        for byte in &buffer[..count] {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Some(format!("{hash:016x}"))
}

fn summarize_app_footprints(items: &[StorageHygieneItem]) -> Vec<StorageAppFootprint> {
    let mut grouped = BTreeMap::<String, AppFootprintAccumulator>::new();
    for item in items {
        let Some(identity) = app_component_identity(item) else {
            continue;
        };
        let entry = grouped
            .entry(identity.key.clone())
            .or_insert_with(|| AppFootprintAccumulator::new(identity));
        entry.add_item(item);
    }

    let mut footprints = grouped
        .into_values()
        .map(AppFootprintAccumulator::finish)
        .collect::<Vec<_>>();
    footprints.sort_by(|left, right| {
        right
            .total_bytes
            .cmp(&left.total_bytes)
            .then_with(|| left.app_name.cmp(&right.app_name))
    });
    footprints.truncate(APP_FOOTPRINT_LIMIT);
    footprints
}

#[derive(Clone, Debug)]
struct AppComponentIdentity {
    key: String,
    app_name: String,
    bundle_identifier: Option<String>,
}

#[derive(Clone, Debug)]
struct AppFootprintAccumulator {
    key: String,
    app_name: String,
    bundle_identifier: Option<String>,
    total_bytes: u64,
    safety: String,
    cleanup_tier: String,
    components: Vec<StorageAppFootprintComponent>,
}

impl AppFootprintAccumulator {
    fn new(identity: AppComponentIdentity) -> Self {
        Self {
            key: identity.key,
            app_name: identity.app_name,
            bundle_identifier: identity.bundle_identifier,
            total_bytes: 0,
            safety: "safe".to_owned(),
            cleanup_tier: "safe".to_owned(),
            components: Vec::new(),
        }
    }

    fn add_item(&mut self, item: &StorageHygieneItem) {
        if self.bundle_identifier.is_none() {
            self.bundle_identifier = app_bundle_identifier(Path::new(&item.path));
        }
        if item.safety != "safe" {
            self.safety = "review".to_owned();
        }
        if cleanup_tier_rank(&item.cleanup_tier) > cleanup_tier_rank(&self.cleanup_tier) {
            self.cleanup_tier = item.cleanup_tier.clone();
        }
        self.total_bytes = self.total_bytes.saturating_add(item.size_bytes);
        let component = app_component_label(&item.kind);
        self.components.push(StorageAppFootprintComponent {
            path: item.path.clone(),
            component,
            size_bytes: item.size_bytes,
            cleanup_tier: item.cleanup_tier.clone(),
            safety: item.safety.clone(),
        });
    }

    fn finish(mut self) -> StorageAppFootprint {
        self.components.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        self.components.truncate(8);
        let component_count = self.components.len();
        let has_bundle = self
            .components
            .iter()
            .any(|component| component.component == "App bundle");
        let has_state = self.components.iter().any(|component| {
            matches!(
                component.component.as_str(),
                "Application Support" | "Container" | "Launch item" | "Preferences" | "Receipt"
            )
        });
        StorageAppFootprint {
            id: self.key,
            app_name: self.app_name,
            bundle_identifier: self.bundle_identifier,
            total_bytes: self.total_bytes,
            component_count,
            cleanup_tier: self.cleanup_tier,
            safety: self.safety,
            confidence_score: if has_bundle && has_state {
                82
            } else if has_bundle {
                70
            } else {
                58
            },
            components: self.components,
            recommendation: "Treat this as an uninstall footprint: quit the app, review support data, preferences, receipts, containers, and launch items, then move selected components to Trash.".to_owned(),
        }
    }
}

fn app_component_identity(item: &StorageHygieneItem) -> Option<AppComponentIdentity> {
    let path = Path::new(&item.path);
    match item.kind.as_str() {
        "macos-app-bundle" => {
            let app_name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or(&item.display_name)
                .to_owned();
            let bundle_identifier = app_bundle_identifier(path);
            let key = bundle_identifier
                .as_deref()
                .map(normalize_app_key)
                .unwrap_or_else(|| normalize_app_key(&app_name));
            Some(AppComponentIdentity {
                key,
                app_name,
                bundle_identifier,
            })
        }
        "app-cache" | "app-support-data" | "app-container" | "app-launch-item"
        | "app-preferences" | "app-receipt" => {
            let component_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&item.display_name)
                .to_owned();
            let bundle_identifier = app_component_bundle_identifier(path, &component_name);
            Some(AppComponentIdentity {
                key: bundle_identifier
                    .as_deref()
                    .map(normalize_app_key)
                    .unwrap_or_else(|| normalize_app_key(&component_name)),
                app_name: bundle_identifier
                    .as_deref()
                    .map(app_name_from_bundle_identifier)
                    .unwrap_or(component_name),
                bundle_identifier,
            })
        }
        _ => None,
    }
}

fn app_component_bundle_identifier(path: &Path, component_name: &str) -> Option<String> {
    let path_lower = path.display().to_string().to_ascii_lowercase();
    if is_app_preferences_path(&path_lower, component_name)
        || is_app_receipt_path(&path_lower, component_name)
        || is_app_container_path(&path_lower)
        || is_app_cache_path(&path_lower)
        || is_app_support_path(&path_lower)
        || is_launch_item_path(&path_lower)
    {
        return component_filename_bundle_identifier(component_name);
    }
    None
}

fn component_filename_bundle_identifier(component_name: &str) -> Option<String> {
    let mut value = component_name
        .trim_end_matches(".lockfile")
        .trim_end_matches(".plist")
        .trim_end_matches(".bom")
        .trim_end_matches(".pkg")
        .trim()
        .to_owned();
    if value.ends_with(".savedState") {
        value.truncate(value.len().saturating_sub(".savedState".len()));
    }
    if value.matches('.').count() >= 2 {
        Some(value)
    } else {
        None
    }
}

fn app_name_from_bundle_identifier(bundle_identifier: &str) -> String {
    bundle_identifier
        .rsplit('.')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(bundle_identifier)
        .to_owned()
}

fn app_bundle_identifier(app_path: &Path) -> Option<String> {
    let info = app_path.join("Contents").join("Info.plist");
    let content = fs::read_to_string(info).ok()?;
    let marker = "<key>CFBundleIdentifier</key>";
    let marker_index = content.find(marker)?;
    let rest = &content[marker_index + marker.len()..];
    let start_marker = "<string>";
    let end_marker = "</string>";
    let start = rest.find(start_marker)? + start_marker.len();
    let end = rest[start..].find(end_marker)? + start;
    let value = rest[start..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn app_component_label(kind: &str) -> String {
    match kind {
        "macos-app-bundle" => "App bundle",
        "app-cache" => "Caches",
        "app-support-data" => "Application Support",
        "app-container" => "Container",
        "app-launch-item" => "Launch item",
        "app-preferences" => "Preferences",
        "app-receipt" => "Receipt",
        _ => "Related data",
    }
    .to_owned()
}

fn normalize_app_key(value: &str) -> String {
    value
        .trim_end_matches(".app")
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '-')
        .collect::<String>()
}

fn summarize_system_data_buckets(items: &[StorageHygieneItem]) -> Vec<StorageSystemDataBucket> {
    let definitions = [
        (
            "local-snapshots",
            "Local snapshots",
            "snapshots",
            &["local-snapshot"][..],
            "Time Machine and APFS snapshots can make System Data look larger than normal free space suggests.",
            "Inspect with `tmutil listlocalsnapshots /`; prefer macOS controls over deleting files manually.",
            true,
        ),
        (
            "simulator-runtimes",
            "Simulator runtimes and device support",
            "developer",
            &["xcode-simulator-runtime"][..],
            "Xcode simulator/device support storage is often reported under System Data.",
            "Remove unused simulators/runtimes from Xcode or Simulator tooling after exports are no longer needed.",
            false,
        ),
        (
            "ios-backups",
            "iOS and iPadOS backups",
            "backups",
            &["ios-backup"][..],
            "Local device backups can grow silently and are commonly mistaken for opaque System Data.",
            "Use Finder device management and keep at least one known-good backup before cleanup.",
            false,
        ),
        (
            "mail-attachments",
            "Mail attachments",
            "attachments",
            &["mail-attachments"][..],
            "Mail attachment caches can be large and may include personal or business records.",
            "Review in Mail or export important attachments before cleanup.",
            true,
        ),
        (
            "message-attachments",
            "Messages attachments",
            "attachments",
            &["message-attachments"][..],
            "Messages attachments can accumulate as photos, videos, and documents under System Data.",
            "Review conversations or attachment folders before deleting.",
            true,
        ),
        (
            "app-caches",
            "Application caches",
            "caches",
            &[
                "app-cache",
                "tool-cache",
                "xcode-module-cache",
                "npm-cache",
                "pnpm-store",
                "yarn-cache",
            ][..],
            "Caches are usually reclaimable but can slow the next app launch, build, or install.",
            "Quit owning apps/tools, then stage only high-confidence cache targets.",
            false,
        ),
    ];

    let mut buckets = definitions
        .iter()
        .map(
            |(
                id,
                title,
                category,
                kinds,
                explanation,
                recommended_action,
                requires_full_disk_access,
            )| {
                let matching = items
                    .iter()
                    .filter(|item| kinds.iter().any(|kind| item.kind == *kind))
                    .collect::<Vec<_>>();
                let size_bytes = matching
                    .iter()
                    .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
                let cleanup_tier = highest_cleanup_tier(matching.iter().copied());
                let safety = if matching.iter().any(|item| item.safety != "safe") {
                    "review"
                } else {
                    "safe"
                }
                .to_owned();
                let paths = matching
                    .iter()
                    .take(6)
                    .map(|item| item.path.clone())
                    .collect::<Vec<_>>();
                StorageSystemDataBucket {
                    id: (*id).to_owned(),
                    title: (*title).to_owned(),
                    category: (*category).to_owned(),
                    size_bytes,
                    item_count: matching.len(),
                    cleanup_tier,
                    safety,
                    explanation: (*explanation).to_owned(),
                    recommended_action: (*recommended_action).to_owned(),
                    paths,
                    requires_full_disk_access: *requires_full_disk_access,
                }
            },
        )
        .collect::<Vec<_>>();
    buckets.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.title.cmp(&right.title))
    });
    buckets
}

fn highest_cleanup_tier<'a>(items: impl Iterator<Item = &'a StorageHygieneItem>) -> String {
    let mut highest = "safe".to_owned();
    for item in items {
        if cleanup_tier_rank(&item.cleanup_tier) > cleanup_tier_rank(&highest) {
            highest = item.cleanup_tier.clone();
        }
    }
    highest
}

fn summarize_repo_footprints(items: &[StorageHygieneItem]) -> Vec<StorageRepoFootprint> {
    let mut grouped = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        if let Some(repo_root) = item.attribution.repo_root.as_deref() {
            grouped.entry(repo_root.to_owned()).or_default().push(item);
        }
    }

    let mut footprints: Vec<_> = grouped
        .into_iter()
        .map(|(repo_root, items)| repo_footprint_for_items(repo_root, items))
        .collect();
    footprints.sort_by(|left, right| {
        right
            .current_size_bytes
            .cmp(&left.current_size_bytes)
            .then_with(|| left.repo_name.cmp(&right.repo_name))
    });
    footprints
}

fn summarize_repository_inventory(
    repositories_by_root: BTreeMap<String, String>,
    not_seen_roots: &BTreeSet<String>,
    cache_states: &BTreeMap<String, RepositoryInventoryCacheState>,
    started: Instant,
    mode: StorageScanMode,
    metrics: &mut StorageScanMetrics,
) -> Vec<StorageRepositoryInventoryItem> {
    let mut repositories: Vec<_> = repositories_by_root
        .into_iter()
        .map(|(repo_root, discovered_root)| {
            let repo_path = Path::new(&repo_root);
            let git_started = Instant::now();
            let git = repository_git_intelligence(repo_path, started, mode);
            metrics.git_millis = metrics
                .git_millis
                .saturating_add(git_started.elapsed().as_millis() as u64);
            let quality = repository_quality(repo_path);
            let tracked_started = Instant::now();
            let tracked_agent_paths =
                git_tracked_paths(repo_path, &agent_contract_candidate_paths());
            metrics.git_millis = metrics
                .git_millis
                .saturating_add(tracked_started.elapsed().as_millis() as u64);
            let mut guidance = agent_guidance_audit(repo_path, &quality, &tracked_agent_paths);
            let contract_coverage =
                agent_contract_coverage_audit(repo_path, &quality, &guidance, &tracked_agent_paths);
            guidance.issues.extend(contract_coverage.issues.clone());
            guidance.status = guidance_status(&guidance.issues);
            let repo_name = repo_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("repository")
                .to_owned();
            let not_seen_in_latest_scan = not_seen_roots.contains(&repo_root);
            let cache_state = cache_states.get(&repo_root).cloned().unwrap_or_else(|| {
                RepositoryInventoryCacheState {
                    status: "uncached".to_owned(),
                    fingerprint: repository_inventory_fingerprint(repo_path),
                    fingerprint_changed: false,
                    last_seen_millis: None,
                    last_scan_millis: None,
                }
            });

            StorageRepositoryInventoryItem {
                id: repo_root.clone(),
                repo_root,
                repo_name,
                inventory_cache_status: cache_state.status,
                inventory_fingerprint: cache_state.fingerprint,
                inventory_fingerprint_changed: cache_state.fingerprint_changed,
                inventory_last_seen_millis: cache_state.last_seen_millis,
                inventory_last_scan_millis: cache_state.last_scan_millis,
                git_branch: git.branch,
                git_head: git.head,
                git_ref: git.reference,
                git_detached_head: git.detached_head,
                git_remote_origin_url: git.remote_origin_url,
                git_remote_key: git.remote_key,
                git_remote_host: git.remote_host,
                git_remote_owner: git.remote_owner,
                git_remote_name: git.remote_name,
                git_dirty_status: git.dirty_status,
                git_dirty_file_count: git.dirty_file_count,
                git_dirty_truncated: git.dirty_truncated,
                not_seen_in_latest_scan,
                clone_group_count: 1,
                clone_group_roots: Vec::new(),
                discovered_root,
                has_agents_md: quality.has_agents_md,
                has_claude_md: quality.has_claude_md,
                claude_md_bytes: quality.claude_md_bytes,
                claude_md_delegation_max_bytes: CLAUDE_MD_DELEGATION_MAX_BYTES,
                claude_md_delegates_to_agents_md: quality.claude_md_delegates_to_agents_md,
                agent_readiness_score: contract_coverage.score,
                agent_readiness_status: contract_coverage.status,
                agent_contract_missing_count: contract_coverage.missing_count,
                agent_contract_coverage: contract_coverage.coverage,
                agent_guidance_status: guidance.status,
                agent_guidance_issue_count: guidance.issues.len() as u64,
                agent_guidance_issues: guidance.issues,
            }
        })
        .collect();
    attach_clone_groups(&mut repositories);
    repositories.sort_by(|left, right| {
        left.repo_name
            .to_lowercase()
            .cmp(&right.repo_name.to_lowercase())
            .then_with(|| left.repo_root.cmp(&right.repo_root))
    });
    repositories
}

#[derive(Clone, Debug, Default)]
struct RepositoryGitIntelligence {
    branch: Option<String>,
    head: Option<String>,
    reference: Option<String>,
    detached_head: bool,
    remote_origin_url: Option<String>,
    remote_key: Option<String>,
    remote_host: Option<String>,
    remote_owner: Option<String>,
    remote_name: Option<String>,
    dirty_status: String,
    dirty_file_count: Option<u64>,
    dirty_truncated: bool,
}

fn repository_git_intelligence(
    repo_path: &Path,
    started: Instant,
    mode: StorageScanMode,
) -> RepositoryGitIntelligence {
    let head = read_git_head(repo_path);
    let remote_origin_url = read_git_config_value(repo_path, "remote \"origin\"", "url")
        .and_then(|value| redact_git_remote_url(&value));
    let remote_key = remote_origin_url
        .as_deref()
        .and_then(normalize_git_remote_key);
    let remote_parts = remote_key.as_deref().map(split_git_remote_key);
    let dirty = if mode.collect_git_status() {
        read_git_dirty_status(repo_path, started)
    } else {
        GitDirtyStatus {
            status: "not_checked_lazy".to_owned(),
            file_count: None,
            truncated: false,
        }
    };

    RepositoryGitIntelligence {
        branch: head.branch,
        head: head.short_head,
        reference: head.reference,
        detached_head: head.detached,
        remote_origin_url,
        remote_key,
        remote_host: remote_parts.as_ref().and_then(|parts| parts.host.clone()),
        remote_owner: remote_parts.as_ref().and_then(|parts| parts.owner.clone()),
        remote_name: remote_parts.and_then(|parts| parts.name),
        dirty_status: dirty.status,
        dirty_file_count: dirty.file_count,
        dirty_truncated: dirty.truncated,
    }
}

fn attach_clone_groups(repositories: &mut [StorageRepositoryInventoryItem]) {
    let mut roots_by_remote = BTreeMap::<String, Vec<String>>::new();
    for repository in repositories.iter() {
        if let Some(remote_key) = repository.git_remote_key.as_deref() {
            roots_by_remote
                .entry(remote_key.to_owned())
                .or_default()
                .push(repository.repo_root.clone());
        }
    }

    for repository in repositories.iter_mut() {
        let Some(remote_key) = repository.git_remote_key.as_deref() else {
            continue;
        };
        let Some(group_roots) = roots_by_remote.get(remote_key) else {
            continue;
        };
        repository.clone_group_count = group_roots.len() as u64;
        if group_roots.len() > 1 {
            repository.clone_group_roots = group_roots.clone();
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RepositoryQuality {
    has_agents_md: bool,
    has_claude_md: bool,
    claude_md_bytes: Option<u64>,
    claude_md_delegates_to_agents_md: bool,
}

#[derive(Clone, Debug, Default)]
struct AgentGuidanceAudit {
    status: String,
    issues: Vec<StorageAgentGuidanceIssue>,
}

#[derive(Clone, Copy, Debug)]
struct AgentContractDefinition {
    id: &'static str,
    label: &'static str,
    path: &'static str,
    kind: &'static str,
    weight: u64,
    missing_severity: &'static str,
    requires_schema: bool,
    requires_review: bool,
    detail: &'static str,
}

#[derive(Clone, Debug, Default)]
struct AgentContractCoverageAudit {
    score: u8,
    status: String,
    missing_count: u64,
    coverage: Vec<StorageAgentContractCoverage>,
    issues: Vec<StorageAgentGuidanceIssue>,
}

fn repository_quality(repo_root: &Path) -> RepositoryQuality {
    let has_agents_md = repo_root.join("AGENTS.md").is_file();
    let claude_md = repo_root.join("CLAUDE.md");
    let has_claude_md = claude_md.is_file();
    let claude_md_bytes = fs::metadata(&claude_md).ok().map(|metadata| metadata.len());
    let claude_md_delegates_to_agents_md = if has_claude_md
        && claude_md_bytes.is_some_and(|bytes| bytes <= CLAUDE_MD_DELEGATION_MAX_BYTES)
    {
        fs::read_to_string(&claude_md)
            .map(|content| {
                content
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim() == "@AGENTS.md")
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    } else {
        false
    };

    RepositoryQuality {
        has_agents_md,
        has_claude_md,
        claude_md_bytes,
        claude_md_delegates_to_agents_md,
    }
}

fn agent_guidance_audit(
    repo_root: &Path,
    quality: &RepositoryQuality,
    tracked_agent_paths: &BTreeSet<String>,
) -> AgentGuidanceAudit {
    let mut issues = Vec::new();
    let agents_path = repo_root.join("AGENTS.md");

    if !quality.has_agents_md {
        push_guidance_issue(
            &mut issues,
            "agents.root.missing",
            "error",
            "Missing root AGENTS.md",
            "Every repository should have a tracked root AGENTS.md as the canonical agent contract.",
            "AGENTS.md",
        );
    } else {
        if !tracked_agent_paths.contains("AGENTS.md") {
            push_guidance_issue(
                &mut issues,
                "agents.root.untracked",
                "error",
                "Root AGENTS.md is not tracked",
                "A referenced agent contract must be committed, not only present in the local worktree.",
                "AGENTS.md",
            );
        }
        if let Ok(content) = fs::read_to_string(&agents_path) {
            audit_agents_markdown(&mut issues, repo_root, "AGENTS.md", &content);
        }
    }

    if quality.has_claude_md && !quality.claude_md_delegates_to_agents_md {
        let detail = if quality
            .claude_md_bytes
            .is_some_and(|bytes| bytes > CLAUDE_MD_DELEGATION_MAX_BYTES)
        {
            "CLAUDE.md is too large to be treated as a delegating adapter."
        } else {
            "CLAUDE.md should delegate to AGENTS.md with @AGENTS.md as its first non-empty line."
        };
        push_guidance_issue(
            &mut issues,
            "agents.adapter.claude_not_delegated",
            "warning",
            "CLAUDE.md does not delegate cleanly",
            detail,
            "CLAUDE.md",
        );
    }

    audit_canonical_agent_links(&mut issues, repo_root, "README.md");
    audit_canonical_agent_links(&mut issues, repo_root, "CONTRIBUTING.md");

    let status = guidance_status(&issues);
    AgentGuidanceAudit { status, issues }
}

fn agent_contract_definitions() -> &'static [AgentContractDefinition] {
    &[
        AgentContractDefinition {
            id: "agents_md",
            label: "AGENTS.md",
            path: "AGENTS.md",
            kind: "human-contract",
            weight: 22,
            missing_severity: "error",
            requires_schema: false,
            requires_review: false,
            detail: "Human-readable operating contract: precedence, workflow, git discipline, approvals, and completion rules.",
        },
        AgentContractDefinition {
            id: "manifest",
            label: "Manifest",
            path: ".agents/manifest.yaml",
            kind: "machine-contract",
            weight: 6,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Root machine contract: expected files, schema ids, generator/check commands, cross-file integrity, and freshness policy.",
        },
        AgentContractDefinition {
            id: "tasks",
            label: "Tasks",
            path: ".agents/tasks.yaml",
            kind: "reviewed-contract",
            weight: 12,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Decision layer: task classification, required reads, likely paths, strategy, validation routing, and completion reporting.",
        },
        AgentContractDefinition {
            id: "repo_map",
            label: "Repo map",
            path: ".agents/repo-map.yaml",
            kind: "machine-contract",
            weight: 9,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Machine-readable topology: roots, packages, services, entrypoints, generated folders, and ignored roots.",
        },
        AgentContractDefinition {
            id: "contracts",
            label: "Contracts",
            path: ".agents/contracts.yaml",
            kind: "reviewed-contract",
            weight: 14,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Invariant layer: API, auth, data, release, storage, process-control, performance, UI, and security contracts agents must not break.",
        },
        AgentContractDefinition {
            id: "commands",
            label: "Commands",
            path: ".agents/commands.yaml",
            kind: "machine-contract",
            weight: 10,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Exact command registry with cwd, cost, mutation, approval, breadth, and expected runtime metadata.",
        },
        AgentContractDefinition {
            id: "validation",
            label: "Validation",
            path: ".agents/validation.yaml",
            kind: "reviewed-contract",
            weight: 12,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Touched-path to validation mapping so agents know which checks prove a change safe.",
        },
        AgentContractDefinition {
            id: "boundaries",
            label: "Boundaries",
            path: ".agents/boundaries.yaml",
            kind: "reviewed-contract",
            weight: 7,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Architecture rules agents and tools can check: layers, forbidden imports, generated-code ownership.",
        },
        AgentContractDefinition {
            id: "risks",
            label: "Risks",
            path: ".agents/risks.yaml",
            kind: "reviewed-contract",
            weight: 6,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "High-risk surfaces: auth, billing, permissions, migrations, secrets, deploy, tenants, webhooks.",
        },
        AgentContractDefinition {
            id: "references",
            label: "References",
            path: ".agents/references.yaml",
            kind: "machine-contract",
            weight: 2,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Pointers to deeper context without loading it by default: architecture, runbooks, standards, API docs.",
        },
    ]
}

fn agent_contract_candidate_paths() -> Vec<&'static str> {
    let mut paths = Vec::with_capacity(agent_contract_definitions().len());
    for definition in agent_contract_definitions() {
        paths.push(definition.path);
    }
    paths
}

fn agent_contract_coverage_audit(
    repo_root: &Path,
    quality: &RepositoryQuality,
    guidance: &AgentGuidanceAudit,
    tracked_agent_paths: &BTreeSet<String>,
) -> AgentContractCoverageAudit {
    let mut audit = AgentContractCoverageAudit::default();
    let mut earned_score = 0_u64;

    for definition in agent_contract_definitions() {
        let evaluated = evaluate_agent_contract(
            repo_root,
            quality,
            guidance,
            definition,
            tracked_agent_paths,
        );
        earned_score = earned_score.saturating_add(evaluated.earned_weight);
        if !evaluated.present {
            audit.missing_count = audit.missing_count.saturating_add(1);
        }
        if let Some(issue) = agent_contract_issue(definition, &evaluated) {
            audit.issues.push(issue);
        }
        audit.coverage.push(evaluated);
    }

    audit.score = earned_score.min(AGENT_READINESS_MAX_SCORE) as u8;
    audit.status = agent_readiness_status(audit.score, guidance, &audit.coverage);
    audit
}

fn evaluate_agent_contract(
    repo_root: &Path,
    quality: &RepositoryQuality,
    guidance: &AgentGuidanceAudit,
    definition: &AgentContractDefinition,
    tracked_agent_paths: &BTreeSet<String>,
) -> StorageAgentContractCoverage {
    let path = repo_root.join(definition.path);
    let present = path.is_file();
    let tracked = present && tracked_agent_paths.contains(definition.path);
    let mut status = "missing".to_owned();
    let mut severity = definition.missing_severity.to_owned();
    let mut detail = format!("Missing {}. {}", definition.path, definition.detail);
    let mut earned_weight = 0_u64;
    let mut schema_version = None;
    let mut generated = false;
    let mut reviewed = false;

    if present {
        let content = fs::read_to_string(&path).unwrap_or_default();
        schema_version =
            yaml_scalar(&content, "schema_version").or_else(|| yaml_scalar(&content, "version"));
        let contract_shape_issues = if definition.requires_schema {
            yaml_contract_shape_issues(repo_root, definition, &content)
        } else {
            Vec::new()
        };
        generated = yaml_boolish(&content, "generated")
            || yaml_scalar(&content, "generated_by").is_some()
            || content.contains("source_files:");
        reviewed = yaml_boolish(&content, "reviewed")
            || has_completed_human_review(&content)
            || yaml_scalar(&content, "last_reviewed").is_some();

        if !tracked {
            status = "untracked".to_owned();
            severity = if definition.path == "AGENTS.md" {
                "error".to_owned()
            } else {
                "warning".to_owned()
            };
            detail = format!(
                "{} exists but is not tracked by git. Agent contracts must be committed to be reliable.",
                definition.path
            );
            earned_weight = definition.weight / 3;
        } else if definition.path == "AGENTS.md" {
            let has_errors = guidance
                .issues
                .iter()
                .any(|issue| issue.severity == "error" && issue.path == "AGENTS.md");
            let has_warnings = guidance
                .issues
                .iter()
                .any(|issue| issue.severity == "warning" && issue.path == "AGENTS.md");
            if !quality.has_agents_md || has_errors {
                status = "error".to_owned();
                severity = "error".to_owned();
                detail = "AGENTS.md exists but has blocking contract issues.".to_owned();
                earned_weight = definition.weight / 3;
            } else if has_warnings {
                status = "partial".to_owned();
                severity = "warning".to_owned();
                detail = "AGENTS.md is tracked but has shape, reference, or workflow warnings."
                    .to_owned();
                earned_weight = definition.weight.saturating_mul(3) / 4;
            } else {
                status = "ok".to_owned();
                severity = "ok".to_owned();
                detail =
                    "AGENTS.md is present, tracked, and passes current portable checks.".to_owned();
                earned_weight = definition.weight;
            }
        } else {
            let schema_missing = definition.requires_schema && schema_version.is_none();
            let review_missing = definition.requires_review && !reviewed;
            let shape_invalid = !contract_shape_issues.is_empty();
            if schema_missing || review_missing || shape_invalid {
                status = "partial".to_owned();
                severity = "warning".to_owned();
                let mut gaps = Vec::new();
                if schema_missing {
                    gaps.push("schema_version/version".to_owned());
                }
                if review_missing {
                    gaps.push("reviewed_by/reviewed_at".to_owned());
                }
                if shape_invalid {
                    gaps.push(format!(
                        "shape checks ({})",
                        contract_shape_issues.join("; ")
                    ));
                }
                detail = format!(
                    "{} is present and tracked but has contract gaps: {}.",
                    definition.path,
                    gaps.join("; ")
                );
                earned_weight = definition.weight.saturating_mul(2) / 3;
            } else {
                status = "ok".to_owned();
                severity = "ok".to_owned();
                detail = format!(
                    "{} is present, tracked, and machine-checkable.",
                    definition.path
                );
                earned_weight = definition.weight;
            }
        }
    }

    let coverage_percent = if definition.weight == 0 {
        0
    } else {
        (earned_weight.saturating_mul(100) / definition.weight).min(100) as u8
    };

    StorageAgentContractCoverage {
        id: definition.id.to_owned(),
        label: definition.label.to_owned(),
        path: definition.path.to_owned(),
        kind: definition.kind.to_owned(),
        status,
        severity,
        detail,
        weight: definition.weight,
        earned_weight,
        coverage_percent,
        present,
        tracked,
        schema_version,
        generated,
        reviewed,
    }
}

fn agent_contract_issue(
    definition: &AgentContractDefinition,
    coverage: &StorageAgentContractCoverage,
) -> Option<StorageAgentGuidanceIssue> {
    if coverage.status == "ok" || definition.path == "AGENTS.md" {
        return None;
    }

    let id = format!("agents.contract.{}.{}", coverage.status, definition.id);
    Some(StorageAgentGuidanceIssue {
        id,
        severity: coverage.severity.clone(),
        title: match coverage.status.as_str() {
            "missing" => format!("Missing {}", definition.label),
            "untracked" => format!("{} is not tracked", definition.label),
            "partial" => format!("{} coverage is partial", definition.label),
            _ => format!("{} needs review", definition.label),
        },
        detail: coverage.detail.clone(),
        path: definition.path.to_owned(),
    })
}

fn agent_readiness_status(
    score: u8,
    guidance: &AgentGuidanceAudit,
    coverage: &[StorageAgentContractCoverage],
) -> String {
    if guidance
        .issues
        .iter()
        .any(|issue| issue.severity == "error")
        || coverage
            .iter()
            .any(|item| item.path == "AGENTS.md" && item.severity == "error")
    {
        return "blocked".to_owned();
    }
    if score >= 90 {
        "ready".to_owned()
    } else if score >= 60 {
        "partial".to_owned()
    } else {
        "weak".to_owned()
    }
}

fn yaml_scalar(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix(&prefix)?.trim();
        if value.is_empty() || value == "|" || value == ">" {
            return None;
        }
        Some(value.trim_matches('"').trim_matches('\'').to_owned())
    })
}

fn yaml_boolish(content: &str, key: &str) -> bool {
    yaml_scalar(content, key)
        .map(|value| {
            let normalized = value.to_lowercase();
            matches!(normalized.as_str(), "true" | "yes" | "1")
        })
        .unwrap_or(false)
}

fn has_completed_human_review(content: &str) -> bool {
    let reviewed_by = yaml_scalar(content, "reviewed_by")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let reviewed_at = yaml_scalar(content, "reviewed_at")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    !reviewed_by.is_empty()
        && reviewed_by != "pending-human-review"
        && reviewed_by != "pending_human_review"
        && reviewed_by != "pending"
        && !reviewed_at.is_empty()
        && reviewed_at != "pending-human-review"
        && reviewed_at != "pending_human_review"
        && reviewed_at != "pending"
}

fn yaml_contract_shape_issues(
    repo_root: &Path,
    definition: &AgentContractDefinition,
    content: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    if content.trim().is_empty() {
        issues.push("empty file".to_owned());
        return issues;
    }
    if content.lines().any(|line| line.starts_with('\t')) {
        issues.push("tab indentation".to_owned());
    }
    for key in yaml_contract_required_keys(definition.id) {
        if !yaml_has_top_level_key(content, key) {
            issues.push(format!("missing top-level `{key}`"));
        }
    }
    for path in local_markdown_paths(content) {
        if !repo_root.join(&path).exists() {
            issues.push(format!("missing local reference `{path}`"));
        }
        if issues.len() >= 6 {
            break;
        }
    }
    unique_limited(issues, 6)
}

fn yaml_contract_required_keys(contract_id: &str) -> &'static [&'static str] {
    match contract_id {
        "manifest" => &["schema_version", "contracts", "integrity"],
        "tasks" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "tasks",
        ],
        "repo_map" => &["schema_version", "generated_by", "source_files", "roots"],
        "contracts" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "contracts",
        ],
        "commands" => &["schema_version", "generated_by", "source_files", "commands"],
        "validation" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "rules",
        ],
        "boundaries" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "layers",
            "rules",
        ],
        "risks" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "surfaces",
        ],
        "references" => &[
            "schema_version",
            "generated_by",
            "source_files",
            "references",
        ],
        _ => &["schema_version"],
    }
}

fn yaml_has_top_level_key(content: &str, key: &str) -> bool {
    let prefix = format!("{key}:");
    content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && line.trim_start() == line
            && trimmed.starts_with(&prefix)
    })
}

fn audit_agents_markdown(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
    content: &str,
) {
    let line_count = content.lines().count();
    if line_count > AGENTS_MD_MAX_LINES {
        push_guidance_issue(
            issues,
            "agents.root.too_large",
            "warning",
            "AGENTS.md is too large",
            &format!(
                "Root AGENTS.md has {line_count} lines; keep it under {AGENTS_MD_MAX_LINES} lines or move generated/detail content behind references."
            ),
            relative_path,
        );
    }

    audit_required_sections(issues, relative_path, content);
    audit_duplicate_headings(issues, relative_path, content);
    audit_banned_agent_phrases(issues, relative_path, content);
    audit_broad_git_examples(issues, relative_path, content);
    audit_reference_paths(issues, repo_root, relative_path, content);
}

fn audit_required_sections(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const REQUIRED_SECTIONS: &[&str] = &[
        "Scope And Precedence",
        "Repository Map",
        "Standard Workflow",
        "Commands",
        "Approval Required",
        "Validation Matrix",
        "Architecture Boundaries",
        "Code Rules",
        "Security Rules",
        "Completion Checklist",
        "References",
    ];

    let headings: Vec<_> = markdown_headings(content)
        .into_iter()
        .filter(|heading| heading.level == 2)
        .collect();
    let mut previous_index = None;
    for required in REQUIRED_SECTIONS {
        let matches: Vec<_> = headings
            .iter()
            .enumerate()
            .filter(|(_, heading)| heading.title == *required)
            .collect();
        if matches.is_empty() {
            push_guidance_issue(
                issues,
                "agents.sections.missing",
                "warning",
                "Required AGENTS.md section is missing",
                &format!("Missing section: {required}."),
                relative_path,
            );
            continue;
        }
        if matches.len() > 1 {
            push_guidance_issue(
                issues,
                "agents.sections.duplicate_required",
                "warning",
                "Required AGENTS.md section is duplicated",
                &format!("Section appears more than once: {required}."),
                relative_path,
            );
        }
        let index = matches[0].0;
        if let Some(previous_index) = previous_index
            && index < previous_index
        {
            push_guidance_issue(
                issues,
                "agents.sections.order",
                "warning",
                "AGENTS.md section order drifted",
                &format!("Section {required} appears before an earlier required section."),
                relative_path,
            );
            break;
        }
        previous_index = Some(index);
    }
}

fn audit_duplicate_headings(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    let mut counts = BTreeMap::<String, u64>::new();
    for heading in markdown_headings(content) {
        *counts.entry(heading.title).or_default() += 1;
    }
    for (heading, count) in counts {
        if count > 1 {
            push_guidance_issue(
                issues,
                "agents.sections.duplicate_heading",
                "warning",
                "Duplicate AGENTS.md heading",
                &format!("Heading appears {count} times: {heading}."),
                relative_path,
            );
        }
    }
}

fn audit_banned_agent_phrases(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const BANNED_PHRASES: &[&str] = &[
        "agents.md",
        "Agent Playbook",
        "AI/human collaboration playbook",
        "AI Quickstart",
        "ALWAYS RUN FIRST",
        "BEFORE YOU CODE",
    ];
    for phrase in BANNED_PHRASES {
        if content.contains(phrase) {
            push_guidance_issue(
                issues,
                "agents.drift.banned_phrase",
                "warning",
                "Stale agent-guidance phrase",
                &format!("Remove or modernize stale phrase: {phrase}."),
                relative_path,
            );
        }
    }
}

fn audit_broad_git_examples(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const BANNED_GIT_EXAMPLES: &[&str] = &["git add .", "git add -A", "git commit -a"];
    for line in content.lines() {
        for command in BANNED_GIT_EXAMPLES {
            if line.contains(command) && !line_explicitly_prohibits_command(line) {
                push_guidance_issue(
                    issues,
                    "agents.git.broad_command_example",
                    "error",
                    "Broad git command example",
                    &format!("Use targeted staging examples instead of `{command}`."),
                    relative_path,
                );
            }
        }
    }
}

fn line_explicitly_prohibits_command(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("never use")
        || lower.contains("do not use")
        || lower.contains("don't use")
        || lower.contains("avoid ")
        || lower.contains("forbid")
        || lower.contains("prohibit")
}

fn audit_reference_paths(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
    content: &str,
) {
    let Some(references) = markdown_section(content, "References") else {
        return;
    };
    for candidate in local_markdown_paths(&references) {
        if !repo_root.join(&candidate).exists() {
            push_guidance_issue(
                issues,
                "agents.references.missing_path",
                "warning",
                "Referenced local path is missing",
                &format!("References points at `{candidate}`, but the path does not exist."),
                relative_path,
            );
        }
    }
}

fn audit_canonical_agent_links(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
) {
    let path = repo_root.join(relative_path);
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    if content.contains("agents.md") {
        push_guidance_issue(
            issues,
            "agents.links.casing",
            "warning",
            "Non-canonical AGENTS.md casing",
            &format!("{relative_path} should reference AGENTS.md with canonical uppercase casing."),
            relative_path,
        );
    }
}

#[derive(Clone, Debug)]
struct MarkdownHeading {
    level: usize,
    title: String,
}

fn markdown_headings(content: &str) -> Vec<MarkdownHeading> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') {
                return None;
            }
            let level = trimmed
                .chars()
                .take_while(|character| *character == '#')
                .count();
            if level == 0
                || level > 6
                || !trimmed.chars().nth(level).is_some_and(char::is_whitespace)
            {
                return None;
            }
            Some(MarkdownHeading {
                level,
                title: normalize_markdown_heading_title(
                    trimmed[level..].trim().trim_matches('#').trim(),
                ),
            })
        })
        .collect()
}

fn normalize_markdown_heading_title(title: &str) -> String {
    let title = title.trim();
    let mut saw_digit = false;
    for (index, character) in title.char_indices() {
        if character.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        if saw_digit && matches!(character, '.' | ')') {
            let rest = title[index + character.len_utf8()..].trim_start();
            if !rest.is_empty() {
                return rest.to_owned();
            }
        }
        break;
    }
    title.to_owned()
}

fn markdown_section(content: &str, title: &str) -> Option<String> {
    let mut in_section = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            let heading = trimmed
                .trim_start_matches('#')
                .trim()
                .trim_matches('#')
                .trim();
            let heading = normalize_markdown_heading_title(heading);
            if in_section {
                break;
            }
            in_section = heading == title;
            continue;
        }
        if in_section {
            lines.push(line);
        }
    }
    if in_section {
        Some(lines.join("\n"))
    } else {
        None
    }
}

fn local_markdown_paths(content: &str) -> Vec<String> {
    let mut paths = BTreeSet::<String>::new();
    for token in content
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '(' | ')' | '[' | ']' | ',' | '`' | '"' | '\'')
        })
        .map(|token| token.trim_end_matches(['.', ':', ';']))
    {
        if token.is_empty()
            || token.starts_with("http://")
            || token.starts_with("https://")
            || token.starts_with('#')
            || token.starts_with('/')
        {
            continue;
        }
        if token.contains("..") {
            continue;
        }
        let lower = token.to_lowercase();
        let has_known_reference_extension = lower.ends_with(".md")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml")
            || lower.ends_with(".json");
        if has_known_reference_extension || token.starts_with("./") {
            paths.insert(token.trim_start_matches("./").to_owned());
        }
    }
    paths.into_iter().collect()
}

fn guidance_status(issues: &[StorageAgentGuidanceIssue]) -> String {
    if issues.iter().any(|issue| issue.severity == "error") {
        "error".to_owned()
    } else if !issues.is_empty() {
        "warning".to_owned()
    } else {
        "ok".to_owned()
    }
}

fn push_guidance_issue(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    id: &str,
    severity: &str,
    title: &str,
    detail: &str,
    path: &str,
) {
    issues.push(StorageAgentGuidanceIssue {
        id: id.to_owned(),
        severity: severity.to_owned(),
        title: title.to_owned(),
        detail: detail.to_owned(),
        path: path.to_owned(),
    });
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

fn repo_footprint_for_items(
    repo_root: String,
    items: Vec<&StorageHygieneItem>,
) -> StorageRepoFootprint {
    let repo_name = items
        .iter()
        .find_map(|item| item.attribution.repo_name.clone())
        .or_else(|| {
            Path::new(&repo_root)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "repository".to_owned());
    let artifact_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    let safe_bytes = tier_bytes(&items, "safe");
    let rebuildable_bytes = tier_bytes(&items, "rebuildable");
    let expensive_bytes = tier_bytes(&items, "expensive");
    let risky_bytes = tier_bytes(&items, "risky");
    let rebuildable_percent = percent(rebuildable_bytes, artifact_bytes);
    let mut top_artifact_folders: Vec<_> = items
        .iter()
        .map(|item| StorageRepoArtifactFolder {
            path: item.path.clone(),
            display_name: item.display_name.clone(),
            kind: item.kind.clone(),
            cleanup_tier: item.cleanup_tier.clone(),
            size_bytes: item.size_bytes,
        })
        .collect();
    top_artifact_folders.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    top_artifact_folders.truncate(5);

    let artifact_mix = repo_artifact_mix(&items);
    let last_branch_touched = latest_branch_for_items(&items);
    let (estimated_rebuild_cost, estimated_rebuild_seconds) = estimate_rebuild_cost(&items);
    let optimization_summary = repo_optimization_summary(
        &repo_name,
        artifact_bytes,
        rebuildable_bytes,
        expensive_bytes,
        risky_bytes,
        estimated_rebuild_seconds,
    );

    StorageRepoFootprint {
        id: repo_root.clone(),
        repo_root,
        repo_name,
        current_size_bytes: artifact_bytes,
        artifact_bytes,
        safe_bytes,
        rebuildable_bytes,
        expensive_bytes,
        risky_bytes,
        rebuildable_percent,
        item_count: items.len(),
        top_artifact_folders,
        artifact_mix,
        duplicate_clone_count: 1,
        duplicate_clone_roots: Vec::new(),
        last_writer_process: None,
        last_writer_pid: None,
        last_branch_touched,
        growth_bytes: None,
        growth_window: "No prior scan baseline in this process.".to_owned(),
        estimated_rebuild_cost,
        estimated_rebuild_seconds,
        optimization_summary,
        caveats: vec![
            "Current size is the bounded attributed artifact footprint, not a full source checkout size."
                .to_owned(),
            "Last writer process and exact growth require a file-event journal or scan baseline."
                .to_owned(),
        ],
    }
}

fn apply_growth_deltas_to_repo_footprints(
    footprints: &mut [StorageRepoFootprint],
    growth_deltas: &[StorageGrowthDelta],
) {
    for footprint in footprints {
        let matching = growth_deltas
            .iter()
            .filter(|delta| {
                delta.delta_bytes > 0
                    && delta
                        .repo_root
                        .as_deref()
                        .is_some_and(|repo_root| repo_root == footprint.repo_root)
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        let growth_bytes = matching
            .iter()
            .fold(0i64, |total, delta| total.saturating_add(delta.delta_bytes));
        let strongest = matching
            .iter()
            .max_by(|left, right| {
                left.attribution_confidence_score
                    .cmp(&right.attribution_confidence_score)
                    .then_with(|| left.delta_bytes.cmp(&right.delta_bytes))
            })
            .copied();
        footprint.growth_bytes = Some(growth_bytes);
        footprint.growth_window = "since indexed storage baseline".to_owned();
        if let Some(delta) = strongest {
            footprint.last_branch_touched = delta
                .git_branch
                .clone()
                .or(delta.git_head.clone())
                .or_else(|| footprint.last_branch_touched.clone());
            footprint.last_writer_process = delta
                .command
                .clone()
                .or(delta.process_tree.clone())
                .or(delta.ai_agent_session.clone());
            if footprint.last_writer_process.is_some() {
                footprint.caveats.push(format!(
                    "Growth attribution confidence: {} ({}%).",
                    delta.attribution_confidence, delta.attribution_confidence_score
                ));
            } else {
                footprint.caveats.push(
                    "Growth is repo-attributed, but command/session writer evidence is missing."
                        .to_owned(),
                );
            }
        }
    }
}

fn apply_clone_groups_to_repo_footprints(
    footprints: &mut [StorageRepoFootprint],
    repository_inventory: &[StorageRepositoryInventoryItem],
) {
    let clone_groups_by_root = repository_inventory
        .iter()
        .map(|repository| {
            (
                repository.repo_root.as_str(),
                (
                    repository.clone_group_count,
                    repository.clone_group_roots.clone(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for footprint in footprints {
        if let Some((count, roots)) = clone_groups_by_root.get(footprint.repo_root.as_str()) {
            footprint.duplicate_clone_count = *count;
            footprint.duplicate_clone_roots = roots.clone();
        }
    }
}

fn repo_artifact_mix(items: &[&StorageHygieneItem]) -> Vec<StorageRepoArtifactMix> {
    let mut grouped = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        grouped.entry(item.kind.clone()).or_default().push(*item);
    }
    let mut mix = grouped
        .into_iter()
        .map(|(kind, group)| {
            let bytes = group
                .iter()
                .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
            let cleanup_tier = highest_cleanup_tier(group.iter().copied());
            let path = group
                .first()
                .map(|item| item.path.as_str())
                .unwrap_or_default();
            let intelligence = artifact_intelligence(&kind, path);
            StorageRepoArtifactMix {
                kind: kind.clone(),
                label: artifact_kind_label(&kind).to_owned(),
                item_count: group.len(),
                bytes,
                cleanup_tier,
                rebuild_command: intelligence.rebuild_command,
                estimated_rebuild_cost: intelligence.estimated_rebuild_cost,
                estimated_rebuild_seconds: intelligence.estimated_rebuild_seconds,
            }
        })
        .collect::<Vec<_>>();
    mix.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.label.cmp(&right.label))
    });
    mix.truncate(8);
    mix
}

fn artifact_kind_label(kind: &str) -> &'static str {
    match kind {
        "xcode-derived-data" => "Xcode DerivedData",
        "swift-build" => "SwiftPM .build",
        "rust-build" => "Cargo target",
        "node-dependencies" => "Node dependencies",
        "npm-cache" => "npm cache",
        "pnpm-store" => "pnpm store",
        "yarn-cache" => "Yarn cache",
        "docker-storage" => "Docker storage",
        "xcode-simulator-runtime" => "Xcode simulators",
        "xcode-archives" => "Xcode archives",
        "release-artifact" => "Release artifacts",
        "log-file" | "logs" => "Logs",
        "test-output" => "Test outputs",
        "coverage-output" => "Coverage outputs",
        "next-cache" => "Next.js cache",
        "next-build" => "Next.js build",
        "frontend-cache" => "Frontend cache",
        "python-cache" => "Python cache",
        "python-environment" => "Python environment",
        "tool-cache" => "Tool cache",
        _ => "Storage artifact",
    }
}

fn latest_branch_for_items(items: &[&StorageHygieneItem]) -> Option<String> {
    items
        .iter()
        .filter_map(|item| {
            let reference = item
                .attribution
                .git_branch
                .as_ref()
                .or(item.attribution.git_head.as_ref())?;
            let modified_millis = item.modified_millis.unwrap_or_default();
            Some((modified_millis, reference.clone()))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, reference)| reference)
}

fn estimate_rebuild_cost(items: &[&StorageHygieneItem]) -> (String, Option<u64>) {
    let expensive_bytes = tier_bytes(items, "expensive");
    let risky_bytes = tier_bytes(items, "risky");
    let rebuildable_bytes = tier_bytes(items, "rebuildable");
    let safe_bytes = tier_bytes(items, "safe");
    let total_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));

    if risky_bytes > 0 {
        return ("Review first".to_owned(), None);
    }
    if total_bytes == 0 {
        return ("None".to_owned(), Some(0));
    }
    let gib = 1_024_f64 * 1_024_f64 * 1_024_f64;
    let estimated = ((rebuildable_bytes as f64 / gib) * 25.0)
        + ((expensive_bytes as f64 / gib) * 120.0)
        + ((safe_bytes as f64 / gib) * 5.0);
    let seconds = estimated.round().clamp(60.0, 3_600.0) as u64;
    let label = if expensive_bytes >= 2 * 1_024 * 1_024 * 1_024 || seconds >= 1_200 {
        "High"
    } else if expensive_bytes > 0 || seconds >= 300 {
        "Medium"
    } else {
        "Low"
    };
    (label.to_owned(), Some(seconds))
}

fn repo_optimization_summary(
    repo_name: &str,
    artifact_bytes: u64,
    rebuildable_bytes: u64,
    expensive_bytes: u64,
    risky_bytes: u64,
    estimated_rebuild_seconds: Option<u64>,
) -> String {
    if artifact_bytes == 0 {
        return format!(
            "{repo_name} has no attributed reclaimable development artifacts in this scan."
        );
    }
    let dominant =
        if risky_bytes > 0 && risky_bytes >= rebuildable_bytes && risky_bytes >= expensive_bytes {
            "needs manual review"
        } else if rebuildable_bytes >= expensive_bytes {
            "mostly rebuildable"
        } else {
            "mostly expensive to restore"
        };
    let time = estimated_rebuild_seconds
        .map(|seconds| {
            format!(
                ", estimated rebuild cost {}",
                rebuild_seconds_label(seconds)
            )
        })
        .unwrap_or_else(|| ", rebuild cost requires review".to_owned());
    format!(
        "{repo_name} has {} reclaimable attributed artifacts, {dominant}{time}.",
        human_bytes(artifact_bytes)
    )
}

fn tier_bytes(items: &[&StorageHygieneItem], tier: &str) -> u64 {
    items
        .iter()
        .filter(|item| item.cleanup_tier == tier)
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes))
}

fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[derive(Clone, Debug, Default)]
struct GitHead {
    branch: Option<String>,
    short_head: Option<String>,
    reference: Option<String>,
    detached: bool,
}

#[derive(Clone, Debug)]
struct GitRemoteParts {
    host: Option<String>,
    owner: Option<String>,
    name: Option<String>,
}

#[derive(Clone, Debug)]
struct GitDirtyStatus {
    status: String,
    file_count: Option<u64>,
    truncated: bool,
}

fn read_git_head(repo_root: &Path) -> GitHead {
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return GitHead::default();
    };
    let Ok(head) = fs::read_to_string(git_dir.join("HEAD")) else {
        return GitHead::default();
    };
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: ") {
        let branch = reference
            .strip_prefix("refs/heads/")
            .unwrap_or(reference)
            .to_owned();
        let head_sha = fs::read_to_string(git_dir.join(reference))
            .ok()
            .map(|value| short_hash(value.trim()))
            .or_else(|| read_packed_ref(&git_dir, reference).map(|value| short_hash(&value)));
        return GitHead {
            branch: Some(branch),
            short_head: head_sha,
            reference: Some(reference.to_owned()),
            detached: false,
        };
    }

    if head.is_empty() {
        GitHead::default()
    } else {
        GitHead {
            branch: None,
            short_head: Some(short_hash(head)),
            reference: Some(head.to_owned()),
            detached: true,
        }
    }
}

fn read_git_config_value(repo_root: &Path, target_section: &str, key: &str) -> Option<String> {
    let git_dir = resolve_git_dir(repo_root)?;
    let config = fs::read_to_string(git_dir.join("config")).ok()?;
    let mut active_section: Option<String> = None;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            active_section = Some(trimmed[1..trimmed.len().saturating_sub(1)].to_owned());
            continue;
        }
        if active_section.as_deref() != Some(target_section) {
            continue;
        }
        let Some((candidate_key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if candidate_key.trim() == key {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn normalize_git_remote_key(remote_url: &str) -> Option<String> {
    let mut value = remote_url.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(stripped) = value.strip_prefix("https://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("http://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("ssh://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("git@") {
        value = stripped;
    }
    value = strip_remote_userinfo(value);
    if let Some((host, path)) = value.split_once(':')
        && !host.contains('/')
        && !path.is_empty()
    {
        return Some(clean_remote_key(&format!("{host}/{path}")));
    }
    Some(clean_remote_key(value))
}

fn redact_git_remote_url(remote_url: &str) -> Option<String> {
    let value = remote_url
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if value.is_empty() {
        return None;
    }
    if let Some((scheme, rest)) = value.split_once("://") {
        return Some(format!("{scheme}://{}", strip_remote_userinfo(rest)));
    }
    if let Some(stripped) = value.strip_prefix("git@") {
        return Some(stripped.to_owned());
    }
    if let Some((user_host, path)) = value.split_once(':')
        && user_host.contains('@')
        && !user_host.contains('/')
        && !path.is_empty()
    {
        let host = user_host.rsplit('@').next().unwrap_or_default();
        return Some(format!("{host}:{path}"));
    }
    Some(strip_remote_userinfo(value).to_owned())
}

fn strip_remote_userinfo(value: &str) -> &str {
    let first_slash = value.find('/');
    if let Some(at) = value.find('@')
        && first_slash.is_none_or(|slash| at < slash)
    {
        return &value[at + 1..];
    }
    value
}

fn clean_remote_key(value: &str) -> String {
    let mut cleaned = value
        .split(['?', '#'])
        .next()
        .unwrap_or(value)
        .trim_matches('/')
        .trim()
        .to_lowercase();
    if let Some(stripped) = cleaned.strip_suffix(".git") {
        cleaned = stripped.to_owned();
    }
    cleaned
}

fn split_git_remote_key(remote_key: &str) -> GitRemoteParts {
    let parts: Vec<_> = remote_key
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let name = parts.last().map(|part| (*part).to_owned());
    let owner = if parts.len() >= 3 {
        parts
            .get(parts.len().saturating_sub(2))
            .map(|part| (*part).to_owned())
    } else {
        None
    };
    let host = parts.first().map(|part| (*part).to_owned());
    GitRemoteParts { host, owner, name }
}

fn read_git_dirty_status(repo_root: &Path, scan_started: Instant) -> GitDirtyStatus {
    if scan_started.elapsed() >= SCAN_TIME_BUDGET {
        return GitDirtyStatus {
            status: "timeout".to_owned(),
            file_count: None,
            truncated: false,
        };
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("--untracked-files=normal")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return GitDirtyStatus {
            status: "unavailable".to_owned(),
            file_count: None,
            truncated: false,
        };
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return GitDirtyStatus {
                        status: "unavailable".to_owned(),
                        file_count: None,
                        truncated: false,
                    };
                }
                let stdout = output
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stdout))
                    .unwrap_or_default();
                let count = stdout.lines().take(GIT_STATUS_MAX_LINES + 1).count();
                let truncated = count > GIT_STATUS_MAX_LINES;
                return GitDirtyStatus {
                    status: if count == 0 { "clean" } else { "dirty" }.to_owned(),
                    file_count: Some(count.min(GIT_STATUS_MAX_LINES) as u64),
                    truncated,
                };
            }
            Ok(None)
                if started.elapsed() < GIT_STATUS_TIME_BUDGET
                    && scan_started.elapsed() < SCAN_TIME_BUDGET =>
            {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return GitDirtyStatus {
                    status: "timeout".to_owned(),
                    file_count: None,
                    truncated: false,
                };
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return GitDirtyStatus {
                    status: "unavailable".to_owned(),
                    file_count: None,
                    truncated: false,
                };
            }
        }
    }
}

fn read_packed_ref(git_dir: &Path, reference: &str) -> Option<String> {
    let content = fs::read_to_string(git_dir.join("packed-refs")).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('^') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let hash = parts.next()?;
        let candidate = parts.next()?;
        if candidate == reference {
            return Some(hash.to_owned());
        }
    }
    None
}

fn git_tracked_paths(repo_root: &Path, relative_paths: &[&str]) -> BTreeSet<String> {
    if relative_paths.is_empty() {
        return BTreeSet::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("ls-files")
        .arg("--")
        .args(relative_paths)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return BTreeSet::new();
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return BTreeSet::new();
                }
                return output
                    .as_ref()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .filter(|path| !path.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeSet::new();
            }
        }
    }
}

fn git_tracked_path_set(repo_root: &Path, relative_paths: &[String]) -> BTreeSet<String> {
    let borrowed: Vec<&str> = relative_paths.iter().map(String::as_str).collect();
    git_tracked_paths(repo_root, &borrowed)
}

fn git_status_path_map(repo_root: &Path, relative_paths: &[String]) -> BTreeMap<String, String> {
    if relative_paths.is_empty() {
        return BTreeMap::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("--untracked-files=normal")
        .arg("--")
        .args(relative_paths)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return BTreeMap::new();
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return BTreeMap::new();
                }
                return output
                    .as_ref()
                    .map(|output| {
                        parse_git_status_porcelain(&String::from_utf8_lossy(&output.stdout))
                    })
                    .unwrap_or_default();
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeMap::new();
            }
        }
    }
}

fn git_repository_has_active_changes(repo_root: &Path) -> bool {
    let status = read_git_dirty_status(repo_root, Instant::now());
    status.status == "dirty"
}

fn parse_git_status_porcelain(output: &str) -> BTreeMap<String, String> {
    let mut statuses = BTreeMap::new();
    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..]
            .split(" -> ")
            .last()
            .unwrap_or_default()
            .trim()
            .trim_matches('"');
        if path.is_empty() {
            continue;
        }
        let status = match code {
            "??" => "untracked",
            "!!" => "ignored",
            _ if code.contains('U') => "conflicted",
            _ if code.contains('D') => "deleted",
            _ if code.contains('R') => "renamed",
            _ if code.contains('M') || code.contains('A') || code.contains('C') => "modified",
            _ => "modified",
        };
        statuses.insert(path.to_owned(), status.to_owned());
    }
    statuses
}

fn git_ignored_path_set(repo_root: &Path, relative_paths: &[String]) -> BTreeSet<String> {
    if relative_paths.is_empty() {
        return BTreeSet::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("check-ignore")
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return local_gitignore_path_set(repo_root, relative_paths);
    };

    if let Some(mut stdin) = child.stdin.take() {
        for path in relative_paths {
            let _ = writeln!(stdin, "{path}");
        }
    }

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let output = child.wait_with_output().ok();
                let mut ignored: BTreeSet<String> = output
                    .as_ref()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .filter(|path| !path.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                ignored.extend(local_gitignore_path_set(repo_root, relative_paths));
                return ignored;
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeSet::new();
            }
        }
    }
}

fn local_gitignore_path_set(repo_root: &Path, relative_paths: &[String]) -> BTreeSet<String> {
    let mut ignore_contents = Vec::new();
    if let Ok(content) = fs::read_to_string(repo_root.join(".gitignore")) {
        ignore_contents.push(content);
    }
    if let Some(git_dir) = resolve_git_dir(repo_root)
        && let Ok(content) = fs::read_to_string(git_dir.join("info").join("exclude"))
    {
        ignore_contents.push(content);
    }
    if ignore_contents.is_empty() {
        return BTreeSet::new();
    }
    let patterns: Vec<_> = ignore_contents
        .iter()
        .flat_map(|content| content.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .map(|line| line.trim_matches('/').to_owned())
        .collect();
    let mut ignored = BTreeSet::new();
    for path in relative_paths {
        let normalized = path.trim_matches('/');
        let file_name = Path::new(normalized)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(normalized);
        if patterns.iter().any(|pattern| {
            pattern == normalized
                || pattern == file_name
                || normalized.starts_with(&format!("{pattern}/"))
        }) {
            ignored.insert(path.clone());
        }
    }
    ignored
}

fn resolve_git_dir(repo_root: &Path) -> Option<PathBuf> {
    let git_marker = repo_root.join(".git");
    if git_marker.is_dir() {
        return Some(git_marker);
    }
    let marker = fs::read_to_string(&git_marker).ok()?;
    let git_dir = marker.trim().strip_prefix("gitdir:")?.trim();
    let path = PathBuf::from(git_dir);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(repo_root.join(path))
    }
}

fn is_git_repository_root(path: &Path) -> bool {
    resolve_git_dir(path).is_some()
}

#[derive(Clone, Debug, Default)]
struct RepositoryInventoryScan {
    repositories_by_root: BTreeMap<String, String>,
    coverage: Vec<RepositoryInventoryRootCoverage>,
    truncated: bool,
}

fn scan_repository_inventory_roots(
    requested_roots: &[PathBuf],
    max_depth: usize,
) -> RepositoryInventoryScan {
    scan_repository_inventory_roots_with_budget(
        requested_roots,
        max_depth,
        REPOSITORY_INVENTORY_TIME_BUDGET,
        None,
    )
}

fn scan_repository_inventory_roots_with_budget(
    requested_roots: &[PathBuf],
    max_depth: usize,
    budget: Duration,
    runtime: Option<&StorageScanRuntimeContext>,
) -> RepositoryInventoryScan {
    let started = Instant::now();
    let mut repositories_by_root = BTreeMap::<String, String>::new();
    let mut coverage = Vec::new();
    let mut truncated = false;

    for root in requested_roots {
        let path = root.display().to_string();
        let label = storage_source_label(root, &storage_source_kind(root));
        if let Some(runtime) = runtime
            && !runtime.set_phase(STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY, Some(root))
        {
            truncated = true;
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "partial",
                "partial",
                "Repository inventory stopped before this root because the scan was cancelled.",
                0,
                0,
                0,
                true,
                false,
            ));
            continue;
        }

        if started.elapsed() >= budget {
            truncated = true;
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "partial",
                "partial",
                "Repository inventory stopped before this root because the inventory budget ended.",
                0,
                0,
                0,
                true,
                false,
            ));
            continue;
        }

        if !root.exists() {
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "unavailable",
                "unavailable",
                "Path does not exist on this machine or provider is not configured.",
                0,
                0,
                0,
                false,
                false,
            ));
            continue;
        }

        let metadata = match fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(error) => {
                let detail = error.to_string();
                let permission_state = skipped_root_permission_state(&detail);
                coverage.push(repository_inventory_coverage_entry(
                    root,
                    label,
                    "skipped",
                    &permission_state,
                    &detail,
                    0,
                    0,
                    0,
                    false,
                    false,
                ));
                continue;
            }
        };

        if metadata.file_type().is_symlink() {
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "skipped",
                "skipped",
                "Symlink root skipped during repository inventory.",
                0,
                0,
                0,
                false,
                false,
            ));
            continue;
        }

        if !is_git_repository_root(root)
            && let Some(reason) = repository_inventory_skip_reason(root)
        {
            coverage.push(repository_inventory_coverage_entry(
                root, label, "skipped", "skipped", reason, 0, 0, 1, false, false,
            ));
            continue;
        }

        let before = repositories_by_root.len();
        let root_scan = scan_repository_inventory_root(root, max_depth, started, budget, runtime);
        truncated |= root_scan.truncated;
        for repo_root in root_scan.repositories {
            repositories_by_root
                .entry(repo_root.display().to_string())
                .or_insert_with(|| path.clone());
        }
        let repository_count = repositories_by_root.len().saturating_sub(before) as u64;
        coverage.push(repository_inventory_coverage_entry(
            root,
            label,
            if root_scan.truncated {
                "partial"
            } else {
                "scanned"
            },
            if root_scan.truncated {
                "partial"
            } else {
                "readable"
            },
            if root_scan.truncated {
                "Repository inventory was partially scanned before the inventory budget ended."
            } else {
                "Readable and scanned by the repository inventory pass."
            },
            repository_count,
            root_scan.scanned_directory_count,
            root_scan.skipped_directory_count,
            root_scan.truncated,
            !root_scan.truncated,
        ));
    }

    RepositoryInventoryScan {
        repositories_by_root,
        coverage,
        truncated,
    }
}

#[derive(Clone, Debug, Default)]
struct RepositoryInventoryRootScan {
    repositories: BTreeSet<PathBuf>,
    scanned_directory_count: u64,
    skipped_directory_count: u64,
    truncated: bool,
}

fn scan_repository_inventory_root(
    root: &Path,
    max_depth: usize,
    started: Instant,
    budget: Duration,
    runtime: Option<&StorageScanRuntimeContext>,
) -> RepositoryInventoryRootScan {
    let max_depth = max_depth.clamp(1, 12);
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut repositories = BTreeSet::new();
    let mut scanned_directory_count = 0u64;
    let mut skipped_directory_count = 0u64;
    let mut truncated = false;

    while let Some((path, depth)) = queue.pop_front() {
        if started.elapsed() >= budget
            || scanned_directory_count >= REPOSITORY_INVENTORY_MAX_DIRECTORIES
        {
            truncated = true;
            break;
        }

        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }

        let is_repo = is_git_repository_root(&path);
        if is_repo {
            repositories.insert(path.clone());
        } else if depth > 0 && repository_inventory_skip_reason(&path).is_some() {
            skipped_directory_count = skipped_directory_count.saturating_add(1);
            continue;
        }

        if depth >= max_depth {
            continue;
        }

        scanned_directory_count = scanned_directory_count.saturating_add(1);
        if let Some(runtime) = runtime
            && !runtime.checkpoint(
                STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY,
                Some(&path),
                0,
                1,
                0,
            )
        {
            truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        let mut children = entries
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            if child
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == ".git")
            {
                skipped_directory_count = skipped_directory_count.saturating_add(1);
                continue;
            }
            queue.push_back((child, depth + 1));
        }
    }

    RepositoryInventoryRootScan {
        repositories,
        scanned_directory_count,
        skipped_directory_count,
        truncated,
    }
}

fn repository_inventory_skip_reason(path: &Path) -> Option<&'static str> {
    match path.file_name().and_then(|name| name.to_str())? {
        ".git" => Some("Git internals skipped during repository inventory."),
        "node_modules" => Some("Dependency tree skipped during repository inventory."),
        "target" => Some("Rust build output skipped during repository inventory."),
        ".build" => Some("Build output skipped during repository inventory."),
        "DerivedData" => Some("Xcode DerivedData skipped during repository inventory."),
        ".cache" => Some("Cache directory skipped during repository inventory."),
        "Library" => Some("macOS Library tree skipped during repository inventory."),
        ".docker" => Some("Docker data skipped during repository inventory."),
        ".npm" => Some("Package cache skipped during repository inventory."),
        ".pnpm-store" => Some("Package cache skipped during repository inventory."),
        ".cargo" => Some("Package cache skipped during repository inventory."),
        ".gradle" => Some("Gradle cache skipped during repository inventory."),
        ".venv" | "venv" => Some("Virtual environment skipped during repository inventory."),
        ".tox" => Some("Python test environment skipped during repository inventory."),
        "__pycache__" => Some("Python bytecode cache skipped during repository inventory."),
        ".next" | ".turbo" => Some("Frontend build cache skipped during repository inventory."),
        "Pods" => Some("Dependency tree skipped during repository inventory."),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn repository_inventory_coverage_entry(
    root: &Path,
    label: String,
    status: &str,
    permission_state: &str,
    detail: &str,
    repository_count: u64,
    scanned_directory_count: u64,
    skipped_directory_count: u64,
    truncated: bool,
    scanned: bool,
) -> RepositoryInventoryRootCoverage {
    let path = root.display().to_string();
    RepositoryInventoryRootCoverage {
        id: path.clone(),
        label,
        path,
        status: status.to_owned(),
        permission_state: permission_state.to_owned(),
        detail: detail.to_owned(),
        repository_count,
        scanned_directory_count,
        skipped_directory_count,
        truncated,
        scanned,
    }
}

fn repository_inventory_completeness(
    coverage: &[RepositoryInventoryRootCoverage],
    truncated: bool,
) -> RepositoryInventoryCompleteness {
    let roots = coverage
        .iter()
        .map(|root| root.path.clone())
        .collect::<Vec<_>>();
    let partial_roots = coverage
        .iter()
        .filter(|root| root.truncated || !root.scanned || root.status == "partial")
        .map(|root| root.path.clone())
        .collect::<Vec<_>>();
    let truncated = truncated || coverage.iter().any(|root| root.truncated);
    RepositoryInventoryCompleteness {
        complete: !truncated && partial_roots.is_empty(),
        truncated,
        roots,
        partial_roots,
    }
}

fn cached_repository_inventory_coverage(
    requested_roots: &[PathBuf],
    repositories_by_root: &BTreeMap<String, String>,
) -> Vec<RepositoryInventoryRootCoverage> {
    requested_roots
        .iter()
        .map(|root| {
            let label = storage_source_label(root, &storage_source_kind(root));
            if !root.exists() {
                return repository_inventory_coverage_entry(
                    root,
                    label,
                    "unavailable",
                    "unavailable",
                    "Path does not exist on this machine or provider is not configured.",
                    0,
                    0,
                    0,
                    false,
                    false,
                );
            }

            let metadata = match fs::symlink_metadata(root) {
                Ok(metadata) => metadata,
                Err(error) => {
                    let detail = error.to_string();
                    let permission_state = skipped_root_permission_state(&detail);
                    return repository_inventory_coverage_entry(
                        root,
                        label,
                        "skipped",
                        &permission_state,
                        &detail,
                        0,
                        0,
                        0,
                        false,
                        false,
                    );
                }
            };
            if metadata.file_type().is_symlink() {
                return repository_inventory_coverage_entry(
                    root,
                    label,
                    "skipped",
                    "skipped",
                    "Symlink root skipped during cached repository inventory coverage.",
                    0,
                    0,
                    0,
                    false,
                    false,
                );
            }

            let root_display = root.display().to_string();
            let repository_count = repositories_by_root
                .values()
                .filter(|discovered_root| *discovered_root == &root_display)
                .count() as u64;
            let (status, detail) = if repository_count == 0 {
                (
                    "cached_empty",
                    "No repositories are currently present in the storage index for this root; run a refresh for authoritative inventory.",
                )
            } else {
                (
                    "cached_partial",
                    "Repository inventory was restored from the storage index snapshot; run a refresh for authoritative inventory.",
                )
            };
            repository_inventory_coverage_entry(
                root,
                label,
                status,
                "cached",
                detail,
                repository_count,
                0,
                0,
                false,
                false,
            )
        })
        .collect()
}

fn repository_inventory_cache_roots(
    entries: &BTreeMap<String, RepositoryInventoryCacheEntry>,
) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(repo_root, entry)| (repo_root.clone(), entry.discovered_root.clone()))
        .collect()
}

fn repository_inventory_cache_states(
    repositories_by_root: &BTreeMap<String, String>,
    latest_repositories_by_root: &BTreeMap<String, String>,
    cached_entries: &BTreeMap<String, RepositoryInventoryCacheEntry>,
) -> BTreeMap<String, RepositoryInventoryCacheState> {
    repositories_by_root
        .keys()
        .map(|repo_root| {
            let current_fingerprint = repository_inventory_fingerprint(Path::new(repo_root));
            let state = if latest_repositories_by_root.contains_key(repo_root) {
                RepositoryInventoryCacheState {
                    status: "scanned".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: false,
                    last_seen_millis: cached_entries
                        .get(repo_root)
                        .map(|entry| entry.last_seen_millis),
                    last_scan_millis: cached_entries
                        .get(repo_root)
                        .map(|entry| entry.last_scan_millis),
                }
            } else if !Path::new(repo_root).exists() {
                let cached = cached_entries.get(repo_root);
                RepositoryInventoryCacheState {
                    status: "missing".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: true,
                    last_seen_millis: cached.map(|entry| entry.last_seen_millis),
                    last_scan_millis: cached.map(|entry| entry.last_scan_millis),
                }
            } else if let Some(cached) = cached_entries.get(repo_root) {
                let cached_fingerprint = cached.repository_fingerprint.trim();
                let (status, fingerprint_changed) = if cached_fingerprint.is_empty() {
                    ("legacy", false)
                } else if cached_fingerprint == current_fingerprint {
                    ("current", false)
                } else {
                    ("changed", true)
                };
                RepositoryInventoryCacheState {
                    status: status.to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed,
                    last_seen_millis: Some(cached.last_seen_millis),
                    last_scan_millis: Some(cached.last_scan_millis),
                }
            } else {
                RepositoryInventoryCacheState {
                    status: "uncached".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: false,
                    last_seen_millis: None,
                    last_scan_millis: None,
                }
            };
            (repo_root.clone(), state)
        })
        .collect()
}

fn merge_repository_inventory_cache(
    cached: BTreeMap<String, String>,
    latest: BTreeMap<String, String>,
) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let mut merged = cached;
    let mut not_seen = merged.keys().cloned().collect::<BTreeSet<_>>();
    for (repo_root, discovered_root) in latest {
        not_seen.remove(&repo_root);
        merged.insert(repo_root, discovered_root);
    }
    (merged, not_seen)
}

fn repository_inventory_fingerprint(repo_root: &Path) -> String {
    let root = repo_root.display().to_string();
    let root_fingerprint = repository_path_fingerprint(repo_root, "root");
    let git_marker = repo_root.join(".git");
    let marker_fingerprint = repository_path_fingerprint(&git_marker, "git-marker");
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return format!("v1|path={root}|{root_fingerprint}|{marker_fingerprint}|git-dir:missing");
    };

    let head = read_git_head(repo_root);
    let head_ref_fingerprint = head
        .reference
        .as_deref()
        .filter(|reference| reference.starts_with("refs/"))
        .map(|reference| repository_path_fingerprint(&git_dir.join(reference), "git-ref"))
        .unwrap_or_else(|| "git-ref:detached-or-missing".to_owned());

    [
        "v1".to_owned(),
        format!("path={root}"),
        root_fingerprint,
        marker_fingerprint,
        repository_path_fingerprint(&git_dir, "git-dir"),
        repository_path_fingerprint(&git_dir.join("config"), "config"),
        repository_path_fingerprint(&git_dir.join("index"), "index"),
        repository_path_fingerprint(&git_dir.join("HEAD"), "HEAD"),
        head_ref_fingerprint,
        repository_path_fingerprint(&git_dir.join("packed-refs"), "packed-refs"),
    ]
    .join("|")
}

fn repository_path_fingerprint(path: &Path, label: &str) -> String {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return format!("{label}:missing");
    };
    let modified_millis = unix_metadata_millis(metadata.mtime(), metadata.mtime_nsec());
    let changed_millis = unix_metadata_millis(metadata.ctime(), metadata.ctime_nsec());
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "dir"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    };
    format!(
        "{}:{}:{}:{}:{}:{}:{}",
        label,
        kind,
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        modified_millis,
        changed_millis
    )
}

fn repository_git_file_fingerprint(repo_root: &Path, file_name: &str) -> String {
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return format!("{file_name}:missing-git-dir");
    };
    repository_path_fingerprint(&git_dir.join(file_name), file_name)
}

fn short_hash(value: &str) -> String {
    value.chars().take(12).collect()
}

fn normalize_roots(roots: Vec<String>) -> Vec<PathBuf> {
    let selected = if roots.is_empty() {
        default_storage_roots()
    } else {
        roots
    };

    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for root in selected.into_iter().take(MAX_ROOTS) {
        let path = expand_home(root.trim());
        if path.as_os_str().is_empty() {
            continue;
        }
        let key = path.display().to_string();
        if seen.insert(key) {
            normalized.push(path);
        }
    }
    normalized
}

fn normalize_dirty_paths(paths: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for path in paths.into_iter().take(512) {
        let expanded = expand_home(path.trim());
        if expanded.as_os_str().is_empty() {
            continue;
        }
        let key = expanded.display().to_string();
        if seen.insert(key.clone()) {
            normalized.push(key);
        }
    }
    normalized
}

fn path_matches_dirty_prefix(path: &Path, dirty_paths: &[String]) -> bool {
    if dirty_paths.is_empty() {
        return false;
    }
    let path = path.display().to_string();
    dirty_paths.iter().any(|dirty| {
        path == *dirty
            || path
                .strip_prefix(dirty)
                .is_some_and(|suffix| suffix.starts_with('/'))
            || dirty
                .strip_prefix(&path)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

fn default_storage_roots() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut roots: Vec<String> = [
        "Repositories",
        "Documents",
        "Desktop",
        "Downloads",
        "Developer",
        "Projects",
        "Applications",
        "Library",
        "Library/Application Support",
        "Library/Containers",
        ".claude",
        ".codex",
        ".cursor",
        ".aider",
        ".cache",
        ".docker",
        ".npm",
        ".pnpm-store",
        ".cargo",
        "Library/Developer/Xcode/DerivedData",
        "Library/Developer/Xcode/Archives",
        "Library/Developer/CoreSimulator",
        "Library/Application Support/MobileSync/Backup",
        "Library/Mail",
        "Library/Messages/Attachments",
        "Library/Containers/com.docker.docker",
        "Library/Group Containers/group.com.docker",
        "Library/Caches/org.swift.swiftpm",
        "Library/Caches/com.apple.dt.Xcode",
        "Library/CloudStorage",
        "Library/Mobile Documents",
        "Dropbox",
        "OneDrive",
        "Google Drive",
    ]
    .into_iter()
    .map(|relative| home.join(relative).display().to_string())
    .collect();

    roots.extend(
        ["/Applications", "/Library", "/Users/Shared"]
            .into_iter()
            .map(String::from),
    );

    if let Ok(entries) = fs::read_dir("/Volumes") {
        for entry in entries.flatten().take(16) {
            let path = entry.path();
            if path.file_name().and_then(|name| name.to_str()) == Some("Macintosh HD") {
                continue;
            }
            roots.push(path.display().to_string());
        }
    }

    roots
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

fn summarize_source_coverage(
    requested_roots: &[PathBuf],
    scanned_roots: &[String],
    skipped_roots: &[StorageSkippedRoot],
    items: &[StorageHygieneItem],
) -> Vec<StorageSourceCoverage> {
    let scanned = scanned_roots.iter().cloned().collect::<BTreeSet<_>>();
    let skipped = skipped_roots
        .iter()
        .map(|root| (root.path.clone(), root.reason.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut coverage = Vec::new();
    for root in requested_roots {
        let path = root.display().to_string();
        let metadata = fs::symlink_metadata(root).ok();
        let kind = storage_source_kind(root);
        let label = storage_source_label(root, &kind);
        let network = is_network_storage_path(root);
        let cloud_root = is_cloud_storage_path(root);
        let cloud_placeholder = metadata.as_ref().is_some_and(|metadata| {
            metadata.is_file() && metadata.len() > 0 && metadata.blocks() == 0
        });
        let reclaimable_bytes = source_reclaimable_bytes(&path, items);
        let protected = is_protected_cleanup_path(&path);
        let (status, permission_state, detail, scanned_flag) = if let Some(reason) =
            skipped.get(&path)
        {
            (
                "skipped".to_owned(),
                skipped_root_permission_state(reason),
                reason.clone(),
                false,
            )
        } else if scanned.contains(&path) {
            (
                if cloud_root {
                    "scanned-cloud-aware".to_owned()
                } else {
                    "scanned".to_owned()
                },
                "readable".to_owned(),
                if cloud_root {
                    "Cloud-backed root scanned for locally materialized files; dehydrated placeholders may report logical size without local bytes.".to_owned()
                } else if network {
                    "Network or external-style root scanned with bounded traversal.".to_owned()
                } else {
                    "Readable and included in the bounded scan.".to_owned()
                },
                true,
            )
        } else if root.exists() {
            (
                "partial".to_owned(),
                "partial".to_owned(),
                "Root exists but did not complete scanning before the current budget ended."
                    .to_owned(),
                false,
            )
        } else {
            (
                "unavailable".to_owned(),
                "unavailable".to_owned(),
                "Path does not exist on this machine or provider is not configured.".to_owned(),
                false,
            )
        };
        let local_bytes = metadata.as_ref().and_then(|metadata| {
            if metadata.is_file() {
                Some(metadata.blocks().saturating_mul(512))
            } else {
                None
            }
        });
        let logical_bytes = metadata.as_ref().and_then(|metadata| {
            if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            }
        });
        let gap_kind = storage_source_gap_kind(
            &status,
            &permission_state,
            cloud_root || cloud_placeholder,
            protected,
            network,
        );
        coverage.push(StorageSourceCoverage {
            id: path.clone(),
            label,
            kind,
            path,
            status,
            permission_state,
            gap_kind,
            detail,
            local_bytes,
            logical_bytes,
            reclaimable_bytes: (reclaimable_bytes > 0).then_some(reclaimable_bytes),
            cloud_placeholder,
            network,
            protected,
            scanned: scanned_flag,
        });
    }
    coverage.sort_by(|left, right| {
        source_status_rank(&left.status)
            .cmp(&source_status_rank(&right.status))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.label.cmp(&right.label))
    });
    coverage
}

fn storage_source_gap_kind(
    status: &str,
    permission_state: &str,
    cloud: bool,
    protected: bool,
    network: bool,
) -> String {
    if permission_state == "needs_full_disk_access" {
        "permission-denied".to_owned()
    } else if status == "unavailable" {
        "unavailable".to_owned()
    } else if status == "skipped" {
        "skipped".to_owned()
    } else if protected {
        "protected".to_owned()
    } else if cloud {
        "cloud-backed".to_owned()
    } else if network {
        "external-or-network".to_owned()
    } else if status == "partial" {
        "partial".to_owned()
    } else {
        "covered".to_owned()
    }
}

fn summarize_volume_states(requested_roots: &[PathBuf]) -> Vec<StorageVolumeState> {
    let mut seen = BTreeSet::<u64>::new();
    let mut volumes = Vec::new();
    for root in requested_roots {
        let probe = if root.exists() {
            root.clone()
        } else {
            existing_parent(root).unwrap_or_else(|| root.clone())
        };
        let Some(state) = volume_state_for_path(&probe) else {
            continue;
        };
        if seen.insert(state.device_id) {
            volumes.push(state);
        }
    }
    volumes.sort_by(|left, right| left.path.cmp(&right.path));
    volumes
}

fn volume_state_for_path(path: &Path) -> Option<StorageVolumeState> {
    let c_path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let metadata = fs::symlink_metadata(path).ok()?;
    let mut stat = MaybeUninit::<libc::statfs>::uninit();
    let rc = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    let block_size = u64::from(stat.f_bsize);
    let blocks = stat.f_blocks;
    let free_blocks = stat.f_bfree;
    let available_blocks = stat.f_bavail;
    let total_bytes = blocks.saturating_mul(block_size);
    let free_now_bytes = free_blocks.saturating_mul(block_size);
    let available_bytes = available_blocks.saturating_mul(block_size);
    let filesystem_type = filesystem_type_name(&stat);
    Some(StorageVolumeState {
        path: volume_mount_path(&stat).unwrap_or_else(|| path.display().to_string()),
        device_id: metadata.dev(),
        filesystem_type,
        total_bytes,
        free_now_bytes,
        available_bytes,
        purgeable_bytes_estimate: available_bytes.saturating_sub(free_now_bytes),
        important_usage_available_bytes: None,
        opportunistic_usage_available_bytes: None,
        detail: "POSIX statfs capacity. Swift enriches important/opportunistic macOS capacity keys when available.".to_owned(),
    })
}

#[cfg(target_os = "macos")]
fn filesystem_type_name(stat: &libc::statfs) -> String {
    c_char_array_to_string(&stat.f_fstypename)
}

#[cfg(not(target_os = "macos"))]
fn filesystem_type_name(_stat: &libc::statfs) -> String {
    "unknown".to_owned()
}

#[cfg(target_os = "macos")]
fn volume_mount_path(stat: &libc::statfs) -> Option<String> {
    let value = c_char_array_to_string(&stat.f_mntonname);
    (!value.is_empty()).then_some(value)
}

#[cfg(not(target_os = "macos"))]
fn volume_mount_path(_stat: &libc::statfs) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn c_char_array_to_string(value: &[libc::c_char]) -> String {
    let bytes = value
        .iter()
        .take_while(|char| **char != 0)
        .map(|char| *char as u8)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).to_string()
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

fn source_reclaimable_bytes(path: &str, items: &[StorageHygieneItem]) -> u64 {
    items
        .iter()
        .filter(|item| item.path == path || item.path.starts_with(&format!("{path}/")))
        .map(|item| item.size_bytes)
        .fold(0u64, u64::saturating_add)
}

fn skipped_root_permission_state(reason: &str) -> String {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("permission") || lower.contains("operation not permitted") {
        "needs_full_disk_access".to_owned()
    } else if lower.contains("missing") {
        "unavailable".to_owned()
    } else {
        "partial".to_owned()
    }
}

fn source_status_rank(status: &str) -> u8 {
    match status {
        "scanned" | "scanned-cloud-aware" => 0,
        "partial" => 1,
        "skipped" => 2,
        _ => 3,
    }
}

fn storage_source_kind(path: &Path) -> String {
    let display = path.display().to_string();
    let home = dirs::home_dir()
        .map(|home| home.display().to_string())
        .unwrap_or_default();
    if is_cloud_storage_path(path) {
        "cloud".to_owned()
    } else if is_network_storage_path(path) {
        "network-or-external".to_owned()
    } else if display == "/Applications" || display.ends_with("/Applications") {
        "applications".to_owned()
    } else if display == "/Library" || display.ends_with("/Library") {
        "library".to_owned()
    } else if display.contains("Xcode") || display.contains("CoreSimulator") {
        "xcode".to_owned()
    } else if display.contains(".docker") || display.contains("Docker") {
        "docker".to_owned()
    } else if display.contains(".cargo")
        || display.contains(".npm")
        || display.contains(".pnpm")
        || display.contains("swiftpm")
    {
        "package-cache".to_owned()
    } else if !home.is_empty() && display.starts_with(&home) {
        "home".to_owned()
    } else {
        "volume".to_owned()
    }
}

fn storage_source_label(path: &Path, kind: &str) -> String {
    let display = path.display().to_string();
    if let Some(name) = path.file_name().and_then(|name| name.to_str())
        && !name.is_empty()
    {
        return match kind {
            "cloud" => format!("Cloud: {name}"),
            "network-or-external" => format!("Volume: {name}"),
            _ => name.to_owned(),
        };
    }
    display
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

#[cfg(test)]
pub(crate) fn build_storage_hygiene_report_for_roots(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
) -> String {
    build_storage_hygiene_report_for_roots_mode(
        roots,
        max_depth,
        limit,
        StorageScanMode::ForensicVerified.as_str(),
    )
}

#[cfg(test)]
pub(crate) fn build_storage_hygiene_report_for_roots_mode(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: &str,
) -> String {
    match finalize_storage_report_json(build_storage_hygiene_report_with_options(
        roots,
        StorageHygieneOptions {
            max_depth,
            limit,
            mode: StorageScanMode::parse(mode),
            runtime: None,
            dirty_paths: Vec::new(),
        },
    )) {
        Ok(json) => json,
        Err(error) => panic!("storage hygiene report serializes: {error}"),
    }
}

mod attribution;
mod cleanup;
mod jobs;
mod models;
mod state_store;
#[cfg(test)]
mod tests;
mod treemap;
mod walk;
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
use state_store::{
    RepositoryInventoryCacheEntry, RepositoryInventoryCacheState, StorageIndexedFileRow,
    StorageScanPersistedRecord, StorageScanPersistedState, StorageScanStateStore, StorageSizeIndex,
};
use treemap::build_storage_treemap_roots;
use walk::{
    SizeWalkResult, file_access_age_days, is_cloud_storage_path, is_network_storage_path,
    scan_root, storage_item_for_indexed_row, unix_metadata_millis,
};
