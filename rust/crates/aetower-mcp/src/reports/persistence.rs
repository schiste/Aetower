//! KnockKnock-style persistence scanner: enumerate macOS startup/persistence
//! surfaces (launchd agents & daemons, login items, cron) and explain which
//! entries deserve operator attention.
//!
//! The default scan is deliberately lightweight: it reads known persistence
//! locations and file metadata, but avoids fork-per-entry code-signing work.
//! A separate deep scan enriches the same inventory with signing data and is
//! cached by path + mtime + size so unchanged startup surfaces do not pay that
//! cost repeatedly.

use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::ProcessSignatureInfo;
use crate::reports::process::{plist_value, read_process_signature};

/// Defensive upper bound on items so a pathological filesystem can't make the
/// scan unbounded.
const MAX_ITEMS: usize = 4000;
const RECENT_CHANGE_WINDOW_MILLIS: u64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistenceScanMode {
    Fast,
    Deep,
}

impl PersistenceScanMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Deep => "deep",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PersistenceItem {
    /// "launch-agent" | "launch-daemon" | "system-launch-agent" |
    /// "system-launch-daemon" | "user-launch-agent" | "login-item" | "cron".
    kind: String,
    /// Human mechanism label. Kept in the report so every client presents the
    /// same grouping instead of duplicating this mapping.
    mechanism: String,
    /// Scope/owner label such as "current user", "machine", or "system".
    owner: String,
    label: Option<String>,
    /// The plist / source path the entry was discovered at.
    path: String,
    /// Resolved executable (Program or ProgramArguments[0]); the signing target
    /// in deep mode.
    program: Option<String>,
    program_name: Option<String>,
    run_at_load: Option<bool>,
    disabled: Option<bool>,
    source_modified_at_millis: Option<u64>,
    source_size_bytes: Option<u64>,
    program_exists: Option<bool>,
    program_modified_at_millis: Option<u64>,
    program_size_bytes: Option<u64>,
    signature: Option<ProcessSignatureInfo>,
    /// True when the entry is Apple-signed or lives under /System. In fast mode
    /// this is intentionally conservative: only /System entries are hidden by
    /// default because third-party Apple-signed apps have not been verified yet.
    is_apple: bool,
    /// expected | review | warning | danger.
    risk_level: String,
    risk_reasons: Vec<String>,
    action_hints: Vec<String>,
    hidden_location: bool,
    recently_modified: bool,
    is_user_scope: bool,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PersistenceScanSummary {
    total_items: usize,
    expected_count: usize,
    review_count: usize,
    warning_count: usize,
    danger_count: usize,
    recently_modified_count: usize,
    user_scope_count: usize,
    system_scope_count: usize,
    missing_target_count: usize,
    unknown_signature_count: usize,
    unsigned_or_adhoc_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DegradedSource {
    source: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PersistenceScanReport {
    captured_at_millis: u64,
    scan_mode: String,
    scan_duration_millis: u64,
    cache_key: String,
    deep_scan_available: bool,
    truncated: bool,
    summary: PersistenceScanSummary,
    items: Vec<PersistenceItem>,
    scanned_locations: Vec<String>,
    degraded: Vec<DegradedSource>,
}

#[derive(Debug, Clone)]
struct PersistenceScanCache {
    mode: String,
    cache_key: String,
    json: String,
}

static SCAN_CACHE: OnceLock<Mutex<Option<PersistenceScanCache>>> = OnceLock::new();

pub fn persistence_scan_json() -> Result<String, String> {
    persistence_scan_json_for_mode(PersistenceScanMode::Fast)
}

pub fn persistence_deep_scan_json() -> Result<String, String> {
    persistence_scan_json_for_mode(PersistenceScanMode::Deep)
}

fn persistence_scan_json_for_mode(mode: PersistenceScanMode) -> Result<String, String> {
    let start = Instant::now();
    let mut report = discover_persistence_scan(mode);

    if let Some(cached) = cached_scan(mode, &report.cache_key) {
        return Ok(cached);
    }

    if mode == PersistenceScanMode::Deep {
        enrich_signatures(&mut report);
    }

    report.summary = summarize_items(&report.items);
    report.scan_duration_millis = start.elapsed().as_millis() as u64;
    let json = serde_json::to_string(&report).map_err(|error| error.to_string())?;
    store_cached_scan(mode, &report.cache_key, &json);
    Ok(json)
}

fn cached_scan(mode: PersistenceScanMode, cache_key: &str) -> Option<String> {
    let cache = SCAN_CACHE.get_or_init(|| Mutex::new(None));
    let guard = cache.lock().ok()?;
    let cached = guard.as_ref()?;
    if cached.mode == mode.as_str() && cached.cache_key == cache_key {
        return Some(cached.json.clone());
    }
    None
}

fn store_cached_scan(mode: PersistenceScanMode, cache_key: &str, json: &str) {
    let cache = SCAN_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(PersistenceScanCache {
            mode: mode.as_str().to_owned(),
            cache_key: cache_key.to_owned(),
            json: json.to_owned(),
        });
    }
}

#[cfg(test)]
pub(crate) fn build_persistence_scan() -> PersistenceScanReport {
    build_persistence_scan_with_mode(PersistenceScanMode::Fast)
}

#[cfg(test)]
pub(crate) fn build_persistence_scan_with_mode(mode: PersistenceScanMode) -> PersistenceScanReport {
    let mut report = discover_persistence_scan(mode);
    if mode == PersistenceScanMode::Deep {
        enrich_signatures(&mut report);
    }
    report.summary = summarize_items(&report.items);
    report
}

fn discover_persistence_scan(mode: PersistenceScanMode) -> PersistenceScanReport {
    let mut items = Vec::new();
    let mut scanned_locations = Vec::new();
    let mut degraded = Vec::new();
    let now_millis = crate::current_unix_millis().unwrap_or_default();

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
            items.push(launchd_item(kind, &path, now_millis));
        }
    }

