//! Repository intelligence MCP tool handlers.

use serde_json::Value;

use crate::*;

impl AetowerMcpServer {
    pub(crate) fn tool_repository_inventory(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default)]
            roots: Vec<String>,
            #[serde(default = "default_repository_inventory_depth")]
            max_depth: usize,
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::storage::repository_inventory_json(args.roots, args.max_depth)
            .map_err(|error| tool_error(format!("repository_inventory_failed: {error}")))?;
        parse_tool_json(&json)
    }

    pub(crate) fn tool_repository_scorecard(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            repo_root: String,
            #[serde(default = "default_repository_scorecard_mode")]
            mode: String,
            #[serde(default = "default_repository_scorecard_timeout_seconds")]
            timeout_seconds: u64,
            #[serde(default)]
            refresh: bool,
        }

        let args: Args = parse_args(arguments)?;
        let json = crate::reports::repository_scorecard::repository_scorecard_json_cached(
            args.repo_root,
            &args.mode,
            args.timeout_seconds,
            args.refresh,
        )
        .map_err(|error| tool_error(format!("repository_scorecard_failed: {error}")))?;
        parse_tool_json(&json)
    }
}

fn parse_tool_json(json: &str) -> Result<Value, Value> {
    // Reports are built as JSON strings; re-wrap them in the MCP tool-result
    // envelope (content + structuredContent). Returning the bare object makes
    // spec-compliant clients render an empty result.
    let value: Value = serde_json::from_str(json)
        .map_err(|error| tool_error(format!("repository_json_failed: {error}")))?;
    tool_json(value)
}

fn default_repository_inventory_depth() -> usize {
    5
}

fn default_repository_scorecard_mode() -> String {
    "auto".to_owned()
}

fn default_repository_scorecard_timeout_seconds() -> u64 {
    30
}
