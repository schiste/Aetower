use super::*;

#[derive(Clone, Debug)]
pub(super) struct StorageScanPersistedState {
    pub(super) progress: StorageScanJobProgress,
    pub(super) persisted_at_millis: u64,
}

#[derive(Clone, Debug)]
pub(super) struct StorageScanPersistedRecord {
    pub(super) job_id: String,
    pub(super) signature: String,
    pub(super) volume_key: String,
    pub(super) roots: Vec<String>,
    pub(super) dirty_paths: Vec<String>,
    pub(super) max_depth: usize,
    pub(super) limit: usize,
    pub(super) mode: String,
    pub(super) throttle_hint: String,
    pub(super) status: String,
    pub(super) progress: StorageScanJobProgress,
    pub(super) started_at_millis: u64,
    pub(super) updated_at_millis: u64,
    pub(super) completed_at_millis: Option<u64>,
    pub(super) result_available: bool,
    pub(super) resume_available: bool,
}

/// Directory holding the persistent storage index database. Unit tests get a
/// process-scoped temporary directory so they neither pollute the user's live
/// index nor race the running app for the WAL writer lock.
#[cfg(not(test))]
fn storage_index_directory() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base_dir| base_dir.join("Aetower"))
}

#[cfg(test)]
thread_local! {
    static STORAGE_INDEX_TEST_DIRECTORY_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[cfg(test)]
fn storage_index_directory() -> Option<PathBuf> {
    if let Some(directory) =
        STORAGE_INDEX_TEST_DIRECTORY_OVERRIDE.with(|override_dir| override_dir.borrow().clone())
    {
        return Some(directory);
    }
    static TEST_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
    Some(
        TEST_DIRECTORY
            .get_or_init(|| {
                std::env::temp_dir()
                    .join(format!("aetower-storage-index-test-{}", std::process::id()))
            })
            .clone(),
    )
}

#[cfg(test)]
pub(super) fn replace_storage_index_directory_for_test(
    directory: Option<PathBuf>,
) -> Option<PathBuf> {
    STORAGE_INDEX_TEST_DIRECTORY_OVERRIDE.with(|override_dir| override_dir.replace(directory))
}

pub(super) struct StorageScanStateStore;

impl StorageScanStateStore {
    fn open_connection() -> Result<Connection, String> {
        let directory = storage_index_directory().ok_or_else(|| "no_data_dir".to_owned())?;
        fs::create_dir_all(&directory).map_err(|error| format!("create_dir:{error}"))?;
        let path = directory.join(STORAGE_INDEX_FILE_NAME);
        let connection = Connection::open(path).map_err(|error| format!("open_failed:{error}"))?;
        StorageSizeIndex::prepare_schema(&connection).map_err(|error| format!("schema:{error}"))?;
        Ok(connection)
    }

    pub(super) fn persist(record: StorageScanPersistedRecord) -> Result<u64, String> {
        let connection = Self::open_connection()?;
        let progress_json = serde_json::to_string(&record.progress)
            .map_err(|error| format!("encode_progress:{error}"))?;
        let roots_json = serde_json::to_string(&record.roots)
            .map_err(|error| format!("encode_roots:{error}"))?;
        let dirty_paths_json = serde_json::to_string(&record.dirty_paths)
            .map_err(|error| format!("encode_dirty_paths:{error}"))?;
        let persisted_at_millis = storage_now_millis();
        connection
            .execute(
                "INSERT INTO storage_scan_job_state (
                    job_id,
                    signature,
                    volume_key,
                    roots_json,
                    dirty_paths_json,
                    max_depth,
                    limit_count,
                    mode,
                    throttle_hint,
                    status,
                    progress_json,
                    started_at_millis,
                    updated_at_millis,
                    completed_at_millis,
                    result_available,
                    resume_available,
                    persisted_at_millis
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
                 )
                 ON CONFLICT(job_id) DO UPDATE SET
                    signature = excluded.signature,
                    volume_key = excluded.volume_key,
                    roots_json = excluded.roots_json,
                    dirty_paths_json = excluded.dirty_paths_json,
                    max_depth = excluded.max_depth,
                    limit_count = excluded.limit_count,
                    mode = excluded.mode,
                    throttle_hint = excluded.throttle_hint,
                    status = excluded.status,
                    progress_json = excluded.progress_json,
                    started_at_millis = excluded.started_at_millis,
                    updated_at_millis = excluded.updated_at_millis,
                    completed_at_millis = excluded.completed_at_millis,
                    result_available = excluded.result_available,
                    resume_available = excluded.resume_available,
                    persisted_at_millis = excluded.persisted_at_millis",
                params![
                    record.job_id,
                    record.signature,
                    record.volume_key,
                    roots_json,
                    dirty_paths_json,
                    record.max_depth.min(i64::MAX as usize) as i64,
                    record.limit.min(i64::MAX as usize) as i64,
                    record.mode,
                    record.throttle_hint,
                    record.status,
                    progress_json,
                    record.started_at_millis.min(i64::MAX as u64) as i64,
                    record.updated_at_millis.min(i64::MAX as u64) as i64,
                    record
                        .completed_at_millis
                        .map(|value| value.min(i64::MAX as u64) as i64),
                    i64::from(record.result_available),
                    i64::from(record.resume_available),
                    persisted_at_millis.min(i64::MAX as u64) as i64,
                ],
            )
            .map_err(|error| format!("persist:{error}"))?;
        Self::prune_old(&connection, persisted_at_millis);
        Ok(persisted_at_millis)
    }

    pub(super) fn load_resume_candidate(signature: &str) -> Option<StorageScanPersistedState> {
        let connection = Self::open_connection().ok()?;
        let now_millis = storage_now_millis();
        Self::prune_old(&connection, now_millis);
        let min_updated_millis = now_millis.saturating_sub(STORAGE_SCAN_STATE_MAX_AGE_MILLIS);
        let mut statement = connection
            .prepare(
                "SELECT progress_json, persisted_at_millis
                 FROM storage_scan_job_state
                 WHERE signature = ?1
                   AND resume_available = 1
                   AND status IN ('queued', 'running', 'paused')
                   AND updated_at_millis >= ?2
                 ORDER BY updated_at_millis DESC
                 LIMIT 1",
            )
            .ok()?;
        statement
            .query_row(
                params![signature, min_updated_millis.min(i64::MAX as u64) as i64],
                |row| {
                    let progress_json: String = row.get(0)?;
                    let persisted_at_millis: i64 = row.get(1)?;
                    let progress = serde_json::from_str::<StorageScanJobProgress>(&progress_json)
                        .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(StorageScanPersistedState {
                        progress,
                        persisted_at_millis: persisted_at_millis.max(0) as u64,
                    })
                },
            )
            .ok()
    }

    fn prune_old(connection: &Connection, now_millis: u64) {
        let cutoff = now_millis.saturating_sub(STORAGE_SCAN_STATE_MAX_AGE_MILLIS);
        let _ = connection.execute(
            "UPDATE storage_scan_job_state
             SET status = 'failed',
                 resume_available = 0,
                 completed_at_millis = COALESCE(completed_at_millis, updated_at_millis)
             WHERE updated_at_millis < ?1
               AND status IN ('queued', 'running', 'paused')",
            params![cutoff.min(i64::MAX as u64) as i64],
        );
        let _ = connection.execute(
            "DELETE FROM storage_scan_job_state
             WHERE updated_at_millis < ?1
               AND status NOT IN ('queued', 'running', 'paused')",
            params![cutoff.min(i64::MAX as u64) as i64],
        );
    }

    #[cfg(test)]
    pub(super) fn load_status_for_job(job_id: &str) -> Option<String> {
        let connection = Self::open_connection().ok()?;
        connection
            .query_row(
                "SELECT status FROM storage_scan_job_state WHERE job_id = ?1",
                params![job_id],
                |row| row.get(0),
            )
            .ok()
    }
}

#[derive(Clone, Debug)]
pub(super) struct StorageIndexedFileRow {
    pub(super) path: String,
    pub(super) device: i64,
    pub(super) inode: i64,
    pub(super) file_id: String,
    pub(super) source_root: String,
    pub(super) repo_root: Option<String>,
    pub(super) kind: String,
    pub(super) storage_role: String,
    pub(super) safety: String,
    pub(super) cleanup_tier: String,
    pub(super) logical_bytes: u64,
    pub(super) physical_bytes: u64,
    pub(super) modified_millis: Option<u64>,
    pub(super) changed_millis: Option<u64>,
    pub(super) accessed_millis: Option<u64>,
    pub(super) birth_millis: Option<u64>,
    pub(super) is_directory: bool,
    pub(super) entries: u64,
    pub(super) truncated: bool,
    pub(super) last_scan_millis: u64,
}

#[derive(Clone, Debug)]
struct MaterializedStoragePathRow {
    path: String,
    parent_path: String,
    name: String,
    device: i64,
    inode: i64,
    file_id: String,
    source_root: String,
    repo_root: Option<String>,
    path_kind: String,
    artifact_kind: String,
    storage_role: String,
    safety: String,
    cleanup_tier: String,
    logical_bytes: u64,
    physical_bytes: u64,
    modified_millis: Option<u64>,
    changed_millis: Option<u64>,
    accessed_millis: Option<u64>,
    birth_millis: Option<u64>,
    entries: u64,
    truncated: bool,
    recommendation_score: f64,
    last_measured_millis: u64,
}

#[derive(Clone, Debug, Default)]
struct MaterializedStorageDomainAggregate {
    item_count: u64,
    directory_count: u64,
    file_count: u64,
    logical_bytes: u64,
    physical_bytes: u64,
    safely_reclaimable_now_bytes: u64,
    maybe_reclaimable_bytes: u64,
    review_required_bytes: u64,
    dangerous_user_data_bytes: u64,
    started_at_millis: u64,
    completed_at_millis: u64,
    partial: bool,
}

