use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ArtifactRule {
    pub(super) kind: &'static str,
    pub(super) safety: &'static str,
    pub(super) cleanup_tier: &'static str,
    pub(super) reason: &'static str,
    pub(super) recommendation: &'static str,
}

/// Review-only rule applied to directories that match no artifact rule but
/// are large enough to matter. The empty cleanup tier keeps the persisted
/// recommendation score at zero and (via `apply_cleanup_guardrails`) blocks
/// every cleanup lane: these items are informational, never actionable.
pub(super) const LARGE_DIRECTORY_RULE: ArtifactRule = ArtifactRule {
    kind: "large-directory",
    safety: "review",
    cleanup_tier: "",
    reason: "Informational: large directory that matches no known artifact rule.",
    recommendation: "Large directory outside known artifact rules — review what lives here.",
};

#[derive(Clone, Debug)]
pub(super) struct ArtifactIntelligence {
    pub(super) rebuild_command: Option<String>,
    pub(super) estimated_rebuild_cost: String,
    pub(super) estimated_rebuild_seconds: Option<u64>,
    pub(super) cleanup_consequence: String,
}

pub(super) fn apply_cleanup_guardrails(items: &mut [StorageHygieneItem], now_millis: u64) {
    for item in items {
        if item.cleanup_tier.is_empty() {
            block_cleanup(
                item,
                "Unclassified item has no cleanup tier; it is review-only and never cleanable.",
            );
        }
        if item.kind == "trash" {
            block_cleanup(item, "Finder owns the Trash; empty it from Finder instead.");
        }
        if item.kind == "docker-vm" {
            block_cleanup(
                item,
                "Docker owns this VM storage; reclaim through `docker system prune`, never by deleting files.",
            );
        }
        if is_protected_cleanup_path(&item.path) {
            item.cleanup_tier = "risky".to_owned();
            item.safety = "review".to_owned();
            block_cleanup(item, "Protected system/application path.");
        }
        if item.cleanup_tier == "risky" {
            block_cleanup(
                item,
                "Risky tier requires manual review and is never unattended.",
            );
        }
        if matches!(
            item.git_status.as_str(),
            "tracked" | "modified" | "deleted" | "renamed" | "conflicted"
        ) {
            item.cleanup_tier = "risky".to_owned();
            item.safety = "review".to_owned();
            block_cleanup(
                item,
                "Path is tracked or actively changed in Git; source work is protected.",
            );
        }
        if item.git_status == "untracked" && is_source_like_storage_item(item) {
            item.cleanup_tier = "risky".to_owned();
            item.safety = "review".to_owned();
            block_cleanup(
                item,
                "Untracked source-like file is protected from automatic cleanup.",
            );
        }
        if let Some(modified_millis) = item.modified_millis
            && now_millis.saturating_sub(modified_millis) <= RECENT_CLEANUP_BLOCK_MILLIS
        {
            block_cleanup(
                item,
                "Recently modified path may still be active; wait or review manually.",
            );
        }
        if item.size_truncated {
            block_cleanup(item, "Size estimate is partial; confirm before cleanup.");
        }
        if !item.cleanup_allowed {
            item.default_cleanup_action = "manual_review".to_owned();
        }
    }
}

pub(super) fn block_cleanup(item: &mut StorageHygieneItem, reason: &str) {
    item.cleanup_allowed = false;
    if !item
        .cleanup_blockers
        .iter()
        .any(|existing| existing == reason)
    {
        item.cleanup_blockers.push(reason.to_owned());
    }
}

fn is_source_like_storage_item(item: &StorageHygieneItem) -> bool {
    if matches!(
        item.storage_role.as_str(),
        "cache" | "build-artifact" | "dependency-tree" | "environment" | "temporary" | "log"
    ) {
        return false;
    }
    let extension = Path::new(&item.path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "swift"
            | "rs"
            | "go"
            | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "m"
            | "mm"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "java"
            | "kt"
            | "rb"
            | "php"
            | "cs"
            | "sql"
            | "yaml"
            | "yml"
            | "json"
            | "toml"
            | "md"
    ) || matches!(
        item.kind.as_str(),
        "large-file"
            | "cold-file"
            | "release-artifact"
            | "macos-app-bundle"
            | "app-support-data"
            | "app-container"
            | "app-launch-item"
            | "ios-backup"
            | "mail-attachments"
            | "message-attachments"
            | "xcode-archives"
            | "build-output"
            | "next-build"
    )
}

pub(super) fn is_protected_cleanup_path(path: &str) -> bool {
    let protected_roots = [
        "/System",
        "/bin",
        "/sbin",
        "/usr/bin",
        "/usr/lib",
        "/usr/sbin",
        "/etc",
        "/private/etc",
        "/var/db",
        "/Library/Apple",
        "/Applications",
    ];
    protected_roots.iter().any(|root| {
        path == *root
            || path
                .strip_prefix(root)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

pub(super) fn storage_role_for_kind(kind: &str) -> &'static str {
    match kind {
        "log-file" | "logs" => "log",
        "rust-build" | "swift-build" | "xcode-derived-data" | "coverage-output"
        | "build-output" | "next-build" | "release-artifact" | "xcode-archives" | "test-output" => {
            "build-artifact"
        }
        "xcode-module-cache"
        | "python-cache"
        | "frontend-cache"
        | "next-cache"
        | "tool-cache"
        | "npm-cache"
        | "pnpm-store"
        | "yarn-cache"
        | "app-cache"
        | "homebrew-cache"
        | "pip-cache"
        | "uv-cache"
        | "gradle-cache"
        | "go-build-cache"
        | "simulator-cache"
        | "xcode-device-support"
        | "browser-cache" => "cache",
        "node-dependencies" | "xcode-source-packages" | "maven-repository" => "dependency-tree",
        "python-environment" | "docker-storage" | "docker-vm" | "xcode-simulator-runtime" => {
            "environment"
        }
        "trash" => "temporary",
        "large-directory" => "artifact",
        "macos-app-bundle" => "application",
        "app-support-data" | "app-container" | "app-launch-item" | "app-preferences"
        | "app-receipt" => "app-data",
        "ios-backup" | "mail-attachments" | "message-attachments" | "local-snapshot" => {
            "system-data"
        }
        "temporary-output" => "temporary",
        "cold-file" => "cold-file",
        "large-file" => "large-file",
        _ => "artifact",
    }
}

pub(super) fn storage_role_label(role: &str) -> String {
    role.replace('-', " ")
}

pub(super) fn git_status_label(status: &str) -> &'static str {
    match status {
        "tracked" => "tracked by git",
        "modified" => "modified in git",
        "deleted" => "deleted in git",
        "renamed" => "renamed in git",
        "conflicted" => "conflicted in git",
        "ignored" => "ignored by git",
        "generated-or-cache" => "generated/cache pattern",
        "untracked" => "untracked by git",
        "outside-git" => "outside a git repository",
        "repo-linked-unchecked" => "repo-linked, not checked yet",
        _ => "unknown",
    }
}

pub(super) fn cleanup_tier_label(tier: &str) -> &'static str {
    match tier {
        "safe" => "safe",
        "rebuildable" => "rebuildable",
        "expensive" => "expensive",
        "risky" => "risky",
        _ => "unknown",
    }
}

