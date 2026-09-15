use super::*;

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
        scan_generation: report.scan_generation.clone(),
        cache_status: report.cache_status,
        diagnostics: report.diagnostics,
        summary: report.summary,
        investigation: report.investigation,
        cleanup_tiers: report.cleanup_tiers,
        cleanup_recipes: report.cleanup_recipes.into_iter().take(8).collect(),
        cleanup_bundles: report.cleanup_bundles.into_iter().take(4).collect(),
        cleanup_lanes: report.cleanup_lanes.into_iter().take(6).collect(),
        budget_guardrails: report.budget_guardrails,
        agent_hygiene: report.agent_hygiene,
        repository_inventory_complete: report.repository_inventory_complete,
        repository_inventory_truncated: report.repository_inventory_truncated,
        repository_inventory_roots: report.repository_inventory_roots,
        repository_inventory_partial_roots: report.repository_inventory_partial_roots,
        repository_inventory_coverage: report.repository_inventory_coverage,
        repo_footprints: report.repo_footprints.into_iter().take(8).collect(),
        duplicate_groups: report.duplicate_groups.into_iter().take(6).collect(),
        redundancy_groups: report.redundancy_groups.into_iter().take(8).collect(),
        app_footprints: report.app_footprints.into_iter().take(6).collect(),
        system_data_buckets: report.system_data_buckets,
        treemap_roots: report.treemap_roots,
        items: report.items.into_iter().take(12).collect(),
        roots: report.roots,
        skipped_roots: report.skipped_roots,
        source_coverage: report.source_coverage,
        volume_states: report.volume_states,
        growth_deltas: report.growth_deltas,
        growth_insights: report.growth_insights,
        cold_data: report.cold_data,
        truncated: report.truncated,
        caveats: report.caveats,
    })
    .map_err(|error| error.to_string())
}

/// Return just the growth-intelligence block (rates, forecasts, and the
/// since-last-scan diff) straight from the persistent index, without paying
/// the full overview/report cost.
pub fn storage_growth_insights_json(
    roots: Vec<String>,
    window_days: u64,
) -> Result<String, String> {
    let now_millis = storage_now_millis();
    let roots = normalize_roots(roots);
    let storage_index = StorageSizeIndex::open();
    let volume_states = summarize_volume_states(&roots);
    let insights = storage_index
        .load_growth_insights(&roots, &volume_states, now_millis, window_days)
        .ok_or_else(|| format!("storage index unavailable: {}", storage_index.status))?;
    serde_json::to_string(&StorageGrowthInsightsResponse {
        captured_at_millis: now_millis,
        roots: roots
            .iter()
            .map(|root| root.display().to_string())
            .collect(),
        storage_index_status: storage_index.status.clone(),
        insights,
    })
    .map_err(|error| error.to_string())
}

pub fn storage_situation_json(roots: Vec<String>, limit: usize) -> Result<String, String> {
    storage_situation_json_with_incremental_drain(roots, limit, false)
}

pub fn storage_backlog_drain_json(roots: Vec<String>, limit: usize) -> Result<String, String> {
    storage_situation_json_with_incremental_drain(roots, limit, true)
}

