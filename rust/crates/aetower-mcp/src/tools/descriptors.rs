//! Typed MCP tool descriptors.
//!
//! The registry below is the single source for tool listing and dispatch. It
//! keeps docs, input schemas, defaults, validation bounds, and handler wiring in
//! one place so new tools cannot drift between `tools/list` and `tools/call`.

use std::sync::LazyLock;

use serde_json::{Value, json};

use crate::*;

type ToolResult = Result<Value, Value>;
type ArgsHandler = fn(&AetowerMcpServer, Value) -> ToolResult;
type NoArgsHandler = fn(&AetowerMcpServer) -> ToolResult;

enum ToolHandler {
    Args(ArgsHandler),
    NoArgs(NoArgsHandler),
}

pub(crate) struct ToolDescriptor {
    name: &'static str,
    description: &'static str,
    properties: Vec<ToolProperty>,
    handler: ToolHandler,
}

struct ToolProperty {
    name: &'static str,
    schema: Value,
    required: bool,
}

impl ToolProperty {
    fn required(mut self) -> Self {
        self.required = true;
        self
    }

    fn described(mut self, description: &'static str) -> Self {
        if let Value::Object(map) = &mut self.schema {
            map.insert(
                "description".to_owned(),
                Value::String(description.to_owned()),
            );
        }
        self
    }
}

impl ToolDescriptor {
    fn with_args(
        name: &'static str,
        description: &'static str,
        properties: Vec<ToolProperty>,
        handler: ArgsHandler,
    ) -> Self {
        Self {
            name,
            description,
            properties,
            handler: ToolHandler::Args(handler),
        }
    }

    fn no_args(name: &'static str, description: &'static str, handler: NoArgsHandler) -> Self {
        Self {
            name,
            description,
            properties: Vec::new(),
            handler: ToolHandler::NoArgs(handler),
        }
    }

    fn definition(&self) -> Value {
        let mut properties = serde_json::Map::new();
        let required = self
            .properties
            .iter()
            .filter(|property| property.required)
            .map(|property| Value::String(property.name.to_owned()))
            .collect::<Vec<_>>();
        for property in &self.properties {
            properties.insert(property.name.to_owned(), property.schema.clone());
        }

        let mut schema = serde_json::Map::new();
        schema.insert("type".to_owned(), Value::String("object".to_owned()));
        schema.insert("properties".to_owned(), Value::Object(properties));
        if !required.is_empty() {
            schema.insert("required".to_owned(), Value::Array(required));
        }
        schema.insert("additionalProperties".to_owned(), Value::Bool(false));

        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": Value::Object(schema),
        })
    }

    fn call(&self, server: &AetowerMcpServer, arguments: Value) -> ToolResult {
        match self.handler {
            ToolHandler::Args(handler) => handler(server, arguments),
            ToolHandler::NoArgs(handler) => handler(server),
        }
    }
}

#[cfg(test)]
pub(crate) fn tool_definitions() -> Vec<Value> {
    TOOL_DEFINITIONS.clone()
}

pub(crate) fn tool_list_result() -> Value {
    TOOL_LIST_RESULT.clone()
}

pub(crate) fn dispatch_tool(server: &AetowerMcpServer, name: &str, arguments: Value) -> ToolResult {
    TOOL_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.name == name)
        .map(|descriptor| descriptor.call(server, arguments))
        .unwrap_or_else(|| Ok(tool_error(format!("Unknown tool: {name}"))))
}

static TOOL_DEFINITIONS: LazyLock<Vec<Value>> = LazyLock::new(|| {
    TOOL_DESCRIPTORS
        .iter()
        .map(ToolDescriptor::definition)
        .collect()
});

static TOOL_LIST_RESULT: LazyLock<Value> =
    LazyLock::new(|| json!({ "tools": TOOL_DEFINITIONS.clone() }));

