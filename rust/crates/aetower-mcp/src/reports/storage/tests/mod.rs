use super::*;
use std::os::unix::fs::PermissionsExt;

fn storage_index_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    lock_or_recover(LOCK.get_or_init(|| Mutex::new(())))
}

#[test]
fn storage_hygiene_detects_reclaimable_build_artifacts() {
    let root = test_root("detects-build-artifacts");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }
    mark_tree_old(&root.join("project"));

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"kind\":\"rust-build\""));
    assert!(json.contains("\"safety\":\"safe\""));
    assert!(json.contains("\"cleanup_tier\":\"rebuildable\""));
    assert!(json.contains("\"cleanup_tiers\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_marks_manifest_proven_rebuildability() {
    let root = test_root("manifest-proven-rebuildability");
    let project = root.join("project");
    let target = project.join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(project.join("Cargo.toml"), "[package]\nname = \"demo\"\n") {
        panic!("write Cargo manifest: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }
    mark_tree_old(&project);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value = parse_json_value(&json, "storage JSON parses");
    let rust_build = value["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["kind"] == "rust-build"))
        .unwrap_or_else(|| panic!("rust-build item exists: {json}"));

    assert_eq!(rust_build["semantic_category"], "generated-build-output");
    assert_eq!(rust_build["taxonomy_source"], "builtin+manifest");
    assert_eq!(rust_build["rebuildability"], "manifest_proven");
    assert!(
        rust_build["manifest_evidence"]
            .as_array()
            .unwrap_or_else(|| panic!("manifest evidence array exists: {rust_build:?}"))
            .iter()
            .any(|path| path
                .as_str()
                .is_some_and(|path| path.ends_with("Cargo.toml")))
    );
    assert!(
        rust_build["evidence"]
            .as_array()
            .unwrap_or_else(|| panic!("evidence array exists: {rust_build:?}"))
            .iter()
            .any(|entry| entry
                .as_str()
                .is_some_and(|entry| entry.contains("Manifest-proven rebuildability")))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_loads_repo_local_artifact_taxonomy() {
    let root = test_root("repo-local-artifact-taxonomy");
    let project = root.join("project");
    let taxonomy_dir = project.join(".aetower");
    let model_cache = project.join("model-cache");
    if let Err(error) = fs::create_dir_all(&taxonomy_dir) {
        panic!("create taxonomy dir: {error}");
    }
    if let Err(error) = fs::create_dir_all(&model_cache) {
        panic!("create model cache: {error}");
    }
    if let Err(error) = fs::write(
        taxonomy_dir.join("storage-taxonomy.json"),
        r#"{
          "rules": [
            {
              "id": "custom-model-cache",
              "name": "model-cache",
              "kind": "llm-model-cache",
              "safety": "review",
              "cleanup_tier": "expensive",
              "reason": "Custom local LLM model cache.",
              "recommendation": "Review active model servers before cleanup.",
              "semantic_category": "ai-model-cache",
              "rebuild_command": "download model artifacts",
              "estimated_rebuild_cost": "High network cost",
              "estimated_rebuild_seconds": 2400,
              "cleanup_consequence": "Model artifacts are re-downloadable but can be large and network-bound.",
              "manifest_names": ["model-manifest.yaml"]
            }
          ]
        }"#,
    ) {
        panic!("write taxonomy: {error}");
    }
    if let Err(error) = fs::write(
        model_cache.join("weights.bin"),
        vec![4u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write model cache: {error}");
    }
    if let Err(error) = fs::write(project.join("model-manifest.yaml"), "models:\n  - local\n") {
        panic!("write model manifest: {error}");
    }
    mark_tree_old(&project);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value = parse_json_value(&json, "storage JSON parses");
    let custom_item = value["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["kind"] == "llm-model-cache"))
        .unwrap_or_else(|| panic!("custom taxonomy item exists: {json}"));

    assert_eq!(custom_item["cleanup_tier"], "expensive");
    assert_eq!(custom_item["semantic_category"], "ai-model-cache");
    assert_eq!(custom_item["taxonomy_source"], "plugin+manifest");
    assert_eq!(custom_item["rebuildability"], "manifest_proven");
    assert_eq!(custom_item["rebuild_command"], "download model artifacts");
    assert_eq!(
        custom_item["estimated_rebuild_seconds"].as_u64(),
        Some(2400)
    );
    assert_eq!(
        custom_item["cleanup_consequence"],
        "Model artifacts are re-downloadable but can be large and network-bound."
    );
    assert!(
        custom_item["manifest_evidence"]
            .as_array()
            .unwrap_or_else(|| panic!("manifest evidence array exists: {custom_item:?}"))
            .iter()
            .any(|path| path
                .as_str()
                .is_some_and(|path| path.ends_with("model-manifest.yaml")))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_fast_mode_uses_lazy_git_status_and_diagnostics() {
    let root = test_root("fast-mode-diagnostics");
    let project = root.join("Aetower");
    let target = project.join("target").join("debug");
    create_git_repo(&project, "master");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let json = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );
    let value = parse_json_value(&json, "storage JSON parses");

    assert_eq!(value["scan_mode"], "fast_changed_only");
    assert_eq!(value["diagnostics"]["lazy_git_status"], true);
    assert_eq!(value["diagnostics"]["top_k_retained"], true);
    assert_eq!(
        value["repository_inventory"][0]["git_dirty_status"],
        "not_checked_lazy"
    );
    assert_eq!(
        value["repository_inventory_coverage"][0]["repository_count"].as_u64(),
        Some(1)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_projection_apis_return_compact_shapes() {
    let root = test_root("projection-apis");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let overview = must_ok(
        storage_hygiene_overview_json(vec![root.display().to_string()], 5, "fast_changed_only"),
        "overview serializes",
    );
    let overview = parse_json_value(&overview, "overview JSON parses");
    assert!(
        overview["items"]
            .as_array()
            .is_some_and(|items| !items.is_empty() && items.len() <= 12)
    );
    assert!(overview.get("cleanup_recipes").is_some());
    assert!(overview.get("cleanup_bundles").is_some());
    assert!(overview.get("summary").is_some());
    assert!(overview.get("diagnostics").is_some());
    assert!(
        overview["treemap_roots"]
            .as_array()
            .is_some_and(|roots| !roots.is_empty())
    );
    assert_eq!(overview["treemap_roots"][0]["node_type"], "root");

    let actions =
        storage_hygiene_actions_json(vec![root.display().to_string()], 5, 80, "fast_changed_only");
    let actions = must_ok(actions, "actions serializes");
    let actions = parse_json_value(&actions, "actions JSON parses");
    assert!(actions.get("items").is_none());
    assert!(actions.get("cleanup_bundles").is_some());

    let page = storage_hygiene_items_page_json(
        vec![root.display().to_string()],
        5,
        0,
        1,
        "fast_changed_only",
        "path",
        false,
    );
    let page = must_ok(page, "items page serializes");
    let page = parse_json_value(&page, "page JSON parses");
    assert_eq!(page["sort_key"], "path");
    assert_eq!(page["sort_descending"], false);
    assert_eq!(page["returned_count"], 1);
    assert!(
        page["items"]
            .as_array()
            .is_some_and(|items| items.len() == 1)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_indexed_snapshot_reuses_persistent_rows() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("indexed-snapshot");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let scan = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );
    assert!(scan.contains("\"storage_index_writes\""));

    let overview = must_ok(
        storage_hygiene_overview_json(vec![root.display().to_string()], 5, "instant_cached"),
        "indexed overview serializes",
    );
    let overview = parse_json_value(&overview, "indexed overview JSON parses");
    assert_eq!(overview["scan_mode"], "instant_cached");
    assert_eq!(overview["diagnostics"]["root_walk_millis"], 0);
    assert!(
        overview["items"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        overview["treemap_roots"]
            .as_array()
            .is_some_and(|roots| !roots.is_empty())
    );

    let indexed = must_ok(
        storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80),
        "indexed snapshot serializes",
    );
    let indexed = parse_json_value(&indexed, "indexed JSON parses");

    assert_eq!(indexed["scan_mode"], "instant_cached");
    assert_eq!(indexed["diagnostics"]["root_walk_millis"], 0);
    assert_eq!(indexed["items"][0]["kind"], "rust-build");
    assert!(
        indexed["diagnostics"]["storage_index_hits"]
            .as_u64()
            .is_some_and(|hits| hits >= 1)
    );

    let _ = fs::remove_dir_all(root);
}

/// Staleness regression: `instant_cached` reports must re-derive from the
/// persistent index, and the index must be fed by EVERY scan mode. A fast
/// scan seeds the index, then a later deep scan discovers a new artifact —
/// the next instant_cached overview has to surface the deep scan's finding
/// instead of replaying only the earlier fast scan's rows.
#[test]
fn instant_cached_report_surfaces_rows_from_later_deep_scan() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("deep-scan-freshness");
    let fast_target = root.join("fast-project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&fast_target) {
        panic!("create fast target dir: {error}");
    }
    if let Err(error) = fs::write(
        fast_target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write fast build artifact: {error}");
    }

    let _ = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );

    let overview = must_ok(
        storage_hygiene_overview_json(vec![root.display().to_string()], 5, "instant_cached"),
        "instant overview after fast scan serializes",
    );
    let overview = parse_json_value(&overview, "instant overview after fast scan parses");
    assert_eq!(overview["scan_mode"], "instant_cached");
    let item_paths = |value: &serde_json::Value| -> Vec<String> {
        value["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["path"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let fast_paths = item_paths(&overview);
    assert!(
        fast_paths.iter().any(|path| path.contains("fast-project")),
        "fast scan rows should be visible via instant_cached: {fast_paths:?}"
    );

    // A later deep scan discovers an artifact the fast scan never saw.
    let deep_target = root.join("deep-project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&deep_target) {
        panic!("create deep target dir: {error}");
    }
    if let Err(error) = fs::write(
        deep_target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 256) as usize],
    ) {
        panic!("write deep build artifact: {error}");
    }
    let _ = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "deep_native",
    );

    let refreshed = must_ok(
        storage_hygiene_overview_json(vec![root.display().to_string()], 5, "instant_cached"),
        "instant overview after deep scan serializes",
    );
    let refreshed = parse_json_value(&refreshed, "instant overview after deep scan parses");
    assert_eq!(refreshed["scan_mode"], "instant_cached");
    let refreshed_paths = item_paths(&refreshed);
    assert!(
        refreshed_paths
            .iter()
            .any(|path| path.contains("deep-project")),
        "instant_cached must surface rows persisted by the later deep scan: {refreshed_paths:?}"
    );
    assert!(
        refreshed_paths
            .iter()
            .any(|path| path.contains("fast-project")),
        "earlier fast-scan rows must remain visible: {refreshed_paths:?}"
    );

    let _ = fs::remove_dir_all(root);
}

#[derive(Clone)]
struct SeededPageRow {
    path: String,
    size_bytes: u64,
    modified_millis: Option<u64>,
    accessed_millis: Option<u64>,
    cleanup_tier: String,
    safety: String,
    kind: String,
}

fn seed_items_page_fixture(root: &Path, count: usize) -> Vec<SeededPageRow> {
    let dir = root.join("artifacts");
    if let Err(error) = fs::create_dir_all(&dir) {
        panic!("create items page fixture dir: {error}");
    }
    let now_millis = storage_now_millis();
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();
    let tiers = ["rebuildable", "review", "risky"];
    let safeties = ["safe", "review", "risky"];
    let kinds = ["rust-build", "node-modules", "generic-cache", "large-file"];
    let mut seeded = Vec::with_capacity(count);
    for index in 0..count {
        let file = dir.join(format!("artifact-{index:04}.bin"));
        if let Err(error) = fs::write(&file, b"fixture") {
            panic!("write items page fixture file: {error}");
        }
        let physical_bytes = MIN_ITEM_BYTES + ((count - index) as u64) * 4096;
        let modified_millis =
            (index % 7 != 3).then(|| now_millis.saturating_sub(index as u64 * 61_000));
        let accessed_millis =
            (index % 5 != 2).then(|| now_millis.saturating_sub(index as u64 * 97_000));
        let row = StorageIndexedFileRow {
            path: file.display().to_string(),
            device: 7,
            inode: index as i64 + 1,
            file_id: format!("7:{index}"),
            source_root: root.display().to_string(),
            repo_root: None,
            kind: kinds[index % kinds.len()].to_owned(),
            storage_role: "build-artifact".to_owned(),
            safety: safeties[index % safeties.len()].to_owned(),
            cleanup_tier: tiers[index % tiers.len()].to_owned(),
            logical_bytes: physical_bytes,
            physical_bytes,
            modified_millis,
            changed_millis: Some(now_millis),
            accessed_millis,
            birth_millis: Some(now_millis),
            is_directory: false,
            entries: 1,
            truncated: false,
            last_scan_millis: now_millis,
        };
        storage_index.store_indexed_row(&row, &mut metrics);
        seeded.push(SeededPageRow {
            path: row.path.clone(),
            size_bytes: physical_bytes,
            modified_millis,
            accessed_millis,
            cleanup_tier: row.cleanup_tier.clone(),
            safety: row.safety.clone(),
            kind: row.kind.clone(),
        });
    }
    drop(storage_index);
    seeded
}

fn expected_page_order(seeded: &[SeededPageRow], sort_key: &str, descending: bool) -> Vec<String> {
    let mut rows = seeded.to_vec();
    rows.sort_by(|left, right| {
        let ordering = match sort_key {
            "path" => left.path.cmp(&right.path),
            "modified" => left
                .modified_millis
                .unwrap_or_default()
                .cmp(&right.modified_millis.unwrap_or_default()),
            "accessed" => left
                .accessed_millis
                .unwrap_or_default()
                .cmp(&right.accessed_millis.unwrap_or_default()),
            "tier" => left
                .cleanup_tier
                .cmp(&right.cleanup_tier)
                .then_with(|| left.safety.cmp(&right.safety)),
            "kind" => left.kind.cmp(&right.kind),
            _ => left.size_bytes.cmp(&right.size_bytes),
        };
        if ordering == Ordering::Equal {
            left.path.cmp(&right.path)
        } else if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    rows.into_iter().map(|row| row.path).collect()
}

fn items_page_paths(
    root: &Path,
    offset: usize,
    limit: usize,
    sort_key: &str,
    descending: bool,
) -> (Vec<String>, u64, String) {
    let page = must_ok(
        storage_hygiene_items_page_json(
            vec![root.display().to_string()],
            5,
            offset,
            limit,
            "instant_cached",
            sort_key,
            descending,
        ),
        "items page serializes",
    );
    let page = parse_json_value(&page, "items page JSON parses");
    let paths = page["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items page items is an array"))
        .iter()
        .map(|item| {
            item["path"]
                .as_str()
                .unwrap_or_else(|| panic!("item path is a string"))
                .to_owned()
        })
        .collect::<Vec<_>>();
    let total_available = page["total_available"]
        .as_u64()
        .unwrap_or_else(|| panic!("total_available is a number"));
    let page_source = page["page_source"]
        .as_str()
        .unwrap_or_else(|| panic!("page_source is a string"))
        .to_owned();
    (paths, total_available, page_source)
}

#[test]
fn storage_hygiene_items_page_serves_beyond_two_hundred_rows_from_index() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("items-page-beyond-200");
    let seeded = seed_items_page_fixture(&root, 260);
    let expected = expected_page_order(&seeded, "size", true);

    let (paths, total_available, page_source) = items_page_paths(&root, 250, 20, "size", true);
    assert_eq!(page_source, "index");
    assert_eq!(total_available, 260);
    assert_eq!(paths, expected[250..260].to_vec());

    let (full, total_available, page_source) = items_page_paths(&root, 0, 260, "size", true);
    assert_eq!(page_source, "index");
    assert_eq!(total_available, 260);
    assert_eq!(full.len(), 260, "page limit must exceed the legacy 200 cap");
    assert_eq!(full, expected);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_items_page_offset_is_stable_for_each_sort_key() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("items-page-sort-keys");
    let seeded = seed_items_page_fixture(&root, 48);

    for sort_key in ["size", "path", "modified", "accessed", "tier", "kind"] {
        for descending in [true, false] {
            let expected = expected_page_order(&seeded, sort_key, descending);
            let (full, total_available, page_source) =
                items_page_paths(&root, 0, 60, sort_key, descending);
            assert_eq!(page_source, "index", "sort {sort_key} desc={descending}");
            assert_eq!(total_available, 48, "sort {sort_key} desc={descending}");
            assert_eq!(full, expected, "sort {sort_key} desc={descending}");

            let (middle, _, _) = items_page_paths(&root, 13, 17, sort_key, descending);
            assert_eq!(
                middle,
                expected[13..30].to_vec(),
                "offset page mismatch for sort {sort_key} desc={descending}"
            );
        }
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_items_page_evicts_missing_rows_and_refills() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("items-page-stale-eviction");
    let seeded = seed_items_page_fixture(&root, 10);
    let expected = expected_page_order(&seeded, "size", true);
    // The two largest rows land on page one; delete their backing files.
    for stale_path in &expected[0..2] {
        if let Err(error) = fs::remove_file(stale_path) {
            panic!("delete stale fixture file: {error}");
        }
    }

    let (paths, total_available, page_source) = items_page_paths(&root, 0, 5, "size", true);
    assert_eq!(page_source, "index");
    assert_eq!(total_available, 8, "stale rows are evicted before counting");
    assert_eq!(
        paths,
        expected[2..7].to_vec(),
        "page refills with the next live rows"
    );

    // The eviction is persistent: a fresh request must not resurrect the rows.
    let (paths, total_available, _) = items_page_paths(&root, 0, 10, "size", true);
    assert_eq!(total_available, 8);
    assert_eq!(paths, expected[2..10].to_vec());

    let _ = fs::remove_dir_all(root);
}

fn batched_flush_row(
    root: &Path,
    name: &str,
    physical_bytes: u64,
    cleanup_tier: &str,
    last_scan_millis: u64,
) -> StorageIndexedFileRow {
    let path = root.join(name);
    StorageIndexedFileRow {
        path: path.display().to_string(),
        device: 11,
        inode: 1,
        file_id: format!("11:{name}"),
        source_root: root.display().to_string(),
        repo_root: None,
        kind: "rust-build".to_owned(),
        storage_role: "build-artifact".to_owned(),
        safety: "safe".to_owned(),
        cleanup_tier: cleanup_tier.to_owned(),
        logical_bytes: physical_bytes,
        physical_bytes,
        modified_millis: Some(last_scan_millis),
        changed_millis: Some(last_scan_millis),
        accessed_millis: Some(last_scan_millis),
        birth_millis: Some(last_scan_millis),
        is_directory: false,
        entries: 1,
        truncated: false,
        last_scan_millis,
    }
}

#[test]
fn storage_index_batched_flush_matches_per_row_growth_semantics() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("batched-flush-parity");
    let prefix = format!("{}/", root.display());
    let now_millis = storage_now_millis();
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();

    // First generation: 600 rows crosses one 512-row chunk boundary, so one
    // flush happens automatically and the remainder stays buffered.
    for index in 0..600usize {
        let row = batched_flush_row(
            &root,
            &format!("row-{index:04}.bin"),
            MIN_ITEM_BYTES + index as u64,
            "rebuildable",
            now_millis,
        );
        storage_index.store_indexed_row(&row, &mut metrics);
    }
    assert_eq!(metrics.storage_index_writes, 600);
    assert_eq!(
        storage_index.pending_row_count(),
        600 - STORAGE_INDEX_FLUSH_CHUNK,
        "chunk-full flush drains exactly one chunk"
    );
    assert_eq!(storage_index.count_indexed_rows_with_prefix(&prefix), 600);
    let deltas = storage_index.growth_deltas_with_prefix(&prefix);
    assert_eq!(deltas.len(), 600, "every new row records one growth delta");
    assert!(
        deltas
            .iter()
            .all(|(_, previous, current, delta, _)| *previous == 0 && *delta == *current as i64)
    );

    // Second generation: half the rows grow and change tier, half are stored
    // again unchanged. Only changed byte counts may record deltas, and the
    // previous cleanup tier must be captured for every row.
    for index in 0..600usize {
        let (physical_bytes, tier) = if index < 300 {
            ((MIN_ITEM_BYTES + index as u64) * 2, "review")
        } else {
            (MIN_ITEM_BYTES + index as u64, "rebuildable")
        };
        let row = batched_flush_row(
            &root,
            &format!("row-{index:04}.bin"),
            physical_bytes,
            tier,
            now_millis,
        );
        storage_index.store_indexed_row(&row, &mut metrics);
    }
    storage_index.flush_pending_rows();
    assert_eq!(storage_index.count_indexed_rows_with_prefix(&prefix), 600);

    let changed_path = root.join("row-0007.bin").display().to_string();
    let unchanged_path = root.join("row-0420.bin").display().to_string();
    let (changed_bytes, changed_tier, changed_previous_tier) = storage_index
        .indexed_row_tier_snapshot(&changed_path)
        .unwrap_or_else(|| panic!("changed row snapshot exists"));
    assert_eq!(changed_bytes, (MIN_ITEM_BYTES + 7) * 2);
    assert_eq!(changed_tier, "review");
    assert_eq!(changed_previous_tier, "rebuildable");
    let (unchanged_bytes, _, unchanged_previous_tier) = storage_index
        .indexed_row_tier_snapshot(&unchanged_path)
        .unwrap_or_else(|| panic!("unchanged row snapshot exists"));
    assert_eq!(unchanged_bytes, MIN_ITEM_BYTES + 420);
    assert_eq!(unchanged_previous_tier, "rebuildable");

    let deltas = storage_index.growth_deltas_with_prefix(&prefix);
    assert_eq!(
        deltas.len(),
        900,
        "600 first-seen deltas plus 300 growth deltas for the changed half"
    );
    let changed_deltas = deltas
        .iter()
        .filter(|(path, _, _, _, _)| *path == changed_path)
        .collect::<Vec<_>>();
    assert_eq!(changed_deltas.len(), 2);
    assert_eq!(changed_deltas[1].1, MIN_ITEM_BYTES + 7);
    assert_eq!(changed_deltas[1].2, (MIN_ITEM_BYTES + 7) * 2);
    assert_eq!(changed_deltas[1].3, (MIN_ITEM_BYTES + 7) as i64);
    assert_eq!(
        deltas
            .iter()
            .filter(|(path, _, _, _, _)| *path == unchanged_path)
            .count(),
        1,
        "re-storing an unchanged row must not record a delta"
    );

    // A path stored twice inside one buffered chunk keeps the sequential
    // per-row semantics: the second store compares against the first.
    let twice_path = root.join("stored-twice.bin").display().to_string();
    storage_index.store_indexed_row(
        &batched_flush_row(&root, "stored-twice.bin", 1_000, "rebuildable", now_millis),
        &mut metrics,
    );
    storage_index.store_indexed_row(
        &batched_flush_row(&root, "stored-twice.bin", 4_000, "review", now_millis),
        &mut metrics,
    );
    storage_index.flush_pending_rows();
    let twice_deltas = storage_index.growth_deltas_with_prefix(&twice_path);
    assert_eq!(twice_deltas.len(), 2);
    assert_eq!((twice_deltas[0].1, twice_deltas[0].2), (0, 1_000));
    assert_eq!((twice_deltas[1].1, twice_deltas[1].2), (1_000, 4_000));
    let (twice_bytes, twice_tier, twice_previous_tier) = storage_index
        .indexed_row_tier_snapshot(&twice_path)
        .unwrap_or_else(|| panic!("twice-stored row snapshot exists"));
    assert_eq!(twice_bytes, 4_000);
    assert_eq!(twice_tier, "review");
    assert_eq!(twice_previous_tier, "rebuildable");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_index_buffered_rows_are_visible_to_reads_without_explicit_flush() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("batched-flush-read-your-writes");
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("create read-your-writes fixture dir: {error}");
    }
    let now_millis = storage_now_millis();
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();
    for index in 0..5usize {
        let name = format!("buffered-{index}.bin");
        if let Err(error) = fs::write(root.join(&name), b"fixture") {
            panic!("write buffered fixture file: {error}");
        }
        let row = batched_flush_row(
            &root,
            &name,
            MIN_ITEM_BYTES + index as u64,
            "rebuildable",
            now_millis,
        );
        storage_index.store_indexed_row(&row, &mut metrics);
    }
    assert_eq!(storage_index.pending_row_count(), 5);

    let page = match storage_index.load_item_rows_page(
        std::slice::from_ref(&root),
        StorageItemSortKey::Size,
        true,
        0,
        10,
        &mut metrics,
    ) {
        Ok(page) => page,
        Err(error) => panic!("load buffered rows page: {error}"),
    };
    assert_eq!(page.total_available, 5, "reads observe buffered rows");
    assert_eq!(page.rows.len(), 5);
    assert_eq!(storage_index.pending_row_count(), 0);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_index_growth_delta_retention_prunes_once_per_flush() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("batched-flush-retention");
    let prefix = format!("{}/", root.display());
    let now_millis = storage_now_millis();
    let stale_scan_millis = now_millis.saturating_sub(40 * 24 * 60 * 60 * 1000);
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();

    storage_index.store_indexed_row(
        &batched_flush_row(
            &root,
            "old-delta.bin",
            2_048,
            "rebuildable",
            stale_scan_millis,
        ),
        &mut metrics,
    );
    storage_index.flush_pending_rows();

    storage_index.store_indexed_row(
        &batched_flush_row(&root, "fresh-delta.bin", 4_096, "rebuildable", now_millis),
        &mut metrics,
    );
    storage_index.flush_pending_rows();

    let deltas = storage_index.growth_deltas_with_prefix(&prefix);
    assert_eq!(
        deltas.len(),
        1,
        "the per-flush retention delete prunes deltas older than 30 days"
    );
    assert!(deltas[0].0.ends_with("fresh-delta.bin"));

    let _ = fs::remove_dir_all(root);
}

#[allow(clippy::too_many_arguments)]
fn seeded_index_row(
    source_root: &Path,
    path: &Path,
    physical_bytes: u64,
    cleanup_tier: &str,
    repo_root: Option<&Path>,
    modified_millis: Option<u64>,
    accessed_millis: Option<u64>,
    last_scan_millis: u64,
) -> StorageIndexedFileRow {
    StorageIndexedFileRow {
        path: path.display().to_string(),
        device: 13,
        inode: 1,
        file_id: format!("13:{}", path.display()),
        source_root: source_root.display().to_string(),
        repo_root: repo_root.map(|repo| repo.display().to_string()),
        kind: "rust-build".to_owned(),
        storage_role: "build-artifact".to_owned(),
        safety: "safe".to_owned(),
        cleanup_tier: cleanup_tier.to_owned(),
        logical_bytes: physical_bytes,
        physical_bytes,
        modified_millis,
        changed_millis: modified_millis,
        accessed_millis,
        birth_millis: modified_millis,
        is_directory: false,
        entries: 1,
        truncated: false,
        last_scan_millis,
    }
}

fn test_volume_state(path: &str, free_now_bytes: u64) -> StorageVolumeState {
    StorageVolumeState {
        path: path.to_owned(),
        device_id: 99,
        filesystem_type: "apfs".to_owned(),
        total_bytes: free_now_bytes * 4,
        free_now_bytes,
        available_bytes: free_now_bytes,
        purgeable_bytes_estimate: 0,
        important_usage_available_bytes: None,
        opportunistic_usage_available_bytes: None,
        detail: String::new(),
    }
}

fn test_volume_state_with_capacity(
    path: &str,
    free_now_bytes: u64,
    available_bytes: u64,
    purgeable_bytes_estimate: u64,
    important_usage_available_bytes: Option<u64>,
    opportunistic_usage_available_bytes: Option<u64>,
) -> StorageVolumeState {
    StorageVolumeState {
        path: path.to_owned(),
        device_id: 100,
        filesystem_type: "apfs".to_owned(),
        total_bytes: available_bytes * 4,
        free_now_bytes,
        available_bytes,
        purgeable_bytes_estimate,
        important_usage_available_bytes,
        opportunistic_usage_available_bytes,
        detail: String::new(),
    }
}

#[test]
fn storage_growth_insights_aggregate_rates_trends_and_forecasts() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("growth-insights-rates");
    let source_a = root.join("roots-a");
    let source_b = root.join("roots-b");
    let source_c = root.join("roots-c");
    let repo_a = source_a.join("RepoA");
    let now_millis = storage_now_millis();
    let day = |offset: u64| now_millis - offset * DAY_MILLIS;
    let mib = MIN_ITEM_BYTES;
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();

    // Repo scope A: 10 + 10 MiB in the first half, 20 + 40 MiB in the second
    // half => accelerating, 80 MiB over a 4-day span = 20 MiB/day.
    for (name, bytes, scan_millis) in [
        ("a-d3.bin", 10 * mib, day(3)),
        ("a-d2.bin", 10 * mib, day(2)),
        ("a-d1.bin", 20 * mib, day(1)),
        ("a-d0.bin", 40 * mib, day(0)),
    ] {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &source_a,
                &repo_a.join("target").join(name),
                bytes,
                "rebuildable",
                Some(&repo_a),
                Some(scan_millis),
                Some(scan_millis),
                scan_millis,
            ),
            &mut metrics,
        );
    }
    // Root B: 30 MiB early, 10 MiB late => slowing.
    for (name, bytes, scan_millis) in [
        ("b-d3.bin", 30 * mib, day(3)),
        ("b-d0.bin", 10 * mib, day(0)),
    ] {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &source_b,
                &source_b.join(name),
                bytes,
                "rebuildable",
                None,
                Some(scan_millis),
                Some(scan_millis),
                scan_millis,
            ),
            &mut metrics,
        );
    }
    // Root C: equal halves => steady.
    for (name, bytes, scan_millis) in [
        ("c-d1.bin", 10 * mib, day(1)),
        ("c-d0.bin", 10 * mib, day(0)),
    ] {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &source_c,
                &source_c.join(name),
                bytes,
                "rebuildable",
                None,
                Some(scan_millis),
                Some(scan_millis),
                scan_millis,
            ),
            &mut metrics,
        );
    }
    storage_index.flush_pending_rows();

    let volumes = vec![test_volume_state("/", 350 * mib)];
    let insights = storage_index
        .load_growth_insights(std::slice::from_ref(&root), &volumes, now_millis, 30)
        .unwrap_or_else(|| panic!("growth insights load from a ready index"));

    assert_eq!(insights.window_days, 30);
    assert_eq!(insights.per_repo_rates.len(), 1);
    let repo_rate = &insights.per_repo_rates[0];
    assert_eq!(repo_rate.scope, repo_a.display().to_string());
    assert_eq!(repo_rate.scope_kind, "repo");
    assert_eq!(repo_rate.repo_name.as_deref(), Some("RepoA"));
    assert_eq!(repo_rate.total_delta_bytes, (80 * mib) as i64);
    assert_eq!(repo_rate.daily_rate_bytes, (20 * mib) as i64);
    assert_eq!(repo_rate.trend, "accelerating");
    assert_eq!(repo_rate.day_bucket_count, 4);

    let root_rates = &insights.per_root_rates;
    assert_eq!(root_rates.len(), 3);
    assert_eq!(root_rates[0].scope, source_a.display().to_string());
    assert_eq!(root_rates[0].trend, "accelerating");
    assert_eq!(root_rates[1].scope, source_b.display().to_string());
    assert_eq!(root_rates[1].trend, "slowing");
    assert_eq!(root_rates[1].daily_rate_bytes, (10 * mib) as i64);
    assert_eq!(root_rates[2].scope, source_c.display().to_string());
    assert_eq!(root_rates[2].trend, "steady");

    assert_eq!(insights.volume_forecasts.len(), 1);
    let forecast = &insights.volume_forecasts[0];
    assert_eq!(forecast.volume_path, "/");
    assert_eq!(forecast.free_now_bytes, 350 * mib);
    assert_eq!(forecast.available_bytes, 350 * mib);
    assert_eq!(forecast.daily_rate_bytes, (35 * mib) as i64);
    assert!(forecast.daily_rate_lower_bytes < forecast.daily_rate_bytes);
    assert!(forecast.daily_rate_upper_bytes > forecast.daily_rate_bytes);
    assert!((forecast.days_to_full - 10.0).abs() < 0.01);
    assert!(forecast.days_to_full_lower_bound < forecast.days_to_full);
    assert!(forecast.days_to_full_upper_bound > forecast.days_to_full);
    assert_eq!(forecast.confidence, "low");
    assert_eq!(forecast.seasonal_pattern, "insufficient-history");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_forecast_reports_seasonality_purgeable_and_cloud_dynamics() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("growth-forecast-dynamics");
    let source = root.join("RepoA");
    let cloud_source = root
        .join("Library")
        .join("CloudStorage")
        .join("ExampleDrive");
    let now_millis = storage_now_millis();
    let day = |offset: u64| now_millis - offset * DAY_MILLIS;
    let mib = MIN_ITEM_BYTES;
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();

    // Two weekly peaks on the same epoch weekday plus lower baseline growth
    // across the other retained day buckets.
    for offset in 0..14u64 {
        let bytes = if offset == 0 || offset == 7 {
            100 * mib
        } else {
            10 * mib
        };
        storage_index.store_indexed_row(
            &seeded_index_row(
                &source,
                &source.join("target").join(format!("day-{offset}.bin")),
                bytes,
                "rebuildable",
                Some(&source),
                Some(day(offset)),
                Some(day(offset)),
                day(offset),
            ),
            &mut metrics,
        );
        storage_index.store_indexed_row(
            &seeded_index_row(
                &cloud_source,
                &cloud_source.join(format!("cloud-day-{offset}.bin")),
                10 * mib,
                "rebuildable",
                None,
                Some(day(offset)),
                Some(day(offset)),
                day(offset),
            ),
            &mut metrics,
        );
    }
    storage_index.flush_pending_rows();

    let volumes = vec![test_volume_state_with_capacity(
        "/",
        1_400 * mib,
        1_700 * mib,
        300 * mib,
        Some(1_600 * mib),
        Some(1_900 * mib),
    )];
    let insights = storage_index
        .load_growth_insights(std::slice::from_ref(&root), &volumes, now_millis, 30)
        .unwrap_or_else(|| panic!("growth insights load from a ready index"));

    let forecast = insights
        .volume_forecasts
        .first()
        .unwrap_or_else(|| panic!("forecast emitted with 14 retained buckets"));
    assert_eq!(forecast.seasonal_pattern, "weekly-peak");
    assert_eq!(forecast.confidence, "medium");
    assert!(forecast.volatility_percent > 0);
    assert_eq!(forecast.cloud_daily_rate_bytes, (10 * mib) as i64);
    assert!(forecast.cloud_growth_share_percent > 0);
    assert_eq!(forecast.purgeable_bytes_estimate, 300 * mib);
    assert!(forecast.purgeable_cushion_days > 0.0);
    assert!(forecast.days_to_available_full > forecast.days_to_full);
    assert!(forecast.days_to_effective_full > forecast.days_to_full);
    assert!(forecast.days_to_full_lower_bound < forecast.days_to_full);
    assert!(forecast.days_to_full_upper_bound > forecast.days_to_full);
    assert!(
        forecast
            .forecast_notes
            .iter()
            .any(|note| note.contains("Purgeable APFS space"))
    );
    assert!(
        forecast
            .forecast_notes
            .iter()
            .any(|note| note.contains("Cloud-backed paths"))
    );

    let top_root = insights
        .per_root_rates
        .iter()
        .find(|rate| rate.scope == source.display().to_string())
        .unwrap_or_else(|| panic!("source-root rate emitted"));
    assert_eq!(top_root.seasonal_pattern, "weekly-peak");
    assert!(top_root.daily_rate_lower_bytes < top_root.daily_rate_bytes);
    assert!(top_root.daily_rate_upper_bytes > top_root.daily_rate_bytes);
    assert_eq!(top_root.confidence, "medium");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_insights_detects_folder_growth_anomalies() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("growth-insights-anomalies");
    let repo = root.join("RepoA");
    let target = repo.join("target");
    let steady = repo.join("logs");
    let now_millis = storage_now_millis();
    let day = |offset: u64| now_millis - offset * DAY_MILLIS;
    let mib = MIN_ITEM_BYTES;
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();

    // Baseline: target grows by only 4 MiB per scan for four retained buckets.
    for (scan_millis, bytes) in [
        (day(4), 4 * mib),
        (day(3), 8 * mib),
        (day(2), 12 * mib),
        (day(1), 16 * mib),
    ] {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &root,
                &target,
                bytes,
                "rebuildable",
                Some(&repo),
                Some(scan_millis),
                Some(scan_millis),
                scan_millis,
            ),
            &mut metrics,
        );
        storage_index.flush_pending_rows();
    }

    // Control lane: a normal latest growth row below the anomaly threshold.
    for (scan_millis, bytes) in [(day(1), 4 * mib), (now_millis, 10 * mib)] {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &root,
                &steady,
                bytes,
                "safe",
                Some(&repo),
                Some(scan_millis),
                Some(scan_millis),
                scan_millis,
            ),
            &mut metrics,
        );
        storage_index.flush_pending_rows();
    }

    // Latest scan: target jumps by 80 MiB versus its quiet baseline.
    storage_index.store_indexed_row(
        &seeded_index_row(
            &root,
            &target,
            96 * mib,
            "rebuildable",
            Some(&repo),
            Some(now_millis),
            Some(now_millis),
            now_millis,
        ),
        &mut metrics,
    );
    storage_index.flush_pending_rows();

    let insights = storage_index
        .load_growth_insights(std::slice::from_ref(&root), &[], now_millis, 30)
        .unwrap_or_else(|| panic!("growth insights load from a ready index"));

    assert_eq!(insights.growth_anomalies.len(), 1);
    let anomaly = &insights.growth_anomalies[0];
    assert_eq!(anomaly.path, target.display().to_string());
    assert_eq!(anomaly.display_name, "target");
    assert_eq!(anomaly.repo_name.as_deref(), Some("RepoA"));
    assert_eq!(anomaly.anomaly_kind, "baseline-spike");
    assert_eq!(anomaly.severity, "critical");
    assert_eq!(anomaly.confidence, "medium");
    assert_eq!(anomaly.current_delta_bytes, 80 * mib);
    assert_eq!(anomaly.baseline_bucket_count, 4);
    assert_eq!(anomaly.baseline_mean_bytes, 4 * mib);
    assert!(anomaly.current_to_baseline_ratio >= 20.0);
    assert!(anomaly.summary.contains("grew by"));
    assert!(anomaly.summary.contains("20x"));
    assert!(
        anomaly
            .evidence
            .iter()
            .any(|entry| entry.contains("Baseline over 30d"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_insights_forecast_requires_three_day_buckets() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("growth-insights-forecast-gate");
    let now_millis = storage_now_millis();
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();
    for (name, scan_millis) in [
        ("gate-d1.bin", now_millis - DAY_MILLIS),
        ("gate-d0.bin", now_millis),
    ] {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &root,
                &root.join(name),
                8 * MIN_ITEM_BYTES,
                "rebuildable",
                None,
                Some(scan_millis),
                Some(scan_millis),
                scan_millis,
            ),
            &mut metrics,
        );
    }
    storage_index.flush_pending_rows();

    let volumes = vec![test_volume_state("/", 350 * MIN_ITEM_BYTES)];
    let insights = storage_index
        .load_growth_insights(std::slice::from_ref(&root), &volumes, now_millis, 30)
        .unwrap_or_else(|| panic!("growth insights load from a ready index"));

    assert!(
        insights.volume_forecasts.is_empty(),
        "fewer than three distinct day buckets must omit the forecast"
    );
    assert!(!insights.per_root_rates.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_insights_report_since_last_scan_diff() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("growth-insights-scan-diff");
    let now_millis = storage_now_millis();
    let first_scan_millis = now_millis - 2 * 60 * 60 * 1000;
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();

    // First generation: two rows.
    for (name, tier) in [("f1.bin", "rebuildable"), ("f2.bin", "safe")] {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &root,
                &root.join(name),
                2 * MIN_ITEM_BYTES,
                tier,
                None,
                Some(first_scan_millis),
                Some(first_scan_millis),
                first_scan_millis,
            ),
            &mut metrics,
        );
    }
    storage_index.flush_pending_rows();

    // Second generation: f1 grows and changes tier, f2 is unchanged, f3 is new.
    for (name, bytes, tier) in [
        ("f1.bin", 4 * MIN_ITEM_BYTES, "review"),
        ("f2.bin", 2 * MIN_ITEM_BYTES, "safe"),
        ("f3.bin", 3 * MIN_ITEM_BYTES, "safe"),
    ] {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &root,
                &root.join(name),
                bytes,
                tier,
                None,
                Some(now_millis),
                Some(now_millis),
                now_millis,
            ),
            &mut metrics,
        );
    }
    storage_index.flush_pending_rows();

    let insights = storage_index
        .load_growth_insights(std::slice::from_ref(&root), &[], now_millis, 30)
        .unwrap_or_else(|| panic!("growth insights load from a ready index"));
    let diff = &insights.since_last_scan;

    assert_eq!(diff.latest_scan_millis, now_millis);
    assert_eq!(diff.appeared_count, 1);
    assert_eq!(diff.appeared_total_bytes, 3 * MIN_ITEM_BYTES);
    assert_eq!(diff.appeared.len(), 1);
    assert!(diff.appeared[0].path.ends_with("f3.bin"));
    assert_eq!(diff.appeared[0].display_name, "f3.bin");

    assert_eq!(diff.tier_changed_count, 1);
    assert_eq!(diff.tier_changed.len(), 1);
    assert!(diff.tier_changed[0].path.ends_with("f1.bin"));
    assert_eq!(diff.tier_changed[0].previous_cleanup_tier, "rebuildable");
    assert_eq!(diff.tier_changed[0].cleanup_tier, "review");
    assert_eq!(diff.tier_changed[0].physical_bytes, 4 * MIN_ITEM_BYTES);

    assert!(diff.disappeared.is_empty());
    assert!(diff.disappeared_note.contains("not cleanly derivable"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_cold_data_lane_bands_and_exclusions() {
    let root = test_root("cold-data-lane");
    let now_millis = storage_now_millis();
    let age = |days: u64| now_millis - days * DAY_MILLIS;
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();

    type ColdFixtureRow = (&'static str, u64, &'static str, Option<u64>, Option<u64>);
    let rows: [ColdFixtureRow; 7] = [
        (
            "c1-cold.bin",
            5 * MIN_ITEM_BYTES,
            "safe",
            Some(age(400)),
            Some(age(400)),
        ),
        (
            "c2-cooling.bin",
            3 * MIN_ITEM_BYTES,
            "rebuildable",
            Some(age(120)),
            Some(age(200)),
        ),
        (
            "c3-fresh.bin",
            3 * MIN_ITEM_BYTES,
            "safe",
            Some(age(10)),
            Some(age(10)),
        ),
        (
            "c4-risky.bin",
            8 * MIN_ITEM_BYTES,
            "risky",
            Some(age(400)),
            Some(age(400)),
        ),
        (
            "c5-small.bin",
            MIN_ITEM_BYTES / 2,
            "safe",
            Some(age(400)),
            Some(age(400)),
        ),
        ("c6-no-times.bin", 3 * MIN_ITEM_BYTES, "safe", None, None),
        (
            "c7-recent-mod.bin",
            3 * MIN_ITEM_BYTES,
            "safe",
            Some(age(400)),
            Some(age(30)),
        ),
    ];
    for (name, bytes, tier, accessed, modified) in rows {
        storage_index.store_indexed_row(
            &seeded_index_row(
                &root,
                &root.join(name),
                bytes,
                tier,
                None,
                modified,
                accessed,
                now_millis,
            ),
            &mut metrics,
        );
    }
    storage_index.flush_pending_rows();

    let cold_data =
        build_storage_cold_data(&storage_index, std::slice::from_ref(&root), now_millis)
            .unwrap_or_else(|| panic!("cold data lane builds from a ready index"));

    assert_eq!(cold_data.bands.len(), 2);
    let yearly = &cold_data.bands[0];
    assert_eq!(yearly.min_age_days, COLD_AFTER_DAYS);
    assert_eq!(yearly.max_age_days, None);
    assert_eq!(yearly.item_count, 1);
    assert_eq!(yearly.total_bytes, 5 * MIN_ITEM_BYTES);
    assert_eq!(yearly.top_items.len(), 1);
    assert!(yearly.top_items[0].path.ends_with("c1-cold.bin"));
    assert!(yearly.top_items[0].recommendation_score > 0.0);

    let cooling = &cold_data.bands[1];
    assert_eq!(cooling.min_age_days, COLD_COOLING_AFTER_DAYS);
    assert_eq!(cooling.max_age_days, Some(COLD_AFTER_DAYS));
    assert_eq!(cooling.item_count, 1);
    assert_eq!(cooling.total_bytes, 3 * MIN_ITEM_BYTES);
    assert!(cooling.top_items[0].path.ends_with("c2-cooling.bin"));

    assert!(cold_data.caveat.contains("max(accessed, modified)"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_cold_data_blocks_paths_with_active_file_holders() {
    let path = "/tmp/aetower-cold-active-fixture.log";
    let mut item = test_storage_item(path, "cold-file", "cold-data", "safe", "safe", 1_000);
    item.cold = true;
    item.access_age_days = Some(COLD_AFTER_DAYS + 1);

    let mut holders_by_path = BTreeMap::new();
    holders_by_path.insert(
        path.to_owned(),
        vec![CleanupPathHolder {
            pid: 42,
            command: "tail".to_owned(),
            fd: "3r".to_owned(),
        }],
    );
    let mut items = vec![item];
    apply_active_cleanup_holders(&mut items, &holders_by_path);
    items[0].evidence = storage_item_evidence(&items[0]);
    items[0].next_step = storage_item_next_step(&items[0]);

    assert!(!items[0].cleanup_allowed);
    assert_eq!(items[0].default_cleanup_action, "manual_review");
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|blocker| blocker.contains("Active file handle detected"))
    );
    assert!(
        items[0]
            .attribution
            .notes
            .iter()
            .any(|note| note.contains("tail pid 42 fd 3r"))
    );
    assert!(
        items[0]
            .evidence
            .iter()
            .any(|evidence| evidence.contains("Active file handle detected"))
    );
}

#[test]
fn active_file_holders_block_general_cleanup_candidates() {
    let path = "/tmp/aetower-active-cleanup-fixture/target";
    let old_millis = crate::current_unix_millis()
        .unwrap_or_default()
        .saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let item = test_storage_item(
        path,
        "rust-build",
        "build-artifact",
        "safe",
        "rebuildable",
        old_millis,
    );
    let mut holders_by_path = BTreeMap::new();
    holders_by_path.insert(
        path.to_owned(),
        vec![CleanupPathHolder {
            pid: 99,
            command: "cargo".to_owned(),
            fd: "cwd".to_owned(),
        }],
    );
    let mut items = vec![item];

    apply_active_cleanup_holders(&mut items, &holders_by_path);

    assert!(!items[0].cleanup_allowed);
    assert_eq!(items[0].default_cleanup_action, "manual_review");
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|blocker| blocker.contains("cargo pid 99 fd cwd"))
    );
}

#[test]
fn storage_recommendation_score_formula_weights_size_tier_and_staleness() {
    let now_millis = storage_now_millis();
    let fresh = Some(now_millis);
    let stale = Some(now_millis - 360 * DAY_MILLIS);
    let score = |bytes: u64, tier: &str, touched: Option<u64>| {
        storage_recommendation_score(bytes, tier, touched, touched, now_millis)
    };

    // Size is log2-scaled above 1 MiB.
    assert!((score(4 * MIN_ITEM_BYTES, "safe", fresh) - 2.0).abs() < 1e-9);
    assert!((score(8 * MIN_ITEM_BYTES, "safe", fresh) - 3.0).abs() < 1e-9);
    assert_eq!(score(MIN_ITEM_BYTES / 2, "safe", fresh), 0.0);
    // Tier weights: safe 1.0, rebuildable 0.8, expensive 0.3, risky 0.1, else 0.
    assert!((score(4 * MIN_ITEM_BYTES, "rebuildable", fresh) - 1.6).abs() < 1e-9);
    assert!((score(4 * MIN_ITEM_BYTES, "expensive", fresh) - 0.6).abs() < 1e-9);
    assert!((score(4 * MIN_ITEM_BYTES, "risky", fresh) - 0.2).abs() < 1e-9);
    assert_eq!(score(4 * MIN_ITEM_BYTES, "", fresh), 0.0);
    // Staleness caps at a 2x multiplier past 180 days.
    assert!((score(4 * MIN_ITEM_BYTES, "safe", stale) - 4.0).abs() < 1e-9);
    let ninety_days = Some(now_millis - 90 * DAY_MILLIS);
    assert!((score(4 * MIN_ITEM_BYTES, "safe", ninety_days) - 3.0).abs() < 1e-9);
    // The newest of modified/accessed drives staleness.
    assert!(
        (storage_recommendation_score(4 * MIN_ITEM_BYTES, "safe", stale, fresh, now_millis) - 2.0)
            .abs()
            < 1e-9
    );
    // No timestamps: no staleness boost rather than a guessed one.
    assert!(
        (storage_recommendation_score(4 * MIN_ITEM_BYTES, "safe", None, None, now_millis) - 2.0)
            .abs()
            < 1e-9
    );
}

#[test]
fn storage_items_page_sorts_by_persisted_recommendation_score() {
    let root = test_root("items-page-score-sort");
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("create score fixture dir: {error}");
    }
    let now_millis = storage_now_millis();
    let stale_millis = now_millis - 360 * DAY_MILLIS;
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();

    // Expected scores: s3 6.0, s1 2.0, s2 1.6, s4 1.5, s5 0.5.
    let rows: [(&str, u64, &str, u64); 5] = [
        ("s1-safe-fresh.bin", 4 * MIN_ITEM_BYTES, "safe", now_millis),
        (
            "s2-rebuildable-fresh.bin",
            4 * MIN_ITEM_BYTES,
            "rebuildable",
            now_millis,
        ),
        (
            "s3-safe-stale.bin",
            8 * MIN_ITEM_BYTES,
            "safe",
            stale_millis,
        ),
        (
            "s4-expensive-fresh.bin",
            32 * MIN_ITEM_BYTES,
            "expensive",
            now_millis,
        ),
        (
            "s5-risky-fresh.bin",
            32 * MIN_ITEM_BYTES,
            "risky",
            now_millis,
        ),
    ];
    for (name, bytes, tier, touched_millis) in rows {
        let path = root.join(name);
        if let Err(error) = fs::write(&path, b"fixture") {
            panic!("write score fixture file: {error}");
        }
        storage_index.store_indexed_row(
            &seeded_index_row(
                &root,
                &path,
                bytes,
                tier,
                None,
                Some(touched_millis),
                Some(touched_millis),
                now_millis,
            ),
            &mut metrics,
        );
    }
    storage_index.flush_pending_rows();

    let persisted = storage_index
        .indexed_row_recommendation_score(&root.join("s3-safe-stale.bin").display().to_string())
        .unwrap_or_else(|| panic!("persisted score exists"));
    assert!(
        (persisted - 6.0).abs() < 1e-9,
        "persisted score {persisted}"
    );

    let (paths, total_available, page_source) = items_page_paths(&root, 0, 10, "score", true);
    assert_eq!(page_source, "index");
    assert_eq!(total_available, 5);
    let expected = [
        "s3-safe-stale.bin",
        "s1-safe-fresh.bin",
        "s2-rebuildable-fresh.bin",
        "s4-expensive-fresh.bin",
        "s5-risky-fresh.bin",
    ]
    .map(|name| root.join(name).display().to_string());
    assert_eq!(paths, expected.to_vec());

    // Ascending flips the order, and the serialized items carry the score.
    let (ascending, _, _) = items_page_paths(&root, 0, 10, "score", false);
    assert_eq!(
        ascending,
        expected.iter().rev().cloned().collect::<Vec<_>>()
    );
    let page = must_ok(
        storage_hygiene_items_page_json(
            vec![root.display().to_string()],
            5,
            0,
            1,
            "instant_cached",
            "score",
            true,
        ),
        "score page serializes",
    );
    let page = parse_json_value(&page, "score page JSON parses");
    assert_eq!(page["sort_key"], "score");
    let top_score = page["items"][0]["recommendation_score"]
        .as_f64()
        .unwrap_or_else(|| panic!("recommendation_score serialized"));
    assert!(
        (top_score - 6.0).abs() < 1e-9,
        "serialized score {top_score}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_attributes_artifacts_to_git_repo_and_branch() {
    let root = test_root("attributes-artifacts");
    let project = root.join("Aetower");
    let target = project.join("target").join("debug");
    create_git_repo(&project, "master");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        project.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/aetower-ffi\"]\n",
    ) {
        panic!("write cargo manifest: {error}");
    }
    let artifact = target.join("blob");
    if let Err(error) = fs::write(&artifact, vec![0u8; (MIN_ITEM_BYTES + 128) as usize]) {
        panic!("write build artifact: {error}");
    }
    let artifact_metadata = fs::metadata(&artifact).expect("artifact metadata is readable");
    let expected_logical_bytes = artifact_metadata.len();
    let expected_physical_bytes = artifact_metadata.blocks().saturating_mul(512);
    mark_tree_old(&project);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"repo_name\":\"Aetower\""));
    assert!(json.contains("\"git_branch\":\"master\""));
    assert!(json.contains("\"git_head\":\"1234567890ab\""));
    assert!(json.contains("\"attributed_repo_count\":1"));
    assert!(json.contains("\"repo_footprints\""));
    assert!(json.contains("\"repository_inventory\""));
    assert!(json.contains(&format!("\"current_size_bytes\":{expected_physical_bytes}")));
    assert!(json.contains(&format!("\"logical_bytes\":{expected_logical_bytes}")));
    assert!(json.contains(&format!("\"physical_bytes\":{expected_physical_bytes}")));
    assert!(json.contains("\"top_artifact_folders\""));
    assert!(json.contains("\"artifact_mix\""));
    assert!(json.contains("\"label\":\"Cargo target\""));
    assert!(json.contains("\"rebuild_command\":\"cargo build\""));
    assert!(json.contains("\"duplicate_clone_count\":1"));
    assert!(json.contains("\"last_branch_touched\":\"master\""));
    assert!(json.contains("\"estimated_rebuild_cost\":\"Low\""));
    assert!(json.contains("\"last_writer_process\":null"));
    assert!(json.contains("\"cleanup_recipes\""));
    assert!(json.contains("\"title\":\"Clean Rust package aetower-ffi\""));
    assert!(json.contains("cargo clean -p aetower-ffi"));
    assert!(json.contains("\"cleanup_bundles\""));
    assert!(json.contains("\"id\":\"safe-reclaim\""));
    assert!(json.contains("\"dry_run_only\":true"));
    assert!(json.contains("\"confidence_score\":97"));
    assert!(json.contains("\"rollback_notes\""));
    assert!(json.contains("\"budget_guardrails\""));
    assert!(json.contains("\"repo_growth_budget_bytes_per_day\":2147483648"));
    assert!(json.contains("\"total_artifact_budget_bytes\":32212254720"));
    assert!(json.contains("\"free_space_floor_bytes\":21474836480"));
    assert!(json.contains("\"volume_pressure_floor_percent\":10"));
    assert!(json.contains("\"warning_only_by_default\":true"));
    assert!(json.contains("\"auto_trash_safe_tier_enabled\":false"));
    assert!(json.contains("\"scheduled_scan_interval_hours\":24"));
    assert!(json.contains("\"id\":\"safe-tier-auto-trash\""));
    assert!(json.contains("\"enabled\":false"));
    assert!(json.contains("\"prevention_suggestions\""));
    assert!(json.contains("\"action_label\":\"Review scan policy\""));
    let value = parse_json_value(&json, "storage hygiene JSON parses");
    let budget_violations = value["budget_guardrails"]["violations"]
        .as_array()
        .unwrap_or_else(|| panic!("budget guardrail violations serialize as an array"));
    assert!(
        budget_violations
            .iter()
            .all(|violation| violation["scope"] == "volume-pressure")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_repo_footprint_surfaces_duplicate_clone_groups() {
    let root = test_root("repo-footprint-clone-groups");
    let clone_a = root.join("CloneA");
    let clone_b = root.join("CloneB");
    create_git_repo(&clone_a, "main");
    create_git_repo(&clone_b, "main");
    for repo in [&clone_a, &clone_b] {
        if let Err(error) = fs::write(
            repo.join(".git").join("config"),
            "[remote \"origin\"]\n\turl = git@github.com:example/shared.git\n",
        ) {
            panic!("write git remote config: {error}");
        }
    }
    let target = clone_a.join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value = parse_json_value(&json, "storage hygiene clone group JSON parses");
    let footprints = value["repo_footprints"]
        .as_array()
        .unwrap_or_else(|| panic!("repo footprints is an array"));
    let clone_a_root = clone_a.display().to_string();
    let clone_a_footprint = footprints
        .iter()
        .find(|footprint| footprint["repo_root"].as_str() == Some(clone_a_root.as_str()))
        .unwrap_or_else(|| panic!("clone A footprint is present"));

    assert_eq!(clone_a_footprint["duplicate_clone_count"].as_u64(), Some(2));
    assert!(
        clone_a_footprint["duplicate_clone_roots"]
            .as_array()
            .is_some_and(|roots| roots.len() == 2)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_redundancy_beyond_byte_duplicates() {
    let root = test_root("redundancy-beyond-duplicates");
    let rust_a = root.join("RepoA").join("target").join("debug");
    let rust_b = root.join("RepoB").join("target").join("debug");
    let npm_a = root.join("CacheA").join(".npm").join("_cacache");
    let npm_b = root.join("CacheB").join(".npm").join("_cacache");
    let downloads = root.join("Downloads");
    for directory in [&rust_a, &rust_b, &npm_a, &npm_b, &downloads] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create redundancy fixture directory: {error}");
        }
    }
    for path in [
        rust_a.join("artifact"),
        rust_b.join("artifact"),
        npm_a.join("blob"),
        npm_b.join("blob"),
    ] {
        if let Err(error) = fs::write(path, vec![7u8; (MIN_ITEM_BYTES + 256) as usize]) {
            panic!("write redundancy fixture artifact: {error}");
        }
    }

    let sparse_a = downloads.join("clone-candidate-a.bin");
    let sparse_b = downloads.join("clone-candidate-b.bin");
    for (path, marker) in [(&sparse_a, 1u8), (&sparse_b, 2u8)] {
        let mut file = fs::File::create(path).unwrap_or_else(|error| {
            panic!(
                "create sparse redundancy fixture file {}: {error}",
                path.display()
            )
        });
        if let Err(error) = file.write_all(&[marker]) {
            panic!("write sparse redundancy fixture marker: {error}");
        }
        if let Err(error) = file.set_len(LARGE_FILE_BYTES + MIN_ITEM_BYTES) {
            panic!("size sparse redundancy fixture file: {error}");
        }
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 6, 120);
    let value = parse_json_value(&json, "redundancy JSON parses");
    let groups = value["redundancy_groups"]
        .as_array()
        .unwrap_or_else(|| panic!("redundancy groups serialize as an array"));
    let classes = groups
        .iter()
        .filter_map(|group| group["redundancy_class"].as_str())
        .collect::<BTreeSet<_>>();

    assert!(classes.contains("generated-output-equivalence"));
    assert!(classes.contains("package-store-overlap"));
    assert!(classes.contains("shared-block-candidates"));

    let shared = groups
        .iter()
        .find(|group| group["redundancy_class"] == "shared-block-candidates")
        .unwrap_or_else(|| panic!("shared-block redundancy group is present"));
    assert!(
        shared["total_bytes"].as_u64().unwrap_or_default()
            > shared["reclaimable_bytes"].as_u64().unwrap_or_default()
    );
    assert!(
        shared["caveat"]
            .as_str()
            .is_some_and(|caveat| caveat.contains("exact APFS clone lineage"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_similar_bucket_confirms_only_full_hash_matches() {
    let root = test_root("similar-bucket-full-hash-filter");
    let downloads = root.join("Downloads");
    if let Err(error) = fs::create_dir_all(&downloads) {
        panic!("create similar bucket fixture directory: {error}");
    }

    let content_len = (MIN_ITEM_BYTES + 512) as usize;
    let exact = vec![7u8; content_len];
    let mut edge_match_decoy = exact.clone();
    edge_match_decoy[DUPLICATE_PARTIAL_HASH_BYTES + 16] = 9;
    for (path, content) in [
        (downloads.join("exact-a.pkg"), exact.clone()),
        (downloads.join("exact-b.pkg"), exact),
        (downloads.join("edge-match-decoy.pkg"), edge_match_decoy),
    ] {
        if let Err(error) = fs::write(path, content) {
            panic!("write similar bucket candidate: {error}");
        }
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 4, 120);
    let value = parse_json_value(&json, "similar bucket JSON parses");
    let groups = value["duplicate_groups"]
        .as_array()
        .unwrap_or_else(|| panic!("duplicate groups serialize as an array"));
    let confirmed = groups
        .iter()
        .find(|group| group["confirmed"].as_bool() == Some(true))
        .unwrap_or_else(|| panic!("full-hash confirmed exact match group is present"));
    assert_eq!(confirmed["file_count"].as_u64(), Some(2));
    assert!(
        confirmed["candidate_key"]
            .as_str()
            .is_some_and(|key| key.starts_with("sha256:"))
    );
    assert!(
        confirmed["caveat"]
            .as_str()
            .is_some_and(|caveat| caveat.contains("same-size and partial-content hashing"))
    );
    let paths = confirmed["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("confirmed paths serialize as an array"))
        .iter()
        .filter_map(|item| item["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path.ends_with("exact-a.pkg")));
    assert!(paths.iter().any(|path| path.ends_with("exact-b.pkg")));
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("edge-match-decoy.pkg"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_similar_bucket_groups_visually_similar_images() {
    let root = test_root("similar-bucket-image-ahash");
    let screenshots = root.join("Screenshots");
    if let Err(error) = fs::create_dir_all(&screenshots) {
        panic!("create image similarity fixture directory: {error}");
    }

    write_similarity_png(&screenshots.join("screenshot-a.png"), 0);
    write_similarity_png(&screenshots.join("screenshot-b.png"), 1);
    write_contrast_png(&screenshots.join("unrelated.png"));

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 4, 120);
    let value = parse_json_value(&json, "image similarity JSON parses");
    let groups = value["duplicate_groups"]
        .as_array()
        .unwrap_or_else(|| panic!("duplicate groups serialize as an array"));
    let image_group = groups
        .iter()
        .find(|group| {
            group["candidate_key"]
                .as_str()
                .is_some_and(|key| key.starts_with("image-ahash:"))
        })
        .unwrap_or_else(|| panic!("image perceptual-hash group is present"));

    assert_eq!(image_group["confirmed"].as_bool(), Some(false));
    assert_eq!(image_group["confidence_score"].as_u64(), Some(82));
    assert!(
        image_group["recommendation"]
            .as_str()
            .is_some_and(|recommendation| recommendation.contains("Potentially similar images"))
    );
    let paths = image_group["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("image group paths serialize as an array"))
        .iter()
        .filter_map(|item| item["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path.ends_with("screenshot-a.png")));
    assert!(paths.iter().any(|path| path.ends_with("screenshot-b.png")));
    assert!(!paths.iter().any(|path| path.ends_with("unrelated.png")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_similar_bucket_groups_near_identical_text() {
    let root = test_root("similar-bucket-text-simhash");
    let exports = root.join("Exports");
    if let Err(error) = fs::create_dir_all(&exports) {
        panic!("create text similarity fixture directory: {error}");
    }

    write_similarity_markdown(&exports.join("agent-report-a.md"), 0);
    write_similarity_markdown(&exports.join("agent-report-b.md"), 1);
    write_unrelated_markdown(&exports.join("unrelated-report.md"));

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 4, 120);
    let value = parse_json_value(&json, "text similarity JSON parses");
    let groups = value["duplicate_groups"]
        .as_array()
        .unwrap_or_else(|| panic!("duplicate groups serialize as an array"));
    let text_group = groups
        .iter()
        .find(|group| {
            group["candidate_key"]
                .as_str()
                .is_some_and(|key| key.starts_with("text-simhash:"))
        })
        .unwrap_or_else(|| panic!("text SimHash group is present"));

    assert_eq!(text_group["confirmed"].as_bool(), Some(false));
    assert_eq!(text_group["confidence_score"].as_u64(), Some(76));
    assert!(
        text_group["recommendation"]
            .as_str()
            .is_some_and(|recommendation| recommendation.contains("Potentially similar text"))
    );
    let paths = text_group["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("text group paths serialize as an array"))
        .iter()
        .filter_map(|item| item["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path.ends_with("agent-report-a.md")));
    assert!(paths.iter().any(|path| path.ends_with("agent-report-b.md")));
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("unrelated-report.md"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_similar_bucket_groups_extracted_document_text() {
    let root = test_root("similar-bucket-document-text");
    let documents = root.join("Documents");
    if let Err(error) = fs::create_dir_all(&documents) {
        panic!("create document similarity fixture directory: {error}");
    }

    write_similarity_pdf(&documents.join("audit-a.pdf"), 0);
    write_similarity_pdf(&documents.join("audit-b.pdf"), 1);
    write_similarity_docx(&documents.join("brief-a.docx"), 0);
    write_similarity_docx(&documents.join("brief-b.docx"), 1);
    write_unrelated_docx(&documents.join("unrelated.docx"));

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 6, 120);
    let value = parse_json_value(&json, "document similarity JSON parses");
    let groups = value["duplicate_groups"]
        .as_array()
        .unwrap_or_else(|| panic!("duplicate groups serialize as an array"));
    let text_groups = groups
        .iter()
        .filter(|group| {
            group["candidate_key"]
                .as_str()
                .is_some_and(|key| key.starts_with("text-simhash:"))
        })
        .collect::<Vec<_>>();
    assert!(
        text_groups.len() >= 2,
        "PDF and DOCX extracted text groups are present"
    );
    assert!(
        text_groups.iter().any(|group| {
            let paths = group["paths"].as_array().cloned().unwrap_or_default();
            paths
                .iter()
                .filter_map(|item| item["path"].as_str())
                .any(|path| path.ends_with("audit-a.pdf"))
                && paths
                    .iter()
                    .filter_map(|item| item["path"].as_str())
                    .any(|path| path.ends_with("audit-b.pdf"))
        }),
        "PDF extracted text group is present"
    );
    assert!(
        text_groups.iter().any(|group| {
            let paths = group["paths"].as_array().cloned().unwrap_or_default();
            paths
                .iter()
                .filter_map(|item| item["path"].as_str())
                .any(|path| path.ends_with("brief-a.docx"))
                && paths
                    .iter()
                    .filter_map(|item| item["path"].as_str())
                    .any(|path| path.ends_with("brief-b.docx"))
                && !paths
                    .iter()
                    .filter_map(|item| item["path"].as_str())
                    .any(|path| path.ends_with("unrelated.docx"))
        }),
        "DOCX extracted text group is present without unrelated document"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_similar_bucket_groups_video_signatures() {
    let root = test_root("similar-bucket-video-signature");
    let videos = root.join("Videos");
    if let Err(error) = fs::create_dir_all(&videos) {
        panic!("create video similarity fixture directory: {error}");
    }

    write_similarity_mov(&videos.join("clip-a.mov"), 0);
    write_similarity_mov(&videos.join("clip-b.mov"), 1);
    write_unrelated_mov(&videos.join("unrelated.mov"));

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 120);
    let value = parse_json_value(&json, "video similarity JSON parses");
    let groups = value["duplicate_groups"]
        .as_array()
        .unwrap_or_else(|| panic!("duplicate groups serialize as an array"));
    let video_group = groups
        .iter()
        .find(|group| {
            group["candidate_key"]
                .as_str()
                .is_some_and(|key| key.starts_with("video-signature:"))
        })
        .unwrap_or_else(|| panic!("video signature group is present"));

    assert_eq!(video_group["confirmed"].as_bool(), Some(false));
    assert_eq!(video_group["confidence_score"].as_u64(), Some(70));
    assert!(
        video_group["recommendation"]
            .as_str()
            .is_some_and(|recommendation| recommendation.contains("Potentially similar videos"))
    );
    let paths = video_group["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("video group paths serialize as an array"))
        .iter()
        .filter_map(|item| item["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path.ends_with("clip-a.mov")));
    assert!(paths.iter().any(|path| path.ends_with("clip-b.mov")));
    assert!(!paths.iter().any(|path| path.ends_with("unrelated.mov")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_similar_bucket_groups_generic_binaries() {
    let root = test_root("similar-bucket-binary-cdc");
    let binaries = root.join("Binaries");
    if let Err(error) = fs::create_dir_all(&binaries) {
        panic!("create binary similarity fixture directory: {error}");
    }

    write_similarity_binary(&binaries.join("artifact-a.bin"), 0);
    write_similarity_binary(&binaries.join("artifact-b.bin"), 1);
    write_unrelated_binary(&binaries.join("unrelated.bin"));

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 120);
    let value = parse_json_value(&json, "binary similarity JSON parses");
    let groups = value["duplicate_groups"]
        .as_array()
        .unwrap_or_else(|| panic!("duplicate groups serialize as an array"));
    let binary_group = groups
        .iter()
        .find(|group| {
            group["candidate_key"]
                .as_str()
                .is_some_and(|key| key.starts_with("binary-cdc:"))
        })
        .unwrap_or_else(|| panic!("binary CDC group is present"));

    assert_eq!(binary_group["confirmed"].as_bool(), Some(false));
    assert_eq!(binary_group["confidence_score"].as_u64(), Some(52));
    assert!(
        binary_group["caveat"]
            .as_str()
            .is_some_and(|caveat| caveat.contains("lower-confidence"))
    );
    let paths = binary_group["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("binary group paths serialize as an array"))
        .iter()
        .filter_map(|item| item["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path.ends_with("artifact-a.bin")));
    assert!(paths.iter().any(|path| path.ends_with("artifact-b.bin")));
    assert!(!paths.iter().any(|path| path.ends_with("unrelated.bin")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_indexes_git_repositories_without_artifacts() {
    let root = test_root("indexes-clean-repositories");
    let repo_a = root.join("CleanOne");
    let repo_b = root.join("Nested").join("CleanTwo");
    create_git_repo(&repo_a, "main");
    create_git_repo(&repo_b, "feature/clean");

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };

    assert_eq!(inventory.len(), 2);
    assert!(inventory.iter().any(|repo| repo["repo_name"] == "CleanOne"));
    assert!(inventory.iter().any(|repo| repo["repo_name"] == "CleanTwo"));
    assert!(
        inventory
            .iter()
            .any(|repo| repo["git_branch"] == "feature/clean")
    );
    assert!(value.get("repo_footprints").is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_reports_agent_guidance_quality() {
    let root = test_root("repository-quality");
    let valid_repo = root.join("ValidGuidance");
    let invalid_repo = root.join("InvalidGuidance");
    let oversized_repo = root.join("OversizedGuidance");
    create_git_repo(&valid_repo, "main");
    create_git_repo(&invalid_repo, "main");
    create_git_repo(&oversized_repo, "main");
    if let Err(error) = fs::write(valid_repo.join("AGENTS.md"), "# Agent guidance\n") {
        panic!("write AGENTS.md: {error}");
    }
    if let Err(error) = fs::write(
        valid_repo.join("CLAUDE.md"),
        "@AGENTS.md\n\n## Claude Code\n\nSupplemental Claude-specific notes.\n",
    ) {
        panic!("write CLAUDE.md: {error}");
    }
    if let Err(error) = fs::write(invalid_repo.join("CLAUDE.md"), "# Claude guidance\n") {
        panic!("write invalid CLAUDE.md: {error}");
    }
    if let Err(error) = fs::write(
        oversized_repo.join("CLAUDE.md"),
        format!(
            "@AGENTS.md\n{}",
            "x".repeat(CLAUDE_MD_DELEGATION_MAX_BYTES as usize)
        ),
    ) {
        panic!("write oversized CLAUDE.md: {error}");
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let valid = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "ValidGuidance")
    {
        Some(repo) => repo,
        None => panic!("valid repo is indexed"),
    };
    let invalid = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "InvalidGuidance")
    {
        Some(repo) => repo,
        None => panic!("invalid repo is indexed"),
    };
    let oversized = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "OversizedGuidance")
    {
        Some(repo) => repo,
        None => panic!("oversized repo is indexed"),
    };

    assert_eq!(valid["has_agents_md"], true);
    assert_eq!(valid["has_claude_md"], true);
    assert_eq!(valid["claude_md_delegates_to_agents_md"], true);
    assert_eq!(
        valid["claude_md_delegation_max_bytes"].as_u64(),
        Some(CLAUDE_MD_DELEGATION_MAX_BYTES)
    );
    assert_eq!(invalid["has_agents_md"], false);
    assert_eq!(invalid["has_claude_md"], true);
    assert_eq!(invalid["claude_md_delegates_to_agents_md"], false);
    assert_eq!(oversized["has_claude_md"], true);
    assert_eq!(oversized["claude_md_delegates_to_agents_md"], false);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_reports_agent_guidance_policy_issues() {
    let root = test_root("agent-guidance-policy");
    let missing_repo = root.join("MissingAgents");
    let untracked_repo = root.join("UntrackedAgents");
    create_git_repo(&missing_repo, "main");
    create_git_repo(&untracked_repo, "main");
    if let Err(error) = fs::write(
        untracked_repo.join("AGENTS.md"),
        "## Scope And Precedence\n\nUse targeted staging.\n",
    ) {
        panic!("write AGENTS.md: {error}");
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let missing = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "MissingAgents")
    {
        Some(repo) => repo,
        None => panic!("missing repo is indexed"),
    };
    let untracked = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "UntrackedAgents")
    {
        Some(repo) => repo,
        None => panic!("untracked repo is indexed"),
    };

    assert_eq!(missing["agent_guidance_status"], "error");
    assert!(guidance_issue_ids(missing).contains(&"agents.root.missing".to_owned()));
    assert_eq!(untracked["agent_guidance_status"], "error");
    assert!(guidance_issue_ids(untracked).contains(&"agents.root.untracked".to_owned()));
    assert!(guidance_issue_ids(untracked).contains(&"agents.sections.missing".to_owned()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_reports_agent_contract_coverage() {
    let root = test_root("agent-contract-coverage");
    let ready_repo = root.join("ReadyRepo");
    let partial_repo = root.join("PartialRepo");
    create_indexed_git_repo(&ready_repo);
    create_indexed_git_repo(&partial_repo);
    write_complete_agent_contracts(&ready_repo);
    write_minimal_agents_contract(&partial_repo);
    git_add_all(&ready_repo);
    git_add_all(&partial_repo);

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let ready = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "ReadyRepo")
    {
        Some(repo) => repo,
        None => panic!("ready repo is indexed"),
    };
    let partial = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "PartialRepo")
    {
        Some(repo) => repo,
        None => panic!("partial repo is indexed"),
    };

    assert_eq!(ready["agent_readiness_status"], "ready");
    assert_eq!(ready["agent_readiness_score"].as_u64(), Some(100));
    assert_eq!(ready["agent_contract_missing_count"].as_u64(), Some(0));
    assert_eq!(
        ready["agent_contract_coverage"].as_array().map(Vec::len),
        Some(10)
    );
    assert_eq!(partial["agent_readiness_status"], "weak");
    assert_eq!(partial["agent_contract_missing_count"].as_u64(), Some(9));
    assert!(guidance_issue_ids(partial).contains(&"agents.contract.missing.repo_map".to_owned()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_pending_human_review_does_not_count_as_reviewed_contract() {
    let root = test_root("pending-human-review-contract");
    let repo = root.join("PendingReviewRepo");
    create_indexed_git_repo(&repo);
    write_complete_agent_contracts(&repo);
    let tasks = repo.join(".agents").join("tasks.yaml");
    if let Err(error) = fs::write(
        &tasks,
        [
            "schema_version: 1",
            "reviewed_by: pending-human-review",
            "reviewed_at: 2026-06-29T00:00:00Z",
            "source_files:",
            "  - AGENTS.md",
            "tasks: []",
            "",
        ]
        .join("\n"),
    ) {
        panic!("write pending review tasks contract: {error}");
    }
    git_add_all(&repo);

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let repository = inventory
        .iter()
        .find(|repo| repo["repo_name"] == "PendingReviewRepo")
        .unwrap_or_else(|| panic!("pending review repo is indexed"));
    let tasks_coverage = repository["agent_contract_coverage"]
        .as_array()
        .and_then(|coverage| coverage.iter().find(|contract| contract["id"] == "tasks"))
        .unwrap_or_else(|| panic!("tasks coverage is present"));

    assert_eq!(tasks_coverage["status"], "partial");
    assert_eq!(tasks_coverage["reviewed"], false);
    assert!(
        tasks_coverage["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("reviewed_by/reviewed_at"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn agent_contract_weights_sum_to_readiness_max() {
    let total: u64 = agent_contract_definitions()
        .iter()
        .map(|definition| definition.weight)
        .sum();
    assert_eq!(total, AGENT_READINESS_MAX_SCORE);
}

#[test]
fn agent_guidance_parser_normalizes_numbered_sections_and_prohibitions() {
    let content = [
        "## 1. Scope And Precedence",
        "",
        "Never use `git add .`, `git add -A`, or `git commit -a`.",
        "",
        "## 2. Repository Map",
        "",
        "## 11. References",
        "",
        "docs/missing.md",
    ]
    .join("\n");
    let headings = markdown_headings(&content)
        .into_iter()
        .map(|heading| heading.title)
        .collect::<Vec<_>>();

    assert_eq!(
        headings,
        vec![
            "Scope And Precedence".to_owned(),
            "Repository Map".to_owned(),
            "References".to_owned()
        ]
    );
    let root = test_root("numbered-references");
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("create numbered references root: {error}");
    }
    let mut issues = Vec::new();
    audit_reference_paths(&mut issues, &root, "AGENTS.md", &content);
    assert!(
        issues
            .iter()
            .any(|issue| issue.id == "agents.references.missing_path")
    );
    let _ = fs::remove_dir_all(root);
    assert!(line_explicitly_prohibits_command(
        "Never use `git add .`, `git add -A`, or `git commit -a`."
    ));
    assert!(!line_explicitly_prohibits_command(
        "Run `git add .` before committing."
    ));
}

#[test]
fn repository_inventory_groups_duplicate_git_remotes() {
    let root = test_root("duplicate-remotes");
    let repo_a = root.join("Mockup");
    let repo_b = root.join("Mockup-Frontend");
    let repo_c = root.join("Aetower");
    create_git_repo(&repo_a, "main");
    create_git_repo(&repo_b, "feature/companion");
    create_git_repo(&repo_c, "main");
    write_git_origin_config(&repo_a, "https://github.com/Aeptus/mockup.git");
    write_git_origin_config(&repo_b, "git@github.com:Aeptus/mockup.git");
    write_git_origin_config(&repo_c, "https://github.com/schiste/Aetower.git");

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let duplicate = match inventory.iter().find(|repo| repo["repo_name"] == "Mockup") {
        Some(repo) => repo,
        None => panic!("duplicate repo is indexed"),
    };
    let unique = match inventory.iter().find(|repo| repo["repo_name"] == "Aetower") {
        Some(repo) => repo,
        None => panic!("unique repo is indexed"),
    };

    assert_eq!(duplicate["git_remote_key"], "github.com/aeptus/mockup");
    assert_eq!(duplicate["git_remote_host"], "github.com");
    assert_eq!(duplicate["git_remote_owner"], "aeptus");
    assert_eq!(duplicate["git_remote_name"], "mockup");
    assert_eq!(duplicate["clone_group_count"], 2);
    assert_eq!(
        duplicate["clone_group_roots"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(unique["clone_group_count"], 1);
    assert_eq!(
        unique["clone_group_roots"].as_array().map(Vec::len),
        Some(0)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_redacts_git_remote_credentials() {
    let root = test_root("remote-redaction");
    let repo = root.join("PrivateRepo");
    create_git_repo(&repo, "main");
    write_git_origin_config(
        &repo,
        "https://user:secret-token@github.com/Aeptus/private.git?access_token=secret-token",
    );

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    assert!(!json.contains("secret-token"));
    assert!(!json.contains("user:"));
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let repo = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "PrivateRepo")
    {
        Some(repo) => repo,
        None => panic!("private repo is indexed"),
    };
    assert_eq!(
        repo["git_remote_origin_url"],
        "https://github.com/Aeptus/private.git"
    );
    assert_eq!(repo["git_remote_key"], "github.com/aeptus/private");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_reads_packed_git_branch_head() {
    let root = test_root("packed-head");
    let repo = root.join("PackedRepo");
    create_git_repo(&repo, "main");
    if let Err(error) = fs::remove_file(repo.join(".git").join("refs").join("heads").join("main")) {
        panic!("remove loose git ref: {error}");
    }
    if let Err(error) = fs::write(
        repo.join(".git").join("packed-refs"),
        "# pack-refs with: peeled fully-peeled sorted\n1234567890abcdef1234567890abcdef12345678 refs/heads/main\n",
    ) {
        panic!("write packed refs: {error}");
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let packed = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "PackedRepo")
    {
        Some(repo) => repo,
        None => panic!("packed repo is indexed"),
    };

    assert_eq!(packed["git_branch"], "main");
    assert_eq!(packed["git_head"], "1234567890ab");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn local_markdown_paths_ignores_slash_separated_prose() {
    let paths = local_markdown_paths(
        "README.md client/server AI/human ./docs/contracts [Schema](.agents/schema-v1/manifest.schema.json)",
    );

    assert!(paths.contains(&"README.md".to_owned()));
    assert!(paths.contains(&"docs/contracts".to_owned()));
    assert!(paths.contains(&".agents/schema-v1/manifest.schema.json".to_owned()));
    assert!(!paths.contains(&"client/server".to_owned()));
    assert!(!paths.contains(&"AI/human".to_owned()));
}

#[test]
fn repository_inventory_continues_without_artifact_payload_limit() {
    let root = test_root("inventory-ignores-artifact-limit");
    for index in 0..3 {
        let project = root.join(format!("Repo{index}"));
        let target = project.join("target").join("debug");
        create_git_repo(&project, "main");
        if let Err(error) = fs::create_dir_all(&target) {
            panic!("create target dir: {error}");
        }
        if let Err(error) = fs::write(
            target.join("blob"),
            vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write build artifact: {error}");
        }
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };

    assert_eq!(inventory.len(), 3);
    assert_eq!(value["repository_inventory_complete"], true);
    assert_eq!(value["repository_inventory_truncated"], false);
    assert_eq!(
        value["repository_inventory_roots"][0].as_str(),
        Some(root.to_str().unwrap_or_default())
    );
    assert!(
        value["repository_inventory_partial_roots"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert!(value.get("items").is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_runs_repository_inventory_before_artifact_scan() {
    let root = test_root("storage-hygiene-inventory-first");
    let repo = root.join("Repo");
    let target = repo.join("target").join("debug");
    let ignored_repo = repo.join("target").join("IgnoredRepo");
    create_git_repo(&repo, "main");
    create_git_repo(&ignored_repo, "main");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("large-artifact"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write artifact file: {error}");
    }
    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 10);
    let value = parse_json_value(&json, "storage hygiene JSON parses");
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let coverage = value["repository_inventory_coverage"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory coverage is an array"));
    let items = value["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items is an array"));
    let names = inventory
        .iter()
        .filter_map(|repo| repo["repo_name"].as_str())
        .collect::<BTreeSet<_>>();

    assert!(names.contains("Repo"));
    assert!(!names.contains("IgnoredRepo"));
    assert_eq!(value["repository_inventory_complete"], true);
    assert_eq!(value["repository_inventory_truncated"], false);
    assert_eq!(coverage[0]["repository_count"].as_u64(), Some(1));
    assert!(!items.is_empty());
    assert!(
        value["summary"]["total_reclaimable_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes >= MIN_ITEM_BYTES)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_json_returns_inventory_without_artifact_payload() {
    let root = test_root("repository-inventory-json");
    let repo = root.join("Repo");
    let ignored_repo = root.join("node_modules").join("IgnoredRepo");

    create_git_repo(&repo, "main");
    create_git_repo(&ignored_repo, "main");
    if let Err(error) = fs::create_dir_all(repo.join("target").join("debug")) {
        panic!("create artifact tree: {error}");
    }
    if let Err(error) = fs::write(
        repo.join("target").join("debug").join("large.o"),
        "artifact",
    ) {
        panic!("write artifact file: {error}");
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value = parse_json_value(&json, "repository inventory JSON parses");
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let names = inventory
        .iter()
        .filter_map(|repo| repo["repo_name"].as_str())
        .collect::<BTreeSet<_>>();

    assert!(names.contains("Repo"));
    assert!(!names.contains("IgnoredRepo"));
    assert!(value.get("items").is_none());
    assert!(value.get("repo_footprints").is_none());
    assert_eq!(
        value["repository_inventory_coverage"][0]["repository_count"].as_u64(),
        Some(1)
    );
    assert_eq!(
        value["diagnostics"]["discovered_repository_count"].as_u64(),
        Some(1)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_cache_marks_repos_not_seen_in_latest_scan() {
    let root = test_root("repository-inventory-cache-stale");
    let repo = root.join("CachedRepo");
    create_git_repo(&repo, "main");

    let first = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("initial repository inventory json: {error}"),
    };
    let first_value = parse_json_value(&first, "initial repository inventory JSON parses");
    let first_inventory = first_value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("initial repository inventory is an array"));
    assert_eq!(first_inventory.len(), 1);
    assert_eq!(
        first_inventory[0]["not_seen_in_latest_scan"].as_bool(),
        Some(false)
    );
    assert_eq!(
        first_inventory[0]["inventory_cache_status"].as_str(),
        Some("scanned")
    );
    assert_eq!(
        first_inventory[0]["inventory_fingerprint_changed"].as_bool(),
        Some(false)
    );

    if let Err(error) = fs::remove_dir_all(&repo) {
        panic!("remove repo before stale inventory scan: {error}");
    }

    let second = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("stale repository inventory json: {error}"),
    };
    let second_value = parse_json_value(&second, "stale repository inventory JSON parses");
    let second_inventory = second_value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("stale repository inventory is an array"));

    assert_eq!(second_inventory.len(), 1);
    assert_eq!(second_inventory[0]["repo_name"], "CachedRepo");
    assert_eq!(
        second_inventory[0]["not_seen_in_latest_scan"].as_bool(),
        Some(true)
    );
    assert_eq!(
        second_inventory[0]["inventory_cache_status"].as_str(),
        Some("missing")
    );
    assert_eq!(
        second_inventory[0]["inventory_fingerprint_changed"].as_bool(),
        Some(true)
    );
    assert_eq!(second_value["repository_inventory_complete"], false);
    assert_eq!(
        second_value["diagnostics"]["discovered_repository_count"].as_u64(),
        Some(0)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_indexed_snapshot_returns_cached_repository_inventory_without_artifacts() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("indexed-cache-inventory-only");
    let repo = root.join("CachedOnlyRepo");
    create_git_repo(&repo, "main");

    match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(_) => {}
        Err(error) => panic!("prime repository inventory cache: {error}"),
    }

    let json = match storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80) {
        Ok(json) => json,
        Err(error) => panic!("indexed storage hygiene json: {error}"),
    };
    let value = parse_json_value(&json, "indexed cache inventory JSON parses");
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let items = value["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items is an array"));

    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0]["repo_name"], "CachedOnlyRepo");
    assert_eq!(
        inventory[0]["not_seen_in_latest_scan"].as_bool(),
        Some(false)
    );
    assert_eq!(
        inventory[0]["inventory_cache_status"].as_str(),
        Some("current")
    );
    assert_eq!(
        inventory[0]["inventory_fingerprint_changed"].as_bool(),
        Some(false)
    );
    assert!(
        inventory[0]["inventory_fingerprint"]
            .as_str()
            .is_some_and(|fingerprint| fingerprint.starts_with("v1|"))
    );
    assert!(items.is_empty());
    assert_eq!(value["repository_inventory_complete"], false);
    assert_eq!(
        value["repository_inventory_coverage"][0]["status"].as_str(),
        Some("cached_partial")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_cache_detects_git_metadata_changes() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("repository-inventory-cache-fingerprint");
    let repo = root.join("FingerprintRepo");
    create_git_repo(&repo, "main");

    match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(_) => {}
        Err(error) => panic!("prime repository inventory cache: {error}"),
    }

    let initial = match storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80) {
        Ok(json) => json,
        Err(error) => panic!("initial indexed storage hygiene json: {error}"),
    };
    let initial_value = parse_json_value(&initial, "initial indexed inventory JSON parses");
    let initial_inventory = initial_value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("initial repository inventory is an array"));
    let initial_fingerprint = initial_inventory[0]["inventory_fingerprint"]
        .as_str()
        .unwrap_or_else(|| panic!("initial inventory fingerprint is a string"))
        .to_owned();
    assert_eq!(
        initial_inventory[0]["inventory_cache_status"].as_str(),
        Some("current")
    );

    if let Err(error) = fs::write(
        repo.join(".git").join("config"),
        "[remote \"origin\"]\n\turl = git@github.com:example/fingerprint.git\n",
    ) {
        panic!("write changed git config: {error}");
    }

    let changed = match storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80) {
        Ok(json) => json,
        Err(error) => panic!("changed indexed storage hygiene json: {error}"),
    };
    let changed_value = parse_json_value(&changed, "changed indexed inventory JSON parses");
    let changed_inventory = changed_value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("changed repository inventory is an array"));
    let changed_fingerprint = changed_inventory[0]["inventory_fingerprint"]
        .as_str()
        .unwrap_or_else(|| panic!("changed inventory fingerprint is a string"));

    assert_eq!(
        changed_inventory[0]["inventory_cache_status"].as_str(),
        Some("changed")
    );
    assert_eq!(
        changed_inventory[0]["inventory_fingerprint_changed"].as_bool(),
        Some(true)
    );
    assert_ne!(initial_fingerprint, changed_fingerprint);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_indexed_snapshot_keeps_artifact_rows_as_repository_overlay_only() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("indexed-overlay-only");
    let repo = root.join("OverlayRepo");
    let artifact = repo.join("target").join("debug").join("large.o");
    create_git_repo(&repo, "main");
    if let Some(parent) = artifact.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create artifact directory: {error}");
    }
    if let Err(error) = fs::write(&artifact, vec![0u8; (MIN_ITEM_BYTES + 128) as usize]) {
        panic!("write artifact file: {error}");
    }
    let metadata = match fs::symlink_metadata(&artifact) {
        Ok(metadata) => metadata,
        Err(error) => panic!("artifact metadata: {error}"),
    };
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let storage_index = StorageSizeIndex::open();
    let mut metrics = StorageScanMetrics::default();
    storage_index.store_indexed_row(
        &StorageIndexedFileRow {
            path: artifact.display().to_string(),
            device: metadata.dev() as i64,
            inode: metadata.ino() as i64,
            file_id: "test-overlay-row".to_owned(),
            source_root: root.display().to_string(),
            repo_root: Some(repo.display().to_string()),
            kind: "rust-build".to_owned(),
            storage_role: "build-artifact".to_owned(),
            safety: "safe".to_owned(),
            cleanup_tier: "rebuildable".to_owned(),
            logical_bytes: MIN_ITEM_BYTES + 128,
            physical_bytes: MIN_ITEM_BYTES + 128,
            modified_millis: Some(now_millis),
            changed_millis: Some(now_millis),
            accessed_millis: Some(now_millis),
            birth_millis: Some(now_millis),
            is_directory: false,
            entries: 1,
            truncated: false,
            last_scan_millis: now_millis,
        },
        &mut metrics,
    );
    // The indexed snapshot below reads through its own connection, so the
    // buffered row must be flushed to the database first.
    storage_index.flush_pending_rows();

    let json = match storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80) {
        Ok(json) => json,
        Err(error) => panic!("indexed storage hygiene json: {error}"),
    };
    let value = parse_json_value(&json, "indexed overlay JSON parses");
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let items = value["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items is an array"));
    let footprints = value["repo_footprints"]
        .as_array()
        .unwrap_or_else(|| panic!("repo footprints is an array"));

    assert!(inventory.is_empty());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["attribution"]["repo_root"].as_str(), repo.to_str());
    assert_eq!(footprints.len(), 1);
    assert_eq!(footprints[0]["repo_root"].as_str(), repo.to_str());
    assert_eq!(
        value["repository_inventory_coverage"][0]["status"].as_str(),
        Some("cached_empty")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_runtime_reports_inventory_before_finalizing() {
    let root = test_root("runtime-repository-inventory-phase");
    let repo = root.join("Repo");
    let target = repo.join("target").join("debug");
    create_git_repo(&repo, "main");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let progress = Arc::new(Mutex::new(StorageScanJobProgress::new(0, None)));
    let runtime = StorageScanRuntimeContext::new(
        Arc::new(StorageScanControl::new()),
        Arc::clone(&progress),
        StorageScanThrottle {
            sleep_every_checkpoints: 0,
            sleep_millis: 0,
            reason: None,
        },
    );
    let report = build_storage_hygiene_report_with_options(
        vec![root.display().to_string()],
        StorageHygieneOptions {
            max_depth: 5,
            limit: 1,
            mode: StorageScanMode::FastChangedOnly,
            runtime: Some(runtime),
            dirty_paths: Vec::new(),
        },
    );
    let progress = lock_or_recover(&progress).clone();

    assert_eq!(progress.phase, STORAGE_SCAN_PHASE_FINALIZING);
    assert!(report.truncated || !report.items.is_empty());
    assert!(report.repository_inventory_complete);
    assert!(!report.repository_inventory_truncated);
    assert!(report.repository_inventory_partial_roots.is_empty());
    assert_eq!(report.repository_inventory.len(), 1);
    assert_eq!(report.repository_inventory_coverage[0].repository_count, 1);
    assert_eq!(report.diagnostics.discovered_repository_count, 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_marks_cancelled_repository_inventory_partial() {
    let root = test_root("runtime-repository-inventory-cancelled");
    let repo = root.join("Repo");
    create_git_repo(&repo, "main");

    let progress = Arc::new(Mutex::new(StorageScanJobProgress::new(0, None)));
    let control = Arc::new(StorageScanControl::new());
    control.cancel();
    let runtime = StorageScanRuntimeContext::new(
        control,
        Arc::clone(&progress),
        StorageScanThrottle {
            sleep_every_checkpoints: 0,
            sleep_millis: 0,
            reason: None,
        },
    );
    let report = build_storage_hygiene_report_with_options(
        vec![root.display().to_string()],
        StorageHygieneOptions {
            max_depth: 5,
            limit: 1,
            mode: StorageScanMode::FastChangedOnly,
            runtime: Some(runtime),
            dirty_paths: Vec::new(),
        },
    );

    assert!(!report.repository_inventory_complete);
    assert!(report.repository_inventory_truncated);
    assert_eq!(
        report.repository_inventory_partial_roots,
        vec![root.display().to_string()]
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_agent_aware_artifact_cost() {
    let root = test_root("agent-aware-artifacts");
    let target = root.join(".codex").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create codex target dir: {error}");
    }
    let artifact = target.join("blob");
    if let Err(error) = fs::write(&artifact, vec![0u8; (MIN_ITEM_BYTES + 128) as usize]) {
        panic!("write codex build artifact: {error}");
    }
    let artifact_metadata = fs::metadata(&artifact).expect("artifact metadata is readable");
    let expected_physical_bytes = artifact_metadata.blocks().saturating_mul(512);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"agent_hygiene\""));
    assert!(json.contains("\"agent_count\":1"));
    assert!(json.contains("\"provider\":\"codex\""));
    assert!(json.contains("\"display_name\":\"Codex\""));
    assert!(json.contains("\"session_id\":\"Codex local artifacts\""));
    assert!(json.contains(&format!(
        "\"week_agent_artifact_bytes\":{expected_physical_bytes}"
    )));
    assert!(json.contains(&format!(
        "\"week_rebuildable_agent_bytes\":{expected_physical_bytes}"
    )));
    assert!(json.contains("\"attribution_sources\":[\"known_agent_directory\"]"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_rebuild_cost_prefers_measured_writer_duration() {
    let root = test_root("measured-rebuild-cost");
    let repo = root.join("project");
    create_git_repo(&repo, "main");
    let artifact_path = repo.join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&artifact_path) {
        panic!("create measured artifact path: {error}");
    }

    let mut item = test_storage_item(
        &artifact_path.display().to_string(),
        "rust-build",
        "build-artifact",
        "safe",
        "rebuildable",
        1_000,
    );
    item.rebuild_command = Some("cargo build".to_owned());
    item.estimated_rebuild_cost = "Low".to_owned();
    item.estimated_rebuild_seconds = Some(60);
    item.attribution.repo_root = Some(repo.display().to_string());
    item.attribution.repo_name = Some("project".to_owned());

    let writer = StorageWriterLedgerRecord {
        started_at_millis: Some(1_000),
        ended_at_millis: Some(721_000),
        repo_root: Some(repo.display().to_string()),
        working_directory: Some(repo.display().to_string()),
        command: Some("cargo build --workspace".to_owned()),
        source: Some("test-ledger".to_owned()),
        ..StorageWriterLedgerRecord::default()
    };

    let mut items = vec![item];
    apply_measured_rebuild_costs(&mut items, &[writer]);

    assert_eq!(items[0].estimated_rebuild_seconds, Some(720));
    assert_eq!(items[0].estimated_rebuild_cost, "Measured medium");
    assert!(
        items[0]
            .attribution
            .notes
            .first()
            .is_some_and(|note| note.contains("Measured rebuild duration"))
    );

    let footprints = summarize_repo_footprints(&items);
    assert_eq!(footprints.len(), 1);
    assert_eq!(footprints[0].estimated_rebuild_seconds, Some(720));
    assert_eq!(footprints[0].estimated_rebuild_cost, "Measured medium");
    assert!(
        footprints[0]
            .optimization_summary
            .contains("estimated rebuild cost 12m")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_rebuild_cost_ignores_incompatible_writer_command() {
    let root = test_root("measured-rebuild-cost-incompatible-command");
    let repo = root.join("project");
    create_git_repo(&repo, "main");
    let artifact_path = repo.join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&artifact_path) {
        panic!("create measured artifact path: {error}");
    }

    let mut item = test_storage_item(
        &artifact_path.display().to_string(),
        "rust-build",
        "build-artifact",
        "safe",
        "rebuildable",
        1_000,
    );
    item.rebuild_command = Some("cargo build".to_owned());
    item.estimated_rebuild_cost = "Low".to_owned();
    item.estimated_rebuild_seconds = Some(60);
    item.attribution.repo_root = Some(repo.display().to_string());

    let writer = StorageWriterLedgerRecord {
        started_at_millis: Some(1_000),
        ended_at_millis: Some(721_000),
        repo_root: Some(repo.display().to_string()),
        working_directory: Some(repo.display().to_string()),
        command: Some("npm install".to_owned()),
        source: Some("test-ledger".to_owned()),
        ..StorageWriterLedgerRecord::default()
    };

    let mut items = vec![item];
    apply_measured_rebuild_costs(&mut items, &[writer]);

    assert_eq!(items[0].estimated_rebuild_seconds, Some(60));
    assert_eq!(items[0].estimated_rebuild_cost, "Low");
    assert!(items[0].attribution.notes.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_generates_reclaim_actions_for_common_dev_artifacts() {
    let root = test_root("reclaim-actions");
    let project = root.join("project");
    let node_modules = project.join("node_modules").join("left-pad");
    let venv = project.join(".venv").join("lib");
    let frontend_cache = project.join(".turbo");
    let python_cache = project.join(".pytest_cache");
    let aethyme_cache = project.join(".aethyme").join("agent-contracts").join("v1");
    let coverage = project.join("coverage");
    let next_cache = project.join(".next").join("cache");
    let test_output = project.join("playwright-report");
    let npm_cache = root.join(".npm").join("_cacache");
    let pnpm_store = root
        .join(".local")
        .join("share")
        .join("pnpm")
        .join("store")
        .join("v3");
    let yarn_cache = root.join(".yarn").join("cache");
    let docker_storage = root.join(".docker").join("overlay2");
    let simulator_storage = root
        .join("Library")
        .join("Developer")
        .join("CoreSimulator")
        .join("Devices");
    let xcode_archive = root
        .join("Library")
        .join("Developer")
        .join("Xcode")
        .join("Archives")
        .join("Aetower.xcarchive");
    create_git_repo(&project, "main");

    for directory in [
        &node_modules,
        &venv,
        &frontend_cache,
        &python_cache,
        &aethyme_cache,
        &coverage,
        &next_cache,
        &test_output,
        &npm_cache,
        &pnpm_store,
        &yarn_cache,
        &docker_storage,
        &simulator_storage,
        &xcode_archive,
    ] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create test artifact directory: {error}");
        }
        if let Err(error) = fs::write(
            directory.join("blob"),
            vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write test artifact: {error}");
        }
    }
    mark_tree_old(&root);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"kind\":\"node-dependencies\""));
    assert!(
        json.contains("\"cleanup_consequence\":\"Dependencies can be reinstalled from lockfiles")
    );
    assert!(json.contains("\"rebuild_command\":\"npm install / pnpm install / yarn install\""));
    assert!(json.contains("\"estimated_rebuild_cost\":\"High\""));
    assert!(json.contains("\"title\":\"Remove Node dependency tree\""));
    assert!(json.contains("\"category\":\"node\""));
    assert!(json.contains("\"kind\":\"python-environment\""));
    assert!(json.contains("\"title\":\"Remove Python virtual environment\""));
    assert!(json.contains("\"kind\":\"frontend-cache\""));
    assert!(json.contains("\"title\":\"Clear frontend cache\""));
    assert!(json.contains("\"kind\":\"next-cache\""));
    assert!(json.contains("\"kind\":\"python-cache\""));
    assert!(json.contains("\"title\":\"Clear Python cache\""));
    assert!(json.contains("\"kind\":\"tool-cache\""));
    assert!(json.contains("\"display_name\":\".aethyme\""));
    assert!(json.contains("\"kind\":\"coverage-output\""));
    assert!(json.contains("\"title\":\"Remove coverage output\""));
    assert!(json.contains("\"kind\":\"test-output\""));
    assert!(json.contains("\"title\":\"Remove local test output\""));
    assert!(json.contains("\"kind\":\"npm-cache\""));
    assert!(json.contains("\"kind\":\"pnpm-store\""));
    assert!(json.contains("\"kind\":\"yarn-cache\""));
    assert!(json.contains("\"title\":\"Clear package-manager cache\""));
    assert!(json.contains("\"kind\":\"docker-storage\""));
    assert!(json.contains("\"title\":\"Review Docker storage cleanup\""));
    assert!(json.contains("\"kind\":\"xcode-simulator-runtime\""));
    assert!(json.contains("\"title\":\"Review simulator/device support storage\""));
    assert!(json.contains("\"kind\":\"xcode-archives\""));
    assert!(json.contains("\"optimization_summary\""));
    assert!(json.contains("\"rebuildable_bytes\""));
    assert!(json.contains("rm -rf"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_whole_computer_optimization_buckets() {
    let root = test_root("whole-computer-buckets");
    let downloads = root.join("Downloads");
    let documents = root.join("Documents");
    let archive = root.join("Archive");
    let app_bundle = root
        .join("Applications")
        .join("Sample.app")
        .join("Contents");
    let app_cache = root
        .join("Library")
        .join("Caches")
        .join("com.example.sample");
    let app_container = root
        .join("Library")
        .join("Containers")
        .join("com.example.sample");
    let app_support = root
        .join("Library")
        .join("Application Support")
        .join("Sample");
    let app_preferences = root.join("Library").join("Preferences");
    let app_receipts = root.join("private").join("var").join("db").join("receipts");
    let app_launch_agents = root.join("Library").join("LaunchAgents");
    let ios_backup = root
        .join("Library")
        .join("Application Support")
        .join("MobileSync")
        .join("Backup")
        .join("device-a");
    let mail_attachments = root
        .join("Library")
        .join("Mail")
        .join("V10")
        .join("Attachments")
        .join("thread-a");
    let message_attachments = root
        .join("Library")
        .join("Messages")
        .join("Attachments")
        .join("chat-a");
    let local_snapshot = root.join(".MobileBackups").join("snapshot-a");

    for directory in [
        &downloads,
        &documents,
        &archive,
        &app_bundle,
        &app_cache,
        &app_container,
        &app_support,
        &app_preferences,
        &app_receipts,
        &app_launch_agents,
        &ios_backup,
        &mail_attachments,
        &message_attachments,
        &local_snapshot,
    ] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create whole-computer fixture directory: {error}");
        }
    }

    for path in [
        downloads.join("duplicate-a.pkg"),
        documents.join("duplicate-b.pkg"),
    ] {
        if let Err(error) = fs::write(path, vec![7u8; (MIN_ITEM_BYTES + 512) as usize]) {
            panic!("write duplicate candidate: {error}");
        }
    }
    let large_file = downloads.join("large-video.mov");
    let file = match fs::File::create(&large_file) {
        Ok(file) => file,
        Err(error) => panic!("create large file: {error}"),
    };
    if let Err(error) = file.set_len(LARGE_FILE_BYTES + MIN_ITEM_BYTES) {
        panic!("size large file: {error}");
    }
    if let Err(error) = fs::write(
        archive.join("cold-data.bin"),
        vec![1u8; (MIN_ITEM_BYTES + 256) as usize],
    ) {
        panic!("write cold file: {error}");
    }
    if let Err(error) = fs::write(
        app_bundle.join("Info.plist"),
        "<plist><dict><key>CFBundleIdentifier</key><string>com.example.sample</string></dict></plist>",
    ) {
        panic!("write app Info.plist: {error}");
    }
    if let Err(error) = fs::write(
        app_bundle.join("blob"),
        vec![2u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write app payload: {error}");
    }
    if let Err(error) = fs::write(
        app_preferences.join("com.example.sample.plist"),
        "<plist><dict><key>Enabled</key><true/></dict></plist>",
    ) {
        panic!("write app preference plist: {error}");
    }
    if let Err(error) = fs::write(app_receipts.join("com.example.sample.bom"), "receipt") {
        panic!("write app receipt bom: {error}");
    }
    if let Err(error) = fs::write(app_receipts.join("com.example.sample.plist"), "receipt") {
        panic!("write app receipt plist: {error}");
    }
    if let Err(error) = fs::write(
        app_launch_agents.join("com.example.sample.plist"),
        "<plist><dict><key>Label</key><string>com.example.sample</string></dict></plist>",
    ) {
        panic!("write app launch agent: {error}");
    }
    for directory in [
        &app_cache,
        &app_container,
        &app_support,
        &ios_backup,
        &mail_attachments,
        &message_attachments,
        &local_snapshot,
    ] {
        if let Err(error) = fs::write(
            directory.join("payload"),
            vec![3u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write whole-computer payload: {error}");
        }
    }
    mark_tree_old(&root);
    match Command::new("touch").arg(&large_file).status() {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("refresh large-file timestamp failed: {status}"),
        Err(error) => panic!("run touch for large-file timestamp: {error}"),
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 8, 120);

    assert!(json.contains("\"kind\":\"video-file\""));
    assert!(json.contains("\"kind\":\"cold-file\""));
    assert!(json.contains("\"duplicate_groups\""));
    assert!(json.contains("\"confirmed\":true"));
    assert!(json.contains("\"reclaimable_bytes\""));
    assert!(json.contains("\"app_footprints\""));
    assert!(json.contains("\"app_name\":\"Sample\""));
    assert!(json.contains("\"bundle_identifier\":\"com.example.sample\""));
    assert!(json.contains("\"ownership_status\":\"installed\""));
    assert!(json.contains("\"orphan_confidence\":\"none\""));
    assert!(json.contains("\"source\":\"app-bundle\""));
    assert!(json.contains("\"source\":\"receipt\""));
    assert!(json.contains("\"source\":\"launch-item\""));
    assert!(json.contains("\"source\":\"running-process\""));
    assert!(json.contains("\"kind\":\"app-cache\""));
    assert!(json.contains("\"kind\":\"app-container\""));
    assert!(json.contains("\"kind\":\"app-support-data\""));
    assert!(json.contains("\"kind\":\"app-preferences\""));
    assert!(json.contains("\"kind\":\"app-receipt\""));
    assert!(json.contains("\"kind\":\"app-launch-item\""));
    assert!(json.contains("\"component\":\"Preferences\""));
    assert!(json.contains("\"component\":\"Receipt\""));
    assert!(json.contains("\"component\":\"Launch item\""));
    assert!(json.contains("\"system_data_buckets\""));
    assert!(json.contains("\"kind\":\"ios-backup\""));
    assert!(json.contains("\"kind\":\"mail-attachments\""));
    assert!(json.contains("\"kind\":\"message-attachments\""));
    assert!(json.contains("\"kind\":\"local-snapshot\""));
    assert!(json.contains("\"title\":\"iOS and iPadOS backups\""));
    assert!(json.contains("\"title\":\"Mail attachments\""));
    assert!(json.contains("\"title\":\"Messages attachments\""));
    assert!(json.contains("\"title\":\"Local snapshots\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_marks_state_only_app_footprints_as_orphan_candidates() {
    let root = test_root("app-orphan-ownership");
    let app_support = root
        .join("Library")
        .join("Application Support")
        .join("OrphanTool");
    if let Err(error) = fs::create_dir_all(&app_support) {
        panic!("create orphan app support fixture: {error}");
    }
    if let Err(error) = fs::write(
        app_support.join("payload"),
        vec![9u8; (MIN_ITEM_BYTES + 512) as usize],
    ) {
        panic!("write orphan app support payload: {error}");
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 16, 64);

    assert!(json.contains("\"app_footprints\""));
    assert!(json.contains("\"app_name\":\"OrphanTool\""));
    assert!(json.contains("\"ownership_status\":\"orphaned\""));
    assert!(json.contains("\"orphan_confidence\":\"medium\""));
    assert!(json.contains("\"source\":\"app-bundle\""));
    assert!(json.contains("\"status\":\"absent\""));
    assert!(json.contains("\"source\":\"running-process\""));
    assert!(json.contains("\"status\":\"unknown\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_resilience_fixture_handles_edge_cases() {
    let root = test_root("resilience-fixtures");
    let npm_cache = root.join(".npm").join("_cacache").join("content-v2");
    let pnpm_store = root.join(".local").join("share").join("pnpm").join("store");
    let logs = root.join("Library").join("Logs").join("Aetower");
    let cloud_root = root
        .join("Library")
        .join("CloudStorage")
        .join("iCloud Drive");
    let denied = root.join("permission-denied");
    let symlink_target = root.join("target").join("debug");
    let symlink_path = root.join("target-link");
    for directory in [
        &npm_cache,
        &pnpm_store,
        &logs,
        &cloud_root,
        &denied,
        &symlink_target,
    ] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create resilience fixture directory: {error}");
        }
    }
    if let Err(error) = fs::write(
        npm_cache.join("npm-cache.bin"),
        vec![1u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write npm cache fixture: {error}");
    }
    if let Err(error) = fs::write(
        pnpm_store.join("pnpm-store.bin"),
        vec![2u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write pnpm store fixture: {error}");
    }
    if let Err(error) = fs::write(
        logs.join("huge.log"),
        vec![3u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write huge log fixture: {error}");
    }
    let sparse_cloud_file = cloud_root.join("dehydrated-placeholder.bin");
    let file = match fs::File::create(&sparse_cloud_file) {
        Ok(file) => file,
        Err(error) => panic!("create sparse cloud placeholder fixture: {error}"),
    };
    if let Err(error) = file.set_len(MIN_ITEM_BYTES + 512) {
        panic!("size sparse cloud placeholder fixture: {error}");
    }
    if let Err(error) = fs::write(
        symlink_target.join("linked-target.bin"),
        vec![4u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write symlink target fixture: {error}");
    }
    if let Err(error) = std::os::unix::fs::symlink(&symlink_target, &symlink_path)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        panic!("create symlink fixture: {error}");
    }
    if let Err(error) = fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)) {
        panic!("restrict permission fixture: {error}");
    }

    let json = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string(), cloud_root.display().to_string()],
        8,
        120,
        "forensic_verified",
    );
    let value = parse_json_value(&json, "resilience fixture JSON parses");
    let item_kinds = value["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items array exists"))
        .iter()
        .filter_map(|item| item["kind"].as_str())
        .collect::<BTreeSet<_>>();

    assert!(item_kinds.contains("npm-cache"));
    assert!(item_kinds.contains("pnpm-store"));
    assert!(item_kinds.contains("log-file"));
    assert!(
        value["source_coverage"]
            .as_array()
            .unwrap_or_else(|| panic!("source coverage array exists"))
            .iter()
            .any(|source| source["cloud_placeholder"].as_bool() == Some(true)
                || source["kind"].as_str() == Some("cloud"))
    );
    assert!(
        value["items"]
            .as_array()
            .unwrap_or_else(|| panic!("items array exists"))
            .iter()
            .all(|item| item["path"]
                .as_str()
                .is_none_or(|path| !path.contains("target-link")))
    );
    assert!(
        value["diagnostics"]["performance_budget"]["status"]
            .as_str()
            .is_some_and(|status| matches!(status, "ok" | "warn" | "critical"))
    );

    let _ = fs::set_permissions(&denied, fs::Permissions::from_mode(0o755));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reclaimable_regression_detects_build_logs_and_caches() {
    let root = test_root("reclaimable-regression");
    let target = root.join("project").join("target").join("debug");
    let logs = root.join("project").join("logs");
    for directory in [&target, &logs] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create reclaimable regression fixture directory: {error}");
        }
    }
    if let Err(error) = fs::write(
        target.join("artifact"),
        vec![1u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact fixture: {error}");
    }
    if let Err(error) = fs::write(
        logs.join("runtime.log"),
        vec![2u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write log fixture: {error}");
    }
    mark_tree_old(&root);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value = parse_json_value(&json, "reclaimable regression JSON parses");

    assert!(
        value["summary"]["total_reclaimable_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes >= MIN_ITEM_BYTES)
    );
    assert!(
        value["cleanup_bundles"]
            .as_array()
            .is_some_and(|bundles| !bundles.is_empty())
    );
    assert!(
        value["diagnostics"]["performance_budget"]["payload_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_counts_zero_block_placeholders_as_zero_local_reclaim() {
    let root = test_root("zero-block-placeholder-reclaim");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    let artifact = target.join("sparse-artifact");
    let file = match fs::File::create(&artifact) {
        Ok(file) => file,
        Err(error) => panic!("create sparse artifact: {error}"),
    };
    if let Err(error) = file.set_len(MIN_ITEM_BYTES + 128) {
        panic!("size sparse artifact: {error}");
    }
    let metadata = fs::metadata(&artifact).expect("sparse metadata is readable");
    if metadata.blocks() > 0 {
        let _ = fs::remove_dir_all(root);
        return;
    }
    mark_tree_old(&root);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value = parse_json_value(&json, "zero-block placeholder JSON parses");
    let item = value["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["kind"] == "rust-build"))
        .expect("rust build item is present");

    assert_eq!(item["logical_bytes"].as_u64(), Some(MIN_ITEM_BYTES + 128));
    assert_eq!(item["physical_bytes"].as_u64(), Some(0));
    assert_eq!(item["size_bytes"].as_u64(), Some(0));
    assert_eq!(item["cloud_placeholder"].as_bool(), Some(true));
    assert_eq!(item["cleanup_allowed"].as_bool(), Some(false));
    assert!(
        item["cleanup_blockers"]
            .as_array()
            .is_some_and(|blockers| blockers.iter().any(|blocker| blocker
                .as_str()
                .is_some_and(|value| value.contains("No local allocated blocks"))))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_deduplicates_hardlinked_files_in_directory_size() {
    let root = test_root("hardlink-deduped-reclaim");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    let artifact = target.join("artifact-a");
    if let Err(error) = fs::write(&artifact, vec![3u8; (MIN_ITEM_BYTES + 128) as usize]) {
        panic!("write hardlink source: {error}");
    }
    let hardlink = target.join("artifact-b");
    if let Err(error) = fs::hard_link(&artifact, &hardlink) {
        panic!("create hardlink: {error}");
    }
    let metadata = fs::metadata(&artifact).expect("hardlink metadata is readable");
    let expected_physical_bytes = metadata.blocks().saturating_mul(512);
    mark_tree_old(&root);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value = parse_json_value(&json, "hardlink dedupe JSON parses");
    let item = value["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["kind"] == "rust-build"))
        .expect("rust build item is present");

    assert_eq!(
        item["logical_bytes"].as_u64(),
        Some((MIN_ITEM_BYTES + 128).saturating_mul(2))
    );
    assert_eq!(
        item["physical_bytes"].as_u64(),
        Some(expected_physical_bytes)
    );
    assert_eq!(item["size_bytes"].as_u64(), Some(expected_physical_bytes));
    assert_eq!(item["has_hardlinks"].as_bool(), Some(true));
    assert_eq!(item["hardlink_count"].as_u64(), Some(2));
    assert_eq!(item["cleanup_allowed"].as_bool(), Some(false));
    assert!(
        item["cleanup_blockers"]
            .as_array()
            .is_some_and(|blockers| blockers.iter().any(|blocker| blocker
                .as_str()
                .is_some_and(|value| value.contains("Hardlinked content"))))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_performance_budget_flags_million_file_payload_and_table_pressure() {
    let root = test_root("million-file-budget-fixture");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create budget fixture target: {error}");
    }
    if let Err(error) = fs::write(
        target.join("artifact"),
        vec![1u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write budget fixture artifact: {error}");
    }
    let mut report = build_storage_hygiene_report_with_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );
    report.scan_duration_millis = STORAGE_SCAN_LATENCY_CRITICAL_MILLIS;
    report.diagnostics.payload_bytes = STORAGE_PAYLOAD_CRITICAL_BYTES;
    report.diagnostics.candidate_seen_count = 1_000_000;

    refresh_storage_performance_budget(
        &mut report,
        STORAGE_TABLE_PAGE_CRITICAL_MILLIS,
        STORAGE_RENDER_CRITICAL_MILLIS,
    );

    assert_eq!(report.diagnostics.performance_budget.status, "critical");
    assert_eq!(
        report
            .diagnostics
            .performance_budget
            .scan_job_latency_millis,
        STORAGE_SCAN_LATENCY_CRITICAL_MILLIS
    );
    assert_eq!(
        report.diagnostics.performance_budget.table_page_millis,
        STORAGE_TABLE_PAGE_CRITICAL_MILLIS
    );
    assert_eq!(
        report.diagnostics.performance_budget.render_publish_millis,
        STORAGE_RENDER_CRITICAL_MILLIS
    );
    assert!(
        report
            .diagnostics
            .performance_budget
            .notes
            .iter()
            .any(|note| note.contains("1000000 candidates seen"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn protected_path_classification_blocks_unattended_cleanup() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let mut items = vec![test_storage_item(
        "/Applications/Important.app/Contents/MacOS/cache.bin",
        "app-cache",
        "cache",
        "safe",
        "safe",
        old_millis,
    )];

    apply_cleanup_guardrails(&mut items, now_millis);

    assert!(items[0].protected_path);
    assert_eq!(items[0].cleanup_tier, "risky");
    assert_eq!(items[0].default_cleanup_action, "manual_review");
    assert!(!items[0].cleanup_allowed);
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|blocker| blocker.contains("Protected system/application path"))
    );
}

#[test]
fn storage_growth_attribution_uses_single_writer_record() {
    let root = test_root("growth-attribution-single-writer");
    let repo = root.join("project");
    create_git_repo(&repo, "main");
    let artifact_path = repo.join("target").join("debug");
    let delta = test_growth_delta(&artifact_path, Some(&repo), 1_000, 128, 4_096);
    let writer = StorageWriterLedgerRecord {
        started_at_millis: Some(900),
        ended_at_millis: Some(1_100),
        path_prefix: Some(repo.display().to_string()),
        repo_root: Some(repo.display().to_string()),
        command: Some("cargo build".to_owned()),
        process_tree: Some("zsh > cargo build".to_owned()),
        ai_agent_session: Some("Claude session A".to_owned()),
        source: Some("test-ledger".to_owned()),
        ..StorageWriterLedgerRecord::default()
    };

    let attribution = attribute_storage_growth_delta(&delta, &[writer], &[]);

    assert_eq!(attribution.confidence, "high");
    assert_eq!(attribution.confidence_score, 92);
    assert!(!attribution.ambiguous);
    assert_eq!(attribution.writer_source.as_deref(), Some("test-ledger"));
    assert_eq!(attribution.matched_writer_count, 1);
    assert_eq!(attribution.matched_filesystem_event_count, 0);
    assert!(attribution.sources.contains(&"writer_ledger".to_owned()));
    assert!(attribution.sources.contains(&"command".to_owned()));
    assert!(attribution.sources.contains(&"process_tree".to_owned()));
    assert!(attribution.sources.contains(&"ai_session".to_owned()));
    assert_eq!(attribution.command.as_deref(), Some("cargo build"));
    assert_eq!(
        attribution.ai_agent_session.as_deref(),
        Some("Claude session A")
    );
    assert_eq!(attribution.git_branch.as_deref(), Some("main"));
    // No provider/session/tab on the record: writer_display falls back to the
    // ledger source and the identity fields stay absent instead of guessed.
    assert!(attribution.provider.is_none());
    assert!(attribution.session_id.is_none());
    assert!(attribution.tab_name.is_none());
    assert_eq!(attribution.writer_display.as_deref(), Some("test-ledger"));
    assert!(
        attribution
            .evidence
            .iter()
            .any(|entry| entry.contains("Writer ledger source"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_attribution_marks_overlapping_writers_ambiguous() {
    let root = test_root("growth-attribution-ambiguous");
    let repo = root.join("project");
    create_git_repo(&repo, "main");
    let artifact_path = repo.join(".build");
    let delta = test_growth_delta(&artifact_path, Some(&repo), 2_000, 256, 8_192);
    let writers = vec![
        StorageWriterLedgerRecord {
            started_at_millis: Some(1_900),
            ended_at_millis: Some(2_100),
            path_prefix: Some(repo.display().to_string()),
            command: Some("swift build".to_owned()),
            ..StorageWriterLedgerRecord::default()
        },
        StorageWriterLedgerRecord {
            started_at_millis: Some(1_950),
            ended_at_millis: Some(2_050),
            path_prefix: Some(repo.display().to_string()),
            ai_agent_session: Some("Codex session B".to_owned()),
            ..StorageWriterLedgerRecord::default()
        },
    ];

    let attribution = attribute_storage_growth_delta(&delta, &writers, &[]);

    assert_eq!(attribution.confidence, "ambiguous");
    assert!(attribution.ambiguous);
    assert!(attribution.command.is_none());
    assert!(
        attribution.provider.is_none()
            && attribution.session_id.is_none()
            && attribution.tab_name.is_none()
            && attribution.writer_display.is_none(),
        "ambiguous matches must not pick one writer's identity"
    );
    assert_eq!(attribution.matched_writer_count, 2);
    assert_eq!(attribution.matched_filesystem_event_count, 0);
    assert!(attribution.sources.contains(&"writer_ledger".to_owned()));
    assert!(
        attribution
            .sources
            .contains(&"ambiguous_writers".to_owned())
    );
    assert!(
        attribution
            .summary
            .contains("Multiple writer records overlapped")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_attribution_reports_controlled_build_from_chau7_writer() {
    let root = test_root("growth-attribution-chau7-controlled-build");
    let repo = root.join("project");
    create_git_repo(&repo, "feature/storage-attribution");
    let artifact_path = repo.join("target").join("release").join("aetower");
    let delta = test_growth_delta(&artifact_path, Some(&repo), 1_000, 0, 32 * 1024 * 1024);
    let writer = StorageWriterLedgerRecord {
        started_at_millis: Some(900),
        ended_at_millis: Some(1_100),
        working_directory: Some(repo.display().to_string()),
        provider: Some("claude".to_owned()),
        session_id: Some("chau7-tab-a".to_owned()),
        tab_name: Some("aetower-fix".to_owned()),
        chau7_session_id: Some("chau7-session-9".to_owned()),
        source: Some("chau7".to_owned()),
        command: Some("cargo build --workspace --release".to_owned()),
        process_tree: Some("Chau7 > zsh > cargo build --workspace --release".to_owned()),
        ..StorageWriterLedgerRecord::default()
    };

    let filesystem_event = StorageFilesystemEventRecord {
        timestamp_millis: Some(1_000),
        path: Some(artifact_path.display().to_string()),
        event_id: Some(42),
        flags: Some(0),
        source: Some("test-fsevents".to_owned()),
    };

    let attribution = attribute_storage_growth_delta(&delta, &[writer], &[filesystem_event]);

    assert_eq!(attribution.confidence, "high");
    assert_eq!(attribution.confidence_score, 95);
    assert!(!attribution.ambiguous);
    assert_eq!(attribution.writer_source.as_deref(), Some("chau7"));
    assert_eq!(attribution.matched_writer_count, 1);
    assert_eq!(attribution.matched_filesystem_event_count, 1);
    assert_eq!(
        attribution.command.as_deref(),
        Some("cargo build --workspace --release")
    );
    assert_eq!(
        attribution.process_tree.as_deref(),
        Some("Chau7 > zsh > cargo build --workspace --release")
    );
    assert_eq!(
        attribution.ai_agent_session.as_deref(),
        Some("claude session chau7-tab-a")
    );
    assert_eq!(
        attribution.git_branch.as_deref(),
        Some("feature/storage-attribution")
    );
    assert_eq!(attribution.provider.as_deref(), Some("claude"));
    assert_eq!(attribution.session_id.as_deref(), Some("chau7-tab-a"));
    assert_eq!(attribution.tab_name.as_deref(), Some("aetower-fix"));
    assert_eq!(
        attribution.chau7_session_id.as_deref(),
        Some("chau7-session-9")
    );
    assert_eq!(
        attribution.writer_display.as_deref(),
        Some("Claude Code session chau7-tab-a in tab 'aetower-fix'")
    );
    assert!(attribution.sources.contains(&"writer_ledger".to_owned()));
    assert!(attribution.sources.contains(&"fsevents".to_owned()));
    assert!(attribution.sources.contains(&"test-fsevents".to_owned()));
    assert!(attribution.sources.contains(&"chau7".to_owned()));
    assert!(attribution.sources.contains(&"command".to_owned()));
    assert!(attribution.sources.contains(&"process_tree".to_owned()));
    assert!(attribution.sources.contains(&"ai_session".to_owned()));
    assert!(
        attribution
            .summary
            .contains("filesystem events confirmed path activity")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_attribution_uses_filesystem_event_without_guessing_writer() {
    let root = test_root("growth-attribution-fsevents-only");
    let repo = root.join("project");
    create_git_repo(&repo, "main");
    let artifact_path = repo.join("target").join("debug");
    let changed_path = artifact_path.join("aetower");
    let delta = test_growth_delta(&artifact_path, Some(&repo), 2_000, 0, 16 * 1024 * 1024);
    let filesystem_event = StorageFilesystemEventRecord {
        timestamp_millis: Some(1_950),
        path: Some(changed_path.display().to_string()),
        event_id: Some(99),
        flags: Some(0),
        source: Some("test-fsevents".to_owned()),
    };

    let attribution = attribute_storage_growth_delta(&delta, &[], &[filesystem_event]);

    assert_eq!(attribution.confidence, "medium");
    assert_eq!(attribution.confidence_score, 72);
    assert!(!attribution.ambiguous);
    assert_eq!(attribution.matched_writer_count, 0);
    assert_eq!(attribution.matched_filesystem_event_count, 1);
    assert!(attribution.command.is_none());
    assert!(attribution.process_tree.is_none());
    assert!(attribution.ai_agent_session.is_none());
    assert!(attribution.sources.contains(&"fsevents".to_owned()));
    assert!(attribution.sources.contains(&"test-fsevents".to_owned()));
    assert!(
        attribution
            .summary
            .contains("Filesystem events confirmed path activity")
    );
    assert!(
        attribution
            .evidence
            .iter()
            .any(|entry| entry.contains("Filesystem event path matched"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn cleanup_guardrails_block_untracked_source_like_files() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let mut items = vec![test_storage_item(
        "/tmp/aetower-storage/source/main.swift",
        "large-file",
        "large-file",
        "safe",
        "safe",
        now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000),
    )];
    items[0].git_status = "untracked".to_owned();

    apply_cleanup_guardrails(&mut items, now_millis);

    assert!(!items[0].cleanup_allowed);
    assert_eq!(items[0].cleanup_tier, "risky");
    assert_eq!(items[0].safety, "review");
    assert_eq!(items[0].default_cleanup_action, "manual_review");
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|reason| reason.contains("Untracked source-like file"))
    );
    assert!(build_cleanup_recipes(&items).is_empty());
    assert!(build_cleanup_bundles(&items).is_empty());
}

#[test]
fn cleanup_guardrails_block_system_developer_cache_trash_actions() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let mut items = vec![test_storage_item(
        "/Library/Developer/CoreSimulator/Caches",
        "simulator-cache",
        "cache",
        "safe",
        "safe",
        now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000),
    )];

    apply_cleanup_guardrails(&mut items, now_millis);

    assert!(!items[0].cleanup_allowed);
    assert_eq!(items[0].safety, "review");
    assert_eq!(items[0].default_cleanup_action, "manual_review");
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|reason| reason.contains("administrator permission"))
    );
    assert!(build_cleanup_bundles(&items).is_empty());
}

#[test]
fn cleanup_guardrails_block_tracked_modified_and_protected_paths() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let mut items = vec![
        test_storage_item(
            "/tmp/aetower-storage/project/target/debug/blob",
            "rust-build",
            "build-artifact",
            "safe",
            "rebuildable",
            old_millis,
        ),
        test_storage_item(
            "/Applications/Aetower.app",
            "tool-cache",
            "cache",
            "safe",
            "safe",
            old_millis,
        ),
    ];
    items[0].git_status = "modified".to_owned();

    apply_cleanup_guardrails(&mut items, now_millis);

    assert!(!items[0].cleanup_allowed);
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|reason| reason.contains("source work is protected"))
    );
    assert!(!items[1].cleanup_allowed);
    assert!(
        items[1]
            .cleanup_blockers
            .iter()
            .any(|reason| reason.contains("Protected system/application path"))
    );
}

#[test]
fn cleanup_recipes_require_trash_actionable_items() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let allowed = test_storage_item(
        "/tmp/aetower-storage/logs/aetower.log",
        "log-file",
        "log",
        "safe",
        "safe",
        old_millis,
    );
    let mut blocked = allowed.clone();
    blocked.path = "/tmp/aetower-storage/logs/recent.log".to_owned();
    blocked.id = blocked.path.clone();
    blocked.modified_millis = Some(now_millis);

    let mut items = vec![allowed.clone(), blocked];
    apply_cleanup_guardrails(&mut items, now_millis);

    let recipes = build_cleanup_recipes(&items);
    assert!(items[0].cleanup_allowed);
    assert!(!items[1].cleanup_allowed);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].affected_path, allowed.path);
}

#[test]
fn cleanup_bundle_manifest_carries_policy_and_excludes_blocked_items() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let mut allowed = test_storage_item(
        "/tmp/aetower-storage/project/.build/debug/cache.bin",
        "swift-build",
        "build-artifact",
        "safe",
        "rebuildable",
        old_millis,
    );
    allowed.evidence = vec![
        "Detected under a SwiftPM .build directory.".to_owned(),
        "Source-control status: outside Git.".to_owned(),
    ];
    let mut blocked = test_storage_item(
        "/tmp/aetower-storage/project/src/main.swift",
        "large-file",
        "large-file",
        "safe",
        "safe",
        old_millis,
    );
    blocked.git_status = "tracked".to_owned();

    let mut items = vec![allowed.clone(), blocked];
    apply_cleanup_guardrails(&mut items, now_millis);

    let bundles = build_cleanup_bundles(&items);
    let manifest_item = bundles
        .iter()
        .flat_map(|bundle| bundle.manifest.iter())
        .find(|item| item.path == allowed.path)
        .expect("allowed item is present in cleanup manifest");

    assert_eq!(manifest_item.path, allowed.path);
    assert_eq!(manifest_item.size_bytes, allowed.size_bytes);
    assert_eq!(manifest_item.cleanup_tier, "rebuildable");
    assert_eq!(manifest_item.default_cleanup_action, "trash");
    assert!(manifest_item.cleanup_allowed);
    assert!(manifest_item.cleanup_blockers.is_empty());
    assert_eq!(manifest_item.reason, "test item");
    assert_eq!(manifest_item.consequence, "test cleanup consequence");
    assert!(
        manifest_item
            .evidence
            .iter()
            .any(|entry| entry.contains("SwiftPM"))
    );
    assert!(
        bundles
            .iter()
            .flat_map(|bundle| bundle.manifest.iter())
            .all(|item| !item.path.ends_with("src/main.swift"))
    );
}

#[test]
fn storage_release_criteria_dry_run_manifest_validates_bytes_paths_and_blocks_risky() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let allowed = test_storage_item(
        "/tmp/aetower-storage/project/target/debug/cache.bin",
        "rust-build",
        "build-artifact",
        "safe",
        "rebuildable",
        old_millis,
    );
    let mut source_like = test_storage_item(
        "/tmp/aetower-storage/project/src/generated.swift",
        "large-file",
        "large-file",
        "safe",
        "safe",
        old_millis,
    );
    source_like.git_status = "untracked".to_owned();

    let mut items = vec![allowed.clone(), source_like];
    apply_cleanup_guardrails(&mut items, now_millis);

    let bundles = build_cleanup_bundles(&items);
    let bundle = bundles
        .iter()
        .find(|bundle| bundle.manifest.iter().any(|item| item.path == allowed.path))
        .expect("dry-run bundle includes only the release-safe artifact");
    let manifest_bytes = bundle
        .manifest
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));

    assert!(bundle.dry_run_only);
    assert_eq!(bundle.estimated_reclaimable_bytes, manifest_bytes);
    assert_eq!(bundle.item_count, bundle.manifest.len());
    assert!(bundle.estimated_reclaimable_bytes > 0);
    assert!(bundle.manifest.iter().all(|item| !item.path.is_empty()));
    assert!(bundle.manifest.iter().all(|item| item.cleanup_allowed));
    assert!(
        bundle
            .manifest
            .iter()
            .all(|item| item.cleanup_blockers.is_empty())
    );
    assert!(
        bundle
            .manifest
            .iter()
            .all(|item| item.default_cleanup_action == "trash")
    );
    assert!(
        bundle
            .dry_run_commands
            .iter()
            .all(|command| command.starts_with("du -sh "))
    );
    assert!(
        bundles
            .iter()
            .flat_map(|bundle| bundle.manifest.iter())
            .all(|item| !item.path.ends_with("src/generated.swift"))
    );
    assert!(!items[1].cleanup_allowed);
    assert_eq!(items[1].default_cleanup_action, "manual_review");
}

#[test]
fn storage_release_criteria_scan_cancel_responds_under_one_second() {
    let root = test_root("release-cancel-budget");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create release cancel fixture: {error}");
    }
    if let Err(error) = fs::write(
        target.join("artifact"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write release cancel artifact: {error}");
    }

    let start = must_ok(
        storage_scan_start_json(
            vec![root.display().to_string()],
            6,
            120,
            "deep_native",
            "battery,thermal-pressure",
            Vec::new(),
        ),
        "start release cancel scan job",
    );
    let start = parse_json_value(&start, "decode release cancel start");
    let job_id = json_string(&start, "job_id").to_owned();

    let cancel_started = Instant::now();
    let cancel = must_ok(storage_scan_cancel_json(&job_id), "cancel release scan job");
    let elapsed = cancel_started.elapsed();
    let cancel = parse_json_value(&cancel, "decode release cancel response");

    assert!(
        elapsed < Duration::from_secs(1),
        "cancel took {:?}, expected under one second",
        elapsed
    );
    assert_eq!(cancel["status"].as_str(), Some("cancelled"));
    assert!(storage_scan_result_json(&job_id).is_err());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_investigation_evidence_and_large_files() {
    let root = test_root("investigation-evidence");
    let project = root.join("LargeRepo");
    create_git_repo(&project, "main");
    if let Err(error) = fs::write(project.join(".gitignore"), "ignored.bin\n") {
        panic!("write gitignore: {error}");
    }
    let large_file = project.join("ignored.bin");
    let file = match fs::File::create(&large_file) {
        Ok(file) => file,
        Err(error) => panic!("create large file: {error}"),
    };
    if let Err(error) = file.set_len(LARGE_FILE_BYTES + MIN_ITEM_BYTES) {
        panic!("size large file: {error}");
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("storage hygiene JSON parses: {error}"),
    };
    let items = match value["items"].as_array() {
        Some(items) => items,
        None => panic!("items is an array"),
    };
    let large = match items.iter().find(|item| item["kind"] == "large-file") {
        Some(item) => item,
        None => panic!("large file is detected"),
    };

    assert_eq!(value["investigation"]["large_file_count"].as_u64(), Some(1));
    assert!(
        value["investigation"]["top_findings"]
            .as_array()
            .is_some_and(|findings| findings
                .iter()
                .any(|finding| finding["path"] == large_file.display().to_string()))
    );
    assert_eq!(large["storage_role"], "large-file");
    assert_eq!(large["git_status"], "ignored");
    assert!(
        large["next_step"]
            .as_str()
            .is_some_and(|step| step.contains("Manual review"))
    );
    assert!(
        large["evidence"]
            .as_array()
            .is_some_and(|evidence| evidence.len() >= 4)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_cold_unaccessed_files() {
    let root = test_root("cold-files");
    let project = root.join("ColdRepo");
    create_git_repo(&project, "main");
    let cold_file = project.join("cold-data.bin");
    let file = match fs::File::create(&cold_file) {
        Ok(file) => file,
        Err(error) => panic!("create cold file: {error}"),
    };
    if let Err(error) = file.set_len(MIN_ITEM_BYTES + 256) {
        panic!("size cold file: {error}");
    }
    match Command::new("touch")
        .arg("-a")
        .arg("-t")
        .arg("202001010000")
        .arg(&cold_file)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("set old access time failed: {status}"),
        Err(error) => panic!("run touch for old access time: {error}"),
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("storage hygiene JSON parses: {error}"),
    };
    let items = match value["items"].as_array() {
        Some(items) => items,
        None => panic!("items is an array"),
    };
    let cold = match items.iter().find(|item| item["kind"] == "cold-file") {
        Some(item) => item,
        None => panic!("cold file is detected"),
    };

    assert_eq!(value["investigation"]["cold_file_count"].as_u64(), Some(1));
    assert_eq!(cold["cold"], true);
    assert!(
        cold["access_age_days"]
            .as_u64()
            .is_some_and(|days| days >= COLD_AFTER_DAYS)
    );
    assert!(
        cold["evidence"]
            .as_array()
            .is_some_and(|evidence| evidence.iter().any(|entry| entry
                .as_str()
                .is_some_and(|text| text.contains("cold-file threshold"))))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_missing_roots_without_failing() {
    let root = test_root("missing-root");
    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"skipped_roots\""));
    assert!(json.contains("\"reason\":\"missing\""));
}

#[test]
fn storage_hygiene_reports_source_and_volume_coverage() {
    let root = test_root("source-coverage");
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("create root: {error}");
    }
    let json = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );

    assert!(json.contains("\"source_coverage\""));
    assert!(json.contains("\"volume_states\""));
    assert!(json.contains("\"permission_state\":\"readable\""));
    assert!(json.contains("\"gap_kind\":\"covered\""));
    assert!(json.contains("\"protected\":false"));
    assert!(json.contains("\"free_now_bytes\""));
}

#[test]
fn storage_scan_job_completes_and_returns_result() {
    let root = test_root("scan-job-completes");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let start = must_ok(
        storage_scan_start_json(
            vec![root.display().to_string()],
            5,
            80,
            "fast_changed_only",
            "normal",
            Vec::new(),
        ),
        "start scan job",
    );
    let start = parse_json_value(&start, "decode start");
    let job_id = json_string(&start, "job_id").to_owned();
    assert_eq!(start["resumed_from_partial"], false);
    assert_eq!(start["partial_state_available"], true);
    assert!(start["persisted_at_millis"].as_u64().is_some());

    let mut terminal_status = String::new();
    for _ in 0..80 {
        let status = must_ok(storage_scan_status_json(&job_id), "status scan job");
        let status = parse_json_value(&status, "decode status");
        terminal_status = status["status"].as_str().unwrap_or_default().to_owned();
        if terminal_status == "complete" || terminal_status == "failed" {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    assert_eq!(terminal_status, "complete");
    assert_eq!(
        StorageScanStateStore::load_status_for_job(&job_id).as_deref(),
        Some("complete")
    );
    let result = must_ok(storage_scan_result_json(&job_id), "scan result");
    assert!(result.contains("\"scan_mode\":\"fast_changed_only\""));
    assert!(result.contains("\"repository_inventory_coverage\""));
    assert!(result.contains("\"rust-build\""));
}

#[test]
fn storage_scan_job_recovers_persisted_partial_state() {
    let root = test_root("scan-job-recovers-partial");
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("create root: {error}");
    }
    let request = StorageScanJobRequest::new(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
        "battery",
        Vec::new(),
    );
    let mut progress =
        StorageScanJobProgress::new(storage_now_millis(), Some("battery".to_owned()));
    progress.phase = "artifact_sizing".to_owned();
    progress.scanned_files = 17;
    progress.scanned_directories = 5;
    progress.scanned_bytes = 42_000;
    must_ok(
        StorageScanStateStore::persist(StorageScanPersistedRecord {
            job_id: format!("persisted-partial-{}", storage_now_millis()),
            signature: request.signature.clone(),
            volume_key: request.volume_key.clone(),
            roots: request.normalized_roots.clone(),
            dirty_paths: request.dirty_paths.clone(),
            max_depth: request.max_depth,
            limit: request.limit,
            mode: request.mode.as_str().to_owned(),
            throttle_hint: request.throttle_hint.clone(),
            status: "running".to_owned(),
            progress,
            started_at_millis: storage_now_millis().saturating_sub(1_000),
            updated_at_millis: storage_now_millis(),
            completed_at_millis: None,
            result_available: false,
            resume_available: true,
        }),
        "persist partial scan state",
    );

    let start = must_ok(
        storage_scan_start_json(
            vec![root.display().to_string()],
            5,
            80,
            "fast_changed_only",
            "battery",
            Vec::new(),
        ),
        "start resumed scan job",
    );
    let start = parse_json_value(&start, "decode resumed start");
    let job_id = json_string(&start, "job_id").to_owned();

    assert_eq!(start["resumed_from_partial"], true);
    assert_eq!(start["partial_state_available"], true);
    assert_eq!(start["recovered_files"].as_u64(), Some(17));
    assert_eq!(start["recovered_directories"].as_u64(), Some(5));
    assert_eq!(start["recovered_bytes"].as_u64(), Some(42_000));
    assert!(
        start["progress"]["throttle_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("battery"))
    );

    let _ = storage_scan_cancel_json(&job_id);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_scan_throttle_detects_pressure_cloud_and_network_roots() {
    let request = StorageScanJobRequest::new(
        vec![
            "/Volumes/SharedBuilds".to_owned(),
            "~/Library/CloudStorage/Dropbox".to_owned(),
        ],
        5,
        80,
        "deep_native",
        "battery,thermal-pressure,network",
        Vec::new(),
    );
    let throttle = StorageScanThrottle::for_request(&request);
    let reason = throttle.reason.unwrap_or_default();

    assert!(throttle.sleep_every_checkpoints <= 96);
    assert!(throttle.sleep_millis >= 12);
    assert!(reason.contains("external-or-secondary-volume"));
    assert!(reason.contains("cloud-root"));
    assert!(reason.contains("battery"));
    assert!(reason.contains("thermal-pressure"));
    assert!(reason.contains("network"));
}

/// Perf regression (live incident, fixed alongside the growth-delta indexes
/// and the per-generation section memo): after a deep scan grew the
/// persistent index to hundreds of thousands of rows, every instant_cached
/// report build re-ran the growth/cold SQL aggregations with no planner
/// statistics (full scans and per-row b-tree seeks) plus per-repository git
/// reads, burning minutes at 100% CPU per call. Seed a 100k-row index and
/// require the report build — and an immediate rebuild — to stay fast even in
/// debug builds.
#[test]
fn indexed_report_build_is_fast_on_large_synthetic_index() {
    let root = test_root("large-synthetic-index");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }
    // A real fast scan seeds genuine candidate rows so the report has items.
    let _ = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );

    // Bulk-seed 100k synthetic indexed-file rows; every new row also records
    // a growth delta, so the insight aggregations see the same scale.
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let synthetic_root = root.join("synthetic");
    {
        let storage_index = StorageSizeIndex::open();
        let mut metrics = StorageScanMetrics::default();
        for index in 0..100_000u64 {
            let path = synthetic_root
                .join(format!("dir-{:03}", index % 512))
                .join(format!("file-{index}.bin"));
            storage_index.store_indexed_row(
                &StorageIndexedFileRow {
                    path: path.display().to_string(),
                    device: 1,
                    inode: index as i64 + 1,
                    file_id: format!("1:{index}"),
                    source_root: root.display().to_string(),
                    repo_root: None,
                    kind: "indexed-file".to_owned(),
                    storage_role: "file".to_owned(),
                    safety: String::new(),
                    cleanup_tier: String::new(),
                    logical_bytes: 4096,
                    physical_bytes: 4096,
                    modified_millis: Some(now_millis),
                    changed_millis: Some(now_millis),
                    accessed_millis: Some(now_millis),
                    birth_millis: None,
                    is_directory: false,
                    entries: 1,
                    truncated: false,
                    last_scan_millis: now_millis,
                },
                &mut metrics,
            );
        }
        // Dropping the handle flushes the tail chunk and refreshes planner
        // statistics via PRAGMA optimize, matching the end-of-scan lifecycle.
    }

    let build_budget = Duration::from_secs(5);
    let started = Instant::now();
    let report = must_ok(
        build_storage_hygiene_report_from_index(vec![root.display().to_string()], 5, 80),
        "index report builds on the 100k-row index",
    );
    let first_build = started.elapsed();
    assert!(
        !report.items.is_empty(),
        "seeded artifact must survive as a report item"
    );
    assert!(
        first_build < build_budget,
        "first index report build took {first_build:?} (budget {build_budget:?})"
    );

    let started = Instant::now();
    let _ = must_ok(
        build_storage_hygiene_report_from_index(vec![root.display().to_string()], 5, 80),
        "index report rebuilds on the 100k-row index",
    );
    let second_build = started.elapsed();
    assert!(
        second_build < build_budget,
        "immediate index report rebuild took {second_build:?} (budget {build_budget:?})"
    );

    let _ = fs::remove_dir_all(root);
}

/// Without `sqlite_stat1` the query planner falls back to full scans and
/// per-row rowid seeks for every report aggregation query; opening the index
/// must therefore guarantee statistics exist.
#[test]
fn storage_index_open_ensures_query_planner_statistics() {
    let _index_guard = storage_index_test_guard();
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    assert!(
        storage_index.has_query_planner_statistics(),
        "opening the storage index must create query-planner statistics (ANALYZE)"
    );
}

/// The per-generation section memo keys on the index generation stamps;
/// flushing new rows must advance the generation so memoized sections cannot
/// outlive the data they were derived from.
#[test]
fn index_report_generation_advances_when_rows_flush() {
    let _index_guard = storage_index_test_guard();
    let storage_index = StorageSizeIndex::open();
    let mut metrics = StorageScanMetrics::default();
    let before = storage_index
        .index_report_generation()
        .unwrap_or_else(|| panic!("generation unavailable: {}", storage_index.status));
    let bumped_millis = before.0.max(before.1) + 1;
    // Writes are best effort under contention (tests share the process-scoped
    // database and a parallel test may hold the writer lock past the busy
    // timeout), so retry the probe write until the generation advances.
    let mut advanced = false;
    for _ in 0..20 {
        storage_index.store_indexed_row(
            &StorageIndexedFileRow {
                path: format!(
                    "{}/generation-probe.bin",
                    test_root("generation-probe").display()
                ),
                device: 7,
                inode: 7,
                file_id: "7:7".to_owned(),
                source_root: "/tmp".to_owned(),
                repo_root: None,
                kind: "indexed-file".to_owned(),
                storage_role: "file".to_owned(),
                safety: String::new(),
                cleanup_tier: String::new(),
                logical_bytes: 4096,
                physical_bytes: 4096,
                modified_millis: Some(bumped_millis),
                changed_millis: Some(bumped_millis),
                accessed_millis: Some(bumped_millis),
                birth_millis: None,
                is_directory: false,
                entries: 1,
                truncated: false,
                last_scan_millis: bumped_millis,
            },
            &mut metrics,
        );
        storage_index.flush_pending_rows();
        let after = storage_index.index_report_generation().unwrap_or_else(|| {
            panic!(
                "generation unavailable after flush: {}",
                storage_index.status
            )
        });
        if after != before && after.0 >= bumped_millis {
            advanced = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        advanced,
        "flushing a new row must advance the index generation past {before:?} (stamp {bumped_millis})"
    );
}

fn must_ok<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{context}: {error}"),
    }
}

fn parse_json_value(json: &str, context: &str) -> serde_json::Value {
    match serde_json::from_str(json) {
        Ok(value) => value,
        Err(error) => panic!("{context}: {error}"),
    }
}

fn json_string<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    match value[key].as_str() {
        Some(value) => value,
        None => panic!("missing JSON string field: {key}"),
    }
}

fn test_root(name: &str) -> PathBuf {
    let millis = crate::current_unix_millis().unwrap_or_default();
    std::env::temp_dir().join(format!(
        "aetower-storage-{name}-{}-{millis}",
        std::process::id()
    ))
}

fn create_git_repo(repo: &Path, branch: &str) {
    let ref_path = repo.join(".git").join("refs").join("heads").join(branch);
    if let Some(parent) = ref_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create git refs: {error}");
    }
    if let Err(error) = fs::write(
        repo.join(".git").join("HEAD"),
        format!("ref: refs/heads/{branch}\n"),
    ) {
        panic!("write git head: {error}");
    }
    if let Err(error) = fs::write(ref_path, "1234567890abcdef1234567890abcdef12345678\n") {
        panic!("write git ref: {error}");
    }
}

fn test_growth_delta(
    path: &Path,
    repo_root: Option<&Path>,
    scan_millis: u64,
    previous_bytes: u64,
    current_bytes: u64,
) -> StorageGrowthDelta {
    StorageGrowthDelta {
        bucket_millis: (scan_millis / STORAGE_GROWTH_BUCKET_MILLIS) * STORAGE_GROWTH_BUCKET_MILLIS,
        scan_millis,
        path: path.display().to_string(),
        source_root: repo_root
            .unwrap_or_else(|| path.parent().unwrap_or(path))
            .display()
            .to_string(),
        repo_root: repo_root.map(|repo| repo.display().to_string()),
        repo_name: None,
        git_branch: None,
        git_head: None,
        kind: "rust-build".to_owned(),
        cleanup_tier: "rebuildable".to_owned(),
        previous_physical_bytes: previous_bytes,
        current_physical_bytes: current_bytes,
        delta_bytes: current_bytes as i64 - previous_bytes as i64,
        command: None,
        process_tree: None,
        ai_agent_session: None,
        writer_source: None,
        provider: None,
        session_id: None,
        tab_name: None,
        chau7_session_id: None,
        writer_display: None,
        matched_writer_count: 0,
        matched_filesystem_event_count: 0,
        attribution_sources: Vec::new(),
        attribution_confidence: "low".to_owned(),
        attribution_confidence_score: 0,
        attribution_ambiguous: false,
        attribution_summary: String::new(),
        attribution_evidence: Vec::new(),
    }
}

fn create_indexed_git_repo(repo: &Path) {
    if let Err(error) = fs::create_dir_all(repo) {
        panic!("create indexed git repo dir: {error}");
    }
    run_git(repo, &["init"]);
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("checkout")
        .arg("-b")
        .arg("main")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn write_complete_agent_contracts(repo: &Path) {
    write_minimal_agents_contract(repo);
    let agents_dir = repo.join(".agents");
    if let Err(error) = fs::create_dir_all(&agents_dir) {
        panic!("create .agents dir: {error}");
    }
    for (file, body) in [
        (
            "manifest",
            "contracts:\n  - id: agents_md\nintegrity:\n  unique_ids: true\n",
        ),
        ("repo-map", "roots: []\n"),
        ("commands", "commands: []\n"),
        ("references", "references: []\n"),
    ] {
        if let Err(error) = fs::write(
            agents_dir.join(format!("{file}.yaml")),
            format!("schema_version: 1\ngenerated_by: test\nsource_files:\n  - AGENTS.md\n{body}"),
        ) {
            panic!("write generated agent contract {file}: {error}");
        }
    }
    for (file, body) in [
        ("tasks", "tasks: []\n"),
        ("contracts", "contracts: []\n"),
        ("validation", "rules: []\n"),
        ("boundaries", "layers: []\nrules: []\n"),
        ("risks", "surfaces: []\n"),
    ] {
        if let Err(error) = fs::write(
            agents_dir.join(format!("{file}.yaml")),
            format!(
                "schema_version: 1\nreviewed_by: test\nreviewed_at: 2026-06-28\nsource_files:\n  - AGENTS.md\n{body}"
            ),
        ) {
            panic!("write reviewed agent contract {file}: {error}");
        }
    }
}

fn write_minimal_agents_contract(repo: &Path) {
    if let Err(error) = fs::write(repo.join("README.md"), "See AGENTS.md.\n") {
        panic!("write README.md: {error}");
    }
    if let Err(error) = fs::write(
        repo.join("AGENTS.md"),
        [
            "## Scope And Precedence",
            "Repository-local instructions override generic defaults.",
            "## Repository Map",
            "Use .agents/manifest.yaml for contract integrity, .agents/tasks.yaml for task routing, .agents/repo-map.yaml for topology, and .agents/contracts.yaml for invariants.",
            "## Standard Workflow",
            "Run git status --short before editing and preserve unrelated changes.",
            "## Commands",
            "Use .agents/commands.yaml for exact command metadata.",
            "## Approval Required",
            "Ask before destructive, network, deploy, migration, or setup actions.",
            "## Validation Matrix",
            "Use .agents/validation.yaml for touched-path validation mapping.",
            "## Architecture Boundaries",
            "Use .agents/boundaries.yaml for import and layer rules.",
            "## Code Rules",
            "Keep changes targeted and readable.",
            "## Security Rules",
            "Never commit secrets, credentials, production data, or local env files.",
            "## Completion Checklist",
            "Report changed files, validation, and residual risk.",
            "## References",
            "README.md",
        ]
        .join("\n\n"),
    ) {
        panic!("write AGENTS.md: {error}");
    }
}

fn git_add_all(repo: &Path) {
    run_git(repo, &["add", "README.md", "AGENTS.md"]);
    if repo.join(".agents").exists() {
        run_git(repo, &["add", ".agents"]);
    }
}

fn run_git(repo: &Path, args: &[&str]) {
    let status = match Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => status,
        Err(error) => panic!("run git {:?}: {error}", args),
    };
    if !status.success() {
        panic!("git {:?} failed with status {status}", args);
    }
}

fn mark_tree_old(path: &Path) {
    if !path.exists() {
        return;
    }
    if path.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) => panic!("read fixture tree: {error}"),
        };
        for entry in entries.flatten() {
            mark_tree_old(&entry.path());
        }
    }
    match Command::new("touch")
        .arg("-t")
        .arg("202001010000")
        .arg(path)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("set old fixture mtime failed: {status}"),
        Err(error) => panic!("run touch for fixture mtime: {error}"),
    }
}

const IMAGE_SIMILARITY_FIXTURE_SIZE: u32 = 1024;
const IMAGE_SIMILARITY_FIXTURE_BLOCK: u32 = IMAGE_SIMILARITY_FIXTURE_SIZE / 8;

fn write_similarity_png(path: &Path, variant: u8) {
    let image = image::RgbImage::from_fn(
        IMAGE_SIMILARITY_FIXTURE_SIZE,
        IMAGE_SIMILARITY_FIXTURE_SIZE,
        |x, y| {
            let block_x = x / IMAGE_SIMILARITY_FIXTURE_BLOCK;
            let block_y = y / IMAGE_SIMILARITY_FIXTURE_BLOCK;
            let bright = (block_x + block_y).is_multiple_of(2);
            let base = if bright { 208u8 } else { 48u8 };
            let noise =
                ((x.wrapping_mul(31) ^ y.wrapping_mul(17) ^ u32::from(variant)) & 0x1f) as u8;
            image::Rgb([
                base.saturating_add(noise / 2),
                base.saturating_add(noise / 3),
                base.saturating_add(noise / 4),
            ])
        },
    );
    if let Err(error) = image.save(path) {
        panic!("write similarity png {}: {error}", path.display());
    }
}

fn write_contrast_png(path: &Path) {
    let image = image::RgbImage::from_fn(
        IMAGE_SIMILARITY_FIXTURE_SIZE,
        IMAGE_SIMILARITY_FIXTURE_SIZE,
        |x, y| {
            let block_x = x / IMAGE_SIMILARITY_FIXTURE_BLOCK;
            let block_y = y / IMAGE_SIMILARITY_FIXTURE_BLOCK;
            let bright = !(block_x + block_y).is_multiple_of(2);
            let base = if bright { 208u8 } else { 48u8 };
            let noise = ((x.wrapping_mul(7).wrapping_add(y.wrapping_mul(53))) & 0x1f) as u8;
            image::Rgb([
                base.saturating_add(noise / 2),
                base.saturating_add(noise / 3),
                base.saturating_add(noise / 4),
            ])
        },
    );
    if let Err(error) = image.save(path) {
        panic!("write contrast png {}: {error}", path.display());
    }
}

fn write_similarity_markdown(path: &Path, variant: usize) {
    let mut content = String::with_capacity((MIN_ITEM_BYTES + 256 * 1024) as usize);
    for index in 0..10_000 {
        content.push_str("## Storage cockpit agent report\n");
        content.push_str(
            "Aetower reviewed generated artifacts, repository build output, command logs, \
             cache folders, branch metadata, and AI session context before recommending \
             any cleanup action.\n",
        );
        content.push_str(
            "The safe action remains review first, reveal in Finder, compare provenance, \
             then stage only rebuildable or explicitly disposable files into the collector.\n",
        );
        if index % 250 == 0 {
            content.push_str(
                "<!-- generator note: whitespace and comments should not dominate similarity -->\n",
            );
        }
        if variant != 0 && index % 111 == 0 {
            content.push_str(
                "Revision note: this exported copy includes a small operator annotation \
                 and a different timestamp but preserves the same operating contract.\n",
            );
        }
    }
    if let Err(error) = fs::write(path, content) {
        panic!("write similarity markdown {}: {error}", path.display());
    }
}

fn write_unrelated_markdown(path: &Path) {
    let mut content = String::with_capacity((MIN_ITEM_BYTES + 256 * 1024) as usize);
    for index in 0..10_000 {
        content.push_str("## Photo archive gardening playlist\n");
        content.push_str(
            "The archive catalog describes travel albums, camera lenses, dinner recipes, \
             bicycle maintenance notes, watercolor palettes, and weekend itinerary drafts.\n",
        );
        content.push_str(
            "Each paragraph intentionally uses a separate vocabulary so SimHash should not \
             group it with storage cleanup reports or developer artifact summaries.\n",
        );
        if index % 97 == 0 {
            content.push_str(
                "Reminder: prune the balcony herbs, label the negatives, and tune the guitar.\n",
            );
        }
    }
    if let Err(error) = fs::write(path, content) {
        panic!("write unrelated markdown {}: {error}", path.display());
    }
}

fn write_similarity_pdf(path: &Path, variant: usize) {
    let mut content = String::from(
        "%PDF-1.4\n1 0 obj\n<< /Type /Page /Contents 2 0 R >>\nendobj\n2 0 obj\n<< /Length 0 >>\nstream\nBT\n",
    );
    for index in 0..12_000 {
        content.push('(');
        content.push_str(
            "Aetower storage audit extracted document text, repository artifacts, generated logs, cache folders, and AI session context before proposing cleanup.",
        );
        if variant != 0 && index % 137 == 0 {
            content.push_str(" Minor revision annotation");
        }
        content.push_str(") Tj\n");
    }
    content.push_str("ET\nendstream\nendobj\n%%EOF\n");
    if let Err(error) = fs::write(path, content) {
        panic!("write similarity pdf {}: {error}", path.display());
    }
}

fn write_similarity_docx(path: &Path, variant: usize) {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
    );
    for index in 0..12_000 {
        xml.push_str("<w:p><w:r><w:t>");
        xml.push_str(
            "Aetower document export reviews storage cleanup evidence, generated artifacts, repository context, branch metadata, and operator-safe reclaim plans.",
        );
        if variant != 0 && index % 127 == 0 {
            xml.push_str(" Small editorial note");
        }
        xml.push_str("</w:t></w:r></w:p>");
    }
    xml.push_str("</w:body></w:document>");
    write_stored_docx(path, &xml);
}

fn write_unrelated_docx(path: &Path) {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
    );
    for index in 0..12_000 {
        xml.push_str("<w:p><w:r><w:t>");
        xml.push_str(
            "The garden archive compares watercolor pigments, bicycle repair notes, recipes, travel photography, and piano practice schedules.",
        );
        if index % 83 == 0 {
            xml.push_str(" Balcony reminder");
        }
        xml.push_str("</w:t></w:r></w:p>");
    }
    xml.push_str("</w:body></w:document>");
    write_stored_docx(path, &xml);
}

fn write_stored_docx(path: &Path, document_xml: &str) {
    let mut bytes = Vec::new();
    append_stored_zip_entry(
        &mut bytes,
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
    );
    append_stored_zip_entry(&mut bytes, "word/document.xml", document_xml.as_bytes());
    if let Err(error) = fs::write(path, bytes) {
        panic!("write stored docx {}: {error}", path.display());
    }
}

fn append_stored_zip_entry(target: &mut Vec<u8>, name: &str, data: &[u8]) {
    target.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    target.extend_from_slice(&20u16.to_le_bytes());
    target.extend_from_slice(&0u16.to_le_bytes());
    target.extend_from_slice(&0u16.to_le_bytes());
    target.extend_from_slice(&0u16.to_le_bytes());
    target.extend_from_slice(&0u16.to_le_bytes());
    target.extend_from_slice(&0u32.to_le_bytes());
    target.extend_from_slice(&(data.len() as u32).to_le_bytes());
    target.extend_from_slice(&(data.len() as u32).to_le_bytes());
    target.extend_from_slice(&(name.len() as u16).to_le_bytes());
    target.extend_from_slice(&0u16.to_le_bytes());
    target.extend_from_slice(name.as_bytes());
    target.extend_from_slice(data);
}

fn write_similarity_mov(path: &Path, variant: usize) {
    write_synthetic_mov(
        path,
        62_000 + variant as u32 * 400,
        1_920,
        1_080,
        *b"avc1",
        41,
        variant as u8,
    );
}

fn write_unrelated_mov(path: &Path) {
    write_synthetic_mov(path, 88_000, 1_280, 720, *b"hvc1", 73, 0);
}

fn write_synthetic_mov(
    path: &Path,
    duration_millis: u32,
    width: u32,
    height: u32,
    codec: [u8; 4],
    media_seed: u8,
    metadata_variant: u8,
) {
    let mut bytes = Vec::new();
    append_mp4_box(&mut bytes, *b"ftyp", b"qt  \0\0\0\0qt  mp42");

    let mut mvhd = Vec::new();
    mvhd.extend_from_slice(&[0, 0, 0, metadata_variant]);
    mvhd.extend_from_slice(&0u32.to_be_bytes());
    mvhd.extend_from_slice(&0u32.to_be_bytes());
    mvhd.extend_from_slice(&1_000u32.to_be_bytes());
    mvhd.extend_from_slice(&duration_millis.to_be_bytes());
    mvhd.extend_from_slice(&[0u8; 64]);
    append_mp4_box(&mut bytes, *b"mvhd", &mvhd);

    let mut tkhd = vec![0u8; 84];
    tkhd[3] = metadata_variant;
    let width_fixed = width << 16;
    let height_fixed = height << 16;
    tkhd[76..80].copy_from_slice(&width_fixed.to_be_bytes());
    tkhd[80..84].copy_from_slice(&height_fixed.to_be_bytes());
    append_mp4_box(&mut bytes, *b"tkhd", &tkhd);

    let mut stsd = Vec::new();
    stsd.extend_from_slice(&[0, 0, 0, 0]);
    stsd.extend_from_slice(&1u32.to_be_bytes());
    stsd.extend_from_slice(&16u32.to_be_bytes());
    stsd.extend_from_slice(&codec);
    stsd.extend_from_slice(&[0u8; 8]);
    append_mp4_box(&mut bytes, *b"stsd", &stsd);

    let mut media = vec![0u8; (MIN_ITEM_BYTES + 96 * 1024) as usize];
    for (index, byte) in media.iter_mut().enumerate() {
        *byte = media_seed.wrapping_add((index as u8).rotate_left((index % 7) as u32));
    }
    append_mp4_box(&mut bytes, *b"mdat", &media);

    if let Err(error) = fs::write(path, bytes) {
        panic!("write synthetic mov {}: {error}", path.display());
    }
}

fn append_mp4_box(target: &mut Vec<u8>, box_type: [u8; 4], payload: &[u8]) {
    let size = payload.len().saturating_add(8);
    target.extend_from_slice(&(size as u32).to_be_bytes());
    target.extend_from_slice(&box_type);
    target.extend_from_slice(payload);
}

fn write_similarity_binary(path: &Path, variant: usize) {
    let mut data = similarity_binary_bytes(17, (MIN_ITEM_BYTES + 320 * 1024) as usize);
    if variant != 0 {
        data.extend_from_slice(&similarity_binary_bytes(91, 24 * 1024));
    }
    if let Err(error) = fs::write(path, data) {
        panic!("write similarity binary {}: {error}", path.display());
    }
}

fn write_unrelated_binary(path: &Path) {
    let data = similarity_binary_bytes(211, (MIN_ITEM_BYTES + 320 * 1024) as usize);
    if let Err(error) = fs::write(path, data) {
        panic!("write unrelated binary {}: {error}", path.display());
    }
}

fn similarity_binary_bytes(seed: u8, len: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(len);
    for index in 0..len {
        let mixed = (index as u64)
            .wrapping_mul(0x9e37_79b1)
            .wrapping_add(u64::from(seed) * 0x1000_001b3)
            .rotate_left((index % 17) as u32);
        data.push((mixed ^ (mixed >> 24) ^ (mixed >> 41)) as u8);
    }
    data
}

fn write_git_origin_config(repo: &Path, origin_url: &str) {
    if let Err(error) = fs::write(
        repo.join(".git").join("config"),
        format!("[remote \"origin\"]\n\turl = {origin_url}\n"),
    ) {
        panic!("write git config: {error}");
    }
}

fn guidance_issue_ids(repo: &serde_json::Value) -> Vec<String> {
    repo["agent_guidance_issues"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|issue| issue["id"].as_str().map(str::to_owned))
        .collect()
}

fn test_storage_item(
    path: &str,
    kind: &str,
    storage_role: &str,
    safety: &str,
    cleanup_tier: &str,
    modified_millis: u64,
) -> StorageHygieneItem {
    StorageHygieneItem {
        id: path.to_owned(),
        path: path.to_owned(),
        display_name: Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact")
            .to_owned(),
        kind: kind.to_owned(),
        storage_role: storage_role.to_owned(),
        git_status: "outside-git".to_owned(),
        safety: safety.to_owned(),
        cleanup_tier: cleanup_tier.to_owned(),
        size_bytes: MIN_ITEM_BYTES + 128,
        logical_bytes: MIN_ITEM_BYTES + 128,
        physical_bytes: MIN_ITEM_BYTES + 128,
        byte_accounting: "test fixture".to_owned(),
        sparse_or_shared: false,
        hardlink_count: 1,
        has_hardlinks: false,
        cloud_placeholder: false,
        protected_path: is_protected_cleanup_path(path),
        size_truncated: false,
        modified_millis: Some(modified_millis),
        age_days: None,
        accessed_millis: None,
        access_age_days: None,
        cold: false,
        stale: true,
        reason: "test item".to_owned(),
        recommendation: "test recommendation".to_owned(),
        next_step: String::new(),
        command_hint: format!("du -sh {}", shell_quote(path)),
        rebuild_command: None,
        estimated_rebuild_cost: "Unknown".to_owned(),
        estimated_rebuild_seconds: None,
        cleanup_consequence: "test cleanup consequence".to_owned(),
        semantic_category: "review-artifact".to_owned(),
        taxonomy_source: "test".to_owned(),
        rebuildability: "unknown".to_owned(),
        manifest_evidence: Vec::new(),
        evidence: Vec::new(),
        cleanup_allowed: true,
        cleanup_blockers: Vec::new(),
        default_cleanup_action: "trash".to_owned(),
        recommendation_score: 0.0,
        attribution: StorageArtifactAttribution {
            repo_root: None,
            repo_name: None,
            git_branch: None,
            git_head: None,
            command: None,
            process_tree: None,
            ai_agent_session: None,
            provider: None,
            session_id: None,
            tab_name: None,
            chau7_session_id: None,
            writer_display: None,
            confidence: "low".to_owned(),
            notes: Vec::new(),
        },
    }
}

#[test]
fn size_walk_budget_is_scaled_per_mode() {
    assert_eq!(
        StorageScanMode::FastChangedOnly.size_walk_time_budget(),
        SCAN_TIME_BUDGET
    );
    assert_eq!(
        StorageScanMode::DeepNative.size_walk_time_budget(),
        Duration::from_secs(60)
    );
    assert_eq!(
        StorageScanMode::ForensicVerified.size_walk_time_budget(),
        Duration::from_secs(120)
    );
}

#[test]
fn forensic_walk_survives_a_slow_pre_walk_phase() {
    // Regression: the walk used to share SCAN_TIME_BUDGET with the git /
    // inventory phase, so a forensic scan over many repositories consumed the
    // whole clock before sizing a single root (observed live: root_walk 0ms,
    // item_count 0). The walk now runs against an explicit per-root deadline
    // derived only from the walk phase: however long the pre-walk phases
    // took, a future deadline must still walk entries.
    let root = test_root("forensic-walk-budget");
    let target = root.join("proj").join("target");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write artifact: {error}");
    }

    let options = StorageHygieneOptions {
        max_depth: 5,
        limit: 10,
        mode: StorageScanMode::ForensicVerified,
        runtime: None,
        dirty_paths: Vec::new(),
    };
    let storage_index = StorageSizeIndex::disabled("test");
    let mut collector = StorageCandidateCollector::new(options.limit);
    let mut metrics = StorageScanMetrics::default();
    let deadline = Instant::now() + options.mode.per_root_slice_floor();

    let (_repos, scanned_dirs, truncated) = scan_root(
        &root,
        &options,
        deadline,
        storage_now_millis(),
        &storage_index,
        &mut collector,
        &mut metrics,
    );

    assert!(
        scanned_dirs > 0,
        "walk must proceed on its own deadline (scanned {scanned_dirs} dirs, truncated={truncated})"
    );
}

#[test]
fn per_root_walk_slice_shares_remaining_budget_with_floor() {
    // Even share of the remaining budget across the remaining roots.
    assert_eq!(
        per_root_walk_slice(Duration::from_secs(60), 6, Duration::from_secs(2)),
        Duration::from_secs(10)
    );
    // Unspent time rolls forward: fewer remaining roots means bigger slices.
    assert_eq!(
        per_root_walk_slice(Duration::from_secs(58), 2, Duration::from_secs(2)),
        Duration::from_secs(29)
    );
    // The floor guarantees a usable slice even when the budget is nearly gone.
    assert_eq!(
        per_root_walk_slice(Duration::from_millis(400), 40, Duration::from_secs(2)),
        Duration::from_secs(2)
    );
    assert_eq!(
        per_root_walk_slice(Duration::from_millis(100), 10, Duration::from_millis(500)),
        Duration::from_millis(500)
    );
    // Zero remaining roots is defensive: treat as one.
    assert_eq!(
        per_root_walk_slice(Duration::from_secs(8), 0, Duration::from_secs(2)),
        Duration::from_secs(8)
    );
    // Mode floors: fast stays snappy, deep/forensic get a real slice.
    assert_eq!(
        StorageScanMode::FastChangedOnly.per_root_slice_floor(),
        Duration::from_millis(500)
    );
    assert_eq!(
        StorageScanMode::DeepNative.per_root_slice_floor(),
        Duration::from_secs(2)
    );
}

#[test]
fn per_root_deadline_keeps_second_root_walkable_after_a_huge_first_root() {
    // Regression for root starvation: all roots used to share one walk clock,
    // so a huge early root (~/Repositories) consumed the whole budget and
    // later roots were never walked. Simulate the exhausted-first-root state
    // with an already-expired deadline, then show the second root still gets
    // walked on its own fresh slice.
    let first = test_root("fairness-first-huge");
    let second = test_root("fairness-second-small");
    for root in [&first, &second] {
        for index in 0..12 {
            let nested = root.join(format!("dir-{index:02}")).join("nested");
            if let Err(error) = fs::create_dir_all(nested) {
                panic!("create fairness fixture: {error}");
            }
        }
        if let Err(error) = fs::write(
            root.join("dir-00").join("app.log"),
            vec![0u8; (MIN_ITEM_BYTES + 64) as usize],
        ) {
            panic!("write fairness artifact: {error}");
        }
    }

    let options = StorageHygieneOptions {
        max_depth: 5,
        limit: 20,
        mode: StorageScanMode::DeepNative,
        runtime: None,
        dirty_paths: Vec::new(),
    };
    let storage_index = StorageSizeIndex::disabled("test");
    let mut collector = StorageCandidateCollector::new(options.limit);
    let mut metrics = StorageScanMetrics::default();

    // First root: injected slice already spent (the "huge root" case).
    let expired = Instant::now();
    let (_repos, first_dirs, first_truncated) = scan_root(
        &first,
        &options,
        expired,
        storage_now_millis(),
        &storage_index,
        &mut collector,
        &mut metrics,
    );
    assert!(first_truncated, "expired slice must report truncation");
    assert_eq!(first_dirs, 0, "expired slice must not keep walking");

    // Second root: a fresh floor-sized slice, exactly what the fairness loop
    // hands out after an earlier root consumed its share.
    let fresh_deadline = Instant::now() + options.mode.per_root_slice_floor();
    let (_repos, second_dirs, second_truncated) = scan_root(
        &second,
        &options,
        fresh_deadline,
        storage_now_millis(),
        &storage_index,
        &mut collector,
        &mut metrics,
    );
    assert!(
        second_dirs > 0,
        "second root must be walked on its own slice (dirs={second_dirs}, truncated={second_truncated})"
    );

    let _ = fs::remove_dir_all(first);
    let _ = fs::remove_dir_all(second);
}

#[test]
fn directory_budget_is_scaled_per_mode() {
    assert_eq!(StorageScanMode::InstantCached.dir_budget(), 25_000);
    assert_eq!(StorageScanMode::FastChangedOnly.dir_budget(), 25_000);
    assert_eq!(StorageScanMode::DeepNative.dir_budget(), 100_000);
    assert_eq!(StorageScanMode::ForensicVerified.dir_budget(), 200_000);
}

#[test]
fn storage_hygiene_surfaces_unclassified_large_directories() {
    let root = test_root("large-directory-surfacing");
    let media = root.join("media");
    let footage = media.join("footage");
    let notes = root.join("notes");
    for directory in [&footage, &notes] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create large-directory fixture: {error}");
        }
    }
    // 20 sparse files below the large-file threshold: no per-file rule fires,
    // so only large-directory surfacing can make this subtree visible.
    for index in 0..20 {
        let file = match fs::File::create(footage.join(format!("clip-{index:02}.raw"))) {
            Ok(file) => file,
            Err(error) => panic!("create sparse clip: {error}"),
        };
        if let Err(error) = file.set_len(60 * 1024 * 1024) {
            panic!("size sparse clip: {error}");
        }
    }
    if let Err(error) = fs::write(notes.join("todo.txt"), b"small") {
        panic!("write small fixture: {error}");
    }

    let json = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        8,
        120,
        "deep",
    );
    let report = parse_json_value(&json, "large-directory report parses");
    let items = report["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items is an array"));
    let large_dirs: Vec<_> = items
        .iter()
        .filter(|item| item["kind"] == "large-directory")
        .collect();
    assert_eq!(
        large_dirs.len(),
        1,
        "exactly one large-directory item (nested child deduped by ancestor prefix)"
    );
    let item = large_dirs[0];
    assert_eq!(item["path"], media.display().to_string());
    assert_eq!(item["cleanup_tier"], "");
    assert_eq!(item["safety"], "review");
    assert_eq!(item["cleanup_allowed"], false);
    assert_eq!(item["default_cleanup_action"], "manual_review");
    assert!(
        json_string(item, "recommendation").contains("review what lives here"),
        "review-only recommendation"
    );
    assert!(
        json_string(item, "reason")
            .to_ascii_lowercase()
            .contains("informational"),
        "reason marks the item informational"
    );
    assert!(
        item["evidence"]
            .as_array()
            .is_some_and(|evidence| !evidence.is_empty()),
        "evidence includes size/age lines"
    );
    assert!(
        item["cleanup_blockers"]
            .as_array()
            .is_some_and(|blockers| !blockers.is_empty()),
        "unclassified tier must carry a cleanup blocker"
    );
    assert!(
        items
            .iter()
            .all(|candidate| candidate["path"] != footage.display().to_string()),
        "nested large directory must not be emitted when its ancestor was"
    );

    // Tier-less items must never enter cleanup lanes.
    let media_path = media.display().to_string();
    let bundles = report["cleanup_bundles"]
        .as_array()
        .unwrap_or_else(|| panic!("cleanup_bundles is an array"));
    for bundle in bundles {
        let manifest = bundle["manifest"]
            .as_array()
            .unwrap_or_else(|| panic!("bundle manifest is an array"));
        assert!(
            manifest
                .iter()
                .all(|entry| entry["path"] != media_path.as_str()),
            "large-directory item leaked into a cleanup bundle"
        );
    }
    let recipes = report["cleanup_recipes"]
        .as_array()
        .unwrap_or_else(|| panic!("cleanup_recipes is an array"));
    assert!(
        recipes
            .iter()
            .all(|recipe| recipe["affected_path"] != media_path.as_str()),
        "large-directory item leaked into cleanup recipes"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn classify_artifact_broadens_cache_and_system_rules() {
    let base = test_root("broadened-rules");
    let now_millis = storage_now_millis();
    let cases: &[(&[&str], &str, &str, &str)] = &[
        (
            &["Library", "Caches", "Homebrew"],
            "homebrew-cache",
            "safe",
            "safe",
        ),
        (
            &["Library", "Caches", "pip"],
            "pip-cache",
            "rebuildable",
            "safe",
        ),
        (
            &["Library", "Caches", "go-build"],
            "go-build-cache",
            "safe",
            "safe",
        ),
        (
            &["Library", "Caches", "com.example.tool"],
            "app-cache",
            "rebuildable",
            "safe",
        ),
        (
            &["Library", "Caches", "com.google.Chrome"],
            "browser-cache",
            "risky",
            "review",
        ),
        (
            &["Library", "Caches", "com.apple.Safari"],
            "browser-cache",
            "risky",
            "review",
        ),
        (&[".cache", "uv"], "uv-cache", "rebuildable", "safe"),
        (
            &[".cache", "huggingface"],
            "tool-cache",
            "rebuildable",
            "safe",
        ),
        (
            &[".gradle", "caches"],
            "gradle-cache",
            "rebuildable",
            "safe",
        ),
        (
            &[".m2", "repository"],
            "maven-repository",
            "rebuildable",
            "review",
        ),
        (&[".Trash"], "trash", "safe", "review"),
        (
            &["Library", "Containers", "com.docker.docker", "Data", "vms"],
            "docker-vm",
            "expensive",
            "review",
        ),
        (
            &["Library", "Developer", "CoreSimulator", "Caches"],
            "simulator-cache",
            "safe",
            "safe",
        ),
        (
            &[
                "Library",
                "Developer",
                "Xcode",
                "iOS DeviceSupport",
                "17.0 (21A329)",
            ],
            "xcode-device-support",
            "rebuildable",
            "review",
        ),
        (
            &[
                "Library",
                "Developer",
                "Xcode",
                "DerivedData",
                "MyApp-abcdef",
            ],
            "xcode-derived-data",
            "safe",
            "safe",
        ),
    ];
    for (components, expected_kind, expected_tier, expected_safety) in cases {
        let mut path = base.clone();
        for component in *components {
            path = path.join(component);
        }
        if let Err(error) = fs::create_dir_all(&path) {
            panic!("create rule fixture {}: {error}", path.display());
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => panic!("stat rule fixture {}: {error}", path.display()),
        };
        let rule = classify_artifact(&path, &metadata, now_millis)
            .unwrap_or_else(|| panic!("no rule matched {}", path.display()));
        assert_eq!(
            rule.kind.as_ref(),
            *expected_kind,
            "kind for {}",
            path.display()
        );
        assert_eq!(
            rule.cleanup_tier.as_ref(),
            *expected_tier,
            "tier for {}",
            path.display()
        );
        assert_eq!(
            rule.safety.as_ref(),
            *expected_safety,
            "safety for {}",
            path.display()
        );
    }
    let _ = fs::remove_dir_all(base);
}

#[test]
fn storage_hygiene_items_page_admits_large_directory_rows() {
    let _index_guard = storage_index_test_guard();
    let root = test_root("items-page-large-directory");
    let large_dir = root.join("huge-unclassified");
    let small_dir = root.join("small-unclassified");
    let artifact = root.join("artifacts").join("artifact.bin");
    for directory in [&large_dir, &small_dir] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create index fixture dir: {error}");
        }
    }
    if let Some(parent) = artifact.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create artifact fixture dir: {error}");
    }
    if let Err(error) = fs::write(&artifact, b"fixture") {
        panic!("write artifact fixture: {error}");
    }
    let now_millis = storage_now_millis();
    let storage_index = StorageSizeIndex::open();
    assert_eq!(storage_index.status, "ready");
    let mut metrics = StorageScanMetrics::default();
    let mut large_row = seeded_index_row(
        &root,
        &large_dir,
        2 * LARGE_DIRECTORY_MIN_BYTES,
        "",
        None,
        Some(now_millis.saturating_sub(DAY_MILLIS)),
        Some(now_millis.saturating_sub(DAY_MILLIS)),
        now_millis,
    );
    large_row.kind = "large-directory".to_owned();
    large_row.storage_role = "artifact".to_owned();
    large_row.safety = "review".to_owned();
    large_row.is_directory = true;
    let mut small_row = seeded_index_row(
        &root,
        &small_dir,
        LARGE_DIRECTORY_MIN_BYTES / 2,
        "",
        None,
        Some(now_millis.saturating_sub(DAY_MILLIS)),
        Some(now_millis.saturating_sub(DAY_MILLIS)),
        now_millis,
    );
    small_row.kind = "large-directory".to_owned();
    small_row.storage_role = "artifact".to_owned();
    small_row.safety = "review".to_owned();
    small_row.is_directory = true;
    small_row.inode = 2;
    let mut classified_row = seeded_index_row(
        &root,
        &artifact,
        MIN_ITEM_BYTES * 4,
        "rebuildable",
        None,
        Some(now_millis.saturating_sub(DAY_MILLIS)),
        Some(now_millis.saturating_sub(DAY_MILLIS)),
        now_millis,
    );
    classified_row.inode = 3;
    for row in [&large_row, &small_row, &classified_row] {
        storage_index.store_indexed_row(row, &mut metrics);
    }
    storage_index.flush_pending_rows();
    drop(storage_index);

    let (paths, total_available, page_source) = items_page_paths(&root, 0, 10, "size", true);
    assert_eq!(page_source, "index");
    assert_eq!(
        total_available, 2,
        "large-directory row above the threshold plus the classified row"
    );
    assert_eq!(
        paths,
        vec![
            large_dir.display().to_string(),
            artifact.display().to_string()
        ],
        "page admits the large-directory row and excludes the sub-threshold one"
    );

    // The hydrated large-directory item stays review-only.
    let page = must_ok(
        storage_hygiene_items_page_json(
            vec![root.display().to_string()],
            5,
            0,
            10,
            "instant_cached",
            "size",
            true,
        ),
        "items page serializes",
    );
    let page = parse_json_value(&page, "items page JSON parses");
    let large_item = page["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["kind"] == "large-directory"))
        .unwrap_or_else(|| panic!("large-directory item present in page"));
    assert_eq!(large_item["cleanup_tier"], "");
    assert_eq!(large_item["cleanup_allowed"], false);
    assert_eq!(large_item["recommendation_score"], 0.0);

    let _ = fs::remove_dir_all(root);
}
