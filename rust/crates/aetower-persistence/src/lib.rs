use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
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
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};

const SNAPSHOT_FORMAT_VERSION: i64 = 2;
const MAX_PENDING_HISTORY_WRITES: u64 = 64;
const MIN_SIZE_PRESSURE_SNAPSHOT_TARGET: u64 = 120;
const HISTORY_MAINTENANCE_WARN_MILLIS: u64 = 5_000;
const HISTORY_MAINTENANCE_WAL_WARNING_BYTES: u64 = 32 * 1024 * 1024;

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
}

struct HistoryWriter {
    command_tx: mpsc::Sender<HistoryCommand>,
    pending_writes: Arc<AtomicUsize>,
    handle: Option<JoinHandle<()>>,
}

enum HistoryCommand {
    Store(Box<SystemSnapshot>),
    Flush(mpsc::Sender<Result<(), String>>),
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
        configure_connection(&conn)?;
        let writer = HistoryWriter::spawn(path.to_path_buf())?;

        Ok(Self {
            conn,
            db_path: path.to_path_buf(),
            write_counter: 0,
            write_interval: write_interval.max(1),
            diagnostics: None,
            writer,
        })
    }

    pub fn set_diagnostics(&mut self, diagnostics: DiagnosticsStore) {
        self.diagnostics = Some(diagnostics);
    }

    /// Called on every engine tick. Only persists every `write_interval`th call.
    pub fn maybe_store(&mut self, snapshot: &SystemSnapshot) {
        self.write_counter += 1;
        if self.write_counter < self.write_interval {
            return;
        }
        self.write_counter = 0;
        let pending_writes = self.writer.pending_writes();
        if pending_writes >= MAX_PENDING_HISTORY_WRITES {
            if let Some(diagnostics) = self.diagnostics.as_ref() {
                diagnostics.emit(
                    DiagnosticsEvent::builder(
                        DiagnosticsLevel::Warn,
                        DiagnosticsSubsystem::Persistence,
                        "history-write-backpressure",
                        "Skipped a persisted history write because the writer queue is backed up.",
                    )
                    .field("pending_writes", pending_writes)
                    .build(),
                );
            }
            return;
        }
        let _ = self.writer.store(snapshot.clone());
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
        self.writer.flush()?;
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
        self.writer.flush()?;
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
        })
    }

    pub fn maintain_storage(&self, aggressive: bool) -> Result<HistoryMaintenanceReport, String> {
        self.maintain_storage_inner(aggressive, None)
    }

    pub fn maintain_storage_cancellable(
        &self,
        aggressive: bool,
        cancel: Arc<AtomicBool>,
    ) -> Result<HistoryMaintenanceReport, String> {
        self.maintain_storage_inner(aggressive, Some(&cancel))
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
                store_bytes_before,
                wal_bytes_before,
                aggressive_reason,
            ));
        }
        let checkpoint_cancelled = self.execute_batch_cancellable(
            "PRAGMA wal_checkpoint(TRUNCATE); PRAGMA optimize;",
            cancel,
            "checkpoint history store",
        )?;
        if checkpoint_cancelled {
            let (store_bytes_after, wal_bytes_after) = self.store_file_sizes();
            return Ok(HistoryMaintenanceReport {
                store_bytes_before,
                wal_bytes_before,
                store_bytes_after,
                wal_bytes_after,
                checkpointed: false,
                vacuumed: false,
                pruned_rows: 0,
                aggressive_reason,
                cancelled: true,
                elapsed_millis: started.elapsed().as_millis() as u64,
            });
        }
        let mut vacuumed = false;
        if aggressive && self.should_vacuum()? {
            if cancellation_requested(cancel) {
                let (store_bytes_after, wal_bytes_after) = self.store_file_sizes();
                return Ok(HistoryMaintenanceReport {
                    store_bytes_before,
                    wal_bytes_before,
                    store_bytes_after,
                    wal_bytes_after,
                    checkpointed: true,
                    vacuumed: false,
                    pruned_rows: 0,
                    aggressive_reason,
                    cancelled: true,
                    elapsed_millis: started.elapsed().as_millis() as u64,
                });
            }
            let vacuum_cancelled =
                self.execute_batch_cancellable("VACUUM;", cancel, "vacuum history store")?;
            if vacuum_cancelled {
                let (store_bytes_after, wal_bytes_after) = self.store_file_sizes();
                return Ok(HistoryMaintenanceReport {
                    store_bytes_before,
                    wal_bytes_before,
                    store_bytes_after,
                    wal_bytes_after,
                    checkpointed: true,
                    vacuumed: false,
                    pruned_rows: 0,
                    aggressive_reason,
                    cancelled: true,
                    elapsed_millis: started.elapsed().as_millis() as u64,
                });
            }
            vacuumed = true;
        }
        let (store_bytes_after, wal_bytes_after) = self.store_file_sizes();
        if let Some(diagnostics) = self.diagnostics.as_ref() {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Persistence,
                    "history-maintained",
                    "Ran persisted history maintenance.",
                )
                .field("aggressive", aggressive)
                .field("checkpointed", true)
                .field("vacuumed", vacuumed)
                .field("store_bytes_before", store_bytes_before)
                .field("wal_bytes_before", wal_bytes_before)
                .field("store_bytes_after", store_bytes_after)
                .field("wal_bytes_after", wal_bytes_after)
                .field("cancelled", false)
                .field("elapsed_millis", started.elapsed().as_millis())
                .build(),
            );
        }
        Ok(HistoryMaintenanceReport {
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
        })
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
                store_bytes_before,
                wal_bytes_before,
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
                    store_bytes_before,
                    wal_bytes_before,
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

        if let Some(diagnostics) = self.diagnostics.as_ref() {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    history_retention_maintenance_level(&report, &policy, quarantine_count),
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
        if page_count <= 0 {
            return Ok(false);
        }
        let (store_bytes, wal_bytes) = self.store_file_sizes();
        let fragmentation_ratio = freelist_count as f64 / page_count as f64;
        Ok(store_bytes >= 512 * 1024 * 1024
            && wal_bytes <= 64 * 1024 * 1024
            && fragmentation_ratio >= 0.25)
    }

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