    if items.len() < MAX_ITEMS {
        items.extend(scan_login_items(now_millis));
    }
    scanned_locations.push("login items".to_owned());

    if items.len() < MAX_ITEMS {
        items.extend(scan_user_cron(now_millis));
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

    let cache_key = cache_key_for(&items, &degraded);
    PersistenceScanReport {
        captured_at_millis: now_millis,
        scan_mode: mode.as_str().to_owned(),
        scan_duration_millis: 0,
        cache_key,
        deep_scan_available: mode == PersistenceScanMode::Fast,
        truncated: items.len() >= MAX_ITEMS,
        summary: summarize_items(&items),
        items,
        scanned_locations,
        degraded,
    }
}

/// Build one launchd item from a plist: Label, the resolved program, RunAtLoad,
/// and filesystem metadata. Code-signing is intentionally left to deep mode.
fn launchd_item(kind: &str, path: &Path, now_millis: u64) -> PersistenceItem {
    let label = plist_value(path, "Label");
    let program = launchd_program(path);
    let program_name = program.as_deref().and_then(file_name);
    let hidden_location = program.as_deref().is_some_and(hidden_or_unusual_path);
    let run_at_load =
        plist_value(path, "RunAtLoad").map(|value| value.eq_ignore_ascii_case("true"));
    let disabled = plist_value(path, "Disabled").map(|value| value.eq_ignore_ascii_case("true"));
    let source_meta = path_metadata(path);
    let program_meta = program.as_deref().map(path_metadata_str);
    let under_system = path.starts_with("/System/");
    let mut item = PersistenceItem {
        kind: kind.to_owned(),
        mechanism: mechanism_label(kind).to_owned(),
        owner: owner_label(kind).to_owned(),
        label,
        path: path.to_string_lossy().to_string(),
        program_name,
        program,
        run_at_load,
        disabled,
        source_modified_at_millis: source_meta.modified_at_millis,
        source_size_bytes: source_meta.size_bytes,
        program_exists: program_meta.as_ref().map(|metadata| metadata.exists),
        program_modified_at_millis: program_meta
            .as_ref()
            .and_then(|metadata| metadata.modified_at_millis),
        program_size_bytes: program_meta
            .as_ref()
            .and_then(|metadata| metadata.size_bytes),
        signature: None,
        is_apple: under_system,
        risk_level: "expected".to_owned(),
        risk_reasons: Vec::new(),
        action_hints: Vec::new(),
        hidden_location,
        recently_modified: recently_modified(now_millis, &source_meta, program_meta.as_ref()),
        is_user_scope: matches!(kind, "user-launch-agent" | "login-item" | "cron"),
        notes: Vec::new(),
    };
    classify_item(&mut item, false);
    item
}

/// Prefer `Program`; fall back to `ProgramArguments[0]`.
fn launchd_program(path: &Path) -> Option<String> {
    plist_value(path, "Program").or_else(|| plist_value(path, "ProgramArguments:0"))
}

/// Same-user login items via System Events. Requires the app's Apple-Events
/// automation permission; degrades to empty if denied.
fn scan_login_items(now_millis: u64) -> Vec<PersistenceItem> {
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
            let program_name = program.as_deref().and_then(file_name);
            let hidden_location = program.as_deref().is_some_and(hidden_or_unusual_path);
            let source_meta = path_metadata(Path::new(item_path));
            let program_meta = program.as_deref().map(path_metadata_str);
            let under_system = item_path.starts_with("/System/");
            let mut item = PersistenceItem {
                kind: "login-item".to_owned(),
                mechanism: mechanism_label("login-item").to_owned(),
                owner: owner_label("login-item").to_owned(),
                label: Path::new(item_path)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string()),
                path: item_path.to_owned(),
                program_name,
                program,
                run_at_load: Some(true),
                disabled: Some(false),
                source_modified_at_millis: source_meta.modified_at_millis,
                source_size_bytes: source_meta.size_bytes,
                program_exists: program_meta.as_ref().map(|metadata| metadata.exists),
                program_modified_at_millis: program_meta
                    .as_ref()
                    .and_then(|metadata| metadata.modified_at_millis),
                program_size_bytes: program_meta
                    .as_ref()
                    .and_then(|metadata| metadata.size_bytes),
                signature: None,
                is_apple: under_system,
                risk_level: "expected".to_owned(),
                risk_reasons: Vec::new(),
                action_hints: Vec::new(),
                hidden_location,
                recently_modified: recently_modified(
                    now_millis,
                    &source_meta,
                    program_meta.as_ref(),
                ),
                is_user_scope: true,
                notes: Vec::new(),
            };
            classify_item(&mut item, false);
            item
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
/// code-sign it (it's a shell line, not necessarily a single binary).
fn scan_user_cron(now_millis: u64) -> Vec<PersistenceItem> {
    let Ok(output) = Command::new("/usr/bin/crontab").arg("-l").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_cron_lines(&stdout, now_millis)
}

/// Parse non-comment, non-empty crontab lines into items. Pure for testing.
fn parse_cron_lines(contents: &str, now_millis: u64) -> Vec<PersistenceItem> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut item = PersistenceItem {
                kind: "cron".to_owned(),
                mechanism: mechanism_label("cron").to_owned(),
                owner: owner_label("cron").to_owned(),
                label: None,
                path: "crontab (current user)".to_owned(),
                program: Some(line.to_owned()),
                program_name: Some("cron command".to_owned()),
                run_at_load: None,
                disabled: None,
                source_modified_at_millis: Some(now_millis),
                source_size_bytes: None,
                program_exists: None,
                program_modified_at_millis: None,
                program_size_bytes: None,
                signature: None,
                is_apple: false,
                risk_level: "expected".to_owned(),
                risk_reasons: Vec::new(),
                action_hints: Vec::new(),
                hidden_location: false,
                recently_modified: false,
                is_user_scope: true,
                notes: vec![
                    "Scheduled command; not code-signed because it is a shell line.".to_owned(),
                ],
            };
            classify_item(&mut item, false);
            item
        })
        .collect()
}

