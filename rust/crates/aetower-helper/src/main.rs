use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
struct HelperSnapshot {
    processes: Vec<PrivilegedProcessSample>,
}

#[derive(Debug, Serialize)]
struct EndpointSecurityStatusSnapshot {
    supported: bool,
    helper_entitled: bool,
    running_as_root: bool,
    can_stream_events: bool,
    eslogger_path: Option<String>,
    detail: String,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct EndpointSecuritySample {
    status: EndpointSecurityStatusSnapshot,
    events: Vec<EndpointSecurityLifecycleEvent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EndpointSecurityLifecycleEvent {
    event_type: String,
    timestamp_millis: u64,
    process_path: Option<String>,
    pid: Option<u32>,
    parent_pid: Option<u32>,
    child_pid: Option<u32>,
    exit_code: Option<i32>,
    signal: Option<i32>,
}

#[derive(Debug, Serialize)]
struct PrivilegedProcessSample {
    pid: u32,
    process_name: String,
    executable_name: Option<String>,
    connections: Vec<String>,
}

#[derive(Debug, Default)]
struct WorkingSample {
    pid: u32,
    process_name: String,
    executable_name: Option<String>,
    connections: BTreeSet<String>,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "sample".to_owned());
    let command_args: Vec<String> = args.collect();
    match command.as_str() {
        "sample" => {
            let snapshot = collect_snapshot()?;
            println!("{}", serde_json::to_string(&snapshot)?);
            Ok(())
        }
        "esf-status" => {
            let snapshot = probe_endpoint_security_status()?;
            println!("{}", serde_json::to_string(&snapshot)?);
            Ok(())
        }
        "esf-sample" => {
            let sample = collect_endpoint_security_sample()?;
            println!("{}", serde_json::to_string(&sample)?);
            Ok(())
        }
        "fan-set" => {
            // `fan-set <fan_id> <rpm>` pins the fan to a minimum RPM via the
            // SMC `F<n>Mn` key. Requires root — the helper is expected to be
            // launched as root by the parent app (via SMJobBless or
            // equivalent). Errors land on stderr and non-zero exit code so
            // callers can surface them to the user.
            let snapshot = apply_fan_command(&command_args, FanCommand::Set)?;
            println!("{}", serde_json::to_string(&snapshot)?);
            Ok(())
        }
        "fan-reset" => {
            // `fan-reset <fan_id>` restores automatic (OS-controlled) mode
            // for the specified fan by writing the original min RPM back.
            let snapshot = apply_fan_command(&command_args, FanCommand::Reset)?;
            println!("{}", serde_json::to_string(&snapshot)?);
            Ok(())
        }
        other => {
            bail!("unsupported command: {other}")
        }
    }
}

/// Outcome of an SMC fan control write, returned as JSON on stdout so the
/// parent app can distinguish success from a root-missing error.
#[derive(Debug, Serialize)]
struct FanControlResult {
    fan_id: u8,
    command: &'static str,
    success: bool,
    requested_rpm: Option<f32>,
}

enum FanCommand {
    Set,
    Reset,
}

/// Parse and execute a fan-set/fan-reset command.
///
/// Validation is intentionally simple: the helper trusts the parent app to
/// have already checked the RPM against each fan's reported min/max. The
/// helper's job is to translate the CLI arguments into an `aetower-sensors`
/// call and surface any SMC-level failure.
fn apply_fan_command(args: &[String], command: FanCommand) -> Result<FanControlResult> {
    let fan_id: u8 = args
        .first()
        .context("missing fan_id argument")?
        .parse()
        .context("fan_id must be a small unsigned integer")?;
    match command {
        FanCommand::Set => {
            let rpm: f32 = args
                .get(1)
                .context("missing rpm argument")?
                .parse()
                .context("rpm must be a floating point number")?;
            if !rpm.is_finite() || rpm < 0.0 {
                bail!("rpm must be a non-negative finite value");
            }
            aetower_sensors::set_fan_min_rpm(fan_id, rpm).map_err(anyhow::Error::msg)?;
            Ok(FanControlResult {
                fan_id,
                command: "fan-set",
                success: true,
                requested_rpm: Some(rpm),
            })
        }
        FanCommand::Reset => {
            aetower_sensors::reset_fan_auto(fan_id).map_err(anyhow::Error::msg)?;
            Ok(FanControlResult {
                fan_id,
                command: "fan-reset",
                success: true,
                requested_rpm: None,
            })
        }
    }
}

fn collect_snapshot() -> Result<HelperSnapshot> {
    let executable_names = collect_executable_names()?;
    let lsof_output = Command::new("lsof")
        .args(["-nP", "-iTCP", "-iUDP", "-Fpcn"])
        .output()
        .context("failed to execute lsof")?;

    if !lsof_output.status.success() {
        bail!("lsof exited with status {}", lsof_output.status);
    }

    let mut samples = BTreeMap::<u32, WorkingSample>::new();
    let stdout = String::from_utf8(lsof_output.stdout).context("lsof output was not utf-8")?;
    let mut current_pid: Option<u32> = None;

    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        let (prefix, value) = line.split_at(1);
        match prefix {
            "p" => {
                if let Ok(pid) = value.parse::<u32>() {
                    current_pid = Some(pid);
                    samples.entry(pid).or_insert_with(|| WorkingSample {
                        pid,
                        executable_name: executable_names.get(&pid).cloned(),
                        ..WorkingSample::default()
                    });
                } else {
                    current_pid = None;
                }
            }
            "c" => {
                if let Some(pid) = current_pid {
                    samples.entry(pid).or_default().process_name = value.to_owned();
                }
            }
            "n" => {
                if let Some(pid) = current_pid {
                    let normalized = value.trim();
                    if !normalized.is_empty() {
                        samples
                            .entry(pid)
                            .or_default()
                            .connections
                            .insert(normalized.to_owned());
                    }
                }
            }
            _ => {}
        }
    }

