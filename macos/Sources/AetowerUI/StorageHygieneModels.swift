import Foundation

enum StorageSupportDirectoryOverride {
    nonisolated(unsafe) static var applicationSupportURL: URL?
}

func storageSupportFileURL(fileName: String, createDirectory: Bool) -> URL? {
    guard let baseURL = StorageSupportDirectoryOverride.applicationSupportURL
        ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
    else {
        return nil
    }
    let directory = baseURL.appendingPathComponent("Aetower", isDirectory: true)
    if createDirectory {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }
    return directory.appendingPathComponent(fileName, isDirectory: false)
}

struct StorageScanJobProgressModel: Decodable, Sendable {
    let phase: String
    let scannedFiles: UInt64
    let scannedDirectories: UInt64
    let scannedBytes: UInt64
    let currentPathHint: String?
    let etaMillis: UInt64?
    let updatedAtMillis: UInt64
    let throttleReason: String?

    static let empty = StorageScanJobProgressModel(
        phase: "idle",
        scannedFiles: 0,
        scannedDirectories: 0,
        scannedBytes: 0,
        currentPathHint: nil,
        etaMillis: nil,
        updatedAtMillis: 0,
        throttleReason: nil
    )
}

struct StorageScanJobResponseModel: Decodable, Sendable {
    let jobId: String
    let status: String
    let coalesced: Bool
    let resultAvailable: Bool
    let errorMessage: String?
    let startedAtMillis: UInt64
    let updatedAtMillis: UInt64
    let completedAtMillis: UInt64?
    let roots: [String]
    let maxDepth: UInt64
    let limit: UInt64
    let mode: String
    let throttleHint: String
    let volumeKey: String
    let resumedFromPartial: Bool?
    let partialStateAvailable: Bool?
    let persistedAtMillis: UInt64?
    let recoveredFiles: UInt64?
    let recoveredDirectories: UInt64?
    let recoveredBytes: UInt64?
    let progress: StorageScanJobProgressModel

    var isActive: Bool {
        status == "queued" || status == "running" || status == "paused"
    }

    var isPaused: Bool {
        status == "paused"
    }
}

struct StorageHygieneReportModel: Decodable, Sendable {
    let capturedAtMillis: UInt64
    let scanDurationMillis: UInt64
    let scanMode: String
    var diagnostics: StorageScanDiagnosticsModel
    let summary: StorageHygieneSummaryModel
    let investigation: StorageInvestigationSummaryModel
    let cleanupTiers: [StorageCleanupTierModel]
    let cleanupRecipes: [StorageCleanupRecipeModel]
    let cleanupBundles: [StorageCleanupBundleModel]
    let cleanupLanes: [StorageCleanupLaneModel]
    let budgetGuardrails: StorageBudgetGuardrailsModel
    let agentHygiene: StorageAgentHygieneSummaryModel
    var repositoryInventory: [StorageRepositoryInventoryModel]
    var repositoryInventoryComplete: Bool
    var repositoryInventoryTruncated: Bool
    var repositoryInventoryRoots: [String]
    var repositoryInventoryPartialRoots: [String]
    var repositoryInventoryCoverage: [StorageRepositoryInventoryCoverageModel]
    let repoFootprints: [StorageRepoFootprintModel]
    let duplicateGroups: [StorageDuplicateGroupModel]
    let redundancyGroups: [StorageRedundancyGroupModel]
    let appFootprints: [StorageAppFootprintModel]
    let systemDataBuckets: [StorageSystemDataBucketModel]
    let treemapRoots: [StorageTreemapNodeModel]
    let growthDeltas: [StorageGrowthDeltaModel]
    let growthInsights: StorageGrowthInsightsModel?
    let coldData: StorageColdDataModel?
    let items: [StorageHygieneItemModel]
    let roots: [String]
    let skippedRoots: [StorageSkippedRootModel]
    let sourceCoverage: [StorageSourceCoverageModel]
    var volumeStates: [StorageVolumeStateModel]
    let truncated: Bool
    let caveats: [String]

    private enum CodingKeys: String, CodingKey {
        case capturedAtMillis
        case scanDurationMillis
        case scanMode
        case diagnostics
        case summary
        case investigation
        case cleanupTiers
        case cleanupRecipes
        case cleanupBundles
        case cleanupLanes
        case budgetGuardrails
        case agentHygiene
        case repositoryInventory
        case repositoryInventoryComplete
        case repositoryInventoryTruncated
        case repositoryInventoryRoots
        case repositoryInventoryPartialRoots
        case repositoryInventoryCoverage
        case repoFootprints
        case duplicateGroups
        case redundancyGroups
        case appFootprints
        case systemDataBuckets
        case treemapRoots
        case growthDeltas
        case growthInsights
        case coldData
        case items
        case roots
        case skippedRoots
        case sourceCoverage
        case volumeStates
        case truncated
        case caveats
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        capturedAtMillis = try container.decode(UInt64.self, forKey: .capturedAtMillis)
        scanDurationMillis = try container.decode(UInt64.self, forKey: .scanDurationMillis)
        scanMode = try container.decodeIfPresent(String.self, forKey: .scanMode) ?? "fast_changed_only"
        diagnostics =
            try container.decodeIfPresent(StorageScanDiagnosticsModel.self, forKey: .diagnostics) ?? .empty
        summary = try container.decode(StorageHygieneSummaryModel.self, forKey: .summary)
        investigation =
            try container.decodeIfPresent(StorageInvestigationSummaryModel.self, forKey: .investigation) ?? .empty
        cleanupTiers = try container.decode([StorageCleanupTierModel].self, forKey: .cleanupTiers)
        cleanupRecipes =
            try container.decodeIfPresent([StorageCleanupRecipeModel].self, forKey: .cleanupRecipes) ?? []
        cleanupBundles =
            try container.decodeIfPresent([StorageCleanupBundleModel].self, forKey: .cleanupBundles) ?? []
        cleanupLanes =
            try container.decodeIfPresent([StorageCleanupLaneModel].self, forKey: .cleanupLanes) ?? []
        budgetGuardrails = try container.decode(StorageBudgetGuardrailsModel.self, forKey: .budgetGuardrails)
        agentHygiene = try container.decode(StorageAgentHygieneSummaryModel.self, forKey: .agentHygiene)
        let decodedRepositoryInventory =
            try container.decodeIfPresent([StorageRepositoryInventoryModel].self, forKey: .repositoryInventory) ?? []
        let decodedRepositoryInventoryCoverage =
            try container.decodeIfPresent(
                [StorageRepositoryInventoryCoverageModel].self,
                forKey: .repositoryInventoryCoverage
            ) ?? []
        let decodedRepositoryInventoryTruncated =
            try container.decodeIfPresent(Bool.self, forKey: .repositoryInventoryTruncated)
                ?? decodedRepositoryInventoryCoverage.contains { $0.truncated }
        let decodedRepositoryInventoryRoots =
            try container.decodeIfPresent([String].self, forKey: .repositoryInventoryRoots)
                ?? decodedRepositoryInventoryCoverage.map(\.path)
        let decodedRepositoryInventoryPartialRoots =
            try container.decodeIfPresent([String].self, forKey: .repositoryInventoryPartialRoots)
                ?? decodedRepositoryInventoryCoverage
                    .filter { !$0.scanned || $0.truncated || $0.status == "partial" }
                    .map(\.path)
        let decodedRepositoryInventoryComplete =
            try container.decodeIfPresent(Bool.self, forKey: .repositoryInventoryComplete)
                ?? (!decodedRepositoryInventoryTruncated && decodedRepositoryInventoryPartialRoots.isEmpty)
        repositoryInventory = decodedRepositoryInventory
        repositoryInventoryCoverage = decodedRepositoryInventoryCoverage
        repositoryInventoryTruncated = decodedRepositoryInventoryTruncated
        repositoryInventoryRoots = decodedRepositoryInventoryRoots
        repositoryInventoryPartialRoots = decodedRepositoryInventoryPartialRoots
        repositoryInventoryComplete = decodedRepositoryInventoryComplete
        repoFootprints = try container.decodeIfPresent([StorageRepoFootprintModel].self, forKey: .repoFootprints) ?? []
        duplicateGroups =
            try container.decodeIfPresent([StorageDuplicateGroupModel].self, forKey: .duplicateGroups) ?? []
        redundancyGroups =
            try container.decodeIfPresent([StorageRedundancyGroupModel].self, forKey: .redundancyGroups) ?? []
        appFootprints = try container.decodeIfPresent([StorageAppFootprintModel].self, forKey: .appFootprints) ?? []
        systemDataBuckets =
            try container.decodeIfPresent([StorageSystemDataBucketModel].self, forKey: .systemDataBuckets) ?? []
        treemapRoots = try container.decodeIfPresent([StorageTreemapNodeModel].self, forKey: .treemapRoots) ?? []
        growthDeltas = try container.decodeIfPresent([StorageGrowthDeltaModel].self, forKey: .growthDeltas) ?? []
        // Additive insight blocks (growth intelligence + cold-data lane); a
        // shape drift there must not sink the whole report decode.
        growthInsights = (try? container.decodeIfPresent(StorageGrowthInsightsModel.self, forKey: .growthInsights)) ?? nil
        coldData = (try? container.decodeIfPresent(StorageColdDataModel.self, forKey: .coldData)) ?? nil
        items = try container.decodeIfPresent([StorageHygieneItemModel].self, forKey: .items) ?? []
        roots = try container.decodeIfPresent([String].self, forKey: .roots) ?? []
        skippedRoots = try container.decodeIfPresent([StorageSkippedRootModel].self, forKey: .skippedRoots) ?? []
        sourceCoverage =
            try container.decodeIfPresent([StorageSourceCoverageModel].self, forKey: .sourceCoverage) ?? []
        volumeStates =
            try container.decodeIfPresent([StorageVolumeStateModel].self, forKey: .volumeStates) ?? []
        truncated = try container.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
        caveats = try container.decodeIfPresent([String].self, forKey: .caveats) ?? []
    }
}

struct StorageHygieneBaselineModel: Codable, Sendable {
    let capturedAtMillis: UInt64
    let repoFootprints: [StorageRepoFootprintBaselineModel]
    let items: [StorageHygieneItemBaselineModel]

    init(report: StorageHygieneReportModel) {
        capturedAtMillis = report.capturedAtMillis
        repoFootprints = report.repoFootprints.map(StorageRepoFootprintBaselineModel.init)
        items = report.items.map(StorageHygieneItemBaselineModel.init)
    }
}

struct RepositoryInventoryReportModel: Decodable, Sendable {
    let capturedAtMillis: UInt64
    let scanDurationMillis: UInt64
    let roots: [String]
    let repositoryInventory: [StorageRepositoryInventoryModel]
    let repositoryInventoryComplete: Bool
    let repositoryInventoryTruncated: Bool
    let repositoryInventoryRoots: [String]
    let repositoryInventoryPartialRoots: [String]
    let repositoryInventoryCoverage: [StorageRepositoryInventoryCoverageModel]
    let truncated: Bool
    let diagnostics: RepositoryInventoryDiagnosticsModel
}

struct RepositoryInventoryDiagnosticsModel: Decodable, Sendable {
    let repositoryWalkMillis: UInt64
    let gitMillis: UInt64
    let discoveredRepositoryCount: UInt64
    let scannedDirectoryCount: UInt64
    let skippedDirectoryCount: UInt64
}

struct StorageRepoFootprintBaselineModel: Codable, Identifiable, Sendable {
    let repoRoot: String
    let repoName: String
    let currentSizeBytes: UInt64

    var id: String { repoRoot }

    init(footprint: StorageRepoFootprintModel) {
        repoRoot = footprint.repoRoot
        repoName = footprint.repoName
        currentSizeBytes = footprint.currentSizeBytes
    }
}

struct StorageHygieneItemBaselineModel: Codable, Identifiable, Sendable {
    let id: String
    let path: String
    let sizeBytes: UInt64
    let modifiedMillis: UInt64?