fn storage_situation_json_with_incremental_drain(
    roots: Vec<String>,
    limit: usize,
    start_incremental_drain: bool,
) -> Result<String, String> {
    let now_millis = storage_now_millis();
    let roots = normalize_roots(roots);
    let storage_index = StorageSizeIndex::open();
    let ledger_records = load_storage_filesystem_event_records();
    let dirty_summary = storage_index.ingest_filesystem_events(&ledger_records, &roots, now_millis);
    if start_incremental_drain {
        ensure_dirty_storage_subtree_measurement(&roots, &dirty_summary);
    }
    let limit = limit.clamp(1, 40);
    if let Some(snapshot) = storage_index.load_situation_snapshot(&roots, limit) {
        let snapshot = overlay_storage_situation_snapshot(
            snapshot,
            &storage_index,
            &roots,
            dirty_summary,
            limit,
        );
        if snapshot.dirty_paths.dirty_path_count > 0 {
            let _ = storage_index.persist_situation_snapshot(
                &roots,
                "situation_snapshot_dirty_overlay",
                &snapshot,
            );
        }
        return serde_json::to_string(&snapshot).map_err(|error| error.to_string());
    }

    let response =
        build_storage_situation_response(&storage_index, &roots, now_millis, limit, dirty_summary);
    let _ = storage_index.persist_situation_snapshot(
        &roots,
        response.cache_status.source.as_str(),
        &response,
    );
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

pub fn storage_pipeline_debug_json(roots: Vec<String>) -> Result<String, String> {
    let now_millis = storage_now_millis();
    let roots = normalize_roots(roots);
    let storage_index = StorageSizeIndex::open();
    let ledger_records = load_storage_filesystem_event_records();
    let dirty_summary = storage_index.ingest_filesystem_events(&ledger_records, &roots, now_millis);
    let snapshot = storage_index.load_situation_snapshot(&roots, 40);
    let event_ledger = storage_pipeline_event_ledger_debug(&ledger_records, &roots, &dirty_summary);
    let measurement = storage_index.latest_measurement_job_debug(&roots);
    let situation_snapshot = snapshot
        .as_ref()
        .map(storage_pipeline_situation_snapshot_debug)
        .unwrap_or_default();
    let diagnosis = storage_pipeline_diagnosis(
        &event_ledger,
        &dirty_summary,
        &measurement,
        &situation_snapshot,
    );

    serde_json::to_string(&StoragePipelineDebugResponse {
        captured_at_millis: now_millis,
        roots: roots
            .iter()
            .map(|root| root.display().to_string())
            .collect(),
        storage_index_status: storage_index.status.clone(),
        event_ledger,
        dirty_paths: dirty_summary,
        measurement,
        situation_snapshot,
        diagnosis,
    })
    .map_err(|error| error.to_string())
}

fn storage_pipeline_event_ledger_debug(
    ledger_records: &[StorageFilesystemEventRecord],
    roots: &[PathBuf],
    dirty_summary: &StorageDirtyPathSummary,
) -> StoragePipelineEventLedgerDebug {
    let mut sample_paths = BTreeSet::new();
    let mut latest_event_id = None;
    let mut latest_event_millis = None;
    let mut total_event_count = 0u64;
    let mut loaded_record_count = 0u64;
    for record in ledger_records {
        let Some(path) = record
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if !roots.is_empty() && !roots.iter().any(|root| path_is_under_root(path, root)) {
            continue;
        }
        loaded_record_count = loaded_record_count.saturating_add(1);
        total_event_count =
            total_event_count.saturating_add(record.event_count.unwrap_or(1).max(1));
        latest_event_id = latest_event_id.max(record.event_id);
        latest_event_millis = latest_event_millis.max(record.timestamp_millis);
        if sample_paths.len() < 8 {
            sample_paths.insert(path.to_owned());
        }
    }

    StoragePipelineEventLedgerDebug {
        loaded_record_count,
        indexed_dirty_path_count: dirty_summary.dirty_path_count,
        latest_event_id,
        latest_event_millis,
        total_event_count,
        sample_paths: sample_paths.into_iter().collect(),
    }
}

fn storage_pipeline_situation_snapshot_debug(
    snapshot: &StorageSituationResponse,
) -> StoragePipelineSituationSnapshotDebug {
    StoragePipelineSituationSnapshotDebug {
        exists: true,
        captured_at_millis: Some(snapshot.captured_at_millis),
        cache_source: Some(snapshot.cache_status.source.clone()),
        stale: snapshot.cache_status.stale,
        partial: snapshot.cache_status.partial,
        item_count: snapshot.summary.item_count,
    }
}

fn storage_pipeline_diagnosis(
    event_ledger: &StoragePipelineEventLedgerDebug,
    dirty_summary: &StorageDirtyPathSummary,
    measurement: &StoragePipelineMeasurementDebug,
    situation_snapshot: &StoragePipelineSituationSnapshotDebug,
) -> Vec<String> {
    let mut diagnosis = Vec::new();
    if !situation_snapshot.exists {
        diagnosis.push(
            "No persisted StorageSituation snapshot exists yet; a first baseline or completed scan is required."
                .to_owned(),
        );
    }
    if event_ledger.loaded_record_count == 0 {
        diagnosis.push("No root-matching filesystem ledger records were loaded.".to_owned());
    } else if dirty_summary.dirty_path_count == 0 {
        diagnosis.push(
            "Filesystem ledger records were loaded, but no dirty paths are pending for these roots."
                .to_owned(),
        );
    } else {
        diagnosis.push(format!(
            "{} dirty path(s) are pending from filesystem events.",
            dirty_summary.dirty_path_count
        ));
    }
    if dirty_summary.unknown_gap {
        diagnosis.push(
            "An unresolved FSEvents gap exists; cleanup should remain blocked until verification."
                .to_owned(),
        );
    }
    match measurement.latest_status.as_deref() {
        Some("complete") if dirty_summary.dirty_path_count == 0 => {
            diagnosis.push(
                "Latest incremental measurement completed and cleared the dirty batch.".to_owned(),
            );
        }
        Some("complete") => {
            diagnosis.push(
                "Latest incremental measurement completed, but additional dirty paths remain queued."
                    .to_owned(),
            );
        }
        Some("pending") | Some("partial") => {
            diagnosis.push(
                "Latest incremental measurement is incomplete and needs continuation.".to_owned(),
            );
        }
        Some(status) => {
            diagnosis.push(format!(
                "Latest incremental measurement status is {status}."
            ));
        }
        None if dirty_summary.dirty_path_count > 0 => {
            diagnosis.push(
                "Dirty paths are queued, but no incremental measurement job is recorded yet."
                    .to_owned(),
            );
        }
        None => {}
    }
    if situation_snapshot.exists && situation_snapshot.stale {
        diagnosis.push("The current situation snapshot is correctly marked stale until dirty paths are remeasured.".to_owned());
    }
    diagnosis
}

pub(super) fn persist_storage_situation_snapshot_from_report(report: &StorageHygieneReport) {
    if report.roots.is_empty() {
        return;
    }
    let roots = report.roots.iter().map(PathBuf::from).collect::<Vec<_>>();
    let storage_index = StorageSizeIndex::open();
    let dirty_summary = storage_index.dirty_path_summary(&roots, 5);
    let dirty_paths = if dirty_summary.dirty_path_count == 0 {
        Vec::new()
    } else {
        storage_index.load_dirty_path_strings(&roots, 512)
    };
    let response = build_storage_situation_response_from_report(
        report,
        &storage_index,
        storage_index.status.clone(),
        dirty_summary,
        &dirty_paths,
    );
    let _ = storage_index.persist_situation_snapshot(&roots, "scan_finalized", &response);
}

pub(super) fn persist_storage_situation_snapshot_for_index(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    source: &str,
) {
    if roots.is_empty() {
        return;
    }
    let now_millis = storage_now_millis();
    let dirty_summary = storage_index.dirty_path_summary(roots, 5);
    let response =
        build_storage_situation_response(storage_index, roots, now_millis, 40, dirty_summary);
    let _ = storage_index.persist_situation_snapshot(roots, source, &response);
}

fn build_storage_situation_response(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    now_millis: u64,
    limit: usize,
    dirty_summary: StorageDirtyPathSummary,
) -> StorageSituationResponse {
    let dirty_paths = storage_index.load_dirty_path_strings(roots, 512);
    let summaries = storage_index.load_index_summaries(roots);
    let domains = storage_index.load_storage_domains(roots, 80);
    let backlog_drain =
        storage_situation_backlog_drain(storage_index, roots, &dirty_summary, now_millis);
    let situation_summary = summarize_storage_situation_with_domains(&summaries, &domains);
    let top_offenders = storage_index
        .load_situation_top_offenders(roots, limit.clamp(1, 40))
        .into_iter()
        .map(|row| {
            let stale = path_matches_dirty_prefix(Path::new(&row.path), &dirty_paths);
            StorageSituationTopOffender {
                path: row.path,
                source_root: row.source_root,
                kind: row.kind,
                cleanup_tier: row.cleanup_tier,
                physical_bytes: row.physical_bytes,
                recommendation_score: row.recommendation_score,
                last_scan_millis: row.last_scan_millis,
                stale,
            }
        })
        .collect::<Vec<_>>();
    let has_cached_facts = situation_summary.item_count > 0
        || !top_offenders.is_empty()
        || !summaries.is_empty()
        || !domains.is_empty();
    let mut cache_status =
        storage_index_cache_status(storage_index, now_millis, true, has_cached_facts);
    apply_dirty_summary_to_cache_status(&mut cache_status, &dirty_summary);
    let mut caveats = vec![
        "Cache-first storage situation: uses Aetower's persistent index summaries and top offenders without walking the filesystem."
            .to_owned(),
        "Rows marked stale were touched by the filesystem watcher and need an incremental refresh before cleanup."
            .to_owned(),
        "Summary bytes are the last known indexed facts; run a refresh to incorporate dirty paths."
            .to_owned(),
    ];
    if dirty_summary.unknown_gap {
        caveats.push(
            "Native FSEvents reported an unknown gap; affected roots need a verifying refresh before indexed-path cleanup actions are trusted. Runtime-owned Docker/Colima data is measured through its own live control plane."
                .to_owned(),
        );
    }
    if !domains.is_empty() {
        caveats.push(
            "Typed storage domains feed the situation and reclaim buckets directly from the materialized domain view."
                .to_owned(),
        );
    }
    let recovery_plan = storage_situation_recovery_plan(&dirty_summary);
    let volume_states = summarize_volume_states(roots);
    let repository_rollups = storage_index.load_repository_workspace_rollups(roots);
    let ownership_generation = storage_index.load_active_ownership_generation();
    let ownership_breakdown = summarize_storage_ownership(StorageOwnershipProjectionInput {
        summaries: &summaries,
        repository_rollups: &repository_rollups,
        ownership_generation: ownership_generation.as_ref(),
        volume_states: &volume_states,
        situation_summary: &situation_summary,
        cache_status: &cache_status,
        system_volume_bytes: summarize_system_volume_usage_bytes(),
        measured_at_millis: now_millis,
    });
    StorageSituationResponse {
        captured_at_millis: now_millis,
        snapshot_updated_at_millis: Some(now_millis),
        scan_generation: storage_index.latest_published_storage_scan_generation(roots),
        cache_status,
        storage_index_status: storage_index.status.clone(),
        roots: roots
            .iter()
            .map(|root| root.display().to_string())
            .collect(),
        dirty_paths: dirty_summary,
        backlog_drain,
        recovery_plan,
        summary: situation_summary,
        top_offenders,
        domains,
        ownership_breakdown,
        volume_states,
        caveats,
    }
}

fn storage_situation_backlog_drain(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    dirty_summary: &StorageDirtyPathSummary,
    now_millis: u64,
) -> StorageSituationBacklogDrain {
    let measurement = storage_index.latest_measurement_job_debug(roots);
    storage_situation_backlog_drain_from_dirty_summary(dirty_summary, now_millis, Some(measurement))
}

fn storage_situation_backlog_drain_from_dirty_summary(
    dirty_summary: &StorageDirtyPathSummary,
    now_millis: u64,
    measurement: Option<StoragePipelineMeasurementDebug>,
) -> StorageSituationBacklogDrain {
    let measurement_status = measurement
        .as_ref()
        .and_then(|measurement| measurement.latest_status.clone());
    let latest_measurement_millis = measurement
        .as_ref()
        .and_then(|measurement| measurement.latest_updated_millis);
    let last_error = measurement
        .as_ref()
        .and_then(|measurement| measurement.latest_error.clone());
    let updated_at_millis = latest_measurement_millis
        .or(dirty_summary.latest_dirty_millis)
        .unwrap_or(now_millis);

    let (state, reason) = if dirty_summary.unknown_gap {
        ("blocked", "unknown-fsevents-gap".to_owned())
    } else if dirty_summary.dirty_path_count == 0 {
        ("idle", "queue-clean".to_owned())
    } else {
        match measurement_status.as_deref() {
            Some("pending") | Some("partial") => {
                let continuation_pending = last_error
                    .as_deref()
                    .map(|error| error.contains("budget") || error.contains("continuation"))
                    .unwrap_or(false);
                if continuation_pending {
                    (
                        "continuation-pending",
                        last_error
                            .clone()
                            .unwrap_or_else(|| "incremental-continuation-pending".to_owned()),
                    )
                } else {
                    ("pending", "incremental-measurement-pending".to_owned())
                }
            }
            Some("complete") => (
                "pending",
                "additional-dirty-paths-after-last-measurement".to_owned(),
            ),
            Some("failed") => (
                "failed",
                last_error
                    .clone()
                    .unwrap_or_else(|| "latest-measurement-failed".to_owned()),
            ),
            Some(status) => ("pending", format!("latest-measurement-{status}")),
            None => ("pending", "dirty-paths-queued".to_owned()),
        }
    };

    StorageSituationBacklogDrain {
        state: state.to_owned(),
        reason,
        updated_at_millis,
        retry_after_millis: None,
        dirty_path_count: dirty_summary.dirty_path_count,
        latest_event_id: dirty_summary.latest_event_id,
        latest_measurement_millis,
        latest_measurement_status: measurement_status,
        last_error,
        source: "storage_index".to_owned(),
    }
}

fn storage_situation_recovery_plan(
    dirty_summary: &StorageDirtyPathSummary,
) -> StorageSituationRecoveryPlan {
    if dirty_summary.unknown_gap {
        return StorageSituationRecoveryPlan {
            state: "verification-required".to_owned(),
            reason: "unknown-fsevents-gap".to_owned(),
            roots: dirty_summary.unknown_gap_roots.clone(),
            next_step: "Run a resumable verifying refresh for the affected roots when host pressure allows it; cached path facts remain displayable and indexed-path cleanup stays blocked until verification clears the gap. Runtime-owned Docker/Colima cleanup remains independently available when its live probe succeeds."
                .to_owned(),
            cleanup_blocked: true,
            automatic: true,
            source: "storage_index".to_owned(),
        };
    }
    if dirty_summary.dirty_path_count > 0 {
        return StorageSituationRecoveryPlan {
            state: "incremental-refresh-pending".to_owned(),
            reason: "dirty-paths-queued".to_owned(),
            roots: Vec::new(),
            next_step: "Continue changed-path measurement when the app scheduler allows storage work; indexed-path cleanup waits for verification, while runtime-owned Docker/Colima cleanup remains independently available when its live probe succeeds.".to_owned(),
            cleanup_blocked: true,
            automatic: true,
            source: "storage_index".to_owned(),
        };
    }
    StorageSituationRecoveryPlan {
        state: "none".to_owned(),
        reason: "clean".to_owned(),
        roots: Vec::new(),
        next_step: "No storage recovery is required for the current snapshot.".to_owned(),
        cleanup_blocked: false,
        automatic: false,
        source: "storage_index".to_owned(),
    }
}

fn overlay_storage_situation_snapshot(
    mut snapshot: StorageSituationResponse,
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    dirty_summary: StorageDirtyPathSummary,
    limit: usize,
) -> StorageSituationResponse {
    let dirty_paths = if dirty_summary.dirty_path_count == 0 {
        Vec::new()
    } else {
        storage_index.load_dirty_path_strings(roots, 512)
    };
    for offender in &mut snapshot.top_offenders {
        offender.stale = path_matches_dirty_prefix(Path::new(&offender.path), &dirty_paths);
    }
    let summaries = storage_index.load_index_summaries(roots);
    let domains = storage_index.load_storage_domains(roots, 80);
    if !domains.is_empty() {
        snapshot.domains = domains;
    }
    let refreshed_summary = summarize_storage_situation_with_domains(&summaries, &snapshot.domains);
    if refreshed_summary.item_count > 0 || refreshed_summary.inventory_size_bytes > 0 {
        snapshot.summary = merge_storage_situation_summaries(snapshot.summary, refreshed_summary);
    }
    let fresh_top_offenders = storage_index.load_situation_top_offenders(roots, limit);
    if !fresh_top_offenders.is_empty() {
        snapshot.top_offenders = fresh_top_offenders
            .into_iter()
            .map(|row| StorageSituationTopOffender {
                stale: path_matches_dirty_prefix(Path::new(&row.path), &dirty_paths),
                path: row.path,
                source_root: row.source_root,
                kind: row.kind,
                cleanup_tier: row.cleanup_tier,
                physical_bytes: row.physical_bytes,
                recommendation_score: row.recommendation_score,
                last_scan_millis: row.last_scan_millis,
            })
            .collect();
    }
    snapshot.cache_status.source = "situation_snapshot".to_owned();
    snapshot.cache_status.age_millis = Some(
        storage_now_millis().saturating_sub(
            snapshot
                .cache_status
                .latest_scan_millis
                .unwrap_or(snapshot.captured_at_millis),
        ),
    );
    if dirty_summary.dirty_path_count == 0 {
        snapshot.cache_status.message =
            "Loaded from Aetower's persisted storage situation snapshot.".to_owned();
    }
    apply_dirty_summary_to_cache_status(&mut snapshot.cache_status, &dirty_summary);
    snapshot.storage_index_status = storage_index.status.clone();
    // Volume capacity is a cheap, live filesystem fact. Never let it inherit
    // the lifetime of the persisted directory-inventory snapshot: free space
    // can change by tens of GiB without a storage scan completing.
    snapshot.volume_states = summarize_volume_states(roots);
    let repository_rollups = storage_index.load_repository_workspace_rollups(roots);
    let ownership_generation = storage_index.load_active_ownership_generation();
    snapshot.ownership_breakdown = summarize_storage_ownership(StorageOwnershipProjectionInput {
        summaries: &summaries,
        repository_rollups: &repository_rollups,
        ownership_generation: ownership_generation.as_ref(),
        volume_states: &snapshot.volume_states,
        situation_summary: &snapshot.summary,
        cache_status: &snapshot.cache_status,
        system_volume_bytes: summarize_system_volume_usage_bytes(),
        measured_at_millis: storage_now_millis(),
    });
    snapshot.snapshot_updated_at_millis = Some(storage_now_millis());
    snapshot.backlog_drain =
        storage_situation_backlog_drain(storage_index, roots, &dirty_summary, storage_now_millis());
    snapshot.recovery_plan = storage_situation_recovery_plan(&dirty_summary);
    snapshot.dirty_paths = dirty_summary;
    if snapshot.dirty_paths.unknown_gap
        && !snapshot
            .caveats
            .iter()
            .any(|caveat| caveat.contains("unknown gap"))
    {
        snapshot.caveats.push(
            "Native FSEvents reported an unknown gap; affected roots need a verifying refresh before cleanup actions are trusted."
                .to_owned(),
        );
    }
    if !snapshot
        .caveats
        .iter()
        .any(|caveat| caveat.starts_with("Snapshot-first storage situation"))
    {
        snapshot.caveats.insert(
            0,
            "Snapshot-first storage situation: loaded from the persisted materialized view before any report projection."
                .to_owned(),
        );
    }
    snapshot
}

fn build_storage_situation_response_from_report(
    report: &StorageHygieneReport,
    storage_index: &StorageSizeIndex,
    storage_index_status: String,
    dirty_summary: StorageDirtyPathSummary,
    dirty_paths: &[String],
) -> StorageSituationResponse {
    let mut cache_status = report.cache_status.clone();
    apply_dirty_summary_to_cache_status(&mut cache_status, &dirty_summary);
    let last_scan_millis = cache_status
        .latest_scan_millis
        .unwrap_or(report.captured_at_millis);
    let mut top_offenders = report
        .items
        .iter()
        .map(|item| StorageSituationTopOffender {
            path: item.path.clone(),
            source_root: source_root_for_report_path(&item.path, &report.roots),
            kind: item.kind.clone(),
            cleanup_tier: item.cleanup_tier.clone(),
            physical_bytes: item.physical_bytes,
            recommendation_score: item.recommendation_score,
            last_scan_millis,
            stale: item.stale || path_matches_dirty_prefix(Path::new(&item.path), dirty_paths),
        })
        .collect::<Vec<_>>();
    top_offenders.sort_by(|left, right| {
        right
            .recommendation_score
            .total_cmp(&left.recommendation_score)
            .then_with(|| right.physical_bytes.cmp(&left.physical_bytes))
            .then_with(|| left.path.cmp(&right.path))
    });
    top_offenders.truncate(40);

    let mut caveats = report.caveats.clone();
    if !caveats
        .iter()
        .any(|caveat| caveat.starts_with("Storage situation snapshot"))
    {
        caveats.insert(
            0,
            "Storage situation snapshot: persisted from the last completed storage report."
                .to_owned(),
        );
    }
    let report_roots = report.roots.iter().map(PathBuf::from).collect::<Vec<_>>();
    let domains =
        typed_storage_domains_for_items(&report_roots, &report.items, report.captured_at_millis);
    let backlog_drain = storage_situation_backlog_drain_from_dirty_summary(
        &dirty_summary,
        report.captured_at_millis,
        None,
    );

    let recovery_plan = storage_situation_recovery_plan(&dirty_summary);
    let summaries = storage_index.load_index_summaries(&report_roots);
    let repository_rollups = storage_index.load_repository_workspace_rollups(&report_roots);
    let ownership_generation = storage_index.load_active_ownership_generation();
    let situation_summary = StorageSituationSummary {
        source_root_count: report.roots.len(),
        item_count: report.summary.item_count.min(u64::MAX as usize) as u64,
        inventory_size_bytes: report.summary.inventory_size_bytes,
        safely_reclaimable_now_bytes: report.summary.safely_reclaimable_now_bytes,
        maybe_reclaimable_bytes: report.summary.maybe_reclaimable_bytes,
        review_required_bytes: report.summary.review_required_bytes,
        dangerous_user_data_bytes: report.summary.dangerous_user_data_bytes,
    };
    let ownership_breakdown = summarize_storage_ownership(StorageOwnershipProjectionInput {
        summaries: &summaries,
        repository_rollups: &repository_rollups,
        ownership_generation: ownership_generation.as_ref(),
        volume_states: &report.volume_states,
        situation_summary: &situation_summary,
        cache_status: &cache_status,
        system_volume_bytes: summarize_system_volume_usage_bytes(),
        measured_at_millis: report.captured_at_millis,
    });
    StorageSituationResponse {
        captured_at_millis: report.captured_at_millis,
        snapshot_updated_at_millis: Some(report.captured_at_millis),
        scan_generation: report.scan_generation.clone(),
        cache_status,
        storage_index_status,
        roots: report.roots.clone(),
        dirty_paths: dirty_summary,
        backlog_drain,
        recovery_plan,
        summary: StorageSituationSummary {
            source_root_count: report.roots.len(),
            item_count: report.summary.item_count.min(u64::MAX as usize) as u64,
            inventory_size_bytes: report.summary.inventory_size_bytes,
            safely_reclaimable_now_bytes: report.summary.safely_reclaimable_now_bytes,
            maybe_reclaimable_bytes: report.summary.maybe_reclaimable_bytes,
            review_required_bytes: report.summary.review_required_bytes,
            dangerous_user_data_bytes: report.summary.dangerous_user_data_bytes,
        },
        top_offenders,
        domains,
        ownership_breakdown,
        volume_states: report.volume_states.clone(),
        caveats,
    }
}

pub(super) struct StorageOwnershipProjectionInput<'a> {
    pub(super) summaries: &'a [StorageIndexSummaryRow],
    pub(super) repository_rollups: &'a [StorageRepositoryWorkspaceRollup],
    pub(super) ownership_generation: Option<&'a StorageOwnershipGeneration>,
    pub(super) volume_states: &'a [StorageVolumeState],
    pub(super) situation_summary: &'a StorageSituationSummary,
    pub(super) cache_status: &'a StorageCacheStatus,
    pub(super) system_volume_bytes: u64,
    pub(super) measured_at_millis: u64,
}