    let mut processes = samples
        .into_values()
        .filter(|sample| !sample.connections.is_empty())
        .map(|sample| PrivilegedProcessSample {
            pid: sample.pid,
            process_name: sample.process_name,
            executable_name: sample.executable_name,
            connections: sample.connections.into_iter().take(5).collect(),
        })
        .collect::<Vec<_>>();
    processes.sort_by(|left, right| {
        left.process_name
            .cmp(&right.process_name)
            .then(left.pid.cmp(&right.pid))
    });

    Ok(HelperSnapshot { processes })
}

fn collect_executable_names() -> Result<BTreeMap<u32, String>> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,comm="])
        .output()
        .context("failed to execute ps")?;
    if !output.status.success() {
        bail!("ps exited with status {}", output.status);
    }

    let stdout = String::from_utf8(output.stdout).context("ps output was not utf-8")?;
    let mut map = BTreeMap::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let Some(command_path) = parts.next() else {
            continue;
        };
        let executable_name = command_path
            .rsplit('/')
            .next()
            .unwrap_or(command_path)
            .to_owned();
        map.insert(pid, executable_name);
    }
    Ok(map)
}

fn collect_endpoint_security_sample() -> Result<EndpointSecuritySample> {
    let status = probe_endpoint_security_status()?;
    if !status.can_stream_events {
        return Ok(EndpointSecuritySample {
            status,
            events: Vec::new(),
        });
    }

    let Some(eslogger_path) = status.eslogger_path.clone() else {
        return Ok(EndpointSecuritySample {
            status,
            events: Vec::new(),
        });
    };

    let mut child = Command::new(eslogger_path)
        .args(["--format", "json", "exec", "fork", "exit"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn eslogger")?;
    thread::sleep(Duration::from_millis(750));
    let _ = child.kill();
    let output = child
        .wait_with_output()
        .context("failed to collect eslogger sample")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        return Ok(EndpointSecuritySample {
            status: EndpointSecurityStatusSnapshot {
                detail: format!("ESF sampling failed: {}", stderr.trim()),
                last_error: Some(stderr.trim().to_owned()),
                ..status
            },
            events: Vec::new(),
        });
    }

    let events = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_eslogger_event)
        .take(64)
        .collect();

    Ok(EndpointSecuritySample { status, events })
}

