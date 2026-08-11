use super::*;

pub(super) const STORAGE_OWNERSHIP_CLASSIFIER_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StorageOwnershipCategoryDefinition {
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    pub(super) rank: u16,
    pub(super) detail: &'static str,
}

pub(super) const STORAGE_OWNERSHIP_CATEGORIES: [StorageOwnershipCategoryDefinition; 6] = [
    StorageOwnershipCategoryDefinition {
        id: "system",
        label: "System",
        rank: 10,
        detail: "macOS volumes, writable system assets, databases, logs, and temporary state.",
    },
    StorageOwnershipCategoryDefinition {
        id: "repositories",
        label: "Repositories",
        rank: 20,
        detail: "Source trees, dependencies, build products, Git data, and repository-local media.",
    },
    StorageOwnershipCategoryDefinition {
        id: "applications",
        label: "Apps & Support",
        rank: 30,
        detail: "Installed applications, containers, group containers, and per-user support data.",
    },
    StorageOwnershipCategoryDefinition {
        id: "developer",
        label: "Developer",
        rank: 40,
        detail: "Container VMs, toolchains, package stores, IDE data, and agent workspaces.",
    },
    StorageOwnershipCategoryDefinition {
        id: "personal",
        label: "Personal Data",
        rank: 50,
        detail: "Documents, media, downloads, backups, and locally materialized cloud files.",
    },
    StorageOwnershipCategoryDefinition {
        id: "other",
        label: "Other",
        rank: 90,
        detail: "Measured miscellaneous data and capacity not yet assigned to a durable boundary.",
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StorageOwnershipBoundarySpec {
    pub(super) boundary_id: String,
    pub(super) category_id: &'static str,
    pub(super) rule_id: &'static str,
    pub(super) root_path: PathBuf,
    pub(super) source: &'static str,
}

impl StorageOwnershipBoundarySpec {
    pub(super) fn fixed(
        category_id: &'static str,
        rule_id: &'static str,
        root_path: PathBuf,
    ) -> Self {
        Self {
            boundary_id: format!("{rule_id}:{}", root_path.display()),
            category_id,
            rule_id,
            root_path,
            source: "canonical_rule",
        }
    }

    pub(super) fn repository(root_path: PathBuf, source: &'static str) -> Self {
        Self {
            boundary_id: format!("repository.workspace:{}", root_path.display()),
            category_id: "repositories",
            rule_id: "repository.workspace",
            root_path,
            source,
        }
    }
}

pub(super) fn storage_ownership_category(
    id: &str,
) -> Option<&'static StorageOwnershipCategoryDefinition> {
    STORAGE_OWNERSHIP_CATEGORIES
        .iter()
        .find(|definition| definition.id == id)
}

pub(super) fn canonical_storage_ownership_boundaries(
    repository_roots: &[PathBuf],
) -> Vec<StorageOwnershipBoundarySpec> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut boundaries = vec![
        StorageOwnershipBoundarySpec::fixed(
            "system",
            "system.data-assets",
            PathBuf::from("/System/Volumes/Data/System"),
        ),
        StorageOwnershipBoundarySpec::fixed("system", "system.private", PathBuf::from("/private")),
        StorageOwnershipBoundarySpec::fixed("system", "system.library", PathBuf::from("/Library")),
        StorageOwnershipBoundarySpec::fixed(
            "applications",
            "applications.installed-system",
            PathBuf::from("/Applications"),
        ),
        StorageOwnershipBoundarySpec::fixed(
            "applications",
            "applications.installed-user",
            home.join("Applications"),
        ),
        StorageOwnershipBoundarySpec::fixed(
            "applications",
            "applications.support",
            home.join("Library/Application Support"),
        ),
        StorageOwnershipBoundarySpec::fixed(
            "applications",
            "applications.containers",
            home.join("Library/Containers"),
        ),
        StorageOwnershipBoundarySpec::fixed(
            "applications",
            "applications.group-containers",
            home.join("Library/Group Containers"),
        ),
        StorageOwnershipBoundarySpec::fixed(
            "applications",
            "applications.caches",
            home.join("Library/Caches"),
        ),
        StorageOwnershipBoundarySpec::fixed(
            "developer",
            "developer.homebrew",
            PathBuf::from("/opt/homebrew"),
        ),
        StorageOwnershipBoundarySpec::fixed(
            "developer",
            "developer.system-library",
            PathBuf::from("/Library/Developer"),
        ),
        StorageOwnershipBoundarySpec::fixed(
            "developer",
            "developer.user-library",
            home.join("Library/Developer"),
        ),
    ];
    for relative in [
        ".cache",
        ".cargo",
        ".claude",
        ".codex",
        ".colima",
        ".continue",
        ".docker",
        ".local",
        ".npm",
        ".nvm",
        ".ollama",
        ".openclaw",
        ".pnpm-store",
        ".rustup",
        ".volta",
        "go",
    ] {
        boundaries.push(StorageOwnershipBoundarySpec::fixed(
            "developer",
            "developer.user-tooling",
            home.join(relative),
        ));
    }
    for relative in [
        "Backups",
        "Desktop",
        "Documents",
        "Downloads",
        "Library/CloudStorage",
        "Library/Mobile Documents",
        "Movies",
        "Music",
        "Pictures",
    ] {
        boundaries.push(StorageOwnershipBoundarySpec::fixed(
            "personal",
            "personal.user-data",
            home.join(relative),
        ));
    }
    boundaries.extend(
        repository_roots
            .iter()
            .cloned()
            .map(|root| StorageOwnershipBoundarySpec::repository(root, "repository_root")),
    );
    normalize_storage_ownership_boundaries(boundaries)
}