pub(super) fn summarize_storage_ownership(
    input: StorageOwnershipProjectionInput<'_>,
) -> StorageOwnershipBreakdown {
    let StorageOwnershipProjectionInput {
        summaries,
        repository_rollups,
        ownership_generation,
        volume_states,
        situation_summary,
        cache_status,
        system_volume_bytes,
        measured_at_millis,
    } = input;
    let used_bytes = volume_states
        .iter()
        .max_by_key(|volume| volume.total_bytes)
        .map(|volume| {
            let free = if volume.available_bytes > 0 {
                volume.available_bytes
            } else {
                volume.free_now_bytes
            };
            volume.total_bytes.saturating_sub(free)
        })
        .unwrap_or_default();

    // A broad root such as ~/Library overlaps its explicitly scanned child
    // roots. Only leaf roots participate in ownership accounting; uncovered
    // parent bytes remain in the honest volume residual instead of being
    // counted twice.
    let leaf_summaries = summaries
        .iter()
        .filter(|candidate| {
            !summaries.iter().any(|other| {
                candidate.source_root != other.source_root
                    && path_is_under_root(
                        &other.source_root,
                        Path::new(candidate.source_root.as_str()),
                    )
            })
        })
        .collect::<Vec<_>>();

    let mut raw = BTreeMap::<&'static str, (u64, u64)>::new();
    let workspace_roots = repository_rollups
        .iter()
        .map(|rollup| PathBuf::from(&rollup.root_path))
        .collect::<Vec<_>>();
    let mut skipped_repository_reclaimable = 0u64;
    raw.insert("system", (system_volume_bytes, 0));
    for row in leaf_summaries {
        let id = storage_ownership_id(Path::new(&row.source_root));
        let overlaps_workspace = workspace_roots.iter().any(|workspace_root| {
            path_is_under_root(&row.source_root, workspace_root)
                || path_is_under_root(
                    &workspace_root.display().to_string(),
                    Path::new(&row.source_root),
                )
        });
        if overlaps_workspace {
            if id == "repositories" {
                skipped_repository_reclaimable =
                    skipped_repository_reclaimable.saturating_add(row.safe_reclaimable_bytes);
            }
            continue;
        }
        let entry = raw.entry(id).or_default();
        entry.0 = entry.0.saturating_add(row.inventory_size_bytes);
        entry.1 = entry.1.saturating_add(row.safe_reclaimable_bytes);
    }

    let mut repository_sub_buckets = BTreeMap::<String, (String, u64)>::new();
    if !repository_rollups.is_empty() {
        let repository_bytes = repository_rollups.iter().fold(0u64, |total, rollup| {
            total.saturating_add(rollup.physical_bytes)
        });
        let entry = raw.entry("repositories").or_default();
        entry.0 = entry.0.saturating_add(repository_bytes);
        entry.1 = entry
            .1
            .saturating_add(skipped_repository_reclaimable)
            .min(entry.0);
        for rollup in repository_rollups {
            for bucket in &rollup.sub_buckets {
                let aggregate = repository_sub_buckets
                    .entry(bucket.id.clone())
                    .or_insert_with(|| (bucket.label.clone(), 0));
                aggregate.1 = aggregate.1.saturating_add(bucket.bytes);
            }
        }
    }

    let active_generation = ownership_generation.filter(|generation| {
        generation.classifier_version == STORAGE_OWNERSHIP_CLASSIFIER_VERSION
            && !generation.rollups.is_empty()
    });
    let mut generation_completeness = BTreeMap::<&'static str, bool>::new();
    if let Some(generation) = active_generation {
        let legacy_reclaimable = raw
            .iter()
            .map(|(id, (_, reclaimable))| (*id, *reclaimable))
            .collect::<BTreeMap<_, _>>();
        raw.clear();
        raw.insert("system", (system_volume_bytes, 0));
        repository_sub_buckets.clear();
        for rollup in &generation.rollups {
            let Some(category) = ownership::storage_ownership_category(&rollup.category_id) else {
                continue;
            };
            let entry = raw.entry(category.id).or_default();
            entry.0 = entry.0.saturating_add(rollup.physical_bytes);
            generation_completeness
                .entry(category.id)
                .and_modify(|complete| *complete &= rollup.complete)
                .or_insert(rollup.complete);
            if category.id == "repositories" {
                for bucket in &rollup.sub_buckets {
                    let aggregate = repository_sub_buckets
                        .entry(bucket.id.clone())
                        .or_insert_with(|| (bucket.label.clone(), 0));
                    aggregate.1 = aggregate.1.saturating_add(bucket.bytes);
                }
            }
        }
        for (id, (_, reclaimable)) in &mut raw {
            *reclaimable = legacy_reclaimable.get(id).copied().unwrap_or_default();
        }
    }

    let mut repository_artifacts = active_generation
        .into_iter()
        .flat_map(|generation| &generation.rollups)
        .filter(|rollup| rollup.category_id == "repositories")
        .flat_map(|rollup| rollup.repository_artifacts.iter().cloned())
        .collect::<Vec<_>>();
    for artifact in &mut repository_artifacts {
        repository_artifacts::refresh_repository_artifact_staleness(artifact, measured_at_millis);
    }
    repository_artifacts::sort_repository_artifacts(&mut repository_artifacts);
    repository_artifacts.truncate(512);
    let artifact_reclaimable = repository_artifacts
        .iter()
        .filter(|artifact| artifact.cleanup_allowed)
        .fold(0u64, |total, artifact| {
            total.saturating_add(artifact.physical_bytes)
        });
    if let Some((repository_bytes, repository_reclaimable)) = raw.get_mut("repositories") {
        *repository_reclaimable = (*repository_reclaimable)
            .max(artifact_reclaimable)
            .min(*repository_bytes);
    }

    let known_total = raw
        .values()
        .fold(0u64, |total, (bytes, _)| total.saturating_add(*bytes));
    let confidence = if cache_status.stale || cache_status.partial {
        "partial"
    } else {
        "indexed"
    }
    .to_owned();
    let mut buckets = Vec::new();
    let mut assigned_bytes = 0u64;
    let mut assigned_reclaimable = 0u64;
    for definition in STORAGE_OWNERSHIP_CATEGORIES {
        let id = definition.id;
        let (raw_bytes, raw_reclaimable) = raw.get(id).copied().unwrap_or_default();
        if raw_bytes == 0 {
            continue;
        }
        let bytes = if known_total > used_bytes && known_total > 0 {
            ((raw_bytes as u128 * used_bytes as u128) / known_total as u128) as u64
        } else {
            raw_bytes
        };
        let reclaimable_bytes = raw_reclaimable.min(bytes);
        let measured_at = active_generation
            .and_then(|generation| {
                generation
                    .rollups
                    .iter()
                    .filter(|rollup| rollup.category_id == id)
                    .map(|rollup| rollup.measured_at_millis)
                    .min()
            })
            .or_else(|| {
                (id == "repositories")
                    .then(|| {
                        repository_rollups
                            .iter()
                            .map(|rollup| rollup.measured_at_millis)
                            .min()
                    })
                    .flatten()
            });
        let repository_rollup_is_fresh = measured_at.is_some_and(|measured| {
            repository_rollups.iter().all(|rollup| rollup.complete)
                && measured_at_millis.saturating_sub(measured) < 6 * 60 * 60 * 1000
        });
        let sub_buckets = if id == "repositories" && raw_bytes > 0 {
            repository_sub_buckets
                .iter()
                .map(
                    |(sub_id, (sub_label, sub_bytes))| StorageOwnershipSubBucket {
                        id: sub_id.clone(),
                        label: sub_label.clone(),
                        bytes: if raw_bytes == bytes {
                            *sub_bytes
                        } else {
                            ((*sub_bytes as u128 * bytes as u128) / raw_bytes as u128) as u64
                        },
                    },
                )
                .collect()
        } else {
            Vec::new()
        };
        assigned_bytes = assigned_bytes.saturating_add(bytes);
        assigned_reclaimable = assigned_reclaimable.saturating_add(reclaimable_bytes);
        buckets.push(StorageOwnershipBucket {
            id: id.to_owned(),
            label: definition.label.to_owned(),
            bytes,
            reclaimable_bytes,
            source: if active_generation.is_some() {
                "ownership_generation+storage_index".to_owned()
            } else if id == "system" {
                "native_volume+storage_index".to_owned()
            } else if id == "repositories" && !repository_rollups.is_empty() {
                "repository_workspace_rollup+storage_index".to_owned()
            } else {
                "storage_index".to_owned()
            },
            confidence: if active_generation.is_some()
                && generation_completeness.get(id).copied().unwrap_or(false)
            {
                "measured".to_owned()
            } else if active_generation.is_some() && generation_completeness.contains_key(id) {
                "partial".to_owned()
            } else if id == "system" && raw_bytes == system_volume_bytes {
                "live".to_owned()
            } else if id == "repositories" && repository_rollup_is_fresh {
                "measured".to_owned()
            } else if id == "repositories" && !repository_rollups.is_empty() {
                "partial".to_owned()
            } else {
                confidence.clone()
            },
            detail: definition.detail.to_owned(),
            rank: definition.rank,
            state: if active_generation.is_some() {
                "classified".to_owned()
            } else {
                "legacy_index".to_owned()
            },
            measured_at_millis: measured_at,
            sub_buckets,
        });
    }

    let attributed_bytes = assigned_bytes.min(used_bytes);
    let unattributed_bytes = used_bytes.saturating_sub(attributed_bytes);
    let total_reclaimable = situation_summary
        .safely_reclaimable_now_bytes
        .min(used_bytes);
    if unattributed_bytes > 0 {
        let reclaimable_bytes = total_reclaimable
            .saturating_sub(assigned_reclaimable)
            .min(unattributed_bytes);
        if let Some(other) = buckets.iter_mut().find(|bucket| bucket.id == "other") {
            other.bytes = other.bytes.saturating_add(unattributed_bytes);
            other.reclaimable_bytes = other
                .reclaimable_bytes
                .saturating_add(reclaimable_bytes)
                .min(other.bytes);
            other.source = "storage_index+volume_residual".to_owned();
            other.confidence = "unattributed".to_owned();
            other.state = if active_generation.is_some() {
                "protected_or_unclassified".to_owned()
            } else {
                "volume_residual".to_owned()
            };
            other.detail =
                "Indexed miscellaneous roots plus used capacity not yet assigned to a scanned ownership root."
                    .to_owned();
        } else {
            buckets.push(StorageOwnershipBucket {
                id: "other".to_owned(),
                label: "Other".to_owned(),
                bytes: unattributed_bytes,
                reclaimable_bytes,
                source: "volume_residual".to_owned(),
                confidence: "unattributed".to_owned(),
                detail: "Used volume capacity not yet assigned to a scanned ownership root."
                    .to_owned(),
                rank: ownership::storage_ownership_category("other")
                    .map_or(90, |definition| definition.rank),
                state: if active_generation.is_some() {
                    "protected_or_unclassified".to_owned()
                } else {
                    "volume_residual".to_owned()
                },
                measured_at_millis: None,
                sub_buckets: Vec::new(),
            });
        }
    }

    StorageOwnershipBreakdown {
        used_bytes,
        attributed_bytes,
        unattributed_bytes,
        reclaimable_bytes: total_reclaimable,
        measured_at_millis,
        confidence,
        generation_id: active_generation.map(|generation| generation.generation_id),
        classifier_version: active_generation.map_or(0, |generation| generation.classifier_version),
        generation_status: active_generation.map_or_else(
            || "legacy".to_owned(),
            |generation| generation.status.clone(),
        ),
        buckets,
        repository_artifacts,
    }
}

