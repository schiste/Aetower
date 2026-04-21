use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticsLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticsSubsystem {
    #[default]
    Engine,
    Collector,
    Identity,
    Attribution,
    Friction,
    History,
    Persistence,
    Telemetry,
    Gpu,
    Ffi,
    Ui,
    AdapterChromium,
    AdapterDocker,
    AdapterHelper,
    AdapterChau7,
    AdapterVsCode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticsField {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticsEvent {
    pub id: String,
    pub timestamp_millis: u64,
    pub level: DiagnosticsLevel,
    pub subsystem: DiagnosticsSubsystem,
    pub event_type: String,
    pub sequence: Option<u64>,
    pub entity_id: Option<String>,
    pub adapter: Option<String>,
    pub capability: Option<String>,
    pub message: String,
    pub fields: Vec<DiagnosticsField>,
    pub sensitive: bool,
}

impl DiagnosticsEvent {
    pub fn builder(
        level: DiagnosticsLevel,
        subsystem: DiagnosticsSubsystem,
        event_type: impl Into<String>,
        message: impl Into<String>,
    ) -> DiagnosticsEventBuilder {
        DiagnosticsEventBuilder {
            event: DiagnosticsEvent {
                id: String::new(),
                timestamp_millis: 0,
                level,
                subsystem,
                event_type: event_type.into(),
                sequence: None,
                entity_id: None,
                adapter: None,
                capability: None,
                message: message.into(),
                fields: Vec::new(),
                sensitive: false,
            },
        }
    }
}

pub struct DiagnosticsEventBuilder {
    event: DiagnosticsEvent,
}

impl DiagnosticsEventBuilder {
    pub fn timestamp_millis(mut self, timestamp_millis: u64) -> Self {
        self.event.timestamp_millis = timestamp_millis;
        self
    }

    pub fn sequence(mut self, sequence: u64) -> Self {
        self.event.sequence = Some(sequence);
        self
    }

    pub fn entity_id(mut self, entity_id: impl Into<String>) -> Self {
        self.event.entity_id = Some(entity_id.into());
        self
    }

    pub fn adapter(mut self, adapter: impl Into<String>) -> Self {
        self.event.adapter = Some(adapter.into());
        self
    }

    pub fn capability(mut self, capability: impl Into<String>) -> Self {
        self.event.capability = Some(capability.into());
        self
    }

    pub fn field(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.event.fields.push(DiagnosticsField {
            key: key.into(),
            value: value.to_string(),
        });
        self
    }

    pub fn sensitive(mut self, sensitive: bool) -> Self {
        self.event.sensitive = sensitive;
        self
    }

    pub fn build(self) -> DiagnosticsEvent {
        self.event
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticsOverview {
    pub ring_capacity: u32,
    pub current_size: u32,
    pub dropped_events: u64,
    pub error_count: u32,
    pub warn_count: u32,
    pub last_event_millis: Option<u64>,
    pub last_error_millis: Option<u64>,
    pub last_error_message: Option<String>,
    pub persisted_events: u64,
    pub persisted_path: Option<String>,
    pub persisted_bytes: u64,
    pub persistence_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticsQuery {
    #[serde(default)]
    pub limit: usize,
    #[serde(default)]
    pub minimum_level: Option<DiagnosticsLevel>,
    #[serde(default)]
    pub subsystem: Option<DiagnosticsSubsystem>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub since_millis: Option<u64>,
    #[serde(default)]
    pub include_persisted: bool,
}

#[derive(Clone, Debug)]
pub struct DiagnosticsStore {
    inner: Arc<Mutex<DiagnosticsState>>,
}

#[derive(Debug)]
struct DiagnosticsState {
    events: VecDeque<DiagnosticsEvent>,
    capacity: usize,
    next_id: u64,
    dropped_events: u64,
    coalesced_events: BTreeMap<String, DiagnosticsCoalesceState>,
    persistence: Option<PersistedDiagnostics>,
}

#[derive(Debug, Clone, Copy)]
struct DiagnosticsCoalesceState {
    last_emitted_millis: u64,
    window_millis: u64,
}

#[derive(Debug)]
struct PersistedDiagnostics {
    path: PathBuf,
    compact_at: usize,
    state: Arc<Mutex<PersistedDiagnosticsState>>,
    command_tx: mpsc::Sender<PersistCommand>,
}

#[derive(Debug, Default)]
struct PersistedDiagnosticsState {
    event_count: usize,
    pending_writes: usize,
    last_error: Option<String>,
}

#[derive(Debug)]
enum PersistCommand {
    Append(String),
    Flush(mpsc::Sender<io::Result<()>>),
    Clear(mpsc::Sender<io::Result<()>>),
}

impl DiagnosticsStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DiagnosticsState {
                events: VecDeque::with_capacity(capacity),
                capacity: capacity.max(1),
                next_id: 1,
                dropped_events: 0,
                coalesced_events: BTreeMap::new(),
                persistence: None,
            })),
        }
    }

    pub fn with_persistence(
        capacity: usize,
        path: impl AsRef<Path>,
        max_persisted_events: usize,
    ) -> std::io::Result<Self> {
        let capacity = capacity.max(1);
        let max_persisted_events = max_persisted_events.max(capacity);
        let (events, persistence) =
            PersistedDiagnostics::open(path.as_ref(), max_persisted_events, capacity)?;
        let next_id = events
            .iter()
            .filter_map(|event| event.id.strip_prefix("diag-")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        Ok(Self {
            inner: Arc::new(Mutex::new(DiagnosticsState {
                coalesced_events: seed_coalescing_state(&events),
                events,
                capacity,
                next_id,
                dropped_events: 0,
                persistence: Some(persistence),
            })),
        })
    }

    pub fn emit(&self, mut event: DiagnosticsEvent) {
        let mut guard = self.inner.lock();
        if event.timestamp_millis == 0 {
            event.timestamp_millis = now_millis();
        }
        if should_coalesce_event(&mut guard, &event) {
            return;
        }
        event.id = format!("diag-{}", guard.next_id);
        guard.next_id = guard.next_id.saturating_add(1);

        if let Some(persistence) = guard.persistence.as_mut() {
            persistence.append(&event);
        }

        if guard.events.len() >= guard.capacity {
            guard.events.pop_front();
            guard.dropped_events = guard.dropped_events.saturating_add(1);
        }
        guard.events.push_back(event);
    }

    pub fn recent(&self, limit: usize) -> Vec<DiagnosticsEvent> {
        let guard = self.inner.lock();
        guard
            .events
            .iter()
            .rev()
            .take(limit.max(1))
            .cloned()
            .collect()
    }

    pub fn overview(&self) -> DiagnosticsOverview {
        let guard = self.inner.lock();
        let last_mcp_server_started_millis = guard
            .events
            .iter()
            .rev()
            .find(|event| event.event_type == "mcp-server-started")
            .map(|event| event.timestamp_millis);
        let mut error_count = 0u32;
        let mut warn_count = 0u32;
        let mut last_error_message = None;
        let mut last_error_millis = None;

        for event in guard.events.iter().rev() {
            match event.level {
                DiagnosticsLevel::Error => {
                    if event.event_type == "mcp-server-start-failed"
                        && last_mcp_server_started_millis
                            .map(|started_at| started_at >= event.timestamp_millis)
                            .unwrap_or(false)
                    {
                        continue;
                    }
                    error_count = error_count.saturating_add(1);
                    if last_error_message.is_none() {
                        last_error_message = Some(event.message.clone());
                        last_error_millis = Some(event.timestamp_millis);
                    }
                }
                DiagnosticsLevel::Warn => {
                    warn_count = warn_count.saturating_add(1);
                }
                _ => {}
            }
        }

        let persistence_stats = guard.persistence.as_ref().map(|persistence| {
            let state = persistence.state.lock();
            (
                state.event_count.min(u64::MAX as usize) as u64,
                state.last_error.clone(),
            )
        });

        DiagnosticsOverview {
            ring_capacity: guard.capacity.min(u32::MAX as usize) as u32,
            current_size: guard.events.len().min(u32::MAX as usize) as u32,
            dropped_events: guard.dropped_events,
            error_count,
            warn_count,
            last_event_millis: guard.events.back().map(|event| event.timestamp_millis),
            last_error_millis,
            last_error_message,
            persisted_events: guard
                .persistence
                .as_ref()
                .and_then(|_| persistence_stats.as_ref().map(|stats| stats.0))
                .unwrap_or(0),
            persisted_path: guard
                .persistence
                .as_ref()
                .map(|persistence| persistence.path.display().to_string()),
            persisted_bytes: guard
                .persistence
                .as_ref()
                .map(|persistence| persistence.file_bytes())
                .unwrap_or(0),
            persistence_error: guard
                .persistence
                .as_ref()
                .and_then(|_| persistence_stats.as_ref().and_then(|stats| stats.1.clone())),
        }
    }

    pub fn pending_writes(&self) -> u64 {
        let guard = self.inner.lock();
        guard
            .persistence
            .as_ref()
            .map(|persistence| {
                persistence
                    .state
                    .lock()
                    .pending_writes
                    .min(u64::MAX as usize) as u64
            })
            .unwrap_or(0)
    }

    pub fn export_json(&self, limit: usize) -> String {
        serde_json::to_string_pretty(&self.recent(limit))
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
    }

    pub fn query(&self, query: &DiagnosticsQuery) -> Vec<DiagnosticsEvent> {
        let guard = self.inner.lock();
        let limit = query.limit.max(1);
        let mut events = if query.include_persisted {
            let mut persisted = guard
                .persistence
                .as_ref()
                .map(|persistence| persistence.read_recent(limit.saturating_mul(4)))
                .unwrap_or_default();
            persisted.extend(guard.events.iter().cloned());
            persisted
        } else {
            guard.events.iter().cloned().collect::<Vec<_>>()
        };
        drop(guard);

        if query.include_persisted {
            events.sort_by(|left, right| {
                left.timestamp_millis
                    .cmp(&right.timestamp_millis)
                    .then(left.id.cmp(&right.id))
            });
            events.dedup_by(|left, right| left.id == right.id);
        }

        let normalized_search = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());

        let mut filtered = events
            .into_iter()
            .filter(|event| diagnostics_event_matches(event, query, normalized_search.as_deref()))
            .collect::<Vec<_>>();
        filtered.sort_by(|left, right| right.timestamp_millis.cmp(&left.timestamp_millis));
        filtered.truncate(limit);
        filtered
    }

    pub fn query_json(&self, query: &DiagnosticsQuery) -> String {
        serde_json::to_string_pretty(&self.query(query))
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
    }

    pub fn clear(&self) -> io::Result<()> {
        let mut guard = self.inner.lock();
        guard.events.clear();
        guard.coalesced_events.clear();
        guard.next_id = 1;
        guard.dropped_events = 0;
        if let Some(persistence) = guard.persistence.as_mut() {
            persistence.clear()?;
        }
        Ok(())
    }

    pub fn flush_persistence(&self) -> io::Result<()> {
        let guard = self.inner.lock();
        if let Some(persistence) = guard.persistence.as_ref() {
            persistence.flush()
        } else {
            Ok(())
        }
    }
}

