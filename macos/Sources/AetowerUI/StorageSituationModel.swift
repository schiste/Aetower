import Foundation

struct StorageSituationModel: Decodable, Sendable {
    let capturedAtMillis: UInt64
    let cacheStatus: StorageCacheStatusModel
    let storageIndexStatus: String
    let roots: [String]
    let dirtyPaths: StorageDirtyPathSummaryModel
    let summary: StorageSituationSummaryModel
    let topOffenders: [StorageSituationTopOffenderModel]
    let volumeStates: [StorageVolumeStateModel]
    let caveats: [String]

    var hasCachedFacts: Bool {
        summary.itemCount > 0 || summary.inventorySizeBytes > 0 || !topOffenders.isEmpty
    }
}

struct StorageDirtyPathSummaryModel: Decodable, Sendable {
    let dirtyPathCount: UInt64
    let oldestDirtyMillis: UInt64?
    let latestDirtyMillis: UInt64?
    let latestEventId: UInt64?
    let samplePaths: [String]
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