#[derive(Clone, Debug)]
struct StorageCachedSizeRow {
    size: SizeWalkResult,
    fingerprint: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct StorageItemRowsPage {
    pub(super) rows: Vec<StorageIndexedFileRow>,
    pub(super) total_available: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageDirtyPathRecord {
    pub(super) path: String,
    pub(super) source: String,
    pub(super) flags: u64,
    pub(super) first_seen_millis: u64,
    pub(super) last_seen_millis: u64,
    pub(super) event_count: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct StorageDirtyPathSummary {
    pub(super) dirty_path_count: u64,
    pub(super) oldest_dirty_millis: Option<u64>,
    pub(super) latest_dirty_millis: Option<u64>,
    pub(super) latest_event_id: Option<u64>,
    pub(super) sample_paths: Vec<String>,
    #[serde(default)]
    pub(super) unknown_gap: bool,
    #[serde(default)]
    pub(super) unknown_gap_roots: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct StorageIncrementalMeasurementResult {
    pub(super) started_at_millis: u64,
    pub(super) measured_path_count: u64,
    pub(super) measured_directory_count: u64,
    pub(super) measured_file_count: u64,
    pub(super) measured_bytes: u64,
    pub(super) partial: bool,
    pub(super) continuation_pending: bool,
    pub(super) last_error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct StorageIndexSummaryRow {
    pub(super) source_root: String,
    pub(super) item_count: u64,
    pub(super) inventory_size_bytes: u64,
    pub(super) safe_reclaimable_bytes: u64,
    pub(super) maybe_reclaimable_bytes: u64,
    pub(super) review_required_bytes: u64,
    pub(super) dangerous_user_data_bytes: u64,
}

#[derive(Clone, Debug)]
pub(super) struct StorageTopOffenderRow {
    pub(super) source_root: String,
    pub(super) path: String,
    pub(super) kind: String,
    pub(super) cleanup_tier: String,
    pub(super) physical_bytes: u64,
    pub(super) recommendation_score: f64,
    pub(super) last_scan_millis: u64,
}

const STORAGE_INDEX_STALE_EVICTION_MAX_PASSES: usize = 128;
const STORAGE_DIRTY_QUEUE_CANDIDATE_CAP: usize = 8192;
const STORAGE_DIRTY_QUEUE_BACKPRESSURE_MAX_ROWS: u64 = 12_000;
const STORAGE_DIRTY_QUEUE_BACKPRESSURE_ROOT_CAP: usize = 128;
const STORAGE_DIRTY_QUEUE_NOISY_EVENT_COUNT: u64 = 8;
const STORAGE_DIRTY_QUEUE_NOISY_WINDOW_MILLIS: u64 = 30_000;
const STORAGE_DIRTY_QUEUE_DEBOUNCE_MILLIS: u64 = 2_000;

#[derive(Clone, Debug)]
struct StorageDirtyPathPolicyRecord {
    record: StorageDirtyPathRecord,
    priority_score: f64,
    debounced: bool,
}

pub(super) struct StorageSizeIndex {
    connection: Option<Connection>,
    path: Option<PathBuf>,
    pub(super) status: String,
    /// Rows buffered by `store_indexed_row` and written in chunked
    /// transactions by `flush_pending_rows`. The walk is single-threaded per
    /// index instance, so interior mutability with `RefCell` is sufficient.
    pending_rows: RefCell<Vec<StorageIndexedFileRow>>,
    budget_flush_count: RefCell<u64>,
}

impl Drop for StorageSizeIndex {
    fn drop(&mut self) {
        // End-of-scan / cancellation safety net: whatever is still buffered
        // must reach the database before the connection closes.
        self.flush_pending_rows();
        // Refresh the query-planner statistics when table sizes changed enough
        // to matter (SQLite's own growth heuristic); a no-op otherwise. Stale
        // or missing statistics make the planner fall back to full scans and
        // per-row b-tree seeks for the report aggregation queries, which is
        // catastrophic once a deep scan grows the index to hundreds of
        // thousands of rows.
        if let Some(connection) = self.connection.as_ref() {
            let _ = connection.execute_batch("PRAGMA optimize;");
        }
        self.enforce_storage_index_budget(StorageIndexBudgetLimits::default());
    }
}

#[derive(Clone, Copy)]
struct StorageIndexBudgetLimits {
    target_bytes: u64,
    hard_cap_bytes: u64,
    max_file_rows: u64,
    max_size_rows: u64,
    max_growth_delta_rows: u64,
}

impl Default for StorageIndexBudgetLimits {
    fn default() -> Self {
        Self {
            target_bytes: STORAGE_INDEX_TARGET_BYTES,
            hard_cap_bytes: STORAGE_INDEX_HARD_CAP_BYTES,
            max_file_rows: STORAGE_FILE_INDEX_MAX_ROWS,
            max_size_rows: STORAGE_SIZE_INDEX_MAX_ROWS,
            max_growth_delta_rows: STORAGE_GROWTH_TOP_OFFENDER_DELTA_ROWS,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct RepositoryInventoryCacheEntry {
    pub(super) discovered_root: String,
    pub(super) repository_fingerprint: String,
    pub(super) last_seen_millis: u64,
    pub(super) last_scan_millis: u64,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RepositoryInventoryCacheState {
    pub(super) status: String,
    pub(super) fingerprint: String,
    pub(super) fingerprint_changed: bool,
    pub(super) last_seen_millis: Option<u64>,
    pub(super) last_scan_millis: Option<u64>,
}

impl StorageSizeIndex {
    fn with_status(connection: Option<Connection>, path: Option<PathBuf>, status: String) -> Self {
        Self {
            connection,
            path,
            status,
            pending_rows: RefCell::new(Vec::new()),
            budget_flush_count: RefCell::new(0),
        }
    }

    pub(super) fn open() -> Self {
        let Some(directory) = storage_index_directory() else {
            return Self::with_status(None, None, "unavailable:no_data_dir".to_owned());
        };
        if let Err(error) = fs::create_dir_all(&directory) {
            return Self::with_status(None, None, format!("unavailable:create_dir:{error}"));
        }
        let path = directory.join(STORAGE_INDEX_FILE_NAME);
        let Ok(connection) = Connection::open(&path) else {
            return Self::with_status(None, None, "unavailable:open_failed".to_owned());
        };
        if let Err(error) = Self::prepare_schema(&connection) {
            return Self::with_status(None, None, format!("unavailable:schema:{error}"));
        }
        // Index reads/writes tolerate short writer contention instead of
        // silently dropping rows. The scan-job state store deliberately keeps
        // the default fail-fast behavior so cancel/pause stay responsive.
        let _ = connection.busy_timeout(Duration::from_millis(2_000));
        #[cfg(not(test))]
        let _ = Self::backfill_materialized_storage_index(&connection);
        // Best effort (a concurrent writer may hold the lock; the next open
        // retries): without `sqlite_stat1` the planner picks full-scan and
        // per-row rowid-seek plans for every report aggregation query, which
        // turned the instant_cached report path into a multi-minute burn once
        // the index reached ~350k rows.
        let _ = Self::ensure_query_planner_statistics(&connection);
        let index = Self::with_status(Some(connection), Some(path), "ready".to_owned());
        index.enforce_storage_index_budget(StorageIndexBudgetLimits::default());
        index
    }

    #[cfg(test)]
    pub(super) fn open_in_directory_for_test(directory: &Path) -> Self {
        if let Err(error) = fs::create_dir_all(directory) {
            return Self::with_status(None, None, format!("unavailable:create_dir:{error}"));
        }
        let path = directory.join(STORAGE_INDEX_FILE_NAME);
        let Ok(connection) = Connection::open(&path) else {
            return Self::with_status(None, None, "unavailable:open_failed".to_owned());
        };
        if let Err(error) = Self::prepare_schema(&connection) {
            return Self::with_status(None, None, format!("unavailable:schema:{error}"));
        }
        let _ = connection.busy_timeout(Duration::from_millis(2_000));
        Self::with_status(Some(connection), Some(path), "ready".to_owned())
    }

    /// Connection-less handle whose reads and writes all no-op. Production
    /// code now opens the index for every scan mode; tests use this to
    /// exercise the unavailable-index paths.
    #[cfg(test)]
    pub(super) fn disabled(reason: &str) -> Self {
        Self::with_status(None, None, format!("disabled:{reason}"))
    }

    fn prepare_schema(connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS storage_index_meta (
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO storage_index_meta (key, value)
                VALUES ('schema_version', 2);
             CREATE TABLE IF NOT EXISTS storage_size_index (
                path TEXT PRIMARY KEY,
                device INTEGER NOT NULL,
                inode INTEGER NOT NULL,
                modified_millis INTEGER NOT NULL,
                changed_millis INTEGER NOT NULL,
                kind TEXT NOT NULL,
                repo_root TEXT,
                size_bytes INTEGER NOT NULL,
                allocated_bytes INTEGER NOT NULL,
                entries INTEGER NOT NULL,
                truncated INTEGER NOT NULL,
                last_scan_millis INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS storage_file_index (
                path TEXT PRIMARY KEY,
                device INTEGER NOT NULL,
                inode INTEGER NOT NULL,
                file_id TEXT NOT NULL,
                source_root TEXT NOT NULL,
                repo_root TEXT,
                kind TEXT NOT NULL,
                storage_role TEXT NOT NULL,
                safety TEXT NOT NULL,
                cleanup_tier TEXT NOT NULL,
                logical_bytes INTEGER NOT NULL,
                physical_bytes INTEGER NOT NULL,
                modified_millis INTEGER,
                changed_millis INTEGER,
                accessed_millis INTEGER,
                birth_millis INTEGER,
                is_directory INTEGER NOT NULL,
                entries INTEGER NOT NULL,
                truncated INTEGER NOT NULL,
                last_scan_millis INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_source
                ON storage_file_index(source_root, physical_bytes DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_repo
                ON storage_file_index(repo_root, physical_bytes DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_last_scan
                ON storage_file_index(last_scan_millis);
             CREATE TABLE IF NOT EXISTS storage_growth_delta (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bucket_millis INTEGER NOT NULL,
                scan_millis INTEGER NOT NULL,
                path TEXT NOT NULL,
                source_root TEXT NOT NULL,
                repo_root TEXT,
                kind TEXT NOT NULL,
                cleanup_tier TEXT NOT NULL,
                previous_physical_bytes INTEGER NOT NULL,
                current_physical_bytes INTEGER NOT NULL,
                delta_bytes INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_storage_growth_delta_bucket
                ON storage_growth_delta(bucket_millis DESC, delta_bytes DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_growth_delta_path
                ON storage_growth_delta(path, bucket_millis DESC);
             CREATE TABLE IF NOT EXISTS storage_growth_rollup (
                granularity TEXT NOT NULL,
                bucket_millis INTEGER NOT NULL,
                source_root TEXT NOT NULL,
                repo_root TEXT NOT NULL,
                kind TEXT NOT NULL,
                cleanup_tier TEXT NOT NULL,
                total_delta_bytes INTEGER NOT NULL,
                positive_delta_bytes INTEGER NOT NULL,
                negative_delta_bytes INTEGER NOT NULL,
                changed_path_count INTEGER NOT NULL,
                max_abs_delta_bytes INTEGER NOT NULL,
                updated_at_millis INTEGER NOT NULL,
                PRIMARY KEY (
                    granularity, bucket_millis, source_root, repo_root, kind, cleanup_tier
                )
             );
             CREATE INDEX IF NOT EXISTS idx_storage_growth_rollup_scope
                ON storage_growth_rollup(granularity, bucket_millis, source_root, repo_root);
             CREATE TABLE IF NOT EXISTS storage_index_summary (
                source_root TEXT PRIMARY KEY,
                captured_at_millis INTEGER NOT NULL,
                item_count INTEGER NOT NULL,
                inventory_size_bytes INTEGER NOT NULL,
                safe_reclaimable_bytes INTEGER NOT NULL,
                maybe_reclaimable_bytes INTEGER NOT NULL,
                review_required_bytes INTEGER NOT NULL,
                dangerous_user_data_bytes INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS storage_top_offender (
                source_root TEXT NOT NULL,
                path TEXT NOT NULL,
                kind TEXT NOT NULL,
                cleanup_tier TEXT NOT NULL,
                physical_bytes INTEGER NOT NULL,
                recommendation_score REAL NOT NULL DEFAULT 0,
                last_scan_millis INTEGER NOT NULL,
                PRIMARY KEY (source_root, path)
             );
             CREATE INDEX IF NOT EXISTS idx_storage_top_offender_rank
                ON storage_top_offender(source_root, recommendation_score DESC, physical_bytes DESC);
             CREATE TABLE IF NOT EXISTS storage_repository_inventory_cache (
                repo_root TEXT PRIMARY KEY,
                discovered_root TEXT NOT NULL,
                git_config_fingerprint TEXT NOT NULL,
                git_index_fingerprint TEXT NOT NULL,
                repository_fingerprint TEXT NOT NULL DEFAULT '',
                first_seen_millis INTEGER NOT NULL,
                last_seen_millis INTEGER NOT NULL,
                last_scan_millis INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_storage_repository_inventory_cache_root
                ON storage_repository_inventory_cache(discovered_root, last_seen_millis DESC);
             CREATE TABLE IF NOT EXISTS storage_scan_job_state (
                job_id TEXT PRIMARY KEY,
                signature TEXT NOT NULL,
                volume_key TEXT NOT NULL,
                roots_json TEXT NOT NULL,
                dirty_paths_json TEXT NOT NULL,
                max_depth INTEGER NOT NULL,
                limit_count INTEGER NOT NULL,
                mode TEXT NOT NULL,
                throttle_hint TEXT NOT NULL,
                status TEXT NOT NULL,
                progress_json TEXT NOT NULL,
                started_at_millis INTEGER NOT NULL,
                updated_at_millis INTEGER NOT NULL,
                completed_at_millis INTEGER,
                result_available INTEGER NOT NULL DEFAULT 0,
                resume_available INTEGER NOT NULL DEFAULT 0,
                persisted_at_millis INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_storage_scan_job_state_signature
                ON storage_scan_job_state(signature, updated_at_millis DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_scan_job_state_status
                ON storage_scan_job_state(status, updated_at_millis DESC);
             CREATE TABLE IF NOT EXISTS storage_dirty_path (
                path TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                flags INTEGER NOT NULL DEFAULT 0,
                last_event_id INTEGER,
                first_seen_millis INTEGER NOT NULL,
                last_seen_millis INTEGER NOT NULL,
                event_count INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'dirty',
                last_error TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_storage_dirty_path_status
                ON storage_dirty_path(status, last_seen_millis DESC);
             CREATE TABLE IF NOT EXISTS storage_event_cursor (
                source TEXT PRIMARY KEY,
                last_event_id INTEGER,
                updated_at_millis INTEGER NOT NULL,
                status TEXT NOT NULL,
                detail TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS storage_unknown_gap (
                root_path TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                reason TEXT NOT NULL,
                first_seen_millis INTEGER NOT NULL,
                last_seen_millis INTEGER NOT NULL,
                last_event_id INTEGER,
                unresolved INTEGER NOT NULL DEFAULT 1
             );
             CREATE INDEX IF NOT EXISTS idx_storage_unknown_gap_unresolved
                ON storage_unknown_gap(unresolved, last_seen_millis DESC);
             CREATE TABLE IF NOT EXISTS storage_path_fingerprint (
                path TEXT PRIMARY KEY,
                fingerprint BLOB NOT NULL,
                source TEXT NOT NULL,
                measured_at_millis INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_storage_path_fingerprint_measured
                ON storage_path_fingerprint(measured_at_millis DESC);
             CREATE TABLE IF NOT EXISTS storage_situation_snapshot (
                root_key TEXT PRIMARY KEY,
                roots_json TEXT NOT NULL,
                captured_at_millis INTEGER NOT NULL,
                updated_at_millis INTEGER NOT NULL,
                source TEXT NOT NULL,
                item_count INTEGER NOT NULL,
                inventory_size_bytes INTEGER NOT NULL,
                safely_reclaimable_now_bytes INTEGER NOT NULL,
                dirty_path_count INTEGER NOT NULL,
                snapshot_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_storage_situation_snapshot_updated
                ON storage_situation_snapshot(updated_at_millis DESC);",
        )?;
        let schema: i64 = connection.query_row(
            "SELECT value FROM storage_index_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        if schema != STORAGE_INDEX_SCHEMA_VERSION {
            connection.execute_batch(
                "DROP TABLE IF EXISTS storage_size_index;
                 DROP TABLE IF EXISTS storage_file_index;
                 DROP TABLE IF EXISTS storage_growth_delta;
                 DROP TABLE IF EXISTS storage_growth_rollup;
                 DROP TABLE IF EXISTS storage_index_summary;
                 DROP TABLE IF EXISTS storage_top_offender;
                 UPDATE storage_index_meta SET value = 2 WHERE key = 'schema_version';
                 CREATE TABLE storage_size_index (
                    path TEXT PRIMARY KEY,
                    device INTEGER NOT NULL,
                    inode INTEGER NOT NULL,
                    modified_millis INTEGER NOT NULL,
                    changed_millis INTEGER NOT NULL,
                    kind TEXT NOT NULL,
                    repo_root TEXT,
                    size_bytes INTEGER NOT NULL,
                    allocated_bytes INTEGER NOT NULL,
                    entries INTEGER NOT NULL,
                    truncated INTEGER NOT NULL,
                    last_scan_millis INTEGER NOT NULL
                 );
                 CREATE TABLE storage_file_index (
                    path TEXT PRIMARY KEY,
                    device INTEGER NOT NULL,
                    inode INTEGER NOT NULL,
                    file_id TEXT NOT NULL,
                    source_root TEXT NOT NULL,
                    repo_root TEXT,
                    kind TEXT NOT NULL,
                    storage_role TEXT NOT NULL,
                    safety TEXT NOT NULL,
                    cleanup_tier TEXT NOT NULL,
                    logical_bytes INTEGER NOT NULL,
                    physical_bytes INTEGER NOT NULL,
                    modified_millis INTEGER,
                    changed_millis INTEGER,
                    accessed_millis INTEGER,
                    birth_millis INTEGER,
                    is_directory INTEGER NOT NULL,
                    entries INTEGER NOT NULL,
                    truncated INTEGER NOT NULL,
                    last_scan_millis INTEGER NOT NULL
                 );
                 CREATE INDEX idx_storage_file_index_source
                    ON storage_file_index(source_root, physical_bytes DESC);
                 CREATE INDEX idx_storage_file_index_repo
                    ON storage_file_index(repo_root, physical_bytes DESC);
                 CREATE INDEX idx_storage_file_index_last_scan
                    ON storage_file_index(last_scan_millis);
                 CREATE TABLE storage_growth_delta (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    bucket_millis INTEGER NOT NULL,
                    scan_millis INTEGER NOT NULL,
                    path TEXT NOT NULL,
                    source_root TEXT NOT NULL,
                    repo_root TEXT,
                    kind TEXT NOT NULL,
                    cleanup_tier TEXT NOT NULL,
                    previous_physical_bytes INTEGER NOT NULL,
                    current_physical_bytes INTEGER NOT NULL,
                    delta_bytes INTEGER NOT NULL
                 );
                 CREATE INDEX idx_storage_growth_delta_bucket
                    ON storage_growth_delta(bucket_millis DESC, delta_bytes DESC);
                 CREATE INDEX idx_storage_growth_delta_path
                    ON storage_growth_delta(path, bucket_millis DESC);
                 CREATE TABLE storage_growth_rollup (
                    granularity TEXT NOT NULL,
                    bucket_millis INTEGER NOT NULL,
                    source_root TEXT NOT NULL,
                    repo_root TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    cleanup_tier TEXT NOT NULL,
                    total_delta_bytes INTEGER NOT NULL,
                    positive_delta_bytes INTEGER NOT NULL,
                    negative_delta_bytes INTEGER NOT NULL,
                    changed_path_count INTEGER NOT NULL,
                    max_abs_delta_bytes INTEGER NOT NULL,
                    updated_at_millis INTEGER NOT NULL,
                    PRIMARY KEY (
                        granularity, bucket_millis, source_root, repo_root, kind, cleanup_tier
                    )
                 );
                 CREATE INDEX idx_storage_growth_rollup_scope
                    ON storage_growth_rollup(granularity, bucket_millis, source_root, repo_root);
                 CREATE TABLE storage_index_summary (
                    source_root TEXT PRIMARY KEY,
                    captured_at_millis INTEGER NOT NULL,
                    item_count INTEGER NOT NULL,
                    inventory_size_bytes INTEGER NOT NULL,
                    safe_reclaimable_bytes INTEGER NOT NULL,
                    maybe_reclaimable_bytes INTEGER NOT NULL,
                    review_required_bytes INTEGER NOT NULL,
                    dangerous_user_data_bytes INTEGER NOT NULL
                 );
                 CREATE TABLE storage_top_offender (
                    source_root TEXT NOT NULL,
                    path TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    cleanup_tier TEXT NOT NULL,
                    physical_bytes INTEGER NOT NULL,
                    recommendation_score REAL NOT NULL DEFAULT 0,
                    last_scan_millis INTEGER NOT NULL,
                    PRIMARY KEY (source_root, path)
                 );
                 CREATE INDEX idx_storage_top_offender_rank
                    ON storage_top_offender(source_root, recommendation_score DESC, physical_bytes DESC);
                 CREATE TABLE storage_repository_inventory_cache (
                    repo_root TEXT PRIMARY KEY,
                    discovered_root TEXT NOT NULL,
                    git_config_fingerprint TEXT NOT NULL,
                    git_index_fingerprint TEXT NOT NULL,
                    repository_fingerprint TEXT NOT NULL DEFAULT '',
                    first_seen_millis INTEGER NOT NULL,
                    last_seen_millis INTEGER NOT NULL,
                    last_scan_millis INTEGER NOT NULL
                 );
                 CREATE INDEX idx_storage_repository_inventory_cache_root
                    ON storage_repository_inventory_cache(discovered_root, last_seen_millis DESC);
                 CREATE TABLE IF NOT EXISTS storage_scan_job_state (
                    job_id TEXT PRIMARY KEY,
                    signature TEXT NOT NULL,
                    volume_key TEXT NOT NULL,
                    roots_json TEXT NOT NULL,
                    dirty_paths_json TEXT NOT NULL,
                    max_depth INTEGER NOT NULL,
                    limit_count INTEGER NOT NULL,
                    mode TEXT NOT NULL,
                    throttle_hint TEXT NOT NULL,
                    status TEXT NOT NULL,
                    progress_json TEXT NOT NULL,
                    started_at_millis INTEGER NOT NULL,
                    updated_at_millis INTEGER NOT NULL,
                    completed_at_millis INTEGER,
                    result_available INTEGER NOT NULL DEFAULT 0,
                    resume_available INTEGER NOT NULL DEFAULT 0,
                    persisted_at_millis INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_storage_scan_job_state_signature
                    ON storage_scan_job_state(signature, updated_at_millis DESC);
                 CREATE INDEX IF NOT EXISTS idx_storage_scan_job_state_status
                    ON storage_scan_job_state(status, updated_at_millis DESC);
                 CREATE TABLE IF NOT EXISTS storage_dirty_path (
                    path TEXT PRIMARY KEY,
                    source TEXT NOT NULL,
                    flags INTEGER NOT NULL DEFAULT 0,
                    last_event_id INTEGER,
                    first_seen_millis INTEGER NOT NULL,
                    last_seen_millis INTEGER NOT NULL,
                    event_count INTEGER NOT NULL DEFAULT 1,
                    status TEXT NOT NULL DEFAULT 'dirty',
                    last_error TEXT
                 );
                 CREATE INDEX IF NOT EXISTS idx_storage_dirty_path_status
                    ON storage_dirty_path(status, last_seen_millis DESC);
                 CREATE TABLE IF NOT EXISTS storage_event_cursor (
                    source TEXT PRIMARY KEY,
                    last_event_id INTEGER,
                    updated_at_millis INTEGER NOT NULL,
                    status TEXT NOT NULL,
                    detail TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS storage_unknown_gap (
                    root_path TEXT PRIMARY KEY,
                    source TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    first_seen_millis INTEGER NOT NULL,
                    last_seen_millis INTEGER NOT NULL,
                    last_event_id INTEGER,
                    unresolved INTEGER NOT NULL DEFAULT 1
                 );
                 CREATE INDEX IF NOT EXISTS idx_storage_unknown_gap_unresolved
                    ON storage_unknown_gap(unresolved, last_seen_millis DESC);
                 CREATE TABLE IF NOT EXISTS storage_path_fingerprint (
                    path TEXT PRIMARY KEY,
                    fingerprint BLOB NOT NULL,
                    source TEXT NOT NULL,
                    measured_at_millis INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_storage_path_fingerprint_measured
                    ON storage_path_fingerprint(measured_at_millis DESC);
                 CREATE TABLE IF NOT EXISTS storage_situation_snapshot (
                    root_key TEXT PRIMARY KEY,
                    roots_json TEXT NOT NULL,
                    captured_at_millis INTEGER NOT NULL,
                    updated_at_millis INTEGER NOT NULL,
                    source TEXT NOT NULL,
                    item_count INTEGER NOT NULL,
                    inventory_size_bytes INTEGER NOT NULL,
                    safely_reclaimable_now_bytes INTEGER NOT NULL,
                    dirty_path_count INTEGER NOT NULL,
                    snapshot_json TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_storage_situation_snapshot_updated
                    ON storage_situation_snapshot(updated_at_millis DESC);",
            )?;
        }
        Self::ensure_repository_inventory_cache_columns(connection)?;
        Self::ensure_storage_file_index_columns(connection)?;
        Self::ensure_storage_file_index_page_indexes(connection)?;
        Self::ensure_storage_growth_delta_indexes(connection)?;
        Self::ensure_storage_growth_rollups(connection)?;
        Self::ensure_storage_dirty_path_columns(connection)?;
        Self::ensure_materialized_storage_index_schema(connection)?;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn backfill_materialized_storage_index_now(&self) -> Result<(), String> {
        let Some(connection) = self.connection.as_ref() else {
            return Err(self.status.clone());
        };
        Self::backfill_materialized_storage_index(connection)
            .map_err(|error| format!("materialized_backfill:{error}"))
    }

    /// Additive DDL only (no `schema_version` bump): index-generation lookups
    /// and the per-flush retention DELETE both key on `scan_millis`, which the
    /// original schema never indexed — each was a full table scan once the
    /// growth-delta table grew past a few hundred thousand rows. The covering
    /// aggregation index serves the growth-insight queries (windowed
    /// SUM/GROUP BY over scope with a roots predicate on `path`) without
    /// touching table rows; without it every insight query re-scanned the full
    /// table per report build.
    fn ensure_storage_growth_delta_indexes(connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_storage_growth_delta_scan
                ON storage_growth_delta(scan_millis);
             CREATE INDEX IF NOT EXISTS idx_storage_growth_delta_agg
                ON storage_growth_delta(bucket_millis, repo_root, source_root, path, delta_bytes);",
        )
    }

    /// One-time additive migration for indexes created before rollups existed.
    /// This must run before budget enforcement can prune raw path deltas.
    fn ensure_storage_growth_rollups(connection: &Connection) -> rusqlite::Result<()> {
        let existing_rollups: i64 =
            connection.query_row("SELECT COUNT(*) FROM storage_growth_rollup", [], |row| {
                row.get(0)
            })?;
        if existing_rollups > 0 {
            return Ok(());
        }
        let existing_deltas: i64 =
            connection.query_row("SELECT COUNT(*) FROM storage_growth_delta", [], |row| {
                row.get(0)
            })?;
        if existing_deltas == 0 {
            return Ok(());
        }
        connection.execute_batch(&format!(
            "INSERT OR IGNORE INTO storage_growth_rollup (
                granularity, bucket_millis, source_root, repo_root, kind, cleanup_tier,
                total_delta_bytes, positive_delta_bytes, negative_delta_bytes,
                changed_path_count, max_abs_delta_bytes, updated_at_millis
             )
             SELECT 'hour', bucket_millis, source_root, COALESCE(repo_root, ''), kind,
                    cleanup_tier, COALESCE(SUM(delta_bytes), 0),
                    COALESCE(SUM(CASE WHEN delta_bytes > 0 THEN delta_bytes ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN delta_bytes < 0 THEN delta_bytes ELSE 0 END), 0),
                    COUNT(*), COALESCE(MAX(ABS(delta_bytes)), 0), COALESCE(MAX(scan_millis), 0)
             FROM storage_growth_delta
             GROUP BY bucket_millis, source_root, COALESCE(repo_root, ''), kind, cleanup_tier;
             INSERT OR IGNORE INTO storage_growth_rollup (
                granularity, bucket_millis, source_root, repo_root, kind, cleanup_tier,
                total_delta_bytes, positive_delta_bytes, negative_delta_bytes,
                changed_path_count, max_abs_delta_bytes, updated_at_millis
             )
             SELECT 'day', (scan_millis / {DAY_MILLIS}) * {DAY_MILLIS}, source_root,
                    COALESCE(repo_root, ''), kind, cleanup_tier,
                    COALESCE(SUM(delta_bytes), 0),
                    COALESCE(SUM(CASE WHEN delta_bytes > 0 THEN delta_bytes ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN delta_bytes < 0 THEN delta_bytes ELSE 0 END), 0),
                    COUNT(*), COALESCE(MAX(ABS(delta_bytes)), 0), COALESCE(MAX(scan_millis), 0)
             FROM storage_growth_delta
            GROUP BY (scan_millis / {DAY_MILLIS}) * {DAY_MILLIS}, source_root,
                      COALESCE(repo_root, ''), kind, cleanup_tier;",
        ))
    }

    /// Additive normalized materialized-view schema. The legacy
    /// `storage_file_index` table remains the compatibility write/read source
    /// while these tables become the future query surface.
    fn ensure_materialized_storage_index_schema(connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS storage_path (
                path TEXT PRIMARY KEY,
                parent_path TEXT NOT NULL,
                name TEXT NOT NULL,
                device INTEGER NOT NULL,
                inode INTEGER NOT NULL,
                file_id TEXT NOT NULL,
                source_root TEXT NOT NULL,
                repo_root TEXT,
                path_kind TEXT NOT NULL,
                artifact_kind TEXT NOT NULL,
                storage_role TEXT NOT NULL,
                safety TEXT NOT NULL,
                cleanup_tier TEXT NOT NULL,
                logical_bytes INTEGER NOT NULL,
                physical_bytes INTEGER NOT NULL,
                modified_millis INTEGER,
                changed_millis INTEGER,
                accessed_millis INTEGER,
                birth_millis INTEGER,
                entries INTEGER NOT NULL,
                truncated INTEGER NOT NULL,
                recommendation_score REAL NOT NULL DEFAULT 0,
                last_measured_millis INTEGER NOT NULL,
                last_event_id INTEGER,
                confidence TEXT NOT NULL DEFAULT 'indexed',
                stale INTEGER NOT NULL DEFAULT 0,
                partial INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'storage_file_index'
             );
             CREATE INDEX IF NOT EXISTS idx_storage_path_source
                ON storage_path(source_root, physical_bytes DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_path_parent
                ON storage_path(parent_path, physical_bytes DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_path_repo
                ON storage_path(repo_root, physical_bytes DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_path_rank
                ON storage_path(recommendation_score DESC, physical_bytes DESC, path);
             CREATE INDEX IF NOT EXISTS idx_storage_path_measured
                ON storage_path(last_measured_millis DESC);
             CREATE TABLE IF NOT EXISTS storage_directory_rollup (
                path TEXT PRIMARY KEY,
                source_root TEXT NOT NULL,
                repo_root TEXT,
                logical_bytes INTEGER NOT NULL,
                physical_bytes INTEGER NOT NULL,
                child_count INTEGER NOT NULL,
                recursive_entry_count INTEGER NOT NULL,
                truncated INTEGER NOT NULL,
                last_measured_millis INTEGER NOT NULL,
                confidence TEXT NOT NULL DEFAULT 'indexed',
                partial INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'storage_file_index'
             );
             CREATE INDEX IF NOT EXISTS idx_storage_directory_rollup_source
                ON storage_directory_rollup(source_root, physical_bytes DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_directory_rollup_repo
                ON storage_directory_rollup(repo_root, physical_bytes DESC);
             CREATE TABLE IF NOT EXISTS storage_domain (
                domain_id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                source_root TEXT NOT NULL,
                domain_kind TEXT NOT NULL,
                path_prefix TEXT NOT NULL,
                item_count INTEGER NOT NULL,
                directory_count INTEGER NOT NULL,
                file_count INTEGER NOT NULL,
                logical_bytes INTEGER NOT NULL,
                physical_bytes INTEGER NOT NULL,
                safely_reclaimable_now_bytes INTEGER NOT NULL,
                maybe_reclaimable_bytes INTEGER NOT NULL,
                review_required_bytes INTEGER NOT NULL,
                dangerous_user_data_bytes INTEGER NOT NULL,
                last_measured_millis INTEGER NOT NULL,
                confidence TEXT NOT NULL DEFAULT 'indexed',
                source TEXT NOT NULL DEFAULT 'storage_file_index'
             );
             CREATE INDEX IF NOT EXISTS idx_storage_domain_source
                ON storage_domain(source_root);
             CREATE INDEX IF NOT EXISTS idx_storage_domain_kind
                ON storage_domain(domain_kind, physical_bytes DESC);
             CREATE TABLE IF NOT EXISTS storage_measurement_job (
                job_id TEXT PRIMARY KEY,
                job_kind TEXT NOT NULL,
                status TEXT NOT NULL,
                source TEXT NOT NULL,
                root_key TEXT NOT NULL,
                roots_json TEXT NOT NULL,
                dirty_paths_json TEXT NOT NULL DEFAULT '[]',
                started_at_millis INTEGER NOT NULL,
                updated_at_millis INTEGER NOT NULL,
                completed_at_millis INTEGER,
                measured_path_count INTEGER NOT NULL,
                measured_directory_count INTEGER NOT NULL,
                measured_file_count INTEGER NOT NULL,
                measured_bytes INTEGER NOT NULL,
                partial INTEGER NOT NULL DEFAULT 0,
                last_error TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_storage_measurement_job_root
                ON storage_measurement_job(root_key, updated_at_millis DESC);
             CREATE INDEX IF NOT EXISTS idx_storage_measurement_job_status
                ON storage_measurement_job(status, updated_at_millis DESC);",
        )
    }

    fn backfill_materialized_storage_index(connection: &Connection) -> rusqlite::Result<()> {
        let generation = materialized_storage_index_generation(connection)?;
        let current_generation = connection
            .query_row(
                "SELECT value FROM storage_index_meta
                 WHERE key = 'materialized_storage_index_generation'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok();
        let legacy_path_count = table_count(connection, "storage_file_index");
        let materialized_path_count = table_count(connection, "storage_path");
        if current_generation.as_deref() == Some(generation.as_str())
            && materialized_path_count == legacy_path_count
        {
            return Ok(());
        }
        if legacy_path_count > STORAGE_MATERIALIZED_SYNC_BACKFILL_MAX_ROWS {
            let deferred_generation = connection
                .query_row(
                    "SELECT value FROM storage_index_meta
                     WHERE key = 'materialized_storage_index_deferred_generation'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .ok();
            if deferred_generation.as_deref() != Some(generation.as_str()) {
                record_deferred_materialized_storage_backfill(
                    connection,
                    &generation,
                    legacy_path_count,
                )?;
            }
            return Ok(());
        }

        let mut statement =
            connection.prepare("SELECT DISTINCT source_root FROM storage_file_index")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let source_roots = rows.flatten().collect::<BTreeSet<_>>();
        refresh_materialized_storage_index_for_roots(connection, &source_roots)?;
        set_materialized_storage_index_generation(connection, &generation)?;
        connection.execute(
            "DELETE FROM storage_index_meta
             WHERE key = 'materialized_storage_index_deferred_generation'",
            [],
        )?;
        Ok(())
    }

    /// Run `ANALYZE` once for databases that have never collected planner
    /// statistics. Ongoing staleness is handled by `PRAGMA optimize` when the
    /// index handle drops.
    fn ensure_query_planner_statistics(connection: &Connection) -> rusqlite::Result<()> {
        let has_statistics: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sqlite_stat1'",
            [],
            |row| row.get(0),
        )?;
        if has_statistics == 0 {
            connection.execute_batch("ANALYZE;")?;
        }
        Ok(())
    }

    /// Additive migration (no `schema_version` bump, mirroring
    /// `ensure_repository_inventory_cache_columns`): records the cleanup tier
    /// a row had before its latest upsert so tier transitions can be surfaced,
    /// and the composite recommendation score computed at flush time. Rows
    /// written before the migration keep the 0 default until their next scan
    /// refreshes them.
    fn ensure_storage_file_index_columns(connection: &Connection) -> rusqlite::Result<()> {
        for (column, ddl) in [
            (
                "previous_cleanup_tier",
                "ALTER TABLE storage_file_index
                 ADD COLUMN previous_cleanup_tier TEXT NOT NULL DEFAULT ''",
            ),
            (
                "recommendation_score",
                "ALTER TABLE storage_file_index
                 ADD COLUMN recommendation_score REAL NOT NULL DEFAULT 0",
            ),
        ] {
            let exists: i64 = connection.query_row(
                "SELECT COUNT(*)
                 FROM pragma_table_info('storage_file_index')
                 WHERE name = ?1",
                params![column],
                |row| row.get(0),
            )?;
            if exists == 0 {
                tolerate_duplicate_column(connection.execute(ddl, []))?;
            }
        }
        Ok(())
    }

    /// Additive DDL only: partial indexes that back `load_item_rows_page` sort
    /// keys. These deliberately avoid a `schema_version` bump because the
    /// version-mismatch path drops the user's scan history tables.
    fn ensure_storage_file_index_page_indexes(connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_storage_file_index_page_size
                ON storage_file_index(physical_bytes DESC, path)
                WHERE cleanup_tier <> '';
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_page_modified
                ON storage_file_index(modified_millis DESC, path)
                WHERE cleanup_tier <> '';
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_page_accessed
                ON storage_file_index(accessed_millis DESC, path)
                WHERE cleanup_tier <> '';
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_page_tier
                ON storage_file_index(cleanup_tier, safety, path)
                WHERE cleanup_tier <> '';
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_page_kind
                ON storage_file_index(kind, path)
                WHERE cleanup_tier <> '';
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_page_score
                ON storage_file_index(recommendation_score DESC, path)
                WHERE cleanup_tier <> '';
             CREATE INDEX IF NOT EXISTS idx_storage_file_index_page_large_dir
                ON storage_file_index(physical_bytes DESC, path)
                WHERE kind = 'large-directory';",
        )
    }

    fn ensure_storage_dirty_path_columns(connection: &Connection) -> rusqlite::Result<()> {
        let exists: i64 = connection.query_row(
            "SELECT COUNT(*)
             FROM pragma_table_info('storage_dirty_path')
             WHERE name = 'last_event_id'",
            [],
            |row| row.get(0),
        )?;
        if exists == 0 {
            tolerate_duplicate_column(connection.execute(
                "ALTER TABLE storage_dirty_path
                 ADD COLUMN last_event_id INTEGER",
                [],
            ))?;
        }
        Ok(())
    }

    fn ensure_repository_inventory_cache_columns(connection: &Connection) -> rusqlite::Result<()> {
        let exists: i64 = connection.query_row(
            "SELECT COUNT(*)
             FROM pragma_table_info('storage_repository_inventory_cache')
             WHERE name = 'repository_fingerprint'",
            [],
            |row| row.get(0),
        )?;
        if exists == 0 {
            tolerate_duplicate_column(connection.execute(
                "ALTER TABLE storage_repository_inventory_cache
                 ADD COLUMN repository_fingerprint TEXT NOT NULL DEFAULT ''",
                [],
            ))?;
        }
        Ok(())
    }

    pub(super) fn lookup(
        &self,
        path: &Path,
        metadata: &fs::Metadata,
        kind: &str,
        dirty_paths: &[String],
        metrics: &mut StorageScanMetrics,
    ) -> Option<SizeWalkResult> {
        if path_matches_dirty_prefix(path, dirty_paths) {
            metrics.storage_index_misses = metrics.storage_index_misses.saturating_add(1);
            return None;
        }
        let connection = self.connection.as_ref()?;
        let path_display = path.display().to_string();
        let result = connection
            .query_row(
                "SELECT s.size_bytes, s.allocated_bytes, s.entries, s.truncated, f.fingerprint
                 FROM storage_size_index s
                 LEFT JOIN storage_path_fingerprint f
                    ON f.path = s.path
                 WHERE s.path = ?1
                   AND s.kind = ?2",
                params![&path_display, kind],
                size_walk_cache_row_from_sql,
            )
            .ok();
        if let Some(cached) = result {
            let matched_fingerprint = cached.fingerprint.as_ref().is_some_and(|fingerprint| {
                if metadata.is_dir()
                    && StoragePathFingerprint::encoded_version(fingerprint)
                        != Some(STORAGE_PATH_FINGERPRINT_VERSION)
                {
                    return false;
                }
                let current = if metadata.is_dir() {
                    let directory = StorageDirectoryFingerprint::for_path(
                        path,
                        cached.size.allocated_bytes,
                        !cached.size.truncated,
                        self.last_event_id_for_path(&path_display),
                    );
                    StoragePathFingerprint::from_metadata_with_directory(metadata, Some(directory))
                } else {
                    StoragePathFingerprint::from_metadata(metadata)
                };
                current.encode() == *fingerprint
            });
            if matched_fingerprint {
                metrics.storage_index_hits = metrics.storage_index_hits.saturating_add(1);
                return Some(cached.size);
            }
            if metadata.is_dir() {
                metrics.storage_index_misses = metrics.storage_index_misses.saturating_add(1);
                return None;
            }
        }
        let device = metadata.dev() as i64;
        let inode = metadata.ino() as i64;
        let modified_millis = unix_metadata_millis(metadata.mtime(), metadata.mtime_nsec());
        let changed_millis = unix_metadata_millis(metadata.ctime(), metadata.ctime_nsec());
        let result = connection
            .query_row(
                "SELECT size_bytes, allocated_bytes, entries, truncated
                 FROM storage_size_index
                 WHERE path = ?1
                   AND device = ?2
                   AND inode = ?3
                   AND modified_millis = ?4
                   AND changed_millis = ?5
                   AND kind = ?6",
                params![
                    path_display,
                    device,
                    inode,
                    modified_millis,
                    changed_millis,
                    kind
                ],
                size_walk_result_from_sql,
            )
            .ok();
        if result.is_some() {
            metrics.storage_index_hits = metrics.storage_index_hits.saturating_add(1);
        } else {
            metrics.storage_index_misses = metrics.storage_index_misses.saturating_add(1);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn store(
        &self,
        path: &Path,
        metadata: &fs::Metadata,
        kind: &str,
        repo_root: Option<&str>,
        size: &SizeWalkResult,
        now_millis: u64,
        metrics: &mut StorageScanMetrics,
    ) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let path = path.display().to_string();
        let directory = metadata.is_dir().then(|| {
            StorageDirectoryFingerprint::for_path(
                Path::new(&path),
                size.allocated_bytes,
                !size.truncated,
                self.last_event_id_for_path(&path),
            )
        });
        let fingerprint =
            StoragePathFingerprint::from_metadata_with_directory(metadata, directory).encode();
        let device = metadata.dev() as i64;
        let inode = metadata.ino() as i64;
        let modified_millis = unix_metadata_millis(metadata.mtime(), metadata.mtime_nsec());
        let changed_millis = unix_metadata_millis(metadata.ctime(), metadata.ctime_nsec());
        if connection
            .execute(
                "INSERT OR REPLACE INTO storage_size_index (
                    path, device, inode, modified_millis, changed_millis, kind, repo_root,
                    size_bytes, allocated_bytes, entries, truncated, last_scan_millis
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &path,
                    device,
                    inode,
                    modified_millis,
                    changed_millis,
                    kind,
                    repo_root,
                    size.bytes.min(i64::MAX as u64) as i64,
                    size.allocated_bytes.min(i64::MAX as u64) as i64,
                    size.entries.min(i64::MAX as u64) as i64,
                    if size.truncated { 1i64 } else { 0i64 },
                    now_millis.min(i64::MAX as u64) as i64
                ],
            )
            .is_ok()
        {
            metrics.storage_index_writes = metrics.storage_index_writes.saturating_add(1);
            self.store_path_fingerprint(&path, fingerprint, now_millis, "size_walk");
            self.mark_dirty_path_clean(&path, now_millis);
        }
    }

    #[cfg(test)]
    pub(super) fn record_filesystem_events(
        &self,
        records: &[StorageFilesystemEventRecord],
        roots: &[PathBuf],
        now_millis: u64,
    ) -> StorageDirtyPathSummary {
        self.record_filesystem_events_for_source(
            records,
            roots,
            now_millis,
            STORAGE_LEDGER_FSEVENTS_SOURCE,
        );
        self.dirty_path_summary(roots, 5)
    }

    pub(super) fn ingest_filesystem_events(
        &self,
        ledger_records: &[StorageFilesystemEventRecord],
        roots: &[PathBuf],
        now_millis: u64,
    ) -> StorageDirtyPathSummary {
        self.record_native_filesystem_events(roots, now_millis);
        self.record_filesystem_events_for_source(
            ledger_records,
            roots,
            now_millis,
            STORAGE_LEDGER_FSEVENTS_SOURCE,
        );
        self.dirty_path_summary(roots, 5)
    }

    fn record_native_filesystem_events(&self, roots: &[PathBuf], now_millis: u64) {
        if self.connection.is_none() {
            return;
        }
        let batch = poll_native_storage_filesystem_events(
            roots,
            self.event_cursor(STORAGE_NATIVE_FSEVENTS_SOURCE),
            now_millis,
        );
        self.record_filesystem_events_for_source(
            &batch.records,
            roots,
            now_millis,
            STORAGE_NATIVE_FSEVENTS_SOURCE,
        );
        self.mark_unknown_gap_roots(
            &batch.unknown_gap_roots,
            now_millis,
            STORAGE_NATIVE_FSEVENTS_SOURCE,
            batch.status.as_str(),
            batch.cursor,
        );
        self.update_event_cursor(
            STORAGE_NATIVE_FSEVENTS_SOURCE,
            batch.cursor,
            now_millis,
            batch.status.as_str(),
            batch.detail.as_str(),
        );
    }

    fn record_filesystem_events_for_source(
        &self,
        records: &[StorageFilesystemEventRecord],
        roots: &[PathBuf],
        now_millis: u64,
        cursor_source: &str,
    ) -> u64 {
        let Some(connection) = self.connection.as_ref() else {
            return 0;
        };
        let Ok(mut upsert) = connection.prepare(
            "INSERT INTO storage_dirty_path (
                path, source, flags, last_event_id, first_seen_millis, last_seen_millis,
                event_count, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, 'dirty')
             ON CONFLICT(path) DO UPDATE SET
                source = excluded.source,
                flags = storage_dirty_path.flags | excluded.flags,
                last_event_id = MAX(
                    COALESCE(storage_dirty_path.last_event_id, 0),
                    COALESCE(excluded.last_event_id, 0)
                ),
                last_seen_millis = MAX(storage_dirty_path.last_seen_millis, excluded.last_seen_millis),
                event_count = storage_dirty_path.event_count + excluded.event_count,
                status = 'dirty',
                last_error = NULL
             WHERE excluded.last_seen_millis > storage_dirty_path.last_seen_millis
                OR COALESCE(excluded.last_event_id, 0) > COALESCE(storage_dirty_path.last_event_id, 0)
                OR storage_dirty_path.status <> 'dirty'",
        ) else {
            return 0;
        };
        let mut inserted = 0u64;
        let mut latest_event_id = None;
        let mut unknown_gap_roots = BTreeSet::new();
        let last_event_id = self.event_cursor(cursor_source);
        for record in records {
            if let (Some(event_id), Some(cursor)) = (record.event_id, last_event_id)
                && event_id <= cursor
            {
                continue;
            }
            let Some(path) = record
                .path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if storage_dirty_event_path_is_ignored(path) {
                continue;
            }
            if !roots.is_empty() && !roots.iter().any(|root| path_is_under_root(path, root)) {
                continue;
            }
            let source = record.source.as_deref().unwrap_or(cursor_source);
            let flags = record.flags.unwrap_or_default();
            if storage_event_flags_indicate_unknown_gap(flags) {
                collect_unknown_gap_roots_for_event_path(path, roots, &mut unknown_gap_roots);
            }
            let dirty_path = self.coalesced_dirty_queue_path(path, roots);
            let event_millis = record.timestamp_millis.unwrap_or(now_millis);
            let event_count = record.event_count.unwrap_or(1).max(1);
            if upsert
                .execute(params![
                    &dirty_path,
                    source,
                    flags.min(i64::MAX as u64) as i64,
                    record
                        .event_id
                        .map(|value| value.min(i64::MAX as u64) as i64),
                    event_millis.min(i64::MAX as u64) as i64,
                    event_count.min(i64::MAX as u64) as i64,
                ])
                .unwrap_or(0)
                > 0
            {
                inserted = inserted.saturating_add(1);
                latest_event_id = latest_event_id.max(record.event_id);
            }
        }
        drop(upsert);
        self.mark_unknown_gap_roots(
            &unknown_gap_roots,
            now_millis,
            cursor_source,
            "event_stream_gap_flag",
            latest_event_id,
        );
        if inserted > 0 {
            let total_events = records
                .iter()
                .map(|record| record.event_count.unwrap_or(1).max(1))
                .fold(0u64, u64::saturating_add);
            let detail = format!(
                "ingested {inserted} filesystem event path(s) covering {total_events} event(s)"
            );
            self.update_event_cursor(cursor_source, latest_event_id, now_millis, "ready", &detail);
            self.collapse_volatile_dirty_paths(roots, now_millis);
            self.apply_dirty_queue_backpressure(roots, now_millis, cursor_source, latest_event_id);
            super::report::invalidate_index_report_sections_memo();
        }
        inserted
    }

    pub(super) fn load_dirty_path_records(
        &self,
        roots: &[PathBuf],
        limit: usize,
    ) -> Vec<StorageDirtyPathRecord> {
        self.load_dirty_path_records_with_policy(roots, limit, storage_now_millis())
    }

    #[cfg(test)]
    pub(super) fn load_dirty_path_records_for_test(
        &self,
        roots: &[PathBuf],
        limit: usize,
        now_millis: u64,
    ) -> Vec<StorageDirtyPathRecord> {
        self.load_dirty_path_records_with_policy(roots, limit, now_millis)
    }

    fn load_dirty_path_records_with_policy(
        &self,
        roots: &[PathBuf],
        limit: usize,
        now_millis: u64,
    ) -> Vec<StorageDirtyPathRecord> {
        self.collapse_volatile_dirty_paths(roots, now_millis);
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let limit = limit.clamp(1, 4096);
        let Ok(mut statement) = connection.prepare(
            "SELECT path, source, flags, first_seen_millis, last_seen_millis, event_count
             FROM storage_dirty_path
             WHERE status = 'dirty'
             ORDER BY first_seen_millis ASC, last_seen_millis DESC, path ASC
             LIMIT ?1",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(
            params![
                limit
                    .saturating_mul(16)
                    .clamp(1, STORAGE_DIRTY_QUEUE_CANDIDATE_CAP) as i64
            ],
            dirty_path_record_from_sql,
        ) else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for record in rows.flatten() {
            if !roots.is_empty()
                && !roots
                    .iter()
                    .any(|root| path_is_under_root(&record.path, root))
            {
                continue;
            }
            candidates.push(self.score_dirty_path_record(record, roots, now_millis));
            if candidates.len() >= STORAGE_DIRTY_QUEUE_CANDIDATE_CAP {
                break;
            }
        }
        candidates.sort_by(|left, right| {
            left.debounced
                .cmp(&right.debounced)
                .then_with(|| {
                    right
                        .priority_score
                        .partial_cmp(&left.priority_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left.record
                        .first_seen_millis
                        .cmp(&right.record.first_seen_millis)
                })
                .then_with(|| {
                    right
                        .record
                        .last_seen_millis
                        .cmp(&left.record.last_seen_millis)
                })
                .then_with(|| left.record.path.cmp(&right.record.path))
        });
        let mut records = Vec::new();
        for candidate in candidates {
            if candidate.debounced && records.len() >= limit {
                break;
            }
            if records.iter().any(|existing: &StorageDirtyPathRecord| {
                path_is_under_root(&candidate.record.path, Path::new(&existing.path))
            }) {
                continue;
            }
            records.push(candidate.record);
            if records.len() >= limit {
                break;
            }
        }
        records
    }

    fn collapse_volatile_dirty_paths(&self, roots: &[PathBuf], now_millis: u64) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let Ok(mut statement) = connection.prepare(
            "SELECT path, source, flags, first_seen_millis, last_seen_millis, event_count
             FROM storage_dirty_path
             WHERE status = 'dirty'
             ORDER BY first_seen_millis ASC, last_seen_millis DESC, path ASC
             LIMIT ?1",
        ) else {
            return;
        };
        let Ok(rows) = statement.query_map(
            params![STORAGE_DIRTY_QUEUE_CANDIDATE_CAP as i64],
            dirty_path_record_from_sql,
        ) else {
            return;
        };
        let mut aggregates = BTreeMap::<String, StorageDirtyPathRecord>::new();
        let mut ignored_paths = Vec::new();
        for record in rows.flatten() {
            if !roots.is_empty()
                && !roots
                    .iter()
                    .any(|root| path_is_under_root(&record.path, root))
            {
                continue;
            }
            if storage_dirty_event_path_is_ignored(&record.path) {
                ignored_paths.push(record.path);
                continue;
            }
            let Some(ancestor) = volatile_dirty_queue_ancestor(&record.path) else {
                continue;
            };
            if ancestor == record.path {
                continue;
            }
            aggregates
                .entry(ancestor.clone())
                .and_modify(|existing| {
                    existing.flags |= record.flags;
                    existing.first_seen_millis =
                        existing.first_seen_millis.min(record.first_seen_millis);
                    existing.last_seen_millis =
                        existing.last_seen_millis.max(record.last_seen_millis);
                    existing.event_count = existing.event_count.saturating_add(record.event_count);
                })
                .or_insert_with(|| StorageDirtyPathRecord {
                    path: ancestor,
                    source: record.source,
                    flags: record.flags,
                    first_seen_millis: record.first_seen_millis,
                    last_seen_millis: record.last_seen_millis,
                    event_count: record.event_count,
                });
        }
        drop(statement);
        if aggregates.is_empty() && ignored_paths.is_empty() {
            return;
        }
        let Ok(transaction) = connection.unchecked_transaction() else {
            return;
        };
        for ignored_path in ignored_paths {
            let _ = transaction.execute(
                "UPDATE storage_dirty_path
                 SET status = 'clean',
                     last_seen_millis = MAX(last_seen_millis, ?2),
                     last_error = 'ignored_storage_event_noise'
                 WHERE path = ?1",
                params![ignored_path, now_millis.min(i64::MAX as u64) as i64],
            );
        }
        for aggregate in aggregates.values() {
            let _ = transaction.execute(
                "INSERT INTO storage_dirty_path (
                    path, source, flags, last_event_id, first_seen_millis, last_seen_millis,
                    event_count, status
                 ) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, 'dirty')
                 ON CONFLICT(path) DO UPDATE SET
                    source = excluded.source,
                    flags = storage_dirty_path.flags | excluded.flags,
                    first_seen_millis = MIN(storage_dirty_path.first_seen_millis, excluded.first_seen_millis),
                    last_seen_millis = MAX(storage_dirty_path.last_seen_millis, excluded.last_seen_millis),
                    event_count = storage_dirty_path.event_count + excluded.event_count,
                    status = 'dirty',
                    last_error = NULL",
                params![
                    &aggregate.path,
                    &aggregate.source,
                    aggregate.flags.min(i64::MAX as u64) as i64,
                    aggregate.first_seen_millis.min(i64::MAX as u64) as i64,
                    aggregate.last_seen_millis.min(i64::MAX as u64) as i64,
                    aggregate.event_count.min(i64::MAX as u64) as i64,
                ],
            );
            let child_prefix = format!("{}/", aggregate.path);
            let _ = transaction.execute(
                "UPDATE storage_dirty_path
                 SET status = 'clean',
                     last_seen_millis = MAX(last_seen_millis, ?2)
                 WHERE status = 'dirty'
                   AND path <> ?1
                   AND substr(path, 1, ?3) = ?4",
                params![
                    &aggregate.path,
                    now_millis.min(i64::MAX as u64) as i64,
                    child_prefix.len().min(i64::MAX as usize) as i64,
                    child_prefix,
                ],
            );
        }
        let _ = transaction.commit();
    }

    pub(super) fn load_dirty_path_strings(&self, roots: &[PathBuf], limit: usize) -> Vec<String> {
        self.load_dirty_path_records(roots, limit)
            .into_iter()
            .map(|record| record.path)
            .collect()
    }

    pub(super) fn dirty_path_summary(
        &self,
        roots: &[PathBuf],
        sample_limit: usize,
    ) -> StorageDirtyPathSummary {
        let Some(connection) = self.connection.as_ref() else {
            return StorageDirtyPathSummary::default();
        };
        let mut records = self.load_dirty_path_records(roots, sample_limit);
        let mut dirty_path_count = 0u64;
        let mut oldest_dirty_millis = None::<u64>;
        let mut latest_dirty_millis = None::<u64>;
        let Ok(mut statement) = connection.prepare(
            "SELECT path, first_seen_millis, last_seen_millis
             FROM storage_dirty_path
             WHERE status = 'dirty'",
        ) else {
            return StorageDirtyPathSummary::default();
        };
        if let Ok(rows) = statement.query_map([], |row| {
            let path: String = row.get(0)?;
            let first_seen_millis: i64 = row.get(1)?;
            let last_seen_millis: i64 = row.get(2)?;
            Ok((
                path,
                first_seen_millis.max(0) as u64,
                last_seen_millis.max(0) as u64,
            ))
        }) {
            for row in rows.flatten() {
                if !roots.is_empty() && !roots.iter().any(|root| path_is_under_root(&row.0, root)) {
                    continue;
                }
                dirty_path_count = dirty_path_count.saturating_add(1);
                oldest_dirty_millis = Some(
                    oldest_dirty_millis
                        .map(|value| value.min(row.1))
                        .unwrap_or(row.1),
                );
                latest_dirty_millis = Some(
                    latest_dirty_millis
                        .map(|value| value.max(row.2))
                        .unwrap_or(row.2),
                );
            }
        }
        let latest_event_id = [
            self.event_cursor(STORAGE_LEDGER_FSEVENTS_SOURCE),
            self.event_cursor(STORAGE_NATIVE_FSEVENTS_SOURCE),
        ]
        .into_iter()
        .flatten()
        .max();
        let unknown_gap_roots = self.load_unknown_gap_roots(roots, sample_limit);
        if records.len() > sample_limit {
            records.truncate(sample_limit);
        }
        StorageDirtyPathSummary {
            dirty_path_count,
            oldest_dirty_millis,
            latest_dirty_millis,
            latest_event_id,
            sample_paths: records.into_iter().map(|record| record.path).collect(),
            unknown_gap: !unknown_gap_roots.is_empty(),
            unknown_gap_roots,
        }
    }

    pub(super) fn mark_dirty_paths_clean(&self, paths: &[String], now_millis: u64) {
        for path in paths {
            self.mark_dirty_path_clean(path, now_millis);
        }
    }

    fn mark_dirty_path_clean(&self, path: &str, now_millis: u64) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let child_prefix = format!("{path}/");
        let _ = connection.execute(
            "UPDATE storage_dirty_path
             SET status = 'clean',
                 last_seen_millis = MAX(last_seen_millis, ?2)
             WHERE path = ?1
                OR substr(path, 1, ?3) = ?4",
            params![
                path,
                now_millis.min(i64::MAX as u64) as i64,
                child_prefix.len().min(i64::MAX as usize) as i64,
                child_prefix,
            ],
        );
        let _ = connection.execute(
            "UPDATE storage_unknown_gap
             SET unresolved = 0,
                 last_seen_millis = MAX(last_seen_millis, ?2)
             WHERE unresolved <> 0
               AND (root_path = ?1 OR substr(root_path, 1, ?3) = ?4)",
            params![
                path,
                now_millis.min(i64::MAX as u64) as i64,
                child_prefix.len().min(i64::MAX as usize) as i64,
                child_prefix,
            ],
        );
    }

    fn coalesced_dirty_queue_path(&self, path: &str, roots: &[PathBuf]) -> String {
        if let Some(volatile_ancestor) = volatile_dirty_queue_ancestor(path) {
            return volatile_ancestor;
        }
        let Some(connection) = self.connection.as_ref() else {
            return path.to_owned();
        };
        nearest_indexed_dirty_queue_ancestor(connection, path, roots)
            .unwrap_or_else(|| path.to_owned())
    }

    fn score_dirty_path_record(
        &self,
        record: StorageDirtyPathRecord,
        roots: &[PathBuf],
        now_millis: u64,
    ) -> StorageDirtyPathPolicyRecord {
        let Some(connection) = self.connection.as_ref() else {
            return StorageDirtyPathPolicyRecord {
                debounced: dirty_path_record_is_debounced(&record, now_millis),
                priority_score: 0.0,
                record,
            };
        };
        let domain_kind = dirty_queue_domain_kind(connection, &record.path);
        let previous_bytes = dirty_queue_previous_physical_bytes(connection, &record.path);
        let visible_root_score = if roots.is_empty()
            || roots
                .iter()
                .any(|root| path_is_under_root(&record.path, root))
        {
            200.0
        } else {
            0.0
        };
        let age_millis = now_millis.saturating_sub(record.first_seen_millis);
        let age_score = (age_millis as f64 / 60_000.0).min(240.0);
        let event_pressure_score = record.event_count.min(64) as f64 * 2.0;
        let size_score = dirty_queue_size_score(previous_bytes);
        let domain_score = dirty_queue_domain_score(&record.path, domain_kind.as_deref());
        StorageDirtyPathPolicyRecord {
            debounced: dirty_path_record_is_debounced(&record, now_millis),
            priority_score: domain_score
                + visible_root_score
                + size_score
                + age_score
                + event_pressure_score,
            record,
        }
    }

    fn apply_dirty_queue_backpressure(
        &self,
        roots: &[PathBuf],
        now_millis: u64,
        source: &str,
        latest_event_id: Option<u64>,
    ) {
        self.apply_dirty_queue_backpressure_with_limit(
            roots,
            now_millis,
            source,
            latest_event_id,
            STORAGE_DIRTY_QUEUE_BACKPRESSURE_MAX_ROWS,
        );
    }

    #[cfg(test)]
    pub(super) fn apply_dirty_queue_backpressure_for_test(
        &self,
        roots: &[PathBuf],
        now_millis: u64,
        source: &str,
        latest_event_id: Option<u64>,
        max_dirty_rows: u64,
    ) {
        self.apply_dirty_queue_backpressure_with_limit(
            roots,
            now_millis,
            source,
            latest_event_id,
            max_dirty_rows,
        );
    }

    fn apply_dirty_queue_backpressure_with_limit(
        &self,
        roots: &[PathBuf],
        now_millis: u64,
        source: &str,
        latest_event_id: Option<u64>,
        max_dirty_rows: u64,
    ) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let dirty_count = connection
            .query_row(
                "SELECT COUNT(*) FROM storage_dirty_path WHERE status = 'dirty'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count.max(0) as u64)
            .unwrap_or_default();
        if dirty_count <= max_dirty_rows {
            return;
        }
        let mut promoted_roots = Vec::new();
        for root in roots.iter().take(STORAGE_DIRTY_QUEUE_BACKPRESSURE_ROOT_CAP) {
            let root = root.display().to_string();
            if root.trim().is_empty() || !dirty_queue_root_has_dirty_descendant(connection, &root) {
                continue;
            }
            promoted_roots.push(root);
        }
        if promoted_roots.is_empty() {
            return;
        }
        for root in promoted_roots {
            let child_prefix = format!("{root}/");
            let _ = connection.execute(
                "INSERT INTO storage_dirty_path (
                    path, source, flags, last_event_id, first_seen_millis, last_seen_millis,
                    event_count, status, last_error
                 ) VALUES (?1, ?2, 0, ?3, ?4, ?4, 1, 'dirty', 'backpressure_root')
                 ON CONFLICT(path) DO UPDATE SET
                    source = excluded.source,
                    last_event_id = CASE
                        WHEN excluded.last_event_id IS NULL THEN storage_dirty_path.last_event_id
                        WHEN storage_dirty_path.last_event_id IS NULL THEN excluded.last_event_id
                        ELSE MAX(storage_dirty_path.last_event_id, excluded.last_event_id)
                    END,
                    last_seen_millis = MAX(storage_dirty_path.last_seen_millis, excluded.last_seen_millis),
                    event_count = storage_dirty_path.event_count + 1,
                    status = 'dirty',
                    last_error = 'backpressure_root'",
                params![
                    &root,
                    source,
                    latest_event_id.map(|value| value.min(i64::MAX as u64) as i64),
                    now_millis.min(i64::MAX as u64) as i64,
                ],
            );
            let _ = connection.execute(
                "UPDATE storage_dirty_path
                 SET status = 'clean',
                     last_error = 'backpressure_coalesced_to_root',
                     last_seen_millis = MAX(last_seen_millis, ?2)
                 WHERE status = 'dirty'
                   AND path <> ?1
                   AND substr(path, 1, ?3) = ?4",
                params![
                    &root,
                    now_millis.min(i64::MAX as u64) as i64,
                    child_prefix.len().min(i64::MAX as usize) as i64,
                    child_prefix,
                ],
            );
        }
    }

    pub(super) fn source_root_for_incremental_path(
        &self,
        path: &Path,
        requested_roots: &[PathBuf],
    ) -> PathBuf {
        let path_display = path.display().to_string();
        if let Some(connection) = self.connection.as_ref()
            && let Some(source_root) = indexed_source_root_for_path(connection, &path_display)
        {
            return PathBuf::from(source_root);
        }
        requested_roots
            .iter()
            .filter(|root| path_is_under_root(&path_display, root))
            .max_by_key(|root| root.display().to_string().len())
            .cloned()
            .unwrap_or_else(|| path.to_path_buf())
    }

    pub(super) fn indexed_source_roots_for_subtree(
        &self,
        path: &Path,
        requested_roots: &[PathBuf],
    ) -> BTreeSet<String> {
        let mut source_roots = BTreeSet::new();
        let path_display = path.display().to_string();
        if let Some(connection) = self.connection.as_ref() {
            load_indexed_source_roots_for_subtree(connection, &path_display, &mut source_roots);
        }
        if source_roots.is_empty() {
            source_roots.insert(
                self.source_root_for_incremental_path(path, requested_roots)
                    .display()
                    .to_string(),
            );
        }
        source_roots
    }

    pub(super) fn remove_indexed_subtree(
        &self,
        path: &Path,
        requested_roots: &[PathBuf],
        now_millis: u64,
    ) -> BTreeSet<String> {
        self.flush_pending_rows();
        let source_roots = self.indexed_source_roots_for_subtree(path, requested_roots);
        let Some(connection) = self.connection.as_ref() else {
            return source_roots;
        };
        let path_display = path.display().to_string();
        let child_prefix = format!("{path_display}/");
        let Ok(transaction) = connection.unchecked_transaction() else {
            return source_roots;
        };
        let params = params![
            &path_display,
            child_prefix.len().min(i64::MAX as usize) as i64,
            &child_prefix,
        ];
        let _ = transaction.execute(
            "DELETE FROM storage_file_index
             WHERE path = ?1 OR substr(path, 1, ?2) = ?3",
            params,
        );
        let params = params![
            &path_display,
            child_prefix.len().min(i64::MAX as usize) as i64,
            &child_prefix,
        ];
        let _ = transaction.execute(
            "DELETE FROM storage_size_index
             WHERE path = ?1 OR substr(path, 1, ?2) = ?3",
            params,
        );
        let params = params![
            &path_display,
            child_prefix.len().min(i64::MAX as usize) as i64,
            &child_prefix,
        ];
        let _ = transaction.execute(
            "DELETE FROM storage_path_fingerprint
             WHERE path = ?1 OR substr(path, 1, ?2) = ?3",
            params,
        );
        let params = params![
            &path_display,
            child_prefix.len().min(i64::MAX as usize) as i64,
            &child_prefix,
        ];
        let _ = transaction.execute(
            "DELETE FROM storage_path
             WHERE path = ?1 OR substr(path, 1, ?2) = ?3",
            params,
        );
        let params = params![
            &path_display,
            child_prefix.len().min(i64::MAX as usize) as i64,
            &child_prefix,
        ];
        let _ = transaction.execute(
            "DELETE FROM storage_directory_rollup
             WHERE path = ?1 OR substr(path, 1, ?2) = ?3",
            params,
        );
        refresh_storage_index_summaries_and_top_offenders(&transaction, &source_roots, now_millis);
        let _ = refresh_materialized_storage_index_for_roots(&transaction, &source_roots);
        if let Ok(generation) = materialized_storage_index_generation(&transaction) {
            let _ = set_materialized_storage_index_generation(&transaction, &generation);
        }
        let _ = transaction.commit();
        super::report::invalidate_index_report_sections_memo();
        source_roots
    }

    pub(super) fn refresh_materialized_storage_for_source_roots(
        &self,
        source_roots: &BTreeSet<String>,
    ) {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        if source_roots.is_empty() {
            return;
        }
        let Ok(transaction) = connection.unchecked_transaction() else {
            return;
        };
        let _ = refresh_materialized_storage_index_for_roots(&transaction, source_roots);
        if let Ok(generation) = materialized_storage_index_generation(&transaction) {
            let _ = set_materialized_storage_index_generation(&transaction, &generation);
        }
        let _ = transaction.commit();
        super::report::invalidate_index_report_sections_memo();
    }

    pub(super) fn record_incremental_measurement_job(
        &self,
        roots: &[PathBuf],
        dirty_paths: &[String],
        result: &StorageIncrementalMeasurementResult,
        now_millis: u64,
    ) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let roots_json = serde_json::to_string(
            &roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".to_owned());
        let dirty_paths_json =
            serde_json::to_string(dirty_paths).unwrap_or_else(|_| "[]".to_owned());
        let root_key = storage_situation_roots_key(roots);
        let status = if result.continuation_pending {
            "pending"
        } else if result.partial {
            "partial"
        } else {
            "complete"
        };
        let last_error = if result.continuation_pending {
            result
                .last_error
                .as_deref()
                .map(|error| format!("{error};continuation_pending"))
                .or_else(|| Some("continuation_pending".to_owned()))
        } else {
            result.last_error.clone()
        };
        let _ = connection.execute(
            "INSERT OR REPLACE INTO storage_measurement_job (
                job_id, job_kind, status, source, root_key, roots_json, dirty_paths_json,
                started_at_millis, updated_at_millis, completed_at_millis, measured_path_count,
                measured_directory_count, measured_file_count, measured_bytes, partial, last_error
             ) VALUES (
                ?1, 'dirty_subtree_incremental', ?2, 'dirty_queue', ?3, ?4, ?5,
                ?6, ?7, ?7, ?8, ?9, ?10, ?11, ?12, ?13
             )",
            params![
                format!("dirty-subtree-incremental:{root_key}"),
                status,
                root_key,
                roots_json,
                dirty_paths_json,
                result.started_at_millis.min(i64::MAX as u64) as i64,
                now_millis.min(i64::MAX as u64) as i64,
                result.measured_path_count.min(i64::MAX as u64) as i64,
                result.measured_directory_count.min(i64::MAX as u64) as i64,
                result.measured_file_count.min(i64::MAX as u64) as i64,
                result.measured_bytes.min(i64::MAX as u64) as i64,
                if result.partial { 1i64 } else { 0i64 },
                last_error.as_deref(),
            ],
        );
    }

    pub(super) fn latest_measurement_job_debug(
        &self,
        roots: &[PathBuf],
    ) -> StoragePipelineMeasurementDebug {
        let Some(connection) = self.connection.as_ref() else {
            return StoragePipelineMeasurementDebug::default();
        };
        let root_key = storage_situation_roots_key(roots);
        let Ok(mut statement) = connection.prepare(
            "SELECT status, updated_at_millis, dirty_paths_json, measured_path_count,
                    measured_file_count, partial, last_error
             FROM storage_measurement_job
             WHERE root_key = ?1
             ORDER BY updated_at_millis DESC
             LIMIT 1",
        ) else {
            return StoragePipelineMeasurementDebug::default();
        };
        statement
            .query_row(params![root_key], |row| {
                let status: String = row.get(0)?;
                let updated_at_millis: i64 = row.get(1)?;
                let dirty_paths_json: String = row.get(2)?;
                let measured_path_count: i64 = row.get(3)?;
                let measured_file_count: i64 = row.get(4)?;
                let partial: i64 = row.get(5)?;
                let last_error: Option<String> = row.get(6)?;
                let dirty_path_count = serde_json::from_str::<Vec<String>>(&dirty_paths_json)
                    .map(|paths| paths.len().min(u64::MAX as usize) as u64)
                    .unwrap_or_default();
                Ok(StoragePipelineMeasurementDebug {
                    latest_status: Some(status),
                    latest_updated_millis: Some(updated_at_millis.max(0) as u64),
                    latest_dirty_path_count: dirty_path_count,
                    latest_measured_path_count: measured_path_count.max(0) as u64,
                    latest_measured_file_count: measured_file_count.max(0) as u64,
                    latest_partial: partial != 0,
                    latest_error: last_error,
                })
            })
            .unwrap_or_default()
    }

    fn mark_unknown_gap_roots(
        &self,
        roots: &BTreeSet<String>,
        now_millis: u64,
        source: &str,
        reason: &str,
        last_event_id: Option<u64>,
    ) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        if roots.is_empty() {
            return;
        }
        let Ok(mut statement) = connection.prepare(
            "INSERT INTO storage_unknown_gap (
                root_path, source, reason, first_seen_millis, last_seen_millis,
                last_event_id, unresolved
             ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, 1)
             ON CONFLICT(root_path) DO UPDATE SET
                source = excluded.source,
                reason = excluded.reason,
                last_seen_millis = excluded.last_seen_millis,
                last_event_id = COALESCE(excluded.last_event_id, storage_unknown_gap.last_event_id),
                unresolved = 1",
        ) else {
            return;
        };
        for root in roots {
            let _ = statement.execute(params![
                root,
                source,
                reason,
                now_millis.min(i64::MAX as u64) as i64,
                last_event_id.map(|value| value.min(i64::MAX as u64) as i64),
            ]);
        }
        super::report::invalidate_index_report_sections_memo();
    }

    fn load_unknown_gap_roots(&self, roots: &[PathBuf], limit: usize) -> Vec<String> {
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let Ok(mut statement) = connection.prepare(
            "SELECT root_path
             FROM storage_unknown_gap
             WHERE unresolved <> 0
             ORDER BY last_seen_millis DESC, root_path ASC
             LIMIT ?1",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(
            params![limit.saturating_mul(4).clamp(1, 512) as i64],
            |row| row.get::<_, String>(0),
        ) else {
            return Vec::new();
        };
        let mut unknown_gap_roots = Vec::new();
        for root in rows.flatten() {
            if !roots.is_empty()
                && !roots.iter().any(|requested_root| {
                    path_is_under_root(&root, requested_root)
                        || path_is_under_root(
                            &requested_root.display().to_string(),
                            Path::new(root.as_str()),
                        )
                })
            {
                continue;
            }
            unknown_gap_roots.push(root);
            if unknown_gap_roots.len() >= limit {
                break;
            }
        }
        unknown_gap_roots
    }

    fn store_path_fingerprint(
        &self,
        path: &str,
        fingerprint: Vec<u8>,
        measured_at_millis: u64,
        source: &str,
    ) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let _ = connection.execute(
            "INSERT INTO storage_path_fingerprint (
                path, fingerprint, source, measured_at_millis
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET
                fingerprint = excluded.fingerprint,
                source = excluded.source,
                measured_at_millis = excluded.measured_at_millis",
            params![
                path,
                fingerprint,
                source,
                measured_at_millis.min(i64::MAX as u64) as i64,
            ],
        );
    }

    fn update_event_cursor(
        &self,
        source: &str,
        last_event_id: Option<u64>,
        updated_at_millis: u64,
        status: &str,
        detail: &str,
    ) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let _ = connection.execute(
            "INSERT INTO storage_event_cursor (
                source, last_event_id, updated_at_millis, status, detail
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source) DO UPDATE SET
                last_event_id = CASE
                    WHEN excluded.last_event_id IS NULL THEN storage_event_cursor.last_event_id
                    WHEN storage_event_cursor.last_event_id IS NULL THEN excluded.last_event_id
                    ELSE MAX(storage_event_cursor.last_event_id, excluded.last_event_id)
                END,
                updated_at_millis = excluded.updated_at_millis,
                status = excluded.status,
                detail = excluded.detail",
            params![
                source,
                last_event_id.map(|value| value.min(i64::MAX as u64) as i64),
                updated_at_millis.min(i64::MAX as u64) as i64,
                status,
                detail,
            ],
        );
    }

    fn event_cursor(&self, source: &str) -> Option<u64> {
        let connection = self.connection.as_ref()?;
        connection
            .query_row(
                "SELECT last_event_id
                 FROM storage_event_cursor
                 WHERE source = ?1",
                params![source],
                |row| {
                    row.get::<_, Option<i64>>(0)
                        .map(|value| value.map(|value| value.max(0) as u64))
                },
            )
            .ok()
            .flatten()
    }

    fn last_event_id_for_path(&self, path: &str) -> u64 {
        let Some(connection) = self.connection.as_ref() else {
            return 0;
        };
        let child_prefix = format!("{}/%", escape_like_pattern(path));
        connection
            .query_row(
                "SELECT COALESCE(MAX(last_event_id), 0)
                 FROM storage_dirty_path
                 WHERE path = ?1 OR path LIKE ?2 ESCAPE '\\'",
                params![path, child_prefix],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value.max(0) as u64)
            .unwrap_or_default()
    }

    /// Buffer one indexed row; rows are written in chunked transactions by
    /// `flush_pending_rows` (previous-values lookup + upserts + growth deltas)
    /// instead of one SELECT and one autocommit INSERT per file.
    pub(super) fn store_indexed_row(
        &self,
        row: &StorageIndexedFileRow,
        metrics: &mut StorageScanMetrics,
    ) {
        if self.connection.is_none() {
            return;
        }
        metrics.storage_index_writes = metrics.storage_index_writes.saturating_add(1);
        let chunk_full = {
            let mut pending = self.pending_rows.borrow_mut();
            pending.push(row.clone());
            pending.len() >= STORAGE_INDEX_FLUSH_CHUNK
        };
        if chunk_full {
            self.flush_pending_rows();
        }
    }

    /// Write all buffered rows in one transaction. Per-row semantics match the
    /// old autocommit path exactly: a growth delta is recorded only when the
    /// physical byte count changed (new rows compare against zero), and a row
    /// stored twice in one chunk compares against the earlier occurrence. Raw
    /// growth deltas are compacted into rollups and top-offender summaries once
    /// per flush instead of retaining every path indefinitely. Failures are
    /// tolerated (best effort), matching the old `.is_ok()` behavior.
    pub(super) fn flush_pending_rows(&self) {
        let rows = std::mem::take(&mut *self.pending_rows.borrow_mut());
        if rows.is_empty() {
            return;
        }
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        // Batched previous-values lookup, chunked to stay well under SQLite's
        // bind-variable limit. Also captures the previous cleanup tier so the
        // upsert can persist it into `previous_cleanup_tier`.
        let mut previous: BTreeMap<String, (u64, String)> = BTreeMap::new();
        let unique_paths: Vec<&String> = rows
            .iter()
            .map(|row| &row.path)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        for chunk in unique_paths.chunks(STORAGE_INDEX_LOOKUP_BIND_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let Ok(mut statement) = connection.prepare(&format!(
                "SELECT path, physical_bytes, cleanup_tier
                 FROM storage_file_index
                 WHERE path IN ({placeholders})"
            )) else {
                continue;
            };
            let Ok(found) = statement.query_map(params_from_iter(chunk.iter()), |row| {
                let path: String = row.get(0)?;
                let physical_bytes: i64 = row.get(1)?;
                let cleanup_tier: String = row.get(2)?;
                Ok((path, (physical_bytes.max(0) as u64, cleanup_tier)))
            }) else {
                continue;
            };
            for (path, entry) in found.flatten() {
                previous.insert(path, entry);
            }
        }
        let Ok(transaction) = connection.unchecked_transaction() else {
            return;
        };
        let mut max_scan_millis = 0u64;
        let mut affected_source_roots = BTreeSet::new();
        {
            let Ok(mut upsert) = transaction.prepare(
                "INSERT OR REPLACE INTO storage_file_index (
                    path, device, inode, file_id, source_root, repo_root, kind, storage_role,
                    safety, cleanup_tier, logical_bytes, physical_bytes, modified_millis,
                    changed_millis, accessed_millis, birth_millis, is_directory, entries,
                    truncated, last_scan_millis, previous_cleanup_tier, recommendation_score
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                    ?17, ?18, ?19, ?20, ?21, ?22
                 )",
            ) else {
                return;
            };
            let Ok(mut insert_delta) = transaction.prepare(
                "INSERT INTO storage_growth_delta (
                    bucket_millis, scan_millis, path, source_root, repo_root, kind, cleanup_tier,
                    previous_physical_bytes, current_physical_bytes, delta_bytes
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            ) else {
                return;
            };
            let Ok(mut upsert_rollup) = transaction.prepare(
                "INSERT INTO storage_growth_rollup (
                    granularity, bucket_millis, source_root, repo_root, kind, cleanup_tier,
                    total_delta_bytes, positive_delta_bytes, negative_delta_bytes,
                    changed_path_count, max_abs_delta_bytes, updated_at_millis
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11)
                 ON CONFLICT (
                    granularity, bucket_millis, source_root, repo_root, kind, cleanup_tier
                 ) DO UPDATE SET
                    total_delta_bytes = total_delta_bytes + excluded.total_delta_bytes,
                    positive_delta_bytes = positive_delta_bytes + excluded.positive_delta_bytes,
                    negative_delta_bytes = negative_delta_bytes + excluded.negative_delta_bytes,
                    changed_path_count = changed_path_count + excluded.changed_path_count,
                    max_abs_delta_bytes = MAX(max_abs_delta_bytes, excluded.max_abs_delta_bytes),
                    updated_at_millis = MAX(updated_at_millis, excluded.updated_at_millis)",
            ) else {
                return;
            };
            for row in &rows {
                affected_source_roots.insert(row.source_root.clone());
                let previous_entry = previous.get(&row.path);
                let previous_physical = previous_entry.map(|(bytes, _)| *bytes);
                let previous_tier = previous_entry
                    .map(|(_, tier)| tier.clone())
                    .unwrap_or_default();
                if upsert
                    .execute(params![
                        &row.path,
                        row.device,
                        row.inode,
                        &row.file_id,
                        &row.source_root,
                        row.repo_root.as_deref(),
                        &row.kind,
                        &row.storage_role,
                        &row.safety,
                        &row.cleanup_tier,
                        row.logical_bytes.min(i64::MAX as u64) as i64,
                        row.physical_bytes.min(i64::MAX as u64) as i64,
                        row.modified_millis
                            .map(|value| value.min(i64::MAX as u64) as i64),
                        row.changed_millis
                            .map(|value| value.min(i64::MAX as u64) as i64),
                        row.accessed_millis
                            .map(|value| value.min(i64::MAX as u64) as i64),
                        row.birth_millis
                            .map(|value| value.min(i64::MAX as u64) as i64),
                        if row.is_directory { 1i64 } else { 0i64 },
                        row.entries.min(i64::MAX as u64) as i64,
                        if row.truncated { 1i64 } else { 0i64 },
                        row.last_scan_millis.min(i64::MAX as u64) as i64,
                        previous_tier,
                        storage_recommendation_score(
                            row.physical_bytes,
                            &row.cleanup_tier,
                            row.modified_millis,
                            row.accessed_millis,
                            row.last_scan_millis,
                        ),
                    ])
                    .is_err()
                {
                    continue;
                }
                max_scan_millis = max_scan_millis.max(row.last_scan_millis);
                let previous_physical = previous_physical.unwrap_or(0);
                if previous_physical != row.physical_bytes {
                    let delta = row.physical_bytes as i128 - previous_physical as i128;
                    let delta = delta.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
                    if delta != 0 {
                        let bucket_millis = (row.last_scan_millis / STORAGE_GROWTH_BUCKET_MILLIS)
                            * STORAGE_GROWTH_BUCKET_MILLIS;
                        let _ = insert_delta.execute(params![
                            bucket_millis.min(i64::MAX as u64) as i64,
                            row.last_scan_millis.min(i64::MAX as u64) as i64,
                            &row.path,
                            &row.source_root,
                            row.repo_root.as_deref(),
                            &row.kind,
                            &row.cleanup_tier,
                            previous_physical.min(i64::MAX as u64) as i64,
                            row.physical_bytes.min(i64::MAX as u64) as i64,
                            delta,
                        ]);
                        upsert_growth_rollups(&mut upsert_rollup, row, bucket_millis, delta);
                    }
                }
                previous.insert(
                    row.path.clone(),
                    (row.physical_bytes, row.cleanup_tier.clone()),
                );
            }
            if max_scan_millis > 0 {
                prune_storage_growth_history(
                    &transaction,
                    max_scan_millis,
                    StorageIndexBudgetLimits::default(),
                );
                refresh_storage_index_summaries_and_top_offenders(
                    &transaction,
                    &affected_source_roots,
                    max_scan_millis,
                );
            }
        }
        let _ = transaction.commit();
        let should_enforce_budget = {
            let mut count = self.budget_flush_count.borrow_mut();
            *count = count.saturating_add(1);
            (*count).is_multiple_of(STORAGE_INDEX_BUDGET_CHECK_FLUSH_INTERVAL)
                || rows.len() < STORAGE_INDEX_FLUSH_CHUNK
        };
        if should_enforce_budget {
            self.enforce_storage_index_budget(StorageIndexBudgetLimits::default());
        }
        // The index content changed, so memoized report sections are stale.
        // The generation key usually changes too; this covers the
        // same-millisecond and unchanged-stamp cases.
        super::report::invalidate_index_report_sections_memo();
    }

    fn enforce_storage_index_budget(&self, limits: StorageIndexBudgetLimits) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };

        let mut changed = false;
        changed |= prune_storage_file_index_rows(connection, limits.max_file_rows, u64::MAX);
        changed |= prune_storage_size_index_rows(connection, limits.max_size_rows, u64::MAX);
        changed |= prune_storage_growth_delta_rows(
            connection,
            limits.max_growth_delta_rows,
            STORAGE_INDEX_EMERGENCY_EVICT_ROWS,
        );

        let mut bytes = self.index_total_file_bytes();
        if bytes <= limits.target_bytes && !changed {
            return;
        }

        if changed {
            let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            bytes = self.index_total_file_bytes();
        }

        let mut emergency_changed = false;
        for _ in 0..32 {
            if bytes <= limits.hard_cap_bytes {
                break;
            }
            let mut deleted = false;
            deleted |=
                evict_storage_file_index_rows(connection, STORAGE_INDEX_EMERGENCY_EVICT_ROWS);
            deleted |=
                evict_storage_size_index_rows(connection, STORAGE_INDEX_EMERGENCY_EVICT_ROWS);
            deleted |=
                evict_storage_growth_delta_rows(connection, STORAGE_INDEX_EMERGENCY_EVICT_ROWS);
            if !deleted {
                break;
            }
            emergency_changed = true;
            let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            bytes = self.index_total_file_bytes();
        }

        if emergency_changed || self.index_total_file_bytes() > limits.hard_cap_bytes {
            let _ = connection.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);");
        }
        if changed || emergency_changed {
            super::report::invalidate_index_report_sections_memo();
        }
    }

    fn index_total_file_bytes(&self) -> u64 {
        let Some(path) = self.path.as_ref() else {
            return 0;
        };
        [
            path.clone(),
            sqlite_sidecar_path(path, "-wal"),
            sqlite_sidecar_path(path, "-shm"),
        ]
        .iter()
        .filter_map(|path| fs::metadata(path).ok())
        .fold(0u64, |total, metadata| total.saturating_add(metadata.len()))
    }

    #[cfg(test)]
    pub(super) fn enforce_storage_index_budget_for_test(
        &self,
        max_file_rows: u64,
        max_size_rows: u64,
        max_growth_delta_rows: u64,
    ) {
        self.flush_pending_rows();
        self.enforce_storage_index_budget(StorageIndexBudgetLimits {
            target_bytes: u64::MAX,
            hard_cap_bytes: u64::MAX,
            max_file_rows,
            max_size_rows,
            max_growth_delta_rows,
        });
    }

    /// Cheap (indexed MAX lookups) fingerprint of the index content used to
    /// key the per-generation report-section memo: the latest file-index scan
    /// stamp plus the latest growth-delta scan stamp. Any scan flush bumps at
    /// least one of them. In-process writes that may not move either stamp
    /// (row deletion, repository-cache refreshes, same-millisecond flushes)
    /// invalidate the memo explicitly instead.
    pub(super) fn index_report_generation(&self) -> Option<(u64, u64)> {
        let connection = self.connection.as_ref()?;
        let file_generation: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(last_scan_millis), 0) FROM storage_file_index",
                [],
                |row| row.get(0),
            )
            .ok()?;
        let delta_generation: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(scan_millis), 0) FROM storage_growth_delta",
                [],
                |row| row.get(0),
            )
            .ok()?;
        Some((
            file_generation.max(0) as u64,
            delta_generation.max(0) as u64,
        ))
    }

    pub(super) fn load_candidate_rows(
        &self,
        roots: &[PathBuf],
        limit: usize,
        metrics: &mut StorageScanMetrics,
    ) -> Result<Vec<StorageIndexedFileRow>, String> {
        self.flush_pending_rows();
        let read_limit = limit
            .saturating_mul(STORAGE_INDEX_SNAPSHOT_READ_MULTIPLIER)
            .clamp(1, 5_000);
        let mut retained = Vec::new();
        for _ in 0..STORAGE_INDEX_STALE_EVICTION_MAX_PASSES {
            let mut stale_paths = Vec::new();
            retained =
                self.query_candidate_rows_once(roots, limit, read_limit, &mut stale_paths)?;
            if stale_paths.is_empty() {
                break;
            }
            self.delete_indexed_rows(&stale_paths)?;
            if retained.len() >= limit {
                break;
            }
        }
        metrics.storage_index_hits = metrics
            .storage_index_hits
            .saturating_add(retained.len().min(u64::MAX as usize) as u64);
        if retained.is_empty() {
            metrics.storage_index_misses = metrics.storage_index_misses.saturating_add(1);
        }
        Ok(retained)
    }

    fn query_candidate_rows_once(
        &self,
        roots: &[PathBuf],
        limit: usize,
        read_limit: usize,
        stale_paths: &mut Vec<String>,
    ) -> Result<Vec<StorageIndexedFileRow>, String> {
        let Some(connection) = self.connection.as_ref() else {
            return Err(self.status.clone());
        };
        let mut predicate = "(cleanup_tier <> ''
                OR (kind = 'large-directory' AND physical_bytes >= ?))
             AND physical_bytes >= ?"
            .to_owned();
        let mut bindings: Vec<rusqlite::types::Value> = vec![
            (LARGE_DIRECTORY_MIN_BYTES.min(i64::MAX as u64) as i64).into(),
            (MIN_ITEM_BYTES.min(i64::MAX as u64) as i64).into(),
        ];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        let mut statement = connection
            .prepare(&format!(
                "SELECT path, device, inode, file_id, source_root, repo_root, kind,
                        storage_role, safety, cleanup_tier, logical_bytes, physical_bytes,
                        modified_millis, changed_millis, accessed_millis, birth_millis,
                        is_directory, entries, truncated, last_scan_millis
                 FROM storage_file_index
                 WHERE {predicate}
                 ORDER BY physical_bytes DESC, path ASC
                 LIMIT ?",
            ))
            .map_err(|error| error.to_string())?;
        bindings.push((read_limit.min(i64::MAX as usize) as i64).into());
        let rows = statement
            .query_map(params_from_iter(bindings.iter()), indexed_file_row_from_sql)
            .map_err(|error| error.to_string())?;
        let mut retained = Vec::with_capacity(limit.min(read_limit));
        for row in rows.flatten() {
            if !indexed_row_matches_live_metadata(&row) {
                stale_paths.push(row.path.clone());
                continue;
            }
            retained.push(row);
            if retained.len() >= limit {
                break;
            }
        }
        Ok(retained)
    }

    /// Serve one page of cleanup-classified rows directly from
    /// `storage_file_index`, mirroring the candidate predicate used by
    /// `load_candidate_rows` (`cleanup_tier <> ''` or a review-only
    /// `large-directory` row at the large-directory threshold, plus the
    /// minimum item size)
    /// and the ordering semantics of `sort_storage_items`. Rows whose paths no
    /// longer match live metadata are evicted from the index and the page is
    /// refilled until the visible rows are clean or the repair pass budget is
    /// exhausted.
    pub(super) fn load_item_rows_page(
        &self,
        roots: &[PathBuf],
        sort_key: StorageItemSortKey,
        sort_descending: bool,
        offset: usize,
        limit: usize,
        metrics: &mut StorageScanMetrics,
    ) -> Result<StorageItemRowsPage, String> {
        self.flush_pending_rows();
        let mut page =
            self.query_item_rows_page(roots, sort_key, sort_descending, offset, limit)?;
        for _ in 0..STORAGE_INDEX_STALE_EVICTION_MAX_PASSES {
            let stale = page
                .rows
                .iter()
                .filter(|row| !indexed_row_matches_live_metadata(row))
                .map(|row| row.path.clone())
                .collect::<Vec<_>>();
            if stale.is_empty() {
                break;
            }
            self.delete_indexed_rows(&stale)?;
            page = self.query_item_rows_page(roots, sort_key, sort_descending, offset, limit)?;
        }
        page.rows.retain(indexed_row_matches_live_metadata);
        if page.rows.is_empty() {
            metrics.storage_index_misses = metrics.storage_index_misses.saturating_add(1);
        } else {
            metrics.storage_index_hits = metrics
                .storage_index_hits
                .saturating_add(page.rows.len().min(u64::MAX as usize) as u64);
        }
        Ok(page)
    }

    fn query_item_rows_page(
        &self,
        roots: &[PathBuf],
        sort_key: StorageItemSortKey,
        sort_descending: bool,
        offset: usize,
        limit: usize,
    ) -> Result<StorageItemRowsPage, String> {
        let Some(connection) = self.connection.as_ref() else {
            return Err(self.status.clone());
        };
        let mut predicate = "(cleanup_tier <> ''
                OR (kind = 'large-directory' AND physical_bytes >= ?))
             AND physical_bytes >= ?"
            .to_owned();
        let mut bindings: Vec<rusqlite::types::Value> = vec![
            (LARGE_DIRECTORY_MIN_BYTES.min(i64::MAX as u64) as i64).into(),
            (MIN_ITEM_BYTES.min(i64::MAX as u64) as i64).into(),
        ];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        let total_available: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM storage_file_index WHERE {predicate}"),
                params_from_iter(bindings.iter()),
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let direction = if sort_descending { "DESC" } else { "ASC" };
        let order_by = match sort_key {
            StorageItemSortKey::Size => format!(
                "CASE WHEN physical_bytes > 0 THEN physical_bytes ELSE logical_bytes END {direction}, path ASC"
            ),
            StorageItemSortKey::Path => format!("path {direction}"),
            StorageItemSortKey::Modified => {
                format!("COALESCE(modified_millis, 0) {direction}, path ASC")
            }
            StorageItemSortKey::Accessed => {
                format!("COALESCE(accessed_millis, 0) {direction}, path ASC")
            }
            StorageItemSortKey::Tier => {
                format!("cleanup_tier {direction}, safety {direction}, path ASC")
            }
            StorageItemSortKey::Kind => format!("kind {direction}, path ASC"),
            StorageItemSortKey::Score => {
                format!("recommendation_score {direction}, path ASC")
            }
        };
        let mut statement = connection
            .prepare(&format!(
                "SELECT path, device, inode, file_id, source_root, repo_root, kind,
                        storage_role, safety, cleanup_tier, logical_bytes, physical_bytes,
                        modified_millis, changed_millis, accessed_millis, birth_millis,
                        is_directory, entries, truncated, last_scan_millis
                 FROM storage_file_index
                 WHERE {predicate}
                 ORDER BY {order_by}
                 LIMIT ? OFFSET ?"
            ))
            .map_err(|error| error.to_string())?;
        bindings.push((limit.min(i64::MAX as usize) as i64).into());
        bindings.push((offset.min(i64::MAX as usize) as i64).into());
        let rows = statement
            .query_map(params_from_iter(bindings.iter()), indexed_file_row_from_sql)
            .map_err(|error| error.to_string())?
            .flatten()
            .collect::<Vec<_>>();
        Ok(StorageItemRowsPage {
            rows,
            total_available: total_available.max(0) as u64,
        })
    }

    fn delete_indexed_rows(&self, paths: &[String]) -> Result<(), String> {
        let Some(connection) = self.connection.as_ref() else {
            return Err(self.status.clone());
        };
        if paths.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; paths.len()].join(", ");
        connection
            .execute(
                &format!("DELETE FROM storage_file_index WHERE path IN ({placeholders})"),
                params_from_iter(paths.iter()),
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                &format!("DELETE FROM storage_size_index WHERE path IN ({placeholders})"),
                params_from_iter(paths.iter()),
            )
            .map_err(|error| error.to_string())?;
        // Deletions do not move the generation stamps, so drop memoized
        // report sections explicitly.
        super::report::invalidate_index_report_sections_memo();
        Ok(())
    }

    pub(super) fn load_repository_inventory_cache(
        &self,
        roots: &[PathBuf],
    ) -> BTreeMap<String, RepositoryInventoryCacheEntry> {
        let Some(connection) = self.connection.as_ref() else {
            return BTreeMap::new();
        };
        let Ok(mut statement) = connection.prepare(
            "SELECT repo_root, discovered_root, repository_fingerprint,
                    last_seen_millis, last_scan_millis
             FROM storage_repository_inventory_cache
             ORDER BY last_seen_millis DESC, repo_root ASC",
        ) else {
            return BTreeMap::new();
        };
        let Ok(rows) = statement.query_map([], |row| {
            let repo_root: String = row.get(0)?;
            let entry = RepositoryInventoryCacheEntry {
                discovered_root: row.get(1)?,
                repository_fingerprint: row.get(2)?,
                last_seen_millis: row.get::<_, i64>(3)?.max(0) as u64,
                last_scan_millis: row.get::<_, i64>(4)?.max(0) as u64,
            };
            Ok((repo_root, entry))
        }) else {
            return BTreeMap::new();
        };

        let mut repositories = BTreeMap::new();
        for row in rows.flatten() {
            let (repo_root, entry) = row;
            if roots.is_empty()
                || roots
                    .iter()
                    .any(|root| path_is_under_root(&repo_root, root))
            {
                repositories.insert(repo_root, entry);
            }
        }
        repositories
    }

    pub(super) fn store_repository_inventory_cache(
        &self,
        repositories_by_root: &BTreeMap<String, String>,
        now_millis: u64,
        metrics: &mut StorageScanMetrics,
    ) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        for (repo_root, discovered_root) in repositories_by_root {
            let repo_path = Path::new(repo_root);
            let git_config_fingerprint = repository_git_file_fingerprint(repo_path, "config");
            let git_index_fingerprint = repository_git_file_fingerprint(repo_path, "index");
            let repository_fingerprint = repository_inventory_fingerprint(repo_path);
            if connection
                .execute(
                    "INSERT INTO storage_repository_inventory_cache (
                        repo_root, discovered_root, git_config_fingerprint, git_index_fingerprint,
                        repository_fingerprint, first_seen_millis, last_seen_millis, last_scan_millis
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?6)
                     ON CONFLICT(repo_root) DO UPDATE SET
                        discovered_root = excluded.discovered_root,
                        git_config_fingerprint = excluded.git_config_fingerprint,
                        git_index_fingerprint = excluded.git_index_fingerprint,
                        repository_fingerprint = excluded.repository_fingerprint,
                        last_seen_millis = excluded.last_seen_millis,
                        last_scan_millis = excluded.last_scan_millis",
                    params![
                        repo_root,
                        discovered_root,
                        git_config_fingerprint,
                        git_index_fingerprint,
                        repository_fingerprint,
                        now_millis.min(i64::MAX as u64) as i64,
                    ],
                )
                .is_ok()
            {
                metrics.storage_index_writes = metrics.storage_index_writes.saturating_add(1);
            }
        }
        if !repositories_by_root.is_empty() {
            // Repository-cache refreshes do not move the generation stamps,
            // so drop memoized report sections explicitly.
            super::report::invalidate_index_report_sections_memo();
        }
    }

    pub(super) fn load_growth_deltas(&self, limit: usize) -> Vec<StorageGrowthDelta> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let writer_ledger = load_storage_writer_ledger_records();
        let filesystem_events = load_storage_filesystem_event_records();
        let Ok(mut statement) = connection.prepare(
            "SELECT bucket_millis, scan_millis, path, source_root, repo_root, kind, cleanup_tier,
                    previous_physical_bytes, current_physical_bytes, delta_bytes
             FROM storage_growth_delta
             ORDER BY bucket_millis DESC, delta_bytes DESC
             LIMIT ?1",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(params![limit.min(200) as i64], |row| {
            let bucket_millis: i64 = row.get(0)?;
            let scan_millis: i64 = row.get(1)?;
            let previous_physical_bytes: i64 = row.get(7)?;
            let current_physical_bytes: i64 = row.get(8)?;
            Ok(StorageGrowthDelta {
                bucket_millis: bucket_millis.max(0) as u64,
                scan_millis: scan_millis.max(0) as u64,
                path: row.get(2)?,
                source_root: row.get(3)?,
                repo_root: row.get(4)?,
                repo_name: None,
                git_branch: None,
                git_head: None,
                kind: row.get(5)?,
                cleanup_tier: row.get(6)?,
                previous_physical_bytes: previous_physical_bytes.max(0) as u64,
                current_physical_bytes: current_physical_bytes.max(0) as u64,
                delta_bytes: row.get(9)?,
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
            })
        }) else {
            return Vec::new();
        };
        rows.flatten()
            .map(|mut delta| {
                let attribution =
                    attribute_storage_growth_delta(&delta, &writer_ledger, &filesystem_events);
                delta.repo_name = attribution.repo_name;
                delta.git_branch = attribution.git_branch;
                delta.git_head = attribution.git_head;
                delta.command = attribution.command;
                delta.process_tree = attribution.process_tree;
                delta.ai_agent_session = attribution.ai_agent_session;
                delta.writer_source = attribution.writer_source;
                delta.provider = attribution.provider;
                delta.session_id = attribution.session_id;
                delta.tab_name = attribution.tab_name;
                delta.chau7_session_id = attribution.chau7_session_id;
                delta.writer_display = attribution.writer_display;
                delta.matched_writer_count = attribution.matched_writer_count;
                delta.matched_filesystem_event_count = attribution.matched_filesystem_event_count;
                delta.attribution_sources = attribution.sources;
                delta.attribution_confidence = attribution.confidence;
                delta.attribution_confidence_score = attribution.confidence_score;
                delta.attribution_ambiguous = attribution.ambiguous;
                delta.attribution_summary = attribution.summary;
                delta.attribution_evidence = attribution.evidence;
                delta
            })
            .collect()
    }

    /// Aggregate retained growth rollups into growth intelligence: per-repo and
    /// per-source-root daily rates with a half-window trend, days-to-disk-full
    /// forecasts per volume, and a "since last scan" diff lane from the recent
    /// raw-delta lane. Returns `None` when the index is unavailable. Scoped to
    /// `roots` (matching `path_is_under_root` semantics) so reports and tests
    /// stay isolated.
    pub(super) fn load_growth_insights(
        &self,
        roots: &[PathBuf],
        volume_states: &[StorageVolumeState],
        now_millis: u64,
        window_days: u64,
    ) -> Option<StorageGrowthInsights> {
        self.flush_pending_rows();
        let connection = self.connection.as_ref()?;
        let window_days = window_days.clamp(1, 365);
        let window_start = now_millis.saturating_sub(window_days.saturating_mul(DAY_MILLIS));
        let per_repo_rates =
            self.load_growth_rates(connection, roots, window_start, window_days, "repo_root");
        let per_root_rates =
            self.load_growth_rates(connection, roots, window_start, window_days, "source_root");
        let volume_forecasts =
            self.load_volume_forecasts(connection, roots, volume_states, window_start);
        let growth_anomalies =
            self.load_growth_anomalies(connection, roots, window_start, window_days);
        let since_last_scan = self.load_since_last_scan_diff(connection, roots);
        Some(StorageGrowthInsights {
            window_days,
            per_repo_rates,
            per_root_rates,
            volume_forecasts,
            growth_anomalies,
            since_last_scan,
        })
    }

    fn load_growth_rates(
        &self,
        connection: &Connection,
        roots: &[PathBuf],
        window_start: u64,
        window_days: u64,
        scope_column: &str,
    ) -> Vec<StorageGrowthRate> {
        let mut predicate = format!(
            "granularity = 'day' AND bucket_millis >= ? AND {scope_column} IS NOT NULL AND {scope_column} <> ''"
        );
        let mut bindings: Vec<rusqlite::types::Value> =
            vec![(window_start.min(i64::MAX as u64) as i64).into()];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "source_root");
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT {scope_column} AS scope,
                    SUM(total_delta_bytes) AS total_delta,
                    COUNT(DISTINCT bucket_millis / {DAY_MILLIS}) AS day_buckets,
                    MIN(bucket_millis) AS first_bucket,
                    MAX(bucket_millis) AS last_bucket
             FROM storage_growth_rollup
             WHERE {predicate}
             GROUP BY scope
             ORDER BY total_delta DESC, scope ASC
             LIMIT {STORAGE_GROWTH_RATE_SCOPE_LIMIT}"
        )) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(params_from_iter(bindings.iter()), |row| {
            let scope: String = row.get(0)?;
            let total_delta: i64 = row.get(1)?;
            let day_buckets: i64 = row.get(2)?;
            let first_bucket: i64 = row.get(3)?;
            let last_bucket: i64 = row.get(4)?;
            Ok((
                scope,
                total_delta,
                day_buckets.max(0) as u64,
                first_bucket.max(0) as u64,
                last_bucket.max(0) as u64,
            ))
        }) else {
            return Vec::new();
        };
        let scoped = rows.flatten().collect::<Vec<_>>();
        scoped
            .into_iter()
            .map(
                |(scope, total_delta, day_buckets, first_bucket, last_bucket)| {
                    // Bytes/day over the observed span inside the retained
                    // window (not the full window), so short histories do not
                    // understate the rate.
                    let span_days =
                        (last_bucket.saturating_sub(first_bucket) / DAY_MILLIS).saturating_add(1);
                    let daily_rate_bytes = total_delta / span_days.max(1) as i64;
                    let trend = if day_buckets < 2 {
                        "steady".to_owned()
                    } else {
                        let midpoint = first_bucket + (last_bucket - first_bucket) / 2;
                        let second_half_delta = self.scope_delta_since(
                            connection,
                            roots,
                            window_start,
                            scope_column,
                            &scope,
                            midpoint,
                        );
                        growth_trend(total_delta, second_half_delta)
                    };
                    let daily_stats =
                        growth_forecast_stats_from_daily_totals(&self.load_daily_growth_totals(
                            connection,
                            roots,
                            window_start,
                            Some((scope_column, &scope)),
                            None,
                        ));
                    let repo_name = Path::new(&scope)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_owned);
                    StorageGrowthRate {
                        scope,
                        scope_kind: if scope_column == "repo_root" {
                            "repo".to_owned()
                        } else {
                            "source_root".to_owned()
                        },
                        repo_name,
                        window_days,
                        total_delta_bytes: total_delta,
                        daily_rate_bytes,
                        daily_rate_lower_bytes: daily_stats.daily_rate_lower_bytes,
                        daily_rate_upper_bytes: daily_stats.daily_rate_upper_bytes,
                        trend,
                        confidence: daily_stats.confidence,
                        volatility_percent: daily_stats.volatility_percent,
                        seasonal_pattern: daily_stats.seasonal_pattern,
                        seasonal_peak_daily_bytes: daily_stats.seasonal_peak_daily_bytes,
                        day_bucket_count: day_buckets,
                    }
                },
            )
            .collect()
    }

    /// Sum of deltas strictly after `midpoint_millis` for one scope; feeds the
    /// half-window trend comparison.
    fn scope_delta_since(
        &self,
        connection: &Connection,
        roots: &[PathBuf],
        window_start: u64,
        scope_column: &str,
        scope: &str,
        midpoint_millis: u64,
    ) -> i64 {
        let mut predicate = format!(
            "granularity = 'day' AND bucket_millis >= ? AND bucket_millis > ? AND {scope_column} = ?"
        );
        let mut bindings: Vec<rusqlite::types::Value> = vec![
            (window_start.min(i64::MAX as u64) as i64).into(),
            (midpoint_millis.min(i64::MAX as u64) as i64).into(),
            scope.to_owned().into(),
        ];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "source_root");
        connection
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(total_delta_bytes), 0)
                     FROM storage_growth_rollup
                     WHERE {predicate}"
                ),
                params_from_iter(bindings.iter()),
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
    }

    fn load_volume_forecasts(
        &self,
        connection: &Connection,
        roots: &[PathBuf],
        volume_states: &[StorageVolumeState],
        window_start: u64,
    ) -> Vec<StorageGrowthForecast> {
        volume_states
            .iter()
            .filter_map(|volume| {
                let daily_totals = self.load_daily_growth_totals(
                    connection,
                    roots,
                    window_start,
                    None,
                    Some(&volume.path),
                );
                let stats = growth_forecast_stats_from_daily_totals(&daily_totals);
                // Forecast gating: require enough distinct day buckets to avoid
                // day-one nonsense, and a positive aggregate rate (a flat or
                // shrinking footprint has no meaningful days-to-full).
                if stats.day_bucket_count < STORAGE_GROWTH_FORECAST_MIN_DAY_BUCKETS
                    || stats.total_delta_bytes <= 0
                    || stats.daily_rate_bytes <= 0
                {
                    return None;
                }
                let cloud_stats =
                    growth_forecast_stats_from_daily_totals(&self.load_cloud_daily_growth_totals(
                        connection,
                        roots,
                        window_start,
                        Some(&volume.path),
                    ));
                let cloud_growth_share_percent = if stats.daily_rate_bytes > 0
                    && cloud_stats.daily_rate_bytes > 0
                {
                    ((cloud_stats.daily_rate_bytes as f64 / stats.daily_rate_bytes as f64) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u64
                } else {
                    0
                };
                let effective_available_bytes = volume
                    .important_usage_available_bytes
                    .unwrap_or(volume.free_now_bytes);
                let forecast_daily_rate_lower_bytes = stats.daily_rate_lower_bytes.max(1);
                let forecast_daily_rate_upper_bytes = stats
                    .daily_rate_upper_bytes
                    .max(forecast_daily_rate_lower_bytes);
                let mut forecast = StorageGrowthForecast {
                    volume_path: volume.path.clone(),
                    free_now_bytes: volume.free_now_bytes,
                    available_bytes: volume.available_bytes,
                    purgeable_bytes_estimate: volume.purgeable_bytes_estimate,
                    important_usage_available_bytes: volume.important_usage_available_bytes,
                    opportunistic_usage_available_bytes: volume.opportunistic_usage_available_bytes,
                    effective_available_bytes,
                    daily_rate_bytes: stats.daily_rate_bytes,
                    daily_rate_lower_bytes: forecast_daily_rate_lower_bytes,
                    daily_rate_upper_bytes: forecast_daily_rate_upper_bytes,
                    days_to_full: days_until_capacity_full(
                        volume.free_now_bytes,
                        stats.daily_rate_bytes,
                    ),
                    days_to_full_lower_bound: days_until_capacity_full(
                        volume.free_now_bytes,
                        forecast_daily_rate_upper_bytes,
                    ),
                    days_to_full_upper_bound: days_until_capacity_full(
                        volume.free_now_bytes,
                        forecast_daily_rate_lower_bytes,
                    ),
                    days_to_effective_full: days_until_capacity_full(
                        effective_available_bytes,
                        stats.daily_rate_bytes,
                    ),
                    days_to_available_full: days_until_capacity_full(
                        volume.available_bytes,
                        stats.daily_rate_bytes,
                    ),
                    purgeable_cushion_days: days_until_capacity_full(
                        volume.purgeable_bytes_estimate,
                        stats.daily_rate_bytes,
                    ),
                    cloud_daily_rate_bytes: cloud_stats.daily_rate_bytes.max(0),
                    cloud_growth_share_percent,
                    volatility_percent: stats.volatility_percent,
                    seasonal_pattern: stats.seasonal_pattern,
                    seasonal_peak_daily_bytes: stats.seasonal_peak_daily_bytes,
                    confidence: stats.confidence,
                    forecast_notes: Vec::new(),
                };
                forecast.forecast_notes = storage_forecast_notes(&forecast);
                Some(forecast)
            })
            .collect()
    }

    fn load_daily_growth_totals(
        &self,
        connection: &Connection,
        roots: &[PathBuf],
        window_start: u64,
        scope_filter: Option<(&str, &str)>,
        volume_path: Option<&str>,
    ) -> Vec<(u64, i64)> {
        let mut predicate = "granularity = 'day' AND bucket_millis >= ?".to_owned();
        let mut bindings: Vec<rusqlite::types::Value> =
            vec![(window_start.min(i64::MAX as u64) as i64).into()];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "source_root");
        if let Some((scope_column, scope)) = scope_filter {
            predicate.push_str(&format!(" AND {scope_column} = ?"));
            bindings.push(scope.to_owned().into());
        }
        push_volume_predicate(&mut predicate, &mut bindings, volume_path, "source_root");
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT bucket_millis / {DAY_MILLIS} AS day_bucket,
                    COALESCE(SUM(total_delta_bytes), 0)
             FROM storage_growth_rollup
             WHERE {predicate}
             GROUP BY day_bucket
             ORDER BY day_bucket ASC"
        )) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(params_from_iter(bindings.iter()), |row| {
            let day_bucket: i64 = row.get(0)?;
            let total_delta: i64 = row.get(1)?;
            Ok((day_bucket.max(0) as u64, total_delta))
        }) else {
            return Vec::new();
        };
        rows.flatten().collect()
    }

    fn load_cloud_daily_growth_totals(
        &self,
        connection: &Connection,
        roots: &[PathBuf],
        window_start: u64,
        volume_path: Option<&str>,
    ) -> Vec<(u64, i64)> {
        let mut predicate = "granularity = 'day' AND bucket_millis >= ?".to_owned();
        let mut bindings: Vec<rusqlite::types::Value> =
            vec![(window_start.min(i64::MAX as u64) as i64).into()];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "source_root");
        push_volume_predicate(&mut predicate, &mut bindings, volume_path, "source_root");
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT bucket_millis / {DAY_MILLIS} AS day_bucket,
                    source_root,
                    repo_root,
                    COALESCE(SUM(total_delta_bytes), 0)
             FROM storage_growth_rollup
             WHERE {predicate}
             GROUP BY day_bucket, source_root, repo_root
             ORDER BY day_bucket ASC"
        )) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(params_from_iter(bindings.iter()), |row| {
            let day_bucket: i64 = row.get(0)?;
            let source_root: String = row.get(1)?;
            let repo_root: String = row.get(2)?;
            let total_delta: i64 = row.get(3)?;
            Ok((
                day_bucket.max(0) as u64,
                source_root,
                repo_root,
                total_delta,
            ))
        }) else {
            return Vec::new();
        };
        let mut totals = BTreeMap::<u64, i64>::new();
        for (day_bucket, source_root, repo_root, total_delta) in rows.flatten() {
            if storage_path_is_cloud(&source_root) || storage_path_is_cloud(&repo_root) {
                *totals.entry(day_bucket).or_default() += total_delta;
            }
        }
        totals.into_iter().collect()
    }

    fn load_growth_anomalies(
        &self,
        connection: &Connection,
        roots: &[PathBuf],
        window_start: u64,
        window_days: u64,
    ) -> Vec<StorageGrowthAnomaly> {
        let latest_scan_millis = self.latest_growth_scan_millis(connection, roots);
        if latest_scan_millis == 0 {
            return Vec::new();
        }

        let mut predicate = "scan_millis = ? AND delta_bytes > 0 AND delta_bytes >= ?".to_owned();
        let mut bindings: Vec<rusqlite::types::Value> = vec![
            (latest_scan_millis.min(i64::MAX as u64) as i64).into(),
            (STORAGE_GROWTH_ANOMALY_MIN_DELTA_BYTES.min(i64::MAX as u64) as i64).into(),
        ];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT bucket_millis, scan_millis, path, source_root, repo_root, kind,
                    cleanup_tier, delta_bytes
             FROM storage_growth_delta
             WHERE {predicate}
             ORDER BY delta_bytes DESC, path ASC
             LIMIT {STORAGE_GROWTH_ANOMALY_CANDIDATE_LIMIT}"
        )) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(params_from_iter(bindings.iter()), |row| {
            let bucket_millis: i64 = row.get(0)?;
            let scan_millis: i64 = row.get(1)?;
            let delta_bytes: i64 = row.get(7)?;
            Ok(GrowthAnomalyCandidate {
                bucket_millis: bucket_millis.max(0) as u64,
                scan_millis: scan_millis.max(0) as u64,
                path: row.get(2)?,
                source_root: row.get(3)?,
                repo_root: row.get(4)?,
                kind: row.get(5)?,
                cleanup_tier: row.get(6)?,
                delta_bytes: delta_bytes.max(0) as u64,
            })
        }) else {
            return Vec::new();
        };

        let mut anomalies = rows
            .flatten()
            .filter_map(|candidate| {
                let baseline = self.growth_baseline_for_path(
                    connection,
                    &candidate.path,
                    window_start,
                    latest_scan_millis,
                );
                growth_anomaly_for_candidate(candidate, baseline, window_days)
            })
            .collect::<Vec<_>>();
        anomalies.sort_by(|left, right| {
            anomaly_rank(&right.severity)
                .cmp(&anomaly_rank(&left.severity))
                .then_with(|| right.current_delta_bytes.cmp(&left.current_delta_bytes))
                .then_with(|| right.z_score.total_cmp(&left.z_score))
                .then_with(|| left.path.cmp(&right.path))
        });
        anomalies.truncate(STORAGE_GROWTH_ANOMALY_LIMIT);
        anomalies
    }

    fn latest_growth_scan_millis(&self, connection: &Connection, roots: &[PathBuf]) -> u64 {
        let mut predicate = "1 = 1".to_owned();
        let mut bindings: Vec<rusqlite::types::Value> = Vec::new();
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        connection
            .query_row(
                &format!(
                    "SELECT COALESCE(MAX(scan_millis), 0)
                     FROM storage_growth_delta
                     WHERE {predicate}"
                ),
                params_from_iter(bindings.iter()),
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            .max(0) as u64
    }

    fn growth_baseline_for_path(
        &self,
        connection: &Connection,
        path: &str,
        window_start: u64,
        latest_scan_millis: u64,
    ) -> GrowthBaseline {
        connection
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(AVG(delta_bytes), 0),
                        COALESCE(AVG(CAST(delta_bytes AS REAL) * CAST(delta_bytes AS REAL)), 0),
                        COALESCE(MAX(delta_bytes), 0)
                 FROM storage_growth_delta
                 WHERE path = ?1
                   AND scan_millis < ?2
                   AND bucket_millis >= ?3
                   AND delta_bytes > 0",
                params![
                    path,
                    latest_scan_millis.min(i64::MAX as u64) as i64,
                    window_start.min(i64::MAX as u64) as i64
                ],
                |row| {
                    let count: i64 = row.get(0)?;
                    let mean: f64 = row.get(1)?;
                    let mean_square: f64 = row.get(2)?;
                    let peak: i64 = row.get(3)?;
                    let variance = (mean_square - mean * mean).max(0.0);
                    Ok(GrowthBaseline {
                        count: count.max(0) as u64,
                        mean,
                        stddev: variance.sqrt(),
                        peak: peak.max(0) as u64,
                    })
                },
            )
            .unwrap_or_default()
    }

    fn load_since_last_scan_diff(
        &self,
        connection: &Connection,
        roots: &[PathBuf],
    ) -> StorageScanDiff {
        let disappeared_note = "Disappeared items are not cleanly derivable: unchanged \
                                directories are served from the size cache and keep prior scan \
                                generations, so a stale generation does not imply deletion."
            .to_owned();
        let mut predicate = "1 = 1".to_owned();
        let mut bindings: Vec<rusqlite::types::Value> = Vec::new();
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        let latest_scan_millis = connection
            .query_row(
                &format!(
                    "SELECT COALESCE(MAX(scan_millis), 0)
                     FROM storage_growth_delta
                     WHERE {predicate}"
                ),
                params_from_iter(bindings.iter()),
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            .max(0) as u64;
        if latest_scan_millis == 0 {
            return StorageScanDiff {
                disappeared_note,
                ..StorageScanDiff::default()
            };
        }

        // Appeared: an insert of a new path always records prev = 0.
        let mut appeared_predicate =
            "scan_millis = ? AND previous_physical_bytes = 0 AND delta_bytes > 0".to_owned();
        let mut appeared_bindings: Vec<rusqlite::types::Value> =
            vec![(latest_scan_millis.min(i64::MAX as u64) as i64).into()];
        push_roots_predicate(
            &mut appeared_predicate,
            &mut appeared_bindings,
            roots,
            "path",
        );
        let (appeared_count, appeared_total_bytes) = connection
            .query_row(
                &format!(
                    "SELECT COUNT(*), COALESCE(SUM(delta_bytes), 0)
                     FROM storage_growth_delta
                     WHERE {appeared_predicate}"
                ),
                params_from_iter(appeared_bindings.iter()),
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?.max(0) as u64,
                        row.get::<_, i64>(1)?.max(0) as u64,
                    ))
                },
            )
            .unwrap_or((0, 0));
        let appeared = connection
            .prepare(&format!(
                "SELECT path, source_root, repo_root, kind, cleanup_tier,
                        current_physical_bytes, delta_bytes, scan_millis
                 FROM storage_growth_delta
                 WHERE {appeared_predicate}
                 ORDER BY delta_bytes DESC, path ASC
                 LIMIT {STORAGE_SCAN_DIFF_ENTRY_LIMIT}"
            ))
            .ok()
            .and_then(|mut statement| {
                statement
                    .query_map(params_from_iter(appeared_bindings.iter()), |row| {
                        let path: String = row.get(0)?;
                        let physical_bytes: i64 = row.get(5)?;
                        let scan_millis: i64 = row.get(7)?;
                        Ok(StorageScanDiffEntry {
                            display_name: diff_display_name(&path),
                            path,
                            source_root: row.get(1)?,
                            repo_root: row.get(2)?,
                            kind: row.get(3)?,
                            cleanup_tier: row.get(4)?,
                            previous_cleanup_tier: String::new(),
                            physical_bytes: physical_bytes.max(0) as u64,
                            delta_bytes: row.get(6)?,
                            scan_millis: scan_millis.max(0) as u64,
                        })
                    })
                    .map(|rows| rows.flatten().collect::<Vec<_>>())
                    .ok()
            })
            .unwrap_or_default();

        // Tier-changed: rows refreshed in the latest index generation whose
        // persisted previous tier differs from the current one.
        let latest_index_generation = {
            let mut generation_predicate = "1 = 1".to_owned();
            let mut generation_bindings: Vec<rusqlite::types::Value> = Vec::new();
            push_roots_predicate(
                &mut generation_predicate,
                &mut generation_bindings,
                roots,
                "path",
            );
            connection
                .query_row(
                    &format!(
                        "SELECT COALESCE(MAX(last_scan_millis), 0)
                         FROM storage_file_index
                         WHERE {generation_predicate}"
                    ),
                    params_from_iter(generation_bindings.iter()),
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0)
                .max(0) as u64
        };
        let mut tier_predicate = "last_scan_millis = ? AND previous_cleanup_tier <> ''
             AND cleanup_tier <> '' AND previous_cleanup_tier <> cleanup_tier"
            .to_owned();
        let mut tier_bindings: Vec<rusqlite::types::Value> =
            vec![(latest_index_generation.min(i64::MAX as u64) as i64).into()];
        push_roots_predicate(&mut tier_predicate, &mut tier_bindings, roots, "path");
        let tier_changed_count = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM storage_file_index WHERE {tier_predicate}"),
                params_from_iter(tier_bindings.iter()),
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            .max(0) as u64;
        let tier_changed = connection
            .prepare(&format!(
                "SELECT path, source_root, repo_root, kind, cleanup_tier,
                        previous_cleanup_tier, physical_bytes, last_scan_millis
                 FROM storage_file_index
                 WHERE {tier_predicate}
                 ORDER BY physical_bytes DESC, path ASC
                 LIMIT {STORAGE_SCAN_DIFF_ENTRY_LIMIT}"
            ))
            .ok()
            .and_then(|mut statement| {
                statement
                    .query_map(params_from_iter(tier_bindings.iter()), |row| {
                        let path: String = row.get(0)?;
                        let physical_bytes: i64 = row.get(6)?;
                        let scan_millis: i64 = row.get(7)?;
                        Ok(StorageScanDiffEntry {
                            display_name: diff_display_name(&path),
                            path,
                            source_root: row.get(1)?,
                            repo_root: row.get(2)?,
                            kind: row.get(3)?,
                            cleanup_tier: row.get(4)?,
                            previous_cleanup_tier: row.get(5)?,
                            physical_bytes: physical_bytes.max(0) as u64,
                            delta_bytes: 0,
                            scan_millis: scan_millis.max(0) as u64,
                        })
                    })
                    .map(|rows| rows.flatten().collect::<Vec<_>>())
                    .ok()
            })
            .unwrap_or_default();

        StorageScanDiff {
            latest_scan_millis,
            appeared_count,
            appeared_total_bytes,
            appeared,
            tier_changed_count,
            tier_changed,
            disappeared: Vec::new(),
            disappeared_note,
        }
    }

    /// Aggregate one cold-data band over `max(accessed, modified)` age:
    /// item count, total bytes, and the largest rows. Restricted to the safe
    /// and rebuildable tiers and the minimum item size; rows with neither
    /// timestamp are excluded rather than guessed.
    pub(super) fn load_cold_band(
        &self,
        roots: &[PathBuf],
        min_age_days: u64,
        max_age_days: Option<u64>,
        now_millis: u64,
        limit: usize,
    ) -> Option<(u64, u64, Vec<StorageIndexedFileRow>)> {
        self.flush_pending_rows();
        let connection = self.connection.as_ref()?;
        let cold_before = now_millis.saturating_sub(min_age_days.saturating_mul(DAY_MILLIS));
        let mut predicate = "cleanup_tier IN ('safe', 'rebuildable')
             AND physical_bytes >= ?
             AND (accessed_millis IS NOT NULL OR modified_millis IS NOT NULL)
             AND MAX(COALESCE(accessed_millis, 0), COALESCE(modified_millis, 0)) < ?"
            .to_owned();
        let mut bindings: Vec<rusqlite::types::Value> = vec![
            (MIN_ITEM_BYTES.min(i64::MAX as u64) as i64).into(),
            (cold_before.min(i64::MAX as u64) as i64).into(),
        ];
        if let Some(max_age_days) = max_age_days {
            let young_bound = now_millis.saturating_sub(max_age_days.saturating_mul(DAY_MILLIS));
            predicate.push_str(
                " AND MAX(COALESCE(accessed_millis, 0), COALESCE(modified_millis, 0)) >= ?",
            );
            bindings.push((young_bound.min(i64::MAX as u64) as i64).into());
        }
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        let (item_count, total_bytes) = connection
            .query_row(
                &format!(
                    "SELECT COUNT(*), COALESCE(SUM(physical_bytes), 0)
                     FROM storage_file_index
                     WHERE {predicate}"
                ),
                params_from_iter(bindings.iter()),
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?.max(0) as u64,
                        row.get::<_, i64>(1)?.max(0) as u64,
                    ))
                },
            )
            .ok()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT path, device, inode, file_id, source_root, repo_root, kind,
                        storage_role, safety, cleanup_tier, logical_bytes, physical_bytes,
                        modified_millis, changed_millis, accessed_millis, birth_millis,
                        is_directory, entries, truncated, last_scan_millis
                 FROM storage_file_index
                 WHERE {predicate}
                 ORDER BY physical_bytes DESC, path ASC
                 LIMIT {limit}"
            ))
            .ok()?;
        let rows = statement
            .query_map(params_from_iter(bindings.iter()), indexed_file_row_from_sql)
            .ok()?
            .flatten()
            .collect::<Vec<_>>();
        Some((item_count, total_bytes, rows))
    }

    pub(super) fn load_index_summaries(&self, roots: &[PathBuf]) -> Vec<StorageIndexSummaryRow> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let Ok(mut statement) = connection.prepare(
            "SELECT source_root, item_count, inventory_size_bytes,
                    safe_reclaimable_bytes, maybe_reclaimable_bytes, review_required_bytes,
                    dangerous_user_data_bytes
             FROM storage_index_summary
             ORDER BY captured_at_millis DESC, source_root ASC",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map([], storage_index_summary_row_from_sql) else {
            return Vec::new();
        };
        rows.flatten()
            .filter(|row| {
                roots.is_empty()
                    || roots
                        .iter()
                        .any(|root| path_is_under_root(&row.source_root, root))
            })
            .collect()
    }

    pub(super) fn load_top_offenders(
        &self,
        roots: &[PathBuf],
        limit: usize,
    ) -> Vec<StorageTopOffenderRow> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let read_limit = limit.saturating_mul(4).clamp(1, 1_000);
        let Ok(mut statement) = connection.prepare(
            "SELECT source_root, path, kind, cleanup_tier, physical_bytes,
                    recommendation_score, last_scan_millis
             FROM storage_top_offender
             ORDER BY recommendation_score DESC, physical_bytes DESC, path ASC
             LIMIT ?1",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(
            params![read_limit as i64],
            storage_top_offender_row_from_sql,
        ) else {
            return Vec::new();
        };
        let mut offenders = Vec::with_capacity(limit.min(read_limit));
        for row in rows.flatten() {
            if !roots.is_empty()
                && !roots
                    .iter()
                    .any(|root| path_is_under_root(&row.source_root, root))
            {
                continue;
            }
            offenders.push(row);
            if offenders.len() >= limit {
                break;
            }
        }
        offenders
    }

    pub(super) fn load_situation_top_offenders(
        &self,
        roots: &[PathBuf],
        limit: usize,
    ) -> Vec<StorageTopOffenderRow> {
        let limit = limit.clamp(1, 40);
        let read_limit = limit.saturating_mul(4).clamp(1, 1_000);
        let mut offenders = self.load_materialized_path_top_offenders(roots, read_limit);
        offenders.extend(self.load_domain_top_offenders(roots, read_limit));
        offenders.extend(self.load_top_offenders(roots, read_limit));

        let mut seen_paths = BTreeSet::new();
        offenders.retain(|row| seen_paths.insert(row.path.clone()));
        offenders.sort_by(|left, right| {
            right
                .physical_bytes
                .cmp(&left.physical_bytes)
                .then_with(|| {
                    right
                        .recommendation_score
                        .total_cmp(&left.recommendation_score)
                })
                .then_with(|| left.path.cmp(&right.path))
        });
        offenders.truncate(limit);
        offenders
    }

    fn load_materialized_path_top_offenders(
        &self,
        roots: &[PathBuf],
        limit: usize,
    ) -> Vec<StorageTopOffenderRow> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let mut predicate = "physical_bytes > 0".to_owned();
        let mut bindings = Vec::new();
        push_roots_predicate(&mut predicate, &mut bindings, roots, "source_root");
        bindings.push((limit.clamp(1, 1_000) as i64).into());
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT source_root, path, artifact_kind, cleanup_tier, physical_bytes,
                    recommendation_score, last_measured_millis
             FROM storage_path
             WHERE {predicate}
             ORDER BY physical_bytes DESC, recommendation_score DESC, last_measured_millis DESC, path ASC
             LIMIT ?"
        )) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(
            params_from_iter(bindings.iter()),
            storage_top_offender_row_from_sql,
        ) else {
            return Vec::new();
        };
        rows.flatten().collect()
    }

    fn load_domain_top_offenders(
        &self,
        roots: &[PathBuf],
        limit: usize,
    ) -> Vec<StorageTopOffenderRow> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let mut predicate = "physical_bytes > 0 AND path_prefix <> ''".to_owned();
        let mut bindings = Vec::new();
        push_roots_predicate(&mut predicate, &mut bindings, roots, "source_root");
        bindings.push((limit.clamp(1, 1_000) as i64).into());
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT source_root, path_prefix, domain_kind,
                    CASE
                        WHEN dangerous_user_data_bytes > 0 THEN 'dangerous'
                        WHEN review_required_bytes > 0 THEN 'review'
                        WHEN safely_reclaimable_now_bytes > 0 THEN 'rebuildable'
                        WHEN maybe_reclaimable_bytes > 0 THEN 'review'
                        ELSE ''
                    END,
                    physical_bytes,
                    CASE
                        WHEN dangerous_user_data_bytes > 0 THEN 20.0
                        WHEN review_required_bytes > 0 THEN 40.0
                        WHEN maybe_reclaimable_bytes > 0 THEN 60.0
                        WHEN safely_reclaimable_now_bytes > 0 THEN 80.0
                        ELSE 0.0
                    END,
                    last_measured_millis
             FROM storage_domain
             WHERE {predicate}
             ORDER BY physical_bytes DESC, last_measured_millis DESC, path_prefix ASC
             LIMIT ?"
        )) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(
            params_from_iter(bindings.iter()),
            storage_top_offender_row_from_sql,
        ) else {
            return Vec::new();
        };
        rows.flatten().collect()
    }

    pub(super) fn store_typed_storage_domains(
        &self,
        roots: &[PathBuf],
        domains: &[StorageSituationDomain],
    ) -> Result<(), String> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Err(self.status.clone());
        };
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| format!("typed_storage_domain_transaction:{error}"))?;
        for root in roots {
            transaction
                .execute(
                    "DELETE FROM storage_domain
                     WHERE source = 'typed_detector' AND source_root = ?1",
                    params![root.display().to_string()],
                )
                .map_err(|error| format!("clear_typed_storage_domains:{error}"))?;
        }
        {
            let mut insert = transaction
                .prepare(
                    "INSERT OR REPLACE INTO storage_domain (
                        domain_id, label, source_root, domain_kind, path_prefix, item_count,
                        directory_count, file_count, logical_bytes, physical_bytes,
                        safely_reclaimable_now_bytes, maybe_reclaimable_bytes,
                        review_required_bytes, dangerous_user_data_bytes, last_measured_millis,
                        confidence, source
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                        ?17
                     )",
                )
                .map_err(|error| format!("prepare_typed_storage_domain_insert:{error}"))?;
            for domain in domains {
                insert
                    .execute(params![
                        &domain.domain_id,
                        &domain.label,
                        &domain.source_root,
                        &domain.domain_kind,
                        &domain.path_prefix,
                        domain.item_count.min(i64::MAX as u64) as i64,
                        domain.directory_count.min(i64::MAX as u64) as i64,
                        domain.file_count.min(i64::MAX as u64) as i64,
                        domain.logical_bytes.min(i64::MAX as u64) as i64,
                        domain.physical_bytes.min(i64::MAX as u64) as i64,
                        domain.safely_reclaimable_now_bytes.min(i64::MAX as u64) as i64,
                        domain.maybe_reclaimable_bytes.min(i64::MAX as u64) as i64,
                        domain.review_required_bytes.min(i64::MAX as u64) as i64,
                        domain.dangerous_user_data_bytes.min(i64::MAX as u64) as i64,
                        domain.last_measured_millis.min(i64::MAX as u64) as i64,
                        &domain.confidence,
                        &domain.source,
                    ])
                    .map_err(|error| format!("insert_typed_storage_domain:{error}"))?;
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("commit_typed_storage_domains:{error}"))?;
        super::report::invalidate_index_report_sections_memo();
        Ok(())
    }

    pub(super) fn load_storage_domains(
        &self,
        roots: &[PathBuf],
        limit: usize,
    ) -> Vec<StorageSituationDomain> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let mut predicate = "1 = 1".to_owned();
        let mut bindings = Vec::new();
        push_roots_predicate(&mut predicate, &mut bindings, roots, "source_root");
        bindings.push((limit.clamp(1, 1_000) as i64).into());
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT domain_id, label, source_root, domain_kind, path_prefix, item_count,
                    directory_count, file_count, logical_bytes, physical_bytes,
                    safely_reclaimable_now_bytes, maybe_reclaimable_bytes, review_required_bytes,
                    dangerous_user_data_bytes, last_measured_millis, confidence, source
             FROM storage_domain
             WHERE {predicate}
             ORDER BY
                CASE WHEN source = 'typed_detector' THEN 0 ELSE 1 END,
                CASE WHEN domain_kind = 'source_root' THEN 1 ELSE 0 END,
                physical_bytes DESC,
                label ASC,
                path_prefix ASC
             LIMIT ?",
        )) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(
            params_from_iter(bindings.iter()),
            storage_situation_domain_from_sql,
        ) else {
            return Vec::new();
        };
        rows.flatten().collect()
    }

    pub(super) fn persist_situation_snapshot(
        &self,
        roots: &[PathBuf],
        source: &str,
        situation: &StorageSituationResponse,
    ) -> Result<(), String> {
        let Some(connection) = self.connection.as_ref() else {
            return Err(self.status.clone());
        };
        let root_key = storage_situation_roots_key(roots);
        let roots_json = serde_json::to_string(
            &roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>(),
        )
        .map_err(|error| format!("encode_roots:{error}"))?;
        let snapshot_json =
            serde_json::to_string(situation).map_err(|error| format!("encode_snapshot:{error}"))?;
        let now_millis = storage_now_millis();
        connection
            .execute(
                "INSERT INTO storage_situation_snapshot (
                    root_key,
                    roots_json,
                    captured_at_millis,
                    updated_at_millis,
                    source,
                    item_count,
                    inventory_size_bytes,
                    safely_reclaimable_now_bytes,
                    dirty_path_count,
                    snapshot_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(root_key) DO UPDATE SET
                    roots_json = excluded.roots_json,
                    captured_at_millis = excluded.captured_at_millis,
                    updated_at_millis = excluded.updated_at_millis,
                    source = excluded.source,
                    item_count = excluded.item_count,
                    inventory_size_bytes = excluded.inventory_size_bytes,
                    safely_reclaimable_now_bytes = excluded.safely_reclaimable_now_bytes,
                    dirty_path_count = excluded.dirty_path_count,
                    snapshot_json = excluded.snapshot_json",
                params![
                    root_key,
                    roots_json,
                    situation.captured_at_millis.min(i64::MAX as u64) as i64,
                    now_millis.min(i64::MAX as u64) as i64,
                    source,
                    situation.summary.item_count.min(i64::MAX as u64) as i64,
                    situation.summary.inventory_size_bytes.min(i64::MAX as u64) as i64,
                    situation
                        .summary
                        .safely_reclaimable_now_bytes
                        .min(i64::MAX as u64) as i64,
                    situation.dirty_paths.dirty_path_count.min(i64::MAX as u64) as i64,
                    snapshot_json,
                ],
            )
            .map_err(|error| format!("persist_situation_snapshot:{error}"))?;
        Ok(())
    }

    pub(super) fn load_situation_snapshot(
        &self,
        roots: &[PathBuf],
        limit: usize,
    ) -> Option<StorageSituationResponse> {
        let connection = self.connection.as_ref()?;
        let root_key = storage_situation_roots_key(roots);
        load_situation_snapshot_for_key(connection, &root_key, limit)
    }

    #[cfg(test)]
    pub(super) fn pending_row_count(&self) -> usize {
        self.pending_rows.borrow().len()
    }

    /// Test-only probe: whether the query-planner statistics table exists.
    #[cfg(test)]
    pub(super) fn has_query_planner_statistics(&self) -> bool {
        let Some(connection) = self.connection.as_ref() else {
            return false;
        };
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'sqlite_stat1'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false)
    }

    /// Test-only snapshot of one indexed row's persisted recommendation score.
    #[cfg(test)]
    pub(super) fn indexed_row_recommendation_score(&self, path: &str) -> Option<f64> {
        self.flush_pending_rows();
        let connection = self.connection.as_ref()?;
        connection
            .query_row(
                "SELECT recommendation_score FROM storage_file_index WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .ok()
    }

    /// Test-only snapshot of one indexed row's byte count, cleanup tier, and
    /// persisted previous cleanup tier. Flushes pending rows first.
    #[cfg(test)]
    pub(super) fn indexed_row_tier_snapshot(&self, path: &str) -> Option<(u64, String, String)> {
        self.flush_pending_rows();
        let connection = self.connection.as_ref()?;
        connection
            .query_row(
                "SELECT physical_bytes, cleanup_tier, previous_cleanup_tier
                 FROM storage_file_index
                 WHERE path = ?1",
                params![path],
                |row| {
                    let physical_bytes: i64 = row.get(0)?;
                    Ok((physical_bytes.max(0) as u64, row.get(1)?, row.get(2)?))
                },
            )
            .ok()
    }

    #[cfg(test)]
    pub(super) fn count_indexed_rows_with_prefix(&self, prefix: &str) -> u64 {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return 0;
        };
        connection
            .query_row(
                "SELECT COUNT(*) FROM storage_file_index WHERE path LIKE ?1 ESCAPE '\\'",
                params![format!("{}%", escape_like_pattern(prefix))],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count.max(0) as u64)
            .unwrap_or_default()
    }

    /// Test-only raw growth-delta view (path, previous, current, delta,
    /// scan_millis) for rows under a prefix, ordered by insertion.
    #[cfg(test)]
    pub(super) fn growth_deltas_with_prefix(
        &self,
        prefix: &str,
    ) -> Vec<(String, u64, u64, i64, u64)> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let Ok(mut statement) = connection.prepare(
            "SELECT path, previous_physical_bytes, current_physical_bytes, delta_bytes,
                    scan_millis
             FROM storage_growth_delta
             WHERE path LIKE ?1 ESCAPE '\\'
             ORDER BY id ASC",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(
            params![format!("{}%", escape_like_pattern(prefix))],
            |row| {
                let previous: i64 = row.get(1)?;
                let current: i64 = row.get(2)?;
                let scan_millis: i64 = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    previous.max(0) as u64,
                    current.max(0) as u64,
                    row.get::<_, i64>(3)?,
                    scan_millis.max(0) as u64,
                ))
            },
        ) else {
            return Vec::new();
        };
        rows.flatten().collect()
    }

    #[cfg(test)]
    pub(super) fn growth_rollup_total_with_prefix(&self, prefix: &str, granularity: &str) -> i64 {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return 0;
        };
        connection
            .query_row(
                "SELECT COALESCE(SUM(total_delta_bytes), 0)
                 FROM storage_growth_rollup
                 WHERE granularity = ?1 AND source_root LIKE ?2 ESCAPE '\\'",
                params![granularity, format!("{}%", escape_like_pattern(prefix))],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(super) fn top_offender_count_with_prefix(&self, prefix: &str) -> u64 {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return 0;
        };
        connection
            .query_row(
                "SELECT COUNT(*)
                 FROM storage_top_offender
                 WHERE source_root LIKE ?1 ESCAPE '\\'",
                params![format!("{}%", escape_like_pattern(prefix))],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count.max(0) as u64)
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(super) fn summary_inventory_with_prefix(&self, prefix: &str) -> (u64, u64) {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return (0, 0);
        };
        connection
            .query_row(
                "SELECT COALESCE(SUM(item_count), 0),
                        COALESCE(SUM(inventory_size_bytes), 0)
                 FROM storage_index_summary
                 WHERE source_root LIKE ?1 ESCAPE '\\'",
                params![format!("{}%", escape_like_pattern(prefix))],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?.max(0) as u64,
                        row.get::<_, i64>(1)?.max(0) as u64,
                    ))
                },
            )
            .unwrap_or_default()
    }
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