pub fn read_recent_persisted(
    path: impl AsRef<Path>,
    limit: usize,
) -> io::Result<Vec<DiagnosticsEvent>> {
    let file = File::open(path)?;
    let mut retained = VecDeque::with_capacity(limit.max(1));
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<DiagnosticsEvent>(&line) else {
            continue;
        };
        if retained.len() >= limit.max(1) {
            retained.pop_front();
        }
        retained.push_back(event);
    }
    Ok(retained.into_iter().collect())
}

pub fn query_persisted(
    path: impl AsRef<Path>,
    query: &DiagnosticsQuery,
) -> io::Result<Vec<DiagnosticsEvent>> {
    let events = read_recent_persisted(path, query.limit.max(1).saturating_mul(4))?;
    let normalized_search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    let mut filtered = events
        .into_iter()
        .filter(|event| diagnostics_event_matches(event, query, normalized_search.as_deref()))
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| right.timestamp_millis.cmp(&left.timestamp_millis));
    filtered.truncate(query.limit.max(1));
    Ok(filtered)
}

impl Default for DiagnosticsStore {
    fn default() -> Self {
        Self::new(2_000)
    }
}

impl PersistedDiagnostics {
    fn open(
        path: &Path,
        max_events: usize,
        preload_limit: usize,
    ) -> std::io::Result<(VecDeque<DiagnosticsEvent>, Self)> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            File::create(path)?;
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut events = VecDeque::with_capacity(preload_limit.max(1));
        let mut retained = VecDeque::new();
        let mut needs_rewrite = false;

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            let Ok(event) = serde_json::from_str::<DiagnosticsEvent>(&line) else {
                needs_rewrite = true;
                continue;
            };
            if !should_persist_event(&event) {
                needs_rewrite = true;
                continue;
            }
            retained.push_back(event.clone());
            if events.len() >= preload_limit.max(1) {
                events.pop_front();
            }
            events.push_back(event);
        }

        let state = Arc::new(Mutex::new(PersistedDiagnosticsState {
            event_count: retained.len(),
            pending_writes: 0,
            last_error: None,
        }));

        let mut persistence = Self {
            path: path.to_path_buf(),
            compact_at: max_events.saturating_add((max_events / 4).max(1)),
            state: Arc::clone(&state),
            command_tx: spawn_persistence_worker(
                path.to_path_buf(),
                max_events,
                Arc::clone(&state),
            ),
        };
        if needs_rewrite {
            persistence.rewrite_from_events(&retained)?;
        }
        if retained.len() > persistence.compact_at {
            let compacted = Self::compact_to_last(path, max_events)?;
            persistence.state.lock().event_count = compacted;
        }
        Ok((events, persistence))
    }

    fn append(&mut self, event: &DiagnosticsEvent) {
        if !should_persist_event(event) {
            return;
        }
        match self.try_append(event) {
            Ok(()) => {
                let mut state = self.state.lock();
                state.pending_writes = state.pending_writes.saturating_add(1);
                state.last_error = None;
            }
            Err(error) => {
                self.state.lock().last_error = Some(error.to_string());
            }
        }
    }

    fn try_append(&mut self, event: &DiagnosticsEvent) -> std::io::Result<()> {
        let mut line = serde_json::to_string(event).map_err(io::Error::other)?;
        line.push('\n');
        self.command_tx
            .send(PersistCommand::Append(line))
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))
    }

    fn compact_to_last(path: &Path, keep: usize) -> std::io::Result<usize> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut retained = VecDeque::with_capacity(keep.max(1));

        for line in reader.lines() {
            let line = line?;
            if retained.len() >= keep.max(1) {
                retained.pop_front();
            }
            retained.push_back(line);
        }

        let mut file = File::create(path)?;
        for line in &retained {
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
        }
        Ok(retained.len())
    }

    fn rewrite_from_events(&mut self, events: &VecDeque<DiagnosticsEvent>) -> std::io::Result<()> {
        let mut file = File::create(&self.path)?;
        for event in events {
            serde_json::to_writer(&mut file, event).map_err(io::Error::other)?;
            file.write_all(b"\n")?;
        }
        let mut state = self.state.lock();
        state.event_count = events.len();
        state.pending_writes = 0;
        state.last_error = None;
        Ok(())
    }

    fn file_bytes(&self) -> u64 {
        std::fs::metadata(&self.path)
            .map(|metadata| metadata.len())
            .unwrap_or(0)
    }

    fn clear(&mut self) -> io::Result<()> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(PersistCommand::Clear(tx))
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))?;
        rx.recv()
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))?
    }

    fn read_recent(&self, limit: usize) -> Vec<DiagnosticsEvent> {
        self.flush().ok();
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(_) => return Vec::new(),
        };
        let mut retained = VecDeque::with_capacity(limit.max(1));
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let Ok(event) = serde_json::from_str::<DiagnosticsEvent>(&line) else {
                continue;
            };
            if retained.len() >= limit.max(1) {
                retained.pop_front();
            }
            retained.push_back(event);
        }
        retained.into_iter().collect()
    }

    fn flush(&self) -> io::Result<()> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(PersistCommand::Flush(tx))
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))?;
        rx.recv()
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))?
    }
}

