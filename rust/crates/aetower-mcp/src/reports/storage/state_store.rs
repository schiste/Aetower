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
fn storage_index_directory() -> Option<PathBuf> {
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
        let min_updated_millis =
            storage_now_millis().saturating_sub(STORAGE_SCAN_STATE_MAX_AGE_MILLIS);
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

#[derive(Clone, Debug, Default)]
pub(super) struct StorageItemRowsPage {
    pub(super) rows: Vec<StorageIndexedFileRow>,
    pub(super) total_available: u64,
}

pub(super) struct StorageSizeIndex {
    connection: Option<Connection>,
    pub(super) status: String,
    /// Rows buffered by `store_indexed_row` and written in chunked
    /// transactions by `flush_pending_rows`. The walk is single-threaded per
    /// index instance, so interior mutability with `RefCell` is sufficient.
    pending_rows: RefCell<Vec<StorageIndexedFileRow>>,
}

impl Drop for StorageSizeIndex {
    fn drop(&mut self) {
        // End-of-scan / cancellation safety net: whatever is still buffered
        // must reach the database before the connection closes.
        self.flush_pending_rows();
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
    fn with_status(connection: Option<Connection>, status: String) -> Self {
        Self {
            connection,
            status,
            pending_rows: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn open() -> Self {
        let Some(directory) = storage_index_directory() else {
            return Self::with_status(None, "unavailable:no_data_dir".to_owned());
        };
        if let Err(error) = fs::create_dir_all(&directory) {
            return Self::with_status(None, format!("unavailable:create_dir:{error}"));
        }
        let path = directory.join(STORAGE_INDEX_FILE_NAME);
        let Ok(connection) = Connection::open(path) else {
            return Self::with_status(None, "unavailable:open_failed".to_owned());
        };
        if let Err(error) = Self::prepare_schema(&connection) {
            return Self::with_status(None, format!("unavailable:schema:{error}"));
        }
        // Index reads/writes tolerate short writer contention instead of
        // silently dropping rows. The scan-job state store deliberately keeps
        // the default fail-fast behavior so cancel/pause stay responsive.
        let _ = connection.busy_timeout(Duration::from_millis(2_000));
        Self::with_status(Some(connection), "ready".to_owned())
    }

    pub(super) fn disabled(reason: &str) -> Self {
        Self::with_status(None, format!("disabled:{reason}"))
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
                ON storage_scan_job_state(status, updated_at_millis DESC);",
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
                    ON storage_scan_job_state(status, updated_at_millis DESC);",
            )?;
        }
        Self::ensure_repository_inventory_cache_columns(connection)?;
        Self::ensure_storage_file_index_columns(connection)?;
        Self::ensure_storage_file_index_page_indexes(connection)?;
        Ok(())
    }

    /// Additive migration (no `schema_version` bump, mirroring
    /// `ensure_repository_inventory_cache_columns`): records the cleanup tier
    /// a row had before its latest upsert so tier transitions can be surfaced
    /// without re-touching the batched write path later.
    fn ensure_storage_file_index_columns(connection: &Connection) -> rusqlite::Result<()> {
        let exists: i64 = connection.query_row(
            "SELECT COUNT(*)
             FROM pragma_table_info('storage_file_index')
             WHERE name = 'previous_cleanup_tier'",
            [],
            |row| row.get(0),
        )?;
        if exists == 0 {
            tolerate_duplicate_column(connection.execute(
                "ALTER TABLE storage_file_index
                 ADD COLUMN previous_cleanup_tier TEXT NOT NULL DEFAULT ''",
                [],
            ))?;
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
                WHERE cleanup_tier <> '';",
        )
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
        let path = path.display().to_string();
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
                params![path, device, inode, modified_millis, changed_millis, kind],
                |row| {
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
                },
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
                    path,
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
        }
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
    /// stored twice in one chunk compares against the earlier occurrence. The
    /// growth-delta retention DELETE runs once per flush instead of once per
    /// changed file. Failures are tolerated (best effort), matching the old
    /// `.is_ok()` behavior.
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
        {
            let Ok(mut upsert) = transaction.prepare(
                "INSERT OR REPLACE INTO storage_file_index (
                    path, device, inode, file_id, source_root, repo_root, kind, storage_role,
                    safety, cleanup_tier, logical_bytes, physical_bytes, modified_millis,
                    changed_millis, accessed_millis, birth_millis, is_directory, entries,
                    truncated, last_scan_millis, previous_cleanup_tier
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                    ?17, ?18, ?19, ?20, ?21
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
            for row in &rows {
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
                    }
                }
                previous.insert(
                    row.path.clone(),
                    (row.physical_bytes, row.cleanup_tier.clone()),
                );
            }
            if max_scan_millis > 0 {
                let retention_before = max_scan_millis
                    .saturating_sub(STORAGE_GROWTH_RETENTION_MILLIS)
                    .min(i64::MAX as u64) as i64;
                let _ = transaction.execute(
                    "DELETE FROM storage_growth_delta WHERE scan_millis < ?1",
                    params![retention_before],
                );
            }
        }
        let _ = transaction.commit();
    }

    pub(super) fn load_candidate_rows(
        &self,
        roots: &[PathBuf],
        limit: usize,
        metrics: &mut StorageScanMetrics,
    ) -> Result<Vec<StorageIndexedFileRow>, String> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Err(self.status.clone());
        };
        let read_limit = limit
            .saturating_mul(STORAGE_INDEX_SNAPSHOT_READ_MULTIPLIER)
            .clamp(1, 5_000);
        let mut statement = connection
            .prepare(
                "SELECT path, device, inode, file_id, source_root, repo_root, kind,
                        storage_role, safety, cleanup_tier, logical_bytes, physical_bytes,
                        modified_millis, changed_millis, accessed_millis, birth_millis,
                        is_directory, entries, truncated, last_scan_millis
                 FROM storage_file_index
                 WHERE cleanup_tier <> ''
                   AND physical_bytes >= ?1
                 ORDER BY physical_bytes DESC, path ASC
                 LIMIT ?2",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params![
                    MIN_ITEM_BYTES.min(i64::MAX as u64) as i64,
                    read_limit as i64
                ],
                indexed_file_row_from_sql,
            )
            .map_err(|error| error.to_string())?;
        let mut retained = Vec::new();
        for row in rows.flatten() {
            if !Path::new(&row.path).exists() {
                continue;
            }
            if roots.is_empty() || roots.iter().any(|root| path_is_under_root(&row.path, root)) {
                retained.push(row);
                metrics.storage_index_hits = metrics.storage_index_hits.saturating_add(1);
                if retained.len() >= limit {
                    break;
                }
            }
        }
        if retained.is_empty() {
            metrics.storage_index_misses = metrics.storage_index_misses.saturating_add(1);
        }
        Ok(retained)
    }

    /// Serve one page of cleanup-classified rows directly from
    /// `storage_file_index`, mirroring the candidate predicate used by
    /// `load_candidate_rows` (`cleanup_tier <> ''` and the minimum item size)
    /// and the ordering semantics of `sort_storage_items`. Rows whose paths no
    /// longer exist on disk are evicted from the index in one statement and the
    /// page is refilled once.
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
        let missing = page
            .rows
            .iter()
            .filter(|row| fs::symlink_metadata(&row.path).is_err())
            .map(|row| row.path.clone())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.delete_indexed_rows(&missing)?;
            page = self.query_item_rows_page(roots, sort_key, sort_descending, offset, limit)?;
        }
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
        let mut predicate = "cleanup_tier <> '' AND physical_bytes >= ?".to_owned();
        let mut bindings: Vec<rusqlite::types::Value> =
            vec![(MIN_ITEM_BYTES.min(i64::MAX as u64) as i64).into()];
        if !roots.is_empty() {
            let mut root_clauses = Vec::with_capacity(roots.len());
            for root in roots.iter().take(MAX_ROOTS) {
                let root_display = root.display().to_string();
                root_clauses.push("(path = ? OR path LIKE ? ESCAPE '\\')".to_owned());
                bindings.push(root_display.clone().into());
                bindings.push(format!("{}/%", escape_like_pattern(&root_display)).into());
            }
            predicate.push_str(&format!(" AND ({})", root_clauses.join(" OR ")));
        }
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
    }

    pub(super) fn load_growth_deltas(&self, limit: usize) -> Vec<StorageGrowthDelta> {
        self.flush_pending_rows();
        let Some(connection) = self.connection.as_ref() else {
            return Vec::new();
        };
        let writer_ledger = load_storage_writer_ledger_records();
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
                matched_writer_count: 0,
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
                let attribution = attribute_storage_growth_delta(&delta, &writer_ledger);
                delta.repo_name = attribution.repo_name;
                delta.git_branch = attribution.git_branch;
                delta.git_head = attribution.git_head;
                delta.command = attribution.command;
                delta.process_tree = attribution.process_tree;
                delta.ai_agent_session = attribution.ai_agent_session;
                delta.writer_source = attribution.writer_source;
                delta.matched_writer_count = attribution.matched_writer_count;
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

    #[cfg(test)]
    pub(super) fn pending_row_count(&self) -> usize {
        self.pending_rows.borrow().len()
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

/// Escape `%`, `_`, and the escape character itself so a filesystem path can be
/// used as a literal prefix in a `LIKE ... ESCAPE '\'` pattern. This keeps the
/// SQL root scoping identical to `path_is_under_root`.
fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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
