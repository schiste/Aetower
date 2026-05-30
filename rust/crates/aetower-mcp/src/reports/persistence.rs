//! KnockKnock-style persistence scanner: enumerate macOS startup/persistence
//! surfaces (launchd agents & daemons, login items, cron, Background Task
//! Management) and code-sign each entry's executable so unsigned/ad-hoc
//! outliers stand out.
//!
//! On-demand only (a fork-per-item codesign cost), surfaced as a JSON query.
//! Same-user reads work without elevation; root-gated sources (BTM, other-user
//! cron) are reported as degraded rather than omitted. A per-scan signing cache
//! keyed by executable path means shared `/System` binaries are codesigned
//! once.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::ProcessSignatureInfo;
use crate::reports::process::{plist_value, read_process_signature};

/// Defensive upper bound on items so a pathological filesystem can't make the
/// scan unbounded.
const MAX_ITEMS: usize = 4000;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PersistenceItem {
    /// "launch-agent" | "launch-daemon" | "system-launch-agent" |
    /// "system-launch-daemon" | "user-launch-agent" | "login-item" | "cron".
    kind: String,
    label: Option<String>,
    /// The plist / source path the entry was discovered at.
    path: String,
    /// Resolved executable (Program or ProgramArguments[0]); the codesign target.
    program: Option<String>,
    run_at_load: Option<bool>,
    signature: Option<ProcessSignatureInfo>,
    /// True when the entry is Apple-signed or lives under /System — the UI can
    /// hide these to surface third-party persistence.
    is_apple: bool,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DegradedSource {
    source: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PersistenceScanReport {
    captured_at_millis: u64,
    items: Vec<PersistenceItem>,
    scanned_locations: Vec<String>,
    degraded: Vec<DegradedSource>,
}

pub fn persistence_scan_json() -> Result<String, String> {
    let report = build_persistence_scan();
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_persistence_scan() -> PersistenceScanReport {
    let mut items = Vec::new();
    let mut scanned_locations = Vec::new();
    let mut degraded = Vec::new();
    // Codesign each unique executable once; /System shares many binaries.
    let mut signing_cache: HashMap<String, ProcessSignatureInfo> = HashMap::new();

    let home = std::env::var("HOME").unwrap_or_default();
    let launchd_dirs: [(&str, String); 5] = [
        ("launch-agent", "/Library/LaunchAgents".to_owned()),
        ("launch-daemon", "/Library/LaunchDaemons".to_owned()),
        ("user-launch-agent", format!("{home}/Library/LaunchAgents")),
        (
            "system-launch-agent",
            "/System/Library/LaunchAgents".to_owned(),
        ),
        (
            "system-launch-daemon",
            "/System/Library/LaunchDaemons".to_owned(),
        ),
    ];

    for (kind, directory) in launchd_dirs {
        if directory.is_empty() {
            continue;
        }
        scanned_locations.push(directory.clone());
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if items.len() >= MAX_ITEMS {
                break;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("plist") {
                continue;
            }
            items.push(launchd_item(kind, &path, &mut signing_cache));
        }
    }

    if items.len() < MAX_ITEMS {
        items.extend(scan_login_items(&mut signing_cache));
    }
    scanned_locations.push("login items".to_owned());

    if items.len() < MAX_ITEMS {
        items.extend(scan_user_cron());
    }
    scanned_locations.push("user crontab".to_owned());

    // Root-gated sources: report rather than omit.
    degraded.push(DegradedSource {
        source: "background-task-management".to_owned(),
        reason: "Background Task Management (sfltool dumpbtm) requires elevated access.".to_owned(),
    });
    degraded.push(DegradedSource {
        source: "system-cron".to_owned(),
        reason: "Other users' cron tables require elevated access.".to_owned(),
    });

    PersistenceScanReport {
        captured_at_millis: crate::current_unix_millis().unwrap_or_default(),
        items,
        scanned_locations,
        degraded,
    }
}

/// Build one launchd item from a plist: Label, the resolved program, RunAtLoad,
/// and a (cached) signing classification.
fn launchd_item(
    kind: &str,
    path: &Path,
    signing_cache: &mut HashMap<String, ProcessSignatureInfo>,
) -> PersistenceItem {
    let label = plist_value(path, "Label");
    let program = launchd_program(path);
    let run_at_load =
        plist_value(path, "RunAtLoad").map(|value| value.eq_ignore_ascii_case("true"));
    let under_system = path.starts_with("/System/");
    let (signature, is_apple) = sign_program(program.as_deref(), under_system, signing_cache);
    PersistenceItem {
        kind: kind.to_owned(),
        label,
        path: path.to_string_lossy().to_string(),
        program,
        run_at_load,
        signature,
        is_apple,
        notes: Vec::new(),
    }
}

/// Prefer `Program`; fall back to `ProgramArguments[0]`.
fn launchd_program(path: &Path) -> Option<String> {
    plist_value(path, "Program").or_else(|| plist_value(path, "ProgramArguments:0"))
}

/// Same-user login items via System Events. Requires the app's Apple-Events
/// automation permission; degrades to empty if denied.
fn scan_login_items(
    signing_cache: &mut HashMap<String, ProcessSignatureInfo>,
) -> Vec<PersistenceItem> {
    let script = "tell application \"System Events\" to get the path of every login item";
    let Ok(output) = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // osascript prints AppleScript lists comma-separated on one line.
    stdout
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|item_path| {
            let program = login_item_executable(item_path);
            let under_system = item_path.starts_with("/System/");
            let (signature, is_apple) =
                sign_program(program.as_deref(), under_system, signing_cache);
            PersistenceItem {
                kind: "login-item".to_owned(),
                label: Path::new(item_path)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string()),
                path: item_path.to_owned(),
                program,
                run_at_load: Some(true),
                signature,
                is_apple,
                notes: Vec::new(),
            }
        })
        .collect()
}

/// A login item path is usually an `.app` bundle; resolve its main executable
/// via the bundle's `CFBundleExecutable`, falling back to the path itself.
fn login_item_executable(item_path: &str) -> Option<String> {
    if item_path.ends_with(".app") {
        let info = format!("{item_path}/Contents/Info.plist");
        if let Some(exe) = plist_value(Path::new(&info), "CFBundleExecutable") {
            let candidate = format!("{item_path}/Contents/MacOS/{exe}");
            if Path::new(&candidate).exists() {
                return Some(candidate);
            }
        }
    }
    Some(item_path.to_owned())
}

/// Current user's crontab. The command line is the persistence payload; we don't
/// codesign it (it's a shell line, not necessarily a single binary).
fn scan_user_cron() -> Vec<PersistenceItem> {
    let Ok(output) = Command::new("/usr/bin/crontab").arg("-l").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_cron_lines(&stdout)
}

/// Parse non-comment, non-empty crontab lines into items. Pure for testing.
fn parse_cron_lines(contents: &str) -> Vec<PersistenceItem> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| PersistenceItem {
            kind: "cron".to_owned(),
            label: None,
            path: "crontab (current user)".to_owned(),
            program: Some(line.to_owned()),
            run_at_load: None,
            signature: None,
            is_apple: false,
            notes: vec!["Scheduled command; not code-signed (shell line).".to_owned()],
        })
        .collect()
}