    init(item: StorageHygieneItemModel) {
        id = item.id
        path = item.path
        sizeBytes = item.sizeBytes
        modifiedMillis = item.modifiedMillis
    }
}

enum StorageHygieneBaselineStore {
    private static let key = "aetower.storageHygiene.baseline.v1"

    static func load() -> StorageHygieneBaselineModel? {
        // Back up corrupt bytes instead of silently discarding: a baseline-shape
        // change previously nil'd here and reset growth deltas with no signal.
        decodeUserDefaultsJSON(StorageHygieneBaselineModel.self, key: key)
    }

    static func save(_ baseline: StorageHygieneBaselineModel) {
        guard let data = try? JSONEncoder().encode(baseline) else {
            return
        }
        UserDefaults.standard.set(data, forKey: key)
    }
}

struct StorageHygieneReportCacheHit: Sendable {
    let report: StorageHygieneReportModel
    let savedAtMillis: UInt64
    let repositoryCount: Int
}

struct StorageHygieneReportCacheDisplayHit: Sendable {
    let cache: StorageHygieneReportCacheHit
    let staleReason: String?

    var isStale: Bool {
        staleReason != nil
    }
}

enum StorageHygieneReportCacheDisplayLoadResult: Sendable {
    case hit(StorageHygieneReportCacheDisplayHit)
    case miss(String)
}

enum StorageHygieneReportCacheLoadResult: Sendable {
    case hit(StorageHygieneReportCacheHit)
    case miss(String)
    case stale(String)
}

private struct StorageHygieneReportCacheRecord: Codable {
    let schemaVersion: UInt8
    let savedAtMillis: UInt64
    let reportCapturedAtMillis: UInt64
    let requestedRoots: [String]
    let scannedRoots: [String]
    let maxDepth: UInt32
    let limit: UInt32
    let scanMode: String
    let reportJson: String
    let repositories: [StorageHygieneRepositoryCacheFingerprint]
}

struct StorageHygieneRepositoryFingerprintAudit: Sendable {
    let checkedRepositoryCount: Int
    let changedRepositoryRoots: [String]
    let missingRepositoryRoots: [String]
    let savedAtMillis: UInt64?
    let unavailableReason: String?

    var changedRepositoryCount: Int {
        changedRepositoryRoots.count
    }

    var missingRepositoryCount: Int {
        missingRepositoryRoots.count
    }

    var hasChangedRepositories: Bool {
        changedRepositoryCount > 0 || missingRepositoryCount > 0
    }

    var sampleRoots: [String] {
        Array((changedRepositoryRoots + missingRepositoryRoots).prefix(3))
    }
}

private let storageAgentContractCachePaths = [
    "AGENTS.md",
    ".agents/manifest.yaml",
    ".agents/tasks.yaml",
    ".agents/repo-map.yaml",
    ".agents/contracts.yaml",
    ".agents/commands.yaml",
    ".agents/validation.yaml",
    ".agents/boundaries.yaml",
    ".agents/risks.yaml",
    ".agents/references.yaml",
]

private struct StorageHygieneRepositoryCacheFingerprint: Codable, Equatable {
    let repoRoot: String
    let repoName: String
    let repoRootFingerprint: String?
    let gitMarkerFingerprint: String?
    let gitDirectoryFingerprint: String?
    let gitHead: String?
    let gitRef: String?
    let gitBranch: String?
    let gitRemoteKey: String?
    let gitDirtyStatus: String
    let gitDirtyFileCount: UInt64?
    let gitDirtyTruncated: Bool
    let gitConfigFingerprint: String?
    let gitIndexFingerprint: String?
    let gitHeadFingerprint: String?
    let gitHeadRefFingerprint: String?
    let gitPackedRefsFingerprint: String?
    let agentReadinessScore: UInt8
    let agentReadinessStatus: String
    let agentGuidanceIssueCount: UInt64
    let agentContracts: [StorageHygieneContractCacheFingerprint]

    init(repository: StorageRepositoryInventoryModel) {
        repoRoot = repository.repoRoot
        repoName = repository.repoName
        let rootURL = URL(fileURLWithPath: repository.repoRoot, isDirectory: true)
        gitHead = repository.gitHead
        gitRef = repository.gitRef
        gitBranch = repository.gitBranch
        gitRemoteKey = repository.gitRemoteKey
        gitDirtyStatus = repository.gitDirtyStatus
        gitDirtyFileCount = repository.gitDirtyFileCount
        gitDirtyTruncated = repository.gitDirtyTruncated
        let gitDirectory = Self.gitDirectory(repoRoot: repository.repoRoot)
        repoRootFingerprint = Self.fileFingerprint(rootURL)
        gitMarkerFingerprint = Self.fileFingerprint(rootURL.appendingPathComponent(".git", isDirectory: false))
        gitDirectoryFingerprint = Self.fileFingerprint(gitDirectory)
        gitConfigFingerprint = Self.fileFingerprint(gitDirectory?.appendingPathComponent("config"))
        gitIndexFingerprint = Self.fileFingerprint(gitDirectory?.appendingPathComponent("index"))
        gitHeadFingerprint = Self.fileFingerprint(gitDirectory?.appendingPathComponent("HEAD"))
        gitHeadRefFingerprint = Self.gitHeadRefFingerprint(repoRoot: repository.repoRoot, gitDirectory: gitDirectory)
        gitPackedRefsFingerprint = Self.fileFingerprint(gitDirectory?.appendingPathComponent("packed-refs"))
        agentReadinessScore = repository.agentReadinessScore
        agentReadinessStatus = repository.agentReadinessStatus
        agentGuidanceIssueCount = repository.agentGuidanceIssueCount
        agentContracts = Self.agentContractFingerprints(repoRoot: repository.repoRoot)
    }

    func matchesCurrent(_ current: StorageHygieneRepositoryCacheFingerprint) -> Bool {
        guard repoRoot == current.repoRoot,
              repoName == current.repoName,
              gitHead == current.gitHead,
              gitRef == current.gitRef,
              gitBranch == current.gitBranch,
              gitRemoteKey == current.gitRemoteKey,
              gitDirtyStatus == current.gitDirtyStatus,
              gitDirtyFileCount == current.gitDirtyFileCount,
              gitDirtyTruncated == current.gitDirtyTruncated,
              agentReadinessScore == current.agentReadinessScore,
              agentReadinessStatus == current.agentReadinessStatus,
              agentGuidanceIssueCount == current.agentGuidanceIssueCount,
              agentContracts == current.agentContracts
        else {
            return false
        }

        return Self.optionalFingerprintMatches(repoRootFingerprint, current.repoRootFingerprint)
            && Self.optionalFingerprintMatches(gitMarkerFingerprint, current.gitMarkerFingerprint)
            && Self.optionalFingerprintMatches(gitDirectoryFingerprint, current.gitDirectoryFingerprint)
            && Self.optionalFingerprintMatches(gitConfigFingerprint, current.gitConfigFingerprint)
            && Self.optionalFingerprintMatches(gitIndexFingerprint, current.gitIndexFingerprint)
            && Self.optionalFingerprintMatches(gitHeadFingerprint, current.gitHeadFingerprint)
            && Self.optionalFingerprintMatches(gitHeadRefFingerprint, current.gitHeadRefFingerprint)
            && Self.optionalFingerprintMatches(gitPackedRefsFingerprint, current.gitPackedRefsFingerprint)
    }

    private static func optionalFingerprintMatches(_ cached: String?, _ current: String?) -> Bool {
        guard let cached else {
            // Older cache records do not contain every cheap filesystem
            // fingerprint. Treat those fields as unknown so a version upgrade
            // does not force reparsing every repository before the next save.
            return true
        }
        return cached == current
    }

    private static func agentContractFingerprints(repoRoot: String) -> [StorageHygieneContractCacheFingerprint] {
        storageAgentContractCachePaths.map { relativePath in
            let url = URL(fileURLWithPath: repoRoot, isDirectory: true)
                .appendingPathComponent(relativePath, isDirectory: false)
            return StorageHygieneContractCacheFingerprint(path: relativePath, fileURL: url)
        }
    }

    private static func gitDirectory(repoRoot: String) -> URL? {
        let rootURL = URL(fileURLWithPath: repoRoot, isDirectory: true)
        let dotGitURL = rootURL.appendingPathComponent(".git", isDirectory: false)
        var isDirectory: ObjCBool = false
        if FileManager.default.fileExists(atPath: dotGitURL.path, isDirectory: &isDirectory) {
            if isDirectory.boolValue {
                return dotGitURL
            }
            guard let content = try? String(contentsOf: dotGitURL, encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines),
                let relativePath = content.stripPrefix("gitdir: "),
                !relativePath.isEmpty
            else {
                return nil
            }
            if relativePath.hasPrefix("/") {
                return URL(fileURLWithPath: relativePath, isDirectory: true)
            }
            return rootURL.appendingPathComponent(relativePath, isDirectory: true)
        }
        return nil
    }

    private static func fileFingerprint(_ url: URL?) -> String? {
        guard let url else { return nil }
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path) else {
            return nil
        }
        let size = (attributes[.size] as? NSNumber)?.uint64Value ?? 0
        let modifiedMillis = ((attributes[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0) * 1000
        return "\(size):\(UInt64(modifiedMillis))"
    }

    private static func gitHeadRefFingerprint(repoRoot: String, gitDirectory: URL?) -> String? {
        guard let gitDirectory else { return nil }
        let rootURL = URL(fileURLWithPath: repoRoot, isDirectory: true)
        let headURL = gitDirectory.appendingPathComponent("HEAD", isDirectory: false)
        guard let content = try? String(contentsOf: headURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines),
            let ref = content.stripPrefix("ref: "),
            !ref.isEmpty
        else {
            return nil
        }
        let refURL = ref.hasPrefix("/")
            ? URL(fileURLWithPath: ref, isDirectory: false)
            : gitDirectory.appendingPathComponent(ref, isDirectory: false)
        return fileFingerprint(refURL) ?? fileFingerprint(rootURL.appendingPathComponent(ref, isDirectory: false))
    }
}

private struct StorageHygieneContractCacheFingerprint: Codable, Equatable {
    let path: String
    let present: Bool
    let sizeBytes: UInt64?
    let modifiedMillis: UInt64?

    init(path: String, fileURL: URL) {
        self.path = path
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: fileURL.path) else {
            present = false
            sizeBytes = nil
            modifiedMillis = nil
            return
        }
        present = true
        sizeBytes = (attributes[.size] as? NSNumber)?.uint64Value
        modifiedMillis = ((attributes[.modificationDate] as? Date)?.timeIntervalSince1970)
            .map { UInt64($0 * 1000) }
    }
}

enum StorageHygieneReportCacheStore {
    // v6: scan truncation semantics split root-walk truncation from partial
    // sizing, and default storage coverage now includes Colima/Lima VM disks.
    // Older records can replay obsolete startup warnings or omit large Colima
    // storage, so force one fresh engine payload.
    private static let schemaVersion: UInt8 = 6
    // v5: growth insights, cold-data lane, recommendation score, and writer
    // identity fields landed; older cached reports without them are treated as
    // stale so the new sections repaint from a fresh engine payload.
    private static let fileName = "storage-hygiene-report-cache-v1.json"
    private static let cacheMaxAgeMillis: UInt64 = 7 * 24 * 60 * 60 * 1000

