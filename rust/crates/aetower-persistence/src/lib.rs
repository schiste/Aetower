use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsStore, DiagnosticsSubsystem,
};
use aetower_model::SystemSnapshot;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const SNAPSHOT_FORMAT_VERSION: i64 = 2;
const MAX_PENDING_HISTORY_WRITES: u64 = 64;
/// Upper bound on how long a history read waits for the background writer
/// queue to drain before serving possibly slightly-stale data. See
/// `HistoryStore::flush_for_read` for the tradeoff.
const HISTORY_READ_FLUSH_TIMEOUT: Duration = Duration::from_millis(250);
const MIN_SIZE_PRESSURE_SNAPSHOT_TARGET: u64 = 120;
const HISTORY_MAINTENANCE_WARN_MILLIS: u64 = 5_000;
const HISTORY_MAINTENANCE_WAL_WARNING_BYTES: u64 = 32 * 1024 * 1024;
const MAX_COALESCED_WRITE_GAP_MILLIS: u64 = 60_000;
const HISTORY_TOP_ENTITY_SIGNATURE_LIMIT: usize = 8;
const HOST_CPU_BUCKET_PERCENT: f32 = 2.0;
const HOST_WAKEUP_BUCKET_PER_SECOND: f32 = 50.0;
const HOST_THROUGHPUT_BUCKET_BYTES: u64 = 512 * 1024;
const MEMORY_BUCKET_BYTES: u64 = 16 * 1024 * 1024;
const ENTITY_CPU_BUCKET_PERCENT: f32 = 2.0;
const ENTITY_FRICTION_BUCKET_POINTS: f32 = 2.0;
const ENTITY_WAKEUP_BUCKET_PER_SECOND: f32 = 25.0;
const HISTORY_PERSISTED_ENTITY_LIMIT: usize = 64;
const HISTORY_PERSISTED_TREND_LIMIT: usize = 12;
const HISTORY_PERSISTED_TIMELINE_LIMIT: usize = 128;
const HISTORY_PERSISTED_RECOMMENDATION_LIMIT: usize = 3;
const HISTORY_PERSISTED_SESSION_MARKER_LIMIT: usize = 8;
const HISTORY_PERSISTED_HOST_VECTOR_LIMIT: usize = 16;
const HISTORY_PERSISTED_AI_REPO_LIMIT: usize = 64;
const HISTORY_PERSISTED_CHAU7_SESSION_LIMIT: usize = 64;
const HISTORY_PERSISTED_CHAU7_LINK_LIMIT: usize = 16;
const HISTORY_ROLLUP_BUCKETS_MILLIS: [u64; 2] = [60_000, 3_600_000];
const HISTORY_ROLLUP_RETENTION_MILLIS: u64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSnapshotEnvelope {
    version: u16,
    snapshot: SystemSnapshot,
}