fn spawn_persistence_worker(
    path: PathBuf,
    max_events: usize,
    state: Arc<Mutex<PersistedDiagnosticsState>>,
) -> mpsc::Sender<PersistCommand> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let compact_at = max_events.saturating_add((max_events / 4).max(1));
        while let Ok(command) = rx.recv() {
            match command {
                PersistCommand::Append(line) => {
                    let result = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                        .and_then(|mut file| file.write_all(line.as_bytes()));
                    let mut guard = state.lock();
                    guard.pending_writes = guard.pending_writes.saturating_sub(1);
                    match result {
                        Ok(()) => {
                            guard.event_count = guard.event_count.saturating_add(1);
                            guard.last_error = None;
                            let should_compact = guard.event_count > compact_at;
                            drop(guard);
                            if should_compact {
                                match PersistedDiagnostics::compact_to_last(&path, max_events) {
                                    Ok(compacted) => {
                                        let mut guard = state.lock();
                                        guard.event_count = compacted;
                                        guard.last_error = None;
                                    }
                                    Err(error) => {
                                        state.lock().last_error = Some(error.to_string());
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            guard.last_error = Some(error.to_string());
                        }
                    }
                }
                PersistCommand::Flush(reply) => {
                    let _ = reply.send(Ok(()));
                }
                PersistCommand::Clear(reply) => {
                    let result = File::create(&path).map(|_| ());
                    let mut guard = state.lock();
                    if result.is_ok() {
                        guard.event_count = 0;
                        guard.pending_writes = 0;
                        guard.last_error = None;
                    } else if let Err(error) = &result {
                        guard.last_error = Some(error.to_string());
                    }
                    let _ = reply.send(result);
                }
            }
        }
    });
    tx
}

