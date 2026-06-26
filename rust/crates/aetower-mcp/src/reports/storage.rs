use std::{
    collections::{BTreeMap, BTreeSet},
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
const REPO_ARTIFACT_BUDGET_BYTES: u64 = 30 * 1024 * 1024 * 1024;
const REPO_GROWTH_BUDGET_BYTES_PER_DAY: u64 = 2 * 1024 * 1024 * 1024;
const TOTAL_ARTIFACT_BUDGET_BYTES: u64 = 30 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StorageHygieneReport {
    captured_at_millis: u64,
    scan_duration_millis: u64,
    summary: StorageHygieneSummary,
    cleanup_tiers: Vec<StorageCleanupTierSummary>,
    cleanup_recipes: Vec<StorageCleanupRecipe>,
    cleanup_bundles: Vec<StorageCleanupBundle>,
    budget_guardrails: StorageBudgetGuardrails,
    agent_hygiene: StorageAgentHygieneSummary,
    repo_footprints: Vec<StorageRepoFootprint>,
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
struct StorageCleanupRecipe {
    id: String,
    title: String,
    category: String,
    safety: String,
    affected_path: String,
    command: String,
    estimated_reclaimable_bytes: u64,
    reason: String,
    prerequisites: Vec<String>,
    destructive: bool,
    requires_review: bool,
}

#[derive(Clone, Debug, Serialize)]
struct StorageCleanupBundle {
    id: String,
    title: String,
    subtitle: String,
    safety: String,
    confidence_score: u8,
    estimated_reclaimable_bytes: u64,
    item_count: usize,
    dry_run_only: bool,
    manifest: Vec<StorageCleanupBundleItem>,
    dry_run_commands: Vec<String>,
    rollback_notes: Vec<String>,
    prerequisites: Vec<String>,
    caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageCleanupBundleItem {
    path: String,
    display_name: String,
    kind: String,
    cleanup_tier: String,
    safety: String,
    size_bytes: u64,
    confidence_score: u8,
    dry_run_command: String,
    cleanup_command: Option<String>,
    rollback_note: String,
    reason: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageBudgetGuardrails {
    repo_growth_budget_bytes_per_day: u64,
    repo_artifact_budget_bytes: u64,
    total_artifact_budget_bytes: u64,
    status: String,
    violations: Vec<StorageBudgetViolation>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageBudgetViolation {
    id: String,
    scope: String,
    severity: String,
    title: String,
    detail: String,
    repo_root: Option<String>,
    repo_name: Option<String>,
    observed_bytes: u64,
    limit_bytes: u64,
    recommendation: String,
}

#[derive(Clone, Debug, Default, Serialize)]
struct StorageAgentHygieneSummary {
    total_agent_artifact_bytes: u64,
    week_agent_artifact_bytes: u64,
    rebuildable_agent_bytes: u64,
    rebuildable_agent_percent: f32,
    week_rebuildable_agent_bytes: u64,
    week_rebuildable_agent_percent: f32,
    attributed_item_count: usize,
    agent_count: usize,
    agents: Vec<StorageAgentArtifactSummary>,
    caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAgentArtifactSummary {
    id: String,
    provider: String,
    display_name: String,
    session_id: Option<String>,
    artifact_bytes: u64,
    week_artifact_bytes: u64,
    rebuildable_bytes: u64,
    rebuildable_percent: f32,
    week_rebuildable_bytes: u64,
    week_rebuildable_percent: f32,
    item_count: usize,
    repo_count: usize,
    top_repositories: Vec<StorageAgentRepoSummary>,
    top_items: Vec<StorageAgentItemSummary>,
    confidence: String,
    attribution_sources: Vec<String>,
    recommendation: String,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAgentRepoSummary {
    repo_root: String,
    repo_name: String,
    artifact_bytes: u64,
    item_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct StorageAgentItemSummary {
    path: String,
    display_name: String,
    kind: String,
    cleanup_tier: String,
    size_bytes: u64,
    modified_millis: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageRepoFootprint {
    id: String,
    repo_root: String,
    repo_name: String,
    current_size_bytes: u64,
    artifact_bytes: u64,
    item_count: usize,
    top_artifact_folders: Vec<StorageRepoArtifactFolder>,
    last_writer_process: Option<String>,
    last_writer_pid: Option<u32>,
    last_branch_touched: Option<String>,
    growth_bytes: Option<i64>,
    growth_window: String,
    estimated_rebuild_cost: String,
    estimated_rebuild_seconds: Option<u64>,
    caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct StorageRepoArtifactFolder {
    path: String,
    display_name: String,
    kind: String,
    cleanup_tier: String,
    size_bytes: u64,
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
    let cleanup_recipes = build_cleanup_recipes(&items);
    let cleanup_bundles = build_cleanup_bundles(&items);
    let repo_footprints = summarize_repo_footprints(&items);
    let budget_guardrails = evaluate_budget_guardrails(&summary, &repo_footprints);
    let agent_hygiene = summarize_agent_hygiene(&items);
    StorageHygieneReport {
        captured_at_millis: now_millis,
        scan_duration_millis: started.elapsed().as_millis() as u64,
        summary,
        cleanup_tiers,
        cleanup_recipes,
        cleanup_bundles,
        budget_guardrails,
        agent_hygiene,
        repo_footprints,
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
            "Command and process-tree attribution require file-event or runtime baselines; known local AI-agent directories are inferred conservatively."
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
    let inferred_agent_session =
        known_agent_path(path).map(|(_, display_name)| format!("{display_name} local artifacts"));
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
    if inferred_agent_session.is_some() {
        notes.push(
            "AI agent attribution inferred from a known local agent support directory.".to_owned(),
        );
    } else {
        notes.push(
            "Command, process-tree, and exact AI-session attribution require a file-event baseline."
                .to_owned(),
        );
    }
    let confidence = if repo_root.is_some() && git_branch.is_some() {
        "high"
    } else if repo_root.is_some() || inferred_agent_session.is_some() {
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
        ai_agent_session: inferred_agent_session,
        confidence: confidence.to_owned(),
        notes,
    }
}

fn summarize_repo_footprints(items: &[StorageHygieneItem]) -> Vec<StorageRepoFootprint> {
    let mut grouped = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        if let Some(repo_root) = item.attribution.repo_root.as_deref() {
            grouped.entry(repo_root.to_owned()).or_default().push(item);
        }
    }

    let mut footprints: Vec<_> = grouped
        .into_iter()
        .map(|(repo_root, items)| repo_footprint_for_items(repo_root, items))
        .collect();
    footprints.sort_by(|left, right| {
        right
            .current_size_bytes
            .cmp(&left.current_size_bytes)
            .then_with(|| left.repo_name.cmp(&right.repo_name))
    });
    footprints
}

fn build_cleanup_recipes(items: &[StorageHygieneItem]) -> Vec<StorageCleanupRecipe> {
    let mut recipes = BTreeMap::<String, StorageCleanupRecipe>::new();
    for item in items {
        for recipe in cleanup_recipes_for_item(item) {
            recipes
                .entry(recipe.id.clone())
                .and_modify(|existing| {
                    existing.estimated_reclaimable_bytes = existing
                        .estimated_reclaimable_bytes
                        .saturating_add(recipe.estimated_reclaimable_bytes);
                })
                .or_insert(recipe);
        }
    }

    let mut recipes: Vec<_> = recipes.into_values().collect();
    recipes.sort_by(|left, right| {
        right
            .estimated_reclaimable_bytes
            .cmp(&left.estimated_reclaimable_bytes)
            .then_with(|| left.title.cmp(&right.title))
    });
    recipes.truncate(16);
    recipes
}

fn cleanup_recipes_for_item(item: &StorageHygieneItem) -> Vec<StorageCleanupRecipe> {
    match item.kind.as_str() {
        "rust-build" => rust_cleanup_recipe(item).into_iter().collect(),
        "swift-build" => swiftpm_cleanup_recipe(item).into_iter().collect(),
        "xcode-derived-data" => vec![derived_data_cleanup_recipe(item)],
        "xcode-module-cache" => vec![direct_reclaim_recipe(
            item,
            "xcode",
            "Clear Xcode module cache",
            "Xcode module cache is rebuildable once active Xcode builds are idle.",
            vec![
                "Close Xcode or stop active xcodebuild jobs first.".to_owned(),
                "Expect the next Xcode build to rebuild modules.".to_owned(),
            ],
            false,
        )],
        "xcode-source-packages" => vec![direct_reclaim_recipe(
            item,
            "xcode",
            "Remove Xcode source package cache",
            "Xcode can restore resolved package checkouts, but package resolution may take time and network access.",
            vec![
                "Close Xcode or stop active xcodebuild jobs first.".to_owned(),
                "Confirm package manifests are committed before deleting package caches.".to_owned(),
            ],
            true,
        )],
        "python-cache" => vec![direct_reclaim_recipe(
            item,
            "python",
            "Clear Python cache",
            "Python test, lint, and import caches are rebuildable.",
            vec!["Stop active Python test/lint runs first.".to_owned()],
            false,
        )],
        "python-environment" => vec![direct_reclaim_recipe(
            item,
            "python",
            "Remove Python virtual environment",
            "Virtual environments are rebuildable from dependency manifests but may be expensive to recreate.",
            vec![
                "Confirm requirements, lock files, or project metadata can recreate this environment.".to_owned(),
                "Stop terminals and agents using this virtual environment first.".to_owned(),
            ],
            true,
        )],
        "frontend-cache" | "next-cache" => vec![direct_reclaim_recipe(
            item,
            "frontend",
            "Clear frontend cache",
            "Frontend tool caches are rebuildable when dev servers and builds are idle.",
            vec!["Stop active frontend dev servers and builds first.".to_owned()],
            false,
        )],
        "next-build" => vec![direct_reclaim_recipe(
            item,
            "frontend",
            "Remove Next.js build output",
            "Next.js build output is rebuildable, but can include generated app artifacts that deserve review.",
            vec![
                "Stop active frontend dev servers and builds first.".to_owned(),
                "Confirm this is local build output, not a release artifact you still need.".to_owned(),
            ],
            true,
        )],
        "node-dependencies" => vec![direct_reclaim_recipe(
            item,
            "node",
            "Remove Node dependency tree",
            "node_modules can be reclaimed but reinstalling may be slow and requires package-manager access.",
            vec![
                "Confirm package-lock, pnpm-lock, yarn.lock, or bun.lockb is present before deleting.".to_owned(),
                "Stop dev servers, test runners, and agents using this dependency tree first.".to_owned(),
            ],
            true,
        )],
        "tool-cache" => vec![direct_reclaim_recipe(
            item,
            "tools",
            "Clear developer tool cache",
            "Tool caches are rebuildable when the owning toolchain is idle.",
            vec!["Stop active builds, package managers, and agents using this cache first.".to_owned()],
            false,
        )],
        "coverage-output" => vec![direct_reclaim_recipe(
            item,
            "tests",
            "Remove coverage output",
            "Coverage reports are usually safe to remove after you no longer need local inspection artifacts.",
            vec!["Keep a copy first if this coverage report is needed for review or support.".to_owned()],
            false,
        )],
        "temporary-output" => vec![direct_reclaim_recipe(
            item,
            "temporary",
            "Remove temporary output",
            "Temporary folders can be reclaimed, but may contain active run output.",
            vec!["Review the folder contents and stop active jobs before deleting.".to_owned()],
            true,
        )],
        "log-file" | "logs" => vec![log_cleanup_recipe(item)],
        "build-output" => stale_release_artifact_recipe(item).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn build_cleanup_bundles(items: &[StorageHygieneItem]) -> Vec<StorageCleanupBundle> {
    let mut bundles = Vec::new();
    let safe_candidates: Vec<_> = items
        .iter()
        .filter(|item| {
            item.safety == "safe"
                && matches!(item.cleanup_tier.as_str(), "safe" | "rebuildable")
                && !item.size_truncated
        })
        .collect();
    if let Some(bundle) = cleanup_bundle_for_items(
        "safe-reclaim",
        "Reclaim safely",
        "High-confidence local artifacts. This is still a dry run: review the manifest before running any cleanup command.",
        "safe",
        safe_candidates,
    ) {
        bundles.push(bundle);
    }

    let review_candidates: Vec<_> = items
        .iter()
        .filter(|item| {
            !item.size_truncated
                && item.cleanup_tier != "risky"
                && !(item.safety == "safe"
                    && matches!(item.cleanup_tier.as_str(), "safe" | "rebuildable"))
        })
        .collect();
    if let Some(bundle) = cleanup_bundle_for_items(
        "review-reclaim",
        "Review reclaim plan",
        "Operator-reviewed candidates such as dependency trees or local environments. Treat as planning data, not an automatic cleanup.",
        "review",
        review_candidates,
    ) {
        bundles.push(bundle);
    }

    bundles.sort_by(|left, right| {
        right
            .estimated_reclaimable_bytes
            .cmp(&left.estimated_reclaimable_bytes)
            .then_with(|| right.confidence_score.cmp(&left.confidence_score))
            .then_with(|| left.title.cmp(&right.title))
    });
    bundles.truncate(3);
    bundles
}

fn cleanup_bundle_for_items(
    id: &str,
    title: &str,
    subtitle: &str,
    safety: &str,
    items: Vec<&StorageHygieneItem>,
) -> Option<StorageCleanupBundle> {
    if items.is_empty() {
        return None;
    }
    let mut manifest: Vec<_> = items.into_iter().map(cleanup_bundle_item).collect();
    manifest.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });

    let estimated_reclaimable_bytes = manifest
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    let confidence_score = average_confidence(&manifest);
    let dry_run_commands = manifest
        .iter()
        .take(16)
        .map(|item| item.dry_run_command.clone())
        .collect();
    let rollback_notes = unique_limited(
        manifest
            .iter()
            .map(|item| item.rollback_note.clone())
            .collect(),
        6,
    );

    Some(StorageCleanupBundle {
        id: id.to_owned(),
        title: format!("{title}: {}", human_bytes(estimated_reclaimable_bytes)),
        subtitle: subtitle.to_owned(),
        safety: safety.to_owned(),
        confidence_score,
        estimated_reclaimable_bytes,
        item_count: manifest.len(),
        dry_run_only: true,
        manifest,
        dry_run_commands,
        rollback_notes,
        prerequisites: cleanup_bundle_prerequisites(safety),
        caveats: vec![
            "Aetower does not delete files from dry-run bundles.".to_owned(),
            "Verify active builds, tests, terminals, and agents are idle before running copied cleanup commands.".to_owned(),
        ],
    })
}

fn cleanup_bundle_item(item: &StorageHygieneItem) -> StorageCleanupBundleItem {
    let cleanup_command = cleanup_recipes_for_item(item)
        .into_iter()
        .next()
        .map(|recipe| recipe.command);
    StorageCleanupBundleItem {
        path: item.path.clone(),
        display_name: item.display_name.clone(),
        kind: item.kind.clone(),
        cleanup_tier: item.cleanup_tier.clone(),
        safety: item.safety.clone(),
        size_bytes: item.size_bytes,
        confidence_score: cleanup_item_confidence(item),
        dry_run_command: item.command_hint.clone(),
        cleanup_command,
        rollback_note: cleanup_item_rollback_note(item),
        reason: item.reason.clone(),
    }
}

fn cleanup_bundle_prerequisites(safety: &str) -> Vec<String> {
    let mut prerequisites = vec![
        "Run the dry-run commands first and confirm the manifest matches your intent.".to_owned(),
        "Stop active builds, tests, package managers, and AI agents that may be writing these paths."
            .to_owned(),
    ];
    if safety != "safe" {
        prerequisites.push(
            "Review every candidate manually; this bundle intentionally includes non-safe items."
                .to_owned(),
        );
    }
    prerequisites
}

fn cleanup_item_confidence(item: &StorageHygieneItem) -> u8 {
    let base: u8 = match item.cleanup_tier.as_str() {
        "rebuildable" if item.safety == "safe" => 94,
        "safe" if item.safety == "safe" => 88,
        "expensive" => 62,
        "risky" => 35,
        _ => 70,
    };
    let stale_bonus: u8 = if item.stale { 3 } else { 0 };
    let truncation_penalty: u8 = if item.size_truncated { 20 } else { 0 };
    (base + stale_bonus)
        .saturating_sub(truncation_penalty)
        .clamp(1, 99)
}

fn cleanup_item_rollback_note(item: &StorageHygieneItem) -> String {
    match item.cleanup_tier.as_str() {
        "rebuildable" => {
            "Rollback: rebuild or rerun the owning package manager/tool; source files are not expected to be affected.".to_owned()
        }
        "safe" if item.kind == "log-file" || item.kind == "logs" => {
            "Rollback: log truncation is not reversible unless you copy the log first; keep evidence needed for support.".to_owned()
        }
        "safe" => {
            "Rollback: usually not needed, but keep a copy first if this artifact is required for debugging.".to_owned()
        }
        "expensive" => {
            "Rollback: reinstall dependencies or recreate the environment from manifests; expect network and rebuild cost.".to_owned()
        }
        "risky" => {
            "Rollback: restore from source control, release archives, or backups; do not automate without manual confirmation.".to_owned()
        }
        _ => "Rollback depends on the owning tool; review before cleanup.".to_owned(),
    }
}

fn average_confidence(items: &[StorageCleanupBundleItem]) -> u8 {
    if items.is_empty() {
        return 0;
    }
    let total: u64 = items.iter().map(|item| item.confidence_score as u64).sum();
    (total / items.len() as u64) as u8
}

fn unique_limited(values: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            unique.push(value);
        }
        if unique.len() >= limit {
            break;
        }
    }
    unique
}

fn evaluate_budget_guardrails(
    summary: &StorageHygieneSummary,
    repo_footprints: &[StorageRepoFootprint],
) -> StorageBudgetGuardrails {
    let mut violations = Vec::new();

    if summary.total_reclaimable_bytes > TOTAL_ARTIFACT_BUDGET_BYTES {
        violations.push(StorageBudgetViolation {
            id: "total-artifact-budget".to_owned(),
            scope: "global".to_owned(),
            severity: "warning".to_owned(),
            title: "Local dev artifacts exceed budget".to_owned(),
            detail: format!(
                "Aetower found {} of local development artifacts; budget is {}.",
                human_bytes(summary.total_reclaimable_bytes),
                human_bytes(TOTAL_ARTIFACT_BUDGET_BYTES)
            ),
            repo_root: None,
            repo_name: None,
            observed_bytes: summary.total_reclaimable_bytes,
            limit_bytes: TOTAL_ARTIFACT_BUDGET_BYTES,
            recommendation: "Review cleanup recipes and reclaim rebuildable caches before the machine starts paging or indexing excessively.".to_owned(),
        });
    }

    for footprint in repo_footprints {
        if footprint.current_size_bytes > REPO_ARTIFACT_BUDGET_BYTES {
            violations.push(StorageBudgetViolation {
                id: format!("repo-artifact-budget|{}", footprint.repo_root),
                scope: "repo".to_owned(),
                severity: "warning".to_owned(),
                title: format!("{} exceeds repo artifact budget", footprint.repo_name),
                detail: format!(
                    "{} has {} of attributed artifacts; budget is {} per repository.",
                    footprint.repo_name,
                    human_bytes(footprint.current_size_bytes),
                    human_bytes(REPO_ARTIFACT_BUDGET_BYTES)
                ),
                repo_root: Some(footprint.repo_root.clone()),
                repo_name: Some(footprint.repo_name.clone()),
                observed_bytes: footprint.current_size_bytes,
                limit_bytes: REPO_ARTIFACT_BUDGET_BYTES,
                recommendation: "Clean rebuildable build outputs or narrow expensive dependency caches for this repository.".to_owned(),
            });
        }
        if let Some(growth_bytes) = footprint.growth_bytes
            && growth_bytes > REPO_GROWTH_BUDGET_BYTES_PER_DAY as i64
        {
            violations.push(StorageBudgetViolation {
                id: format!("repo-growth-budget|{}", footprint.repo_root),
                scope: "repo-growth".to_owned(),
                severity: "warning".to_owned(),
                title: format!("{} is growing too fast", footprint.repo_name),
                detail: format!(
                    "{} grew by {} in {}; budget is {} per day.",
                    footprint.repo_name,
                    human_bytes(growth_bytes as u64),
                    footprint.growth_window,
                    human_bytes(REPO_GROWTH_BUDGET_BYTES_PER_DAY)
                ),
                repo_root: Some(footprint.repo_root.clone()),
                repo_name: Some(footprint.repo_name.clone()),
                observed_bytes: growth_bytes as u64,
                limit_bytes: REPO_GROWTH_BUDGET_BYTES_PER_DAY,
                recommendation: "Inspect the disk growth timeline and active build/session activity before the repository accumulates more artifacts.".to_owned(),
            });
        }
    }

    let status = if violations.is_empty() {
        "ok"
    } else if violations
        .iter()
        .any(|violation| violation.severity == "critical")
    {
        "critical"
    } else {
        "warning"
    }
    .to_owned();

    StorageBudgetGuardrails {
        repo_growth_budget_bytes_per_day: REPO_GROWTH_BUDGET_BYTES_PER_DAY,
        repo_artifact_budget_bytes: REPO_ARTIFACT_BUDGET_BYTES,
        total_artifact_budget_bytes: TOTAL_ARTIFACT_BUDGET_BYTES,
        status,
        violations,
    }
}

#[derive(Clone, Debug)]
struct AgentAttributionEvidence {
    id: String,
    provider: String,
    display_name: String,
    session_id: Option<String>,
    confidence: String,
    source: String,
}

#[derive(Clone, Debug)]
struct AgentArtifactAccumulator {
    id: String,
    provider: String,
    display_name: String,
    session_id: Option<String>,
    artifact_bytes: u64,
    week_artifact_bytes: u64,
    rebuildable_bytes: u64,
    week_rebuildable_bytes: u64,
    item_count: usize,
    repos: BTreeMap<String, AgentRepoAccumulator>,
    top_items: Vec<StorageAgentItemSummary>,
    confidence: String,
    confidence_rank: u8,
    attribution_sources: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
struct AgentRepoAccumulator {
    repo_root: String,
    repo_name: String,
    artifact_bytes: u64,
    item_count: usize,
}

impl AgentArtifactAccumulator {
    fn new(evidence: AgentAttributionEvidence) -> Self {
        let mut attribution_sources = BTreeSet::new();
        attribution_sources.insert(evidence.source);
        let confidence_rank = confidence_rank(&evidence.confidence);
        Self {
            id: evidence.id,
            provider: evidence.provider,
            display_name: evidence.display_name,
            session_id: evidence.session_id,
            artifact_bytes: 0,
            week_artifact_bytes: 0,
            rebuildable_bytes: 0,
            week_rebuildable_bytes: 0,
            item_count: 0,
            repos: BTreeMap::new(),
            top_items: Vec::new(),
            confidence: evidence.confidence,
            confidence_rank,
            attribution_sources,
        }
    }

    fn merge_evidence(&mut self, evidence: AgentAttributionEvidence) {
        self.attribution_sources.insert(evidence.source);
        let rank = confidence_rank(&evidence.confidence);
        if rank > self.confidence_rank {
            self.confidence = evidence.confidence;
            self.confidence_rank = rank;
        }
        if self.session_id.is_none() {
            self.session_id = evidence.session_id;
        }
    }

    fn add_item(&mut self, item: &StorageHygieneItem) {
        self.artifact_bytes = self.artifact_bytes.saturating_add(item.size_bytes);
        self.item_count += 1;
        let is_rebuildable = item.cleanup_tier == "rebuildable";
        if is_rebuildable {
            self.rebuildable_bytes = self.rebuildable_bytes.saturating_add(item.size_bytes);
        }
        if item.age_days.is_some_and(|days| days <= 7) {
            self.week_artifact_bytes = self.week_artifact_bytes.saturating_add(item.size_bytes);
            if is_rebuildable {
                self.week_rebuildable_bytes =
                    self.week_rebuildable_bytes.saturating_add(item.size_bytes);
            }
        }

        if let Some(repo_root) = item.attribution.repo_root.as_deref() {
            let repo =
                self.repos
                    .entry(repo_root.to_owned())
                    .or_insert_with(|| AgentRepoAccumulator {
                        repo_root: repo_root.to_owned(),
                        repo_name: item
                            .attribution
                            .repo_name
                            .clone()
                            .or_else(|| {
                                Path::new(repo_root)
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .map(str::to_owned)
                            })
                            .unwrap_or_else(|| "repository".to_owned()),
                        ..Default::default()
                    });
            repo.artifact_bytes = repo.artifact_bytes.saturating_add(item.size_bytes);
            repo.item_count += 1;
        }

        self.top_items.push(StorageAgentItemSummary {
            path: item.path.clone(),
            display_name: item.display_name.clone(),
            kind: item.kind.clone(),
            cleanup_tier: item.cleanup_tier.clone(),
            size_bytes: item.size_bytes,
            modified_millis: item.modified_millis,
        });
    }

    fn into_summary(mut self) -> StorageAgentArtifactSummary {
        let repo_count = self.repos.len();
        let mut top_repositories: Vec<_> = self
            .repos
            .into_values()
            .map(|repo| StorageAgentRepoSummary {
                repo_root: repo.repo_root,
                repo_name: repo.repo_name,
                artifact_bytes: repo.artifact_bytes,
                item_count: repo.item_count,
            })
            .collect();
        top_repositories.sort_by(|left, right| {
            right
                .artifact_bytes
                .cmp(&left.artifact_bytes)
                .then_with(|| left.repo_name.cmp(&right.repo_name))
        });
        top_repositories.truncate(4);

        self.top_items.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        self.top_items.truncate(4);

        StorageAgentArtifactSummary {
            id: self.id,
            provider: self.provider,
            display_name: self.display_name,
            session_id: self.session_id,
            artifact_bytes: self.artifact_bytes,
            week_artifact_bytes: self.week_artifact_bytes,
            rebuildable_bytes: self.rebuildable_bytes,
            rebuildable_percent: percent(self.rebuildable_bytes, self.artifact_bytes),
            week_rebuildable_bytes: self.week_rebuildable_bytes,
            week_rebuildable_percent: percent(
                self.week_rebuildable_bytes,
                self.week_artifact_bytes,
            ),
            item_count: self.item_count,
            repo_count,
            top_repositories,
            top_items: self.top_items,
            confidence: self.confidence,
            attribution_sources: self.attribution_sources.into_iter().collect(),
            recommendation: agent_cleanup_recommendation(
                self.artifact_bytes,
                self.rebuildable_bytes,
                self.week_artifact_bytes,
            ),
        }
    }
}

fn summarize_agent_hygiene(items: &[StorageHygieneItem]) -> StorageAgentHygieneSummary {
    let mut grouped = BTreeMap::<String, AgentArtifactAccumulator>::new();

    for item in items {
        let Some(evidence) = infer_agent_evidence(item) else {
            continue;
        };
        grouped
            .entry(evidence.id.clone())
            .and_modify(|entry| entry.merge_evidence(evidence.clone()))
            .or_insert_with(|| AgentArtifactAccumulator::new(evidence))
            .add_item(item);
    }

    let mut agents: Vec<_> = grouped
        .into_values()
        .map(AgentArtifactAccumulator::into_summary)
        .collect();
    agents.sort_by(|left, right| {
        right
            .week_artifact_bytes
            .cmp(&left.week_artifact_bytes)
            .then_with(|| right.artifact_bytes.cmp(&left.artifact_bytes))
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    agents.truncate(12);

    let total_agent_artifact_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.artifact_bytes)
    });
    let week_agent_artifact_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.week_artifact_bytes)
    });
    let rebuildable_agent_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.rebuildable_bytes)
    });
    let week_rebuildable_agent_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.week_rebuildable_bytes)
    });
    let attributed_item_count = agents.iter().fold(0usize, |total, agent| {
        total.saturating_add(agent.item_count)
    });

    StorageAgentHygieneSummary {
        total_agent_artifact_bytes,
        week_agent_artifact_bytes,
        rebuildable_agent_bytes,
        rebuildable_agent_percent: percent(rebuildable_agent_bytes, total_agent_artifact_bytes),
        week_rebuildable_agent_bytes,
        week_rebuildable_agent_percent: percent(
            week_rebuildable_agent_bytes,
            week_agent_artifact_bytes,
        ),
        attributed_item_count,
        agent_count: agents.len(),
        agents,
        caveats: vec![
            "Agent cost is direct attribution only: explicit AI session metadata, command/process-tree evidence, or known local agent directories."
                .to_owned(),
            "Repository build artifacts are not blamed on an agent unless Aetower has writer evidence."
                .to_owned(),
        ],
    }
}

