use super::*;

pub(super) const REPOSITORY_BUCKETS: [(&str, &str); 6] = [
    ("source", "Source & other"),
    ("dependencies", "Dependencies"),
    ("builds", "Build & test"),
    ("git", "Git data"),
    ("media", "Media & assets"),
    ("workspace", "Workspace files"),
];

const DEPENDENCY_COMPONENTS: [&str; 6] =
    ["node_modules", ".venv", "venv", "vendor", "pods", ".bundle"];
const BUILD_COMPONENTS: [&str; 8] = [
    "target",
    ".build",
    "build",
    "dist",
    ".next",
    "coverage",
    "test-results",
    "out",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RepositoryPathClassification {
    pub(super) bucket_id: &'static str,
    pub(super) artifact_root: Option<PathBuf>,
    pub(super) artifact_kind: Option<&'static str>,
    pub(super) artifact_label: Option<&'static str>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RepositoryArtifactAccumulator {
    pub(super) path: PathBuf,
    pub(super) repository_root: PathBuf,
    pub(super) kind: String,
    pub(super) label: String,
    pub(super) physical_bytes: u64,
    pub(super) file_count: u64,
    pub(super) newest_modified_millis: Option<u64>,
}

/// Classify a path relative to its owning repository. Absolute ancestors are
/// intentionally excluded: a checkout nested below a directory named `build`
/// must not become generated output merely because of its parent location.
pub(super) fn classify_repository_path(
    path: &Path,
    repository_root: Option<&Path>,
) -> RepositoryPathClassification {
    let Some(repository_root) = repository_root else {
        return classification("workspace", None, None, None);
    };
    let relative = path.strip_prefix(repository_root).unwrap_or(path);
    let components = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if components.iter().any(|value| value == ".git") {
        return classification("git", None, None, None);
    }
    if components
        .iter()
        .any(|value| DEPENDENCY_COMPONENTS.contains(&value.as_str()))
    {
        return classification("dependencies", None, None, None);
    }
    if let Some(index) = components
        .iter()
        .position(|value| BUILD_COMPONENTS.contains(&value.as_str()))
    {
        let component = components[index].as_str();
        let (kind, label) = artifact_kind(component, &components[..index]);
        let mut artifact_root = repository_root.to_path_buf();
        for component in relative.components().take(index + 1) {
            artifact_root.push(component.as_os_str());
        }
        return classification("builds", Some(artifact_root), Some(kind), Some(label));
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if [
        "7z", "avi", "gif", "gz", "heic", "jpeg", "jpg", "m4v", "mov", "mp4", "png", "tar", "tgz",
        "webm", "webp", "xz", "zip",
    ]
    .contains(&extension.as_str())
    {
        return classification("media", None, None, None);
    }
    classification("source", None, None, None)
}

fn classification(
    bucket_id: &'static str,
    artifact_root: Option<PathBuf>,
    artifact_kind: Option<&'static str>,
    artifact_label: Option<&'static str>,
) -> RepositoryPathClassification {
    RepositoryPathClassification {
        bucket_id,
        artifact_root,
        artifact_kind,
        artifact_label,
    }
}

fn artifact_kind(component: &str, ancestors: &[String]) -> (&'static str, &'static str) {
    let generated_data = ancestors.iter().any(|value| {
        matches!(
            value.as_str(),
            "data" | "dataset" | "datasets" | "processed"
        )
    });
    if generated_data && matches!(component, "build" | "coverage" | "out") {
        return ("generated-data", "Generated data");
    }
    match component {
        "target" => ("rust-build", "Rust builds"),
        ".build" => ("swift-build", "Swift builds"),
        "dist" | ".next" | "out" => ("web-build", "Web distributions"),
        "coverage" => ("coverage-output", "Coverage reports"),
        "test-results" => ("test-output", "Test results"),
        _ => ("generic-build", "Other builds"),
    }
}

pub(super) fn repository_sub_buckets(
    bucket_bytes: &BTreeMap<&'static str, u64>,
) -> Vec<StorageOwnershipSubBucket> {
    REPOSITORY_BUCKETS
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

pub(super) fn accumulate_repository_artifact(
    accumulators: &mut BTreeMap<String, RepositoryArtifactAccumulator>,
    classification: &RepositoryPathClassification,
    repository_root: &Path,
    metadata: &fs::Metadata,
    physical_bytes: u64,
) {
    let (Some(path), Some(kind), Some(label)) = (
        classification.artifact_root.as_ref(),
        classification.artifact_kind,
        classification.artifact_label,
    ) else {
        return;
    };
    let key = path.display().to_string();
    let accumulator = accumulators
        .entry(key)
        .or_insert_with(|| RepositoryArtifactAccumulator {
            path: path.clone(),
            repository_root: repository_root.to_path_buf(),
            kind: kind.to_owned(),
            label: label.to_owned(),
            ..RepositoryArtifactAccumulator::default()
        });
    accumulator.physical_bytes = accumulator.physical_bytes.saturating_add(physical_bytes);
    accumulator.file_count = accumulator.file_count.saturating_add(1);
    let modified_millis = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64);
    accumulator.newest_modified_millis = accumulator.newest_modified_millis.max(modified_millis);
}

pub(super) fn finalize_repository_artifacts(
    accumulators: BTreeMap<String, RepositoryArtifactAccumulator>,
    measured_at_millis: u64,
) -> Vec<StorageRepositoryArtifact> {
    let mut by_repository = BTreeMap::<PathBuf, Vec<RepositoryArtifactAccumulator>>::new();
    for accumulator in accumulators.into_values() {
        by_repository
            .entry(accumulator.repository_root.clone())
            .or_default()
            .push(accumulator);
    }
    let mut artifacts = Vec::new();
    for (repository_root, repository_artifacts) in by_repository {
        let relative_paths = repository_artifacts
            .iter()
            .filter_map(|artifact| artifact.path.strip_prefix(&repository_root).ok())
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        let ignored = super::repo::git_ignored_path_set(&repository_root, &relative_paths);
        let tracked = super::repo::git_tracked_path_set(&repository_root, &relative_paths);
        let family = super::repo::repository_family_identity(&repository_root);
        for (artifact, relative_path) in repository_artifacts.into_iter().zip(relative_paths) {
            let git_ignored = ignored.contains(&relative_path);
            let git_tracked = tracked.iter().any(|tracked_path| {
                tracked_path == &relative_path
                    || tracked_path.starts_with(&format!("{relative_path}/"))
            });
            let marker = artifact_marker_evidence(&artifact, &repository_root);
            let confidence = if marker.is_some() && git_ignored && !git_tracked {
                "confirmed"
            } else if git_ignored && !git_tracked {
                "likely"
            } else {
                "ambiguous"
            };
            let recently_modified = artifact.newest_modified_millis.is_some_and(|modified| {
                measured_at_millis.saturating_sub(modified) < 60 * 60 * 1_000
            });
            let mut blockers = Vec::new();
            if git_tracked {
                blockers.push("Contains Git-tracked files.".to_owned());
            }
            if !git_ignored {
                blockers.push("Git does not confirm this path as ignored.".to_owned());
            }
            if marker.is_none() {
                blockers.push("No build-tool marker confirms this artifact type.".to_owned());
            }
            if recently_modified {
                blockers.push(
                    "Modified within the last hour; it may belong to active work.".to_owned(),
                );
            }
            let cleanup_allowed = confidence == "confirmed" && blockers.is_empty();
            let mut evidence = vec![format!("Matched {} convention.", artifact.label)];
            if let Some(marker) = marker {
                evidence.push(marker);
            }
            evidence.push(if git_ignored {
                "Git confirms the artifact root is ignored.".to_owned()
            } else {
                "Git ignore evidence is unavailable.".to_owned()
            });
            let repository_identity = fs::symlink_metadata(&repository_root).map_or_else(
                |_| repository_root.display().to_string(),
                |metadata| format!("{}:{}", metadata.dev(), metadata.ino()),
            );
            let id = format!("{}:{repository_identity}:{relative_path}", family.id);
            let rebuild_instruction = rebuild_instruction(&artifact.kind);
            let estimated_rebuild_cost = rebuild_cost(artifact.physical_bytes).to_owned();
            artifacts.push(StorageRepositoryArtifact {
                id,
                path: artifact.path.display().to_string(),
                relative_path,
                repository_root: repository_root.display().to_string(),
                repository_family_id: family.id.clone(),
                repository_family_root: family.root.display().to_string(),
                repository_family_label: family.label.clone(),
                worktree: family.worktree,
                kind: artifact.kind,
                label: artifact.label,
                physical_bytes: artifact.physical_bytes,
                file_count: artifact.file_count,
                newest_modified_millis: artifact.newest_modified_millis,
                evidence,
                confidence: confidence.to_owned(),
                git_ignored,
                git_tracked,
                cleanup_tier: if cleanup_allowed {
                    "rebuildable".to_owned()
                } else {
                    "review".to_owned()
                },
                cleanup_allowed,
                cleanup_blockers: blockers,
                default_cleanup_action: if cleanup_allowed { "trash" } else { "review" }.to_owned(),
                rebuild_instruction,
                estimated_rebuild_cost,
            });
        }
    }
    artifacts.sort_by(|left, right| {
        right
            .physical_bytes
            .cmp(&left.physical_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    artifacts
}

fn rebuild_instruction(kind: &str) -> String {
    match kind {
        "rust-build" => "Run the owning Cargo workspace build or tests.".to_owned(),
        "swift-build" => "Run the owning Swift package build or tests.".to_owned(),
        "web-build" => "Run the owning package's build script.".to_owned(),
        "test-output" => "Run the owning test suite to recreate these results.".to_owned(),
        "coverage-output" => "Run the owning coverage task to recreate this report.".to_owned(),
        "generated-data" => "Use the repository's data pipeline; review before cleanup.".to_owned(),
        _ => "Run the owning project's build workflow.".to_owned(),
    }
}

fn rebuild_cost(bytes: u64) -> &'static str {
    match bytes {
        0..1_000_000_000 => "low",
        1_000_000_000..5_000_000_000 => "moderate",
        _ => "high",
    }
}

fn artifact_marker_evidence(
    artifact: &RepositoryArtifactAccumulator,
    repository_root: &Path,
) -> Option<String> {
    let marker = match artifact.kind.as_str() {
        "rust-build" => ancestor_with_file(&artifact.path, repository_root, "Cargo.toml")
            .map(|_| "Cargo.toml confirms a Cargo target directory."),
        "swift-build" => ancestor_with_file(&artifact.path, repository_root, "Package.swift")
            .map(|_| "Package.swift confirms a SwiftPM build directory."),
        "web-build" | "test-output" | "coverage-output" => {
            ancestor_with_file(&artifact.path, repository_root, "package.json")
                .map(|_| "package.json confirms a project-generated output directory.")
        }
        "generic-build" => [
            "CMakeLists.txt",
            "Package.swift",
            "Cargo.toml",
            "package.json",
        ]
        .into_iter()
        .find_map(|name| ancestor_with_file(&artifact.path, repository_root, name).map(|_| name))
        .map(|name| match name {
            "CMakeLists.txt" => "CMakeLists.txt confirms a generated build directory.",
            "Package.swift" => "Package.swift confirms a generated build directory.",
            "Cargo.toml" => "Cargo.toml confirms a generated build directory.",
            _ => "package.json confirms a generated build directory.",
        }),
        _ => None,
    }?;
    Some(marker.to_owned())
}

fn ancestor_with_file(path: &Path, repository_root: &Path, file_name: &str) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(directory) = current {
        if directory.join(file_name).is_file() {
            return Some(directory.to_path_buf());
        }
        if directory == repository_root {
            break;
        }
        current = directory.parent();
    }
    None
}