pub(super) fn normalize_storage_ownership_boundaries(
    boundaries: Vec<StorageOwnershipBoundarySpec>,
) -> Vec<StorageOwnershipBoundarySpec> {
    let mut by_path = BTreeMap::<PathBuf, StorageOwnershipBoundarySpec>::new();
    for boundary in boundaries
        .into_iter()
        .filter(|boundary| boundary.root_path.is_dir())
    {
        let replace = by_path
            .get(&boundary.root_path)
            .is_none_or(|existing| boundary_precedence(&boundary) < boundary_precedence(existing));
        if replace {
            by_path.insert(boundary.root_path.clone(), boundary);
        }
    }
    let mut normalized = by_path.into_values().collect::<Vec<_>>();
    normalized.sort_by(|left, right| {
        boundary_precedence(left)
            .cmp(&boundary_precedence(right))
            .then_with(|| left.root_path.cmp(&right.root_path))
    });
    normalized
}

fn boundary_precedence(boundary: &StorageOwnershipBoundarySpec) -> (Reverse<usize>, u16, &str) {
    (
        Reverse(boundary.root_path.components().count()),
        storage_ownership_category(boundary.category_id)
            .map(|category| category.rank)
            .unwrap_or(u16::MAX),
        boundary.boundary_id.as_str(),
    )
}

pub(super) fn storage_ownership_boundary_for_path<'a>(
    path: &Path,
    boundaries: &'a [StorageOwnershipBoundarySpec],
) -> Option<&'a StorageOwnershipBoundarySpec> {
    boundaries
        .iter()
        .filter(|boundary| path_is_under_root(&path.display().to_string(), &boundary.root_path))
        .min_by(|left, right| boundary_precedence(left).cmp(&boundary_precedence(right)))
}

pub(super) fn storage_ownership_excluded_descendants(
    boundary: &StorageOwnershipBoundarySpec,
    boundaries: &[StorageOwnershipBoundarySpec],
) -> Vec<PathBuf> {
    let mut descendants = boundaries
        .iter()
        .filter(|candidate| {
            candidate.root_path != boundary.root_path
                && path_is_under_root(
                    &candidate.root_path.display().to_string(),
                    &boundary.root_path,
                )
        })
        .map(|candidate| candidate.root_path.clone())
        .collect::<Vec<_>>();
    descendants.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });
    let mut exclusive = Vec::<PathBuf>::new();
    for descendant in descendants {
        if exclusive
            .iter()
            .any(|parent| path_is_under_root(&descendant.display().to_string(), parent))
        {
            continue;
        }
        exclusive.push(descendant);
    }
    exclusive
}