fn infer_agent_evidence(item: &StorageHygieneItem) -> Option<AgentAttributionEvidence> {
    if let Some(session) = item.attribution.ai_agent_session.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(session)
    {
        let inferred_local_artifacts = session.ends_with(" local artifacts");
        return Some(AgentAttributionEvidence {
            id: if inferred_local_artifacts {
                format!("known-path|{provider}")
            } else {
                format!("session|{}|{}", provider, session)
            },
            provider,
            display_name,
            session_id: Some(session.to_owned()),
            confidence: if inferred_local_artifacts {
                "medium".to_owned()
            } else {
                "high".to_owned()
            },
            source: if inferred_local_artifacts {
                "known_agent_directory".to_owned()
            } else {
                "ai_agent_session".to_owned()
            },
        });
    }

    if let Some(command) = item.attribution.command.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(command)
    {
        return Some(AgentAttributionEvidence {
            id: format!("command|{provider}"),
            provider,
            display_name,
            session_id: None,
            confidence: "medium".to_owned(),
            source: "command".to_owned(),
        });
    }

    if let Some(process_tree) = item.attribution.process_tree.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(process_tree)
    {
        return Some(AgentAttributionEvidence {
            id: format!("process-tree|{provider}"),
            provider,
            display_name,
            session_id: None,
            confidence: "medium".to_owned(),
            source: "process_tree".to_owned(),
        });
    }

    let path = Path::new(&item.path);
    known_agent_path(path).map(|(provider, display_name)| AgentAttributionEvidence {
        id: format!("known-path|{provider}"),
        provider,
        display_name,
        session_id: None,
        confidence: "medium".to_owned(),
        source: "known_agent_directory".to_owned(),
    })
}

