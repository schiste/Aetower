//! Local storage hygiene MCP tool handlers.

use serde_json::{Value, json};

use crate::*;

impl AetowerMcpServer {
    pub(crate) fn tool_storage_hygiene(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            roots: Vec<String>,
            #[serde(default = "default_storage_scan_depth")]
            max_depth: usize,
            #[serde(default = "default_storage_scan_limit")]
            limit: usize,
            #[serde(default = "default_storage_scan_mode")]
            mode: String,
            #[serde(default)]
            refresh: bool,
            #[serde(default)]
            background_scan: bool,
            #[serde(default)]
            refresh_mode: String,
            #[serde(default = "default_storage_throttle_hint")]
            throttle_hint: String,
            #[serde(default)]
            dirty_paths: Vec<String>,
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_indexed_json(
            args.roots.clone(),
            args.max_depth,
            args.limit,
        )
        .map_err(|error| tool_error(format!("storage_hygiene_failed: {error}")))?;
        let background_scan = start_optional_storage_refresh(
            args.roots,
            args.max_depth,
            args.limit,
            &args.mode,
            &args.refresh_mode,
            args.refresh || args.background_scan,
            &args.throttle_hint,
            args.dirty_paths,
        );
        parse_tool_json_with_background_scan(&json, background_scan)
    }

    pub(crate) fn tool_storage_hygiene_overview(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            roots: Vec<String>,
            #[serde(default = "default_storage_scan_depth")]
            max_depth: usize,
            #[serde(default = "default_storage_scan_mode")]
            mode: String,
            #[serde(default)]
            refresh: bool,
            #[serde(default)]
            background_scan: bool,
            #[serde(default = "default_storage_scan_limit")]
            limit: usize,
            #[serde(default)]
            refresh_mode: String,
            #[serde(default = "default_storage_throttle_hint")]
            throttle_hint: String,
            #[serde(default)]
            dirty_paths: Vec<String>,
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_overview_json(
            args.roots.clone(),
            args.max_depth,
            default_storage_scan_mode().as_str(),
        )
        .map_err(|error| tool_error(format!("storage_hygiene_overview_failed: {error}")))?;
        let background_scan = start_optional_storage_refresh(
            args.roots,
            args.max_depth,
            args.limit,
            &args.mode,
            &args.refresh_mode,
            args.refresh || args.background_scan,
            &args.throttle_hint,
            args.dirty_paths,
        );
        parse_tool_json_with_background_scan(&json, background_scan)
    }

    pub(crate) fn tool_storage_hygiene_actions(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            roots: Vec<String>,
            #[serde(default = "default_storage_scan_depth")]
            max_depth: usize,
            #[serde(default = "default_storage_scan_limit")]
            limit: usize,
            #[serde(default = "default_storage_scan_mode")]
            mode: String,
            #[serde(default)]
            refresh: bool,
            #[serde(default)]
            background_scan: bool,
            #[serde(default)]
            refresh_mode: String,
            #[serde(default = "default_storage_throttle_hint")]
            throttle_hint: String,
            #[serde(default)]
            dirty_paths: Vec<String>,
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_actions_json(
            args.roots.clone(),
            args.max_depth,
            args.limit,
            default_storage_scan_mode().as_str(),
        )
        .map_err(|error| tool_error(format!("storage_hygiene_actions_failed: {error}")))?;
        let background_scan = start_optional_storage_refresh(
            args.roots,
            args.max_depth,
            args.limit,
            &args.mode,
            &args.refresh_mode,
            args.refresh || args.background_scan,
            &args.throttle_hint,
            args.dirty_paths,
        );
        parse_tool_json_with_background_scan(&json, background_scan)
    }

    pub(crate) fn tool_storage_hygiene_items_page(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            roots: Vec<String>,
            #[serde(default = "default_storage_scan_depth")]
            max_depth: usize,
            #[serde(default)]
            offset: usize,
            #[serde(default = "default_storage_scan_limit")]
            limit: usize,
            #[serde(default = "default_storage_scan_mode")]
            mode: String,
            #[serde(default = "default_storage_item_sort_key")]
            sort_key: String,
            #[serde(default = "default_storage_item_sort_descending")]
            sort_descending: bool,
            #[serde(default)]
            refresh: bool,
            #[serde(default)]
            background_scan: bool,
            #[serde(default)]
            refresh_mode: String,
            #[serde(default = "default_storage_throttle_hint")]
            throttle_hint: String,
            #[serde(default)]
            dirty_paths: Vec<String>,
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_items_page_json(
            args.roots.clone(),
            args.max_depth,
            args.offset,
            args.limit,
            default_storage_scan_mode().as_str(),
            &args.sort_key,
            args.sort_descending,
        )
        .map_err(|error| tool_error(format!("storage_hygiene_items_page_failed: {error}")))?;
        let background_scan = start_optional_storage_refresh(
            args.roots,
            args.max_depth,
            args.limit,
            &args.mode,
            &args.refresh_mode,
            args.refresh || args.background_scan,
            &args.throttle_hint,
            args.dirty_paths,
        );
        parse_tool_json_with_background_scan(&json, background_scan)
    }

    pub(crate) fn tool_storage_growth_insights(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            roots: Vec<String>,
            #[serde(default = "default_storage_growth_window_days")]
            window_days: u64,
        }