const STORAGE_OWNERSHIP_FRESH_MILLIS: u64 = 6 * 60 * 60 * 1000;
const REPOSITORY_WORKSPACE_DISCOVERY_MAX_DEPTH: usize = 4;
const REPOSITORY_WORKSPACE_DISCOVERY_DIRECTORY_BUDGET: usize = 12_000;

pub fn storage_ownership_refresh_json(
    repository_roots: Vec<String>,
    force: bool,
) -> Result<String, String> {
    let captured_at_millis = storage_now_millis();
    let configured_repository_roots =
        super::repo::normalize_repository_workspace_roots(repository_roots);
    let storage_index = StorageSizeIndex::open();
    let repository_roots = durable_repository_workspace_roots(
        &storage_index,
        configured_repository_roots,
        captured_at_millis,
    )?;
    let cached_repository_roots = storage_index
        .load_repository_inventory_cache(&repository_roots)
        .into_keys()
        .collect::<BTreeSet<_>>();
    let repository_boundary_roots =
        repository_ownership_boundary_roots(&repository_roots, &cached_repository_roots);
    let boundaries = canonical_storage_ownership_boundaries(&repository_boundary_roots);
    let active_generation = storage_index.load_active_ownership_generation();
    let current_boundary_ids = boundaries
        .iter()
        .map(|boundary| boundary.boundary_id.as_str())
        .collect::<BTreeSet<_>>();
    let reusable_by_boundary = active_generation
        .as_ref()
        .filter(|generation| {
            generation.classifier_version == STORAGE_OWNERSHIP_CLASSIFIER_VERSION
                && generation
                    .rollups
                    .iter()
                    .map(|rollup| rollup.boundary_id.as_str())
                    .collect::<BTreeSet<_>>()
                    == current_boundary_ids
        })
        .map(|generation| {
            generation
                .rollups
                .iter()
                .map(|rollup| (rollup.boundary_id.as_str(), rollup))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    let mut measured_boundary_count = 0u64;
    let mut reused_boundary_count = 0u64;
    let mut rollups = Vec::with_capacity(boundaries.len());
    for boundary in &boundaries {
        let cached = reusable_by_boundary
            .get(boundary.boundary_id.as_str())
            .copied();
        let current_identity = fs::symlink_metadata(&boundary.root_path)
            .ok()
            .map(|metadata| (metadata.dev(), metadata.ino()));
        let excluded_roots = storage_ownership_excluded_descendants(boundary, &boundaries);
        let latest_dirty_millis =
            storage_index.latest_dirty_millis_for_boundary(&boundary.root_path, &excluded_roots);
        let cache_is_fresh = cached.is_some_and(|rollup| {
            current_identity == Some((rollup.filesystem_device, rollup.filesystem_inode))
                && captured_at_millis.saturating_sub(rollup.measured_at_millis)
                    < STORAGE_OWNERSHIP_FRESH_MILLIS
                && latest_dirty_millis
                    .is_none_or(|dirty_millis| dirty_millis <= rollup.measured_at_millis)
        });
        if !force && cache_is_fresh {
            if let Some(cached) = cached {
                rollups.push(cached.clone());
                reused_boundary_count = reused_boundary_count.saturating_add(1);
            }
            continue;
        }
        rollups.push(measure_storage_ownership_boundary(
            boundary,
            &boundaries,
            &cached_repository_roots,
        ));
        measured_boundary_count = measured_boundary_count.saturating_add(1);
    }
    validate_storage_ownership_rollups(&boundaries, &rollups)?;
    let status = if rollups.iter().all(|rollup| rollup.complete) {
        "complete"
    } else {
        "partial"
    };
    let generation = storage_index.activate_ownership_generation(
        STORAGE_OWNERSHIP_CLASSIFIER_VERSION,
        captured_at_millis,
        status,
        &rollups,
    )?;
    serde_json::to_string(&StorageOwnershipRefreshResponse {
        captured_at_millis,
        generation_id: generation.generation_id,
        classifier_version: generation.classifier_version,
        status: generation.status,
        measured_boundary_count,
        reused_boundary_count,
        rollups: generation.rollups,
    })
    .map_err(|error| error.to_string())
}

pub(super) fn repository_ownership_boundary_roots(
    workspace_roots: &[PathBuf],
    repository_roots: &BTreeSet<String>,
) -> Vec<PathBuf> {
    let mut boundaries = workspace_roots.iter().cloned().collect::<BTreeSet<_>>();
    for repository_root in repository_roots {
        let repository_root = Path::new(repository_root);
        let Some(workspace_root) = workspace_roots.iter().find(|workspace_root| {
            path_is_under_root(&repository_root.display().to_string(), workspace_root)
        }) else {
            continue;
        };
        let Ok(relative) = repository_root.strip_prefix(workspace_root) else {
            continue;
        };
        let owner = relative.components().next().map_or_else(
            || workspace_root.clone(),
            |component| workspace_root.join(component),
        );
        if owner.is_dir() {
            boundaries.insert(owner);
        }
    }
    boundaries.into_iter().collect()
}

fn durable_repository_workspace_roots(
    storage_index: &StorageSizeIndex,
    configured_roots: Vec<PathBuf>,
    captured_at_millis: u64,
) -> Result<Vec<PathBuf>, String> {
    let persisted_roots = storage_index.load_repository_workspace_roots();
    let mut candidates = configured_roots
        .iter()
        .cloned()
        .map(|path| (path, "configured".to_owned()))
        .collect::<BTreeMap<_, _>>();
    for root in &persisted_roots {
        let path = PathBuf::from(&root.root_path);
        if path.is_dir() {
            candidates
                .entry(path)
                .or_insert_with(|| root.source.clone());
        }
    }
    if let Some(home) = dirs::home_dir() {
        for path in discover_repository_workspace_roots(
            &home,
            &candidates.keys().cloned().collect::<Vec<_>>(),
            REPOSITORY_WORKSPACE_DISCOVERY_MAX_DEPTH,
            REPOSITORY_WORKSPACE_DISCOVERY_DIRECTORY_BUDGET,
        ) {
            candidates
                .entry(path)
                .or_insert_with(|| "discovered".to_owned());
        }
    }

    let records = candidates
        .iter()
        .filter_map(|(path, source)| {
            repository_workspace_root_record(path, source, captured_at_millis, &persisted_roots)
        })
        .collect::<Vec<_>>();
    storage_index.store_repository_workspace_roots(&records)?;
    Ok(super::repo::normalize_repository_workspace_roots(
        candidates
            .into_keys()
            .map(|path| path.display().to_string())
            .collect(),
    ))
}

fn repository_workspace_root_record(
    path: &Path,
    source: &str,
    captured_at_millis: u64,
    persisted_roots: &[StorageRepositoryWorkspaceRoot],
) -> Option<StorageRepositoryWorkspaceRoot> {
    let metadata = fs::symlink_metadata(path).ok()?;
    let previous = persisted_roots.iter().find(|root| {
        root.root_path == path.display().to_string()
            || (root.filesystem_device == metadata.dev() && root.filesystem_inode == metadata.ino())
    });
    metadata.is_dir().then(|| StorageRepositoryWorkspaceRoot {
        root_path: path.display().to_string(),
        filesystem_device: metadata.dev(),
        filesystem_inode: metadata.ino(),
        source: previous
            .filter(|root| root.source == "configured")
            .map_or_else(|| source.to_owned(), |root| root.source.clone()),
        first_seen_millis: previous.map_or(captured_at_millis, |root| root.first_seen_millis),
        last_seen_millis: captured_at_millis,
    })
}

pub(super) fn discover_repository_workspace_roots(
    home: &Path,
    known_roots: &[PathBuf],
    max_depth: usize,
    directory_budget: usize,
) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(home) else {
        return Vec::new();
    };
    let mut candidates = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| repository_workspace_discovery_candidate(path, known_roots))
        .collect::<Vec<_>>();
    candidates.sort();
    let per_candidate_budget = directory_budget
        .checked_div(candidates.len().max(1))
        .unwrap_or_default()
        .max(1);
    candidates
        .into_iter()
        .filter(|candidate| {
            let mut remaining_budget = per_candidate_budget;
            contains_git_repository(candidate, max_depth, &mut remaining_budget)
        })
        .collect()
}