fn cancelled_maintenance_report(
    started: Instant,
    store_bytes: u64,
    wal_bytes: u64,
    aggressive_reason: Option<String>,
) -> HistoryMaintenanceReport {
    HistoryMaintenanceReport {
        store_bytes_before: store_bytes,
        wal_bytes_before: wal_bytes,
        store_bytes_after: store_bytes,
        wal_bytes_after: wal_bytes,
        checkpointed: false,
        vacuumed: false,
        pruned_rows: 0,
        aggressive_reason,
        cancelled: true,
        elapsed_millis: started.elapsed().as_millis() as u64,
    }
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

    if report.cancelled || slow || quarantine_count > 0 || wal_unexpected {
        DiagnosticsLevel::Warn
    } else {
        DiagnosticsLevel::Info
    }
}

pub fn load_range_page_read_only(
    path: &Path,
    start_millis: u64,
    end_millis: u64,
    before_millis_exclusive: Option<u64>,
    limit: u32,
) -> Result<Vec<SystemSnapshot>, String> {
    let conn = open_read_only_connection(path)?;
    let mut stmt = conn
        .prepare(
            "SELECT captured_at_millis, sequence, format_version, bincode_blob, json_blob FROM snapshots
             WHERE captured_at_millis >= ?1
               AND captured_at_millis <= ?2
               AND (?3 IS NULL OR captured_at_millis < ?3)
             ORDER BY captured_at_millis DESC
             LIMIT ?4",
        )
        .map_err(|e| format!("prepare read-only history page: {e}"))?;

    let rows = stmt
        .query_map(
            params![
                start_millis as i64,
                end_millis as i64,
                before_millis_exclusive.map(|value| value as i64),
                limit.max(1) as i64
            ],
            |row| {
                let captured_at_millis: i64 = row.get(0)?;
                let sequence: i64 = row.get(1)?;
                let format_version: i64 = row.get(2)?;
                let bincode_blob: Option<Vec<u8>> = row.get(3)?;
                let json_blob: Option<String> = row.get(4)?;
                Ok((
                    captured_at_millis,
                    sequence,
                    format_version,
                    bincode_blob,
                    json_blob,
                ))
            },
        )
        .map_err(|e| format!("query read-only history page: {e}"))?;

    let mut snapshots = Vec::new();
    for row in rows {
        let (captured_at_millis, sequence, format_version, bincode_blob, json_blob) =
            row.map_err(|e| format!("read-only history row: {e}"))?;
        let snapshot = decode_snapshot_record(
            captured_at_millis as u64,
            sequence as u64,
            format_version,
            bincode_blob.as_deref(),
            json_blob.as_deref(),
        )?;
        snapshots.push(snapshot);
    }
    snapshots.reverse();
    Ok(snapshots)
}

