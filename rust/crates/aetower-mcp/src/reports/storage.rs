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
    cleanup_tiers: Vec<StorageCleanupTierSummary>,
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
    attributed_repo_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct StorageHygieneItem {
    id: String,
    path: String,
    display_name: String,
    kind: String,
    safety: String,
    cleanup_tier: String,
    size_bytes: u64,
    size_truncated: bool,
    modified_millis: Option<u64>,
    age_days: Option<u64>,
    stale: bool,
    reason: String,
    recommendation: String,
    command_hint: String,
    attribution: StorageArtifactAttribution,
}

#[derive(Clone, Debug, Serialize)]
struct StorageArtifactAttribution {
    repo_root: Option<String>,
    repo_name: Option<String>,
    git_branch: Option<String>,
    git_head: Option<String>,
    command: Option<String>,
    process_tree: Option<String>,
    ai_agent_session: Option<String>,
    confidence: String,
    notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StorageCleanupTierSummary {
    tier: String,
    label: String,
    description: String,
    item_count: usize,
    bytes: u64,
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
    cleanup_tier: &'static str,
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
    let cleanup_tiers = summarize_cleanup_tiers(&items);
    StorageHygieneReport {
        captured_at_millis: now_millis,
        scan_duration_millis: started.elapsed().as_millis() as u64,
        summary,
        cleanup_tiers,
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
            "Command, process-tree, and AI-session attribution require file-event or runtime baselines and are marked unknown when unavailable."
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
        cleanup_tier: rule.cleanup_tier.to_owned(),
        size_bytes: size.bytes,
        size_truncated: size.truncated,
        modified_millis,
        age_days,
        stale,
        reason: rule.reason.to_owned(),
        recommendation: rule.recommendation.to_owned(),
        command_hint: format!("du -sh {}", shell_quote(&path_display)),
        attribution: artifact_attribution(path),
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
            "rebuildable",
            "Rust Cargo build output.",
            "Usually safe to remove after builds/tests are idle; Cargo will rebuild it.",
        )),
        ".build" => Some(rule(
            "swift-build",
            "safe",
            "rebuildable",
            "Swift Package Manager build output.",
            "Usually safe to remove after builds/tests are idle; SwiftPM will rebuild it.",
        )),
        "DerivedData" => Some(rule(
            "xcode-derived-data",
            "safe",
            "rebuildable",
            "Xcode DerivedData cache.",
            "Usually safe to remove when Xcode builds are idle; Xcode will recreate it.",
        )),
        "ModuleCache.noindex" => Some(rule(
            "xcode-module-cache",
            "safe",
            "rebuildable",
            "Xcode module cache.",
            "Usually safe to remove when Xcode builds are idle.",
        )),
        ".pytest_cache" | ".mypy_cache" | ".ruff_cache" | "__pycache__" => Some(rule(
            "python-cache",
            "safe",
            "rebuildable",
            "Python test/lint/import cache.",
            "Usually safe to remove; Python tools will recreate it.",
        )),
        ".turbo" | ".vite" | ".parcel-cache" => Some(rule(
            "frontend-cache",
            "safe",
            "rebuildable",
            "Frontend build cache.",
            "Usually safe to remove when frontend dev servers/builds are idle.",
        )),
        ".aetower-cache" | ".aeptus-cache" | "org.swift.swiftpm" | "com.apple.dt.Xcode" => {
            Some(rule(
                "tool-cache",
                "safe",
                "rebuildable",
                "Developer tool cache.",
                "Usually safe to remove when the related toolchain is idle; tools will recreate it.",
            ))
        }
        "coverage" => Some(rule(
            "coverage-output",
            "safe",
            "safe",
            "Test coverage output.",
            "Safe to remove if you do not need the local coverage report.",
        )),
        "tmp" | "temp" => Some(rule(
            "temporary-output",
            "review",
            "safe",
            "Local temporary output directory.",
            "Review before deleting because temporary folders can contain active run output.",
        )),
        "logs" => Some(rule(
            "logs",
            "safe",
            "safe",
            "Local logs directory.",
            "Safe to review and rotate once the relevant debugging session is over.",
        )),
        "node_modules" => Some(rule(
            "node-dependencies",
            "review",
            "expensive",
            "Node dependency install tree.",
            "Review before deleting; reinstalling may take time and requires package-manager access.",
        )),
        ".venv" | "venv" => Some(rule(
            "python-environment",
            "review",
            "expensive",
            "Python virtual environment.",
            "Review before deleting; recreate from dependency manifests if still needed.",
        )),
        "dist" | "build" => Some(rule(
            "build-output",
            "review",
            "risky",
            "Generic build output directory.",
            "Review before deleting because it may contain release artifacts.",
        )),
        "SourcePackages" => Some(rule(
            "xcode-source-packages",
            "review",
            "expensive",
            "Xcode resolved package cache.",
            "Review before deleting; Xcode can rebuild it but package resolution may take time.",
        )),
        "cache" if parent_name == ".next" => Some(rule(
            "next-cache",
            "safe",
            "rebuildable",
            "Next.js build cache.",
            "Usually safe to remove when frontend builds/dev servers are idle.",
        )),
        ".next" => Some(rule(
            "next-build",
            "review",
            "risky",
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
        if item.attribution.repo_root.is_some() {
            summary.attributed_repo_count += 1;
        }
        if item.size_bytes > summary.largest_item_bytes {
            summary.largest_item_bytes = item.size_bytes;
            summary.largest_item_path = Some(item.path.clone());
        }
    }

    summary
}

fn summarize_cleanup_tiers(items: &[StorageHygieneItem]) -> Vec<StorageCleanupTierSummary> {
    cleanup_tier_definitions()
        .into_iter()
        .map(|(tier, label, description)| {
            let mut summary = StorageCleanupTierSummary {
                tier: tier.to_owned(),
                label: label.to_owned(),
                description: description.to_owned(),
                ..StorageCleanupTierSummary::default()
            };
            for item in items.iter().filter(|item| item.cleanup_tier == tier) {
                summary.item_count += 1;
                summary.bytes = summary.bytes.saturating_add(item.size_bytes);
            }
            summary
        })
        .collect()
}

fn cleanup_tier_definitions() -> [(&'static str, &'static str, &'static str); 4] {
    [
        (
            "safe",
            "Safe",
            "Old logs, temporary output, stale crash reports, and reports that are normally safe to rotate after review.",
        ),
        (
            "rebuildable",
            "Rebuildable",
            "Compiler, package, and framework caches that should be recreated by the owning tool.",
        ),
        (
            "expensive",
            "Expensive",
            "Dependency stores and package trees that are removable but costly to restore.",
        ),
        (
            "risky",
            "Risky",
            "Generated or source-like output that may contain release artifacts or local work.",
        ),
    ]
}

fn artifact_attribution(path: &Path) -> StorageArtifactAttribution {
    let repo_root = find_git_root(path);
    let (git_branch, git_head) = repo_root
        .as_ref()
        .map(|root| read_git_head(root))
        .unwrap_or((None, None));
    let repo_name = repo_root.as_ref().and_then(|root| {
        root.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    });
    let mut notes = Vec::new();

    if repo_root.is_some() {
        notes.push("Attributed by nearest parent Git repository.".to_owned());
    } else {
        notes.push("No enclosing Git repository was found for this artifact.".to_owned());
    }
    notes.push(
        "Command, process-tree, and AI-session attribution require a file-event baseline and are unavailable in this read-only scan."
            .to_owned(),
    );
    let confidence = if repo_root.is_some() && git_branch.is_some() {
        "high"
    } else if repo_root.is_some() {
        "medium"
    } else {
        "low"
    };

    StorageArtifactAttribution {
        repo_root: repo_root.map(|root| root.display().to_string()),
        repo_name,
        git_branch,
        git_head,
        command: None,
        process_tree: None,
        ai_agent_session: None,
        confidence: confidence.to_owned(),
        notes,
    }
}

fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn read_git_head(repo_root: &Path) -> (Option<String>, Option<String>) {
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return (None, None);
    };
    let Ok(head) = fs::read_to_string(git_dir.join("HEAD")) else {
        return (None, None);
    };
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: ") {
        let branch = reference
            .strip_prefix("refs/heads/")
            .unwrap_or(reference)
            .to_owned();
        let head_sha = fs::read_to_string(git_dir.join(reference))
            .ok()
            .map(|value| short_hash(value.trim()));
        return (Some(branch), head_sha);
    }

