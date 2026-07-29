import Foundation

enum StorageReclaimListMode: String, CaseIterable, Identifiable {
    case files
    case folders

    var id: String { rawValue }

    var label: String {
        switch self {
        case .files: return "Files"
        case .folders: return "Folders"
        }
    }
}

enum StorageScanModeSelection: String, CaseIterable, Identifiable {
    case fast = "fast_changed_only"
    case complete = "deep_native"
    case forensic = "forensic_verified"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .fast: return "Quick"
        case .complete: return "Complete"
        case .forensic: return "Forensic"
        }
    }

    var actionTitle: String {
        switch self {
        case .fast: return "Quick scan"
        case .complete: return "Complete scan"
        case .forensic: return "Forensic scan"
        }
    }

    var resultLimit: UInt32 {
        switch self {
        case .fast: return 120
        case .complete, .forensic: return 200
        }
    }

    var rowLimitLabel: String {
        switch self {
        case .fast: return "\(resultLimit) top rows"
        case .complete, .forensic: return "all normal rows"
        }
    }

    var defaultMaxDepth: UInt32 {
        switch self {
        case .fast: return 5
        case .complete, .forensic: return 12
        }
    }

    static func label(for rawMode: String) -> String {
        StorageScanModeSelection(rawValue: rawMode)?.label ?? rawMode
    }
}

enum StorageReclaimActionDecision: Equatable {
    case copyPlan
    case stageOnly
    case moveToTrash

    var title: String {
        switch self {
        case .copyPlan: return "Copy cleanup plan"
        case .stageOnly: return "Stage cleanup"
        case .moveToTrash: return "Move to Trash"
        }
    }

    var systemImage: String {
        switch self {
        case .copyPlan: return "doc.on.doc"
        case .stageOnly: return "tray.and.arrow.down"
        case .moveToTrash: return "trash"
        }
    }
}

enum StorageDataCardActionKind: Equatable {
    case review
    case clean
    case scan

    var title: String {
        switch self {
        case .review: return "Review"
        case .clean: return "Clean"
        case .scan: return "Complete scan"
        }
    }

    var systemImage: String {
        switch self {
        case .review: return "magnifyingglass"
        case .clean: return "sparkles"
        case .scan: return "arrow.triangle.2.circlepath"
        }
    }
}

enum StorageReclaimPrimaryVerb: String, Equatable {
    case free = "Free"
    case review = "Review"
}

enum StorageReclaimPrimarySafety: Equatable {
    case safe
    case review
    case toolCleanup

    var label: String {
        switch self {
        case .safe: return "Safe"
        case .review: return "Review required"
        case .toolCleanup: return "Use tool cleanup"
        }
    }
}

enum StorageReclaimPrimaryKind: String, CaseIterable, Identifiable {
    case xcodeDeviceSupport = "xcode-device-support"
    case dockerBuildCache = "docker-build-cache"
    case buildOutputs = "build-outputs"
    case colimaVM = "colima-vm"
    case codexSessions = "codex-sessions"

    var id: String { rawValue }

    var noun: String {
        switch self {
        case .xcodeDeviceSupport: return "Xcode DeviceSupport"
        case .dockerBuildCache: return "Docker build cache"
        case .buildOutputs: return "build outputs"
        case .colimaVM: return "Colima VM"
        case .codexSessions: return "Codex sessions"
        }
    }

    var detail: String {
        switch self {
        case .xcodeDeviceSupport:
            return "Device OS support folders kept by Xcode for debugging attached devices."
        case .dockerBuildCache:
            return "BuildKit/image-layer cache that should be reclaimed through Docker tooling."
        case .buildOutputs:
            return "Rebuildable target, .build, DerivedData, frontend, test, and coverage outputs."
        case .colimaVM:
            return "Local VM disk backing Docker/Colima containers, images, and volumes."
        case .codexSessions:
            return "Local AI session transcripts, logs, and recovery context."
        }
    }

    var consequence: String {
        switch self {
        case .xcodeDeviceSupport:
            return "Removing old support folders can break debugging for that device/OS until Xcode restores the files."
        case .dockerBuildCache:
            return "Pruning may require Docker images to rebuild and re-download layers; use Docker cleanup commands, not Finder Trash."
        case .buildOutputs:
            return "Next build or test run regenerates these files and may take longer once."
        case .colimaVM:
            return "Do not Trash the VM disk directly; pruning containers, images, or volumes can delete local service data."
        case .codexSessions:
            return "Session files can contain prompts, code, credentials, or recovery context; export anything important first."
        }
    }

    var systemImage: String {
        switch self {
        case .xcodeDeviceSupport: return "iphone.gen3"
        case .dockerBuildCache: return "shippingbox"
        case .buildOutputs: return "hammer"
        case .colimaVM: return "server.rack"
        case .codexSessions: return "terminal"
        }
    }

    var verb: StorageReclaimPrimaryVerb {
        switch self {
        case .colimaVM, .codexSessions:
            return .review
        case .xcodeDeviceSupport, .dockerBuildCache, .buildOutputs:
            return .free
        }
    }

    var safety: StorageReclaimPrimarySafety {
        switch self {
        case .buildOutputs:
            return .safe
        case .dockerBuildCache:
            return .toolCleanup
        case .xcodeDeviceSupport, .colimaVM, .codexSessions:
            return .review
        }
    }

    var priority: Int {
        switch self {
        case .xcodeDeviceSupport: return 0
        case .dockerBuildCache: return 1
        case .buildOutputs: return 2
        case .colimaVM: return 3
        case .codexSessions: return 4
        }
    }

