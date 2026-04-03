use std::path::Path;

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsStore, DiagnosticsSubsystem,
};
use aetower_model::SystemSnapshot;
use rusqlite::{Connection, params};

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
                json_blob TEXT,
                bincode_blob BLOB
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_time
                ON snapshots(captured_at_millis);",
        )
        .map_err(|e| format!("schema: {e}"))?;

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
        let blob = bincode::serialize(snapshot).map_err(|e| format!("serialize snapshot: {e}"))?;
        let result = self.conn
            .execute(
                "INSERT INTO snapshots (captured_at_millis, sequence, bincode_blob) VALUES (?1, ?2, ?3)",
                params![snapshot.captured_at_millis as i64, snapshot.sequence as i64, blob],
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
                "SELECT bincode_blob, json_blob FROM snapshots
                 WHERE captured_at_millis >= ?1 AND captured_at_millis <= ?2
                 ORDER BY captured_at_millis ASC
                 LIMIT 500",
            )
            .map_err(|e| format!("prepare: {e}"))
        {
            Ok(stmt) => stmt,
            Err(error) => {
                self.emit_load_event(start_millis, end_millis, 0, Some(&error));
                return Err(error);
            }
        };

        let rows = match stmt
            .query_map(params![start_millis as i64, end_millis as i64], |row| {
                let bincode_blob: Option<Vec<u8>> = row.get(0)?;
                let json_blob: Option<String> = row.get(1)?;
                Ok((bincode_blob, json_blob))
            })
            .map_err(|e| format!("query: {e}"))
        {
            Ok(rows) => rows,
            Err(error) => {
                self.emit_load_event(start_millis, end_millis, 0, Some(&error));
                return Err(error);
            }
        };

        let mut snapshots = Vec::new();
        for row in rows {
            let (bincode_blob, json_blob) = match row.map_err(|e| format!("row: {e}")) {
                Ok(row) => row,
                Err(error) => {
                    self.emit_load_event(start_millis, end_millis, 0, Some(&error));
                    return Err(error);
                }
            };
            let snapshot = if let Some(blob) = bincode_blob {
                match bincode::deserialize(&blob).map_err(|e| format!("bincode deserialize: {e}")) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        self.emit_load_event(start_millis, end_millis, 0, Some(&error));
                        return Err(error);
                    }
                }
            } else if let Some(json) = json_blob {
                match serde_json::from_str(&json).map_err(|e| format!("json deserialize: {e}")) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        self.emit_load_event(start_millis, end_millis, 0, Some(&error));
                        return Err(error);
                    }
                }
            } else {
                continue;
            };
            snapshots.push(snapshot);
        }
        self.emit_load_event(start_millis, end_millis, snapshots.len() as u64, None);
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
        error: Option<&str>,
    ) {
        let Some(diagnostics) = self.diagnostics.as_ref() else {
            return;
        };
        let mut builder = DiagnosticsEvent::builder(
            if error.is_some() {
                DiagnosticsLevel::Error
            } else {
                DiagnosticsLevel::Info
            },
            DiagnosticsSubsystem::Persistence,
            if error.is_some() {
                "history-load-failed"
            } else {
                "history-loaded"
            },
            if error.is_some() {
                "Failed to load persisted history range."
            } else {
                "Loaded persisted history range."
            },
        )
        .field("start_millis", start_millis)
        .field("end_millis", end_millis)
        .field("loaded_count", loaded_count);
        if let Some(error) = error {
            builder = builder.field("error", error);
        }
        diagnostics.emit(builder.build());
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
}