    /// Loads the last full report when it still matches the requested roots.
    /// This intentionally avoids live repository/root validation so the UI can
    /// paint immediately; lightweight inventory verification runs after.
    static func loadIfValid(
        roots: [String]
    ) -> StorageHygieneReportCacheLoadResult {
        guard let record = loadRecord() else {
            return .miss("cache record could not be decoded")
        }
        guard record.schemaVersion == schemaVersion else {
            return .stale("cache schema changed")
        }
        let nowMillis = currentMillis()
        guard nowMillis >= record.savedAtMillis,
            nowMillis - record.savedAtMillis <= cacheMaxAgeMillis
        else {
            return .stale("cache age exceeded")
        }
        let requestedRoots = normalizedRequestedRoots(roots)
        guard record.requestedRoots == requestedRoots else {
            return .stale("scan roots changed")
        }
        guard let report = decodeReport(from: record) else {
            return .stale("cached report no longer matches the app model")
        }
        guard record.reportCapturedAtMillis == report.capturedAtMillis else {
            return .stale("cached report metadata mismatch")
        }
        return .hit(cacheHit(record: record, report: report))
    }

    /// Loads the last displayable report even when it needs a refresh. This is
    /// intentionally looser than `loadIfValid`: startup should paint the last
    /// known scan first, then let the freshness layer verify or rescan.
    static func loadForDisplay(
        roots: [String]
    ) -> StorageHygieneReportCacheDisplayLoadResult {
        guard let record = loadRecord() else {
            return .miss("cache record could not be decoded")
        }

        let requestedRoots = normalizedRequestedRoots(roots)
        let rootsMatch = record.requestedRoots == requestedRoots
        let canDisplayRootMismatch = roots.isEmpty && !record.requestedRoots.isEmpty
        guard rootsMatch || canDisplayRootMismatch else {
            return .miss("scan roots changed")
        }
        guard let report = decodeReport(from: record) else {
            return .miss("cached report no longer matches the app model")
        }
        guard record.reportCapturedAtMillis == report.capturedAtMillis else {
            return .miss("cached report metadata mismatch")
        }

        var staleReasons: [String] = []
        if record.schemaVersion != schemaVersion {
            staleReasons.append("cache schema changed")
        }
        let nowMillis = currentMillis()
        if nowMillis < record.savedAtMillis || nowMillis - record.savedAtMillis > cacheMaxAgeMillis {
            staleReasons.append("cache age exceeded")
        }
        if !rootsMatch {
            staleReasons.append("scan roots changed")
        }

        return .hit(
            StorageHygieneReportCacheDisplayHit(
                cache: cacheHit(record: record, report: report),
                staleReason: staleReasons.isEmpty ? nil : staleReasons.joined(separator: ", ")
            )
        )
    }

    static func auditRepositoryFingerprints(
        report: StorageHygieneReportModel,
        roots: [String]
    ) -> StorageHygieneRepositoryFingerprintAudit {
        guard let record = loadRecordForAudit(roots: roots) else {
            return StorageHygieneRepositoryFingerprintAudit(
                checkedRepositoryCount: 0,
                changedRepositoryRoots: [],
                missingRepositoryRoots: [],
                savedAtMillis: nil,
                unavailableReason: "cache record unavailable"
            )
        }

        let cachedByRoot = Dictionary(uniqueKeysWithValues: record.repositories.map { ($0.repoRoot, $0) })
        var changedRoots: [String] = []
        var missingRoots: [String] = []

        for repository in report.repositoryInventory {
            if !FileManager.default.fileExists(atPath: repository.repoRoot) {
                missingRoots.append(repository.repoRoot)
                continue
            }
            guard let cached = cachedByRoot[repository.repoRoot] else {
                changedRoots.append(repository.repoRoot)
                continue
            }
            let current = StorageHygieneRepositoryCacheFingerprint(repository: repository)
            if !cached.matchesCurrent(current) {
                changedRoots.append(repository.repoRoot)
            }
        }

        return StorageHygieneRepositoryFingerprintAudit(
            checkedRepositoryCount: report.repositoryInventory.count,
            changedRepositoryRoots: changedRoots,
            missingRepositoryRoots: missingRoots,
            savedAtMillis: record.savedAtMillis,
            unavailableReason: nil
        )
    }

    static func save(
        report: StorageHygieneReportModel,
        rawJSON: String,
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32,
        mode: String = "fast_changed_only"
    ) {
        guard let url = cacheURL(createDirectory: true) else { return }
        let record = StorageHygieneReportCacheRecord(
            schemaVersion: schemaVersion,
            savedAtMillis: currentMillis(),
            reportCapturedAtMillis: report.capturedAtMillis,
            requestedRoots: normalizedRequestedRoots(roots),
            scannedRoots: normalizedPathSet(report.roots),
            maxDepth: maxDepth,
            limit: limit,
            scanMode: mode,
            reportJson: rawJSON,
            repositories: report.repositoryInventory.map(StorageHygieneRepositoryCacheFingerprint.init)
        )
        guard let data = try? JSONEncoder().encode(record) else { return }
        try? data.write(to: url, options: [.atomic])
    }

    static func invalidate() {
        guard let url = cacheURL(createDirectory: false) else { return }
        try? FileManager.default.removeItem(at: url)
    }

    private static func loadRecordForAudit(roots: [String]) -> StorageHygieneReportCacheRecord? {
        guard let record = loadRecord() else { return nil }
        guard record.schemaVersion == schemaVersion else { return nil }
        guard record.requestedRoots == normalizedRequestedRoots(roots) else { return nil }
        return record
    }

    private static func loadRecord() -> StorageHygieneReportCacheRecord? {
        guard let url = cacheURL(createDirectory: false),
              let data = try? Data(contentsOf: url)
        else {
            return nil
        }
        return try? JSONDecoder().decode(StorageHygieneReportCacheRecord.self, from: data)
    }

    private static func decodeReport(
        from record: StorageHygieneReportCacheRecord
    ) -> StorageHygieneReportModel? {
        guard let reportData = record.reportJson.data(using: .utf8) else { return nil }
        return try? AetowerJSON.snakeCaseDecoder()
            .decode(StorageHygieneReportModel.self, from: reportData)
    }

    private static func cacheHit(
        record: StorageHygieneReportCacheRecord,
        report: StorageHygieneReportModel
    ) -> StorageHygieneReportCacheHit {
        StorageHygieneReportCacheHit(
            report: report,
            savedAtMillis: record.savedAtMillis,
            repositoryCount: record.repositories.count
        )
    }

    private static func cacheURL(createDirectory: Bool) -> URL? {
        storageSupportFileURL(fileName: fileName, createDirectory: createDirectory)
    }

    private static func normalizedRequestedRoots(_ roots: [String]) -> [String] {
        let selected = roots.isEmpty ? defaultStorageRoots() : roots
        return normalizedPathSet(Array(selected.prefix(24)))
    }

    private static func defaultStorageRoots() -> [String] {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        guard !home.isEmpty else { return [] }
        return [
            "Repositories",
            "Downloads",
            "Developer",
            "Projects",
            ".claude",
            ".codex",
            ".cursor",
            ".aider",
            ".colima",
            "Library/Developer/Xcode/DerivedData",
            "Library/Caches/org.swift.swiftpm",
            "Library/Caches/com.apple.dt.Xcode",
        ].map { "\(home)/\($0)" }
    }

    private static func normalizedPathSet(_ paths: [String]) -> [String] {
        Array(Set(paths.map(normalizedPath).filter { !$0.isEmpty })).sorted()
    }

    private static func normalizedPath(_ path: String) -> String {
        PathNormalization.standardizedDirectoryPath(path)
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}

private extension String {
    func stripPrefix(_ prefix: String) -> String? {
        guard hasPrefix(prefix) else { return nil }
        return String(dropFirst(prefix.count))
    }
}

struct StorageHygieneSummaryModel: Decodable, Sendable {
    let itemCount: Int
    let inventorySizeBytes: UInt64
    let maybeReclaimableBytes: UInt64
    let safelyReclaimableNowBytes: UInt64
    let reviewRequiredBytes: UInt64
    let dangerousUserDataBytes: UInt64
    let totalReclaimableBytes: UInt64
    let safeCandidateCount: Int
    let reviewCandidateCount: Int
    let staleCandidateCount: Int
    let scannedDirectoryCount: UInt64
    let largestItemPath: String?
    let largestItemBytes: UInt64
    let attributedRepoCount: Int

    private enum CodingKeys: String, CodingKey {
        case itemCount
        case inventorySizeBytes
        case maybeReclaimableBytes
        case safelyReclaimableNowBytes
        case reviewRequiredBytes
        case dangerousUserDataBytes
        case totalReclaimableBytes
        case safeCandidateCount
        case reviewCandidateCount
        case staleCandidateCount
        case scannedDirectoryCount
        case largestItemPath
        case largestItemBytes
        case attributedRepoCount
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        itemCount = try container.decode(Int.self, forKey: .itemCount)
        totalReclaimableBytes = try container.decode(UInt64.self, forKey: .totalReclaimableBytes)
        inventorySizeBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .inventorySizeBytes)
            ?? totalReclaimableBytes
        safelyReclaimableNowBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .safelyReclaimableNowBytes)
            ?? totalReclaimableBytes
        maybeReclaimableBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .maybeReclaimableBytes) ?? 0
        reviewRequiredBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .reviewRequiredBytes) ?? 0
        dangerousUserDataBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .dangerousUserDataBytes) ?? 0
        safeCandidateCount = try container.decode(Int.self, forKey: .safeCandidateCount)
        reviewCandidateCount = try container.decode(Int.self, forKey: .reviewCandidateCount)
        staleCandidateCount = try container.decode(Int.self, forKey: .staleCandidateCount)
        scannedDirectoryCount = try container.decode(UInt64.self, forKey: .scannedDirectoryCount)
        largestItemPath = try container.decodeIfPresent(String.self, forKey: .largestItemPath)
        largestItemBytes = try container.decode(UInt64.self, forKey: .largestItemBytes)
        attributedRepoCount = try container.decode(Int.self, forKey: .attributedRepoCount)
    }
}

struct StorageScanDiagnosticsModel: Decodable, Sendable {
    let mode: String
    let rootWalkMillis: UInt64
    let sizeWalkMillis: UInt64
    let gitMillis: UInt64
    let serializeMillis: UInt64
    let payloadBytes: UInt64
    var decodeMillis: UInt64
    let scannedDirectoryCount: UInt64
    let discoveredRepositoryCount: UInt64
    let sizedEntryCount: UInt64
    let candidateSeenCount: UInt64
    let candidateRetainedCount: UInt64
    let storageIndexStatus: String
    let storageIndexHits: UInt64
    let storageIndexMisses: UInt64
    let storageIndexWrites: UInt64
    let nativeMetadataStrategy: String
    let fseventsStatus: String
    let lazyGitStatus: Bool
    let topKRetained: Bool
    let performanceBudget: StoragePerformanceBudgetDiagnosticsModel?

    static let empty = StorageScanDiagnosticsModel(
        mode: "unknown",
        rootWalkMillis: 0,
        sizeWalkMillis: 0,
        gitMillis: 0,
        serializeMillis: 0,
        payloadBytes: 0,
        decodeMillis: 0,
        scannedDirectoryCount: 0,
        discoveredRepositoryCount: 0,
        sizedEntryCount: 0,
        candidateSeenCount: 0,
        candidateRetainedCount: 0,
        storageIndexStatus: "unknown",
        storageIndexHits: 0,
        storageIndexMisses: 0,
        storageIndexWrites: 0,
        nativeMetadataStrategy: "unknown",
        fseventsStatus: "unknown",
        lazyGitStatus: false,
        topKRetained: false,
        performanceBudget: nil
    )
}

struct StoragePerformanceBudgetDiagnosticsModel: Decodable, Sendable {
    let status: String
    let scanJobLatencyMillis: UInt64
    let payloadBytes: UInt64
    let payloadBudgetBytes: UInt64
    let tablePageMillis: UInt64
    let tablePageBudgetMillis: UInt64
    let renderPublishMillis: UInt64
    let renderBudgetMillis: UInt64
    let notes: [String]
}

