use super::*;

pub fn repository_inventory_json(roots: Vec<String>, max_depth: usize) -> Result<String, String> {
    let started = Instant::now();
    let captured_at_millis = crate::current_unix_millis().unwrap_or_default();
    let requested_roots = normalize_roots(roots);
    let root_labels = requested_roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>();
    let repository_cache = StorageSizeIndex::open();
    let cached_repository_entries =
        repository_cache.load_repository_inventory_cache(&requested_roots);
    let scan = scan_repository_inventory_roots(&requested_roots, max_depth);
    let repository_walk_millis = started.elapsed().as_millis() as u64;
    let discovered_repository_count = scan.repositories_by_root.len().min(u64::MAX as usize) as u64;
    let scanned_directory_count = scan
        .coverage
        .iter()
        .map(|coverage| coverage.scanned_directory_count)
        .fold(0u64, u64::saturating_add);
    let skipped_directory_count = scan
        .coverage
        .iter()
        .map(|coverage| coverage.skipped_directory_count)
        .fold(0u64, u64::saturating_add);

    let mut metrics = StorageScanMetrics {
        storage_index_status: "repository_inventory_only".to_owned(),
        ..StorageScanMetrics::default()
    };
    repository_cache.store_repository_inventory_cache(
        &scan.repositories_by_root,
        captured_at_millis,
        &mut metrics,
    );
    let cached_repository_roots = repository_inventory_cache_roots(&cached_repository_entries);
    let (repository_roots, not_seen_repository_roots) = merge_repository_inventory_cache(
        cached_repository_roots,
        scan.repositories_by_root.clone(),
    );
    let repository_cache_states = repository_inventory_cache_states(
        &repository_roots,
        &scan.repositories_by_root,
        &cached_repository_entries,
    );
    let repository_inventory = summarize_repository_inventory(
        repository_roots,
        &not_seen_repository_roots,
        &repository_cache_states,
        started,
        StorageScanMode::FastChangedOnly,
        &mut metrics,
    );
    let mut repository_inventory_completeness =
        repository_inventory_completeness(&scan.coverage, scan.truncated);
    if !not_seen_repository_roots.is_empty() {
        repository_inventory_completeness.complete = false;
    }
    let report = RepositoryInventoryReport {
        captured_at_millis,
        scan_duration_millis: started.elapsed().as_millis() as u64,
        roots: root_labels,
        repository_inventory,
        repository_inventory_complete: repository_inventory_completeness.complete,
        repository_inventory_truncated: repository_inventory_completeness.truncated,
        repository_inventory_roots: repository_inventory_completeness.roots,
        repository_inventory_partial_roots: repository_inventory_completeness.partial_roots,
        repository_inventory_coverage: scan.coverage,
        truncated: scan.truncated,
        diagnostics: RepositoryInventoryDiagnostics {
            repository_walk_millis,
            git_millis: metrics.git_millis,
            discovered_repository_count,
            scanned_directory_count,
            skipped_directory_count,
        },
    };
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(super) fn summarize_repo_footprints(items: &[StorageHygieneItem]) -> Vec<StorageRepoFootprint> {
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

pub(super) fn summarize_repository_inventory(
    repositories_by_root: BTreeMap<String, String>,
    not_seen_roots: &BTreeSet<String>,
    cache_states: &BTreeMap<String, RepositoryInventoryCacheState>,
    started: Instant,
    mode: StorageScanMode,
    metrics: &mut StorageScanMetrics,
) -> Vec<StorageRepositoryInventoryItem> {
    let mut repositories: Vec<_> = repositories_by_root
        .into_iter()
        .map(|(repo_root, discovered_root)| {
            let repo_path = Path::new(&repo_root);
            let git_started = Instant::now();
            let git = repository_git_intelligence(repo_path, started, mode);
            metrics.git_millis = metrics
                .git_millis
                .saturating_add(git_started.elapsed().as_millis() as u64);
            let quality = repository_quality(repo_path);
            let tracked_started = Instant::now();
            let tracked_agent_paths =
                git_tracked_paths(repo_path, &agent_contract_candidate_paths());
            metrics.git_millis = metrics
                .git_millis
                .saturating_add(tracked_started.elapsed().as_millis() as u64);
            let mut guidance = agent_guidance_audit(repo_path, &quality, &tracked_agent_paths);
            let contract_coverage =
                agent_contract_coverage_audit(repo_path, &quality, &guidance, &tracked_agent_paths);
            guidance.issues.extend(contract_coverage.issues.clone());
            guidance.status = guidance_status(&guidance.issues);
            let repo_name = repo_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("repository")
                .to_owned();
            let not_seen_in_latest_scan = not_seen_roots.contains(&repo_root);
            let cache_state = cache_states.get(&repo_root).cloned().unwrap_or_else(|| {
                RepositoryInventoryCacheState {
                    status: "uncached".to_owned(),
                    fingerprint: repository_inventory_fingerprint(repo_path),
                    fingerprint_changed: false,
                    last_seen_millis: None,
                    last_scan_millis: None,
                }
            });

            StorageRepositoryInventoryItem {
                id: repo_root.clone(),
                repo_root,
                repo_name,
                inventory_cache_status: cache_state.status,
                inventory_fingerprint: cache_state.fingerprint,
                inventory_fingerprint_changed: cache_state.fingerprint_changed,
                inventory_last_seen_millis: cache_state.last_seen_millis,
                inventory_last_scan_millis: cache_state.last_scan_millis,
                git_branch: git.branch,
                git_head: git.head,
                git_ref: git.reference,
                git_detached_head: git.detached_head,
                git_remote_origin_url: git.remote_origin_url,
                git_remote_key: git.remote_key,
                git_remote_host: git.remote_host,
                git_remote_owner: git.remote_owner,
                git_remote_name: git.remote_name,
                git_dirty_status: git.dirty_status,
                git_dirty_file_count: git.dirty_file_count,
                git_dirty_truncated: git.dirty_truncated,
                not_seen_in_latest_scan,
                clone_group_count: 1,
                clone_group_roots: Vec::new(),
                discovered_root,
                has_agents_md: quality.has_agents_md,
                has_claude_md: quality.has_claude_md,
                claude_md_bytes: quality.claude_md_bytes,
                claude_md_delegation_max_bytes: CLAUDE_MD_DELEGATION_MAX_BYTES,
                claude_md_delegates_to_agents_md: quality.claude_md_delegates_to_agents_md,
                agent_readiness_score: contract_coverage.score,
                agent_readiness_status: contract_coverage.status,
                agent_contract_missing_count: contract_coverage.missing_count,
                agent_contract_coverage: contract_coverage.coverage,
                agent_guidance_status: guidance.status,
                agent_guidance_issue_count: guidance.issues.len() as u64,
                agent_guidance_issues: guidance.issues,
            }
        })
        .collect();
    attach_clone_groups(&mut repositories);
    repositories.sort_by(|left, right| {
        left.repo_name
            .to_lowercase()
            .cmp(&right.repo_name.to_lowercase())
            .then_with(|| left.repo_root.cmp(&right.repo_root))
    });
    repositories
}

#[derive(Clone, Debug, Default)]
struct RepositoryGitIntelligence {
    branch: Option<String>,
    head: Option<String>,
    reference: Option<String>,
    detached_head: bool,
    remote_origin_url: Option<String>,
    remote_key: Option<String>,
    remote_host: Option<String>,
    remote_owner: Option<String>,
    remote_name: Option<String>,
    dirty_status: String,
    dirty_file_count: Option<u64>,
    dirty_truncated: bool,
}

fn repository_git_intelligence(
    repo_path: &Path,
    started: Instant,
    mode: StorageScanMode,
) -> RepositoryGitIntelligence {
    let head = read_git_head(repo_path);
    let remote_origin_url = read_git_config_value(repo_path, "remote \"origin\"", "url")
        .and_then(|value| redact_git_remote_url(&value));
    let remote_key = remote_origin_url
        .as_deref()
        .and_then(normalize_git_remote_key);
    let remote_parts = remote_key.as_deref().map(split_git_remote_key);
    let dirty = if mode.collect_git_status() {
        read_git_dirty_status(repo_path, started)
    } else {
        GitDirtyStatus {
            status: "not_checked_lazy".to_owned(),
            file_count: None,
            truncated: false,
        }
    };

    RepositoryGitIntelligence {
        branch: head.branch,
        head: head.short_head,
        reference: head.reference,
        detached_head: head.detached,
        remote_origin_url,
        remote_key,
        remote_host: remote_parts.as_ref().and_then(|parts| parts.host.clone()),
        remote_owner: remote_parts.as_ref().and_then(|parts| parts.owner.clone()),
        remote_name: remote_parts.and_then(|parts| parts.name),
        dirty_status: dirty.status,
        dirty_file_count: dirty.file_count,
        dirty_truncated: dirty.truncated,
    }
}

fn attach_clone_groups(repositories: &mut [StorageRepositoryInventoryItem]) {
    let mut roots_by_remote = BTreeMap::<String, Vec<String>>::new();
    for repository in repositories.iter() {
        if let Some(remote_key) = repository.git_remote_key.as_deref() {
            roots_by_remote
                .entry(remote_key.to_owned())
                .or_default()
                .push(repository.repo_root.clone());
        }
    }

    for repository in repositories.iter_mut() {
        let Some(remote_key) = repository.git_remote_key.as_deref() else {
            continue;
        };
        let Some(group_roots) = roots_by_remote.get(remote_key) else {
            continue;
        };
        repository.clone_group_count = group_roots.len() as u64;
        if group_roots.len() > 1 {
            repository.clone_group_roots = group_roots.clone();
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RepositoryQuality {
    pub(super) has_agents_md: bool,
    pub(super) has_claude_md: bool,
    pub(super) claude_md_bytes: Option<u64>,
    pub(super) claude_md_delegates_to_agents_md: bool,
}

fn repository_quality(repo_root: &Path) -> RepositoryQuality {
    let has_agents_md = repo_root.join("AGENTS.md").is_file();
    let claude_md = repo_root.join("CLAUDE.md");
    let has_claude_md = claude_md.is_file();
    let claude_md_bytes = fs::metadata(&claude_md).ok().map(|metadata| metadata.len());
    let claude_md_delegates_to_agents_md = if has_claude_md
        && claude_md_bytes.is_some_and(|bytes| bytes <= CLAUDE_MD_DELEGATION_MAX_BYTES)
    {
        fs::read_to_string(&claude_md)
            .map(|content| {
                content
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim() == "@AGENTS.md")
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    } else {
        false
    };

    RepositoryQuality {
        has_agents_md,
        has_claude_md,
        claude_md_bytes,
        claude_md_delegates_to_agents_md,
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
    let safe_bytes = tier_bytes(&items, "safe");
    let rebuildable_bytes = tier_bytes(&items, "rebuildable");
    let expensive_bytes = tier_bytes(&items, "expensive");
    let risky_bytes = tier_bytes(&items, "risky");
    let rebuildable_percent = percent(rebuildable_bytes, artifact_bytes);
    let mut top_artifact_folders: Vec<_> = items
        .iter()
        .map(|item| StorageRepoArtifactFolder {
            path: item.path.clone(),
            display_name: item.display_name.clone(),
            kind: item.kind.clone(),
            cleanup_tier: item.cleanup_tier.clone(),
            size_bytes: item.size_bytes,
            cleanup_allowed: item.cleanup_allowed,
            cleanup_blockers: item.cleanup_blockers.clone(),
            default_cleanup_action: item.default_cleanup_action.clone(),
            size_truncated: item.size_truncated,
            cloud_placeholder: item.cloud_placeholder,
            has_hardlinks: item.has_hardlinks,
            hardlink_count: item.hardlink_count,
        })
        .collect();
    top_artifact_folders.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    top_artifact_folders.truncate(5);

    let artifact_mix = repo_artifact_mix(&items);
    let last_branch_touched = latest_branch_for_items(&items);
    let (estimated_rebuild_cost, estimated_rebuild_seconds) = estimate_rebuild_cost(&items);
    let optimization_summary = repo_optimization_summary(
        &repo_name,
        artifact_bytes,
        rebuildable_bytes,
        expensive_bytes,
        risky_bytes,
        estimated_rebuild_seconds,
    );

    StorageRepoFootprint {
        id: repo_root.clone(),
        repo_root,
        repo_name,
        current_size_bytes: artifact_bytes,
        artifact_bytes,
        safe_bytes,
        rebuildable_bytes,
        expensive_bytes,
        risky_bytes,
        rebuildable_percent,
        item_count: items.len(),
        top_artifact_folders,
        artifact_mix,
        duplicate_clone_count: 1,
        duplicate_clone_roots: Vec::new(),
        last_writer_process: None,
        last_writer_pid: None,
        last_branch_touched,
        growth_bytes: None,
        growth_window: "No prior scan baseline in this process.".to_owned(),
        estimated_rebuild_cost,
        estimated_rebuild_seconds,
        optimization_summary,
        caveats: vec![
            "Current size is the bounded attributed artifact footprint, not a full source checkout size."
                .to_owned(),
            "Last writer process and exact growth require a file-event journal or scan baseline."
                .to_owned(),
        ],
    }
}

pub(super) fn apply_growth_deltas_to_repo_footprints(
    footprints: &mut [StorageRepoFootprint],
    growth_deltas: &[StorageGrowthDelta],
) {
    for footprint in footprints {
        let matching = growth_deltas
            .iter()
            .filter(|delta| {
                delta.delta_bytes > 0
                    && delta
                        .repo_root
                        .as_deref()
                        .is_some_and(|repo_root| repo_root == footprint.repo_root)
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        let growth_bytes = matching
            .iter()
            .fold(0i64, |total, delta| total.saturating_add(delta.delta_bytes));
        let strongest = matching
            .iter()
            .max_by(|left, right| {
                left.attribution_confidence_score
                    .cmp(&right.attribution_confidence_score)
                    .then_with(|| left.delta_bytes.cmp(&right.delta_bytes))
            })
            .copied();
        footprint.growth_bytes = Some(growth_bytes);
        footprint.growth_window = "since indexed storage baseline".to_owned();
        if let Some(delta) = strongest {
            footprint.last_branch_touched = delta
                .git_branch
                .clone()
                .or(delta.git_head.clone())
                .or_else(|| footprint.last_branch_touched.clone());
            footprint.last_writer_process = delta
                .command
                .clone()
                .or(delta.process_tree.clone())
                .or(delta.ai_agent_session.clone());
            if footprint.last_writer_process.is_some() {
                footprint.caveats.push(format!(
                    "Growth attribution confidence: {} ({}%).",
                    delta.attribution_confidence, delta.attribution_confidence_score
                ));
            } else {
                footprint.caveats.push(
                    "Growth is repo-attributed, but command/session writer evidence is missing."
                        .to_owned(),
                );
            }
        }
    }
}

pub(super) fn apply_clone_groups_to_repo_footprints(
    footprints: &mut [StorageRepoFootprint],
    repository_inventory: &[StorageRepositoryInventoryItem],
) {
    let clone_groups_by_root = repository_inventory
        .iter()
        .map(|repository| {
            (
                repository.repo_root.as_str(),
                (
                    repository.clone_group_count,
                    repository.clone_group_roots.clone(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for footprint in footprints {
        if let Some((count, roots)) = clone_groups_by_root.get(footprint.repo_root.as_str()) {
            footprint.duplicate_clone_count = *count;
            footprint.duplicate_clone_roots = roots.clone();
        }
    }
}

fn repo_artifact_mix(items: &[&StorageHygieneItem]) -> Vec<StorageRepoArtifactMix> {
    let mut grouped = BTreeMap::<String, Vec<&StorageHygieneItem>>::new();
    for item in items {
        grouped.entry(item.kind.clone()).or_default().push(*item);
    }
    let mut mix = grouped
        .into_iter()
        .map(|(kind, group)| {
            let bytes = group
                .iter()
                .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
            let cleanup_tier = highest_cleanup_tier(group.iter().copied());
            let path = group
                .first()
                .map(|item| item.path.as_str())
                .unwrap_or_default();
            let intelligence = artifact_intelligence(&kind, path);
            StorageRepoArtifactMix {
                kind: kind.clone(),
                label: artifact_kind_label(&kind).to_owned(),
                item_count: group.len(),
                bytes,
                cleanup_tier,
                rebuild_command: intelligence.rebuild_command,
                estimated_rebuild_cost: intelligence.estimated_rebuild_cost,
                estimated_rebuild_seconds: intelligence.estimated_rebuild_seconds,
            }
        })
        .collect::<Vec<_>>();
    mix.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.label.cmp(&right.label))
    });
    mix.truncate(8);
    mix
}

fn artifact_kind_label(kind: &str) -> &'static str {
    match kind {
        "xcode-derived-data" => "Xcode DerivedData",
        "swift-build" => "SwiftPM .build",
        "rust-build" => "Cargo target",
        "node-dependencies" => "Node dependencies",
        "npm-cache" => "npm cache",
        "pnpm-store" => "pnpm store",
        "yarn-cache" => "Yarn cache",
        "docker-storage" => "Docker storage",
        "xcode-simulator-runtime" => "Xcode simulators",
        "xcode-archives" => "Xcode archives",
        "release-artifact" => "Release artifacts",
        "log-file" | "logs" => "Logs",
        "test-output" => "Test outputs",
        "coverage-output" => "Coverage outputs",
        "next-cache" => "Next.js cache",
        "next-build" => "Next.js build",
        "frontend-cache" => "Frontend cache",
        "python-cache" => "Python cache",
        "python-environment" => "Python environment",
        "tool-cache" => "Tool cache",
        _ => "Storage artifact",
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
    let safe_bytes = tier_bytes(items, "safe");
    let total_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));

    if risky_bytes > 0 {
        return ("Review first".to_owned(), None);
    }
    if total_bytes == 0 {
        return ("None".to_owned(), Some(0));
    }
    let gib = 1_024_f64 * 1_024_f64 * 1_024_f64;
    let estimated = ((rebuildable_bytes as f64 / gib) * 25.0)
        + ((expensive_bytes as f64 / gib) * 120.0)
        + ((safe_bytes as f64 / gib) * 5.0);
    let seconds = estimated.round().clamp(60.0, 3_600.0) as u64;
    if let Some(measured_seconds) = measured_repo_rebuild_seconds(items) {
        let seconds = seconds.max(measured_seconds);
        return (
            format!(
                "Measured {}",
                rebuild_cost_band(expensive_bytes, seconds).to_ascii_lowercase()
            ),
            Some(seconds),
        );
    }
    let label = rebuild_cost_band(expensive_bytes, seconds);
    (label.to_owned(), Some(seconds))
}

fn measured_repo_rebuild_seconds(items: &[&StorageHygieneItem]) -> Option<u64> {
    items
        .iter()
        .filter(|item| item.estimated_rebuild_cost.starts_with("Measured "))
        .filter_map(|item| item.estimated_rebuild_seconds)
        .max()
}

fn rebuild_cost_band(expensive_bytes: u64, seconds: u64) -> &'static str {
    if expensive_bytes >= 2 * 1_024 * 1_024 * 1_024 || seconds >= 1_200 {
        "High"
    } else if expensive_bytes > 0 || seconds >= 300 {
        "Medium"
    } else {
        "Low"
    }
}

fn repo_optimization_summary(
    repo_name: &str,
    artifact_bytes: u64,
    rebuildable_bytes: u64,
    expensive_bytes: u64,
    risky_bytes: u64,
    estimated_rebuild_seconds: Option<u64>,
) -> String {
    if artifact_bytes == 0 {
        return format!(
            "{repo_name} has no attributed reclaimable development artifacts in this scan."
        );
    }
    let dominant =
        if risky_bytes > 0 && risky_bytes >= rebuildable_bytes && risky_bytes >= expensive_bytes {
            "needs manual review"
        } else if rebuildable_bytes >= expensive_bytes {
            "mostly rebuildable"
        } else {
            "mostly expensive to restore"
        };
    let time = estimated_rebuild_seconds
        .map(|seconds| {
            format!(
                ", estimated rebuild cost {}",
                rebuild_seconds_label(seconds)
            )
        })
        .unwrap_or_else(|| ", rebuild cost requires review".to_owned());
    format!(
        "{repo_name} has {} reclaimable attributed artifacts, {dominant}{time}.",
        human_bytes(artifact_bytes)
    )
}

fn tier_bytes(items: &[&StorageHygieneItem], tier: &str) -> u64 {
    items
        .iter()
        .filter(|item| item.cleanup_tier == tier)
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes))
}

pub(super) fn find_git_root(path: &Path) -> Option<PathBuf> {
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

#[derive(Clone, Debug, Default)]
pub(super) struct GitHead {
    pub(super) branch: Option<String>,
    pub(super) short_head: Option<String>,
    reference: Option<String>,
    detached: bool,
}

#[derive(Clone, Debug)]
struct GitRemoteParts {
    host: Option<String>,
    owner: Option<String>,
    name: Option<String>,
}

#[derive(Clone, Debug)]
struct GitDirtyStatus {
    status: String,
    file_count: Option<u64>,
    truncated: bool,
}

pub(super) fn read_git_head(repo_root: &Path) -> GitHead {
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return GitHead::default();
    };
    let Ok(head) = fs::read_to_string(git_dir.join("HEAD")) else {
        return GitHead::default();
    };
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: ") {
        let branch = reference
            .strip_prefix("refs/heads/")
            .unwrap_or(reference)
            .to_owned();
        let head_sha = fs::read_to_string(git_dir.join(reference))
            .ok()
            .map(|value| short_hash(value.trim()))
            .or_else(|| read_packed_ref(&git_dir, reference).map(|value| short_hash(&value)));
        return GitHead {
            branch: Some(branch),
            short_head: head_sha,
            reference: Some(reference.to_owned()),
            detached: false,
        };
    }

    if head.is_empty() {
        GitHead::default()
    } else {
        GitHead {
            branch: None,
            short_head: Some(short_hash(head)),
            reference: Some(head.to_owned()),
            detached: true,
        }
    }
}

fn read_git_config_value(repo_root: &Path, target_section: &str, key: &str) -> Option<String> {
    let git_dir = resolve_git_dir(repo_root)?;
    let config = fs::read_to_string(git_dir.join("config")).ok()?;
    let mut active_section: Option<String> = None;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            active_section = Some(trimmed[1..trimmed.len().saturating_sub(1)].to_owned());
            continue;
        }
        if active_section.as_deref() != Some(target_section) {
            continue;
        }
        let Some((candidate_key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if candidate_key.trim() == key {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn normalize_git_remote_key(remote_url: &str) -> Option<String> {
    let mut value = remote_url.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(stripped) = value.strip_prefix("https://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("http://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("ssh://") {
        value = stripped;
    } else if let Some(stripped) = value.strip_prefix("git@") {
        value = stripped;
    }
    value = strip_remote_userinfo(value);
    if let Some((host, path)) = value.split_once(':')
        && !host.contains('/')
        && !path.is_empty()
    {
        return Some(clean_remote_key(&format!("{host}/{path}")));
    }
    Some(clean_remote_key(value))
}

fn redact_git_remote_url(remote_url: &str) -> Option<String> {
    let value = remote_url
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if value.is_empty() {
        return None;
    }
    if let Some((scheme, rest)) = value.split_once("://") {
        return Some(format!("{scheme}://{}", strip_remote_userinfo(rest)));
    }
    if let Some(stripped) = value.strip_prefix("git@") {
        return Some(stripped.to_owned());
    }
    if let Some((user_host, path)) = value.split_once(':')
        && user_host.contains('@')
        && !user_host.contains('/')
        && !path.is_empty()
    {
        let host = user_host.rsplit('@').next().unwrap_or_default();
        return Some(format!("{host}:{path}"));
    }
    Some(strip_remote_userinfo(value).to_owned())
}

fn strip_remote_userinfo(value: &str) -> &str {
    let first_slash = value.find('/');
    if let Some(at) = value.find('@')
        && first_slash.is_none_or(|slash| at < slash)
    {
        return &value[at + 1..];
    }
    value
}

fn clean_remote_key(value: &str) -> String {
    let mut cleaned = value
        .split(['?', '#'])
        .next()
        .unwrap_or(value)
        .trim_matches('/')
        .trim()
        .to_lowercase();
    if let Some(stripped) = cleaned.strip_suffix(".git") {
        cleaned = stripped.to_owned();
    }
    cleaned
}

fn split_git_remote_key(remote_key: &str) -> GitRemoteParts {
    let parts: Vec<_> = remote_key
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let name = parts.last().map(|part| (*part).to_owned());
    let owner = if parts.len() >= 3 {
        parts
            .get(parts.len().saturating_sub(2))
            .map(|part| (*part).to_owned())
    } else {
        None
    };
    let host = parts.first().map(|part| (*part).to_owned());
    GitRemoteParts { host, owner, name }
}

fn read_git_dirty_status(repo_root: &Path, scan_started: Instant) -> GitDirtyStatus {
    if scan_started.elapsed() >= SCAN_TIME_BUDGET {
        return GitDirtyStatus {
            status: "timeout".to_owned(),
            file_count: None,
            truncated: false,
        };
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        // Disable core.quotePath so non-ASCII paths come through as raw UTF-8
        // rather than octal-escaped; the parser still C-unquotes the remainder.
        .arg("-c")
        .arg("core.quotePath=false")
        .arg("status")
        .arg("--porcelain=v1")
        .arg("--untracked-files=normal")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return GitDirtyStatus {
            status: "unavailable".to_owned(),
            file_count: None,
            truncated: false,
        };
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return GitDirtyStatus {
                        status: "unavailable".to_owned(),
                        file_count: None,
                        truncated: false,
                    };
                }
                let stdout = output
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stdout))
                    .unwrap_or_default();
                let count = stdout.lines().take(GIT_STATUS_MAX_LINES + 1).count();
                let truncated = count > GIT_STATUS_MAX_LINES;
                return GitDirtyStatus {
                    status: if count == 0 { "clean" } else { "dirty" }.to_owned(),
                    file_count: Some(count.min(GIT_STATUS_MAX_LINES) as u64),
                    truncated,
                };
            }
            Ok(None)
                if started.elapsed() < GIT_STATUS_TIME_BUDGET
                    && scan_started.elapsed() < SCAN_TIME_BUDGET =>
            {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return GitDirtyStatus {
                    status: "timeout".to_owned(),
                    file_count: None,
                    truncated: false,
                };
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return GitDirtyStatus {
                    status: "unavailable".to_owned(),
                    file_count: None,
                    truncated: false,
                };
            }
        }
    }
}

fn read_packed_ref(git_dir: &Path, reference: &str) -> Option<String> {
    let content = fs::read_to_string(git_dir.join("packed-refs")).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('^') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let hash = parts.next()?;
        let candidate = parts.next()?;
        if candidate == reference {
            return Some(hash.to_owned());
        }
    }
    None
}

fn git_tracked_paths(repo_root: &Path, relative_paths: &[&str]) -> BTreeSet<String> {
    if relative_paths.is_empty() {
        return BTreeSet::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("ls-files")
        .arg("--")
        .args(relative_paths)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return BTreeSet::new();
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return BTreeSet::new();
                }
                return output
                    .as_ref()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .filter(|path| !path.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeSet::new();
            }
        }
    }
}

