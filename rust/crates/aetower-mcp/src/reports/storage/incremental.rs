use super::*;

#[cfg(not(test))]
const STORAGE_MEASUREMENT_LEASE_MILLIS: u64 = 30_000;
#[cfg(not(test))]
const STORAGE_BACKGROUND_WORKER_POLL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug)]
pub(super) struct StorageIncrementalDrainPolicy {
    dirty_batch_limit: usize,
    per_subtree_budget: Duration,
    report_item_limit: usize,
    worker_round_limit: usize,
    idle_round_limit: usize,
    continuation_delay: Duration,
    round_budget_error: &'static str,
}

impl StorageIncrementalDrainPolicy {
    pub(super) fn background_launch() -> Self {
        Self {
            dirty_batch_limit: 2,
            per_subtree_budget: Duration::from_millis(220),
            report_item_limit: 24,
            worker_round_limit: 1,
            idle_round_limit: 1,
            continuation_delay: Duration::from_millis(1_500),
            round_budget_error: "incremental_background_round_budget_exhausted",
        }
    }

    fn full_drain() -> Self {
        Self {
            dirty_batch_limit: 8,
            per_subtree_budget: Duration::from_millis(900),
            report_item_limit: 40,
            worker_round_limit: 64,
            idle_round_limit: 2,
            continuation_delay: Duration::from_millis(500),
            round_budget_error: "incremental_worker_round_budget_exhausted",
        }
    }
}

pub(super) fn ensure_dirty_storage_subtree_measurement(
    roots: &[PathBuf],
    dirty_summary: &StorageDirtyPathSummary,
) {
    if dirty_summary.dirty_path_count == 0 {
        return;
    }
    let storage_index = StorageSizeIndex::open();
    let queued_paths = storage_index.load_dirty_path_strings(roots, 512);
    let now_millis = storage_now_millis();
    let result = StorageIncrementalMeasurementResult {
        started_at_millis: now_millis,
        dirty_paths: queued_paths,
        partial: true,
        continuation_pending: true,
        last_error: Some("durable_measurement_queued".to_owned()),
        ..StorageIncrementalMeasurementResult::default()
    };
    storage_index.record_incremental_measurement_job(roots, &[], &result, now_millis);
}

#[cfg(not(test))]
pub(crate) fn start_storage_background_services() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let _ = thread::Builder::new()
        .name("aetower-storage-worker".to_owned())
        .spawn(storage_background_worker_loop);
}

#[cfg(not(test))]
fn storage_background_worker_loop() {
    loop {
        let storage_index = StorageSizeIndex::open();
        let now_millis = storage_now_millis();
        match storage_index
            .claim_next_storage_measurement_job(now_millis, STORAGE_MEASUREMENT_LEASE_MILLIS)
        {
            Ok(Some(job)) => run_claimed_storage_measurement_job(&storage_index, job),
            Ok(None) => thread::sleep(STORAGE_BACKGROUND_WORKER_POLL),
            Err(_) => thread::sleep(STORAGE_BACKGROUND_WORKER_POLL),
        }
    }
}

#[cfg(not(test))]
fn run_claimed_storage_measurement_job(
    storage_index: &StorageSizeIndex,
    job: StorageMeasurementJob,
) {
    match job.job_kind.as_str() {
        "dirty_subtree_incremental" => {
            run_dirty_storage_subtree_measurement_worker_with_policy(
                storage_index,
                &job.roots,
                StorageIncrementalDrainPolicy::background_launch(),
            );
        }
        "legacy_file_index_backfill" => {
            let result = storage_index
                .indexed_source_roots()
                .and_then(|source_roots| {
                    storage_index
                        .refresh_materialized_storage_for_source_roots_checked(&source_roots)
                });
            if let Err(error) = result {
                let _ = storage_index.finish_storage_measurement_job(
                    &job.job_id,
                    false,
                    Some(&error),
                    storage_now_millis(),
                );
            } else {
                let _ = storage_index.finish_storage_measurement_job(
                    &job.job_id,
                    true,
                    None,
                    storage_now_millis(),
                );
            }
        }
        _ => {
            let error = format!("unsupported_measurement_job_kind:{}", job.job_kind);
            let _ = storage_index.finish_storage_measurement_job(
                &job.job_id,
                false,
                Some(&error),
                storage_now_millis(),
            );
        }
    }
}

pub(super) fn run_dirty_storage_subtree_measurement_worker(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
) {
    run_dirty_storage_subtree_measurement_worker_with_policy(
        storage_index,
        roots,
        StorageIncrementalDrainPolicy::full_drain(),
    );
}