    if head.is_empty() {
        (None, None)
    } else {
        (None, Some(short_hash(head)))
    }
}

fn resolve_git_dir(repo_root: &Path) -> Option<PathBuf> {
    let git_marker = repo_root.join(".git");
    if git_marker.is_dir() {
        return Some(git_marker);
    }
    let marker = fs::read_to_string(&git_marker).ok()?;
    let git_dir = marker.trim().strip_prefix("gitdir:")?.trim();
    let path = PathBuf::from(git_dir);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(repo_root.join(path))
    }
}

fn short_hash(value: &str) -> String {
    value.chars().take(12).collect()
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
    cleanup_tier: &'static str,
    reason: &'static str,
    recommendation: &'static str,
) -> ArtifactRule {
    ArtifactRule {
        kind,
        safety,
        cleanup_tier,
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
        assert!(json.contains("\"cleanup_tier\":\"rebuildable\""));
        assert!(json.contains("\"cleanup_tiers\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_hygiene_attributes_artifacts_to_git_repo_and_branch() {
        let root = test_root("attributes-artifacts");
        let project = root.join("Aetower");
        let refs = project.join(".git").join("refs").join("heads");
        let target = project.join("target").join("debug");
        if let Err(error) = fs::create_dir_all(&refs) {
            panic!("create git refs: {error}");
        }
        if let Err(error) = fs::create_dir_all(&target) {
            panic!("create target dir: {error}");
        }
        if let Err(error) = fs::write(
            project.join(".git").join("HEAD"),
            "ref: refs/heads/master\n",
        ) {
            panic!("write git head: {error}");
        }
        if let Err(error) = fs::write(
            refs.join("master"),
            "1234567890abcdef1234567890abcdef12345678\n",
        ) {
            panic!("write git ref: {error}");
        }
        if let Err(error) = fs::write(
            target.join("blob"),
            vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write build artifact: {error}");
        }

        let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

        assert!(json.contains("\"repo_name\":\"Aetower\""));
        assert!(json.contains("\"git_branch\":\"master\""));
        assert!(json.contains("\"git_head\":\"1234567890ab\""));
        assert!(json.contains("\"attributed_repo_count\":1"));

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