struct QuarantineCandidate<'a> {
    snapshot_id: i64,
    captured_at_millis: u64,
    sequence: u64,
    format_version: i64,
    json_blob: Option<&'a str>,
    bincode_blob: Option<&'a [u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HistorySnapshotSignature {
    host: HostWriteSignature,
    entity_count: u32,
    top_entities: Vec<EntityWriteSignature>,
    capability_states: Vec<CapabilityWriteSignature>,
    timeline_tail: Option<TimelineWriteSignature>,
    ai_repo_count: u32,
    chau7_session_count: u32,
    thermal_forecast_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostWriteSignature {
    cpu_bucket: i32,
    memory_bucket: u64,
    swap_bucket: u64,
    compressed_memory_bucket: u64,
    disk_bucket: u64,
    network_bucket: u64,
    wakeup_bucket: i32,
    thermal_state: String,
    low_power_mode: bool,
    on_battery: bool,
    ai_agent_count: u32,
    gpu_bucket: i32,
    gpu_memory_bucket: u64,
    frontmost_app_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntityWriteSignature {
    entity_id: String,
    friction_bucket: i32,
    cpu_bucket: i32,
    memory_bucket: u64,
    disk_bucket: u64,
    network_bucket: u64,
    wakeup_bucket: i32,
    process_count: u32,
    anomaly_detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapabilityWriteSignature {
    kind: String,
    state: String,
    health: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimelineWriteSignature {
    id: String,
    timestamp_millis: u64,
    severity: String,
    category: String,
    entity_id: Option<String>,
}

impl HistorySnapshotSignature {
    fn from_snapshot(snapshot: &SystemSnapshot) -> Self {
        let mut capability_states = snapshot
            .capabilities
            .iter()
            .map(|capability| CapabilityWriteSignature {
                kind: format!("{:?}", capability.kind),
                state: format!("{:?}", capability.state),
                health: format!("{:?}", capability.health),
            })
            .collect::<Vec<_>>();
        capability_states.sort_by(|lhs, rhs| lhs.kind.cmp(&rhs.kind));

        Self {
            host: HostWriteSignature::from_snapshot(snapshot),
            entity_count: snapshot.entities.len().min(u32::MAX as usize) as u32,
            top_entities: snapshot
                .entities
                .iter()
                .take(HISTORY_TOP_ENTITY_SIGNATURE_LIMIT)
                .map(EntityWriteSignature::from_entity)
                .collect(),
            capability_states,
            timeline_tail: snapshot
                .timeline
                .last()
                .map(TimelineWriteSignature::from_event),
            ai_repo_count: snapshot.ai_repo_summaries.len().min(u32::MAX as usize) as u32,
            chau7_session_count: snapshot.chau7_sessions.len().min(u32::MAX as usize) as u32,
            thermal_forecast_active: snapshot.thermal_forecast.is_some(),
        }
    }
}

impl HostWriteSignature {
    fn from_snapshot(snapshot: &SystemSnapshot) -> Self {
        let host = &snapshot.host;
        Self {
            cpu_bucket: bucket_f32(host.cpu_percent, HOST_CPU_BUCKET_PERCENT),
            memory_bucket: bucket_u64(host.memory_used_bytes, MEMORY_BUCKET_BYTES),
            swap_bucket: bucket_u64(host.swap_used_bytes, MEMORY_BUCKET_BYTES),
            compressed_memory_bucket: bucket_u64(host.compressed_memory_bytes, MEMORY_BUCKET_BYTES),
            disk_bucket: bucket_u64(
                host.disk_read_bps.saturating_add(host.disk_write_bps),
                HOST_THROUGHPUT_BUCKET_BYTES,
            ),
            network_bucket: bucket_u64(
                host.network_receive_bps
                    .saturating_add(host.network_send_bps),
                HOST_THROUGHPUT_BUCKET_BYTES,
            ),
            wakeup_bucket: bucket_f32(host.wakeups_per_second, HOST_WAKEUP_BUCKET_PER_SECOND),
            thermal_state: format!("{:?}", host.thermal_state),
            low_power_mode: host.low_power_mode,
            on_battery: host.on_battery,
            ai_agent_count: host.ai_agent_count,
            gpu_bucket: bucket_f32(host.gpu_percent, HOST_CPU_BUCKET_PERCENT),
            gpu_memory_bucket: bucket_u64(host.gpu_memory_bytes, MEMORY_BUCKET_BYTES),
            frontmost_app_name: host.frontmost_app_name.clone(),
        }
    }
}

impl EntityWriteSignature {
    fn from_entity(entity: &aetower_model::EntitySnapshot) -> Self {
        Self {
            entity_id: entity.entity_id.clone(),
            friction_bucket: bucket_f32(entity.friction.total_score, ENTITY_FRICTION_BUCKET_POINTS),
            cpu_bucket: bucket_f32(entity.metrics.cpu_percent, ENTITY_CPU_BUCKET_PERCENT),
            memory_bucket: bucket_u64(
                aetower_model::entity_effective_memory_bytes(&entity.metrics),
                MEMORY_BUCKET_BYTES,
            ),
            disk_bucket: bucket_u64(
                entity
                    .metrics
                    .disk_read_bps
                    .saturating_add(entity.metrics.disk_write_bps),
                HOST_THROUGHPUT_BUCKET_BYTES,
            ),
            network_bucket: bucket_u64(
                entity
                    .metrics
                    .network_receive_bps
                    .saturating_add(entity.metrics.network_send_bps),
                HOST_THROUGHPUT_BUCKET_BYTES,
            ),
            wakeup_bucket: bucket_f32(
                entity.metrics.wakeups_per_second,
                ENTITY_WAKEUP_BUCKET_PER_SECOND,
            ),
            process_count: entity.metrics.process_count,
            anomaly_detected: entity.anomaly_detected,
        }
    }
}

impl TimelineWriteSignature {
    fn from_event(event: &aetower_model::TimelineEvent) -> Self {
        Self {
            id: event.id.clone(),
            timestamp_millis: event.timestamp_millis,
            severity: format!("{:?}", event.severity),
            category: format!("{:?}", event.category),
            entity_id: event.entity_id.clone(),
        }
    }
}

fn bucket_f32(value: f32, bucket_size: f32) -> i32 {
    if !value.is_finite() || bucket_size <= 0.0 {
        return 0;
    }
    (value / bucket_size).round() as i32
}

fn bucket_u64(value: u64, bucket_size: u64) -> u64 {
    if bucket_size == 0 {
        return value;
    }
    value.saturating_add(bucket_size / 2) / bucket_size
}

fn compact_snapshot_for_history(snapshot: &SystemSnapshot) -> SystemSnapshot {
    let mut compact = snapshot.clone();

    compact.host.frontmost_window_title = None;
    compact
        .host
        .fans
        .truncate(HISTORY_PERSISTED_HOST_VECTOR_LIMIT);
    compact
        .host
        .cpu_temperatures
        .truncate(HISTORY_PERSISTED_HOST_VECTOR_LIMIT);
    compact
        .host
        .power_readings
        .truncate(HISTORY_PERSISTED_HOST_VECTOR_LIMIT);
    compact
        .host
        .network_interfaces
        .truncate(HISTORY_PERSISTED_HOST_VECTOR_LIMIT);
    compact
        .host
        .disks
        .truncate(HISTORY_PERSISTED_HOST_VECTOR_LIMIT);
    compact
        .host
        .bluetooth_devices
        .truncate(HISTORY_PERSISTED_HOST_VECTOR_LIMIT);
    compact
        .host
        .per_core_cpu
        .truncate(HISTORY_PERSISTED_HOST_VECTOR_LIMIT);

    truncate_tail(
        &mut compact.host_trend.machine_friction,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.cpu_percent,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.memory_used_bytes,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.memory_pressure_score,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.disk_activity_bps,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.network_activity_bps,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.wakeups_per_second,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.compressed_memory_bytes,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.ai_agent_friction,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.gpu_percent,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.gpu_memory_bytes,
        HISTORY_PERSISTED_TREND_LIMIT,
    );
    truncate_tail(
        &mut compact.host_trend.max_cpu_temperature,
        HISTORY_PERSISTED_TREND_LIMIT,
    );

    truncate_tail(&mut compact.timeline, HISTORY_PERSISTED_TIMELINE_LIMIT);
    compact
        .ai_repo_summaries
        .truncate(HISTORY_PERSISTED_AI_REPO_LIMIT);
    compact
        .chau7_sessions
        .truncate(HISTORY_PERSISTED_CHAU7_SESSION_LIMIT);
    for session in &mut compact.chau7_sessions {
        session
            .linked_entity_ids
            .truncate(HISTORY_PERSISTED_CHAU7_LINK_LIMIT);
    }

    compact.entities.truncate(HISTORY_PERSISTED_ENTITY_LIMIT);
    for entity in &mut compact.entities {
        entity.primary_provenance = None;
        entity.launcher_summary = None;
        entity.attribution_notes.clear();
        entity.active_window_title = None;
        entity.components.clear();
        entity.network_connections.clear();
        entity
            .friction
            .contributors
            .truncate(HISTORY_PERSISTED_RECOMMENDATION_LIMIT);
        entity
            .recommendations
            .truncate(HISTORY_PERSISTED_RECOMMENDATION_LIMIT);
        entity
            .session_markers
            .truncate(HISTORY_PERSISTED_SESSION_MARKER_LIMIT);
        truncate_tail(&mut entity.trend.friction, HISTORY_PERSISTED_TREND_LIMIT);
        truncate_tail(&mut entity.trend.cpu_percent, HISTORY_PERSISTED_TREND_LIMIT);
        truncate_tail(
            &mut entity.trend.memory_resident_bytes,
            HISTORY_PERSISTED_TREND_LIMIT,
        );
        truncate_tail(
            &mut entity.trend.disk_activity_bps,
            HISTORY_PERSISTED_TREND_LIMIT,
        );
        truncate_tail(
            &mut entity.trend.network_activity_bps,
            HISTORY_PERSISTED_TREND_LIMIT,
        );
        truncate_tail(
            &mut entity.trend.wakeups_per_second,
            HISTORY_PERSISTED_TREND_LIMIT,
        );
    }

    compact
}

fn truncate_tail<T>(items: &mut Vec<T>, limit: usize) {
    if items.len() > limit {
        let drop_count = items.len() - limit;
        items.drain(0..drop_count);
    }
}

#[derive(Debug, Clone, Default)]
pub struct HistoryRangeSummary {
    pub store_bytes: u64,
    pub wal_bytes: u64,
    pub snapshot_count: u64,
    pub quarantine_count: u64,
    pub range_count: u64,
    pub oldest_millis: Option<u64>,
    pub newest_millis: Option<u64>,
    pub pending_writes: u64,
    pub store_modified_millis: Option<u64>,
    pub wal_modified_millis: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryMaintenanceReport {
    pub store_bytes_before: u64,
    pub wal_bytes_before: u64,
    pub store_bytes_after: u64,
    pub wal_bytes_after: u64,
    pub checkpointed: bool,
    pub vacuumed: bool,
    pub pruned_rows: u64,
    pub aggressive_reason: Option<String>,
    pub cancelled: bool,
    pub elapsed_millis: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct HistoryRetentionPolicy {
    pub max_age_millis: u64,
    pub emergency_max_age_millis: u64,
    pub soft_max_store_bytes: u64,
    pub hard_max_store_bytes: u64,
    pub max_wal_bytes: u64,
    pub target_snapshot_count: u64,
    pub soft_max_snapshot_count: u64,
    pub hard_max_snapshot_count: u64,
    pub aggressive_quarantine_rows: u64,
    pub hard_max_quarantine_rows: u64,
}

impl HistoryRetentionPolicy {
    fn cutoff_for(&self, newest_millis: u64, age_millis: u64) -> u64 {
        newest_millis.saturating_sub(age_millis)
    }
}

pub struct HistoryStore {
    conn: Connection,
    db_path: PathBuf,
    write_counter: u32,
    write_interval: u32,
    diagnostics: Option<DiagnosticsStore>,
    writer: HistoryWriter,
    last_persisted_signature: Option<HistorySnapshotSignature>,
    last_persisted_millis: Option<u64>,
    coalesced_write_count: u64,
    /// `Some` only when the most recent `open()` ran the auto_vacuum
    /// migration. Drained by `set_diagnostics` so the audit-trail event is
    /// emitted exactly once per process, *after* the sink is wired up
    /// (the migration itself runs before the engine plumbs diagnostics).
    pending_migration_event: Option<AutoVacuumMigrationOutcome>,
}

struct HistoryWriter {
    command_tx: mpsc::Sender<HistoryCommand>,
    pending_writes: Arc<AtomicUsize>,
    /// Shared with the writer thread, which records the first async write
    /// failure here. Readers drain it (via `flush`/`flush_with_timeout`) so a
    /// past write error fails the next read instead of vanishing silently.
    last_error: Arc<Mutex<Option<String>>>,
    handle: Option<JoinHandle<()>>,
}

enum HistoryCommand {
    Store(Box<SystemSnapshot>),
    /// Pure queue barrier: the reply carries no data. Callers drain
    /// `HistoryWriter::last_error` themselves after the barrier completes,
    /// so an abandoned reply channel (bounded-wait timeout) never swallows a
    /// recorded write error.
    Flush(mpsc::Sender<()>),
    Prune(u64, mpsc::Sender<Result<u64, String>>),
    PruneKeepLatest(u64, mpsc::Sender<Result<u64, String>>),
    TrimQuarantineKeepLatest(u64, mpsc::Sender<Result<u64, String>>),
    ClearAll(mpsc::Sender<Result<(), String>>),
    Shutdown,
}

impl HistoryStore {
    /// Open (or create) the history database at `path`.
    /// `write_interval` controls how many ticks between persisted snapshots
    /// (e.g. 5 = write every 5th tick).
    pub fn open(path: &Path, write_interval: u32) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        let pending_migration_event = configure_connection(&conn, path)?;
        let writer = HistoryWriter::spawn(path.to_path_buf())?;

        Ok(Self {
            conn,
            db_path: path.to_path_buf(),
            write_counter: 0,
            write_interval: write_interval.max(1),
            diagnostics: None,
            writer,
            last_persisted_signature: None,
            last_persisted_millis: None,
            coalesced_write_count: 0,
            pending_migration_event,
        })
    }

    pub fn set_diagnostics(&mut self, diagnostics: DiagnosticsStore) {
        self.diagnostics = Some(diagnostics);
        // If the most recent open() ran the auto_vacuum migration, drop a
        // single audit-trail event now that the sink is plumbed in. `take`
        // makes this exactly-once even if `set_diagnostics` is called
        // multiple times in the lifetime of a store.
        if let Some(outcome) = self.pending_migration_event.take()
            && let Some(diag) = self.diagnostics.as_ref()
        {
            let bytes_reclaimed = outcome
                .store_bytes_before
                .saturating_sub(outcome.store_bytes_after);
            diag.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Persistence,
                    "history-migration-applied",
                    "Converted history store to auto_vacuum=INCREMENTAL.",
                )
                .field("store_bytes_before", outcome.store_bytes_before)
                .field("store_bytes_after", outcome.store_bytes_after)
                .field("bytes_reclaimed", bytes_reclaimed)
                .field("elapsed_millis", outcome.elapsed_millis)
                .build(),
            );
        }
    }

    /// Called on every engine tick. Only persists every `write_interval`th call.
    pub fn maybe_store(&mut self, snapshot: &SystemSnapshot) {
        self.write_counter += 1;
        if self.write_counter < self.write_interval {
            return;
        }
        self.write_counter = 0;
        let Some(signature) = self.prepare_store(snapshot, "interval") else {
            return;
        };
        if self.enqueue_store(snapshot, "history-write-backpressure") {
            self.mark_snapshot_persisted(snapshot, signature);
        }
    }

    /// Queue a snapshot immediately, bypassing the tick-based write interval.
    /// This is used to flush the latest deferred snapshot after brief store
    /// contention, so maintenance windows do not permanently drop the newest
    /// history point.
    pub fn store_immediately(&mut self, snapshot: &SystemSnapshot) {
        let Some(signature) = self.prepare_store(snapshot, "immediate") else {
            return;
        };
        if self.enqueue_store(snapshot, "history-deferred-write-backpressure") {
            self.mark_snapshot_persisted(snapshot, signature);
        }
    }

    fn prepare_store(
        &mut self,
        snapshot: &SystemSnapshot,
        trigger: &'static str,
    ) -> Option<HistorySnapshotSignature> {
        let signature = HistorySnapshotSignature::from_snapshot(snapshot);
        let signature_changed = self.last_persisted_signature.as_ref() != Some(&signature);
        let gap_elapsed = self
            .last_persisted_millis
            .map(|last| {
                snapshot.captured_at_millis.saturating_sub(last) >= MAX_COALESCED_WRITE_GAP_MILLIS
            })
            .unwrap_or(true);

        if signature_changed || gap_elapsed {
            return Some(signature);
        }

        self.coalesced_write_count = self.coalesced_write_count.saturating_add(1);
        if self.coalesced_write_count.is_multiple_of(60)
            && let Some(diagnostics) = self.diagnostics.as_ref()
        {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Persistence,
                    "history-write-coalesced",
                    "Skipped persisted history writes because live snapshots were materially unchanged.",
                )
                .field("coalesced_writes", self.coalesced_write_count)
                .field("trigger", trigger)
                .field("last_persisted_millis", self.last_persisted_millis.unwrap_or(0))
                .field("current_millis", snapshot.captured_at_millis)
                .field("max_gap_millis", MAX_COALESCED_WRITE_GAP_MILLIS)
                .build(),
            );
        }
        None
    }

    fn mark_snapshot_persisted(
        &mut self,
        snapshot: &SystemSnapshot,
        signature: HistorySnapshotSignature,
    ) {
        self.last_persisted_signature = Some(signature);
        self.last_persisted_millis = Some(snapshot.captured_at_millis);
        self.coalesced_write_count = 0;
    }

    fn enqueue_store(&mut self, snapshot: &SystemSnapshot, event_type: &'static str) -> bool {
        let pending_writes = self.writer.pending_writes();
        if pending_writes >= MAX_PENDING_HISTORY_WRITES {
            if let Some(diagnostics) = self.diagnostics.as_ref() {
                diagnostics.emit(
                    DiagnosticsEvent::builder(
                        DiagnosticsLevel::Warn,
                        DiagnosticsSubsystem::Persistence,
                        event_type,
                        "Skipped a persisted history write because the writer queue is backed up.",
                    )
                    .field("pending_writes", pending_writes)
                    .build(),
                );
            }
            return false;
        }
        self.writer.store(snapshot.clone()).is_ok()
    }

    /// Bounded pre-read flush shared by `load_range_page` and
    /// `range_summary`.
    ///
    /// Tradeoff: an unbounded `writer.flush()` guarantees read-your-writes but
    /// couples read latency to the writer queue — when the writer falls
    /// behind (slow disk, checkpoint stall) a UI read could block for the
    /// full backlog. Instead we skip the round trip entirely when the queue
    /// is empty (the common case, so read-your-writes still holds), wait up
    /// to `HISTORY_READ_FLUSH_TIMEOUT` when it is not, and on timeout serve
    /// the read against slightly-stale data while emitting a
    /// `history-read-behind` diagnostic. Either path still drains and
    /// surfaces any stored async writer error, so a past write failure fails
    /// the next read exactly as before.
    fn flush_for_read(&self, context: &'static str) -> Result<(), String> {
        match self.writer.flush_with_timeout(HISTORY_READ_FLUSH_TIMEOUT) {
            Ok(true) => Ok(()),
            Ok(false) => {
                if let Some(diagnostics) = self.diagnostics.as_ref() {
                    diagnostics.emit(
                        DiagnosticsEvent::builder(
                            DiagnosticsLevel::Warn,
                            DiagnosticsSubsystem::Persistence,
                            "history-read-behind",
                            "Served a history read before the writer queue fully drained.",
                        )
                        .field("context", context)
                        .field("pending_writes", self.writer.pending_writes())
                        .field(
                            "flush_timeout_millis",
                            HISTORY_READ_FLUSH_TIMEOUT.as_millis() as u64,
                        )
                        .build(),
                    );
                }
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Load snapshots in a time range (inclusive).
    pub fn load_range(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<Vec<SystemSnapshot>, String> {
        self.load_range_page(start_millis, end_millis, None, 500)
    }

    pub fn load_range_page(
        &self,
        start_millis: u64,
        end_millis: u64,
        before_millis_exclusive: Option<u64>,
        limit: u32,
    ) -> Result<Vec<SystemSnapshot>, String> {
        self.flush_for_read("load-range")?;
        let mut stmt = match self
            .conn
            .prepare(
                "SELECT id, captured_at_millis, sequence, format_version, bincode_blob, json_blob FROM snapshots
                 WHERE captured_at_millis >= ?1
                   AND captured_at_millis <= ?2
                   AND (?3 IS NULL OR captured_at_millis < ?3)
                 ORDER BY captured_at_millis DESC
                 LIMIT ?4",
            )
            .map_err(|e| format!("prepare: {e}"))
        {
            Ok(stmt) => stmt,
            Err(error) => {
                self.emit_load_event(start_millis, end_millis, 0, 0, None, Some(&error));
                return Err(error);
            }
        };

        let rows = match stmt
            .query_map(
                params![
                    start_millis as i64,
                    end_millis as i64,
                    before_millis_exclusive.map(|value| value as i64),
                    limit.max(1) as i64
                ],
                |row| {
                    let id: i64 = row.get(0)?;
                    let captured_at_millis: i64 = row.get(1)?;
                    let sequence: i64 = row.get(2)?;
                    let format_version: i64 = row.get(3)?;
                    let bincode_blob: Option<Vec<u8>> = row.get(4)?;
                    let json_blob: Option<String> = row.get(5)?;
                    Ok((
                        id,
                        captured_at_millis,
                        sequence,
                        format_version,
                        bincode_blob,
                        json_blob,
                    ))
                },
            )
            .map_err(|e| format!("query: {e}"))
        {
            Ok(rows) => rows,
            Err(error) => {
                self.emit_load_event(start_millis, end_millis, 0, 0, None, Some(&error));
                return Err(error);
            }
        };

        let mut snapshots = Vec::new();
        let mut quarantined_rows = 0u64;
        let mut quarantine_reason: Option<String> = None;
        for row in rows {
            let (id, captured_at_millis, sequence, format_version, bincode_blob, json_blob) =
                match row.map_err(|e| format!("row: {e}")) {
                    Ok(row) => row,
                    Err(error) => {
                        self.emit_load_event(start_millis, end_millis, 0, 0, None, Some(&error));
                        return Err(error);
                    }
                };
            let snapshot = if let Some(blob) = bincode_blob {
                match self.decode_snapshot(&blob, format_version) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        if let Err(quarantine_error) = self.quarantine_row(
                            QuarantineCandidate {
                                snapshot_id: id,
                                captured_at_millis: captured_at_millis as u64,
                                sequence: sequence as u64,
                                format_version,
                                json_blob: json_blob.as_deref(),
                                bincode_blob: Some(&blob),
                            },
                            &error,
                        ) {
                            self.emit_load_event(
                                start_millis,
                                end_millis,
                                snapshots.len() as u64,
                                quarantined_rows,
                                None,
                                Some(&quarantine_error),
                            );
                            return Err(quarantine_error);
                        }
                        quarantined_rows = quarantined_rows.saturating_add(1);
                        quarantine_reason.get_or_insert(error);
                        continue;
                    }
                }
            } else if let Some(json) = json_blob {
                match serde_json::from_str(&json).map_err(|e| format!("json deserialize: {e}")) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        if let Err(quarantine_error) = self.quarantine_row(
                            QuarantineCandidate {
                                snapshot_id: id,
                                captured_at_millis: captured_at_millis as u64,
                                sequence: sequence as u64,
                                format_version,
                                json_blob: Some(&json),
                                bincode_blob: None,
                            },
                            &error,
                        ) {
                            self.emit_load_event(
                                start_millis,
                                end_millis,
                                snapshots.len() as u64,
                                quarantined_rows,
                                None,
                                Some(&quarantine_error),
                            );
                            return Err(quarantine_error);
                        }
                        quarantined_rows = quarantined_rows.saturating_add(1);
                        quarantine_reason.get_or_insert(error);
                        continue;
                    }
                }
            } else {
                continue;
            };
            snapshots.push(snapshot);
        }
        snapshots.reverse();
        self.emit_load_event(
            start_millis,
            end_millis,
            snapshots.len() as u64,
            quarantined_rows,
            quarantine_reason.as_deref(),
            None,
        );
        Ok(snapshots)
    }

    pub fn range_summary(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<HistoryRangeSummary, String> {
        self.flush_for_read("range-summary")?;
        let (range_count, oldest_millis, newest_millis): (u64, Option<u64>, Option<u64>) = self
            .conn
            .query_row(
                "SELECT COUNT(*), MIN(captured_at_millis), MAX(captured_at_millis)
                 FROM snapshots
                 WHERE captured_at_millis >= ?1 AND captured_at_millis <= ?2",
                params![start_millis as i64, end_millis as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u64,
                        row.get::<_, Option<i64>>(1)?.map(|value| value as u64),
                        row.get::<_, Option<i64>>(2)?.map(|value| value as u64),
                    ))
                },
            )
            .map_err(|e| format!("range summary: {e}"))?;
        let snapshot_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
            .map_err(|e| format!("snapshot count: {e}"))?;
        let quarantine_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshot_quarantine", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
            .map_err(|e| format!("quarantine count: {e}"))?;
        let (store_bytes, wal_bytes) = self.store_file_sizes();

        Ok(HistoryRangeSummary {
            store_bytes,
            wal_bytes,
            snapshot_count,
            quarantine_count,
            range_count,
            oldest_millis,
            newest_millis,
            pending_writes: self.writer.pending_writes(),
            store_modified_millis: file_modified_millis(&self.db_path),
            wal_modified_millis: file_modified_millis(&self.wal_path()),
        })
    }

    pub fn maintain_storage(&self, aggressive: bool) -> Result<HistoryMaintenanceReport, String> {
        self.maintain_storage_inner(aggressive, None)
    }

    fn maintain_storage_inner(
        &self,
        aggressive: bool,
        cancel: Option<&Arc<AtomicBool>>,
    ) -> Result<HistoryMaintenanceReport, String> {
        let started = Instant::now();
        self.writer.flush()?;
        let (store_bytes_before, wal_bytes_before) = self.store_file_sizes();
        let aggressive_reason = if aggressive {
            Some("manual-request".to_owned())
        } else {
            None
        };
        if cancellation_requested(cancel) {
            return Ok(cancelled_maintenance_report(
                started,
                (store_bytes_before, wal_bytes_before),
                (store_bytes_before, wal_bytes_before),
                false,
                aggressive_reason,
            ));
        }
        // `incremental_vacuum` returns pages on the freelist back to the OS
        // when `auto_vacuum=INCREMENTAL` is active (set by the migration in
        // `configure_connection`). It is a documented no-op when the database
        // is still in legacy `auto_vacuum=NONE` mode, so the call is safe to
        // include unconditionally during the transition window. Running it on
        // every maintenance pass keeps the on-disk freelist small enough that
        // the heavier `VACUUM` gate below almost never fires after the initial
        // migration.
        let checkpoint_cancelled = self.execute_batch_cancellable(
            "PRAGMA wal_checkpoint(TRUNCATE); PRAGMA optimize; PRAGMA incremental_vacuum;",
            cancel,
            "checkpoint history store",
        )?;
        if checkpoint_cancelled {
            let (store_bytes_after, wal_bytes_after) = self.store_file_sizes();
            return Ok(cancelled_maintenance_report(
                started,
                (store_bytes_before, wal_bytes_before),
                (store_bytes_after, wal_bytes_after),
                false,
                aggressive_reason,
            ));
        }
        let mut vacuumed = false;
        if aggressive && self.should_vacuum()? {
            if cancellation_requested(cancel) {
                let (store_bytes_after, wal_bytes_after) = self.store_file_sizes();
                return Ok(cancelled_maintenance_report(
                    started,
                    (store_bytes_before, wal_bytes_before),
                    (store_bytes_after, wal_bytes_after),
                    true,
                    aggressive_reason,
                ));
            }
            let vacuum_cancelled =
                self.execute_batch_cancellable("VACUUM;", cancel, "vacuum history store")?;
            if vacuum_cancelled {
                let (store_bytes_after, wal_bytes_after) = self.store_file_sizes();
                return Ok(cancelled_maintenance_report(
                    started,
                    (store_bytes_before, wal_bytes_before),
                    (store_bytes_after, wal_bytes_after),
                    true,
                    aggressive_reason,
                ));
            }
            vacuumed = true;
        }
        let (store_bytes_after, wal_bytes_after) = self.store_file_sizes();
        let report = HistoryMaintenanceReport {
            store_bytes_before,
            wal_bytes_before,
            store_bytes_after,
            wal_bytes_after,
            checkpointed: true,
            vacuumed,
            pruned_rows: 0,
            aggressive_reason,
            cancelled: false,
            elapsed_millis: started.elapsed().as_millis() as u64,
        };
        if history_storage_maintenance_should_emit(&report, aggressive)
            && let Some(diagnostics) = self.diagnostics.as_ref()
        {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Persistence,
                    "history-maintained",
                    "Ran persisted history maintenance.",
                )
                .field("aggressive", aggressive)
                .field("checkpointed", report.checkpointed)
                .field("vacuumed", report.vacuumed)
                .field("store_bytes_before", report.store_bytes_before)
                .field("wal_bytes_before", report.wal_bytes_before)
                .field("store_bytes_after", report.store_bytes_after)
                .field("wal_bytes_after", report.wal_bytes_after)
                .field("cancelled", report.cancelled)
                .field("elapsed_millis", report.elapsed_millis)
                .build(),
            );
        }
        Ok(report)
    }

    pub fn maintain_with_policy(
        &self,
        policy: HistoryRetentionPolicy,
    ) -> Result<HistoryMaintenanceReport, String> {
        self.maintain_with_policy_inner(policy, None)
    }

    pub fn maintain_with_policy_cancellable(
        &self,
        policy: HistoryRetentionPolicy,
        cancel: Arc<AtomicBool>,
    ) -> Result<HistoryMaintenanceReport, String> {
        self.maintain_with_policy_inner(policy, Some(&cancel))
    }

    fn maintain_with_policy_inner(
        &self,
        policy: HistoryRetentionPolicy,
        cancel: Option<&Arc<AtomicBool>>,
    ) -> Result<HistoryMaintenanceReport, String> {
        let started = Instant::now();
        self.writer.flush()?;
        let (store_bytes_before, wal_bytes_before) = self.store_file_sizes();
        if cancellation_requested(cancel) {
            return Ok(cancelled_maintenance_report(
                started,
                (store_bytes_before, wal_bytes_before),
                (store_bytes_before, wal_bytes_before),
                false,
                None,
            ));
        }
        let quarantine_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshot_quarantine", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
            .map_err(|e| format!("quarantine count: {e}"))?;
        let snapshot_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
            .map_err(|e| format!("snapshot count: {e}"))?;
        let newest_millis = self
            .conn
            .query_row("SELECT MAX(captured_at_millis) FROM snapshots", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .map(|value| value.map(|millis| millis as u64))
            .map_err(|e| format!("max captured_at_millis: {e}"))?;
        let mut pruned_rows = 0u64;
        let mut aggressive_reasons = Vec::new();

        if let Some(newest_millis) = newest_millis {
            let cutoff = policy.cutoff_for(newest_millis, policy.max_age_millis);
            if cancellation_requested(cancel) {
                return Ok(cancelled_maintenance_report(
                    started,
                    (store_bytes_before, wal_bytes_before),
                    (store_bytes_before, wal_bytes_before),
                    false,
                    None,
                ));
            }
            let deleted = self.writer.prune(cutoff)?;
            if deleted > 0 {
                pruned_rows = pruned_rows.saturating_add(deleted);
                aggressive_reasons.push(format!(
                    "retention-window>{}h",
                    policy.max_age_millis / 3_600_000
                ));
            }
            let rollup_cutoff = newest_millis.saturating_sub(HISTORY_ROLLUP_RETENTION_MILLIS);
            let deleted_rollups = self.prune_rollups(rollup_cutoff)?;
            if deleted_rollups > 0 {
                pruned_rows = pruned_rows.saturating_add(deleted_rollups);
                aggressive_reasons.push("rollup-retention".to_owned());
            }
        }

        let threshold_aggressive = store_bytes_before >= policy.soft_max_store_bytes
            || wal_bytes_before >= policy.max_wal_bytes
            || snapshot_count >= policy.soft_max_snapshot_count
            || quarantine_count >= policy.aggressive_quarantine_rows;
        if store_bytes_before >= policy.soft_max_store_bytes {
            aggressive_reasons.push("store-bytes".to_owned());
        }
        if wal_bytes_before >= policy.max_wal_bytes {
            aggressive_reasons.push("wal-bytes".to_owned());
        }
        if snapshot_count >= policy.soft_max_snapshot_count {
            aggressive_reasons.push("snapshot-count".to_owned());
        }
        if quarantine_count >= policy.aggressive_quarantine_rows {
            aggressive_reasons.push("quarantine-rows".to_owned());
        }
        let hard_pressure = store_bytes_before >= policy.hard_max_store_bytes
            || snapshot_count > policy.hard_max_snapshot_count
            || quarantine_count > policy.hard_max_quarantine_rows;
        let wal_checkpoint_due = wal_bytes_before > 0;
        if pruned_rows == 0 && !threshold_aggressive && !hard_pressure && !wal_checkpoint_due {
            return Ok(HistoryMaintenanceReport {
                store_bytes_before,
                wal_bytes_before,
                store_bytes_after: store_bytes_before,
                wal_bytes_after: wal_bytes_before,
                checkpointed: false,
                vacuumed: false,
                pruned_rows: 0,
                aggressive_reason: None,
                cancelled: false,
                elapsed_millis: started.elapsed().as_millis() as u64,
            });
        }

        let mut report = self.maintain_storage_inner(threshold_aggressive, cancel)?;
        report.elapsed_millis = started.elapsed().as_millis() as u64;
        if report.cancelled {
            return Ok(report);
        }
        report.pruned_rows = pruned_rows;
        report.aggressive_reason = if aggressive_reasons.is_empty() {
            None
        } else {
            Some(aggressive_reasons.join(","))
        };

        if report.store_bytes_after > policy.hard_max_store_bytes
            && let Some(newest_millis) = newest_millis
        {
            let emergency_cutoff =
                policy.cutoff_for(newest_millis, policy.emergency_max_age_millis);
            if cancellation_requested(cancel) {
                report.cancelled = true;
                report.elapsed_millis = started.elapsed().as_millis() as u64;
                return Ok(report);
            }
            let deleted = self.writer.prune(emergency_cutoff)?;
            if deleted > 0 {
                report.pruned_rows = report.pruned_rows.saturating_add(deleted);
                let mut reasons = report.aggressive_reason.take().unwrap_or_default();
                if !reasons.is_empty() {
                    reasons.push(',');
                }
                reasons.push_str("emergency-retention");
                report.aggressive_reason = Some(reasons);
            }
            let post_emergency = self.maintain_storage_inner(true, cancel)?;
            report.store_bytes_after = post_emergency.store_bytes_after;
            report.wal_bytes_after = post_emergency.wal_bytes_after;
            report.vacuumed = report.vacuumed || post_emergency.vacuumed;
            report.checkpointed = report.checkpointed || post_emergency.checkpointed;
            report.cancelled = post_emergency.cancelled;
            report.elapsed_millis = started.elapsed().as_millis() as u64;
            if report.cancelled {
                return Ok(report);
            }
        }

        let snapshot_count_after = self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
            .map_err(|e| format!("snapshot count after maintenance: {e}"))?;
        let size_pressure_target = size_pressure_snapshot_target(
            snapshot_count_after,
            report.store_bytes_after,
            policy.soft_max_store_bytes,
        );
        let should_trim_to_target = report.store_bytes_after > policy.soft_max_store_bytes
            || report.wal_bytes_after > policy.max_wal_bytes
            || snapshot_count_after > policy.soft_max_snapshot_count;
        let target_keep =
            if should_trim_to_target && snapshot_count_after > policy.target_snapshot_count {
                Some(policy.target_snapshot_count)
            } else {
                size_pressure_target
            };
        if let Some(target_keep) = target_keep.filter(|target| snapshot_count_after > *target) {
            let deleted = self.writer.prune_keep_latest(target_keep)?;
            report.pruned_rows = report.pruned_rows.saturating_add(deleted);
            let mut reasons = report.aggressive_reason.take().unwrap_or_default();
            if !reasons.is_empty() {
                reasons.push(',');
            }
            if size_pressure_target == Some(target_keep) {
                reasons.push_str("size-pressure-snapshot-cap");
            } else {
                reasons.push_str("target-snapshot-cap");
            }
            report.aggressive_reason = Some(reasons);
            let post_trim = self.maintain_storage_inner(true, cancel)?;
            report.store_bytes_after = post_trim.store_bytes_after;
            report.wal_bytes_after = post_trim.wal_bytes_after;
            report.vacuumed = report.vacuumed || post_trim.vacuumed;
            report.checkpointed = report.checkpointed || post_trim.checkpointed;
            report.cancelled = post_trim.cancelled;
            report.elapsed_millis = started.elapsed().as_millis() as u64;
            if report.cancelled {
                return Ok(report);
            }
        } else if snapshot_count_after > policy.hard_max_snapshot_count {
            if cancellation_requested(cancel) {
                report.cancelled = true;
                report.elapsed_millis = started.elapsed().as_millis() as u64;
                return Ok(report);
            }
            let deleted = self
                .writer
                .prune_keep_latest(policy.hard_max_snapshot_count)?;
            report.pruned_rows = report.pruned_rows.saturating_add(deleted);
            let mut reasons = report.aggressive_reason.take().unwrap_or_default();
            if !reasons.is_empty() {
                reasons.push(',');
            }
            reasons.push_str("snapshot-hard-cap");
            report.aggressive_reason = Some(reasons);
            let post_trim = self.maintain_storage_inner(true, cancel)?;
            report.store_bytes_after = post_trim.store_bytes_after;
            report.wal_bytes_after = post_trim.wal_bytes_after;
            report.vacuumed = report.vacuumed || post_trim.vacuumed;
            report.checkpointed = report.checkpointed || post_trim.checkpointed;
            report.cancelled = post_trim.cancelled;
            report.elapsed_millis = started.elapsed().as_millis() as u64;
            if report.cancelled {
                return Ok(report);
            }
        }

        if quarantine_count > policy.hard_max_quarantine_rows {
            if cancellation_requested(cancel) {
                report.cancelled = true;
                report.elapsed_millis = started.elapsed().as_millis() as u64;
                return Ok(report);
            }
            let deleted = self
                .writer
                .trim_quarantine_keep_latest(policy.hard_max_quarantine_rows)?;
            report.pruned_rows = report.pruned_rows.saturating_add(deleted);
            let mut reasons = report.aggressive_reason.take().unwrap_or_default();
            if !reasons.is_empty() {
                reasons.push(',');
            }
            reasons.push_str("quarantine-hard-cap");
            report.aggressive_reason = Some(reasons);
        }

        report.elapsed_millis = started.elapsed().as_millis() as u64;

        let level = history_retention_maintenance_level(&report, &policy, quarantine_count);
        if history_retention_maintenance_should_emit(&report, &level)
            && let Some(diagnostics) = self.diagnostics.as_ref()
        {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    level,
                    DiagnosticsSubsystem::Persistence,
                    "history-retention-maintained",
                    "Applied persisted history retention and maintenance policy.",
                )
                .field("store_bytes_before", report.store_bytes_before)
                .field("wal_bytes_before", report.wal_bytes_before)
                .field("store_bytes_after", report.store_bytes_after)
                .field("wal_bytes_after", report.wal_bytes_after)
                .field("pruned_rows", report.pruned_rows)
                .field("checkpointed", report.checkpointed)
                .field("vacuumed", report.vacuumed)
                .field("cancelled", report.cancelled)
                .field("elapsed_millis", report.elapsed_millis)
                .field("snapshot_count", snapshot_count)
                .field("quarantine_count", quarantine_count)
                .field(
                    "aggressive_reason",
                    report
                        .aggressive_reason
                        .clone()
                        .unwrap_or_else(|| "none".to_owned()),
                )
                .build(),
            );
        }

        Ok(report)
    }

    /// Delete snapshots older than `cutoff_millis`.
    pub fn prune(&self, cutoff_millis: u64) -> Result<u64, String> {
        let deleted = self.writer.prune(cutoff_millis)?;
        if let Some(diagnostics) = self.diagnostics.as_ref() {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Persistence,
                    "history-pruned",
                    "Pruned persisted history rows.",
                )
                .field("cutoff_millis", cutoff_millis)
                .field("deleted_rows", deleted)
                .build(),
            );
        }
        Ok(deleted)
    }

    pub fn clear_all(&self) -> Result<(), String> {
        self.writer.clear_all()?;
        if let Some(diagnostics) = self.diagnostics.as_ref() {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Warn,
                    DiagnosticsSubsystem::Persistence,
                    "history-cleared",
                    "Cleared persisted history and quarantine rows.",
                )
                .build(),
            );
        }
        Ok(())
    }

    fn emit_load_event(
        &self,
        start_millis: u64,
        end_millis: u64,
        loaded_count: u64,
        quarantined_rows: u64,
        quarantine_reason: Option<&str>,
        error: Option<&str>,
    ) {
        let Some(diagnostics) = self.diagnostics.as_ref() else {
            return;
        };
        let mut builder = DiagnosticsEvent::builder(
            if error.is_some() {
                DiagnosticsLevel::Error
            } else if quarantined_rows > 0 {
                DiagnosticsLevel::Warn
            } else {
                DiagnosticsLevel::Info
            },
            DiagnosticsSubsystem::Persistence,
            if error.is_some() {
                "history-load-failed"
            } else if quarantined_rows > 0 {
                "history-loaded-with-quarantine"
            } else {
                "history-loaded"
            },
            if error.is_some() {
                "Failed to load persisted history range."
            } else if quarantined_rows > 0 {
                "Loaded history after quarantining incompatible persisted rows."
            } else {
                "Loaded persisted history range."
            },
        )
        .field("start_millis", start_millis)
        .field("end_millis", end_millis)
        .field("loaded_count", loaded_count)
        .field("quarantined_rows", quarantined_rows);
        let (store_bytes, wal_bytes) = self.store_file_sizes();
        builder = builder
            .field("store_bytes", store_bytes)
            .field("wal_bytes", wal_bytes)
            .field("pending_writes", self.writer.pending_writes());
        if let Some(reason) = quarantine_reason {
            builder = builder.field("quarantine_reason", reason);
        }
        if let Some(error) = error {
            builder = builder.field("error", error);
        }
        diagnostics.emit(builder.build());
    }

    fn decode_snapshot(&self, blob: &[u8], format_version: i64) -> Result<SystemSnapshot, String> {
        decode_snapshot_blob(blob, format_version)
    }

    fn quarantine_row(
        &self,
        candidate: QuarantineCandidate<'_>,
        reason: &str,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO snapshot_quarantine (
                    snapshot_id, captured_at_millis, sequence, format_version, quarantined_at_millis, reason, json_blob, bincode_blob
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    candidate.snapshot_id,
                    candidate.captured_at_millis as i64,
                    candidate.sequence as i64,
                    candidate.format_version,
                    aetower_time::now_millis() as i64,
                    reason,
                    candidate.json_blob,
                    candidate.bincode_blob,
                ],
            )
            .map_err(|error| format!("quarantine insert: {error}"))?;
        self.conn
            .execute(
                "DELETE FROM snapshots WHERE id = ?1",
                params![candidate.snapshot_id],
            )
            .map_err(|error| format!("quarantine delete: {error}"))?;

        Ok(())
    }

    fn store_file_sizes(&self) -> (u64, u64) {
        let store_bytes = fs::metadata(&self.db_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        let wal_bytes = fs::metadata(self.wal_path())
            .map(|meta| meta.len())
            .unwrap_or(0);
        (store_bytes, wal_bytes)
    }

    /// Number of pages currently on the SQLite freelist.
    ///
    /// Surfaced so the engine heartbeat can publish on-disk waste over time,
    /// letting us spot a growing freelist long before it crosses
    /// `should_vacuum`'s gate. After the auto_vacuum=INCREMENTAL migration
    /// this should stay near zero in steady state; persistent growth here
    /// means `incremental_vacuum` is not keeping up with the delete rate.
    pub fn freelist_pages(&self) -> Result<u64, String> {
        self.conn
            .query_row("PRAGMA freelist_count", [], |row| row.get::<_, i64>(0))
            .map(|n| n.max(0) as u64)
            .map_err(|e| format!("pragma freelist_count: {e}"))
    }

    fn wal_path(&self) -> PathBuf {
        PathBuf::from(format!("{}-wal", self.db_path.display()))
    }

    fn should_vacuum(&self) -> Result<bool, String> {
        let page_count = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("pragma page_count: {e}"))?;
        let freelist_count = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("pragma freelist_count: {e}"))?;
        let (store_bytes, wal_bytes) = self.store_file_sizes();
        Ok(should_vacuum_decision(
            store_bytes,
            wal_bytes,
            page_count,
            freelist_count,
        ))
    }

    fn prune_rollups(&self, cutoff_millis: u64) -> Result<u64, String> {
        self.conn
            .execute(
                "DELETE FROM history_rollups WHERE bucket_start_millis < ?1",
                params![cutoff_millis as i64],
            )
            .map(|deleted| deleted as u64)
            .map_err(|error| format!("prune history rollups: {error}"))
    }
}