fn upsert_growth_rollups(
    statement: &mut rusqlite::Statement<'_>,
    row: &StorageIndexedFileRow,
    hour_bucket_millis: u64,
    delta_bytes: i64,
) {
    let day_bucket_millis = (row.last_scan_millis / DAY_MILLIS) * DAY_MILLIS;
    for (granularity, bucket_millis) in [("hour", hour_bucket_millis), ("day", day_bucket_millis)] {
        let positive_delta_bytes = delta_bytes.max(0);
        let negative_delta_bytes = delta_bytes.min(0);
        let max_abs_delta_bytes = delta_bytes.saturating_abs();
        let _ = statement.execute(params![
            granularity,
            bucket_millis.min(i64::MAX as u64) as i64,
            &row.source_root,
            row.repo_root.as_deref().unwrap_or(""),
            &row.kind,
            &row.cleanup_tier,
            delta_bytes,
            positive_delta_bytes,
            negative_delta_bytes,
            max_abs_delta_bytes,
            row.last_scan_millis.min(i64::MAX as u64) as i64,
        ]);
    }
}

fn prune_storage_growth_history(
    transaction: &rusqlite::Transaction<'_>,
    max_scan_millis: u64,
    limits: StorageIndexBudgetLimits,
) {
    let oldest_path_delta = max_scan_millis
        .saturating_sub(STORAGE_GROWTH_TOP_OFFENDER_RETENTION_MILLIS)
        .min(i64::MAX as u64) as i64;
    let full_path_cutoff = max_scan_millis
        .saturating_sub(STORAGE_GROWTH_PATH_RETENTION_MILLIS)
        .min(i64::MAX as u64) as i64;
    let hourly_rollup_cutoff = max_scan_millis
        .saturating_sub(STORAGE_GROWTH_HOURLY_ROLLUP_RETENTION_MILLIS)
        .min(i64::MAX as u64) as i64;
    let daily_rollup_cutoff = max_scan_millis
        .saturating_sub(STORAGE_GROWTH_DAILY_ROLLUP_RETENTION_MILLIS)
        .min(i64::MAX as u64) as i64;

    let _ = transaction.execute(
        "DELETE FROM storage_growth_delta WHERE scan_millis < ?1",
        params![oldest_path_delta],
    );
    let _ = transaction.execute(
        "DELETE FROM storage_growth_delta
         WHERE scan_millis < ?1
           AND id NOT IN (
                SELECT id
                FROM storage_growth_delta
                WHERE scan_millis >= ?2
                ORDER BY ABS(delta_bytes) DESC, scan_millis DESC, id DESC
                LIMIT ?3
           )",
        params![
            full_path_cutoff,
            oldest_path_delta,
            limits.max_growth_delta_rows.min(i64::MAX as u64) as i64,
        ],
    );
    let _ = transaction.execute(
        "DELETE FROM storage_growth_rollup
         WHERE granularity = 'hour' AND bucket_millis < ?1",
        params![hourly_rollup_cutoff],
    );
    let _ = transaction.execute(
        "DELETE FROM storage_growth_rollup
         WHERE granularity = 'day' AND bucket_millis < ?1",
        params![daily_rollup_cutoff],
    );
}