    var command: String? {
        switch self {
        case .dockerBuildCache:
            return "docker system df && docker builder prune"
        case .colimaVM:
            return "colima list && docker system df"
        default:
            return nil
        }
    }

    func matches(_ item: StorageHygieneItemModel) -> Bool {
        let kind = item.kind.lowercased()
        let path = item.path.lowercased()
        let provider = item.attribution.provider?.lowercased()
        let session = item.attribution.aiAgentSession?.lowercased()

        switch self {
        case .xcodeDeviceSupport:
            return kind == "xcode-device-support" || path.contains("devicesupport")
        case .dockerBuildCache:
            return kind == "docker-build-cache"
                || (kind == "docker-storage" && StorageReclaimPrimaryKind.pathLooksLikeDockerBuildCache(path))
        case .buildOutputs:
            return StorageReclaimPrimaryKind.buildOutputKinds.contains(kind)
        case .colimaVM:
            return kind == "colima-vm" || path.contains("/.colima/_lima/")
        case .codexSessions:
            return kind == "codex-session"
                || (kind == "ai-session-data" && (
                    path.contains("/.codex/")
                        || provider == "codex"
                        || session?.contains("codex") == true
                ))
        }
    }

    private static let buildOutputKinds: Set<String> = [
        "build-output",
        "coverage-output",
        "frontend-cache",
        "next-build",
        "next-cache",
        "release-artifact",
        "rust-build",
        "swift-build",
        "temporary-output",
        "test-output",
        "xcode-derived-data",
        "xcode-module-cache",
    ]

    private static func pathLooksLikeDockerBuildCache(_ path: String) -> Bool {
        path.contains("/.docker/buildx/")
            || path.contains("/.docker/buildkit/")
            || path.contains("/.docker/overlay2")
            || path.contains("/docker/build-cache/")
            || path.contains("/buildkit/")
    }
}

struct StorageReclaimPrimaryAction: Identifiable {
    let kind: StorageReclaimPrimaryKind
    let bytes: UInt64
    let itemCount: Int
    let items: [StorageHygieneItemModel]

    var id: String { kind.rawValue }
    var verb: StorageReclaimPrimaryVerb { kind.verb }
    var noun: String { kind.noun }
    var detail: String { kind.detail }
    var consequence: String { kind.consequence }
    var safety: StorageReclaimPrimarySafety { kind.safety }
    var systemImage: String { kind.systemImage }
    var command: String? { kind.command }

    var hasStageableItems: Bool {
        canStageTrash
    }

    var canStageTrash: Bool {
        switch kind {
        case .dockerBuildCache, .colimaVM, .codexSessions:
            return false
        case .xcodeDeviceSupport, .buildOutputs:
            return items.contains(where: StorageReclaimPolicy.itemIsTrashActionable)
        }
    }

    var canMoveToTrash: Bool {
        switch kind {
        case .buildOutputs:
            return !items.isEmpty && items.allSatisfy(StorageReclaimPolicy.itemIsSafeDirectTrash)
        case .xcodeDeviceSupport, .dockerBuildCache, .colimaVM, .codexSessions:
            return false
        }
    }
}

enum StorageReclaimPolicy {
    static func primaryActionDecision(
        hasStageableContent: Bool,
        canMoveToTrash: Bool
    ) -> StorageReclaimActionDecision {
        guard hasStageableContent else { return .copyPlan }
        return canMoveToTrash ? .moveToTrash : .stageOnly
    }

    static func primaryActions(items: [StorageHygieneItemModel]) -> [StorageReclaimPrimaryAction] {
        let sorted = uniqueStorageItems(items).sorted { left, right in
            left.sizeBytes == right.sizeBytes ? left.path < right.path : left.sizeBytes > right.sizeBytes
        }

        return StorageReclaimPrimaryKind.allCases
            .compactMap { kind -> StorageReclaimPrimaryAction? in
                let matches = sorted.filter(kind.matches)
                guard !matches.isEmpty else { return nil }
                return StorageReclaimPrimaryAction(
                    kind: kind,
                    bytes: sumItemBytes(matches),
                    itemCount: matches.count,
                    items: matches
                )
            }
            .sorted { left, right in
                left.kind.priority == right.kind.priority
                    ? left.bytes > right.bytes
                    : left.kind.priority < right.kind.priority
            }
    }

    static func itemIsTrashActionable(_ item: StorageHygieneItemModel) -> Bool {
        item.cleanupAllowed
            && item.defaultCleanupAction == "trash"
            && item.cleanupBlockers.isEmpty
            && item.cleanupTier != "risky"
            && !item.sizeTruncated
            && !item.cloudPlaceholder
            && !item.protectedPath
    }

    static func itemIsSafeDirectTrash(_ item: StorageHygieneItemModel) -> Bool {
        itemIsTrashActionable(item)
            && item.safety == "safe"
            && (item.cleanupTier == "safe" || item.cleanupTier == "rebuildable")
            && !item.hasHardlinks
    }

    private static func sumItemBytes(_ items: [StorageHygieneItemModel]) -> UInt64 {
        items.reduce(UInt64(0)) { total, item in
            let result = total.addingReportingOverflow(item.sizeBytes)
            return result.overflow ? UInt64.max : result.partialValue
        }
    }

    private static func uniqueStorageItems(_ items: [StorageHygieneItemModel]) -> [StorageHygieneItemModel] {
        var seen = Set<String>()
        var unique: [StorageHygieneItemModel] = []
        for item in items where seen.insert(item.path).inserted {
            unique.append(item)
        }
        return unique
    }
}
