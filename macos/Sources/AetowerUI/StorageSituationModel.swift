import Foundation

struct StorageSituationModel: Decodable, Sendable {
    let capturedAtMillis: UInt64
    let snapshotUpdatedAtMillis: UInt64?
    let cacheStatus: StorageCacheStatusModel
    let storageIndexStatus: String
    let roots: [String]
    let dirtyPaths: StorageDirtyPathSummaryModel
    let backlogDrain: StorageSituationBacklogDrainModel?
    let recoveryPlan: StorageSituationRecoveryPlanModel?
    let summary: StorageSituationSummaryModel
    let topOffenders: [StorageSituationTopOffenderModel]
    let ownershipBreakdown: StorageOwnershipBreakdownModel?
    var volumeStates: [StorageVolumeStateModel]
    let caveats: [String]

    var hasCachedFacts: Bool {
        summary.itemCount > 0 || summary.inventorySizeBytes > 0 || !topOffenders.isEmpty
    }
}

struct StorageOwnershipBreakdownModel: Decodable, Sendable {
    let usedBytes: UInt64
    let attributedBytes: UInt64
    let unattributedBytes: UInt64
    let reclaimableBytes: UInt64
    let measuredAtMillis: UInt64
    let confidence: String
    let generationId: Int64?
    let classifierVersion: UInt32?
    let generationStatus: String?
    let buckets: [StorageOwnershipBucketModel]
    let repositoryArtifacts: [StorageRepositoryArtifactModel]?

    var isUsable: Bool {
        usedBytes > 0 && !buckets.isEmpty
    }

    var stableBuckets: [StorageOwnershipBucketModel] {
        buckets.sorted { left, right in
            (left.effectiveRank, left.id) < (right.effectiveRank, right.id)
        }
    }

    var stableRepositoryArtifacts: [StorageRepositoryArtifactModel] {
        (repositoryArtifacts ?? []).sorted {
            ($0.physicalBytes, $1.path) > ($1.physicalBytes, $0.path)
        }
    }

    var repositoryArtifactKinds: [StorageRepositoryArtifactKindModel] {
        Dictionary(grouping: stableRepositoryArtifacts, by: \StorageRepositoryArtifactModel.kind)
            .map { kind, artifacts in
                StorageRepositoryArtifactKindModel(
                    id: kind,
                    label: artifacts.first?.label ?? kind,
                    bytes: artifacts.reduce(0) { $0 &+ $1.physicalBytes },
                    reclaimableBytes: artifacts
                        .filter(\.cleanupAllowed)
                        .reduce(0) { $0 &+ $1.physicalBytes },
                    artifactCount: artifacts.count
                )
            }
            .sorted { ($0.bytes, $1.id) > ($1.bytes, $0.id) }
    }

    var repositoryArtifactFamilies: [StorageRepositoryArtifactFamilyModel] {
        Dictionary(
            grouping: stableRepositoryArtifacts,
            by: \StorageRepositoryArtifactModel.repositoryFamilyId
        )
        .map { familyID, artifacts in
            StorageRepositoryArtifactFamilyModel(
                id: familyID,
                label: artifacts.first?.repositoryFamilyLabel ?? "Repository",
                root: artifacts.first?.repositoryFamilyRoot ?? "",
                bytes: artifacts.reduce(0) { $0 &+ $1.physicalBytes },
                reclaimableBytes: artifacts
                    .filter(\.cleanupAllowed)
                    .reduce(0) { $0 &+ $1.physicalBytes },
                worktreeBytes: artifacts
                    .filter(\.worktree)
                    .reduce(0) { $0 &+ $1.physicalBytes },
                artifacts: artifacts.sorted {
                    ($0.physicalBytes, $1.path) > ($1.physicalBytes, $0.path)
                }
            )
        }
        .sorted { ($0.bytes, $1.id) > ($1.bytes, $0.id) }
    }
}

struct StorageOwnershipBucketModel: Decodable, Identifiable, Sendable {
    let id: String
    let label: String
    let bytes: UInt64
    let reclaimableBytes: UInt64
    let source: String
    let confidence: String
    let detail: String
    let rank: UInt16?
    let state: String?
    let measuredAtMillis: UInt64?
    let subBuckets: [StorageOwnershipSubBucketModel]?

    var effectiveRank: UInt16 {
        if let rank, rank > 0 { return rank }
        switch id {
        case "system": return 10
        case "repositories": return 20
        case "applications": return 30
        case "developer": return 40
        case "personal": return 50
        case "other": return 90
        default: return .max
        }
    }
}

