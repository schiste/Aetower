//! Export-domain report builder for the MCP server.

use aetower_model::SystemSnapshot;
use serde_json::Value;

use crate::*;

pub(crate) fn build_export_query_response(
    data_source: &dyn AetowerMcpDataSource,
    mut snapshot: SystemSnapshot,
    options: ExportQueryOptions<'_>,
) -> Result<Value, String> {
    if !options.entity_ids.is_empty() {
        let ids = options.entity_ids.iter().cloned().collect::<BTreeSet<_>>();
        snapshot
            .entities
            .retain(|entity| ids.contains(&entity.entity_id));
        snapshot
            .timeline
            .retain(|event| match event.entity_id.as_ref() {
                Some(entity_id) => ids.contains(entity_id),
                None => true,
            });
    }

    let mut payload = serde_json::Map::new();
    payload.insert("privacyTier".to_owned(), json!(options.privacy_tier));
    payload.insert("startMillis".to_owned(), json!(options.start_millis));
    payload.insert("endMillis".to_owned(), json!(options.end_millis));

    if options.include_snapshot {
        payload.insert(
            "snapshot".to_owned(),
            export_controlled_json(
                serde_json::to_value(&snapshot).map_err(|error| error.to_string())?,
                options.privacy_tier,
            ),
        );
    }

    if options.include_history {
        let summary =
            data_source.history_range_summary(options.start_millis, options.end_millis)?;
        let page = data_source.load_history_page(
            options.start_millis,
            options.end_millis,
            None,
            options.history_limit,
        )?;
        payload.insert(
            "history".to_owned(),
            export_controlled_json(
                json!({
                    "summary": summary,
                    "snapshots": page,
                }),
                options.privacy_tier,
            ),
        );
    }

    if options.include_diagnostics {
        let events = data_source.query_diagnostics(DiagnosticsQuery {
            limit: options.diagnostics_limit,
            minimum_level: None,
            subsystem: None,
            search: None,
            since_millis: Some(options.start_millis),
            include_persisted: true,
        })?;
        payload.insert(
            "diagnostics".to_owned(),
            export_controlled_json(json!(events), options.privacy_tier),
        );
    }

    if options.include_session_health {
        let diagnostics = data_source.diagnostics_overview()?;
        let runtime = data_source.latest_runtime_lag_metrics()?;
        let history =
            data_source.history_range_summary(options.start_millis, options.end_millis)?;
        let history_events =
            data_source.query_diagnostics(history_diagnostics_query(options.start_millis, 64))?;
        let runtime_events = data_source.query_diagnostics(runtime_diagnostics_query(
            options.start_millis,
            DEFAULT_RUNTIME_BURST_EVENT_LIMIT,
        ))?;
        let checks = build_session_health_checks(
            &snapshot,
            &diagnostics,
            &runtime,
            &history,
            &history_events,
            &runtime_events,
        );
        payload.insert(
            "sessionHealth".to_owned(),
            export_controlled_json(
                serde_json::to_value(checks).map_err(|error| error.to_string())?,
                options.privacy_tier,
            ),
        );
    }

    if options.include_ai_runtime_report {
        let (history_page, history_warning) = load_ai_runtime_history(
            data_source,
            options.start_millis,
            options.end_millis,
            options.history_limit,
        );
        let report = build_ai_runtime_report(&snapshot, &history_page);
        payload.insert(
            "aiRuntimeReport".to_owned(),
            export_controlled_json(
                serde_json::to_value(json!({
                    "historyStatus": if history_warning.is_some() { "degraded" } else { "ok" },
                    "historyWarning": history_warning,
                    "summary": report.summary,
                    "burdenLeaders": report.burden_leaders,
                    "runtimeGroups": report.runtime_groups,
                    "approvals": report.approvals,
                    "delegations": report.delegations,
                    "recentChanges": report.recent_changes,
                    "historicalGroups": report.historical_groups,
                    "recommendations": report.recommendations,
                }))
                .map_err(|error| error.to_string())?,
                options.privacy_tier,
            ),
        );
    }

    Ok(Value::Object(payload))
}