pub(super) fn git_tracked_path_set(
    repo_root: &Path,
    relative_paths: &[String],
) -> BTreeSet<String> {
    let borrowed: Vec<&str> = relative_paths.iter().map(String::as_str).collect();
    git_tracked_paths(repo_root, &borrowed)
}

pub(super) fn git_status_path_map(
    repo_root: &Path,
    relative_paths: &[String],
) -> BTreeMap<String, String> {
    if relative_paths.is_empty() {
        return BTreeMap::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        // Disable core.quotePath so non-ASCII paths come through as raw UTF-8
        // rather than octal-escaped; the parser still C-unquotes the remainder.
        .arg("-c")
        .arg("core.quotePath=false")
        .arg("status")
        .arg("--porcelain=v1")
        .arg("--untracked-files=normal")
        .arg("--")
        .args(relative_paths)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return BTreeMap::new();
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok();
                if !status.success() {
                    return BTreeMap::new();
                }
                return output
                    .as_ref()
                    .map(|output| {
                        parse_git_status_porcelain(&String::from_utf8_lossy(&output.stdout))
                    })
                    .unwrap_or_default();
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeMap::new();
            }
        }
    }
}

pub(super) fn git_repository_has_active_changes(repo_root: &Path) -> bool {
    let status = read_git_dirty_status(repo_root, Instant::now());
    status.status == "dirty"
}