pub(super) fn artifact_intelligence(kind: &str, path: &str) -> ArtifactIntelligence {
    let quoted_path = shell_quote(path);
    match kind {
        "rust-build" => ArtifactIntelligence {
            rebuild_command: Some("cargo build".to_owned()),
            estimated_rebuild_cost: "Low to medium".to_owned(),
            estimated_rebuild_seconds: Some(420),
            cleanup_consequence:
                "Cargo will rebuild target artifacts; first build after cleanup may recompile dependencies."
                    .to_owned(),
        },
        "swift-build" => ArtifactIntelligence {
            rebuild_command: Some("swift build".to_owned()),
            estimated_rebuild_cost: "Low to medium".to_owned(),
            estimated_rebuild_seconds: Some(420),
            cleanup_consequence:
                "SwiftPM will recreate .build; package resolution and compilation may take several minutes."
                    .to_owned(),
        },
        "xcode-derived-data" | "xcode-module-cache" => ArtifactIntelligence {
            rebuild_command: Some("xcodebuild build".to_owned()),
            estimated_rebuild_cost: "Medium".to_owned(),
            estimated_rebuild_seconds: Some(600),
            cleanup_consequence:
                "Xcode will regenerate indexes, modules, and build intermediates on the next build."
                    .to_owned(),
        },
        "xcode-source-packages" => ArtifactIntelligence {
            rebuild_command: Some("xcodebuild -resolvePackageDependencies".to_owned()),
            estimated_rebuild_cost: "Medium to high".to_owned(),
            estimated_rebuild_seconds: Some(900),
            cleanup_consequence:
                "Xcode can restore package checkouts, but this may require network access and package resolution."
                    .to_owned(),
        },
        "xcode-simulator-runtime" => ArtifactIntelligence {
            rebuild_command: Some("xcrun simctl list".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(1_800),
            cleanup_consequence:
                "Simulator data may include installed apps, device state, and runtimes; use Xcode/Simulator tools for cleanup."
                    .to_owned(),
        },
        "xcode-archives" | "release-artifact" => ArtifactIntelligence {
            rebuild_command: Some("scripts/release.sh".to_owned()),
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Archives and release packages may be the only signed/notarized copy; confirm supersession before cleanup."
                    .to_owned(),
        },
        "node-dependencies" => ArtifactIntelligence {
            rebuild_command: Some("npm install / pnpm install / yarn install".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(900),
            cleanup_consequence:
                "Dependencies can be reinstalled from lockfiles, but cleanup can cost network time and break offline work."
                    .to_owned(),
        },
        "npm-cache" | "pnpm-store" | "yarn-cache" => ArtifactIntelligence {
            rebuild_command: Some("package manager install".to_owned()),
            estimated_rebuild_cost: "Medium to high".to_owned(),
            estimated_rebuild_seconds: Some(1_200),
            cleanup_consequence:
                "Package caches are refetchable but expensive; cleanup may slow future installs and require network access."
                    .to_owned(),
        },
        "next-cache" | "next-build" | "frontend-cache" => ArtifactIntelligence {
            rebuild_command: Some("npm run build".to_owned()),
            estimated_rebuild_cost: "Medium".to_owned(),
            estimated_rebuild_seconds: Some(480),
            cleanup_consequence:
                "Frontend build caches are rebuildable; the next dev/build run may be noticeably slower."
                    .to_owned(),
        },
        "python-cache" => ArtifactIntelligence {
            rebuild_command: Some("pytest / python import".to_owned()),
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(120),
            cleanup_consequence:
                "Python tools will recreate caches; first test/lint/import pass may be slower."
                    .to_owned(),
        },
        "python-environment" => ArtifactIntelligence {
            rebuild_command: Some("python -m venv .venv && pip install -r requirements.txt".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(1_200),
            cleanup_consequence:
                "Virtual environments are rebuildable from manifests but can be slow and environment-specific."
                    .to_owned(),
        },
        "docker-storage" => ArtifactIntelligence {
            rebuild_command: Some("docker system df && docker builder prune".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(1_800),
            cleanup_consequence:
                "Docker layers are reclaimable through Docker, but images/build caches may need to be pulled or rebuilt."
                    .to_owned(),
        },
        "test-output" | "coverage-output" => ArtifactIntelligence {
            rebuild_command: Some("test command / coverage command".to_owned()),
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(180),
            cleanup_consequence:
                "Reports are usually disposable after review; rerun tests to regenerate them."
                    .to_owned(),
        },
        "logs" | "log-file" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "None".to_owned(),
            estimated_rebuild_seconds: Some(0),
            cleanup_consequence:
                "Logs are not rebuildable; keep a copy first if they are needed for debugging or support."
                    .to_owned(),
        },
        "app-cache" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(60),
            cleanup_consequence:
                "The owning app rebuilds its cache automatically; the app may relaunch slower once."
                    .to_owned(),
        },
        "homebrew-cache" => ArtifactIntelligence {
            rebuild_command: Some("brew cleanup".to_owned()),
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(60),
            cleanup_consequence:
                "Homebrew re-downloads bottles on demand; `brew cleanup` is the supported way to prune this cache."
                    .to_owned(),
        },
        "pip-cache" => ArtifactIntelligence {
            rebuild_command: Some("pip install".to_owned()),
            estimated_rebuild_cost: "Low to medium".to_owned(),
            estimated_rebuild_seconds: Some(300),
            cleanup_consequence:
                "pip repopulates its wheel cache on the next install; future installs may need network access."
                    .to_owned(),
        },
        "uv-cache" => ArtifactIntelligence {
            rebuild_command: Some("uv sync".to_owned()),
            estimated_rebuild_cost: "Low to medium".to_owned(),
            estimated_rebuild_seconds: Some(300),
            cleanup_consequence:
                "uv repopulates its cache on the next sync/install; future installs may need network access."
                    .to_owned(),
        },
        "gradle-cache" => ArtifactIntelligence {
            rebuild_command: Some("gradle build".to_owned()),
            estimated_rebuild_cost: "Medium".to_owned(),
            estimated_rebuild_seconds: Some(600),
            cleanup_consequence:
                "Gradle re-downloads dependencies and rebuilds caches on the next build; expect a slower first build."
                    .to_owned(),
        },
        "maven-repository" => ArtifactIntelligence {
            rebuild_command: Some("mvn dependency:resolve".to_owned()),
            estimated_rebuild_cost: "Medium to high".to_owned(),
            estimated_rebuild_seconds: Some(900),
            cleanup_consequence:
                "Maven refetches the local repository from remotes, but locally installed artifacts are lost."
                    .to_owned(),
        },
        "go-build-cache" => ArtifactIntelligence {
            rebuild_command: Some("go clean -cache".to_owned()),
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(180),
            cleanup_consequence:
                "Go rebuilds its build cache on the next compile; `go clean -cache` is the supported cleanup command."
                    .to_owned(),
        },
        "simulator-cache" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Low".to_owned(),
            estimated_rebuild_seconds: Some(120),
            cleanup_consequence:
                "CoreSimulator caches are recreated on demand; the next simulator boot may be slower."
                    .to_owned(),
        },
        "xcode-device-support" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Medium".to_owned(),
            estimated_rebuild_seconds: Some(600),
            cleanup_consequence:
                "Xcode regenerates device support symbols the next time the matching device is connected."
                    .to_owned(),
        },
        "browser-cache" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Browser cache folders can sit next to session and profile state; clear caches from inside the browser instead of deleting folders."
                    .to_owned(),
        },
        "trash" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "None".to_owned(),
            estimated_rebuild_seconds: Some(0),
            cleanup_consequence:
                "Items in the Trash are already staged for deletion; empty the Trash from Finder to reclaim the space."
                    .to_owned(),
        },
        "docker-vm" => ArtifactIntelligence {
            rebuild_command: Some("docker system prune".to_owned()),
            estimated_rebuild_cost: "High".to_owned(),
            estimated_rebuild_seconds: Some(1_800),
            cleanup_consequence:
                "Docker VM disk images hold images, containers, and volumes; reclaim only through `docker system prune`, never by deleting files."
                    .to_owned(),
        },
        "large-directory" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Informational only: no artifact rule matched, so contents may be personal data or project files. Review manually."
                    .to_owned(),
        },
        "macos-app-bundle" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Removing an app bundle is an uninstall action; review related support data, containers, launch items, and receipts first."
                    .to_owned(),
        },
        "app-support-data" | "app-container" | "app-launch-item" | "app-preferences"
        | "app-receipt" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "App data can contain settings, receipts, databases, launch configuration, or user state; clean it only as part of an intentional uninstall."
                    .to_owned(),
        },
        "ios-backup" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Device backups may be the only local recovery copy; use Finder device backup management or confirm another backup exists."
                    .to_owned(),
        },
        "mail-attachments" | "message-attachments" => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Review first".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Attachments can be personal records; review inside the owning app or export what matters before cleanup."
                    .to_owned(),
        },
        "local-snapshot" => ArtifactIntelligence {
            rebuild_command: Some("tmutil listlocalsnapshots /".to_owned()),
            estimated_rebuild_cost: "System managed".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Local snapshots are managed by macOS; prefer Time Machine controls or tmutil over deleting files directly."
                    .to_owned(),
        },
        "temporary-output" | "tool-cache" => ArtifactIntelligence {
            rebuild_command: Some(format!("du -sh {quoted_path}")),
            estimated_rebuild_cost: "Low to medium".to_owned(),
            estimated_rebuild_seconds: Some(300),
            cleanup_consequence:
                "Tool caches and temp outputs are usually rebuildable, but active jobs may still be writing them."
                    .to_owned(),
        },
        _ => ArtifactIntelligence {
            rebuild_command: None,
            estimated_rebuild_cost: "Unknown".to_owned(),
            estimated_rebuild_seconds: None,
            cleanup_consequence:
                "Aetower cannot prove rebuildability; inspect the path and classification evidence first."
                    .to_owned(),
        },
    }
}