/// Minimum freelist size (in bytes) before VACUUM is worth running. Public so
/// the engine crate can assert it against `HISTORY_SOFT_MAX_BYTES` — the
/// original bug was that this number lived only inside `should_vacuum_decision`
/// at 512 MB while the policy treated 384 MB as "trouble," so the gate could
/// never fire before emergency retention kicked in. Keeping the constant
/// addressable makes that invariant testable across crates.
pub const VACUUM_TRIGGER_FREE_BYTES: u64 = 128 * 1024 * 1024;
/// Minimum fragmentation ratio (freelist / page_count) before VACUUM fires.
/// Pairs with `VACUUM_TRIGGER_FREE_BYTES` — both gates must be satisfied so
/// small databases with a few free pages don't trigger an expensive rewrite.
pub const VACUUM_TRIGGER_FRAGMENTATION_RATIO: f64 = 0.25;
/// Maximum WAL size that still permits a VACUUM. Above this the WAL would be
/// folded into the rewrite, which is expensive and pointless on a checkpoint
/// that hasn't yet truncated.
pub const VACUUM_MAX_WAL_BYTES: u64 = 64 * 1024 * 1024;

/// Pure decision used by `HistoryStore::should_vacuum`, extracted so tests
/// can pin down the boundary cases without needing a real SQLite file at a
/// specific size and fragmentation.
///
/// Gate on absolute waste, not total file size: a 200 MB file that is 90%
/// free wastes 180 MB of real disk while a 600 MB file that is 5% free
/// wastes only 30 MB. The previous gate of `store_bytes >= 512 MB` missed
/// the first case entirely and is why a real user's 394 MB DB accumulated
/// 250 MB of unreclaimed pages without ever triggering a VACUUM. The
/// 128 MB / 25%-ratio threshold catches pathological fragmentation early
/// but skips small healthy stores where the rewrite cost would dwarf the
/// gain. WAL is bounded separately so a large pending WAL doesn't get
/// rewritten alongside.
pub fn should_vacuum_decision(
    store_bytes: u64,
    wal_bytes: u64,
    page_count: i64,
    freelist_count: i64,
) -> bool {
    if page_count <= 0 {
        return false;
    }
    let fragmentation_ratio = freelist_count.max(0) as f64 / page_count as f64;
    let free_bytes = (store_bytes as f64 * fragmentation_ratio) as u64;
    free_bytes >= VACUUM_TRIGGER_FREE_BYTES
        && fragmentation_ratio >= VACUUM_TRIGGER_FRAGMENTATION_RATIO
        && wal_bytes <= VACUUM_MAX_WAL_BYTES
}

