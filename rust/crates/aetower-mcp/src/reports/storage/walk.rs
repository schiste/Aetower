use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SizeWalkResult {
    pub(super) bytes: u64,
    pub(super) allocated_bytes: u64,
    pub(super) truncated: bool,
    pub(super) entries: u64,
    pub(super) max_hardlink_count: u64,
    pub(super) has_hardlinks: bool,
    pub(super) sparse_or_shared: bool,
    pub(super) cloud_placeholder: bool,
}

pub(super) fn scan_root(
    root: &Path,
    options: &StorageHygieneOptions,
    started: Instant,
    now_millis: u64,
    storage_index: &StorageSizeIndex,
    collector: &mut StorageCandidateCollector,
    metrics: &mut StorageScanMetrics,
) -> (BTreeSet<PathBuf>, u64, bool) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut repositories = BTreeSet::new();
    let mut large_directory_candidates: Vec<(usize, PathBuf)> = Vec::new();
    let mut scanned_dirs = 0;
    let mut truncated = false;

    while let Some((path, depth)) = stack.pop() {
        if started.elapsed() >= options.mode.size_walk_time_budget()
            || scanned_dirs >= MAX_DIRECTORIES
        {
            truncated = true;
            break;
        }
        if let Some(runtime) = options.runtime.as_ref()
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&path), 0, 0, 0)
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

        if metadata.is_dir() && is_git_repository_root(&path) {
            repositories.insert(path.clone());
        }

        if let Some(rule) = classify_artifact(&path, &metadata, now_millis) {
            let size = size_of_path(
                &path,
                &metadata,
                root,
                rule,
                started,
                options.mode,
                storage_index,
                &options.dirty_paths,
                metrics,
                now_millis,
                options.runtime.as_ref(),
            );
            if should_retain_storage_item(rule.kind, size.bytes) {
                let item = storage_item_for_path(
                    &path,
                    metadata.modified().ok(),
                    metadata.accessed().ok(),
                    rule,
                    size,
                    now_millis,
                );
                collector.push(item);
            }
            if metadata.is_dir() {
                continue;
            }
        }

        if !metadata.is_dir() || depth >= options.max_depth || is_source_control_dir(&path) {
            continue;
        }

        // Reaching here means the directory matched no artifact rule (matched
        // directories `continue` above). Shallow unclassified directories are
        // large-directory candidates; repositories are excluded because they
        // have their own inventory/footprint lanes. Sizing is deferred to a
        // budgeted pass after the walk so candidate discovery never starves
        // artifact discovery.
        if (1..=LARGE_DIRECTORY_MAX_DEPTH).contains(&depth)
            && !repositories.iter().any(|repo| path.starts_with(repo))
        {
            large_directory_candidates.push((depth, path.clone()));
        }

        scanned_dirs += 1;
        if let Some(runtime) = options.runtime.as_ref()
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&path), 0, 1, 0)
        {
            truncated = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            stack.push((entry.path(), depth + 1));
        }
    }

    truncated |= surface_large_directories(
        root,
        options,
        started,
        now_millis,
        storage_index,
        collector,
        metrics,
        large_directory_candidates,
        &repositories,
    );

    // Root boundary (including cancellation/truncation breaks): make buffered
    // index rows durable before the caller moves on or tears down.
    storage_index.flush_pending_rows();

    (repositories, scanned_dirs, truncated)
}

