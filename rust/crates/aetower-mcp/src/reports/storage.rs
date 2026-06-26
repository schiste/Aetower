use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

const MAX_LIMIT: usize = 200;
const MAX_ROOTS: usize = 24;
const MAX_DIRECTORIES: u64 = 25_000;
const SIZE_WALK_MAX_ENTRIES: u64 = 150_000;
const MIN_ITEM_BYTES: u64 = 1024 * 1024;
const SCAN_TIME_BUDGET: Duration = Duration::from_millis(6_500);
const STALE_AFTER_DAYS: u64 = 7;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StorageHygieneReport {
    captured_at_millis: u64,
    scan_duration_millis: u64,
    summary: StorageHygieneSummary,
    items: Vec<StorageHygieneItem>,
    roots: Vec<String>,
    skipped_roots: Vec<StorageSkippedRoot>,
    truncated: bool,
    caveats: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StorageHygieneSummary {
    item_count: usize,
    total_reclaimable_bytes: u64,
    safe_candidate_count: usize,
    review_candidate_count: usize,
    stale_candidate_count: usize,
    scanned_directory_count: u64,
    largest_item_path: Option<String>,
    largest_item_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
struct StorageHygieneItem {
    id: String,
    path: String,
    display_name: String,
    kind: String,
    safety: String,
    size_bytes: u64,
    size_truncated: bool,
    modified_millis: Option<u64>,
    age_days: Option<u64>,
    stale: bool,
    reason: String,
    recommendation: String,
    command_hint: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageSkippedRoot {
    path: String,
    reason: String,
}

#[derive(Clone, Copy, Debug)]
struct StorageHygieneOptions {
    max_depth: usize,
    limit: usize,
}

#[derive(Clone, Copy, Debug)]
struct ArtifactRule {
    kind: &'static str,
    safety: &'static str,
    reason: &'static str,
    recommendation: &'static str,
}

#[derive(Clone, Copy, Debug, Default)]
struct SizeWalkResult {
    bytes: u64,
    truncated: bool,
    entries: u64,
}

pub fn storage_hygiene_json(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
) -> Result<String, String> {
    let report = build_storage_hygiene_report(roots, max_depth, limit);
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_storage_hygiene_report(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
) -> StorageHygieneReport {
    build_storage_hygiene_report_with_options(
        roots,
        StorageHygieneOptions {
            max_depth: max_depth.clamp(1, 12),
            limit: limit.clamp(1, MAX_LIMIT),
        },
    )
}

fn build_storage_hygiene_report_with_options(
    roots: Vec<String>,
    options: StorageHygieneOptions,
) -> StorageHygieneReport {
    let started = Instant::now();
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let roots = normalize_roots(roots);
    let mut items = Vec::new();
    let mut scanned_roots = Vec::new();
    let mut skipped_roots = Vec::new();
    let mut scanned_directory_count = 0;
    let mut truncated = false;

    for root in roots {
        if started.elapsed() >= SCAN_TIME_BUDGET {
            truncated = true;
            break;
        }

        let root_display = root.display().to_string();
        if !root.exists() {
            skipped_roots.push(StorageSkippedRoot {
                path: root_display,
                reason: "missing".to_owned(),
            });
            continue;
        }

        let metadata = match fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) => {
                skipped_roots.push(StorageSkippedRoot {
                    path: root_display,
                    reason: error.to_string(),
                });
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            skipped_roots.push(StorageSkippedRoot {
                path: root_display,
                reason: "symlink root skipped".to_owned(),
            });
            continue;
        }

        scanned_roots.push(root.display().to_string());
        let (mut root_items, root_dirs, root_truncated) =
            scan_root(&root, &options, started, now_millis);
        scanned_directory_count += root_dirs;
        truncated |= root_truncated;
        items.append(&mut root_items);

        if items.len() >= options.limit {
            truncated = true;
            break;
        }
    }

    items.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    items.truncate(options.limit);

    let summary = summarize_storage_items(&items, scanned_directory_count);
    StorageHygieneReport {
        captured_at_millis: now_millis,
        scan_duration_millis: started.elapsed().as_millis() as u64,
        summary,
        items,
        roots: scanned_roots,
        skipped_roots,
        truncated,
        caveats: vec![
            "Read-only scan: Aetower does not delete anything from this report.".to_owned(),
            "Sizes are bounded estimates and may omit paths that require additional permissions."
                .to_owned(),
            "Review candidates may be rebuildable but can still contain release artifacts or local environments."
                .to_owned(),
        ],
    }
}

fn scan_root(
    root: &Path,
    options: &StorageHygieneOptions,
    started: Instant,
    now_millis: u64,
) -> (Vec<StorageHygieneItem>, u64, bool) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut items = Vec::new();
    let mut scanned_dirs = 0;
    let mut truncated = false;

    while let Some((path, depth)) = stack.pop() {
        if started.elapsed() >= SCAN_TIME_BUDGET
            || scanned_dirs >= MAX_DIRECTORIES
            || items.len() >= options.limit
        {
            truncated = true;
            break;
        }

        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }

        if let Some(rule) = classify_artifact(&path, &metadata) {
            let size = size_of_path(&path, started);
            if size.bytes >= MIN_ITEM_BYTES {
                items.push(storage_item_for_path(
                    &path,
                    metadata.modified().ok(),
                    rule,
                    size,
                    now_millis,
                ));
            }
            if metadata.is_dir() {
                continue;
            }
        }

        if !metadata.is_dir() || depth >= options.max_depth || is_source_control_dir(&path) {
            continue;
        }

        scanned_dirs += 1;
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            stack.push((entry.path(), depth + 1));
        }
    }

    (items, scanned_dirs, truncated)
}