/// Decode git's C-style path quoting (used for paths with special characters
/// even when core.quotePath is off): a leading/trailing `"` wraps a body with
/// `\n \t \r \" \\` and `\ooo` octal escapes over UTF-8 bytes. Unquoted paths
/// pass through unchanged.
fn git_unquote_path(raw: &str) -> String {
    if !(raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2) {
        return raw.to_owned();
    }
    let body = &raw[1..raw.len() - 1];
    let mut bytes: Vec<u8> = Vec::with_capacity(body.len());
    let mut iter = body.bytes().peekable();
    while let Some(byte) = iter.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        match iter.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'r') => bytes.push(b'\r'),
            Some(b'"') => bytes.push(b'"'),
            Some(b'\\') => bytes.push(b'\\'),
            Some(octal @ b'0'..=b'7') => {
                // Up to three octal digits -> one byte.
                let mut value = (octal - b'0') as u32;
                for _ in 0..2 {
                    match iter.peek() {
                        Some(&digit @ b'0'..=b'7') => {
                            value = value * 8 + (digit - b'0') as u32;
                            iter.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(value as u8);
            }
            Some(other) => {
                bytes.push(b'\\');
                bytes.push(other);
            }
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn parse_git_status_porcelain(output: &str) -> BTreeMap<String, String> {
    let mut statuses = BTreeMap::new();
    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let raw_path = line[3..].split(" -> ").last().unwrap_or_default().trim();
        let path = git_unquote_path(raw_path);
        let path = path.as_str();
        if path.is_empty() {
            continue;
        }
        let status = match code {
            "??" => "untracked",
            "!!" => "ignored",
            _ if code.contains('U') => "conflicted",
            _ if code.contains('D') => "deleted",
            _ if code.contains('R') => "renamed",
            _ if code.contains('M') || code.contains('A') || code.contains('C') => "modified",
            _ => "modified",
        };
        statuses.insert(path.to_owned(), status.to_owned());
    }
    statuses
}

pub(super) fn git_ignored_path_set(
    repo_root: &Path,
    relative_paths: &[String],
) -> BTreeSet<String> {
    if relative_paths.is_empty() {
        return BTreeSet::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("check-ignore")
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return local_gitignore_path_set(repo_root, relative_paths);
    };

    if let Some(mut stdin) = child.stdin.take() {
        for path in relative_paths {
            let _ = writeln!(stdin, "{path}");
        }
    }

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let output = child.wait_with_output().ok();
                let mut ignored: BTreeSet<String> = output
                    .as_ref()
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .filter(|path| !path.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                ignored.extend(local_gitignore_path_set(repo_root, relative_paths));
                return ignored;
            }
            Ok(None) if started.elapsed() < GIT_STATUS_TIME_BUDGET => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return BTreeSet::new();
            }
        }
    }
}

