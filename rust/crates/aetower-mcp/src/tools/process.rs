//! Process-domain MCP tool handlers.

use serde_json::Value;

use crate::reports::process::{
    build_process_action_with_context, process_action_plan,
    process_action_target_identity_is_stable,
};
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
            restore_nice_value: Option<i32>,
            #[serde(default)]
            approval_token: Option<String>,
        }

        let args: Args = parse_args(arguments)?;
        if args.dry_run {
            if args.approval_token.is_some() {
                return Err(tool_error(
                    "approval_token is only valid when executing a previously issued preview.",
                ));
            }
            let context = ProcessActionRequestContext {
                action_id: args.action_id,
                reason: args.reason,
                restore_nice_value: args.restore_nice_value,
                ..ProcessActionRequestContext::default()
            };
            let mut report = build_process_action_with_context(
                &*self.data_source,
                args.pid,
                &args.action,
                true,
                context,
            )
            .map_err(tool_error)?;
            let target_identities_are_stable = report.target_identities.len()
                == report.target_pids.len()
                && report
                    .target_identities
                    .iter()
                    .all(process_action_target_identity_is_stable);
            let targets_are_visible = report
                .target_outcomes
                .iter()
                .all(|outcome| outcome.visible_before);
            if target_identities_are_stable && targets_are_visible {
                let approval_token = self
                    .approval_store
                    .issue(
                        report.action_id.clone(),
                        report.pid,
                        report.action.clone(),
                        args.restore_nice_value,
                        report.target_identities.clone(),
                    )
                    .map_err(tool_error)?;
                report.approval_token = Some(approval_token);
            } else {
                report.safety_notes.push(
                    "Execution approval was not issued because every visible target needs a stable start time and executable path.".to_owned(),
                );
            }
            return tool_json(report);
        }

        let token = args.approval_token.as_deref().ok_or_else(|| {
            tool_error(
                "Execution requires approval_token from a recent dry-run preview; Aetower will also show a native confirmation dialog.",
            )
        })?;
        let approval = self.approval_store.claim(token).map_err(tool_error)?;
        if approval.pid != args.pid {
            return Err(tool_error(
                "Execution refused: approval_token is bound to a different PID.",
            ));
        }
        let snapshot = self.data_source.latest_snapshot().map_err(tool_error)?;
        let current_plan =
            process_action_plan(Some(&snapshot), args.pid, &args.action).map_err(tool_error)?;
        if current_plan.normalized_action != approval.normalized_action {
            return Err(tool_error(
                "Execution refused: approval_token is bound to a different action.",
            ));
        }
        if args.restore_nice_value != approval.restore_nice_value {
            return Err(tool_error(
                "Execution refused: restore_nice_value differs from the approved preview.",
            ));
        }
        if let Some(action_id) = args.action_id.as_deref()
            && action_id.trim() != approval.action_id
        {
            return Err(tool_error(
                "Execution refused: action_id differs from the approved preview.",
            ));
        }
        let context = ProcessActionRequestContext {
            action_id: Some(approval.action_id),
            reason: args.reason,
            expected_targets: approval.expected_targets,
            restore_nice_value: approval.restore_nice_value,
            // The token is issued by the server, but the operator confirmation
            // below is still required before any signal or renice command.
            privileged_helper_approved: true,
            require_operator_confirmation: true,
        };
        let report = build_process_action_with_context(
            &*self.data_source,
            args.pid,
            &args.action,
            false,
            context,
        )
        .map_err(tool_error)?;
        tool_json(report)
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
