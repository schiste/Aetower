use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
    ffi::CString,
    fs,
    io::{Read, Write},
    mem::MaybeUninit,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const MAX_LIMIT: usize = 200;
const MAX_ROOTS: usize = 64;
const MAX_DIRECTORIES: u64 = 25_000;
const SIZE_WALK_MAX_ENTRIES: u64 = 150_000;
const MIN_ITEM_BYTES: u64 = 1024 * 1024;
const LARGE_FILE_BYTES: u64 = 100 * 1024 * 1024;
const COLD_AFTER_DAYS: u64 = 365;
const DUPLICATE_FULL_HASH_MAX_BYTES: u64 = 256 * 1024 * 1024;
const DUPLICATE_GROUP_LIMIT: usize = 8;
const APP_FOOTPRINT_LIMIT: usize = 10;
const SCAN_TIME_BUDGET: Duration = Duration::from_millis(6_500);
const GIT_STATUS_TIME_BUDGET: Duration = Duration::from_millis(650);
const GIT_STATUS_MAX_LINES: usize = 80;
const STALE_AFTER_DAYS: u64 = 7;
const CLAUDE_MD_DELEGATION_MAX_BYTES: u64 = 1024;
const AGENTS_MD_MAX_LINES: usize = 250;
const AGENT_READINESS_MAX_SCORE: u64 = 100;
const REPO_ARTIFACT_BUDGET_BYTES: u64 = 30 * 1024 * 1024 * 1024;
const REPO_GROWTH_BUDGET_BYTES_PER_DAY: u64 = 2 * 1024 * 1024 * 1024;
const TOTAL_ARTIFACT_BUDGET_BYTES: u64 = 30 * 1024 * 1024 * 1024;
const FREE_SPACE_FLOOR_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const VOLUME_PRESSURE_FLOOR_PERCENT: u64 = 10;
const SCHEDULED_SCAN_INTERVAL_HOURS: u64 = 24;
const FAST_SIZE_WALK_MAX_ENTRIES: u64 = 30_000;
const DEEP_SIZE_WALK_MAX_ENTRIES: u64 = SIZE_WALK_MAX_ENTRIES;
const FORENSIC_SIZE_WALK_MAX_ENTRIES: u64 = 300_000;
const STORAGE_INDEX_SCHEMA_VERSION: i64 = 2;
const STORAGE_INDEX_FILE_NAME: &str = "storage-index-v1.sqlite3";
const STORAGE_SCAN_STATE_MAX_AGE_MILLIS: u64 = 24 * 60 * 60 * 1000;
const STORAGE_SCAN_PROGRESS_PERSIST_INTERVAL_MILLIS: u64 = 1_000;
const STORAGE_SCAN_PAUSE_POLL: Duration = Duration::from_millis(80);
const STORAGE_SCAN_QUEUE_POLL: Duration = Duration::from_millis(120);
const STORAGE_GROWTH_BUCKET_MILLIS: u64 = 60 * 60 * 1000;
const STORAGE_INDEX_SNAPSHOT_READ_MULTIPLIER: usize = 24;
const RECENT_CLEANUP_BLOCK_MILLIS: u64 = 10 * 60 * 1000;
const STORAGE_TREEMAP_MAX_DEPTH: usize = 4;
const STORAGE_TREEMAP_MAX_CHILDREN: usize = 14;
const STORAGE_TREEMAP_MAX_ITEMS: usize = 160;
const STORAGE_WRITER_LEDGER_MAX_BYTES: u64 = 2 * 1024 * 1024;
const STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS: u64 = 10 * 60 * 1000;
const REPOSITORY_INVENTORY_TIME_BUDGET: Duration = Duration::from_millis(30_000);
const REPOSITORY_INVENTORY_MAX_DIRECTORIES: u64 = 200_000;
const STORAGE_SCAN_LATENCY_WARN_MILLIS: u64 = 3_000;
const STORAGE_SCAN_LATENCY_CRITICAL_MILLIS: u64 = 8_000;
const STORAGE_PAYLOAD_WARN_BYTES: u64 = 2 * 1024 * 1024;
const STORAGE_PAYLOAD_CRITICAL_BYTES: u64 = 6 * 1024 * 1024;
const STORAGE_TABLE_PAGE_WARN_MILLIS: u64 = 80;
const STORAGE_TABLE_PAGE_CRITICAL_MILLIS: u64 = 250;
const STORAGE_RENDER_WARN_MILLIS: u64 = 50;
const STORAGE_RENDER_CRITICAL_MILLIS: u64 = 120;
const STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY: &str = "repository_inventory";
const STORAGE_SCAN_PHASE_ARTIFACT_SIZING: &str = "artifact_sizing";
const STORAGE_SCAN_PHASE_SCORECARD_OVERLAY: &str = "scorecard_overlay";
const STORAGE_SCAN_PHASE_FINALIZING: &str = "finalizing";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageScanMode {
    InstantCached,
    FastChangedOnly,
    DeepNative,
    ForensicVerified,
}

impl StorageScanMode {
    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "instant" | "instant_cached" | "instant-cached" => Self::InstantCached,
            "deep" | "deep_native" | "deep-native" => Self::DeepNative,
            "forensic" | "forensic_verified" | "forensic-verified" => Self::ForensicVerified,
            _ => Self::FastChangedOnly,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::InstantCached => "instant_cached",
            Self::FastChangedOnly => "fast_changed_only",
            Self::DeepNative => "deep_native",
            Self::ForensicVerified => "forensic_verified",
        }
    }

    fn size_walk_entry_budget(self) -> u64 {
        match self {
            Self::InstantCached => 8_000,
            Self::FastChangedOnly => FAST_SIZE_WALK_MAX_ENTRIES,
            Self::DeepNative => DEEP_SIZE_WALK_MAX_ENTRIES,
            Self::ForensicVerified => FORENSIC_SIZE_WALK_MAX_ENTRIES,
        }
    }

    fn verify_source_control(self) -> bool {
        matches!(self, Self::ForensicVerified)
    }

    fn collect_git_status(self) -> bool {
        matches!(self, Self::ForensicVerified)
    }

    fn use_storage_index(self) -> bool {
        matches!(self, Self::InstantCached | Self::FastChangedOnly)
    }

    fn native_metadata_strategy(self) -> &'static str {
        match self {
            Self::DeepNative | Self::ForensicVerified if cfg!(target_os = "macos") => {
                "macos_native_metadata"
            }
            Self::DeepNative | Self::ForensicVerified => "std_metadata_fallback",
            _ => "std_metadata",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageScanJobStatusKind {
    Queued,
    Running,
    Paused,
    Cancelled,
    Complete,
    Failed,
}

impl StorageScanJobStatusKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Cancelled => "cancelled",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Paused)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct StorageScanJobProgress {
    phase: String,
    scanned_files: u64,
    scanned_directories: u64,
    scanned_bytes: u64,
    current_path_hint: Option<String>,
    eta_millis: Option<u64>,
    updated_at_millis: u64,
    throttle_reason: Option<String>,
}

