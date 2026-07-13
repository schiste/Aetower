use super::*;
use miniz_oxide::inflate;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

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
pub(super) struct StorageCandidateCollector {
    limit: usize,
    heap: BinaryHeap<Reverse<RankedStorageItem>>,
    seen: u64,
}

impl StorageCandidateCollector {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            heap: BinaryHeap::new(),
            seen: 0,
        }
    }

    pub(super) fn push(&mut self, item: StorageHygieneItem) {
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

    pub(super) fn into_sorted_items(self) -> Vec<StorageHygieneItem> {
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

pub(super) fn finalize_storage_report_json(
    mut report: StorageHygieneReport,
) -> Result<String, String> {
    let serialize_started = Instant::now();
    let json = serde_json::to_string(&report).map_err(|error| error.to_string())?;
    report.diagnostics.serialize_millis = serialize_started.elapsed().as_millis() as u64;
    report.diagnostics.payload_bytes = json.len().min(u64::MAX as usize) as u64;
    refresh_storage_performance_budget(&mut report, 0, 0);
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(super) fn refresh_storage_performance_budget(
    report: &mut StorageHygieneReport,
    table_page_millis: u64,
    render_publish_millis: u64,
) {
    report.diagnostics.performance_budget = storage_performance_budget_diagnostics(
        StorageScanMode::parse(&report.scan_mode),
        report.scan_duration_millis,
        report.diagnostics.payload_bytes,
        report.diagnostics.candidate_seen_count,
        table_page_millis,
        render_publish_millis,
    );
}

pub(super) fn storage_performance_budget_diagnostics(
    mode: StorageScanMode,
    scan_duration_millis: u64,
    payload_bytes: u64,
    candidate_seen_count: u64,
    table_page_millis: u64,
    render_publish_millis: u64,
) -> StoragePerformanceBudgetDiagnostics {
    let mut notes = Vec::new();
    let mut severity = 0u8;
    let scan_latency_warn_millis = mode.scan_latency_warn_millis();
    let scan_latency_critical_millis = mode.scan_latency_critical_millis();
    let payload_warn_bytes = mode.payload_warn_bytes();
    let payload_critical_bytes = mode.payload_critical_bytes();
    if scan_duration_millis >= scan_latency_critical_millis {
        severity = severity.max(2);
        notes.push(format!(
            "scan latency exceeded critical budget for {}: {scan_duration_millis}ms >= {scan_latency_critical_millis}ms",
            mode.as_str()
        ));
    } else if scan_duration_millis >= scan_latency_warn_millis {
        severity = severity.max(1);
        notes.push(format!(
            "scan latency exceeded warning budget for {}: {scan_duration_millis}ms >= {scan_latency_warn_millis}ms",
            mode.as_str()
        ));
    }
    if payload_bytes >= payload_critical_bytes {
        severity = severity.max(2);
        notes.push(format!(
            "payload exceeded critical budget for {}: {payload_bytes} bytes >= {payload_critical_bytes} bytes",
            mode.as_str()
        ));
    } else if payload_bytes >= payload_warn_bytes {
        severity = severity.max(1);
        notes.push(format!(
            "payload exceeded warning budget for {}: {payload_bytes} bytes >= {payload_warn_bytes} bytes",
            mode.as_str()
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
    if candidate_seen_count >= 1_000_000 {
        notes.push(format!(
            "stress fixture scale: {candidate_seen_count} candidates seen"
        ));
    }
    if notes.is_empty() {
        notes.push("storage scan and UI payload stayed within current budgets".to_owned());
    }
    StoragePerformanceBudgetDiagnostics {
        status: match severity {
            0 => "ok",
            1 => "warn",
            _ => "critical",
        }
        .to_owned(),
        scan_job_latency_millis: scan_duration_millis,
        payload_bytes,
        payload_budget_bytes: payload_warn_bytes,
        table_page_millis,
        table_page_budget_millis: STORAGE_TABLE_PAGE_WARN_MILLIS,
        render_publish_millis,
        render_budget_millis: STORAGE_RENDER_WARN_MILLIS,
        notes,
    }
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

pub(super) fn build_storage_hygiene_report_with_options(
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
    // Every mode persists its findings into the shared index so instant_cached
    // readers (overview, launch repaint) re-derive from the freshest scan.
    // Deep/Forensic still walk fresh: they only skip READING cached directory
    // sizes (`StorageScanMode::serve_sizes_from_index`).
    let storage_index = StorageSizeIndex::open();
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
        options.mode.repository_inventory_time_budget(),
        options.runtime.as_ref(),
    );
    if let Some(runtime) = options.runtime.as_ref() {
        let _ = runtime.set_phase(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, None);
    }
    let report_item_limit = options.mode.report_item_limit(options.limit);
    let mut collector = StorageCandidateCollector::new(report_item_limit);
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
    let mut storage_walk_truncated = false;
    let mut storage_sizing_truncated = false;

    // The walk phase runs on its own clock: the repository/git phase above is
    // bounded separately, so a git-heavy start (forensic mode over many repos)
    // can no longer exhaust the budget before a single root is sized. Within
    // the walk phase, each root gets a fair slice of the remaining budget so
    // one huge early root cannot starve every later root.
    let walk_started = Instant::now();
    let walk_budget = options.mode.size_walk_time_budget();
    let total_roots = roots.len();
    for (root_index, root) in roots.into_iter().enumerate() {
        if walk_started.elapsed() >= walk_budget {
            storage_walk_truncated = true;
            break;
        }
        if let Some(runtime) = options.runtime.as_ref()
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&root), 0, 0, 0)
        {
            storage_walk_truncated = true;
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
        let root_deadline = root_started
            + per_root_walk_slice(
                walk_budget.saturating_sub(walk_started.elapsed()),
                total_roots.saturating_sub(root_index),
                options.mode.per_root_slice_floor(),
            );
        let root_scan = scan_root(
            &root,
            &options,
            root_deadline,
            now_millis,
            &storage_index,
            &mut collector,
            &mut metrics,
        );
        metrics.root_walk_millis = metrics
            .root_walk_millis
            .saturating_add(root_started.elapsed().as_millis() as u64);
        scanned_directory_count += root_scan.scanned_dirs;
        storage_walk_truncated |= root_scan.walk_truncated;
        storage_sizing_truncated |= root_scan.sizing_truncated;
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
    let writer_ledger = load_storage_writer_ledger_records();
    apply_measured_rebuild_costs(&mut items, &writer_ledger);
    apply_cleanup_guardrails(&mut items, now_millis);
    annotate_cleanup_items_active_holders(&mut items);
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
    let investigation = summarize_storage_investigation(&items, storage_walk_truncated);
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
    let redundancy_groups = summarize_redundancy_groups(
        &items,
        &duplicate_groups,
        &repo_footprints,
        &repository_inventory,
    );
    let agent_hygiene = summarize_agent_hygiene(&items);
    let source_coverage =
        summarize_source_coverage(&requested_roots, &scanned_roots, &skipped_roots, &items);
    let volume_states = summarize_volume_states(&requested_roots);
    let growth_insights =
        build_storage_growth_insights(&storage_index, &requested_roots, &volume_states, now_millis);
    let cold_data = build_storage_cold_data(&storage_index, &requested_roots, now_millis);
    let budget_guardrails = evaluate_budget_guardrails(
        &summary,
        &repo_footprints,
        &volume_states,
        &items,
        &growth_deltas,
    );
    let retained_count = items.len().min(u64::MAX as usize) as u64;
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
        candidate_retained_count: retained_count,
        storage_index_status: metrics.storage_index_status,
        storage_index_hits: metrics.storage_index_hits,
        storage_index_misses: metrics.storage_index_misses,
        storage_index_writes: metrics.storage_index_writes,
        native_metadata_strategy: options.mode.native_metadata_strategy().to_owned(),
        fsevents_status: "swift_cache_invalidation".to_owned(),
        lazy_git_status: !options.mode.collect_git_status(),
        top_k_retained: metrics.candidate_seen_count > retained_count,
        performance_budget: StoragePerformanceBudgetDiagnostics::default(),
    };
    let mut caveats = vec![
        "Cleanup cockpit: Aetower prepares evidence, reveal targets, verification commands, and Trash-first cleanup manifests."
            .to_owned(),
        "Sizes are bounded estimates and may omit paths that require additional permissions."
            .to_owned(),
        "Reclaimable bytes use local allocated blocks when available; zero-block cloud/sparse placeholders are counted as 0 local reclaim."
            .to_owned(),
        "Hardlinked content is deduplicated inside a sized directory, but external hardlinks and APFS clone sharing can reduce the bytes actually freed."
            .to_owned(),
        "Review candidates may be rebuildable but can still contain release artifacts or local environments."
            .to_owned(),
        "Last-access timestamps can be unavailable, coarse, or lazily updated depending on the macOS volume."
            .to_owned(),
        "Command and process-tree attribution uses indexed deltas plus optional writer ledgers from Aetower/Chau7; unmatched writers are reported as low-confidence instead of guessed."
            .to_owned(),
        "Repository inventory runs before artifact sizing so repository coverage remains available even when artifact sizing truncates."
            .to_owned(),
    ];
    if storage_sizing_truncated || items.iter().any(|item| item.size_truncated) {
        caveats.push(
            "Some large item byte estimates are partial; those rows are marked individually as partial sizes."
                .to_owned(),
        );
    }

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
        redundancy_groups,
        app_footprints,
        system_data_buckets,
        treemap_roots,
        items,
        roots: scanned_roots,
        skipped_roots,
        source_coverage,
        volume_states,
        growth_deltas,
        growth_insights,
        cold_data,
        truncated: storage_walk_truncated,
        caveats,
    }
}

/// Fair walk slice for the next root: an even share of the remaining walk
/// budget across the remaining roots (current one included), floored per mode
/// so late roots always get a usable slice. Unspent slice time rolls forward
/// automatically because the remaining budget is recomputed for every root;
/// the caller's overall-budget check keeps total walk time bounded.
pub(super) fn per_root_walk_slice(
    remaining_budget: Duration,
    remaining_roots: usize,
    floor: Duration,
) -> Duration {
    let share = remaining_budget / remaining_roots.clamp(1, u32::MAX as usize) as u32;
    share.max(floor)
}

/// Limit-independent, expensive-to-derive report sections that only change
/// when the persistent index changes: the per-repository inventory
/// intelligence (git reads for every cached repository root) and the
/// growth/cold SQL aggregations. Both are recomputed on every
/// `build_storage_hygiene_report_from_index` call, so once a deep scan grows
/// the index to hundreds of thousands of rows the launch repaint's concurrent
/// overview + indexed builds each burned the same multi-second cost. The memo
/// below caches them per index generation instead.
#[derive(Clone)]
struct IndexReportSections {
    discovered_repository_count: u64,
    repository_inventory: Vec<StorageRepositoryInventoryItem>,
    repository_inventory_coverage: Vec<RepositoryInventoryRootCoverage>,
    repository_inventory_completeness: RepositoryInventoryCompleteness,
    growth_insights: Option<StorageGrowthInsights>,
    cold_data: Option<StorageColdData>,
}

struct IndexReportSectionsMemo {
    dirty_stamp: u64,
    generation: (u64, u64),
    repository_fingerprint_key: Vec<(String, String)>,
    roots_key: Vec<String>,
    sections: IndexReportSections,
}

/// Single-entry memo guarded by one mutex that is intentionally held across
/// the compute on a miss: when the overview and indexed snapshots are
/// requested concurrently (the Storage tab launch repaint does exactly this),
/// the second caller blocks until the first finishes and then serves the
/// memoized sections instead of duplicating the work.
static INDEX_REPORT_SECTIONS_MEMO: Mutex<Option<IndexReportSectionsMemo>> = Mutex::new(None);

/// Bumped by the state store whenever index content changes in-process
/// (flushes, row deletion, repository-cache refreshes); entries memoized under
/// an older stamp stop matching. A lock-free counter (instead of clearing the
/// memo under its mutex) so state-store write paths can never deadlock
/// against a section compute that is holding the memo lock. Cross-process
/// writers are caught by the generation key instead.
static INDEX_REPORT_SECTIONS_DIRTY: AtomicU64 = AtomicU64::new(0);

pub(super) fn invalidate_index_report_sections_memo() {
    INDEX_REPORT_SECTIONS_DIRTY.fetch_add(1, AtomicOrdering::Release);
}

fn repository_fingerprint_memo_key(
    storage_index: &StorageSizeIndex,
    requested_roots: &[PathBuf],
) -> Vec<(String, String)> {
    storage_index
        .load_repository_inventory_cache(requested_roots)
        .into_keys()
        .map(|repo_root| {
            let fingerprint = repository_inventory_fingerprint(Path::new(&repo_root));
            (repo_root, fingerprint)
        })
        .collect()
}

fn compute_index_report_sections(
    storage_index: &StorageSizeIndex,
    requested_roots: &[PathBuf],
    volume_states: &[StorageVolumeState],
    started: Instant,
    now_millis: u64,
    metrics: &mut StorageScanMetrics,
) -> IndexReportSections {
    let cached_repository_entries = storage_index.load_repository_inventory_cache(requested_roots);
    let repository_roots = repository_inventory_cache_roots(&cached_repository_entries);
    let repository_inventory_coverage =
        cached_repository_inventory_coverage(requested_roots, &repository_roots);
    let completeness = repository_inventory_completeness(&repository_inventory_coverage, false);
    let not_seen_repository_roots = BTreeSet::new();
    let latest_repository_roots = BTreeMap::new();
    let repository_cache_states = repository_inventory_cache_states(
        &repository_roots,
        &latest_repository_roots,
        &cached_repository_entries,
    );
    let discovered_repository_count = repository_roots.len().min(u64::MAX as usize) as u64;
    metrics.discovered_repository_count = discovered_repository_count;
    let repository_inventory = summarize_repository_inventory(
        repository_roots,
        &not_seen_repository_roots,
        &repository_cache_states,
        started,
        StorageScanMode::InstantCached,
        metrics,
    );
    let growth_insights =
        build_storage_growth_insights(storage_index, requested_roots, volume_states, now_millis);
    let cold_data = build_storage_cold_data(storage_index, requested_roots, now_millis);
    IndexReportSections {
        discovered_repository_count,
        repository_inventory,
        repository_inventory_coverage,
        repository_inventory_completeness: completeness,
        growth_insights,
        cold_data,
    }
}

/// Serve the limit-independent sections from the per-generation memo, or
/// compute-and-store them while holding the memo lock. Returns the sections
/// plus whether they came from the memo (for diagnostics).
fn index_report_sections(
    storage_index: &StorageSizeIndex,
    requested_roots: &[PathBuf],
    volume_states: &[StorageVolumeState],
    started: Instant,
    now_millis: u64,
    metrics: &mut StorageScanMetrics,
) -> (IndexReportSections, bool) {
    let generation = storage_index.index_report_generation();
    let roots_key = requested_roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    let repository_fingerprint_key =
        repository_fingerprint_memo_key(storage_index, requested_roots);
    let mut memo = lock_or_recover(&INDEX_REPORT_SECTIONS_MEMO);
    let dirty_stamp = INDEX_REPORT_SECTIONS_DIRTY.load(AtomicOrdering::Acquire);
    if let Some(entry) = memo.as_ref()
        && entry.dirty_stamp == dirty_stamp
        && generation.is_some_and(|generation| generation == entry.generation)
        && entry.roots_key == roots_key
        && entry.repository_fingerprint_key == repository_fingerprint_key
    {
        metrics.discovered_repository_count = entry.sections.discovered_repository_count;
        return (entry.sections.clone(), true);
    }
    let sections = compute_index_report_sections(
        storage_index,
        requested_roots,
        volume_states,
        started,
        now_millis,
        metrics,
    );
    if let Some(generation) = generation {
        // Stored under the stamp read before the compute: if a writer bumped
        // the counter mid-compute, the entry stops matching on its next probe.
        *memo = Some(IndexReportSectionsMemo {
            dirty_stamp,
            generation,
            repository_fingerprint_key,
            roots_key,
            sections: sections.clone(),
        });
    }
    (sections, false)
}

pub(super) fn build_storage_hygiene_report_from_index(
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
    let volume_states = summarize_volume_states(&requested_roots);
    let (sections, sections_from_memo) = index_report_sections(
        &storage_index,
        &requested_roots,
        &volume_states,
        started,
        now_millis,
        &mut metrics,
    );
    let limit = limit.clamp(1, MAX_LIMIT);
    let rows = storage_index.load_candidate_rows(&roots, limit, &mut metrics)?;
    if rows.is_empty() && sections.discovered_repository_count == 0 {
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
    let writer_ledger = load_storage_writer_ledger_records();
    apply_measured_rebuild_costs(&mut items, &writer_ledger);
    apply_cleanup_guardrails(&mut items, now_millis);
    annotate_cleanup_items_active_holders(&mut items);
    for item in &mut items {
        item.evidence = storage_item_evidence(item);
        item.next_step = storage_item_next_step(item);
    }
    let IndexReportSections {
        discovered_repository_count: _,
        repository_inventory,
        repository_inventory_coverage,
        repository_inventory_completeness,
        growth_insights,
        cold_data,
    } = sections;

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
    let redundancy_groups = summarize_redundancy_groups(
        &items,
        &duplicate_groups,
        &repo_footprints,
        &repository_inventory,
    );
    let agent_hygiene = summarize_agent_hygiene(&items);
    let source_coverage =
        summarize_source_coverage(&requested_roots, &scanned_roots, &skipped_roots, &items);
    let budget_guardrails = evaluate_budget_guardrails(
        &summary,
        &repo_footprints,
        &volume_states,
        &items,
        &growth_deltas,
    );
    let sections_marker = if sections_from_memo {
        "+sections_memo"
    } else {
        "+sections_fresh"
    };
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
        storage_index_status: format!("snapshot:{}{sections_marker}", metrics.storage_index_status),
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
        redundancy_groups,
        app_footprints,
        system_data_buckets,
        treemap_roots,
        items,
        roots: scanned_roots,
        skipped_roots,
        source_coverage,
        volume_states,
        growth_deltas,
        growth_insights,
        cold_data,
        truncated: false,
        caveats: vec![
            "Loaded from Aetower's persistent storage index for instant display.".to_owned(),
            "Run a refresh before destructive cleanup when the displayed path changed recently."
                .to_owned(),
            "Cached reclaimable bytes use local allocated blocks; refresh for the latest APFS sparse/cloud/hardlink accounting."
                .to_owned(),
            "Growth attribution is based on indexed size deltas and optional Aetower/Chau7 writer ledger records."
                .to_owned(),
        ],
    })
}

/// Assemble the cold-data reclaim lane from the persistent index: one band for
/// rows untouched for a year or more and one "cooling" band for 90 days to a
/// year, both restricted to safe/rebuildable rows at the minimum item size and
/// hydrated through the same guardrail pass as every other item surface.
pub(super) fn build_storage_cold_data(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    now_millis: u64,
) -> Option<StorageColdData> {
    let band_definitions = [
        (
            "cold-1y",
            format!("Untouched {COLD_AFTER_DAYS}+ days"),
            COLD_AFTER_DAYS,
            None,
        ),
        (
            "cold-90d",
            format!("Untouched {COLD_COOLING_AFTER_DAYS}-{COLD_AFTER_DAYS} days"),
            COLD_COOLING_AFTER_DAYS,
            Some(COLD_AFTER_DAYS),
        ),
    ];
    let mut bands = Vec::with_capacity(band_definitions.len());
    for (id, label, min_age_days, max_age_days) in band_definitions {
        let (item_count, total_bytes, rows) = storage_index.load_cold_band(
            roots,
            min_age_days,
            max_age_days,
            now_millis,
            STORAGE_COLD_BAND_TOP_ITEMS,
        )?;
        let mut top_items = rows
            .into_iter()
            .map(|row| storage_item_for_indexed_row(row, now_millis))
            .collect::<Vec<_>>();
        apply_cleanup_guardrails(&mut top_items, now_millis);
        annotate_cleanup_items_active_holders(&mut top_items);
        for item in &mut top_items {
            item.evidence = storage_item_evidence(item);
            item.next_step = storage_item_next_step(item);
        }
        bands.push(StorageColdDataBand {
            id: id.to_owned(),
            label,
            min_age_days,
            max_age_days,
            item_count,
            total_bytes,
            top_items,
        });
    }
    Some(StorageColdData {
        bands,
        caveat: "Age uses max(accessed, modified); macOS last-access timestamps can be coarse or \
                 lazily updated. Top cold candidates are checked for visible active file holders \
                 before cleanup, but permissions can still hide processes."
            .to_owned(),
    })
}

fn annotate_cleanup_items_active_holders(items: &mut [StorageHygieneItem]) {
    let paths = items
        .iter()
        .filter(|item| cleanup_item_needs_active_holder_check(item))
        .map(|item| item.path.clone())
        .take(CLEANUP_ACTIVE_HOLDER_PATH_LIMIT)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return;
    }
    match build_resource_holders_by_files(&paths) {
        Ok(holders_by_path) => {
            let active_holders = holders_by_path
                .into_iter()
                .map(|(path, holders)| {
                    (
                        path,
                        holders
                            .into_iter()
                            .map(|holder| CleanupPathHolder {
                                pid: holder.pid,
                                command: holder.command,
                                fd: holder.fd,
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            apply_active_cleanup_holders(items, &active_holders);
        }
        Err(error) => {
            let note = format!("Active file-handle check unavailable: {error}.");
            for item in items {
                item.attribution.notes.push(note.clone());
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct CleanupPathHolder {
    pub(super) pid: u32,
    pub(super) command: String,
    pub(super) fd: String,
}

fn cleanup_item_needs_active_holder_check(item: &StorageHygieneItem) -> bool {
    item.cleanup_allowed
        && item.default_cleanup_action == "trash"
        && item.cleanup_blockers.is_empty()
        && item.cleanup_tier != "risky"
}

pub(super) fn apply_active_cleanup_holders(
    items: &mut [StorageHygieneItem],
    holders_by_path: &BTreeMap<String, Vec<CleanupPathHolder>>,
) {
    for item in items {
        let Some(holders) = holders_by_path
            .get(&item.path)
            .filter(|holders| !holders.is_empty())
        else {
            continue;
        };
        let summary = summarize_cleanup_path_holders(holders);
        block_cleanup(
            item,
            &format!("Active file handle detected: {summary} currently holds this path."),
        );
        item.default_cleanup_action = "manual_review".to_owned();
        item.attribution.notes.push(format!(
            "Active file-handle check matched {} process{}: {summary}. Stop or close the holder, then rescan before staging cleanup.",
            holders.len(),
            if holders.len() == 1 { "" } else { "es" }
        ));
    }
}

fn summarize_cleanup_path_holders(holders: &[CleanupPathHolder]) -> String {
    let mut parts = holders
        .iter()
        .take(3)
        .map(|holder| format!("{} pid {} fd {}", holder.command, holder.pid, holder.fd))
        .collect::<Vec<_>>();
    if holders.len() > parts.len() {
        parts.push(format!("+{} more", holders.len() - parts.len()));
    }
    parts.join(", ")
}

fn build_storage_growth_insights(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    volume_states: &[StorageVolumeState],
    now_millis: u64,
) -> Option<StorageGrowthInsights> {
    storage_index.load_growth_insights(
        roots,
        volume_states,
        now_millis,
        STORAGE_GROWTH_INSIGHTS_WINDOW_DAYS,
    )
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

pub(super) fn storage_item_evidence(item: &StorageHygieneItem) -> Vec<String> {
    let mut evidence = Vec::new();
    evidence.push(format!(
        "Matched {} rule; storage role is {}.",
        item.kind,
        storage_role_label(&item.storage_role)
    ));
    evidence.push(format!(
        "Semantic category is {}; taxonomy source is {}; rebuildability is {}.",
        item.semantic_category, item.taxonomy_source, item.rebuildability
    ));
    if !item.manifest_evidence.is_empty() {
        evidence.push(format!(
            "Manifest-proven rebuildability evidence: {}.",
            item.manifest_evidence.join(", ")
        ));
    }
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
            "Hardlink count reached {}; Aetower deduplicates repeated links inside a sized directory, but external links can still reduce actual reclaim.",
            item.hardlink_count
        ));
    }
    if item.cloud_placeholder {
        evidence.push(
            "This path has zero local allocated blocks; Aetower treats its local reclaim estimate as 0 bytes."
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

pub(super) fn storage_local_reclaimable_bytes(
    logical_bytes: u64,
    physical_bytes: u64,
    cloud_placeholder: bool,
) -> u64 {
    if logical_bytes == 0 || cloud_placeholder {
        0
    } else if physical_bytes > 0 {
        physical_bytes
    } else {
        logical_bytes
    }
}

pub(super) fn storage_byte_accounting_label(logical_bytes: u64, physical_bytes: u64) -> String {
    if logical_bytes == 0 && physical_bytes == 0 {
        "empty path".to_owned()
    } else if physical_bytes == 0 {
        "zero local allocated blocks".to_owned()
    } else if physical_bytes < logical_bytes {
        "APFS physical blocks".to_owned()
    } else if physical_bytes > logical_bytes {
        "allocated blocks including filesystem overhead".to_owned()
    } else {
        "logical and physical bytes match".to_owned()
    }
}

pub(super) fn storage_item_next_step(item: &StorageHygieneItem) -> String {
    if item.size_truncated {
        return "Reveal the path and run the copied `du` command when the machine is idle to confirm true size.".to_owned();
    }
    if item.cloud_placeholder {
        return "Manual review: reveal in Finder to confirm whether the file is cloud-only or sparse; Aetower will not stage it because no local blocks are proven reclaimable.".to_owned();
    }
    if item.has_hardlinks {
        return "Manual review: reveal the path and inspect hardlink ownership first; Aetower blocks automatic cleanup because another link may retain the blocks.".to_owned();
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
    let mut same_size_groups = BTreeMap::<u64, Vec<&StorageHygieneItem>>::new();
    for item in items {
        let path = Path::new(&item.path);
        if !path.is_file() || item.size_bytes < MIN_ITEM_BYTES {
            continue;
        }
        same_size_groups
            .entry(item.size_bytes)
            .or_default()
            .push(item);
    }

    let mut groups = Vec::new();
    for (size_bytes, candidates) in same_size_groups {
        if candidates.len() < 2 {
            continue;
        }

        let mut by_partial_hash = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
        for item in candidates {
            if let Some(partial_hash) =
                file_partial_content_hash(Path::new(&item.path), item.size_bytes)
            {
                by_partial_hash.entry(partial_hash).or_default().push(item);
            }
        }

        for (partial_hash, partial_items) in by_partial_hash {
            if partial_items.len() < 2 {
                continue;
            }
            let partial_key = format!("size:{size_bytes}|partial:{partial_hash}");
            let can_hash = partial_items
                .iter()
                .all(|item| item.size_bytes <= DUPLICATE_FULL_HASH_MAX_BYTES);
            if !can_hash {
                groups.push(duplicate_group_from_items(
                    format!("partial|{partial_key}"),
                    partial_key,
                    false,
                    58,
                    partial_items,
                ));
                continue;
            }

            let mut by_hash = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
            for item in partial_items {
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
        }
    }
    append_image_similarity_groups(items, &mut groups);
    append_text_similarity_groups(items, &mut groups);
    append_video_similarity_groups(items, &mut groups);
    append_binary_similarity_groups(items, &mut groups);

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

fn append_image_similarity_groups(
    items: &[StorageHygieneItem],
    groups: &mut Vec<StorageDuplicateGroup>,
) {
    let exact_duplicate_paths = groups
        .iter()
        .filter(|group| group.confirmed)
        .flat_map(|group| group.paths.iter().map(|item| item.path.clone()))
        .collect::<BTreeSet<_>>();
    let mut candidates = items
        .iter()
        .filter(|item| {
            let path = Path::new(&item.path);
            path.is_file()
                && !exact_duplicate_paths.contains(&item.path)
                && item.size_bytes >= MIN_ITEM_BYTES
                && item.size_bytes <= IMAGE_SIMILARITY_HASH_MAX_BYTES
                && is_similarity_image_path(path)
        })
        .filter_map(|item| image_average_hash(Path::new(&item.path)).map(|hash| (hash, item)))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.1
            .path
            .cmp(&right.1.path)
            .then_with(|| left.0.cmp(&right.0))
    });

    let mut assigned = BTreeSet::<String>::new();
    for (hash, item) in &candidates {
        if assigned.contains(&item.path) {
            continue;
        }
        let mut similar_items = vec![*item];
        for (candidate_hash, candidate) in &candidates {
            if candidate.path == item.path || assigned.contains(&candidate.path) {
                continue;
            }
            if image_hash_hamming_distance(*hash, *candidate_hash)
                <= IMAGE_SIMILARITY_HAMMING_THRESHOLD
            {
                similar_items.push(*candidate);
            }
        }
        if similar_items.len() < 2 {
            continue;
        }
        for similar in &similar_items {
            assigned.insert(similar.path.clone());
        }
        groups.push(similar_image_group_from_items(
            format!("image-ahash|{hash:016x}"),
            *hash,
            similar_items,
        ));
    }
}

fn append_text_similarity_groups(
    items: &[StorageHygieneItem],
    groups: &mut Vec<StorageDuplicateGroup>,
) {
    let exact_duplicate_paths = groups
        .iter()
        .filter(|group| group.confirmed)
        .flat_map(|group| group.paths.iter().map(|item| item.path.clone()))
        .collect::<BTreeSet<_>>();
    let mut candidates = items
        .iter()
        .filter(|item| {
            let path = Path::new(&item.path);
            path.is_file()
                && !exact_duplicate_paths.contains(&item.path)
                && item.size_bytes >= MIN_ITEM_BYTES
                && item.size_bytes <= TEXT_SIMILARITY_HASH_MAX_BYTES
                && (is_similarity_text_path(path) || is_similarity_document_path(path))
        })
        .filter_map(|item| {
            text_simhash(Path::new(&item.path)).map(|(hash, token_count)| TextSimilarityCandidate {
                hash,
                token_count,
                item,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.item
            .path
            .cmp(&right.item.path)
            .then_with(|| left.hash.cmp(&right.hash))
    });

    let mut assigned = BTreeSet::<String>::new();
    for candidate in &candidates {
        if assigned.contains(&candidate.item.path) {
            continue;
        }
        let mut similar_items = vec![candidate.item];
        for other in &candidates {
            if other.item.path == candidate.item.path || assigned.contains(&other.item.path) {
                continue;
            }
            if text_similarity_token_counts_compatible(candidate.token_count, other.token_count)
                && text_hash_hamming_distance(candidate.hash, other.hash)
                    <= TEXT_SIMILARITY_HAMMING_THRESHOLD
            {
                similar_items.push(other.item);
            }
        }
        if similar_items.len() < 2 {
            continue;
        }
        for similar in &similar_items {
            assigned.insert(similar.path.clone());
        }
        groups.push(similar_text_group_from_items(
            format!("text-simhash|{:016x}", candidate.hash),
            candidate.hash,
            candidate.token_count,
            similar_items,
        ));
    }
}

#[derive(Clone, Copy)]
struct TextSimilarityCandidate<'a> {
    hash: u64,
    token_count: usize,
    item: &'a StorageHygieneItem,
}

fn append_video_similarity_groups(
    items: &[StorageHygieneItem],
    groups: &mut Vec<StorageDuplicateGroup>,
) {
    let exact_duplicate_paths = groups
        .iter()
        .filter(|group| group.confirmed)
        .flat_map(|group| group.paths.iter().map(|item| item.path.clone()))
        .collect::<BTreeSet<_>>();
    let mut candidates = items
        .iter()
        .filter(|item| {
            let path = Path::new(&item.path);
            path.is_file()
                && !exact_duplicate_paths.contains(&item.path)
                && item.size_bytes >= MIN_ITEM_BYTES
                && is_similarity_video_path(path)
        })
        .filter_map(|item| {
            video_signature(Path::new(&item.path), item.size_bytes)
                .map(|signature| VideoSimilarityCandidate { signature, item })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.item
            .path
            .cmp(&right.item.path)
            .then_with(|| left.signature.sample_hash.cmp(&right.signature.sample_hash))
    });

    let mut assigned = BTreeSet::<String>::new();
    for candidate in &candidates {
        if assigned.contains(&candidate.item.path) {
            continue;
        }
        let mut similar_items = vec![candidate.item];
        for other in &candidates {
            if other.item.path == candidate.item.path || assigned.contains(&other.item.path) {
                continue;
            }
            if candidate.signature.is_compatible_with(&other.signature) {
                similar_items.push(other.item);
            }
        }
        if similar_items.len() < 2 {
            continue;
        }
        for similar in &similar_items {
            assigned.insert(similar.path.clone());
        }
        groups.push(similar_video_group_from_items(
            format!(
                "video-signature|{}|{:016x}",
                candidate.signature.codec_string(),
                candidate.signature.sample_hash
            ),
            &candidate.signature,
            similar_items,
        ));
    }
}

#[derive(Clone, Copy)]
struct VideoSimilarityCandidate<'a> {
    signature: VideoSimilaritySignature,
    item: &'a StorageHygieneItem,
}

#[derive(Clone, Copy)]
struct VideoSimilaritySignature {
    duration_millis: u64,
    width: u32,
    height: u32,
    codec: [u8; 4],
    sample_hash: u64,
    size_bytes: u64,
}

impl VideoSimilaritySignature {
    fn is_compatible_with(&self, other: &Self) -> bool {
        self.codec == other.codec
            && self.sample_hash == other.sample_hash
            && self.duration_millis.abs_diff(other.duration_millis)
                <= VIDEO_SIMILARITY_DURATION_TOLERANCE_MS
            && self.width.abs_diff(other.width) <= VIDEO_SIMILARITY_DIMENSION_TOLERANCE_PX
            && self.height.abs_diff(other.height) <= VIDEO_SIMILARITY_DIMENSION_TOLERANCE_PX
            && video_size_ratio_compatible(self.size_bytes, other.size_bytes)
    }

    fn codec_string(&self) -> String {
        String::from_utf8_lossy(&self.codec).into_owned()
    }
}

fn append_binary_similarity_groups(
    items: &[StorageHygieneItem],
    groups: &mut Vec<StorageDuplicateGroup>,
) {
    let exact_duplicate_paths = groups
        .iter()
        .filter(|group| group.confirmed)
        .flat_map(|group| group.paths.iter().map(|item| item.path.clone()))
        .collect::<BTreeSet<_>>();
    let mut candidates = items
        .iter()
        .filter(|item| {
            let path = Path::new(&item.path);
            path.is_file()
                && !exact_duplicate_paths.contains(&item.path)
                && item.size_bytes >= MIN_ITEM_BYTES
                && item.size_bytes <= BINARY_SIMILARITY_HASH_MAX_BYTES
                && is_similarity_binary_path(path)
        })
        .filter_map(|item| {
            binary_similarity_signature(Path::new(&item.path), item.size_bytes)
                .map(|signature| BinarySimilarityCandidate { signature, item })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.item.path.cmp(&right.item.path).then_with(|| {
            left.signature
                .min_feature()
                .cmp(&right.signature.min_feature())
        })
    });

    let mut assigned = BTreeSet::<String>::new();
    for candidate in &candidates {
        if assigned.contains(&candidate.item.path) {
            continue;
        }
        let mut similar_items = vec![candidate.item];
        let mut best_overlap = BinarySimilarityOverlap::default();
        for other in &candidates {
            if other.item.path == candidate.item.path || assigned.contains(&other.item.path) {
                continue;
            }
            let overlap = candidate.signature.overlap_with(&other.signature);
            if overlap.is_compatible() {
                best_overlap = best_overlap.max(overlap);
                similar_items.push(other.item);
            }
        }
        if similar_items.len() < 2 {
            continue;
        }
        for similar in &similar_items {
            assigned.insert(similar.path.clone());
        }
        groups.push(similar_binary_group_from_items(
            format!(
                "binary-cdc|{:016x}",
                candidate.signature.min_feature().unwrap_or_default()
            ),
            &candidate.signature,
            best_overlap,
            similar_items,
        ));
    }
}

#[derive(Clone)]
struct BinarySimilarityCandidate<'a> {
    signature: BinarySimilaritySignature,
    item: &'a StorageHygieneItem,
}

#[derive(Clone)]
struct BinarySimilaritySignature {
    features: BTreeSet<u64>,
    size_bytes: u64,
}

impl BinarySimilaritySignature {
    fn min_feature(&self) -> Option<u64> {
        self.features.iter().next().copied()
    }

    fn overlap_with(&self, other: &Self) -> BinarySimilarityOverlap {
        if !binary_size_ratio_compatible(self.size_bytes, other.size_bytes) {
            return BinarySimilarityOverlap::default();
        }
        let shared = self.features.intersection(&other.features).count();
        let union = self.features.union(&other.features).count();
        let jaccard_percent = if union == 0 {
            0
        } else {
            shared.saturating_mul(100) / union
        };
        BinarySimilarityOverlap {
            shared_features: shared,
            jaccard_percent,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct BinarySimilarityOverlap {
    shared_features: usize,
    jaccard_percent: usize,
}

impl BinarySimilarityOverlap {
    fn is_compatible(self) -> bool {
        self.shared_features >= BINARY_SIMILARITY_MIN_SHARED_FEATURES
            && self.jaccard_percent >= BINARY_SIMILARITY_MIN_JACCARD_PERCENT as usize
    }

    fn max(self, other: Self) -> Self {
        if other.jaccard_percent > self.jaccard_percent
            || (other.jaccard_percent == self.jaccard_percent
                && other.shared_features > self.shared_features)
        {
            other
        } else {
            self
        }
    }
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
    let actionability = if confirmed {
        StorageDuplicateActionability::CleanableExact
    } else {
        StorageDuplicateActionability::ReviewOnly
    };

    StorageDuplicateGroup {
        id,
        candidate_key,
        detector_kind: if confirmed {
            StorageDuplicateDetectorKind::Exact
        } else {
            StorageDuplicateDetectorKind::BinarySimilarity
        },
        actionability,
        confidence_band: if confirmed {
            StorageDuplicateConfidenceBand::Confirmed
        } else {
            confidence_band_for_similarity_score(confidence_score)
        },
        actions: duplicate_group_action_projection(actionability, paths.len()),
        confirmed,
        confidence_score,
        file_count: items.len(),
        total_bytes,
        reclaimable_bytes: total_bytes.saturating_sub(keep_bytes),
        paths,
        recommendation: if confirmed {
            "Confirmed byte-identical files. Keep the canonical copy, Quick Look samples, then stage only deliberate duplicates.".to_owned()
        } else {
            "Potentially similar files by same-size and partial-content hash. Quick Look samples and run a deeper check before deleting.".to_owned()
        },
        caveat: if confirmed {
            "Full content hashing was performed only after same-size and partial-content hashing identified candidates.".to_owned()
        } else {
            format!(
                "Files share size and edge-content fingerprints but exceed the {} per-file full-hash cap, so this is review-only.",
                human_bytes(DUPLICATE_FULL_HASH_MAX_BYTES)
            )
        },
    }
}

fn similar_image_group_from_items(
    id: String,
    hash: u64,
    items: Vec<&StorageHygieneItem>,
) -> StorageDuplicateGroup {
    review_similarity_group_from_items(
        id,
        format!("image-ahash:{hash:016x}:hamming<={IMAGE_SIMILARITY_HAMMING_THRESHOLD}"),
        StorageDuplicateDetectorKind::ImageSimilarity,
        82,
        "Potentially similar images by perceptual thumbnail hash. Quick Look side by side before staging any cleanup.",
        "Image similarity uses an 8x8 average hash from decoded PNG/JPEG thumbnails. It is intentionally review-only and can miss crops, edits, or HEIC-only photos.",
        items,
    )
}

fn similar_text_group_from_items(
    id: String,
    hash: u64,
    token_count: usize,
    items: Vec<&StorageHygieneItem>,
) -> StorageDuplicateGroup {
    let detector_kind = if items
        .iter()
        .any(|item| is_document_similarity_path(&item.path))
    {
        StorageDuplicateDetectorKind::DocumentSimilarity
    } else {
        StorageDuplicateDetectorKind::TextSimilarity
    };
    review_similarity_group_from_items(
        id,
        format!(
            "text-simhash:{hash:016x}:tokens~{token_count}:hamming<={TEXT_SIMILARITY_HAMMING_THRESHOLD}"
        ),
        detector_kind,
        76,
        "Potentially similar text, code, log, PDF, or Office documents by normalized extracted-text SimHash. Diff or Quick Look side by side before staging cleanup.",
        "Text/document similarity removes formatting, hashes token shingles, and reads only bounded text. It is review-only and can miss reordered, scanned, generated, appended, or image-only content.",
        items,
    )
}

fn similar_video_group_from_items(
    id: String,
    signature: &VideoSimilaritySignature,
    items: Vec<&StorageHygieneItem>,
) -> StorageDuplicateGroup {
    review_similarity_group_from_items(
        id,
        format!(
            "video-signature:{}:{}x{}:duration~{}ms:size±{}%:sample:{:016x}",
            signature.codec_string(),
            signature.width,
            signature.height,
            signature.duration_millis,
            VIDEO_SIMILARITY_SIZE_RATIO_PERCENT,
            signature.sample_hash
        ),
        StorageDuplicateDetectorKind::VideoSimilarity,
        70,
        "Potentially similar videos by cheap container metadata and sampled media-byte fingerprints. Open side by side before staging cleanup.",
        "Video similarity compares duration, resolution, codec, size ratio, and bounded media payload samples without decoding full video frames. True sampled-frame perceptual hashes belong in Deep/Forensic mode.",
        items,
    )
}

fn similar_binary_group_from_items(
    id: String,
    signature: &BinarySimilaritySignature,
    overlap: BinarySimilarityOverlap,
    items: Vec<&StorageHygieneItem>,
) -> StorageDuplicateGroup {
    review_similarity_group_from_items(
        id,
        format!(
            "binary-cdc:features~{}:shared>={}:jaccard~{}%",
            signature.features.len(),
            overlap.shared_features,
            overlap.jaccard_percent
        ),
        StorageDuplicateDetectorKind::BinarySimilarity,
        52,
        "Potentially related binary artifacts by content-defined chunk fingerprints. Compare provenance and tool outputs before staging cleanup.",
        "Generic binary similarity is lower-confidence than exact, media, or text matching. It samples bounded file regions and compares fuzzy chunk fingerprints, so false positives are possible.",
        items,
    )
}

fn review_similarity_group_from_items(
    id: String,
    candidate_key: String,
    detector_kind: StorageDuplicateDetectorKind,
    confidence_score: u8,
    recommendation: &str,
    caveat: &str,
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
        detector_kind,
        actionability: StorageDuplicateActionability::ReviewOnly,
        confidence_band: confidence_band_for_similarity_score(confidence_score),
        actions: duplicate_group_action_projection(
            StorageDuplicateActionability::ReviewOnly,
            paths.len(),
        ),
        confirmed: false,
        confidence_score,
        file_count: items.len(),
        total_bytes,
        reclaimable_bytes: total_bytes.saturating_sub(keep_bytes),
        paths,
        recommendation: recommendation.to_owned(),
        caveat: caveat.to_owned(),
    }
}

fn duplicate_group_action_projection(
    actionability: StorageDuplicateActionability,
    item_count: usize,
) -> StorageSimilarityActionProjection {
    let can_stage_cleanup =
        actionability == StorageDuplicateActionability::CleanableExact && item_count >= 2;
    let block_reason = if can_stage_cleanup {
        None
    } else if item_count < 2 {
        Some("Cleanup staging requires at least two files in the group.".to_owned())
    } else {
        Some(
            "Similarity detector output is review-only; automatic cleanup staging is disabled."
                .to_owned(),
        )
    };
    StorageSimilarityActionProjection {
        can_reveal: item_count > 0,
        can_quick_look: item_count > 0,
        can_stage_cleanup,
        requires_manual_review: true,
        block_reason,
    }
}

fn redundancy_group_action_projection(
    item_count: usize,
    block_reason: impl Into<String>,
) -> StorageSimilarityActionProjection {
    StorageSimilarityActionProjection {
        can_reveal: item_count > 0,
        can_quick_look: item_count > 0,
        can_stage_cleanup: false,
        requires_manual_review: true,
        block_reason: Some(block_reason.into()),
    }
}

fn confidence_band_for_similarity_score(score: u8) -> StorageDuplicateConfidenceBand {
    if score >= 80 {
        StorageDuplicateConfidenceBand::High
    } else if score >= 60 {
        StorageDuplicateConfidenceBand::Medium
    } else {
        StorageDuplicateConfidenceBand::Low
    }
}

fn is_document_similarity_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("pdf" | "doc" | "docx" | "ppt" | "pptx" | "xls" | "xlsx" | "rtf" | "odt")
    )
}

fn summarize_redundancy_groups(
    items: &[StorageHygieneItem],
    duplicate_groups: &[StorageDuplicateGroup],
    repo_footprints: &[StorageRepoFootprint],
    repository_inventory: &[StorageRepositoryInventoryItem],
) -> Vec<StorageRedundancyGroup> {
    let mut groups = duplicate_groups
        .iter()
        .map(redundancy_group_from_duplicate_group)
        .collect::<Vec<_>>();

    append_shared_block_redundancy(&mut groups, items);
    append_package_store_redundancy(&mut groups, items);
    append_generated_output_redundancy(&mut groups, items);
    append_git_clone_redundancy(&mut groups, repo_footprints, repository_inventory);

    groups.sort_by(|left, right| {
        right
            .reclaimable_bytes
            .cmp(&left.reclaimable_bytes)
            .then_with(|| right.total_bytes.cmp(&left.total_bytes))
            .then_with(|| right.confidence_score.cmp(&left.confidence_score))
            .then_with(|| left.id.cmp(&right.id))
    });
    groups.truncate(REDUNDANCY_GROUP_LIMIT);
    groups
}

fn redundancy_group_from_duplicate_group(group: &StorageDuplicateGroup) -> StorageRedundancyGroup {
    StorageRedundancyGroup {
        id: format!("byte-duplicates|{}", group.id),
        redundancy_class: "byte-duplicates".to_owned(),
        title: if group.confirmed {
            "Byte-identical duplicate files".to_owned()
        } else {
            "Potential duplicate files".to_owned()
        },
        total_bytes: group.total_bytes,
        reclaimable_bytes: group.reclaimable_bytes,
        item_count: group.file_count,
        confidence_score: group.confidence_score,
        safety: group_safety_from_duplicate_items(&group.paths),
        recommendation: group.recommendation.clone(),
        caveat: group.caveat.clone(),
        evidence: vec![format!("Duplicate candidate key: {}", group.candidate_key)],
        actions: group.actions.clone(),
        items: group
            .paths
            .iter()
            .map(|item| StorageRedundancyItem {
                path: item.path.clone(),
                display_name: item.display_name.clone(),
                kind: "duplicate-file".to_owned(),
                size_bytes: item.size_bytes,
                logical_bytes: item.size_bytes,
                physical_bytes: item.size_bytes,
                cleanup_tier: item.cleanup_tier.clone(),
                safety: item.safety.clone(),
                role: "duplicate-candidate".to_owned(),
            })
            .collect(),
    }
}

fn append_shared_block_redundancy(
    groups: &mut Vec<StorageRedundancyGroup>,
    items: &[StorageHygieneItem],
) {
    let candidates = items
        .iter()
        .filter(|item| {
            item.sparse_or_shared
                && !item.cloud_placeholder
                && item.logical_bytes > item.physical_bytes
        })
        .collect::<Vec<_>>();
    if candidates.len() < 2 {
        return;
    }

    let logical_bytes = sum_logical_bytes(&candidates);
    let physical_bytes = sum_physical_bytes(&candidates);
    let item_count = candidates.len();
    groups.push(StorageRedundancyGroup {
        id: "shared-block-candidates|apfs".to_owned(),
        redundancy_class: "shared-block-candidates".to_owned(),
        title: "APFS clone/sparse shared-block candidates".to_owned(),
        total_bytes: logical_bytes,
        reclaimable_bytes: physical_bytes,
        item_count,
        confidence_score: 56,
        safety: group_safety_from_items(&candidates),
        recommendation: "Treat these as shared-block candidates: inspect physical bytes before cleanup and do not assume logical size equals reclaimable space.".to_owned(),
        caveat: "Aetower can see logical bytes exceed allocated blocks, but exact APFS clone lineage still requires a deeper filesystem ownership collector.".to_owned(),
        evidence: vec![
            format!("Logical size: {}", human_bytes(logical_bytes)),
            format!("Allocated local bytes: {}", human_bytes(physical_bytes)),
            "Physical bytes below logical bytes can mean APFS clones, compression, sparse files, or partial materialization.".to_owned(),
        ],
        actions: redundancy_group_action_projection(
            item_count,
            "Shared-block and sparse allocation candidates require manual filesystem review before cleanup.",
        ),
        items: candidates
            .into_iter()
            .take(8)
            .map(|item| redundancy_item_from_storage_item(item, "shared-block-candidate"))
            .collect(),
    });
}

fn append_package_store_redundancy(
    groups: &mut Vec<StorageRedundancyGroup>,
    items: &[StorageHygieneItem],
) {
    let mut by_kind = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        if package_store_label(&item.kind).is_some() {
            by_kind.entry(item.kind.clone()).or_default().push(item);
        }
    }

    for (kind, mut candidates) in by_kind {
        if candidates.len() < 2 {
            continue;
        }
        candidates.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        let total_bytes = sum_size_bytes(&candidates);
        let label = package_store_label(&kind).unwrap_or("Package store");
        let item_count = candidates.len();
        groups.push(StorageRedundancyGroup {
            id: format!("package-store-overlap|{kind}"),
            redundancy_class: "package-store-overlap".to_owned(),
            title: format!("{label} exists in multiple locations"),
            total_bytes,
            reclaimable_bytes: total_bytes,
            item_count,
            confidence_score: 62,
            safety: group_safety_from_items(&candidates),
            recommendation: "Review whether these package stores are all needed. Prefer tool-native cache cleanup and keep lockfiles before removing expensive stores.".to_owned(),
            caveat: "Multiple stores are not proof of duplicate package objects; this flags redundant package-manager storage surfaces for consolidation or targeted cleanup.".to_owned(),
            evidence: vec![
                format!("{item_count} package-store paths found."),
                format!("Store kind: {kind}"),
            ],
            actions: redundancy_group_action_projection(
                item_count,
                "Package-store overlap is review-only; use tool-native cleanup after confirming ownership.",
            ),
            items: candidates
                .into_iter()
                .take(8)
                .map(|item| redundancy_item_from_storage_item(item, "package-store"))
                .collect(),
        });
    }
}

fn append_generated_output_redundancy(
    groups: &mut Vec<StorageRedundancyGroup>,
    items: &[StorageHygieneItem],
) {
    let mut by_kind = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        if generated_output_label(&item.kind).is_some() {
            by_kind.entry(item.kind.clone()).or_default().push(item);
        }
    }

    for (kind, mut candidates) in by_kind {
        if candidates.len() < 2 {
            continue;
        }
        candidates.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        let total_bytes = sum_size_bytes(&candidates);
        let label = generated_output_label(&kind).unwrap_or("Generated output");
        let item_count = candidates.len();
        groups.push(StorageRedundancyGroup {
            id: format!("generated-output-equivalence|{kind}"),
            redundancy_class: "generated-output-equivalence".to_owned(),
            title: format!("Equivalent {label} outputs"),
            total_bytes,
            reclaimable_bytes: total_bytes,
            item_count,
            confidence_score: generated_output_confidence(&kind),
            safety: group_safety_from_items(&candidates),
            recommendation: "These outputs are equivalent by tool/class rather than byte-identical. Clean with the owning tool once builds, tests, and agents are idle.".to_owned(),
            caveat: "Equivalent generated outputs may come from different repos, branches, or tool versions; Aetower does not treat them as interchangeable source data.".to_owned(),
            evidence: vec![
                format!("{item_count} generated-output paths found."),
                format!("Generated output kind: {kind}"),
            ],
            actions: redundancy_group_action_projection(
                item_count,
                "Generated outputs are equivalent by class, not file identity; review the owning workflow first.",
            ),
            items: candidates
                .into_iter()
                .take(8)
                .map(|item| redundancy_item_from_storage_item(item, "generated-output"))
                .collect(),
        });
    }
}

fn append_git_clone_redundancy(
    groups: &mut Vec<StorageRedundancyGroup>,
    repo_footprints: &[StorageRepoFootprint],
    repository_inventory: &[StorageRepositoryInventoryItem],
) {
    let footprint_by_root = repo_footprints
        .iter()
        .map(|footprint| (footprint.repo_root.as_str(), footprint))
        .collect::<BTreeMap<_, _>>();
    let mut by_remote = BTreeMap::<String, Vec<&StorageRepositoryInventoryItem>>::new();
    for repository in repository_inventory {
        if let Some(remote_key) = repository.git_remote_key.as_deref() {
            by_remote
                .entry(remote_key.to_owned())
                .or_default()
                .push(repository);
        }
    }

    for (remote_key, mut repositories) in by_remote {
        if repositories.len() < 2 {
            continue;
        }
        repositories.sort_by(|left, right| left.repo_root.cmp(&right.repo_root));
        let total_bytes = repositories.iter().fold(0u64, |total, repository| {
            total.saturating_add(
                footprint_by_root
                    .get(repository.repo_root.as_str())
                    .map(|footprint| footprint.current_size_bytes)
                    .unwrap_or(0),
            )
        });
        let keep_bytes = repositories
            .iter()
            .filter_map(|repository| footprint_by_root.get(repository.repo_root.as_str()))
            .map(|footprint| footprint.current_size_bytes)
            .max()
            .unwrap_or(0);
        let dirty_count = repositories
            .iter()
            .filter(|repository| {
                !matches!(
                    repository.git_dirty_status.as_str(),
                    "clean" | "not_checked_lazy"
                )
            })
            .count();
        let item_count = repositories.len();
        groups.push(StorageRedundancyGroup {
            id: format!("git-remote-clones|{remote_key}"),
            redundancy_class: "git-remote-clones".to_owned(),
            title: "Multiple checkouts of the same Git remote".to_owned(),
            total_bytes,
            reclaimable_bytes: total_bytes.saturating_sub(keep_bytes),
            item_count,
            confidence_score: if dirty_count == 0 { 84 } else { 68 },
            safety: if dirty_count == 0 { "review" } else { "risky" }.to_owned(),
            recommendation: "Pick a canonical checkout, compare branch/HEAD/dirty state, then archive or Trash stale duplicate clones only after review.".to_owned(),
            caveat: "Same remote does not mean the working trees are equivalent; branches, unpushed commits, ignored files, and local artifacts can differ.".to_owned(),
            evidence: vec![
                format!("Remote key: {remote_key}"),
                format!("{item_count} checkout roots found."),
                format!("{dirty_count} checkout(s) have dirty status that needs review."),
            ],
            actions: redundancy_group_action_projection(
                item_count,
                "Git checkouts can differ by branch, commit, dirty state, and ignored files; choose a canonical checkout manually.",
            ),
            items: repositories
                .into_iter()
                .take(8)
                .map(|repository| {
                    let footprint = footprint_by_root.get(repository.repo_root.as_str());
                    let size_bytes = footprint
                        .map(|footprint| footprint.current_size_bytes)
                        .unwrap_or(0);
                    StorageRedundancyItem {
                        path: repository.repo_root.clone(),
                        display_name: repository.repo_name.clone(),
                        kind: "git-clone".to_owned(),
                        size_bytes,
                        logical_bytes: size_bytes,
                        physical_bytes: size_bytes,
                        cleanup_tier: "review".to_owned(),
                        safety: if matches!(
                            repository.git_dirty_status.as_str(),
                            "clean" | "not_checked_lazy"
                        ) {
                            "review"
                        } else {
                            "risky"
                        }
                        .to_owned(),
                        role: "remote-clone".to_owned(),
                    }
                })
                .collect(),
        });
    }
}

fn package_store_label(kind: &str) -> Option<&'static str> {
    match kind {
        "npm-cache" => Some("npm cache"),
        "pnpm-store" => Some("pnpm store"),
        "yarn-cache" => Some("Yarn cache"),
        "pip-cache" => Some("pip cache"),
        "uv-cache" => Some("uv cache"),
        "homebrew-cache" => Some("Homebrew cache"),
        "gradle-cache" => Some("Gradle cache"),
        "maven-repository" => Some("Maven repository"),
        "go-build-cache" => Some("Go build cache"),
        "xcode-source-packages" => Some("Xcode SourcePackages cache"),
        _ => None,
    }
}

fn generated_output_label(kind: &str) -> Option<&'static str> {
    match kind {
        "rust-build" => Some("Cargo target"),
        "swift-build" => Some("SwiftPM .build"),
        "xcode-derived-data" => Some("Xcode DerivedData"),
        "xcode-module-cache" => Some("Xcode module cache"),
        "next-build" => Some("Next.js build"),
        "next-cache" => Some("Next.js cache"),
        "frontend-cache" => Some("frontend cache"),
        "python-cache" => Some("Python cache"),
        "coverage-output" => Some("coverage"),
        "test-output" => Some("test output"),
        "temporary-output" => Some("temporary output"),
        "build-output" => Some("generic build output"),
        _ => None,
    }
}

fn generated_output_confidence(kind: &str) -> u8 {
    match kind {
        "build-output" | "temporary-output" => 58,
        "coverage-output" | "test-output" => 86,
        _ => 74,
    }
}

fn redundancy_item_from_storage_item(
    item: &StorageHygieneItem,
    role: &str,
) -> StorageRedundancyItem {
    StorageRedundancyItem {
        path: item.path.clone(),
        display_name: item.display_name.clone(),
        kind: item.kind.clone(),
        size_bytes: item.size_bytes,
        logical_bytes: item.logical_bytes,
        physical_bytes: item.physical_bytes,
        cleanup_tier: item.cleanup_tier.clone(),
        safety: item.safety.clone(),
        role: role.to_owned(),
    }
}

fn group_safety_from_items(items: &[&StorageHygieneItem]) -> String {
    if items.iter().any(|item| item.cleanup_tier == "risky") {
        "risky"
    } else if items.iter().any(|item| item.safety != "safe") {
        "review"
    } else {
        "safe"
    }
    .to_owned()
}

fn group_safety_from_duplicate_items(items: &[StorageDuplicateItem]) -> String {
    if items.iter().any(|item| item.cleanup_tier == "risky") {
        "risky"
    } else if items.iter().any(|item| item.safety != "safe") {
        "review"
    } else {
        "safe"
    }
    .to_owned()
}

fn sum_size_bytes(items: &[&StorageHygieneItem]) -> u64 {
    items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes))
}

fn sum_logical_bytes(items: &[&StorageHygieneItem]) -> u64 {
    items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.logical_bytes))
}

fn sum_physical_bytes(items: &[&StorageHygieneItem]) -> u64 {
    items.iter().fold(0u64, |total, item| {
        total.saturating_add(item.physical_bytes)
    })
}

fn file_content_hash(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Some(format!("sha256:{:x}", hasher.finalize()))
}

fn file_partial_content_hash(path: &Path, size_bytes: u64) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hash = 0xcbf29ce484222325u64;
    let first_count = hash_file_region(&mut file, 0, size_bytes, &mut hash)?;
    if size_bytes > DUPLICATE_PARTIAL_HASH_BYTES as u64 {
        let tail_offset = size_bytes.saturating_sub(DUPLICATE_PARTIAL_HASH_BYTES as u64);
        let tail_count = hash_file_region(&mut file, tail_offset, size_bytes, &mut hash)?;
        Some(format!(
            "fnv64-edge:{size_bytes}:{first_count}:{tail_offset}:{tail_count}:{hash:016x}"
        ))
    } else {
        Some(format!(
            "fnv64-edge:{size_bytes}:{first_count}:0:0:{hash:016x}"
        ))
    }
}

fn hash_file_region(
    file: &mut fs::File,
    offset: u64,
    size_bytes: u64,
    hash: &mut u64,
) -> Option<usize> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let remaining = size_bytes.saturating_sub(offset);
    let target = remaining.min(DUPLICATE_PARTIAL_HASH_BYTES as u64) as usize;
    if target == 0 {
        return Some(0);
    }
    let mut buffer = vec![0u8; target];
    let count = file.read(&mut buffer).ok()?;
    fnv64_update(hash, &offset.to_le_bytes());
    fnv64_update(hash, &(count as u64).to_le_bytes());
    fnv64_update(hash, &buffer[..count]);
    Some(count)
}

fn fnv64_update(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

fn image_average_hash(path: &Path) -> Option<u64> {
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let thumbnail = image
        .resize_exact(8, 8, image::imageops::FilterType::Triangle)
        .to_luma8();
    let pixels = thumbnail.as_raw();
    if pixels.len() != 64 {
        return None;
    }
    let average = pixels.iter().map(|pixel| u64::from(*pixel)).sum::<u64>() / 64;
    let mut hash = 0u64;
    for (index, pixel) in pixels.iter().enumerate() {
        if u64::from(*pixel) >= average {
            hash |= 1u64 << index;
        }
    }
    Some(hash)
}

fn image_hash_hamming_distance(left: u64, right: u64) -> u32 {
    (left ^ right).count_ones()
}

fn video_signature(path: &Path, size_bytes: u64) -> Option<VideoSimilaritySignature> {
    let probe = read_video_probe_bytes(path, size_bytes)?;
    let duration_millis = mp4_duration_millis(&probe)?;
    let (width, height) = mp4_dimensions(&probe)?;
    let codec = mp4_codec(&probe)?;
    let sample_hash = mp4_mdat_sample_hash(&probe)?;
    Some(VideoSimilaritySignature {
        duration_millis,
        width,
        height,
        codec,
        sample_hash,
        size_bytes,
    })
}

fn read_video_probe_bytes(path: &Path, size_bytes: u64) -> Option<Vec<u8>> {
    let mut file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    let first_len = size_bytes.min(VIDEO_SIMILARITY_PROBE_BYTES as u64) as usize;
    let mut first = vec![0u8; first_len];
    let read_first = file.read(&mut first).ok()?;
    bytes.extend_from_slice(&first[..read_first]);
    if size_bytes > VIDEO_SIMILARITY_PROBE_BYTES as u64 * 2 {
        file.seek(SeekFrom::Start(
            size_bytes.saturating_sub(VIDEO_SIMILARITY_PROBE_BYTES as u64),
        ))
        .ok()?;
        let mut last = vec![0u8; VIDEO_SIMILARITY_PROBE_BYTES];
        let read_last = file.read(&mut last).ok()?;
        bytes.extend_from_slice(&last[..read_last]);
    }
    if bytes.is_empty() { None } else { Some(bytes) }
}

fn mp4_duration_millis(bytes: &[u8]) -> Option<u64> {
    for type_offset in box_type_offsets(bytes, b"mvhd") {
        let payload = type_offset + 4;
        let version = *bytes.get(payload)?;
        let (timescale_offset, duration_offset) = if version == 1 {
            (payload + 20, payload + 24)
        } else {
            (payload + 12, payload + 16)
        };
        let timescale = u64::from(read_be_u32(bytes, timescale_offset)?);
        if timescale == 0 {
            continue;
        }
        let duration = if version == 1 {
            read_be_u64(bytes, duration_offset)?
        } else {
            u64::from(read_be_u32(bytes, duration_offset)?)
        };
        return Some(duration.saturating_mul(1_000) / timescale);
    }
    None
}

fn mp4_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut best = None::<(u32, u32)>;
    for type_offset in box_type_offsets(bytes, b"tkhd") {
        let box_start = type_offset.checked_sub(4)?;
        let box_size = read_be_u32(bytes, box_start)? as usize;
        if box_size < 92 {
            continue;
        }
        let box_end = box_start.saturating_add(box_size);
        if box_end > bytes.len() {
            continue;
        }
        let width = read_be_u32(bytes, box_end - 8)? >> 16;
        let height = read_be_u32(bytes, box_end - 4)? >> 16;
        if width == 0 || height == 0 {
            continue;
        }
        if best.is_none_or(|(best_width, best_height)| {
            width.saturating_mul(height) > best_width.saturating_mul(best_height)
        }) {
            best = Some((width, height));
        }
    }
    best
}

fn mp4_codec(bytes: &[u8]) -> Option<[u8; 4]> {
    for type_offset in box_type_offsets(bytes, b"stsd") {
        let box_start = type_offset.checked_sub(4)?;
        let box_size = read_be_u32(bytes, box_start)? as usize;
        let box_end = box_start.saturating_add(box_size).min(bytes.len());
        let mut entry_offset = type_offset + 12;
        while entry_offset + 8 <= box_end {
            let entry_size = read_be_u32(bytes, entry_offset)? as usize;
            if entry_size < 8 || entry_offset + 8 > bytes.len() {
                break;
            }
            let codec = bytes.get(entry_offset + 4..entry_offset + 8)?;
            if codec.iter().all(u8::is_ascii_alphanumeric) {
                return Some([codec[0], codec[1], codec[2], codec[3]]);
            }
            entry_offset = entry_offset.saturating_add(entry_size);
        }
    }
    None
}

fn mp4_mdat_sample_hash(bytes: &[u8]) -> Option<u64> {
    for type_offset in box_type_offsets(bytes, b"mdat") {
        let box_start = type_offset.checked_sub(4)?;
        let box_size = read_be_u32(bytes, box_start)? as usize;
        if box_size < 16 {
            continue;
        }
        let data_start = type_offset + 4;
        let data_end = box_start.saturating_add(box_size).min(bytes.len());
        if data_end <= data_start {
            continue;
        }
        let data = &bytes[data_start..data_end];
        let mut hash = 0xcbf29ce484222325u64;
        hash_video_sample_region(&mut hash, data, 0);
        if data.len() > VIDEO_SIMILARITY_SAMPLE_BYTES * 2 {
            hash_video_sample_region(&mut hash, data, data.len() / 2);
        }
        if data.len() > VIDEO_SIMILARITY_SAMPLE_BYTES {
            hash_video_sample_region(
                &mut hash,
                data,
                data.len().saturating_sub(VIDEO_SIMILARITY_SAMPLE_BYTES),
            );
        }
        return Some(hash);
    }
    None
}

fn hash_video_sample_region(hash: &mut u64, data: &[u8], offset: usize) {
    let start = offset.min(data.len());
    let end = start
        .saturating_add(VIDEO_SIMILARITY_SAMPLE_BYTES)
        .min(data.len());
    fnv64_update(hash, &(start as u64).to_le_bytes());
    fnv64_update(hash, &((end - start) as u64).to_le_bytes());
    fnv64_update(hash, &data[start..end]);
}

fn box_type_offsets<'a>(
    bytes: &'a [u8],
    box_type: &'a [u8; 4],
) -> impl Iterator<Item = usize> + 'a {
    bytes
        .windows(4)
        .enumerate()
        .filter_map(move |(offset, window)| (window == box_type).then_some(offset))
        .filter(|offset| *offset >= 4)
}