static TOOL_DESCRIPTORS: LazyLock<Vec<ToolDescriptor>> = LazyLock::new(|| {
    vec![
        ToolDescriptor::with_args(
            "aetower_current_snapshot",
            "Return the latest live Aetower snapshot. Optionally skip output unless the sequence advanced.",
            vec![uint("last_sequence", Some(0), None, None), uint("entity_limit", Some(1), None, None)],
            AetowerMcpServer::tool_current_snapshot,
        ),
        ToolDescriptor::with_args(
            "aetower_host_summary",
            "Return a concise host summary plus the top friction entities.",
            vec![uint("top_entities", Some(1), Some(50), None)],
            AetowerMcpServer::tool_host_summary,
        ),
        ToolDescriptor::with_args(
            "aetower_entity_details",
            "Return the full entity snapshot for one entity_id.",
            vec![string("entity_id").required()],
            AetowerMcpServer::tool_entity_details,
        ),
        ToolDescriptor::no_args(
            "aetower_runtime_lag",
            "Return the latest self-observability and runtime lag metrics for Aetower itself.",
            AetowerMcpServer::tool_runtime_lag,
        ),
        ToolDescriptor::with_args(
            "aetower_watch_self",
            "Run a bounded live watch of Aetower's own runtime overhead, memory peak, MCP pressure, UI latency, and optional self vmmap attribution.",
            vec![
                uint("duration_seconds", Some(1), Some(MAX_SELF_WATCH_DURATION_SECONDS), Some(DEFAULT_SELF_WATCH_DURATION_SECONDS)),
                uint("interval_millis", Some(MIN_SELF_WATCH_INTERVAL_MILLIS), Some(MAX_SELF_WATCH_INTERVAL_MILLIS), Some(DEFAULT_SELF_WATCH_INTERVAL_MILLIS)),
                boolean("include_memory_breakdown", Some(false)),
                uint("top_regions", Some(1), Some(50), None),
            ],
            AetowerMcpServer::tool_watch_self,
        ),
        ToolDescriptor::with_args(
            "aetower_runtime_burst_explanation",
            "Explain current Aetower observer overhead by correlating runtime lag, self CPU/wakeups, UI render latency, MCP request pressure, history work, and recent adapter diagnostics.",
            vec![
                uint("window_minutes", Some(1), Some(120), Some(DEFAULT_RUNTIME_BURST_WINDOW_MINUTES)),
                uint("event_limit", Some(1), Some(256), Some(DEFAULT_RUNTIME_BURST_EVENT_LIMIT as u64)),
            ],
            AetowerMcpServer::tool_runtime_burst_explanation,
        ),
        ToolDescriptor::with_args(
            "aetower_diff_snapshots",
            "Compare two persisted time points and return host plus per-entity deltas for friction, CPU, memory, wakeups, and process count, including boot-boundary metadata when the snapshots span a reboot.",
            vec![
                uint("before_millis", Some(0), None, None).required(),
                uint("after_millis", Some(0), None, None).required(),
                string_array("entity_ids", Some(100)),
                uint("limit", Some(1), Some(100), None),
            ],
            AetowerMcpServer::tool_diff_snapshots,
        ),
        ToolDescriptor::with_args(
            "aetower_reboot_report",
            "Summarize detected boot-session boundaries, pre-reboot pressure, and correlated sleep/wake/panic markers for a time range.",
            vec![uint("start_millis", Some(0), None, None), uint("end_millis", Some(0), None, None)],
            AetowerMcpServer::tool_reboot_report,
        ),
        ToolDescriptor::with_args(
            "aetower_explain_anomalies",
            "Explain current anomalous entities by highlighting the dominant changed metrics and recent supporting events.",
            vec![
                string_array("entity_ids", Some(100)),
                uint("limit", Some(1), Some(100), None),
                uint("window_minutes", Some(1), Some(1440), None),
            ],
            AetowerMcpServer::tool_explain_anomalies,
        ),
        ToolDescriptor::with_args(
            "aetower_entity_process_tree",
            "Return a per-process tree for one entity with subtree burden, grouping scope, and expansion reasons.",
            vec![string("entity_id").required()],
            AetowerMcpServer::tool_entity_process_tree,
        ),
        ToolDescriptor::with_args(
            "aetower_top_findings",
            "Return the highest-signal current findings across host load, diagnostics, history health, and top friction groups.",
            vec![uint("limit", Some(1), Some(50), None)],
            AetowerMcpServer::tool_top_findings,
        ),
        ToolDescriptor::with_args(
            "aetower_host_alerts",
            "Return current host alerts such as memory pressure and wakeup storms with impacted entity IDs.",
            vec![uint("top_entities", Some(1), Some(20), None)],
            AetowerMcpServer::tool_host_alerts,
        ),
        ToolDescriptor::with_args(
            "aetower_investigation_bundle",
            "Return a focused crash/freeze investigation bundle with current pressure, recent changes, diagnostics, history diff, and optional process trees.",
            vec![
                uint("window_minutes", Some(1), Some(1440), None),
                uint("start_millis", Some(0), None, None),
                uint("end_millis", Some(0), None, None),
                string_array("entity_ids", Some(100)),
                uint("findings_limit", Some(1), Some(100), None),
                uint("entity_limit", Some(1), Some(25), None),
                uint("diagnostics_limit", Some(1), Some(5000), None),
                boolean("include_process_trees", None),
            ],
            AetowerMcpServer::tool_investigation_bundle,
        ),
        ToolDescriptor::with_args(
            "aetower_entity_group_tree",
            "Return a grouped entity family view for one entity_id using runtime/session/repo relationships.",
            vec![string("entity_id").required()],
            AetowerMcpServer::tool_entity_group_tree,
        ),
        ToolDescriptor::with_args(
            "aetower_ai_runtime_report",
            "Return grouped AI runtime insights including burden leaders, approval queue, delegated sessions, recent changes, and recent persisted history trends.",
            vec![
                uint("history_window_hours", Some(1), Some(168), None),
                uint("history_limit", Some(1), Some(500), None),
            ],
            AetowerMcpServer::tool_ai_runtime_report,
        ),
        ToolDescriptor::with_args(
            "aetower_recent_changes",
            "Return a concise feed of recent timeline changes and entity change summaries.",
            vec![uint("window_minutes", Some(1), Some(1440), None), uint("limit", Some(1), Some(200), None)],
            AetowerMcpServer::tool_recent_changes,
        ),
        ToolDescriptor::no_args(
            "aetower_capability_status",
            "Return operator-grade capability state, health, and next-action labels for permissions and adapters.",
            AetowerMcpServer::tool_capability_status,
        ),
        ToolDescriptor::with_args(
            "aetower_history_summary",
            "Return persisted history coverage and store size information for a time range.",
            vec![uint("start_millis", Some(0), None, None).required(), uint("end_millis", Some(0), None, None).required()],
            AetowerMcpServer::tool_history_summary,
        ),
        ToolDescriptor::with_args(
            "aetower_history_page",
            "Return a bounded page of persisted snapshots for a time range, newest first.",
            vec![
                uint("start_millis", Some(0), None, None).required(),
                uint("end_millis", Some(0), None, None).required(),
                uint("before_millis_exclusive", Some(0), None, None),
                uint("limit", Some(1), Some(500), None),
            ],
            AetowerMcpServer::tool_history_page,
        ),
        ToolDescriptor::with_args(
            "aetower_history_store_health",
            "Return persisted history store health, thresholds, and recent history-related diagnostics.",
            vec![uint("window_hours", Some(1), Some(720), None)],
            AetowerMcpServer::tool_history_store_health,
        ),
        ToolDescriptor::with_args(
            "aetower_history_data_quality",
            "Analyze persisted snapshot ordering and coverage for gaps, duplicate timestamps, sequence regressions, and boot boundaries.",
            vec![
                uint("window_hours", Some(1), Some(720), None),
                uint("expected_interval_millis", Some(1), Some(3_600_000), None),
                uint("max_snapshots", Some(1), Some(20_000), None),
            ],
            AetowerMcpServer::tool_history_data_quality,
        ),
        ToolDescriptor::with_args(
            "aetower_repository_inventory",
            "Discover local Git repository roots with a cheap inventory-only scan. Skips heavy artifact directories and returns per-root coverage without sizing files.",
            vec![
                string_array("roots", Some(24)).described("Optional absolute paths or ~/ paths. Defaults to common developer repository roots."),
                uint("max_depth", Some(1), Some(12), Some(5)),
            ],
            AetowerMcpServer::tool_repository_inventory,
        ),
        ToolDescriptor::with_args(
            "aetower_repository_scorecard",
            "Run an explicit OpenSSF Scorecard repository readiness scan for one GitHub repository root. Uses cached results unless refresh is true; never runs during default repository discovery.",
            vec![
                string("repo_root").required().described("Absolute path to the local Git repository root."),
                string_enum("mode", &["auto", "public_api", "live_cli"]).described("Scorecard source mode. auto tries the public API, then falls back to the local scorecard CLI."),
                uint("timeout_seconds", Some(1), Some(120), Some(30)),
                boolean("refresh", Some(false)).described("Bypass the local Scorecard cache and run the selected source again."),
            ],
            AetowerMcpServer::tool_repository_scorecard,
        ),
        ToolDescriptor::with_args(
            "aetower_storage_hygiene",
            "Scan bounded local developer storage roots for build artifacts, logs, caches, and dependency trees. Read-only: reports size, age, safety tier, caveats, and review guidance without deleting anything.",
            vec![
                string_array("roots", Some(24)).described("Optional absolute paths or ~/ paths. Defaults to common developer and Xcode cache locations."),
                uint("max_depth", Some(1), Some(12), Some(5)),
                uint("limit", Some(1), Some(200), Some(80)),
                string("mode").described("Scan mode: instant_cached, fast_changed_only, deep_native, or forensic_verified. Defaults to fast_changed_only."),
            ],
            AetowerMcpServer::tool_storage_hygiene,
        ),
        ToolDescriptor::with_args(
            "aetower_storage_hygiene_overview",
            "Return a compact storage overview: summary, top findings, guardrails, top repo footprints, and scan diagnostics.",
            vec![
                string_array("roots", Some(24)).described("Optional absolute paths or ~/ paths."),
                uint("max_depth", Some(1), Some(12), Some(5)),
                string("mode").described("Scan mode. Defaults to fast_changed_only."),
            ],
            AetowerMcpServer::tool_storage_hygiene_overview,
        ),
        ToolDescriptor::with_args(
            "aetower_storage_hygiene_actions",
            "Return cleanup tiers, recipes, bundles, guardrails, and diagnostics without the raw artifact list.",
            vec![
                string_array("roots", Some(24)).described("Optional absolute paths or ~/ paths."),
                uint("max_depth", Some(1), Some(12), Some(5)),
                uint("limit", Some(1), Some(200), Some(80)),
                string("mode").described("Scan mode. Defaults to fast_changed_only."),
            ],
            AetowerMcpServer::tool_storage_hygiene_actions,
        ),
        ToolDescriptor::with_args(
            "aetower_storage_hygiene_items_page",
            "Return one page of ranked storage items plus diagnostics. Pages in instant_cached mode are served directly from the persistent index at full depth.",
            vec![
                string_array("roots", Some(24)).described("Optional absolute paths or ~/ paths."),
                uint("max_depth", Some(1), Some(12), Some(5)),
                uint("offset", Some(0), Some(1_000_000), Some(0)),
                uint("limit", Some(1), Some(10_000), Some(80)),
                string("mode").described("Scan mode. Defaults to fast_changed_only."),
                string_enum("sort_key", &["size", "path", "modified", "accessed", "tier", "kind"])
                    .described("Server-side sort key for the returned page. Defaults to size."),
                boolean("sort_descending", Some(true))
                    .described("Sort descending when true. Defaults to true for largest-first pages."),
            ],
            AetowerMcpServer::tool_storage_hygiene_items_page,
        ),
        ToolDescriptor::with_args(
            "aetower_storage_hygiene_repo_detail",
            "Return storage detail for one repository root: repo intelligence, top artifacts, cleanup actions, and diagnostics.",
            vec![
                string("repo_root").required(),
                string("mode").described("Scan mode. Defaults to fast_changed_only."),
            ],
            AetowerMcpServer::tool_storage_hygiene_repo_detail,
        ),
        ToolDescriptor::with_args(
            "aetower_storage_hygiene_deep_scan",
            "Run an explicit deep storage scan using the deep_native mode. Heavier than the default fast scan.",
            vec![
                string_array("roots", Some(24)).described("Optional absolute paths or ~/ paths."),
                uint("max_depth", Some(1), Some(12), Some(8)),
                uint("limit", Some(1), Some(200), Some(120)),
            ],
            AetowerMcpServer::tool_storage_hygiene_deep_scan,
        ),
        ToolDescriptor::with_args(
            "aetower_memory_breakdown",
            "Ask the running Aetower app to collect a vmmap-style memory region breakdown for one entity.",
            vec![string("entity_id").required(), uint("top_regions", Some(1), Some(50), None)],
            AetowerMcpServer::tool_memory_breakdown,
        ),
        ToolDescriptor::with_args(
            "aetower_profile_entity",
            "Ask the running Aetower app to run a short sampled profile for one entity and summarize hot threads, queues, and stacks.",
            vec![
                string("entity_id").required(),
                uint("duration_seconds", Some(1), Some(MAX_PROFILE_DURATION_SECONDS), None),
                uint("top_stacks", Some(1), Some(20), None),
            ],
            AetowerMcpServer::tool_profile_entity,
        ),
        ToolDescriptor::with_args(
            "aetower_wakeup_attribution",
            "Ask the running Aetower app to sample one entity and return heuristic wakeup attribution by thread, queue, and dominant sampled cause.",
            vec![
                string("entity_id").required(),
                uint("duration_seconds", Some(1), Some(MAX_PROFILE_DURATION_SECONDS), None),
                uint("top_stacks", Some(1), Some(20), None),
            ],
            AetowerMcpServer::tool_wakeup_attribution,
        ),
        ToolDescriptor::with_args(
            "aetower_process_inspect",
            "Inspect one running process by PID with current attribution, ps state, children, and safety notes.",
            vec![uint("pid", Some(2), None, None).required()],
            AetowerMcpServer::tool_process_inspect,
        ),
        ToolDescriptor::with_args(
            "aetower_process_open_resources",
            "List open files and sockets for one process using lsof with a bounded result limit.",
            vec![uint("pid", Some(2), None, None).required(), uint("limit", Some(1), Some(500), None)],
            AetowerMcpServer::tool_process_open_resources,
        ),
        ToolDescriptor::with_args(
            "aetower_process_sample",
            "Run a short bounded sample for one process and summarize the hottest sampled threads.",
            vec![
                uint("pid", Some(2), None, None).required(),
                uint("duration_seconds", Some(1), Some(MAX_PROFILE_DURATION_SECONDS), None),
                uint("top_stacks", Some(1), Some(20), None),
            ],
            AetowerMcpServer::tool_process_sample,
        ),
        ToolDescriptor::with_args(
            "aetower_process_action",
            "Preview or execute a guarded process action. Defaults to dry_run=true. Execution requires explicit operator approval plus expected_targets copied from the dry-run preview so Aetower can reject PID reuse or target-set drift. Executed reports include command_result, target_outcomes, and a bounded verification status so callers can tell whether macOS accepted the command and whether the target post-condition was confirmed.",
            vec![
                uint("pid", Some(2), None, None).required(),
                string_enum("action", &["terminate", "force-kill", "suspend", "resume", "terminate-tree", "force-kill-tree", "lower-priority", "normal-priority"]).required(),
                boolean("dry_run", None),
                string("reason"),
                string("action_id").described("Optional caller-provided correlation id. Reuse the preview action_id when executing."),
                process_expected_targets(),
                int("restore_nice_value", Some(-20), Some(20), None).described("For normal-priority, restore this exact nice value instead of assuming 0."),
                boolean("privileged_helper_approved", None).described("Explicit operator approval for an elevated retry. Current public builds report approved-but-unavailable rather than silently escalating."),
            ],
            AetowerMcpServer::tool_process_action,
        ),
        ToolDescriptor::with_args(
            "aetower_process_action_history",
            "Return recent operator process actions recorded by Aetower diagnostics.",
            vec![uint("window_minutes", Some(1), Some(10_080), None), uint("limit", Some(1), Some(200), None)],
            AetowerMcpServer::tool_process_action_history,
        ),
        ToolDescriptor::no_args(
            "aetower_diagnostics_overview",
            "Return diagnostics ring and persisted diagnostics health.",
            AetowerMcpServer::tool_diagnostics_overview,
        ),
        ToolDescriptor::with_args(
            "aetower_diagnostics_summary",
            "Return diagnostics grouped by subsystem, event type, and level with counts, latest samples, and noise-reduction recommendations.",
            diagnostics_properties(true),
            AetowerMcpServer::tool_diagnostics_summary,
        ),
        ToolDescriptor::with_args(
            "aetower_query_diagnostics",
            "Query recent or persisted diagnostics by level, subsystem, text, and time window.",
            diagnostics_properties(false),
            AetowerMcpServer::tool_query_diagnostics,
        ),
        ToolDescriptor::with_args(
            "aetower_support_bundle_manifest",
            "Return a machine-readable preview of what an Aetower support bundle would contain for a given privacy tier.",
            vec![
                string_enum("privacy_tier", &["redacted", "operator-mode", "full"]),
                uint("diagnostics_limit", Some(1), Some(5000), None),
                uint("history_window_hours", Some(1), Some(720), None),
            ],
            AetowerMcpServer::tool_support_bundle_manifest,
        ),
        ToolDescriptor::with_args(
            "aetower_recommendations",
            "Return structured remediation recommendations derived from host load, history health, diagnostics, and entity recommendations. Items may include a suggested_action (e.g. \"suspend\", \"lower-priority\") with target_pid and target_label - pass these to aetower_process_action with dry_run:true to preview, then dry_run:false after confirming, to fix what is slowing the machine.",
            vec![uint("limit", Some(1), Some(50), None)],
            AetowerMcpServer::tool_recommendations,
        ),
        ToolDescriptor::with_args(
            "aetower_session_health",
            "Return a merged health view across runtime lag, diagnostics, history store, capabilities, host load, and MCP state.",
            vec![uint("history_window_hours", Some(1), Some(720), None)],
            AetowerMcpServer::tool_session_health,
        ),
        ToolDescriptor::with_args(
            "aetower_export_query",
            "Return a scoped, privacy-tiered export payload for current snapshot, history, diagnostics, and session health.",
            vec![
                string_enum("privacy_tier", &["redacted", "operator-mode", "full"]),
                string_array("entity_ids", Some(100)),
                uint("start_millis", Some(0), None, None),
                uint("end_millis", Some(0), None, None),
                boolean("include_history", None),
                boolean("include_snapshot", None),
                boolean("include_diagnostics", None),
                boolean("include_session_health", None),
                boolean("include_ai_runtime_report", None),
                uint("diagnostics_limit", Some(1), Some(5000), None),
                uint("history_limit", Some(1), Some(500), None),
            ],
            AetowerMcpServer::tool_export_query,
        ),
    ]
});