impl StorageScanJobProgress {
    fn new(now_millis: u64, throttle_reason: Option<String>) -> Self {
        Self {
            phase: "queued".to_owned(),
            scanned_files: 0,
            scanned_directories: 0,
            scanned_bytes: 0,
            current_path_hint: None,
            eta_millis: None,
            updated_at_millis: now_millis,
            throttle_reason,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StorageScanJobResponse {
    job_id: String,
    status: String,
    coalesced: bool,
    result_available: bool,
    error_message: Option<String>,
    started_at_millis: u64,
    updated_at_millis: u64,
    completed_at_millis: Option<u64>,
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: String,
    throttle_hint: String,
    volume_key: String,
    resumed_from_partial: bool,
    partial_state_available: bool,
    persisted_at_millis: Option<u64>,
    recovered_files: u64,
    recovered_directories: u64,
    recovered_bytes: u64,
    progress: StorageScanJobProgress,
}

#[derive(Clone, Debug)]
struct StorageScanJobRequest {
    roots: Vec<String>,
    normalized_roots: Vec<String>,
    dirty_paths: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: StorageScanMode,
    throttle_hint: String,
    signature: String,
    volume_key: String,
}

impl StorageScanJobRequest {
    fn new(
        roots: Vec<String>,
        max_depth: usize,
        limit: usize,
        mode: &str,
        throttle_hint: &str,
        dirty_paths: Vec<String>,
    ) -> Self {
        let mode = StorageScanMode::parse(mode);
        let normalized_roots = normalize_roots(roots.clone())
            .into_iter()
            .map(|root| root.display().to_string())
            .collect::<Vec<_>>();
        let dirty_paths = normalize_dirty_paths(dirty_paths);
        let max_depth = max_depth.clamp(1, 12);
        let limit = limit.clamp(1, MAX_LIMIT);
        let throttle_hint = throttle_hint.trim().to_ascii_lowercase();
        let volume_key = storage_scan_volume_key(&normalized_roots);
        let signature = format!(
            "{}|{}|{}|{}|{}|{}",
            normalized_roots.join("\u{1f}"),
            dirty_paths.join("\u{1f}"),
            max_depth,
            limit,
            mode.as_str(),
            throttle_hint
        );
        Self {
            roots,
            normalized_roots,
            dirty_paths,
            max_depth,
            limit,
            mode,
            throttle_hint,
            signature,
            volume_key,
        }
    }
}

#[derive(Debug)]
struct StorageScanJobState {
    status: StorageScanJobStatusKind,
    progress: StorageScanJobProgress,
    result_json: Option<String>,
    error_message: Option<String>,
    started_at_millis: u64,
    updated_at_millis: u64,
    completed_at_millis: Option<u64>,
    persisted_at_millis: Option<u64>,
    last_progress_persist_millis: u64,
    resumed_from_partial: bool,
    recovered_files: u64,
    recovered_directories: u64,
    recovered_bytes: u64,
}

impl StorageScanJobState {
    fn new(
        now_millis: u64,
        throttle_reason: Option<String>,
        persisted_state: Option<&StorageScanPersistedState>,
    ) -> Self {
        let recovered_progress = persisted_state.map(|state| &state.progress);
        Self {
            status: StorageScanJobStatusKind::Queued,
            progress: StorageScanJobProgress::new(now_millis, throttle_reason),
            result_json: None,
            error_message: None,
            started_at_millis: now_millis,
            updated_at_millis: now_millis,
            completed_at_millis: None,
            persisted_at_millis: persisted_state.map(|state| state.persisted_at_millis),
            last_progress_persist_millis: persisted_state
                .map(|state| state.persisted_at_millis)
                .unwrap_or_default(),
            resumed_from_partial: persisted_state.is_some(),
            recovered_files: recovered_progress
                .map(|progress| progress.scanned_files)
                .unwrap_or_default(),
            recovered_directories: recovered_progress
                .map(|progress| progress.scanned_directories)
                .unwrap_or_default(),
            recovered_bytes: recovered_progress
                .map(|progress| progress.scanned_bytes)
                .unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug)]
struct StorageScanPersistedState {
    progress: StorageScanJobProgress,
    persisted_at_millis: u64,
}

#[derive(Clone, Debug)]
struct StorageScanPersistedRecord {
    job_id: String,
    signature: String,
    volume_key: String,
    roots: Vec<String>,
    dirty_paths: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: String,
    throttle_hint: String,
    status: String,
    progress: StorageScanJobProgress,
    started_at_millis: u64,
    updated_at_millis: u64,
    completed_at_millis: Option<u64>,
    result_available: bool,
    resume_available: bool,
}

struct StorageScanStateStore;

impl StorageScanStateStore {
    fn open_connection() -> Result<Connection, String> {
        let base_dir = dirs::data_local_dir().ok_or_else(|| "no_data_dir".to_owned())?;
        let directory = base_dir.join("Aetower");
        fs::create_dir_all(&directory).map_err(|error| format!("create_dir:{error}"))?;
        let path = directory.join(STORAGE_INDEX_FILE_NAME);
        let connection = Connection::open(path).map_err(|error| format!("open_failed:{error}"))?;
        StorageSizeIndex::prepare_schema(&connection).map_err(|error| format!("schema:{error}"))?;
        Ok(connection)
    }

    fn persist(record: StorageScanPersistedRecord) -> Result<u64, String> {
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

    fn load_resume_candidate(signature: &str) -> Option<StorageScanPersistedState> {
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
    fn load_status_for_job(job_id: &str) -> Option<String> {
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

#[derive(Debug)]
struct StorageScanControl {
    cancelled: AtomicBool,
    paused: AtomicBool,
}

impl StorageScanControl {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            paused: AtomicBool::new(false),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, AtomicOrdering::SeqCst);
    }

    fn pause(&self) {
        self.paused.store(true, AtomicOrdering::SeqCst);
    }

    fn resume(&self) {
        self.paused.store(false, AtomicOrdering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(AtomicOrdering::SeqCst)
    }

    fn wait_until_runnable(&self) -> bool {
        while self.paused.load(AtomicOrdering::SeqCst) {
            if self.is_cancelled() {
                return false;
            }
            thread::sleep(STORAGE_SCAN_PAUSE_POLL);
        }
        !self.is_cancelled()
    }
}

#[derive(Clone, Debug)]
struct StorageScanThrottle {
    sleep_every_checkpoints: u64,
    sleep_millis: u64,
    reason: Option<String>,
}

impl StorageScanThrottle {
    fn for_request(request: &StorageScanJobRequest) -> Self {
        let mut sleep_every_checkpoints = match request.mode {
            StorageScanMode::InstantCached | StorageScanMode::FastChangedOnly => 0,
            StorageScanMode::DeepNative => 256,
            StorageScanMode::ForensicVerified => 128,
        };
        let mut sleep_millis = match request.mode {
            StorageScanMode::InstantCached | StorageScanMode::FastChangedOnly => 0,
            StorageScanMode::DeepNative => 2,
            StorageScanMode::ForensicVerified => 5,
        };
        let mut reasons = Vec::new();
        let has_external_or_network_root = request
            .normalized_roots
            .iter()
            .any(|root| is_network_storage_path(Path::new(root)));
        let has_cloud_root = request
            .normalized_roots
            .iter()
            .any(|root| is_cloud_storage_path(Path::new(root)));
        if has_external_or_network_root {
            sleep_every_checkpoints = 96;
            sleep_millis = sleep_millis.max(8);
            reasons.push("external-or-secondary-volume");
        }
        if has_cloud_root {
            sleep_every_checkpoints = sleep_every_checkpoints.clamp(64, 128);
            sleep_millis = sleep_millis.max(10);
            reasons.push("cloud-root");
        }
        if request.throttle_hint.contains("battery") {
            sleep_every_checkpoints = 96;
            sleep_millis = sleep_millis.max(8);
            reasons.push("battery");
        }
        if request.throttle_hint.contains("thermal") || request.throttle_hint.contains("pressure") {
            sleep_every_checkpoints = 64;
            sleep_millis = sleep_millis.max(12);
            reasons.push("thermal-pressure");
        }
        if request.throttle_hint.contains("network") {
            sleep_every_checkpoints = 64;
            sleep_millis = sleep_millis.max(12);
            reasons.push("network");
        }
        if request.throttle_hint.contains("cloud") {
            sleep_every_checkpoints = 64;
            sleep_millis = sleep_millis.max(12);
            reasons.push("cloud");
        }
        Self {
            sleep_every_checkpoints,
            sleep_millis,
            reason: if reasons.is_empty() {
                None
            } else {
                Some(reasons.join(","))
            },
        }
    }

    fn maybe_sleep(&self, checkpoint_index: u64, control: &StorageScanControl) {
        if self.sleep_every_checkpoints == 0 || self.sleep_millis == 0 {
            return;
        }
        if checkpoint_index == 0 || !checkpoint_index.is_multiple_of(self.sleep_every_checkpoints) {
            return;
        }
        let mut remaining = self.sleep_millis;
        while remaining > 0 && !control.is_cancelled() {
            let slice = remaining.min(10);
            thread::sleep(Duration::from_millis(slice));
            remaining -= slice;
        }
    }
}

#[derive(Clone, Debug)]
struct StorageScanRuntimeContext {
    control: Arc<StorageScanControl>,
    progress: Arc<Mutex<StorageScanJobProgress>>,
    throttle: StorageScanThrottle,
    checkpoint_count: Arc<AtomicU64>,
}

impl StorageScanRuntimeContext {
    fn new(
        control: Arc<StorageScanControl>,
        progress: Arc<Mutex<StorageScanJobProgress>>,
        throttle: StorageScanThrottle,
    ) -> Self {
        Self {
            control,
            progress,
            throttle,
            checkpoint_count: Arc::new(AtomicU64::new(0)),
        }
    }

    fn checkpoint(
        &self,
        phase: &str,
        path_hint: Option<&Path>,
        file_increment: u64,
        directory_increment: u64,
        byte_increment: u64,
    ) -> bool {
        if !self.control.wait_until_runnable() {
            return false;
        }
        let checkpoint_index = self
            .checkpoint_count
            .fetch_add(1, AtomicOrdering::Relaxed)
            .saturating_add(1);
        {
            let mut progress = lock_or_recover(&self.progress);
            progress.phase = phase.to_owned();
            progress.scanned_files = progress.scanned_files.saturating_add(file_increment);
            progress.scanned_directories = progress
                .scanned_directories
                .saturating_add(directory_increment);
            progress.scanned_bytes = progress.scanned_bytes.saturating_add(byte_increment);
            if let Some(path_hint) = path_hint {
                progress.current_path_hint = Some(path_hint.display().to_string());
            }
            progress.updated_at_millis = storage_now_millis();
        }
        self.throttle.maybe_sleep(checkpoint_index, &self.control);
        !self.control.is_cancelled()
    }

    fn set_phase(&self, phase: &str, path_hint: Option<&Path>) -> bool {
        if !self.control.wait_until_runnable() {
            return false;
        }
        {
            let mut progress = lock_or_recover(&self.progress);
            progress.phase = phase.to_owned();
            if let Some(path_hint) = path_hint {
                progress.current_path_hint = Some(path_hint.display().to_string());
            }
            progress.updated_at_millis = storage_now_millis();
        }
        !self.control.is_cancelled()
    }
}

struct StorageScanJob {
    id: String,
    request: StorageScanJobRequest,
    control: Arc<StorageScanControl>,
    progress: Arc<Mutex<StorageScanJobProgress>>,
    state: Mutex<StorageScanJobState>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl StorageScanJob {
    fn new(
        id: String,
        request: StorageScanJobRequest,
        persisted_state: Option<StorageScanPersistedState>,
    ) -> Arc<Self> {
        let now_millis = storage_now_millis();
        let throttle = StorageScanThrottle::for_request(&request);
        let progress = StorageScanJobProgress::new(now_millis, throttle.reason.clone());
        Arc::new(Self {
            id,
            request,
            control: Arc::new(StorageScanControl::new()),
            progress: Arc::new(Mutex::new(progress.clone())),
            state: Mutex::new(StorageScanJobState::new(
                now_millis,
                throttle.reason.clone(),
                persisted_state.as_ref(),
            )),
            handle: Mutex::new(None),
        })
    }

    fn response(&self, coalesced: bool) -> StorageScanJobResponse {
        let state = lock_or_recover(&self.state);
        StorageScanJobResponse {
            job_id: self.id.clone(),
            status: state.status.as_str().to_owned(),
            coalesced,
            result_available: state.result_json.is_some(),
            error_message: state.error_message.clone(),
            started_at_millis: state.started_at_millis,
            updated_at_millis: state.updated_at_millis,
            completed_at_millis: state.completed_at_millis,
            roots: self.request.normalized_roots.clone(),
            max_depth: self.request.max_depth,
            limit: self.request.limit,
            mode: self.request.mode.as_str().to_owned(),
            throttle_hint: self.request.throttle_hint.clone(),
            volume_key: self.request.volume_key.clone(),
            resumed_from_partial: state.resumed_from_partial,
            partial_state_available: state.resumed_from_partial
                || state.persisted_at_millis.is_some(),
            persisted_at_millis: state.persisted_at_millis,
            recovered_files: state.recovered_files,
            recovered_directories: state.recovered_directories,
            recovered_bytes: state.recovered_bytes,
            progress: state.progress.clone(),
        }
    }

    fn persist_state(&self, force: bool) {
        let now_millis = storage_now_millis();
        let record = {
            let state = lock_or_recover(&self.state);
            if !force
                && state.last_progress_persist_millis > 0
                && now_millis.saturating_sub(state.last_progress_persist_millis)
                    < STORAGE_SCAN_PROGRESS_PERSIST_INTERVAL_MILLIS
            {
                return;
            }
            StorageScanPersistedRecord {
                job_id: self.id.clone(),
                signature: self.request.signature.clone(),
                volume_key: self.request.volume_key.clone(),
                roots: self.request.normalized_roots.clone(),
                dirty_paths: self.request.dirty_paths.clone(),
                max_depth: self.request.max_depth,
                limit: self.request.limit,
                mode: self.request.mode.as_str().to_owned(),
                throttle_hint: self.request.throttle_hint.clone(),
                status: state.status.as_str().to_owned(),
                progress: state.progress.clone(),
                started_at_millis: state.started_at_millis,
                updated_at_millis: state.updated_at_millis,
                completed_at_millis: state.completed_at_millis,
                result_available: state.result_json.is_some(),
                resume_available: state.status.is_active(),
            }
        };
        if let Ok(persisted_at_millis) = StorageScanStateStore::persist(record) {
            let mut state = lock_or_recover(&self.state);
            state.persisted_at_millis = Some(persisted_at_millis);
            state.last_progress_persist_millis = persisted_at_millis;
        }
    }

    fn set_status(&self, status: StorageScanJobStatusKind, phase: &str) {
        let now_millis = storage_now_millis();
        let progress_snapshot = {
            let mut progress = lock_or_recover(&self.progress);
            progress.phase = phase.to_owned();
            progress.updated_at_millis = now_millis;
            progress.clone()
        };
        let mut state = lock_or_recover(&self.state);
        state.status = status;
        state.progress = progress_snapshot;
        state.updated_at_millis = now_millis;
        if !status.is_active() {
            state.completed_at_millis = Some(now_millis);
        }
        drop(state);
        self.persist_state(true);
    }

    fn set_status_preserving_phase(&self, status: StorageScanJobStatusKind) {
        let now_millis = storage_now_millis();
        let progress_snapshot = {
            let mut progress = lock_or_recover(&self.progress);
            progress.updated_at_millis = now_millis;
            progress.clone()
        };
        let mut state = lock_or_recover(&self.state);
        state.status = status;
        state.progress = progress_snapshot;
        state.updated_at_millis = now_millis;
        if !status.is_active() {
            state.completed_at_millis = Some(now_millis);
        }
        drop(state);
        self.persist_state(true);
    }

    fn refresh_progress(&self) {
        let now_millis = storage_now_millis();
        let progress_snapshot = lock_or_recover(&self.progress).clone();
        let mut state = lock_or_recover(&self.state);
        state.progress = progress_snapshot;
        state.updated_at_millis = now_millis;
        drop(state);
        self.persist_state(false);
    }

    fn complete(&self, result_json: String) {
        let now_millis = storage_now_millis();
        let progress_snapshot = {
            let mut progress = lock_or_recover(&self.progress);
            progress.phase = "complete".to_owned();
            progress.updated_at_millis = now_millis;
            progress.clone()
        };
        let mut state = lock_or_recover(&self.state);
        state.status = StorageScanJobStatusKind::Complete;
        state.result_json = Some(result_json);
        state.error_message = None;
        state.updated_at_millis = now_millis;
        state.completed_at_millis = Some(state.updated_at_millis);
        state.progress = progress_snapshot;
        drop(state);
        self.persist_state(true);
    }

    fn fail(&self, error: String) {
        let now_millis = storage_now_millis();
        let progress_snapshot = {
            let mut progress = lock_or_recover(&self.progress);
            progress.phase = "failed".to_owned();
            progress.updated_at_millis = now_millis;
            progress.clone()
        };
        let mut state = lock_or_recover(&self.state);
        state.status = StorageScanJobStatusKind::Failed;
        state.error_message = Some(error);
        state.updated_at_millis = now_millis;
        state.completed_at_millis = Some(state.updated_at_millis);
        state.progress = progress_snapshot;
        drop(state);
        self.persist_state(true);
    }

    fn cancel(&self) {
        self.control.cancel();
        self.set_status(StorageScanJobStatusKind::Cancelled, "cancelled");
    }

    fn pause(&self) {
        self.control.pause();
        self.set_status_preserving_phase(StorageScanJobStatusKind::Paused);
    }

    fn resume(&self) {
        self.control.resume();
        self.set_status_preserving_phase(StorageScanJobStatusKind::Running);
    }
}

struct StorageScanJobManager {
    next_id: AtomicU64,
    jobs: Mutex<BTreeMap<String, Arc<StorageScanJob>>>,
    active_volume_keys: Mutex<BTreeSet<String>>,
    volume_condvar: Condvar,
}

impl StorageScanJobManager {
    fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            jobs: Mutex::new(BTreeMap::new()),
            active_volume_keys: Mutex::new(BTreeSet::new()),
            volume_condvar: Condvar::new(),
        }
    }

    fn start(
        &'static self,
        roots: Vec<String>,
        max_depth: usize,
        limit: usize,
        mode: &str,
        throttle_hint: &str,
        dirty_paths: Vec<String>,
    ) -> StorageScanJobResponse {
        let request =
            StorageScanJobRequest::new(roots, max_depth, limit, mode, throttle_hint, dirty_paths);
        if let Some(existing) = self.find_active_by_signature(&request.signature) {
            return existing.response(true);
        }

        let persisted_state = StorageScanStateStore::load_resume_candidate(&request.signature);
        let sequence = self.next_id.fetch_add(1, AtomicOrdering::Relaxed);
        let job_id = format!("storage-scan-{}-{sequence}", storage_now_millis());
        let job = StorageScanJob::new(job_id.clone(), request, persisted_state);
        {
            let mut jobs = lock_or_recover(&self.jobs);
            jobs.insert(job_id, Arc::clone(&job));
        }
        job.persist_state(true);

        let manager = storage_scan_jobs();
        let job_for_thread = Arc::clone(&job);
        match thread::Builder::new()
            .name("aetower-storage-scan".to_owned())
            .spawn(move || run_storage_scan_job(manager, job_for_thread))
        {
            Ok(handle) => {
                *lock_or_recover(&job.handle) = Some(handle);
            }
            Err(error) => {
                job.fail(format!("failed to spawn storage scan job: {error}"));
            }
        }
        job.response(false)
    }

    fn status(&self, job_id: &str) -> Option<StorageScanJobResponse> {
        self.job(job_id).map(|job| {
            job.refresh_progress();
            job.response(false)
        })
    }

    fn pause(&self, job_id: &str) -> Option<StorageScanJobResponse> {
        self.job(job_id).map(|job| {
            job.pause();
            job.response(false)
        })
    }

    fn resume(&self, job_id: &str) -> Option<StorageScanJobResponse> {
        self.job(job_id).map(|job| {
            job.resume();
            self.volume_condvar.notify_all();
            job.response(false)
        })
    }

    fn cancel(&self, job_id: &str) -> Option<StorageScanJobResponse> {
        self.job(job_id).map(|job| {
            job.cancel();
            self.volume_condvar.notify_all();
            job.response(false)
        })
    }

    fn result(&self, job_id: &str) -> Result<String, String> {
        let Some(job) = self.job(job_id) else {
            return Err(format!("storage scan job not found: {job_id}"));
        };
        job.refresh_progress();
        let state = lock_or_recover(&job.state);
        match state.status {
            StorageScanJobStatusKind::Complete => state
                .result_json
                .clone()
                .ok_or_else(|| "storage scan completed without a result".to_owned()),
            StorageScanJobStatusKind::Failed => Err(state
                .error_message
                .clone()
                .unwrap_or_else(|| "storage scan failed".to_owned())),
            StorageScanJobStatusKind::Cancelled => Err("storage scan was cancelled".to_owned()),
            _ => Err(format!("storage scan is still {}", state.status.as_str())),
        }
    }

    fn job(&self, job_id: &str) -> Option<Arc<StorageScanJob>> {
        lock_or_recover(&self.jobs).get(job_id).cloned()
    }

    fn find_active_by_signature(&self, signature: &str) -> Option<Arc<StorageScanJob>> {
        lock_or_recover(&self.jobs)
            .values()
            .find(|job| {
                job.request.signature == signature && lock_or_recover(&job.state).status.is_active()
            })
            .cloned()
    }

    fn acquire_volume_slot(&self, job: &StorageScanJob) -> bool {
        let mut active = lock_or_recover(&self.active_volume_keys);
        while active.contains(&job.request.volume_key) {
            if job.control.is_cancelled() {
                return false;
            }
            job.set_status(StorageScanJobStatusKind::Queued, "queued");
            let (next_active, _) = self
                .volume_condvar
                .wait_timeout(active, STORAGE_SCAN_QUEUE_POLL)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            active = next_active;
        }
        active.insert(job.request.volume_key.clone());
        true
    }

    fn release_volume_slot(&self, volume_key: &str) {
        lock_or_recover(&self.active_volume_keys).remove(volume_key);
        self.volume_condvar.notify_all();
    }
}

fn storage_scan_jobs() -> &'static StorageScanJobManager {
    static JOBS: OnceLock<StorageScanJobManager> = OnceLock::new();
    JOBS.get_or_init(StorageScanJobManager::new)
}

fn run_storage_scan_job(manager: &'static StorageScanJobManager, job: Arc<StorageScanJob>) {
    if !manager.acquire_volume_slot(&job) {
        job.cancel();
        return;
    }

    job.set_status(
        StorageScanJobStatusKind::Running,
        STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY,
    );
    let result = std::panic::catch_unwind({
        let job = Arc::clone(&job);
        move || {
            let throttle = StorageScanThrottle::for_request(&job.request);
            let runtime = StorageScanRuntimeContext::new(
                Arc::clone(&job.control),
                Arc::clone(&job.progress),
                throttle,
            );
            let report = build_storage_hygiene_report_with_options(
                job.request.roots.clone(),
                StorageHygieneOptions {
                    max_depth: job.request.max_depth,
                    limit: job.request.limit,
                    mode: job.request.mode,
                    runtime: Some(runtime),
                    dirty_paths: job.request.dirty_paths.clone(),
                },
            );
            if job.control.is_cancelled() {
                return Err("cancelled".to_owned());
            }
            job.set_status(
                StorageScanJobStatusKind::Running,
                STORAGE_SCAN_PHASE_FINALIZING,
            );
            finalize_storage_report_json(report)
        }
    });

    manager.release_volume_slot(&job.request.volume_key);

    match result {
        Ok(Ok(json)) => {
            if job.control.is_cancelled() {
                job.cancel();
            } else {
                job.complete(json);
            }
        }
        Ok(Err(error)) if error == "cancelled" => job.cancel(),
        Ok(Err(error)) => job.fail(error),
        Err(_) => job.fail("storage scan job panicked".to_owned()),
    }
}

fn storage_now_millis() -> u64 {
    crate::current_unix_millis().unwrap_or_default()
}

fn storage_scan_volume_key(normalized_roots: &[String]) -> String {
    let mut device_ids = BTreeSet::new();
    for root in normalized_roots {
        if let Ok(metadata) = fs::symlink_metadata(root) {
            device_ids.insert(metadata.dev().to_string());
        }
    }
    if device_ids.is_empty() {
        format!("roots:{}", normalized_roots.join("\u{1f}"))
    } else {
        format!(
            "dev:{}",
            device_ids.into_iter().collect::<Vec<_>>().join(",")
        )
    }
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StorageHygieneReport {
    captured_at_millis: u64,
    scan_duration_millis: u64,
    scan_mode: String,
    diagnostics: StorageScanDiagnostics,
    summary: StorageHygieneSummary,
    investigation: StorageInvestigationSummary,
    cleanup_tiers: Vec<StorageCleanupTierSummary>,
    cleanup_recipes: Vec<StorageCleanupRecipe>,
    cleanup_bundles: Vec<StorageCleanupBundle>,
    budget_guardrails: StorageBudgetGuardrails,
    agent_hygiene: StorageAgentHygieneSummary,
    repository_inventory: Vec<StorageRepositoryInventoryItem>,
    repository_inventory_complete: bool,
    repository_inventory_truncated: bool,
    repository_inventory_roots: Vec<String>,
    repository_inventory_partial_roots: Vec<String>,
    repository_inventory_coverage: Vec<RepositoryInventoryRootCoverage>,
    repo_footprints: Vec<StorageRepoFootprint>,
    duplicate_groups: Vec<StorageDuplicateGroup>,
    app_footprints: Vec<StorageAppFootprint>,
    system_data_buckets: Vec<StorageSystemDataBucket>,
    treemap_roots: Vec<StorageTreemapNode>,
    items: Vec<StorageHygieneItem>,
    roots: Vec<String>,
    skipped_roots: Vec<StorageSkippedRoot>,
    source_coverage: Vec<StorageSourceCoverage>,
    volume_states: Vec<StorageVolumeState>,
    growth_deltas: Vec<StorageGrowthDelta>,
    truncated: bool,
    caveats: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StorageHygieneSummary {
    item_count: usize,
    total_reclaimable_bytes: u64,
    safe_candidate_count: usize,
    review_candidate_count: usize,
    stale_candidate_count: usize,
    scanned_directory_count: u64,
    largest_item_path: Option<String>,
    largest_item_bytes: u64,
    attributed_repo_count: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StorageScanDiagnostics {
    mode: String,
    root_walk_millis: u64,
    size_walk_millis: u64,
    git_millis: u64,
    serialize_millis: u64,
    payload_bytes: u64,
    decode_millis: u64,
    scanned_directory_count: u64,
    discovered_repository_count: u64,
    sized_entry_count: u64,
    candidate_seen_count: u64,
    candidate_retained_count: u64,
    storage_index_status: String,
    storage_index_hits: u64,
    storage_index_misses: u64,
    storage_index_writes: u64,
    native_metadata_strategy: String,
    fsevents_status: String,
    lazy_git_status: bool,
    top_k_retained: bool,
    performance_budget: StoragePerformanceBudgetDiagnostics,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StoragePerformanceBudgetDiagnostics {
    status: String,
    scan_job_latency_millis: u64,
    payload_bytes: u64,
    payload_budget_bytes: u64,
    table_page_millis: u64,
    table_page_budget_millis: u64,
    render_publish_millis: u64,
    render_budget_millis: u64,
    notes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct StorageScanMetrics {
    root_walk_millis: u64,
    size_walk_millis: u64,
    git_millis: u64,
    scanned_directory_count: u64,
    discovered_repository_count: u64,
    sized_entry_count: u64,
    candidate_seen_count: u64,
    storage_index_hits: u64,
    storage_index_misses: u64,
    storage_index_writes: u64,
    storage_index_status: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageSourceCoverage {
    id: String,
    label: String,
    kind: String,
    path: String,
    status: String,
    permission_state: String,
    gap_kind: String,
    detail: String,
    local_bytes: Option<u64>,
    logical_bytes: Option<u64>,
    reclaimable_bytes: Option<u64>,
    cloud_placeholder: bool,
    network: bool,
    protected: bool,
    scanned: bool,
}

#[derive(Clone, Debug, Serialize)]
struct RepositoryInventoryRootCoverage {
    id: String,
    label: String,
    path: String,
    status: String,
    permission_state: String,
    detail: String,
    repository_count: u64,
    scanned_directory_count: u64,
    skipped_directory_count: u64,
    truncated: bool,
    scanned: bool,
}

#[derive(Clone, Debug)]
struct RepositoryInventoryCompleteness {
    complete: bool,
    truncated: bool,
    roots: Vec<String>,
    partial_roots: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageVolumeState {
    path: String,
    device_id: u64,
    filesystem_type: String,
    total_bytes: u64,
    free_now_bytes: u64,
    available_bytes: u64,
    purgeable_bytes_estimate: u64,
    important_usage_available_bytes: Option<u64>,
    opportunistic_usage_available_bytes: Option<u64>,
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageGrowthDelta {
    bucket_millis: u64,
    scan_millis: u64,
    path: String,
    source_root: String,
    repo_root: Option<String>,
    repo_name: Option<String>,
    git_branch: Option<String>,
    git_head: Option<String>,
    kind: String,
    cleanup_tier: String,
    previous_physical_bytes: u64,
    current_physical_bytes: u64,
    delta_bytes: i64,
    command: Option<String>,
    process_tree: Option<String>,
    ai_agent_session: Option<String>,
    writer_source: Option<String>,
    matched_writer_count: u64,
    attribution_sources: Vec<String>,
    attribution_confidence: String,
    attribution_confidence_score: u8,
    attribution_ambiguous: bool,
    attribution_summary: String,
    attribution_evidence: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct StorageWriterLedgerRecord {
    #[serde(default)]
    timestamp_millis: Option<u64>,
    #[serde(default)]
    started_at_millis: Option<u64>,
    #[serde(default)]
    ended_at_millis: Option<u64>,
    #[serde(default)]
    path_prefix: Option<String>,
    #[serde(default)]
    repo_root: Option<String>,
    #[serde(default)]
    repo_name: Option<String>,
    #[serde(default)]
    git_branch: Option<String>,
    #[serde(default)]
    git_head: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    working_directory: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    process_tree: Option<String>,
    #[serde(default)]
    ai_agent_session: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    tab_id: Option<String>,
    #[serde(default)]
    tab_name: Option<String>,
    #[serde(default)]
    chau7_session_id: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Clone, Debug)]
struct StorageGrowthAttribution {
    repo_name: Option<String>,
    git_branch: Option<String>,
    git_head: Option<String>,
    command: Option<String>,
    process_tree: Option<String>,
    ai_agent_session: Option<String>,
    writer_source: Option<String>,
    matched_writer_count: u64,
    sources: Vec<String>,
    confidence: String,
    confidence_score: u8,
    ambiguous: bool,
    summary: String,
    evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageHygieneItem {
    id: String,
    path: String,
    display_name: String,
    kind: String,
    storage_role: String,
    git_status: String,
    safety: String,
    cleanup_tier: String,
    size_bytes: u64,
    logical_bytes: u64,
    physical_bytes: u64,
    byte_accounting: String,
    sparse_or_shared: bool,
    hardlink_count: u64,
    has_hardlinks: bool,
    cloud_placeholder: bool,
    protected_path: bool,
    size_truncated: bool,
    modified_millis: Option<u64>,
    age_days: Option<u64>,
    accessed_millis: Option<u64>,
    access_age_days: Option<u64>,
    cold: bool,
    stale: bool,
    reason: String,
    recommendation: String,
    next_step: String,
    command_hint: String,
    rebuild_command: Option<String>,
    estimated_rebuild_cost: String,
    estimated_rebuild_seconds: Option<u64>,
    cleanup_consequence: String,
    evidence: Vec<String>,
    cleanup_allowed: bool,
    cleanup_blockers: Vec<String>,
    default_cleanup_action: String,
    attribution: StorageArtifactAttribution,
}

#[derive(Clone, Debug, Serialize)]
struct StorageTreemapNode {
    id: String,
    path: String,
    label: String,
    depth: usize,
    node_type: String,
    file_type: String,
    color_key: String,
    size_bytes: u64,
    item_count: usize,
    children: Vec<StorageTreemapNode>,
    has_more: bool,
}

#[derive(Clone, Debug, Default)]
struct StorageTreemapAccumulator {
    path: String,
    label: String,
    depth: usize,
    size_bytes: u64,
    item_count: usize,
    kind_bytes: BTreeMap<String, u64>,
    color_bytes: BTreeMap<String, u64>,
    children: BTreeMap<String, StorageTreemapAccumulator>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StorageInvestigationSummary {
    top_findings: Vec<StorageInvestigationFinding>,
    known_cache_bytes: u64,
    rebuildable_bytes: u64,
    expensive_bytes: u64,
    risky_bytes: u64,
    large_file_count: usize,
    cold_file_count: usize,
    cold_file_bytes: u64,
    review_item_count: usize,
    open_conflict_status: String,
    recommended_next_steps: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageInvestigationFinding {
    id: String,
    title: String,
    detail: String,
    path: String,
    storage_role: String,
    cleanup_tier: String,
    safety: String,
    size_bytes: u64,
    confidence_score: u8,
    evidence: Vec<String>,
    recommended_action: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageArtifactAttribution {
    repo_root: Option<String>,
    repo_name: Option<String>,
    git_branch: Option<String>,
    git_head: Option<String>,
    command: Option<String>,
    process_tree: Option<String>,
    ai_agent_session: Option<String>,
    confidence: String,
    notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StorageCleanupTierSummary {
    tier: String,
    label: String,
    description: String,
    item_count: usize,
    bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
struct StorageCleanupRecipe {
    id: String,
    title: String,
    category: String,
    safety: String,
    affected_path: String,
    command: String,
    estimated_reclaimable_bytes: u64,
    reason: String,
    prerequisites: Vec<String>,
    destructive: bool,
    requires_review: bool,
}

#[derive(Clone, Debug, Serialize)]
struct StorageCleanupBundle {
    id: String,
    title: String,
    subtitle: String,
    safety: String,
    confidence_score: u8,
    estimated_reclaimable_bytes: u64,
    item_count: usize,
    dry_run_only: bool,
    manifest: Vec<StorageCleanupBundleItem>,
    dry_run_commands: Vec<String>,
    rollback_notes: Vec<String>,
    prerequisites: Vec<String>,
    caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageCleanupBundleItem {
    path: String,
    display_name: String,
    kind: String,
    cleanup_tier: String,
    safety: String,
    size_bytes: u64,
    confidence_score: u8,
    dry_run_command: String,
    cleanup_command: Option<String>,
    rollback_note: String,
    reason: String,
    consequence: String,
    evidence: Vec<String>,
    cleanup_allowed: bool,
    cleanup_blockers: Vec<String>,
    default_cleanup_action: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageBudgetGuardrails {
    repo_growth_budget_bytes_per_day: u64,
    repo_artifact_budget_bytes: u64,
    total_artifact_budget_bytes: u64,
    free_space_floor_bytes: u64,
    volume_pressure_floor_percent: u64,
    warning_only_by_default: bool,
    auto_trash_safe_tier_enabled: bool,
    scheduled_scan_recommended: bool,
    scheduled_scan_interval_hours: u64,
    status: String,
    violations: Vec<StorageBudgetViolation>,
    policies: Vec<StoragePreventionPolicy>,
    prevention_suggestions: Vec<StoragePreventionSuggestion>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageBudgetViolation {
    id: String,
    scope: String,
    severity: String,
    title: String,
    detail: String,
    repo_root: Option<String>,
    repo_name: Option<String>,
    observed_bytes: u64,
    limit_bytes: u64,
    recommendation: String,
}

#[derive(Clone, Debug, Serialize)]
struct StoragePreventionPolicy {
    id: String,
    title: String,
    mode: String,
    enabled: bool,
    action: String,
    tier: String,
    detail: String,
    next_step: String,
}

#[derive(Clone, Debug, Serialize)]
struct StoragePreventionSuggestion {
    id: String,
    trigger: String,
    title: String,
    detail: String,
    action_label: String,
    estimated_reclaimable_bytes: u64,
    safety: String,
    requires_approval: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StorageAgentHygieneSummary {
    total_agent_artifact_bytes: u64,
    week_agent_artifact_bytes: u64,
    rebuildable_agent_bytes: u64,
    rebuildable_agent_percent: f32,
    week_rebuildable_agent_bytes: u64,
    week_rebuildable_agent_percent: f32,
    attributed_item_count: usize,
    agent_count: usize,
    agents: Vec<StorageAgentArtifactSummary>,
    caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAgentArtifactSummary {
    id: String,
    provider: String,
    display_name: String,
    session_id: Option<String>,
    artifact_bytes: u64,
    week_artifact_bytes: u64,
    rebuildable_bytes: u64,
    rebuildable_percent: f32,
    week_rebuildable_bytes: u64,
    week_rebuildable_percent: f32,
    item_count: usize,
    repo_count: usize,
    top_repositories: Vec<StorageAgentRepoSummary>,
    top_items: Vec<StorageAgentItemSummary>,
    confidence: String,
    attribution_sources: Vec<String>,
    recommendation: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAgentRepoSummary {
    repo_root: String,
    repo_name: String,
    artifact_bytes: u64,
    item_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAgentItemSummary {
    path: String,
    display_name: String,
    kind: String,
    cleanup_tier: String,
    size_bytes: u64,
    modified_millis: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageDuplicateGroup {
    id: String,
    candidate_key: String,
    confirmed: bool,
    confidence_score: u8,
    file_count: usize,
    total_bytes: u64,
    reclaimable_bytes: u64,
    paths: Vec<StorageDuplicateItem>,
    recommendation: String,
    caveat: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageDuplicateItem {
    path: String,
    display_name: String,
    size_bytes: u64,
    modified_millis: Option<u64>,
    cleanup_tier: String,
    safety: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAppFootprint {
    id: String,
    app_name: String,
    bundle_identifier: Option<String>,
    total_bytes: u64,
    component_count: usize,
    cleanup_tier: String,
    safety: String,
    confidence_score: u8,
    components: Vec<StorageAppFootprintComponent>,
    recommendation: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAppFootprintComponent {
    path: String,
    component: String,
    size_bytes: u64,
    cleanup_tier: String,
    safety: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageSystemDataBucket {
    id: String,
    title: String,
    category: String,
    size_bytes: u64,
    item_count: usize,
    cleanup_tier: String,
    safety: String,
    explanation: String,
    recommended_action: String,
    paths: Vec<String>,
    requires_full_disk_access: bool,
}

#[derive(Clone, Debug, Serialize)]
struct StorageRepoFootprint {
    id: String,
    repo_root: String,
    repo_name: String,
    current_size_bytes: u64,
    artifact_bytes: u64,
    safe_bytes: u64,
    rebuildable_bytes: u64,
    expensive_bytes: u64,
    risky_bytes: u64,
    rebuildable_percent: f32,
    item_count: usize,
    top_artifact_folders: Vec<StorageRepoArtifactFolder>,
    artifact_mix: Vec<StorageRepoArtifactMix>,
    duplicate_clone_count: u64,
    duplicate_clone_roots: Vec<String>,
    last_writer_process: Option<String>,
    last_writer_pid: Option<u32>,
    last_branch_touched: Option<String>,
    growth_bytes: Option<i64>,
    growth_window: String,
    estimated_rebuild_cost: String,
    estimated_rebuild_seconds: Option<u64>,
    optimization_summary: String,
    caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageRepositoryInventoryItem {
    id: String,
    repo_root: String,
    repo_name: String,
    inventory_cache_status: String,
    inventory_fingerprint: String,
    inventory_fingerprint_changed: bool,
    inventory_last_seen_millis: Option<u64>,
    inventory_last_scan_millis: Option<u64>,
    git_branch: Option<String>,
    git_head: Option<String>,
    git_ref: Option<String>,
    git_detached_head: bool,
    git_remote_origin_url: Option<String>,
    git_remote_key: Option<String>,
    git_remote_host: Option<String>,
    git_remote_owner: Option<String>,
    git_remote_name: Option<String>,
    git_dirty_status: String,
    git_dirty_file_count: Option<u64>,
    git_dirty_truncated: bool,
    not_seen_in_latest_scan: bool,
    clone_group_count: u64,
    clone_group_roots: Vec<String>,
    discovered_root: String,
    has_agents_md: bool,
    has_claude_md: bool,
    claude_md_bytes: Option<u64>,
    claude_md_delegation_max_bytes: u64,
    claude_md_delegates_to_agents_md: bool,
    agent_readiness_score: u8,
    agent_readiness_status: String,
    agent_contract_missing_count: u64,
    agent_contract_coverage: Vec<StorageAgentContractCoverage>,
    agent_guidance_status: String,
    agent_guidance_issue_count: u64,
    agent_guidance_issues: Vec<StorageAgentGuidanceIssue>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAgentContractCoverage {
    id: String,
    label: String,
    path: String,
    kind: String,
    status: String,
    severity: String,
    detail: String,
    weight: u64,
    earned_weight: u64,
    coverage_percent: u8,
    present: bool,
    tracked: bool,
    schema_version: Option<String>,
    generated: bool,
    reviewed: bool,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAgentGuidanceIssue {
    id: String,
    severity: String,
    title: String,
    detail: String,
    path: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageRepoArtifactFolder {
    path: String,
    display_name: String,
    kind: String,
    cleanup_tier: String,
    size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
struct StorageRepoArtifactMix {
    kind: String,
    label: String,
    item_count: usize,
    bytes: u64,
    cleanup_tier: String,
    rebuild_command: Option<String>,
    estimated_rebuild_cost: String,
    estimated_rebuild_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageSkippedRoot {
    path: String,
    reason: String,
}

#[derive(Clone, Debug)]
struct StorageHygieneOptions {
    max_depth: usize,
    limit: usize,
    mode: StorageScanMode,
    runtime: Option<StorageScanRuntimeContext>,
    dirty_paths: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
struct ArtifactRule {
    kind: &'static str,
    safety: &'static str,
    cleanup_tier: &'static str,
    reason: &'static str,
    recommendation: &'static str,
}

#[derive(Clone, Debug)]
struct ArtifactIntelligence {
    rebuild_command: Option<String>,
    estimated_rebuild_cost: String,
    estimated_rebuild_seconds: Option<u64>,
    cleanup_consequence: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct SizeWalkResult {
    bytes: u64,
    allocated_bytes: u64,
    truncated: bool,
    entries: u64,
    max_hardlink_count: u64,
    has_hardlinks: bool,
    sparse_or_shared: bool,
    cloud_placeholder: bool,
}

#[derive(Clone, Debug)]
struct StorageIndexedFileRow {
    path: String,
    device: i64,
    inode: i64,
    file_id: String,
    source_root: String,
    repo_root: Option<String>,
    kind: String,
    storage_role: String,
    safety: String,
    cleanup_tier: String,
    logical_bytes: u64,
    physical_bytes: u64,
    modified_millis: Option<u64>,
    changed_millis: Option<u64>,
    accessed_millis: Option<u64>,
    birth_millis: Option<u64>,
    is_directory: bool,
    entries: u64,
    truncated: bool,
    last_scan_millis: u64,
}

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
struct StorageCandidateCollector {
    limit: usize,
    heap: BinaryHeap<Reverse<RankedStorageItem>>,
    seen: u64,
}

impl StorageCandidateCollector {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            heap: BinaryHeap::new(),
            seen: 0,
        }
    }

    fn push(&mut self, item: StorageHygieneItem) {
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

struct StorageSizeIndex {
    connection: Option<Connection>,
    status: String,
}

#[derive(Clone, Debug, Default)]
struct RepositoryInventoryCacheEntry {
    discovered_root: String,
    repository_fingerprint: String,
    last_seen_millis: u64,
    last_scan_millis: u64,
}

#[derive(Clone, Debug, Default)]
struct RepositoryInventoryCacheState {
    status: String,
    fingerprint: String,
    fingerprint_changed: bool,
    last_seen_millis: Option<u64>,
    last_scan_millis: Option<u64>,
}

impl StorageSizeIndex {
    fn open() -> Self {
        let Some(base_dir) = dirs::data_local_dir() else {
            return Self {
                connection: None,
                status: "unavailable:no_data_dir".to_owned(),
            };
        };
        let directory = base_dir.join("Aetower");
        if let Err(error) = fs::create_dir_all(&directory) {
            return Self {
                connection: None,
                status: format!("unavailable:create_dir:{error}"),
            };
        }
        let path = directory.join(STORAGE_INDEX_FILE_NAME);
        let Ok(connection) = Connection::open(path) else {
            return Self {
                connection: None,
                status: "unavailable:open_failed".to_owned(),
            };
        };
        if let Err(error) = Self::prepare_schema(&connection) {
            return Self {
                connection: None,
                status: format!("unavailable:schema:{error}"),
            };
        }
        Self {
            connection: Some(connection),
            status: "ready".to_owned(),
        }
    }

    fn disabled(reason: &str) -> Self {
        Self {
            connection: None,
            status: format!("disabled:{reason}"),
        }
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
            connection.execute(
                "ALTER TABLE storage_repository_inventory_cache
                 ADD COLUMN repository_fingerprint TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        Ok(())
    }

    fn lookup(
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
    fn store(
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

    fn store_indexed_row(&self, row: &StorageIndexedFileRow, metrics: &mut StorageScanMetrics) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let previous_physical = connection
            .query_row(
                "SELECT physical_bytes FROM storage_file_index WHERE path = ?1",
                params![&row.path],
                |row| row.get::<_, i64>(0),
            )
            .ok()
            .map(|value| value.max(0) as u64);
        if connection
            .execute(
                "INSERT OR REPLACE INTO storage_file_index (
                    path, device, inode, file_id, source_root, repo_root, kind, storage_role,
                    safety, cleanup_tier, logical_bytes, physical_bytes, modified_millis,
                    changed_millis, accessed_millis, birth_millis, is_directory, entries,
                    truncated, last_scan_millis
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                    ?17, ?18, ?19, ?20
                 )",
                params![
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
                    row.last_scan_millis.min(i64::MAX as u64) as i64
                ],
            )
            .is_ok()
        {
            metrics.storage_index_writes = metrics.storage_index_writes.saturating_add(1);
            self.record_growth_delta(row, previous_physical);
        }
    }

    fn record_growth_delta(&self, row: &StorageIndexedFileRow, previous_physical: Option<u64>) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let previous_physical = previous_physical.unwrap_or(0);
        if previous_physical == row.physical_bytes {
            return;
        }
        let delta = row.physical_bytes as i128 - previous_physical as i128;
        let delta = delta.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        if delta == 0 {
            return;
        }
        let bucket_millis =
            (row.last_scan_millis / STORAGE_GROWTH_BUCKET_MILLIS) * STORAGE_GROWTH_BUCKET_MILLIS;
        let _ = connection.execute(
            "INSERT INTO storage_growth_delta (
                bucket_millis, scan_millis, path, source_root, repo_root, kind, cleanup_tier,
                previous_physical_bytes, current_physical_bytes, delta_bytes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
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
            ],
        );
        let retention_before = row
            .last_scan_millis
            .saturating_sub(30 * 24 * 60 * 60 * 1000)
            .min(i64::MAX as u64) as i64;
        let _ = connection.execute(
            "DELETE FROM storage_growth_delta WHERE scan_millis < ?1",
            params![retention_before],
        );
    }

    fn load_candidate_rows(
        &self,
        roots: &[PathBuf],
        limit: usize,
        metrics: &mut StorageScanMetrics,
    ) -> Result<Vec<StorageIndexedFileRow>, String> {
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

    fn load_repository_inventory_cache(
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

    fn store_repository_inventory_cache(
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

    fn load_growth_deltas(&self, limit: usize) -> Vec<StorageGrowthDelta> {
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

fn path_is_under_root(path: &str, root: &Path) -> bool {
    let root = root.display().to_string();
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('/'))
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

pub fn repository_inventory_json(roots: Vec<String>, max_depth: usize) -> Result<String, String> {
    let started = Instant::now();
    let captured_at_millis = crate::current_unix_millis().unwrap_or_default();
    let requested_roots = normalize_roots(roots);
    let root_labels = requested_roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    let repository_cache = StorageSizeIndex::open();
    let cached_repository_entries =
        repository_cache.load_repository_inventory_cache(&requested_roots);
    let scan = scan_repository_inventory_roots(&requested_roots, max_depth);
    let repository_walk_millis = started.elapsed().as_millis() as u64;
    let discovered_repository_count = scan.repositories_by_root.len().min(u64::MAX as usize) as u64;
    let scanned_directory_count = scan
        .coverage
        .iter()
        .map(|coverage| coverage.scanned_directory_count)
        .fold(0u64, u64::saturating_add);
    let skipped_directory_count = scan
        .coverage
        .iter()
        .map(|coverage| coverage.skipped_directory_count)
        .fold(0u64, u64::saturating_add);

    let mut metrics = StorageScanMetrics {
        storage_index_status: "repository_inventory_only".to_owned(),
        ..StorageScanMetrics::default()
    };
    repository_cache.store_repository_inventory_cache(
        &scan.repositories_by_root,
        captured_at_millis,
        &mut metrics,
    );
    let cached_repository_roots = repository_inventory_cache_roots(&cached_repository_entries);
    let (repository_roots, not_seen_repository_roots) = merge_repository_inventory_cache(
        cached_repository_roots,
        scan.repositories_by_root.clone(),
    );
    let repository_cache_states = repository_inventory_cache_states(
        &repository_roots,
        &scan.repositories_by_root,
        &cached_repository_entries,
    );
    let repository_inventory = summarize_repository_inventory(
        repository_roots,
        &not_seen_repository_roots,
        &repository_cache_states,
        started,
        StorageScanMode::FastChangedOnly,
        &mut metrics,
    );
    let mut repository_inventory_completeness =
        repository_inventory_completeness(&scan.coverage, scan.truncated);
    if !not_seen_repository_roots.is_empty() {
        repository_inventory_completeness.complete = false;
    }
    let report = RepositoryInventoryReport {
        captured_at_millis,
        scan_duration_millis: started.elapsed().as_millis() as u64,
        roots: root_labels,
        repository_inventory,
        repository_inventory_complete: repository_inventory_completeness.complete,
        repository_inventory_truncated: repository_inventory_completeness.truncated,
        repository_inventory_roots: repository_inventory_completeness.roots,
        repository_inventory_partial_roots: repository_inventory_completeness.partial_roots,
        repository_inventory_coverage: scan.coverage,
        truncated: scan.truncated,
        diagnostics: RepositoryInventoryDiagnostics {
            repository_walk_millis,
            git_millis: metrics.git_millis,
            discovered_repository_count,
            scanned_directory_count,
            skipped_directory_count,
        },
    };
    serde_json::to_string(&report).map_err(|error| error.to_string())
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

fn finalize_storage_report_json(mut report: StorageHygieneReport) -> Result<String, String> {
    let serialize_started = Instant::now();
    let json = serde_json::to_string(&report).map_err(|error| error.to_string())?;
    report.diagnostics.serialize_millis = serialize_started.elapsed().as_millis() as u64;
    report.diagnostics.payload_bytes = json.len().min(u64::MAX as usize) as u64;
    refresh_storage_performance_budget(&mut report, 0, 0);
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn refresh_storage_performance_budget(
    report: &mut StorageHygieneReport,
    table_page_millis: u64,
    render_publish_millis: u64,
) {
    let mut notes = Vec::new();
    let mut severity = 0u8;
    if report.scan_duration_millis >= STORAGE_SCAN_LATENCY_CRITICAL_MILLIS {
        severity = severity.max(2);
        notes.push(format!(
            "scan latency exceeded critical budget: {}ms >= {}ms",
            report.scan_duration_millis, STORAGE_SCAN_LATENCY_CRITICAL_MILLIS
        ));
    } else if report.scan_duration_millis >= STORAGE_SCAN_LATENCY_WARN_MILLIS {
        severity = severity.max(1);
        notes.push(format!(
            "scan latency exceeded warning budget: {}ms >= {}ms",
            report.scan_duration_millis, STORAGE_SCAN_LATENCY_WARN_MILLIS
        ));
    }
    if report.diagnostics.payload_bytes >= STORAGE_PAYLOAD_CRITICAL_BYTES {
        severity = severity.max(2);
        notes.push(format!(
            "payload exceeded critical budget: {} bytes >= {} bytes",
            report.diagnostics.payload_bytes, STORAGE_PAYLOAD_CRITICAL_BYTES
        ));
    } else if report.diagnostics.payload_bytes >= STORAGE_PAYLOAD_WARN_BYTES {
        severity = severity.max(1);
        notes.push(format!(
            "payload exceeded warning budget: {} bytes >= {} bytes",
            report.diagnostics.payload_bytes, STORAGE_PAYLOAD_WARN_BYTES
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
    if report.diagnostics.candidate_seen_count >= 1_000_000 {
        notes.push(format!(
            "stress fixture scale: {} candidates seen",
            report.diagnostics.candidate_seen_count
        ));
    }
    if notes.is_empty() {
        notes.push("storage scan and UI payload stayed within current budgets".to_owned());
    }
    report.diagnostics.performance_budget = StoragePerformanceBudgetDiagnostics {
        status: match severity {
            0 => "ok",
            1 => "warn",
            _ => "critical",
        }
        .to_owned(),
        scan_job_latency_millis: report.scan_duration_millis,
        payload_bytes: report.diagnostics.payload_bytes,
        payload_budget_bytes: STORAGE_PAYLOAD_WARN_BYTES,
        table_page_millis,
        table_page_budget_millis: STORAGE_TABLE_PAGE_WARN_MILLIS,
        render_publish_millis,
        render_budget_millis: STORAGE_RENDER_WARN_MILLIS,
        notes,
    };
}

pub fn storage_scan_start_json(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    mode: &str,
    throttle_hint: &str,
    dirty_paths: Vec<String>,
) -> Result<String, String> {
    serde_json::to_string(&storage_scan_jobs().start(
        roots,
        max_depth,
        limit,
        mode,
        throttle_hint,
        dirty_paths,
    ))
    .map_err(|error| error.to_string())
}

pub fn storage_scan_status_json(job_id: &str) -> Result<String, String> {
    let response = storage_scan_jobs()
        .status(job_id)
        .ok_or_else(|| format!("storage scan job not found: {job_id}"))?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

pub fn storage_scan_cancel_json(job_id: &str) -> Result<String, String> {
    let response = storage_scan_jobs()
        .cancel(job_id)
        .ok_or_else(|| format!("storage scan job not found: {job_id}"))?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

pub fn storage_scan_pause_json(job_id: &str) -> Result<String, String> {
    let response = storage_scan_jobs()
        .pause(job_id)
        .ok_or_else(|| format!("storage scan job not found: {job_id}"))?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

pub fn storage_scan_resume_json(job_id: &str) -> Result<String, String> {
    let response = storage_scan_jobs()
        .resume(job_id)
        .ok_or_else(|| format!("storage scan job not found: {job_id}"))?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

pub fn storage_scan_result_json(job_id: &str) -> Result<String, String> {
    storage_scan_jobs().result(job_id)
}

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

#[derive(Clone, Copy)]
enum StorageItemSortKey {
    Size,
    Path,
    Modified,
    Accessed,
    Tier,
    Kind,
}

impl StorageItemSortKey {
    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "path" | "name" => Self::Path,
            "modified" | "mtime" | "newest" | "oldest" => Self::Modified,
            "accessed" | "atime" | "unused" => Self::Accessed,
            "tier" | "cleanup_tier" | "safety" => Self::Tier,
            "kind" | "type" => Self::Kind,
            _ => Self::Size,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Path => "path",
            Self::Modified => "modified",
            Self::Accessed => "accessed",
            Self::Tier => "tier",
            Self::Kind => "kind",
        }
    }
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
    let requested_limit = offset.saturating_add(limit).clamp(1, MAX_LIMIT);
    let mut report =
        build_storage_hygiene_projection_report(roots, max_depth, requested_limit, mode);
    let table_started = Instant::now();
    let sort_key = StorageItemSortKey::parse(sort_key);
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
        items,
    })
    .map_err(|error| error.to_string())
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

#[derive(Clone, Debug, Serialize)]
struct StorageHygieneOverviewResponse {
    captured_at_millis: u64,
    scan_duration_millis: u64,
    scan_mode: String,
    diagnostics: StorageScanDiagnostics,
    summary: StorageHygieneSummary,
    investigation: StorageInvestigationSummary,
    cleanup_tiers: Vec<StorageCleanupTierSummary>,
    cleanup_recipes: Vec<StorageCleanupRecipe>,
    cleanup_bundles: Vec<StorageCleanupBundle>,
    budget_guardrails: StorageBudgetGuardrails,
    agent_hygiene: StorageAgentHygieneSummary,
    repository_inventory_complete: bool,
    repository_inventory_truncated: bool,
    repository_inventory_roots: Vec<String>,
    repository_inventory_partial_roots: Vec<String>,
    repository_inventory_coverage: Vec<RepositoryInventoryRootCoverage>,
    repo_footprints: Vec<StorageRepoFootprint>,
    duplicate_groups: Vec<StorageDuplicateGroup>,
    app_footprints: Vec<StorageAppFootprint>,
    system_data_buckets: Vec<StorageSystemDataBucket>,
    treemap_roots: Vec<StorageTreemapNode>,
    items: Vec<StorageHygieneItem>,
    roots: Vec<String>,
    skipped_roots: Vec<StorageSkippedRoot>,
    source_coverage: Vec<StorageSourceCoverage>,
    volume_states: Vec<StorageVolumeState>,
    growth_deltas: Vec<StorageGrowthDelta>,
    truncated: bool,
    caveats: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RepositoryInventoryReport {
    captured_at_millis: u64,
    scan_duration_millis: u64,
    roots: Vec<String>,
    repository_inventory: Vec<StorageRepositoryInventoryItem>,
    repository_inventory_complete: bool,
    repository_inventory_truncated: bool,
    repository_inventory_roots: Vec<String>,
    repository_inventory_partial_roots: Vec<String>,
    repository_inventory_coverage: Vec<RepositoryInventoryRootCoverage>,
    truncated: bool,
    diagnostics: RepositoryInventoryDiagnostics,
}

#[derive(Clone, Debug, Serialize)]
struct RepositoryInventoryDiagnostics {
    repository_walk_millis: u64,
    git_millis: u64,
    discovered_repository_count: u64,
    scanned_directory_count: u64,
    skipped_directory_count: u64,
}

#[derive(Clone, Debug, Serialize)]
struct StorageHygieneActionsResponse {
    captured_at_millis: u64,
    scan_mode: String,
    diagnostics: StorageScanDiagnostics,
    cleanup_tiers: Vec<StorageCleanupTierSummary>,
    cleanup_recipes: Vec<StorageCleanupRecipe>,
    cleanup_bundles: Vec<StorageCleanupBundle>,
    budget_guardrails: StorageBudgetGuardrails,
}

#[derive(Clone, Debug, Serialize)]
struct StorageHygieneItemsPageResponse {
    captured_at_millis: u64,
    scan_mode: String,
    diagnostics: StorageScanDiagnostics,
    offset: usize,
    limit: usize,
    sort_key: String,
    sort_descending: bool,
    returned_count: usize,
    total_available: usize,
    has_more: bool,
    items: Vec<StorageHygieneItem>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageHygieneRepoDetailResponse {
    captured_at_millis: u64,
    scan_mode: String,
    diagnostics: StorageScanDiagnostics,
    repository: Option<StorageRepositoryInventoryItem>,
    repo_footprints: Vec<StorageRepoFootprint>,
    items: Vec<StorageHygieneItem>,
    cleanup_recipes: Vec<StorageCleanupRecipe>,
    cleanup_bundles: Vec<StorageCleanupBundle>,
    caveats: Vec<String>,
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

fn build_storage_hygiene_report_with_options(
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
    let storage_index = if options.mode.use_storage_index() {
        StorageSizeIndex::open()
    } else {
        StorageSizeIndex::disabled("mode_requires_fresh_walk")
    };
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

    for root in roots {
        if started.elapsed() >= SCAN_TIME_BUDGET {
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
        let (_root_repositories, root_dirs, root_truncated) = scan_root(
            &root,
            &options,
            started,
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
        truncated,
        caveats: vec![
            "Cleanup cockpit: Aetower prepares evidence, reveal targets, verification commands, and Trash-first cleanup manifests."
                .to_owned(),
            "Sizes are bounded estimates and may omit paths that require additional permissions."
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

fn build_storage_hygiene_report_from_index(
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
    let cached_repository_entries = storage_index.load_repository_inventory_cache(&requested_roots);
    let repository_roots = repository_inventory_cache_roots(&cached_repository_entries);
    let limit = limit.clamp(1, MAX_LIMIT);
    let rows = storage_index.load_candidate_rows(&roots, limit, &mut metrics)?;
    if rows.is_empty() && repository_roots.is_empty() {
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
    apply_cleanup_guardrails(&mut items, now_millis);
    for item in &mut items {
        item.evidence = storage_item_evidence(item);
        item.next_step = storage_item_next_step(item);
    }
    metrics.discovered_repository_count = repository_roots.len().min(u64::MAX as usize) as u64;
    let repository_inventory_coverage =
        cached_repository_inventory_coverage(&requested_roots, &repository_roots);
    let repository_inventory_completeness =
        repository_inventory_completeness(&repository_inventory_coverage, false);
    let not_seen_repository_roots = BTreeSet::new();
    let latest_repository_roots = BTreeMap::new();
    let repository_cache_states = repository_inventory_cache_states(
        &repository_roots,
        &latest_repository_roots,
        &cached_repository_entries,
    );
    let repository_inventory = summarize_repository_inventory(
        repository_roots,
        &not_seen_repository_roots,
        &repository_cache_states,
        started,
        StorageScanMode::InstantCached,
        &mut metrics,
    );

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
    let volume_states = summarize_volume_states(&requested_roots);
    let budget_guardrails = evaluate_budget_guardrails(
        &summary,
        &repo_footprints,
        &volume_states,
        &items,
        &growth_deltas,
    );
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
        storage_index_status: format!("snapshot:{}", metrics.storage_index_status),
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
        truncated: false,
        caveats: vec![
            "Loaded from Aetower's persistent storage index for instant display.".to_owned(),
            "Run a refresh before destructive cleanup when the displayed path changed recently."
                .to_owned(),
            "Growth attribution is based on indexed size deltas and optional Aetower/Chau7 writer ledger records."
                .to_owned(),
        ],
    })
}

fn scan_root(
    root: &Path,
    options: &StorageHygieneOptions,
    started: Instant,
    now_millis: u64,
    storage_index: &StorageSizeIndex,
    collector: &mut StorageCandidateCollector,
    metrics: &mut StorageScanMetrics,
) -> (BTreeSet<PathBuf>, u64, bool) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut repositories = BTreeSet::new();
    let mut scanned_dirs = 0;
    let mut truncated = false;

    while let Some((path, depth)) = stack.pop() {
        if started.elapsed() >= SCAN_TIME_BUDGET || scanned_dirs >= MAX_DIRECTORIES {
            truncated = true;
            break;
        }
        if let Some(runtime) = options.runtime.as_ref()
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&path), 0, 0, 0)
        {
            truncated = true;
            break;
        }

        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }

        if metadata.is_dir() && is_git_repository_root(&path) {
            repositories.insert(path.clone());
        }

        if let Some(rule) = classify_artifact(&path, &metadata, now_millis) {
            let size = size_of_path(
                &path,
                &metadata,
                root,
                rule,
                started,
                options.mode,
                storage_index,
                &options.dirty_paths,
                metrics,
                now_millis,
                options.runtime.as_ref(),
            );
            if should_retain_storage_item(rule.kind, size.bytes) {
                let item = storage_item_for_path(
                    &path,
                    metadata.modified().ok(),
                    metadata.accessed().ok(),
                    rule,
                    size,
                    now_millis,
                );
                collector.push(item);
            }
            if metadata.is_dir() {
                continue;
            }
        }

        if !metadata.is_dir() || depth >= options.max_depth || is_source_control_dir(&path) {
            continue;
        }

        scanned_dirs += 1;
        if let Some(runtime) = options.runtime.as_ref()
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&path), 0, 1, 0)
        {
            truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            stack.push((entry.path(), depth + 1));
        }
    }

    (repositories, scanned_dirs, truncated)
}

fn should_retain_storage_item(kind: &str, bytes: u64) -> bool {
    bytes >= MIN_ITEM_BYTES
        || matches!(kind, "app-preferences" | "app-receipt" | "app-launch-item") && bytes > 0
}

fn storage_item_for_path(
    path: &Path,
    modified: Option<SystemTime>,
    accessed: Option<SystemTime>,
    rule: ArtifactRule,
    size: SizeWalkResult,
    now_millis: u64,
) -> StorageHygieneItem {
    let modified_millis = modified.and_then(system_time_millis);
    let age_days = modified_millis.and_then(|millis| {
        now_millis
            .checked_sub(millis)
            .map(|delta| delta / 86_400_000)
    });
    let accessed_millis = accessed.and_then(system_time_millis);
    let access_age_days = accessed_millis.and_then(|millis| {
        now_millis
            .checked_sub(millis)
            .map(|delta| delta / 86_400_000)
    });
    let cold = access_age_days.is_some_and(|days| days >= COLD_AFTER_DAYS);
    let stale = age_days.is_some_and(|days| days >= STALE_AFTER_DAYS);
    let path_display = path.display().to_string();
    let attribution = artifact_attribution(path);
    let storage_role = storage_role_for_kind(rule.kind);
    let git_status = if attribution.repo_root.is_some() {
        "repo-linked-unchecked"
    } else {
        "outside-git"
    };
    let intelligence = artifact_intelligence(rule.kind, &path_display);
    let logical_bytes = size.bytes;
    let physical_bytes = size.allocated_bytes;
    let reclaimable_bytes = if physical_bytes > 0 {
        physical_bytes
    } else {
        logical_bytes
    };
    StorageHygieneItem {
        id: path_display.clone(),
        path: path_display.clone(),
        display_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact")
            .to_owned(),
        kind: rule.kind.to_owned(),
        storage_role: storage_role.to_owned(),
        git_status: git_status.to_owned(),
        safety: rule.safety.to_owned(),
        cleanup_tier: rule.cleanup_tier.to_owned(),
        size_bytes: reclaimable_bytes,
        logical_bytes,
        physical_bytes,
        byte_accounting: storage_byte_accounting_label(logical_bytes, physical_bytes),
        sparse_or_shared: size.sparse_or_shared,
        hardlink_count: size.max_hardlink_count,
        has_hardlinks: size.has_hardlinks,
        cloud_placeholder: size.cloud_placeholder,
        protected_path: is_protected_cleanup_path(&path_display),
        size_truncated: size.truncated,
        modified_millis,
        age_days,
        accessed_millis,
        access_age_days,
        cold,
        stale,
        reason: rule.reason.to_owned(),
        recommendation: rule.recommendation.to_owned(),
        next_step: String::new(),
        command_hint: format!("du -sh {}", shell_quote(&path_display)),
        rebuild_command: intelligence.rebuild_command,
        estimated_rebuild_cost: intelligence.estimated_rebuild_cost,
        estimated_rebuild_seconds: intelligence.estimated_rebuild_seconds,
        cleanup_consequence: intelligence.cleanup_consequence,
        evidence: Vec::new(),
        cleanup_allowed: true,
        cleanup_blockers: Vec::new(),
        default_cleanup_action: "trash".to_owned(),
        attribution,
    }
}

fn storage_item_for_indexed_row(row: StorageIndexedFileRow, now_millis: u64) -> StorageHygieneItem {
    let modified_millis = row.modified_millis;
    let age_days = modified_millis.and_then(|millis| {
        now_millis
            .checked_sub(millis)
            .map(|delta| delta / 86_400_000)
    });
    let accessed_millis = row.accessed_millis;
    let access_age_days = accessed_millis.and_then(|millis| {
        now_millis
            .checked_sub(millis)
            .map(|delta| delta / 86_400_000)
    });
    let cold = access_age_days.is_some_and(|days| days >= COLD_AFTER_DAYS);
    let stale = age_days.is_some_and(|days| days >= STALE_AFTER_DAYS);
    let path = Path::new(&row.path);
    let mut attribution = artifact_attribution(path);
    if attribution.repo_root.is_none()
        && let Some(repo_root) = row.repo_root.as_deref()
    {
        attribution.repo_root = Some(repo_root.to_owned());
        attribution.repo_name = Path::new(repo_root)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        attribution.confidence = "medium".to_owned();
        attribution
            .notes
            .push("Repository attribution came from the persistent storage index.".to_owned());
    }
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact")
        .to_owned();
    let path_display = row.path.clone();
    let intelligence = artifact_intelligence(&row.kind, &path_display);
    let logical_bytes = row.logical_bytes;
    let physical_bytes = row.physical_bytes;
    let mut item = StorageHygieneItem {
        id: path_display.clone(),
        path: path_display.clone(),
        display_name,
        kind: row.kind,
        storage_role: row.storage_role,
        git_status: if attribution.repo_root.is_some() {
            "repo-linked-unchecked".to_owned()
        } else {
            "outside-git".to_owned()
        },
        safety: row.safety,
        cleanup_tier: row.cleanup_tier,
        size_bytes: if physical_bytes > 0 {
            physical_bytes
        } else {
            logical_bytes
        },
        logical_bytes,
        physical_bytes,
        byte_accounting: storage_byte_accounting_label(logical_bytes, physical_bytes),
        sparse_or_shared: physical_bytes > 0 && physical_bytes < logical_bytes,
        hardlink_count: 1,
        has_hardlinks: false,
        cloud_placeholder: logical_bytes > 0 && physical_bytes == 0,
        protected_path: is_protected_cleanup_path(&path_display),
        size_truncated: row.truncated,
        modified_millis,
        age_days,
        accessed_millis,
        access_age_days,
        cold,
        stale,
        reason: "Loaded from Aetower's persistent storage index.".to_owned(),
        recommendation:
            "Review the indexed candidate; run a refresh before destructive cleanup if the path changed recently."
                .to_owned(),
        next_step: String::new(),
        command_hint: format!("du -sh {}", shell_quote(&path_display)),
        rebuild_command: intelligence.rebuild_command,
        estimated_rebuild_cost: intelligence.estimated_rebuild_cost,
        estimated_rebuild_seconds: intelligence.estimated_rebuild_seconds,
        cleanup_consequence: intelligence.cleanup_consequence,
        evidence: Vec::new(),
        cleanup_allowed: true,
        cleanup_blockers: Vec::new(),
        default_cleanup_action: "trash".to_owned(),
        attribution,
    };
    item.evidence = storage_item_evidence(&item);
    item.next_step = storage_item_next_step(&item);
    item
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

fn storage_item_evidence(item: &StorageHygieneItem) -> Vec<String> {
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
            "Hardlink count reached {}; reclaim may be lower while another link remains.",
            item.hardlink_count
        ));
    }
    if item.cloud_placeholder {
        evidence.push(
            "This path looks cloud-backed or dehydrated: logical bytes may not be present locally."
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

fn storage_byte_accounting_label(logical_bytes: u64, physical_bytes: u64) -> String {
    if logical_bytes == 0 && physical_bytes == 0 {
        "empty path".to_owned()
    } else if physical_bytes == 0 {
        "logical fallback or cloud placeholder".to_owned()
    } else if physical_bytes < logical_bytes {
        "APFS physical blocks".to_owned()
    } else if physical_bytes > logical_bytes {
        "allocated blocks including filesystem overhead".to_owned()
    } else {
        "logical and physical bytes match".to_owned()
    }
}

fn storage_item_next_step(item: &StorageHygieneItem) -> String {
    if item.size_truncated {
        return "Reveal the path and run the copied `du` command when the machine is idle to confirm true size.".to_owned();
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

fn apply_cleanup_guardrails(items: &mut [StorageHygieneItem], now_millis: u64) {
    for item in items {
        if is_protected_cleanup_path(&item.path) {
            item.cleanup_tier = "risky".to_owned();
            item.safety = "review".to_owned();
            block_cleanup(item, "Protected system/application path.");
        }
        if item.cleanup_tier == "risky" {
            block_cleanup(
                item,
                "Risky tier requires manual review and is never unattended.",
            );
        }
        if matches!(
            item.git_status.as_str(),
            "tracked" | "modified" | "deleted" | "renamed" | "conflicted"
        ) {
            item.cleanup_tier = "risky".to_owned();
            item.safety = "review".to_owned();
            block_cleanup(
                item,
                "Path is tracked or actively changed in Git; source work is protected.",
            );
        }
        if item.git_status == "untracked" && is_source_like_storage_item(item) {
            item.cleanup_tier = "risky".to_owned();
            item.safety = "review".to_owned();
            block_cleanup(
                item,
                "Untracked source-like file is protected from automatic cleanup.",
            );
        }
        if let Some(modified_millis) = item.modified_millis
            && now_millis.saturating_sub(modified_millis) <= RECENT_CLEANUP_BLOCK_MILLIS
        {
            block_cleanup(
                item,
                "Recently modified path may still be active; wait or review manually.",
            );
        }
        if item.size_truncated {
            block_cleanup(item, "Size estimate is partial; confirm before cleanup.");
        }
        if !item.cleanup_allowed {
            item.default_cleanup_action = "manual_review".to_owned();
        }
    }
}

fn block_cleanup(item: &mut StorageHygieneItem, reason: &str) {
    item.cleanup_allowed = false;
    if !item
        .cleanup_blockers
        .iter()
        .any(|existing| existing == reason)
    {
        item.cleanup_blockers.push(reason.to_owned());
    }
}

fn is_source_like_storage_item(item: &StorageHygieneItem) -> bool {
    if matches!(
        item.storage_role.as_str(),
        "cache" | "build-artifact" | "dependency-tree" | "environment" | "temporary" | "log"
    ) {
        return false;
    }
    let extension = Path::new(&item.path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "swift"
            | "rs"
            | "go"
            | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "m"
            | "mm"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "java"
            | "kt"
            | "rb"
            | "php"
            | "cs"
            | "sql"
            | "yaml"
            | "yml"
            | "json"
            | "toml"
            | "md"
    ) || matches!(
        item.kind.as_str(),
        "large-file"
            | "cold-file"
            | "release-artifact"
            | "macos-app-bundle"
            | "app-support-data"
            | "app-container"
            | "app-launch-item"
            | "ios-backup"
            | "mail-attachments"
            | "message-attachments"
            | "xcode-archives"
            | "build-output"
            | "next-build"
    )
}

fn is_protected_cleanup_path(path: &str) -> bool {
    let protected_roots = [
        "/System",
        "/bin",
        "/sbin",
        "/usr/bin",
        "/usr/lib",
        "/usr/sbin",
        "/etc",
        "/private/etc",
        "/var/db",
        "/Library/Apple",
        "/Applications",
    ];
    protected_roots.iter().any(|root| {
        path == *root
            || path
                .strip_prefix(root)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

fn storage_role_for_kind(kind: &str) -> &'static str {
    match kind {
        "log-file" | "logs" => "log",
        "rust-build" | "swift-build" | "xcode-derived-data" | "coverage-output"
        | "build-output" | "next-build" | "release-artifact" | "xcode-archives" | "test-output" => {
            "build-artifact"
        }
        "xcode-module-cache" | "python-cache" | "frontend-cache" | "next-cache" | "tool-cache"
        | "npm-cache" | "pnpm-store" | "yarn-cache" | "app-cache" => "cache",
        "node-dependencies" | "xcode-source-packages" => "dependency-tree",
        "python-environment" | "docker-storage" | "xcode-simulator-runtime" => "environment",
        "macos-app-bundle" => "application",
        "app-support-data" | "app-container" | "app-launch-item" | "app-preferences"
        | "app-receipt" => "app-data",
        "ios-backup" | "mail-attachments" | "message-attachments" | "local-snapshot" => {
            "system-data"
        }
        "temporary-output" => "temporary",
        "cold-file" => "cold-file",
        "large-file" => "large-file",
        _ => "artifact",
    }
}

fn storage_role_label(role: &str) -> String {
    role.replace('-', " ")
}

fn git_status_label(status: &str) -> &'static str {
    match status {
        "tracked" => "tracked by git",
        "modified" => "modified in git",
        "deleted" => "deleted in git",
        "renamed" => "renamed in git",
        "conflicted" => "conflicted in git",
        "ignored" => "ignored by git",
        "generated-or-cache" => "generated/cache pattern",
        "untracked" => "untracked by git",
        "outside-git" => "outside a git repository",
        "repo-linked-unchecked" => "repo-linked, not checked yet",
        _ => "unknown",
    }
}

fn cleanup_tier_label(tier: &str) -> &'static str {
    match tier {
        "safe" => "safe",
        "rebuildable" => "rebuildable",
        "expensive" => "expensive",
        "risky" => "risky",
        _ => "unknown",
    }
}

fn artifact_intelligence(kind: &str, path: &str) -> ArtifactIntelligence {
    let quoted_path = shell_quote(path);
    match kind {
        "rust-build" => ArtifactIntelligence {
            rebuild_command: Some("cargo build".to_owned()),
            estimated_rebuild_cost: "Low to medium".to_owned(),
            estimated_rebuild_seconds: Some(420),
            cleanup_consequence:
                "Cargo will rebuild target artifacts; first build after cleanup may recompile dependencies."
                    .to_owned(),
        },
        "swift-build" => ArtifactIntelligence {
            rebuild_command: Some("swift build".to_owned()),
            estimated_rebuild_cost: "Low to medium".to_owned(),
            estimated_rebuild_seconds: Some(420),
            cleanup_consequence:
                "SwiftPM will recreate .build; package resolution and compilation may take several minutes."
                    .to_owned(),
        },
        "xcode-derived-data" | "xcode-module-cache" => ArtifactIntelligence {
            rebuild_command: Some("xcodebuild build".to_owned()),
            estimated_rebuild_cost: "Medium".to_owned(),
            estimated_rebuild_seconds: Some(600),
            cleanup_consequence:
                "Xcode will regenerate indexes, modules, and build intermediates on the next build."
                    .to_owned(),
        },
        "xcode-source-packages" => ArtifactIntelligence {
            rebuild_command: Some("xcodebuild -resolvePackageDependencies".to_owned()),
            estimated_rebuild_cost: "Medium to high".to_owned(),
            estimated_rebuild_seconds: Some(900),
            cleanup_consequence:
                "Xcode can restore package checkouts, but this may require network access and package resolution."
                    .to_owned(),
        },
        "xcode-simulator-runtime" => ArtifactIntelligence {
            rebuild_command: Some("xcrun simctl list".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(1_800),
            cleanup_consequence:
                "Simulator data may include installed apps, device state, and runtimes; use Xcode/Simulator tools for cleanup."
                    .to_owned(),
        },
        "xcode-archives" | "release-artifact" => ArtifactIntelligence {
            rebuild_command: Some("scripts/release.sh".to_owned()),
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Archives and release packages may be the only signed/notarized copy; confirm supersession before cleanup."
                    .to_owned(),
        },
        "node-dependencies" => ArtifactIntelligence {
            rebuild_command: Some("npm install / pnpm install / yarn install".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(900),
            cleanup_consequence:
                "Dependencies can be reinstalled from lockfiles, but cleanup can cost network time and break offline work."
                    .to_owned(),
        },
        "npm-cache" | "pnpm-store" | "yarn-cache" => ArtifactIntelligence {
            rebuild_command: Some("package manager install".to_owned()),
            estimated_rebuild_cost: "Medium to high".to_owned(),
            estimated_rebuild_seconds: Some(1_200),
            cleanup_consequence:
                "Package caches are refetchable but expensive; cleanup may slow future installs and require network access."
                    .to_owned(),
        },
        "next-cache" | "next-build" | "frontend-cache" => ArtifactIntelligence {
            rebuild_command: Some("npm run build".to_owned()),
            estimated_rebuild_cost: "Medium".to_owned(),
            estimated_rebuild_seconds: Some(480),
            cleanup_consequence:
                "Frontend build caches are rebuildable; the next dev/build run may be noticeably slower."
                    .to_owned(),
        },
        "python-cache" => ArtifactIntelligence {
            rebuild_command: Some("pytest / python import".to_owned()),
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(120),
            cleanup_consequence:
                "Python tools will recreate caches; first test/lint/import pass may be slower."
                    .to_owned(),
        },
        "python-environment" => ArtifactIntelligence {
            rebuild_command: Some("python -m venv .venv && pip install -r requirements.txt".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(1_200),
            cleanup_consequence:
                "Virtual environments are rebuildable from manifests but can be slow and environment-specific."
                    .to_owned(),
        },
        "docker-storage" => ArtifactIntelligence {
            rebuild_command: Some("docker system df && docker builder prune".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(1_800),
            cleanup_consequence:
                "Docker layers are reclaimable through Docker, but images/build caches may need to be pulled or rebuilt."
                    .to_owned(),
        },
        "test-output" | "coverage-output" => ArtifactIntelligence {
            rebuild_command: Some("test command / coverage command".to_owned()),
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(180),
            cleanup_consequence:
                "Reports are usually disposable after review; rerun tests to regenerate them."
                    .to_owned(),
        },
        "logs" | "log-file" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "None".to_owned(),
            estimated_rebuild_seconds: Some(0),
            cleanup_consequence:
                "Logs are not rebuildable; keep a copy first if they are needed for debugging or support."
                    .to_owned(),
        },
        "app-cache" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(60),
            cleanup_consequence:
                "App caches are usually recreated by the owning app, but the next launch may be slower."
                    .to_owned(),
        },
        "macos-app-bundle" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Removing an app bundle is an uninstall action; review related support data, containers, launch items, and receipts first."
                    .to_owned(),
        },
        "app-support-data" | "app-container" | "app-launch-item" | "app-preferences"
        | "app-receipt" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "App data can contain settings, receipts, databases, launch configuration, or user state; clean it only as part of an intentional uninstall."
                    .to_owned(),
        },
        "ios-backup" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Device backups may be the only local recovery copy; use Finder device backup management or confirm another backup exists."
                    .to_owned(),
        },
        "mail-attachments" | "message-attachments" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Attachments can be personal records; review inside the owning app or export what matters before cleanup."
                    .to_owned(),
        },
        "local-snapshot" => ArtifactIntelligence {
            rebuild_command: Some("tmutil listlocalsnapshots /".to_owned()),
            estimated_rebuild_cost: "System managed".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Local snapshots are managed by macOS; prefer Time Machine controls or tmutil over deleting files directly."
                    .to_owned(),
        },
        "temporary-output" | "tool-cache" => ArtifactIntelligence {
            rebuild_command: Some(format!("du -sh {quoted_path}")),
            estimated_rebuild_cost: "Low to medium".to_owned(),
            estimated_rebuild_seconds: Some(300),
            cleanup_consequence:
                "Tool caches and temp outputs are usually rebuildable, but active jobs may still be writing them."
                    .to_owned(),
        },
        _ => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Unknown".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Aetower cannot prove rebuildability; inspect the path and classification evidence first."
                    .to_owned(),
        },
    }
}

fn is_release_artifact_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".pkg")
        || lower.ends_with(".dmg")
        || lower.ends_with(".zip")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tar")
        || lower.ends_with(".tgz")
        || lower.ends_with(".xcarchive")
        || lower.ends_with(".xcresult")
}

fn is_docker_storage_path(path_lower: &str, name: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    let docker_root = path_lower.contains("/.docker/")
        || path_lower.contains("/docker/")
        || path_lower.contains("/com.docker.docker/");
    docker_root
        && matches!(
            name_lower.as_str(),
            "overlay2"
                | "buildkit"
                | "containers"
                | "image"
                | "volumes"
                | "vfs"
                | "aufs"
                | "btrfs"
                | "zfs"
        )
}

fn is_simulator_storage_path(path_lower: &str, name: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    (path_lower.contains("/developer/coresimulator/")
        || path_lower.contains("/coredevice/")
        || path_lower.contains("/developer/xcode/ios devicesupport/")
        || path_lower.contains("/developer/xcode/watchos devicesupport/")
        || path_lower.contains("/developer/xcode/tvos devicesupport/"))
        && matches!(
            name_lower.as_str(),
            "devices" | "runtimes" | "data" | "device support" | "ios devicesupport"
        )
}

fn is_package_cache_path(path_lower: &str, name: &str, parent_name: &str) -> Option<&'static str> {
    let name_lower = name.to_ascii_lowercase();
    let parent_lower = parent_name.to_ascii_lowercase();
    if name_lower == ".npm" || path_lower.contains("/.npm/_cacache") {
        return Some("npm-cache");
    }
    if name_lower == ".pnpm-store"
        || (name_lower == "store" && (path_lower.contains("/pnpm/") || parent_lower == "pnpm"))
        || path_lower.contains("/.local/share/pnpm/store")
    {
        return Some("pnpm-store");
    }
    if (name_lower == "cache" && parent_lower == ".yarn")
        || path_lower.contains("/.yarn/cache")
        || path_lower.contains("/yarn/cache")
    {
        return Some("yarn-cache");
    }
    None
}

fn is_library_child(path_lower: &str, folder: &str) -> bool {
    path_lower.contains(&format!("/library/{folder}/"))
}

fn is_app_support_path(path_lower: &str) -> bool {
    is_library_child(path_lower, "application support")
}

fn is_app_cache_path(path_lower: &str) -> bool {
    is_library_child(path_lower, "caches")
}

fn is_app_container_path(path_lower: &str) -> bool {
    is_library_child(path_lower, "containers") || is_library_child(path_lower, "group containers")
}

fn is_launch_item_path(path_lower: &str) -> bool {
    is_library_child(path_lower, "launchagents") || path_lower.contains("/library/launchdaemons/")
}

fn is_app_preferences_path(path_lower: &str, name: &str) -> bool {
    is_library_child(path_lower, "preferences") && name.to_ascii_lowercase().ends_with(".plist")
}

fn is_app_receipt_path(path_lower: &str, name: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    (path_lower.contains("/var/db/receipts/") || path_lower.contains("/library/receipts/"))
        && (name_lower.ends_with(".plist")
            || name_lower.ends_with(".bom")
            || name_lower.ends_with(".pkg")
            || name_lower.ends_with(".pkg/"))
}

fn is_ios_backup_path(path_lower: &str) -> bool {
    path_lower.contains("/library/application support/mobilesync/backup/")
}

fn is_mail_attachment_path(path_lower: &str) -> bool {
    path_lower.contains("/library/mail/") && path_lower.contains("/attachments/")
}

fn is_message_attachment_path(path_lower: &str) -> bool {
    path_lower.contains("/library/messages/attachments/")
}

fn is_snapshot_like_path(path_lower: &str) -> bool {
    path_lower.contains("/.mobilebackups/")
        || path_lower.contains("/com.apple.timemachine.localsnapshots/")
        || path_lower.contains("/.timemachine/")
}

fn classify_artifact(
    path: &Path,
    metadata: &fs::Metadata,
    now_millis: u64,
) -> Option<ArtifactRule> {
    let name = path.file_name()?.to_str()?;
    let parent_name = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let path_display = path.display().to_string();
    let path_lower = path_display.to_ascii_lowercase();

    if metadata.is_file() && name.ends_with(".log") {
        return Some(rule(
            "log-file",
            "safe",
            "safe",
            "Development log file.",
            "Safe to review and rotate when no current task depends on it.",
        ));
    }
    if metadata.is_file() && is_app_preferences_path(&path_lower, name) {
        return Some(rule(
            "app-preferences",
            "review",
            "risky",
            "Application preference plist.",
            "Review only as part of an app uninstall; preferences can hold settings, license state, or account hints.",
        ));
    }
    if metadata.is_file() && is_app_receipt_path(&path_lower, name) {
        return Some(rule(
            "app-receipt",
            "review",
            "risky",
            "Application install receipt.",
            "Review only as part of an app uninstall; receipts help macOS and installers understand what was installed.",
        ));
    }
    if metadata.is_file() && is_launch_item_path(&path_lower) {
        return Some(rule(
            "app-launch-item",
            "review",
            "risky",
            "App launch agent or daemon.",
            "Review launch items alongside their owning app before disabling or deleting anything.",
        ));
    }
    if metadata.is_file() && is_release_artifact_file(name) {
        return Some(rule(
            "release-artifact",
            "review",
            "risky",
            "Potential local release artifact.",
            "Review provenance and whether a newer signed/notarized artifact supersedes it before deleting.",
        ));
    }
    if metadata.is_file()
        && metadata.len() >= MIN_ITEM_BYTES
        && file_access_age_days(metadata, now_millis).is_some_and(|days| days >= COLD_AFTER_DAYS)
    {
        return Some(rule(
            "cold-file",
            "review",
            "risky",
            "File not accessed for more than a year.",
            "Reveal and inspect ownership before deleting; access time is useful evidence but not proof that the file is disposable.",
        ));
    }
    if metadata.is_file() && metadata.len() >= LARGE_FILE_BYTES {
        return Some(rule(
            "large-file",
            "review",
            "risky",
            "Large standalone file.",
            "Reveal and inspect ownership before deleting; Aetower cannot prove whether this is source data, a local export, or a generated artifact.",
        ));
    }

    if !metadata.is_dir() {
        return None;
    }

    if name.to_ascii_lowercase().ends_with(".app") {
        return Some(rule(
            "macos-app-bundle",
            "review",
            "risky",
            "macOS application bundle.",
            "Review app ownership and related support data before uninstalling or moving to Trash.",
        ));
    }

    if name.to_ascii_lowercase().ends_with(".xcarchive") {
        return Some(rule(
            "xcode-archives",
            "review",
            "risky",
            "Xcode archive or release build product.",
            "Review before deleting because archives may be the only signed or notarized release copy.",
        ));
    }

    if name.to_ascii_lowercase().ends_with(".xcresult") {
        return Some(rule(
            "test-output",
            "safe",
            "safe",
            "Xcode result bundle.",
            "Safe to remove after test review or export; rerun tests to regenerate.",
        ));
    }

    if is_docker_storage_path(&path_lower, name) {
        return Some(rule(
            "docker-storage",
            "review",
            "expensive",
            "Docker image, layer, build cache, container, or volume storage.",
            "Use Docker cleanup tools when containers are idle; refetching images and rebuilding layers can be expensive.",
        ));
    }

    if is_simulator_storage_path(&path_lower, name) {
        return Some(rule(
            "xcode-simulator-runtime",
            "review",
            "expensive",
            "Xcode simulator/device support storage.",
            "Review in Xcode or Simulator tooling; deleting can remove installed simulator apps, runtimes, or device support.",
        ));
    }

    if is_package_cache_path(&path_lower, name, parent_name) == Some("npm-cache") {
        return Some(rule(
            "npm-cache",
            "review",
            "expensive",
            "npm package cache.",
            "Review before deleting; package caches are refetchable but can make future installs slower.",
        ));
    }
    if is_package_cache_path(&path_lower, name, parent_name) == Some("pnpm-store") {
        return Some(rule(
            "pnpm-store",
            "review",
            "expensive",
            "pnpm content-addressable store.",
            "Review before deleting; pnpm can repopulate the store but future installs may require network access.",
        ));
    }
    if is_package_cache_path(&path_lower, name, parent_name) == Some("yarn-cache") {
        return Some(rule(
            "yarn-cache",
            "review",
            "expensive",
            "Yarn package cache.",
            "Review before deleting; Yarn caches are refetchable but can be intentionally checked in for offline installs.",
        ));
    }

    if is_ios_backup_path(&path_lower) {
        return Some(rule(
            "ios-backup",
            "review",
            "risky",
            "iPhone or iPad local backup.",
            "Use Finder device management or a deliberate backup policy before deleting local device backups.",
        ));
    }
    if is_mail_attachment_path(&path_lower) {
        return Some(rule(
            "mail-attachments",
            "review",
            "risky",
            "Mail attachment storage.",
            "Review in Mail or Finder before deleting; attachments can be personal or business records.",
        ));
    }
    if is_message_attachment_path(&path_lower) {
        return Some(rule(
            "message-attachments",
            "review",
            "risky",
            "Messages attachment storage.",
            "Review in Messages or Finder before deleting; attachments can be personal records.",
        ));
    }
    if is_snapshot_like_path(&path_lower) {
        return Some(rule(
            "local-snapshot",
            "review",
            "expensive",
            "Local snapshot-like storage.",
            "Prefer macOS Time Machine/snapshot tooling instead of deleting snapshot folders manually.",
        ));
    }
    if is_app_container_path(&path_lower) {
        return Some(rule(
            "app-container",
            "review",
            "risky",
            "App sandbox container or group container.",
            "Review as part of an app footprint; containers can hold user data and app state.",
        ));
    }
    if is_launch_item_path(&path_lower) {
        return Some(rule(
            "app-launch-item",
            "review",
            "risky",
            "App launch agent or daemon area.",
            "Review launch items alongside their owning app before disabling or deleting anything.",
        ));
    }
    if is_app_receipt_path(&path_lower, name) {
        return Some(rule(
            "app-receipt",
            "review",
            "risky",
            "Application install receipt.",
            "Review only as part of an app uninstall; receipts help macOS and installers understand what was installed.",
        ));
    }
    if is_app_support_path(&path_lower) && !path_lower.contains("/mobilesync") {
        return Some(rule(
            "app-support-data",
            "review",
            "risky",
            "Application Support data.",
            "Review as part of an app footprint; support data can include databases, settings, and user content.",
        ));
    }
    if is_app_cache_path(&path_lower) {
        return Some(rule(
            "app-cache",
            "safe",
            "safe",
            "Application cache data.",
            "Usually safe to remove when the owning app is quit, but cache deletion can slow the next launch.",
        ));
    }

    match name {
        "target" => Some(rule(
            "rust-build",
            "safe",
            "rebuildable",
            "Rust Cargo build output.",
            "Usually safe to remove after builds/tests are idle; Cargo will rebuild it.",
        )),
        ".build" => Some(rule(
            "swift-build",
            "safe",
            "rebuildable",
            "Swift Package Manager build output.",
            "Usually safe to remove after builds/tests are idle; SwiftPM will rebuild it.",
        )),
        "DerivedData" => Some(rule(
            "xcode-derived-data",
            "safe",
            "rebuildable",
            "Xcode DerivedData cache.",
            "Usually safe to remove when Xcode builds are idle; Xcode will recreate it.",
        )),
        "ModuleCache.noindex" => Some(rule(
            "xcode-module-cache",
            "safe",
            "rebuildable",
            "Xcode module cache.",
            "Usually safe to remove when Xcode builds are idle.",
        )),
        ".pytest_cache" | ".mypy_cache" | ".ruff_cache" | "__pycache__" => Some(rule(
            "python-cache",
            "safe",
            "rebuildable",
            "Python test/lint/import cache.",
            "Usually safe to remove; Python tools will recreate it.",
        )),
        ".turbo" | ".vite" | ".parcel-cache" => Some(rule(
            "frontend-cache",
            "safe",
            "rebuildable",
            "Frontend build cache.",
            "Usually safe to remove when frontend dev servers/builds are idle.",
        )),
        ".aethyme" | ".aetower-cache" | ".aeptus-cache" | "org.swift.swiftpm"
        | "com.apple.dt.Xcode" => Some(rule(
            "tool-cache",
            "safe",
            "rebuildable",
            "Developer tool cache.",
            "Usually safe to remove when the related toolchain is idle; tools will recreate it.",
        )),
        "coverage" => Some(rule(
            "coverage-output",
            "safe",
            "safe",
            "Test coverage output.",
            "Safe to remove if you do not need the local coverage report.",
        )),
        ".nyc_output" | "playwright-report" | "test-results" | "junit" | "reports" => Some(rule(
            "test-output",
            "safe",
            "safe",
            "Local test output.",
            "Safe to remove after test results are reviewed or exported.",
        )),
        "tmp" | "temp" => Some(rule(
            "temporary-output",
            "review",
            "safe",
            "Local temporary output directory.",
            "Review before deleting because temporary folders can contain active run output.",
        )),
        "logs" => Some(rule(
            "logs",
            "safe",
            "safe",
            "Local logs directory.",
            "Safe to review and rotate once the relevant debugging session is over.",
        )),
        "node_modules" => Some(rule(
            "node-dependencies",
            "review",
            "expensive",
            "Node dependency install tree.",
            "Review before deleting; reinstalling may take time and requires package-manager access.",
        )),
        ".venv" | "venv" => Some(rule(
            "python-environment",
            "review",
            "expensive",
            "Python virtual environment.",
            "Review before deleting; recreate from dependency manifests if still needed.",
        )),
        "dist" | "build" => Some(rule(
            "build-output",
            "review",
            "risky",
            "Generic build output directory.",
            "Review before deleting because it may contain release artifacts.",
        )),
        "SourcePackages" => Some(rule(
            "xcode-source-packages",
            "review",
            "expensive",
            "Xcode resolved package cache.",
            "Review before deleting; Xcode can rebuild it but package resolution may take time.",
        )),
        "Archives" if path_lower.contains("/developer/xcode/archives") => Some(rule(
            "xcode-archives",
            "review",
            "risky",
            "Xcode archive folder.",
            "Review before deleting because archives can contain signed distribution builds.",
        )),
        "cache" if parent_name == ".next" => Some(rule(
            "next-cache",
            "safe",
            "rebuildable",
            "Next.js build cache.",
            "Usually safe to remove when frontend builds/dev servers are idle.",
        )),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn indexed_row_for_path(
    path: &Path,
    metadata: &fs::Metadata,
    source_root: &Path,
    repo_root: Option<&str>,
    kind: &str,
    storage_role: &str,
    safety: &str,
    cleanup_tier: &str,
    logical_bytes: u64,
    physical_bytes: u64,
    entries: u64,
    truncated: bool,
    now_millis: u64,
) -> StorageIndexedFileRow {
    let device = metadata.dev() as i64;
    let inode = metadata.ino() as i64;
    let modified_millis = metadata_time_millis(metadata.mtime(), metadata.mtime_nsec());
    let changed_millis = metadata_time_millis(metadata.ctime(), metadata.ctime_nsec());
    let accessed_millis = metadata_time_millis(metadata.atime(), metadata.atime_nsec());
    StorageIndexedFileRow {
        path: path.display().to_string(),
        device,
        inode,
        file_id: format!("{device}:{inode}"),
        source_root: source_root.display().to_string(),
        repo_root: repo_root.map(str::to_owned),
        kind: kind.to_owned(),
        storage_role: storage_role.to_owned(),
        safety: safety.to_owned(),
        cleanup_tier: cleanup_tier.to_owned(),
        logical_bytes,
        physical_bytes,
        modified_millis,
        changed_millis,
        accessed_millis,
        birth_millis: metadata.created().ok().and_then(system_time_millis),
        is_directory: metadata.is_dir(),
        entries,
        truncated,
        last_scan_millis: now_millis,
    }
}

#[allow(clippy::too_many_arguments)]
fn size_of_path(
    path: &Path,
    metadata: &fs::Metadata,
    source_root: &Path,
    rule: ArtifactRule,
    started: Instant,
    mode: StorageScanMode,
    storage_index: &StorageSizeIndex,
    dirty_paths: &[String],
    metrics: &mut StorageScanMetrics,
    now_millis: u64,
    runtime: Option<&StorageScanRuntimeContext>,
) -> SizeWalkResult {
    let size_started = Instant::now();
    if metadata.file_type().is_symlink() {
        return SizeWalkResult::default();
    }
    let kind = rule.kind;
    if metadata.is_file() {
        let blocks = metadata.blocks();
        let hardlink_count = metadata.nlink();
        metrics.sized_entry_count = metrics.sized_entry_count.saturating_add(1);
        let allocated_bytes = blocks.saturating_mul(512);
        let sparse_or_shared = allocated_bytes > 0 && allocated_bytes < metadata.len();
        let cloud_placeholder = metadata.len() > 0 && allocated_bytes == 0;
        let repo_root = if mode.use_storage_index() {
            find_git_root(path).map(|root| root.display().to_string())
        } else {
            None
        };
        if let Some(runtime) = runtime
            && !runtime.checkpoint(
                STORAGE_SCAN_PHASE_ARTIFACT_SIZING,
                Some(path),
                1,
                0,
                metadata.len(),
            )
        {
            let result = SizeWalkResult {
                bytes: metadata.len(),
                allocated_bytes,
                truncated: true,
                entries: 1,
                max_hardlink_count: hardlink_count,
                has_hardlinks: hardlink_count > 1,
                sparse_or_shared,
                cloud_placeholder,
            };
            if mode.use_storage_index() {
                storage_index.store_indexed_row(
                    &indexed_row_for_path(
                        path,
                        metadata,
                        source_root,
                        repo_root.as_deref(),
                        rule.kind,
                        storage_role_for_kind(rule.kind),
                        rule.safety,
                        rule.cleanup_tier,
                        result.bytes,
                        result.allocated_bytes,
                        result.entries,
                        result.truncated,
                        now_millis,
                    ),
                    metrics,
                );
            }
            return result;
        }
        let result = SizeWalkResult {
            bytes: metadata.len(),
            allocated_bytes,
            truncated: false,
            entries: 1,
            max_hardlink_count: hardlink_count,
            has_hardlinks: hardlink_count > 1,
            sparse_or_shared,
            cloud_placeholder,
        };
        if mode.use_storage_index() {
            storage_index.store_indexed_row(
                &indexed_row_for_path(
                    path,
                    metadata,
                    source_root,
                    repo_root.as_deref(),
                    rule.kind,
                    storage_role_for_kind(rule.kind),
                    rule.safety,
                    rule.cleanup_tier,
                    result.bytes,
                    result.allocated_bytes,
                    result.entries,
                    result.truncated,
                    now_millis,
                ),
                metrics,
            );
        }
        return result;
    }
    if !metadata.is_dir() {
        return SizeWalkResult::default();
    }

    if mode.use_storage_index()
        && let Some(cached) = storage_index.lookup(path, metadata, kind, dirty_paths, metrics)
    {
        metrics.sized_entry_count = metrics.sized_entry_count.saturating_add(cached.entries);
        return cached;
    }

    let repo_root = if mode.use_storage_index() {
        find_git_root(path).map(|root| root.display().to_string())
    } else {
        None
    };
    let mut result = SizeWalkResult::default();
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        if started.elapsed() >= SCAN_TIME_BUDGET || result.entries >= mode.size_walk_entry_budget()
        {
            result.truncated = true;
            break;
        }
        if let Some(runtime) = runtime
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&current), 0, 0, 0)
        {
            result.truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            result.entries += 1;
            if metadata.is_dir() {
                stack.push(path);
            } else {
                let logical_bytes = metadata.len();
                let physical_bytes = metadata.blocks().saturating_mul(512);
                result.bytes = result.bytes.saturating_add(logical_bytes);
                result.allocated_bytes = result.allocated_bytes.saturating_add(physical_bytes);
                let hardlink_count = metadata.nlink();
                result.max_hardlink_count = result.max_hardlink_count.max(hardlink_count);
                result.has_hardlinks |= hardlink_count > 1;
                result.sparse_or_shared |= physical_bytes > 0 && physical_bytes < logical_bytes;
                result.cloud_placeholder |= logical_bytes > 0 && physical_bytes == 0;
                if mode.use_storage_index() {
                    storage_index.store_indexed_row(
                        &indexed_row_for_path(
                            &path,
                            &metadata,
                            source_root,
                            repo_root.as_deref(),
                            "indexed-file",
                            "file",
                            "",
                            "",
                            logical_bytes,
                            physical_bytes,
                            1,
                            false,
                            now_millis,
                        ),
                        metrics,
                    );
                }
                if let Some(runtime) = runtime
                    && !runtime.checkpoint(
                        STORAGE_SCAN_PHASE_ARTIFACT_SIZING,
                        Some(&path),
                        1,
                        0,
                        logical_bytes,
                    )
                {
                    result.truncated = true;
                    break;
                }
            }
        }
    }
    metrics.sized_entry_count = metrics.sized_entry_count.saturating_add(result.entries);
    metrics.size_walk_millis = metrics
        .size_walk_millis
        .saturating_add(size_started.elapsed().as_millis() as u64);
    if mode.use_storage_index() {
        storage_index.store(
            path,
            metadata,
            kind,
            repo_root.as_deref(),
            &result,
            now_millis,
            metrics,
        );
        storage_index.store_indexed_row(
            &indexed_row_for_path(
                path,
                metadata,
                source_root,
                repo_root.as_deref(),
                rule.kind,
                storage_role_for_kind(rule.kind),
                rule.safety,
                rule.cleanup_tier,
                result.bytes,
                result.allocated_bytes,
                result.entries,
                result.truncated,
                now_millis,
            ),
            metrics,
        );
    }
    result
}

impl StorageTreemapAccumulator {
    fn new(path: String, label: String, depth: usize) -> Self {
        Self {
            path,
            label,
            depth,
            ..Self::default()
        }
    }

    fn add_item(&mut self, item: &StorageHygieneItem, components: &[String]) {
        self.size_bytes = self.size_bytes.saturating_add(item.size_bytes);
        self.item_count = self.item_count.saturating_add(1);
        add_bytes(&mut self.kind_bytes, &item.kind, item.size_bytes);
        add_bytes(
            &mut self.color_bytes,
            &storage_treemap_color_key(item),
            item.size_bytes,
        );

        if components.is_empty() || self.depth >= STORAGE_TREEMAP_MAX_DEPTH {
            return;
        }

        let segment = components[0].clone();
        let child_path = Path::new(&self.path).join(&segment).display().to_string();
        let child = self.children.entry(segment.clone()).or_insert_with(|| {
            StorageTreemapAccumulator::new(child_path, segment, self.depth.saturating_add(1))
        });
        child.add_item(item, &components[1..]);
    }
}

fn build_storage_treemap_roots(
    items: &[StorageHygieneItem],
    roots: &[String],
) -> Vec<StorageTreemapNode> {
    let normalized_roots = roots
        .iter()
        .map(|root| PathBuf::from(root).display().to_string())
        .collect::<Vec<_>>();
    let mut root_nodes = BTreeMap::<String, StorageTreemapAccumulator>::new();

    for item in items.iter().take(STORAGE_TREEMAP_MAX_ITEMS) {
        let root_path = storage_treemap_root_for_item(item, &normalized_roots);
        let root_label = storage_treemap_label_for_path(&root_path);
        let components = storage_treemap_components(&item.path, &root_path);
        let root = root_nodes
            .entry(root_path.clone())
            .or_insert_with(|| StorageTreemapAccumulator::new(root_path, root_label, 0));
        root.add_item(item, &components);
    }

    let mut roots = root_nodes
        .into_values()
        .map(storage_treemap_node_from_accumulator)
        .collect::<Vec<_>>();
    roots.sort_by(storage_treemap_node_sort);
    roots
}

fn storage_treemap_node_from_accumulator(
    accumulator: StorageTreemapAccumulator,
) -> StorageTreemapNode {
    let mut children = accumulator
        .children
        .into_values()
        .collect::<Vec<StorageTreemapAccumulator>>();
    children.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.label.cmp(&right.label))
    });
    let has_more = children.len() > STORAGE_TREEMAP_MAX_CHILDREN;
    let mut visible_children = children
        .iter()
        .take(STORAGE_TREEMAP_MAX_CHILDREN)
        .cloned()
        .map(storage_treemap_node_from_accumulator)
        .collect::<Vec<_>>();
    if has_more {
        let overflow = children.iter().skip(STORAGE_TREEMAP_MAX_CHILDREN).fold(
            StorageTreemapAccumulator::new(
                format!("{}/__other", accumulator.path),
                "Other".to_owned(),
                accumulator.depth.saturating_add(1),
            ),
            |mut overflow, child| {
                overflow.size_bytes = overflow.size_bytes.saturating_add(child.size_bytes);
                overflow.item_count = overflow.item_count.saturating_add(child.item_count);
                merge_bytes(&mut overflow.kind_bytes, &child.kind_bytes);
                merge_bytes(&mut overflow.color_bytes, &child.color_bytes);
                overflow
            },
        );
        visible_children.push(storage_treemap_node_from_accumulator(overflow));
    }

    let file_type = dominant_key(&accumulator.kind_bytes).unwrap_or_else(|| "unknown".to_owned());
    let color_key = dominant_key(&accumulator.color_bytes).unwrap_or_else(|| "other".to_owned());
    let node_type = if accumulator.depth == 0 {
        "root"
    } else if visible_children.is_empty() {
        "item"
    } else {
        "folder"
    }
    .to_owned();

    StorageTreemapNode {
        id: accumulator.path.clone(),
        path: accumulator.path,
        label: accumulator.label,
        depth: accumulator.depth,
        node_type,
        file_type,
        color_key,
        size_bytes: accumulator.size_bytes,
        item_count: accumulator.item_count,
        children: visible_children,
        has_more,
    }
}

fn storage_treemap_node_sort(left: &StorageTreemapNode, right: &StorageTreemapNode) -> Ordering {
    right
        .size_bytes
        .cmp(&left.size_bytes)
        .then_with(|| left.path.cmp(&right.path))
}

fn attribute_storage_growth_delta(
    delta: &StorageGrowthDelta,
    writer_ledger: &[StorageWriterLedgerRecord],
) -> StorageGrowthAttribution {
    let repo_root = delta.repo_root.as_deref().map(Path::new);
    let git_head = repo_root.map(read_git_head).unwrap_or_default();
    let repo_name = delta.repo_root.as_deref().and_then(|root| {
        Path::new(root)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    });
    let inferred_agent_session = known_agent_path(Path::new(&delta.path))
        .map(|(_, display_name)| format!("{display_name} local artifacts"));
    let mut sources = vec!["index_delta".to_owned()];
    let matching_records = writer_ledger
        .iter()
        .filter(|record| storage_writer_record_matches_delta(record, delta))
        .collect::<Vec<_>>();
    let mut evidence = vec![format!(
        "Indexed growth delta: {} -> {} bytes.",
        delta.previous_physical_bytes, delta.current_physical_bytes
    )];

    if let Some(repo_root) = delta.repo_root.as_deref() {
        evidence.push(format!("Repo matched by indexed path: {repo_root}."));
        sources.push("repo_path".to_owned());
    }
    if let Some(session) = inferred_agent_session.as_deref() {
        evidence.push(format!("AI-agent directory evidence: {session}."));
        sources.push("agent_path".to_owned());
    }

    if matching_records.len() == 1 {
        let record = matching_records[0];
        let command = first_non_empty(record.command.as_deref(), None);
        let process_tree = first_non_empty(record.process_tree.as_deref(), None);
        let derived_agent_session = derived_writer_agent_session(record);
        let ai_agent_session = first_non_empty(
            record.ai_agent_session.as_deref(),
            derived_agent_session.as_deref(),
        );
        let ai_agent_session = first_non_empty(
            ai_agent_session.as_deref(),
            inferred_agent_session.as_deref(),
        );
        let writer_source = writer_record_source(record);
        if let Some(source) = record.source.as_deref().filter(|value| !value.is_empty()) {
            evidence.push(format!("Writer ledger source: {source}."));
        }
        if let Some(source) = writer_source.as_deref() {
            sources.push(source.to_owned());
        }
        sources.push("writer_ledger".to_owned());
        if let Some(prefix) = writer_record_path_prefixes(record).first() {
            evidence.push(format!("Writer ledger path prefix matched: {prefix}."));
        }
        if let Some(command) = command.as_deref() {
            evidence.push(format!("Command matched: {command}."));
            sources.push("command".to_owned());
        }
        if let Some(process_tree) = process_tree.as_deref() {
            evidence.push(format!("Process tree matched: {process_tree}."));
            sources.push("process_tree".to_owned());
        }
        if let Some(session) = ai_agent_session.as_deref() {
            evidence.push(format!("AI session matched: {session}."));
            sources.push("ai_session".to_owned());
        }

        return StorageGrowthAttribution {
            repo_name: first_non_empty(record.repo_name.as_deref(), repo_name.as_deref()),
            git_branch: first_non_empty(record.git_branch.as_deref(), git_head.branch.as_deref()),
            git_head: first_non_empty(record.git_head.as_deref(), git_head.short_head.as_deref()),
            command,
            process_tree,
            ai_agent_session,
            writer_source,
            matched_writer_count: 1,
            sources: unique_limited(sources, 12),
            confidence: "high".to_owned(),
            confidence_score: 92,
            ambiguous: false,
            summary: "Single writer ledger record matched this growth window.".to_owned(),
            evidence,
        };
    }

    if matching_records.len() > 1 {
        let candidate_labels = matching_records
            .iter()
            .take(4)
            .map(|record| {
                record
                    .command
                    .as_deref()
                    .or(record.ai_agent_session.as_deref())
                    .or(record.process_tree.as_deref())
                    .or(record.source.as_deref())
                    .unwrap_or("unknown writer")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        evidence.push(format!(
            "{} writer ledger records overlapped this path/time window: {}.",
            matching_records.len(),
            candidate_labels.join(", ")
        ));
        sources.push("writer_ledger".to_owned());
        sources.push("ambiguous_writers".to_owned());
        return StorageGrowthAttribution {
            repo_name,
            git_branch: git_head.branch,
            git_head: git_head.short_head,
            command: None,
            process_tree: None,
            ai_agent_session: inferred_agent_session,
            writer_source: None,
            matched_writer_count: matching_records.len().min(u64::MAX as usize) as u64,
            sources: unique_limited(sources, 12),
            confidence: "ambiguous".to_owned(),
            confidence_score: 45,
            ambiguous: true,
            summary: "Multiple writer records overlapped; Aetower will not pick a single culprit."
                .to_owned(),
            evidence,
        };
    }

    let (confidence, confidence_score, summary) = if delta.repo_root.is_some()
        && inferred_agent_session.is_some()
    {
        (
            "medium",
            74,
            "Repo and AI-agent directory evidence matched, but no command writer record was available.",
        )
    } else if delta.repo_root.is_some() {
        (
            "medium",
            64,
            "Repo/branch attribution is available from the indexed path, but command/session writer evidence is missing.",
        )
    } else if inferred_agent_session.is_some() {
        (
            "medium",
            58,
            "AI-agent directory evidence matched, but no repo or command writer record was available.",
        )
    } else {
        (
            "low",
            25,
            "Only an indexed storage delta is available; no repo, command, process tree, or AI session matched.",
        )
    };
    evidence.push("No matching writer ledger record was available for this delta.".to_owned());

    StorageGrowthAttribution {
        repo_name,
        git_branch: git_head.branch,
        git_head: git_head.short_head,
        command: None,
        process_tree: None,
        ai_agent_session: inferred_agent_session,
        writer_source: None,
        matched_writer_count: 0,
        sources: unique_limited(sources, 12),
        confidence: confidence.to_owned(),
        confidence_score,
        ambiguous: false,
        summary: summary.to_owned(),
        evidence,
    }
}

fn storage_writer_record_matches_delta(
    record: &StorageWriterLedgerRecord,
    delta: &StorageGrowthDelta,
) -> bool {
    let path_matches = writer_record_path_prefixes(record)
        .iter()
        .any(|prefix| delta.path == *prefix || delta.path.starts_with(&format!("{prefix}/")));
    let repo_matches = record
        .repo_root
        .as_deref()
        .zip(delta.repo_root.as_deref())
        .is_some_and(|(record_repo, delta_repo)| record_repo == delta_repo);
    if !path_matches && !repo_matches {
        return false;
    }
    storage_writer_record_time_matches(record, delta.scan_millis, delta.bucket_millis)
}

fn writer_record_path_prefixes(record: &StorageWriterLedgerRecord) -> Vec<String> {
    unique_limited(
        [
            record.path_prefix.as_deref(),
            record.working_directory.as_deref(),
            record.cwd.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter_map(normalized_writer_path_prefix)
        .collect(),
        4,
    )
}

fn normalized_writer_path_prefix(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.trim_end_matches('/').to_owned())
}

fn writer_record_source(record: &StorageWriterLedgerRecord) -> Option<String> {
    first_non_empty(record.source.as_deref(), record.provider.as_deref())
}

fn derived_writer_agent_session(record: &StorageWriterLedgerRecord) -> Option<String> {
    first_non_empty(
        record.ai_agent_session.as_deref(),
        record
            .session_id
            .as_deref()
            .or(record.chau7_session_id.as_deref())
            .or(record.tab_id.as_deref())
            .or(record.tab_name.as_deref()),
    )
    .map(|session| {
        if let Some(provider) = record.provider.as_deref().filter(|value| !value.is_empty()) {
            format!("{provider} session {session}")
        } else if record
            .source
            .as_deref()
            .is_some_and(|source| source.eq_ignore_ascii_case("chau7"))
        {
            format!("Chau7 session {session}")
        } else {
            session
        }
    })
}

fn storage_writer_record_time_matches(
    record: &StorageWriterLedgerRecord,
    scan_millis: u64,
    bucket_millis: u64,
) -> bool {
    if let Some(started) = record.started_at_millis {
        let ended = record.ended_at_millis.unwrap_or(scan_millis);
        return scan_millis.saturating_add(STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS) >= started
            && ended.saturating_add(STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS) >= bucket_millis;
    }
    if let Some(timestamp) = record.timestamp_millis {
        let lower = bucket_millis.saturating_sub(STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS);
        let upper = scan_millis.saturating_add(STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS);
        return (lower..=upper).contains(&timestamp);
    }
    true
}

fn load_storage_writer_ledger_records() -> Vec<StorageWriterLedgerRecord> {
    storage_writer_ledger_paths()
        .into_iter()
        .flat_map(|path| load_storage_writer_ledger_records_from_path(&path))
        .take(512)
        .collect()
}

fn storage_writer_ledger_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(path) = std::env::var("AETOWER_STORAGE_WRITER_LEDGER")
        && !path.trim().is_empty()
    {
        paths.push(PathBuf::from(path));
    }
    if let Some(base_dir) = dirs::data_local_dir() {
        paths.push(
            base_dir
                .join("Aetower")
                .join("storage-writer-ledger.ndjson"),
        );
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".aetower").join("storage-writer-ledger.ndjson"));
        paths.push(home.join(".chau7").join("storage-writer-ledger.ndjson"));
    }
    paths
}

fn load_storage_writer_ledger_records_from_path(path: &Path) -> Vec<StorageWriterLedgerRecord> {
    let Ok(metadata) = fs::metadata(path) else {
        return Vec::new();
    };
    if !metadata.is_file() || metadata.len() > STORAGE_WRITER_LEDGER_MAX_BYTES {
        return Vec::new();
    }
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .rev()
        .take(512)
        .filter_map(|line| serde_json::from_str::<StorageWriterLedgerRecord>(line).ok())
        .collect()
}

fn first_non_empty(primary: Option<&str>, fallback: Option<&str>) -> Option<String> {
    primary
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fallback.filter(|value| !value.trim().is_empty()))
        .map(str::to_owned)
}

fn storage_treemap_root_for_item(item: &StorageHygieneItem, roots: &[String]) -> String {
    roots
        .iter()
        .filter(|root| item.path == **root || item.path.starts_with(&format!("{root}/")))
        .max_by_key(|root| root.len())
        .cloned()
        .or_else(|| item.attribution.repo_root.clone())
        .unwrap_or_else(|| {
            Path::new(&item.path)
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_else(|| item.path.clone())
        })
}

fn storage_treemap_components(path: &str, root: &str) -> Vec<String> {
    let path = Path::new(path);
    let root = Path::new(root);
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut components = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|component| !component.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if components.is_empty()
        && let Some(name) = path.file_name().and_then(|name| name.to_str())
    {
        components.push(name.to_owned());
    }
    components
}

fn storage_treemap_label_for_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_owned())
}

fn storage_treemap_color_key(item: &StorageHygieneItem) -> String {
    let kind = item.kind.as_str();
    if item.cleanup_tier == "risky" || item.safety == "risky" {
        "risky"
    } else if matches!(item.cleanup_tier.as_str(), "safe" | "rebuildable") {
        match kind {
            kind if kind.contains("xcode") || kind.contains("swift") => "xcode",
            kind if kind.contains("rust") || kind.contains("cargo") => "rust",
            kind if kind.contains("node") || kind.contains("npm") || kind.contains("pnpm") => {
                "node"
            }
            kind if kind.contains("docker") => "docker",
            kind if kind.contains("log") => "log",
            kind if kind.contains("app") => "app",
            kind if kind.contains("system") || kind.contains("snapshot") => "system",
            kind if kind.contains("large") || kind.contains("cold") => "file",
            _ => "cache",
        }
    } else if item.cleanup_tier == "expensive" {
        "expensive"
    } else {
        "other"
    }
    .to_owned()
}

fn add_bytes(map: &mut BTreeMap<String, u64>, key: &str, bytes: u64) {
    let entry = map.entry(key.to_owned()).or_insert(0);
    *entry = entry.saturating_add(bytes);
}

fn merge_bytes(target: &mut BTreeMap<String, u64>, source: &BTreeMap<String, u64>) {
    for (key, bytes) in source {
        add_bytes(target, key, *bytes);
    }
}

fn dominant_key(map: &BTreeMap<String, u64>) -> Option<String> {
    map.iter()
        .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(key, _)| key.clone())
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
        .map(AppFootprintAccumulator::finish)
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

    fn finish(mut self) -> StorageAppFootprint {
        self.components.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        self.components.truncate(8);
        let component_count = self.components.len();
        let has_bundle = self
            .components
            .iter()
            .any(|component| component.component == "App bundle");
        let has_state = self.components.iter().any(|component| {
            matches!(
                component.component.as_str(),
                "Application Support" | "Container" | "Launch item" | "Preferences" | "Receipt"
            )
        });
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
            components: self.components,
            recommendation: "Treat this as an uninstall footprint: quit the app, review support data, preferences, receipts, containers, and launch items, then move selected components to Trash.".to_owned(),
        }
    }
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

fn cleanup_tier_rank(tier: &str) -> u8 {
    match tier {
        "safe" => 0,
        "rebuildable" => 1,
        "expensive" => 2,
        "risky" => 3,
        _ => 4,
    }
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

fn highest_cleanup_tier<'a>(items: impl Iterator<Item = &'a StorageHygieneItem>) -> String {
    let mut highest = "safe".to_owned();
    for item in items {
        if cleanup_tier_rank(&item.cleanup_tier) > cleanup_tier_rank(&highest) {
            highest = item.cleanup_tier.clone();
        }
    }
    highest
}

fn summarize_cleanup_tiers(items: &[StorageHygieneItem]) -> Vec<StorageCleanupTierSummary> {
    cleanup_tier_definitions()
        .into_iter()
        .map(|(tier, label, description)| {
            let mut summary = StorageCleanupTierSummary {
                tier: tier.to_owned(),
                label: label.to_owned(),
                description: description.to_owned(),
                ..StorageCleanupTierSummary::default()
            };
            for item in items.iter().filter(|item| item.cleanup_tier == tier) {
                summary.item_count += 1;
                summary.bytes = summary.bytes.saturating_add(item.size_bytes);
            }
            summary
        })
        .collect()
}

fn cleanup_tier_definitions() -> [(&'static str, &'static str, &'static str); 4] {
    [
        (
            "safe",
            "Safe",
            "Old logs, temporary output, stale crash reports, and reports that are normally safe to rotate after review.",
        ),
        (
            "rebuildable",
            "Rebuildable",
            "Compiler, package, and framework caches that should be recreated by the owning tool.",
        ),
        (
            "expensive",
            "Expensive",
            "Dependency stores and package trees that are removable but costly to restore.",
        ),
        (
            "risky",
            "Risky",
            "Generated or source-like output that may contain release artifacts or local work.",
        ),
    ]
}

fn artifact_attribution(path: &Path) -> StorageArtifactAttribution {
    let repo_root = find_git_root(path);
    let git_head = repo_root
        .as_ref()
        .map(|root| read_git_head(root))
        .unwrap_or_default();
    let inferred_agent_session =
        known_agent_path(path).map(|(_, display_name)| format!("{display_name} local artifacts"));
    let repo_name = repo_root.as_ref().and_then(|root| {
        root.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    });
    let mut notes = Vec::new();

    if repo_root.is_some() {
        notes.push("Attributed by nearest parent Git repository.".to_owned());
    } else {
        notes.push("No enclosing Git repository was found for this artifact.".to_owned());
    }
    if inferred_agent_session.is_some() {
        notes.push(
            "AI agent attribution inferred from a known local agent support directory.".to_owned(),
        );
    } else {
        notes.push(
            "Command, process-tree, and exact AI-session attribution require a file-event baseline."
                .to_owned(),
        );
    }
    let confidence = if repo_root.is_some() && git_head.branch.is_some() {
        "high"
    } else if repo_root.is_some() || inferred_agent_session.is_some() {
        "medium"
    } else {
        "low"
    };

    StorageArtifactAttribution {
        repo_root: repo_root.map(|root| root.display().to_string()),
        repo_name,
        git_branch: git_head.branch,
        git_head: git_head.short_head,
        command: None,
        process_tree: None,
        ai_agent_session: inferred_agent_session,
        confidence: confidence.to_owned(),
        notes,
    }
}

fn summarize_repo_footprints(items: &[StorageHygieneItem]) -> Vec<StorageRepoFootprint> {
    let mut grouped = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        if let Some(repo_root) = item.attribution.repo_root.as_deref() {
            grouped.entry(repo_root.to_owned()).or_default().push(item);
        }
    }

    let mut footprints: Vec<_> = grouped
        .into_iter()
        .map(|(repo_root, items)| repo_footprint_for_items(repo_root, items))
        .collect();
    footprints.sort_by(|left, right| {
        right
            .current_size_bytes
            .cmp(&left.current_size_bytes)
            .then_with(|| left.repo_name.cmp(&right.repo_name))
    });
    footprints
}

fn summarize_repository_inventory(
    repositories_by_root: BTreeMap<String, String>,
    not_seen_roots: &BTreeSet<String>,
    cache_states: &BTreeMap<String, RepositoryInventoryCacheState>,
    started: Instant,
    mode: StorageScanMode,
    metrics: &mut StorageScanMetrics,
) -> Vec<StorageRepositoryInventoryItem> {
    let mut repositories: Vec<_> = repositories_by_root
        .into_iter()
        .map(|(repo_root, discovered_root)| {
            let repo_path = Path::new(&repo_root);
            let git_started = Instant::now();
            let git = repository_git_intelligence(repo_path, started, mode);
            metrics.git_millis = metrics
                .git_millis
                .saturating_add(git_started.elapsed().as_millis() as u64);
            let quality = repository_quality(repo_path);
            let tracked_started = Instant::now();
            let tracked_agent_paths =
                git_tracked_paths(repo_path, &agent_contract_candidate_paths());
            metrics.git_millis = metrics
                .git_millis
                .saturating_add(tracked_started.elapsed().as_millis() as u64);
            let mut guidance = agent_guidance_audit(repo_path, &quality, &tracked_agent_paths);
            let contract_coverage =
                agent_contract_coverage_audit(repo_path, &quality, &guidance, &tracked_agent_paths);
            guidance.issues.extend(contract_coverage.issues.clone());
            guidance.status = guidance_status(&guidance.issues);
            let repo_name = repo_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("repository")
                .to_owned();
            let not_seen_in_latest_scan = not_seen_roots.contains(&repo_root);
            let cache_state = cache_states.get(&repo_root).cloned().unwrap_or_else(|| {
                RepositoryInventoryCacheState {
                    status: "uncached".to_owned(),
                    fingerprint: repository_inventory_fingerprint(repo_path),
                    fingerprint_changed: false,
                    last_seen_millis: None,
                    last_scan_millis: None,
                }
            });

            StorageRepositoryInventoryItem {
                id: repo_root.clone(),
                repo_root,
                repo_name,
                inventory_cache_status: cache_state.status,
                inventory_fingerprint: cache_state.fingerprint,
                inventory_fingerprint_changed: cache_state.fingerprint_changed,
                inventory_last_seen_millis: cache_state.last_seen_millis,
                inventory_last_scan_millis: cache_state.last_scan_millis,
                git_branch: git.branch,
                git_head: git.head,
                git_ref: git.reference,
                git_detached_head: git.detached_head,
                git_remote_origin_url: git.remote_origin_url,
                git_remote_key: git.remote_key,
                git_remote_host: git.remote_host,
                git_remote_owner: git.remote_owner,
                git_remote_name: git.remote_name,
                git_dirty_status: git.dirty_status,
                git_dirty_file_count: git.dirty_file_count,
                git_dirty_truncated: git.dirty_truncated,
                not_seen_in_latest_scan,
                clone_group_count: 1,
                clone_group_roots: Vec::new(),
                discovered_root,
                has_agents_md: quality.has_agents_md,
                has_claude_md: quality.has_claude_md,
                claude_md_bytes: quality.claude_md_bytes,
                claude_md_delegation_max_bytes: CLAUDE_MD_DELEGATION_MAX_BYTES,
                claude_md_delegates_to_agents_md: quality.claude_md_delegates_to_agents_md,
                agent_readiness_score: contract_coverage.score,
                agent_readiness_status: contract_coverage.status,
                agent_contract_missing_count: contract_coverage.missing_count,
                agent_contract_coverage: contract_coverage.coverage,
                agent_guidance_status: guidance.status,
                agent_guidance_issue_count: guidance.issues.len() as u64,
                agent_guidance_issues: guidance.issues,
            }
        })
        .collect();
    attach_clone_groups(&mut repositories);
    repositories.sort_by(|left, right| {
        left.repo_name
            .to_lowercase()
            .cmp(&right.repo_name.to_lowercase())
            .then_with(|| left.repo_root.cmp(&right.repo_root))
    });
    repositories
}

#[derive(Clone, Debug, Default)]
struct RepositoryGitIntelligence {
    branch: Option<String>,
    head: Option<String>,
    reference: Option<String>,
    detached_head: bool,
    remote_origin_url: Option<String>,
    remote_key: Option<String>,
    remote_host: Option<String>,
    remote_owner: Option<String>,
    remote_name: Option<String>,
    dirty_status: String,
    dirty_file_count: Option<u64>,
    dirty_truncated: bool,
}

fn repository_git_intelligence(
    repo_path: &Path,
    started: Instant,
    mode: StorageScanMode,
) -> RepositoryGitIntelligence {
    let head = read_git_head(repo_path);
    let remote_origin_url = read_git_config_value(repo_path, "remote \"origin\"", "url")
        .and_then(|value| redact_git_remote_url(&value));
    let remote_key = remote_origin_url
        .as_deref()
        .and_then(normalize_git_remote_key);
    let remote_parts = remote_key.as_deref().map(split_git_remote_key);
    let dirty = if mode.collect_git_status() {
        read_git_dirty_status(repo_path, started)
    } else {
        GitDirtyStatus {
            status: "not_checked_lazy".to_owned(),
            file_count: None,
            truncated: false,
        }
    };

    RepositoryGitIntelligence {
        branch: head.branch,
        head: head.short_head,
        reference: head.reference,
        detached_head: head.detached,
        remote_origin_url,
        remote_key,
        remote_host: remote_parts.as_ref().and_then(|parts| parts.host.clone()),
        remote_owner: remote_parts.as_ref().and_then(|parts| parts.owner.clone()),
        remote_name: remote_parts.and_then(|parts| parts.name),
        dirty_status: dirty.status,
        dirty_file_count: dirty.file_count,
        dirty_truncated: dirty.truncated,
    }
}

fn attach_clone_groups(repositories: &mut [StorageRepositoryInventoryItem]) {
    let mut roots_by_remote = BTreeMap::<String, Vec<String>>::new();
    for repository in repositories.iter() {
        if let Some(remote_key) = repository.git_remote_key.as_deref() {
            roots_by_remote
                .entry(remote_key.to_owned())
                .or_default()
                .push(repository.repo_root.clone());
        }
    }

    for repository in repositories.iter_mut() {
        let Some(remote_key) = repository.git_remote_key.as_deref() else {
            continue;
        };
        let Some(group_roots) = roots_by_remote.get(remote_key) else {
            continue;
        };
        repository.clone_group_count = group_roots.len() as u64;
        if group_roots.len() > 1 {
            repository.clone_group_roots = group_roots.clone();
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RepositoryQuality {
    has_agents_md: bool,
    has_claude_md: bool,
    claude_md_bytes: Option<u64>,
    claude_md_delegates_to_agents_md: bool,
}

#[derive(Clone, Debug, Default)]
struct AgentGuidanceAudit {
    status: String,
    issues: Vec<StorageAgentGuidanceIssue>,
}

#[derive(Clone, Copy, Debug)]
struct AgentContractDefinition {
    id: &'static str,
    label: &'static str,
    path: &'static str,
    kind: &'static str,
    weight: u64,
    missing_severity: &'static str,
    requires_schema: bool,
    requires_review: bool,
    detail: &'static str,
}

#[derive(Clone, Debug, Default)]
struct AgentContractCoverageAudit {
    score: u8,
    status: String,
    missing_count: u64,
    coverage: Vec<StorageAgentContractCoverage>,
    issues: Vec<StorageAgentGuidanceIssue>,
}

fn repository_quality(repo_root: &Path) -> RepositoryQuality {
    let has_agents_md = repo_root.join("AGENTS.md").is_file();
    let claude_md = repo_root.join("CLAUDE.md");
    let has_claude_md = claude_md.is_file();
    let claude_md_bytes = fs::metadata(&claude_md).ok().map(|metadata| metadata.len());
    let claude_md_delegates_to_agents_md = if has_claude_md
        && claude_md_bytes.is_some_and(|bytes| bytes <= CLAUDE_MD_DELEGATION_MAX_BYTES)
    {
        fs::read_to_string(&claude_md)
            .map(|content| {
                content
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim() == "@AGENTS.md")
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    } else {
        false
    };

    RepositoryQuality {
        has_agents_md,
        has_claude_md,
        claude_md_bytes,
        claude_md_delegates_to_agents_md,
    }
}

fn agent_guidance_audit(
    repo_root: &Path,
    quality: &RepositoryQuality,
    tracked_agent_paths: &BTreeSet<String>,
) -> AgentGuidanceAudit {
    let mut issues = Vec::new();
    let agents_path = repo_root.join("AGENTS.md");

    if !quality.has_agents_md {
        push_guidance_issue(
            &mut issues,
            "agents.root.missing",
            "error",
            "Missing root AGENTS.md",
            "Every repository should have a tracked root AGENTS.md as the canonical agent contract.",
            "AGENTS.md",
        );
    } else {
        if !tracked_agent_paths.contains("AGENTS.md") {
            push_guidance_issue(
                &mut issues,
                "agents.root.untracked",
                "error",
                "Root AGENTS.md is not tracked",
                "A referenced agent contract must be committed, not only present in the local worktree.",
                "AGENTS.md",
            );
        }
        if let Ok(content) = fs::read_to_string(&agents_path) {
            audit_agents_markdown(&mut issues, repo_root, "AGENTS.md", &content);
        }
    }

    if quality.has_claude_md && !quality.claude_md_delegates_to_agents_md {
        let detail = if quality
            .claude_md_bytes
            .is_some_and(|bytes| bytes > CLAUDE_MD_DELEGATION_MAX_BYTES)
        {
            "CLAUDE.md is too large to be treated as a delegating adapter."
        } else {
            "CLAUDE.md should delegate to AGENTS.md with @AGENTS.md as its first non-empty line."
        };
        push_guidance_issue(
            &mut issues,
            "agents.adapter.claude_not_delegated",
            "warning",
            "CLAUDE.md does not delegate cleanly",
            detail,
            "CLAUDE.md",
        );
    }

    audit_canonical_agent_links(&mut issues, repo_root, "README.md");
    audit_canonical_agent_links(&mut issues, repo_root, "CONTRIBUTING.md");

    let status = guidance_status(&issues);
    AgentGuidanceAudit { status, issues }
}

fn agent_contract_definitions() -> &'static [AgentContractDefinition] {
    &[
        AgentContractDefinition {
            id: "agents_md",
            label: "AGENTS.md",
            path: "AGENTS.md",
            kind: "human-contract",
            weight: 22,
            missing_severity: "error",
            requires_schema: false,
            requires_review: false,
            detail: "Human-readable operating contract: precedence, workflow, git discipline, approvals, and completion rules.",
        },
        AgentContractDefinition {
            id: "manifest",
            label: "Manifest",
            path: ".agents/manifest.yaml",
            kind: "machine-contract",
            weight: 6,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Root machine contract: expected files, schema ids, generator/check commands, cross-file integrity, and freshness policy.",
        },
        AgentContractDefinition {
            id: "tasks",
            label: "Tasks",
            path: ".agents/tasks.yaml",
            kind: "reviewed-contract",
            weight: 12,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Decision layer: task classification, required reads, likely paths, strategy, validation routing, and completion reporting.",
        },
        AgentContractDefinition {
            id: "repo_map",
            label: "Repo map",
            path: ".agents/repo-map.yaml",
            kind: "machine-contract",
            weight: 9,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Machine-readable topology: roots, packages, services, entrypoints, generated folders, and ignored roots.",
        },
        AgentContractDefinition {
            id: "contracts",
            label: "Contracts",
            path: ".agents/contracts.yaml",
            kind: "reviewed-contract",
            weight: 14,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Invariant layer: API, auth, data, release, storage, process-control, performance, UI, and security contracts agents must not break.",
        },
        AgentContractDefinition {
            id: "commands",
            label: "Commands",
            path: ".agents/commands.yaml",
            kind: "machine-contract",
            weight: 10,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Exact command registry with cwd, cost, mutation, approval, breadth, and expected runtime metadata.",
        },
        AgentContractDefinition {
            id: "validation",
            label: "Validation",
            path: ".agents/validation.yaml",
            kind: "reviewed-contract",
            weight: 12,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Touched-path to validation mapping so agents know which checks prove a change safe.",
        },
        AgentContractDefinition {
            id: "boundaries",
            label: "Boundaries",
            path: ".agents/boundaries.yaml",
            kind: "reviewed-contract",
            weight: 7,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Architecture rules agents and tools can check: layers, forbidden imports, generated-code ownership.",
        },
        AgentContractDefinition {
            id: "risks",
            label: "Risks",
            path: ".agents/risks.yaml",
            kind: "reviewed-contract",
            weight: 6,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "High-risk surfaces: auth, billing, permissions, migrations, secrets, deploy, tenants, webhooks.",
        },
        AgentContractDefinition {
            id: "references",
            label: "References",
            path: ".agents/references.yaml",
            kind: "machine-contract",
            weight: 2,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Pointers to deeper context without loading it by default: architecture, runbooks, standards, API docs.",
        },
    ]
}

fn agent_contract_candidate_paths() -> Vec<&'static str> {
    let mut paths = Vec::with_capacity(agent_contract_definitions().len());
    for definition in agent_contract_definitions() {
        paths.push(definition.path);
    }
    paths
}

fn agent_contract_coverage_audit(
    repo_root: &Path,
    quality: &RepositoryQuality,
    guidance: &AgentGuidanceAudit,
    tracked_agent_paths: &BTreeSet<String>,
) -> AgentContractCoverageAudit {
    let mut audit = AgentContractCoverageAudit::default();
    let mut earned_score = 0_u64;

    for definition in agent_contract_definitions() {
        let evaluated = evaluate_agent_contract(
            repo_root,
            quality,
            guidance,
            definition,
            tracked_agent_paths,
        );
        earned_score = earned_score.saturating_add(evaluated.earned_weight);
        if !evaluated.present {
            audit.missing_count = audit.missing_count.saturating_add(1);
        }
        if let Some(issue) = agent_contract_issue(definition, &evaluated) {
            audit.issues.push(issue);
        }
        audit.coverage.push(evaluated);
    }

    audit.score = earned_score.min(AGENT_READINESS_MAX_SCORE) as u8;
    audit.status = agent_readiness_status(audit.score, guidance, &audit.coverage);
    audit
}

fn evaluate_agent_contract(
    repo_root: &Path,
    quality: &RepositoryQuality,
    guidance: &AgentGuidanceAudit,
    definition: &AgentContractDefinition,
    tracked_agent_paths: &BTreeSet<String>,
) -> StorageAgentContractCoverage {
    let path = repo_root.join(definition.path);
    let present = path.is_file();
    let tracked = present && tracked_agent_paths.contains(definition.path);
    let mut status = "missing".to_owned();
    let mut severity = definition.missing_severity.to_owned();
    let mut detail = format!("Missing {}. {}", definition.path, definition.detail);
    let mut earned_weight = 0_u64;
    let mut schema_version = None;
    let mut generated = false;
    let mut reviewed = false;

    if present {
        let content = fs::read_to_string(&path).unwrap_or_default();
        schema_version =
            yaml_scalar(&content, "schema_version").or_else(|| yaml_scalar(&content, "version"));
        let contract_shape_issues = if definition.requires_schema {
            yaml_contract_shape_issues(repo_root, definition, &content)
        } else {
            Vec::new()
        };
        generated = yaml_boolish(&content, "generated")
            || yaml_scalar(&content, "generated_by").is_some()
            || content.contains("source_files:");
        reviewed = yaml_boolish(&content, "reviewed")
            || has_completed_human_review(&content)
            || yaml_scalar(&content, "last_reviewed").is_some();

        if !tracked {
            status = "untracked".to_owned();
            severity = if definition.path == "AGENTS.md" {
                "error".to_owned()
            } else {
                "warning".to_owned()
            };
            detail = format!(
                "{} exists but is not tracked by git. Agent contracts must be committed to be reliable.",
                definition.path
            );
            earned_weight = definition.weight / 3;
        } else if definition.path == "AGENTS.md" {
            let has_errors = guidance
                .issues
                .iter()
                .any(|issue| issue.severity == "error" && issue.path == "AGENTS.md");
            let has_warnings = guidance
                .issues
                .iter()
                .any(|issue| issue.severity == "warning" && issue.path == "AGENTS.md");
            if !quality.has_agents_md || has_errors {
                status = "error".to_owned();
                severity = "error".to_owned();
                detail = "AGENTS.md exists but has blocking contract issues.".to_owned();
                earned_weight = definition.weight / 3;
            } else if has_warnings {
                status = "partial".to_owned();
                severity = "warning".to_owned();
                detail = "AGENTS.md is tracked but has shape, reference, or workflow warnings."
                    .to_owned();
                earned_weight = definition.weight.saturating_mul(3) / 4;
            } else {
                status = "ok".to_owned();
                severity = "ok".to_owned();
                detail =
                    "AGENTS.md is present, tracked, and passes current portable checks.".to_owned();
                earned_weight = definition.weight;
            }
        } else {
            let schema_missing = definition.requires_schema && schema_version.is_none();
            let review_missing = definition.requires_review && !reviewed;
            let shape_invalid = !contract_shape_issues.is_empty();
            if schema_missing || review_missing || shape_invalid {
                status = "partial".to_owned();
                severity = "warning".to_owned();
                let mut gaps = Vec::new();
                if schema_missing {
                    gaps.push("schema_version/version".to_owned());
                }
                if review_missing {
                    gaps.push("reviewed_by/reviewed_at".to_owned());
                }
                if shape_invalid {
                    gaps.push(format!(
                        "shape checks ({})",
                        contract_shape_issues.join("; ")
                    ));
                }
                detail = format!(
                    "{} is present and tracked but has contract gaps: {}.",
                    definition.path,
                    gaps.join("; ")
                );
                earned_weight = definition.weight.saturating_mul(2) / 3;
            } else {
                status = "ok".to_owned();
                severity = "ok".to_owned();
                detail = format!(
                    "{} is present, tracked, and machine-checkable.",
                    definition.path
                );
                earned_weight = definition.weight;
            }
        }
    }

    let coverage_percent = if definition.weight == 0 {
        0
    } else {
        (earned_weight.saturating_mul(100) / definition.weight).min(100) as u8
    };

    StorageAgentContractCoverage {
        id: definition.id.to_owned(),
        label: definition.label.to_owned(),
        path: definition.path.to_owned(),
        kind: definition.kind.to_owned(),
        status,
        severity,
        detail,
        weight: definition.weight,
        earned_weight,
        coverage_percent,
        present,
        tracked,
        schema_version,
        generated,
        reviewed,
    }
}

fn agent_contract_issue(
    definition: &AgentContractDefinition,
    coverage: &StorageAgentContractCoverage,
) -> Option<StorageAgentGuidanceIssue> {
    if coverage.status == "ok" || definition.path == "AGENTS.md" {
        return None;
    }

    let id = format!("agents.contract.{}.{}", coverage.status, definition.id);
    Some(StorageAgentGuidanceIssue {
        id,
        severity: coverage.severity.clone(),
        title: match coverage.status.as_str() {
            "missing" => format!("Missing {}", definition.label),
            "untracked" => format!("{} is not tracked", definition.label),
            "partial" => format!("{} coverage is partial", definition.label),
            _ => format!("{} needs review", definition.label),
        },
        detail: coverage.detail.clone(),
        path: definition.path.to_owned(),
    })
}

fn agent_readiness_status(
    score: u8,
    guidance: &AgentGuidanceAudit,
    coverage: &[StorageAgentContractCoverage],
) -> String {
    if guidance
        .issues
        .iter()
        .any(|issue| issue.severity == "error")
        || coverage
            .iter()
            .any(|item| item.path == "AGENTS.md" && item.severity == "error")
    {
        return "blocked".to_owned();
    }
    if score >= 90 {
        "ready".to_owned()
    } else if score >= 60 {
        "partial".to_owned()
    } else {
        "weak".to_owned()
    }
}

fn yaml_scalar(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix(&prefix)?.trim();
        if value.is_empty() || value == "|" || value == ">" {
            return None;
        }
        Some(value.trim_matches('"').trim_matches('\'').to_owned())
    })
}

fn yaml_boolish(content: &str, key: &str) -> bool {
    yaml_scalar(content, key)
        .map(|value| {
            let normalized = value.to_lowercase();
            matches!(normalized.as_str(), "true" | "yes" | "1")
        })
        .unwrap_or(false)
}

fn has_completed_human_review(content: &str) -> bool {
    let reviewed_by = yaml_scalar(content, "reviewed_by")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let reviewed_at = yaml_scalar(content, "reviewed_at")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    !reviewed_by.is_empty()
        && reviewed_by != "pending-human-review"
        && reviewed_by != "pending_human_review"
        && reviewed_by != "pending"
        && !reviewed_at.is_empty()
        && reviewed_at != "pending-human-review"
        && reviewed_at != "pending_human_review"
        && reviewed_at != "pending"
}

fn yaml_contract_shape_issues(
    repo_root: &Path,
    definition: &AgentContractDefinition,
    content: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    if content.trim().is_empty() {
        issues.push("empty file".to_owned());
        return issues;
    }
    if content.lines().any(|line| line.starts_with('\t')) {
        issues.push("tab indentation".to_owned());
    }
    for key in yaml_contract_required_keys(definition.id) {
        if !yaml_has_top_level_key(content, key) {
            issues.push(format!("missing top-level `{key}`"));
        }
    }
    for path in local_markdown_paths(content) {
        if !repo_root.join(&path).exists() {
            issues.push(format!("missing local reference `{path}`"));
        }
        if issues.len() >= 6 {
            break;
        }
    }
    unique_limited(issues, 6)
}

fn yaml_contract_required_keys(contract_id: &str) -> &'static [&'static str] {
    match contract_id {
        "manifest" => &["schema_version", "contracts", "integrity"],
        "tasks" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "tasks",
        ],
        "repo_map" => &["schema_version", "generated_by", "source_files", "roots"],
        "contracts" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "contracts",
        ],
        "commands" => &["schema_version", "generated_by", "source_files", "commands"],
        "validation" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "rules",
        ],
        "boundaries" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "layers",
            "rules",
        ],
        "risks" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "surfaces",
        ],
        "references" => &[
            "schema_version",
            "generated_by",
            "source_files",
            "references",
        ],
        _ => &["schema_version"],
    }
}

fn yaml_has_top_level_key(content: &str, key: &str) -> bool {
    let prefix = format!("{key}:");
    content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && line.trim_start() == line
            && trimmed.starts_with(&prefix)
    })
}

fn audit_agents_markdown(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
    content: &str,
) {
    let line_count = content.lines().count();
    if line_count > AGENTS_MD_MAX_LINES {
        push_guidance_issue(
            issues,
            "agents.root.too_large",
            "warning",
            "AGENTS.md is too large",
            &format!(
                "Root AGENTS.md has {line_count} lines; keep it under {AGENTS_MD_MAX_LINES} lines or move generated/detail content behind references."
            ),
            relative_path,
        );
    }

    audit_required_sections(issues, relative_path, content);
    audit_duplicate_headings(issues, relative_path, content);
    audit_banned_agent_phrases(issues, relative_path, content);
    audit_broad_git_examples(issues, relative_path, content);
    audit_reference_paths(issues, repo_root, relative_path, content);
}

fn audit_required_sections(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const REQUIRED_SECTIONS: &[&str] = &[
        "Scope And Precedence",
        "Repository Map",
        "Standard Workflow",
        "Commands",
        "Approval Required",
        "Validation Matrix",
        "Architecture Boundaries",
        "Code Rules",
        "Security Rules",
        "Completion Checklist",
        "References",
    ];

    let headings: Vec<_> = markdown_headings(content)
        .into_iter()
        .filter(|heading| heading.level == 2)
        .collect();
    let mut previous_index = None;
    for required in REQUIRED_SECTIONS {
        let matches: Vec<_> = headings
            .iter()
            .enumerate()
            .filter(|(_, heading)| heading.title == *required)
            .collect();
        if matches.is_empty() {
            push_guidance_issue(
                issues,
                "agents.sections.missing",
                "warning",
                "Required AGENTS.md section is missing",
                &format!("Missing section: {required}."),
                relative_path,
            );
            continue;
        }
        if matches.len() > 1 {
            push_guidance_issue(
                issues,
                "agents.sections.duplicate_required",
                "warning",
                "Required AGENTS.md section is duplicated",
                &format!("Section appears more than once: {required}."),
                relative_path,
            );
        }
        let index = matches[0].0;
        if let Some(previous_index) = previous_index
            && index < previous_index
        {
            push_guidance_issue(
                issues,
                "agents.sections.order",
                "warning",
                "AGENTS.md section order drifted",
                &format!("Section {required} appears before an earlier required section."),
                relative_path,
            );
            break;
        }
        previous_index = Some(index);
    }
}

fn audit_duplicate_headings(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    let mut counts = BTreeMap::<String, u64>::new();
    for heading in markdown_headings(content) {
        *counts.entry(heading.title).or_default() += 1;
    }
    for (heading, count) in counts {
        if count > 1 {
            push_guidance_issue(
                issues,
                "agents.sections.duplicate_heading",
                "warning",
                "Duplicate AGENTS.md heading",
                &format!("Heading appears {count} times: {heading}."),
                relative_path,
            );
        }
    }
}

fn audit_banned_agent_phrases(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const BANNED_PHRASES: &[&str] = &[
        "agents.md",
        "Agent Playbook",
        "AI/human collaboration playbook",
        "AI Quickstart",
        "ALWAYS RUN FIRST",
        "BEFORE YOU CODE",
    ];
    for phrase in BANNED_PHRASES {
        if content.contains(phrase) {
            push_guidance_issue(
                issues,
                "agents.drift.banned_phrase",
                "warning",
                "Stale agent-guidance phrase",
                &format!("Remove or modernize stale phrase: {phrase}."),
                relative_path,
            );
        }
    }
}

fn audit_broad_git_examples(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const BANNED_GIT_EXAMPLES: &[&str] = &["git add .", "git add -A", "git commit -a"];
    for line in content.lines() {
        for command in BANNED_GIT_EXAMPLES {
            if line.contains(command) && !line_explicitly_prohibits_command(line) {
                push_guidance_issue(
                    issues,
                    "agents.git.broad_command_example",
                    "error",
                    "Broad git command example",
                    &format!("Use targeted staging examples instead of `{command}`."),
                    relative_path,
                );
            }
        }
    }
}

fn line_explicitly_prohibits_command(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("never use")
        || lower.contains("do not use")
        || lower.contains("don't use")
        || lower.contains("avoid ")
        || lower.contains("forbid")
        || lower.contains("prohibit")
}

fn audit_reference_paths(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
    content: &str,
) {
    let Some(references) = markdown_section(content, "References") else {
        return;
    };
    for candidate in local_markdown_paths(&references) {
        if !repo_root.join(&candidate).exists() {
            push_guidance_issue(
                issues,
                "agents.references.missing_path",
                "warning",
                "Referenced local path is missing",
                &format!("References points at `{candidate}`, but the path does not exist."),
                relative_path,
            );
        }
    }
}

fn audit_canonical_agent_links(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
) {
    let path = repo_root.join(relative_path);
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    if content.contains("agents.md") {
        push_guidance_issue(
            issues,
            "agents.links.casing",
            "warning",
            "Non-canonical AGENTS.md casing",
            &format!("{relative_path} should reference AGENTS.md with canonical uppercase casing."),
            relative_path,
        );
    }
}

#[derive(Clone, Debug)]
struct MarkdownHeading {
    level: usize,
    title: String,
}

fn markdown_headings(content: &str) -> Vec<MarkdownHeading> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') {
                return None;
            }
            let level = trimmed
                .chars()
                .take_while(|character| *character == '#')
                .count();
            if level == 0
                || level > 6
                || !trimmed.chars().nth(level).is_some_and(char::is_whitespace)
            {
                return None;
            }
            Some(MarkdownHeading {
                level,
                title: normalize_markdown_heading_title(
                    trimmed[level..].trim().trim_matches('#').trim(),
                ),
            })
        })
        .collect()
}

fn normalize_markdown_heading_title(title: &str) -> String {
    let title = title.trim();
    let mut saw_digit = false;
    for (index, character) in title.char_indices() {
        if character.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        if saw_digit && matches!(character, '.' | ')') {
            let rest = title[index + character.len_utf8()..].trim_start();
            if !rest.is_empty() {
                return rest.to_owned();
            }
        }
        break;
    }
    title.to_owned()
}

fn markdown_section(content: &str, title: &str) -> Option<String> {
    let mut in_section = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            let heading = trimmed
                .trim_start_matches('#')
                .trim()
                .trim_matches('#')
                .trim();
            let heading = normalize_markdown_heading_title(heading);
            if in_section {
                break;
            }
            in_section = heading == title;
            continue;
        }
        if in_section {
            lines.push(line);
        }
    }
    if in_section {
        Some(lines.join("\n"))
    } else {
        None
    }
}

fn local_markdown_paths(content: &str) -> Vec<String> {
    let mut paths = BTreeSet::<String>::new();
    for token in content
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '(' | ')' | '[' | ']' | ',' | '`' | '"' | '\'')
        })
        .map(|token| token.trim_end_matches(['.', ':', ';']))
    {
        if token.is_empty()
            || token.starts_with("http://")
            || token.starts_with("https://")
            || token.starts_with('#')
            || token.starts_with('/')
        {
            continue;
        }
        if token.contains("..") {
            continue;
        }
        let lower = token.to_lowercase();
        let has_known_reference_extension = lower.ends_with(".md")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml")
            || lower.ends_with(".json");
        if has_known_reference_extension || token.starts_with("./") {
            paths.insert(token.trim_start_matches("./").to_owned());
        }
    }
    paths.into_iter().collect()
}

fn guidance_status(issues: &[StorageAgentGuidanceIssue]) -> String {
    if issues.iter().any(|issue| issue.severity == "error") {
        "error".to_owned()
    } else if !issues.is_empty() {
        "warning".to_owned()
    } else {
        "ok".to_owned()
    }
}

fn push_guidance_issue(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    id: &str,
    severity: &str,
    title: &str,
    detail: &str,
    path: &str,
) {
    issues.push(StorageAgentGuidanceIssue {
        id: id.to_owned(),
        severity: severity.to_owned(),
        title: title.to_owned(),
        detail: detail.to_owned(),
        path: path.to_owned(),
    });
}

fn build_cleanup_recipes(items: &[StorageHygieneItem]) -> Vec<StorageCleanupRecipe> {
    let mut recipes = BTreeMap::<String, StorageCleanupRecipe>::new();
    for item in items {
        if !cleanup_item_is_trash_actionable(item) {
            continue;
        }
        for recipe in cleanup_recipes_for_item(item) {
            recipes
                .entry(recipe.id.clone())
                .and_modify(|existing| {
                    existing.estimated_reclaimable_bytes = existing
                        .estimated_reclaimable_bytes
                        .saturating_add(recipe.estimated_reclaimable_bytes);
                })
                .or_insert(recipe);
        }
    }

    let mut recipes: Vec<_> = recipes.into_values().collect();
    recipes.sort_by(|left, right| {
        right
            .estimated_reclaimable_bytes
            .cmp(&left.estimated_reclaimable_bytes)
            .then_with(|| left.title.cmp(&right.title))
    });
    recipes.truncate(16);
    recipes
}

fn cleanup_recipes_for_item(item: &StorageHygieneItem) -> Vec<StorageCleanupRecipe> {
    if !cleanup_item_is_trash_actionable(item) {
        return Vec::new();
    }
    match item.kind.as_str() {
        "rust-build" => rust_cleanup_recipe(item).into_iter().collect(),
        "swift-build" => swiftpm_cleanup_recipe(item).into_iter().collect(),
        "xcode-derived-data" => vec![derived_data_cleanup_recipe(item)],
        "xcode-module-cache" => vec![direct_reclaim_recipe(
            item,
            "xcode",
            "Clear Xcode module cache",
            "Xcode module cache is rebuildable once active Xcode builds are idle.",
            vec![
                "Close Xcode or stop active xcodebuild jobs first.".to_owned(),
                "Expect the next Xcode build to rebuild modules.".to_owned(),
            ],
            false,
        )],
        "xcode-source-packages" => vec![direct_reclaim_recipe(
            item,
            "xcode",
            "Remove Xcode source package cache",
            "Xcode can restore resolved package checkouts, but package resolution may take time and network access.",
            vec![
                "Close Xcode or stop active xcodebuild jobs first.".to_owned(),
                "Confirm package manifests are committed before deleting package caches.".to_owned(),
            ],
            true,
        )],
        "python-cache" => vec![direct_reclaim_recipe(
            item,
            "python",
            "Clear Python cache",
            "Python test, lint, and import caches are rebuildable.",
            vec!["Stop active Python test/lint runs first.".to_owned()],
            false,
        )],
        "python-environment" => vec![direct_reclaim_recipe(
            item,
            "python",
            "Remove Python virtual environment",
            "Virtual environments are rebuildable from dependency manifests but may be expensive to recreate.",
            vec![
                "Confirm requirements, lock files, or project metadata can recreate this environment.".to_owned(),
                "Stop terminals and agents using this virtual environment first.".to_owned(),
            ],
            true,
        )],
        "frontend-cache" | "next-cache" => vec![direct_reclaim_recipe(
            item,
            "frontend",
            "Clear frontend cache",
            "Frontend tool caches are rebuildable when dev servers and builds are idle.",
            vec!["Stop active frontend dev servers and builds first.".to_owned()],
            false,
        )],
        "next-build" => vec![direct_reclaim_recipe(
            item,
            "frontend",
            "Remove Next.js build output",
            "Next.js build output is rebuildable, but can include generated app artifacts that deserve review.",
            vec![
                "Stop active frontend dev servers and builds first.".to_owned(),
                "Confirm this is local build output, not a release artifact you still need.".to_owned(),
            ],
            true,
        )],
        "npm-cache" | "pnpm-store" | "yarn-cache" => vec![direct_reclaim_recipe(
            item,
            "package-cache",
            "Clear package-manager cache",
            "Package-manager caches are reclaimable but can make future installs slower or require network access.",
            vec![
                "Confirm no package install is running.".to_owned(),
                "Keep project lockfiles before clearing package-manager caches.".to_owned(),
            ],
            true,
        )],
        "node-dependencies" => vec![direct_reclaim_recipe(
            item,
            "node",
            "Remove Node dependency tree",
            "node_modules can be reclaimed but reinstalling may be slow and requires package-manager access.",
            vec![
                "Confirm package-lock, pnpm-lock, yarn.lock, or bun.lockb is present before deleting.".to_owned(),
                "Stop dev servers, test runners, and agents using this dependency tree first.".to_owned(),
            ],
            true,
        )],
        "tool-cache" => vec![direct_reclaim_recipe(
            item,
            "tools",
            "Clear developer tool cache",
            "Tool caches are rebuildable when the owning toolchain is idle.",
            vec!["Stop active builds, package managers, and agents using this cache first.".to_owned()],
            false,
        )],
        "app-cache" => vec![direct_reclaim_recipe(
            item,
            "app-cache",
            "Clear app cache",
            "Application caches are usually safe to remove after the owning app is quit.",
            vec![
                "Quit the owning app first.".to_owned(),
                "Keep support data and containers unless you are intentionally uninstalling the app."
                    .to_owned(),
            ],
            false,
        )],
        "coverage-output" => vec![direct_reclaim_recipe(
            item,
            "tests",
            "Remove coverage output",
            "Coverage reports are usually safe to remove after you no longer need local inspection artifacts.",
            vec!["Keep a copy first if this coverage report is needed for review or support.".to_owned()],
            false,
        )],
        "test-output" => vec![direct_reclaim_recipe(
            item,
            "tests",
            "Remove local test output",
            "Test reports and result bundles are usually safe after review or export.",
            vec!["Keep a copy first if the report is needed for review, CI triage, or support.".to_owned()],
            false,
        )],
        "docker-storage" => vec![docker_cleanup_recipe(item)],
        "xcode-simulator-runtime" => vec![direct_reclaim_recipe(
            item,
            "xcode-simulator",
            "Review simulator/device support storage",
            "Simulator and device-support storage can be reclaimed through Xcode tools, but may contain local app/device state.",
            vec![
                "Close Simulator and Xcode first.".to_owned(),
                "Prefer Xcode Settings or `xcrun simctl` cleanup over deleting active device folders.".to_owned(),
            ],
            true,
        )],
        "temporary-output" => vec![direct_reclaim_recipe(
            item,
            "temporary",
            "Remove temporary output",
            "Temporary folders can be reclaimed, but may contain active run output.",
            vec!["Review the folder contents and stop active jobs before deleting.".to_owned()],
            true,
        )],
        "log-file" | "logs" => vec![log_cleanup_recipe(item)],
        "build-output" | "xcode-archives" => {
            stale_release_artifact_recipe(item).into_iter().collect()
        }
        _ => Vec::new(),
    }
}

fn cleanup_item_is_trash_actionable(item: &StorageHygieneItem) -> bool {
    item.cleanup_allowed
        && item.default_cleanup_action == "trash"
        && item.cleanup_blockers.is_empty()
        && !item.size_truncated
}

fn build_cleanup_bundles(items: &[StorageHygieneItem]) -> Vec<StorageCleanupBundle> {
    let mut bundles = Vec::new();
    let safe_candidates: Vec<_> = items
        .iter()
        .filter(|item| {
            cleanup_item_is_trash_actionable(item)
                && item.safety == "safe"
                && matches!(item.cleanup_tier.as_str(), "safe" | "rebuildable")
        })
        .collect();
    if let Some(bundle) = cleanup_bundle_for_items(
        "safe-reclaim",
        "Reclaim safely",
        "High-confidence local artifacts. Review the manifest, then stage approved paths for Finder Trash cleanup.",
        "safe",
        safe_candidates,
    ) {
        bundles.push(bundle);
    }

    let review_candidates: Vec<_> = items
        .iter()
        .filter(|item| {
            cleanup_item_is_trash_actionable(item)
                && item.cleanup_tier != "risky"
                && !(item.safety == "safe"
                    && matches!(item.cleanup_tier.as_str(), "safe" | "rebuildable"))
        })
        .collect();
    if let Some(bundle) = cleanup_bundle_for_items(
        "review-reclaim",
        "Review reclaim plan",
        "Operator-reviewed candidates such as dependency trees or local environments. Treat as planning data, not an automatic cleanup.",
        "review",
        review_candidates,
    ) {
        bundles.push(bundle);
    }

    bundles.sort_by(|left, right| {
        right
            .estimated_reclaimable_bytes
            .cmp(&left.estimated_reclaimable_bytes)
            .then_with(|| right.confidence_score.cmp(&left.confidence_score))
            .then_with(|| left.title.cmp(&right.title))
    });
    bundles.truncate(3);
    bundles
}

fn cleanup_bundle_for_items(
    id: &str,
    title: &str,
    subtitle: &str,
    safety: &str,
    items: Vec<&StorageHygieneItem>,
) -> Option<StorageCleanupBundle> {
    if items.is_empty() {
        return None;
    }
    let mut manifest: Vec<_> = items.into_iter().map(cleanup_bundle_item).collect();
    manifest.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });

    let estimated_reclaimable_bytes = manifest
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    let confidence_score = average_confidence(&manifest);
    let dry_run_commands = manifest
        .iter()
        .take(16)
        .map(|item| item.dry_run_command.clone())
        .collect();
    let rollback_notes = unique_limited(
        manifest
            .iter()
            .map(|item| item.rollback_note.clone())
            .collect(),
        6,
    );

    Some(StorageCleanupBundle {
        id: id.to_owned(),
        title: format!("{title}: {}", human_bytes(estimated_reclaimable_bytes)),
        subtitle: subtitle.to_owned(),
        safety: safety.to_owned(),
        confidence_score,
        estimated_reclaimable_bytes,
        item_count: manifest.len(),
        dry_run_only: true,
        manifest,
        dry_run_commands,
        rollback_notes,
        prerequisites: cleanup_bundle_prerequisites(safety),
        caveats: vec![
            "Run verification commands first. In-app cleanup moves approved paths to Finder Trash; command references are manual only."
                .to_owned(),
            "Verify active builds, tests, terminals, and agents are idle before moving cleanup targets."
                .to_owned(),
        ],
    })
}

fn cleanup_bundle_item(item: &StorageHygieneItem) -> StorageCleanupBundleItem {
    let cleanup_command = cleanup_recipes_for_item(item)
        .into_iter()
        .next()
        .map(|recipe| recipe.command);
    StorageCleanupBundleItem {
        path: item.path.clone(),
        display_name: item.display_name.clone(),
        kind: item.kind.clone(),
        cleanup_tier: item.cleanup_tier.clone(),
        safety: item.safety.clone(),
        size_bytes: item.size_bytes,
        confidence_score: cleanup_item_confidence(item),
        dry_run_command: item.command_hint.clone(),
        cleanup_command,
        rollback_note: cleanup_item_rollback_note(item),
        reason: item.reason.clone(),
        consequence: item.cleanup_consequence.clone(),
        evidence: item.evidence.clone(),
        cleanup_allowed: item.cleanup_allowed,
        cleanup_blockers: item.cleanup_blockers.clone(),
        default_cleanup_action: item.default_cleanup_action.clone(),
    }
}

fn cleanup_bundle_prerequisites(safety: &str) -> Vec<String> {
    let mut prerequisites = vec![
        "Run the dry-run commands first and confirm the manifest matches your intent.".to_owned(),
        "Stop active builds, tests, package managers, and AI agents that may be writing these paths."
            .to_owned(),
    ];
    if safety != "safe" {
        prerequisites.push(
            "Review every candidate manually; this bundle intentionally includes non-safe items."
                .to_owned(),
        );
    }
    prerequisites
}

fn cleanup_item_confidence(item: &StorageHygieneItem) -> u8 {
    let base: u8 = match item.cleanup_tier.as_str() {
        "rebuildable" if item.safety == "safe" => 94,
        "safe" if item.safety == "safe" => 88,
        "expensive" => 62,
        "risky" => 35,
        _ => 70,
    };
    let stale_bonus: u8 = if item.stale { 3 } else { 0 };
    let truncation_penalty: u8 = if item.size_truncated { 20 } else { 0 };
    (base + stale_bonus)
        .saturating_sub(truncation_penalty)
        .clamp(1, 99)
}

fn cleanup_item_rollback_note(item: &StorageHygieneItem) -> String {
    match item.cleanup_tier.as_str() {
        "rebuildable" => {
            "Rollback: rebuild or rerun the owning package manager/tool; source files are not expected to be affected.".to_owned()
        }
        "safe" if item.kind == "log-file" || item.kind == "logs" => {
            "Rollback: log truncation is not reversible unless you copy the log first; keep evidence needed for support.".to_owned()
        }
        "safe" => {
            "Rollback: usually not needed, but keep a copy first if this artifact is required for debugging.".to_owned()
        }
        "expensive" => {
            "Rollback: reinstall dependencies or recreate the environment from manifests; expect network and rebuild cost.".to_owned()
        }
        "risky" => {
            "Rollback: restore from source control, release archives, or backups; do not automate without manual confirmation.".to_owned()
        }
        _ => "Rollback depends on the owning tool; review before cleanup.".to_owned(),
    }
}

fn average_confidence(items: &[StorageCleanupBundleItem]) -> u8 {
    if items.is_empty() {
        return 0;
    }
    let total: u64 = items.iter().map(|item| item.confidence_score as u64).sum();
    (total / items.len() as u64) as u8
}

fn unique_limited(values: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            unique.push(value);
        }
        if unique.len() >= limit {
            break;
        }
    }
    unique
}

fn evaluate_budget_guardrails(
    summary: &StorageHygieneSummary,
    repo_footprints: &[StorageRepoFootprint],
    volume_states: &[StorageVolumeState],
    items: &[StorageHygieneItem],
    growth_deltas: &[StorageGrowthDelta],
) -> StorageBudgetGuardrails {
    let mut violations = Vec::new();

    if summary.total_reclaimable_bytes > TOTAL_ARTIFACT_BUDGET_BYTES {
        violations.push(StorageBudgetViolation {
            id: "total-artifact-budget".to_owned(),
            scope: "global".to_owned(),
            severity: "warning".to_owned(),
            title: "Local dev artifacts exceed budget".to_owned(),
            detail: format!(
                "Aetower found {} of local development artifacts; budget is {}.",
                human_bytes(summary.total_reclaimable_bytes),
                human_bytes(TOTAL_ARTIFACT_BUDGET_BYTES)
            ),
            repo_root: None,
            repo_name: None,
            observed_bytes: summary.total_reclaimable_bytes,
            limit_bytes: TOTAL_ARTIFACT_BUDGET_BYTES,
            recommendation: "Review cleanup recipes and reclaim rebuildable caches before the machine starts paging or indexing excessively.".to_owned(),
        });
    }

    for footprint in repo_footprints {
        if footprint.current_size_bytes > REPO_ARTIFACT_BUDGET_BYTES {
            violations.push(StorageBudgetViolation {
                id: format!("repo-artifact-budget|{}", footprint.repo_root),
                scope: "repo".to_owned(),
                severity: "warning".to_owned(),
                title: format!("{} exceeds repo artifact budget", footprint.repo_name),
                detail: format!(
                    "{} has {} of attributed artifacts; budget is {} per repository.",
                    footprint.repo_name,
                    human_bytes(footprint.current_size_bytes),
                    human_bytes(REPO_ARTIFACT_BUDGET_BYTES)
                ),
                repo_root: Some(footprint.repo_root.clone()),
                repo_name: Some(footprint.repo_name.clone()),
                observed_bytes: footprint.current_size_bytes,
                limit_bytes: REPO_ARTIFACT_BUDGET_BYTES,
                recommendation: "Clean rebuildable build outputs or narrow expensive dependency caches for this repository.".to_owned(),
            });
        }
        if let Some(growth_bytes) = footprint.growth_bytes
            && growth_bytes > REPO_GROWTH_BUDGET_BYTES_PER_DAY as i64
        {
            violations.push(StorageBudgetViolation {
                id: format!("repo-growth-budget|{}", footprint.repo_root),
                scope: "repo-growth".to_owned(),
                severity: "warning".to_owned(),
                title: format!("{} is growing too fast", footprint.repo_name),
                detail: format!(
                    "{} grew by {} in {}; budget is {} per day.",
                    footprint.repo_name,
                    human_bytes(growth_bytes as u64),
                    footprint.growth_window,
                    human_bytes(REPO_GROWTH_BUDGET_BYTES_PER_DAY)
                ),
                repo_root: Some(footprint.repo_root.clone()),
                repo_name: Some(footprint.repo_name.clone()),
                observed_bytes: growth_bytes as u64,
                limit_bytes: REPO_GROWTH_BUDGET_BYTES_PER_DAY,
                recommendation: "Inspect the disk growth timeline and active build/session activity before the repository accumulates more artifacts.".to_owned(),
            });
        }
    }

    for volume in volume_states {
        let pressure_percent = percent_u64(volume.available_bytes, volume.total_bytes);
        if volume.available_bytes < FREE_SPACE_FLOOR_BYTES
            || pressure_percent < VOLUME_PRESSURE_FLOOR_PERCENT
        {
            let severity = if volume.available_bytes < FREE_SPACE_FLOOR_BYTES / 2 {
                "critical"
            } else {
                "warning"
            };
            violations.push(StorageBudgetViolation {
                id: format!("volume-pressure|{}", volume.device_id),
                scope: "volume-pressure".to_owned(),
                severity: severity.to_owned(),
                title: format!("Volume {} is under storage pressure", volume.path),
                detail: format!(
                    "{} available on {}; floor is {} or {}%.",
                    human_bytes(volume.available_bytes),
                    volume.path,
                    human_bytes(FREE_SPACE_FLOOR_BYTES),
                    VOLUME_PRESSURE_FLOOR_PERCENT
                ),
                repo_root: None,
                repo_name: None,
                observed_bytes: volume.available_bytes,
                limit_bytes: FREE_SPACE_FLOOR_BYTES,
                recommendation: "Stage Safe and Rebuildable cleanup first; Aetower will not delete anything automatically unless Safe-tier auto-trash is explicitly enabled.".to_owned(),
            });
        }
    }

    let policies = storage_prevention_policies();
    let prevention_suggestions =
        storage_prevention_suggestions(summary, repo_footprints, items, growth_deltas, &violations);
    let scheduled_scan_recommended = !violations.is_empty() || !prevention_suggestions.is_empty();
    let status = if violations.is_empty() {
        "ok"
    } else if violations
        .iter()
        .any(|violation| violation.severity == "critical")
    {
        "critical"
    } else {
        "warning"
    }
    .to_owned();

    StorageBudgetGuardrails {
        repo_growth_budget_bytes_per_day: REPO_GROWTH_BUDGET_BYTES_PER_DAY,
        repo_artifact_budget_bytes: REPO_ARTIFACT_BUDGET_BYTES,
        total_artifact_budget_bytes: TOTAL_ARTIFACT_BUDGET_BYTES,
        free_space_floor_bytes: FREE_SPACE_FLOOR_BYTES,
        volume_pressure_floor_percent: VOLUME_PRESSURE_FLOOR_PERCENT,
        warning_only_by_default: true,
        auto_trash_safe_tier_enabled: false,
        scheduled_scan_recommended,
        scheduled_scan_interval_hours: SCHEDULED_SCAN_INTERVAL_HOURS,
        status,
        violations,
        policies,
        prevention_suggestions,
    }
}

fn storage_prevention_policies() -> Vec<StoragePreventionPolicy> {
    vec![
        StoragePreventionPolicy {
            id: "repo-growth-budget".to_owned(),
            title: "Repository growth budget".to_owned(),
            mode: "warn".to_owned(),
            enabled: true,
            action: "warn".to_owned(),
            tier: "prevention".to_owned(),
            detail: format!(
                "Warn when a repository grows more than {} per day.",
                human_bytes(REPO_GROWTH_BUDGET_BYTES_PER_DAY)
            ),
            next_step: "Inspect the growth timeline, then stage rebuildable artifacts if the build/session is finished.".to_owned(),
        },
        StoragePreventionPolicy {
            id: "dev-artifact-budget".to_owned(),
            title: "Developer artifact budget".to_owned(),
            mode: "warn".to_owned(),
            enabled: true,
            action: "warn".to_owned(),
            tier: "prevention".to_owned(),
            detail: format!(
                "Warn when local developer artifacts exceed {} total or {} in one repo.",
                human_bytes(TOTAL_ARTIFACT_BUDGET_BYTES),
                human_bytes(REPO_ARTIFACT_BUDGET_BYTES)
            ),
            next_step: "Prefer Safe and Rebuildable cleanup bundles before touching package stores or source-like files.".to_owned(),
        },
        StoragePreventionPolicy {
            id: "volume-pressure-floor".to_owned(),
            title: "Free-space floor".to_owned(),
            mode: "warn".to_owned(),
            enabled: true,
            action: "warn".to_owned(),
            tier: "prevention".to_owned(),
            detail: format!(
                "Warn when any scanned volume drops below {} or {}% available.",
                human_bytes(FREE_SPACE_FLOOR_BYTES),
                VOLUME_PRESSURE_FLOOR_PERCENT
            ),
            next_step: "Stage cleanup, then move to Finder Trash only after reviewing the manifest.".to_owned(),
        },
        StoragePreventionPolicy {
            id: "scheduled-scan".to_owned(),
            title: "Optional scheduled scan".to_owned(),
            mode: "opt-in".to_owned(),
            enabled: false,
            action: "suggest".to_owned(),
            tier: "prevention".to_owned(),
            detail: format!(
                "Suggested cadence is every {} hours when storage pressure or repeated growth is detected.",
                SCHEDULED_SCAN_INTERVAL_HOURS
            ),
            next_step: "Enable later from Settings; current public behavior remains manual/warning-only.".to_owned(),
        },
        StoragePreventionPolicy {
            id: "safe-tier-auto-trash".to_owned(),
            title: "Safe-tier auto-trash".to_owned(),
            mode: "explicit-opt-in".to_owned(),
            enabled: false,
            action: "trash-after-approval".to_owned(),
            tier: "safe-only".to_owned(),
            detail: "Disabled by default. If enabled later, it may only target Safe-tier items that already pass Trash guardrails.".to_owned(),
            next_step: "Use Stage cleanup for now; never auto-trash Rebuildable, Expensive, Risky, source-like, tracked, modified, or recent files.".to_owned(),
        },
    ]
}

fn storage_prevention_suggestions(
    summary: &StorageHygieneSummary,
    repo_footprints: &[StorageRepoFootprint],
    items: &[StorageHygieneItem],
    growth_deltas: &[StorageGrowthDelta],
    violations: &[StorageBudgetViolation],
) -> Vec<StoragePreventionSuggestion> {
    let mut suggestions = Vec::new();

    if let Some(bytes) = safe_reclaimable_bytes(items)
        && bytes > 0
    {
        suggestions.push(StoragePreventionSuggestion {
            id: "safe-reclaim-before-pressure".to_owned(),
            trigger: "safe-reclaim".to_owned(),
            title: "Stage Safe reclaim before pressure rises".to_owned(),
            detail: "Safe-tier candidates can be reviewed and moved to Finder Trash without touching source-like work.".to_owned(),
            action_label: "Stage Safe cleanup".to_owned(),
            estimated_reclaimable_bytes: bytes,
            safety: "safe".to_owned(),
            requires_approval: true,
        });
    }

    if summary.total_reclaimable_bytes > TOTAL_ARTIFACT_BUDGET_BYTES / 2 {
        suggestions.push(StoragePreventionSuggestion {
            id: "developer-artifacts-drift".to_owned(),
            trigger: "artifact-budget".to_owned(),
            title: "Set a developer artifact cleanup habit".to_owned(),
            detail: format!(
                "{} of local artifacts are visible now; review rebuildable outputs before package stores.",
                human_bytes(summary.total_reclaimable_bytes)
            ),
            action_label: "Review developer artifacts".to_owned(),
            estimated_reclaimable_bytes: summary.total_reclaimable_bytes,
            safety: "review".to_owned(),
            requires_approval: true,
        });
    }

    if let Some((repo, bytes)) = post_build_cleanup_candidate(repo_footprints, items, growth_deltas)
    {
        suggestions.push(StoragePreventionSuggestion {
            id: format!("post-build-cleanup|{}", repo.repo_root),
            trigger: "post-build".to_owned(),
            title: format!("Post-build cleanup available for {}", repo.repo_name),
            detail: "A recent build/session grew this repo and left rebuildable artifacts. Stage cleanup only after the build and related agents are idle.".to_owned(),
            action_label: "Stage post-build cleanup".to_owned(),
            estimated_reclaimable_bytes: bytes,
            safety: "rebuildable".to_owned(),
            requires_approval: true,
        });
    }

    if !violations.is_empty() {
        suggestions.push(StoragePreventionSuggestion {
            id: "scheduled-scan-recommended".to_owned(),
            trigger: "policy-violation".to_owned(),
            title: "Consider scheduled warning scans".to_owned(),
            detail: format!(
                "{} policy warning{} detected. A daily warning-only scan would catch this earlier.",
                violations.len(),
                if violations.len() == 1 { "" } else { "s" }
            ),
            action_label: "Review scan policy".to_owned(),
            estimated_reclaimable_bytes: 0,
            safety: "non-destructive".to_owned(),
            requires_approval: false,
        });
    }

    suggestions.truncate(6);
    suggestions
}

fn safe_reclaimable_bytes(items: &[StorageHygieneItem]) -> Option<u64> {
    let bytes = items
        .iter()
        .filter(|item| item.cleanup_tier == "safe" && cleanup_item_is_trash_actionable(item))
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    (bytes > 0).then_some(bytes)
}

fn post_build_cleanup_candidate<'a>(
    repo_footprints: &'a [StorageRepoFootprint],
    items: &[StorageHygieneItem],
    growth_deltas: &[StorageGrowthDelta],
) -> Option<(&'a StorageRepoFootprint, u64)> {
    let grown_repo = growth_deltas
        .iter()
        .filter(|delta| delta.delta_bytes > 0)
        .filter(|delta| {
            delta.cleanup_tier == "rebuildable"
                || delta
                    .command
                    .as_deref()
                    .is_some_and(command_looks_like_build)
                || delta
                    .attribution_summary
                    .to_ascii_lowercase()
                    .contains("writer ledger")
        })
        .filter_map(|delta| delta.repo_root.as_deref())
        .find_map(|repo_root| {
            repo_footprints
                .iter()
                .find(|footprint| footprint.repo_root == repo_root)
        })?;
    let bytes = items
        .iter()
        .filter(|item| {
            item.attribution
                .repo_root
                .as_deref()
                .is_some_and(|repo_root| repo_root == grown_repo.repo_root)
                && item.cleanup_tier == "rebuildable"
                && cleanup_item_is_trash_actionable(item)
        })
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    (bytes > 0).then_some((grown_repo, bytes))
}

fn command_looks_like_build(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    [
        "build",
        "xcodebuild",
        "cargo",
        "swift",
        "npm",
        "pnpm",
        "yarn",
        "make",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

fn percent_u64(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        0
    } else {
        numerator.saturating_mul(100) / denominator
    }
}

#[derive(Clone, Debug)]
struct AgentAttributionEvidence {
    id: String,
    provider: String,
    display_name: String,
    session_id: Option<String>,
    confidence: String,
    source: String,
}

#[derive(Clone, Debug)]
struct AgentArtifactAccumulator {
    id: String,
    provider: String,
    display_name: String,
    session_id: Option<String>,
    artifact_bytes: u64,
    week_artifact_bytes: u64,
    rebuildable_bytes: u64,
    week_rebuildable_bytes: u64,
    item_count: usize,
    repos: BTreeMap<String, AgentRepoAccumulator>,
    top_items: Vec<StorageAgentItemSummary>,
    confidence: String,
    confidence_rank: u8,
    attribution_sources: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
struct AgentRepoAccumulator {
    repo_root: String,
    repo_name: String,
    artifact_bytes: u64,
    item_count: usize,
}

impl AgentArtifactAccumulator {
    fn new(evidence: AgentAttributionEvidence) -> Self {
        let mut attribution_sources = BTreeSet::new();
        attribution_sources.insert(evidence.source);
        let confidence_rank = confidence_rank(&evidence.confidence);
        Self {
            id: evidence.id,
            provider: evidence.provider,
            display_name: evidence.display_name,
            session_id: evidence.session_id,
            artifact_bytes: 0,
            week_artifact_bytes: 0,
            rebuildable_bytes: 0,
            week_rebuildable_bytes: 0,
            item_count: 0,
            repos: BTreeMap::new(),
            top_items: Vec::new(),
            confidence: evidence.confidence,
            confidence_rank,
            attribution_sources,
        }
    }

    fn merge_evidence(&mut self, evidence: AgentAttributionEvidence) {
        self.attribution_sources.insert(evidence.source);
        let rank = confidence_rank(&evidence.confidence);
        if rank > self.confidence_rank {
            self.confidence = evidence.confidence;
            self.confidence_rank = rank;
        }
        if self.session_id.is_none() {
            self.session_id = evidence.session_id;
        }
    }

    fn add_item(&mut self, item: &StorageHygieneItem) {
        self.artifact_bytes = self.artifact_bytes.saturating_add(item.size_bytes);
        self.item_count += 1;
        let is_rebuildable = item.cleanup_tier == "rebuildable";
        if is_rebuildable {
            self.rebuildable_bytes = self.rebuildable_bytes.saturating_add(item.size_bytes);
        }
        if item.age_days.is_some_and(|days| days <= 7) {
            self.week_artifact_bytes = self.week_artifact_bytes.saturating_add(item.size_bytes);
            if is_rebuildable {
                self.week_rebuildable_bytes =
                    self.week_rebuildable_bytes.saturating_add(item.size_bytes);
            }
        }

        if let Some(repo_root) = item.attribution.repo_root.as_deref() {
            let repo =
                self.repos
                    .entry(repo_root.to_owned())
                    .or_insert_with(|| AgentRepoAccumulator {
                        repo_root: repo_root.to_owned(),
                        repo_name: item
                            .attribution
                            .repo_name
                            .clone()
                            .or_else(|| {
                                Path::new(repo_root)
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .map(str::to_owned)
                            })
                            .unwrap_or_else(|| "repository".to_owned()),
                        ..Default::default()
                    });
            repo.artifact_bytes = repo.artifact_bytes.saturating_add(item.size_bytes);
            repo.item_count += 1;
        }

        self.top_items.push(StorageAgentItemSummary {
            path: item.path.clone(),
            display_name: item.display_name.clone(),
            kind: item.kind.clone(),
            cleanup_tier: item.cleanup_tier.clone(),
            size_bytes: item.size_bytes,
            modified_millis: item.modified_millis,
        });
    }

    fn into_summary(mut self) -> StorageAgentArtifactSummary {
        let repo_count = self.repos.len();
        let mut top_repositories: Vec<_> = self
            .repos
            .into_values()
            .map(|repo| StorageAgentRepoSummary {
                repo_root: repo.repo_root,
                repo_name: repo.repo_name,
                artifact_bytes: repo.artifact_bytes,
                item_count: repo.item_count,
            })
            .collect();
        top_repositories.sort_by(|left, right| {
            right
                .artifact_bytes
                .cmp(&left.artifact_bytes)
                .then_with(|| left.repo_name.cmp(&right.repo_name))
        });
        top_repositories.truncate(4);

        self.top_items.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        self.top_items.truncate(4);

        StorageAgentArtifactSummary {
            id: self.id,
            provider: self.provider,
            display_name: self.display_name,
            session_id: self.session_id,
            artifact_bytes: self.artifact_bytes,
            week_artifact_bytes: self.week_artifact_bytes,
            rebuildable_bytes: self.rebuildable_bytes,
            rebuildable_percent: percent(self.rebuildable_bytes, self.artifact_bytes),
            week_rebuildable_bytes: self.week_rebuildable_bytes,
            week_rebuildable_percent: percent(
                self.week_rebuildable_bytes,
                self.week_artifact_bytes,
            ),
            item_count: self.item_count,
            repo_count,
            top_repositories,
            top_items: self.top_items,
            confidence: self.confidence,
            attribution_sources: self.attribution_sources.into_iter().collect(),
            recommendation: agent_cleanup_recommendation(
                self.artifact_bytes,
                self.rebuildable_bytes,
                self.week_artifact_bytes,
            ),
        }
    }
}

fn summarize_agent_hygiene(items: &[StorageHygieneItem]) -> StorageAgentHygieneSummary {
    let mut grouped = BTreeMap::<String, AgentArtifactAccumulator>::new();

    for item in items {
        let Some(evidence) = infer_agent_evidence(item) else {
            continue;
        };
        grouped
            .entry(evidence.id.clone())
            .and_modify(|entry| entry.merge_evidence(evidence.clone()))
            .or_insert_with(|| AgentArtifactAccumulator::new(evidence))
            .add_item(item);
    }

    let mut agents: Vec<_> = grouped
        .into_values()
        .map(AgentArtifactAccumulator::into_summary)
        .collect();
    agents.sort_by(|left, right| {
        right
            .week_artifact_bytes
            .cmp(&left.week_artifact_bytes)
            .then_with(|| right.artifact_bytes.cmp(&left.artifact_bytes))
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    agents.truncate(12);

    let total_agent_artifact_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.artifact_bytes)
    });
    let week_agent_artifact_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.week_artifact_bytes)
    });
    let rebuildable_agent_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.rebuildable_bytes)
    });
    let week_rebuildable_agent_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.week_rebuildable_bytes)
    });
    let attributed_item_count = agents.iter().fold(0usize, |total, agent| {
        total.saturating_add(agent.item_count)
    });

    StorageAgentHygieneSummary {
        total_agent_artifact_bytes,
        week_agent_artifact_bytes,
        rebuildable_agent_bytes,
        rebuildable_agent_percent: percent(rebuildable_agent_bytes, total_agent_artifact_bytes),
        week_rebuildable_agent_bytes,
        week_rebuildable_agent_percent: percent(
            week_rebuildable_agent_bytes,
            week_agent_artifact_bytes,
        ),
        attributed_item_count,
        agent_count: agents.len(),
        agents,
        caveats: vec![
            "Agent cost is direct attribution only: explicit AI session metadata, command/process-tree evidence, or known local agent directories."
                .to_owned(),
            "Repository build artifacts are not blamed on an agent unless Aetower has writer evidence."
                .to_owned(),
        ],
    }
}

fn infer_agent_evidence(item: &StorageHygieneItem) -> Option<AgentAttributionEvidence> {
    if let Some(session) = item.attribution.ai_agent_session.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(session)
    {
        let inferred_local_artifacts = session.ends_with(" local artifacts");
        return Some(AgentAttributionEvidence {
            id: if inferred_local_artifacts {
                format!("known-path|{provider}")
            } else {
                format!("session|{}|{}", provider, session)
            },
            provider,
            display_name,
            session_id: Some(session.to_owned()),
            confidence: if inferred_local_artifacts {
                "medium".to_owned()
            } else {
                "high".to_owned()
            },
            source: if inferred_local_artifacts {
                "known_agent_directory".to_owned()
            } else {
                "ai_agent_session".to_owned()
            },
        });
    }

    if let Some(command) = item.attribution.command.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(command)
    {
        return Some(AgentAttributionEvidence {
            id: format!("command|{provider}"),
            provider,
            display_name,
            session_id: None,
            confidence: "medium".to_owned(),
            source: "command".to_owned(),
        });
    }

    if let Some(process_tree) = item.attribution.process_tree.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(process_tree)
    {
        return Some(AgentAttributionEvidence {
            id: format!("process-tree|{provider}"),
            provider,
            display_name,
            session_id: None,
            confidence: "medium".to_owned(),
            source: "process_tree".to_owned(),
        });
    }

    let path = Path::new(&item.path);
    known_agent_path(path).map(|(provider, display_name)| AgentAttributionEvidence {
        id: format!("known-path|{provider}"),
        provider,
        display_name,
        session_id: None,
        confidence: "medium".to_owned(),
        source: "known_agent_directory".to_owned(),
    })
}

fn agent_provider_from_text(value: &str) -> Option<(String, String)> {
    let lowered = value.to_ascii_lowercase();
    if lowered.contains("claude") {
        return Some(("claude".to_owned(), "Claude Code".to_owned()));
    }
    if lowered.contains("codex") {
        return Some(("codex".to_owned(), "Codex".to_owned()));
    }
    if lowered.contains("cursor-agent") || lowered.contains("cursor") {
        return Some(("cursor".to_owned(), "Cursor".to_owned()));
    }
    if lowered.contains("aider") {
        return Some(("aider".to_owned(), "Aider".to_owned()));
    }
    None
}

fn known_agent_path(path: &Path) -> Option<(String, String)> {
    for component in path.components() {
        let normalized = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        match normalized.as_str() {
            ".claude" => return Some(("claude".to_owned(), "Claude Code".to_owned())),
            ".codex" => return Some(("codex".to_owned(), "Codex".to_owned())),
            ".cursor" => return Some(("cursor".to_owned(), "Cursor".to_owned())),
            ".aider" => return Some(("aider".to_owned(), "Aider".to_owned())),
            _ => {}
        }
    }
    None
}

fn agent_cleanup_recommendation(
    artifact_bytes: u64,
    rebuildable_bytes: u64,
    week_artifact_bytes: u64,
) -> String {
    if artifact_bytes == 0 {
        return "No agent-attributed storage pressure detected.".to_owned();
    }
    let rebuildable_percent = percent(rebuildable_bytes, artifact_bytes);
    if rebuildable_percent >= 75.0 {
        return "Most attributed bytes are rebuildable; clean them after the agent session and related builds are idle.".to_owned();
    }
    if week_artifact_bytes >= REPO_GROWTH_BUDGET_BYTES_PER_DAY {
        return "This agent added significant storage this week; inspect the top items before starting more build-heavy tasks.".to_owned();
    }
    "Review top items and prefer targeted cleanup recipes over broad cache deletion.".to_owned()
}

fn percent(part: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    ((part as f64 / total as f64) * 1000.0).round() as f32 / 10.0
}

fn confidence_rank(confidence: &str) -> u8 {
    match confidence {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn direct_reclaim_recipe(
    item: &StorageHygieneItem,
    category: &str,
    title: &str,
    reason: &str,
    mut prerequisites: Vec<String>,
    requires_review: bool,
) -> StorageCleanupRecipe {
    prerequisites
        .push("Reveal and inspect the target before running the copied command.".to_owned());
    let command = if Path::new(&item.path).is_file() {
        format!("rm -f {}", shell_quote(&item.path))
    } else {
        format!("rm -rf {}", shell_quote(&item.path))
    };
    StorageCleanupRecipe {
        id: format!("{category}-reclaim|{}", item.path),
        title: title.to_owned(),
        category: category.to_owned(),
        safety: item.cleanup_tier.clone(),
        affected_path: item.path.clone(),
        command,
        estimated_reclaimable_bytes: item.size_bytes,
        reason: reason.to_owned(),
        prerequisites,
        destructive: true,
        requires_review: requires_review || item.safety != "safe",
    }
}

fn rust_cleanup_recipe(item: &StorageHygieneItem) -> Option<StorageCleanupRecipe> {
    let path = Path::new(&item.path);
    let manifest_dir = nearest_parent_with_file(path, "Cargo.toml")?;
    let manifest_path = manifest_dir.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap_or_default();
    let package = if manifest.contains("aetower-ffi") {
        Some("aetower-ffi")
    } else {
        None
    };
    let command = if let Some(package) = package {
        format!(
            "cd {} && cargo clean -p {package}",
            shell_quote(&manifest_dir.display().to_string())
        )
    } else {
        format!(
            "cd {} && cargo clean",
            shell_quote(&manifest_dir.display().to_string())
        )
    };
    Some(StorageCleanupRecipe {
        id: format!("rust-clean|{}", manifest_dir.display()),
        title: package
            .map(|package| format!("Clean Rust package {package}"))
            .unwrap_or_else(|| "Clean Rust build artifacts".to_owned()),
        category: "rust".to_owned(),
        safety: "rebuildable".to_owned(),
        affected_path: item.path.clone(),
        command,
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Cargo build output is rebuildable and can usually be reclaimed with cargo clean once builds/tests are idle.".to_owned(),
        prerequisites: vec![
            "Stop active cargo builds/tests first.".to_owned(),
            "Expect the next Rust build to recompile dependencies.".to_owned(),
        ],
        destructive: true,
        requires_review: false,
    })
}

fn swiftpm_cleanup_recipe(item: &StorageHygieneItem) -> Option<StorageCleanupRecipe> {
    let package_root = Path::new(&item.path).parent()?;
    Some(StorageCleanupRecipe {
        id: format!("swiftpm-clean|{}", package_root.display()),
        title: "Clear SwiftPM .build".to_owned(),
        category: "swiftpm".to_owned(),
        safety: "rebuildable".to_owned(),
        affected_path: item.path.clone(),
        command: format!(
            "cd {} && swift package clean",
            shell_quote(&package_root.display().to_string())
        ),
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "SwiftPM .build output is rebuildable; swift package clean removes package build products without touching sources.".to_owned(),
        prerequisites: vec![
            "Stop active Swift builds/tests first.".to_owned(),
            "Expect the next Swift build to fetch or rebuild dependencies if needed.".to_owned(),
        ],
        destructive: true,
        requires_review: false,
    })
}

fn docker_cleanup_recipe(item: &StorageHygieneItem) -> StorageCleanupRecipe {
    StorageCleanupRecipe {
        id: format!("docker-clean|{}", item.path),
        title: "Review Docker storage cleanup".to_owned(),
        category: "docker".to_owned(),
        safety: item.cleanup_tier.clone(),
        affected_path: item.path.clone(),
        command: "docker system df && docker builder prune".to_owned(),
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Docker storage should be reclaimed through Docker so active containers, named volumes, and layer metadata stay consistent.".to_owned(),
        prerequisites: vec![
            "Stop containers and builds that are actively using Docker.".to_owned(),
            "Run `docker system df` first to review images, build cache, containers, and volumes.".to_owned(),
            "Use `docker builder prune` or targeted Docker cleanup before deleting files manually.".to_owned(),
        ],
        destructive: true,
        requires_review: true,
    }
}

fn derived_data_cleanup_recipe(item: &StorageHygieneItem) -> StorageCleanupRecipe {
    StorageCleanupRecipe {
        id: format!("xcode-derived-data|{}", item.path),
        title: "Prune old Xcode DerivedData".to_owned(),
        category: "xcode".to_owned(),
        safety: "rebuildable".to_owned(),
        affected_path: item.path.clone(),
        command: format!(
            "find {} -mindepth 1 -maxdepth 1 -mtime +14 -exec rm -rf {{}} +",
            shell_quote(&item.path)
        ),
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Old DerivedData entries for inactive projects are rebuildable but can consume significant disk.".to_owned(),
        prerequisites: vec![
            "Close Xcode or stop active xcodebuild jobs first.".to_owned(),
            "Review the matching folders with the same find command using -print before running deletion.".to_owned(),
        ],
        destructive: true,
        requires_review: true,
    }
}

fn log_cleanup_recipe(item: &StorageHygieneItem) -> StorageCleanupRecipe {
    let path = Path::new(&item.path);
    let command = if path.is_file() {
        format!(": > {}", shell_quote(&item.path))
    } else {
        format!(
            "find {} -type f -name '*.log' -size +50M -exec sh -c ': > \"$1\"' sh {{}} \\;",
            shell_quote(&item.path)
        )
    };
    StorageCleanupRecipe {
        id: format!("logs-truncate|{}", item.path),
        title: "Truncate oversized local logs".to_owned(),
        category: "logs".to_owned(),
        safety: "safe".to_owned(),
        affected_path: item.path.clone(),
        command,
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Large local logs are usually safe to truncate after the relevant debugging session is over.".to_owned(),
        prerequisites: vec![
            "Keep a copy first if the log is needed for a bug report.".to_owned(),
            "Avoid truncating logs that are actively being collected for support.".to_owned(),
        ],
        destructive: true,
        requires_review: item.safety != "safe",
    }
}

fn stale_release_artifact_recipe(item: &StorageHygieneItem) -> Option<StorageCleanupRecipe> {
    if !item.stale {
        return None;
    }
    Some(StorageCleanupRecipe {
        id: format!("release-artifacts|{}", item.path),
        title: "Remove stale release artifacts".to_owned(),
        category: "release".to_owned(),
        safety: "risky".to_owned(),
        affected_path: item.path.clone(),
        command: format!(
            "find {} -type f \\( -name '*.zip' -o -name '*.pkg' -o -name '*.dmg' -o -name '*.tar.gz' \\) -mtime +14 -delete",
            shell_quote(&item.path)
        ),
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Stale release folders can contain packages superseded by newer versions, but Aetower cannot prove supersession without release metadata.".to_owned(),
        prerequisites: vec![
            "Confirm newer signed/notarized artifacts exist before deleting old packages.".to_owned(),
            "Run the same find expression with -print instead of -delete first.".to_owned(),
        ],
        destructive: true,
        requires_review: true,
    })
}

fn nearest_parent_with_file(path: &Path, file_name: &str) -> Option<PathBuf> {
    let mut current = path.parent()?.to_path_buf();
    loop {
        if current.join(file_name).is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn repo_footprint_for_items(
    repo_root: String,
    items: Vec<&StorageHygieneItem>,
) -> StorageRepoFootprint {
    let repo_name = items
        .iter()
        .find_map(|item| item.attribution.repo_name.clone())
        .or_else(|| {
            Path::new(&repo_root)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "repository".to_owned());
    let artifact_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    let safe_bytes = tier_bytes(&items, "safe");
    let rebuildable_bytes = tier_bytes(&items, "rebuildable");
    let expensive_bytes = tier_bytes(&items, "expensive");
    let risky_bytes = tier_bytes(&items, "risky");
    let rebuildable_percent = percent(rebuildable_bytes, artifact_bytes);
    let mut top_artifact_folders: Vec<_> = items
        .iter()
        .map(|item| StorageRepoArtifactFolder {
            path: item.path.clone(),
            display_name: item.display_name.clone(),
            kind: item.kind.clone(),
            cleanup_tier: item.cleanup_tier.clone(),
            size_bytes: item.size_bytes,
        })
        .collect();
    top_artifact_folders.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    top_artifact_folders.truncate(5);

    let artifact_mix = repo_artifact_mix(&items);
    let last_branch_touched = latest_branch_for_items(&items);
    let (estimated_rebuild_cost, estimated_rebuild_seconds) = estimate_rebuild_cost(&items);
    let optimization_summary = repo_optimization_summary(
        &repo_name,
        artifact_bytes,
        rebuildable_bytes,
        expensive_bytes,
        risky_bytes,
        estimated_rebuild_seconds,
    );

    StorageRepoFootprint {
        id: repo_root.clone(),
        repo_root,
        repo_name,
        current_size_bytes: artifact_bytes,
        artifact_bytes,
        safe_bytes,
        rebuildable_bytes,
        expensive_bytes,
        risky_bytes,
        rebuildable_percent,
        item_count: items.len(),
        top_artifact_folders,
        artifact_mix,
        duplicate_clone_count: 1,
        duplicate_clone_roots: Vec::new(),
        last_writer_process: None,
        last_writer_pid: None,
        last_branch_touched,
        growth_bytes: None,
        growth_window: "No prior scan baseline in this process.".to_owned(),
        estimated_rebuild_cost,
        estimated_rebuild_seconds,
        optimization_summary,
        caveats: vec![
            "Current size is the bounded attributed artifact footprint, not a full source checkout size."
                .to_owned(),
            "Last writer process and exact growth require a file-event journal or scan baseline."
                .to_owned(),
        ],
    }
}

fn apply_growth_deltas_to_repo_footprints(
    footprints: &mut [StorageRepoFootprint],
    growth_deltas: &[StorageGrowthDelta],
) {
    for footprint in footprints {
        let matching = growth_deltas
            .iter()
            .filter(|delta| {
                delta.delta_bytes > 0
                    && delta
                        .repo_root
                        .as_deref()
                        .is_some_and(|repo_root| repo_root == footprint.repo_root)
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        let growth_bytes = matching
            .iter()
            .fold(0i64, |total, delta| total.saturating_add(delta.delta_bytes));
        let strongest = matching
            .iter()
            .max_by(|left, right| {
                left.attribution_confidence_score
                    .cmp(&right.attribution_confidence_score)
                    .then_with(|| left.delta_bytes.cmp(&right.delta_bytes))
            })
            .copied();
        footprint.growth_bytes = Some(growth_bytes);
        footprint.growth_window = "since indexed storage baseline".to_owned();
        if let Some(delta) = strongest {
            footprint.last_branch_touched = delta
                .git_branch
                .clone()
                .or(delta.git_head.clone())
                .or_else(|| footprint.last_branch_touched.clone());
            footprint.last_writer_process = delta
                .command
                .clone()
                .or(delta.process_tree.clone())
                .or(delta.ai_agent_session.clone());
            if footprint.last_writer_process.is_some() {
                footprint.caveats.push(format!(
                    "Growth attribution confidence: {} ({}%).",
                    delta.attribution_confidence, delta.attribution_confidence_score
                ));
            } else {
                footprint.caveats.push(
                    "Growth is repo-attributed, but command/session writer evidence is missing."
                        .to_owned(),
                );
            }
        }
    }
}

fn apply_clone_groups_to_repo_footprints(
    footprints: &mut [StorageRepoFootprint],
    repository_inventory: &[StorageRepositoryInventoryItem],
) {
    let clone_groups_by_root = repository_inventory
        .iter()
        .map(|repository| {
            (
                repository.repo_root.as_str(),
                (
                    repository.clone_group_count,
                    repository.clone_group_roots.clone(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for footprint in footprints {
        if let Some((count, roots)) = clone_groups_by_root.get(footprint.repo_root.as_str()) {
            footprint.duplicate_clone_count = *count;
            footprint.duplicate_clone_roots = roots.clone();
        }
    }
}

fn repo_artifact_mix(items: &[&StorageHygieneItem]) -> Vec<StorageRepoArtifactMix> {
    let mut grouped = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        grouped.entry(item.kind.clone()).or_default().push(*item);
    }
    let mut mix = grouped
        .into_iter()
        .map(|(kind, group)| {
            let bytes = group
                .iter()
                .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
            let cleanup_tier = highest_cleanup_tier(group.iter().copied());
            let path = group
                .first()
                .map(|item| item.path.as_str())
                .unwrap_or_default();
            let intelligence = artifact_intelligence(&kind, path);
            StorageRepoArtifactMix {
                kind: kind.clone(),
                label: artifact_kind_label(&kind).to_owned(),
                item_count: group.len(),
                bytes,
                cleanup_tier,
                rebuild_command: intelligence.rebuild_command,
                estimated_rebuild_cost: intelligence.estimated_rebuild_cost,
                estimated_rebuild_seconds: intelligence.estimated_rebuild_seconds,
            }
        })
        .collect::<Vec<_>>();
    mix.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.label.cmp(&right.label))
    });
    mix.truncate(8);
    mix
}

fn artifact_kind_label(kind: &str) -> &'static str {
    match kind {
        "xcode-derived-data" => "Xcode DerivedData",
        "swift-build" => "SwiftPM .build",
        "rust-build" => "Cargo target",
        "node-dependencies" => "Node dependencies",
        "npm-cache" => "npm cache",
        "pnpm-store" => "pnpm store",
        "yarn-cache" => "Yarn cache",
        "docker-storage" => "Docker storage",
        "xcode-simulator-runtime" => "Xcode simulators",
        "xcode-archives" => "Xcode archives",
        "release-artifact" => "Release artifacts",
        "log-file" | "logs" => "Logs",
        "test-output" => "Test outputs",
        "coverage-output" => "Coverage outputs",
        "next-cache" => "Next.js cache",
        "next-build" => "Next.js build",
        "frontend-cache" => "Frontend cache",
        "python-cache" => "Python cache",
        "python-environment" => "Python environment",
        "tool-cache" => "Tool cache",
        _ => "Storage artifact",
    }
}

fn latest_branch_for_items(items: &[&StorageHygieneItem]) -> Option<String> {
    items
        .iter()
        .filter_map(|item| {
            let reference = item
                .attribution
                .git_branch
                .as_ref()
                .or(item.attribution.git_head.as_ref())?;
            let modified_millis = item.modified_millis.unwrap_or_default();
            Some((modified_millis, reference.clone()))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, reference)| reference)
}

fn estimate_rebuild_cost(items: &[&StorageHygieneItem]) -> (String, Option<u64>) {
    let expensive_bytes = tier_bytes(items, "expensive");
    let risky_bytes = tier_bytes(items, "risky");
    let rebuildable_bytes = tier_bytes(items, "rebuildable");
    let safe_bytes = tier_bytes(items, "safe");
    let total_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));

    if risky_bytes > 0 {
        return ("Review first".to_owned(), None);
    }
    if total_bytes == 0 {
        return ("None".to_owned(), Some(0));
    }
    let gib = 1_024_f64 * 1_024_f64 * 1_024_f64;
    let estimated = ((rebuildable_bytes as f64 / gib) * 25.0)
        + ((expensive_bytes as f64 / gib) * 120.0)
        + ((safe_bytes as f64 / gib) * 5.0);
    let seconds = estimated.round().clamp(60.0, 3_600.0) as u64;
    let label = if expensive_bytes >= 2 * 1_024 * 1_024 * 1_024 || seconds >= 1_200 {
        "High"
    } else if expensive_bytes > 0 || seconds >= 300 {
        "Medium"
    } else {
        "Low"
    };
    (label.to_owned(), Some(seconds))
}

fn repo_optimization_summary(
    repo_name: &str,
    artifact_bytes: u64,
    rebuildable_bytes: u64,
    expensive_bytes: u64,
    risky_bytes: u64,
    estimated_rebuild_seconds: Option<u64>,
) -> String {
    if artifact_bytes == 0 {
        return format!(
            "{repo_name} has no attributed reclaimable development artifacts in this scan."
        );
    }
    let dominant =
        if risky_bytes > 0 && risky_bytes >= rebuildable_bytes && risky_bytes >= expensive_bytes {
            "needs manual review"
        } else if rebuildable_bytes >= expensive_bytes {
            "mostly rebuildable"
        } else {
            "mostly expensive to restore"
        };
    let time = estimated_rebuild_seconds
        .map(|seconds| {
            format!(
                ", estimated rebuild cost {}",
                rebuild_seconds_label(seconds)
            )
        })
        .unwrap_or_else(|| ", rebuild cost requires review".to_owned());
    format!(
        "{repo_name} has {} reclaimable attributed artifacts, {dominant}{time}.",
        human_bytes(artifact_bytes)
    )
}

fn tier_bytes(items: &[&StorageHygieneItem], tier: &str) -> u64 {
    items
        .iter()
        .filter(|item| item.cleanup_tier == tier)
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes))
}

fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[derive(Clone, Debug, Default)]
struct GitHead {
    branch: Option<String>,
    short_head: Option<String>,
    reference: Option<String>,
    detached: bool,
}

#[derive(Clone, Debug)]
struct GitRemoteParts {
    host: Option<String>,
    owner: Option<String>,
    name: Option<String>,
}

#[derive(Clone, Debug)]
struct GitDirtyStatus {
    status: String,
    file_count: Option<u64>,
    truncated: bool,
}

fn read_git_head(repo_root: &Path) -> GitHead {
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return GitHead::default();
    };
    let Ok(head) = fs::read_to_string(git_dir.join("HEAD")) else {
        return GitHead::default();
    };
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: ") {
        let branch = reference
            .strip_prefix("refs/heads/")
            .unwrap_or(reference)
            .to_owned();
        let head_sha = fs::read_to_string(git_dir.join(reference))
            .ok()
            .map(|value| short_hash(value.trim()))
            .or_else(|| read_packed_ref(&git_dir, reference).map(|value| short_hash(&value)));
        return GitHead {
            branch: Some(branch),
            short_head: head_sha,
            reference: Some(reference.to_owned()),
            detached: false,
        };
    }

    if head.is_empty() {
        GitHead::default()
    } else {
        GitHead {
            branch: None,
            short_head: Some(short_hash(head)),
            reference: Some(head.to_owned()),
            detached: true,
        }
    }
}

fn read_git_config_value(repo_root: &Path, target_section: &str, key: &str) -> Option<String> {
    let git_dir = resolve_git_dir(repo_root)?;
    let config = fs::read_to_string(git_dir.join("config")).ok()?;
    let mut active_section: Option<String> = None;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            active_section = Some(trimmed[1..trimmed.len().saturating_sub(1)].to_owned());
            continue;
        }
        if active_section.as_deref() != Some(target_section) {
            continue;
        }
        let Some((candidate_key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if candidate_key.trim() == key {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn normalize_git_remote_key(remote_url: &str) -> Option<String> {
    let mut value = remote_url.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(stripped) = value.strip_prefix("https://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("http://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("ssh://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("git@") {
        value = stripped;
    }
    value = strip_remote_userinfo(value);
    if let Some((host, path)) = value.split_once(':')
        && !host.contains('/')
        && !path.is_empty()
    {
        return Some(clean_remote_key(&format!("{host}/{path}")));
    }
    Some(clean_remote_key(value))
}

fn redact_git_remote_url(remote_url: &str) -> Option<String> {
    let value = remote_url
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if value.is_empty() {
        return None;
    }
    if let Some((scheme, rest)) = value.split_once("://") {
        return Some(format!("{scheme}://{}", strip_remote_userinfo(rest)));
    }
    if let Some(stripped) = value.strip_prefix("git@") {
        return Some(stripped.to_owned());
    }
    if let Some((user_host, path)) = value.split_once(':')
        && user_host.contains('@')
        && !user_host.contains('/')
        && !path.is_empty()
    {
        let host = user_host.rsplit('@').next().unwrap_or_default();
        return Some(format!("{host}:{path}"));
    }
    Some(strip_remote_userinfo(value).to_owned())
}

fn strip_remote_userinfo(value: &str) -> &str {
    let first_slash = value.find('/');
    if let Some(at) = value.find('@')
        && first_slash.is_none_or(|slash| at < slash)
    {
        return &value[at + 1..];
    }
    value
}

fn clean_remote_key(value: &str) -> String {
    let mut cleaned = value
        .split(['?', '#'])
        .next()
        .unwrap_or(value)
        .trim_matches('/')
        .trim()
        .to_lowercase();
    if let Some(stripped) = cleaned.strip_suffix(".git") {
        cleaned = stripped.to_owned();
    }
    cleaned
}

fn split_git_remote_key(remote_key: &str) -> GitRemoteParts {
    let parts: Vec<_> = remote_key
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let name = parts.last().map(|part| (*part).to_owned());
    let owner = if parts.len() >= 3 {
        parts
            .get(parts.len().saturating_sub(2))
            .map(|part| (*part).to_owned())
    } else {
        None
    };
    let host = parts.first().map(|part| (*part).to_owned());
    GitRemoteParts { host, owner, name }
}

fn read_git_dirty_status(repo_root: &Path, scan_started: Instant) -> GitDirtyStatus {
    if scan_started.elapsed() >= SCAN_TIME_BUDGET {
        return GitDirtyStatus {
            status: "timeout".to_owned(),
            file_count: None,
            truncated: false,
        };
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("--untracked-files=normal")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return GitDirtyStatus {
            status: "unavailable".to_owned(),
            file_count: None,
            truncated: false,
        };
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return GitDirtyStatus {
                        status: "unavailable".to_owned(),
                        file_count: None,
                        truncated: false,
                    };
                }
                let stdout = output
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stdout))
                    .unwrap_or_default();
                let count = stdout.lines().take(GIT_STATUS_MAX_LINES + 1).count();
                let truncated = count > GIT_STATUS_MAX_LINES;
                return GitDirtyStatus {
                    status: if count == 0 { "clean" } else { "dirty" }.to_owned(),
                    file_count: Some(count.min(GIT_STATUS_MAX_LINES) as u64),
                    truncated,
                };
            }
            Ok(None)
                if started.elapsed() < GIT_STATUS_TIME_BUDGET
                    && scan_started.elapsed() < SCAN_TIME_BUDGET =>
            {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return GitDirtyStatus {
                    status: "timeout".to_owned(),
                    file_count: None,
                    truncated: false,
                };
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return GitDirtyStatus {
                    status: "unavailable".to_owned(),
                    file_count: None,
                    truncated: false,
                };
            }
        }
    }
}

fn read_packed_ref(git_dir: &Path, reference: &str) -> Option<String> {
    let content = fs::read_to_string(git_dir.join("packed-refs")).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('^') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let hash = parts.next()?;
        let candidate = parts.next()?;
        if candidate == reference {
            return Some(hash.to_owned());
        }
    }
    None
}

fn git_tracked_paths(repo_root: &Path, relative_paths: &[&str]) -> BTreeSet<String> {
    if relative_paths.is_empty() {
        return BTreeSet::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("ls-files")
        .arg("--")
        .args(relative_paths)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return BTreeSet::new();
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return BTreeSet::new();
                }
                return output
                    .as_ref()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .filter(|path| !path.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeSet::new();
            }
        }
    }
}

fn git_tracked_path_set(repo_root: &Path, relative_paths: &[String]) -> BTreeSet<String> {
    let borrowed: Vec<&str> = relative_paths.iter().map(String::as_str).collect();
    git_tracked_paths(repo_root, &borrowed)
}

fn git_status_path_map(repo_root: &Path, relative_paths: &[String]) -> BTreeMap<String, String> {
    if relative_paths.is_empty() {
        return BTreeMap::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("--untracked-files=normal")
        .arg("--")
        .args(relative_paths)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return BTreeMap::new();
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return BTreeMap::new();
                }
                return output
                    .as_ref()
                    .map(|output| {
                        parse_git_status_porcelain(&String::from_utf8_lossy(&output.stdout))
                    })
                    .unwrap_or_default();
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeMap::new();
            }
        }
    }
}

fn git_repository_has_active_changes(repo_root: &Path) -> bool {
    let status = read_git_dirty_status(repo_root, Instant::now());
    status.status == "dirty"
}

fn parse_git_status_porcelain(output: &str) -> BTreeMap<String, String> {
    let mut statuses = BTreeMap::new();
    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..]
            .split(" -> ")
            .last()
            .unwrap_or_default()
            .trim()
            .trim_matches('"');
        if path.is_empty() {
            continue;
        }
        let status = match code {
            "??" => "untracked",
            "!!" => "ignored",
            _ if code.contains('U') => "conflicted",
            _ if code.contains('D') => "deleted",
            _ if code.contains('R') => "renamed",
            _ if code.contains('M') || code.contains('A') || code.contains('C') => "modified",
            _ => "modified",
        };
        statuses.insert(path.to_owned(), status.to_owned());
    }
    statuses
}