fn video_size_ratio_compatible(left: u64, right: u64) -> bool {
    let smaller = left.min(right);
    let larger = left.max(right);
    larger == 0
        || smaller.saturating_mul(100)
            >= larger.saturating_mul(100 - VIDEO_SIMILARITY_SIZE_RATIO_PERCENT)
}

fn binary_similarity_signature(path: &Path, size_bytes: u64) -> Option<BinarySimilaritySignature> {
    let regions = read_binary_probe_regions(path, size_bytes)?;
    let mut features = BTreeSet::new();
    for region in regions {
        collect_binary_cdc_features(&region, &mut features);
    }
    if features.len() < BINARY_SIMILARITY_MIN_FEATURES {
        return None;
    }
    Some(BinarySimilaritySignature {
        features,
        size_bytes,
    })
}

fn read_binary_probe_regions(path: &Path, size_bytes: u64) -> Option<Vec<Vec<u8>>> {
    let mut file = fs::File::open(path).ok()?;
    let mut regions = Vec::new();
    let first_len = size_bytes.min(BINARY_SIMILARITY_READ_MAX_BYTES as u64) as usize;
    let mut first = vec![0u8; first_len];
    let read_first = file.read(&mut first).ok()?;
    if read_first > 0 {
        first.truncate(read_first);
        regions.push(first);
    }
    if size_bytes > BINARY_SIMILARITY_READ_MAX_BYTES as u64 * 2 {
        file.seek(SeekFrom::Start(
            size_bytes.saturating_sub(BINARY_SIMILARITY_READ_MAX_BYTES as u64),
        ))
        .ok()?;
        let mut last = vec![0u8; BINARY_SIMILARITY_READ_MAX_BYTES];
        let read_last = file.read(&mut last).ok()?;
        if read_last > 0 {
            last.truncate(read_last);
            regions.push(last);
        }
    }
    if regions.is_empty() {
        None
    } else {
        Some(regions)
    }
}