struct StorageInvestigationSummaryModel: Decodable, Sendable {
    let topFindings: [StorageInvestigationFindingModel]
    let knownCacheBytes: UInt64
    let rebuildableBytes: UInt64
    let expensiveBytes: UInt64
    let riskyBytes: UInt64
    let largeFileCount: Int
    let coldFileCount: Int
    let coldFileBytes: UInt64
    let reviewItemCount: Int
    let openConflictStatus: String
    let recommendedNextSteps: [String]

    static let empty = StorageInvestigationSummaryModel(
        topFindings: [],
        knownCacheBytes: 0,
        rebuildableBytes: 0,
        expensiveBytes: 0,
        riskyBytes: 0,
        largeFileCount: 0,
        coldFileCount: 0,
        coldFileBytes: 0,
        reviewItemCount: 0,
        openConflictStatus: "not_checked",
        recommendedNextSteps: []
    )
}

struct StorageInvestigationFindingModel: Decodable, Identifiable, Sendable {
    let id: String
    let title: String
    let detail: String
    let path: String
    let storageRole: String
    let cleanupTier: String
    let safety: String
    let sizeBytes: UInt64
    let confidenceScore: UInt8
    let evidence: [String]
    let recommendedAction: String
}

struct StorageCleanupTierModel: Decodable, Identifiable, Sendable {
    let tier: String
    let label: String
    let description: String
    let itemCount: Int
    let bytes: UInt64

    var id: String { tier }
}

struct StorageCleanupRecipeModel: Decodable, Identifiable, Sendable {
    let id: String
    let title: String
    let category: String
    let safety: String
    let affectedPath: String
    let command: String
    let estimatedReclaimableBytes: UInt64
    let reason: String
    let prerequisites: [String]
    let destructive: Bool
    let requiresReview: Bool
}

struct StorageCleanupBundleModel: Decodable, Identifiable, Sendable {
    let id: String
    let title: String
    let subtitle: String
    let safety: String
    let confidenceScore: UInt8
    let estimatedReclaimableBytes: UInt64
    let itemCount: Int
    let dryRunOnly: Bool
    let manifest: [StorageCleanupBundleItemModel]
    let dryRunCommands: [String]
    let rollbackNotes: [String]
    let prerequisites: [String]
    let caveats: [String]
}

struct StorageCleanupBundleItemModel: Decodable, Identifiable, Sendable {
    let path: String
    let displayName: String
    let kind: String
    let cleanupTier: String
    let safety: String
    let sizeBytes: UInt64
    let confidenceScore: UInt8
    let dryRunCommand: String
    let cleanupCommand: String?
    let rollbackNote: String
    let reason: String
    let consequence: String
    let evidence: [String]
    let cleanupAllowed: Bool
    let cleanupBlockers: [String]
    let defaultCleanupAction: String

    var id: String { path }

    private enum CodingKeys: String, CodingKey {
        case path
        case displayName
        case kind
        case cleanupTier
        case safety
        case sizeBytes
        case confidenceScore
        case dryRunCommand
        case cleanupCommand
        case rollbackNote
        case reason
        case consequence
        case evidence
        case cleanupAllowed
        case cleanupBlockers
        case defaultCleanupAction
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        path = try container.decode(String.self, forKey: .path)
        displayName = try container.decode(String.self, forKey: .displayName)
        kind = try container.decode(String.self, forKey: .kind)
        cleanupTier = try container.decode(String.self, forKey: .cleanupTier)
        safety = try container.decode(String.self, forKey: .safety)
        sizeBytes = try container.decode(UInt64.self, forKey: .sizeBytes)
        confidenceScore = try container.decode(UInt8.self, forKey: .confidenceScore)
        dryRunCommand = try container.decode(String.self, forKey: .dryRunCommand)
        cleanupCommand = try container.decodeIfPresent(String.self, forKey: .cleanupCommand)
        rollbackNote = try container.decode(String.self, forKey: .rollbackNote)
        reason = try container.decode(String.self, forKey: .reason)
        consequence = try container.decodeIfPresent(String.self, forKey: .consequence) ?? rollbackNote
        evidence = try container.decodeIfPresent([String].self, forKey: .evidence) ?? []
        cleanupAllowed = try container.decodeIfPresent(Bool.self, forKey: .cleanupAllowed) ?? true
        cleanupBlockers = try container.decodeIfPresent([String].self, forKey: .cleanupBlockers) ?? []
        defaultCleanupAction = try container.decodeIfPresent(String.self, forKey: .defaultCleanupAction) ?? "trash"
    }
}

struct StorageCleanupLaneModel: Decodable, Identifiable, Sendable {
    let id: String
    let title: String
    let subtitle: String
    let laneKind: String
    let safety: String
    let actionLabel: String
    let command: String?
    let estimatedReclaimableBytes: UInt64
    let itemCount: Int
    let requiresAdmin: Bool
    let requiresReview: Bool
    let canStageTrash: Bool
    let items: [StorageCleanupLaneItemModel]
    let blockers: [String]
    let caveats: [String]
}

struct StorageCleanupLaneItemModel: Decodable, Identifiable, Sendable {
    let path: String
    let displayName: String
    let kind: String
    let cleanupTier: String
    let safety: String
    let sizeBytes: UInt64
    let reason: String
    let nextStep: String
    let evidence: [String]
    let cleanupAllowed: Bool
    let cleanupBlockers: [String]
    let defaultCleanupAction: String

    var id: String { path }
}

struct StorageBudgetGuardrailsModel: Decodable, Sendable {
    let repoGrowthBudgetBytesPerDay: UInt64
    let repoArtifactBudgetBytes: UInt64
    let totalArtifactBudgetBytes: UInt64
    let freeSpaceFloorBytes: UInt64
    let volumePressureFloorPercent: UInt64
    let warningOnlyByDefault: Bool
    let autoTrashSafeTierEnabled: Bool
    let scheduledScanRecommended: Bool
    let scheduledScanIntervalHours: UInt64
    let status: String
    let violations: [StorageBudgetViolationModel]
    let policies: [StoragePreventionPolicyModel]
    let preventionSuggestions: [StoragePreventionSuggestionModel]

    private enum CodingKeys: String, CodingKey {
        case repoGrowthBudgetBytesPerDay
        case repoArtifactBudgetBytes
        case totalArtifactBudgetBytes
        case freeSpaceFloorBytes
        case volumePressureFloorPercent
        case warningOnlyByDefault
        case autoTrashSafeTierEnabled
        case scheduledScanRecommended
        case scheduledScanIntervalHours
        case status
        case violations
        case policies
        case preventionSuggestions
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        repoGrowthBudgetBytesPerDay = try container.decode(UInt64.self, forKey: .repoGrowthBudgetBytesPerDay)
        repoArtifactBudgetBytes = try container.decode(UInt64.self, forKey: .repoArtifactBudgetBytes)
        totalArtifactBudgetBytes = try container.decode(UInt64.self, forKey: .totalArtifactBudgetBytes)
        freeSpaceFloorBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .freeSpaceFloorBytes) ?? 20 * 1_024 * 1_024 * 1_024
        volumePressureFloorPercent =
            try container.decodeIfPresent(UInt64.self, forKey: .volumePressureFloorPercent) ?? 10
        warningOnlyByDefault = try container.decodeIfPresent(Bool.self, forKey: .warningOnlyByDefault) ?? true
        autoTrashSafeTierEnabled = try container.decodeIfPresent(Bool.self, forKey: .autoTrashSafeTierEnabled) ?? false
        scheduledScanRecommended = try container.decodeIfPresent(Bool.self, forKey: .scheduledScanRecommended) ?? false
        scheduledScanIntervalHours = try container.decodeIfPresent(UInt64.self, forKey: .scheduledScanIntervalHours) ?? 24
        status = try container.decode(String.self, forKey: .status)
        violations = try container.decode([StorageBudgetViolationModel].self, forKey: .violations)
        policies = try container.decodeIfPresent([StoragePreventionPolicyModel].self, forKey: .policies) ?? []
        preventionSuggestions =
            try container.decodeIfPresent([StoragePreventionSuggestionModel].self, forKey: .preventionSuggestions) ?? []
    }
}

struct StorageBudgetViolationModel: Decodable, Identifiable, Sendable {
    let id: String
    let scope: String
    let severity: String
    let title: String
    let detail: String
    let repoRoot: String?
    let repoName: String?
    let observedBytes: UInt64
    let limitBytes: UInt64
    let recommendation: String
}

struct StoragePreventionPolicyModel: Decodable, Identifiable, Sendable {
    let id: String
    let title: String
    let mode: String
    let enabled: Bool
    let action: String
    let tier: String
    let detail: String
    let nextStep: String
}

struct StoragePreventionSuggestionModel: Decodable, Identifiable, Sendable {
    let id: String
    let trigger: String
    let title: String
    let detail: String
    let actionLabel: String
    let estimatedReclaimableBytes: UInt64
    let safety: String
    let requiresApproval: Bool
}

struct StorageAgentHygieneSummaryModel: Decodable, Sendable {
    let totalAgentArtifactBytes: UInt64
    let weekAgentArtifactBytes: UInt64
    let rebuildableAgentBytes: UInt64
    let rebuildableAgentPercent: Double
    let weekRebuildableAgentBytes: UInt64
    let weekRebuildableAgentPercent: Double
    let attributedItemCount: Int
    let agentCount: Int
    let agents: [StorageAgentArtifactSummaryModel]
    let caveats: [String]
}

struct StorageAgentArtifactSummaryModel: Decodable, Identifiable, Sendable {
    let id: String
    let provider: String
    let displayName: String
    let sessionId: String?
    let artifactBytes: UInt64
    let weekArtifactBytes: UInt64
    let rebuildableBytes: UInt64
    let rebuildablePercent: Double
    let weekRebuildableBytes: UInt64
    let weekRebuildablePercent: Double
    let itemCount: Int
    let repoCount: Int
    let topRepositories: [StorageAgentRepoSummaryModel]
    let topItems: [StorageAgentItemSummaryModel]
    let confidence: String
    let attributionSources: [String]
    let recommendation: String
}

struct StorageAgentRepoSummaryModel: Decodable, Identifiable, Sendable {
    let repoRoot: String
    let repoName: String
    let artifactBytes: UInt64
    let itemCount: Int

    var id: String { repoRoot }
}

struct StorageAgentItemSummaryModel: Decodable, Identifiable, Sendable {
    let path: String
    let displayName: String
    let kind: String
    let cleanupTier: String
    let sizeBytes: UInt64
    let modifiedMillis: UInt64?

    var id: String { path }
}

struct StorageDuplicateGroupModel: Decodable, Identifiable, Sendable {
    let id: String
    let candidateKey: String
    let detectorKind: StorageDuplicateDetectorKindModel
    let actionability: StorageDuplicateActionabilityModel
    let confidenceBand: StorageDuplicateConfidenceBandModel
    let confirmed: Bool
    let confidenceScore: UInt8
    let fileCount: Int
    let totalBytes: UInt64
    let reclaimableBytes: UInt64
    let paths: [StorageDuplicateItemModel]
    let recommendation: String
    let caveat: String
    let actions: StorageSimilarityActionProjectionModel

