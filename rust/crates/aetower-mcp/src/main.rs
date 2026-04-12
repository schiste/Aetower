use std::sync::{Arc, Mutex};

use aetower_core::{Engine, RuntimeCollectionSettings};
use aetower_diagnostics::{DiagnosticsEvent, DiagnosticsOverview, DiagnosticsQuery};
use aetower_mcp::{
    AetowerMcpDataSource, HistorySummaryResponse, default_socket_path, proxy_stdio_to_socket,
    serve_stdio,
};
use aetower_model::{RuntimeLagMetrics, SystemSnapshot};

struct StandaloneEngineDataSource {
    engine: Mutex<Engine>,
}

impl StandaloneEngineDataSource {
    fn new() -> Self {
        let mut engine = Engine::new();
        engine.configure_runtime_collection(RuntimeCollectionSettings {
            full_collection: false,
            adaptive_cadence: true,
            active_tick_millis: 2_000,
            idle_tick_millis: 5_000,
            low_power_tick_millis: 8_000,
            gpu_sample_interval_millis: 30_000,
            gpu_sample_low_power_interval_millis: 60_000,
        });
        engine.start();
        Self {
            engine: Mutex::new(engine),
        }
    }
}

impl Drop for StandaloneEngineDataSource {
    fn drop(&mut self) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.stop();
        }
    }
}

impl AetowerMcpDataSource for StandaloneEngineDataSource {
    fn latest_snapshot(&self) -> Result<SystemSnapshot, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.latest_snapshot())
    }

    fn latest_snapshot_if_newer(
        &self,
        last_sequence: u64,
    ) -> Result<Option<SystemSnapshot>, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.latest_snapshot_if_newer(last_sequence))
    }

    fn latest_sequence(&self) -> Result<u64, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.latest_sequence())
    }

    fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.latest_runtime_lag_metrics())
    }

    fn history_range_summary(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<HistorySummaryResponse, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        engine
            .try_history_range_summary(start_millis, end_millis)
            .map(|summary| HistorySummaryResponse {
                store_bytes: summary.store_bytes,
                wal_bytes: summary.wal_bytes,
                snapshot_count: summary.snapshot_count,
                quarantine_count: summary.quarantine_count,
                range_count: summary.range_count,
                oldest_millis: summary.oldest_millis,
                newest_millis: summary.newest_millis,
                pending_writes: summary.pending_writes,
            })
    }

    fn load_history_page(
        &self,
        start_millis: u64,
        end_millis: u64,
        before_millis_exclusive: Option<u64>,
        limit: u32,
    ) -> Result<Vec<SystemSnapshot>, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        engine.try_load_history_page(start_millis, end_millis, before_millis_exclusive, limit)
    }

    fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.diagnostics_overview())
    }

    fn query_diagnostics(&self, query: DiagnosticsQuery) -> Result<Vec<DiagnosticsEvent>, String> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_owned())?;
        Ok(engine.query_diagnostics(query))
    }
}

fn main() -> Result<(), String> {
    let socket_path = match std::env::args().nth(1) {
        Some(path) => std::path::PathBuf::from(path),
        None => default_socket_path(),
    };

    match proxy_stdio_to_socket(&socket_path) {
        Ok(()) => Ok(()),
        Err(proxy_error) => {
            eprintln!(
                "aetower-mcp: proxy path unavailable ({}), falling back to direct stdio server",
                proxy_error
            );
            serve_stdio(Arc::new(StandaloneEngineDataSource::new()))
        }
    }
}