fn refresh_storage_index_summaries_and_top_offenders(
    transaction: &rusqlite::Transaction<'_>,
    source_roots: &BTreeSet<String>,
    captured_at_millis: u64,
) {
    for source_root in source_roots {
        let _ = transaction.execute(
            "INSERT OR REPLACE INTO storage_index_summary (
                source_root, captured_at_millis, item_count, inventory_size_bytes,
                safe_reclaimable_bytes, maybe_reclaimable_bytes, review_required_bytes,
                dangerous_user_data_bytes
             )
             SELECT ?1, ?2, COUNT(*), COALESCE(SUM(physical_bytes), 0),
                    COALESCE(SUM(CASE
                        WHEN cleanup_tier IN ('safe', 'rebuildable') AND safety = 'safe'
                        THEN physical_bytes ELSE 0 END), 0),
                    COALESCE(SUM(CASE
                        WHEN cleanup_tier IN ('safe', 'rebuildable') AND safety <> 'safe'
                          AND NOT (
                            kind IN ('macos-app-bundle', 'app-support-data', 'app-container',
                                     'app-launch-item', 'app-preferences', 'app-receipt',
                                     'ai-session-data', 'offline-media', 'colima-vm',
                                     'docker-vm', 'ios-backup', 'mail-attachments',
                                     'message-attachments', 'local-snapshot')
                            OR storage_role IN ('application', 'app-data', 'agent-data',
                                                'offline-media', 'system-data', 'user-data')
                            OR cleanup_tier IN ('blocked', 'dangerous')
                            OR safety IN ('blocked', 'dangerous')
                          )
                        THEN physical_bytes ELSE 0 END), 0),
                    COALESCE(SUM(CASE
                        WHEN (cleanup_tier = 'review' OR safety = 'review')
                          AND NOT (
                            kind IN ('macos-app-bundle', 'app-support-data', 'app-container',
                                     'app-launch-item', 'app-preferences', 'app-receipt',
                                     'ai-session-data', 'offline-media', 'colima-vm',
                                     'docker-vm', 'ios-backup', 'mail-attachments',
                                     'message-attachments', 'local-snapshot')
                            OR storage_role IN ('application', 'app-data', 'agent-data',
                                                'offline-media', 'system-data', 'user-data')
                            OR cleanup_tier IN ('blocked', 'dangerous')
                            OR safety IN ('blocked', 'dangerous')
                          )
                        THEN physical_bytes ELSE 0 END), 0),
                    COALESCE(SUM(CASE
                        WHEN kind IN ('macos-app-bundle', 'app-support-data', 'app-container',
                                      'app-launch-item', 'app-preferences', 'app-receipt',
                                      'ai-session-data', 'offline-media', 'colima-vm',
                                      'docker-vm', 'ios-backup', 'mail-attachments',
                                      'message-attachments', 'local-snapshot')
                          OR storage_role IN ('application', 'app-data', 'agent-data',
                                              'offline-media', 'system-data', 'user-data')
                          OR cleanup_tier IN ('blocked', 'dangerous')
                          OR safety IN ('blocked', 'dangerous')
                        THEN physical_bytes ELSE 0 END), 0)
             FROM storage_file_index
             WHERE source_root = ?1",
            params![source_root, captured_at_millis.min(i64::MAX as u64) as i64,],
        );
        let _ = transaction.execute(
            "INSERT OR REPLACE INTO storage_top_offender (
                source_root, path, kind, cleanup_tier, physical_bytes,
                recommendation_score, last_scan_millis
             )
             SELECT source_root, path, kind, cleanup_tier, physical_bytes,
                    recommendation_score, last_scan_millis
             FROM storage_file_index
             WHERE source_root = ?1
             ORDER BY recommendation_score DESC, physical_bytes DESC, last_scan_millis DESC, path ASC
            LIMIT ?2",
            params![source_root, STORAGE_TOP_OFFENDERS_PER_ROOT as i64],
        );
        let _ = transaction.execute(
            "DELETE FROM storage_top_offender
             WHERE source_root = ?1
               AND path NOT IN (
                    SELECT path
                    FROM storage_top_offender
                    WHERE source_root = ?1
                    ORDER BY physical_bytes DESC, recommendation_score DESC, last_scan_millis DESC, path ASC
                    LIMIT ?2
               )",
            params![source_root, STORAGE_TOP_OFFENDERS_PER_ROOT as i64],
        );
    }
}

