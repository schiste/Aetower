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
/// the full projection report. Returns `None` when the index cannot serve the
/// request (no connection, query failure, or an empty index), which falls the
/// caller back to the report-building path.
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
    if page.total_available == 0 {
        return None;
    }
    let total_available = page.total_available.min(usize::MAX as u64) as usize;
    let mut items = page
        .rows
        .into_iter()
        .map(|row| storage_item_for_indexed_row(row, now_millis))
        .collect::<Vec<_>>();
    apply_cleanup_guardrails(&mut items, now_millis);
    for item in &mut items {
        item.evidence = storage_item_evidence(item);
        item.next_step = storage_item_next_step(item);
    }
    let table_page_millis = started.elapsed().as_millis() as u64;
    let item_count = items.len().min(u64::MAX as usize) as u64;
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