fn git_ignored_path_set(repo_root: &Path, relative_paths: &[String]) -> BTreeSet<String> {
    if relative_paths.is_empty() {
        return BTreeSet::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("check-ignore")
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return local_gitignore_path_set(repo_root, relative_paths);
    };

    if let Some(mut stdin) = child.stdin.take() {
        for path in relative_paths {
            let _ = writeln!(stdin, "{path}");
        }
    }

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let output = child.wait_with_output().ok();
                let mut ignored: BTreeSet<String> = output
                    .as_ref()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .filter(|path| !path.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                ignored.extend(local_gitignore_path_set(repo_root, relative_paths));
                return ignored;
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeSet::new();
            }
        }
    }
}

fn local_gitignore_path_set(repo_root: &Path, relative_paths: &[String]) -> BTreeSet<String> {
    let mut ignore_contents = Vec::new();
    if let Ok(content) = fs::read_to_string(repo_root.join(".gitignore")) {
        ignore_contents.push(content);
    }
    if let Some(git_dir) = resolve_git_dir(repo_root)
        && let Ok(content) = fs::read_to_string(git_dir.join("info").join("exclude"))
    {
        ignore_contents.push(content);
    }
    if ignore_contents.is_empty() {
        return BTreeSet::new();
    }
    let patterns: Vec<_> = ignore_contents
        .iter()
        .flat_map(|content| content.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .map(|line| line.trim_matches('/').to_owned())
        .collect();
    let mut ignored = BTreeSet::new();
    for path in relative_paths {
        let normalized = path.trim_matches('/');
        let file_name = Path::new(normalized)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(normalized);
        if patterns.iter().any(|pattern| {
            pattern == normalized
                || pattern == file_name
                || normalized.starts_with(&format!("{pattern}/"))
        }) {
            ignored.insert(path.clone());
        }
    }
    ignored
}

fn resolve_git_dir(repo_root: &Path) -> Option<PathBuf> {
    let git_marker = repo_root.join(".git");
    if git_marker.is_dir() {
        return Some(git_marker);
    }
    let marker = fs::read_to_string(&git_marker).ok()?;
    let git_dir = marker.trim().strip_prefix("gitdir:")?.trim();
    let path = PathBuf::from(git_dir);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(repo_root.join(path))
    }
}