impl HistoryStore {
    fn execute_batch_cancellable(
        &self,
        sql: &str,
        cancel: Option<&Arc<AtomicBool>>,
        context: &str,
    ) -> Result<bool, String> {
        if cancellation_requested(cancel) {
            return Ok(true);
        }
        let mut cancel_watcher = None;
        let cancel_watcher_done = Arc::new(AtomicBool::new(false));
        if let Some(cancel) = cancel {
            let cancel = Arc::clone(cancel);
            let done = Arc::clone(&cancel_watcher_done);
            let interrupt = self.conn.get_interrupt_handle();
            cancel_watcher = Some(thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        interrupt.interrupt();
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            }));
        }
        let result = self.conn.execute_batch(sql);
        cancel_watcher_done.store(true, Ordering::Relaxed);
        if let Some(watcher) = cancel_watcher {
            let _ = watcher.join();
        }
        match result {
            Ok(()) => Ok(false),
            Err(error) if cancellation_requested(cancel) => Ok(true),
            Err(error) => Err(format!("{context}: {error}")),
        }
    }
}

fn cancellation_requested(cancel: Option<&Arc<AtomicBool>>) -> bool {
    cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed))
}

fn file_modified_millis(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

fn cancelled_maintenance_report(
    started: Instant,
    (store_bytes_before, wal_bytes_before): (u64, u64),
    (store_bytes_after, wal_bytes_after): (u64, u64),
    checkpointed: bool,
    aggressive_reason: Option<String>,
) -> HistoryMaintenanceReport {
    HistoryMaintenanceReport {
        store_bytes_before,
        wal_bytes_before,
        store_bytes_after,
        wal_bytes_after,
        checkpointed,
        vacuumed: false,
        pruned_rows: 0,
        aggressive_reason,
        cancelled: true,
        elapsed_millis: started.elapsed().as_millis() as u64,
    }
}

fn history_storage_maintenance_should_emit(
    report: &HistoryMaintenanceReport,
    aggressive: bool,
) -> bool {
    aggressive
        || report.cancelled
        || report.vacuumed
        || report.elapsed_millis >= HISTORY_MAINTENANCE_WARN_MILLIS
        || report.wal_bytes_after >= HISTORY_MAINTENANCE_WAL_WARNING_BYTES
        || report.store_bytes_after != report.store_bytes_before
        || report.wal_bytes_after != report.wal_bytes_before
}

fn history_retention_maintenance_level(
    report: &HistoryMaintenanceReport,
    policy: &HistoryRetentionPolicy,
    quarantine_count: u64,
) -> DiagnosticsLevel {
    let slow = report.elapsed_millis >= HISTORY_MAINTENANCE_WARN_MILLIS;
    let wal_unexpected = report.wal_bytes_after > policy.max_wal_bytes
        || (report.wal_bytes_after >= HISTORY_MAINTENANCE_WAL_WARNING_BYTES
            && report.wal_bytes_after >= report.wal_bytes_before);

    if report.cancelled
        || slow
        || quarantine_count >= policy.aggressive_quarantine_rows
        || wal_unexpected
    {
        DiagnosticsLevel::Warn
    } else {
        DiagnosticsLevel::Info
    }
}

fn history_retention_maintenance_should_emit(
    report: &HistoryMaintenanceReport,
    level: &DiagnosticsLevel,
) -> bool {
    matches!(level, DiagnosticsLevel::Warn | DiagnosticsLevel::Error)
        || report.pruned_rows > 0
        || report.vacuumed
        || report.cancelled
        || report.aggressive_reason.is_some()
}

impl HistoryStore {
    pub fn pending_writes(&self) -> u64 {
        self.writer.pending_writes()
    }
}

impl Drop for HistoryStore {
    fn drop(&mut self) {
        let _ = self.writer.flush();
        self.writer.shutdown();
    }
}

fn record_writer_error(last_error: &Arc<Mutex<Option<String>>>, error: String) {
    if let Ok(mut guard) = last_error.lock()
        && guard.is_none()
    {
        *guard = Some(error);
    }
}

fn drain_writer_error(last_error: &Arc<Mutex<Option<String>>>) -> Result<(), String> {
    let mut guard = last_error
        .lock()
        .map_err(|_| "history writer error state poisoned".to_owned())?;
    match guard.take() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

impl HistoryWriter {
    fn spawn(path: PathBuf) -> Result<Self, String> {
        let (command_tx, command_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let pending_writes = Arc::new(AtomicUsize::new(0));
        let pending_for_thread = Arc::clone(&pending_writes);
        let last_error = Arc::new(Mutex::new(None));
        let last_error_for_thread = Arc::clone(&last_error);
        let handle = thread::spawn(move || {
            let mut conn = match Connection::open(&path) {
                Ok(conn) => conn,
                Err(error) => {
                    let _ = ready_tx.send(Err(format!(
                        "open history writer {}: {error}",
                        path.display()
                    )));
                    return;
                }
            };
            // Writer thread opens its own connection to the same file; by
            // the time this runs the main connection has already migrated
            // auto_vacuum mode, so configure_connection will find it set
            // and skip the VACUUM. The migration outcome (if any) is
            // discarded here — only the main store reports it.
            if let Err(error) = configure_connection(&conn, &path) {
                let _ = ready_tx.send(Err(format!("configure history writer: {error}")));
                return;
            }
            let _ = ready_tx.send(Ok(()));

            while let Ok(command) = command_rx.recv() {
                match command {
                    HistoryCommand::Store(snapshot) => {
                        if let Err(error) = store_snapshot(&mut conn, &snapshot) {
                            record_writer_error(&last_error_for_thread, error);
                        }
                        pending_for_thread.fetch_sub(1, Ordering::Relaxed);
                    }
                    HistoryCommand::Flush(reply) => {
                        let _ = reply.send(());
                    }
                    HistoryCommand::Prune(cutoff_millis, reply) => {
                        let result = conn
                            .execute(
                                "DELETE FROM snapshots WHERE captured_at_millis < ?1",
                                params![cutoff_millis as i64],
                            )
                            .and_then(|deleted_snapshots| {
                                conn.execute(
                                    "DELETE FROM snapshot_quarantine WHERE captured_at_millis < ?1",
                                    params![cutoff_millis as i64],
                                )
                                .map(|deleted_quarantine| {
                                    deleted_snapshots as u64 + deleted_quarantine as u64
                                })
                            })
                            .map_err(|e| format!("prune: {e}"));
                        let _ = reply.send(result);
                    }
                    HistoryCommand::PruneKeepLatest(keep, reply) => {
                        let result = conn
                            .execute(
                                "DELETE FROM snapshots
                                 WHERE id IN (
                                     SELECT id FROM snapshots
                                     ORDER BY captured_at_millis DESC, id DESC
                                     LIMIT -1 OFFSET ?1
                                 )",
                                params![keep as i64],
                            )
                            .map(|deleted| deleted as u64)
                            .map_err(|e| format!("prune keep latest: {e}"));
                        let _ = reply.send(result);
                    }
                    HistoryCommand::TrimQuarantineKeepLatest(keep, reply) => {
                        let result = conn
                            .execute(
                                "DELETE FROM snapshot_quarantine
                                 WHERE id IN (
                                     SELECT id FROM snapshot_quarantine
                                     ORDER BY captured_at_millis DESC, id DESC
                                     LIMIT -1 OFFSET ?1
                                 )",
                                params![keep as i64],
                            )
                            .map(|deleted| deleted as u64)
                            .map_err(|e| format!("trim quarantine keep latest: {e}"));
                        let _ = reply.send(result);
                    }
                    HistoryCommand::ClearAll(reply) => {
                        let result = conn
                            .execute("DELETE FROM snapshots", [])
                            .map_err(|e| format!("clear snapshots: {e}"))
                            .and_then(|_| {
                                conn.execute("DELETE FROM snapshot_quarantine", [])
                                    .map_err(|e| format!("clear quarantine: {e}"))
                            })
                            .and_then(|_| {
                                conn.execute("DELETE FROM history_rollups", [])
                                    .map_err(|e| format!("clear rollups: {e}"))
                            })
                            .map(|_| ());
                        pending_for_thread.store(0, Ordering::Relaxed);
                        let _ = reply.send(result);
                    }
                    HistoryCommand::Shutdown => break,
                }
            }
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                command_tx,
                pending_writes,
                last_error,
                handle: Some(handle),
            }),
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(error) => {
                let _ = handle.join();
                Err(format!("history writer startup: {error}"))
            }
        }
    }

    fn store(&self, snapshot: SystemSnapshot) -> Result<(), String> {
        self.pending_writes.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = self
            .command_tx
            .send(HistoryCommand::Store(Box::new(snapshot)))
        {
            self.pending_writes.fetch_sub(1, Ordering::Relaxed);
            return Err(format!("history writer queue: {error}"));
        }
        Ok(())
    }

    /// Full read barrier: blocks until every write queued so far has been
    /// applied, then surfaces (and clears) any async write error recorded
    /// since the previous drain.
    fn flush(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(HistoryCommand::Flush(tx))
            .map_err(|error| format!("history writer queue: {error}"))?;
        rx.recv()
            .map_err(|error| format!("history writer flush: {error}"))?;
        drain_writer_error(&self.last_error)
    }

    /// Bounded read barrier for latency-sensitive readers.
    ///
    /// Fast path: if the queue is already empty (the common case — the engine
    /// stores at most one snapshot per tick), skip the writer-thread round
    /// trip entirely and just drain any stored async write error. Otherwise
    /// send a barrier and wait at most `timeout` for the queue to drain.
    ///
    /// Returns `Ok(true)` when the queue fully drained (read-your-writes
    /// holds), `Ok(false)` on timeout — the caller may proceed against
    /// slightly-stale data — and `Err` if a past async write failed (error
    /// slot drained either way, matching `flush` semantics).
    fn flush_with_timeout(&self, timeout: Duration) -> Result<bool, String> {
        if self.pending_writes() == 0 {
            drain_writer_error(&self.last_error)?;
            return Ok(true);
        }
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(HistoryCommand::Flush(tx))
            .map_err(|error| format!("history writer queue: {error}"))?;
        let drained = match rx.recv_timeout(timeout) {
            Ok(()) => true,
            Err(mpsc::RecvTimeoutError::Timeout) => false,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("history writer flush: writer thread gone".to_owned());
            }
        };
        // Drain the error slot even on timeout: writes that already completed
        // may have failed, and the whole point of the pre-read flush is that
        // a past write error fails the next read.
        drain_writer_error(&self.last_error)?;
        Ok(drained)
    }

    fn prune(&self, cutoff_millis: u64) -> Result<u64, String> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(HistoryCommand::Prune(cutoff_millis, tx))
            .map_err(|error| format!("history writer queue: {error}"))?;
        rx.recv()
            .map_err(|error| format!("history writer prune: {error}"))?
    }

    fn prune_keep_latest(&self, keep: u64) -> Result<u64, String> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(HistoryCommand::PruneKeepLatest(keep.max(1), tx))
            .map_err(|error| format!("history writer queue: {error}"))?;
        rx.recv()
            .map_err(|error| format!("history writer prune_keep_latest: {error}"))?
    }

    fn trim_quarantine_keep_latest(&self, keep: u64) -> Result<u64, String> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(HistoryCommand::TrimQuarantineKeepLatest(keep.max(1), tx))
            .map_err(|error| format!("history writer queue: {error}"))?;
        rx.recv()
            .map_err(|error| format!("history writer trim_quarantine_keep_latest: {error}"))?
    }

    fn clear_all(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(HistoryCommand::ClearAll(tx))
            .map_err(|error| format!("history writer queue: {error}"))?;
        rx.recv()
            .map_err(|error| format!("history writer clear: {error}"))?
    }

    fn pending_writes(&self) -> u64 {
        self.pending_writes.load(Ordering::Relaxed) as u64
    }

    fn shutdown(&mut self) {
        let _ = self.command_tx.send(HistoryCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn size_pressure_snapshot_target(
    snapshot_count: u64,
    store_bytes: u64,
    soft_max_store_bytes: u64,
) -> Option<u64> {
    if snapshot_count <= MIN_SIZE_PRESSURE_SNAPSHOT_TARGET || store_bytes <= soft_max_store_bytes {
        return None;
    }
    let proportional_target = ((snapshot_count as u128 * soft_max_store_bytes as u128)
        / store_bytes.max(1) as u128) as u64;
    let target = proportional_target
        .max(MIN_SIZE_PRESSURE_SNAPSHOT_TARGET)
        .min(snapshot_count.saturating_sub(1));
    (target < snapshot_count).then_some(target)
}

/// Outcome of the one-time `auto_vacuum=INCREMENTAL` migration, captured at
/// `HistoryStore::open` so we can emit a single audit-trail diagnostic event
/// once `set_diagnostics` plumbs the diagnostics sink in. Without this the
/// migration is silent — if it ever fails partially or takes minutes on a
/// slow disk we'd have no record.
#[derive(Debug, Clone)]
pub(crate) struct AutoVacuumMigrationOutcome {
    pub store_bytes_before: u64,
    pub store_bytes_after: u64,
    pub elapsed_millis: u64,
}

fn configure_connection(
    conn: &Connection,
    path: &Path,
) -> Result<Option<AutoVacuumMigrationOutcome>, String> {
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("WAL: {e}"))?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| format!("synchronous: {e}"))?;
    conn.pragma_update(None, "busy_timeout", 5_000)
        .map_err(|e| format!("busy_timeout: {e}"))?;

    // One-time migration: switch the file to `auto_vacuum=INCREMENTAL`.
    // SQLite only honors `auto_vacuum` changes on an existing database when
    // followed by a `VACUUM` that rewrites the file in the new mode, so the
    // first launch after upgrade absorbs that cost (measured at ~1.3 s on a
    // 394 MB store with 63% freelist — far less than initially feared).
    // Subsequent opens find the mode already set and skip the VACUUM. On a
    // brand-new empty database the VACUUM is a near-instant no-op.
    //
    // The before/after/elapsed numbers are captured here and returned to
    // the caller; `HistoryStore::open` stashes them and `set_diagnostics`
    // emits a `history-migration-applied` event once the sink is wired up
    // — that gives us a one-shot audit trail per upgrade.
    const AUTO_VACUUM_INCREMENTAL: i64 = 2;
    let auto_vacuum_mode: i64 = conn
        .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))
        .map_err(|e| format!("auto_vacuum query: {e}"))?;
    let migration = if auto_vacuum_mode != AUTO_VACUUM_INCREMENTAL {
        let store_bytes_before = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let started = Instant::now();
        conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")
            .map_err(|e| format!("auto_vacuum set: {e}"))?;
        conn.execute_batch("VACUUM;")
            .map_err(|e| format!("auto_vacuum migration VACUUM: {e}"))?;
        let elapsed_millis = started.elapsed().as_millis() as u64;
        let store_bytes_after = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        Some(AutoVacuumMigrationOutcome {
            store_bytes_before,
            store_bytes_after,
            elapsed_millis,
        })
    } else {
        None
    };

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            captured_at_millis INTEGER NOT NULL,
            sequence INTEGER NOT NULL,
            format_version INTEGER NOT NULL DEFAULT 1,
            json_blob TEXT,
            bincode_blob BLOB
        );
        CREATE TABLE IF NOT EXISTS snapshot_quarantine (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER,
            captured_at_millis INTEGER NOT NULL,
            sequence INTEGER NOT NULL,
            format_version INTEGER,
            quarantined_at_millis INTEGER NOT NULL,
            reason TEXT NOT NULL,
            json_blob TEXT,
            bincode_blob BLOB
        );
        CREATE TABLE IF NOT EXISTS history_rollups (
            bucket_millis INTEGER NOT NULL,
            bucket_start_millis INTEGER NOT NULL,
            bucket_end_millis INTEGER NOT NULL,
            sample_count INTEGER NOT NULL,
            sum_cpu_percent REAL NOT NULL,
            sum_memory_used_bytes REAL NOT NULL,
            sum_memory_pressure_score REAL NOT NULL,
            sum_wakeups_per_second REAL NOT NULL,
            sum_machine_friction REAL NOT NULL,
            max_machine_friction REAL NOT NULL,
            sum_gpu_percent REAL NOT NULL,
            top_entity_json TEXT,
            updated_at_millis INTEGER NOT NULL,
            PRIMARY KEY (bucket_millis, bucket_start_millis)
        );
        CREATE INDEX IF NOT EXISTS idx_snapshots_time
            ON snapshots(captured_at_millis);
        CREATE INDEX IF NOT EXISTS idx_history_rollups_time
            ON history_rollups(bucket_millis, bucket_start_millis);",
    )
    .map_err(|e| format!("schema: {e}"))?;
    let _ = conn.execute(
        "ALTER TABLE snapshots ADD COLUMN format_version INTEGER NOT NULL DEFAULT 1",
        [],
    );
    Ok(migration)
}