        let args: Args = parse_args(arguments)?;
        let json =
            crate::reports::storage::storage_growth_insights_json(args.roots, args.window_days)
                .map_err(|error| tool_error(format!("storage_growth_insights_failed: {error}")))?;
        parse_tool_json(&json)
    }

    pub(crate) fn tool_storage_hygiene_repo_detail(
        &self,
        arguments: Value,
    ) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            repo_root: String,
            #[serde(default = "default_storage_scan_mode")]
            mode: String,
            #[serde(default)]
            refresh: bool,
            #[serde(default)]
            background_scan: bool,
            #[serde(default)]
            refresh_mode: String,
            #[serde(default = "default_storage_throttle_hint")]
            throttle_hint: String,
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_repo_detail_json(
            args.repo_root.clone(),
            default_storage_scan_mode().as_str(),
        )
        .map_err(|error| tool_error(format!("storage_hygiene_repo_detail_failed: {error}")))?;
        let background_scan = start_optional_storage_refresh(
            vec![args.repo_root],
            8,
            120,
            &args.mode,
            &args.refresh_mode,
            args.refresh || args.background_scan,
            &args.throttle_hint,
            Vec::new(),
        );
        parse_tool_json_with_background_scan(&json, background_scan)
    }

    pub(crate) fn tool_storage_hygiene_deep_scan(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            roots: Vec<String>,
            #[serde(default = "default_storage_scan_depth")]
            max_depth: usize,
            #[serde(default = "default_storage_scan_limit")]
            limit: usize,
            #[serde(default = "default_storage_throttle_hint")]
            throttle_hint: String,
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_scan_start_json(
            args.roots,
            args.max_depth,
            args.limit,
            "deep_native",
            &args.throttle_hint,
            Vec::new(),
        )
        .map_err(|error| tool_error(format!("storage_hygiene_deep_scan_failed: {error}")))?;
        parse_tool_json(&json)
    }
}

fn parse_tool_json(json: &str) -> Result<Value, Value> {
    // Reports are built as JSON strings; re-wrap them in the MCP tool-result
    // envelope (content + structuredContent). Returning the bare object makes
    // spec-compliant clients render an empty result.
    let value: Value = serde_json::from_str(json)
        .map_err(|error| tool_error(format!("storage_hygiene_json_failed: {error}")))?;
    tool_json(value)
}

fn parse_tool_json_with_background_scan(
    json: &str,
    background_scan: Option<Value>,
) -> Result<Value, Value> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| tool_error(format!("storage_hygiene_json_failed: {error}")))?;
    tool_json_with_background_scan(value, background_scan)
}

fn tool_json_with_background_scan(
    mut value: Value,
    background_scan: Option<Value>,
) -> Result<Value, Value> {
    if let Some(background_scan) = background_scan {
        if !value.get("cache_status").is_some_and(Value::is_object) {
            value["cache_status"] = json!({});
        }
        if let Some(cache_status) = value.get_mut("cache_status").and_then(Value::as_object_mut) {
            cache_status.insert("background_scan".to_owned(), background_scan);
        }
    }
    tool_json(value)
}

#[allow(clippy::too_many_arguments)]
fn start_optional_storage_refresh(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
    requested_mode: &str,
    refresh_mode: &str,
    refresh: bool,
    throttle_hint: &str,
    dirty_paths: Vec<String>,
) -> Option<Value> {
    if !refresh {
        return None;
    }
    let mode = storage_background_scan_mode(requested_mode, refresh_mode);
    match crate::reports::storage::storage_scan_start_json(
        roots,
        max_depth,
        limit,
        &mode,
        throttle_hint,
        dirty_paths,
    ) {
        Ok(json) => serde_json::from_str::<Value>(&json)
            .ok()
            .map(|mut value| {
                if let Some(object) = value.as_object_mut() {
                    object.insert("requested".to_owned(), Value::Bool(true));
                }
                value
            })
            .or_else(|| {
                Some(json!({
                    "requested": true,
                    "status": "failed",
                    "mode": mode,
                    "error_message": "background scan response was not valid JSON"
                }))
            }),
        Err(error) => Some(json!({
            "requested": true,
            "status": "failed",
            "mode": mode,
            "error_message": error
        })),
    }
}

fn storage_background_scan_mode(requested_mode: &str, refresh_mode: &str) -> String {
    let refresh_mode = refresh_mode.trim();
    if !refresh_mode.is_empty() {
        return refresh_mode.to_owned();
    }
    let requested_mode = requested_mode.trim();
    if requested_mode.is_empty() || requested_mode == default_storage_scan_mode() {
        "fast_changed_only".to_owned()
    } else {
        requested_mode.to_owned()
    }
}

fn default_storage_scan_depth() -> usize {
    5
}

fn default_storage_scan_limit() -> usize {
    80
}

fn default_storage_scan_mode() -> String {
    "instant_cached".to_owned()
}

fn default_storage_throttle_hint() -> String {
    "normal".to_owned()
}

fn default_storage_item_sort_key() -> String {
    "size".to_owned()
}

fn default_storage_growth_window_days() -> u64 {
    30
}

fn default_storage_item_sort_descending() -> bool {
    true
}
