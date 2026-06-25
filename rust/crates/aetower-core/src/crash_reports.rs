use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use aetower_diagnostics::DiagnosticsLevel;

const DIAGNOSTIC_REPORTS_DIR: &str = "/Library/Logs/DiagnosticReports";
const MAX_REPORT_BYTES: u64 = 128 * 1024;
const MAX_FIELD_VALUE_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CrashReportMarker {
    pub(crate) timestamp_millis: u64,
    pub(crate) level: DiagnosticsLevel,
    pub(crate) event_type: &'static str,
    pub(crate) message: &'static str,
    pub(crate) fields: Vec<(&'static str, String)>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ResetCounterReport {
    bug_type: Option<String>,
    reset_count: Option<String>,
    boot_failure_count: Option<String>,
    boot_faults: Option<String>,
    boot_stage: Option<String>,
}

pub(crate) fn load_recent_crash_report_markers(
    now_millis: u64,
    lookback_millis: u64,
) -> Vec<CrashReportMarker> {
    load_recent_crash_report_markers_from_dir(
        Path::new(DIAGNOSTIC_REPORTS_DIR),
        now_millis,
        lookback_millis,
    )
}

fn load_recent_crash_report_markers_from_dir(
    dir: &Path,
    now_millis: u64,
    lookback_millis: u64,
) -> Vec<CrashReportMarker> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let earliest_millis = now_millis.saturating_sub(lookback_millis);
    let mut markers = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(timestamp_millis) = modified_millis(&path) else {
            continue;
        };
        if timestamp_millis < earliest_millis || timestamp_millis > now_millis.saturating_add(1_000)
        {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if file_name.starts_with("ResetCounter-") && file_name.ends_with(".diag") {
            markers.extend(load_reset_counter_marker(&path, timestamp_millis));
        } else if file_name == ".contents.panic" {
            markers.extend(load_panic_manifest_markers(&path, timestamp_millis));
        } else if file_name.ends_with(".panic") {
            markers.extend(load_panic_file_marker(&path, timestamp_millis));
        }
    }

    markers.sort_by_key(|marker| marker.timestamp_millis);
    markers.dedup_by(|left, right| {
        left.event_type == right.event_type
            && marker_field(left, "marker_key") == marker_field(right, "marker_key")
    });
    markers
}

fn load_reset_counter_marker(path: &Path, timestamp_millis: u64) -> Option<CrashReportMarker> {
    let content = read_bounded_text(path)?;
    let report = parse_reset_counter_report(&content);
    let boot_failure_count = report
        .boot_failure_count
        .as_deref()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let has_boot_faults = report
        .boot_faults
        .as_deref()
        .map(|value| !value.trim().is_empty() && value.trim() != "none")
        .unwrap_or(false);
    let level = if boot_failure_count > 0 || has_boot_faults {
        DiagnosticsLevel::Warn
    } else {
        DiagnosticsLevel::Info
    };

    let mut fields = base_file_fields(path);
    push_optional_field(&mut fields, "bug_type", report.bug_type);
    push_optional_field(&mut fields, "reset_count", report.reset_count);
    push_optional_field(&mut fields, "boot_failure_count", report.boot_failure_count);
    push_optional_field(&mut fields, "boot_faults", report.boot_faults);
    push_optional_field(&mut fields, "boot_stage", report.boot_stage);
    fields.push(("marker_key", file_marker_key("reset-counter", path)));

    Some(CrashReportMarker {
        timestamp_millis,
        level,
        event_type: "system-reset-counter-report",
        message: "Observed a macOS ResetCounter diagnostic report after a reboot.",
        fields,
    })
}