fn decode_snapshot_blob(blob: &[u8], format_version: i64) -> Result<SystemSnapshot, String> {
    if format_version >= SNAPSHOT_FORMAT_VERSION {
        return bincode::deserialize::<PersistedSnapshotEnvelope>(blob)
            .map(|envelope| envelope.snapshot)
            .map_err(|error| format!("bincode envelope deserialize: {error}"));
    }

    bincode::deserialize(blob).map_err(|error| format!("bincode deserialize: {error}"))
}

fn store_snapshot(conn: &mut Connection, snapshot: &SystemSnapshot) -> Result<usize, String> {
    let envelope = PersistedSnapshotEnvelope {
        version: SNAPSHOT_FORMAT_VERSION as u16,
        snapshot: compact_snapshot_for_history(snapshot),
    };
    let blob = bincode::serialize(&envelope).map_err(|e| format!("serialize snapshot: {e}"))?;
    // One explicit transaction per stored snapshot: the snapshots INSERT and
    // the per-bucket rollup upserts commit together as a single WAL commit
    // instead of three. This is both the dominant per-tick write cost and a
    // consistency win — a failure anywhere rolls the whole snapshot back, so
    // rollups can never drift ahead of (or behind) the snapshots table.
    let tx = conn
        .transaction()
        .map_err(|e| format!("begin snapshot transaction: {e}"))?;
    let inserted = tx
        .execute(
        "INSERT INTO snapshots (captured_at_millis, sequence, format_version, bincode_blob) VALUES (?1, ?2, ?3, ?4)",
        params![
            snapshot.captured_at_millis as i64,
            snapshot.sequence as i64,
            SNAPSHOT_FORMAT_VERSION,
            blob
        ],
    )
        .map_err(|e| format!("insert: {e}"))?;
    upsert_history_rollups(&tx, snapshot)?;
    tx.commit()
        .map_err(|e| format!("commit snapshot transaction: {e}"))?;
    Ok(inserted)
}