    private enum CodingKeys: String, CodingKey {
        case id
        case candidateKey
        case detectorKind
        case actionability
        case confidenceBand
        case confirmed
        case confidenceScore
        case fileCount
        case totalBytes
        case reclaimableBytes
        case paths
        case recommendation
        case caveat
        case actions
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        candidateKey = try container.decode(String.self, forKey: .candidateKey)
        confirmed = try container.decode(Bool.self, forKey: .confirmed)
        confidenceScore = try container.decode(UInt8.self, forKey: .confidenceScore)
        detectorKind =
            try container.decodeIfPresent(StorageDuplicateDetectorKindModel.self, forKey: .detectorKind)
            ?? (confirmed ? .exact : .binarySimilarity)
        actionability =
            try container.decodeIfPresent(StorageDuplicateActionabilityModel.self, forKey: .actionability)
            ?? (confirmed ? .cleanableExact : .reviewOnly)
        confidenceBand =
            try container.decodeIfPresent(StorageDuplicateConfidenceBandModel.self, forKey: .confidenceBand)
            ?? StorageDuplicateConfidenceBandModel.band(confirmed: confirmed, score: confidenceScore)
        fileCount = try container.decode(Int.self, forKey: .fileCount)
        totalBytes = try container.decode(UInt64.self, forKey: .totalBytes)
        reclaimableBytes = try container.decode(UInt64.self, forKey: .reclaimableBytes)
        paths = try container.decodeIfPresent([StorageDuplicateItemModel].self, forKey: .paths) ?? []
        recommendation = try container.decodeIfPresent(String.self, forKey: .recommendation) ?? ""
        caveat = try container.decodeIfPresent(String.self, forKey: .caveat) ?? ""
        actions =
            try container.decodeIfPresent(StorageSimilarityActionProjectionModel.self, forKey: .actions)
            ?? StorageSimilarityActionProjectionModel.defaultForDuplicate(
                actionability: actionability,
                itemCount: paths.count
            )
    }
}

struct StorageSimilarityActionProjectionModel: Decodable, Equatable, Sendable {
    let canReveal: Bool
    let canQuickLook: Bool
    let canStageCleanup: Bool
    let requiresManualReview: Bool
    let blockReason: String?

    static func defaultForDuplicate(
        actionability: StorageDuplicateActionabilityModel,
        itemCount: Int
    ) -> StorageSimilarityActionProjectionModel {
        let canStageCleanup = actionability == .cleanableExact && itemCount >= 2
        return StorageSimilarityActionProjectionModel(
            canReveal: itemCount > 0,
            canQuickLook: itemCount > 0,
            canStageCleanup: canStageCleanup,
            requiresManualReview: true,
            blockReason: canStageCleanup
                ? nil
                : "Similarity detector output is review-only; automatic cleanup staging is disabled."
        )
    }

    static func defaultForRedundancy(itemCount: Int) -> StorageSimilarityActionProjectionModel {
        StorageSimilarityActionProjectionModel(
            canReveal: itemCount > 0,
            canQuickLook: itemCount > 0,
            canStageCleanup: false,
            requiresManualReview: true,
            blockReason: "Redundancy groups require manual review before cleanup."
        )
    }
}

enum StorageDuplicateDetectorKindModel: String, Decodable, Sendable {
    case exact
    case imageSimilarity = "image_similarity"
    case textSimilarity = "text_similarity"
    case documentSimilarity = "document_similarity"
    case videoSimilarity = "video_similarity"
    case binarySimilarity = "binary_similarity"
    case unknown

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let rawValue = try container.decode(String.self)
        self = StorageDuplicateDetectorKindModel(rawValue: rawValue) ?? .unknown
    }
}

enum StorageDuplicateActionabilityModel: String, Decodable, Sendable {
    case cleanableExact = "cleanable_exact"
    case reviewOnly = "review_only"
    case informational
    case unknown

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let rawValue = try container.decode(String.self)
        self = StorageDuplicateActionabilityModel(rawValue: rawValue) ?? .unknown
    }
}

enum StorageDuplicateConfidenceBandModel: String, Decodable, Sendable {
    case confirmed
    case high
    case medium
    case low
    case unknown

    static func band(confirmed: Bool, score: UInt8) -> StorageDuplicateConfidenceBandModel {
        if confirmed {
            return .confirmed
        }
        if score >= 80 {
            return .high
        }
        if score >= 60 {
            return .medium
        }
        return .low
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let rawValue = try container.decode(String.self)
        self = StorageDuplicateConfidenceBandModel(rawValue: rawValue) ?? .unknown
    }
}

struct StorageDuplicateItemModel: Decodable, Identifiable, Sendable {
    let path: String
    let displayName: String
    let sizeBytes: UInt64
    let modifiedMillis: UInt64?
    let cleanupTier: String
    let safety: String

    var id: String { path }
}

struct StorageRedundancyGroupModel: Decodable, Identifiable, Sendable {
    let id: String
    let redundancyClass: String
    let title: String
    let totalBytes: UInt64
    let reclaimableBytes: UInt64
    let itemCount: Int
    let confidenceScore: UInt8
    let safety: String
    let recommendation: String
    let caveat: String
    let evidence: [String]
    let actions: StorageSimilarityActionProjectionModel
    let items: [StorageRedundancyItemModel]

    private enum CodingKeys: String, CodingKey {
        case id
        case redundancyClass
        case title
        case totalBytes
        case reclaimableBytes
        case itemCount
        case confidenceScore
        case safety
        case recommendation
        case caveat
        case evidence
        case actions
        case items
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        redundancyClass = try container.decode(String.self, forKey: .redundancyClass)
        title = try container.decode(String.self, forKey: .title)
        totalBytes = try container.decode(UInt64.self, forKey: .totalBytes)
        reclaimableBytes = try container.decode(UInt64.self, forKey: .reclaimableBytes)
        itemCount = try container.decode(Int.self, forKey: .itemCount)
        confidenceScore = try container.decode(UInt8.self, forKey: .confidenceScore)
        safety = try container.decode(String.self, forKey: .safety)
        recommendation = try container.decodeIfPresent(String.self, forKey: .recommendation) ?? ""
        caveat = try container.decodeIfPresent(String.self, forKey: .caveat) ?? ""
        evidence = try container.decodeIfPresent([String].self, forKey: .evidence) ?? []
        items = try container.decodeIfPresent([StorageRedundancyItemModel].self, forKey: .items) ?? []
        actions =
            try container.decodeIfPresent(StorageSimilarityActionProjectionModel.self, forKey: .actions)
            ?? StorageSimilarityActionProjectionModel.defaultForRedundancy(itemCount: items.count)
    }
}

struct StorageRedundancyItemModel: Decodable, Identifiable, Sendable {
    let path: String
    let displayName: String
    let kind: String
    let sizeBytes: UInt64
    let logicalBytes: UInt64
    let physicalBytes: UInt64
    let cleanupTier: String
    let safety: String
    let role: String

    var id: String { "\(role)|\(path)" }
}

struct StorageAppFootprintModel: Decodable, Identifiable, Sendable {
    let id: String
    let appName: String
    let bundleIdentifier: String?
    let totalBytes: UInt64
    let componentCount: Int
    let cleanupTier: String
    let safety: String
    let confidenceScore: UInt8
    let components: [StorageAppFootprintComponentModel]
    let recommendation: String
}

struct StorageAppFootprintComponentModel: Decodable, Identifiable, Sendable {
    let path: String
    let component: String
    let sizeBytes: UInt64
    let cleanupTier: String
    let safety: String

    var id: String { "\(component)|\(path)" }
}

struct StorageSystemDataBucketModel: Decodable, Identifiable, Sendable {
    let id: String
    let title: String
    let category: String
    let sizeBytes: UInt64
    let itemCount: Int
    let cleanupTier: String
    let safety: String
    let explanation: String
    let recommendedAction: String
    let paths: [String]
    let requiresFullDiskAccess: Bool
}

struct StorageTreemapNodeModel: Decodable, Identifiable, Sendable {
    let id: String
    let path: String
    let label: String
    let depth: Int
    let nodeType: String
    let fileType: String
    let colorKey: String
    let sizeBytes: UInt64
    let itemCount: Int
    let children: [StorageTreemapNodeModel]
    let hasMore: Bool
}

struct StorageGrowthDeltaModel: Decodable, Identifiable, Sendable {
    let bucketMillis: UInt64
    let scanMillis: UInt64
    let path: String
    let sourceRoot: String
    let repoRoot: String?
    let repoName: String?
    let gitBranch: String?
    let gitHead: String?
    let kind: String
    let cleanupTier: String
    let previousPhysicalBytes: UInt64
    let currentPhysicalBytes: UInt64
    let deltaBytes: Int64
    let command: String?
    let processTree: String?
    let aiAgentSession: String?
    let writerSource: String?
    let provider: String?
    let sessionId: String?
    let tabName: String?
    let chau7SessionId: String?
    let writerDisplay: String?
    let matchedWriterCount: UInt64?
    let matchedFilesystemEventCount: UInt64?
    let attributionSources: [String]?
    let attributionConfidence: String
    let attributionConfidenceScore: UInt8
    let attributionAmbiguous: Bool
    let attributionSummary: String
    let attributionEvidence: [String]

    var id: String { "\(bucketMillis)|\(path)|\(deltaBytes)" }
}

struct StorageRepositoryInventoryModel: Decodable, Identifiable, Sendable {
    let id: String
    let repoRoot: String
    let repoName: String
    let inventoryCacheStatus: String
    let inventoryFingerprint: String
    let inventoryFingerprintChanged: Bool
    let inventoryLastSeenMillis: UInt64?
    let inventoryLastScanMillis: UInt64?
    let gitBranch: String?
    let gitHead: String?
    let gitRef: String?
    let gitDetachedHead: Bool
    let gitRemoteOriginUrl: String?
    let gitRemoteKey: String?
    let gitRemoteHost: String?
    let gitRemoteOwner: String?
    let gitRemoteName: String?
    let gitDirtyStatus: String
    let gitDirtyFileCount: UInt64?
    let gitDirtyTruncated: Bool
    let notSeenInLatestScan: Bool
    let cloneGroupCount: UInt64
    let cloneGroupRoots: [String]
    let discoveredRoot: String
    let hasAgentsMd: Bool
    let hasClaudeMd: Bool
    let claudeMdBytes: UInt64?
    let claudeMdDelegationMaxBytes: UInt64
    let claudeMdDelegatesToAgentsMd: Bool
    let agentReadinessScore: UInt8
    let agentReadinessStatus: String
    let agentContractMissingCount: UInt64
    let agentContractCoverage: [StorageAgentContractCoverageModel]
    let agentGuidanceStatus: String
    let agentGuidanceIssueCount: UInt64
    let agentGuidanceIssues: [StorageAgentGuidanceIssueModel]