fn load_panic_manifest_markers(path: &Path, timestamp_millis: u64) -> Vec<CrashReportMarker> {
    let Some(content) = read_bounded_text(path) else {
        return Vec::new();
    };
    let mut fields = base_file_fields(path);
    if let Some(summary) = extract_panic_summary(&content) {
        fields.push(("summary", truncate_field(summary)));
    }

    let mut referenced_paths = extract_referenced_panic_paths(&content);
    referenced_paths.sort();
    referenced_paths.dedup();
    if referenced_paths.is_empty() {
        fields.push(("referenced_panic_count", "0".to_owned()));
    } else {
        fields.push(("referenced_panic_count", referenced_paths.len().to_string()));
        fields.push((
            "referenced_panic_paths",
            truncate_field(referenced_paths.join(",")),
        ));
        fields.push((
            "referenced_panic_exists",
            referenced_paths
                .iter()
                .any(|referenced| Path::new(referenced).exists())
                .to_string(),
        ));
    }
    fields.push(("marker_key", file_marker_key("panic-manifest", path)));

    let mut markers = vec![CrashReportMarker {
        timestamp_millis,
        level: DiagnosticsLevel::Warn,
        event_type: "system-panic-manifest",
        message: "Observed a macOS panic manifest in DiagnosticReports.",
        fields,
    }];

    for referenced in referenced_paths {
        let referenced_path = PathBuf::from(referenced);
        if let Some(referenced_timestamp) = modified_millis(&referenced_path) {
            markers.extend(load_panic_file_marker(
                &referenced_path,
                referenced_timestamp,
            ));
        }
    }
    markers
}

fn load_panic_file_marker(path: &Path, timestamp_millis: u64) -> Option<CrashReportMarker> {
    let content = read_bounded_text(path)?;
    let mut fields = base_file_fields(path);
    if let Some(summary) = extract_panic_summary(&content) {
        fields.push(("summary", truncate_field(summary)));
    }
    fields.push(("marker_key", file_marker_key("panic-file", path)));

    Some(CrashReportMarker {
        timestamp_millis,
        level: DiagnosticsLevel::Warn,
        event_type: "system-panic-file-observed",
        message: "Observed a macOS panic report file in DiagnosticReports.",
        fields,
    })
}

fn parse_reset_counter_report(content: &str) -> ResetCounterReport {
    let mut report = ResetCounterReport::default();
    for line in content.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "bug_type" | "bug type" => report.bug_type = Some(value.to_owned()),
            "reset count" | "reset_count" => report.reset_count = Some(value.to_owned()),
            "boot failure count" | "boot_failure_count" => {
                report.boot_failure_count = Some(value.to_owned());
            }
            "boot faults" | "boot_faults" => report.boot_faults = Some(value.to_owned()),
            "boot stage" | "boot_stage" => report.boot_stage = Some(value.to_owned()),
            _ => {}
        }
    }
    report
}

fn extract_referenced_panic_paths(content: &str) -> Vec<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
        let mut paths = Vec::new();
        collect_json_panic_paths(&value, &mut paths);
        return paths;
    }

    content
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | '[' | ']'))
        .map(|token| token.trim_matches(|ch| matches!(ch, '"' | '\'' | ',')))
        .filter(|token| token.starts_with('/') && token.ends_with(".panic"))
        .map(ToOwned::to_owned)
        .collect()
}

fn collect_json_panic_paths(value: &serde_json::Value, paths: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => {
            if text.starts_with('/') && text.ends_with(".panic") {
                paths.push(text.clone());
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_json_panic_paths(value, paths);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                collect_json_panic_paths(value, paths);
            }
        }
        _ => {}
    }
}

fn extract_panic_summary(content: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content)
        && let Some(summary) = find_json_panic_summary(&value)
    {
        return Some(summary);
    }

    content
        .lines()
        .map(str::trim)
        .find(|line| {
            let normalized = line.to_ascii_lowercase();
            normalized.contains("socd report detected")
                || normalized.contains("panic(cpu")
                || normalized.contains("userspace watchdog timeout")
                || normalized.contains("panic string")
        })
        .map(ToOwned::to_owned)
}

fn find_json_panic_summary(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let normalized = text.to_ascii_lowercase();
            if normalized.contains("socd report detected")
                || normalized.contains("panic(cpu")
                || normalized.contains("userspace watchdog timeout")
            {
                Some(text.clone())
            } else {
                None
            }
        }
        serde_json::Value::Array(values) => values.iter().find_map(find_json_panic_summary),
        serde_json::Value::Object(values) => {
            for key in [
                "panic_string",
                "panicString",
                "panic",
                "description",
                "message",
            ] {
                if let Some(summary) = values.get(key).and_then(find_json_panic_summary) {
                    return Some(summary);
                }
            }
            values.values().find_map(find_json_panic_summary)
        }
        _ => None,
    }
}