/// Size shallow unclassified directories with the budget left after the main
/// walk and surface the large ones (>= `LARGE_DIRECTORY_MIN_BYTES`) as
/// review-only "large-directory" items. Candidates are processed shallowest
/// first so an emitted ancestor suppresses all of its descendants, and a
/// fully-sized small ancestor proves its descendants small without walking
/// them again. Sizing reuses `size_of_path`, so fast modes answer from the
/// `StorageSizeIndex` when a previous scan already sized the directory.
/// Returns whether the pass ran out of budget before finishing.
#[allow(clippy::too_many_arguments)]
fn surface_large_directories(
    root: &Path,
    options: &StorageHygieneOptions,
    started: Instant,
    now_millis: u64,
    storage_index: &StorageSizeIndex,
    collector: &mut StorageCandidateCollector,
    metrics: &mut StorageScanMetrics,
    mut candidates: Vec<(usize, PathBuf)>,
    repositories: &BTreeSet<PathBuf>,
) -> bool {
    if candidates.is_empty() {
        return false;
    }
    candidates.sort();
    let mut emitted: Vec<StorageHygieneItem> = Vec::new();
    let mut emitted_prefixes: Vec<PathBuf> = Vec::new();
    let mut proven_small: Vec<PathBuf> = Vec::new();
    let mut truncated = false;
    for (_, path) in candidates {
        if started.elapsed() >= options.mode.size_walk_time_budget() {
            truncated = true;
            break;
        }
        if let Some(runtime) = options.runtime.as_ref()
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&path), 0, 0, 0)
        {
            truncated = true;
            break;
        }
        // Ancestor already emitted (dedupe by prefix) or fully sized below the
        // threshold (a child cannot be larger than its non-truncated parent).
        if emitted_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix))
            || proven_small.iter().any(|prefix| path.starts_with(prefix))
        {
            continue;
        }
        // Directories that contain repositories are dominated by repository
        // storage that the inventory/footprint lanes already report.
        if repositories.iter().any(|repo| repo.starts_with(&path)) {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        let size = size_of_path(
            &path,
            &metadata,
            root,
            LARGE_DIRECTORY_RULE,
            started,
            options.mode,
            storage_index,
            &options.dirty_paths,
            metrics,
            now_millis,
            options.runtime.as_ref(),
        );
        let effective_bytes = if size.allocated_bytes > 0 {
            size.allocated_bytes
        } else {
            size.bytes
        };
        if effective_bytes >= LARGE_DIRECTORY_MIN_BYTES {
            emitted_prefixes.push(path.clone());
            emitted.push(storage_item_for_path(
                &path,
                metadata.modified().ok(),
                metadata.accessed().ok(),
                LARGE_DIRECTORY_RULE,
                size,
                now_millis,
            ));
        } else if !size.truncated {
            proven_small.push(path.clone());
        }
    }
    emitted.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    emitted.truncate(LARGE_DIRECTORY_MAX_PER_ROOT);
    for item in emitted {
        collector.push(item);
    }
    truncated
}

fn should_retain_storage_item(kind: &str, bytes: u64) -> bool {
    bytes >= MIN_ITEM_BYTES
        || matches!(kind, "app-preferences" | "app-receipt" | "app-launch-item") && bytes > 0
}

fn storage_item_for_path(
    path: &Path,
    modified: Option<SystemTime>,
    accessed: Option<SystemTime>,
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
    let accessed_millis = accessed.and_then(system_time_millis);
    let access_age_days = accessed_millis.and_then(|millis| {
        now_millis
            .checked_sub(millis)
            .map(|delta| delta / 86_400_000)
    });
    let cold = access_age_days.is_some_and(|days| days >= COLD_AFTER_DAYS);
    let stale = age_days.is_some_and(|days| days >= STALE_AFTER_DAYS);
    let path_display = path.display().to_string();
    let attribution = artifact_attribution(path);
    let storage_role = storage_role_for_kind(rule.kind);
    let git_status = if attribution.repo_root.is_some() {
        "repo-linked-unchecked"
    } else {
        "outside-git"
    };
    let intelligence = artifact_intelligence(rule.kind, &path_display);
    let logical_bytes = size.bytes;
    let physical_bytes = size.allocated_bytes;
    let reclaimable_bytes = if physical_bytes > 0 {
        physical_bytes
    } else {
        logical_bytes
    };
    StorageHygieneItem {
        id: path_display.clone(),
        path: path_display.clone(),
        display_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact")
            .to_owned(),
        kind: rule.kind.to_owned(),
        storage_role: storage_role.to_owned(),
        git_status: git_status.to_owned(),
        safety: rule.safety.to_owned(),
        cleanup_tier: rule.cleanup_tier.to_owned(),
        size_bytes: reclaimable_bytes,
        logical_bytes,
        physical_bytes,
        byte_accounting: storage_byte_accounting_label(logical_bytes, physical_bytes),
        sparse_or_shared: size.sparse_or_shared,
        hardlink_count: size.max_hardlink_count,
        has_hardlinks: size.has_hardlinks,
        cloud_placeholder: size.cloud_placeholder,
        protected_path: is_protected_cleanup_path(&path_display),
        size_truncated: size.truncated,
        modified_millis,
        age_days,
        accessed_millis,
        access_age_days,
        cold,
        stale,
        reason: rule.reason.to_owned(),
        recommendation: rule.recommendation.to_owned(),
        next_step: String::new(),
        command_hint: format!("du -sh {}", shell_quote(&path_display)),
        rebuild_command: intelligence.rebuild_command,
        estimated_rebuild_cost: intelligence.estimated_rebuild_cost,
        estimated_rebuild_seconds: intelligence.estimated_rebuild_seconds,
        cleanup_consequence: intelligence.cleanup_consequence,
        evidence: Vec::new(),
        cleanup_allowed: true,
        cleanup_blockers: Vec::new(),
        default_cleanup_action: "trash".to_owned(),
        recommendation_score: storage_recommendation_score(
            physical_bytes,
            rule.cleanup_tier,
            modified_millis,
            accessed_millis,
            now_millis,
        ),
        attribution,
    }
}