fn collect_binary_cdc_features(region: &[u8], features: &mut BTreeSet<u64>) {
    let mut chunk_start = 0usize;
    let mut rolling = 0u64;
    for (index, byte) in region.iter().enumerate() {
        rolling = rolling.rotate_left(7) ^ binary_gear_hash(*byte);
        let chunk_len = index + 1 - chunk_start;
        if chunk_len >= BINARY_SIMILARITY_CHUNK_MIN_BYTES
            && ((rolling & BINARY_SIMILARITY_CHUNK_MASK) == 0
                || chunk_len >= BINARY_SIMILARITY_CHUNK_MAX_BYTES)
        {
            push_binary_chunk_feature(&region[chunk_start..=index], features);
            chunk_start = index + 1;
            rolling = 0;
        }
    }
    if region.len().saturating_sub(chunk_start) >= BINARY_SIMILARITY_CHUNK_MIN_BYTES {
        push_binary_chunk_feature(&region[chunk_start..], features);
    }
}

fn push_binary_chunk_feature(chunk: &[u8], features: &mut BTreeSet<u64>) {
    if chunk.is_empty() {
        return;
    }
    let mut hash = 0xcbf29ce484222325u64;
    fnv64_update(&mut hash, &(chunk.len() as u64).to_le_bytes());
    fnv64_update(&mut hash, chunk);
    features.insert(hash);
}