fn materialized_storage_index_generation(connection: &Connection) -> rusqlite::Result<String> {
    connection.query_row(
        "SELECT COUNT(*), COALESCE(MAX(last_scan_millis), 0)
         FROM storage_file_index",
        [],
        |row| {
            let row_count: i64 = row.get(0)?;
            let max_scan_millis: i64 = row.get(1)?;
            Ok(format!("{}:{}", row_count.max(0), max_scan_millis.max(0)))
        },
    )
}

fn set_materialized_storage_index_generation(
    connection: &Connection,
    generation: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR REPLACE INTO storage_index_meta (key, value)
         VALUES ('materialized_storage_index_generation', ?1)",
        params![generation],
    )?;
    Ok(())
}

fn record_deferred_materialized_storage_backfill(
    connection: &Connection,
    generation: &str,
    legacy_path_count: u64,
) -> rusqlite::Result<()> {
    let now_millis = storage_now_millis();
    connection.execute(
        "INSERT OR REPLACE INTO storage_measurement_job (
            job_id, job_kind, status, source, root_key, roots_json, dirty_paths_json,
            started_at_millis, updated_at_millis, completed_at_millis, measured_path_count,
            measured_directory_count, measured_file_count, measured_bytes, partial, last_error
         ) VALUES (
            'legacy-file-index:deferred-materialized-backfill',
            'legacy_file_index_backfill',
            'pending',
            'storage_file_index',
            '*',
            '[]',
            '[]',
            ?1,
            ?1,
            NULL,
            0,
            0,
            0,
            0,
            1,
            ?2
         )",
        params![
            now_millis.min(i64::MAX as u64) as i64,
            format!(
                "deferred_large_legacy_index:{legacy_path_count}:sync_cap:{}",
                STORAGE_MATERIALIZED_SYNC_BACKFILL_MAX_ROWS
            ),
        ],
    )?;
    connection.execute(
        "INSERT OR REPLACE INTO storage_index_meta (key, value)
         VALUES ('materialized_storage_index_deferred_generation', ?1)",
        params![generation],
    )?;
    Ok(())
}