pub(super) fn storage_item_for_indexed_row(
    row: StorageIndexedFileRow,
    now_millis: u64,
) -> StorageHygieneItem {
    let modified_millis = row.modified_millis;
    let age_days = modified_millis.and_then(|millis| {
        now_millis
            .checked_sub(millis)
            .map(|delta| delta / 86_400_000)
    });
    let accessed_millis = row.accessed_millis;
    let access_age_days = accessed_millis.and_then(|millis| {
        now_millis
            .checked_sub(millis)
            .map(|delta| delta / 86_400_000)
    });
    let cold = access_age_days.is_some_and(|days| days >= COLD_AFTER_DAYS);
    let stale = age_days.is_some_and(|days| days >= STALE_AFTER_DAYS);
    let path = Path::new(&row.path);
    let mut attribution = artifact_attribution(path);
    if attribution.repo_root.is_none()
        && let Some(repo_root) = row.repo_root.as_deref()
    {
        attribution.repo_root = Some(repo_root.to_owned());
        attribution.repo_name = Path::new(repo_root)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        attribution.confidence = "medium".to_owned();
        attribution
            .notes
            .push("Repository attribution came from the persistent storage index.".to_owned());
    }
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact")
        .to_owned();
    let path_display = row.path.clone();
    let intelligence = artifact_intelligence(&row.kind, &path_display);
    let logical_bytes = row.logical_bytes;
    let physical_bytes = row.physical_bytes;
    // Recomputed from the same row-local inputs used at flush time (with the
    // row's own last_scan_millis as the staleness reference) so the displayed
    // score always matches the persisted column that SQL sorts by.
    let recommendation_score = storage_recommendation_score(
        physical_bytes,
        &row.cleanup_tier,
        row.modified_millis,
        row.accessed_millis,
        row.last_scan_millis,
    );
    let mut item = StorageHygieneItem {
        id: path_display.clone(),
        path: path_display.clone(),
        display_name,
        kind: row.kind,
        storage_role: row.storage_role,
        git_status: if attribution.repo_root.is_some() {
            "repo-linked-unchecked".to_owned()
        } else {
            "outside-git".to_owned()
        },
        safety: row.safety,
        cleanup_tier: row.cleanup_tier,
        size_bytes: if physical_bytes > 0 {
            physical_bytes
        } else {
            logical_bytes
        },
        logical_bytes,
        physical_bytes,
        byte_accounting: storage_byte_accounting_label(logical_bytes, physical_bytes),
        sparse_or_shared: physical_bytes > 0 && physical_bytes < logical_bytes,
        hardlink_count: 1,
        has_hardlinks: false,
        cloud_placeholder: logical_bytes > 0 && physical_bytes == 0,
        protected_path: is_protected_cleanup_path(&path_display),
        size_truncated: row.truncated,
        modified_millis,
        age_days,
        accessed_millis,
        access_age_days,
        cold,
        stale,
        reason: "Loaded from Aetower's persistent storage index.".to_owned(),
        recommendation:
            "Review the indexed candidate; run a refresh before destructive cleanup if the path changed recently."
                .to_owned(),
        next_step: String::new(),
        command_hint: format!("du -sh {}", shell_quote(&path_display)),
        rebuild_command: intelligence.rebuild_command,
        estimated_rebuild_cost: intelligence.estimated_rebuild_cost,
        estimated_rebuild_seconds: intelligence.estimated_rebuild_seconds,
        cleanup_consequence: intelligence.cleanup_consequence,
        evidence: Vec::new(),
        cleanup_allowed: true,
        cleanup_blockers: Vec::new(),
        default_cleanup_action: "trash".to_owned(),
        recommendation_score,
        attribution,
    };
    item.evidence = storage_item_evidence(&item);
    item.next_step = storage_item_next_step(&item);
    item
}

