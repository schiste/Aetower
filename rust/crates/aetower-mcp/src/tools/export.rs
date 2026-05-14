//! Export-domain MCP tool handlers.

use serde_json::Value;

use crate::*;

impl AetowerMcpServer {
    pub(crate) fn tool_export_query(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            privacy_tier: Option<ExportPrivacyTier>,
            #[serde(default)]
            entity_ids: Vec<String>,
            #[serde(default)]
            start_millis: Option<u64>,
            #[serde(default)]
            end_millis: Option<u64>,
            #[serde(default)]
            include_history: bool,
            #[serde(default = "default_include_true")]
            include_snapshot: bool,
            #[serde(default)]
            include_diagnostics: bool,
            #[serde(default)]
            include_session_health: bool,
            #[serde(default)]
            include_ai_runtime_report: bool,
            #[serde(default = "default_support_bundle_diagnostics_limit")]
            diagnostics_limit: usize,
            #[serde(default = "default_export_history_limit")]
            history_limit: u32,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let end_millis = args.end_millis.unwrap_or(snapshot.captured_at_millis);
        let start_millis = args
            .start_millis
            .unwrap_or(end_millis.saturating_sub(DEFAULT_HISTORY_WINDOW_MILLIS));
        let tier = args.privacy_tier.unwrap_or(ExportPrivacyTier::Redacted);
        let export = build_export_query_response(
            &*self.data_source,
            snapshot,
            ExportQueryOptions {
                privacy_tier: tier,
                entity_ids: &args.entity_ids,
                start_millis,
                end_millis,
                include_snapshot: args.include_snapshot,
                include_history: args.include_history,
                include_diagnostics: args.include_diagnostics,
                include_session_health: args.include_session_health,
                include_ai_runtime_report: args.include_ai_runtime_report,
                diagnostics_limit: args.diagnostics_limit,
                history_limit: args.history_limit,
            },
        )
        .map_err(tool_error)?;
        tool_json(export)
    }
}