/// Resolve and cache the signing classification for a program path. Returns the
/// signature and whether it should be treated as Apple/system (for UI filtering).
fn sign_program(
    program: Option<&str>,
    under_system: bool,
    signing_cache: &mut HashMap<String, ProcessSignatureInfo>,
) -> (Option<ProcessSignatureInfo>, bool) {
    let Some(program) = program else {
        return (None, under_system);
    };
    if !Path::new(program).exists() {
        return (None, under_system);
    }
    let signature = signing_cache
        .entry(program.to_owned())
        .or_insert_with(|| read_process_signature(program))
        .clone();
    let is_apple =
        under_system || matches!(signature.classification.as_str(), "apple" | "mac_app_store");
    (Some(signature), is_apple)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_lines_skip_comments_and_blanks() {
        let contents = "# header comment\n\n0 9 * * * /usr/local/bin/backup.sh\n  # indented comment\n*/5 * * * * curl https://example.com\n";
        let items = parse_cron_lines(contents);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "cron");
        assert_eq!(
            items[0].program.as_deref(),
            Some("0 9 * * * /usr/local/bin/backup.sh")
        );
        assert_eq!(
            items[1].program.as_deref().map(|p| p.contains("curl")),
            Some(true)
        );
        // Cron entries are never treated as Apple-signed.
        assert!(items.iter().all(|item| !item.is_apple));
    }

    #[test]
    fn login_item_executable_falls_back_to_path() {
        // A non-.app path resolves to itself.
        assert_eq!(
            login_item_executable("/usr/local/bin/widget"),
            Some("/usr/local/bin/widget".to_owned())
        );
    }

    #[test]
    fn scan_report_serializes_with_degraded_sources() {
        // build_persistence_scan touches the real filesystem but must always
        // return a serializable report with the degraded root-gated sources.
        let report = build_persistence_scan();
        assert!(
            report
                .degraded
                .iter()
                .any(|source| source.source == "background-task-management")
        );
        assert!(serde_json::to_string(&report).is_ok());
    }
}
