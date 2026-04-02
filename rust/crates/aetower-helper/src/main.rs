use std::{
    collections::{BTreeMap, BTreeSet},
    process::Command,
};

use anyhow::{bail, Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct HelperSnapshot {
    processes: Vec<PrivilegedProcessSample>,
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
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sample".to_owned());
    match command.as_str() {
        "sample" => {
            let snapshot = collect_snapshot()?;
            println!("{}", serde_json::to_string(&snapshot)?);
            Ok(())
        }
        other => {
            bail!("unsupported command: {other}")
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

#[cfg(test)]
mod tests {
    use super::collect_executable_names;

    #[test]
    fn executable_names_collection_runs() {
        let result = collect_executable_names();
        assert!(result.is_ok());
    }
}
