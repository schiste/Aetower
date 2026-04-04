use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::Arc,
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
    pub last_error_message: Option<String>,
    pub persisted_events: u64,
    pub persisted_path: Option<String>,
    pub persisted_bytes: u64,
    pub persistence_error: Option<String>,
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
    persistence: Option<PersistedDiagnostics>,
}

#[derive(Debug)]
struct PersistedDiagnostics {
    path: PathBuf,
    max_events: usize,
    event_count: usize,
    compact_at: usize,
    last_error: Option<String>,
}

impl DiagnosticsStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DiagnosticsState {
                events: VecDeque::with_capacity(capacity),
                capacity: capacity.max(1),
                next_id: 1,
                dropped_events: 0,
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
        let mut error_count = 0u32;
        let mut warn_count = 0u32;
        let mut last_error_message = None;

        for event in guard.events.iter().rev() {
            match event.level {
                DiagnosticsLevel::Error => {
                    error_count = error_count.saturating_add(1);
                    if last_error_message.is_none() {
                        last_error_message = Some(event.message.clone());
                    }
                }
                DiagnosticsLevel::Warn => {
                    warn_count = warn_count.saturating_add(1);
                }
                _ => {}
            }
        }

        DiagnosticsOverview {
            ring_capacity: guard.capacity.min(u32::MAX as usize) as u32,
            current_size: guard.events.len().min(u32::MAX as usize) as u32,
            dropped_events: guard.dropped_events,
            error_count,
            warn_count,
            last_event_millis: guard.events.back().map(|event| event.timestamp_millis),
            last_error_message,
            persisted_events: guard
                .persistence
                .as_ref()
                .map(|persistence| persistence.event_count.min(u64::MAX as usize) as u64)
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
                .and_then(|persistence| persistence.last_error.clone()),
        }
    }

    pub fn export_json(&self, limit: usize) -> String {
        serde_json::to_string_pretty(&self.recent(limit))
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
    }

    pub fn clear(&self) -> io::Result<()> {
        let mut guard = self.inner.lock();
        guard.events.clear();
        guard.next_id = 1;
        guard.dropped_events = 0;
        if let Some(persistence) = guard.persistence.as_mut() {
            persistence.clear()?;
        }
        Ok(())
    }
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

        let event_count = retained.len();

        let mut persistence = Self {
            path: path.to_path_buf(),
            max_events,
            event_count,
            compact_at: max_events.saturating_add((max_events / 4).max(1)),
            last_error: None,
        };
        if needs_rewrite {
            persistence.rewrite_from_events(&retained)?;
        }
        persistence.compact_if_needed();
        Ok((events, persistence))
    }

    fn append(&mut self, event: &DiagnosticsEvent) {
        if !should_persist_event(event) {
            return;
        }
        match self.try_append(event) {
            Ok(()) => {
                self.last_error = None;
                self.compact_if_needed();
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
            }
        }
    }

    fn try_append(&mut self, event: &DiagnosticsEvent) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, event).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        self.event_count = self.event_count.saturating_add(1);
        Ok(())
    }

    fn compact_if_needed(&mut self) {
        if self.event_count <= self.compact_at {
            return;
        }
        match self.compact_to_last(self.max_events) {
            Ok(()) => self.last_error = None,
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    fn compact_to_last(&mut self, keep: usize) -> std::io::Result<()> {
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut retained = VecDeque::with_capacity(keep.max(1));

        for line in reader.lines() {
            let line = line?;
            if retained.len() >= keep.max(1) {
                retained.pop_front();
            }
            retained.push_back(line);
        }

        let mut file = File::create(&self.path)?;
        for line in &retained {
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")?;
        }
        self.event_count = retained.len();
        Ok(())
    }

    fn rewrite_from_events(&mut self, events: &VecDeque<DiagnosticsEvent>) -> std::io::Result<()> {
        let mut file = File::create(&self.path)?;
        for event in events {
            serde_json::to_writer(&mut file, event).map_err(io::Error::other)?;
            file.write_all(b"\n")?;
        }
        self.event_count = events.len();
        Ok(())
    }

    fn file_bytes(&self) -> u64 {
        std::fs::metadata(&self.path)
            .map(|metadata| metadata.len())
            .unwrap_or(0)
    }

    fn clear(&mut self) -> io::Result<()> {
        File::create(&self.path)?;
        self.event_count = 0;
        self.last_error = None;
        Ok(())
    }
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
                | "history-pruned"
                | "history-row-quarantined"
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
                | "session-log-window-noise"
                | "session-log-metal-error"
                | "session-log-analysis-failed"
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

        let persisted = std::fs::read_to_string(&path).expect("persisted file");
        assert!(!persisted.contains("tick-completed"));
        assert!(persisted.contains("history-load-failed"));

        let _ = std::fs::remove_file(&path);
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
        assert_eq!(store.recent(10).len(), 1);
        assert!(store.overview().persisted_events >= 1);

        store.clear().expect("clear");

        let overview = store.overview();
        assert_eq!(overview.current_size, 0);
        assert_eq!(overview.persisted_events, 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), "");

        let _ = std::fs::remove_file(&path);
    }
}
