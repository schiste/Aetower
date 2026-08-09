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
    let now_millis = storage_now_millis();
    let roots = normalize_roots(roots);
    let storage_index = StorageSizeIndex::open();
    let dirty_summary = storage_index.ingest_filesystem_events(
        &load_storage_filesystem_event_records(),
        &roots,
        now_millis,
    );
    ensure_dirty_storage_subtree_measurement(&roots, &dirty_summary);
    if let Some(snapshot) = storage_index.load_situation_snapshot(&roots, limit.clamp(1, 40)) {
        let snapshot =
            overlay_storage_situation_snapshot(snapshot, &storage_index, &roots, dirty_summary);
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
    let situation_summary = summarize_storage_situation_with_domains(&summaries, &domains);
    let top_offenders = storage_index
        .load_top_offenders(roots, limit.clamp(1, 40))
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
            "Native FSEvents reported an unknown gap; affected roots need a verifying refresh before cleanup actions are trusted."
                .to_owned(),
        );
    }
    if !domains.is_empty() {
        caveats.push(
            "Typed storage domains feed the situation and reclaim buckets directly from the materialized domain view."
                .to_owned(),
        );
    }
    StorageSituationResponse {
        captured_at_millis: now_millis,
        cache_status,
        storage_index_status: storage_index.status.clone(),
        roots: roots
            .iter()
            .map(|root| root.display().to_string())
            .collect(),
        dirty_paths: dirty_summary,
        summary: situation_summary,
        top_offenders,
        domains,
        volume_states: summarize_volume_states(roots),
        caveats,
    }
}

fn overlay_storage_situation_snapshot(
    mut snapshot: StorageSituationResponse,
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    dirty_summary: StorageDirtyPathSummary,
) -> StorageSituationResponse {
    let dirty_paths = if dirty_summary.dirty_path_count == 0 {
        Vec::new()
    } else {
        storage_index.load_dirty_path_strings(roots, 512)
    };
    for offender in &mut snapshot.top_offenders {
        offender.stale = path_matches_dirty_prefix(Path::new(&offender.path), &dirty_paths);
    }
    let domains = storage_index.load_storage_domains(roots, 80);
    if !domains.is_empty() {
        snapshot.domains = domains;
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

    StorageSituationResponse {
        captured_at_millis: report.captured_at_millis,
        cache_status,
        storage_index_status,
        roots: report.roots.clone(),
        dirty_paths: dirty_summary,
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
        volume_states: report.volume_states.clone(),
        caveats,
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
    let covered_roots = rows
        .iter()
        .map(|row| row.source_root.as_str())
        .collect::<BTreeSet<_>>();
    let mut domain_roots = BTreeSet::new();
    for domain in domains
        .iter()
        .filter(|domain| domain.source == "typed_detector")
    {
        domain_roots.insert(domain.source_root.as_str());
        if covered_roots.contains(domain.source_root.as_str()) {
            continue;
        }
        summary.item_count = summary.item_count.saturating_add(domain.item_count);
        summary.inventory_size_bytes = summary
            .inventory_size_bytes
            .saturating_add(domain.physical_bytes);
        summary.safely_reclaimable_now_bytes = summary
            .safely_reclaimable_now_bytes
            .saturating_add(domain.safely_reclaimable_now_bytes);
        summary.maybe_reclaimable_bytes = summary
            .maybe_reclaimable_bytes
            .saturating_add(domain.maybe_reclaimable_bytes);
        summary.review_required_bytes = summary
            .review_required_bytes
            .saturating_add(domain.review_required_bytes);
        summary.dangerous_user_data_bytes = summary
            .dangerous_user_data_bytes
            .saturating_add(domain.dangerous_user_data_bytes);
    }
    if summary.source_root_count == 0 {
        summary.source_root_count = domain_roots.len();
    }
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
        .cloned()
        .or_else(|| report.repository_inventory.first().cloned());
    serde_json::to_string(&StorageHygieneRepoDetailResponse {
        captured_at_millis: report.captured_at_millis,
        scan_mode: report.scan_mode,
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