fn repository_workspace_discovery_candidate(path: &Path, known_roots: &[PathBuf]) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    if known_roots
        .iter()
        .any(|root| path_is_under_root(&path.display().to_string(), root))
    {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    !name.starts_with('.')
        && !matches!(
            name,
            "Applications"
                | "Backups"
                | "Desktop"
                | "Documents"
                | "Downloads"
                | "Library"
                | "Movies"
                | "Music"
                | "Pictures"
                | "Public"
        )
}

fn contains_git_repository(
    candidate: &Path,
    max_depth: usize,
    remaining_budget: &mut usize,
) -> bool {
    let mut pending = VecDeque::from([(candidate.to_path_buf(), 0usize)]);
    while let Some((directory, depth)) = pending.pop_front() {
        if *remaining_budget == 0 {
            return false;
        }
        *remaining_budget -= 1;
        if directory.join(".git").exists() {
            return true;
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        let mut children = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| repository_discovery_descends_into(path))
            .collect::<Vec<_>>();
        children.sort();
        pending.extend(children.into_iter().map(|path| (path, depth + 1)));
    }
    false
}

fn repository_discovery_descends_into(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    !name.starts_with('.')
        && !matches!(
            name,
            "Pods" | "build" | "Build" | "dist" | "node_modules" | "target" | "vendor"
        )
}