    private enum CodingKeys: String, CodingKey {
        case id
        case repoRoot
        case repoName
        case inventoryCacheStatus
        case inventoryFingerprint
        case inventoryFingerprintChanged
        case inventoryLastSeenMillis
        case inventoryLastScanMillis
        case gitBranch
        case gitHead
        case gitRef
        case gitDetachedHead
        case gitRemoteOriginUrl
        case gitRemoteKey
        case gitRemoteHost
        case gitRemoteOwner
        case gitRemoteName
        case gitDirtyStatus
        case gitDirtyFileCount
        case gitDirtyTruncated
        case notSeenInLatestScan
        case cloneGroupCount
        case cloneGroupRoots
        case discoveredRoot
        case hasAgentsMd
        case hasClaudeMd
        case claudeMdBytes
        case claudeMdDelegationMaxBytes
        case claudeMdDelegatesToAgentsMd
        case agentReadinessScore
        case agentReadinessStatus
        case agentContractMissingCount
        case agentContractCoverage
        case agentGuidanceStatus
        case agentGuidanceIssueCount
        case agentGuidanceIssues
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        repoRoot = try container.decode(String.self, forKey: .repoRoot)
        repoName = try container.decode(String.self, forKey: .repoName)
        inventoryCacheStatus =
            try container.decodeIfPresent(String.self, forKey: .inventoryCacheStatus) ?? "unknown"
        inventoryFingerprint =
            try container.decodeIfPresent(String.self, forKey: .inventoryFingerprint) ?? ""
        inventoryFingerprintChanged =
            try container.decodeIfPresent(Bool.self, forKey: .inventoryFingerprintChanged) ?? false
        inventoryLastSeenMillis =
            try container.decodeIfPresent(UInt64.self, forKey: .inventoryLastSeenMillis)
        inventoryLastScanMillis =
            try container.decodeIfPresent(UInt64.self, forKey: .inventoryLastScanMillis)
        gitBranch = try container.decodeIfPresent(String.self, forKey: .gitBranch)
        gitHead = try container.decodeIfPresent(String.self, forKey: .gitHead)
        gitRef = try container.decodeIfPresent(String.self, forKey: .gitRef)
        gitDetachedHead = try container.decodeIfPresent(Bool.self, forKey: .gitDetachedHead) ?? false
        gitRemoteOriginUrl = try container.decodeIfPresent(String.self, forKey: .gitRemoteOriginUrl)
        gitRemoteKey = try container.decodeIfPresent(String.self, forKey: .gitRemoteKey)
        gitRemoteHost = try container.decodeIfPresent(String.self, forKey: .gitRemoteHost)
        gitRemoteOwner = try container.decodeIfPresent(String.self, forKey: .gitRemoteOwner)
        gitRemoteName = try container.decodeIfPresent(String.self, forKey: .gitRemoteName)
        gitDirtyStatus = try container.decodeIfPresent(String.self, forKey: .gitDirtyStatus) ?? "unknown"
        gitDirtyFileCount = try container.decodeIfPresent(UInt64.self, forKey: .gitDirtyFileCount)
        gitDirtyTruncated = try container.decodeIfPresent(Bool.self, forKey: .gitDirtyTruncated) ?? false
        notSeenInLatestScan = try container.decodeIfPresent(Bool.self, forKey: .notSeenInLatestScan) ?? false
        cloneGroupCount = try container.decodeIfPresent(UInt64.self, forKey: .cloneGroupCount) ?? 1
        cloneGroupRoots = try container.decodeIfPresent([String].self, forKey: .cloneGroupRoots) ?? []
        discoveredRoot = try container.decode(String.self, forKey: .discoveredRoot)
        hasAgentsMd = try container.decodeIfPresent(Bool.self, forKey: .hasAgentsMd) ?? false
        hasClaudeMd = try container.decodeIfPresent(Bool.self, forKey: .hasClaudeMd) ?? false
        claudeMdBytes = try container.decodeIfPresent(UInt64.self, forKey: .claudeMdBytes)
        claudeMdDelegationMaxBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .claudeMdDelegationMaxBytes) ?? 1_024
        claudeMdDelegatesToAgentsMd =
            try container.decodeIfPresent(Bool.self, forKey: .claudeMdDelegatesToAgentsMd) ?? false
        agentReadinessScore = try container.decodeIfPresent(UInt8.self, forKey: .agentReadinessScore) ?? 0
        agentReadinessStatus = try container.decodeIfPresent(String.self, forKey: .agentReadinessStatus) ?? "unknown"
        agentContractMissingCount =
            try container.decodeIfPresent(UInt64.self, forKey: .agentContractMissingCount) ?? 0
        agentContractCoverage =
            try container.decodeIfPresent([StorageAgentContractCoverageModel].self, forKey: .agentContractCoverage) ?? []
        agentGuidanceStatus = try container.decodeIfPresent(String.self, forKey: .agentGuidanceStatus) ?? "unknown"
        agentGuidanceIssueCount = try container.decodeIfPresent(UInt64.self, forKey: .agentGuidanceIssueCount) ?? 0
        agentGuidanceIssues =
            try container.decodeIfPresent([StorageAgentGuidanceIssueModel].self, forKey: .agentGuidanceIssues) ?? []
    }
}

struct StorageAgentContractCoverageModel: Decodable, Identifiable, Sendable {
    let id: String
    let label: String
    let path: String
    let kind: String
    let status: String
    let severity: String
    let detail: String
    let weight: UInt64
    let earnedWeight: UInt64
    let coveragePercent: UInt8
    let present: Bool
    let tracked: Bool
    let schemaVersion: String?
    let generated: Bool
    let reviewed: Bool
}

struct StorageAgentGuidanceIssueModel: Decodable, Identifiable, Sendable {
    let id: String
    let severity: String
    let title: String
    let detail: String
    let path: String
}

struct StorageRepoFootprintModel: Decodable, Identifiable, Sendable {
    let id: String
    let repoRoot: String
    let repoName: String
    let currentSizeBytes: UInt64
    let artifactBytes: UInt64
    let safeBytes: UInt64
    let rebuildableBytes: UInt64
    let expensiveBytes: UInt64
    let riskyBytes: UInt64
    let rebuildablePercent: Double
    let itemCount: Int
    let topArtifactFolders: [StorageRepoArtifactFolderModel]
    let artifactMix: [StorageRepoArtifactMixModel]
    let duplicateCloneCount: UInt64
    let duplicateCloneRoots: [String]
    let lastWriterProcess: String?
    let lastWriterPid: UInt32?
    let lastBranchTouched: String?
    let growthBytes: Int64?
    let growthWindow: String
    let estimatedRebuildCost: String
    let estimatedRebuildSeconds: UInt64?
    let optimizationSummary: String
    let caveats: [String]

    private enum CodingKeys: String, CodingKey {
        case id
        case repoRoot
        case repoName
        case currentSizeBytes
        case artifactBytes
        case safeBytes
        case rebuildableBytes
        case expensiveBytes
        case riskyBytes
        case rebuildablePercent
        case itemCount
        case topArtifactFolders
        case artifactMix
        case duplicateCloneCount
        case duplicateCloneRoots
        case lastWriterProcess
        case lastWriterPid
        case lastBranchTouched
        case growthBytes
        case growthWindow
        case estimatedRebuildCost
        case estimatedRebuildSeconds
        case optimizationSummary
        case caveats
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        repoRoot = try container.decode(String.self, forKey: .repoRoot)
        repoName = try container.decode(String.self, forKey: .repoName)
        currentSizeBytes = try container.decode(UInt64.self, forKey: .currentSizeBytes)
        artifactBytes = try container.decode(UInt64.self, forKey: .artifactBytes)
        safeBytes = try container.decodeIfPresent(UInt64.self, forKey: .safeBytes) ?? 0
        rebuildableBytes = try container.decodeIfPresent(UInt64.self, forKey: .rebuildableBytes) ?? 0
        expensiveBytes = try container.decodeIfPresent(UInt64.self, forKey: .expensiveBytes) ?? 0
        riskyBytes = try container.decodeIfPresent(UInt64.self, forKey: .riskyBytes) ?? 0
        rebuildablePercent = try container.decodeIfPresent(Double.self, forKey: .rebuildablePercent) ?? 0
        itemCount = try container.decode(Int.self, forKey: .itemCount)
        topArtifactFolders =
            try container.decodeIfPresent([StorageRepoArtifactFolderModel].self, forKey: .topArtifactFolders) ?? []
        artifactMix = try container.decodeIfPresent([StorageRepoArtifactMixModel].self, forKey: .artifactMix) ?? []
        duplicateCloneCount = try container.decodeIfPresent(UInt64.self, forKey: .duplicateCloneCount) ?? 1
        duplicateCloneRoots = try container.decodeIfPresent([String].self, forKey: .duplicateCloneRoots) ?? []
        lastWriterProcess = try container.decodeIfPresent(String.self, forKey: .lastWriterProcess)
        lastWriterPid = try container.decodeIfPresent(UInt32.self, forKey: .lastWriterPid)
        lastBranchTouched = try container.decodeIfPresent(String.self, forKey: .lastBranchTouched)
        growthBytes = try container.decodeIfPresent(Int64.self, forKey: .growthBytes)
        growthWindow = try container.decode(String.self, forKey: .growthWindow)
        estimatedRebuildCost = try container.decodeIfPresent(String.self, forKey: .estimatedRebuildCost) ?? "Unknown"
        estimatedRebuildSeconds = try container.decodeIfPresent(UInt64.self, forKey: .estimatedRebuildSeconds)
        optimizationSummary =
            try container.decodeIfPresent(String.self, forKey: .optimizationSummary)
            ?? "\(repoName) has \(ByteCountFormatter.string(fromByteCount: Int64(artifactBytes), countStyle: .file)) attributed artifacts."
        caveats = try container.decode([String].self, forKey: .caveats)
    }
}

struct StorageRepoArtifactFolderModel: Decodable, Identifiable, Sendable {
    let path: String
    let displayName: String
    let kind: String
    let cleanupTier: String
    let sizeBytes: UInt64
    let cleanupAllowed: Bool
    let cleanupBlockers: [String]
    let defaultCleanupAction: String
    let sizeTruncated: Bool
    let cloudPlaceholder: Bool
    let hasHardlinks: Bool
    let hardlinkCount: UInt64

    var id: String { path }

    private enum CodingKeys: String, CodingKey {
        case path
        case displayName
        case kind
        case cleanupTier
        case sizeBytes
        case cleanupAllowed
        case cleanupBlockers
        case defaultCleanupAction
        case sizeTruncated
        case cloudPlaceholder
        case hasHardlinks
        case hardlinkCount
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        path = try container.decode(String.self, forKey: .path)
        displayName = try container.decode(String.self, forKey: .displayName)
        kind = try container.decode(String.self, forKey: .kind)
        cleanupTier = try container.decode(String.self, forKey: .cleanupTier)
        sizeBytes = try container.decode(UInt64.self, forKey: .sizeBytes)
        cleanupAllowed = try container.decodeIfPresent(Bool.self, forKey: .cleanupAllowed) ?? false
        cleanupBlockers = try container.decodeIfPresent([String].self, forKey: .cleanupBlockers)
            ?? ["Missing cleanup policy in repository artifact projection."]
        defaultCleanupAction = try container.decodeIfPresent(String.self, forKey: .defaultCleanupAction) ?? "manual_review"
        sizeTruncated = try container.decodeIfPresent(Bool.self, forKey: .sizeTruncated) ?? true
        cloudPlaceholder = try container.decodeIfPresent(Bool.self, forKey: .cloudPlaceholder) ?? false
        hasHardlinks = try container.decodeIfPresent(Bool.self, forKey: .hasHardlinks) ?? false
        hardlinkCount = try container.decodeIfPresent(UInt64.self, forKey: .hardlinkCount) ?? 1
    }
}

struct StorageRepoArtifactMixModel: Decodable, Identifiable, Sendable {
    let kind: String
    let label: String
    let itemCount: Int
    let bytes: UInt64
    let cleanupTier: String
    let rebuildCommand: String?
    let estimatedRebuildCost: String
    let estimatedRebuildSeconds: UInt64?

    var id: String { kind }
}

struct StorageHygieneItemModel: Decodable, Identifiable, Sendable {
    let id: String
    let path: String
    let displayName: String
    let kind: String
    let storageRole: String
    let gitStatus: String
    let safety: String
    let cleanupTier: String
    let sizeBytes: UInt64
    let logicalBytes: UInt64
    let physicalBytes: UInt64
    let byteAccounting: String
    let sparseOrShared: Bool
    let hardlinkCount: UInt64
    let hasHardlinks: Bool
    let cloudPlaceholder: Bool
    let protectedPath: Bool
    let sizeTruncated: Bool
    let modifiedMillis: UInt64?
    let ageDays: UInt64?
    let accessedMillis: UInt64?
    let accessAgeDays: UInt64?
    let cold: Bool
    let stale: Bool
    let reason: String
    let recommendation: String
    let nextStep: String
    let commandHint: String
    let rebuildCommand: String?
    let estimatedRebuildCost: String
    let estimatedRebuildSeconds: UInt64?
    let cleanupConsequence: String
    let semanticCategory: String
    let taxonomySource: String
    let rebuildability: String
    let manifestEvidence: [String]
    let evidence: [String]
    let cleanupAllowed: Bool
    let cleanupBlockers: [String]
    let defaultCleanupAction: String
    /// Composite reclaim-recommendation score from the engine; 0 on payloads
    /// that predate the score column.
    let recommendationScore: Double
    let attribution: StorageArtifactAttributionModel