fn storage_ownership_id(path: &Path) -> &'static str {
    let normalized = path.display().to_string().to_ascii_lowercase();
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let has_component = |value: &str| components.iter().any(|component| component == value);

    if has_component("repositories") || has_component("projects") {
        "repositories"
    } else if normalized == "/library"
        || normalized.starts_with("/private/var/db")
        || normalized.starts_with("/private/var/log")
    {
        "system"
    } else if has_component("applications")
        || normalized.contains("/library/application support")
        || normalized.contains("/library/containers")
    {
        "applications"
    } else if [
        ".colima",
        ".docker",
        ".cargo",
        ".npm",
        ".pnpm-store",
        ".cache",
        ".codex",
        ".claude",
    ]
    .iter()
    .any(|component| has_component(component))
        || normalized.contains("/library/developer")
        || normalized.contains("/library/caches/org.swift.swiftpm")
        || normalized.contains("/library/caches/com.apple.dt.xcode")
    {
        "developer"
    } else if [
        "documents",
        "desktop",
        "downloads",
        "cloudstorage",
        "mobile documents",
    ]
    .iter()
    .any(|component| has_component(component))
    {
        "personal"
    } else {
        "other"
    }
}

fn source_root_for_report_path(path: &str, roots: &[String]) -> String {
    roots
        .iter()
        .find(|root| path_is_under_root(path, Path::new(root.as_str())))
        .cloned()
        .or_else(|| roots.first().cloned())
        .unwrap_or_default()
}

