//! Process-domain report builders for the MCP server.

use aetower_model::SystemSnapshot;
#[cfg(test)]
use aetower_model::classify_signature;
use serde_json::Value;

use crate::*;

mod inspection;
mod memory;
mod process_action;
mod process_tree;
mod resources;
mod sampling;

pub use inspection::process_inspect_json;
pub(crate) use inspection::{
    build_process_inspection, plist_value, process_component_context, process_ps_summary,
    read_process_signature,
};
#[cfg(test)]
pub(crate) use inspection::{parse_dylibs_from_lsof, parse_procargs2, value_is_secret};
#[cfg(test)]
pub(crate) use memory::parse_vmmap_region_line;
pub(crate) use memory::{build_entity_memory_breakdown, vmmap_regions_for_processes};
pub use memory::{memory_breakdown_json, self_memory_attribution_json};
#[cfg(test)]
pub(crate) use process_action::{
    build_process_action, process_action_history_item, process_action_plan,
};
pub(crate) use process_action::{build_process_action_history, build_process_action_with_context};
pub use process_action::{process_action_history_json, process_action_json};
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
    process_exists_native(pid)
}

#[cfg(unix)]
fn process_exists_native(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_exists_native(pid: u32) -> bool {
    Command::new("/bin/ps")
        .arg("-p")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn process_nice_value(pid: u32) -> Option<i32> {
    process_nice_value_native(pid)
}

#[cfg(target_os = "macos")]
fn process_nice_value_native(pid: u32) -> Option<i32> {
    let pid = pid as libc::id_t;
    unsafe {
        *libc::__error() = 0;
        let value = libc::getpriority(libc::PRIO_PROCESS, pid);
        let errno = *libc::__error();
        if value == -1 && errno != 0 {
            None
        } else {
            Some(value)
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn process_nice_value_native(_pid: u32) -> Option<i32> {
    None
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