fn diagnostics_event_matches(
    event: &DiagnosticsEvent,
    query: &DiagnosticsQuery,
    normalized_search: Option<&str>,
) -> bool {
    if let Some(minimum_level) = query.minimum_level.as_ref()
        && &event.level < minimum_level
    {
        return false;
    }
    if let Some(subsystem) = query.subsystem.as_ref()
        && &event.subsystem != subsystem
    {
        return false;
    }
    if let Some(since_millis) = query.since_millis
        && event.timestamp_millis < since_millis
    {
        return false;
    }
    if let Some(search) = normalized_search {
        let haystack = [
            Some(event.event_type.as_str()),
            Some(event.message.as_str()),
            event.entity_id.as_deref(),
            event.adapter.as_deref(),
            event.capability.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(
            event
                .fields
                .iter()
                .flat_map(|field| [field.key.as_str(), field.value.as_str()]),
        )
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
        if !haystack.contains(search) {
            return false;
        }
    }
    true
}

fn seed_coalescing_state(
    events: &VecDeque<DiagnosticsEvent>,
) -> BTreeMap<String, DiagnosticsCoalesceState> {
    let mut state = BTreeMap::new();
    for event in events {
        let Some((key, window_millis)) = diagnostics_coalescing_key(event) else {
            continue;
        };
        state
            .entry(key)
            .and_modify(|existing: &mut DiagnosticsCoalesceState| {
                existing.last_emitted_millis =
                    existing.last_emitted_millis.max(event.timestamp_millis);
            })
            .or_insert(DiagnosticsCoalesceState {
                last_emitted_millis: event.timestamp_millis,
                window_millis,
            });
    }
    state
}

fn should_coalesce_event(state: &mut DiagnosticsState, event: &DiagnosticsEvent) -> bool {
    let Some((key, window_millis)) = diagnostics_coalescing_key(event) else {
        return false;
    };
    state.coalesced_events.retain(|_, previous| {
        event
            .timestamp_millis
            .saturating_sub(previous.last_emitted_millis)
            < previous.window_millis.saturating_mul(4)
    });

    if let Some(previous) = state.coalesced_events.get_mut(&key) {
        let elapsed = event
            .timestamp_millis
            .saturating_sub(previous.last_emitted_millis);
        if elapsed < window_millis {
            return true;
        }
        previous.last_emitted_millis = event.timestamp_millis;
        previous.window_millis = window_millis;
        return false;
    }

    state.coalesced_events.insert(
        key,
        DiagnosticsCoalesceState {
            last_emitted_millis: event.timestamp_millis,
            window_millis,
        },
    );
    false
}

fn diagnostics_coalescing_key(event: &DiagnosticsEvent) -> Option<(String, u64)> {
    let window_millis = match event.event_type.as_str() {
        "host-incident-snapshot" => 60 * 60 * 1000,
        "system-previous-shutdown-cause" => 12 * 60 * 60 * 1000,
        "adapter-refresh-failed" => 15 * 60 * 1000,
        "history-store-busy" | "history-write-backpressure" => 5 * 60 * 1000,
        "history-maintenance-over-budget" | "history-maintenance-failed" => 15 * 60 * 1000,
        "tick-over-budget" => 5 * 60 * 1000,
        "mcp-helper-lifecycle" => 60 * 1000,
        "mcp-helper-reaped" => 5 * 60 * 1000,
        "tick-started" | "tick-completed" | "snapshot-published" | "gpu-sample-read" => 60 * 1000,
        _ => return None,
    };

    let identity_fields = [
        "incident_key",
        "marker_key",
        "error",
        "path",
        "capability",
        "adapter",
    ];
    let mut parts = vec![
        format!("{:?}", event.level),
        format!("{:?}", event.subsystem),
        event.event_type.clone(),
    ];
    if let Some(adapter) = event.adapter.as_deref() {
        parts.push(format!("adapter={adapter}"));
    }
    if let Some(capability) = event.capability.as_deref() {
        parts.push(format!("capability={capability}"));
    }
    if let Some(entity_id) = event.entity_id.as_deref() {
        parts.push(format!("entity={entity_id}"));
    }
    for field in &event.fields {
        if identity_fields.contains(&field.key.as_str()) {
            parts.push(format!("{}={}", field.key, field.value));
        }
    }

    Some((parts.join("|"), window_millis))
}

fn should_persist_event(event: &DiagnosticsEvent) -> bool {
    match event.level {
        DiagnosticsLevel::Warn | DiagnosticsLevel::Error => true,
        DiagnosticsLevel::Info => matches!(
            event.event_type.as_str(),
            "engine-initialized"
                | "capability-state-changed"
                | "telemetry-config-updated"
                | "telemetry-verification-succeeded"
                | "telemetry-verification-failed"
                | "runtime-heartbeat"
                | "history-pruned"
                | "notification-settings-disabled"
                | "notification-permission-ready"
                | "notification-permission-denied"
                | "notification-permission-requested"
                | "notification-permission-request-failed"
                | "notification-permission-unknown"
                | "session-log-notification-churn"
                | "session-log-notification-permission-failure"
                | "session-log-tcc-churn"
                | "session-log-cursor-noise"
                | "session-log-view-bridge-cancelled"
                | "session-log-invalid-display-identifier"
                | "session-log-window-noise"
                | "session-log-metal-error"
                | "session-log-analysis-failed"
                | "mcp-server-started"
                | "mcp-helper-lifecycle"
                | "boot-session-observed"
                | "system-previous-shutdown-cause"
                | "system-sleep-marker"
                | "system-wake-marker"
                | "system-power-marker"
        ),
        DiagnosticsLevel::Trace | DiagnosticsLevel::Debug => false,
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_drops_oldest_events() {
        let store = DiagnosticsStore::new(2);
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "tick",
                "first",
            )
            .build(),
        );
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "tick",
                "second",
            )
            .build(),
        );
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "tick",
                "third",
            )
            .build(),
        );

        let events = store.recent(10);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, "third");
        assert_eq!(events[1].message, "second");
        assert_eq!(store.overview().dropped_events, 1);
    }

    #[test]
    fn persistence_reloads_recent_events() {
        let path =
            std::env::temp_dir().join(format!("aetower-diagnostics-{}.ndjson", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let store = DiagnosticsStore::with_persistence(3, &path, 6).expect("store");
        for idx in 0..5 {
            store.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Warn,
                    DiagnosticsSubsystem::Engine,
                    "session-log-notification-churn",
                    format!("event-{idx}"),
                )
                .build(),
            );
        }
        store.flush_persistence().expect("flush");

        let reloaded = DiagnosticsStore::with_persistence(3, &path, 6).expect("reloaded");
        let events = reloaded.recent(10);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].message, "event-4");
        assert_eq!(events[2].message, "event-2");
        assert_eq!(reloaded.overview().persisted_events, 5);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn debug_events_do_not_persist() {
        let path =
            std::env::temp_dir().join(format!("aetower-diag-policy-{}.ndjson", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let store = DiagnosticsStore::with_persistence(8, &path, 16).expect("store");
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Debug,
                DiagnosticsSubsystem::Engine,
                "tick-completed",
                "debug tick",
            )
            .build(),
        );
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::Persistence,
                "history-load-failed",
                "warn event",
            )
            .build(),
        );
        store.flush_persistence().expect("flush");

        let persisted = std::fs::read_to_string(&path).expect("persisted file");
        assert!(!persisted.contains("tick-completed"));
        assert!(persisted.contains("history-load-failed"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn repeated_diagnostics_noise_is_coalesced() {
        let store = DiagnosticsStore::new(16);

        for timestamp_millis in [1_000, 2_000, 3_000] {
            store.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Warn,
                    DiagnosticsSubsystem::AdapterChau7,
                    "adapter-refresh-failed",
                    "Chau7 adapter refresh failed.",
                )
                .timestamp_millis(timestamp_millis)
                .field("error", "timeout")
                .build(),
            );
        }
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::AdapterChau7,
                "adapter-refresh-failed",
                "Chau7 adapter refresh failed.",
            )
            .timestamp_millis(16 * 60 * 1000)
            .field("error", "timeout")
            .build(),
        );

        let events = store.recent(10);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].timestamp_millis, 16 * 60 * 1000);
        assert_eq!(events[1].timestamp_millis, 1_000);
    }

    #[test]
    fn coalescing_keeps_distinct_incident_severities() {
        let store = DiagnosticsStore::new(16);
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::History,
                "host-incident-snapshot",
                "Host memory pressure warning.",
            )
            .timestamp_millis(1_000)
            .field("incident_key", "memory-pressure")
            .build(),
        );
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Error,
                DiagnosticsSubsystem::History,
                "host-incident-snapshot",
                "Critical host memory pressure.",
            )
            .timestamp_millis(2_000)
            .field("incident_key", "memory-pressure")
            .build(),
        );

        let events = store.recent(10);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].level, DiagnosticsLevel::Error);
        assert_eq!(events[1].level, DiagnosticsLevel::Warn);
    }

    #[test]
    fn previous_shutdown_markers_coalesce_by_code() {
        let store = DiagnosticsStore::new(16);
        for timestamp_millis in [1_000, 2_000] {
            store.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Engine,
                    "system-previous-shutdown-cause",
                    "Observed a previous shutdown cause marker in the recent system log.",
                )
                .timestamp_millis(timestamp_millis)
                .field("marker_key", "previous-shutdown-cause:-128")
                .build(),
            );
        }
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "system-previous-shutdown-cause",
                "Observed a previous shutdown cause marker in the recent system log.",
            )
            .timestamp_millis(3_000)
            .field("marker_key", "previous-shutdown-cause:-20")
            .build(),
        );

        let events = store.recent(10);
        assert_eq!(events.len(), 2);
        assert!(events[0].fields.iter().any(|field| {
            field.key == "marker_key" && field.value == "previous-shutdown-cause:-20"
        }));
        assert!(events[1].fields.iter().any(|field| {
            field.key == "marker_key" && field.value == "previous-shutdown-cause:-128"
        }));
    }

    #[test]
    fn overview_reports_last_error_timestamp() {
        let store = DiagnosticsStore::new(8);
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::Engine,
                "warn",
                "warn event",
            )
            .timestamp_millis(10)
            .build(),
        );
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Error,
                DiagnosticsSubsystem::Persistence,
                "error",
                "real failure",
            )
            .timestamp_millis(25)
            .build(),
        );
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "info",
                "recovery",
            )
            .timestamp_millis(40)
            .build(),
        );

        let overview = store.overview();
        assert_eq!(overview.last_error_message.as_deref(), Some("real failure"));
        assert_eq!(overview.last_error_millis, Some(25));
        assert_eq!(overview.last_event_millis, Some(40));
    }

    #[test]
    fn persistence_open_rewrites_events_that_no_longer_match_policy() {
        let path = std::env::temp_dir().join(format!(
            "aetower-diag-rewrite-{}.ndjson",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        std::fs::write(
            &path,
            concat!(
                "{\"id\":\"1\",\"timestamp_millis\":1,\"level\":\"info\",\"subsystem\":\"ui\",\"event_type\":\"anomaly-notifications-suppressed\",\"sequence\":null,\"entity_id\":null,\"adapter\":null,\"capability\":null,\"message\":\"old noise\",\"fields\":[],\"sensitive\":false}\n",
                "{\"id\":\"2\",\"timestamp_millis\":2,\"level\":\"warn\",\"subsystem\":\"persistence\",\"event_type\":\"history-load-failed\",\"sequence\":null,\"entity_id\":null,\"adapter\":null,\"capability\":null,\"message\":\"kept\",\"fields\":[],\"sensitive\":false}\n"
            ),
        )
        .expect("seed persisted file");

        let store = DiagnosticsStore::with_persistence(8, &path, 16).expect("store");
        let events = store.recent(10);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "history-load-failed");

        let persisted = std::fs::read_to_string(&path).expect("persisted file");
        assert!(!persisted.contains("anomaly-notifications-suppressed"));
        assert!(persisted.contains("history-load-failed"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_empties_ring_and_persisted_file() {
        let path =
            std::env::temp_dir().join(format!("aetower-diag-clear-{}.ndjson", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let store = DiagnosticsStore::with_persistence(8, &path, 16).expect("store");
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::Engine,
                "kept",
                "persist me",
            )
            .build(),
        );
        store.flush_persistence().expect("flush");
        assert_eq!(store.recent(10).len(), 1);
        assert!(store.overview().persisted_events >= 1);

        store.clear().expect("clear");

        let overview = store.overview();
        assert_eq!(overview.current_size, 0);
        assert_eq!(overview.persisted_events, 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), "");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn overview_ignores_recovered_mcp_server_start_failures() {
        let store = DiagnosticsStore::new(16);
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Error,
                DiagnosticsSubsystem::Ffi,
                "mcp-server-start-failed",
                "Failed to start local MCP server",
            )
            .timestamp_millis(10)
            .build(),
        );
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Ffi,
                "mcp-server-started",
                "Started local MCP server",
            )
            .timestamp_millis(20)
            .build(),
        );

        let overview = store.overview();
        assert_eq!(overview.error_count, 0);
        assert_eq!(overview.last_error_millis, None);
        assert_eq!(overview.last_error_message, None);
    }

    #[test]
    fn query_filters_recent_ring_events() {
        let store = DiagnosticsStore::new(8);
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "engine-initialized",
                "ready",
            )
            .field("scope", "startup")
            .build(),
        );
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::Telemetry,
                "telemetry-verification-failed",
                "export failed",
            )
            .field("endpoint", "localhost")
            .build(),
        );

        let results = store.query(&DiagnosticsQuery {
            limit: 10,
            minimum_level: Some(DiagnosticsLevel::Warn),
            subsystem: Some(DiagnosticsSubsystem::Telemetry),
            search: Some("localhost".to_owned()),
            since_millis: None,
            include_persisted: false,
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type, "telemetry-verification-failed");
    }

    #[test]
    fn query_reads_persisted_events() {
        let path =
            std::env::temp_dir().join(format!("aetower-diag-query-{}.ndjson", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let store = match DiagnosticsStore::with_persistence(4, &path, 16) {
            Ok(store) => store,
            Err(error) => panic!("store: {error}"),
        };
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Warn,
                DiagnosticsSubsystem::Persistence,
                "history-load-failed",
                "load failed",
            )
            .field("error", "decode")
            .build(),
        );

        let query = DiagnosticsQuery {
            limit: 10,
            minimum_level: Some(DiagnosticsLevel::Warn),
            subsystem: Some(DiagnosticsSubsystem::Persistence),
            search: Some("decode".to_owned()),
            since_millis: None,
            include_persisted: true,
        };
        let results = store.query(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type, "history-load-failed");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn runtime_heartbeat_info_events_persist() {
        let path = std::env::temp_dir().join(format!(
            "aetower-diag-heartbeat-{}.ndjson",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let store = match DiagnosticsStore::with_persistence(4, &path, 16) {
            Ok(store) => store,
            Err(error) => panic!("store: {error}"),
        };
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Engine,
                "runtime-heartbeat",
                "heartbeat",
            )
            .field("entity_count", 12)
            .build(),
        );
        if let Err(error) = store.flush_persistence() {
            panic!("flush: {error}");
        }

        let persisted = std::fs::read_to_string(&path).expect("persisted file");
        assert!(persisted.contains("runtime-heartbeat"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn row_level_history_quarantine_events_do_not_persist() {
        let path = std::env::temp_dir().join(format!(
            "aetower-diag-history-quarantine-{}.ndjson",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let store = match DiagnosticsStore::with_persistence(4, &path, 16) {
            Ok(store) => store,
            Err(error) => panic!("store: {error}"),
        };
        store.emit(
            DiagnosticsEvent::builder(
                DiagnosticsLevel::Info,
                DiagnosticsSubsystem::Persistence,
                "history-row-quarantined",
                "quarantined row",
            )
            .field("sequence", 42)
            .build(),
        );
        if let Err(error) = store.flush_persistence() {
            panic!("flush: {error}");
        }

        let persisted = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!persisted.contains("history-row-quarantined"));

        let _ = std::fs::remove_file(&path);
    }
}
