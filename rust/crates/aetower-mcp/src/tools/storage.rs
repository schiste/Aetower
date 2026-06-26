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
        }

        let args: Args = parse_args(arguments)?;
        let report = crate::reports::storage::build_storage_hygiene_report(
            args.roots,
            args.max_depth,
            args.limit,
        );
        tool_json(report)
    }
}

fn default_storage_scan_depth() -> usize {
    5
}

fn default_storage_scan_limit() -> usize {
    80
}