fn binary_gear_hash(byte: u8) -> u64 {
    let mut hash = u64::from(byte).wrapping_mul(0x9e37_79b1_85eb_ca87);
    hash ^= hash >> 29;
    hash = hash.wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    hash ^ (hash >> 32)
}

fn binary_size_ratio_compatible(left: u64, right: u64) -> bool {
    let smaller = left.min(right);
    let larger = left.max(right);
    larger == 0
        || smaller.saturating_mul(100)
            >= larger.saturating_mul(100 - BINARY_SIMILARITY_SIZE_RATIO_PERCENT)
}

fn text_hash_hamming_distance(left: u64, right: u64) -> u32 {
    (left ^ right).count_ones()
}

fn text_similarity_token_counts_compatible(left: usize, right: usize) -> bool {
    let smaller = left.min(right);
    let larger = left.max(right);
    larger == 0 || smaller.saturating_mul(100) >= larger.saturating_mul(60)
}

fn text_simhash(path: &Path) -> Option<(u64, usize)> {
    let text = similarity_text_for_path(path)?;
    let tokens = normalized_text_tokens(&text);
    if tokens.len() < TEXT_SIMILARITY_MIN_TOKENS {
        return None;
    }

    let mut weights = [0i32; 64];
    for shingle in tokens.windows(TEXT_SIMILARITY_SHINGLE_TOKENS) {
        let hash = text_shingle_hash(shingle);
        for (bit, weight) in weights.iter_mut().enumerate() {
            if (hash & (1u64 << bit)) == 0 {
                *weight -= 1;
            } else {
                *weight += 1;
            }
        }
    }

    let mut hash = 0u64;
    for (bit, weight) in weights.iter().enumerate() {
        if *weight >= 0 {
            hash |= 1u64 << bit;
        }
    }
    Some((hash, tokens.len()))
}