fn is_git_repository_root(path: &Path) -> bool {
    resolve_git_dir(path).is_some()
}

#[derive(Clone, Debug, Default)]
struct RepositoryInventoryScan {
    repositories_by_root: BTreeMap<String, String>,
    coverage: Vec<RepositoryInventoryRootCoverage>,
    truncated: bool,
}

fn scan_repository_inventory_roots(
    requested_roots: &[PathBuf],
    max_depth: usize,
) -> RepositoryInventoryScan {
    scan_repository_inventory_roots_with_budget(
        requested_roots,
        max_depth,
        REPOSITORY_INVENTORY_TIME_BUDGET,
        None,
    )
}

fn scan_repository_inventory_roots_with_budget(
    requested_roots: &[PathBuf],
    max_depth: usize,
    budget: Duration,
    runtime: Option<&StorageScanRuntimeContext>,
) -> RepositoryInventoryScan {
    let started = Instant::now();
    let mut repositories_by_root = BTreeMap::<String, String>::new();
    let mut coverage = Vec::new();
    let mut truncated = false;

    for root in requested_roots {
        let path = root.display().to_string();
        let label = storage_source_label(root, &storage_source_kind(root));
        if let Some(runtime) = runtime
            && !runtime.set_phase(STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY, Some(root))
        {
            truncated = true;
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "partial",
                "partial",
                "Repository inventory stopped before this root because the scan was cancelled.",
                0,
                0,
                0,
                true,
                false,
            ));
            continue;
        }

        if started.elapsed() >= budget {
            truncated = true;
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "partial",
                "partial",
                "Repository inventory stopped before this root because the inventory budget ended.",
                0,
                0,
                0,
                true,
                false,
            ));
            continue;
        }

        if !root.exists() {
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "unavailable",
                "unavailable",
                "Path does not exist on this machine or provider is not configured.",
                0,
                0,
                0,
                false,
                false,
            ));
            continue;
        }

        let metadata = match fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(error) => {
                let detail = error.to_string();
                let permission_state = skipped_root_permission_state(&detail);
                coverage.push(repository_inventory_coverage_entry(
                    root,
                    label,
                    "skipped",
                    &permission_state,
                    &detail,
                    0,
                    0,
                    0,
                    false,
                    false,
                ));
                continue;
            }
        };

        if metadata.file_type().is_symlink() {
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "skipped",
                "skipped",
                "Symlink root skipped during repository inventory.",
                0,
                0,
                0,
                false,
                false,
            ));
            continue;
        }

        if !is_git_repository_root(root)
            && let Some(reason) = repository_inventory_skip_reason(root)
        {
            coverage.push(repository_inventory_coverage_entry(
                root, label, "skipped", "skipped", reason, 0, 0, 1, false, false,
            ));
            continue;
        }

        let before = repositories_by_root.len();
        let root_scan = scan_repository_inventory_root(root, max_depth, started, budget, runtime);
        truncated |= root_scan.truncated;
        for repo_root in root_scan.repositories {
            repositories_by_root
                .entry(repo_root.display().to_string())
                .or_insert_with(|| path.clone());
        }
        let repository_count = repositories_by_root.len().saturating_sub(before) as u64;
        coverage.push(repository_inventory_coverage_entry(
            root,
            label,
            if root_scan.truncated {
                "partial"
            } else {
                "scanned"
            },
            if root_scan.truncated {
                "partial"
            } else {
                "readable"
            },
            if root_scan.truncated {
                "Repository inventory was partially scanned before the inventory budget ended."
            } else {
                "Readable and scanned by the repository inventory pass."
            },
            repository_count,
            root_scan.scanned_directory_count,
            root_scan.skipped_directory_count,
            root_scan.truncated,
            !root_scan.truncated,
        ));
    }

    RepositoryInventoryScan {
        repositories_by_root,
        coverage,
        truncated,
    }
}