    private enum CodingKeys: String, CodingKey {
        case id
        case path
        case displayName
        case kind
        case storageRole
        case gitStatus
        case safety
        case cleanupTier
        case sizeBytes
        case logicalBytes
        case physicalBytes
        case byteAccounting
        case sparseOrShared
        case hardlinkCount
        case hasHardlinks
        case cloudPlaceholder
        case protectedPath
        case sizeTruncated
        case modifiedMillis
        case ageDays
        case accessedMillis
        case accessAgeDays
        case cold
        case stale
        case reason
        case recommendation
        case nextStep
        case commandHint
        case rebuildCommand
        case estimatedRebuildCost
        case estimatedRebuildSeconds
        case cleanupConsequence
        case semanticCategory
        case taxonomySource
        case rebuildability
        case manifestEvidence
        case evidence
        case cleanupAllowed
        case cleanupBlockers
        case defaultCleanupAction
        case recommendationScore
        case attribution
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        path = try container.decode(String.self, forKey: .path)
        displayName = try container.decode(String.self, forKey: .displayName)
        kind = try container.decode(String.self, forKey: .kind)
        safety = try container.decode(String.self, forKey: .safety)
        cleanupTier = try container.decode(String.self, forKey: .cleanupTier)
        sizeBytes = try container.decode(UInt64.self, forKey: .sizeBytes)
        logicalBytes = try container.decodeIfPresent(UInt64.self, forKey: .logicalBytes) ?? sizeBytes
        physicalBytes = try container.decodeIfPresent(UInt64.self, forKey: .physicalBytes) ?? sizeBytes
        byteAccounting = try container.decodeIfPresent(String.self, forKey: .byteAccounting) ?? "legacy size estimate"
        sparseOrShared = try container.decodeIfPresent(Bool.self, forKey: .sparseOrShared) ?? false
        hardlinkCount = try container.decodeIfPresent(UInt64.self, forKey: .hardlinkCount) ?? 1
        hasHardlinks = try container.decodeIfPresent(Bool.self, forKey: .hasHardlinks) ?? false
        cloudPlaceholder = try container.decodeIfPresent(Bool.self, forKey: .cloudPlaceholder) ?? false
        protectedPath = try container.decodeIfPresent(Bool.self, forKey: .protectedPath) ?? false
        sizeTruncated = try container.decode(Bool.self, forKey: .sizeTruncated)
        modifiedMillis = try container.decodeIfPresent(UInt64.self, forKey: .modifiedMillis)
        ageDays = try container.decodeIfPresent(UInt64.self, forKey: .ageDays)
        accessedMillis = try container.decodeIfPresent(UInt64.self, forKey: .accessedMillis)
        accessAgeDays = try container.decodeIfPresent(UInt64.self, forKey: .accessAgeDays)
        cold = try container.decodeIfPresent(Bool.self, forKey: .cold) ?? false
        stale = try container.decode(Bool.self, forKey: .stale)
        reason = try container.decode(String.self, forKey: .reason)
        recommendation = try container.decode(String.self, forKey: .recommendation)
        commandHint = try container.decode(String.self, forKey: .commandHint)
        rebuildCommand = try container.decodeIfPresent(String.self, forKey: .rebuildCommand)
        estimatedRebuildCost = try container.decodeIfPresent(String.self, forKey: .estimatedRebuildCost) ?? "Unknown"
        estimatedRebuildSeconds = try container.decodeIfPresent(UInt64.self, forKey: .estimatedRebuildSeconds)
        cleanupConsequence = try container.decodeIfPresent(String.self, forKey: .cleanupConsequence) ?? recommendation
        semanticCategory = try container.decodeIfPresent(String.self, forKey: .semanticCategory)
            ?? StorageHygieneItemModel.legacyStorageRole(kind: kind)
        taxonomySource = try container.decodeIfPresent(String.self, forKey: .taxonomySource) ?? "legacy"
        rebuildability = try container.decodeIfPresent(String.self, forKey: .rebuildability) ?? "unknown"
        manifestEvidence = try container.decodeIfPresent([String].self, forKey: .manifestEvidence) ?? []
        attribution = try container.decode(StorageArtifactAttributionModel.self, forKey: .attribution)

        storageRole = try container.decodeIfPresent(String.self, forKey: .storageRole)
            ?? StorageHygieneItemModel.legacyStorageRole(kind: kind)
        gitStatus = try container.decodeIfPresent(String.self, forKey: .gitStatus)
            ?? (attribution.repoRoot == nil ? "outside-git" : "repo-linked")
        nextStep = try container.decodeIfPresent(String.self, forKey: .nextStep) ?? recommendation
        evidence = try container.decodeIfPresent([String].self, forKey: .evidence) ?? attribution.notes
        cleanupAllowed = try container.decodeIfPresent(Bool.self, forKey: .cleanupAllowed) ?? true
        cleanupBlockers = try container.decodeIfPresent([String].self, forKey: .cleanupBlockers) ?? []
        defaultCleanupAction = try container.decodeIfPresent(String.self, forKey: .defaultCleanupAction) ?? "trash"
        recommendationScore = try container.decodeIfPresent(Double.self, forKey: .recommendationScore) ?? 0
    }

    private static func legacyStorageRole(kind: String) -> String {
        switch kind {
        case "log-file", "logs":
            return "log"
        case "rust-build", "swift-build", "xcode-derived-data", "coverage-output", "build-output", "next-build",
            "xcode-archives", "test-output":
            return "build-artifact"
        case let value where value.contains("cache"):
            return "cache"
        case "node-dependencies", "xcode-source-packages":
            return "dependency-tree"
        case "python-environment", "docker-storage", "xcode-simulator-runtime":
            return "environment"
        case "macos-app-bundle":
            return "application"
        case "app-support-data", "app-container", "app-launch-item", "app-preferences", "app-receipt":
            return "app-data"
        case "ios-backup", "mail-attachments", "message-attachments", "local-snapshot":
            return "system-data"
        case "temporary-output":
            return "temporary"
        case "cold-file":
            return "cold-file"
        default:
            return "artifact"
        }
    }
}

struct StorageArtifactAttributionModel: Decodable, Sendable {
    let repoRoot: String?
    let repoName: String?
    let gitBranch: String?
    let gitHead: String?
    let command: String?
    let processTree: String?
    let aiAgentSession: String?
    let provider: String?
    let sessionId: String?
    let tabName: String?
    let chau7SessionId: String?
    let writerDisplay: String?
    let confidence: String
    let notes: [String]
}

struct StorageSkippedRootModel: Decodable, Identifiable, Sendable {
    let path: String
    let reason: String

    var id: String { "\(path)|\(reason)" }
}

struct StorageSourceCoverageModel: Decodable, Identifiable, Sendable {
    let id: String
    let label: String
    let kind: String
    let path: String
    let status: String
    let permissionState: String
    let gapKind: String
    let detail: String
    let localBytes: UInt64?
    let logicalBytes: UInt64?
    let reclaimableBytes: UInt64?
    let cloudPlaceholder: Bool
    let network: Bool
    let protected: Bool
    let scanned: Bool

    private enum CodingKeys: String, CodingKey {
        case id
        case label
        case kind
        case path
        case status
        case permissionState
        case gapKind
        case detail
        case localBytes
        case logicalBytes
        case reclaimableBytes
        case cloudPlaceholder
        case network
        case protected
        case scanned
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        label = try container.decode(String.self, forKey: .label)
        kind = try container.decode(String.self, forKey: .kind)
        path = try container.decode(String.self, forKey: .path)
        status = try container.decode(String.self, forKey: .status)
        permissionState = try container.decode(String.self, forKey: .permissionState)
        gapKind = try container.decodeIfPresent(String.self, forKey: .gapKind)
            ?? StorageSourceCoverageModel.legacyGapKind(
                status: status,
                permissionState: permissionState,
                kind: kind,
                cloudPlaceholder: (try container.decodeIfPresent(Bool.self, forKey: .cloudPlaceholder) ?? false)
            )
        detail = try container.decode(String.self, forKey: .detail)
        localBytes = try container.decodeIfPresent(UInt64.self, forKey: .localBytes)
        logicalBytes = try container.decodeIfPresent(UInt64.self, forKey: .logicalBytes)
        reclaimableBytes = try container.decodeIfPresent(UInt64.self, forKey: .reclaimableBytes)
        cloudPlaceholder = try container.decodeIfPresent(Bool.self, forKey: .cloudPlaceholder) ?? false
        network = try container.decodeIfPresent(Bool.self, forKey: .network) ?? false
        protected = try container.decodeIfPresent(Bool.self, forKey: .protected) ?? false
        scanned = try container.decode(Bool.self, forKey: .scanned)
    }

    private static func legacyGapKind(
        status: String,
        permissionState: String,
        kind: String,
        cloudPlaceholder: Bool
    ) -> String {
        if permissionState == "needs_full_disk_access" { return "permission-denied" }
        if status == "unavailable" { return "unavailable" }
        if status == "skipped" { return "skipped" }
        if kind == "cloud" || cloudPlaceholder { return "cloud-backed" }
        if status == "partial" { return "partial" }
        return "covered"
    }
}

struct StorageRepositoryInventoryCoverageModel: Decodable, Identifiable, Sendable {
    let id: String
    let label: String
    let path: String
    let status: String
    let permissionState: String
    let detail: String
    let repositoryCount: UInt64
    let scannedDirectoryCount: UInt64
    let skippedDirectoryCount: UInt64
    let truncated: Bool
    let scanned: Bool
}

struct StorageVolumeStateModel: Decodable, Identifiable, Sendable {
    let path: String
    let deviceId: UInt64
    let filesystemType: String
    let totalBytes: UInt64
    let freeNowBytes: UInt64
    let availableBytes: UInt64
    let purgeableBytesEstimate: UInt64
    var importantUsageAvailableBytes: UInt64?
    var opportunisticUsageAvailableBytes: UInt64?
    let detail: String

    var id: String { "\(deviceId)|\(path)" }
}

/// Daily growth rate for one repository or source root. Lenient decoding:
/// every field is defaulted so later payload enrichments never sink a decode.
struct StorageGrowthRateModel: Decodable, Identifiable, Sendable {
    let scope: String
    let scopeKind: String
    let repoName: String?
    let windowDays: UInt64
    let totalDeltaBytes: Int64
    let dailyRateBytes: Int64
    let dailyRateLowerBytes: Int64
    let dailyRateUpperBytes: Int64
    let trend: String
    let confidence: String
    let volatilityPercent: UInt64
    let seasonalPattern: String
    let seasonalPeakDailyBytes: Int64
    let dayBucketCount: UInt64

    var id: String { "\(scopeKind)|\(scope)" }

    private enum CodingKeys: String, CodingKey {
        case scope
        case scopeKind
        case repoName
        case windowDays
        case totalDeltaBytes
        case dailyRateBytes
        case dailyRateLowerBytes
        case dailyRateUpperBytes
        case trend
        case confidence
        case volatilityPercent
        case seasonalPattern
        case seasonalPeakDailyBytes
        case dayBucketCount
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        scope = try container.decodeIfPresent(String.self, forKey: .scope) ?? ""
        scopeKind = try container.decodeIfPresent(String.self, forKey: .scopeKind) ?? "source_root"
        repoName = try container.decodeIfPresent(String.self, forKey: .repoName)
        windowDays = try container.decodeIfPresent(UInt64.self, forKey: .windowDays) ?? 30
        totalDeltaBytes = try container.decodeIfPresent(Int64.self, forKey: .totalDeltaBytes) ?? 0
        dailyRateBytes = try container.decodeIfPresent(Int64.self, forKey: .dailyRateBytes) ?? 0
        dailyRateLowerBytes = try container.decodeIfPresent(Int64.self, forKey: .dailyRateLowerBytes) ?? dailyRateBytes
        dailyRateUpperBytes = try container.decodeIfPresent(Int64.self, forKey: .dailyRateUpperBytes) ?? dailyRateBytes
        trend = try container.decodeIfPresent(String.self, forKey: .trend) ?? "steady"
        confidence = try container.decodeIfPresent(String.self, forKey: .confidence) ?? "low"
        volatilityPercent = try container.decodeIfPresent(UInt64.self, forKey: .volatilityPercent) ?? 0
        seasonalPattern = try container.decodeIfPresent(String.self, forKey: .seasonalPattern) ?? "unknown"
        seasonalPeakDailyBytes = try container.decodeIfPresent(Int64.self, forKey: .seasonalPeakDailyBytes) ?? dailyRateBytes
        dayBucketCount = try container.decodeIfPresent(UInt64.self, forKey: .dayBucketCount) ?? 0
    }
}