fn agent_provider_from_text(value: &str) -> Option<(String, String)> {
    let lowered = value.to_ascii_lowercase();
    if lowered.contains("claude") {
        return Some(("claude".to_owned(), "Claude Code".to_owned()));
    }
    if lowered.contains("codex") {
        return Some(("codex".to_owned(), "Codex".to_owned()));
    }
    if lowered.contains("cursor-agent") || lowered.contains("cursor") {
        return Some(("cursor".to_owned(), "Cursor".to_owned()));
    }
    if lowered.contains("aider") {
        return Some(("aider".to_owned(), "Aider".to_owned()));
    }
    None
}

fn known_agent_path(path: &Path) -> Option<(String, String)> {
    for component in path.components() {
        let normalized = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        match normalized.as_str() {
            ".claude" => return Some(("claude".to_owned(), "Claude Code".to_owned())),
            ".codex" => return Some(("codex".to_owned(), "Codex".to_owned())),
            ".cursor" => return Some(("cursor".to_owned(), "Cursor".to_owned())),
            ".aider" => return Some(("aider".to_owned(), "Aider".to_owned())),
            _ => {}
        }
    }
    None
}

fn agent_cleanup_recommendation(
    artifact_bytes: u64,
    rebuildable_bytes: u64,
    week_artifact_bytes: u64,
) -> String {
    if artifact_bytes == 0 {
        return "No agent-attributed storage pressure detected.".to_owned();
    }
    let rebuildable_percent = percent(rebuildable_bytes, artifact_bytes);
    if rebuildable_percent >= 75.0 {
        return "Most attributed bytes are rebuildable; clean them after the agent session and related builds are idle.".to_owned();
    }
    if week_artifact_bytes >= REPO_GROWTH_BUDGET_BYTES_PER_DAY {
        return "This agent added significant storage this week; inspect the top items before starting more build-heavy tasks.".to_owned();
    }
    "Review top items and prefer targeted cleanup recipes over broad cache deletion.".to_owned()
}

