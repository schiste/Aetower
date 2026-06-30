//! Repository intelligence MCP tool handlers.

use serde_json::Value;

use crate::*;

impl AetowerMcpServer {
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
    serde_json::from_str(json)
        .map_err(|error| tool_error(format!("repository_scorecard_json_failed: {error}")))
}

fn default_repository_scorecard_mode() -> String {
    "auto".to_owned()
}

fn default_repository_scorecard_timeout_seconds() -> u64 {
    30
}