fn enrich_signatures(report: &mut PersistenceScanReport) {
    let mut signing_cache: HashMap<String, ProcessSignatureInfo> = HashMap::new();
    for item in &mut report.items {
        let Some(program) = item.program.as_deref() else {
            classify_item(item, true);
            continue;
        };
        if item.kind == "cron" || !Path::new(program).exists() {
            classify_item(item, true);
            continue;
        }
        let signature = signing_cache
            .entry(program.to_owned())
            .or_insert_with(|| read_process_signature(program))
            .clone();
        item.is_apple =
            item.is_apple || matches!(signature.classification.as_str(), "apple" | "mac_app_store");
        item.signature = Some(signature);
        classify_item(item, true);
    }
}

fn classify_item(item: &mut PersistenceItem, deep_mode: bool) {
    item.risk_reasons.clear();
    item.action_hints.clear();

    if item.disabled == Some(true) {
        item.risk_reasons
            .push("Disabled entry; retained for inventory but not expected to launch.".to_owned());
    }
    if item.kind == "cron" {
        item.risk_reasons.push(
            "Cron runs shell text, so review the command rather than only the binary.".to_owned(),
        );
        item.action_hints
            .push("Review the crontab command.".to_owned());
    }
    if item.program.is_none() {
        item.risk_reasons
            .push("No executable target could be resolved from this entry.".to_owned());
    }
    if item.program_exists == Some(false) {
        item.risk_reasons
            .push("Target executable was not found on disk.".to_owned());
        item.action_hints
            .push("Remove or repair the stale launch item.".to_owned());
    }
    if item.hidden_location {
        item.risk_reasons
            .push("Target lives outside the usual app/system locations.".to_owned());
        item.action_hints
            .push("Reveal the target and inspect ownership.".to_owned());
    }
    if item.recently_modified {
        item.risk_reasons
            .push("Entry or target changed within the last 7 days.".to_owned());
        item.action_hints
            .push("Compare this item with your expected recent installs.".to_owned());
    }

    match item
        .signature
        .as_ref()
        .map(|signature| signature.classification.as_str())
    {
        Some("unsigned") => {
            item.risk_reasons.push("Executable is unsigned.".to_owned());
            item.action_hints
                .push("Inspect the binary before allowing it to auto-start.".to_owned());
        }
        Some("adhoc") | Some("other") => {
            item.risk_reasons
                .push("Executable is ad-hoc signed or has an unusual signing class.".to_owned());
            item.action_hints
                .push("Check signing identity and provenance.".to_owned());
        }
        Some("apple" | "mac_app_store" | "developer_id") => {}
        Some(_) => {
            item.risk_reasons
                .push("Executable signing result is not recognized.".to_owned());
        }
        None if deep_mode && item.kind != "cron" => {
            item.risk_reasons
                .push("Deep audit could not collect signing data for this target.".to_owned());
        }
        None if !item.is_apple && item.kind != "cron" => {
            item.risk_reasons.push(
                "Signature not checked in fast scan; run Deep audit for signing details."
                    .to_owned(),
            );
            item.action_hints
                .push("Run Deep audit if this entry is unexpected.".to_owned());
        }
        None => {}
    }

    item.risk_level = risk_level_for(item).to_owned();
    if item.action_hints.is_empty() {
        item.action_hints
            .push("Keep cached unless it changes or starts consuming resources.".to_owned());
    }
}

