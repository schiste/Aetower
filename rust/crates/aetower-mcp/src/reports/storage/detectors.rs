use super::*;

const TYPED_DETECTOR_FAST_BUDGET: Duration = Duration::from_millis(1_500);
const TYPED_DETECTOR_DEEP_BUDGET: Duration = Duration::from_secs(8);
const TYPED_DETECTOR_FORENSIC_BUDGET: Duration = Duration::from_secs(20);
const TYPED_DETECTOR_DISCOVERY_MAX_DEPTH: usize = 6;
const TYPED_DETECTOR_DISCOVERY_ENTRY_BUDGET: u64 = 20_000;
const TYPED_DETECTOR_DOWNLOAD_ENTRY_BUDGET: u64 = 2_000;

pub(super) fn collect_typed_detector_items(
    requested_roots: &[PathBuf],
    options: &StorageHygieneOptions,
    now_millis: u64,
    storage_index: &StorageSizeIndex,
    existing_paths: &BTreeSet<String>,
    metrics: &mut StorageScanMetrics,
) -> Vec<StorageHygieneItem> {
    let detector_started = Instant::now();
    let detector_deadline = detector_started + typed_detector_budget(options.mode);
    let mut candidate_paths = BTreeSet::<PathBuf>::new();

    for root in requested_roots {
        collect_catalog_candidates(root, &mut candidate_paths);
        collect_download_candidates(root, &mut candidate_paths, detector_deadline);
        collect_build_output_candidates(root, &mut candidate_paths, detector_deadline);
    }

    let mut items = Vec::new();
    let mut seen = existing_paths.clone();
    for path in candidate_paths {
        if Instant::now() >= detector_deadline {
            break;
        }
        let path_display = path.display().to_string();
        if !seen.insert(path_display.clone()) {
            continue;
        }
        if let Some(item) = typed_detector_item_for_path(
            &path,
            requested_roots,
            options,
            detector_deadline,
            now_millis,
            storage_index,
            metrics,
        ) {
            items.push(item);
        }
    }

    metrics.root_walk_millis = metrics
        .root_walk_millis
        .saturating_add(detector_started.elapsed().as_millis() as u64);
    items.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    items
}

pub(super) fn merge_typed_detector_items(
    items: &mut Vec<StorageHygieneItem>,
    detector_items: Vec<StorageHygieneItem>,
) -> usize {
    let mut merged = 0usize;
    let mut positions = items
        .iter()
        .enumerate()
        .map(|(index, item)| (item.path.clone(), index))
        .collect::<BTreeMap<_, _>>();

    for detector_item in detector_items {
        if let Some(index) = positions.get(&detector_item.path).copied() {
            let existing = &items[index];
            if typed_detector_item_should_replace(existing, &detector_item) {
                items[index] = detector_item;
                merged = merged.saturating_add(1);
            }
            continue;
        }
        positions.insert(detector_item.path.clone(), items.len());
        items.push(detector_item);
        merged = merged.saturating_add(1);
    }

    items.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    merged
}

fn typed_detector_budget(mode: StorageScanMode) -> Duration {
    match mode {
        StorageScanMode::InstantCached | StorageScanMode::FastChangedOnly => {
            TYPED_DETECTOR_FAST_BUDGET
        }
        StorageScanMode::DeepNative => TYPED_DETECTOR_DEEP_BUDGET,
        StorageScanMode::ForensicVerified => TYPED_DETECTOR_FORENSIC_BUDGET,
    }
}

fn typed_detector_item_should_replace(
    existing: &StorageHygieneItem,
    detected: &StorageHygieneItem,
) -> bool {
    (existing.kind == "large-directory" && detected.kind != "large-directory")
        || (existing.size_truncated && !detected.size_truncated)
        || (existing.size_bytes == 0 && detected.size_bytes > 0)
}

fn collect_catalog_candidates(root: &Path, candidate_paths: &mut BTreeSet<PathBuf>) {
    candidate_paths.insert(root.to_path_buf());
    for relative in home_relative_detector_paths() {
        candidate_paths.insert(root.join(relative));
    }

    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match name {
        "Library" => {
            for relative in library_relative_detector_paths() {
                candidate_paths.insert(root.join(relative));
            }
        }
        "Developer" => {
            for relative in developer_relative_detector_paths() {
                candidate_paths.insert(root.join(relative));
            }
        }
        "Xcode" => {
            for relative in xcode_relative_detector_paths() {
                candidate_paths.insert(root.join(relative));
            }
        }
        "CoreSimulator" => {
            candidate_paths.insert(root.join("Caches"));
            candidate_paths.insert(root.join("Devices"));
        }
        "Caches" => {
            for relative in cache_relative_detector_paths() {
                candidate_paths.insert(root.join(relative));
            }
        }
        "Application Support" => {
            candidate_paths.insert(root.join("Claude"));
            candidate_paths.insert(root.join("Chau7"));
            candidate_paths.insert(root.join("Aetower"));
        }
        "Containers" => {
            for relative in offline_media_container_names() {
                candidate_paths.insert(root.join(relative));
            }
        }
        ".colima" => {
            candidate_paths.insert(root.join("_lima"));
            candidate_paths.insert(root.join("_lima").join("_disks"));
        }
        ".docker" => {
            candidate_paths.insert(root.join("buildx"));
            candidate_paths.insert(root.join("overlay2"));
            candidate_paths.insert(root.join("volumes"));
        }
        ".codex" => {
            candidate_paths.insert(root.join("sessions"));
        }
        ".claude" => {
            candidate_paths.insert(root.join("projects"));
        }
        ".cache" => {
            for relative in dot_cache_detector_paths() {
                candidate_paths.insert(root.join(relative));
            }
        }
        _ => {}
    }
}