fn is_release_artifact_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".pkg")
        || lower.ends_with(".dmg")
        || lower.ends_with(".zip")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tar")
        || lower.ends_with(".tgz")
        || lower.ends_with(".xcarchive")
        || lower.ends_with(".xcresult")
}

fn is_docker_storage_path(path_lower: &str, name: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    let docker_root = path_lower.contains("/.docker/")
        || path_lower.contains("/docker/")
        || path_lower.contains("/com.docker.docker/");
    docker_root
        && matches!(
            name_lower.as_str(),
            "overlay2"
                | "buildkit"
                | "containers"
                | "image"
                | "volumes"
                | "vfs"
                | "aufs"
                | "btrfs"
                | "zfs"
        )
}

fn is_simulator_storage_path(path_lower: &str, name: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    (path_lower.contains("/developer/coresimulator/")
        || path_lower.contains("/coredevice/")
        || path_lower.contains("/developer/xcode/ios devicesupport/")
        || path_lower.contains("/developer/xcode/watchos devicesupport/")
        || path_lower.contains("/developer/xcode/tvos devicesupport/"))
        && matches!(
            name_lower.as_str(),
            "devices" | "runtimes" | "data" | "device support" | "ios devicesupport"
        )
}

fn is_package_cache_path(path_lower: &str, name: &str, parent_name: &str) -> Option<&'static str> {
    let name_lower = name.to_ascii_lowercase();
    let parent_lower = parent_name.to_ascii_lowercase();
    if name_lower == ".npm" || path_lower.contains("/.npm/_cacache") {
        return Some("npm-cache");
    }
    if name_lower == ".pnpm-store"
        || (name_lower == "store" && (path_lower.contains("/pnpm/") || parent_lower == "pnpm"))
        || path_lower.contains("/.local/share/pnpm/store")
    {
        return Some("pnpm-store");
    }
    if (name_lower == "cache" && parent_lower == ".yarn")
        || path_lower.contains("/.yarn/cache")
        || path_lower.contains("/yarn/cache")
    {
        return Some("yarn-cache");
    }
    None
}

