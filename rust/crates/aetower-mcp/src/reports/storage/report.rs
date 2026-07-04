use super::*;
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
        report.scan_duration_millis,
        report.diagnostics.payload_bytes,
        report.diagnostics.candidate_seen_count,
        table_page_millis,
        render_publish_millis,
    );
}

pub(super) fn storage_performance_budget_diagnostics(
    scan_duration_millis: u64,
    payload_bytes: u64,
    candidate_seen_count: u64,
    table_page_millis: u64,
    render_publish_millis: u64,
) -> StoragePerformanceBudgetDiagnostics {
    let mut notes = Vec::new();
    let mut severity = 0u8;
    if scan_duration_millis >= STORAGE_SCAN_LATENCY_CRITICAL_MILLIS {
        severity = severity.max(2);
        notes.push(format!(
            "scan latency exceeded critical budget: {scan_duration_millis}ms >= {STORAGE_SCAN_LATENCY_CRITICAL_MILLIS}ms"
        ));
    } else if scan_duration_millis >= STORAGE_SCAN_LATENCY_WARN_MILLIS {
        severity = severity.max(1);
        notes.push(format!(
            "scan latency exceeded warning budget: {scan_duration_millis}ms >= {STORAGE_SCAN_LATENCY_WARN_MILLIS}ms"
        ));
    }
    if payload_bytes >= STORAGE_PAYLOAD_CRITICAL_BYTES {
        severity = severity.max(2);
        notes.push(format!(
            "payload exceeded critical budget: {payload_bytes} bytes >= {STORAGE_PAYLOAD_CRITICAL_BYTES} bytes"
        ));
    } else if payload_bytes >= STORAGE_PAYLOAD_WARN_BYTES {
        severity = severity.max(1);
        notes.push(format!(
            "payload exceeded warning budget: {payload_bytes} bytes >= {STORAGE_PAYLOAD_WARN_BYTES} bytes"
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
        payload_budget_bytes: STORAGE_PAYLOAD_WARN_BYTES,
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
        let root_deadline = root_started
            + per_root_walk_slice(
                walk_budget.saturating_sub(walk_started.elapsed()),
                total_roots.saturating_sub(root_index),
                options.mode.per_root_slice_floor(),
            );
        let (_root_repositories, root_dirs, root_truncated) = scan_root(
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
    let writer_ledger = load_storage_writer_ledger_records();
    apply_measured_rebuild_costs(&mut items, &writer_ledger);
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
        growth_insights,
        cold_data,
        truncated,
        caveats: vec![
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
        ],
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
    let mut memo = lock_or_recover(&INDEX_REPORT_SECTIONS_MEMO);
    let dirty_stamp = INDEX_REPORT_SECTIONS_DIRTY.load(AtomicOrdering::Acquire);
    if let Some(entry) = memo.as_ref()
        && entry.dirty_stamp == dirty_stamp
        && generation.is_some_and(|generation| generation == entry.generation)
        && entry.roots_key == roots_key
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
        annotate_cold_items_active_holders(&mut top_items);
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

fn annotate_cold_items_active_holders(items: &mut [StorageHygieneItem]) {
    let paths = items
        .iter()
        .filter(|item| item.cleanup_allowed)
        .map(|item| item.path.clone())
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
                            .map(|holder| ColdPathHolder {
                                pid: holder.pid,
                                command: holder.command,
                                fd: holder.fd,
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            apply_active_cold_holders(items, &active_holders);
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
pub(super) struct ColdPathHolder {
    pub(super) pid: u32,
    pub(super) command: String,
    pub(super) fd: String,
}

pub(super) fn apply_active_cold_holders(
    items: &mut [StorageHygieneItem],
    holders_by_path: &BTreeMap<String, Vec<ColdPathHolder>>,
) {
    for item in items {
        let Some(holders) = holders_by_path
            .get(&item.path)
            .filter(|holders| !holders.is_empty())
        else {
            continue;
        };
        let summary = summarize_cold_path_holders(holders);
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

fn summarize_cold_path_holders(holders: &[ColdPathHolder]) -> String {
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