fn risk_level_for(item: &PersistenceItem) -> &'static str {
    if matches!(
        item.signature
            .as_ref()
            .map(|signature| signature.classification.as_str()),
        Some("unsigned")
    ) || item.program_exists == Some(false)
    {
        return "danger";
    }
    if matches!(
        item.signature
            .as_ref()
            .map(|signature| signature.classification.as_str()),
        Some("adhoc" | "other")
    ) || item.hidden_location
        || item.kind == "cron"
    {
        return "warning";
    }
    if item.recently_modified || (!item.is_apple && item.signature.is_none()) {
        return "review";
    }
    "expected"
}

fn summarize_items(items: &[PersistenceItem]) -> PersistenceScanSummary {
    PersistenceScanSummary {
        total_items: items.len(),
        expected_count: items
            .iter()
            .filter(|item| item.risk_level == "expected")
            .count(),
        review_count: items
            .iter()
            .filter(|item| item.risk_level == "review")
            .count(),
        warning_count: items
            .iter()
            .filter(|item| item.risk_level == "warning")
            .count(),
        danger_count: items
            .iter()
            .filter(|item| item.risk_level == "danger")
            .count(),
        recently_modified_count: items.iter().filter(|item| item.recently_modified).count(),
        user_scope_count: items.iter().filter(|item| item.is_user_scope).count(),
        system_scope_count: items.iter().filter(|item| !item.is_user_scope).count(),
        missing_target_count: items
            .iter()
            .filter(|item| item.program_exists == Some(false))
            .count(),
        unknown_signature_count: items
            .iter()
            .filter(|item| item.signature.is_none() && item.kind != "cron")
            .count(),
        unsigned_or_adhoc_count: items
            .iter()
            .filter(|item| {
                matches!(
                    item.signature
                        .as_ref()
                        .map(|signature| signature.classification.as_str()),
                    Some("unsigned" | "adhoc" | "other")
                )
            })
            .count(),
    }
}