fn refresh_materialized_storage_index_for_roots(
    connection: &Connection,
    source_roots: &BTreeSet<String>,
) -> rusqlite::Result<()> {
    if source_roots.is_empty() {
        connection.execute("DELETE FROM storage_path", [])?;
        connection.execute("DELETE FROM storage_directory_rollup", [])?;
        connection.execute("DELETE FROM storage_domain", [])?;
        connection.execute(
            "DELETE FROM storage_measurement_job WHERE source = 'storage_file_index'",
            [],
        )?;
        return Ok(());
    }

    for source_root in source_roots {
        let rows = materialized_storage_rows_for_source_root(connection, source_root)?;
        connection.execute(
            "DELETE FROM storage_path WHERE source_root = ?1",
            params![source_root],
        )?;
        connection.execute(
            "DELETE FROM storage_directory_rollup WHERE source_root = ?1",
            params![source_root],
        )?;
        connection.execute(
            "DELETE FROM storage_domain WHERE source_root = ?1 AND source = 'storage_file_index'",
            params![source_root],
        )?;
        connection.execute(
            "DELETE FROM storage_measurement_job
             WHERE source = 'storage_file_index' AND root_key = ?1",
            params![source_root],
        )?;
        if rows.is_empty() {
            continue;
        }
        write_materialized_storage_rows(connection, &rows)?;
        refresh_materialized_storage_domain_job_for_root(connection, source_root)?;
    }
    Ok(())
}

