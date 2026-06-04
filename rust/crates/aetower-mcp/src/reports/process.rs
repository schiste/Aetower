//! Process-domain report builders for the MCP server.

use std::collections::BTreeMap;

use aetower_model::SystemSnapshot;
use aetower_model::classify_signature;
use serde_json::Value;

use crate::*;

mod memory;
mod process_tree;
mod resources;
mod sampling;

#[cfg(test)]
pub(crate) use memory::parse_vmmap_region_line;
pub(crate) use memory::{build_entity_memory_breakdown, vmmap_regions_for_processes};
pub use memory::{memory_breakdown_json, self_memory_attribution_json};
pub use process_tree::entity_process_tree_json;
pub(crate) use process_tree::{build_process_tree_report, extract_parent_pid};
pub(crate) use resources::build_process_open_resources;
#[cfg(test)]
pub(crate) use resources::{
    build_resource_holders_by_file, build_resource_holders_by_port, parse_lsof_holder_line,
    parse_lsof_holders, parse_lsof_resources,
};
pub use resources::{
    process_open_resources_json, resource_holders_by_file_json, resource_holders_by_port_json,
};
#[cfg(test)]
pub(crate) use sampling::parse_sample_threads;
pub(crate) use sampling::{build_entity_profile, build_process_sample, build_wakeup_attribution};
pub use sampling::{process_sample_json, profile_entity_json, wakeup_attribution_json};

const PROCESS_ACTION_VERIFICATION_DELAY_MILLIS: u64 = 180;
const PROCESS_ACTION_COMMAND_OUTPUT_LIMIT: usize = 4096;

#[derive(Debug, Clone)]
struct ProcessActionTargetState {
    visible: bool,
    status: Option<String>,
    nice: Option<i32>,
}