#[derive(Debug, Clone)]
struct FileMetadataSnapshot {
    exists: bool,
    modified_at_millis: Option<u64>,
    size_bytes: Option<u64>,
}

fn path_metadata_str(path: &str) -> FileMetadataSnapshot {
    path_metadata(Path::new(path))
}

fn path_metadata(path: &Path) -> FileMetadataSnapshot {
    match std::fs::metadata(path) {
        Ok(metadata) => FileMetadataSnapshot {
            exists: true,
            modified_at_millis: metadata.modified().ok().and_then(system_time_millis),
            size_bytes: Some(metadata.len()),
        },
        Err(_) => FileMetadataSnapshot {
            exists: false,
            modified_at_millis: None,
            size_bytes: None,
        },
    }
}

fn system_time_millis(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn recently_modified(
    now_millis: u64,
    source: &FileMetadataSnapshot,
    program: Option<&FileMetadataSnapshot>,
) -> bool {
    let recent = |modified: Option<u64>| {
        modified
            .and_then(|millis| now_millis.checked_sub(millis))
            .is_some_and(|age| age <= RECENT_CHANGE_WINDOW_MILLIS)
    };
    recent(source.modified_at_millis)
        || program.is_some_and(|metadata| recent(metadata.modified_at_millis))
}

fn hidden_or_unusual_path(path: &str) -> bool {
    if path.trim().is_empty() || !path.starts_with('/') {
        return false;
    }
    let usual_prefixes = [
        "/Applications/",
        "/System/",
        "/Library/",
        "/usr/bin/",
        "/usr/libexec/",
        "/bin/",
        "/sbin/",
        "/opt/homebrew/",
        "/usr/local/",
    ];
    if usual_prefixes.iter().any(|prefix| path.starts_with(prefix)) {
        return false;
    }
    path.starts_with("/tmp/")
        || path.starts_with("/private/tmp/")
        || path.starts_with("/var/tmp/")
        || path.contains("/.")
        || path.contains("/Downloads/")
        || path.contains("/Library/Application Support/")
}

fn file_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

fn mechanism_label(kind: &str) -> &'static str {
    match kind {
        "launch-agent" => "LaunchAgent",
        "launch-daemon" => "LaunchDaemon",
        "user-launch-agent" => "User LaunchAgent",
        "system-launch-agent" => "System LaunchAgent",
        "system-launch-daemon" => "System LaunchDaemon",
        "login-item" => "Login Item",
        "cron" => "Cron",
        _ => "Persistence Item",
    }
}

fn owner_label(kind: &str) -> &'static str {
    match kind {
        "user-launch-agent" | "login-item" | "cron" => "current user",
        "system-launch-agent" | "system-launch-daemon" => "system",
        "launch-agent" | "launch-daemon" => "machine",
        _ => "unknown",
    }
}

fn cache_key_for(items: &[PersistenceItem], degraded: &[DegradedSource]) -> String {
    let mut hasher = DefaultHasher::new();
    for item in items {
        item.kind.hash(&mut hasher);
        item.path.hash(&mut hasher);
        item.program.hash(&mut hasher);
        item.source_modified_at_millis.hash(&mut hasher);
        item.source_size_bytes.hash(&mut hasher);
        item.program_exists.hash(&mut hasher);
        item.program_modified_at_millis.hash(&mut hasher);
        item.program_size_bytes.hash(&mut hasher);
        item.disabled.hash(&mut hasher);
    }
    for source in degraded {
        source.source.hash(&mut hasher);
        source.reason.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_lines_skip_comments_and_blanks() {
        let contents = "# header comment\n\n0 9 * * * /usr/local/bin/backup.sh\n  # indented comment\n*/5 * * * * curl https://example.com\n";
        let items = parse_cron_lines(contents, 1234);
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
        assert!(items.iter().all(|item| !item.is_apple));
        assert!(items.iter().all(|item| item.risk_level == "warning"));
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
    fn fast_scan_keeps_signing_lazy() {
        let report = build_persistence_scan_with_mode(PersistenceScanMode::Fast);
        assert_eq!(report.scan_mode, "fast");
        assert!(report.deep_scan_available);
        assert!(report.items.iter().all(|item| item.signature.is_none()));
        assert!(serde_json::to_string(&report).is_ok());
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
