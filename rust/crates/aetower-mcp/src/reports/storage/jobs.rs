use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StorageScanMode {
    InstantCached,
    FastChangedOnly,
    DeepNative,
    ForensicVerified,
}

impl StorageScanMode {
    pub(super) fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "instant" | "instant_cached" | "instant-cached" => Self::InstantCached,
            "deep" | "deep_native" | "deep-native" => Self::DeepNative,
            "forensic" | "forensic_verified" | "forensic-verified" => Self::ForensicVerified,
            _ => Self::FastChangedOnly,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::InstantCached => "instant_cached",
            Self::FastChangedOnly => "fast_changed_only",
            Self::DeepNative => "deep_native",
            Self::ForensicVerified => "forensic_verified",
        }
    }

    pub(super) fn size_walk_entry_budget(self) -> u64 {
        match self {
            Self::InstantCached => 8_000,
            Self::FastChangedOnly => FAST_SIZE_WALK_MAX_ENTRIES,
            Self::DeepNative => DEEP_SIZE_WALK_MAX_ENTRIES,
            Self::ForensicVerified => FORENSIC_SIZE_WALK_MAX_ENTRIES,
        }
    }

    /// Retained rows in the report envelope. Fast scans are deliberately
    /// top-K; Complete/Forensic scans keep the full normal operator table and
    /// rely on the dedicated page API only for extreme (>10k row) machines.
    pub(super) fn report_item_limit(self, requested_limit: usize) -> usize {
        match self {
            Self::InstantCached | Self::FastChangedOnly => requested_limit.clamp(1, MAX_LIMIT),
            Self::DeepNative | Self::ForensicVerified => MAX_ITEMS_PAGE_LIMIT,
        }
    }

    /// Wall-clock budget for the artifact size walk. The ambient fast pass
    /// must stay snappy; Deep/Forensic run as cancellable background jobs
    /// with checkpoints, so minutes-scale walks are safe there. This budget
    /// covers ONLY the walk phase — the git/inventory phase has its own
    /// (SCAN_TIME_BUDGET) so it can never starve the walk to zero again.
    pub(super) fn size_walk_time_budget(self) -> Duration {
        match self {
            Self::InstantCached | Self::FastChangedOnly => SCAN_TIME_BUDGET,
            Self::DeepNative => Duration::from_secs(60),
            Self::ForensicVerified => Duration::from_secs(120),
        }
    }

    /// Repository discovery is useful context for repo attribution, but it is
    /// not the storage walk itself. Quick scans keep this bounded tightly;
    /// Complete/Forensic scans can spend longer so repo coverage does not mark
    /// a successful whole-computer storage scan as globally partial.
    pub(super) fn repository_inventory_time_budget(self) -> Duration {
        match self {
            Self::InstantCached | Self::FastChangedOnly => REPOSITORY_INVENTORY_TIME_BUDGET,
            Self::DeepNative => Duration::from_secs(120),
            Self::ForensicVerified => Duration::from_secs(300),
        }
    }

    pub(super) fn scan_latency_warn_millis(self) -> u64 {
        match self {
            Self::InstantCached | Self::FastChangedOnly => STORAGE_SCAN_LATENCY_WARN_MILLIS,
            Self::DeepNative => 120_000,
            Self::ForensicVerified => 300_000,
        }
    }

    pub(super) fn scan_latency_critical_millis(self) -> u64 {
        match self {
            Self::InstantCached | Self::FastChangedOnly => STORAGE_SCAN_LATENCY_CRITICAL_MILLIS,
            Self::DeepNative => 300_000,
            Self::ForensicVerified => 600_000,
        }
    }

    pub(super) fn payload_warn_bytes(self) -> u64 {
        match self {
            Self::InstantCached | Self::FastChangedOnly => STORAGE_PAYLOAD_WARN_BYTES,
            Self::DeepNative | Self::ForensicVerified => 8 * 1024 * 1024,
        }
    }

    pub(super) fn payload_critical_bytes(self) -> u64 {
        match self {
            Self::InstantCached | Self::FastChangedOnly => STORAGE_PAYLOAD_CRITICAL_BYTES,
            Self::DeepNative | Self::ForensicVerified => 16 * 1024 * 1024,
        }
    }

    /// Minimum walk slice granted to each root regardless of how much of the
    /// overall walk budget earlier roots consumed. Keeps one huge early root
    /// (e.g. ~/Repositories) from starving every later root to zero; the
    /// outer loop's overall-budget check still bounds total walk time.
    pub(super) fn per_root_slice_floor(self) -> Duration {
        match self {
            Self::InstantCached | Self::FastChangedOnly => Duration::from_millis(500),
            Self::DeepNative | Self::ForensicVerified => Duration::from_secs(2),
        }
    }

    /// Directory-visit cap for a single `scan_root` call. Instant/Fast keep
    /// the historical 25k cap for snappiness; Deep/Forensic are cancellable
    /// background jobs that can afford whole-computer coverage.
    pub(super) fn dir_budget(self) -> u64 {
        match self {
            Self::InstantCached | Self::FastChangedOnly => MAX_DIRECTORIES,
            Self::DeepNative => 100_000,
            Self::ForensicVerified => 200_000,
        }
    }

    pub(super) fn verify_source_control(self) -> bool {
        matches!(self, Self::ForensicVerified)
    }

    pub(super) fn collect_git_status(self) -> bool {
        matches!(self, Self::ForensicVerified)
    }

    /// Whether the size walk may ANSWER from cached directory sizes in the
    /// persistent `StorageSizeIndex`. Deep/Forensic must walk fresh, so they
    /// never read cached sizes — but every mode still PERSISTS its findings
    /// into the index so `instant_cached` readers (overview, launch repaint)
    /// always see the freshest scan.
    pub(super) fn serve_sizes_from_index(self) -> bool {
        matches!(self, Self::InstantCached | Self::FastChangedOnly)
    }

    pub(super) fn native_metadata_strategy(self) -> &'static str {
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
    pub(super) phase: String,
    pub(super) scanned_files: u64,
    pub(super) scanned_directories: u64,
    pub(super) scanned_bytes: u64,
    current_path_hint: Option<String>,
    eta_millis: Option<u64>,
    updated_at_millis: u64,
    throttle_reason: Option<String>,
}

