//! Runtime-domain MCP tool handlers.

use serde_json::Value;

use crate::*;

impl AetowerMcpServer {
    pub(crate) fn tool_watch_self(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_self_watch_duration_seconds")]
            duration_seconds: u64,
            #[serde(default = "default_self_watch_interval_millis")]
            interval_millis: u64,
            #[serde(default = "default_include_true")]
            include_memory_breakdown: bool,
            #[serde(default = "default_top_regions")]
            top_regions: usize,
        }

        let args: Args = parse_args(arguments)?;
        let duration_seconds = args
            .duration_seconds
            .clamp(1, MAX_SELF_WATCH_DURATION_SECONDS);
        let interval_millis = args.interval_millis.clamp(
            MIN_SELF_WATCH_INTERVAL_MILLIS,
            MAX_SELF_WATCH_INTERVAL_MILLIS,
        );
        let report = build_self_runtime_watch_report(
            &*self.data_source,
            duration_seconds,
            interval_millis,
            args.include_memory_breakdown,
            args.top_regions.max(1),
        )
        .map_err(tool_error)?;
        tool_json(report)
    }

    pub(crate) fn tool_runtime_burst_explanation(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_runtime_burst_window_minutes")]
            window_minutes: u64,
            #[serde(default = "default_runtime_burst_event_limit")]
            event_limit: usize,
        }

        let args: Args = parse_args(arguments)?;
        let window_minutes = args.window_minutes.clamp(1, 120);
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let runtime = self
            .data_source
            .latest_runtime_lag_metrics()
            .map_err(tool_error)?;
        let start_millis = snapshot
            .captured_at_millis
            .saturating_sub(window_minutes.saturating_mul(60 * 1000));
        let events = self
            .data_source
            .query_diagnostics(runtime_diagnostics_query(
                start_millis,
                args.event_limit
                    .clamp(1, DEFAULT_RUNTIME_BURST_EVENT_LIMIT * 4),
            ))
            .map_err(tool_error)?;
        tool_json(build_runtime_burst_explanation(
            snapshot.captured_at_millis,
            window_minutes,
            &runtime,
            &events,
        ))
    }

    pub(crate) fn tool_ai_runtime_report(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_history_window_hours")]
            history_window_hours: u64,
            #[serde(default = "default_export_history_limit")]
            history_limit: u32,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            history_status: &'static str,
            history_warning: Option<String>,
            summary: AiRuntimeSummary,
            burden_leaders: Vec<AiBurdenLeaderReport>,
            runtime_groups: Vec<AiRuntimeGroupReport>,
            approvals: Vec<AiApprovalReport>,
            delegations: Vec<AiDelegationReport>,
            recent_changes: Vec<RecentChangeItem>,
            historical_groups: Vec<AiHistoricalTrendReport>,
            recommendations: Vec<RecommendationItem>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let history_start = snapshot
            .captured_at_millis
            .saturating_sub(args.history_window_hours.saturating_mul(60 * 60 * 1000));
        let (history, history_warning) = load_ai_runtime_history(
            self.data_source.as_ref(),
            history_start,
            snapshot.captured_at_millis,
            args.history_limit,
        );
        let report = build_ai_runtime_report(&snapshot, &history);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            history_status: if history_warning.is_some() {
                "degraded"
            } else {
                "ok"
            },
            history_warning,
            summary: report.summary,
            burden_leaders: report.burden_leaders,
            runtime_groups: report.runtime_groups,
            approvals: report.approvals,
            delegations: report.delegations,
            recent_changes: report.recent_changes,
            historical_groups: report.historical_groups,
            recommendations: report.recommendations,
        })
    }

    pub(crate) fn tool_session_health(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_history_window_hours")]
            history_window_hours: u64,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            runtime_lag: RuntimeLagMetrics,
            overall: SeverityBand,
            checks: Vec<SessionHealthCheck>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let diagnostics = self
            .data_source
            .diagnostics_overview()
            .map_err(tool_error)?;
        let runtime = self
            .data_source
            .latest_runtime_lag_metrics()
            .map_err(tool_error)?;
        let window_start_millis = snapshot
            .captured_at_millis
            .saturating_sub(args.history_window_hours.saturating_mul(60 * 60 * 1000));
        let history = self
            .data_source
            .history_range_summary(window_start_millis, snapshot.captured_at_millis)
            .map_err(tool_error)?;
        let history_events = self
            .data_source
            .query_diagnostics(history_diagnostics_query(window_start_millis, 64))
            .map_err(tool_error)?;
        let runtime_events = self
            .data_source
            .query_diagnostics(runtime_diagnostics_query(
                window_start_millis,
                DEFAULT_RUNTIME_BURST_EVENT_LIMIT,
            ))
            .map_err(tool_error)?;
        let checks = build_session_health_checks(
            &snapshot,
            &diagnostics,
            &runtime,
            &history,
            &history_events,
            &runtime_events,
        );
        let overall = checks
            .iter()
            .map(|check| check.severity)
            .max_by_key(|severity| severity.score())
            .unwrap_or(SeverityBand::Info);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            runtime_lag: runtime,
            overall,
            checks,
        })
    }
}
