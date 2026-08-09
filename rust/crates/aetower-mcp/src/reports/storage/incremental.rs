use super::*;

const STORAGE_INCREMENTAL_DIRTY_BATCH_LIMIT: usize = 24;
const STORAGE_INCREMENTAL_PER_SUBTREE_BUDGET: Duration = Duration::from_millis(1_500);
const STORAGE_INCREMENTAL_REPORT_ITEM_LIMIT: usize = 40;

pub(super) fn ensure_dirty_storage_subtree_measurement(
    roots: &[PathBuf],
    dirty_summary: &StorageDirtyPathSummary,
) {
    if dirty_summary.dirty_path_count == 0 {
        return;
    }
    let roots = roots.to_vec();
    let roots_key = incremental_roots_key(&roots);
    let active = incremental_measurement_roots();
    {
        let mut active_roots = lock_or_recover(active);
        if !active_roots.insert(roots_key.clone()) {
            return;
        }
    }

    let worker_roots_key = roots_key.clone();
    match thread::Builder::new()
        .name("aetower-storage-incremental".to_owned())
        .spawn(move || {
            let _guard = IncrementalActiveRootGuard {
                roots_key: worker_roots_key,
            };
            let storage_index = StorageSizeIndex::open();
            let _ = measure_dirty_storage_subtrees_once(&storage_index, &roots);
        }) {
        Ok(_handle) => {}
        Err(_) => {
            lock_or_recover(active).remove(&roots_key);
        }
    }
}

pub(super) fn measure_dirty_storage_subtrees_once(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
) -> StorageIncrementalMeasurementResult {
    let started_at_millis = storage_now_millis();
    let dirty_records =
        storage_index.load_dirty_path_records(roots, STORAGE_INCREMENTAL_DIRTY_BATCH_LIMIT);
    let dirty_paths = dirty_records
        .iter()
        .map(|record| record.path.clone())
        .collect::<Vec<_>>();
    let mut result = StorageIncrementalMeasurementResult {
        started_at_millis,
        ..StorageIncrementalMeasurementResult::default()
    };
    if dirty_records.is_empty() {
        storage_index.record_incremental_measurement_job(
            roots,
            &dirty_paths,
            &result,
            storage_now_millis(),
        );
        super::projection::persist_storage_situation_snapshot_for_index(
            storage_index,
            roots,
            "incremental_dirty_subtree_idle",
        );
        return result;
    }

    let mut metrics = StorageScanMetrics {
        storage_index_status: storage_index.status.clone(),
        ..StorageScanMetrics::default()
    };
    let mut affected_source_roots = BTreeSet::new();
    let mut completed_dirty_paths = Vec::new();
    for record in dirty_records {
        let path = PathBuf::from(&record.path);
        let source_root = storage_index.source_root_for_incremental_path(&path, roots);
        affected_source_roots.insert(source_root.display().to_string());

        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_symlink() => metadata,
            Ok(_) | Err(_) => {
                affected_source_roots.extend(storage_index.remove_indexed_subtree(
                    &path,
                    roots,
                    storage_now_millis(),
                ));
                completed_dirty_paths.push(record.path);
                continue;
            }
        };
        if !metadata.is_dir() && !metadata.is_file() {
            affected_source_roots.extend(storage_index.remove_indexed_subtree(
                &path,
                roots,
                storage_now_millis(),
            ));
            completed_dirty_paths.push(record.path);
            continue;
        }

        let mut collector = StorageCandidateCollector::new(STORAGE_INCREMENTAL_REPORT_ITEM_LIMIT);
        let options = StorageHygieneOptions {
            max_depth: 12,
            limit: STORAGE_INCREMENTAL_REPORT_ITEM_LIMIT,
            mode: StorageScanMode::FastChangedOnly,
            runtime: None,
            dirty_paths: vec![record.path.clone()],
        };
        let deadline = Instant::now() + STORAGE_INCREMENTAL_PER_SUBTREE_BUDGET;
        let scan_result = scan_root_with_source_root(
            &path,
            &source_root,
            &options,
            deadline,
            storage_now_millis(),
            storage_index,
            &mut collector,
            &mut metrics,
        );
        result.measured_directory_count = result
            .measured_directory_count
            .saturating_add(scan_result.scanned_dirs);
        result.partial |= scan_result.walk_truncated || scan_result.sizing_truncated;
        if scan_result.walk_truncated || scan_result.sizing_truncated {
            result.last_error = Some("incremental_subtree_budget_exhausted".to_owned());
        } else {
            completed_dirty_paths.push(record.path);
        }
    }

    storage_index.flush_pending_rows();
    storage_index.refresh_materialized_storage_for_source_roots(&affected_source_roots);
    if !completed_dirty_paths.is_empty() {
        storage_index.mark_dirty_paths_clean(&completed_dirty_paths, storage_now_millis());
    }
    result.measured_path_count = metrics.storage_index_writes;
    result.measured_file_count = metrics.sized_entry_count;
    storage_index.record_incremental_measurement_job(
        roots,
        &dirty_paths,
        &result,
        storage_now_millis(),
    );
    super::projection::persist_storage_situation_snapshot_for_index(
        storage_index,
        roots,
        "incremental_dirty_subtree",
    );
    result
}

fn incremental_measurement_roots() -> &'static Mutex<BTreeSet<String>> {
    static ACTIVE: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(BTreeSet::new()))
}

struct IncrementalActiveRootGuard {
    roots_key: String,
}

impl Drop for IncrementalActiveRootGuard {
    fn drop(&mut self) {
        lock_or_recover(incremental_measurement_roots()).remove(&self.roots_key);
    }
}

fn incremental_roots_key(roots: &[PathBuf]) -> String {
    roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join("\u{1f}")
}
