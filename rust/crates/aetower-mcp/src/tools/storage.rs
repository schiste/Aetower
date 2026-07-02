//! Local storage hygiene MCP tool handlers.

use serde_json::Value;

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
        }

        let args: Args = parse_args(arguments)?;
        let report = crate::reports::storage::build_storage_hygiene_report_with_mode(
            args.roots,
            args.max_depth,
            args.limit,
            &args.mode,
        );
        tool_json(report)
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
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_overview_json(
            args.roots,
            args.max_depth,
            &args.mode,
        )
        .map_err(|error| tool_error(format!("storage_hygiene_overview_failed: {error}")))?;
        parse_tool_json(&json)
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
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_actions_json(
            args.roots,
            args.max_depth,
            args.limit,
            &args.mode,
        )
        .map_err(|error| tool_error(format!("storage_hygiene_actions_failed: {error}")))?;
        parse_tool_json(&json)
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
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_items_page_json(
            args.roots,
            args.max_depth,
            args.offset,
            args.limit,
            &args.mode,
            &args.sort_key,
            args.sort_descending,
        )
        .map_err(|error| tool_error(format!("storage_hygiene_items_page_failed: {error}")))?;
        parse_tool_json(&json)
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
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_repo_detail_json(
            args.repo_root,
            &args.mode,
        )
        .map_err(|error| tool_error(format!("storage_hygiene_repo_detail_failed: {error}")))?;
        parse_tool_json(&json)
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
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::storage_hygiene_deep_scan_json(
            args.roots,
            args.max_depth,
            args.limit,
        )
        .map_err(|error| tool_error(format!("storage_hygiene_deep_scan_failed: {error}")))?;
        parse_tool_json(&json)
    }
}

fn parse_tool_json(json: &str) -> Result<Value, Value> {
    serde_json::from_str(json)
        .map_err(|error| tool_error(format!("storage_hygiene_json_failed: {error}")))
}

fn default_storage_scan_depth() -> usize {
    5
}

fn default_storage_scan_limit() -> usize {
    80
}

fn default_storage_scan_mode() -> String {
    "fast_changed_only".to_owned()
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
