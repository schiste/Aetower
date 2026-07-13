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

enum StorageReclaimPolicy {
    static func primaryActionDecision(
        hasStageableContent: Bool,
        canMoveToTrash: Bool
    ) -> StorageReclaimActionDecision {
        guard hasStageableContent else { return .copyPlan }
        return canMoveToTrash ? .moveToTrash : .stageOnly
    }
}
