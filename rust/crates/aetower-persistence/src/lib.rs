use std::path::Path;

use aetower_model::SystemSnapshot;
use rusqlite::{params, Connection};

pub struct HistoryStore {
    conn: Connection,
    write_counter: u32,
    write_interval: u32,
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
                json_blob TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_time
                ON snapshots(captured_at_millis);",
        )
        .map_err(|e| format!("schema: {e}"))?;

        Ok(Self {
            conn,
            write_counter: 0,
            write_interval: write_interval.max(1),
        })
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
        let json =
            serde_json::to_string(snapshot).map_err(|e| format!("serialize snapshot: {e}"))?;
        self.conn
            .execute(
                "INSERT INTO snapshots (captured_at_millis, sequence, json_blob) VALUES (?1, ?2, ?3)",
                params![snapshot.captured_at_millis as i64, snapshot.sequence as i64, json],
            )
            .map_err(|e| format!("insert: {e}"))?;
        Ok(())
    }

    /// Load snapshots in a time range (inclusive).
    pub fn load_range(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<Vec<SystemSnapshot>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT json_blob FROM snapshots
                 WHERE captured_at_millis >= ?1 AND captured_at_millis <= ?2
                 ORDER BY captured_at_millis ASC
                 LIMIT 500",
            )
            .map_err(|e| format!("prepare: {e}"))?;

        let rows = stmt
            .query_map(params![start_millis as i64, end_millis as i64], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(|e| format!("query: {e}"))?;

        let mut snapshots = Vec::new();
        for row in rows {
            let json = row.map_err(|e| format!("row: {e}"))?;
            let snapshot: SystemSnapshot =
                serde_json::from_str(&json).map_err(|e| format!("deserialize: {e}"))?;
            snapshots.push(snapshot);
        }
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
        Ok(deleted as u64)
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