fn diagnostics_properties(include_summary_limit: bool) -> Vec<ToolProperty> {
    let mut properties = Vec::new();
    if include_summary_limit {
        properties.push(uint("limit", Some(1), Some(100), None));
        properties.push(uint(
            "query_limit",
            Some(1),
            Some(MAX_DIAGNOSTICS_QUERY_LIMIT as u64),
            Some(DEFAULT_DIAGNOSTICS_SUMMARY_QUERY_LIMIT as u64),
        ));
    } else {
        properties.push(uint(
            "limit",
            Some(1),
            Some(MAX_DIAGNOSTICS_QUERY_LIMIT as u64),
            None,
        ));
    }
    properties.push(string_enum(
        "minimum_level",
        &["trace", "debug", "info", "warn", "error"],
    ));
    properties.push(string_enum(
        "subsystem",
        &[
            "engine",
            "collector",
            "identity",
            "attribution",
            "friction",
            "history",
            "persistence",
            "telemetry",
            "gpu",
            "ffi",
            "ui",
            "adapter-chromium",
            "adapter-docker",
            "adapter-helper",
            "adapter-chau7",
            "adapter-vscode",
        ],
    ));
    properties.push(string("search"));
    properties.push(uint("since_millis", Some(0), None, None));
    properties.push(boolean("include_persisted", None));
    properties
}

