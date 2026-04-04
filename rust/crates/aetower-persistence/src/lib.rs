use std::path::Path;

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsStore, DiagnosticsSubsystem,
};
use aetower_model::SystemSnapshot;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const SNAPSHOT_FORMAT_VERSION: i64 = 2;

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

pub struct HistoryStore {
    conn: Connection,
    write_counter: u32,
    write_interval: u32,
    diagnostics: Option<DiagnosticsStore>,
}

impl HistoryStore {
    /// Open (or create) the history database at `path`.
    /// `write_interval` controls how many ticks between persisted snapshots
    /// (e.g. 5 = write every 5th tick).
    pub fn open(path: &Path, write_interval: u32) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("WAL: {e}"))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| format!("synchronous: {e}"))?;

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

        Ok(Self {
            conn,
            write_counter: 0,
            write_interval: write_interval.max(1),
            diagnostics: None,
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
        let _ = self.store(snapshot);
    }

    fn store(&self, snapshot: &SystemSnapshot) -> Result<(), String> {
        let envelope = PersistedSnapshotEnvelope {
            version: SNAPSHOT_FORMAT_VERSION as u16,
            snapshot: snapshot.clone(),
        };
        let blob = bincode::serialize(&envelope).map_err(|e| format!("serialize snapshot: {e}"))?;
        let result = self.conn
            .execute(
                "INSERT INTO snapshots (captured_at_millis, sequence, format_version, bincode_blob) VALUES (?1, ?2, ?3, ?4)",
                params![
                    snapshot.captured_at_millis as i64,
                    snapshot.sequence as i64,
                    SNAPSHOT_FORMAT_VERSION,
                    blob
                ],
            )
            .map_err(|e| format!("insert: {e}"));
        self.emit_store_event(snapshot, &result);
        result?;
        Ok(())
    }

    /// Load snapshots in a time range (inclusive).
    pub fn load_range(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<Vec<SystemSnapshot>, String> {
        let mut stmt = match self
            .conn
            .prepare(
                "SELECT id, captured_at_millis, sequence, format_version, bincode_blob, json_blob FROM snapshots
                 WHERE captured_at_millis >= ?1 AND captured_at_millis <= ?2
                 ORDER BY captured_at_millis ASC
                 LIMIT 500",
            )
            .map_err(|e| format!("prepare: {e}"))
        {
            Ok(stmt) => stmt,
            Err(error) => {
                self.emit_load_event(start_millis, end_millis, 0, 0, Some(&error));
                return Err(error);
            }
        };

        let rows = match stmt
            .query_map(params![start_millis as i64, end_millis as i64], |row| {
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
            })
            .map_err(|e| format!("query: {e}"))
        {
            Ok(rows) => rows,
            Err(error) => {
                self.emit_load_event(start_millis, end_millis, 0, 0, Some(&error));
                return Err(error);
            }
        };

        let mut snapshots = Vec::new();
        let mut quarantined_rows = 0u64;
        for row in rows {
            let (id, captured_at_millis, sequence, format_version, bincode_blob, json_blob) =
                match row.map_err(|e| format!("row: {e}")) {
                    Ok(row) => row,
                    Err(error) => {
                        self.emit_load_event(start_millis, end_millis, 0, 0, Some(&error));
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
                                Some(&quarantine_error),
                            );
                            return Err(quarantine_error);
                        }
                        quarantined_rows = quarantined_rows.saturating_add(1);
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
                                Some(&quarantine_error),
                            );
                            return Err(quarantine_error);
                        }
                        quarantined_rows = quarantined_rows.saturating_add(1);
                        continue;
                    }
                }
            } else {
                continue;
            };
            snapshots.push(snapshot);
        }
        self.emit_load_event(
            start_millis,
            end_millis,
            snapshots.len() as u64,
            quarantined_rows,
            None,
        );
        Ok(snapshots)
    }

    /// Delete snapshots older than `cutoff_millis`.
    pub fn prune(&self, cutoff_millis: u64) -> Result<u64, String> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM snapshots WHERE captured_at_millis < ?1",
                params![cutoff_millis as i64],
            )
            .map_err(|e| format!("prune: {e}"))?;
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
        Ok(deleted as u64)
    }

    pub fn clear_all(&self) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM snapshots", [])
            .map_err(|e| format!("clear snapshots: {e}"))?;
        self.conn
            .execute("DELETE FROM snapshot_quarantine", [])
            .map_err(|e| format!("clear quarantine: {e}"))?;
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

    fn emit_store_event(&self, snapshot: &SystemSnapshot, result: &Result<usize, String>) {
        let Some(diagnostics) = self.diagnostics.as_ref() else {
            return;
        };
        match result {
            Ok(rows) => diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Info,
                    DiagnosticsSubsystem::Persistence,
                    "history-persisted",
                    "Persisted snapshot into history store.",
                )
                .sequence(snapshot.sequence)
                .field("captured_at_millis", snapshot.captured_at_millis)
                .field("rows_written", rows)
                .build(),
            ),
            Err(error) => diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Error,
                    DiagnosticsSubsystem::Persistence,
                    "history-store-failed",
                    "Failed to persist snapshot into history store.",
                )
                .sequence(snapshot.sequence)
                .field("captured_at_millis", snapshot.captured_at_millis)
                .field("error", error)
                .build(),
            ),
        }
    }

    fn emit_load_event(
        &self,
        start_millis: u64,
        end_millis: u64,
        loaded_count: u64,
        quarantined_rows: u64,
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
        if let Some(error) = error {
            builder = builder.field("error", error);
        }
        diagnostics.emit(builder.build());
    }

    fn decode_snapshot(&self, blob: &[u8], format_version: i64) -> Result<SystemSnapshot, String> {
        if format_version >= SNAPSHOT_FORMAT_VERSION {
            return bincode::deserialize::<PersistedSnapshotEnvelope>(blob)
                .map(|envelope| envelope.snapshot)
                .map_err(|error| format!("bincode envelope deserialize: {error}"));
        }

        bincode::deserialize(blob).map_err(|error| format!("bincode deserialize: {error}"))
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

        if let Some(diagnostics) = self.diagnostics.as_ref() {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Warn,
                    DiagnosticsSubsystem::Persistence,
                    "history-row-quarantined",
                    "Quarantined an incompatible persisted history row.",
                )
                .field("snapshot_id", candidate.snapshot_id)
                .field("captured_at_millis", candidate.captured_at_millis)
                .field("sequence", candidate.sequence)
                .field("format_version", candidate.format_version)
                .field("reason", reason)
                .build(),
            );
        }

        Ok(())
    }
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
}