#[allow(clippy::too_many_arguments)]
fn indexed_row_for_path(
    path: &Path,
    metadata: &fs::Metadata,
    source_root: &Path,
    repo_root: Option<&str>,
    kind: &str,
    storage_role: &str,
    safety: &str,
    cleanup_tier: &str,
    logical_bytes: u64,
    physical_bytes: u64,
    entries: u64,
    truncated: bool,
    now_millis: u64,
) -> StorageIndexedFileRow {
    let device = metadata.dev() as i64;
    let inode = metadata.ino() as i64;
    let modified_millis = metadata_time_millis(metadata.mtime(), metadata.mtime_nsec());
    let changed_millis = metadata_time_millis(metadata.ctime(), metadata.ctime_nsec());
    let accessed_millis = metadata_time_millis(metadata.atime(), metadata.atime_nsec());
    StorageIndexedFileRow {
        path: path.display().to_string(),
        device,
        inode,
        file_id: format!("{device}:{inode}"),
        source_root: source_root.display().to_string(),
        repo_root: repo_root.map(str::to_owned),
        kind: kind.to_owned(),
        storage_role: storage_role.to_owned(),
        safety: safety.to_owned(),
        cleanup_tier: cleanup_tier.to_owned(),
        logical_bytes,
        physical_bytes,
        modified_millis,
        changed_millis,
        accessed_millis,
        birth_millis: metadata.created().ok().and_then(system_time_millis),
        is_directory: metadata.is_dir(),
        entries,
        truncated,
        last_scan_millis: now_millis,
    }
}