fn collect_download_candidates(
    root: &Path,
    candidate_paths: &mut BTreeSet<PathBuf>,
    deadline: Instant,
) {
    let mut download_roots = Vec::new();
    if root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "Downloads")
    {
        download_roots.push(root.to_path_buf());
    }
    let nested_downloads = root.join("Downloads");
    if nested_downloads.is_dir() {
        download_roots.push(nested_downloads);
    }

    for downloads in download_roots {
        if Instant::now() >= deadline {
            break;
        }
        let Ok(entries) = fs::read_dir(downloads) else {
            continue;
        };
        for (index, entry) in entries.flatten().enumerate() {
            if index as u64 >= TYPED_DETECTOR_DOWNLOAD_ENTRY_BUDGET || Instant::now() >= deadline {
                break;
            }
            let path = entry.path();
            if typed_download_candidate(&path) {
                candidate_paths.insert(path);
            }
        }
    }
}

fn collect_build_output_candidates(
    root: &Path,
    candidate_paths: &mut BTreeSet<PathBuf>,
    deadline: Instant,
) {
    if !root.is_dir() || !root_can_hold_source_projects(root) {
        return;
    }

    let mut stack = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut scanned = 0u64;
    while let Some((path, depth)) = stack.pop_front() {
        if Instant::now() >= deadline
            || scanned >= TYPED_DETECTOR_DISCOVERY_ENTRY_BUDGET
            || depth > TYPED_DETECTOR_DISCOVERY_MAX_DEPTH
        {
            break;
        }
        scanned = scanned.saturating_add(1);
        if is_source_control_dir(&path) {
            continue;
        }

        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if name == "target" && ancestor_has_manifest(path.parent(), "Cargo.toml", 4) {
            candidate_paths.insert(path);
            continue;
        }
        if name == ".build" && ancestor_has_manifest(path.parent(), "Package.swift", 4) {
            candidate_paths.insert(path);
            continue;
        }
        if depth > 0 && should_skip_source_project_branch(name) {
            continue;
        }

        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                stack.push_back((child, depth + 1));
            }
        }
    }
}

fn typed_detector_item_for_path(
    path: &Path,
    requested_roots: &[PathBuf],
    options: &StorageHygieneOptions,
    deadline: Instant,
    now_millis: u64,
    storage_index: &StorageSizeIndex,
    metrics: &mut StorageScanMetrics,
) -> Option<StorageHygieneItem> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }
    let rule = classify_artifact(path, &metadata, now_millis)?;
    let source_root = detector_source_root(path, requested_roots);
    let size = size_of_path(
        path,
        &metadata,
        &source_root,
        rule.clone(),
        deadline,
        options.mode,
        storage_index,
        &options.dirty_paths,
        metrics,
        now_millis,
        options.runtime.as_ref(),
    );
    if !should_retain_storage_item(rule.kind.as_ref(), size.bytes) {
        return None;
    }
    let mut item = storage_item_for_path(
        path,
        metadata.modified().ok(),
        metadata.accessed().ok(),
        rule,
        size,
        now_millis,
    );
    item.attribution.notes.push(
        "Surfaced by Aetower's typed storage detector before the generic walk completed."
            .to_owned(),
    );
    Some(item)
}

fn detector_source_root(path: &Path, requested_roots: &[PathBuf]) -> PathBuf {
    requested_roots
        .iter()
        .filter(|root| path.starts_with(root))
        .max_by_key(|root| root.components().count())
        .cloned()
        .unwrap_or_else(|| path.to_path_buf())
}

fn typed_download_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".dmg")
        || lower.ends_with(".pkg")
        || lower.ends_with(".zip")
        || lower.ends_with(".tar")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".iso")
        || lower.ends_with(".mp4")
        || lower.ends_with(".mov")
        || lower.ends_with(".mkv")
        || lower.ends_with(".webm")
        || lower.ends_with(".m4v")
        || lower.ends_with(".avi")
}