pub(super) fn run_dirty_storage_subtree_measurement_worker_with_policy(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    policy: StorageIncrementalDrainPolicy,
) {
    let mut idle_rounds = 0usize;
    for round in 0..policy.worker_round_limit {
        let mut result =
            measure_dirty_storage_subtrees_once_with_policy(storage_index, roots, policy);
        let remaining_dirty = storage_index.dirty_path_summary(roots, 1).dirty_path_count;
        if remaining_dirty == 0 {
            break;
        }
        if result.measured_path_count == 0 && result.measured_file_count == 0 {
            idle_rounds = idle_rounds.saturating_add(1);
            if idle_rounds >= policy.idle_round_limit && !result.partial {
                break;
            }
        } else {
            idle_rounds = 0;
        }
        if round + 1 >= policy.worker_round_limit {
            result.partial = true;
            result.continuation_pending = true;
            result.last_error = Some(policy.round_budget_error.to_owned());
            storage_index.record_incremental_measurement_job(
                roots,
                &[],
                &result,
                storage_now_millis(),
            );
            break;
        }
        thread::sleep(policy.continuation_delay);
    }
}

pub(super) fn measure_dirty_storage_subtrees_once(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
) -> StorageIncrementalMeasurementResult {
    measure_dirty_storage_subtrees_once_with_policy(
        storage_index,
        roots,
        StorageIncrementalDrainPolicy::full_drain(),
    )
}

pub(super) fn measure_dirty_storage_subtrees_once_with_policy(
    storage_index: &StorageSizeIndex,
    roots: &[PathBuf],
    policy: StorageIncrementalDrainPolicy,
) -> StorageIncrementalMeasurementResult {
    let started_at_millis = storage_now_millis();
    let dirty_records = storage_index.load_dirty_path_records(roots, policy.dirty_batch_limit);
    let dirty_paths = dirty_records
        .iter()
        .map(|record| record.path.clone())
        .collect::<Vec<_>>();
    let mut result = StorageIncrementalMeasurementResult {
        started_at_millis,
        dirty_paths: dirty_paths.clone(),
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
    let mut deferred_dirty_paths = Vec::new();
    for record in dirty_records {
        let path = PathBuf::from(&record.path);
        let source_root = storage_index.source_root_for_incremental_path(&path, roots);
        affected_source_roots.insert(source_root.display().to_string());

        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_symlink() => metadata,
            Ok(_) | Err(_) => {
                match storage_index.remove_indexed_subtree_checked(
                    &path,
                    roots,
                    storage_now_millis(),
                ) {
                    Ok(source_roots) => {
                        affected_source_roots.extend(source_roots);
                        completed_dirty_paths.push(record.path);
                    }
                    Err(error) => {
                        result.partial = true;
                        result.continuation_pending = true;
                        result.last_error = Some(format!("remove_indexed_subtree:{error}"));
                        deferred_dirty_paths.push(record.path);
                    }
                }
                continue;
            }
        };
        if !metadata.is_dir() && !metadata.is_file() {
            match storage_index.remove_indexed_subtree_checked(&path, roots, storage_now_millis()) {
                Ok(source_roots) => {
                    affected_source_roots.extend(source_roots);
                    completed_dirty_paths.push(record.path);
                }
                Err(error) => {
                    result.partial = true;
                    result.continuation_pending = true;
                    result.last_error = Some(format!("remove_indexed_subtree:{error}"));
                    deferred_dirty_paths.push(record.path);
                }
            }
            continue;
        }

        let mut collector = StorageCandidateCollector::new(policy.report_item_limit);
        let options = StorageHygieneOptions {
            max_depth: 12,
            limit: policy.report_item_limit,
            mode: StorageScanMode::FastChangedOnly,
            runtime: None,
            dirty_paths: Vec::new(),
        };
        let deadline = Instant::now() + policy.per_subtree_budget;
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
            result.continuation_pending = true;
            result.last_error = Some("incremental_subtree_budget_exhausted".to_owned());
            deferred_dirty_paths.push(record.path);
        } else {
            completed_dirty_paths.push(record.path);
        }
    }

    if let Err(error) = storage_index.flush_pending_rows_checked() {
        result.partial = true;
        result.continuation_pending = true;
        result.last_error = Some(format!("flush_incremental_measurements:{error}"));
    } else if let Err(error) =
        storage_index.refresh_materialized_storage_for_source_roots_checked(&affected_source_roots)
    {
        result.partial = true;
        result.continuation_pending = true;
        result.last_error = Some(format!("refresh_incremental_materialized_index:{error}"));
    }
    if result.last_error.is_none()
        && !completed_dirty_paths.is_empty()
        && let Err(error) = storage_index
            .mark_dirty_paths_clean_checked(&completed_dirty_paths, storage_now_millis())
    {
        result.partial = true;
        result.continuation_pending = true;
        result.last_error = Some(format!("mark_incremental_paths_clean:{error}"));
    }
    if !deferred_dirty_paths.is_empty()
        && let Err(error) = storage_index
            .mark_dirty_paths_deferred_checked(&deferred_dirty_paths, storage_now_millis())
    {
        result.partial = true;
        result.continuation_pending = true;
        result.last_error = Some(format!("mark_incremental_paths_deferred:{error}"));
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