fn similarity_text_for_path(path: &Path) -> Option<String> {
    if is_similarity_document_path(path) {
        return document_text_for_path(path);
    }
    raw_text_for_path(path)
}

fn raw_text_for_path(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut limited = file.take(TEXT_SIMILARITY_READ_MAX_BYTES);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).ok()?;
    if bytes.is_empty() || bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn document_text_for_path(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "pdf" => pdf_text_for_path(path),
        "docx" | "pptx" | "xlsx" => ooxml_text_for_path(path),
        _ => None,
    }
}

fn pdf_text_for_path(path: &Path) -> Option<String> {
    let bytes = read_limited_file(path, TEXT_SIMILARITY_HASH_MAX_BYTES as usize)?;
    if !bytes.starts_with(b"%PDF") {
        return None;
    }
    let mut text = String::new();
    append_pdf_literal_strings(&bytes, &mut text);
    append_pdf_flate_stream_text(&bytes, &mut text);
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn append_pdf_flate_stream_text(bytes: &[u8], text: &mut String) {
    let mut search_start = 0usize;
    while let Some(relative_stream) = find_bytes(&bytes[search_start..], b"stream") {
        let stream_marker = search_start + relative_stream;
        let dictionary_start = stream_marker.saturating_sub(512);
        let dictionary = &bytes[dictionary_start..stream_marker];
        if !dictionary
            .windows(b"FlateDecode".len())
            .any(|window| window == b"FlateDecode")
        {
            search_start = stream_marker.saturating_add(b"stream".len());
            continue;
        }
        let Some(relative_endstream) = find_bytes(&bytes[stream_marker..], b"endstream") else {
            break;
        };
        let mut data_start = stream_marker + b"stream".len();
        if bytes.get(data_start) == Some(&b'\r') {
            data_start += 1;
        }
        if bytes.get(data_start) == Some(&b'\n') {
            data_start += 1;
        }
        let mut data_end = stream_marker + relative_endstream;
        while data_end > data_start && matches!(bytes[data_end - 1], b'\n' | b'\r') {
            data_end -= 1;
        }
        if data_end > data_start
            && let Ok(decompressed) = inflate::decompress_to_vec_zlib_with_limit(
                &bytes[data_start..data_end],
                TEXT_SIMILARITY_READ_MAX_BYTES as usize,
            )
        {
            append_pdf_literal_strings(&decompressed, text);
        }
        search_start = stream_marker + relative_endstream + b"endstream".len();
        if text.len() >= TEXT_SIMILARITY_READ_MAX_BYTES as usize {
            break;
        }
    }
}

fn append_pdf_literal_strings(bytes: &[u8], text: &mut String) {
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'(' {
            index += 1;
            continue;
        }
        index += 1;
        let mut depth = 1usize;
        while index < bytes.len() && depth > 0 {
            let byte = bytes[index];
            match byte {
                b'\\' => {
                    index += 1;
                    if let Some(escaped) = bytes.get(index) {
                        match escaped {
                            b'n' | b'r' | b't' => text.push(' '),
                            b'b' | b'f' => {}
                            b'(' | b')' | b'\\' => text.push(char::from(*escaped)),
                            value if value.is_ascii_digit() => {
                                text.push(' ');
                                while bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
                                    index += 1;
                                }
                            }
                            value if value.is_ascii_graphic() || *value == b' ' => {
                                text.push(char::from(*value));
                            }
                            _ => {}
                        }
                    }
                }
                b'(' => {
                    depth += 1;
                    text.push(' ');
                }
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth > 0 {
                        text.push(' ');
                    }
                }
                value if value.is_ascii_graphic() || value == b' ' => {
                    text.push(char::from(value));
                }
                _ => text.push(' '),
            }
            index += 1;
        }
        text.push(' ');
        if text.len() >= TEXT_SIMILARITY_READ_MAX_BYTES as usize {
            break;
        }
    }
}

