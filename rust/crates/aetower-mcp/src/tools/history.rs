//! History-domain MCP tool handlers.

use serde_json::Value;

use crate::*;

impl AetowerMcpServer {
    pub(crate) fn tool_diff_snapshots(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            before_millis: u64,
            after_millis: u64,
            #[serde(default)]
            entity_ids: Vec<String>,
            #[serde(default = "default_findings_limit")]
            limit: usize,
        }

        let args: Args = parse_args(arguments)?;
        let before = snapshot_at_or_before(&*self.data_source, args.before_millis)
            .map_err(tool_error)?
            .ok_or_else(|| {
                tool_error(format!(
                    "No persisted snapshot found at or before {}.",
                    args.before_millis
                ))
            })?;
        let after = snapshot_at_or_before(&*self.data_source, args.after_millis)
            .map_err(tool_error)?
            .ok_or_else(|| {
                tool_error(format!(
                    "No persisted snapshot found at or before {}.",
                    args.after_millis
                ))
            })?;
        tool_json(build_snapshot_diff_report(
            &before,
            &after,
            &args.entity_ids,
            args.limit.max(1),
        ))
    }

    pub(crate) fn tool_reboot_report(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            start_millis: Option<u64>,
            #[serde(default)]
            end_millis: Option<u64>,
        }

        let args: Args = parse_args(arguments)?;
        let latest = self.wait_for_nonzero_snapshot()?;
        let end_millis = args.end_millis.unwrap_or(latest.captured_at_millis);
        let start_millis = args
            .start_millis
            .unwrap_or_else(|| end_millis.saturating_sub(DEFAULT_HISTORY_WINDOW_MILLIS));
        tool_json(
            build_reboot_report(
                &*self.data_source,
                start_millis.min(end_millis),
                start_millis.max(end_millis),
            )
            .map_err(tool_error)?,
        )
    }

    pub(crate) fn tool_history_summary(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize)]
        struct Args {
            start_millis: u64,
            end_millis: u64,
        }

        let args: Args = parse_args(arguments)?;
        let summary = self
            .data_source
            .history_range_summary(args.start_millis, args.end_millis)
            .map_err(tool_error)?;
        tool_json(summary)
    }

    pub(crate) fn tool_history_page(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize)]
        struct Args {
            start_millis: u64,
            end_millis: u64,
            before_millis_exclusive: Option<u64>,
            limit: Option<u32>,
        }

        #[derive(Serialize)]
        struct HistoryPageResponse {
            snapshots: Vec<aetower_model::SystemSnapshot>,
            next_before_millis_exclusive: Option<u64>,
            returned: usize,
        }

        let args: Args = parse_args(arguments)?;
        let snapshots = self
            .data_source
            .load_history_page(
                args.start_millis,
                args.end_millis,
                args.before_millis_exclusive,
                args.limit.unwrap_or(120),
            )
            .map_err(tool_error)?;
        let next_before_millis_exclusive = older_page_cursor(&snapshots);
        tool_json(HistoryPageResponse {
            returned: snapshots.len(),
            snapshots,
            next_before_millis_exclusive,
        })
    }

    pub(crate) fn tool_history_store_health(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_history_window_hours")]
            window_hours: u64,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let start_millis = snapshot
            .captured_at_millis
            .saturating_sub(args.window_hours.saturating_mul(60 * 60 * 1000));
        let summary = self
            .data_source
            .history_range_summary(start_millis, snapshot.captured_at_millis)
            .map_err(tool_error)?;
        let recent_history_events = self
            .data_source
            .query_diagnostics(history_diagnostics_query(start_millis, 64))
            .map_err(tool_error)?;
        let data_quality = load_history_snapshots_raw(
            &*self.data_source,
            start_millis,
            snapshot.captured_at_millis,
            DEFAULT_HISTORY_QUALITY_MAX_SNAPSHOTS,
        )
        .ok()
        .map(|snapshots| {
            build_history_data_quality_report(
                &snapshots,
                start_millis,
                snapshot.captured_at_millis,
                DEFAULT_HISTORY_EXPECTED_INTERVAL_MILLIS,
            )
        });
        tool_json(build_history_store_health(
            summary,
            recent_history_events,
            data_quality,
        ))
    }

    pub(crate) fn tool_history_data_quality(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_history_window_hours")]
            window_hours: u64,
            #[serde(default = "default_history_expected_interval_millis")]
            expected_interval_millis: u64,
            #[serde(default = "default_history_quality_max_snapshots")]
            max_snapshots: usize,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let start_millis = snapshot
            .captured_at_millis
            .saturating_sub(args.window_hours.saturating_mul(60 * 60 * 1000));
        let snapshots = load_history_snapshots_raw(
            &*self.data_source,
            start_millis,
            snapshot.captured_at_millis,
            args.max_snapshots.max(1),
        )
        .map_err(tool_error)?;
        tool_json(build_history_data_quality_report(
            &snapshots,
            start_millis,
            snapshot.captured_at_millis,
            args.expected_interval_millis.max(1),
        ))
    }
}