fn summarize_storage_situation(rows: &[StorageIndexSummaryRow]) -> StorageSituationSummary {
    rows.iter().fold(
        StorageSituationSummary {
            source_root_count: rows.len(),
            ..StorageSituationSummary::default()
        },
        |mut summary, row| {
            summary.item_count = summary.item_count.saturating_add(row.item_count);
            summary.inventory_size_bytes = summary
                .inventory_size_bytes
                .saturating_add(row.inventory_size_bytes);
            summary.safely_reclaimable_now_bytes = summary
                .safely_reclaimable_now_bytes
                .saturating_add(row.safe_reclaimable_bytes);
            summary.maybe_reclaimable_bytes = summary
                .maybe_reclaimable_bytes
                .saturating_add(row.maybe_reclaimable_bytes);
            summary.review_required_bytes = summary
                .review_required_bytes
                .saturating_add(row.review_required_bytes);
            summary.dangerous_user_data_bytes = summary
                .dangerous_user_data_bytes
                .saturating_add(row.dangerous_user_data_bytes);
            summary
        },
    )
}

fn summarize_storage_situation_with_domains(
    rows: &[StorageIndexSummaryRow],
    domains: &[StorageSituationDomain],
) -> StorageSituationSummary {
    let mut summary = summarize_storage_situation(rows);
    let mut domain_roots = BTreeSet::new();
    let mut domain_summary = StorageSituationSummary::default();
    for domain in domains
        .iter()
        .filter(|domain| domain.source == "typed_detector")
    {
        domain_roots.insert(domain.source_root.as_str());
        domain_summary.item_count = domain_summary.item_count.saturating_add(domain.item_count);
        domain_summary.inventory_size_bytes = domain_summary
            .inventory_size_bytes
            .saturating_add(domain.physical_bytes);
        domain_summary.safely_reclaimable_now_bytes = domain_summary
            .safely_reclaimable_now_bytes
            .saturating_add(domain.safely_reclaimable_now_bytes);
        domain_summary.maybe_reclaimable_bytes = domain_summary
            .maybe_reclaimable_bytes
            .saturating_add(domain.maybe_reclaimable_bytes);
        domain_summary.review_required_bytes = domain_summary
            .review_required_bytes
            .saturating_add(domain.review_required_bytes);
        domain_summary.dangerous_user_data_bytes = domain_summary
            .dangerous_user_data_bytes
            .saturating_add(domain.dangerous_user_data_bytes);
    }
    if summary.source_root_count == 0 {
        summary.source_root_count = domain_roots.len();
    }
    merge_storage_situation_summaries(summary, domain_summary)
}