fn read_bounded_text(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > MAX_REPORT_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn modified_millis(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    system_time_millis(modified)
}

fn system_time_millis(value: SystemTime) -> Option<u64> {
    let duration = value.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    u64::try_from(duration.as_millis()).ok()
}

fn base_file_fields(path: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("path", path.display().to_string()),
        (
            "filename",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_owned(),
        ),
    ]
}

fn push_optional_field(
    fields: &mut Vec<(&'static str, String)>,
    key: &'static str,
    value: Option<String>,
) {
    if let Some(value) = value {
        fields.push((key, truncate_field(value)));
    }
}

fn file_marker_key(prefix: &str, path: &Path) -> String {
    format!("{prefix}:{}", path.display())
}

fn marker_field<'a>(marker: &'a CrashReportMarker, key: &str) -> Option<&'a str> {
    marker
        .fields
        .iter()
        .find_map(|(field_key, value)| (*field_key == key).then_some(value.as_str()))
}

fn truncate_field(value: String) -> String {
    let mut chars = value.chars();
    let truncated = chars
        .by_ref()
        .take(MAX_FIELD_VALUE_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_counter_report_extracts_boot_failure_fields() {
        let report = parse_reset_counter_report(
            "bug_type: 115\nReset count: 1\nBoot failure count: 1\nBoot faults: wdog,reset_in_1\nBoot stage: iBoot\n",
        );

        assert_eq!(report.bug_type.as_deref(), Some("115"));
        assert_eq!(report.reset_count.as_deref(), Some("1"));
        assert_eq!(report.boot_failure_count.as_deref(), Some("1"));
        assert_eq!(report.boot_faults.as_deref(), Some("wdog,reset_in_1"));
        assert_eq!(report.boot_stage.as_deref(), Some("iBoot"));
    }

    #[test]
    fn panic_manifest_extracts_json_paths_and_socd_summary() {
        let content = r#"{
            "panic_string": "SOCD report detected: (iBoot async abort)",
            "files_to_attach": ["/Library/Logs/DiagnosticReports/panic-base+socd-2026-06-25-141049.000.panic"]
        }"#;

        assert_eq!(
            extract_panic_summary(content).as_deref(),
            Some("SOCD report detected: (iBoot async abort)")
        );
        assert_eq!(
            extract_referenced_panic_paths(content),
            vec![
                "/Library/Logs/DiagnosticReports/panic-base+socd-2026-06-25-141049.000.panic"
                    .to_owned()
            ]
        );
    }

    #[test]
    fn crash_report_loader_bounds_to_recent_files() {
        let dir =
            std::env::temp_dir().join(format!("aetower-crash-report-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        if let Err(error) = fs::create_dir_all(&dir) {
            panic!("failed to create temp crash report dir: {error}");
        }
        if let Err(error) = fs::write(
            dir.join("ResetCounter-2026-06-25-141049.diag"),
            "bug_type: 115\nBoot failure count: 1\nBoot faults: wdog,reset_in_1\n",
        ) {
            panic!("failed to write reset counter fixture: {error}");
        }
        if let Err(error) = fs::write(
            dir.join(".contents.panic"),
            r#"{"panic_string":"SOCD report detected: (iBoot async abort)"}"#,
        ) {
            panic!("failed to write panic manifest fixture: {error}");
        }

        let Some(now) = system_time_millis(SystemTime::now()).map(|millis| millis + 1_000) else {
            panic!("failed to convert current time to millis");
        };
        let markers = load_recent_crash_report_markers_from_dir(&dir, now, 60_000);

        assert!(
            markers
                .iter()
                .any(|marker| marker.event_type == "system-reset-counter-report")
        );
        assert!(
            markers
                .iter()
                .any(|marker| marker.event_type == "system-panic-manifest")
        );

        if let Err(error) = fs::remove_dir_all(&dir) {
            panic!("failed to remove temp crash report dir: {error}");
        }
    }
}