#[derive(Clone, Debug, Default)]
struct RepositoryInventoryRootScan {
    repositories: BTreeSet<PathBuf>,
    scanned_directory_count: u64,
    skipped_directory_count: u64,
    truncated: bool,
}

fn scan_repository_inventory_root(
    root: &Path,
    max_depth: usize,
    started: Instant,
    budget: Duration,
    runtime: Option<&StorageScanRuntimeContext>,
) -> RepositoryInventoryRootScan {
    let max_depth = max_depth.clamp(1, 12);
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut repositories = BTreeSet::new();
    let mut scanned_directory_count = 0u64;
    let mut skipped_directory_count = 0u64;
    let mut truncated = false;

    while let Some((path, depth)) = queue.pop_front() {
        if started.elapsed() >= budget
            || scanned_directory_count >= REPOSITORY_INVENTORY_MAX_DIRECTORIES
        {
            truncated = true;
            break;
        }

        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }

        let is_repo = is_git_repository_root(&path);
        if is_repo {
            repositories.insert(path.clone());
        } else if depth > 0 && repository_inventory_skip_reason(&path).is_some() {
            skipped_directory_count = skipped_directory_count.saturating_add(1);
            continue;
        }

        if depth >= max_depth {
            continue;
        }

        scanned_directory_count = scanned_directory_count.saturating_add(1);
        if let Some(runtime) = runtime
            && !runtime.checkpoint(
                STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY,
                Some(&path),
                0,
                1,
                0,
            )
        {
            truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        let mut children = entries
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            if child
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == ".git")
            {
                skipped_directory_count = skipped_directory_count.saturating_add(1);
                continue;
            }
            queue.push_back((child, depth + 1));
        }
    }

    RepositoryInventoryRootScan {
        repositories,
        scanned_directory_count,
        skipped_directory_count,
        truncated,
    }
}