fn local_gitignore_path_set(repo_root: &Path, relative_paths: &[String]) -> BTreeSet<String> {
    let mut ignore_contents = Vec::new();
    if let Ok(content) = fs::read_to_string(repo_root.join(".gitignore")) {
        ignore_contents.push(content);
    }
    if let Some(git_dir) = resolve_git_dir(repo_root)
        && let Ok(content) = fs::read_to_string(git_dir.join("info").join("exclude"))
    {
        ignore_contents.push(content);
    }
    if ignore_contents.is_empty() {
        return BTreeSet::new();
    }
    let patterns: Vec<_> = ignore_contents
        .iter()
        .flat_map(|content| content.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .map(|line| line.trim_matches('/').to_owned())
        .collect();
    let mut ignored = BTreeSet::new();
    for path in relative_paths {
        let normalized = path.trim_matches('/');
        let file_name = Path::new(normalized)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(normalized);
        if patterns.iter().any(|pattern| {
            pattern == normalized
                || pattern == file_name
                || normalized.starts_with(&format!("{pattern}/"))
        }) {
            ignored.insert(path.clone());
        }
    }
    ignored
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

pub(super) fn is_git_repository_root(path: &Path) -> bool {
    resolve_git_dir(path).is_some()
}

#[derive(Clone, Debug, Default)]
pub(super) struct RepositoryInventoryScan {
    pub(super) repositories_by_root: BTreeMap<String, String>,
    pub(super) coverage: Vec<RepositoryInventoryRootCoverage>,
    pub(super) truncated: bool,
}

fn scan_repository_inventory_roots(
    requested_roots: &[PathBuf],
    max_depth: usize,
) -> RepositoryInventoryScan {
    scan_repository_inventory_roots_with_budget(
        requested_roots,
        max_depth,
        REPOSITORY_INVENTORY_TIME_BUDGET,
        None,
    )
}

pub(super) fn scan_repository_inventory_roots_with_budget(
    requested_roots: &[PathBuf],
    max_depth: usize,
    budget: Duration,
    runtime: Option<&StorageScanRuntimeContext>,
) -> RepositoryInventoryScan {
    let started = Instant::now();
    let mut repositories_by_root = BTreeMap::<String, String>::new();
    let mut coverage = Vec::new();
    let mut truncated = false;

    for root in requested_roots {
        let path = root.display().to_string();
        let label = storage_source_label(root, &storage_source_kind(root));
        if let Some(runtime) = runtime
            && !runtime.set_phase(STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY, Some(root))
        {
            truncated = true;
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "partial",
                "partial",
                "Repository inventory stopped before this root because the scan was cancelled.",
                0,
                0,
                0,
                true,
                false,
            ));
            continue;
        }

        if started.elapsed() >= budget {
            truncated = true;
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "partial",
                "partial",
                "Repository inventory stopped before this root because the inventory budget ended.",
                0,
                0,
                0,
                true,
                false,
            ));
            continue;
        }

        if !root.exists() {
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "unavailable",
                "unavailable",
                "Path does not exist on this machine or provider is not configured.",
                0,
                0,
                0,
                false,
                false,
            ));
            continue;
        }

        let metadata = match fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(error) => {
                let detail = error.to_string();
                let permission_state = skipped_root_permission_state(&detail);
                coverage.push(repository_inventory_coverage_entry(
                    root,
                    label,
                    "skipped",
                    &permission_state,
                    &detail,
                    0,
                    0,
                    0,
                    false,
                    false,
                ));
                continue;
            }
        };

        if metadata.file_type().is_symlink() {
            coverage.push(repository_inventory_coverage_entry(
                root,
                label,
                "skipped",
                "skipped",
                "Symlink root skipped during repository inventory.",
                0,
                0,
                0,
                false,
                false,
            ));
            continue;
        }

        if !is_git_repository_root(root)
            && let Some(reason) = repository_inventory_skip_reason(root)
        {
            coverage.push(repository_inventory_coverage_entry(
                root, label, "skipped", "skipped", reason, 0, 0, 1, false, false,
            ));
            continue;
        }

        let before = repositories_by_root.len();
        let root_scan = scan_repository_inventory_root(root, max_depth, started, budget, runtime);
        truncated |= root_scan.truncated;
        for repo_root in root_scan.repositories {
            repositories_by_root
                .entry(repo_root.display().to_string())
                .or_insert_with(|| path.clone());
        }
        let repository_count = repositories_by_root.len().saturating_sub(before) as u64;
        coverage.push(repository_inventory_coverage_entry(
            root,
            label,
            if root_scan.truncated {
                "partial"
            } else {
                "scanned"
            },
            if root_scan.truncated {
                "partial"
            } else {
                "readable"
            },
            if root_scan.truncated {
                "Repository inventory was partially scanned before the inventory budget ended."
            } else {
                "Readable and scanned by the repository inventory pass."
            },
            repository_count,
            root_scan.scanned_directory_count,
            root_scan.skipped_directory_count,
            root_scan.truncated,
            !root_scan.truncated,
        ));
    }

    RepositoryInventoryScan {
        repositories_by_root,
        coverage,
        truncated,
    }
}