pub fn range_summary_read_only(
    path: &Path,
    start_millis: u64,
    end_millis: u64,
) -> Result<HistoryRangeSummary, String> {
    let conn = open_read_only_connection(path)?;
    let (range_count, oldest_millis, newest_millis): (u64, Option<u64>, Option<u64>) = conn
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
        .map_err(|e| format!("read-only range summary: {e}"))?;
    let snapshot_count = conn
        .query_row("SELECT COUNT(*) FROM snapshots", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count as u64)
        .map_err(|e| format!("read-only snapshot count: {e}"))?;
    let quarantine_count = conn
        .query_row("SELECT COUNT(*) FROM snapshot_quarantine", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count as u64)
        .map_err(|e| format!("read-only quarantine count: {e}"))?;
    let store_bytes = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let wal_bytes = fs::metadata(PathBuf::from(format!("{}-wal", path.display())))
        .map(|meta| meta.len())
        .unwrap_or(0);

    Ok(HistoryRangeSummary {
        store_bytes,
        wal_bytes,
        snapshot_count,
        quarantine_count,
        range_count,
        oldest_millis,
        newest_millis,
        pending_writes: 0,
    })
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

impl HistoryWriter {
    fn spawn(path: PathBuf) -> Result<Self, String> {
        let (command_tx, command_rx) = mpsc::channel();
        let pending_writes = Arc::new(AtomicUsize::new(0));
        let pending_for_thread = Arc::clone(&pending_writes);
        let handle = thread::spawn(move || {
            let conn = match Connection::open(&path) {
                Ok(conn) => conn,
                Err(_) => return,
            };
            if configure_connection(&conn).is_err() {
                return;
            }

            while let Ok(command) = command_rx.recv() {
                match command {
                    HistoryCommand::Store(snapshot) => {
                        let _ = store_snapshot(&conn, &snapshot);
                        pending_for_thread.fetch_sub(1, Ordering::Relaxed);
                    }
                    HistoryCommand::Flush(reply) => {
                        let _ = reply.send(Ok(()));
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
                            .map(|_| ());
                        pending_for_thread.store(0, Ordering::Relaxed);
                        let _ = reply.send(result);
                    }
                    HistoryCommand::Shutdown => break,
                }
            }
        });

        Ok(Self {
            command_tx,
            pending_writes,
            handle: Some(handle),
        })
    }

    fn store(&self, snapshot: SystemSnapshot) -> Result<(), String> {
        self.pending_writes.fetch_add(1, Ordering::Relaxed);
        self.command_tx
            .send(HistoryCommand::Store(Box::new(snapshot)))
            .map_err(|error| format!("history writer queue: {error}"))
    }

    fn flush(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(HistoryCommand::Flush(tx))
            .map_err(|error| format!("history writer queue: {error}"))?;
        rx.recv()
            .map_err(|error| format!("history writer flush: {error}"))?
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

fn configure_connection(conn: &Connection) -> Result<(), String> {
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("WAL: {e}"))?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| format!("synchronous: {e}"))?;
    conn.pragma_update(None, "busy_timeout", 5_000)
        .map_err(|e| format!("busy_timeout: {e}"))?;

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
        CREATE INDEX IF NOT EXISTS idx_snapshots_time
            ON snapshots(captured_at_millis);",
    )
    .map_err(|e| format!("schema: {e}"))?;
    let _ = conn.execute(
        "ALTER TABLE snapshots ADD COLUMN format_version INTEGER NOT NULL DEFAULT 1",
        [],
    );
    Ok(())
}

fn open_read_only_connection(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("open read-only {}: {error}", path.display()))
}

fn decode_snapshot_record(
    captured_at_millis: u64,
    sequence: u64,
    format_version: i64,
    bincode_blob: Option<&[u8]>,
    json_blob: Option<&str>,
) -> Result<SystemSnapshot, String> {
    if let Some(blob) = bincode_blob {
        return decode_snapshot_blob(blob, format_version);
    }
    if let Some(json) = json_blob {
        return serde_json::from_str(json).map_err(|error| format!("json deserialize: {error}"));
    }
    Err(format!(
        "snapshot {} at {} has no persisted payload",
        sequence, captured_at_millis
    ))
}

