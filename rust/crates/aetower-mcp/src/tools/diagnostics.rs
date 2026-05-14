//! Diagnostics-domain MCP tool handlers.

use serde_json::Value;

use crate::*;

impl AetowerMcpServer {
    pub(crate) fn tool_explain_anomalies(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            entity_ids: Vec<String>,
            #[serde(default = "default_findings_limit")]
            limit: usize,
            #[serde(default = "default_recent_changes_window_minutes")]
            window_minutes: u64,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            explanations: Vec<AnomalyExplanation>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let explanations = build_anomaly_explanations(
            &snapshot,
            &args.entity_ids,
            args.limit.max(1),
            args.window_minutes.saturating_mul(60 * 1000),
        );
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            explanations,
        })
    }

    pub(crate) fn tool_diagnostics_overview(&self) -> Result<Value, Value> {
        tool_json(
            self.data_source
                .diagnostics_overview()
                .map_err(tool_error)?,
        )
    }

    pub(crate) fn tool_diagnostics_summary(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            limit: Option<usize>,
            query_limit: Option<usize>,
            since_millis: Option<u64>,
            include_persisted: Option<bool>,
            minimum_level: Option<DiagnosticsLevel>,
            subsystem: Option<aetower_diagnostics::DiagnosticsSubsystem>,
            search: Option<String>,
        }

        let args: Args = parse_args(arguments)?;
        let limit = args.limit.unwrap_or(DEFAULT_DIAGNOSTICS_SUMMARY_LIMIT);
        let query_limit = args
            .query_limit
            .unwrap_or(DEFAULT_DIAGNOSTICS_SUMMARY_QUERY_LIMIT);
        let include_persisted = args.include_persisted.unwrap_or(true);
        let query = DiagnosticsQuery {
            limit: query_limit,
            minimum_level: args.minimum_level.clone(),
            subsystem: args.subsystem.clone(),
            search: args.search.clone(),
            since_millis: args.since_millis,
            include_persisted,
        };
        let overview = self
            .data_source
            .diagnostics_overview()
            .map_err(tool_error)?;
        let events = self
            .data_source
            .query_diagnostics(query)
            .map_err(tool_error)?;
        tool_json(build_diagnostics_summary_report(
            overview,
            events,
            DiagnosticsSummaryOptions {
                limit,
                since_millis: args.since_millis,
                include_persisted,
                minimum_level: args.minimum_level,
                subsystem: args.subsystem,
                search: args.search,
            },
        ))
    }

    pub(crate) fn tool_query_diagnostics(&self, arguments: Value) -> Result<Value, Value> {
        let query: DiagnosticsQuery = parse_args(arguments)?;
        tool_json(
            self.data_source
                .query_diagnostics(query)
                .map_err(tool_error)?,
        )
    }

    pub(crate) fn tool_support_bundle_manifest(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            privacy_tier: Option<ExportPrivacyTier>,
            #[serde(default = "default_support_bundle_diagnostics_limit")]
            diagnostics_limit: usize,
            #[serde(default = "default_history_window_hours")]
            history_window_hours: u64,
        }

        #[derive(Serialize)]
        struct Response {
            privacy_tier: ExportPrivacyTier,
            total_estimated_bytes: usize,
            sections: Vec<SupportBundleSectionManifest>,
            redaction_notes: Vec<String>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let diagnostics = self
            .data_source
            .diagnostics_overview()
            .map_err(tool_error)?;
        let runtime_lag = self
            .data_source
            .latest_runtime_lag_metrics()
            .map_err(tool_error)?;
        let history = self
            .data_source
            .history_range_summary(
                snapshot
                    .captured_at_millis
                    .saturating_sub(args.history_window_hours.saturating_mul(60 * 60 * 1000)),
                snapshot.captured_at_millis,
            )
            .map_err(tool_error)?;
        let diagnostic_events = self
            .data_source
            .query_diagnostics(DiagnosticsQuery {
                limit: args.diagnostics_limit,
                minimum_level: None,
                subsystem: None,
                search: None,
                since_millis: None,
                include_persisted: true,
            })
            .map_err(tool_error)?;
        let tier = args.privacy_tier.unwrap_or(ExportPrivacyTier::Redacted);
        let manifest = build_support_bundle_manifest(
            tier,
            snapshot,
            runtime_lag,
            diagnostics,
            history,
            diagnostic_events,
        )?;
        tool_json(Response {
            privacy_tier: tier,
            total_estimated_bytes: manifest.iter().map(|section| section.estimated_bytes).sum(),
            sections: manifest,
            redaction_notes: privacy_tier_notes(tier),
        })
    }

    pub(crate) fn tool_recommendations(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_findings_limit")]
            limit: usize,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            recommendations: Vec<RecommendationItem>,
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
        let history = self
            .data_source
            .history_range_summary(
                snapshot
                    .captured_at_millis
                    .saturating_sub(DEFAULT_HISTORY_WINDOW_MILLIS),
                snapshot.captured_at_millis,
            )
            .map_err(tool_error)?;
        let recommendations =
            build_recommendations(&snapshot, &diagnostics, &runtime, &history, args.limit);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            recommendations,
        })
    }
}