#[derive(Clone, Debug, Default)]
struct RepositoryInventoryRootScan {
    repositories: BTreeSet<PathBuf>,
    scanned_directory_count: u64,
    skipped_directory_count: u64,
    truncated: bool,
}

fn scan_repository_inventory_root(
    root: &Path,
    max_depth: usize,
    started: Instant,
    budget: Duration,
    runtime: Option<&StorageScanRuntimeContext>,
) -> RepositoryInventoryRootScan {
    let max_depth = max_depth.clamp(1, 12);
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut repositories = BTreeSet::new();
    let mut scanned_directory_count = 0u64;
    let mut skipped_directory_count = 0u64;
    let mut truncated = false;

    while let Some((path, depth)) = queue.pop_front() {
        if started.elapsed() >= budget
            || scanned_directory_count >= REPOSITORY_INVENTORY_MAX_DIRECTORIES
        {
            truncated = true;
            break;
        }

        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }

        let is_repo = is_git_repository_root(&path);
        if is_repo {
            repositories.insert(path.clone());
        } else if depth > 0 && repository_inventory_skip_reason(&path).is_some() {
            skipped_directory_count = skipped_directory_count.saturating_add(1);
            continue;
        }

        if depth >= max_depth {
            continue;
        }

        scanned_directory_count = scanned_directory_count.saturating_add(1);
        if let Some(runtime) = runtime
            && !runtime.checkpoint(
                STORAGE_SCAN_PHASE_REPOSITORY_INVENTORY,
                Some(&path),
                0,
                1,
                0,
            )
        {
            truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        let mut children = entries
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            if child
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == ".git")
            {
                skipped_directory_count = skipped_directory_count.saturating_add(1);
                continue;
            }
            queue.push_back((child, depth + 1));
        }
    }

    RepositoryInventoryRootScan {
        repositories,
        scanned_directory_count,
        skipped_directory_count,
        truncated,
    }
}