impl StorageScanJobProgress {
    pub(super) fn new(now_millis: u64, throttle_reason: Option<String>) -> Self {
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
pub(super) struct StorageScanJobRequest {
    roots: Vec<String>,
    pub(super) normalized_roots: Vec<String>,
    pub(super) dirty_paths: Vec<String>,
    pub(super) max_depth: usize,
    pub(super) limit: usize,
    pub(super) mode: StorageScanMode,
    pub(super) throttle_hint: String,
    pub(super) signature: String,
    pub(super) volume_key: String,
}

impl StorageScanJobRequest {
    pub(super) fn new(
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

#[derive(Debug)]
pub(super) struct StorageScanControl {
    cancelled: AtomicBool,
    paused: AtomicBool,
}

impl StorageScanControl {
    pub(super) fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            paused: AtomicBool::new(false),
        }
    }

    pub(super) fn cancel(&self) {
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
pub(super) struct StorageScanThrottle {
    pub(super) sleep_every_checkpoints: u64,
    pub(super) sleep_millis: u64,
    pub(super) reason: Option<String>,
}

impl StorageScanThrottle {
    pub(super) fn for_request(request: &StorageScanJobRequest) -> Self {
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
pub(super) struct StorageScanRuntimeContext {
    control: Arc<StorageScanControl>,
    progress: Arc<Mutex<StorageScanJobProgress>>,
    throttle: StorageScanThrottle,
    checkpoint_count: Arc<AtomicU64>,
}

impl StorageScanRuntimeContext {
    pub(super) fn new(
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

    pub(super) fn checkpoint(
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

    pub(super) fn set_phase(&self, phase: &str, path_hint: Option<&Path>) -> bool {
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
        // Cancellation is sticky: the worker thread's queued/running
        // transitions must not resurrect a job the caller already cancelled.
        if state.status == StorageScanJobStatusKind::Cancelled && status.is_active() {
            return;
        }
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
        if state.status == StorageScanJobStatusKind::Cancelled && status.is_active() {
            return;
        }
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