fn upsert_history_rollups(conn: &Connection, snapshot: &SystemSnapshot) -> Result<(), String> {
    let memory_pressure_score = aetower_model::host_memory_pressure_score(&snapshot.host);
    let machine_friction = aetower_model::machine_friction_score(&snapshot.host);
    let top_entity_json = snapshot
        .entities
        .first()
        .map(|entity| {
            serde_json::json!({
                "entity_id": entity.entity_id.as_str(),
                "display_name": entity.display_name.as_str(),
                "friction": entity.friction.total_score,
            })
            .to_string()
        })
        .unwrap_or_default();

    for bucket_millis in HISTORY_ROLLUP_BUCKETS_MILLIS {
        let bucket_start_millis =
            snapshot.captured_at_millis / bucket_millis.max(1) * bucket_millis.max(1);
        let bucket_end_millis = bucket_start_millis.saturating_add(bucket_millis.saturating_sub(1));
        conn.execute(
            "INSERT INTO history_rollups (
                bucket_millis,
                bucket_start_millis,
                bucket_end_millis,
                sample_count,
                sum_cpu_percent,
                sum_memory_used_bytes,
                sum_memory_pressure_score,
                sum_wakeups_per_second,
                sum_machine_friction,
                max_machine_friction,
                sum_gpu_percent,
                top_entity_json,
                updated_at_millis
             ) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?11)
             ON CONFLICT(bucket_millis, bucket_start_millis) DO UPDATE SET
                bucket_end_millis = MAX(history_rollups.bucket_end_millis, excluded.bucket_end_millis),
                sample_count = history_rollups.sample_count + excluded.sample_count,
                sum_cpu_percent = history_rollups.sum_cpu_percent + excluded.sum_cpu_percent,
                sum_memory_used_bytes = history_rollups.sum_memory_used_bytes + excluded.sum_memory_used_bytes,
                sum_memory_pressure_score = history_rollups.sum_memory_pressure_score + excluded.sum_memory_pressure_score,
                sum_wakeups_per_second = history_rollups.sum_wakeups_per_second + excluded.sum_wakeups_per_second,
                sum_machine_friction = history_rollups.sum_machine_friction + excluded.sum_machine_friction,
                max_machine_friction = MAX(history_rollups.max_machine_friction, excluded.max_machine_friction),
                sum_gpu_percent = history_rollups.sum_gpu_percent + excluded.sum_gpu_percent,
                top_entity_json = CASE
                    WHEN excluded.max_machine_friction >= history_rollups.max_machine_friction
                    THEN excluded.top_entity_json
                    ELSE history_rollups.top_entity_json
                END,
                updated_at_millis = excluded.updated_at_millis",
            params![
                bucket_millis as i64,
                bucket_start_millis as i64,
                bucket_end_millis as i64,
                snapshot.host.cpu_percent as f64,
                snapshot.host.memory_used_bytes as f64,
                memory_pressure_score as f64,
                snapshot.host.wakeups_per_second as f64,
                machine_friction as f64,
                snapshot.host.gpu_percent as f64,
                top_entity_json,
                aetower_time::now_millis() as i64,
            ],
        )
        .map_err(|error| format!("upsert history rollup: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db() -> PathBuf {
        let dir = std::env::temp_dir().join("aetower-test");
        std::fs::create_dir_all(&dir).ok();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        dir.join(format!("history-{}-{n}.db", std::process::id()))
    }

    fn test_retention_policy() -> HistoryRetentionPolicy {
        HistoryRetentionPolicy {
            max_age_millis: u64::MAX,
            emergency_max_age_millis: u64::MAX,
            soft_max_store_bytes: u64::MAX,
            hard_max_store_bytes: u64::MAX,
            max_wal_bytes: u64::MAX,
            target_snapshot_count: u64::MAX,
            soft_max_snapshot_count: u64::MAX,
            hard_max_snapshot_count: u64::MAX,
            aggressive_quarantine_rows: u64::MAX,
            hard_max_quarantine_rows: u64::MAX,
        }
    }

    fn material_snapshot(sequence: u64, captured_at_millis: u64) -> SystemSnapshot {
        SystemSnapshot {
            sequence,
            captured_at_millis,
            host: aetower_model::HostSnapshot {
                cpu_percent: ((sequence % 50) as f32 + 1.0) * 3.0,
                memory_used_bytes: sequence.saturating_add(1) * MEMORY_BUCKET_BYTES,
                wakeups_per_second: sequence as f32 * 30.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn bulky_snapshot(sequence: u64, captured_at_millis: u64) -> SystemSnapshot {
        let mut snapshot = material_snapshot(sequence, captured_at_millis);
        snapshot.host.frontmost_window_title = Some("frontmost-window-title".repeat(256));
        snapshot.host_trend.cpu_percent = (0..128).map(|value| value as f32).collect();
        snapshot.host_trend.memory_used_bytes =
            (0..128).map(|value| value * MEMORY_BUCKET_BYTES).collect();
        snapshot.timeline = (0..200)
            .map(|idx| aetower_model::TimelineEvent {
                id: format!("timeline-{idx}"),
                timestamp_millis: captured_at_millis + idx,
                title: format!("Timeline event {idx}"),
                detail: "history detail ".repeat(128),
                ..Default::default()
            })
            .collect();
        snapshot.chau7_sessions = (0..96)
            .map(|idx| aetower_model::Chau7SessionSummary {
                id: format!("session-{idx}"),
                title: "session title ".repeat(64),
                linked_entity_ids: (0..64)
                    .map(|entity_idx| format!("entity-{entity_idx}"))
                    .collect(),
                ..Default::default()
            })
            .collect();
        snapshot.entities = (0..96)
            .map(|idx| {
                let trend_values = (0..96).map(|value| value as f32).collect::<Vec<_>>();
                aetower_model::EntitySnapshot {
                    entity_id: format!("entity-{idx}"),
                    display_name: format!("Entity {idx}"),
                    entity_kind: aetower_model::EntityKind::App,
                    executable_path: Some(format!("/Applications/Entity{idx}.app/Contents/MacOS/entity")),
                    metrics: aetower_model::AggregateMetrics {
                        cpu_percent: idx as f32,
                        memory_resident_bytes: (idx + 1) * MEMORY_BUCKET_BYTES,
                        disk_read_bps: 1024 * idx,
                        network_receive_bps: 2048 * idx,
                        process_count: 8,
                        ..Default::default()
                    },
                    friction: aetower_model::FrictionBreakdown {
                        total_score: idx as f32,
                        contributors: (0..8)
                            .map(|contributor_idx| aetower_model::FrictionContributor {
                                key: format!("contributor-{contributor_idx}"),
                                label: format!("Contributor {contributor_idx}"),
                                score: contributor_idx as f32,
                                detail: "contributor detail ".repeat(64),
                            })
                            .collect(),
                        ..Default::default()
                    },
                    components: (0..8)
                        .map(|component_idx| aetower_model::ComponentSnapshot {
                            title: format!("component-{component_idx}"),
                            detail: "component detail ".repeat(128),
                            command_line: Some("command argument ".repeat(256)),
                            executable_path: Some(format!(
                                "/Applications/Entity{idx}.app/Contents/MacOS/helper-{component_idx}"
                            )),
                            cpu_percent: idx as f32,
                            memory_bytes: (idx + 1) * MEMORY_BUCKET_BYTES,
                            ..Default::default()
                        })
                        .collect(),
                    trend: aetower_model::MetricTrend {
                        friction: trend_values.clone(),
                        cpu_percent: trend_values,
                        memory_resident_bytes: (0..96)
                            .map(|value| value * MEMORY_BUCKET_BYTES)
                            .collect(),
                        disk_activity_bps: (0..96).map(|value| value * 1024).collect(),
                        network_activity_bps: (0..96).map(|value| value * 2048).collect(),
                        wakeups_per_second: (0..96).map(|value| value as f32).collect(),
                    },
                    recommendations: (0..8)
                        .map(|recommendation_idx| aetower_model::Recommendation {
                            title: format!("Recommendation {recommendation_idx}"),
                            detail: "recommendation detail ".repeat(128),
                            ..Default::default()
                        })
                        .collect(),
                    session_markers: (0..16)
                        .map(|marker_idx| aetower_model::SessionMarker {
                            timestamp_millis: captured_at_millis + marker_idx,
                            label: format!("marker-{marker_idx}"),
                            ..Default::default()
                        })
                        .collect(),
                    network_connections: (0..16)
                        .map(|conn_idx| aetower_model::NetworkConnection {
                            protocol: "tcp".to_owned(),
                            local: format!("127.0.0.1:{}", 10_000 + conn_idx),
                            remote: Some(format!("203.0.113.{conn_idx}:443")),
                            state: "ESTABLISHED".to_owned(),
                        })
                        .collect(),
                    ..Default::default()
                }
            })
            .collect();
        snapshot
    }

    #[test]
    fn persisted_snapshots_update_minute_and_hour_rollups() {
        let path = temp_db();
        let mut store = match HistoryStore::open(&path, 1) {
            Ok(store) => store,
            Err(error) => panic!("open: {error}"),
        };
        store.maybe_store(&material_snapshot(1, 61_000));
        store.maybe_store(&material_snapshot(2, 62_000));
        if let Err(error) = store.writer.flush() {
            panic!("flush: {error}");
        }

        let rows = {
            let mut stmt = match store.conn.prepare(
                "SELECT bucket_millis, sample_count
                 FROM history_rollups
                 ORDER BY bucket_millis",
            ) {
                Ok(stmt) => stmt,
                Err(error) => panic!("prepare: {error}"),
            };
            let mapped = match stmt
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            {
                Ok(mapped) => mapped,
                Err(error) => panic!("query: {error}"),
            };
            match mapped.collect::<Result<Vec<_>, _>>() {
                Ok(rows) => rows,
                Err(error) => panic!("rows: {error}"),
            }
        };

        assert_eq!(rows, vec![(60_000, 2), (3_600_000, 2)]);
        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn snapshot_insert_and_rollups_commit_atomically() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        // Sabotage the rollup upsert (second statement of the write
        // transaction) so the snapshot INSERT (first statement) must roll
        // back with it.
        store
            .conn
            .execute("DROP TABLE history_rollups", [])
            .unwrap_or_else(|error| panic!("drop rollups: {error}"));

        store.maybe_store(&material_snapshot(1, 61_000));
        let error = store
            .writer
            .flush()
            .expect_err("rollup failure must surface through flush");
        assert!(error.contains("rollup"), "unexpected error: {error}");

        let snapshot_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
            .unwrap_or_else(|error| panic!("count snapshots: {error}"));
        assert_eq!(
            snapshot_count, 0,
            "snapshot insert must roll back when the rollup upsert fails"
        );

        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn read_fast_path_skips_flush_but_still_surfaces_writer_errors() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        store
            .conn
            .execute("DROP TABLE history_rollups", [])
            .unwrap_or_else(|error| panic!("drop rollups: {error}"));

        store.maybe_store(&material_snapshot(1, 61_000));
        // Wait for the writer to drain the queue so the subsequent read takes
        // the non-blocking fast path (pending == 0, no barrier round trip).
        let deadline = Instant::now() + Duration::from_secs(10);
        while store.pending_writes() > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(store.pending_writes(), 0, "writer did not drain in time");

        let error = store
            .load_range(0, 100_000)
            .expect_err("stored async writer error must fail the next read");
        assert!(error.contains("rollup"), "unexpected error: {error}");

        // The error slot drains once (same as the old flush semantics); the
        // next read succeeds and sees the rolled-back (empty) store.
        let loaded = store
            .load_range(0, 100_000)
            .unwrap_or_else(|error| panic!("second read should succeed: {error}"));
        assert!(loaded.is_empty());

        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn retention_maintenance_successful_pruning_is_info() {
        let report = HistoryMaintenanceReport {
            pruned_rows: 42,
            checkpointed: true,
            vacuumed: true,
            elapsed_millis: 25,
            ..HistoryMaintenanceReport::default()
        };

        assert_eq!(
            history_retention_maintenance_level(&report, &test_retention_policy(), 0),
            DiagnosticsLevel::Info
        );
    }

    #[test]
    fn retention_maintenance_warns_for_operational_risk() {
        let mut policy = test_retention_policy();
        policy.aggressive_quarantine_rows = 4;
        let cancelled = HistoryMaintenanceReport {
            cancelled: true,
            ..HistoryMaintenanceReport::default()
        };
        let slow = HistoryMaintenanceReport {
            elapsed_millis: HISTORY_MAINTENANCE_WARN_MILLIS,
            ..HistoryMaintenanceReport::default()
        };
        let large_wal = HistoryMaintenanceReport {
            wal_bytes_before: HISTORY_MAINTENANCE_WAL_WARNING_BYTES,
            wal_bytes_after: HISTORY_MAINTENANCE_WAL_WARNING_BYTES,
            ..HistoryMaintenanceReport::default()
        };

        assert_eq!(
            history_retention_maintenance_level(&cancelled, &policy, 0),
            DiagnosticsLevel::Warn
        );
        assert_eq!(
            history_retention_maintenance_level(&slow, &policy, 0),
            DiagnosticsLevel::Warn
        );
        assert_eq!(
            history_retention_maintenance_level(&large_wal, &policy, 0),
            DiagnosticsLevel::Warn
        );
        assert_eq!(
            history_retention_maintenance_level(&HistoryMaintenanceReport::default(), &policy, 1),
            DiagnosticsLevel::Info
        );
        assert_eq!(
            history_retention_maintenance_level(
                &HistoryMaintenanceReport::default(),
                &policy,
                policy.aggressive_quarantine_rows
            ),
            DiagnosticsLevel::Warn
        );
    }

    #[test]
    fn quiet_retention_maintenance_noop_is_not_emitted() {
        let report = HistoryMaintenanceReport::default();
        let level = history_retention_maintenance_level(&report, &test_retention_policy(), 0);

        assert_eq!(level, DiagnosticsLevel::Info);
        assert!(!history_retention_maintenance_should_emit(&report, &level));
    }

    #[test]
    fn store_and_load_roundtrip() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        let snapshot = SystemSnapshot {
            sequence: 42,
            captured_at_millis: 1000,
            ..Default::default()
        };
        store.maybe_store(&snapshot);

        let loaded = store.load_range(0, 2000).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sequence, 42);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn write_interval_throttles() {
        let path = temp_db();
        let mut store = HistoryStore::open(&path, 3).unwrap();

        for i in 0..9 {
            store.maybe_store(&material_snapshot(i, i * 1000));
        }

        let loaded = store
            .load_range(0, 100_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(loaded.len(), 3); // writes at tick 3, 6, 9

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn unchanged_snapshots_are_coalesced() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        for i in 0..5 {
            store.maybe_store(&SystemSnapshot {
                sequence: i,
                captured_at_millis: (i + 1) * 1_000,
                ..Default::default()
            });
        }

        let loaded = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sequence, 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn material_snapshot_change_bypasses_coalescing() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        store.maybe_store(&material_snapshot(1, 1_000));
        store.maybe_store(&material_snapshot(2, 2_000));

        let loaded = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].sequence, 1);
        assert_eq!(loaded[1].sequence, 2);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn persisted_snapshots_are_compacted_before_serialization() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        let snapshot = bulky_snapshot(42, 1_000);
        let full_blob = bincode::serialize(&PersistedSnapshotEnvelope {
            version: SNAPSHOT_FORMAT_VERSION as u16,
            snapshot: snapshot.clone(),
        })
        .unwrap_or_else(|error| panic!("serialize full snapshot: {error}"));

        store.maybe_store(&snapshot);
        store
            .writer
            .flush()
            .unwrap_or_else(|error| panic!("flush writer: {error}"));

        let persisted_blob_len = store
            .conn
            .query_row("SELECT length(bincode_blob) FROM snapshots", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or_else(|error| panic!("read persisted blob length: {error}"))
            as usize;
        assert!(
            persisted_blob_len * 4 < full_blob.len(),
            "compacted blob was {persisted_blob_len} bytes vs {} full bytes",
            full_blob.len()
        );

        let loaded = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load compacted snapshot: {error}"));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].entities.len(), HISTORY_PERSISTED_ENTITY_LIMIT);
        assert_eq!(loaded[0].timeline.len(), HISTORY_PERSISTED_TIMELINE_LIMIT);
        assert!(loaded[0]
            .entities
            .iter()
            .all(|entity| entity.components.is_empty() && entity.network_connections.is_empty()));
        assert!(
            loaded[0]
                .entities
                .iter()
                .all(|entity| entity.trend.cpu_percent.len() <= HISTORY_PERSISTED_TREND_LIMIT)
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn max_coalesced_gap_preserves_sparse_baseline() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        store.maybe_store(&SystemSnapshot {
            sequence: 1,
            captured_at_millis: 1_000,
            ..Default::default()
        });
        store.maybe_store(&SystemSnapshot {
            sequence: 2,
            captured_at_millis: MAX_COALESCED_WRITE_GAP_MILLIS,
            ..Default::default()
        });
        store.maybe_store(&SystemSnapshot {
            sequence: 3,
            captured_at_millis: MAX_COALESCED_WRITE_GAP_MILLIS + 1_000,
            ..Default::default()
        });

        let loaded = store
            .load_range(0, MAX_COALESCED_WRITE_GAP_MILLIS + 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].sequence, 1);
        assert_eq!(loaded[1].sequence, 3);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn immediate_store_bypasses_write_interval() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 10).unwrap_or_else(|error| panic!("open store: {error}"));

        store.maybe_store(&SystemSnapshot {
            sequence: 1,
            captured_at_millis: 1_000,
            ..Default::default()
        });
        store.store_immediately(&SystemSnapshot {
            sequence: 2,
            captured_at_millis: 2_000,
            ..Default::default()
        });

        let loaded = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sequence, 2);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn prune_removes_old_data() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        for i in 0..5 {
            store.maybe_store(&material_snapshot(i, i * 1000));
        }

        let deleted = store.prune(3000).unwrap();
        assert_eq!(deleted, 3); // 0, 1000, 2000

        let remaining = store
            .load_range(0, 100_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(remaining.len(), 2); // 3000, 4000

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_range_reads_legacy_bincode_rows() {
        let path = temp_db();
        let store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        let snapshot = SystemSnapshot {
            sequence: 7,
            captured_at_millis: 7000,
            ..Default::default()
        };
        let legacy_blob = bincode::serialize(&snapshot).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO snapshots (captured_at_millis, sequence, format_version, bincode_blob) VALUES (?1, ?2, ?3, ?4)",
                params![7000_i64, 7_i64, 1_i64, legacy_blob],
            )
            .unwrap();

        let loaded = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sequence, 7);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn incompatible_rows_are_quarantined_once() {
        let path = temp_db();
        let store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        store
            .conn
            .execute(
                "INSERT INTO snapshots (captured_at_millis, sequence, format_version, bincode_blob) VALUES (?1, ?2, ?3, ?4)",
                params![9000_i64, 9_i64, 1_i64, vec![1_u8, 2, 3, 4]],
            )
            .unwrap();

        let first = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert!(first.is_empty());

        let quarantined = store
            .conn
            .query_row("SELECT COUNT(*) FROM snapshot_quarantine", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(quarantined, 1);

        let second = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert!(second.is_empty());

        let remaining = store
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(remaining, 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn clear_all_removes_snapshots_and_quarantine_rows() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        store.maybe_store(&SystemSnapshot {
            sequence: 1,
            captured_at_millis: 1_000,
            ..Default::default()
        });
        store
            .conn
            .execute(
                "INSERT INTO snapshot_quarantine (snapshot_id, captured_at_millis, sequence, format_version, quarantined_at_millis, reason, json_blob, bincode_blob)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    1_i64,
                    1_000_i64,
                    1_i64,
                    1_i64,
                    2_000_i64,
                    "test",
                    Option::<String>::None,
                    Option::<Vec<u8>>::None
                ],
            )
            .unwrap();

        store.clear_all().unwrap();

        let snapshot_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
            .unwrap();
        let quarantine_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM snapshot_quarantine", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(snapshot_count, 0);
        assert_eq!(quarantine_count, 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_range_page_returns_latest_then_older_samples() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        for i in 0..6 {
            store.maybe_store(&material_snapshot(i, (i + 1) * 1_000));
        }

        let latest = store.load_range_page(0, 10_000, None, 2).unwrap();
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].captured_at_millis, 5_000);
        assert_eq!(latest[1].captured_at_millis, 6_000);

        let older = store.load_range_page(0, 10_000, Some(5_000), 3).unwrap();
        assert_eq!(older.len(), 3);
        assert_eq!(older[0].captured_at_millis, 2_000);
        assert_eq!(older[2].captured_at_millis, 4_000);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn range_summary_reports_counts_and_bounds() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        for i in 0..4 {
            store.maybe_store(&material_snapshot(i, (i + 1) * 2_000));
        }

        let summary = store.range_summary(2_000, 6_000).unwrap();
        assert_eq!(summary.snapshot_count, 4);
        assert_eq!(summary.range_count, 3);
        assert_eq!(summary.oldest_millis, Some(2_000));
        assert_eq!(summary.newest_millis, Some(6_000));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn maintain_with_policy_prunes_old_history() {
        let path = temp_db();
        let mut store = match HistoryStore::open(&path, 1) {
            Ok(store) => store,
            Err(error) => panic!("open store: {error}"),
        };

        for i in 0..6 {
            store.maybe_store(&material_snapshot(i, i * 1_000));
        }

        let report = store
            .maintain_with_policy(HistoryRetentionPolicy {
                max_age_millis: 2_500,
                emergency_max_age_millis: 1_500,
                soft_max_store_bytes: u64::MAX,
                hard_max_store_bytes: u64::MAX,
                max_wal_bytes: u64::MAX,
                target_snapshot_count: u64::MAX,
                soft_max_snapshot_count: u64::MAX,
                hard_max_snapshot_count: u64::MAX,
                aggressive_quarantine_rows: u64::MAX,
                hard_max_quarantine_rows: u64::MAX,
            })
            .unwrap_or_else(|error| panic!("maintain_with_policy: {error}"));

        assert!(!report.cancelled);
        assert!(report.checkpointed);
        assert_eq!(report.pruned_rows, 3);
        let remaining = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        let sequences: Vec<u64> = remaining
            .into_iter()
            .map(|snapshot| snapshot.sequence)
            .collect();
        assert_eq!(sequences, vec![3, 4, 5]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn maintain_with_policy_skips_repeated_noop_maintenance() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        store.maybe_store(&material_snapshot(1, 1_000));
        store
            .maintain_storage(false)
            .unwrap_or_else(|error| panic!("pre-clean maintenance: {error}"));

        let report = store
            .maintain_with_policy(HistoryRetentionPolicy {
                max_age_millis: u64::MAX,
                emergency_max_age_millis: u64::MAX,
                soft_max_store_bytes: u64::MAX,
                hard_max_store_bytes: u64::MAX,
                max_wal_bytes: u64::MAX,
                target_snapshot_count: u64::MAX,
                soft_max_snapshot_count: u64::MAX,
                hard_max_snapshot_count: u64::MAX,
                aggressive_quarantine_rows: u64::MAX,
                hard_max_quarantine_rows: u64::MAX,
            })
            .unwrap_or_else(|error| panic!("maintain_with_policy: {error}"));

        assert!(!report.cancelled);
        assert!(!report.vacuumed);
        assert_eq!(report.pruned_rows, 0);
        assert!(report.aggressive_reason.is_none());
        assert!(report.wal_bytes_after <= report.wal_bytes_before);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn maintain_with_policy_checkpoints_nonempty_wal_without_prune() {
        let path = temp_db();
        let store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        store
            .conn
            .execute("CREATE TABLE wal_pressure(value INTEGER)", [])
            .unwrap_or_else(|error| panic!("create wal pressure table: {error}"));
        store
            .conn
            .execute("INSERT INTO wal_pressure(value) VALUES (1)", [])
            .unwrap_or_else(|error| panic!("insert wal pressure row: {error}"));
        let (_, wal_bytes_before) = store.store_file_sizes();
        assert!(wal_bytes_before > 0, "test setup did not create a WAL");

        let report = store
            .maintain_with_policy(HistoryRetentionPolicy {
                max_age_millis: u64::MAX,
                emergency_max_age_millis: u64::MAX,
                soft_max_store_bytes: u64::MAX,
                hard_max_store_bytes: u64::MAX,
                max_wal_bytes: u64::MAX,
                target_snapshot_count: u64::MAX,
                soft_max_snapshot_count: u64::MAX,
                hard_max_snapshot_count: u64::MAX,
                aggressive_quarantine_rows: u64::MAX,
                hard_max_quarantine_rows: u64::MAX,
            })
            .unwrap_or_else(|error| panic!("maintain_with_policy: {error}"));

        assert!(!report.cancelled);
        assert!(report.checkpointed);
        assert!(!report.vacuumed);
        assert_eq!(report.pruned_rows, 0);
        assert!(report.wal_bytes_after < wal_bytes_before);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn cancellable_maintenance_stops_before_sql_work() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));
        store.maybe_store(&SystemSnapshot {
            sequence: 1,
            captured_at_millis: 1_000,
            ..Default::default()
        });
        let cancel = Arc::new(AtomicBool::new(true));

        let report = store
            .maintain_with_policy_cancellable(
                HistoryRetentionPolicy {
                    max_age_millis: u64::MAX,
                    emergency_max_age_millis: u64::MAX,
                    soft_max_store_bytes: u64::MAX,
                    hard_max_store_bytes: u64::MAX,
                    max_wal_bytes: u64::MAX,
                    target_snapshot_count: u64::MAX,
                    soft_max_snapshot_count: u64::MAX,
                    hard_max_snapshot_count: u64::MAX,
                    aggressive_quarantine_rows: u64::MAX,
                    hard_max_quarantine_rows: u64::MAX,
                },
                cancel,
            )
            .unwrap_or_else(|error| panic!("maintain_with_policy_cancellable: {error}"));

        assert!(report.cancelled);
        assert!(!report.checkpointed);
        assert!(!report.vacuumed);
        assert_eq!(report.pruned_rows, 0);

        let remaining = store
            .load_range(0, 10_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(remaining.len(), 1);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn maintain_with_policy_trims_snapshot_count_and_quarantine_count() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        for i in 0..8 {
            store.maybe_store(&material_snapshot(i, (i + 1) * 1_000));
        }
        store
            .writer
            .flush()
            .unwrap_or_else(|error| panic!("flush: {error}"));
        for i in 0..6 {
            store
                .conn
                .execute(
                    "INSERT INTO snapshot_quarantine (snapshot_id, captured_at_millis, sequence, format_version, quarantined_at_millis, reason, json_blob, bincode_blob)
                     VALUES (NULL, ?1, ?2, 2, ?3, 'old-format', NULL, NULL)",
                    params![i as i64 * 1_000, i as i64, 10_000 + i as i64],
                )
                .unwrap_or_else(|error| panic!("insert quarantine: {error}"));
        }

        let report = store
            .maintain_with_policy(HistoryRetentionPolicy {
                max_age_millis: u64::MAX,
                emergency_max_age_millis: u64::MAX,
                soft_max_store_bytes: u64::MAX,
                hard_max_store_bytes: u64::MAX,
                max_wal_bytes: u64::MAX,
                target_snapshot_count: 3,
                soft_max_snapshot_count: 4,
                hard_max_snapshot_count: 5,
                aggressive_quarantine_rows: 4,
                hard_max_quarantine_rows: 3,
            })
            .unwrap_or_else(|error| panic!("maintain_with_policy: {error}"));

        assert!(report.pruned_rows >= 8);
        let remaining = store
            .load_range(0, 20_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(remaining.len(), 3);
        let remaining_quarantine = store
            .conn
            .query_row("SELECT COUNT(*) FROM snapshot_quarantine", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
            .unwrap_or_else(|error| panic!("count quarantine: {error}"));
        assert_eq!(remaining_quarantine, 3);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn maintain_with_policy_trims_under_byte_pressure_below_count_target() {
        let path = temp_db();
        let mut store =
            HistoryStore::open(&path, 1).unwrap_or_else(|error| panic!("open store: {error}"));

        for i in 0..130 {
            store.maybe_store(&material_snapshot(i, (i + 1) * 1_000));
            if i % 40 == 39 {
                store
                    .writer
                    .flush()
                    .unwrap_or_else(|error| panic!("flush: {error}"));
            }
        }

        let report = store
            .maintain_with_policy(HistoryRetentionPolicy {
                max_age_millis: u64::MAX,
                emergency_max_age_millis: u64::MAX,
                soft_max_store_bytes: 1,
                hard_max_store_bytes: u64::MAX,
                max_wal_bytes: u64::MAX,
                target_snapshot_count: 1_000,
                soft_max_snapshot_count: 1_000,
                hard_max_snapshot_count: 1_000,
                aggressive_quarantine_rows: u64::MAX,
                hard_max_quarantine_rows: u64::MAX,
            })
            .unwrap_or_else(|error| panic!("maintain_with_policy: {error}"));

        assert!(report.pruned_rows >= 10);
        assert!(
            report
                .aggressive_reason
                .as_deref()
                .unwrap_or_default()
                .contains("size-pressure-snapshot-cap")
        );
        let remaining = store
            .load_range(0, 200_000)
            .unwrap_or_else(|error| panic!("load_range: {error}"));
        assert_eq!(remaining.len(), MIN_SIZE_PRESSURE_SNAPSHOT_TARGET as usize);

        std::fs::remove_file(&path).ok();
    }
}