fn storage_item_for_path(
    path: &Path,
    modified: Option<SystemTime>,
    rule: ArtifactRule,
    size: SizeWalkResult,
    now_millis: u64,
) -> StorageHygieneItem {
    let modified_millis = modified.and_then(system_time_millis);
    let age_days = modified_millis.and_then(|millis| {
        now_millis
            .checked_sub(millis)
            .map(|delta| delta / 86_400_000)
    });
    let stale = age_days.is_some_and(|days| days >= STALE_AFTER_DAYS);
    let path_display = path.display().to_string();
    StorageHygieneItem {
        id: path_display.clone(),
        path: path_display.clone(),
        display_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact")
            .to_owned(),
        kind: rule.kind.to_owned(),
        safety: rule.safety.to_owned(),
        size_bytes: size.bytes,
        size_truncated: size.truncated,
        modified_millis,
        age_days,
        stale,
        reason: rule.reason.to_owned(),
        recommendation: rule.recommendation.to_owned(),
        command_hint: format!("du -sh {}", shell_quote(&path_display)),
    }
}

fn classify_artifact(path: &Path, metadata: &fs::Metadata) -> Option<ArtifactRule> {
    let name = path.file_name()?.to_str()?;
    let parent_name = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if metadata.is_file() && name.ends_with(".log") {
        return Some(rule(
            "log-file",
            "safe",
            "Development log file.",
            "Safe to review and rotate when no current task depends on it.",
        ));
    }

    if !metadata.is_dir() {
        return None;
    }

    match name {
        "target" => Some(rule(
            "rust-build",
            "safe",
            "Rust Cargo build output.",
            "Usually safe to remove after builds/tests are idle; Cargo will rebuild it.",
        )),
        ".build" => Some(rule(
            "swift-build",
            "safe",
            "Swift Package Manager build output.",
            "Usually safe to remove after builds/tests are idle; SwiftPM will rebuild it.",
        )),
        "DerivedData" => Some(rule(
            "xcode-derived-data",
            "safe",
            "Xcode DerivedData cache.",
            "Usually safe to remove when Xcode builds are idle; Xcode will recreate it.",
        )),
        "ModuleCache.noindex" => Some(rule(
            "xcode-module-cache",
            "safe",
            "Xcode module cache.",
            "Usually safe to remove when Xcode builds are idle.",
        )),
        ".pytest_cache" | ".mypy_cache" | ".ruff_cache" | "__pycache__" => Some(rule(
            "python-cache",
            "safe",
            "Python test/lint/import cache.",
            "Usually safe to remove; Python tools will recreate it.",
        )),
        ".turbo" | ".vite" | ".parcel-cache" => Some(rule(
            "frontend-cache",
            "safe",
            "Frontend build cache.",
            "Usually safe to remove when frontend dev servers/builds are idle.",
        )),
        ".aetower-cache" | ".aeptus-cache" | "org.swift.swiftpm" | "com.apple.dt.Xcode" => {
            Some(rule(
                "tool-cache",
                "safe",
                "Developer tool cache.",
                "Usually safe to remove when the related toolchain is idle; tools will recreate it.",
            ))
        }
        "coverage" => Some(rule(
            "coverage-output",
            "safe",
            "Test coverage output.",
            "Safe to remove if you do not need the local coverage report.",
        )),
        "tmp" | "temp" => Some(rule(
            "temporary-output",
            "review",
            "Local temporary output directory.",
            "Review before deleting because temporary folders can contain active run output.",
        )),
        "logs" => Some(rule(
            "logs",
            "safe",
            "Local logs directory.",
            "Safe to review and rotate once the relevant debugging session is over.",
        )),
        "node_modules" => Some(rule(
            "node-dependencies",
            "review",
            "Node dependency install tree.",
            "Review before deleting; reinstalling may take time and requires package-manager access.",
        )),
        ".venv" | "venv" => Some(rule(
            "python-environment",
            "review",
            "Python virtual environment.",
            "Review before deleting; recreate from dependency manifests if still needed.",
        )),
        "dist" | "build" => Some(rule(
            "build-output",
            "review",
            "Generic build output directory.",
            "Review before deleting because it may contain release artifacts.",
        )),
        "SourcePackages" => Some(rule(
            "xcode-source-packages",
            "review",
            "Xcode resolved package cache.",
            "Review before deleting; Xcode can rebuild it but package resolution may take time.",
        )),
        "cache" if parent_name == ".next" => Some(rule(
            "next-cache",
            "safe",
            "Next.js build cache.",
            "Usually safe to remove when frontend builds/dev servers are idle.",
        )),
        ".next" => Some(rule(
            "next-build",
            "review",
            "Next.js build output.",
            "Review before deleting because it may contain generated app artifacts.",
        )),
        _ => None,
    }
}