fn process_expected_targets() -> ToolProperty {
    raw(
        "expected_targets",
        json!({
            "type": "array",
            "description": "Required for dry_run=false. Copy target_identities from the matching dry-run preview. Execution is refused if the planned targets changed or if a PID no longer matches the previewed start time or executable path.",
            "items": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "minimum": 2 },
                    "start_time_millis": { "type": "integer", "minimum": 0 },
                    "executable_path": { "type": "string" },
                    "display_name": { "type": "string" },
                    "nice_value": { "type": "integer", "minimum": -20, "maximum": 20 }
                },
                "required": ["pid"],
                "additionalProperties": false
            },
            "maxItems": 256
        }),
    )
}

fn string(name: &'static str) -> ToolProperty {
    raw(name, json!({ "type": "string" }))
}

fn string_enum(name: &'static str, values: &[&'static str]) -> ToolProperty {
    raw(
        name,
        json!({
            "type": "string",
            "enum": values,
        }),
    )
}

fn string_array(name: &'static str, max_items: Option<u64>) -> ToolProperty {
    let mut schema = json!({
        "type": "array",
        "items": { "type": "string" },
    });
    if let (Value::Object(map), Some(max_items)) = (&mut schema, max_items) {
        map.insert("maxItems".to_owned(), json!(max_items));
    }
    raw(name, schema)
}

