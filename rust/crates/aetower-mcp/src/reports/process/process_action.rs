use std::{collections::BTreeMap, process::Command};

use aetower_model::SystemSnapshot;

use super::*;

const PROCESS_ACTION_VERIFICATION_DELAY_MILLIS: u64 = 180;
const PROCESS_ACTION_COMMAND_OUTPUT_LIMIT: usize = 4096;

#[derive(Debug, Clone)]
struct ProcessActionTargetState {
    visible: bool,
    status: Option<String>,
    nice: Option<i32>,
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
    let verification = final_process_action_verification(&verification, &follow_up_checks);
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
    let status = ps.as_ref().and_then(|summary| summary.status.clone());
    let nice = ps.as_ref().and_then(|summary| summary.nice_value);
    ProcessActionTargetState {
        visible,
        status,
        nice,
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

fn final_process_action_verification(
    immediate_verification: &str,
    follow_up_checks: &[ProcessActionFollowUpCheck],
) -> String {
    follow_up_checks
        .last()
        .map(|check| check.verification.clone())
        .unwrap_or_else(|| immediate_verification.to_owned())
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
