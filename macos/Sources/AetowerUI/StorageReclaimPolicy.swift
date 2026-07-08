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
        case .scan: return "Start scan"
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