fn size_of_path(path: &Path, started: Instant) -> SizeWalkResult {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return SizeWalkResult::default();
    };
    if metadata.file_type().is_symlink() {
        return SizeWalkResult::default();
    }
    if metadata.is_file() {
        return SizeWalkResult {
            bytes: metadata.len(),
            truncated: false,
            entries: 1,
        };
    }
    if !metadata.is_dir() {
        return SizeWalkResult::default();
    }

    let mut result = SizeWalkResult::default();
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        if started.elapsed() >= SCAN_TIME_BUDGET || result.entries >= SIZE_WALK_MAX_ENTRIES {
            result.truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            result.entries += 1;
            if metadata.is_dir() {
                stack.push(path);
            } else {
                result.bytes = result.bytes.saturating_add(metadata.len());
            }
        }
    }
    result
}

fn summarize_storage_items(
    items: &[StorageHygieneItem],
    scanned_directory_count: u64,
) -> StorageHygieneSummary {
    let mut summary = StorageHygieneSummary {
        item_count: items.len(),
        scanned_directory_count,
        ..StorageHygieneSummary::default()
    };

    for item in items {
        summary.total_reclaimable_bytes = summary
            .total_reclaimable_bytes
            .saturating_add(item.size_bytes);
        if item.safety == "safe" {
            summary.safe_candidate_count += 1;
        } else {
            summary.review_candidate_count += 1;
        }
        if item.stale {
            summary.stale_candidate_count += 1;
        }
        if item.size_bytes > summary.largest_item_bytes {
            summary.largest_item_bytes = item.size_bytes;
            summary.largest_item_path = Some(item.path.clone());
        }
    }

    summary
}

fn normalize_roots(roots: Vec<String>) -> Vec<PathBuf> {
    let selected = if roots.is_empty() {
        default_storage_roots()
    } else {
        roots
    };

    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for root in selected.into_iter().take(MAX_ROOTS) {
        let path = expand_home(root.trim());
        if path.as_os_str().is_empty() {
            continue;
        }
        let key = path.display().to_string();
        if seen.insert(key) {
            normalized.push(path);
        }
    }
    normalized
}

fn default_storage_roots() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    [
        "Repositories",
        "Downloads/Repositories",
        "Developer",
        "Projects",
        "Library/Developer/Xcode/DerivedData",
        "Library/Caches/org.swift.swiftpm",
        "Library/Caches/com.apple.dt.Xcode",
    ]
    .into_iter()
    .map(|relative| home.join(relative).display().to_string())
    .collect()
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn is_source_control_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | ".hg" | ".svn")
    )
}

fn system_time_millis(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

fn rule(
    kind: &'static str,
    safety: &'static str,
    reason: &'static str,
    recommendation: &'static str,
) -> ArtifactRule {
    ArtifactRule {
        kind,
        safety,
        reason,
        recommendation,
    }
}

#[cfg(test)]
pub(crate) fn build_storage_hygiene_report_for_roots(
    roots: Vec<String>,
    max_depth: usize,
    limit: usize,
) -> String {
    match serde_json::to_string(&build_storage_hygiene_report_with_options(
        roots,
        StorageHygieneOptions { max_depth, limit },
    )) {
        Ok(json) => json,
        Err(error) => panic!("storage hygiene report serializes: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_hygiene_detects_reclaimable_build_artifacts() {
        let root = test_root("detects-build-artifacts");
        let target = root.join("project").join("target").join("debug");
        if let Err(error) = fs::create_dir_all(&target) {
            panic!("create target dir: {error}");
        }
        if let Err(error) = fs::write(
            target.join("blob"),
            vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write build artifact: {error}");
        }

        let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

        assert!(json.contains("\"kind\":\"rust-build\""));
        assert!(json.contains("\"safety\":\"safe\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_hygiene_reports_missing_roots_without_failing() {
        let root = test_root("missing-root");
        let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

        assert!(json.contains("\"skipped_roots\""));
        assert!(json.contains("\"reason\":\"missing\""));
    }

    fn test_root(name: &str) -> PathBuf {
        let millis = crate::current_unix_millis().unwrap_or_default();
        std::env::temp_dir().join(format!(
            "aetower-storage-{name}-{}-{millis}",
            std::process::id()
        ))
    }
}