fn is_library_child(path_lower: &str, folder: &str) -> bool {
    path_lower.contains(&format!("/library/{folder}/"))
}

pub(super) fn is_app_support_path(path_lower: &str) -> bool {
    is_library_child(path_lower, "application support")
}

pub(super) fn is_app_cache_path(path_lower: &str) -> bool {
    is_library_child(path_lower, "caches")
}

pub(super) fn is_app_container_path(path_lower: &str) -> bool {
    is_library_child(path_lower, "containers") || is_library_child(path_lower, "group containers")
}

pub(super) fn is_launch_item_path(path_lower: &str) -> bool {
    is_library_child(path_lower, "launchagents") || path_lower.contains("/library/launchdaemons/")
}

pub(super) fn is_app_preferences_path(path_lower: &str, name: &str) -> bool {
    is_library_child(path_lower, "preferences") && name.to_ascii_lowercase().ends_with(".plist")
}

pub(super) fn is_app_receipt_path(path_lower: &str, name: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    (path_lower.contains("/var/db/receipts/") || path_lower.contains("/library/receipts/"))
        && (name_lower.ends_with(".plist")
            || name_lower.ends_with(".bom")
            || name_lower.ends_with(".pkg")
            || name_lower.ends_with(".pkg/"))
}

fn is_browser_cache_path(path_lower: &str) -> bool {
    let browser_owned = [
        "com.google.chrome",
        "com.apple.safari",
        "org.mozilla.firefox",
        "company.thebrowser.browser",
        "/google/chrome/",
        "/firefox/profiles/",
        "/application support/arc/",
    ]
    .iter()
    .any(|marker| path_lower.contains(marker));
    browser_owned
        && (is_app_cache_path(path_lower)
            || (is_app_support_path(path_lower) && path_lower.contains("cache")))
}

fn is_docker_vm_path(path_lower: &str) -> bool {
    path_lower.contains("/com.docker.docker/data/vms")
}

fn is_trash_directory(path_lower: &str, name: &str) -> bool {
    name == ".Trash" || (path_lower.contains("/.trashes/") && name.chars().all(char::is_numeric))
}

fn is_ios_backup_path(path_lower: &str) -> bool {
    path_lower.contains("/library/application support/mobilesync/backup/")
}

fn is_mail_attachment_path(path_lower: &str) -> bool {
    path_lower.contains("/library/mail/") && path_lower.contains("/attachments/")
}

fn is_message_attachment_path(path_lower: &str) -> bool {
    path_lower.contains("/library/messages/attachments/")
}

fn is_snapshot_like_path(path_lower: &str) -> bool {
    path_lower.contains("/.mobilebackups/")
        || path_lower.contains("/com.apple.timemachine.localsnapshots/")
        || path_lower.contains("/.timemachine/")
}

