use std::{
    fs::{self, File},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use super::*;

const LSOF_TIMEOUT: Duration = Duration::from_secs(2);
static LSOF_RUN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn build_process_open_resources(
    pid: u32,
    limit: usize,
) -> Result<ProcessOpenResourcesReport, String> {
    validate_pid(pid)?;
    if !process_exists(pid) {
        return Err(format!("process {pid} is not visible to macOS right now"));
    }
    let native_fd_count = native_process_fd_count(pid);
    let output = run_lsof(&[
        "-nP".to_owned(),
        "-w".to_owned(),
        "-p".to_owned(),
        pid.to_string(),
    ])?;
    let mut resources = parse_lsof_resources(&output);
    let lsof_resource_count = resources.len();
    let resource_count = native_fd_count
        .map(|count| count.max(lsof_resource_count))
        .unwrap_or(lsof_resource_count);
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
        native_fd_count,
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
    let output = run_lsof(&["-nP".to_owned(), "-w".to_owned(), format!("-i:{port}")])?;
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
    let output = run_lsof(&[
        "-nP".to_owned(),
        "-w".to_owned(),
        "--".to_owned(),
        trimmed.to_owned(),
    ])?;
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
    let sequence = LSOF_RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let output_path = std::env::temp_dir().join(format!(
        "aetower-lsof-{}-{}-{sequence}.out",
        std::process::id(),
        current_unix_millis().unwrap_or_default()
    ));
    let output_file =
        File::create(&output_path).map_err(|error| format!("create lsof output file: {error}"))?;
    let mut child = match Command::new("/usr/sbin/lsof")
        .args(args)
        .stdout(Stdio::from(output_file))
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_file(&output_path);
            return Err(format!("run lsof: {error}"));
        }
    };
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("wait for lsof: {error}"))?
        {
            break status;
        }
        if started.elapsed() >= LSOF_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&output_path);
            return Err(format!(
                "lsof timed out after {}ms",
                LSOF_TIMEOUT.as_millis()
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    let output = match fs::read_to_string(&output_path) {
        Ok(output) => output,
        Err(error) => {
            let _ = fs::remove_file(&output_path);
            return Err(format!("read lsof output file: {error}"));
        }
    };
    let _ = fs::remove_file(&output_path);
    if !status.success() && output.trim().is_empty() {
        return Ok(String::new());
    }
    Ok(output)
}

#[cfg(target_os = "macos")]
fn native_process_fd_count(pid: u32) -> Option<usize> {
    const PROC_PIDLISTFDS: i32 = 1;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ProcFdInfo {
        proc_fd: i32,
        proc_fdtype: u32,
    }

    let Ok(pid) = i32::try_from(pid) else {
        return None;
    };
    let record_size = std::mem::size_of::<ProcFdInfo>();
    let required_bytes = unsafe { proc_pidinfo(pid, PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if required_bytes <= 0 {
        return None;
    }
    let capacity = usize::try_from(required_bytes)
        .ok()?
        .checked_div(record_size)?;
    if capacity == 0 || capacity > 200_000 {
        return None;
    }
    let mut fds = vec![
        ProcFdInfo {
            proc_fd: 0,
            proc_fdtype: 0,
        };
        capacity
    ];
    let returned_bytes = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            i32::try_from(fds.len().checked_mul(record_size)?).ok()?,
        )
    };
    if returned_bytes <= 0 {
        return None;
    }
    usize::try_from(returned_bytes)
        .ok()?
        .checked_div(record_size)
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn proc_pidinfo(
        pid: i32,
        flavor: i32,
        arg: u64,
        buffer: *mut std::ffi::c_void,
        buffersize: i32,
    ) -> i32;
}

#[cfg(not(target_os = "macos"))]
fn native_process_fd_count(_pid: u32) -> Option<usize> {
    None
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
