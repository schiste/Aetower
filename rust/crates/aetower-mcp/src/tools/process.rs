//! Process-domain MCP tool handlers.

use serde_json::Value;

use crate::*;

impl AetowerMcpServer {
    pub(crate) fn tool_entity_process_tree(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
        }

        let args: Args = parse_args(arguments)?;
        let snapshot = self.wait_for_nonzero_snapshot()?;
        let report = build_process_tree_report(&snapshot, &args.entity_id).map_err(tool_error)?;
        tool_json(report)
    }

    pub(crate) fn tool_memory_breakdown(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
            #[serde(default = "default_top_regions")]
            top_regions: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::MemoryBreakdown {
            entity_id: args.entity_id,
            top_regions: args.top_regions.max(1),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    pub(crate) fn tool_profile_entity(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
            #[serde(default = "default_profile_duration_seconds")]
            duration_seconds: u64,
            #[serde(default = "default_top_stacks")]
            top_stacks: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProfileEntity {
            entity_id: args.entity_id,
            duration_seconds: args.duration_seconds.clamp(1, MAX_PROFILE_DURATION_SECONDS),
            top_stacks: args.top_stacks.max(1),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    pub(crate) fn tool_wakeup_attribution(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            entity_id: String,
            #[serde(default = "default_profile_duration_seconds")]
            duration_seconds: u64,
            #[serde(default = "default_top_stacks")]
            top_stacks: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::WakeupAttribution {
            entity_id: args.entity_id,
            duration_seconds: args.duration_seconds.clamp(1, MAX_PROFILE_DURATION_SECONDS),
            top_stacks: args.top_stacks.max(1),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    pub(crate) fn tool_process_inspect(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            pid: u32,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessInspect { pid: args.pid };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    pub(crate) fn tool_process_open_resources(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            pid: u32,
            #[serde(default = "default_open_resource_limit")]
            limit: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessOpenResources {
            pid: args.pid,
            limit: args.limit.clamp(1, 500),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    pub(crate) fn tool_process_sample(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            pid: u32,
            #[serde(default = "default_profile_duration_seconds")]
            duration_seconds: u64,
            #[serde(default = "default_top_stacks")]
            top_stacks: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessSample {
            pid: args.pid,
            duration_seconds: args.duration_seconds.clamp(1, MAX_PROFILE_DURATION_SECONDS),
            top_stacks: args.top_stacks.max(1),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    pub(crate) fn tool_process_action(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            pid: u32,
            action: String,
            #[serde(default = "default_include_true")]
            dry_run: bool,
            #[serde(default)]
            reason: Option<String>,
            #[serde(default)]
            action_id: Option<String>,
            #[serde(default)]
            expected_targets: Vec<ProcessActionTargetIdentity>,
            #[serde(default)]
            restore_nice_value: Option<i32>,
            #[serde(default)]
            privileged_helper_approved: bool,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessAction {
            pid: args.pid,
            action: args.action,
            dry_run: args.dry_run,
            reason: args.reason,
            action_id: args.action_id,
            expected_targets: args.expected_targets,
            restore_nice_value: args.restore_nice_value,
            privileged_helper_approved: args.privileged_helper_approved,
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }

    pub(crate) fn tool_process_action_history(&self, arguments: Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(default = "default_process_action_history_window_minutes")]
            window_minutes: u64,
            #[serde(default = "default_process_action_history_limit")]
            limit: usize,
        }

        let args: Args = parse_args(arguments)?;
        let request = DynamicToolRequest::ProcessActionHistory {
            window_minutes: args.window_minutes.max(1),
            limit: args.limit.clamp(1, 200),
        };
        let result = self.execute_dynamic_request(&request).map_err(tool_error)?;
        Ok(result)
    }
}