#[allow(clippy::too_many_arguments)]
fn size_of_path(
    path: &Path,
    metadata: &fs::Metadata,
    source_root: &Path,
    rule: ArtifactRule,
    started: Instant,
    mode: StorageScanMode,
    storage_index: &StorageSizeIndex,
    dirty_paths: &[String],
    metrics: &mut StorageScanMetrics,
    now_millis: u64,
    runtime: Option<&StorageScanRuntimeContext>,
) -> SizeWalkResult {
    let size_started = Instant::now();
    if metadata.file_type().is_symlink() {
        return SizeWalkResult::default();
    }
    let kind = rule.kind;
    if metadata.is_file() {
        let blocks = metadata.blocks();
        let hardlink_count = metadata.nlink();
        metrics.sized_entry_count = metrics.sized_entry_count.saturating_add(1);
        let allocated_bytes = blocks.saturating_mul(512);
        let sparse_or_shared = allocated_bytes > 0 && allocated_bytes < metadata.len();
        let cloud_placeholder = metadata.len() > 0 && allocated_bytes == 0;
        let repo_root = if mode.use_storage_index() {
            find_git_root(path).map(|root| root.display().to_string())
        } else {
            None
        };
        if let Some(runtime) = runtime
            && !runtime.checkpoint(
                STORAGE_SCAN_PHASE_ARTIFACT_SIZING,
                Some(path),
                1,
                0,
                metadata.len(),
            )
        {
            let result = SizeWalkResult {
                bytes: metadata.len(),
                allocated_bytes,
                truncated: true,
                entries: 1,
                max_hardlink_count: hardlink_count,
                has_hardlinks: hardlink_count > 1,
                sparse_or_shared,
                cloud_placeholder,
            };
            if mode.use_storage_index() {
                storage_index.store_indexed_row(
                    &indexed_row_for_path(
                        path,
                        metadata,
                        source_root,
                        repo_root.as_deref(),
                        rule.kind,
                        storage_role_for_kind(rule.kind),
                        rule.safety,
                        rule.cleanup_tier,
                        result.bytes,
                        result.allocated_bytes,
                        result.entries,
                        result.truncated,
                        now_millis,
                    ),
                    metrics,
                );
                // Checkpoint refused to continue (cancellation/pause): flush
                // so the interrupted scan leaves no buffered rows behind.
                storage_index.flush_pending_rows();
            }
            return result;
        }
        let result = SizeWalkResult {
            bytes: metadata.len(),
            allocated_bytes,
            truncated: false,
            entries: 1,
            max_hardlink_count: hardlink_count,
            has_hardlinks: hardlink_count > 1,
            sparse_or_shared,
            cloud_placeholder,
        };
        if mode.use_storage_index() {
            storage_index.store_indexed_row(
                &indexed_row_for_path(
                    path,
                    metadata,
                    source_root,
                    repo_root.as_deref(),
                    rule.kind,
                    storage_role_for_kind(rule.kind),
                    rule.safety,
                    rule.cleanup_tier,
                    result.bytes,
                    result.allocated_bytes,
                    result.entries,
                    result.truncated,
                    now_millis,
                ),
                metrics,
            );
        }
        return result;
    }
    if !metadata.is_dir() {
        return SizeWalkResult::default();
    }

    if mode.use_storage_index()
        && let Some(cached) = storage_index.lookup(path, metadata, kind, dirty_paths, metrics)
    {
        metrics.sized_entry_count = metrics.sized_entry_count.saturating_add(cached.entries);
        return cached;
    }

    let repo_root = if mode.use_storage_index() {
        find_git_root(path).map(|root| root.display().to_string())
    } else {
        None
    };
    let mut result = SizeWalkResult::default();
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        if started.elapsed() >= mode.size_walk_time_budget()
            || result.entries >= mode.size_walk_entry_budget()
        {
            result.truncated = true;
            break;
        }
        if let Some(runtime) = runtime
            && !runtime.checkpoint(STORAGE_SCAN_PHASE_ARTIFACT_SIZING, Some(&current), 0, 0, 0)
        {
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
                let logical_bytes = metadata.len();
                let physical_bytes = metadata.blocks().saturating_mul(512);
                result.bytes = result.bytes.saturating_add(logical_bytes);
                result.allocated_bytes = result.allocated_bytes.saturating_add(physical_bytes);
                let hardlink_count = metadata.nlink();
                result.max_hardlink_count = result.max_hardlink_count.max(hardlink_count);
                result.has_hardlinks |= hardlink_count > 1;
                result.sparse_or_shared |= physical_bytes > 0 && physical_bytes < logical_bytes;
                result.cloud_placeholder |= logical_bytes > 0 && physical_bytes == 0;
                if mode.use_storage_index() {
                    storage_index.store_indexed_row(
                        &indexed_row_for_path(
                            &path,
                            &metadata,
                            source_root,
                            repo_root.as_deref(),
                            "indexed-file",
                            "file",
                            "",
                            "",
                            logical_bytes,
                            physical_bytes,
                            1,
                            false,
                            now_millis,
                        ),
                        metrics,
                    );
                }
                if let Some(runtime) = runtime
                    && !runtime.checkpoint(
                        STORAGE_SCAN_PHASE_ARTIFACT_SIZING,
                        Some(&path),
                        1,
                        0,
                        logical_bytes,
                    )
                {
                    result.truncated = true;
                    break;
                }
            }
        }
    }
    metrics.sized_entry_count = metrics.sized_entry_count.saturating_add(result.entries);
    metrics.size_walk_millis = metrics
        .size_walk_millis
        .saturating_add(size_started.elapsed().as_millis() as u64);
    if mode.use_storage_index() {
        storage_index.store(
            path,
            metadata,
            kind,
            repo_root.as_deref(),
            &result,
            now_millis,
            metrics,
        );
        storage_index.store_indexed_row(
            &indexed_row_for_path(
                path,
                metadata,
                source_root,
                repo_root.as_deref(),
                rule.kind,
                storage_role_for_kind(rule.kind),
                rule.safety,
                rule.cleanup_tier,
                result.bytes,
                result.allocated_bytes,
                result.entries,
                result.truncated,
                now_millis,
            ),
            metrics,
        );
    }
    result
}

pub(super) fn is_cloud_storage_path(path: &Path) -> bool {
    let display = path.display().to_string();
    display.contains("/Library/CloudStorage/")
        || display.ends_with("/Library/CloudStorage")
        || display.contains("Mobile Documents")
        || display.contains("iCloud Drive")
}

pub(super) fn is_network_storage_path(path: &Path) -> bool {
    path.display().to_string().starts_with("/Volumes/")
}

pub(super) fn file_access_age_days(metadata: &fs::Metadata, now_millis: u64) -> Option<u64> {
    let accessed_millis = metadata.accessed().ok().and_then(system_time_millis)?;
    now_millis
        .checked_sub(accessed_millis)
        .map(|delta| delta / 86_400_000)
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

pub(super) fn unix_metadata_millis(seconds: i64, nanoseconds: i64) -> i64 {
    seconds
        .saturating_mul(1000)
        .saturating_add(nanoseconds.max(0) / 1_000_000)
}

fn metadata_time_millis(seconds: i64, nanoseconds: i64) -> Option<u64> {
    let millis = unix_metadata_millis(seconds, nanoseconds);
    (millis >= 0).then_some(millis as u64)
}