pub(crate) fn process_dynamic_tool_request(
    data_source: &dyn AetowerMcpDataSource,
    request: &DynamicToolRequest,
) -> Result<Value, String> {
    match request {
        DynamicToolRequest::MemoryBreakdown {
            entity_id,
            top_regions,
        } => tool_json(build_entity_memory_breakdown(
            data_source,
            entity_id,
            *top_regions,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProfileEntity {
            entity_id,
            duration_seconds,
            top_stacks,
        } => tool_json(build_entity_profile(
            data_source,
            entity_id,
            *duration_seconds,
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::WakeupAttribution {
            entity_id,
            duration_seconds,
            top_stacks,
        } => tool_json(build_wakeup_attribution(
            data_source,
            entity_id,
            *duration_seconds,
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProcessInspect { pid } => {
            tool_json(build_process_inspection(data_source, *pid)?)
                .map_err(|error| extract_tool_error_message(&error))
        }
        DynamicToolRequest::ProcessOpenResources { pid, limit } => {
            tool_json(build_process_open_resources(*pid, *limit)?)
                .map_err(|error| extract_tool_error_message(&error))
        }
        DynamicToolRequest::ProcessSample {
            pid,
            duration_seconds,
            top_stacks,
        } => tool_json(build_process_sample(
            *pid,
            (*duration_seconds).clamp(1, MAX_PROFILE_DURATION_SECONDS),
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProcessAction {
            pid,
            action,
            dry_run,
            reason,
            action_id,
            expected_targets,
            restore_nice_value,
            privileged_helper_approved,
        } => {
            let context = ProcessActionRequestContext {
                action_id: action_id.clone(),
                reason: reason.clone(),
                expected_targets: expected_targets.clone(),
                restore_nice_value: *restore_nice_value,
                privileged_helper_approved: *privileged_helper_approved,
            };
            tool_json(build_process_action_with_context(
                data_source,
                *pid,
                action,
                *dry_run,
                context,
            )?)
            .map_err(|error| extract_tool_error_message(&error))
        }
        DynamicToolRequest::ProcessActionHistory {
            window_minutes,
            limit,
        } => tool_json(build_process_action_history(
            data_source,
            *window_minutes,
            *limit,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProcessComponentContext {
    entity_id: String,
    display_name: String,
    component_title: String,
    component_kind: String,
    executable_path: Option<String>,
    command_line: Option<String>,
    cwd: Option<String>,
    user: Option<String>,
    parent_summary: Option<String>,
    cpu_percent: f32,
    memory_bytes: u64,
    memory_physical_footprint_bytes: u64,
    start_time_millis: u64,
    sibling_process_count: u32,
}

pub(crate) fn build_process_inspection(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
) -> Result<ProcessInspectionReport, String> {
    validate_pid(pid)?;
    let snapshot = data_source.latest_snapshot()?;
    let context = process_component_context(&snapshot, pid);
    let ps = process_ps_summary(pid).ok();
    let alive = ps.is_some() || process_exists(pid);
    let child_pids = process_child_pids(&snapshot, pid);
    let parent_pid = context
        .as_ref()
        .and_then(|context| extract_parent_pid(context.parent_summary.as_deref()))
        .or_else(|| ps.as_ref().and_then(|summary| summary.parent_pid));
    let mut safety_notes = Vec::new();
    if pid == std::process::id() {
        safety_notes.push(
            "This is the running Aetower process; destructive actions are blocked.".to_owned(),
        );
    }
    if context.is_none() {
        safety_notes.push(
            "This PID is not currently attributed to an Aetower entity; live ps data may still exist."
                .to_owned(),
        );
    }
    if !alive {
        safety_notes.push("The process is not visible to ps right now.".to_owned());
    }

    let executable_path = context
        .as_ref()
        .and_then(|context| context.executable_path.clone());
    let bundle = executable_path
        .as_deref()
        .and_then(read_process_bundle_info);
    let signature = executable_path.as_deref().map(read_process_signature);
    let entitlements = executable_path
        .as_deref()
        .map(read_process_entitlements)
        .unwrap_or_default();
    let (environment, environment_note) = read_process_environment(pid);
    let startup_entry = executable_path.as_deref().and_then(lookup_startup_entry);
    let dyld_insert = environment
        .iter()
        .find(|entry| entry.key == "DYLD_INSERT_LIBRARIES")
        .map(|entry| entry.value.clone())
        .unwrap_or_default();
    let (loaded_dylibs, dylib_summary) = read_process_dylibs(pid, &dyld_insert);

    Ok(ProcessInspectionReport {
        captured_at_millis: snapshot.captured_at_millis,
        pid,
        alive,
        entity_id: context.as_ref().map(|context| context.entity_id.clone()),
        display_name: context.as_ref().map(|context| context.display_name.clone()),
        component_title: context
            .as_ref()
            .map(|context| context.component_title.clone()),
        component_kind: context
            .as_ref()
            .map(|context| context.component_kind.clone()),
        executable_path,
        command_line: context
            .as_ref()
            .and_then(|context| context.command_line.clone())
            .or_else(|| ps.as_ref().and_then(|summary| summary.command.clone())),
        cwd: context.as_ref().and_then(|context| context.cwd.clone()),
        user: context
            .as_ref()
            .and_then(|context| context.user.clone())
            .or_else(|| ps.as_ref().and_then(|summary| summary.user.clone())),
        parent_pid,
        parent_summary: context
            .as_ref()
            .and_then(|context| context.parent_summary.clone()),
        cpu_percent: context
            .as_ref()
            .map(|context| context.cpu_percent)
            .or_else(|| ps.as_ref().and_then(|summary| summary.cpu_percent)),
        memory_bytes: context
            .as_ref()
            .map(|context| context.memory_bytes)
            .or_else(|| ps.as_ref().and_then(|summary| summary.resident_bytes)),
        memory_physical_footprint_bytes: context
            .as_ref()
            .map(|context| context.memory_physical_footprint_bytes),
        start_time_millis: context.as_ref().map(|context| context.start_time_millis),
        child_pids,
        sibling_process_count: context
            .as_ref()
            .map(|context| context.sibling_process_count)
            .unwrap_or(0),
        ps,
        bundle,
        signature,
        entitlements,
        environment,
        environment_note,
        startup_entry,
        loaded_dylibs,
        dylib_summary,
        safety_notes,
    })
}

pub fn process_inspect_json(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
) -> Result<String, String> {
    let report = build_process_inspection(data_source, pid)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_process_action(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
    action: &str,
    dry_run: bool,
    reason: Option<String>,
) -> Result<ProcessActionReport, String> {
    let context = parse_process_action_request_context(reason);
    build_process_action_with_context(data_source, pid, action, dry_run, context)
}

pub(crate) fn build_process_action_with_context(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
    action: &str,
    dry_run: bool,
    request_context: ProcessActionRequestContext,
) -> Result<ProcessActionReport, String> {
    validate_pid(pid)?;
    let snapshot = data_source.latest_snapshot().ok();
    let mut plan = process_action_plan(snapshot.as_ref(), pid, action)?;
    if plan.normalized_action == "normal-priority"
        && let Some(restore_nice_value) = request_context.restore_nice_value
    {
        plan = renice_process_action_plan(
            "normal-priority",
            restore_nice_value.clamp(-20, 20),
            pid,
            "Dry run only; priority was not changed.",
            format!(
                "Requested restored priority for process {pid} with nice value {}.",
                restore_nice_value.clamp(-20, 20)
            ),
        );
    }
    let context = snapshot
        .as_ref()
        .and_then(|snapshot| process_component_context(snapshot, pid));
    verify_expected_process_identities(
        snapshot.as_ref(),
        &request_context.expected_targets,
        &plan.target_pids,
        !dry_run,
    )?;
    let action_id = normalized_action_id(request_context.action_id.as_deref(), pid);
    let target_identities = process_action_target_identities(snapshot.as_ref(), &plan.target_pids);
    let blast_radius = process_action_blast_radius(&plan);
    let before_states = collect_process_action_target_states(&plan.target_pids);
    let mut safety_notes = Vec::new();
    if request_context.privileged_helper_approved {
        safety_notes.push(
            "Privileged process-action helper was explicitly approved. Aetower will use it only if the normal macOS command path fails."
                .to_owned(),
        );
    }
    if plan.target_pids.contains(&std::process::id()) {
        safety_notes.push("Aetower refuses to target its own running process.".to_owned());
        if !dry_run {
            return Err("Refusing to target the running Aetower process.".to_owned());
        }
    }
    if !dry_run {
        let missing_pids = plan
            .target_pids
            .iter()
            .copied()
            .filter(|target_pid| {
                before_states
                    .get(target_pid)
                    .is_none_or(|state| !state.visible)
            })
            .collect::<Vec<_>>();
        if !missing_pids.is_empty() {
            return Err(format!(
                "Target process(es) are not visible to ps right now: {missing_pids:?}."
            ));
        }
    }

    if dry_run {
        let target_outcomes = process_action_preview_outcomes(&plan.target_pids, &before_states);
        return Ok(ProcessActionReport {
            captured_at_millis: current_unix_millis().unwrap_or_default(),
            action_id,
            pid,
            target_pids: plan.target_pids,
            target_identities,
            blast_radius,
            action: plan.normalized_action,
            signal: plan.signal,
            dry_run,
            executed: false,
            success: true,
            command: plan.command,
            verification: "preview".to_owned(),
            target_outcomes,
            follow_up_checks: Vec::new(),
            command_result: None,
            privileged_helper_status: privileged_helper_status(
                request_context.privileged_helper_approved,
            ),
            reason: request_context.reason,
            entity_id: context.as_ref().map(|context| context.entity_id.clone()),
            display_name: context.as_ref().map(|context| context.display_name.clone()),
            message: plan.dry_run_message,
            safety_notes,
        });
    }

    let (mut success, mut command_result, mut failure_message) =
        run_process_action_command(&plan.program, &plan.args)?;
    let mut privileged_helper_status =
        privileged_helper_status(request_context.privileged_helper_approved);
    if !success && request_context.privileged_helper_approved {
        match run_privileged_helper_process_action(&plan) {
            Ok(helper_result) => {
                success = true;
                command_result = helper_result;
                privileged_helper_status = "used".to_owned();
            }
            Err(error) => {
                privileged_helper_status = "approved-but-failed".to_owned();
                failure_message = format!(
                    "{failure_message} Privileged helper fallback did not complete: {error}"
                );
            }
        }
    }
    if should_delay_process_action_verification(&plan.normalized_action, success) {
        std::thread::sleep(std::time::Duration::from_millis(
            PROCESS_ACTION_VERIFICATION_DELAY_MILLIS,
        ));
    }
    let after_states = collect_process_action_target_states(&plan.target_pids);
    let (verification, target_outcomes) = process_action_target_outcomes(
        &plan.normalized_action,
        &plan.signal,
        &plan.target_pids,
        &before_states,
        &after_states,
        success,
    );
    let follow_up_checks = process_action_follow_up_checks(
        &plan.normalized_action,
        &plan.signal,
        &plan.target_pids,
        &before_states,
        success,
    );
    let message = if success {
        format!(
            "{} {}",
            plan.success_message,
            process_action_verification_sentence(&verification)
        )
    } else {
        failure_message
    };
    let report = ProcessActionReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        action_id,
        pid,
        target_pids: plan.target_pids,
        target_identities,
        blast_radius,
        action: plan.normalized_action,
        signal: plan.signal,
        dry_run,
        executed: true,
        success,
        command: plan.command,
        verification,
        target_outcomes,
        follow_up_checks,
        command_result: Some(command_result),
        privileged_helper_status,
        reason: request_context.reason.clone(),
        entity_id: context.as_ref().map(|context| context.entity_id.clone()),
        display_name: context.as_ref().map(|context| context.display_name.clone()),
        message,
        safety_notes,
    };
    data_source.record_diagnostics_event(process_action_diagnostics_event(&report));
    Ok(report)
}

pub fn process_action_json(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
    action: &str,
    dry_run: bool,
    reason: Option<String>,
) -> Result<String, String> {
    let report = build_process_action(data_source, pid, action, dry_run, reason)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_process_action_history(
    data_source: &dyn AetowerMcpDataSource,
    window_minutes: u64,
    limit: usize,
) -> Result<ProcessActionHistoryReport, String> {
    let now = current_unix_millis().unwrap_or_default();
    let events = data_source.query_diagnostics(DiagnosticsQuery {
        limit: limit.saturating_mul(4).max(limit).max(1),
        search: Some("process-action".to_owned()),
        since_millis: Some(now.saturating_sub(window_minutes.saturating_mul(60 * 1000))),
        include_persisted: true,
        ..DiagnosticsQuery::default()
    })?;
    let actions = events
        .into_iter()
        .filter(|event| event.event_type == "process-action")
        .take(limit.max(1))
        .map(process_action_history_item)
        .collect::<Vec<_>>();
    Ok(ProcessActionHistoryReport {
        window_minutes,
        returned: actions.len(),
        actions,
    })
}

pub fn process_action_history_json(
    data_source: &dyn AetowerMcpDataSource,
    window_minutes: u64,
    limit: usize,
) -> Result<String, String> {
    let report = build_process_action_history(data_source, window_minutes.max(1), limit.max(1))?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn parse_process_action_request_context(reason: Option<String>) -> ProcessActionRequestContext {
    let Some(reason) = reason else {
        return ProcessActionRequestContext::default();
    };
    if let Ok(envelope) = serde_json::from_str::<ProcessActionReasonEnvelope>(&reason)
        && envelope.aetower_process_action_context == Some(1)
    {
        return ProcessActionRequestContext {
            action_id: envelope.action_id,
            reason: envelope.reason,
            expected_targets: envelope.expected_targets,
            restore_nice_value: envelope.restore_nice_value,
            privileged_helper_approved: envelope.privileged_helper_approved,
        };
    }
    ProcessActionRequestContext {
        reason: Some(reason),
        ..ProcessActionRequestContext::default()
    }
}

fn normalized_action_id(action_id: Option<&str>, pid: u32) -> String {
    let cleaned = action_id
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 96
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
        })
        .map(str::to_owned);
    cleaned.unwrap_or_else(|| format!("action-{}-{pid}", current_unix_millis().unwrap_or_default()))
}

fn privileged_helper_status(approved: bool) -> String {
    if approved {
        "approved-not-needed".to_owned()
    } else {
        "not-requested".to_owned()
    }
}

fn process_action_target_identities(
    snapshot: Option<&SystemSnapshot>,
    target_pids: &[u32],
) -> Vec<ProcessActionTargetIdentity> {
    target_pids
        .iter()
        .copied()
        .map(|pid| {
            let context = snapshot.and_then(|snapshot| process_component_context(snapshot, pid));
            let state = process_action_target_state(pid);
            ProcessActionTargetIdentity {
                pid,
                start_time_millis: context
                    .as_ref()
                    .map(|context| context.start_time_millis)
                    .filter(|value| *value > 0),
                executable_path: context
                    .as_ref()
                    .and_then(|context| context.executable_path.clone()),
                display_name: context.as_ref().map(|context| context.display_name.clone()),
                nice_value: state.nice,
            }
        })
        .collect()
}

fn verify_expected_process_identities(
    snapshot: Option<&SystemSnapshot>,
    expected_targets: &[ProcessActionTargetIdentity],
    planned_targets: &[u32],
    require_expected_targets: bool,
) -> Result<(), String> {
    if expected_targets.is_empty() {
        if require_expected_targets {
            return Err(
                "Refusing process action: execution requires expected_targets from a dry-run preview."
                    .to_owned(),
            );
        }
        return Ok(());
    }
    let planned = planned_targets
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let expected = expected_targets
        .iter()
        .map(|target| target.pid)
        .collect::<std::collections::BTreeSet<_>>();
    if expected != planned {
        let missing = planned.difference(&expected).copied().collect::<Vec<_>>();
        let unexpected = expected.difference(&planned).copied().collect::<Vec<_>>();
        return Err(format!(
            "Refusing process action: preview target set does not match current plan. Missing expected identities for {missing:?}; unexpected preview identities {unexpected:?}."
        ));
    }
    for expected in expected_targets {
        let current =
            snapshot.and_then(|snapshot| process_component_context(snapshot, expected.pid));
        if expected.start_time_millis.is_some() || expected.executable_path.is_some() {
            let Some(current) = current else {
                return Err(format!(
                    "Refusing process action: PID {} is no longer attributed, so Aetower cannot verify it is the previewed process.",
                    expected.pid
                ));
            };
            if let Some(expected_start) = expected.start_time_millis
                && expected_start > 0
                && current.start_time_millis > 0
                && current.start_time_millis != expected_start
            {
                return Err(format!(
                    "Refusing process action: PID {} start time changed from {} to {}.",
                    expected.pid, expected_start, current.start_time_millis
                ));
            }
            if let Some(expected_path) = expected.executable_path.as_deref()
                && current.executable_path.as_deref() != Some(expected_path)
            {
                return Err(format!(
                    "Refusing process action: PID {} executable changed since preview.",
                    expected.pid
                ));
            }
        }
    }
    Ok(())
}

fn process_action_blast_radius(plan: &ProcessActionPlan) -> ProcessActionBlastRadius {
    let tree_action = matches!(
        plan.normalized_action.as_str(),
        "terminate-tree" | "force-kill-tree"
    );
    let target_count = plan.target_pids.len();
    let confirmation_required = tree_action || target_count > 1;
    let summary = if tree_action {
        format!(
            "{} will target {} process{} in the known process tree.",
            plan.normalized_action,
            target_count,
            if target_count == 1 { "" } else { "es" }
        )
    } else {
        format!(
            "{} will target {} process{}.",
            plan.normalized_action,
            target_count,
            if target_count == 1 { "" } else { "es" }
        )
    };
    ProcessActionBlastRadius {
        target_count,
        tree_action,
        confirmation_required,
        summary,
    }
}

fn collect_process_action_target_states(
    target_pids: &[u32],
) -> BTreeMap<u32, ProcessActionTargetState> {
    target_pids
        .iter()
        .copied()
        .map(|pid| (pid, process_action_target_state(pid)))
        .collect()
}

fn process_action_target_state(pid: u32) -> ProcessActionTargetState {
    let ps = process_ps_summary(pid).ok();
    let visible = ps.is_some() || process_exists(pid);
    let status = ps.and_then(|summary| summary.status);
    ProcessActionTargetState {
        visible,
        status,
        nice: process_nice_value(pid),
    }
}

fn process_action_preview_outcomes(
    target_pids: &[u32],
    before_states: &BTreeMap<u32, ProcessActionTargetState>,
) -> Vec<ProcessActionTargetOutcome> {
    target_pids
        .iter()
        .copied()
        .map(|pid| {
            let before = before_states
                .get(&pid)
                .cloned()
                .unwrap_or(ProcessActionTargetState {
                    visible: false,
                    status: None,
                    nice: None,
                });
            ProcessActionTargetOutcome {
                pid,
                visible_before: before.visible,
                visible_after: None,
                nice_before: before.nice,
                status_after: None,
                nice_after: None,
                verification: if before.visible {
                    "target-visible".to_owned()
                } else {
                    "target-not-visible".to_owned()
                },
                detail: if before.visible {
                    "Target is visible before execution.".to_owned()
                } else {
                    "Target is not visible before execution; executing this action would fail."
                        .to_owned()
                },
            }
        })
        .collect()
}

fn run_process_action_command(
    program: &str,
    args: &[String],
) -> Result<(bool, ProcessActionCommandResult, String), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("run {program}: {error}"))?;
    let stdout = bounded_command_text(&output.stdout);
    let stderr = bounded_command_text(&output.stderr);
    let result = ProcessActionCommandResult {
        exit_status: output.status.code(),
        stdout,
        stderr: stderr.clone(),
    };
    let failure_message = if output.status.success() {
        String::new()
    } else if stderr.is_empty() {
        format!("{program} exited with status {}.", output.status)
    } else {
        format!("{program} failed: {stderr}")
    };
    Ok((output.status.success(), result, failure_message))
}

fn run_privileged_helper_process_action(
    plan: &ProcessActionPlan,
) -> Result<ProcessActionCommandResult, String> {
    let helper_path = bundled_privileged_helper_path()
        .ok_or_else(|| "bundled aetower-helper was not found".to_owned())?;
    let mut args = vec!["process-action".to_owned(), plan.signal.clone()];
    args.extend(plan.target_pids.iter().map(u32::to_string));
    let output = Command::new(&helper_path)
        .args(&args)
        .output()
        .map_err(|error| format!("helper execution failed: {error}"))?;
    let stdout = bounded_command_text(&output.stdout);
    let stderr = bounded_command_text(&output.stderr);
    if !output.status.success() {
        return Err(if stderr.is_empty() {
            format!("helper exited with status {}", output.status)
        } else {
            format!("helper exited with status {}: {stderr}", output.status)
        });
    }
    Ok(ProcessActionCommandResult {
        exit_status: output.status.code(),
        stdout,
        stderr,
    })
}

fn bundled_privileged_helper_path() -> Option<std::path::PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let executable_dir = executable.parent()?;
    let sibling = executable_dir.join("aetower-helper");
    if sibling.is_file() {
        return Some(sibling);
    }
    let app_helper = executable_dir
        .parent()
        .map(|contents| contents.join("Helpers").join("aetower-helper"))?;
    app_helper.is_file().then_some(app_helper)
}

fn bounded_command_text(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).trim().to_owned();
    if text.len() > PROCESS_ACTION_COMMAND_OUTPUT_LIMIT {
        text.truncate(PROCESS_ACTION_COMMAND_OUTPUT_LIMIT);
        text.push_str("...");
    }
    text
}

fn should_delay_process_action_verification(action: &str, command_success: bool) -> bool {
    command_success
        && matches!(
            action,
            "terminate"
                | "force-kill"
                | "terminate-tree"
                | "force-kill-tree"
                | "suspend"
                | "resume"
                | "lower-priority"
                | "normal-priority"
        )
}

fn process_action_target_outcomes(
    action: &str,
    signal: &str,
    target_pids: &[u32],
    before_states: &BTreeMap<u32, ProcessActionTargetState>,
    after_states: &BTreeMap<u32, ProcessActionTargetState>,
    command_success: bool,
) -> (String, Vec<ProcessActionTargetOutcome>) {
    let outcomes = target_pids
        .iter()
        .copied()
        .map(|pid| {
            let before = before_states
                .get(&pid)
                .cloned()
                .unwrap_or(ProcessActionTargetState {
                    visible: false,
                    status: None,
                    nice: None,
                });
            let after = after_states
                .get(&pid)
                .cloned()
                .unwrap_or(ProcessActionTargetState {
                    visible: false,
                    status: None,
                    nice: None,
                });
            let (verification, detail) =
                target_action_verification(action, signal, &after, command_success);
            ProcessActionTargetOutcome {
                pid,
                visible_before: before.visible,
                visible_after: Some(after.visible),
                nice_before: before.nice,
                status_after: after.status,
                nice_after: after.nice,
                verification,
                detail,
            }
        })
        .collect::<Vec<_>>();
    (
        process_action_verification_summary(action, command_success, &outcomes),
        outcomes,
    )
}

fn process_action_follow_up_checks(
    action: &str,
    signal: &str,
    target_pids: &[u32],
    before_states: &BTreeMap<u32, ProcessActionTargetState>,
    command_success: bool,
) -> Vec<ProcessActionFollowUpCheck> {
    if !command_success || !matches!(action, "terminate" | "terminate-tree") {
        return Vec::new();
    }

    let mut checks = Vec::new();
    for (sleep_millis, delay_millis) in [(2_000_u64, 2_000_u64), (8_000_u64, 10_000_u64)] {
        std::thread::sleep(std::time::Duration::from_millis(sleep_millis));
        let after_states = collect_process_action_target_states(target_pids);
        let (verification, target_outcomes) = process_action_target_outcomes(
            action,
            signal,
            target_pids,
            before_states,
            &after_states,
            command_success,
        );
        checks.push(ProcessActionFollowUpCheck {
            delay_millis,
            verification,
            target_outcomes,
        });
        if checks
            .last()
            .is_some_and(|check| check.verification == "verified-exited")
        {
            break;
        }
    }
    checks
}

fn target_action_verification(
    action: &str,
    signal: &str,
    after: &ProcessActionTargetState,
    command_success: bool,
) -> (String, String) {
    if !command_success {
        return (
            "command-failed".to_owned(),
            "macOS did not accept the command; no effect was verified.".to_owned(),
        );
    }

    match action {
        "terminate" | "terminate-tree" => {
            if target_is_effectively_exited(after) {
                (
                    "exited".to_owned(),
                    if status_is_zombie(after.status.as_deref()) {
                        "No longer running after TERM; ps reports zombie state until its parent reaps it."
                            .to_owned()
                    } else {
                        "No longer visible after TERM.".to_owned()
                    },
                )
            } else {
                (
                    "still-visible".to_owned(),
                    "Still visible after TERM; it may still be shutting down or ignoring the signal."
                        .to_owned(),
                )
            }
        }
        "force-kill" | "force-kill-tree" => {
            if target_is_effectively_exited(after) {
                (
                    "exited".to_owned(),
                    if status_is_zombie(after.status.as_deref()) {
                        "No longer running after KILL; ps reports zombie state until its parent reaps it."
                            .to_owned()
                    } else {
                        "No longer visible after KILL.".to_owned()
                    },
                )
            } else {
                (
                    "still-visible".to_owned(),
                    "Still visible after KILL; it may be uninterruptible or PID-reused.".to_owned(),
                )
            }
        }
        "suspend" => {
            if !after.visible {
                (
                    "not-visible".to_owned(),
                    "Target disappeared after STOP.".to_owned(),
                )
            } else if status_is_stopped(after.status.as_deref()) {
                (
                    "suspended".to_owned(),
                    "ps reports stopped state.".to_owned(),
                )
            } else {
                (
                    "not-suspended".to_owned(),
                    "Target is still visible but ps does not report stopped state.".to_owned(),
                )
            }
        }
        "resume" => {
            if !after.visible {
                (
                    "not-visible".to_owned(),
                    "Target disappeared after CONT.".to_owned(),
                )
            } else if status_is_stopped(after.status.as_deref()) {
                (
                    "still-suspended".to_owned(),
                    "Target is still visible and ps still reports stopped state.".to_owned(),
                )
            } else {
                (
                    "running".to_owned(),
                    "Target is visible and ps no longer reports stopped state.".to_owned(),
                )
            }
        }
        "lower-priority" => {
            if !after.visible {
                (
                    "not-visible".to_owned(),
                    "Target disappeared after renice.".to_owned(),
                )
            } else if after.nice.is_some_and(|nice| nice >= 10) {
                (
                    "priority-lowered".to_owned(),
                    format!("ps reports nice value {}.", after.nice.unwrap_or_default()),
                )
            } else {
                (
                    "priority-not-confirmed".to_owned(),
                    "renice was accepted but the expected nice value was not confirmed.".to_owned(),
                )
            }
        }
        "normal-priority" => {
            if !after.visible {
                (
                    "not-visible".to_owned(),
                    "Target disappeared after renice.".to_owned(),
                )
            } else if after.nice.is_some_and(|nice| nice <= 0) {
                (
                    "priority-normal".to_owned(),
                    format!("ps reports nice value {}.", after.nice.unwrap_or_default()),
                )
            } else {
                (
                    "priority-not-confirmed".to_owned(),
                    "renice was accepted but normal priority was not confirmed.".to_owned(),
                )
            }
        }
        _ => (
            "command-accepted".to_owned(),
            format!("Command accepted for {signal}."),
        ),
    }
}

fn process_action_verification_summary(
    action: &str,
    command_success: bool,
    outcomes: &[ProcessActionTargetOutcome],
) -> String {
    if !command_success {
        return "command-failed".to_owned();
    }
    match action {
        "terminate" | "force-kill" | "terminate-tree" | "force-kill-tree" => {
            if outcomes
                .iter()
                .all(|outcome| outcome.verification == "exited")
            {
                "verified-exited".to_owned()
            } else {
                "targets-still-visible".to_owned()
            }
        }
        "suspend" => {
            if outcomes
                .iter()
                .all(|outcome| outcome.verification == "suspended")
            {
                "verified-suspended".to_owned()
            } else {
                "suspend-not-confirmed".to_owned()
            }
        }
        "resume" => {
            if outcomes
                .iter()
                .all(|outcome| outcome.verification == "running")
            {
                "verified-running".to_owned()
            } else {
                "resume-not-confirmed".to_owned()
            }
        }
        "lower-priority" => {
            if outcomes
                .iter()
                .all(|outcome| outcome.verification == "priority-lowered")
            {
                "verified-priority".to_owned()
            } else {
                "priority-not-confirmed".to_owned()
            }
        }
        "normal-priority" => {
            if outcomes
                .iter()
                .all(|outcome| outcome.verification == "priority-normal")
            {
                "verified-priority".to_owned()
            } else {
                "priority-not-confirmed".to_owned()
            }
        }
        _ => "command-accepted".to_owned(),
    }
}

fn process_action_verification_sentence(verification: &str) -> &'static str {
    match verification {
        "verified-exited" => "Aetower verified the target is no longer visible.",
        "targets-still-visible" => {
            "Aetower sent the signal, but at least one target is still visible."
        }
        "verified-suspended" => "Aetower verified stopped process state.",
        "suspend-not-confirmed" => "Aetower could not confirm stopped process state.",
        "verified-running" => "Aetower verified running process state.",
        "resume-not-confirmed" => "Aetower could not confirm resumed process state.",
        "verified-priority" => "Aetower verified the requested nice value.",
        "priority-not-confirmed" => "Aetower could not confirm the requested nice value.",
        _ => "Aetower captured bounded post-action verification.",
    }
}

fn status_is_stopped(status: Option<&str>) -> bool {
    status.is_some_and(|status| status.contains('T'))
}

fn status_is_zombie(status: Option<&str>) -> bool {
    status.is_some_and(|status| status.contains('Z'))
}

fn target_is_effectively_exited(after: &ProcessActionTargetState) -> bool {
    !after.visible || status_is_zombie(after.status.as_deref())
}

fn process_nice_value(pid: u32) -> Option<i32> {
    let pid_argument = pid.to_string();
    let output = Command::new("/bin/ps")
        .args(["-p", pid_argument.as_str(), "-o", "ni="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i32>()
        .ok()
}

pub(crate) fn entity_process_ids(entity: &aetower_model::EntitySnapshot) -> Vec<u32> {
    let mut process_ids = entity
        .components
        .iter()
        .filter_map(|component| {
            (component.kind != aetower_model::ComponentKind::AdapterContext)
                .then_some(component.process_id)
                .flatten()
        })
        .collect::<Vec<_>>();
    process_ids.sort_unstable();
    process_ids.dedup();
    process_ids
}

pub(crate) fn process_component_context(
    snapshot: &SystemSnapshot,
    pid: u32,
) -> Option<ProcessComponentContext> {
    for entity in &snapshot.entities {
        for component in &entity.components {
            if component.process_id != Some(pid) {
                continue;
            }
            let sibling_process_count = entity_process_ids(entity).len() as u32;
            return Some(ProcessComponentContext {
                entity_id: entity.entity_id.clone(),
                display_name: entity.display_name.clone(),
                component_title: component.title.clone(),
                component_kind: format!("{:?}", component.kind),
                executable_path: component.executable_path.clone(),
                command_line: component.command_line.clone(),
                cwd: component.cwd.clone(),
                user: component.user.clone(),
                parent_summary: component.parent_summary.clone(),
                cpu_percent: component.cpu_percent,
                memory_bytes: component.memory_bytes,
                memory_physical_footprint_bytes: component.memory_physical_footprint_bytes,
                start_time_millis: component.start_time_millis,
                sibling_process_count,
            });
        }
    }
    None
}

pub(crate) fn process_child_pids(snapshot: &SystemSnapshot, pid: u32) -> Vec<u32> {
    let mut child_pids = snapshot
        .entities
        .iter()
        .flat_map(|entity| &entity.components)
        .filter(|component| extract_parent_pid(component.parent_summary.as_deref()) == Some(pid))
        .filter_map(|component| component.process_id)
        .collect::<Vec<_>>();
    child_pids.sort_unstable();
    child_pids.dedup();
    child_pids
}

pub(crate) fn validate_pid(pid: u32) -> Result<(), String> {
    if pid <= 1 {
        Err(format!(
            "Refusing to inspect or signal protected pid {pid}."
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn process_exists(pid: u32) -> bool {
    Command::new("/bin/ps")
        .arg("-p")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn process_ps_summary(pid: u32) -> Result<ProcessPsSummary, String> {
    let output = run_os_command(
        "/bin/ps",
        &[
            "-p".to_owned(),
            pid.to_string(),
            "-o".to_owned(),
            "ppid=".to_owned(),
            "-o".to_owned(),
            "user=".to_owned(),
            "-o".to_owned(),
            "stat=".to_owned(),
            "-o".to_owned(),
            "pcpu=".to_owned(),
            "-o".to_owned(),
            "rss=".to_owned(),
            "-o".to_owned(),
            "command=".to_owned(),
        ],
    )?;
    parse_ps_summary(&output)
}

pub(crate) fn read_process_bundle_info(executable_path: &str) -> Option<ProcessBundleInfo> {
    let bundle_path = containing_app_bundle(executable_path)?;
    let info_plist = bundle_path.join("Contents/Info.plist");
    Some(ProcessBundleInfo {
        bundle_id: plist_value(&info_plist, "CFBundleIdentifier"),
        bundle_path: Some(bundle_path.to_string_lossy().to_string()),
        name: plist_value(&info_plist, "CFBundleName")
            .or_else(|| plist_value(&info_plist, "CFBundleDisplayName")),
        short_version: plist_value(&info_plist, "CFBundleShortVersionString"),
        version: plist_value(&info_plist, "CFBundleVersion"),
    })
}

pub(crate) fn read_process_signature(executable_path: &str) -> ProcessSignatureInfo {
    let output = Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4", executable_path])
        .output();
    let Ok(output) = output else {
        return ProcessSignatureInfo {
            signed: false,
            signing_id: None,
            team_id: None,
            authority: Vec::new(),
            notarized: None,
            classification: "unknown".to_owned(),
            is_adhoc: false,
            note: Some("codesign could not be executed.".to_owned()),
        };
    };

    let details = String::from_utf8_lossy(&output.stderr);
    let authority = details
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Authority="))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let signed = output.status.success();
    let (classification, is_adhoc) = classify_signature(signed, &authority);
    ProcessSignatureInfo {
        signed,
        signing_id: parse_codesign_field(&details, "Identifier="),
        team_id: parse_codesign_field(&details, "TeamIdentifier="),
        authority,
        notarized: signed.then(|| spctl_accepts_executable(executable_path)),
        classification,
        is_adhoc,
        note: (!signed).then(|| {
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("codesign did not accept this executable.")
                .trim()
                .to_owned()
        }),
    }
}

pub(crate) fn read_process_entitlements(executable_path: &str) -> Vec<String> {
    let output = Command::new("/usr/bin/codesign")
        .args(["-d", "--entitlements", ":-", executable_path])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut entitlements = text
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("<key>")
                .and_then(|value| value.strip_suffix("</key>"))
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    entitlements.sort();
    entitlements.dedup();
    entitlements
}

/// Read a same-user process's environment via `KERN_PROCARGS2`, redacting
/// values whose key or shape looks like a secret. Other-user/system processes
/// return empty with a friendly note (no elevation in this build).
pub(crate) fn read_process_environment(pid: u32) -> (Vec<EnvVarEntry>, Option<String>) {
    const MAX_ENV: usize = 512;
    match procargs_environment(pid) {
        Ok(pairs) => {
            let truncated = pairs.len() > MAX_ENV;
            let mut redacted_count = 0usize;
            let entries = pairs
                .into_iter()
                .take(MAX_ENV)
                .map(|(key, value)| {
                    if value_is_secret(&key, &value) {
                        redacted_count += 1;
                        EnvVarEntry {
                            key,
                            value: format!("‹redacted · {} chars›", value.chars().count()),
                        }
                    } else {
                        EnvVarEntry { key, value }
                    }
                })
                .collect::<Vec<_>>();
            let mut notes = Vec::new();
            if truncated {
                notes.push(format!("showing the first {MAX_ENV}"));
            }
            if redacted_count > 0 {
                notes.push(format!(
                    "{redacted_count} value(s) redacted as likely secrets"
                ));
            }
            let note = if entries.is_empty() {
                Some("No environment variables were readable for this process.".to_owned())
            } else if notes.is_empty() {
                None
            } else {
                Some(notes.join("; "))
            };
            (entries, note)
        }
        Err(message) => (Vec::new(), Some(message)),
    }
}

/// Heuristic secret detection: redact by key name (the common case) or by a few
/// well-known value prefixes (OpenAI `sk-`, GitHub `ghp_`, AWS `AKIA`, PEM).
fn value_is_secret(key: &str, value: &str) -> bool {
    const SECRET_KEY_MARKERS: [&str; 11] = [
        "KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "AUTH",
        "SESSION",
        "CREDENTIAL",
        "PRIVATE",
        "APIKEY",
        "ACCESS_KEY",
    ];
    let upper_key = key.to_ascii_uppercase();
    if SECRET_KEY_MARKERS
        .iter()
        .any(|marker| upper_key.contains(marker))
    {
        return true;
    }
    let trimmed = value.trim_start();
    ["sk-", "ghp_", "gho_", "github_pat_", "AKIA", "-----BEGIN"]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

const CTL_KERN: libc::c_int = 1;
const KERN_ARGMAX: libc::c_int = 8;
const KERN_PROCARGS2: libc::c_int = 49;

/// Pull another process's argv+environment buffer from `KERN_PROCARGS2`. Works
/// for same-user processes without elevation; otherwise the kernel returns an
/// error we translate into a friendly message.
fn procargs_environment(pid: u32) -> Result<Vec<(String, String)>, String> {
    let mut argmax: libc::c_int = 0;
    let mut argmax_size = std::mem::size_of::<libc::c_int>();
    let mut argmax_mib = [CTL_KERN, KERN_ARGMAX];
    let argmax_rc = unsafe {
        libc::sysctl(
            argmax_mib.as_mut_ptr(),
            argmax_mib.len() as libc::c_uint,
            &mut argmax as *mut _ as *mut libc::c_void,
            &mut argmax_size,
            std::ptr::null_mut(),
            0,
        )
    };
    if argmax_rc != 0 || argmax <= 0 {
        return Err("Could not determine the process argument buffer size.".to_owned());
    }

    let mut buffer = vec![0u8; argmax as usize];
    let mut buffer_size = buffer.len();
    let mut mib = [CTL_KERN, KERN_PROCARGS2, pid as libc::c_int];
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            buffer.as_mut_ptr() as *mut libc::c_void,
            &mut buffer_size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(
            "Environment variables require same-user ownership (elevation is not enabled in this build)."
                .to_owned(),
        );
    }
    buffer.truncate(buffer_size);
    Ok(parse_procargs2(&buffer))
}

/// Parse a `KERN_PROCARGS2` buffer: `[argc:i32][exec_path\0][\0..][argv\0..][env\0..]`.
pub(crate) fn parse_procargs2(buffer: &[u8]) -> Vec<(String, String)> {
    if buffer.len() < 4 {
        return Vec::new();
    }
    let argc = i32::from_ne_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]).max(0) as usize;
    let mut pos = 4;
    // Skip the executable path string.
    while pos < buffer.len() && buffer[pos] != 0 {
        pos += 1;
    }
    // Skip the NUL padding between exec path and argv.
    while pos < buffer.len() && buffer[pos] == 0 {
        pos += 1;
    }
    // Skip argc argument strings.
    let mut args_seen = 0;
    while args_seen < argc && pos < buffer.len() {
        while pos < buffer.len() && buffer[pos] != 0 {
            pos += 1;
        }
        pos += 1; // consume the NUL terminator
        args_seen += 1;
    }
    // The remainder is KEY=VALUE environment entries until an empty string.
    let mut envs = Vec::new();
    while pos < buffer.len() {
        let start = pos;
        while pos < buffer.len() && buffer[pos] != 0 {
            pos += 1;
        }
        let bytes = &buffer[start..pos];
        pos += 1; // consume the NUL terminator
        if bytes.is_empty() {
            break;
        }
        if let Ok(text) = std::str::from_utf8(bytes)
            && let Some((key, value)) = text.split_once('=')
        {
            envs.push((key.to_owned(), value.to_owned()));
        }
    }
    envs
}

pub(crate) fn lookup_startup_entry(executable_path: &str) -> Option<StartupEntryInfo> {
    for (kind, directory) in [
        ("launch-agent", "/Library/LaunchAgents"),
        ("launch-daemon", "/Library/LaunchDaemons"),
        ("user-launch-agent", "~/Library/LaunchAgents"),
    ] {
        let directory = directory.replace('~', &std::env::var("HOME").unwrap_or_default());
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("plist") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            if contents.contains(executable_path) {
                return Some(StartupEntryInfo {
                    kind: kind.to_owned(),
                    label: plist_value(&path, "Label"),
                    plist_path: path.to_string_lossy().to_string(),
                });
            }
        }
    }
    None
}

fn containing_app_bundle(executable_path: &str) -> Option<std::path::PathBuf> {
    std::path::Path::new(executable_path)
        .ancestors()
        .find(|path| path.extension().and_then(|extension| extension.to_str()) == Some("app"))
        .map(std::path::Path::to_path_buf)
}

pub(crate) fn plist_value(plist_path: &std::path::Path, key: &str) -> Option<String> {
    if !plist_path.exists() {
        return None;
    }
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}")])
        .arg(plist_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn parse_codesign_field(details: &str, prefix: &str) -> Option<String> {
    details
        .lines()
        .find_map(|line| line.trim().strip_prefix(prefix))
        .map(str::to_owned)
}

/// Enumerate dynamic libraries mapped into a process via `lsof` (reusing the
/// open-resources command), classify each system vs third-party, and flag any
/// listed in the process's `DYLD_INSERT_LIBRARIES` as injected.
pub(crate) fn read_process_dylibs(
    pid: u32,
    dyld_insert: &str,
) -> (Vec<ProcessDylib>, DylibSummary) {
    let injected = dyld_insert
        .split(':')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    match run_os_command(
        "/usr/sbin/lsof",
        &["-nP".to_owned(), "-p".to_owned(), pid.to_string()],
    ) {
        Ok(output) => parse_dylibs_from_lsof(&output, &injected),
        Err(_) => (Vec::new(), DylibSummary::default()),
    }
}

pub(crate) fn parse_dylibs_from_lsof(
    output: &str,
    injected: &[&str],
) -> (Vec<ProcessDylib>, DylibSummary) {
    const MAX_DYLIBS: usize = 400;
    let mut seen = std::collections::BTreeSet::new();
    let mut dylibs = Vec::new();
    for line in output.lines().skip(1) {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 9 {
            continue;
        }
        let path = parts[8..].join(" ");
        if !(path.ends_with(".dylib") || path.contains(".framework/")) {
            continue;
        }
        if !seen.insert(path.clone()) {
            continue;
        }
        let category = if path.starts_with("/System/")
            || path.starts_with("/usr/lib/")
            || path.starts_with("/Library/Apple/")
        {
            "system"
        } else {
            "third_party"
        };
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        dylibs.push(ProcessDylib {
            injected: injected.iter().any(|entry| *entry == path),
            path,
            name,
            category: category.to_owned(),
        });
        if dylibs.len() >= MAX_DYLIBS {
            break;
        }
    }
    dylibs.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then(left.name.cmp(&right.name))
    });
    let summary = DylibSummary {
        total: dylibs.len() as u32,
        third_party: dylibs
            .iter()
            .filter(|d| d.category == "third_party")
            .count() as u32,
        injected: dylibs.iter().filter(|d| d.injected).count() as u32,
    };
    (dylibs, summary)
}

fn spctl_accepts_executable(executable_path: &str) -> bool {
    Command::new("/usr/sbin/spctl")
        .args(["--assess", "--type", "execute", executable_path])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn parse_ps_summary(output: &str) -> Result<ProcessPsSummary, String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "ps returned no process rows".to_owned())?;
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 5 {
        return Err(format!("ps row had too few columns: {line}"));
    }
    Ok(ProcessPsSummary {
        parent_pid: parts[0].parse::<u32>().ok(),
        user: Some(parts[1].to_owned()),
        status: Some(parts[2].to_owned()),
        cpu_percent: parts[3].parse::<f32>().ok(),
        resident_bytes: parts[4]
            .parse::<u64>()
            .ok()
            .map(|rss_kib| rss_kib.saturating_mul(1024)),
        command: (parts.len() > 5).then(|| parts[5..].join(" ")),
    })
}

pub(crate) fn process_action_plan(
    snapshot: Option<&SystemSnapshot>,
    pid: u32,
    action: &str,
) -> Result<ProcessActionPlan, String> {
    match action.trim().to_ascii_lowercase().as_str() {
        "terminate" | "term" | "sigterm" => Ok(signal_process_action_plan(
            "terminate",
            "TERM",
            vec![pid],
            "Dry run only; no terminate signal was sent.",
            format!("Sent TERM to process {pid}."),
        )),
        "force-kill" | "force_kill" | "kill" | "sigkill" => Ok(signal_process_action_plan(
            "force-kill",
            "KILL",
            vec![pid],
            "Dry run only; no force-kill signal was sent.",
            format!("Sent KILL to process {pid}."),
        )),
        "suspend" | "stop" | "sigstop" => Ok(signal_process_action_plan(
            "suspend",
            "STOP",
            vec![pid],
            "Dry run only; no suspend signal was sent.",
            format!("Sent STOP to process {pid}."),
        )),
        "resume" | "continue" | "cont" | "sigcont" => Ok(signal_process_action_plan(
            "resume",
            "CONT",
            vec![pid],
            "Dry run only; no resume signal was sent.",
            format!("Sent CONT to process {pid}."),
        )),
        "terminate-tree" | "tree-terminate" | "sigterm-tree" => Ok(signal_process_action_plan(
            "terminate-tree",
            "TERM",
            process_tree_target_pids(snapshot, pid),
            "Dry run only; no terminate-tree signal was sent.",
            format!("Sent TERM to process tree rooted at {pid}."),
        )),
        "force-kill-tree" | "tree-kill" | "tree-force-kill" | "sigkill-tree" => {
            Ok(signal_process_action_plan(
                "force-kill-tree",
                "KILL",
                process_tree_target_pids(snapshot, pid),
                "Dry run only; no force-kill-tree signal was sent.",
                format!("Sent KILL to process tree rooted at {pid}."),
            ))
        }
        "lower-priority" | "renice-background" | "background" => Ok(renice_process_action_plan(
            "lower-priority",
            10,
            pid,
            "Dry run only; priority was not changed.",
            format!("Requested lower priority for process {pid} with nice value 10."),
        )),
        "normal-priority" | "restore-priority" | "renice-normal" => Ok(renice_process_action_plan(
            "normal-priority",
            0,
            pid,
            "Dry run only; priority was not changed.",
            format!("Requested normal priority for process {pid} with nice value 0."),
        )),
        _ => Err(format!(
            "Unsupported process action '{action}'. Use terminate, force-kill, suspend, resume, terminate-tree, force-kill-tree, lower-priority, or normal-priority."
        )),
    }
}

pub(crate) fn signal_process_action_plan(
    normalized_action: &str,
    signal: &str,
    target_pids: Vec<u32>,
    dry_run_message: &str,
    success_message: String,
) -> ProcessActionPlan {
    let mut args = vec![format!("-{signal}")];
    args.extend(target_pids.iter().map(u32::to_string));
    ProcessActionPlan {
        normalized_action: normalized_action.to_owned(),
        signal: signal.to_owned(),
        command: format!("/bin/kill {}", args.join(" ")),
        program: "/bin/kill".to_owned(),
        args,
        target_pids,
        dry_run_message: dry_run_message.to_owned(),
        success_message,
    }
}

pub(crate) fn renice_process_action_plan(
    normalized_action: &str,
    nice_value: i32,
    pid: u32,
    dry_run_message: &str,
    success_message: String,
) -> ProcessActionPlan {
    let args = vec![nice_value.to_string(), "-p".to_owned(), pid.to_string()];
    ProcessActionPlan {
        normalized_action: normalized_action.to_owned(),
        signal: format!("renice:{nice_value}"),
        command: format!("/usr/bin/renice {}", args.join(" ")),
        program: "/usr/bin/renice".to_owned(),
        args,
        target_pids: vec![pid],
        dry_run_message: dry_run_message.to_owned(),
        success_message,
    }
}

pub(crate) fn process_tree_target_pids(snapshot: Option<&SystemSnapshot>, pid: u32) -> Vec<u32> {
    let mut target_pids = snapshot
        .map(|snapshot| process_descendant_pids(snapshot, pid))
        .unwrap_or_default();
    target_pids.push(pid);
    target_pids.sort_unstable();
    target_pids.dedup();
    target_pids.retain(|target_pid| *target_pid != 0);
    target_pids
}

pub(crate) fn process_descendant_pids(snapshot: &SystemSnapshot, pid: u32) -> Vec<u32> {
    let mut descendants = Vec::new();
    let mut stack = vec![pid];
    while let Some(parent_pid) = stack.pop() {
        for child_pid in process_child_pids(snapshot, parent_pid) {
            if descendants.contains(&child_pid) {
                continue;
            }
            descendants.push(child_pid);
            stack.push(child_pid);
        }
    }
    descendants
}

pub(crate) fn process_action_diagnostics_event(report: &ProcessActionReport) -> DiagnosticsEvent {
    let mut builder = DiagnosticsEvent::builder(
        if report.success && process_action_verification_is_confirmed(&report.verification) {
            DiagnosticsLevel::Info
        } else {
            DiagnosticsLevel::Warn
        },
        DiagnosticsSubsystem::Engine,
        "process-action",
        report.message.clone(),
    )
    .field("action_id", report.action_id.clone())
    .field("pid", report.pid)
    .field(
        "target_pids",
        report
            .target_pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(","),
    )
    .field("target_count", report.target_pids.len())
    .field("action", report.action.clone())
    .field("signal", report.signal.clone())
    .field("success", report.success)
    .field("command", report.command.clone())
    .field("verification", report.verification.clone())
    .field("blast_radius", report.blast_radius.target_count)
    .field(
        "privileged_helper_status",
        report.privileged_helper_status.clone(),
    );
    if let Some(command_result) = report.command_result.as_ref() {
        if let Some(exit_status) = command_result.exit_status {
            builder = builder.field("exit_status", exit_status);
        }
        if !command_result.stderr.is_empty() {
            builder = builder.field("stderr", command_result.stderr.clone());
        }
    }
    if let Some(reason) = report.reason.as_ref() {
        builder = builder.field("reason", reason);
    }
    if let Some(entity_id) = report.entity_id.as_ref() {
        builder = builder.entity_id(entity_id.clone());
    }
    if let Some(display_name) = report.display_name.as_ref() {
        builder = builder.field("display_name", display_name);
    }
    builder.build()
}

fn process_action_verification_is_confirmed(verification: &str) -> bool {
    matches!(
        verification,
        "verified-exited"
            | "verified-suspended"
            | "verified-running"
            | "verified-priority"
            | "command-accepted"
            | "preview"
    )
}

pub(crate) fn process_action_history_item(event: DiagnosticsEvent) -> ProcessActionHistoryItem {
    let display_name = diagnostics_field(&event, "display_name").map(str::to_owned);
    ProcessActionHistoryItem {
        timestamp_millis: event.timestamp_millis,
        action_id: diagnostics_field(&event, "action_id").map(str::to_owned),
        pid: diagnostics_field(&event, "pid").and_then(|value| value.parse::<u32>().ok()),
        target_pids: diagnostics_field(&event, "target_pids")
            .map(parse_pid_list)
            .unwrap_or_default(),
        action: diagnostics_field(&event, "action").map(str::to_owned),
        signal: diagnostics_field(&event, "signal").map(str::to_owned),
        success: diagnostics_field(&event, "success")
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(
                event.level != DiagnosticsLevel::Warn && event.level != DiagnosticsLevel::Error,
            ),
        verification: diagnostics_field(&event, "verification").map(str::to_owned),
        exit_status: diagnostics_field(&event, "exit_status").and_then(|value| value.parse().ok()),
        stderr: diagnostics_field(&event, "stderr").map(str::to_owned),
        blast_radius: diagnostics_field(&event, "blast_radius")
            .and_then(|value| value.parse().ok()),
        privileged_helper_status: diagnostics_field(&event, "privileged_helper_status")
            .map(str::to_owned),
        reason: diagnostics_field(&event, "reason").map(str::to_owned),
        entity_id: event.entity_id,
        display_name,
        message: event.message,
    }
}

pub(crate) fn parse_pid_list(value: &str) -> Vec<u32> {
    value
        .split(',')
        .filter_map(|part| part.trim().parse::<u32>().ok())
        .collect()
}

pub(crate) fn diagnostics_field<'a>(event: &'a DiagnosticsEvent, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.value.as_str())
}

pub(crate) fn run_os_command(program: &str, args: &[String]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("run {}: {error}", program))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if stderr.is_empty() {
            format!("{} exited with status {}", program, output.status)
        } else {
            format!("{} failed: {}", program, stderr)
        });
    }
    String::from_utf8(output.stdout).map_err(|error| format!("decode {} output: {error}", program))
}

pub(crate) fn extract_tool_error_message(value: &Value) -> String {
    value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("tool request failed")
        .to_owned()
}

pub(crate) fn format_metric_value(metric: &str, value: f64) -> String {
    match metric {
        "memory_bytes" => format_bytes(value.max(0.0).round() as u64),
        "disk_activity_bps" | "network_activity_bps" => {
            format!("{}/s", format_bytes(value.max(0.0).round() as u64))
        }
        _ => format!("{value:.1}"),
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    #[test]
    fn parse_procargs2_extracts_env_after_argv() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&1i32.to_ne_bytes()); // argc = 1
        buffer.extend_from_slice(b"/bin/exec\0"); // executable path
        buffer.extend_from_slice(b"/bin/exec\0"); // argv[0]
        buffer.extend_from_slice(b"FOO=bar\0");
        buffer.extend_from_slice(b"SECRET_KEY=zzz\0");
        buffer.extend_from_slice(b"\0"); // empty string terminates env

        let envs = parse_procargs2(&buffer);
        assert_eq!(
            envs,
            vec![
                ("FOO".to_owned(), "bar".to_owned()),
                ("SECRET_KEY".to_owned(), "zzz".to_owned()),
            ]
        );
    }

    #[test]
    fn parse_procargs2_handles_truncated_buffer() {
        assert!(parse_procargs2(&[]).is_empty());
        assert!(parse_procargs2(&[0, 1]).is_empty());
    }

    #[test]
    fn value_is_secret_flags_keys_and_known_prefixes() {
        assert!(value_is_secret("AWS_SECRET_ACCESS_KEY", "x"));
        assert!(value_is_secret("GITHUB_TOKEN", "x"));
        assert!(value_is_secret("anything", "sk-abc123"));
        assert!(value_is_secret("anything", "ghp_abc"));
        assert!(!value_is_secret("PATH", "/usr/bin:/bin"));
        assert!(!value_is_secret("HOME", "/Users/someone"));
    }

    #[test]
    fn classify_signature_distinguishes_authorities() {
        let dev_id = vec![
            "Developer ID Application: Acme Inc (TEAMID)".to_owned(),
            "Developer ID Certification Authority".to_owned(),
            "Apple Root CA".to_owned(),
        ];
        assert_eq!(
            classify_signature(true, &dev_id),
            ("developer_id".to_owned(), false)
        );

        let mas = vec![
            "Apple Mac OS Application Signing".to_owned(),
            "Apple Worldwide Developer Relations Certification Authority".to_owned(),
            "Apple Root CA".to_owned(),
        ];
        assert_eq!(
            classify_signature(true, &mas),
            ("mac_app_store".to_owned(), false)
        );

        let apple = vec![
            "Software Signing".to_owned(),
            "Apple Code Signing Certification Authority".to_owned(),
            "Apple Root CA".to_owned(),
        ];
        assert_eq!(
            classify_signature(true, &apple),
            ("apple".to_owned(), false)
        );

        assert_eq!(classify_signature(true, &[]), ("adhoc".to_owned(), true));
        assert_eq!(
            classify_signature(false, &[]),
            ("unsigned".to_owned(), false)
        );
    }

    #[test]
    fn parse_dylibs_classifies_and_flags_injected() {
        let output = "COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\n\
            App 100 alice txt REG 1,4 100 200 /usr/lib/libSystem.B.dylib\n\
            App 100 alice txt REG 1,4 100 200 /opt/acme/libplugin.dylib\n\
            App 100 alice txt REG 1,4 100 200 /tmp/inject.dylib\n\
            App 100 alice 1u IPv4 0x1 0 0t0 TCP 1.2.3.4:443\n";
        let injected = ["/tmp/inject.dylib"];
        let (dylibs, summary) = parse_dylibs_from_lsof(output, &injected);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.third_party, 2);
        assert_eq!(summary.injected, 1);
        assert!(
            dylibs.iter().any(|d| d.path == "/tmp/inject.dylib"
                && d.injected
                && d.category == "third_party")
        );
        assert!(
            dylibs
                .iter()
                .any(|d| d.path == "/usr/lib/libSystem.B.dylib" && d.category == "system")
        );
    }
}

#[cfg(test)]
mod resource_holder_tests {
    use super::*;

    #[test]
    fn parse_lsof_holders_extracts_command_pid_user_and_name() {
        // Header line is skipped; each data row -> one holder.
        let output = "\
COMMAND   PID  USER   FD   TYPE DEVICE SIZE/OFF NODE NAME
nginx    4321  root   6u  IPv4 0x1234      0t0  TCP *:443 (LISTEN)
ruby     8800  jane   12u IPv4 0x5678      0t0  TCP 127.0.0.1:443->127.0.0.1:5050 (ESTABLISHED)
";
        let holders = parse_lsof_holders(output);
        assert_eq!(holders.len(), 2);
        assert_eq!(holders[0].command, "nginx");
        assert_eq!(holders[0].pid, 4321);
        assert_eq!(holders[0].user, "root");
        assert_eq!(holders[0].resource_type, "IPv4");
        assert!(holders[0].name.contains("*:443"));
        assert_eq!(holders[1].pid, 8800);
        assert_eq!(holders[1].user, "jane");
    }

    #[test]
    fn parse_lsof_holder_line_rejects_non_numeric_pid_and_short_rows() {
        assert!(parse_lsof_holder_line("too few columns").is_none());
        assert!(
            parse_lsof_holder_line("cmd notapid user fd TYPE dev off node name").is_none(),
            "a non-numeric PID column must be rejected"
        );
    }

    #[test]
    fn resource_holders_by_port_rejects_zero() {
        assert!(build_resource_holders_by_port(0).is_err());
    }

    #[test]
    fn resource_holders_by_file_rejects_empty_and_control_chars() {
        assert!(build_resource_holders_by_file("   ").is_err());
        assert!(build_resource_holders_by_file("/tmp/a\u{0}b").is_err());
    }
}