pub(super) fn classify_artifact(
    path: &Path,
    metadata: &fs::Metadata,
    now_millis: u64,
) -> Option<ArtifactRule> {
    let name = path.file_name()?.to_str()?;
    let parent_name = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let path_display = path.display().to_string();
    let path_lower = path_display.to_ascii_lowercase();

    if metadata.is_file() && name.ends_with(".log") {
        return Some(rule(
            "log-file",
            "safe",
            "safe",
            "Development log file.",
            "Safe to review and rotate when no current task depends on it.",
        ));
    }
    if metadata.is_file() && is_app_preferences_path(&path_lower, name) {
        return Some(rule(
            "app-preferences",
            "review",
            "risky",
            "Application preference plist.",
            "Review only as part of an app uninstall; preferences can hold settings, license state, or account hints.",
        ));
    }
    if metadata.is_file() && is_app_receipt_path(&path_lower, name) {
        return Some(rule(
            "app-receipt",
            "review",
            "risky",
            "Application install receipt.",
            "Review only as part of an app uninstall; receipts help macOS and installers understand what was installed.",
        ));
    }
    if metadata.is_file() && is_launch_item_path(&path_lower) {
        return Some(rule(
            "app-launch-item",
            "review",
            "risky",
            "App launch agent or daemon.",
            "Review launch items alongside their owning app before disabling or deleting anything.",
        ));
    }
    if metadata.is_file() && is_release_artifact_file(name) {
        return Some(rule(
            "release-artifact",
            "review",
            "risky",
            "Potential local release artifact.",
            "Review provenance and whether a newer signed/notarized artifact supersedes it before deleting.",
        ));
    }
    if metadata.is_file()
        && metadata.len() >= MIN_ITEM_BYTES
        && file_access_age_days(metadata, now_millis).is_some_and(|days| days >= COLD_AFTER_DAYS)
    {
        return Some(rule(
            "cold-file",
            "review",
            "risky",
            "File not accessed for more than a year.",
            "Reveal and inspect ownership before deleting; access time is useful evidence but not proof that the file is disposable.",
        ));
    }
    if metadata.is_file() && metadata.len() >= LARGE_FILE_BYTES {
        return Some(rule(
            "large-file",
            "review",
            "risky",
            "Large standalone file.",
            "Reveal and inspect ownership before deleting; Aetower cannot prove whether this is source data, a local export, or a generated artifact.",
        ));
    }

    if !metadata.is_dir() {
        return None;
    }

    if name.to_ascii_lowercase().ends_with(".app") {
        return Some(rule(
            "macos-app-bundle",
            "review",
            "risky",
            "macOS application bundle.",
            "Review app ownership and related support data before uninstalling or moving to Trash.",
        ));
    }

    if name.to_ascii_lowercase().ends_with(".xcarchive") {
        return Some(rule(
            "xcode-archives",
            "review",
            "risky",
            "Xcode archive or release build product.",
            "Review before deleting because archives may be the only signed or notarized release copy.",
        ));
    }

    if name.to_ascii_lowercase().ends_with(".xcresult") {
        return Some(rule(
            "test-output",
            "safe",
            "safe",
            "Xcode result bundle.",
            "Safe to remove after test review or export; rerun tests to regenerate.",
        ));
    }

    if is_trash_directory(&path_lower, name) {
        return Some(rule(
            "trash",
            "review",
            "safe",
            "macOS Trash contents already staged for deletion.",
            "Empty the Trash from Finder to reclaim this space; Finder owns Trash lifecycle.",
        ));
    }

    if is_docker_vm_path(&path_lower) {
        return Some(rule(
            "docker-vm",
            "review",
            "expensive",
            "Docker Desktop VM disk storage (images, containers, volumes).",
            "Reclaim through Docker (`docker system prune`) after reviewing `docker system df`; never delete VM files directly.",
        ));
    }

    if is_docker_storage_path(&path_lower, name) {
        return Some(rule(
            "docker-storage",
            "review",
            "expensive",
            "Docker image, layer, build cache, container, or volume storage.",
            "Use Docker cleanup tools when containers are idle; refetching images and rebuilding layers can be expensive.",
        ));
    }

    if path_lower.contains("/developer/coresimulator/caches") {
        return Some(rule(
            "simulator-cache",
            "safe",
            "safe",
            "CoreSimulator cache storage.",
            "Usually safe to remove when Simulator and Xcode are quit; CoreSimulator recreates caches on demand.",
        ));
    }

    if path_lower.contains(" devicesupport/") {
        return Some(rule(
            "xcode-device-support",
            "review",
            "rebuildable",
            "Xcode device support symbols for a connected device version.",
            "Removable when the device/OS version is no longer used; Xcode regenerates symbols on the next device connection.",
        ));
    }

    if parent_name == "DerivedData" && !name.ends_with(".noindex") {
        return Some(rule(
            "xcode-derived-data",
            "safe",
            "safe",
            "Per-project Xcode DerivedData entry.",
            "Safe to remove when Xcode builds are idle; xcodebuild re-derives indexes and intermediates.",
        ));
    }

    if is_simulator_storage_path(&path_lower, name) {
        return Some(rule(
            "xcode-simulator-runtime",
            "review",
            "expensive",
            "Xcode simulator/device support storage.",
            "Review in Xcode or Simulator tooling; deleting can remove installed simulator apps, runtimes, or device support.",
        ));
    }

    if is_package_cache_path(&path_lower, name, parent_name) == Some("npm-cache") {
        return Some(rule(
            "npm-cache",
            "review",
            "expensive",
            "npm package cache.",
            "Review before deleting; package caches are refetchable but can make future installs slower.",
        ));
    }
    if is_package_cache_path(&path_lower, name, parent_name) == Some("pnpm-store") {
        return Some(rule(
            "pnpm-store",
            "review",
            "expensive",
            "pnpm content-addressable store.",
            "Review before deleting; pnpm can repopulate the store but future installs may require network access.",
        ));
    }
    if is_package_cache_path(&path_lower, name, parent_name) == Some("yarn-cache") {
        return Some(rule(
            "yarn-cache",
            "review",
            "expensive",
            "Yarn package cache.",
            "Review before deleting; Yarn caches are refetchable but can be intentionally checked in for offline installs.",
        ));
    }

    if is_ios_backup_path(&path_lower) {
        return Some(rule(
            "ios-backup",
            "review",
            "risky",
            "iPhone or iPad local backup.",
            "Use Finder device management or a deliberate backup policy before deleting local device backups.",
        ));
    }
    if is_mail_attachment_path(&path_lower) {
        return Some(rule(
            "mail-attachments",
            "review",
            "risky",
            "Mail attachment storage.",
            "Review in Mail or Finder before deleting; attachments can be personal or business records.",
        ));
    }
    if is_message_attachment_path(&path_lower) {
        return Some(rule(
            "message-attachments",
            "review",
            "risky",
            "Messages attachment storage.",
            "Review in Messages or Finder before deleting; attachments can be personal records.",
        ));
    }
    if is_snapshot_like_path(&path_lower) {
        return Some(rule(
            "local-snapshot",
            "review",
            "expensive",
            "Local snapshot-like storage.",
            "Prefer macOS Time Machine/snapshot tooling instead of deleting snapshot folders manually.",
        ));
    }
    if is_app_container_path(&path_lower) {
        return Some(rule(
            "app-container",
            "review",
            "risky",
            "App sandbox container or group container.",
            "Review as part of an app footprint; containers can hold user data and app state.",
        ));
    }
    if is_launch_item_path(&path_lower) {
        return Some(rule(
            "app-launch-item",
            "review",
            "risky",
            "App launch agent or daemon area.",
            "Review launch items alongside their owning app before disabling or deleting anything.",
        ));
    }
    if is_app_receipt_path(&path_lower, name) {
        return Some(rule(
            "app-receipt",
            "review",
            "risky",
            "Application install receipt.",
            "Review only as part of an app uninstall; receipts help macOS and installers understand what was installed.",
        ));
    }
    if is_app_support_path(&path_lower) && !path_lower.contains("/mobilesync") {
        return Some(rule(
            "app-support-data",
            "review",
            "risky",
            "Application Support data.",
            "Review as part of an app footprint; support data can include databases, settings, and user content.",
        ));
    }
    if is_browser_cache_path(&path_lower) {
        return Some(rule(
            "browser-cache",
            "review",
            "risky",
            "Browser cache storage that can sit next to session/profile state.",
            "Review only; clear caches from inside the browser rather than deleting profile-adjacent folders.",
        ));
    }
    if is_app_cache_path(&path_lower) && name.eq_ignore_ascii_case("Homebrew") {
        return Some(rule(
            "homebrew-cache",
            "safe",
            "safe",
            "Homebrew download and bottle cache.",
            "Safe to prune with `brew cleanup`; Homebrew re-downloads bottles on demand.",
        ));
    }
    if is_app_cache_path(&path_lower) && name.eq_ignore_ascii_case("pip") {
        return Some(rule(
            "pip-cache",
            "safe",
            "rebuildable",
            "pip wheel and HTTP cache.",
            "Usually safe to remove when no install is running; pip repopulates the cache on the next install.",
        ));
    }
    if is_app_cache_path(&path_lower) && name.eq_ignore_ascii_case("go-build") {
        return Some(rule(
            "go-build-cache",
            "safe",
            "safe",
            "Go build cache.",
            "Safe to clear with `go clean -cache`; Go rebuilds the cache on the next compile.",
        ));
    }
    if is_app_cache_path(&path_lower) {
        return Some(rule(
            "app-cache",
            "safe",
            "rebuildable",
            "Application cache data.",
            "Apps rebuild their caches automatically; remove via Trash when the owning app is quit, expecting one slower relaunch.",
        ));
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
        ".aethyme" | ".aetower-cache" | ".aeptus-cache" | "org.swift.swiftpm"
        | "com.apple.dt.Xcode" => Some(rule(
            "tool-cache",
            "safe",
            "rebuildable",
            "Developer tool cache.",
            "Usually safe to remove when the related toolchain is idle; tools will recreate it.",
        )),
        "coverage" => Some(rule(
            "coverage-output",
            "safe",
            "safe",
            "Test coverage output.",
            "Safe to remove if you do not need the local coverage report.",
        )),
        ".nyc_output" | "playwright-report" | "test-results" | "junit" | "reports" => Some(rule(
            "test-output",
            "safe",
            "safe",
            "Local test output.",
            "Safe to remove after test results are reviewed or exported.",
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
        "Archives" if path_lower.contains("/developer/xcode/archives") => Some(rule(
            "xcode-archives",
            "review",
            "risky",
            "Xcode archive folder.",
            "Review before deleting because archives can contain signed distribution builds.",
        )),
        "cache" if parent_name == ".next" => Some(rule(
            "next-cache",
            "safe",
            "rebuildable",
            "Next.js build cache.",
            "Usually safe to remove when frontend builds/dev servers are idle.",
        )),
        "caches" if parent_name == ".gradle" => Some(rule(
            "gradle-cache",
            "safe",
            "rebuildable",
            "Gradle dependency and build cache.",
            "Usually safe to remove when Gradle builds are idle; Gradle re-downloads dependencies on the next build.",
        )),
        "repository" if parent_name == ".m2" => Some(rule(
            "maven-repository",
            "review",
            "rebuildable",
            "Maven local artifact repository.",
            "Review before deleting; Maven refetches remote artifacts but locally installed ones would be lost.",
        )),
        "uv" if parent_name == ".cache" => Some(rule(
            "uv-cache",
            "safe",
            "rebuildable",
            "uv package cache.",
            "Usually safe to remove when no uv install is running; uv repopulates the cache on the next sync.",
        )),
        _ if parent_name == ".cache" => Some(rule(
            "tool-cache",
            "safe",
            "rebuildable",
            "Per-tool cache directory under ~/.cache.",
            "Usually safe to remove when the owning tool is idle; XDG-style caches are recreated on demand.",
        )),
        _ => None,
    }
}

pub(super) fn cleanup_tier_rank(tier: &str) -> u8 {
    match tier {
        "safe" => 0,
        "rebuildable" => 1,
        "expensive" => 2,
        "risky" => 3,
        _ => 4,
    }
}

pub(super) fn summarize_cleanup_tiers(
    items: &[StorageHygieneItem],
) -> Vec<StorageCleanupTierSummary> {
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

pub(super) fn artifact_attribution(path: &Path) -> StorageArtifactAttribution {
    let repo_root = find_git_root(path);
    let git_head = repo_root
        .as_ref()
        .map(|root| read_git_head(root))
        .unwrap_or_default();
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
    let confidence = if repo_root.is_some() && git_head.branch.is_some() {
        "high"
    } else if repo_root.is_some() || inferred_agent_session.is_some() {
        "medium"
    } else {
        "low"
    };

    StorageArtifactAttribution {
        repo_root: repo_root.map(|root| root.display().to_string()),
        repo_name,
        git_branch: git_head.branch,
        git_head: git_head.short_head,
        command: None,
        process_tree: None,
        ai_agent_session: inferred_agent_session,
        // Writer identity fields (provider/session/tab/display) are only
        // populated from the writer ledger on the growth-attribution path;
        // path-derived item attribution has no ledger evidence.
        provider: None,
        session_id: None,
        tab_name: None,
        chau7_session_id: None,
        writer_display: None,
        confidence: confidence.to_owned(),
        notes,
    }
}

pub(super) fn build_cleanup_recipes(items: &[StorageHygieneItem]) -> Vec<StorageCleanupRecipe> {
    let mut recipes = BTreeMap::<String, StorageCleanupRecipe>::new();
    for item in items {
        if !cleanup_item_is_trash_actionable(item) {
            continue;
        }
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
    if !cleanup_item_is_trash_actionable(item) {
        return Vec::new();
    }
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
        "npm-cache" | "pnpm-store" | "yarn-cache" => vec![direct_reclaim_recipe(
            item,
            "package-cache",
            "Clear package-manager cache",
            "Package-manager caches are reclaimable but can make future installs slower or require network access.",
            vec![
                "Confirm no package install is running.".to_owned(),
                "Keep project lockfiles before clearing package-manager caches.".to_owned(),
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
        "app-cache" => vec![direct_reclaim_recipe(
            item,
            "app-cache",
            "Clear app cache",
            "Application caches are usually safe to remove after the owning app is quit.",
            vec![
                "Quit the owning app first.".to_owned(),
                "Keep support data and containers unless you are intentionally uninstalling the app."
                    .to_owned(),
            ],
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
        "test-output" => vec![direct_reclaim_recipe(
            item,
            "tests",
            "Remove local test output",
            "Test reports and result bundles are usually safe after review or export.",
            vec!["Keep a copy first if the report is needed for review, CI triage, or support.".to_owned()],
            false,
        )],
        "docker-storage" => vec![docker_cleanup_recipe(item)],
        "xcode-simulator-runtime" => vec![direct_reclaim_recipe(
            item,
            "xcode-simulator",
            "Review simulator/device support storage",
            "Simulator and device-support storage can be reclaimed through Xcode tools, but may contain local app/device state.",
            vec![
                "Close Simulator and Xcode first.".to_owned(),
                "Prefer Xcode Settings or `xcrun simctl` cleanup over deleting active device folders.".to_owned(),
            ],
            true,
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
        "build-output" | "xcode-archives" => {
            stale_release_artifact_recipe(item).into_iter().collect()
        }
        _ => Vec::new(),
    }
}

fn cleanup_item_is_trash_actionable(item: &StorageHygieneItem) -> bool {
    item.cleanup_allowed
        && item.default_cleanup_action == "trash"
        && item.cleanup_blockers.is_empty()
        && !item.size_truncated
}

pub(super) fn build_cleanup_bundles(items: &[StorageHygieneItem]) -> Vec<StorageCleanupBundle> {
    let mut bundles = Vec::new();
    let safe_candidates: Vec<_> = items
        .iter()
        .filter(|item| {
            cleanup_item_is_trash_actionable(item)
                && item.safety == "safe"
                && matches!(item.cleanup_tier.as_str(), "safe" | "rebuildable")
        })
        .collect();
    if let Some(bundle) = cleanup_bundle_for_items(
        "safe-reclaim",
        "Reclaim safely",
        "High-confidence local artifacts. Review the manifest, then stage approved paths for Finder Trash cleanup.",
        "safe",
        safe_candidates,
    ) {
        bundles.push(bundle);
    }

    let review_candidates: Vec<_> = items
        .iter()
        .filter(|item| {
            cleanup_item_is_trash_actionable(item)
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
        // No embedded size: clients render estimated_reclaimable_bytes with
        // their own locale-aware formatter; a second pre-formatted copy in the
        // title inevitably drifts from it.
        title: title.to_owned(),
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
            "Run verification commands first. In-app cleanup moves approved paths to Finder Trash; command references are manual only."
                .to_owned(),
            "Verify active builds, tests, terminals, and agents are idle before moving cleanup targets."
                .to_owned(),
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
        consequence: item.cleanup_consequence.clone(),
        evidence: item.evidence.clone(),
        cleanup_allowed: item.cleanup_allowed,
        cleanup_blockers: item.cleanup_blockers.clone(),
        default_cleanup_action: item.default_cleanup_action.clone(),
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

pub(super) fn cleanup_item_confidence(item: &StorageHygieneItem) -> u8 {
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

pub(super) fn evaluate_budget_guardrails(
    summary: &StorageHygieneSummary,
    repo_footprints: &[StorageRepoFootprint],
    volume_states: &[StorageVolumeState],
    items: &[StorageHygieneItem],
    growth_deltas: &[StorageGrowthDelta],
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

    for volume in volume_states {
        let pressure_percent = percent_u64(volume.available_bytes, volume.total_bytes);
        if volume.available_bytes < FREE_SPACE_FLOOR_BYTES
            || pressure_percent < VOLUME_PRESSURE_FLOOR_PERCENT
        {
            let severity = if volume.available_bytes < FREE_SPACE_FLOOR_BYTES / 2 {
                "critical"
            } else {
                "warning"
            };
            violations.push(StorageBudgetViolation {
                id: format!("volume-pressure|{}", volume.device_id),
                scope: "volume-pressure".to_owned(),
                severity: severity.to_owned(),
                title: format!("Volume {} is under storage pressure", volume.path),
                detail: format!(
                    "{} available on {}; floor is {} or {}%.",
                    human_bytes(volume.available_bytes),
                    volume.path,
                    human_bytes(FREE_SPACE_FLOOR_BYTES),
                    VOLUME_PRESSURE_FLOOR_PERCENT
                ),
                repo_root: None,
                repo_name: None,
                observed_bytes: volume.available_bytes,
                limit_bytes: FREE_SPACE_FLOOR_BYTES,
                recommendation: "Stage Safe and Rebuildable cleanup first; Aetower will not delete anything automatically unless Safe-tier auto-trash is explicitly enabled.".to_owned(),
            });
        }
    }

    let policies = storage_prevention_policies();
    let prevention_suggestions =
        storage_prevention_suggestions(summary, repo_footprints, items, growth_deltas, &violations);
    let scheduled_scan_recommended = !violations.is_empty() || !prevention_suggestions.is_empty();
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
        free_space_floor_bytes: FREE_SPACE_FLOOR_BYTES,
        volume_pressure_floor_percent: VOLUME_PRESSURE_FLOOR_PERCENT,
        warning_only_by_default: true,
        auto_trash_safe_tier_enabled: false,
        scheduled_scan_recommended,
        scheduled_scan_interval_hours: SCHEDULED_SCAN_INTERVAL_HOURS,
        status,
        violations,
        policies,
        prevention_suggestions,
    }
}

fn storage_prevention_policies() -> Vec<StoragePreventionPolicy> {
    vec![
        StoragePreventionPolicy {
            id: "repo-growth-budget".to_owned(),
            title: "Repository growth budget".to_owned(),
            mode: "warn".to_owned(),
            enabled: true,
            action: "warn".to_owned(),
            tier: "prevention".to_owned(),
            detail: format!(
                "Warn when a repository grows more than {} per day.",
                human_bytes(REPO_GROWTH_BUDGET_BYTES_PER_DAY)
            ),
            next_step: "Inspect the growth timeline, then stage rebuildable artifacts if the build/session is finished.".to_owned(),
        },
        StoragePreventionPolicy {
            id: "dev-artifact-budget".to_owned(),
            title: "Developer artifact budget".to_owned(),
            mode: "warn".to_owned(),
            enabled: true,
            action: "warn".to_owned(),
            tier: "prevention".to_owned(),
            detail: format!(
                "Warn when local developer artifacts exceed {} total or {} in one repo.",
                human_bytes(TOTAL_ARTIFACT_BUDGET_BYTES),
                human_bytes(REPO_ARTIFACT_BUDGET_BYTES)
            ),
            next_step: "Prefer Safe and Rebuildable cleanup bundles before touching package stores or source-like files.".to_owned(),
        },
        StoragePreventionPolicy {
            id: "volume-pressure-floor".to_owned(),
            title: "Free-space floor".to_owned(),
            mode: "warn".to_owned(),
            enabled: true,
            action: "warn".to_owned(),
            tier: "prevention".to_owned(),
            detail: format!(
                "Warn when any scanned volume drops below {} or {}% available.",
                human_bytes(FREE_SPACE_FLOOR_BYTES),
                VOLUME_PRESSURE_FLOOR_PERCENT
            ),
            next_step: "Stage cleanup, then move to Finder Trash only after reviewing the manifest.".to_owned(),
        },
        StoragePreventionPolicy {
            id: "scheduled-scan".to_owned(),
            title: "Optional scheduled scan".to_owned(),
            mode: "opt-in".to_owned(),
            enabled: false,
            action: "suggest".to_owned(),
            tier: "prevention".to_owned(),
            detail: format!(
                "Suggested cadence is every {} hours when storage pressure or repeated growth is detected.",
                SCHEDULED_SCAN_INTERVAL_HOURS
            ),
            next_step: "Enable later from Settings; current public behavior remains manual/warning-only.".to_owned(),
        },
        StoragePreventionPolicy {
            id: "safe-tier-auto-trash".to_owned(),
            title: "Safe-tier auto-trash".to_owned(),
            mode: "explicit-opt-in".to_owned(),
            enabled: false,
            action: "trash-after-approval".to_owned(),
            tier: "safe-only".to_owned(),
            detail: "Disabled by default. If enabled later, it may only target Safe-tier items that already pass Trash guardrails.".to_owned(),
            next_step: "Use Stage cleanup for now; never auto-trash Rebuildable, Expensive, Risky, source-like, tracked, modified, or recent files.".to_owned(),
        },
    ]
}

fn storage_prevention_suggestions(
    summary: &StorageHygieneSummary,
    repo_footprints: &[StorageRepoFootprint],
    items: &[StorageHygieneItem],
    growth_deltas: &[StorageGrowthDelta],
    violations: &[StorageBudgetViolation],
) -> Vec<StoragePreventionSuggestion> {
    let mut suggestions = Vec::new();

    if let Some(bytes) = safe_reclaimable_bytes(items)
        && bytes > 0
    {
        suggestions.push(StoragePreventionSuggestion {
            id: "safe-reclaim-before-pressure".to_owned(),
            trigger: "safe-reclaim".to_owned(),
            title: "Stage Safe reclaim before pressure rises".to_owned(),
            detail: "Safe-tier candidates can be reviewed and moved to Finder Trash without touching source-like work.".to_owned(),
            action_label: "Stage Safe cleanup".to_owned(),
            estimated_reclaimable_bytes: bytes,
            safety: "safe".to_owned(),
            requires_approval: true,
        });
    }

    if summary.total_reclaimable_bytes > TOTAL_ARTIFACT_BUDGET_BYTES / 2 {
        suggestions.push(StoragePreventionSuggestion {
            id: "developer-artifacts-drift".to_owned(),
            trigger: "artifact-budget".to_owned(),
            title: "Set a developer artifact cleanup habit".to_owned(),
            detail: format!(
                "{} of local artifacts are visible now; review rebuildable outputs before package stores.",
                human_bytes(summary.total_reclaimable_bytes)
            ),
            action_label: "Review developer artifacts".to_owned(),
            estimated_reclaimable_bytes: summary.total_reclaimable_bytes,
            safety: "review".to_owned(),
            requires_approval: true,
        });
    }

    if let Some((repo, bytes)) = post_build_cleanup_candidate(repo_footprints, items, growth_deltas)
    {
        suggestions.push(StoragePreventionSuggestion {
            id: format!("post-build-cleanup|{}", repo.repo_root),
            trigger: "post-build".to_owned(),
            title: format!("Post-build cleanup available for {}", repo.repo_name),
            detail: "A recent build/session grew this repo and left rebuildable artifacts. Stage cleanup only after the build and related agents are idle.".to_owned(),
            action_label: "Stage post-build cleanup".to_owned(),
            estimated_reclaimable_bytes: bytes,
            safety: "rebuildable".to_owned(),
            requires_approval: true,
        });
    }

    if !violations.is_empty() {
        suggestions.push(StoragePreventionSuggestion {
            id: "scheduled-scan-recommended".to_owned(),
            trigger: "policy-violation".to_owned(),
            title: "Consider scheduled warning scans".to_owned(),
            detail: format!(
                "{} policy warning{} detected. A daily warning-only scan would catch this earlier.",
                violations.len(),
                if violations.len() == 1 { "" } else { "s" }
            ),
            action_label: "Review scan policy".to_owned(),
            estimated_reclaimable_bytes: 0,
            safety: "non-destructive".to_owned(),
            requires_approval: false,
        });
    }

    suggestions.truncate(6);
    suggestions
}

fn safe_reclaimable_bytes(items: &[StorageHygieneItem]) -> Option<u64> {
    let bytes = items
        .iter()
        .filter(|item| item.cleanup_tier == "safe" && cleanup_item_is_trash_actionable(item))
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    (bytes > 0).then_some(bytes)
}

fn post_build_cleanup_candidate<'a>(
    repo_footprints: &'a [StorageRepoFootprint],
    items: &[StorageHygieneItem],
    growth_deltas: &[StorageGrowthDelta],
) -> Option<(&'a StorageRepoFootprint, u64)> {
    let grown_repo = growth_deltas
        .iter()
        .filter(|delta| delta.delta_bytes > 0)
        .filter(|delta| {
            delta.cleanup_tier == "rebuildable"
                || delta
                    .command
                    .as_deref()
                    .is_some_and(command_looks_like_build)
                || delta
                    .attribution_summary
                    .to_ascii_lowercase()
                    .contains("writer ledger")
        })
        .filter_map(|delta| delta.repo_root.as_deref())
        .find_map(|repo_root| {
            repo_footprints
                .iter()
                .find(|footprint| footprint.repo_root == repo_root)
        })?;
    let bytes = items
        .iter()
        .filter(|item| {
            item.attribution
                .repo_root
                .as_deref()
                .is_some_and(|repo_root| repo_root == grown_repo.repo_root)
                && item.cleanup_tier == "rebuildable"
                && cleanup_item_is_trash_actionable(item)
        })
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));
    (bytes > 0).then_some((grown_repo, bytes))
}

fn command_looks_like_build(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    [
        "build",
        "xcodebuild",
        "cargo",
        "swift",
        "npm",
        "pnpm",
        "yarn",
        "make",
    ]
    .iter()
    .any(|needle| command.contains(needle))
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

fn docker_cleanup_recipe(item: &StorageHygieneItem) -> StorageCleanupRecipe {
    StorageCleanupRecipe {
        id: format!("docker-clean|{}", item.path),
        title: "Review Docker storage cleanup".to_owned(),
        category: "docker".to_owned(),
        safety: item.cleanup_tier.clone(),
        affected_path: item.path.clone(),
        command: "docker system df && docker builder prune".to_owned(),
        estimated_reclaimable_bytes: item.size_bytes,
        reason: "Docker storage should be reclaimed through Docker so active containers, named volumes, and layer metadata stay consistent.".to_owned(),
        prerequisites: vec![
            "Stop containers and builds that are actively using Docker.".to_owned(),
            "Run `docker system df` first to review images, build cache, containers, and volumes.".to_owned(),
            "Use `docker builder prune` or targeted Docker cleanup before deleting files manually.".to_owned(),
        ],
        destructive: true,
        requires_review: true,
    }
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