fn ooxml_text_for_path(path: &Path) -> Option<String> {
    let bytes = read_limited_file(path, TEXT_SIMILARITY_HASH_MAX_BYTES as usize)?;
    let mut text = String::new();
    for entry in ooxml_xml_entries(&bytes) {
        append_xml_text(&entry, &mut text);
        if text.len() >= TEXT_SIMILARITY_READ_MAX_BYTES as usize {
            break;
        }
    }
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn ooxml_xml_entries(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    while offset + 30 <= bytes.len() {
        if read_le_u32(bytes, offset) != Some(0x0403_4b50) {
            offset += 1;
            continue;
        }
        let flags = read_le_u16(bytes, offset + 6).unwrap_or_default();
        let method = read_le_u16(bytes, offset + 8).unwrap_or_default();
        let compressed_size = read_le_u32(bytes, offset + 18).unwrap_or_default() as usize;
        let uncompressed_size = read_le_u32(bytes, offset + 22).unwrap_or_default() as usize;
        let name_len = read_le_u16(bytes, offset + 26).unwrap_or_default() as usize;
        let extra_len = read_le_u16(bytes, offset + 28).unwrap_or_default() as usize;
        let name_start = offset + 30;
        let data_start = name_start
            .saturating_add(name_len)
            .saturating_add(extra_len);
        let data_end = data_start.saturating_add(compressed_size);
        if data_start > bytes.len() || data_end > bytes.len() || compressed_size == 0 {
            break;
        }
        let name = std::str::from_utf8(&bytes[name_start..name_start + name_len]).unwrap_or("");
        if flags & 0x08 == 0 && is_ooxml_text_part(name) {
            let data = &bytes[data_start..data_end];
            match method {
                0 => entries.push(data.to_vec()),
                8 => {
                    if let Ok(decompressed) = inflate::decompress_to_vec_with_limit(
                        data,
                        uncompressed_size
                            .max(TEXT_SIMILARITY_MIN_TOKENS)
                            .min(TEXT_SIMILARITY_READ_MAX_BYTES as usize),
                    ) {
                        entries.push(decompressed);
                    }
                }
                _ => {}
            }
        }
        offset = data_end;
    }
    entries
}

fn is_ooxml_text_part(name: &str) -> bool {
    (name == "word/document.xml")
        || name.starts_with("word/header")
        || name.starts_with("word/footer")
        || matches!(
            name,
            "word/footnotes.xml" | "word/endnotes.xml" | "xl/sharedStrings.xml"
        )
        || (name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        || (name.starts_with("ppt/notesSlides/notesSlide") && name.ends_with(".xml"))
        || (name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
}

fn append_xml_text(bytes: &[u8], text: &mut String) {
    let xml = String::from_utf8_lossy(bytes);
    let mut in_tag = false;
    let mut entity = String::new();
    let mut in_entity = false;
    for character in xml.chars() {
        if in_tag {
            if character == '>' {
                in_tag = false;
                text.push(' ');
            }
            continue;
        }
        if in_entity {
            if character == ';' {
                append_xml_entity(text, &entity);
                entity.clear();
                in_entity = false;
            } else if entity.len() < 16 {
                entity.push(character);
            } else {
                entity.clear();
                in_entity = false;
                text.push(' ');
            }
            continue;
        }
        match character {
            '<' => in_tag = true,
            '&' => in_entity = true,
            _ => text.push(character),
        }
        if text.len() >= TEXT_SIMILARITY_READ_MAX_BYTES as usize {
            break;
        }
    }
}

fn append_xml_entity(text: &mut String, entity: &str) {
    match entity {
        "amp" => text.push('&'),
        "apos" => text.push('\''),
        "gt" => text.push('>'),
        "lt" => text.push('<'),
        "quot" => text.push('"'),
        value if value.starts_with("#x") => {
            if let Ok(codepoint) = u32::from_str_radix(&value[2..], 16)
                && let Some(character) = char::from_u32(codepoint)
            {
                text.push(character);
            }
        }
        value if value.starts_with('#') => {
            if let Ok(codepoint) = value[1..].parse::<u32>()
                && let Some(character) = char::from_u32(codepoint)
            {
                text.push(character);
            }
        }
        _ => text.push(' '),
    }
}

fn read_limited_file(path: &Path, max_bytes: usize) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    let mut limited = file.take(max_bytes as u64);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_le_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_be_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let slice = bytes.get(offset..offset + 8)?;
    Some(u64::from_be_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

fn normalized_text_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        if character.is_alphanumeric() || character == '_' {
            for lower in character.to_lowercase() {
                current.push(lower);
            }
            continue;
        }
        push_normalized_text_token(&mut tokens, &mut current);
    }
    push_normalized_text_token(&mut tokens, &mut current);
    tokens
}

fn push_normalized_text_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.len() >= 2 {
        tokens.push(current.chars().take(80).collect());
    }
    current.clear();
}

fn text_shingle_hash(shingle: &[String]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for token in shingle {
        fnv64_update(&mut hash, token.as_bytes());
        fnv64_update(&mut hash, &[0xff]);
    }
    hash
}

fn summarize_app_footprints(items: &[StorageHygieneItem]) -> Vec<StorageAppFootprint> {
    let ownership_graph = AppOwnershipGraph::from_items(items);
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
        .map(|footprint| footprint.finish(&ownership_graph))
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

#[derive(Clone, Debug, Default)]
struct AppOwnershipGraph {
    installed_bundle_ids: BTreeSet<String>,
    installed_app_names: BTreeSet<String>,
    launchservices_bundle_ids: BTreeSet<String>,
    running_bundle_ids: BTreeSet<String>,
    running_app_names: BTreeSet<String>,
}

impl AppOwnershipGraph {
    fn from_items(items: &[StorageHygieneItem]) -> Self {
        let mut graph = Self::default();
        for item in items {
            if item.kind != "macos-app-bundle" {
                continue;
            }
            let path = Path::new(&item.path);
            let app_name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or(&item.display_name);
            graph
                .installed_app_names
                .insert(normalize_app_key(app_name));
            if let Some(bundle_identifier) = app_bundle_identifier(path) {
                graph
                    .installed_bundle_ids
                    .insert(normalize_app_key(&bundle_identifier));
            }
        }
        graph
    }

    fn installed_match(&self, app_name: &str, bundle_identifier: Option<&str>) -> bool {
        bundle_identifier
            .map(normalize_app_key)
            .is_some_and(|bundle_identifier| {
                self.installed_bundle_ids.contains(&bundle_identifier)
                    || self.launchservices_bundle_ids.contains(&bundle_identifier)
            })
            || self
                .installed_app_names
                .contains(&normalize_app_key(app_name))
    }

    fn running_match(&self, app_name: &str, bundle_identifier: Option<&str>) -> bool {
        bundle_identifier
            .map(normalize_app_key)
            .is_some_and(|bundle_identifier| self.running_bundle_ids.contains(&bundle_identifier))
            || self
                .running_app_names
                .contains(&normalize_app_key(app_name))
    }

    fn launchservices_match(&self, bundle_identifier: Option<&str>) -> bool {
        bundle_identifier
            .map(normalize_app_key)
            .is_some_and(|bundle_identifier| {
                self.launchservices_bundle_ids.contains(&bundle_identifier)
            })
    }
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

    fn finish(mut self, ownership_graph: &AppOwnershipGraph) -> StorageAppFootprint {
        let component_count = self.components.len();
        let has_bundle = self
            .components
            .iter()
            .any(|component| component.component == "App bundle");
        let has_receipt = self
            .components
            .iter()
            .any(|component| component.component == "Receipt");
        let has_launch_item = self
            .components
            .iter()
            .any(|component| component.component == "Launch item");
        let has_state = self.components.iter().any(|component| {
            matches!(
                component.component.as_str(),
                "Application Support" | "Caches" | "Container" | "Preferences"
            )
        });
        let bundle_identifier = self.bundle_identifier.as_deref();
        let installed_match =
            has_bundle || ownership_graph.installed_match(&self.app_name, bundle_identifier);
        let running_match = ownership_graph.running_match(&self.app_name, bundle_identifier);
        let launchservices_match = ownership_graph.launchservices_match(bundle_identifier);
        let ownership_status = app_ownership_status(
            installed_match,
            has_state,
            has_receipt,
            has_launch_item,
            running_match,
        );
        let orphan_confidence = app_orphan_confidence(
            &ownership_status,
            has_receipt,
            has_launch_item,
            running_match,
            launchservices_match,
        );
        let ownership_signals = app_ownership_signals(
            &self.components,
            installed_match,
            has_bundle,
            has_receipt,
            has_launch_item,
            running_match,
            launchservices_match,
        );
        let orphan_recommendation =
            app_orphan_recommendation(&ownership_status, &orphan_confidence);

        self.components.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        self.components.truncate(8);
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
            ownership_status,
            orphan_confidence,
            ownership_signals,
            orphan_recommendation,
            components: self.components,
            recommendation: "Treat this as an uninstall footprint: quit the app, review support data, preferences, receipts, containers, and launch items, then move selected components to Trash.".to_owned(),
        }
    }
}

fn app_ownership_status(
    installed_match: bool,
    has_state: bool,
    has_receipt: bool,
    has_launch_item: bool,
    running_match: bool,
) -> String {
    if installed_match {
        "installed"
    } else if has_state && !has_receipt && !has_launch_item && !running_match {
        "orphaned"
    } else if has_state || has_receipt || has_launch_item || running_match {
        "partial"
    } else {
        "unknown"
    }
    .to_owned()
}

fn app_orphan_confidence(
    ownership_status: &str,
    has_receipt: bool,
    has_launch_item: bool,
    running_match: bool,
    launchservices_match: bool,
) -> String {
    match ownership_status {
        "installed" => "none",
        "orphaned"
            if !has_receipt && !has_launch_item && !running_match && !launchservices_match =>
        {
            "medium"
        }
        "orphaned" => "low",
        "partial" => "low",
        _ => "low",
    }
    .to_owned()
}

fn app_orphan_recommendation(ownership_status: &str, orphan_confidence: &str) -> String {
    match ownership_status {
        "installed" => "Installed app ownership is present. Use the uninstall footprint view to review state, caches, receipts, and launch items before staging anything.".to_owned(),
        "orphaned" => format!(
            "No installed app, receipt, launch item, or running-process evidence matched in this scan. Treat as a possible leftover with {orphan_confidence} orphan confidence and review before staging."
        ),
        "partial" => "Only partial ownership evidence matched. Verify whether the app was removed or still has login/launch/runtime ownership before cleanup.".to_owned(),
        _ => "Ownership evidence is incomplete. Inspect the paths before cleanup and prefer Trash over permanent deletion.".to_owned(),
    }
}

fn app_ownership_signals(
    components: &[StorageAppFootprintComponent],
    installed_match: bool,
    has_bundle: bool,
    has_receipt: bool,
    has_launch_item: bool,
    running_match: bool,
    launchservices_match: bool,
) -> Vec<StorageAppOwnershipSignal> {
    vec![
        app_ownership_signal(
            "app-bundle",
            if installed_match { "present" } else { "absent" },
            if has_bundle {
                "App bundle component found in this footprint."
            } else if installed_match {
                "Matching installed app bundle found elsewhere in the scan."
            } else {
                "No matching app bundle found in this scan."
            },
            component_path(components, &["App bundle"]),
        ),
        app_ownership_signal(
            "receipt",
            if has_receipt { "present" } else { "absent" },
            if has_receipt {
                "Installer receipt matched this footprint."
            } else {
                "No installer receipt matched this footprint."
            },
            component_path(components, &["Receipt"]),
        ),
        app_ownership_signal(
            "launch-item",
            if has_launch_item { "present" } else { "absent" },
            if has_launch_item {
                "Launch agent or daemon matched this footprint."
            } else {
                "No launch agent or daemon matched this footprint."
            },
            component_path(components, &["Launch item"]),
        ),
        app_ownership_signal(
            "running-process",
            if running_match { "present" } else { "unknown" },
            if running_match {
                "A running process matched this footprint."
            } else {
                "Runtime process ownership is not collected by the bounded storage scan yet."
            },
            None,
        ),
        app_ownership_signal(
            "launchservices",
            if launchservices_match {
                "present"
            } else {
                "unknown"
            },
            if launchservices_match {
                "LaunchServices ownership matched this footprint."
            } else {
                "LaunchServices ownership is not collected by the bounded storage scan yet."
            },
            None,
        ),
    ]
}

fn app_ownership_signal(
    source: &str,
    status: &str,
    detail: &str,
    path: Option<String>,
) -> StorageAppOwnershipSignal {
    StorageAppOwnershipSignal {
        source: source.to_owned(),
        status: status.to_owned(),
        detail: detail.to_owned(),
        path,
    }
}

fn component_path(components: &[StorageAppFootprintComponent], labels: &[&str]) -> Option<String> {
    components
        .iter()
        .find(|component| labels.contains(&component.component.as_str()))
        .map(|component| component.path.clone())
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
                "homebrew-cache",
                "pip-cache",
                "uv-cache",
                "gradle-cache",
                "go-build-cache",
                "simulator-cache",
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

pub(super) fn highest_cleanup_tier<'a>(
    items: impl Iterator<Item = &'a StorageHygieneItem>,
) -> String {
    let mut highest = "safe".to_owned();
    for item in items {
        if cleanup_tier_rank(&item.cleanup_tier) > cleanup_tier_rank(&highest) {
            highest = item.cleanup_tier.clone();
        }
    }
    highest
}

pub(super) fn normalize_roots(roots: Vec<String>) -> Vec<PathBuf> {
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

pub(super) fn normalize_dirty_paths(paths: Vec<String>) -> Vec<String> {
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

pub(super) fn path_matches_dirty_prefix(path: &Path, dirty_paths: &[String]) -> bool {
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

pub(super) fn summarize_volume_states(requested_roots: &[PathBuf]) -> Vec<StorageVolumeState> {
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

fn source_reclaimable_bytes(path: &str, items: &[StorageHygieneItem]) -> u64 {
    items
        .iter()
        .filter(|item| item.path == path || item.path.starts_with(&format!("{path}/")))
        .map(|item| item.size_bytes)
        .fold(0u64, u64::saturating_add)
}

pub(super) fn skipped_root_permission_state(reason: &str) -> String {
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

pub(super) fn storage_source_kind(path: &Path) -> String {
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

pub(super) fn storage_source_label(path: &Path, kind: &str) -> String {
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