fn boolean(name: &'static str, default: Option<bool>) -> ToolProperty {
    let mut schema = json!({ "type": "boolean" });
    if let (Value::Object(map), Some(default)) = (&mut schema, default) {
        map.insert("default".to_owned(), json!(default));
    }
    raw(name, schema)
}

fn uint(
    name: &'static str,
    minimum: Option<u64>,
    maximum: Option<u64>,
    default: Option<u64>,
) -> ToolProperty {
    let mut schema = json!({ "type": "integer" });
    if let Value::Object(map) = &mut schema {
        if let Some(minimum) = minimum {
            map.insert("minimum".to_owned(), json!(minimum));
        }
        if let Some(maximum) = maximum {
            map.insert("maximum".to_owned(), json!(maximum));
        }
        if let Some(default) = default {
            map.insert("default".to_owned(), json!(default));
        }
    }
    raw(name, schema)
}

fn int(
    name: &'static str,
    minimum: Option<i64>,
    maximum: Option<i64>,
    default: Option<i64>,
) -> ToolProperty {
    let mut schema = json!({ "type": "integer" });
    if let Value::Object(map) = &mut schema {
        if let Some(minimum) = minimum {
            map.insert("minimum".to_owned(), json!(minimum));
        }
        if let Some(maximum) = maximum {
            map.insert("maximum".to_owned(), json!(maximum));
        }
        if let Some(default) = default {
            map.insert("default".to_owned(), json!(default));
        }
    }
    raw(name, schema)
}

fn raw(name: &'static str, schema: Value) -> ToolProperty {
    ToolProperty {
        name,
        schema,
        required: false,
    }
}