fn repository_inventory_skip_reason(path: &Path) -> Option<&'static str> {
    match path.file_name().and_then(|name| name.to_str())? {
        ".git" => Some("Git internals skipped during repository inventory."),
        "node_modules" => Some("Dependency tree skipped during repository inventory."),
        "target" => Some("Rust build output skipped during repository inventory."),
        ".build" => Some("Build output skipped during repository inventory."),
        "DerivedData" => Some("Xcode DerivedData skipped during repository inventory."),
        ".cache" => Some("Cache directory skipped during repository inventory."),
        "Library" => Some("macOS Library tree skipped during repository inventory."),
        ".docker" => Some("Docker data skipped during repository inventory."),
        ".npm" => Some("Package cache skipped during repository inventory."),
        ".pnpm-store" => Some("Package cache skipped during repository inventory."),
        ".cargo" => Some("Package cache skipped during repository inventory."),
        ".gradle" => Some("Gradle cache skipped during repository inventory."),
        ".venv" | "venv" => Some("Virtual environment skipped during repository inventory."),
        ".tox" => Some("Python test environment skipped during repository inventory."),
        "__pycache__" => Some("Python bytecode cache skipped during repository inventory."),
        ".next" | ".turbo" => Some("Frontend build cache skipped during repository inventory."),
        "Pods" => Some("Dependency tree skipped during repository inventory."),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn repository_inventory_coverage_entry(
    root: &Path,
    label: String,
    status: &str,
    permission_state: &str,
    detail: &str,
    repository_count: u64,
    scanned_directory_count: u64,
    skipped_directory_count: u64,
    truncated: bool,
    scanned: bool,
) -> RepositoryInventoryRootCoverage {
    let path = root.display().to_string();
    RepositoryInventoryRootCoverage {
        id: path.clone(),
        label,
        path,
        status: status.to_owned(),
        permission_state: permission_state.to_owned(),
        detail: detail.to_owned(),
        repository_count,
        scanned_directory_count,
        skipped_directory_count,
        truncated,
        scanned,
    }
}

pub(super) fn repository_inventory_completeness(
    coverage: &[RepositoryInventoryRootCoverage],
    truncated: bool,
) -> RepositoryInventoryCompleteness {
    let roots = coverage
        .iter()
        .map(|root| root.path.clone())
        .collect::<Vec<_>>();
    let partial_roots = coverage
        .iter()
        .filter(|root| root.truncated || !root.scanned || root.status == "partial")
        .map(|root| root.path.clone())
        .collect::<Vec<_>>();
    let truncated = truncated || coverage.iter().any(|root| root.truncated);
    RepositoryInventoryCompleteness {
        complete: !truncated && partial_roots.is_empty(),
        truncated,
        roots,
        partial_roots,
    }
}

pub(super) fn cached_repository_inventory_coverage(
    requested_roots: &[PathBuf],
    repositories_by_root: &BTreeMap<String, String>,
) -> Vec<RepositoryInventoryRootCoverage> {
    requested_roots
        .iter()
        .map(|root| {
            let label = storage_source_label(root, &storage_source_kind(root));
            if !root.exists() {
                return repository_inventory_coverage_entry(
                    root,
                    label,
                    "unavailable",
                    "unavailable",
                    "Path does not exist on this machine or provider is not configured.",
                    0,
                    0,
                    0,
                    false,
                    false,
                );
            }

            let metadata = match fs::symlink_metadata(root) {
                Ok(metadata) => metadata,
                Err(error) => {
                    let detail = error.to_string();
                    let permission_state = skipped_root_permission_state(&detail);
                    return repository_inventory_coverage_entry(
                        root,
                        label,
                        "skipped",
                        &permission_state,
                        &detail,
                        0,
                        0,
                        0,
                        false,
                        false,
                    );
                }
            };
            if metadata.file_type().is_symlink() {
                return repository_inventory_coverage_entry(
                    root,
                    label,
                    "skipped",
                    "skipped",
                    "Symlink root skipped during cached repository inventory coverage.",
                    0,
                    0,
                    0,
                    false,
                    false,
                );
            }

            let root_display = root.display().to_string();
            let repository_count = repositories_by_root
                .values()
                .filter(|discovered_root| *discovered_root == &root_display)
                .count() as u64;
            let (status, detail) = if repository_count == 0 {
                (
                    "cached_empty",
                    "No repositories are currently present in the storage index for this root; run a refresh for authoritative inventory.",
                )
            } else {
                (
                    "cached_partial",
                    "Repository inventory was restored from the storage index snapshot; run a refresh for authoritative inventory.",
                )
            };
            repository_inventory_coverage_entry(
                root,
                label,
                status,
                "cached",
                detail,
                repository_count,
                0,
                0,
                false,
                false,
            )
        })
        .collect()
}

pub(super) fn repository_inventory_cache_roots(
    entries: &BTreeMap<String, RepositoryInventoryCacheEntry>,
) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(repo_root, entry)| (repo_root.clone(), entry.discovered_root.clone()))
        .collect()
}