fn probe_endpoint_security_status() -> Result<EndpointSecurityStatusSnapshot> {
    let eslogger_path = resolve_eslogger_path();
    let helper_entitled = helper_has_endpoint_security_entitlement()?;
    let running_as_root = current_uid() == Some(0);
    let mut detail = String::new();
    let mut last_error = None;
    let mut can_stream_events = false;

    if let Some(path) = eslogger_path.as_deref() {
        let probe_started = Instant::now();
        let mut child = Command::new(path)
            .args(["--format", "json", "exec"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn eslogger")?;
        while probe_started.elapsed() < Duration::from_millis(500) {
            if let Some(status) = child.try_wait().context("failed to poll eslogger")? {
                let mut stderr = String::new();
                if let Some(mut reader) = child.stderr.take() {
                    let _ = reader.read_to_string(&mut stderr);
                }
                let stderr = stderr.trim().to_owned();
                if stderr.contains("Not privileged") {
                    detail = "Endpoint Security is present but this helper is not privileged enough to subscribe.".to_owned();
                    last_error = Some(stderr);
                } else if status.success() {
                    detail = "Endpoint Security command exited without producing a live stream."
                        .to_owned();
                } else {
                    detail = "Endpoint Security probe exited unexpectedly.".to_owned();
                    if !stderr.is_empty() {
                        last_error = Some(stderr);
                    }
                }
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        if detail.is_empty() {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .context("failed to stop eslogger probe")?;
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !stderr.is_empty() {
                if stderr.contains("Not privileged") {
                    detail = "Endpoint Security is present but this helper lacks the required privilege or entitlement.".to_owned();
                } else {
                    detail = "Endpoint Security probe returned an error.".to_owned();
                }
                last_error = Some(stderr);
            } else if !stdout.is_empty() {
                can_stream_events = true;
                detail = "Endpoint Security event streaming is available.".to_owned();
            } else {
                detail = "Endpoint Security tooling is present, but no event stream was observed during the probe window.".to_owned();
            }
        }
    } else {
        detail = "Endpoint Security tooling is not available on this machine.".to_owned();
    }

    if !helper_entitled {
        if detail.is_empty() {
            detail = "Helper is not signed with the Endpoint Security entitlement.".to_owned();
        } else if !detail.contains("entitlement") {
            detail.push_str(" Helper is not signed with the Endpoint Security entitlement.");
        }
    }

    Ok(EndpointSecurityStatusSnapshot {
        supported: eslogger_path.is_some(),
        helper_entitled,
        running_as_root,
        can_stream_events,
        eslogger_path,
        detail,
        last_error,
    })
}

fn resolve_eslogger_path() -> Option<String> {
    let output = Command::new("which").arg("eslogger").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!path.is_empty()).then_some(path)
}

fn helper_has_endpoint_security_entitlement() -> Result<bool> {
    let exe = std::env::current_exe().context("current_exe")?;
    let output = Command::new("codesign")
        .args(["-d", "--entitlements", ":-"])
        .arg(&exe)
        .output()
        .context("failed to inspect helper entitlements")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(
        stdout.contains("com.apple.developer.endpoint-security.client")
            || stderr.contains("com.apple.developer.endpoint-security.client"),
    )
}

fn current_uid() -> Option<u32> {
    let output = Command::new("id").arg("-u").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn parse_eslogger_event(line: &str) -> Option<EndpointSecurityLifecycleEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    let event_type = find_string(&value, &["event_type", "eventType", "event"])
        .or_else(|| find_string(&value, &["es_event_type"]))
        .unwrap_or_else(|| "unknown".to_owned());
    Some(EndpointSecurityLifecycleEvent {
        event_type,
        timestamp_millis: find_u64(&value, &["timestamp", "timestamp_millis"])
            .unwrap_or_else(now_millis),
        process_path: find_string(&value, &["path", "process_path", "executable"]),
        pid: find_u64(&value, &["pid", "process_id"]).and_then(|value| u32::try_from(value).ok()),
        parent_pid: find_u64(&value, &["ppid", "parent_pid"])
            .and_then(|value| u32::try_from(value).ok()),
        child_pid: find_u64(&value, &["child_pid"]).and_then(|value| u32::try_from(value).ok()),
        exit_code: find_i64(&value, &["exit_code", "stat"])
            .and_then(|value| i32::try_from(value).ok()),
        signal: find_i64(&value, &["signal"]).and_then(|value| i32::try_from(value).ok()),
    })
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(found) = find_by_key(value, key)
            && let Some(text) = found.as_str()
        {
            return Some(text.to_owned());
        }
    }
    None
}

fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(found) = find_by_key(value, key)
            && let Some(number) = found.as_u64()
        {
            return Some(number);
        }
    }
    None
}

fn find_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(found) = find_by_key(value, key)
            && let Some(number) = found.as_i64()
        {
            return Some(number);
        }
    }
    None
}

fn find_by_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(key) {
                return Some(found);
            }
            map.values().find_map(|child| find_by_key(child, key))
        }
        Value::Array(values) => values.iter().find_map(|child| find_by_key(child, key)),
        _ => None,
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{collect_executable_names, parse_eslogger_event};
    use serde_json::json;

    #[test]
    fn executable_names_collection_runs() {
        let result = collect_executable_names();
        assert!(result.is_ok());
    }

    #[test]
    fn parses_nested_eslogger_event() {
        let line = json!({
            "event_type": "exec",
            "mach_time": 123,
            "process": {
                "pid": 41,
                "ppid": 1,
                "executable": {
                    "path": "/bin/zsh"
                }
            }
        })
        .to_string();
        let event = parse_eslogger_event(&line).expect("event");
        assert_eq!(event.event_type, "exec");
        assert_eq!(event.pid, Some(41));
        assert_eq!(event.parent_pid, Some(1));
        assert_eq!(event.process_path.as_deref(), Some("/bin/zsh"));
    }
}