fn merge_storage_situation_summaries(
    mut summary: StorageSituationSummary,
    domain_summary: StorageSituationSummary,
) -> StorageSituationSummary {
    summary.source_root_count = summary
        .source_root_count
        .max(domain_summary.source_root_count);
    summary.item_count = summary.item_count.max(domain_summary.item_count);
    summary.inventory_size_bytes = summary
        .inventory_size_bytes
        .max(domain_summary.inventory_size_bytes);
    summary.safely_reclaimable_now_bytes = summary
        .safely_reclaimable_now_bytes
        .max(domain_summary.safely_reclaimable_now_bytes);
    summary.maybe_reclaimable_bytes = summary
        .maybe_reclaimable_bytes
        .max(domain_summary.maybe_reclaimable_bytes);
    summary.review_required_bytes = summary
        .review_required_bytes
        .max(domain_summary.review_required_bytes);
    summary.dangerous_user_data_bytes = summary
        .dangerous_user_data_bytes
        .max(domain_summary.dangerous_user_data_bytes);
    let bucket_total = summary
        .safely_reclaimable_now_bytes
        .saturating_add(summary.maybe_reclaimable_bytes)
        .saturating_add(summary.review_required_bytes)
        .saturating_add(summary.dangerous_user_data_bytes);
    summary.inventory_size_bytes = summary.inventory_size_bytes.max(bucket_total);
    summary
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
        scan_generation: report.scan_generation.clone(),
        cache_status: report.cache_status,
        diagnostics: report.diagnostics,
        cleanup_tiers: report.cleanup_tiers,
        cleanup_recipes: report.cleanup_recipes,
        cleanup_bundles: report.cleanup_bundles,
        cleanup_lanes: report.cleanup_lanes,
        duplicate_groups: report.duplicate_groups,
        redundancy_groups: report.redundancy_groups,
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
            StorageItemSortKey::Score => left
                .recommendation_score
                .total_cmp(&right.recommendation_score),
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
    let sort_key = StorageItemSortKey::parse(sort_key);
    if StorageScanMode::parse(mode) == StorageScanMode::InstantCached
        && let Some(json) = storage_hygiene_items_page_from_index(
            roots.clone(),
            offset,
            limit,
            sort_key,
            sort_descending,
        )
    {
        return json;
    }
    let requested_limit = offset.saturating_add(limit).clamp(1, MAX_LIMIT);
    let mut report =
        build_storage_hygiene_projection_report(roots, max_depth, requested_limit, mode);
    let table_started = Instant::now();
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
        scan_generation: report.scan_generation.clone(),
        cache_status: report.cache_status,
        diagnostics: report.diagnostics,
        offset,
        limit,
        sort_key: sort_key.as_str().to_owned(),
        sort_descending,
        returned_count: items.len(),
        total_available,
        has_more: offset.saturating_add(items.len()) < total_available,
        page_source: "report".to_owned(),
        items,
    })
    .map_err(|error| error.to_string())
}