fn percent(part: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    ((part as f64 / total as f64) * 1000.0).round() as f32 / 10.0
}

fn confidence_rank(confidence: &str) -> u8 {
    match confidence {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn direct_reclaim_recipe(
    item: &StorageHygieneItem,
    category: &str,
    title: &str,
    reason: &str,
    mut prerequisites: Vec<String>,
    requires_review: bool,
) -> StorageCleanupRecipe {
    prerequisites
        .push("Reveal and inspect the target before running the copied command.".to_owned());
    let command = if Path::new(&item.path).is_file() {
        format!("rm -f {}", shell_quote(&item.path))
    } else {
        format!("rm -rf {}", shell_quote(&item.path))
    };
    StorageCleanupRecipe {
        id: format!("{category}-reclaim|{}", item.path),
        title: title.to_owned(),
        category: category.to_owned(),
        safety: item.cleanup_tier.clone(),
        affected_path: item.path.clone(),
        command,
        estimated_reclaimable_bytes: item.size_bytes,
        reason: reason.to_owned(),
        prerequisites,
        destructive: true,
        requires_review: requires_review || item.safety != "safe",
    }
}

fn rust_cleanup_recipe(item: &StorageHygieneItem) -> Option<StorageCleanupRecipe> {
    let path = Path::new(&item.path);
    let manifest_dir = nearest_parent_with_file(path, "Cargo.toml")?;
    let manifest_path = manifest_dir.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap_or_default();
    let package = if manifest.contains("aetower-ffi") {
        Some("aetower-ffi")
    } else {
        None
    };
    let command = if let Some(package) = package {
        format!(
            "cd {} && cargo clean -p {package}",
            shell_quote(&manifest_dir.display().to_string())
        )
    } else {
        format!(
            "cd {} && cargo clean",
            shell_quote(&manifest_dir.display().to_string())
        )
    };
    Some(StorageCleanupRecipe {
        id: format!("rust-clean|{}", manifest_dir.display()),
        title: package
            .map(|package| format!("Clean Rust package {package}"))
            .unwrap_or_else(|| "Clean Rust build artifacts".to_owned()),
        category: "rust".to_owned(),
        safety: "rebuildable".to_owned(),
        affected_path: item.path.clone(),
        command,
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Cargo build output is rebuildable and can usually be reclaimed with cargo clean once builds/tests are idle.".to_owned(),
        prerequisites: vec![
            "Stop active cargo builds/tests first.".to_owned(),
            "Expect the next Rust build to recompile dependencies.".to_owned(),
        ],
        destructive: true,
        requires_review: false,
    })
}

fn swiftpm_cleanup_recipe(item: &StorageHygieneItem) -> Option<StorageCleanupRecipe> {
    let package_root = Path::new(&item.path).parent()?;
    Some(StorageCleanupRecipe {
        id: format!("swiftpm-clean|{}", package_root.display()),
        title: "Clear SwiftPM .build".to_owned(),
        category: "swiftpm".to_owned(),
        safety: "rebuildable".to_owned(),
        affected_path: item.path.clone(),
        command: format!(
            "cd {} && swift package clean",
            shell_quote(&package_root.display().to_string())
        ),
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "SwiftPM .build output is rebuildable; swift package clean removes package build products without touching sources.".to_owned(),
        prerequisites: vec![
            "Stop active Swift builds/tests first.".to_owned(),
            "Expect the next Swift build to fetch or rebuild dependencies if needed.".to_owned(),
        ],
        destructive: true,
        requires_review: false,
    })
}

fn derived_data_cleanup_recipe(item: &StorageHygieneItem) -> StorageCleanupRecipe {
    StorageCleanupRecipe {
        id: format!("xcode-derived-data|{}", item.path),
        title: "Prune old Xcode DerivedData".to_owned(),
        category: "xcode".to_owned(),
        safety: "rebuildable".to_owned(),
        affected_path: item.path.clone(),
        command: format!(
            "find {} -mindepth 1 -maxdepth 1 -mtime +14 -exec rm -rf {{}} +",
            shell_quote(&item.path)
        ),
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Old DerivedData entries for inactive projects are rebuildable but can consume significant disk.".to_owned(),
        prerequisites: vec![
            "Close Xcode or stop active xcodebuild jobs first.".to_owned(),
            "Review the matching folders with the same find command using -print before running deletion.".to_owned(),
        ],
        destructive: true,
        requires_review: true,
    }
}

fn log_cleanup_recipe(item: &StorageHygieneItem) -> StorageCleanupRecipe {
    let path = Path::new(&item.path);
    let command = if path.is_file() {
        format!(": > {}", shell_quote(&item.path))
    } else {
        format!(
            "find {} -type f -name '*.log' -size +50M -exec sh -c ': > \"$1\"' sh {{}} \\;",
            shell_quote(&item.path)
        )
    };
    StorageCleanupRecipe {
        id: format!("logs-truncate|{}", item.path),
        title: "Truncate oversized local logs".to_owned(),
        category: "logs".to_owned(),
        safety: "safe".to_owned(),
        affected_path: item.path.clone(),
        command,
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Large local logs are usually safe to truncate after the relevant debugging session is over.".to_owned(),
        prerequisites: vec![
            "Keep a copy first if the log is needed for a bug report.".to_owned(),
            "Avoid truncating logs that are actively being collected for support.".to_owned(),
        ],
        destructive: true,
        requires_review: item.safety != "safe",
    }
}

fn stale_release_artifact_recipe(item: &StorageHygieneItem) -> Option<StorageCleanupRecipe> {
    if !item.stale {
        return None;
    }
    Some(StorageCleanupRecipe {
        id: format!("release-artifacts|{}", item.path),
        title: "Remove stale release artifacts".to_owned(),
        category: "release".to_owned(),
        safety: "risky".to_owned(),
        affected_path: item.path.clone(),
        command: format!(
            "find {} -type f \\( -name '*.zip' -o -name '*.pkg' -o -name '*.dmg' -o -name '*.tar.gz' \\) -mtime +14 -delete",
            shell_quote(&item.path)
        ),
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Stale release folders can contain packages superseded by newer versions, but Aetower cannot prove supersession without release metadata.".to_owned(),
        prerequisites: vec![
            "Confirm newer signed/notarized artifacts exist before deleting old packages.".to_owned(),
            "Run the same find expression with -print instead of -delete first.".to_owned(),
        ],
        destructive: true,
        requires_review: true,
    })
}

fn nearest_parent_with_file(path: &Path, file_name: &str) -> Option<PathBuf> {
    let mut current = path.parent()?.to_path_buf();
    loop {
        if current.join(file_name).is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn repo_footprint_for_items(
    repo_root: String,
    items: Vec<&StorageHygieneItem>,
) -> StorageRepoFootprint {
    let repo_name = items
        .iter()
        .find_map(|item| item.attribution.repo_name.clone())
        .or_else(|| {
            Path::new(&repo_root)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "repository".to_owned());
    let artifact_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    let mut top_artifact_folders: Vec<_> = items
        .iter()
        .map(|item| StorageRepoArtifactFolder {
            path: item.path.clone(),
            display_name: item.display_name.clone(),
            kind: item.kind.clone(),
            cleanup_tier: item.cleanup_tier.clone(),
            size_bytes: item.size_bytes,
        })
        .collect();
    top_artifact_folders.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    top_artifact_folders.truncate(5);

    let last_branch_touched = latest_branch_for_items(&items);
    let (estimated_rebuild_cost, estimated_rebuild_seconds) = estimate_rebuild_cost(&items);

    StorageRepoFootprint {
        id: repo_root.clone(),
        repo_root,
        repo_name,
        current_size_bytes: artifact_bytes,
        artifact_bytes,
        item_count: items.len(),
        top_artifact_folders,
        last_writer_process: None,
        last_writer_pid: None,
        last_branch_touched,
        growth_bytes: None,
        growth_window: "No prior scan baseline in this process.".to_owned(),
        estimated_rebuild_cost,
        estimated_rebuild_seconds,
        caveats: vec![
            "Current size is the bounded attributed artifact footprint, not a full source checkout size."
                .to_owned(),
            "Last writer process and exact growth require a file-event journal or scan baseline."
                .to_owned(),
        ],
    }
}

fn latest_branch_for_items(items: &[&StorageHygieneItem]) -> Option<String> {
    items
        .iter()
        .filter_map(|item| {
            let reference = item
                .attribution
                .git_branch
                .as_ref()
                .or(item.attribution.git_head.as_ref())?;
            let modified_millis = item.modified_millis.unwrap_or_default();
            Some((modified_millis, reference.clone()))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, reference)| reference)
}

fn estimate_rebuild_cost(items: &[&StorageHygieneItem]) -> (String, Option<u64>) {
    let expensive_bytes = tier_bytes(items, "expensive");
    let risky_bytes = tier_bytes(items, "risky");
    let rebuildable_bytes = tier_bytes(items, "rebuildable");
    let total_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));

    if risky_bytes > 0 {
        return ("Review first".to_owned(), None);
    }
    if expensive_bytes >= 1_024 * 1_024 * 1_024 {
        return ("High".to_owned(), Some(1_800));
    }
    if expensive_bytes > 0 || rebuildable_bytes >= 512 * 1_024 * 1_024 {
        return ("Medium".to_owned(), Some(600));
    }
    if total_bytes > 0 {
        return ("Low".to_owned(), Some(120));
    }
    ("None".to_owned(), Some(0))
}

fn tier_bytes(items: &[&StorageHygieneItem], tier: &str) -> u64 {
    items
        .iter()
        .filter(|item| item.cleanup_tier == tier)
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes))
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
        ".claude",
        ".codex",
        ".cursor",
        ".aider",
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

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * KIB;
    const GIB: f64 = 1024.0 * MIB;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KB", value / KIB)
    } else {
        format!("{bytes} B")
    }
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
            project.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/aetower-ffi\"]\n",
        ) {
            panic!("write cargo manifest: {error}");
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
        assert!(json.contains("\"repo_footprints\""));
        assert!(json.contains("\"current_size_bytes\":1048704"));
        assert!(json.contains("\"top_artifact_folders\""));
        assert!(json.contains("\"last_branch_touched\":\"master\""));
        assert!(json.contains("\"estimated_rebuild_cost\":\"Low\""));
        assert!(json.contains("\"last_writer_process\":null"));
        assert!(json.contains("\"cleanup_recipes\""));
        assert!(json.contains("\"title\":\"Clean Rust package aetower-ffi\""));
        assert!(json.contains("cargo clean -p aetower-ffi"));
        assert!(json.contains("\"cleanup_bundles\""));
        assert!(json.contains("\"id\":\"safe-reclaim\""));
        assert!(json.contains("\"dry_run_only\":true"));
        assert!(json.contains("\"confidence_score\":94"));
        assert!(json.contains("\"rollback_notes\""));
        assert!(json.contains("\"budget_guardrails\""));
        assert!(json.contains("\"repo_growth_budget_bytes_per_day\":2147483648"));
        assert!(json.contains("\"total_artifact_budget_bytes\":32212254720"));
        assert!(json.contains("\"status\":\"ok\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_hygiene_reports_agent_aware_artifact_cost() {
        let root = test_root("agent-aware-artifacts");
        let target = root.join(".codex").join("target").join("debug");
        if let Err(error) = fs::create_dir_all(&target) {
            panic!("create codex target dir: {error}");
        }
        if let Err(error) = fs::write(
            target.join("blob"),
            vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write codex build artifact: {error}");
        }

        let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

        assert!(json.contains("\"agent_hygiene\""));
        assert!(json.contains("\"agent_count\":1"));
        assert!(json.contains("\"provider\":\"codex\""));
        assert!(json.contains("\"display_name\":\"Codex\""));
        assert!(json.contains("\"session_id\":\"Codex local artifacts\""));
        assert!(json.contains("\"week_agent_artifact_bytes\":1048704"));
        assert!(json.contains("\"week_rebuildable_agent_bytes\":1048704"));
        assert!(json.contains("\"attribution_sources\":[\"known_agent_directory\"]"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_hygiene_generates_reclaim_actions_for_common_dev_artifacts() {
        let root = test_root("reclaim-actions");
        let project = root.join("project");
        let node_modules = project.join("node_modules").join("left-pad");
        let venv = project.join(".venv").join("lib");
        let frontend_cache = project.join(".turbo");
        let python_cache = project.join(".pytest_cache");
        let coverage = project.join("coverage");

        for directory in [
            &node_modules,
            &venv,
            &frontend_cache,
            &python_cache,
            &coverage,
        ] {
            if let Err(error) = fs::create_dir_all(directory) {
                panic!("create test artifact directory: {error}");
            }
            if let Err(error) = fs::write(
                directory.join("blob"),
                vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
            ) {
                panic!("write test artifact: {error}");
            }
        }

        let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

        assert!(json.contains("\"kind\":\"node-dependencies\""));
        assert!(json.contains("\"title\":\"Remove Node dependency tree\""));
        assert!(json.contains("\"category\":\"node\""));
        assert!(json.contains("\"kind\":\"python-environment\""));
        assert!(json.contains("\"title\":\"Remove Python virtual environment\""));
        assert!(json.contains("\"kind\":\"frontend-cache\""));
        assert!(json.contains("\"title\":\"Clear frontend cache\""));
        assert!(json.contains("\"kind\":\"python-cache\""));
        assert!(json.contains("\"title\":\"Clear Python cache\""));
        assert!(json.contains("\"kind\":\"coverage-output\""));
        assert!(json.contains("\"title\":\"Remove coverage output\""));
        assert!(json.contains("rm -rf"));

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