pub(super) fn repository_inventory_cache_states(
    repositories_by_root: &BTreeMap<String, String>,
    latest_repositories_by_root: &BTreeMap<String, String>,
    cached_entries: &BTreeMap<String, RepositoryInventoryCacheEntry>,
) -> BTreeMap<String, RepositoryInventoryCacheState> {
    repositories_by_root
        .keys()
        .map(|repo_root| {
            let current_fingerprint = repository_inventory_fingerprint(Path::new(repo_root));
            let state = if latest_repositories_by_root.contains_key(repo_root) {
                RepositoryInventoryCacheState {
                    status: "scanned".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: false,
                    last_seen_millis: cached_entries
                        .get(repo_root)
                        .map(|entry| entry.last_seen_millis),
                    last_scan_millis: cached_entries
                        .get(repo_root)
                        .map(|entry| entry.last_scan_millis),
                }
            } else if !Path::new(repo_root).exists() {
                let cached = cached_entries.get(repo_root);
                RepositoryInventoryCacheState {
                    status: "missing".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: true,
                    last_seen_millis: cached.map(|entry| entry.last_seen_millis),
                    last_scan_millis: cached.map(|entry| entry.last_scan_millis),
                }
            } else if let Some(cached) = cached_entries.get(repo_root) {
                let cached_fingerprint = cached.repository_fingerprint.trim();
                let (status, fingerprint_changed) = if cached_fingerprint.is_empty() {
                    ("legacy", false)
                } else if cached_fingerprint == current_fingerprint {
                    ("current", false)
                } else {
                    ("changed", true)
                };
                RepositoryInventoryCacheState {
                    status: status.to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed,
                    last_seen_millis: Some(cached.last_seen_millis),
                    last_scan_millis: Some(cached.last_scan_millis),
                }
            } else {
                RepositoryInventoryCacheState {
                    status: "uncached".to_owned(),
                    fingerprint: current_fingerprint,
                    fingerprint_changed: false,
                    last_seen_millis: None,
                    last_scan_millis: None,
                }
            };
            (repo_root.clone(), state)
        })
        .collect()
}