struct StorageGrowthForecastModel: Decodable, Identifiable, Sendable {
    let volumePath: String
    let freeNowBytes: UInt64
    let availableBytes: UInt64
    let purgeableBytesEstimate: UInt64
    let importantUsageAvailableBytes: UInt64?
    let opportunisticUsageAvailableBytes: UInt64?
    let effectiveAvailableBytes: UInt64
    let dailyRateBytes: Int64
    let dailyRateLowerBytes: Int64
    let dailyRateUpperBytes: Int64
    let daysToFull: Double
    let daysToFullLowerBound: Double
    let daysToFullUpperBound: Double
    let daysToEffectiveFull: Double
    let daysToAvailableFull: Double
    let purgeableCushionDays: Double
    let cloudDailyRateBytes: Int64
    let cloudGrowthSharePercent: UInt64
    let volatilityPercent: UInt64
    let seasonalPattern: String
    let seasonalPeakDailyBytes: Int64
    let confidence: String
    let forecastNotes: [String]

    var id: String { volumePath }

    private enum CodingKeys: String, CodingKey {
        case volumePath
        case freeNowBytes
        case availableBytes
        case purgeableBytesEstimate
        case importantUsageAvailableBytes
        case opportunisticUsageAvailableBytes
        case effectiveAvailableBytes
        case dailyRateBytes
        case dailyRateLowerBytes
        case dailyRateUpperBytes
        case daysToFull
        case daysToFullLowerBound
        case daysToFullUpperBound
        case daysToEffectiveFull
        case daysToAvailableFull
        case purgeableCushionDays
        case cloudDailyRateBytes
        case cloudGrowthSharePercent
        case volatilityPercent
        case seasonalPattern
        case seasonalPeakDailyBytes
        case confidence
        case forecastNotes
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        volumePath = try container.decodeIfPresent(String.self, forKey: .volumePath) ?? ""
        freeNowBytes = try container.decodeIfPresent(UInt64.self, forKey: .freeNowBytes) ?? 0
        availableBytes = try container.decodeIfPresent(UInt64.self, forKey: .availableBytes) ?? freeNowBytes
        purgeableBytesEstimate = try container.decodeIfPresent(UInt64.self, forKey: .purgeableBytesEstimate) ?? 0
        importantUsageAvailableBytes = try container.decodeIfPresent(UInt64.self, forKey: .importantUsageAvailableBytes)
        opportunisticUsageAvailableBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .opportunisticUsageAvailableBytes)
        effectiveAvailableBytes =
            try container.decodeIfPresent(UInt64.self, forKey: .effectiveAvailableBytes)
                ?? importantUsageAvailableBytes ?? freeNowBytes
        dailyRateBytes = try container.decodeIfPresent(Int64.self, forKey: .dailyRateBytes) ?? 0
        dailyRateLowerBytes = try container.decodeIfPresent(Int64.self, forKey: .dailyRateLowerBytes) ?? dailyRateBytes
        dailyRateUpperBytes = try container.decodeIfPresent(Int64.self, forKey: .dailyRateUpperBytes) ?? dailyRateBytes
        daysToFull = try container.decodeIfPresent(Double.self, forKey: .daysToFull) ?? 0
        daysToFullLowerBound = try container.decodeIfPresent(Double.self, forKey: .daysToFullLowerBound) ?? daysToFull
        daysToFullUpperBound = try container.decodeIfPresent(Double.self, forKey: .daysToFullUpperBound) ?? daysToFull
        daysToEffectiveFull = try container.decodeIfPresent(Double.self, forKey: .daysToEffectiveFull) ?? daysToFull
        daysToAvailableFull = try container.decodeIfPresent(Double.self, forKey: .daysToAvailableFull) ?? daysToFull
        purgeableCushionDays = try container.decodeIfPresent(Double.self, forKey: .purgeableCushionDays) ?? 0
        cloudDailyRateBytes = try container.decodeIfPresent(Int64.self, forKey: .cloudDailyRateBytes) ?? 0
        cloudGrowthSharePercent = try container.decodeIfPresent(UInt64.self, forKey: .cloudGrowthSharePercent) ?? 0
        volatilityPercent = try container.decodeIfPresent(UInt64.self, forKey: .volatilityPercent) ?? 0
        seasonalPattern = try container.decodeIfPresent(String.self, forKey: .seasonalPattern) ?? "unknown"
        seasonalPeakDailyBytes =
            try container.decodeIfPresent(Int64.self, forKey: .seasonalPeakDailyBytes) ?? dailyRateBytes
        confidence = try container.decodeIfPresent(String.self, forKey: .confidence) ?? "low"
        forecastNotes = try container.decodeIfPresent([String].self, forKey: .forecastNotes) ?? []
    }
}

struct StorageScanDiffEntryModel: Decodable, Identifiable, Sendable {
    let path: String
    let displayName: String
    let sourceRoot: String
    let repoRoot: String?
    let kind: String
    let cleanupTier: String
    let previousCleanupTier: String
    let physicalBytes: UInt64
    let deltaBytes: Int64
    let scanMillis: UInt64

    var id: String { path }

    private enum CodingKeys: String, CodingKey {
        case path
        case displayName
        case sourceRoot
        case repoRoot
        case kind
        case cleanupTier
        case previousCleanupTier
        case physicalBytes
        case deltaBytes
        case scanMillis
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        path = try container.decodeIfPresent(String.self, forKey: .path) ?? ""
        displayName = try container.decodeIfPresent(String.self, forKey: .displayName)
            ?? (path as NSString).lastPathComponent
        sourceRoot = try container.decodeIfPresent(String.self, forKey: .sourceRoot) ?? ""
        repoRoot = try container.decodeIfPresent(String.self, forKey: .repoRoot)
        kind = try container.decodeIfPresent(String.self, forKey: .kind) ?? ""
        cleanupTier = try container.decodeIfPresent(String.self, forKey: .cleanupTier) ?? ""
        previousCleanupTier = try container.decodeIfPresent(String.self, forKey: .previousCleanupTier) ?? ""
        physicalBytes = try container.decodeIfPresent(UInt64.self, forKey: .physicalBytes) ?? 0
        deltaBytes = try container.decodeIfPresent(Int64.self, forKey: .deltaBytes) ?? 0
        scanMillis = try container.decodeIfPresent(UInt64.self, forKey: .scanMillis) ?? 0
    }
}

struct StorageScanDiffModel: Decodable, Sendable {
    let latestScanMillis: UInt64
    let appearedCount: UInt64
    let appearedTotalBytes: UInt64
    let appeared: [StorageScanDiffEntryModel]
    let tierChangedCount: UInt64
    let tierChanged: [StorageScanDiffEntryModel]
    let disappeared: [StorageScanDiffEntryModel]
    let disappearedNote: String

    private enum CodingKeys: String, CodingKey {
        case latestScanMillis
        case appearedCount
        case appearedTotalBytes
        case appeared
        case tierChangedCount
        case tierChanged
        case disappeared
        case disappearedNote
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        latestScanMillis = try container.decodeIfPresent(UInt64.self, forKey: .latestScanMillis) ?? 0
        appearedCount = try container.decodeIfPresent(UInt64.self, forKey: .appearedCount) ?? 0
        appearedTotalBytes = try container.decodeIfPresent(UInt64.self, forKey: .appearedTotalBytes) ?? 0
        appeared = try container.decodeIfPresent([StorageScanDiffEntryModel].self, forKey: .appeared) ?? []
        tierChangedCount = try container.decodeIfPresent(UInt64.self, forKey: .tierChangedCount) ?? 0
        tierChanged = try container.decodeIfPresent([StorageScanDiffEntryModel].self, forKey: .tierChanged) ?? []
        disappeared = try container.decodeIfPresent([StorageScanDiffEntryModel].self, forKey: .disappeared) ?? []
        disappearedNote = try container.decodeIfPresent(String.self, forKey: .disappearedNote) ?? ""
    }
}

struct StorageGrowthInsightsModel: Decodable, Sendable {
    let windowDays: UInt64
    let perRepoRates: [StorageGrowthRateModel]
    let perRootRates: [StorageGrowthRateModel]
    let volumeForecasts: [StorageGrowthForecastModel]
    let sinceLastScan: StorageScanDiffModel?

    private enum CodingKeys: String, CodingKey {
        case windowDays
        case perRepoRates
        case perRootRates
        case volumeForecasts
        case sinceLastScan
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        windowDays = try container.decodeIfPresent(UInt64.self, forKey: .windowDays) ?? 30
        perRepoRates = try container.decodeIfPresent([StorageGrowthRateModel].self, forKey: .perRepoRates) ?? []
        perRootRates = try container.decodeIfPresent([StorageGrowthRateModel].self, forKey: .perRootRates) ?? []
        volumeForecasts =
            try container.decodeIfPresent([StorageGrowthForecastModel].self, forKey: .volumeForecasts) ?? []
        sinceLastScan = try container.decodeIfPresent(StorageScanDiffModel.self, forKey: .sinceLastScan)
    }
}

struct StorageColdDataBandModel: Decodable, Identifiable, Sendable {
    let id: String
    let label: String
    let minAgeDays: UInt64
    let maxAgeDays: UInt64?
    let itemCount: UInt64
    let totalBytes: UInt64
    let topItems: [StorageHygieneItemModel]

    private enum CodingKeys: String, CodingKey {
        case id
        case label
        case minAgeDays
        case maxAgeDays
        case itemCount
        case totalBytes
        case topItems
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        minAgeDays = try container.decodeIfPresent(UInt64.self, forKey: .minAgeDays) ?? 0
        id = try container.decodeIfPresent(String.self, forKey: .id) ?? "cold-\(minAgeDays)"
        label = try container.decodeIfPresent(String.self, forKey: .label) ?? "Untouched \(minAgeDays)+ days"
        maxAgeDays = try container.decodeIfPresent(UInt64.self, forKey: .maxAgeDays)
        itemCount = try container.decodeIfPresent(UInt64.self, forKey: .itemCount) ?? 0
        totalBytes = try container.decodeIfPresent(UInt64.self, forKey: .totalBytes) ?? 0
        topItems = try container.decodeIfPresent([StorageHygieneItemModel].self, forKey: .topItems) ?? []
    }
}

struct StorageColdDataModel: Decodable, Sendable {
    let bands: [StorageColdDataBandModel]
    let caveat: String

    private enum CodingKeys: String, CodingKey {
        case bands
        case caveat
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        bands = try container.decodeIfPresent([StorageColdDataBandModel].self, forKey: .bands) ?? []
        caveat = try container.decodeIfPresent(String.self, forKey: .caveat) ?? ""
    }
}

enum StorageVolumeCapacityEnricher {
    static func enrich(_ volumes: [StorageVolumeStateModel]) -> [StorageVolumeStateModel] {
        volumes.map { volume in
            var enriched = volume
            let url = URL(fileURLWithPath: volume.path, isDirectory: true)
            let values = try? url.resourceValues(forKeys: [
                .volumeAvailableCapacityForImportantUsageKey,
                .volumeAvailableCapacityForOpportunisticUsageKey,
            ])
            if let important = values?.volumeAvailableCapacityForImportantUsage {
                enriched.importantUsageAvailableBytes = UInt64(max(important, 0))
            }
            if let opportunistic = values?.volumeAvailableCapacityForOpportunisticUsage {
                enriched.opportunisticUsageAvailableBytes = UInt64(max(opportunistic, 0))
            }
            return enriched
        }
    }
}
