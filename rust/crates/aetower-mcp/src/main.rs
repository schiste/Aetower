use std::path::PathBuf;
use std::sync::Arc;

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsOverview, DiagnosticsQuery, query_persisted,
};
use aetower_mcp::{
    AetowerMcpDataSource, HistorySummaryResponse, default_app_support_dir, default_cache_path,
    proxy_stdio_to_socket, read_local_cache, serve_stdio,
};
use aetower_model::{RuntimeLagMetrics, SystemSnapshot};
use aetower_persistence::{load_range_page_read_only, range_summary_read_only};

struct FileBackedDataSource {
    cache_path: PathBuf,
    history_db_path: PathBuf,
    diagnostics_path: PathBuf,
}

impl FileBackedDataSource {
    fn new() -> Self {
        let app_support_dir = default_app_support_dir();
        Self {
            cache_path: default_cache_path(),
            history_db_path: app_support_dir.join("history.db"),
            diagnostics_path: app_support_dir.join("diagnostics.ndjson"),
        }
    }
}

impl AetowerMcpDataSource for FileBackedDataSource {
    fn latest_snapshot(&self) -> Result<SystemSnapshot, String> {
        Ok(read_local_cache(&self.cache_path)?.snapshot)
    }

    fn latest_snapshot_if_newer(
        &self,
        last_sequence: u64,
    ) -> Result<Option<SystemSnapshot>, String> {
        let cache = read_local_cache(&self.cache_path)?;
        if cache.snapshot.sequence > last_sequence {
            Ok(Some(cache.snapshot))
        } else {
            Ok(None)
        }
    }

    fn latest_sequence(&self) -> Result<u64, String> {
        Ok(read_local_cache(&self.cache_path)?.snapshot.sequence)
    }

    fn latest_runtime_lag_metrics(&self) -> Result<RuntimeLagMetrics, String> {
        Ok(read_local_cache(&self.cache_path)?.runtime_lag)
    }

    fn history_range_summary(
        &self,
        start_millis: u64,
        end_millis: u64,
    ) -> Result<HistorySummaryResponse, String> {
        let summary = range_summary_read_only(&self.history_db_path, start_millis, end_millis)?;
        Ok(HistorySummaryResponse {
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
        load_range_page_read_only(
            &self.history_db_path,
            start_millis,
            end_millis,
            before_millis_exclusive,
            limit,
        )
    }

    fn diagnostics_overview(&self) -> Result<DiagnosticsOverview, String> {
        Ok(read_local_cache(&self.cache_path)?.diagnostics_overview)
    }

    fn query_diagnostics(&self, query: DiagnosticsQuery) -> Result<Vec<DiagnosticsEvent>, String> {
        if query.include_persisted {
            return query_persisted(&self.diagnostics_path, &query)
                .map_err(|error| format!("read persisted diagnostics: {error}"));
        }

        let cache = read_local_cache(&self.cache_path)?;
        let normalized_search = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());

        let mut events = cache
            .recent_diagnostics
            .into_iter()
            .filter(|event| diagnostics_event_matches(event, &query, normalized_search.as_deref()))
            .collect::<Vec<_>>();
        events.sort_by(|left, right| right.timestamp_millis.cmp(&left.timestamp_millis));
        events.truncate(query.limit.max(1));
        Ok(events)
    }
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

fn main() -> Result<(), String> {
    if let Some(path) = std::env::args().nth(1) {
        return proxy_stdio_to_socket(PathBuf::from(path));
    }

    serve_stdio(Arc::new(FileBackedDataSource::new()))
}
