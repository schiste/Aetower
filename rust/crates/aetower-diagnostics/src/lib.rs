use std::{collections::VecDeque, sync::Arc};

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
}

impl DiagnosticsStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DiagnosticsState {
                events: VecDeque::with_capacity(capacity),
                capacity: capacity.max(1),
                next_id: 1,
                dropped_events: 0,
            })),
        }
    }

    pub fn emit(&self, mut event: DiagnosticsEvent) {
        let mut guard = self.inner.lock();
        if event.timestamp_millis == 0 {
            event.timestamp_millis = now_millis();
        }
        event.id = format!("diag-{}", guard.next_id);
        guard.next_id = guard.next_id.saturating_add(1);

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
        }
    }

    pub fn export_json(&self, limit: usize) -> String {
        serde_json::to_string_pretty(&self.recent(limit))
            .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
    }
}

impl Default for DiagnosticsStore {
    fn default() -> Self {
        Self::new(2_000)
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
}