fn materialized_storage_rows_for_source_root(
    connection: &Connection,
    source_root: &str,
) -> rusqlite::Result<Vec<MaterializedStoragePathRow>> {
    let mut statement = connection.prepare(
        "SELECT path, device, inode, file_id, source_root, repo_root, kind, storage_role,
                safety, cleanup_tier, logical_bytes, physical_bytes, modified_millis,
                changed_millis, accessed_millis, birth_millis, is_directory, entries,
                truncated, last_scan_millis, recommendation_score
         FROM storage_file_index
         WHERE source_root = ?1
         ORDER BY path ASC",
    )?;
    let rows = statement.query_map(params![source_root], materialized_storage_path_row_from_sql)?;
    Ok(rows.flatten().collect())
}

fn write_materialized_storage_rows(
    connection: &Connection,
    rows: &[MaterializedStoragePathRow],
) -> rusqlite::Result<()> {
    let mut upsert_path = connection.prepare(
        "INSERT OR REPLACE INTO storage_path (
            path, parent_path, name, device, inode, file_id, source_root, repo_root, path_kind,
            artifact_kind, storage_role, safety, cleanup_tier, logical_bytes, physical_bytes,
            modified_millis, changed_millis, accessed_millis, birth_millis, entries, truncated,
            recommendation_score, last_measured_millis, last_event_id, confidence, stale, partial,
            source
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22, ?23, NULL, 'indexed', 0, ?24, 'storage_file_index'
         )",
    )?;
    let mut upsert_directory = connection.prepare(
        "INSERT OR REPLACE INTO storage_directory_rollup (
            path, source_root, repo_root, logical_bytes, physical_bytes, child_count,
            recursive_entry_count, truncated, last_measured_millis, confidence, partial, source
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'indexed', ?10, 'storage_file_index'
         )",
    )?;
    let mut delete_directory =
        connection.prepare("DELETE FROM storage_directory_rollup WHERE path = ?1")?;
    for row in rows {
        upsert_path.execute(params![
            &row.path,
            &row.parent_path,
            &row.name,
            row.device,
            row.inode,
            &row.file_id,
            &row.source_root,
            row.repo_root.as_deref(),
            &row.path_kind,
            &row.artifact_kind,
            &row.storage_role,
            &row.safety,
            &row.cleanup_tier,
            row.logical_bytes.min(i64::MAX as u64) as i64,
            row.physical_bytes.min(i64::MAX as u64) as i64,
            row.modified_millis
                .map(|value| value.min(i64::MAX as u64) as i64),
            row.changed_millis
                .map(|value| value.min(i64::MAX as u64) as i64),
            row.accessed_millis
                .map(|value| value.min(i64::MAX as u64) as i64),
            row.birth_millis
                .map(|value| value.min(i64::MAX as u64) as i64),
            row.entries.min(i64::MAX as u64) as i64,
            if row.truncated { 1i64 } else { 0i64 },
            row.recommendation_score,
            row.last_measured_millis.min(i64::MAX as u64) as i64,
            if row.truncated { 1i64 } else { 0i64 },
        ])?;
        if row.path_kind == "directory" {
            upsert_directory.execute(params![
                &row.path,
                &row.source_root,
                row.repo_root.as_deref(),
                row.logical_bytes.min(i64::MAX as u64) as i64,
                row.physical_bytes.min(i64::MAX as u64) as i64,
                row.entries.min(i64::MAX as u64) as i64,
                row.entries.min(i64::MAX as u64) as i64,
                if row.truncated { 1i64 } else { 0i64 },
                row.last_measured_millis.min(i64::MAX as u64) as i64,
                if row.truncated { 1i64 } else { 0i64 },
            ])?;
        } else {
            delete_directory.execute(params![&row.path])?;
        }
    }
    Ok(())
}

fn refresh_materialized_storage_domain_job_for_root(
    connection: &Connection,
    source_root: &str,
) -> rusqlite::Result<()> {
    let aggregate = connection.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN is_directory <> 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(logical_bytes), 0),
                COALESCE(SUM(physical_bytes), 0),
                COALESCE(SUM(CASE
                    WHEN cleanup_tier IN ('safe', 'rebuildable') AND safety = 'safe'
                    THEN physical_bytes ELSE 0 END), 0),
                COALESCE(SUM(CASE
                    WHEN cleanup_tier IN ('safe', 'rebuildable') AND safety <> 'safe'
                      AND NOT (
                        kind IN ('macos-app-bundle', 'app-support-data', 'app-container',
                                 'app-launch-item', 'app-preferences', 'app-receipt',
                                 'ai-session-data', 'offline-media', 'colima-vm',
                                 'docker-vm', 'ios-backup', 'mail-attachments',
                                 'message-attachments', 'local-snapshot')
                        OR storage_role IN ('application', 'app-data', 'agent-data',
                                            'offline-media', 'system-data', 'user-data')
                        OR cleanup_tier IN ('blocked', 'dangerous')
                        OR safety IN ('blocked', 'dangerous')
                      )
                    THEN physical_bytes ELSE 0 END), 0),
                COALESCE(SUM(CASE
                    WHEN (cleanup_tier = 'review' OR safety = 'review')
                      AND NOT (
                        kind IN ('macos-app-bundle', 'app-support-data', 'app-container',
                                 'app-launch-item', 'app-preferences', 'app-receipt',
                                 'ai-session-data', 'offline-media', 'colima-vm',
                                 'docker-vm', 'ios-backup', 'mail-attachments',
                                 'message-attachments', 'local-snapshot')
                        OR storage_role IN ('application', 'app-data', 'agent-data',
                                            'offline-media', 'system-data', 'user-data')
                        OR cleanup_tier IN ('blocked', 'dangerous')
                        OR safety IN ('blocked', 'dangerous')
                      )
                    THEN physical_bytes ELSE 0 END), 0),
                COALESCE(SUM(CASE
                    WHEN kind IN ('macos-app-bundle', 'app-support-data', 'app-container',
                                  'app-launch-item', 'app-preferences', 'app-receipt',
                                  'ai-session-data', 'offline-media', 'colima-vm',
                                  'docker-vm', 'ios-backup', 'mail-attachments',
                                  'message-attachments', 'local-snapshot')
                      OR storage_role IN ('application', 'app-data', 'agent-data',
                                          'offline-media', 'system-data', 'user-data')
                      OR cleanup_tier IN ('blocked', 'dangerous')
                      OR safety IN ('blocked', 'dangerous')
                    THEN physical_bytes ELSE 0 END), 0),
                COALESCE(MIN(last_scan_millis), 0),
                COALESCE(MAX(last_scan_millis), 0),
                COALESCE(MAX(truncated), 0)
         FROM storage_file_index
         WHERE source_root = ?1",
        params![source_root],
        materialized_storage_domain_aggregate_from_sql,
    )?;
    if aggregate.item_count == 0 {
        connection.execute(
            "DELETE FROM storage_domain WHERE source_root = ?1 AND source = 'storage_file_index'",
            params![source_root],
        )?;
        connection.execute(
            "DELETE FROM storage_measurement_job
             WHERE source = 'storage_file_index' AND root_key = ?1",
            params![source_root],
        )?;
        return Ok(());
    }
    connection.execute(
        "INSERT OR REPLACE INTO storage_domain (
            domain_id, label, source_root, domain_kind, path_prefix, item_count,
            directory_count, file_count, logical_bytes, physical_bytes,
            safely_reclaimable_now_bytes, maybe_reclaimable_bytes, review_required_bytes,
            dangerous_user_data_bytes, last_measured_millis, confidence, source
         ) VALUES (
            ?1, ?2, ?3, ?4, ?3, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
            'storage_file_index'
         )",
        params![
            source_root,
            materialized_storage_domain_label(source_root),
            source_root,
            materialized_storage_domain_kind(source_root),
            aggregate.item_count.min(i64::MAX as u64) as i64,
            aggregate.directory_count.min(i64::MAX as u64) as i64,
            aggregate.file_count.min(i64::MAX as u64) as i64,
            aggregate.logical_bytes.min(i64::MAX as u64) as i64,
            aggregate.physical_bytes.min(i64::MAX as u64) as i64,
            aggregate.safely_reclaimable_now_bytes.min(i64::MAX as u64) as i64,
            aggregate.maybe_reclaimable_bytes.min(i64::MAX as u64) as i64,
            aggregate.review_required_bytes.min(i64::MAX as u64) as i64,
            aggregate.dangerous_user_data_bytes.min(i64::MAX as u64) as i64,
            aggregate.completed_at_millis.min(i64::MAX as u64) as i64,
            if aggregate.partial {
                "partial"
            } else {
                "indexed"
            },
        ],
    )?;
    let roots_json =
        serde_json::to_string(&vec![source_root.to_owned()]).unwrap_or_else(|_| "[]".to_owned());
    connection.execute(
        "INSERT OR REPLACE INTO storage_measurement_job (
            job_id, job_kind, status, source, root_key, roots_json, dirty_paths_json,
            started_at_millis, updated_at_millis, completed_at_millis, measured_path_count,
            measured_directory_count, measured_file_count, measured_bytes, partial, last_error
         ) VALUES (
            ?1, 'legacy_file_index_backfill', 'complete', 'storage_file_index', ?2, ?3, '[]',
            ?4, ?5, ?5, ?6, ?7, ?8, ?9, ?10, NULL
         )",
        params![
            format!("legacy-file-index:{}", source_root),
            source_root,
            roots_json,
            aggregate.started_at_millis.min(i64::MAX as u64) as i64,
            aggregate.completed_at_millis.min(i64::MAX as u64) as i64,
            aggregate.item_count.min(i64::MAX as u64) as i64,
            aggregate.directory_count.min(i64::MAX as u64) as i64,
            aggregate.file_count.min(i64::MAX as u64) as i64,
            aggregate.physical_bytes.min(i64::MAX as u64) as i64,
            if aggregate.partial { 1i64 } else { 0i64 },
        ],
    )?;
    Ok(())
}

fn materialized_storage_domain_aggregate_from_sql(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MaterializedStorageDomainAggregate> {
    let item_count: i64 = row.get(0)?;
    let directory_count: i64 = row.get(1)?;
    let file_count: i64 = row.get(2)?;
    let logical_bytes: i64 = row.get(3)?;
    let physical_bytes: i64 = row.get(4)?;
    let safely_reclaimable_now_bytes: i64 = row.get(5)?;
    let maybe_reclaimable_bytes: i64 = row.get(6)?;
    let review_required_bytes: i64 = row.get(7)?;
    let dangerous_user_data_bytes: i64 = row.get(8)?;
    let started_at_millis: i64 = row.get(9)?;
    let completed_at_millis: i64 = row.get(10)?;
    Ok(MaterializedStorageDomainAggregate {
        item_count: item_count.max(0) as u64,
        directory_count: directory_count.max(0) as u64,
        file_count: file_count.max(0) as u64,
        logical_bytes: logical_bytes.max(0) as u64,
        physical_bytes: physical_bytes.max(0) as u64,
        safely_reclaimable_now_bytes: safely_reclaimable_now_bytes.max(0) as u64,
        maybe_reclaimable_bytes: maybe_reclaimable_bytes.max(0) as u64,
        review_required_bytes: review_required_bytes.max(0) as u64,
        dangerous_user_data_bytes: dangerous_user_data_bytes.max(0) as u64,
        started_at_millis: started_at_millis.max(0) as u64,
        completed_at_millis: completed_at_millis.max(0) as u64,
        partial: row.get::<_, i64>(11)? != 0,
    })
}

fn materialized_storage_path_row_from_sql(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MaterializedStoragePathRow> {
    let path: String = row.get(0)?;
    let is_directory = row.get::<_, i64>(16)? != 0;
    let (parent_path, name) = materialized_storage_path_parent_and_name(&path);
    let logical_bytes: i64 = row.get(10)?;
    let physical_bytes: i64 = row.get(11)?;
    let entries: i64 = row.get(17)?;
    let truncated = row.get::<_, i64>(18)? != 0;
    let last_measured_millis: i64 = row.get(19)?;
    Ok(MaterializedStoragePathRow {
        path,
        parent_path,
        name,
        device: row.get(1)?,
        inode: row.get(2)?,
        file_id: row.get(3)?,
        source_root: row.get(4)?,
        repo_root: row.get(5)?,
        path_kind: if is_directory {
            "directory".to_owned()
        } else {
            "file".to_owned()
        },
        artifact_kind: row.get(6)?,
        storage_role: row.get(7)?,
        safety: row.get(8)?,
        cleanup_tier: row.get(9)?,
        logical_bytes: logical_bytes.max(0) as u64,
        physical_bytes: physical_bytes.max(0) as u64,
        modified_millis: row
            .get::<_, Option<i64>>(12)?
            .map(|value| value.max(0) as u64),
        changed_millis: row
            .get::<_, Option<i64>>(13)?
            .map(|value| value.max(0) as u64),
        accessed_millis: row
            .get::<_, Option<i64>>(14)?
            .map(|value| value.max(0) as u64),
        birth_millis: row
            .get::<_, Option<i64>>(15)?
            .map(|value| value.max(0) as u64),
        entries: entries.max(0) as u64,
        truncated,
        recommendation_score: row.get(20)?,
        last_measured_millis: last_measured_millis.max(0) as u64,
    })
}

fn materialized_storage_path_parent_and_name(path: &str) -> (String, String) {
    let path = Path::new(path);
    let parent_path = path
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path.to_str().unwrap_or_default())
        .to_owned();
    (parent_path, name)
}

fn materialized_storage_domain_label(source_root: &str) -> String {
    Path::new(source_root)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(source_root)
        .to_owned()
}

fn materialized_storage_domain_kind(source_root: &str) -> &'static str {
    let lower = source_root.to_ascii_lowercase();
    if lower.contains("/library/developer") || lower.contains("/deriveddata") {
        "developer"
    } else if lower.contains("/.colima") || lower.contains("/docker") {
        "container"
    } else if lower.contains("/downloads") {
        "downloads"
    } else if lower.contains("/pictures") || lower.contains("/movies") {
        "media"
    } else {
        "source_root"
    }
}

fn table_count(connection: &Connection, table: &str) -> u64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count.max(0) as u64)
        .unwrap_or_default()
}

fn prune_storage_file_index_rows(
    connection: &Connection,
    max_rows: u64,
    delete_limit: u64,
) -> bool {
    let count = table_count(connection, "storage_file_index");
    if count <= max_rows {
        return false;
    }
    evict_storage_file_index_rows(connection, count.saturating_sub(max_rows).min(delete_limit))
}

fn evict_storage_file_index_rows(connection: &Connection, delete_limit: u64) -> bool {
    if delete_limit == 0 {
        return false;
    }
    connection
        .execute(
            "DELETE FROM storage_file_index
             WHERE rowid IN (
                SELECT rowid
                FROM storage_file_index
                ORDER BY
                    CASE
                        WHEN cleanup_tier IN ('safe', 'rebuildable', 'review')
                          OR physical_bytes >= ?2
                          OR recommendation_score > 0
                        THEN 1 ELSE 0 END ASC,
                    recommendation_score ASC,
                    physical_bytes ASC,
                    last_scan_millis ASC,
                    path ASC
                LIMIT ?1
             )",
            params![
                delete_limit.min(i64::MAX as u64) as i64,
                LARGE_FILE_BYTES.min(i64::MAX as u64) as i64,
            ],
        )
        .map(|deleted| deleted > 0)
        .unwrap_or(false)
}

fn prune_storage_size_index_rows(
    connection: &Connection,
    max_rows: u64,
    delete_limit: u64,
) -> bool {
    let count = table_count(connection, "storage_size_index");
    if count <= max_rows {
        return false;
    }
    evict_storage_size_index_rows(connection, count.saturating_sub(max_rows).min(delete_limit))
}

fn evict_storage_size_index_rows(connection: &Connection, delete_limit: u64) -> bool {
    if delete_limit == 0 {
        return false;
    }
    connection
        .execute(
            "DELETE FROM storage_size_index
             WHERE rowid IN (
                SELECT rowid
                FROM storage_size_index
                ORDER BY allocated_bytes ASC, size_bytes ASC, last_scan_millis ASC, path ASC
                LIMIT ?1
             )",
            params![delete_limit.min(i64::MAX as u64) as i64],
        )
        .map(|deleted| deleted > 0)
        .unwrap_or(false)
}

fn prune_storage_growth_delta_rows(
    connection: &Connection,
    max_rows: u64,
    delete_limit: u64,
) -> bool {
    let count = table_count(connection, "storage_growth_delta");
    if count <= max_rows {
        return false;
    }
    evict_storage_growth_delta_rows(connection, count.saturating_sub(max_rows).min(delete_limit))
}

fn evict_storage_growth_delta_rows(connection: &Connection, delete_limit: u64) -> bool {
    if delete_limit == 0 {
        return false;
    }
    connection
        .execute(
            "DELETE FROM storage_growth_delta
             WHERE id IN (
                SELECT id
                FROM storage_growth_delta
                ORDER BY ABS(delta_bytes) ASC, scan_millis ASC, id ASC
                LIMIT ?1
             )",
            params![delete_limit.min(i64::MAX as u64) as i64],
        )
        .map(|deleted| deleted > 0)
        .unwrap_or(false)
}

#[derive(Clone, Debug)]
struct GrowthAnomalyCandidate {
    bucket_millis: u64,
    scan_millis: u64,
    path: String,
    source_root: String,
    repo_root: Option<String>,
    kind: String,
    cleanup_tier: String,
    delta_bytes: u64,
}

#[derive(Clone, Debug, Default)]
struct GrowthBaseline {
    count: u64,
    mean: f64,
    stddev: f64,
    peak: u64,
}