struct StorageOwnershipSubBucketModel: Decodable, Identifiable, Sendable {
    let id: String
    let label: String
    let bytes: UInt64
}

struct StorageRepositoryArtifactModel: Decodable, Identifiable, Sendable {
    let id: String
    let path: String
    let relativePath: String
    let repositoryRoot: String
    let repositoryFamilyId: String
    let repositoryFamilyRoot: String
    let repositoryFamilyLabel: String
    let worktree: Bool
    let kind: String
    let label: String
    let physicalBytes: UInt64
    let fileCount: UInt64
    let newestModifiedMillis: UInt64?
    let evidence: [String]
    let confidence: String
    let gitIgnored: Bool
    let gitTracked: Bool
    let cleanupTier: String
    let cleanupAllowed: Bool
    let cleanupBlockers: [String]
    let defaultCleanupAction: String
    let rebuildInstruction: String
    let estimatedRebuildCost: String
}

struct StorageRepositoryArtifactKindModel: Identifiable, Sendable {
    let id: String
    let label: String
    let bytes: UInt64
    let reclaimableBytes: UInt64
    let artifactCount: Int
}

struct StorageRepositoryArtifactFamilyModel: Identifiable, Sendable {
    let id: String
    let label: String
    let root: String
    let bytes: UInt64
    let reclaimableBytes: UInt64
    let worktreeBytes: UInt64
    let artifacts: [StorageRepositoryArtifactModel]
}

struct StorageSituationRecoveryPlanModel: Decodable, Sendable {
    let state: String
    let reason: String
    let roots: [String]
    let nextStep: String
    let cleanupBlocked: Bool
    let automatic: Bool
    let source: String

    var isActive: Bool {
        cleanupBlocked || state != "none"
    }
}

struct StorageSituationBacklogDrainModel: Decodable, Sendable {
    let state: String
    let reason: String
    let updatedAtMillis: UInt64
    let retryAfterMillis: UInt64?
    let dirtyPathCount: UInt64
    let latestEventId: UInt64?
    let latestMeasurementMillis: UInt64?
    let latestMeasurementStatus: String?
    let lastError: String?
    let source: String
}

struct StorageDirtyPathSummaryModel: Decodable, Sendable {
    let dirtyPathCount: UInt64
    let oldestDirtyMillis: UInt64?
    let latestDirtyMillis: UInt64?
    let latestEventId: UInt64?
    let samplePaths: [String]
    let unknownGap: Bool
    let unknownGapRoots: [String]

    /// Partial cached coverage can be durable (for example, protected paths).
    /// Only queued filesystem changes or an event-stream gap require the
    /// background maintenance worker to keep running at its active cadence.
    var hasPendingMaintenance: Bool {
        dirtyPathCount > 0 || unknownGap
    }

    private enum CodingKeys: String, CodingKey {
        case dirtyPathCount
        case oldestDirtyMillis
        case latestDirtyMillis
        case latestEventId
        case samplePaths
        case unknownGap
        case unknownGapRoots
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        dirtyPathCount = try container.decode(UInt64.self, forKey: .dirtyPathCount)
        oldestDirtyMillis = try container.decodeIfPresent(UInt64.self, forKey: .oldestDirtyMillis)
        latestDirtyMillis = try container.decodeIfPresent(UInt64.self, forKey: .latestDirtyMillis)
        latestEventId = try container.decodeIfPresent(UInt64.self, forKey: .latestEventId)
        samplePaths = try container.decodeIfPresent([String].self, forKey: .samplePaths) ?? []
        unknownGap = try container.decodeIfPresent(Bool.self, forKey: .unknownGap) ?? false
        unknownGapRoots = try container.decodeIfPresent([String].self, forKey: .unknownGapRoots) ?? []
    }
}

struct StorageSituationSummaryModel: Decodable, Sendable {
    let sourceRootCount: Int
    let itemCount: UInt64
    let inventorySizeBytes: UInt64
    let safelyReclaimableNowBytes: UInt64
    let maybeReclaimableBytes: UInt64
    let reviewRequiredBytes: UInt64
    let dangerousUserDataBytes: UInt64
}

struct StorageSituationTopOffenderModel: Decodable, Identifiable, Sendable {
    let path: String
    let sourceRoot: String
    let kind: String
    let cleanupTier: String
    let physicalBytes: UInt64
    let recommendationScore: Double
    let lastScanMillis: UInt64
    let stale: Bool

    var id: String { path }
}