pub(super) fn merge_repository_inventory_cache(
    cached: BTreeMap<String, String>,
    latest: BTreeMap<String, String>,
) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let mut merged = cached;
    let mut not_seen = merged.keys().cloned().collect::<BTreeSet<_>>();
    for (repo_root, discovered_root) in latest {
        not_seen.remove(&repo_root);
        merged.insert(repo_root, discovered_root);
    }
    (merged, not_seen)
}

pub(super) fn repository_inventory_fingerprint(repo_root: &Path) -> String {
    let root = repo_root.display().to_string();
    let root_fingerprint = repository_path_fingerprint(repo_root, "root");
    let git_marker = repo_root.join(".git");
    let marker_fingerprint = repository_path_fingerprint(&git_marker, "git-marker");
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return format!("v1|path={root}|{root_fingerprint}|{marker_fingerprint}|git-dir:missing");
    };

    let head = read_git_head(repo_root);
    let head_ref_fingerprint = head
        .reference
        .as_deref()
        .filter(|reference| reference.starts_with("refs/"))
        .map(|reference| repository_path_fingerprint(&git_dir.join(reference), "git-ref"))
        .unwrap_or_else(|| "git-ref:detached-or-missing".to_owned());

    [
        "v1".to_owned(),
        format!("path={root}"),
        root_fingerprint,
        marker_fingerprint,
        repository_path_fingerprint(&git_dir, "git-dir"),
        repository_path_fingerprint(&git_dir.join("config"), "config"),
        repository_path_fingerprint(&git_dir.join("index"), "index"),
        repository_path_fingerprint(&git_dir.join("HEAD"), "HEAD"),
        head_ref_fingerprint,
        repository_path_fingerprint(&git_dir.join("packed-refs"), "packed-refs"),
    ]
    .join("|")
}

fn repository_path_fingerprint(path: &Path, label: &str) -> String {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return format!("{label}:missing");
    };
    let modified_millis = unix_metadata_millis(metadata.mtime(), metadata.mtime_nsec());
    let changed_millis = unix_metadata_millis(metadata.ctime(), metadata.ctime_nsec());
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "dir"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    };
    let content_fingerprint = if metadata.is_file() && metadata.len() <= 1024 * 1024 {
        repository_small_file_hash(path)
            .map(|hash| format!("content:{hash:016x}"))
            .unwrap_or_else(|| "content:unreadable".to_owned())
    } else if metadata.is_file() {
        "content:skipped-large".to_owned()
    } else {
        "content:not-file".to_owned()
    };
    format!(
        "{}:{}:{}:{}:{}:{}:{}:{}",
        label,
        kind,
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        modified_millis,
        changed_millis,
        content_fingerprint
    )
}

pub(super) fn repository_git_file_fingerprint(repo_root: &Path, file_name: &str) -> String {
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return format!("{file_name}:missing-git-dir");
    };
    repository_path_fingerprint(&git_dir.join(file_name), file_name)
}

fn repository_small_file_hash(path: &Path) -> Option<u64> {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut file = fs::File::open(path).ok()?;
    let mut hash = FNV_OFFSET;
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    Some(hash)
}

fn short_hash(value: &str) -> String {
    value.chars().take(12).collect()
}

#[cfg(test)]
mod git_unquote_tests {
    use super::git_unquote_path;

    #[test]
    fn plain_paths_pass_through() {
        assert_eq!(git_unquote_path("src/main.rs"), "src/main.rs");
        assert_eq!(git_unquote_path("dir/with space.txt"), "dir/with space.txt");
    }

    #[test]
    fn octal_escapes_decode_to_utf8() {
        // git quotes "café/x" as "caf\303\251/x" (\303\251 = U+00E9 in UTF-8).
        assert_eq!(git_unquote_path("\"caf\\303\\251/x\""), "café/x");
    }

    #[test]
    fn simple_escapes_decode() {
        assert_eq!(git_unquote_path("\"a\\tb\""), "a\tb");
        assert_eq!(git_unquote_path("\"quote\\\"here\""), "quote\"here");
        assert_eq!(git_unquote_path("\"back\\\\slash\""), "back\\slash");
    }
}
