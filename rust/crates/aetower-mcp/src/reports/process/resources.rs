use std::process::Command;

use super::*;

pub(crate) fn build_process_open_resources(
    pid: u32,
    limit: usize,
) -> Result<ProcessOpenResourcesReport, String> {
    validate_pid(pid)?;
    let output = run_os_command(
        "/usr/sbin/lsof",
        &["-nP".to_owned(), "-p".to_owned(), pid.to_string()],
    )?;
    let mut resources = parse_lsof_resources(&output);
    let resource_count = resources.len();
    let socket_count = resources
        .iter()
        .filter(|resource| resource.is_socket)
        .count();
    let file_count = resource_count.saturating_sub(socket_count);
    resources.truncate(limit.max(1));
    Ok(ProcessOpenResourcesReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        pid,
        resource_count,
        returned: resources.len(),
        file_count,
        socket_count,
        resources,
    })
}

pub fn process_open_resources_json(pid: u32, limit: usize) -> Result<String, String> {
    let report = build_process_open_resources(pid, limit)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

/// Defensive cap on holders returned for one reverse lookup.
const MAX_HOLDERS: usize = 200;

struct LsofColumns<'a> {
    command: &'a str,
    pid: u32,
    user: &'a str,
    fd: &'a str,
    resource_type: &'a str,
    device: &'a str,
    size_or_offset: &'a str,
    node: &'a str,
    name: String,
}

/// Reverse pivot: every process currently holding `port` (TCP or UDP).
pub(crate) fn build_resource_holders_by_port(port: u16) -> Result<ResourceHoldersReport, String> {
    if port == 0 {
        return Err("port must be between 1 and 65535".to_owned());
    }
    let output = run_lsof(&["-nP".to_owned(), format!("-i:{port}")])?;
    Ok(holders_report(format!(":{port}"), "port", &output))
}

/// Reverse pivot: every process currently holding the file at `path`.
pub(crate) fn build_resource_holders_by_file(path: &str) -> Result<ResourceHoldersReport, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("file path must not be empty".to_owned());
    }
    // Args are passed to lsof directly (no shell), so injection isn't a concern;
    // still reject control characters and absurd lengths defensively.
    if trimmed.len() > 4096 || trimmed.chars().any(char::is_control) {
        return Err("file path is invalid".to_owned());
    }
    let output = run_lsof(&["-nP".to_owned(), "--".to_owned(), trimmed.to_owned()])?;
    Ok(holders_report(trimmed.to_owned(), "file", &output))
}

/// Build a holders report from raw lsof output, capping the list defensively.
fn holders_report(query: String, kind: &str, output: &str) -> ResourceHoldersReport {
    let mut holders = parse_lsof_holders(output);
    let holder_count = holders.len();
    holders.truncate(MAX_HOLDERS);
    ResourceHoldersReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        query,
        kind: kind.to_owned(),
        holder_count,
        returned: holders.len(),
        holders,
    }
}

/// Run `lsof` tolerating its non-zero exit when *nothing* matches: an empty
/// result is "no holders", not an error. Only a spawn failure is a real error.
/// (`run_os_command` treats any non-zero exit as failure, so it can't be reused
/// here — lsof exits 1 for the common "no open files found" case.)
fn run_lsof(args: &[String]) -> Result<String, String> {
    let output = Command::new("/usr/sbin/lsof")
        .args(args)
        .output()
        .map_err(|error| format!("run lsof: {error}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn parse_lsof_holders(output: &str) -> Vec<ResourceHolder> {
    output
        .lines()
        .skip(1) // header: COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
        .filter_map(parse_lsof_holder_line)
        .collect()
}

pub(crate) fn parse_lsof_holder_line(line: &str) -> Option<ResourceHolder> {
    let columns = parse_lsof_columns(line)?;
    Some(ResourceHolder {
        pid: columns.pid,
        command: columns.command.to_owned(),
        user: columns.user.to_owned(),
        fd: columns.fd.to_owned(),
        resource_type: columns.resource_type.to_owned(),
        name: columns.name,
    })
}

pub fn resource_holders_by_port_json(port: u16) -> Result<String, String> {
    let report = build_resource_holders_by_port(port)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub fn resource_holders_by_file_json(path: &str) -> Result<String, String> {
    let report = build_resource_holders_by_file(path)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn parse_lsof_resources(output: &str) -> Vec<ProcessOpenResource> {
    output
        .lines()
        .skip(1)
        .filter_map(parse_lsof_resource_line)
        .collect()
}

pub(crate) fn parse_lsof_resource_line(line: &str) -> Option<ProcessOpenResource> {
    let columns = parse_lsof_columns(line)?;
    let resource_type = columns.resource_type.to_owned();
    let name = columns.name;
    let detail = Some(format!(
        "{} {} {}",
        columns.device, columns.size_or_offset, columns.node
    ));
    let is_socket = matches!(resource_type.as_str(), "IPv4" | "IPv6" | "unix")
        || name.contains("TCP ")
        || name.contains("UDP ")
        || name.contains("->");
    Some(ProcessOpenResource {
        fd: columns.fd.to_owned(),
        resource_type,
        name,
        detail,
        is_socket,
    })
}

fn parse_lsof_columns(line: &str) -> Option<LsofColumns<'_>> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 9 {
        return None;
    }
    Some(LsofColumns {
        command: parts[0],
        pid: parts[1].parse::<u32>().ok()?,
        user: parts[2],
        fd: parts[3],
        resource_type: parts[4],
        device: parts[5],
        size_or_offset: parts[6],
        node: parts[7],
        name: parts[8..].join(" "),
    })
}
