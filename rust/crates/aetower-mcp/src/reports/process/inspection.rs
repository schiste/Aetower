use std::process::Command;

use aetower_model::{SystemSnapshot, classify_signature};

use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ProcessComponentContext {
    pub(crate) entity_id: String,
    pub(crate) display_name: String,
    pub(crate) component_title: String,
    pub(crate) component_kind: String,
    pub(crate) executable_path: Option<String>,
    pub(crate) command_line: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) user: Option<String>,
    pub(crate) parent_summary: Option<String>,
    pub(crate) cpu_percent: f32,
    pub(crate) memory_bytes: u64,
    pub(crate) memory_physical_footprint_bytes: u64,
    pub(crate) start_time_millis: u64,
    pub(crate) sibling_process_count: u32,
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
            "This PID is not currently attributed to an Aetower entity; live macOS process data may still exist."
                .to_owned(),
        );
    }
    if !alive {
        safety_notes.push("The process is not visible to macOS right now.".to_owned());
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
            "ni=".to_owned(),
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
pub(crate) fn value_is_secret(key: &str, value: &str) -> bool {
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
    if parts.len() < 6 {
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
        nice_value: parts[5].parse::<i32>().ok(),
        command: (parts.len() > 6).then(|| parts[6..].join(" ")),
    })
}