/// Serve a page of items straight from the persistent index without building
/// the full projection report. Empty pages are valid cache-first answers; only
/// index connection/query failures fall back to the report-building path.
fn storage_hygiene_items_page_from_index(
    roots: Vec<String>,
    offset: usize,
    limit: usize,
    sort_key: StorageItemSortKey,
    sort_descending: bool,
) -> Option<Result<String, String>> {
    let started = Instant::now();
    let now_millis = storage_now_millis();
    let roots = normalize_roots(roots);
    let offset = offset.min(MAX_ITEMS_PAGE_OFFSET);
    let limit = limit.clamp(1, MAX_ITEMS_PAGE_LIMIT);
    let storage_index = StorageSizeIndex::open();
    let mut metrics = StorageScanMetrics {
        storage_index_status: storage_index.status.clone(),
        ..StorageScanMetrics::default()
    };
    let page = storage_index
        .load_item_rows_page(
            &roots,
            sort_key,
            sort_descending,
            offset,
            limit,
            &mut metrics,
        )
        .ok()?;
    let total_available = page.total_available.min(usize::MAX as u64) as usize;
    let mut items = page
        .rows
        .into_iter()
        .map(|row| storage_item_for_indexed_row(row, now_millis))
        .collect::<Vec<_>>();
    let writer_ledger = load_storage_writer_ledger_records();
    apply_measured_rebuild_costs(&mut items, &writer_ledger);
    apply_cleanup_guardrails(&mut items, now_millis);
    for item in &mut items {
        item.evidence = storage_item_evidence(item);
        item.next_step = storage_item_next_step(item);
    }
    let table_page_millis = started.elapsed().as_millis() as u64;
    let item_count = items.len().min(u64::MAX as usize) as u64;
    let cache_status =
        storage_index_cache_status(&storage_index, now_millis, true, total_available > 0);
    let diagnostics = StorageScanDiagnostics {
        mode: StorageScanMode::InstantCached.as_str().to_owned(),
        root_walk_millis: 0,
        size_walk_millis: 0,
        git_millis: 0,
        serialize_millis: 0,
        payload_bytes: 0,
        decode_millis: 0,
        scanned_directory_count: 0,
        discovered_repository_count: 0,
        sized_entry_count: item_count,
        candidate_seen_count: item_count,
        candidate_retained_count: item_count,
        storage_index_status: format!("items_page:{}", metrics.storage_index_status),
        storage_index_hits: metrics.storage_index_hits,
        storage_index_misses: metrics.storage_index_misses,
        storage_index_writes: 0,
        native_metadata_strategy: "persistent_index".to_owned(),
        fsevents_status: "dirty_paths_refresh_full_scan".to_owned(),
        lazy_git_status: true,
        top_k_retained: false,
        performance_budget: storage_performance_budget_diagnostics(
            StorageScanMode::InstantCached,
            0,
            0,
            item_count,
            table_page_millis,
            0,
        ),
    };
    Some(
        serde_json::to_string(&StorageHygieneItemsPageResponse {
            captured_at_millis: now_millis,
            scan_mode: StorageScanMode::InstantCached.as_str().to_owned(),
            scan_generation: storage_index.latest_published_storage_scan_generation(&roots),
            cache_status,
            diagnostics,
            offset,
            limit,
            sort_key: sort_key.as_str().to_owned(),
            sort_descending,
            returned_count: items.len(),
            total_available,
            has_more: offset.saturating_add(items.len()) < total_available,
            page_source: "index".to_owned(),
            items,
        })
        .map_err(|error| error.to_string()),
    )
}