fn root_can_hold_source_projects(root: &Path) -> bool {
    let display = root.display().to_string();
    if display.contains("/Library/") || display.ends_with("/Library") {
        return false;
    }
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    !matches!(
        name,
        "Downloads"
            | "Desktop"
            | "Documents"
            | "Caches"
            | "Application Support"
            | "Containers"
            | ".colima"
            | ".docker"
            | ".codex"
            | ".claude"
            | ".cache"
            | ".npm"
            | ".cargo"
    )
}

fn should_skip_source_project_branch(name: &str) -> bool {
    matches!(
        name,
        "Library"
            | "Applications"
            | "Desktop"
            | "Documents"
            | "Downloads"
            | "Movies"
            | "Music"
            | "Pictures"
            | "Caches"
            | "Application Support"
            | "Containers"
            | ".colima"
            | ".docker"
            | ".codex"
            | ".claude"
            | ".cache"
            | ".npm"
            | ".cargo"
            | "node_modules"
    )
}

fn ancestor_has_manifest(
    mut current: Option<&Path>,
    manifest_name: &str,
    max_levels: usize,
) -> bool {
    for _ in 0..max_levels {
        let Some(path) = current else {
            return false;
        };
        if path.join(manifest_name).is_file() {
            return true;
        }
        current = path.parent();
    }
    false
}

fn home_relative_detector_paths() -> &'static [&'static str] {
    &[
        "Library/Developer/Xcode/DerivedData",
        "Library/Developer/Xcode/iOS DeviceSupport",
        "Library/Developer/Xcode/watchOS DeviceSupport",
        "Library/Developer/Xcode/tvOS DeviceSupport",
        "Library/Developer/CoreSimulator/Caches",
        "Library/Developer/CoreSimulator/Devices",
        "Library/Caches/Homebrew",
        "Library/Caches/pip",
        "Library/Caches/go-build",
        "Library/Caches/pnpm",
        "Library/Caches/org.swift.swiftpm",
        "Library/Caches/com.apple.dt.Xcode",
        "Library/Application Support/Claude",
        "Library/Application Support/Chau7",
        "Library/Application Support/Aetower",
        "Library/Containers/com.apple.podcasts",
        "Library/Containers/com.apple.TV",
        "Library/Containers/com.apple.Music",
        "Library/Containers/com.spotify.client",
        "Library/Containers/com.docker.docker/Data/vms",
        ".colima/_lima",
        ".colima/_lima/_disks",
        ".docker/buildx",
        ".docker/overlay2",
        ".docker/volumes",
        ".cache/uv",
        ".cache/pre-commit",
        ".cache/pip",
        ".npm",
        ".pnpm-store",
        ".yarn/cache",
        ".codex/sessions",
        ".claude/projects",
        ".cursor",
        ".aider",
        "Downloads",
    ]
}

fn library_relative_detector_paths() -> &'static [&'static str] {
    &[
        "Developer/Xcode/DerivedData",
        "Developer/Xcode/iOS DeviceSupport",
        "Developer/Xcode/watchOS DeviceSupport",
        "Developer/Xcode/tvOS DeviceSupport",
        "Developer/CoreSimulator/Caches",
        "Developer/CoreSimulator/Devices",
        "Caches/Homebrew",
        "Caches/pip",
        "Caches/go-build",
        "Caches/pnpm",
        "Caches/org.swift.swiftpm",
        "Caches/com.apple.dt.Xcode",
        "Application Support/Claude",
        "Application Support/Chau7",
        "Application Support/Aetower",
        "Containers/com.apple.podcasts",
        "Containers/com.apple.TV",
        "Containers/com.apple.Music",
        "Containers/com.spotify.client",
        "Containers/com.docker.docker/Data/vms",
    ]
}

fn developer_relative_detector_paths() -> &'static [&'static str] {
    &[
        "Xcode/DerivedData",
        "Xcode/iOS DeviceSupport",
        "Xcode/watchOS DeviceSupport",
        "Xcode/tvOS DeviceSupport",
        "CoreSimulator/Caches",
        "CoreSimulator/Devices",
    ]
}

fn xcode_relative_detector_paths() -> &'static [&'static str] {
    &[
        "DerivedData",
        "iOS DeviceSupport",
        "watchOS DeviceSupport",
        "tvOS DeviceSupport",
    ]
}

fn cache_relative_detector_paths() -> &'static [&'static str] {
    &[
        "Homebrew",
        "pip",
        "go-build",
        "pnpm",
        "org.swift.swiftpm",
        "com.apple.dt.Xcode",
    ]
}

fn dot_cache_detector_paths() -> &'static [&'static str] {
    &["uv", "pre-commit", "pip"]
}

fn offline_media_container_names() -> &'static [&'static str] {
    &[
        "com.apple.podcasts",
        "com.apple.TV",
        "com.apple.Music",
        "com.spotify.client",
    ]
}