pub(super) fn measure_storage_ownership_boundary(
    boundary: &StorageOwnershipBoundarySpec,
    boundaries: &[StorageOwnershipBoundarySpec],
    cached_repository_roots: &BTreeSet<String>,
) -> StorageOwnershipBoundaryRollup {
    let started = Instant::now();
    let measured_at_millis = storage_now_millis();
    let root_metadata = fs::symlink_metadata(&boundary.root_path).ok();
    let filesystem_device = root_metadata.as_ref().map_or(0, MetadataExt::dev);
    let filesystem_inode = root_metadata.as_ref().map_or(0, MetadataExt::ino);
    let excluded_roots = storage_ownership_excluded_descendants(boundary, boundaries)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let known_repository_roots = cached_repository_roots
        .iter()
        .filter(|root| path_is_under_root(root, &boundary.root_path))
        .cloned()
        .collect::<BTreeSet<_>>();
    let root_key = boundary.root_path.display().to_string();
    let root_is_repository = boundary.category_id == "repositories"
        && (known_repository_roots.contains(&root_key)
            || is_git_repository_root(&boundary.root_path));
    let mut stack = vec![(boundary.root_path.clone(), root_is_repository)];
    let mut seen_hardlinks = BTreeSet::<(u64, u64)>::new();
    let mut logical_bytes = 0u64;
    let mut physical_bytes = 0u64;
    let mut entry_count = 0u64;
    let mut complete = root_metadata.is_some();
    let mut sub_bucket_bytes = BTreeMap::<&'static str, u64>::new();

    while let Some((directory, directory_is_repository)) = stack.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            let path = entry.path();
            if excluded_roots.contains(&path) {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            entry_count = entry_count.saturating_add(1);
            let path_key = path.display().to_string();
            let path_is_repository = directory_is_repository
                || known_repository_roots.contains(&path_key)
                || (boundary.category_id == "repositories" && is_git_repository_root(&path));
            if metadata.is_dir() {
                stack.push((path, path_is_repository));
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            if metadata.nlink() > 1 && !seen_hardlinks.insert((metadata.dev(), metadata.ino())) {
                continue;
            }
            let logical = metadata.len();
            let physical = metadata.blocks().saturating_mul(512);
            logical_bytes = logical_bytes.saturating_add(logical);
            physical_bytes = physical_bytes.saturating_add(physical);
            if boundary.category_id == "repositories" {
                let sub_bucket =
                    super::repo::repository_workspace_bucket(&path, path_is_repository);
                let total = sub_bucket_bytes.entry(sub_bucket).or_default();
                *total = total.saturating_add(physical);
            }
        }
    }
    let sub_buckets = repository_sub_buckets(&sub_bucket_bytes);
    let category = storage_ownership_category(boundary.category_id);
    StorageOwnershipBoundaryRollup {
        boundary_id: boundary.boundary_id.clone(),
        category_id: boundary.category_id.to_owned(),
        rule_id: boundary.rule_id.to_owned(),
        rank: category.map_or(u16::MAX, |definition| definition.rank),
        root_path: root_key,
        filesystem_device,
        filesystem_inode,
        logical_bytes,
        physical_bytes,
        entry_count,
        measured_at_millis,
        duration_millis: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        complete,
        confidence: if complete { "measured" } else { "partial" }.to_owned(),
        source: boundary.source.to_owned(),
        sub_buckets,
    }
}