pub fn storage_hygiene_repo_detail_json(repo_root: String, mode: &str) -> Result<String, String> {
    let report = build_storage_hygiene_projection_report(vec![repo_root.clone()], 8, 120, mode);
    let repository = report
        .repository_inventory
        .iter()
        .find(|repository| repository.repo_root == repo_root)
        .cloned();
    serde_json::to_string(&StorageHygieneRepoDetailResponse {
        captured_at_millis: report.captured_at_millis,
        scan_mode: report.scan_mode,
        scan_generation: report.scan_generation.clone(),
        cache_status: report.cache_status,
        diagnostics: report.diagnostics,
        repository,
        repo_footprints: report.repo_footprints,
        items: report.items,
        cleanup_recipes: report.cleanup_recipes,
        cleanup_bundles: report.cleanup_bundles,
        cleanup_lanes: report.cleanup_lanes,
        caveats: report.caveats,
    })
    .map_err(|error| error.to_string())
}

fn build_storage_hygiene_projection_report(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: &str,
) -> StorageHygieneReport {
    if StorageScanMode::parse(mode) == StorageScanMode::InstantCached {
        return build_storage_hygiene_report_from_index(roots, max_depth, limit);
    }
    build_storage_hygiene_report_with_mode(roots, max_depth, limit, mode)
}
