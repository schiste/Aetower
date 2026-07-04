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
        // Refresh the query-planner statistics when table sizes changed enough
        // to matter (SQLite's own growth heuristic); a no-op otherwise. Stale
        // or missing statistics make the planner fall back to full scans and
        // per-row b-tree seeks for the report aggregation queries, which is
        // catastrophic once a deep scan grows the index to hundreds of
        // thousands of rows.
        if let Some(connection) = self.connection.as_ref() {
            let _ = connection.execute_batch("PRAGMA optimize;");
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
        // Best effort (a concurrent writer may hold the lock; the next open
        // retries): without `sqlite_stat1` the planner picks full-scan and
        // per-row rowid-seek plans for every report aggregation query, which
        // turned the instant_cached report path into a multi-minute burn once
        // the index reached ~350k rows.
        let _ = Self::ensure_query_planner_statistics(&connection);
        Self::with_status(Some(connection), "ready".to_owned())
    }

    /// Connection-less handle whose reads and writes all no-op. Production
    /// code now opens the index for every scan mode; tests use this to
    /// exercise the unavailable-index paths.
    #[cfg(test)]
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
        Self::ensure_storage_growth_delta_indexes(connection)?;
        Ok(())
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
        // The index content changed, so memoized report sections are stale.
        // The generation key usually changes too; this covers the
        // same-millisecond and unchanged-stamp cases.
        super::report::invalidate_index_report_sections_memo();
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
                 WHERE (cleanup_tier <> ''
                        OR (kind = 'large-directory' AND physical_bytes >= ?1))
                   AND physical_bytes >= ?2
                 ORDER BY physical_bytes DESC, path ASC
                 LIMIT ?3",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params![
                    LARGE_DIRECTORY_MIN_BYTES.min(i64::MAX as u64) as i64,
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
    /// `load_candidate_rows` (`cleanup_tier <> ''` or a review-only
    /// `large-directory` row at the large-directory threshold, plus the
    /// minimum item size)
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

    /// Aggregate the full retained `storage_growth_delta` series into growth
    /// intelligence: per-repo and per-source-root daily rates with a
    /// half-window trend, days-to-disk-full forecasts per volume, and a
    /// "since last scan" diff lane. Returns `None` when the index is
    /// unavailable. Scoped to `roots` (matching `path_is_under_root`
    /// semantics) so reports and tests stay isolated.
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
        let mut predicate =
            format!("bucket_millis >= ? AND {scope_column} IS NOT NULL AND {scope_column} <> ''");
        let mut bindings: Vec<rusqlite::types::Value> =
            vec![(window_start.min(i64::MAX as u64) as i64).into()];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT {scope_column} AS scope,
                    SUM(delta_bytes) AS total_delta,
                    COUNT(DISTINCT bucket_millis / {DAY_MILLIS}) AS day_buckets,
                    MIN(bucket_millis) AS first_bucket,
                    MAX(bucket_millis) AS last_bucket
             FROM storage_growth_delta
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
        let mut predicate =
            format!("bucket_millis >= ? AND bucket_millis > ? AND {scope_column} = ?");
        let mut bindings: Vec<rusqlite::types::Value> = vec![
            (window_start.min(i64::MAX as u64) as i64).into(),
            (midpoint_millis.min(i64::MAX as u64) as i64).into(),
            scope.to_owned().into(),
        ];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        connection
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(delta_bytes), 0)
                     FROM storage_growth_delta
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
        let mut predicate = "bucket_millis >= ?".to_owned();
        let mut bindings: Vec<rusqlite::types::Value> =
            vec![(window_start.min(i64::MAX as u64) as i64).into()];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        if let Some((scope_column, scope)) = scope_filter {
            predicate.push_str(&format!(" AND {scope_column} = ?"));
            bindings.push(scope.to_owned().into());
        }
        push_volume_predicate(&mut predicate, &mut bindings, volume_path, "path");
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT bucket_millis / {DAY_MILLIS} AS day_bucket,
                    COALESCE(SUM(delta_bytes), 0)
             FROM storage_growth_delta
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
        let mut predicate = "bucket_millis >= ?".to_owned();
        let mut bindings: Vec<rusqlite::types::Value> =
            vec![(window_start.min(i64::MAX as u64) as i64).into()];
        push_roots_predicate(&mut predicate, &mut bindings, roots, "path");
        push_volume_predicate(&mut predicate, &mut bindings, volume_path, "path");
        let Ok(mut statement) = connection.prepare(&format!(
            "SELECT bucket_millis / {DAY_MILLIS} AS day_bucket,
                    path,
                    source_root,
                    COALESCE(SUM(delta_bytes), 0)
             FROM storage_growth_delta
             WHERE {predicate}
             GROUP BY day_bucket, path, source_root
             ORDER BY day_bucket ASC"
        )) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map(params_from_iter(bindings.iter()), |row| {
            let day_bucket: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            let source_root: String = row.get(2)?;
            let total_delta: i64 = row.get(3)?;
            Ok((day_bucket.max(0) as u64, path, source_root, total_delta))
        }) else {
            return Vec::new();
        };
        let mut totals = BTreeMap::<u64, i64>::new();
        for (day_bucket, path, source_root, total_delta) in rows.flatten() {
            if storage_path_is_cloud(&path) || storage_path_is_cloud(&source_root) {
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