fn repository_inventory_skip_reason(path: &Path) -> Option<&'static str> {
    match path.file_name().and_then(|name| name.to_str())? {
        ".git" => Some("Git internals skipped during repository inventory."),
        "node_modules" => Some("Dependency tree skipped during repository inventory."),
        "target" => Some("Rust build output skipped during repository inventory."),
        ".build" => Some("Build output skipped during repository inventory."),
        "DerivedData" => Some("Xcode DerivedData skipped during repository inventory."),
        ".cache" => Some("Cache directory skipped during repository inventory."),
        "Library" => Some("macOS Library tree skipped during repository inventory."),
        ".docker" => Some("Docker data skipped during repository inventory."),
        ".npm" => Some("Package cache skipped during repository inventory."),
        ".pnpm-store" => Some("Package cache skipped during repository inventory."),
        ".cargo" => Some("Package cache skipped during repository inventory."),
        ".gradle" => Some("Gradle cache skipped during repository inventory."),
        ".venv" | "venv" => Some("Virtual environment skipped during repository inventory."),
        ".tox" => Some("Python test environment skipped during repository inventory."),
        "__pycache__" => Some("Python bytecode cache skipped during repository inventory."),
        ".next" | ".turbo" => Some("Frontend build cache skipped during repository inventory."),
        "Pods" => Some("Dependency tree skipped during repository inventory."),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn repository_inventory_coverage_entry(
    root: &Path,
    label: String,
    status: &str,
    permission_state: &str,
    detail: &str,
    repository_count: u64,
    scanned_directory_count: u64,
    skipped_directory_count: u64,
    truncated: bool,
    scanned: bool,
) -> RepositoryInventoryRootCoverage {
    let path = root.display().to_string();
    RepositoryInventoryRootCoverage {
        id: path.clone(),
        label,
        path,
        status: status.to_owned(),
        permission_state: permission_state.to_owned(),
        detail: detail.to_owned(),
        repository_count,
        scanned_directory_count,
        skipped_directory_count,
        truncated,
        scanned,
    }
}

fn repository_inventory_completeness(
    coverage: &[RepositoryInventoryRootCoverage],
    truncated: bool,
) -> RepositoryInventoryCompleteness {
    let roots = coverage
        .iter()
        .map(|root| root.path.clone())
        .collect::<Vec<_>>();
    let partial_roots = coverage
        .iter()
        .filter(|root| root.truncated || !root.scanned || root.status == "partial")
        .map(|root| root.path.clone())
        .collect::<Vec<_>>();
    let truncated = truncated || coverage.iter().any(|root| root.truncated);
    RepositoryInventoryCompleteness {
        complete: !truncated && partial_roots.is_empty(),
        truncated,
        roots,
        partial_roots,
    }
}

fn cached_repository_inventory_coverage(
    requested_roots: &[PathBuf],
    repositories_by_root: &BTreeMap<String, String>,
) -> Vec<RepositoryInventoryRootCoverage> {
    requested_roots
        .iter()
        .map(|root| {
            let label = storage_source_label(root, &storage_source_kind(root));
            if !root.exists() {
                return repository_inventory_coverage_entry(
                    root,
                    label,
                    "unavailable",
                    "unavailable",
                    "Path does not exist on this machine or provider is not configured.",
                    0,
                    0,
                    0,
                    false,
                    false,
                );
            }

            let metadata = match fs::symlink_metadata(root) {
                Ok(metadata) => metadata,
                Err(error) => {
                    let detail = error.to_string();
                    let permission_state = skipped_root_permission_state(&detail);
                    return repository_inventory_coverage_entry(
                        root,
                        label,
                        "skipped",
                        &permission_state,
                        &detail,
                        0,
                        0,
                        0,
                        false,
                        false,
                    );
                }
            };
            if metadata.file_type().is_symlink() {
                return repository_inventory_coverage_entry(
                    root,
                    label,
                    "skipped",
                    "skipped",
                    "Symlink root skipped during cached repository inventory coverage.",
                    0,
                    0,
                    0,
                    false,
                    false,
                );
            }

            let root_display = root.display().to_string();
            let repository_count = repositories_by_root
                .values()
                .filter(|discovered_root| *discovered_root == &root_display)
                .count() as u64;
            let (status, detail) = if repository_count == 0 {
                (
                    "cached_empty",
                    "No repositories are currently present in the storage index for this root; run a refresh for authoritative inventory.",
                )
            } else {
                (
                    "cached_partial",
                    "Repository inventory was restored from the storage index snapshot; run a refresh for authoritative inventory.",
                )
            };
            repository_inventory_coverage_entry(
                root,
                label,
                status,
                "cached",
                detail,
                repository_count,
                0,
                0,
                false,
                false,
            )
        })
        .collect()
}

fn repository_inventory_cache_roots(
    entries: &BTreeMap<String, RepositoryInventoryCacheEntry>,
) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(repo_root, entry)| (repo_root.clone(), entry.discovered_root.clone()))
        .collect()
}