fn repository_sub_buckets(
    bucket_bytes: &BTreeMap<&'static str, u64>,
) -> Vec<StorageOwnershipSubBucket> {
    [
        ("source", "Source & other"),
        ("dependencies", "Dependencies"),
        ("builds", "Build & test"),
        ("git", "Git data"),
        ("media", "Media & assets"),
        ("workspace", "Workspace files"),
    ]
    .into_iter()
    .filter_map(|(id, label)| {
        let bytes = bucket_bytes.get(id).copied().unwrap_or_default();
        (bytes > 0).then(|| StorageOwnershipSubBucket {
            id: id.to_owned(),
            label: label.to_owned(),
            bytes,
        })
    })
    .collect()
}

fn validate_storage_ownership_rollups(
    boundaries: &[StorageOwnershipBoundarySpec],
    rollups: &[StorageOwnershipBoundaryRollup],
) -> Result<(), String> {
    if rollups.len() != boundaries.len() {
        return Err("ownership_generation_incomplete_boundary_set".to_owned());
    }
    let unique_boundaries = rollups
        .iter()
        .map(|rollup| rollup.boundary_id.as_str())
        .collect::<BTreeSet<_>>();
    if unique_boundaries.len() != rollups.len() {
        return Err("ownership_generation_duplicate_boundary".to_owned());
    }
    for rollup in rollups {
        let root_path = Path::new(&rollup.root_path);
        let Some(expected_boundary) = storage_ownership_boundary_for_path(root_path, boundaries)
        else {
            return Err(format!(
                "ownership_generation_unmatched_boundary:{}",
                rollup.boundary_id
            ));
        };
        if expected_boundary.boundary_id != rollup.boundary_id {
            return Err(format!(
                "ownership_generation_noncanonical_boundary:{}",
                rollup.boundary_id
            ));
        }
        if storage_ownership_category(&rollup.category_id).is_none() {
            return Err(format!(
                "ownership_generation_unknown_category:{}",
                rollup.category_id
            ));
        }
        let sub_bucket_total = rollup
            .sub_buckets
            .iter()
            .fold(0u64, |total, bucket| total.saturating_add(bucket.bytes));
        if sub_bucket_total > rollup.physical_bytes {
            return Err(format!(
                "ownership_generation_sub_bucket_overflow:{}",
                rollup.boundary_id
            ));
        }
    }
    Ok(())
}