fn decode_snapshot_blob(blob: &[u8], format_version: i64) -> Result<SystemSnapshot, String> {
    if format_version >= SNAPSHOT_FORMAT_VERSION {
        return bincode::deserialize::<PersistedSnapshotEnvelope>(blob)
            .map(|envelope| envelope.snapshot)
            .map_err(|error| format!("bincode envelope deserialize: {error}"));
    }

    bincode::deserialize(blob).map_err(|error| format!("bincode deserialize: {error}"))
}

fn store_snapshot(conn: &Connection, snapshot: &SystemSnapshot) -> Result<usize, String> {
    let envelope = PersistedSnapshotEnvelope {
        version: SNAPSHOT_FORMAT_VERSION as u16,
        snapshot: snapshot.clone(),
    };
    let blob = bincode::serialize(&envelope).map_err(|e| format!("serialize snapshot: {e}"))?;
    conn.execute(
        "INSERT INTO snapshots (captured_at_millis, sequence, format_version, bincode_blob) VALUES (?1, ?2, ?3, ?4)",
        params![
            snapshot.captured_at_millis as i64,
            snapshot.sequence as i64,
            SNAPSHOT_FORMAT_VERSION,
            blob
        ],
    )
    .map_err(|e| format!("insert: {e}"))
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
        let policy = test_retention_policy();
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
            DiagnosticsLevel::Warn
        );
    }

    #[test]
    fn store_and_load_roundtrip() {
        let path = temp_db();
        let mut store = HistoryStore::open(&path, 1).unwrap();

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
            store.maybe_store(&SystemSnapshot {
                sequence: i,
                captured_at_millis: i * 1000,
                ..Default::default()
            });
        }

        let loaded = store.load_range(0, 100_000).unwrap();
        assert_eq!(loaded.len(), 3); // writes at tick 3, 6, 9

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn prune_removes_old_data() {
        let path = temp_db();
        let mut store = HistoryStore::open(&path, 1).unwrap();

        for i in 0..5 {
            store.maybe_store(&SystemSnapshot {
                sequence: i,
                captured_at_millis: i * 1000,
                ..Default::default()
            });
        }

        let deleted = store.prune(3000).unwrap();
        assert_eq!(deleted, 3); // 0, 1000, 2000

        let remaining = store.load_range(0, 100_000).unwrap();
        assert_eq!(remaining.len(), 2); // 3000, 4000

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_range_reads_legacy_bincode_rows() {
        let path = temp_db();
        let store = HistoryStore::open(&path, 1).unwrap();
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

        let loaded = store.load_range(0, 10_000).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sequence, 7);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn incompatible_rows_are_quarantined_once() {
        let path = temp_db();
        let store = HistoryStore::open(&path, 1).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO snapshots (captured_at_millis, sequence, format_version, bincode_blob) VALUES (?1, ?2, ?3, ?4)",
                params![9000_i64, 9_i64, 1_i64, vec![1_u8, 2, 3, 4]],
            )
            .unwrap();

        let first = store.load_range(0, 10_000).unwrap();
        assert!(first.is_empty());

        let quarantined = store
            .conn
            .query_row("SELECT COUNT(*) FROM snapshot_quarantine", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(quarantined, 1);

        let second = store.load_range(0, 10_000).unwrap();
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
        let mut store = HistoryStore::open(&path, 1).unwrap();
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
        let mut store = HistoryStore::open(&path, 1).unwrap();

        for i in 0..6 {
            store.maybe_store(&SystemSnapshot {
                sequence: i,
                captured_at_millis: (i + 1) * 1_000,
                ..Default::default()
            });
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
        let mut store = HistoryStore::open(&path, 1).unwrap();

        for i in 0..4 {
            store.maybe_store(&SystemSnapshot {
                sequence: i,
                captured_at_millis: (i + 1) * 2_000,
                ..Default::default()
            });
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
            store.maybe_store(&SystemSnapshot {
                sequence: i,
                captured_at_millis: i * 1_000,
                ..Default::default()
            });
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
            store.maybe_store(&SystemSnapshot {
                sequence: i,
                captured_at_millis: (i + 1) * 1_000,
                ..Default::default()
            });
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
            store.maybe_store(&SystemSnapshot {
                sequence: i,
                captured_at_millis: (i + 1) * 1_000,
                ..Default::default()
            });
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