fn repository_inventory_cache_states(
    repositories_by_root: &BTreeMap<String, String>,
    latest_repositories_by_root: &BTreeMap<String, String>,
    cached_entries: &BTreeMap<String, RepositoryInventoryCacheEntry>,
) -> BTreeMap<String, RepositoryInventoryCacheState> {
    repositories_by_root
        .keys()
        .map(|repo_root| {
            let current_fingerprint = repository_inventory_fingerprint(Path::new(repo_root));
            let state = if latest_repositories_by_root.contains_key(repo_root) {
                RepositoryInventoryCacheState {
                    status: "scanned".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: false,
                    last_seen_millis: cached_entries
                        .get(repo_root)
                        .map(|entry| entry.last_seen_millis),
                    last_scan_millis: cached_entries
                        .get(repo_root)
                        .map(|entry| entry.last_scan_millis),
                }
            } else if !Path::new(repo_root).exists() {
                let cached = cached_entries.get(repo_root);
                RepositoryInventoryCacheState {
                    status: "missing".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: true,
                    last_seen_millis: cached.map(|entry| entry.last_seen_millis),
                    last_scan_millis: cached.map(|entry| entry.last_scan_millis),
                }
            } else if let Some(cached) = cached_entries.get(repo_root) {
                let cached_fingerprint = cached.repository_fingerprint.trim();
                let (status, fingerprint_changed) = if cached_fingerprint.is_empty() {
                    ("legacy", false)
                } else if cached_fingerprint == current_fingerprint {
                    ("current", false)
                } else {
                    ("changed", true)
                };
                RepositoryInventoryCacheState {
                    status: status.to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed,
                    last_seen_millis: Some(cached.last_seen_millis),
                    last_scan_millis: Some(cached.last_scan_millis),
                }
            } else {
                RepositoryInventoryCacheState {
                    status: "uncached".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: false,
                    last_seen_millis: None,
                    last_scan_millis: None,
                }
            };
            (repo_root.clone(), state)
        })
        .collect()
}

