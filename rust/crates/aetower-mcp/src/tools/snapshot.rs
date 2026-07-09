//! Snapshot-domain MCP tool handlers.
//!
//! Each method is an entry point dispatched by AetowerMcpServer::handle_message
//! for an `aetower_*` tool that reads from the live SystemSnapshot.

use serde_json::{Value, json};

use crate::*;

impl AetowerMcpServer {
    pub(crate) fn tool_current_snapshot(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize, Default)]
        struct Args {
            last_sequence: Option<u64>,
            entity_limit: Option<usize>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = if let Some(last_sequence) = args.last_sequence {
            match self
                .data_source
                .latest_snapshot_if_newer(last_sequence)
                .map_err(tool_error)?
            {
                Some(snapshot) => {
                    let summary = SnapshotEnvelope::updated(snapshot, args.entity_limit);
                    return tool_json(summary);
                }
                None => {
                    let warmed = self.wait_for_nonzero_snapshot()?;
                    if warmed.sequence > last_sequence {
                        return tool_json(SnapshotEnvelope::updated(warmed, args.entity_limit));
                    }
                    return tool_json(json!({
                        "updated": false,
                        "sequence": self.data_source.latest_sequence().map_err(tool_error)?,
                    }));
                }
            }
        } else {
            self.wait_for_nonzero_snapshot()?
        };

        tool_json(SnapshotEnvelope::updated(snapshot, args.entity_limit))
    }

    pub(crate) fn tool_host_summary(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize)]
        struct Args {
            #[serde(default = "default_top_entities")]
            top_entities: usize,
        }

        fn default_top_entities() -> usize {
            8
        }

        #[derive(Serialize)]
        struct EntitySummary {
            entity_id: String,
            display_name: String,
            friction: f32,
            cpu_percent: f32,
            memory_bytes: u64,
            badges: Vec<String>,
            recent_change_summary: Option<String>,
        }

        #[derive(Serialize)]
        struct HostSummary {
            sequence: u64,
            captured_at_millis: u64,
            host: aetower_model::HostSnapshot,
            capability_states: BTreeMap<String, String>,
            top_entities: Vec<EntitySummary>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let top_entities = snapshot
            .entities
            .iter()
            .take(args.top_entities.max(1))
            .map(|entity| EntitySummary {
                entity_id: entity.entity_id.clone(),
                display_name: entity.display_name.clone(),
                friction: entity.friction.total_score,
                cpu_percent: entity.metrics.cpu_percent,
                memory_bytes: entity.metrics.memory_resident_bytes,
                badges: entity.badges.clone(),
                recent_change_summary: entity.recent_change_summary.clone(),
            })
            .collect();
        let capability_states = snapshot
            .capabilities
            .iter()
            .map(|capability| {
                (
                    format!("{:?}", capability.kind),
                    format!("{:?}", capability.state),
                )
            })
            .collect();

        tool_json(HostSummary {
            sequence: snapshot.sequence,
            captured_at_millis: snapshot.captured_at_millis,
            host: snapshot.host,
            capability_states,
            top_entities,
        })
    }

    pub(crate) fn tool_resource_cost_rollups(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            scope: Option<String>,
            #[serde(default)]
            id: Option<String>,
            #[serde(default = "default_resource_cost_limit")]
            limit: usize,
        }

        #[derive(Serialize)]
        struct Response {
            sequence: u64,
            captured_at_millis: u64,
            total_rollups: usize,
            returned_rollups: usize,
            rollups: Vec<aetower_model::ResourceCostRollup>,
        }

        fn default_resource_cost_limit() -> usize {
            100
        }

        let args: Args = parse_args(arguments)?;
        let limit = args.limit.clamp(1, 200);
        let scope = args.scope.as_deref();
        let id = args.id.as_deref().filter(|value| !value.is_empty());
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let total_rollups = snapshot.resource_cost_rollups.len();
        let rollups = snapshot
            .resource_cost_rollups
            .into_iter()
            .filter(|rollup| {
                scope
                    .map(|scope| rollup.scope.as_str() == scope)
                    .unwrap_or(true)
            })
            .filter(|rollup| {
                id.map(|id| resource_cost_rollup_matches_id(rollup, id))
                    .unwrap_or(true)
            })
            .take(limit)
            .collect::<Vec<_>>();

        tool_json(Response {
            sequence: snapshot.sequence,
            captured_at_millis: snapshot.captured_at_millis,
            total_rollups,
            returned_rollups: rollups.len(),
            rollups,
        })
    }

    pub(crate) fn tool_entity_details(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(serde::Deserialize)]
        struct Args {
            entity_id: String,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let entity = snapshot
            .entities
            .into_iter()
            .find(|entity| entity.entity_id == args.entity_id)
            .ok_or_else(|| tool_error(format!("Unknown entity_id: {}", args.entity_id)))?;
        tool_json(entity)
    }

    pub(crate) fn tool_runtime_lag(&self) -> Result<Value, Value> {
        tool_json(
            self.data_source
                .latest_runtime_lag_metrics()
                .map_err(tool_error)?,
        )
    }

    pub(crate) fn tool_top_findings(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_findings_limit")]
            limit: usize,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            findings: Vec<TopFinding>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let diagnostics = self
            .data_source
            .diagnostics_overview()
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
        let findings = build_top_findings(&snapshot, &diagnostics, &history, args.limit);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            findings,
        })
    }

    pub(crate) fn tool_host_alerts(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_host_alert_top_entities")]
            top_entities: usize,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            alerts: Vec<HostAlert>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            alerts: build_host_alerts(&snapshot, args.top_entities),
        })
    }

    pub(crate) fn tool_investigation_bundle(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_investigation_window_minutes")]
            window_minutes: u64,
            #[serde(default)]
            start_millis: Option<u64>,
            #[serde(default)]
            end_millis: Option<u64>,
            #[serde(default)]
            entity_ids: Vec<String>,
            #[serde(default = "default_findings_limit")]
            findings_limit: usize,
            #[serde(default = "default_investigation_entity_limit")]
            entity_limit: usize,
            #[serde(default = "default_investigation_diagnostics_limit")]
            diagnostics_limit: usize,
            #[serde(default = "default_include_true")]
            include_process_trees: bool,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let requested_end_millis = args.end_millis.unwrap_or(snapshot.captured_at_millis);
        let requested_start_millis = args.start_millis.unwrap_or_else(|| {
            requested_end_millis
                .saturating_sub(args.window_minutes.max(1).saturating_mul(60 * 1000))
        });
        let window_start_millis = requested_start_millis.min(requested_end_millis);
        let window_end_millis = requested_start_millis.max(requested_end_millis);
        let window_minutes =
            ((window_end_millis.saturating_sub(window_start_millis)) / (60 * 1000)).max(1);
        let mut caveats = Vec::new();

        let diagnostics_overview = match self.data_source.diagnostics_overview() {
            Ok(overview) => overview,
            Err(error) => {
                caveats.push(format!("Diagnostics overview unavailable: {error}"));
                DiagnosticsOverview::default()
            }
        };
        let history_summary = match self
            .data_source
            .history_range_summary(window_start_millis, window_end_millis)
        {
            Ok(summary) => summary,
            Err(error) => {
                caveats.push(format!("History summary unavailable: {error}"));
                empty_history_summary()
            }
        };
        let mut diagnostics = match self.data_source.query_diagnostics(DiagnosticsQuery {
            limit: args.diagnostics_limit.max(1),
            minimum_level: None,
            subsystem: None,
            search: None,
            since_millis: Some(window_start_millis),
            include_persisted: true,
        }) {
            Ok(events) => events,
            Err(error) => {
                caveats.push(format!("Diagnostics events unavailable: {error}"));
                Vec::new()
            }
        };
        diagnostics.retain(|event| event.timestamp_millis <= window_end_millis);
        diagnostics.sort_by(|left, right| {
            right
                .timestamp_millis
                .cmp(&left.timestamp_millis)
                .then(left.id.cmp(&right.id))
        });
        diagnostics.truncate(args.diagnostics_limit.max(1));

        let host_alerts = build_host_alerts(&snapshot, DEFAULT_HOST_ALERT_TOP_ENTITIES);
        let top_findings = build_top_findings(
            &snapshot,
            &diagnostics_overview,
            &history_summary,
            args.findings_limit.max(1),
        );
        let entity_ids = select_investigation_entity_ids(
            &snapshot,
            &args.entity_ids,
            &host_alerts,
            args.entity_limit.max(1),
        );
        let recent_changes = build_recent_changes(
            &snapshot,
            window_end_millis.saturating_sub(window_start_millis),
            DEFAULT_RECENT_CHANGES_LIMIT,
        );

        let history_snapshots = match load_history_snapshots(
            &*self.data_source,
            window_start_millis,
            window_end_millis,
            DEFAULT_INVESTIGATION_HISTORY_LIMIT,
        ) {
            Ok(snapshots) => snapshots,
            Err(error) => {
                caveats.push(format!("Persisted history unavailable: {error}"));
                Vec::new()
            }
        };
        let history_diff = investigation_history_diff(
            &snapshot,
            &history_snapshots,
            &entity_ids,
            args.findings_limit.max(1),
            &diagnostics,
            &mut caveats,
        );

        let mut process_trees = Vec::new();
        if args.include_process_trees {
            for entity_id in &entity_ids {
                match build_process_tree_report(&snapshot, entity_id) {
                    Ok(report) => process_trees.push(report),
                    Err(error) => caveats.push(format!(
                        "Process tree unavailable for entity {entity_id}: {error}"
                    )),
                }
            }
        }

        tool_json(InvestigationBundleReport {
            captured_at_millis: snapshot.captured_at_millis,
            window_start_millis,
            window_end_millis,
            window_minutes,
            entity_ids,
            host_alerts,
            top_findings,
            recent_changes,
            diagnostics,
            history_diff,
            process_trees,
            caveats,
        })
    }

    pub(crate) fn tool_entity_group_tree(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            root_entity_id: String,
            grouped_entity_count: usize,
            grouped_process_count: u32,
            relations: Vec<String>,
            tree: GroupTreeNode,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let root = snapshot
            .entities
            .iter()
            .find(|entity| entity.entity_id == args.entity_id)
            .cloned()
            .ok_or_else(|| tool_error(format!("Unknown entity_id: {}", args.entity_id)))?;
        let (children, relations, grouped_process_count) = related_entity_nodes(&snapshot, &root);
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            root_entity_id: root.entity_id.clone(),
            grouped_entity_count: children.len() + 1,
            grouped_process_count,
            relations,
            tree: GroupTreeNode {
                entity_id: root.entity_id,
                display_name: root.display_name,
                relation: "selected-root".to_owned(),
                friction: root.friction.total_score,
                cpu_percent: root.metrics.cpu_percent,
                memory_bytes: root.metrics.memory_resident_bytes,
                process_count: root.metrics.process_count,
                badges: root.badges,
                recent_change_summary: root.recent_change_summary,
                children,
            },
        })
    }

    pub(crate) fn tool_recent_changes(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_recent_changes_window_minutes")]
            window_minutes: u64,
            #[serde(default = "default_recent_changes_limit")]
            limit: usize,
        }

        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            window_minutes: u64,
            changes: Vec<RecentChangeItem>,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let changes = build_recent_changes(
            &snapshot,
            args.window_minutes.saturating_mul(60 * 1000),
            args.limit.max(1),
        );
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            window_minutes: args.window_minutes,
            changes,
        })
    }

    pub(crate) fn tool_capability_status(&self) -> Result<Value, Value> {
        #[derive(Serialize)]
        struct Response {
            captured_at_millis: u64,
            counts: BTreeMap<String, usize>,
            capabilities: Vec<CapabilityStatusItem>,
        }

        let snapshot = self.wait_for_nonzero_snapshot()?;
        let capabilities = build_capability_status(&snapshot);
        let mut counts = BTreeMap::new();
        counts.insert(
            "critical".to_owned(),
            capabilities
                .iter()
                .filter(|capability| capability.severity == SeverityBand::Critical)
                .count(),
        );
        counts.insert(
            "warning".to_owned(),
            capabilities
                .iter()
                .filter(|capability| capability.severity == SeverityBand::Warning)
                .count(),
        );
        counts.insert(
            "ok".to_owned(),
            capabilities
                .iter()
                .filter(|capability| capability.severity == SeverityBand::Info)
                .count(),
        );
        tool_json(Response {
            captured_at_millis: snapshot.captured_at_millis,
            counts,
            capabilities,
        })
    }
}

fn resource_cost_rollup_matches_id(rollup: &aetower_model::ResourceCostRollup, id: &str) -> bool {
    rollup.id == id
        || rollup.entity_id.as_deref() == Some(id)
        || rollup.session_id.as_deref() == Some(id)
        || rollup.repository_path.as_deref() == Some(id)
}