fn growth_anomaly_for_candidate(
    candidate: GrowthAnomalyCandidate,
    baseline: GrowthBaseline,
    window_days: u64,
) -> Option<StorageGrowthAnomaly> {
    let current = candidate.delta_bytes;
    let baseline_mean = baseline.mean.max(0.0);
    let ratio = if baseline_mean >= 1.0 {
        current as f64 / baseline_mean
    } else if baseline.peak > 0 {
        current as f64 / baseline.peak as f64
    } else {
        0.0
    };
    let z_score = if baseline.stddev >= 1.0 {
        ((current as f64 - baseline_mean) / baseline.stddev).max(0.0)
    } else if baseline_mean >= 1.0 {
        ((current as f64 - baseline_mean) / baseline_mean.max(1.0)).max(0.0)
    } else {
        0.0
    };

    let (anomaly_kind, confidence) = if baseline.count
        >= STORAGE_GROWTH_ANOMALY_MIN_BASELINE_BUCKETS
    {
        let statistical_threshold = baseline_mean
            + (baseline.stddev * 3.0).max((baseline_mean * 2.0).max(MIN_ITEM_BYTES as f64));
        let peak_threshold = (baseline.peak as f64 * 2.5).ceil() as u64;
        let threshold = STORAGE_GROWTH_ANOMALY_MIN_DELTA_BYTES
            .max(statistical_threshold.ceil() as u64)
            .max(peak_threshold);
        if current < threshold {
            return None;
        }
        (
            "baseline-spike",
            if baseline.count >= 7 {
                "high"
            } else {
                "medium"
            },
        )
    } else if baseline.count == 0 && current >= STORAGE_GROWTH_ANOMALY_NEW_PATH_BYTES {
        ("new-large-growth", "low")
    } else if baseline.count > 0 && current >= STORAGE_GROWTH_ANOMALY_NEW_PATH_BYTES && ratio >= 8.0
    {
        ("thin-baseline-spike", "low")
    } else {
        return None;
    };

    let severity = if current >= 1_024 * 1_024 * 1_024 || ratio >= 8.0 || z_score >= 8.0 {
        "critical"
    } else {
        "warning"
    };
    let repo_name = candidate.repo_root.as_deref().and_then(|repo| {
        Path::new(repo)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    });
    let baseline_mean_bytes = baseline_mean.round().max(0.0) as u64;
    let baseline_stddev_bytes = baseline.stddev.round().max(0.0) as u64;
    let current_to_baseline_ratio = round_one_decimal(ratio);
    let z_score = round_one_decimal(z_score);
    let summary = match anomaly_kind {
        "baseline-spike" => format!(
            "{} grew by {}, which is {}x its {}-bucket baseline average.",
            diff_display_name(&candidate.path),
            human_bytes(current),
            current_to_baseline_ratio,
            baseline.count
        ),
        "thin-baseline-spike" => format!(
            "{} grew by {} with only {} prior baseline bucket{}; treat as suspicious but low-confidence.",
            diff_display_name(&candidate.path),
            human_bytes(current),
            baseline.count,
            if baseline.count == 1 { "" } else { "s" }
        ),
        _ => format!(
            "{} is new or baseline-free and appeared with {} of growth.",
            diff_display_name(&candidate.path),
            human_bytes(current)
        ),
    };
    let evidence = vec![
        format!("Current latest-scan growth: {}.", human_bytes(current)),
        format!(
            "Baseline over {window_days}d: {} bucket{}, mean {}, stddev {}, peak {}.",
            baseline.count,
            if baseline.count == 1 { "" } else { "s" },
            human_bytes(baseline_mean_bytes),
            human_bytes(baseline_stddev_bytes),
            human_bytes(baseline.peak)
        ),
        format!("Anomaly score: ratio {current_to_baseline_ratio}x, z-score {z_score}."),
    ];

    Some(StorageGrowthAnomaly {
        path: candidate.path.clone(),
        display_name: diff_display_name(&candidate.path),
        source_root: candidate.source_root,
        repo_root: candidate.repo_root,
        repo_name,
        kind: candidate.kind,
        cleanup_tier: candidate.cleanup_tier,
        bucket_millis: candidate.bucket_millis,
        scan_millis: candidate.scan_millis,
        current_delta_bytes: current,
        baseline_mean_bytes,
        baseline_stddev_bytes,
        baseline_peak_bytes: baseline.peak,
        baseline_bucket_count: baseline.count,
        current_to_baseline_ratio,
        z_score,
        severity: severity.to_owned(),
        confidence: confidence.to_owned(),
        anomaly_kind: anomaly_kind.to_owned(),
        summary,
        evidence,
    })
}

fn round_one_decimal(value: f64) -> f64 {
    if value.is_finite() {
        (value * 10.0).round() / 10.0
    } else {
        0.0
    }
}

fn anomaly_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 2,
        "warning" => 1,
        _ => 0,
    }
}

/// Two connections can race the pragma check in a guarded `ALTER TABLE ... ADD
/// COLUMN` migration; the loser's error is benign and must not disable the
/// index.
fn tolerate_duplicate_column(result: rusqlite::Result<usize>) -> rusqlite::Result<()> {
    match result {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("duplicate column name") => Ok(()),
        Err(error) => Err(error),
    }
}

/// Append the roots-scoping clause used across index queries: a column value
/// matches when it equals a root or lives strictly under it, mirroring
/// `path_is_under_root`.
fn push_roots_predicate(
    predicate: &mut String,
    bindings: &mut Vec<rusqlite::types::Value>,
    roots: &[PathBuf],
    column: &str,
) {
    if roots.is_empty() {
        return;
    }
    let mut clauses = Vec::with_capacity(roots.len().min(MAX_ROOTS));
    for root in roots.iter().take(MAX_ROOTS) {
        let root_display = root.display().to_string();
        clauses.push(format!("({column} = ? OR {column} LIKE ? ESCAPE '\\')"));
        bindings.push(root_display.clone().into());
        bindings.push(format!("{}/%", escape_like_pattern(&root_display)).into());
    }
    predicate.push_str(&format!(" AND ({})", clauses.join(" OR ")));
}

fn push_volume_predicate(
    predicate: &mut String,
    bindings: &mut Vec<rusqlite::types::Value>,
    volume_path: Option<&str>,
    column: &str,
) {
    let Some(volume_path) = volume_path else {
        return;
    };
    if volume_path.is_empty() || volume_path == "/" {
        return;
    }
    let volume = PathBuf::from(volume_path);
    push_roots_predicate(predicate, bindings, &[volume], column);
}

/// Half-window trend classification: compare the second half of the observed
/// span against the first, with a 10%-of-total dead band so near-equal halves
/// read as steady.
fn growth_trend(total_delta: i64, second_half_delta: i64) -> String {
    if total_delta < 0 {
        return "shrinking".to_owned();
    }
    let first_half_delta = total_delta.saturating_sub(second_half_delta);
    let threshold = (total_delta.saturating_abs() / 10).max(1);
    if second_half_delta.saturating_sub(first_half_delta) > threshold {
        "accelerating".to_owned()
    } else if first_half_delta.saturating_sub(second_half_delta) > threshold {
        "slowing".to_owned()
    } else {
        "steady".to_owned()
    }
}

#[derive(Clone, Debug, Default)]
struct GrowthForecastStats {
    total_delta_bytes: i64,
    day_bucket_count: u64,
    daily_rate_bytes: i64,
    daily_rate_lower_bytes: i64,
    daily_rate_upper_bytes: i64,
    volatility_percent: u64,
    seasonal_pattern: String,
    seasonal_peak_daily_bytes: i64,
    confidence: String,
}

fn growth_forecast_stats_from_daily_totals(daily_totals: &[(u64, i64)]) -> GrowthForecastStats {
    if daily_totals.is_empty() {
        return GrowthForecastStats {
            seasonal_pattern: "insufficient-history".to_owned(),
            confidence: "low".to_owned(),
            ..GrowthForecastStats::default()
        };
    }
    let mut totals_by_day = BTreeMap::<u64, i64>::new();
    for (day, delta) in daily_totals {
        *totals_by_day.entry(*day).or_default() += *delta;
    }
    let first_day = totals_by_day.keys().next().copied().unwrap_or_default();
    let last_day = totals_by_day
        .keys()
        .next_back()
        .copied()
        .unwrap_or(first_day);
    let dense = (first_day..=last_day)
        .map(|day| (day, *totals_by_day.get(&day).unwrap_or(&0)))
        .collect::<Vec<_>>();
    let total_delta = dense.iter().map(|(_, delta)| *delta).sum::<i64>();
    let span_days = dense.len().max(1) as f64;
    let mean = total_delta as f64 / span_days;
    let variance = dense
        .iter()
        .map(|(_, delta)| {
            let distance = *delta as f64 - mean;
            distance * distance
        })
        .sum::<f64>()
        / span_days;
    let stddev = variance.sqrt();
    let daily_rate_bytes = mean.round() as i64;
    let daily_rate_lower_bytes = (mean - stddev).floor() as i64;
    let daily_rate_upper_bytes = (mean + stddev).ceil() as i64;
    let volatility_percent = if mean.abs() < 1.0 {
        0
    } else {
        ((stddev / mean.abs()) * 100.0).round().max(0.0) as u64
    };
    let day_bucket_count = totals_by_day.len() as u64;
    let seasonal_pattern = seasonal_pattern_for_daily_totals(&dense, volatility_percent);
    let seasonal_peak_daily_bytes = dense
        .iter()
        .map(|(_, delta)| *delta)
        .max()
        .unwrap_or_default();
    let confidence = growth_forecast_confidence(day_bucket_count, volatility_percent);
    GrowthForecastStats {
        total_delta_bytes: total_delta,
        day_bucket_count,
        daily_rate_bytes,
        daily_rate_lower_bytes,
        daily_rate_upper_bytes,
        volatility_percent,
        seasonal_pattern,
        seasonal_peak_daily_bytes,
        confidence,
    }
}

fn seasonal_pattern_for_daily_totals(dense: &[(u64, i64)], volatility_percent: u64) -> String {
    if dense.len() < 7 {
        return "insufficient-history".to_owned();
    }
    if dense.len() >= 14 {
        let mut weekly_totals = [0i64; 7];
        let mut weekly_counts = [0u64; 7];
        for (day, delta) in dense {
            let index = (*day % 7) as usize;
            weekly_totals[index] += *delta;
            weekly_counts[index] += 1;
        }
        let weekly_averages = weekly_totals
            .iter()
            .zip(weekly_counts)
            .filter_map(|(total, count)| (count > 0).then_some(*total as f64 / count as f64))
            .collect::<Vec<_>>();
        if !weekly_averages.is_empty() {
            let mean = dense.iter().map(|(_, delta)| *delta as f64).sum::<f64>()
                / dense.len().max(1) as f64;
            let peak = weekly_averages
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let trough = weekly_averages
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
            if mean > 0.0 && peak >= mean * 1.5 && peak - trough >= mean * 0.5 {
                return "weekly-peak".to_owned();
            }
        }
    }
    if volatility_percent >= 100 {
        "spiky".to_owned()
    } else if volatility_percent >= 50 {
        "variable".to_owned()
    } else {
        "steady".to_owned()
    }
}

fn growth_forecast_confidence(day_bucket_count: u64, volatility_percent: u64) -> String {
    if day_bucket_count >= 14 && volatility_percent <= 75 {
        "high".to_owned()
    } else if day_bucket_count >= 7 {
        "medium".to_owned()
    } else {
        "low".to_owned()
    }
}

fn days_until_capacity_full(capacity_bytes: u64, daily_rate_bytes: i64) -> f64 {
    if capacity_bytes == 0 || daily_rate_bytes <= 0 {
        return 0.0;
    }
    capacity_bytes as f64 / daily_rate_bytes as f64
}

fn storage_forecast_notes(forecast: &StorageGrowthForecast) -> Vec<String> {
    let mut notes = Vec::new();
    if forecast.purgeable_bytes_estimate > 0 {
        notes.push(format!(
            "Purgeable APFS space adds about {:.1} day(s) of cushion at the current rate.",
            forecast.purgeable_cushion_days
        ));
    }
    if forecast.cloud_growth_share_percent >= 20 {
        notes.push(format!(
            "Cloud-backed paths account for {}% of observed local growth; placeholder hydration can change quickly.",
            forecast.cloud_growth_share_percent
        ));
    }
    if forecast.seasonal_pattern != "steady" {
        notes.push(format!(
            "Observed daily growth pattern is {}; use the lower/upper forecast bounds instead of a single date.",
            forecast.seasonal_pattern
        ));
    }
    if forecast.confidence == "low" {
        notes.push(
            "Forecast confidence is low until more daily growth buckets are retained.".to_owned(),
        );
    }
    if forecast.important_usage_available_bytes.is_some()
        || forecast.opportunistic_usage_available_bytes.is_some()
    {
        notes.push(
            "APFS important/opportunistic capacity is available, so free-now and effective capacity can diverge."
                .to_owned(),
        );
    }
    notes
}

fn storage_path_is_cloud(path: &str) -> bool {
    is_cloud_storage_path(Path::new(path))
}

fn diff_display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact")
        .to_owned()
}

/// Escape `%`, `_`, and the escape character itself so a filesystem path can be
/// used as a literal prefix in a `LIKE ... ESCAPE '\'` pattern. This keeps the
/// SQL root scoping identical to `path_is_under_root`.
fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn collect_unknown_gap_roots_for_event_path(
    path: &str,
    roots: &[PathBuf],
    unknown_gap_roots: &mut BTreeSet<String>,
) {
    if roots.is_empty() {
        unknown_gap_roots.insert(path.to_owned());
        return;
    }
    let event_path = Path::new(path);
    for root in roots {
        let root_display = root.display().to_string();
        if path_is_under_root(path, root) || path_is_under_root(&root_display, event_path) {
            unknown_gap_roots.insert(root_display);
        }
    }
}

fn nearest_indexed_dirty_queue_ancestor(
    connection: &Connection,
    path: &str,
    roots: &[PathBuf],
) -> Option<String> {
    dirty_queue_ancestor_candidates(path, roots)
        .into_iter()
        .find(|candidate| dirty_queue_has_indexed_directory(connection, candidate))
}

fn storage_dirty_event_path_is_ignored(path: &str) -> bool {
    let lowercase = path.to_ascii_lowercase();
    let file_name = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if file_name == ".ds_store" {
        return true;
    }
    if lowercase.contains("/library/application support/aetower/")
        || lowercase.contains("/library/caches/aetower/")
    {
        return true;
    }
    if lowercase.contains("/library/metadata/corespotlight/")
        || lowercase.contains("/library/biome/")
        || lowercase.contains("/library/duetexpertcenter/")
    {
        return true;
    }
    if lowercase.contains("/library/preferences/") && file_name.ends_with(".plist") {
        return true;
    }
    if lowercase.contains("/library/applemediaservices/")
        && (file_name.ends_with("-wal")
            || file_name.ends_with("-shm")
            || file_name == "cookies.sqlitedb"
            || file_name.ends_with(".sqlitedb-wal")
            || file_name.ends_with(".sqlitedb-shm"))
    {
        return true;
    }
    false
}

fn volatile_dirty_queue_ancestor(path: &str) -> Option<String> {
    const VOLATILE_DIR_MARKERS: [&str; 6] = [
        "/Library/Application Support/Chau7/TabRestoreBundles",
        "/Library/Application Support/Chau7/TabStateBackups",
        "/Library/Application Support/Google/Chrome",
        "/Library/Caches/Google/Chrome",
        "/.claude",
        "/.codex",
    ];
    let lowercase = path.to_ascii_lowercase();
    if let Some(index) = lowercase.find("/.git/") {
        return Some(path[..index].to_owned());
    }
    if let Some(index) = lowercase.find("/node_modules/") {
        let end = index.saturating_add("/node_modules".len()).min(path.len());
        return Some(path[..end].to_owned());
    }
    for marker in VOLATILE_DIR_MARKERS {
        let marker_lowercase = marker.to_ascii_lowercase();
        if let Some(index) = lowercase.find(&marker_lowercase) {
            let end = index.saturating_add(marker.len()).min(path.len());
            return Some(path[..end].to_owned());
        }
    }
    if lowercase.contains(".dat.nosync") {
        return Path::new(path)
            .parent()
            .map(|parent| parent.display().to_string());
    }
    None
}

fn dirty_queue_ancestor_candidates(path: &str, roots: &[PathBuf]) -> Vec<String> {
    let path = Path::new(path);
    let mut candidates = Vec::new();
    for ancestor in path.ancestors() {
        let candidate = ancestor.display().to_string();
        if candidate.is_empty() || candidate == "." || candidate == "/" {
            continue;
        }
        if !roots.is_empty()
            && !roots
                .iter()
                .any(|root| path_is_under_root(&candidate, root))
        {
            continue;
        }
        candidates.push(candidate);
    }
    candidates
}

fn dirty_queue_has_indexed_directory(connection: &Connection, path: &str) -> bool {
    connection
        .query_row(
            "SELECT 1
             WHERE EXISTS (
                SELECT 1 FROM storage_directory_rollup WHERE path = ?1
             )
             OR EXISTS (
                SELECT 1 FROM storage_path WHERE path = ?1 AND path_kind = 'directory'
             )
             OR EXISTS (
                SELECT 1 FROM storage_size_index
                WHERE path = ?1 AND kind <> 'indexed-file'
             )",
            params![path],
            |row| row.get::<_, i64>(0),
        )
        .is_ok()
}

fn dirty_path_record_is_debounced(record: &StorageDirtyPathRecord, now_millis: u64) -> bool {
    record.event_count >= STORAGE_DIRTY_QUEUE_NOISY_EVENT_COUNT
        && record
            .last_seen_millis
            .saturating_sub(record.first_seen_millis)
            <= STORAGE_DIRTY_QUEUE_NOISY_WINDOW_MILLIS
        && now_millis.saturating_sub(record.last_seen_millis) < STORAGE_DIRTY_QUEUE_DEBOUNCE_MILLIS
}

fn dirty_queue_domain_kind(connection: &Connection, path: &str) -> Option<String> {
    connection
        .query_row(
            "SELECT domain_kind
             FROM storage_domain
             WHERE ?1 = path_prefix
                OR substr(?1, 1, length(path_prefix) + 1) = path_prefix || '/'
             ORDER BY length(path_prefix) DESC
             LIMIT 1",
            params![path],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

fn dirty_queue_previous_physical_bytes(connection: &Connection, path: &str) -> u64 {
    connection
        .query_row(
            "SELECT MAX(physical_bytes)
             FROM (
                SELECT physical_bytes FROM storage_directory_rollup WHERE path = ?1
                UNION ALL
                SELECT physical_bytes FROM storage_path WHERE path = ?1
                UNION ALL
                SELECT allocated_bytes AS physical_bytes FROM storage_size_index WHERE path = ?1
             )",
            params![path],
            |row| row.get::<_, Option<i64>>(0),
        )
        .ok()
        .flatten()
        .unwrap_or_default()
        .max(0) as u64
}

fn dirty_queue_size_score(bytes: u64) -> f64 {
    if bytes == 0 {
        return 0.0;
    }
    (bytes as f64).log2().min(46.0) * 8.0
}

fn dirty_queue_domain_score(path: &str, domain_kind: Option<&str>) -> f64 {
    let lower = path.to_ascii_lowercase();
    if lower.contains("/library/developer/")
        || lower.contains("/deriveddata")
        || lower.contains("/devicesupport")
        || lower.contains("/.colima/")
        || lower.contains("/docker")
        || lower.contains("/target/")
        || lower.ends_with("/target")
        || lower.contains("/.build/")
        || lower.contains("/.cargo/")
        || lower.contains("/.swiftpm/")
        || lower.contains("/.codex/")
        || lower.contains("/.claude/")
    {
        return 520.0;
    }
    match domain_kind {
        Some("developer") | Some("container") => 460.0,
        Some("downloads") => 260.0,
        Some("media") => 180.0,
        Some("source_root") => 120.0,
        Some(_) => 100.0,
        None => 0.0,
    }
}

fn dirty_queue_root_has_dirty_descendant(connection: &Connection, root: &str) -> bool {
    let child_prefix = format!("{root}/");
    connection
        .query_row(
            "SELECT 1
             FROM storage_dirty_path
             WHERE status = 'dirty'
               AND path <> ?1
               AND substr(path, 1, ?2) = ?3
             LIMIT 1",
            params![
                root,
                child_prefix.len().min(i64::MAX as usize) as i64,
                child_prefix
            ],
            |row| row.get::<_, i64>(0),
        )
        .is_ok()
}

fn indexed_source_root_for_path(connection: &Connection, path: &str) -> Option<String> {
    connection
        .query_row(
            "SELECT source_root
             FROM storage_file_index
             WHERE ?1 = source_root
                OR substr(?1, 1, length(source_root) + 1) = source_root || '/'
                OR path = ?1
                OR substr(path, 1, length(?1) + 1) = ?1 || '/'
             GROUP BY source_root
             ORDER BY
                CASE
                    WHEN ?1 = source_root
                      OR substr(?1, 1, length(source_root) + 1) = source_root || '/'
                    THEN 0 ELSE 1
                END,
                length(source_root) DESC
             LIMIT 1",
            params![path],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

fn load_indexed_source_roots_for_subtree(
    connection: &Connection,
    path: &str,
    source_roots: &mut BTreeSet<String>,
) {
    let child_prefix = format!("{path}/");
    let Ok(mut statement) = connection.prepare(
        "SELECT DISTINCT source_root
         FROM storage_file_index
         WHERE path = ?1 OR substr(path, 1, ?2) = ?3",
    ) else {
        return;
    };
    let Ok(rows) = statement.query_map(
        params![
            path,
            child_prefix.len().min(i64::MAX as usize) as i64,
            child_prefix
        ],
        |row| row.get::<_, String>(0),
    ) else {
        return;
    };
    source_roots.extend(rows.flatten());
}

fn indexed_file_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageIndexedFileRow> {
    let logical_bytes: i64 = row.get(10)?;
    let physical_bytes: i64 = row.get(11)?;
    let entries: i64 = row.get(17)?;
    let last_scan_millis: i64 = row.get(19)?;
    Ok(StorageIndexedFileRow {
        path: row.get(0)?,
        device: row.get(1)?,
        inode: row.get(2)?,
        file_id: row.get(3)?,
        source_root: row.get(4)?,
        repo_root: row.get(5)?,
        kind: row.get(6)?,
        storage_role: row.get(7)?,
        safety: row.get(8)?,
        cleanup_tier: row.get(9)?,
        logical_bytes: logical_bytes.max(0) as u64,
        physical_bytes: physical_bytes.max(0) as u64,
        modified_millis: row
            .get::<_, Option<i64>>(12)?
            .map(|value| value.max(0) as u64),
        changed_millis: row
            .get::<_, Option<i64>>(13)?
            .map(|value| value.max(0) as u64),
        accessed_millis: row
            .get::<_, Option<i64>>(14)?
            .map(|value| value.max(0) as u64),
        birth_millis: row
            .get::<_, Option<i64>>(15)?
            .map(|value| value.max(0) as u64),
        is_directory: row.get::<_, i64>(16)? != 0,
        entries: entries.max(0) as u64,
        truncated: row.get::<_, i64>(18)? != 0,
        last_scan_millis: last_scan_millis.max(0) as u64,
    })
}

fn size_walk_result_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<SizeWalkResult> {
    let size_bytes: i64 = row.get(0)?;
    let allocated_bytes: i64 = row.get(1)?;
    let entries: i64 = row.get(2)?;
    let truncated: i64 = row.get(3)?;
    Ok(SizeWalkResult {
        bytes: size_bytes.max(0) as u64,
        allocated_bytes: allocated_bytes.max(0) as u64,
        entries: entries.max(0) as u64,
        truncated: truncated != 0,
        max_hardlink_count: 1,
        has_hardlinks: false,
        sparse_or_shared: allocated_bytes > 0 && allocated_bytes < size_bytes,
        cloud_placeholder: size_bytes > 0 && allocated_bytes == 0,
    })
}

fn size_walk_cache_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageCachedSizeRow> {
    Ok(StorageCachedSizeRow {
        size: size_walk_result_from_sql(row)?,
        fingerprint: row.get(4)?,
    })
}

fn dirty_path_record_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageDirtyPathRecord> {
    let flags: i64 = row.get(2)?;
    let first_seen_millis: i64 = row.get(3)?;
    let last_seen_millis: i64 = row.get(4)?;
    let event_count: i64 = row.get(5)?;
    Ok(StorageDirtyPathRecord {
        path: row.get(0)?,
        source: row.get(1)?,
        flags: flags.max(0) as u64,
        first_seen_millis: first_seen_millis.max(0) as u64,
        last_seen_millis: last_seen_millis.max(0) as u64,
        event_count: event_count.max(0) as u64,
    })
}

fn storage_index_summary_row_from_sql(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StorageIndexSummaryRow> {
    Ok(StorageIndexSummaryRow {
        source_root: row.get(0)?,
        item_count: row.get::<_, i64>(1)?.max(0) as u64,
        inventory_size_bytes: row.get::<_, i64>(2)?.max(0) as u64,
        safe_reclaimable_bytes: row.get::<_, i64>(3)?.max(0) as u64,
        maybe_reclaimable_bytes: row.get::<_, i64>(4)?.max(0) as u64,
        review_required_bytes: row.get::<_, i64>(5)?.max(0) as u64,
        dangerous_user_data_bytes: row.get::<_, i64>(6)?.max(0) as u64,
    })
}

fn storage_top_offender_row_from_sql(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StorageTopOffenderRow> {
    Ok(StorageTopOffenderRow {
        source_root: row.get(0)?,
        path: row.get(1)?,
        kind: row.get(2)?,
        cleanup_tier: row.get(3)?,
        physical_bytes: row.get::<_, i64>(4)?.max(0) as u64,
        recommendation_score: row.get(5)?,
        last_scan_millis: row.get::<_, i64>(6)?.max(0) as u64,
    })
}

fn storage_situation_domain_from_sql(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StorageSituationDomain> {
    Ok(StorageSituationDomain {
        domain_id: row.get(0)?,
        label: row.get(1)?,
        source_root: row.get(2)?,
        domain_kind: row.get(3)?,
        path_prefix: row.get(4)?,
        item_count: row.get::<_, i64>(5)?.max(0) as u64,
        directory_count: row.get::<_, i64>(6)?.max(0) as u64,
        file_count: row.get::<_, i64>(7)?.max(0) as u64,
        logical_bytes: row.get::<_, i64>(8)?.max(0) as u64,
        physical_bytes: row.get::<_, i64>(9)?.max(0) as u64,
        safely_reclaimable_now_bytes: row.get::<_, i64>(10)?.max(0) as u64,
        maybe_reclaimable_bytes: row.get::<_, i64>(11)?.max(0) as u64,
        review_required_bytes: row.get::<_, i64>(12)?.max(0) as u64,
        dangerous_user_data_bytes: row.get::<_, i64>(13)?.max(0) as u64,
        last_measured_millis: row.get::<_, i64>(14)?.max(0) as u64,
        confidence: row.get(15)?,
        source: row.get(16)?,
    })
}

fn storage_situation_roots_key(roots: &[PathBuf]) -> String {
    let mut values = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    values.sort();
    values.join("\u{1f}")
}

fn load_situation_snapshot_for_key(
    connection: &Connection,
    root_key: &str,
    limit: usize,
) -> Option<StorageSituationResponse> {
    let snapshot_json = connection
        .query_row(
            "SELECT snapshot_json
             FROM storage_situation_snapshot
             WHERE root_key = ?1",
            params![root_key],
            |row| row.get::<_, String>(0),
        )
        .ok()?;
    decode_situation_snapshot(&snapshot_json, limit)
}

fn decode_situation_snapshot(
    snapshot_json: &str,
    limit: usize,
) -> Option<StorageSituationResponse> {
    let mut snapshot = serde_json::from_str::<StorageSituationResponse>(snapshot_json).ok()?;
    snapshot.top_offenders.truncate(limit.clamp(1, 40));
    Some(snapshot)
}

fn indexed_row_matches_live_metadata(row: &StorageIndexedFileRow) -> bool {
    let Ok(metadata) = fs::symlink_metadata(&row.path) else {
        return false;
    };
    if metadata.dev() as i64 != row.device || metadata.ino() as i64 != row.inode {
        return false;
    }
    true
}