fn merge_repository_inventory_cache(
    cached: BTreeMap<String, String>,
    latest: BTreeMap<String, String>,
) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let mut merged = cached;
    let mut not_seen = merged.keys().cloned().collect::<BTreeSet<_>>();
    for (repo_root, discovered_root) in latest {
        not_seen.remove(&repo_root);
        merged.insert(repo_root, discovered_root);
    }
    (merged, not_seen)
}

fn repository_inventory_fingerprint(repo_root: &Path) -> String {
    let root = repo_root.display().to_string();
    let root_fingerprint = repository_path_fingerprint(repo_root, "root");
    let git_marker = repo_root.join(".git");
    let marker_fingerprint = repository_path_fingerprint(&git_marker, "git-marker");
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return format!("v1|path={root}|{root_fingerprint}|{marker_fingerprint}|git-dir:missing");
    };

    let head = read_git_head(repo_root);
    let head_ref_fingerprint = head
        .reference
        .as_deref()
        .filter(|reference| reference.starts_with("refs/"))
        .map(|reference| repository_path_fingerprint(&git_dir.join(reference), "git-ref"))
        .unwrap_or_else(|| "git-ref:detached-or-missing".to_owned());

    [
        "v1".to_owned(),
        format!("path={root}"),
        root_fingerprint,
        marker_fingerprint,
        repository_path_fingerprint(&git_dir, "git-dir"),
        repository_path_fingerprint(&git_dir.join("config"), "config"),
        repository_path_fingerprint(&git_dir.join("index"), "index"),
        repository_path_fingerprint(&git_dir.join("HEAD"), "HEAD"),
        head_ref_fingerprint,
        repository_path_fingerprint(&git_dir.join("packed-refs"), "packed-refs"),
    ]
    .join("|")
}

fn repository_path_fingerprint(path: &Path, label: &str) -> String {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return format!("{label}:missing");
    };
    let modified_millis = unix_metadata_millis(metadata.mtime(), metadata.mtime_nsec());
    let changed_millis = unix_metadata_millis(metadata.ctime(), metadata.ctime_nsec());
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "dir"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    };
    format!(
        "{}:{}:{}:{}:{}:{}:{}",
        label,
        kind,
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        modified_millis,
        changed_millis
    )
}

fn repository_git_file_fingerprint(repo_root: &Path, file_name: &str) -> String {
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return format!("{file_name}:missing-git-dir");
    };
    repository_path_fingerprint(&git_dir.join(file_name), file_name)
}

fn short_hash(value: &str) -> String {
    value.chars().take(12).collect()
}

fn normalize_roots(roots: Vec<String>) -> Vec<PathBuf> {
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

fn normalize_dirty_paths(paths: Vec<String>) -> Vec<String> {
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

fn path_matches_dirty_prefix(path: &Path, dirty_paths: &[String]) -> bool {
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

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
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

fn summarize_volume_states(requested_roots: &[PathBuf]) -> Vec<StorageVolumeState> {
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

fn existing_parent(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(parent) = current {
        if parent.exists() {
            return Some(parent.to_path_buf());
        }
        current = parent.parent();
    }
    None
}

fn source_reclaimable_bytes(path: &str, items: &[StorageHygieneItem]) -> u64 {
    items
        .iter()
        .filter(|item| item.path == path || item.path.starts_with(&format!("{path}/")))
        .map(|item| item.size_bytes)
        .fold(0u64, u64::saturating_add)
}

fn skipped_root_permission_state(reason: &str) -> String {
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

fn storage_source_kind(path: &Path) -> String {
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

fn storage_source_label(path: &Path, kind: &str) -> String {
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

fn is_cloud_storage_path(path: &Path) -> bool {
    let display = path.display().to_string();
    display.contains("/Library/CloudStorage/")
        || display.ends_with("/Library/CloudStorage")
        || display.contains("Mobile Documents")
        || display.contains("iCloud Drive")
}

fn is_network_storage_path(path: &Path) -> bool {
    path.display().to_string().starts_with("/Volumes/")
}

fn file_access_age_days(metadata: &fs::Metadata, now_millis: u64) -> Option<u64> {
    let accessed_millis = metadata.accessed().ok().and_then(system_time_millis)?;
    now_millis
        .checked_sub(accessed_millis)
        .map(|delta| delta / 86_400_000)
}

fn is_source_control_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | ".hg" | ".svn")
    )
}

fn system_time_millis(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn unix_metadata_millis(seconds: i64, nanoseconds: i64) -> i64 {
    seconds
        .saturating_mul(1000)
        .saturating_add(nanoseconds.max(0) / 1_000_000)
}

fn metadata_time_millis(seconds: i64, nanoseconds: i64) -> Option<u64> {
    let millis = unix_metadata_millis(seconds, nanoseconds);
    (millis >= 0).then_some(millis as u64)
}

fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * KIB;
    const GIB: f64 = 1024.0 * MIB;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KB", value / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn rebuild_seconds_label(seconds: u64) -> String {
    if seconds >= 3_600 {
        let hours = seconds / 3_600;
        let minutes = (seconds % 3_600) / 60;
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes}m")
        }
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn rule(
    kind: &'static str,
    safety: &'static str,
    cleanup_tier: &'static str,
    reason: &'static str,
    recommendation: &'static str,
) -> ArtifactRule {
    ArtifactRule {
        kind,
        safety,
        cleanup_tier,
        reason,
        recommendation,
    }
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

#[cfg(test)]
mod tests;
