import Foundation

struct StorageHygieneReportModel: Decodable {
    let capturedAtMillis: UInt64
    let scanDurationMillis: UInt64
    let summary: StorageHygieneSummaryModel
    let cleanupTiers: [StorageCleanupTierModel]
    let cleanupRecipes: [StorageCleanupRecipeModel]
    let cleanupBundles: [StorageCleanupBundleModel]
    let budgetGuardrails: StorageBudgetGuardrailsModel
    let agentHygiene: StorageAgentHygieneSummaryModel
    let repositoryInventory: [StorageRepositoryInventoryModel]
    let repoFootprints: [StorageRepoFootprintModel]
    let items: [StorageHygieneItemModel]
    let roots: [String]
    let skippedRoots: [StorageSkippedRootModel]
    let truncated: Bool
    let caveats: [String]

    private enum CodingKeys: String, CodingKey {
        case capturedAtMillis
        case scanDurationMillis
        case summary
        case cleanupTiers
        case cleanupRecipes
        case cleanupBundles
        case budgetGuardrails
        case agentHygiene
        case repositoryInventory
        case repoFootprints
        case items
        case roots
        case skippedRoots
        case truncated
        case caveats
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        capturedAtMillis = try container.decode(UInt64.self, forKey: .capturedAtMillis)
        scanDurationMillis = try container.decode(UInt64.self, forKey: .scanDurationMillis)
        summary = try container.decode(StorageHygieneSummaryModel.self, forKey: .summary)
        cleanupTiers = try container.decode([StorageCleanupTierModel].self, forKey: .cleanupTiers)
        cleanupRecipes = try container.decode([StorageCleanupRecipeModel].self, forKey: .cleanupRecipes)
        cleanupBundles = try container.decode([StorageCleanupBundleModel].self, forKey: .cleanupBundles)
        budgetGuardrails = try container.decode(StorageBudgetGuardrailsModel.self, forKey: .budgetGuardrails)
        agentHygiene = try container.decode(StorageAgentHygieneSummaryModel.self, forKey: .agentHygiene)
        repositoryInventory =
            try container.decodeIfPresent([StorageRepositoryInventoryModel].self, forKey: .repositoryInventory) ?? []
        repoFootprints = try container.decode([StorageRepoFootprintModel].self, forKey: .repoFootprints)
        items = try container.decode([StorageHygieneItemModel].self, forKey: .items)
        roots = try container.decode([String].self, forKey: .roots)
        skippedRoots = try container.decode([StorageSkippedRootModel].self, forKey: .skippedRoots)
        truncated = try container.decode(Bool.self, forKey: .truncated)
        caveats = try container.decode([String].self, forKey: .caveats)
    }
}

struct StorageHygieneBaselineModel: Codable {
    let capturedAtMillis: UInt64
    let repoFootprints: [StorageRepoFootprintBaselineModel]
    let items: [StorageHygieneItemBaselineModel]

    init(report: StorageHygieneReportModel) {
        capturedAtMillis = report.capturedAtMillis
        repoFootprints = report.repoFootprints.map(StorageRepoFootprintBaselineModel.init)
        items = report.items.map(StorageHygieneItemBaselineModel.init)
    }
}

struct StorageRepoFootprintBaselineModel: Codable, Identifiable {
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

struct StorageHygieneItemBaselineModel: Codable, Identifiable {
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
        guard let data = UserDefaults.standard.data(forKey: key) else {
            return nil
        }
        return try? JSONDecoder().decode(StorageHygieneBaselineModel.self, from: data)
    }

    static func save(_ baseline: StorageHygieneBaselineModel) {
        guard let data = try? JSONEncoder().encode(baseline) else {
            return
        }
        UserDefaults.standard.set(data, forKey: key)
    }
}

struct StorageHygieneReportCacheHit {
    let report: StorageHygieneReportModel
    let savedAtMillis: UInt64
    let repositoryCount: Int
}

enum StorageHygieneReportCacheLoadResult {
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
    let reportJson: String
    let repositories: [StorageHygieneRepositoryCacheFingerprint]
}

private struct StorageHygieneRepositoryCacheFingerprint: Codable {
    let repoRoot: String
    let repoName: String
    let gitHead: String?
    let gitRef: String?
    let gitBranch: String?

    init(repository: StorageRepositoryInventoryModel) {
        repoRoot = repository.repoRoot
        repoName = repository.repoName
        gitHead = repository.gitHead
        gitRef = repository.gitRef
        gitBranch = repository.gitBranch
    }
}

enum StorageHygieneReportCacheStore {
    private static let schemaVersion: UInt8 = 2
    private static let fileName = "storage-hygiene-report-cache-v1.json"

    static func loadIfValid(
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32
    ) -> StorageHygieneReportCacheLoadResult {
        guard let url = cacheURL(createDirectory: false) else {
            return .miss("cache directory unavailable")
        }
        guard let data = try? Data(contentsOf: url) else {
            return .miss("cache file missing")
        }
        let decoder = JSONDecoder()
        guard let record = try? decoder.decode(StorageHygieneReportCacheRecord.self, from: data) else {
            return .miss("cache record could not be decoded")
        }
        guard record.schemaVersion == schemaVersion else {
            return .stale("cache schema changed")
        }
        let requestedRoots = normalizedRequestedRoots(roots)
        guard record.requestedRoots == requestedRoots else {
            return .stale("scan roots changed")
        }
        guard record.maxDepth == maxDepth else {
            return .stale("scan max depth changed")
        }
        guard record.limit == limit else {
            return .stale("scan result limit changed")
        }
        let currentScannedRoots = currentScannableRoots(for: roots)
        guard Set(record.scannedRoots) == Set(currentScannedRoots) else {
            return .stale("scannable root set changed")
        }
        let currentRepoRoots = currentRepositoryRoots(
            scanRoots: currentScannedRoots,
            maxDepth: Int(maxDepth)
        )
        let cachedRepoRoots = Set(record.repositories.map(\.repoRoot))
        guard cachedRepoRoots == currentRepoRoots else {
            return .stale("repository set changed")
        }
        guard let reportData = record.reportJson.data(using: .utf8) else {
            return .stale("cached report JSON is invalid")
        }
        let reportDecoder = JSONDecoder()
        reportDecoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let report = try? reportDecoder.decode(StorageHygieneReportModel.self, from: reportData) else {
            return .stale("cached report no longer matches the app model")
        }
        guard record.reportCapturedAtMillis == report.capturedAtMillis else {
            return .stale("cached report metadata mismatch")
        }
        guard let staleReason = staleReason(for: record.repositories) else {
            return .hit(
                StorageHygieneReportCacheHit(
                    report: report,
                    savedAtMillis: record.savedAtMillis,
                    repositoryCount: record.repositories.count
                )
            )
        }
        return .stale(staleReason)
    }

    static func save(
        report: StorageHygieneReportModel,
        rawJSON: String,
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32
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

    private static func cacheURL(createDirectory: Bool) -> URL? {
        guard let baseURL = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first else {
            return nil
        }
        let directory = baseURL.appendingPathComponent("Aetower", isDirectory: true)
        if createDirectory {
            try? FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
        }
        return directory.appendingPathComponent(fileName, isDirectory: false)
    }

    private static func normalizedRequestedRoots(_ roots: [String]) -> [String] {
        let selected = roots.isEmpty ? defaultStorageRoots() : roots
        return normalizedPathSet(Array(selected.prefix(24)))
    }

    private static func currentScannableRoots(for roots: [String]) -> [String] {
        normalizedRequestedRoots(roots).filter { path in
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
                isDirectory.boolValue
            else {
                return false
            }
            let url = URL(fileURLWithPath: path, isDirectory: true)
            let values = try? url.resourceValues(forKeys: [.isSymbolicLinkKey])
            return values?.isSymbolicLink != true
        }
    }

    private static func currentRepositoryRoots(
        scanRoots: [String],
        maxDepth: Int
    ) -> Set<String> {
        var repositories = Set<String>()
        for root in scanRoots {
            collectRepositoryRoots(
                at: URL(fileURLWithPath: root, isDirectory: true),
                depth: 0,
                maxDepth: maxDepth,
                repositories: &repositories
            )
        }
        return repositories
    }

    private static func collectRepositoryRoots(
        at url: URL,
        depth: Int,
        maxDepth: Int,
        repositories: inout Set<String>
    ) {
        guard depth <= maxDepth else { return }
        let dotGit = url.appendingPathComponent(".git", isDirectory: false)
        if FileManager.default.fileExists(atPath: dotGit.path) {
            repositories.insert(normalizedPath(url.path))
        }
        guard depth < maxDepth else { return }
        let contents = (try? FileManager.default.contentsOfDirectory(
            at: url,
            includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
            options: []
        )) ?? []
        for child in contents {
            let name = child.lastPathComponent
            guard !directoryNamesSkippedForRepoDiscovery.contains(name) else { continue }
            let values = try? child.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard values?.isDirectory == true, values?.isSymbolicLink != true else { continue }
            collectRepositoryRoots(
                at: child,
                depth: depth + 1,
                maxDepth: maxDepth,
                repositories: &repositories
            )
        }
    }

    private static let directoryNamesSkippedForRepoDiscovery: Set<String> = [
        ".git",
        ".build",
        ".cache",
        ".gradle",
        ".next",
        ".swiftpm",
        ".venv",
        "DerivedData",
        "build",
        "coverage",
        "dist",
        "node_modules",
        "target",
    ]

    private static func defaultStorageRoots() -> [String] {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        guard !home.isEmpty else { return [] }
        return [
            "Repositories",
            "Downloads/Repositories",
            "Developer",
            "Projects",
            ".claude",
            ".codex",
            ".cursor",
            ".aider",
            "Library/Developer/Xcode/DerivedData",
            "Library/Caches/org.swift.swiftpm",
            "Library/Caches/com.apple.dt.Xcode",
        ].map { "\(home)/\($0)" }
    }

    private static func normalizedPathSet(_ paths: [String]) -> [String] {
        Array(Set(paths.map(normalizedPath).filter { !$0.isEmpty })).sorted()
    }

    private static func normalizedPath(_ path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        let expanded: String
        if trimmed == "~" {
            expanded = FileManager.default.homeDirectoryForCurrentUser.path
        } else if trimmed.hasPrefix("~/") {
            expanded = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(trimmed.dropFirst(2)))
                .path
        } else {
            expanded = trimmed
        }
        return URL(fileURLWithPath: expanded, isDirectory: true).standardizedFileURL.path
    }

    private static func staleReason(
        for repositories: [StorageHygieneRepositoryCacheFingerprint]
    ) -> String? {
        for repository in repositories {
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(
                atPath: repository.repoRoot,
                isDirectory: &isDirectory
            ), isDirectory.boolValue else {
                return "repository no longer exists: \(repository.repoName)"
            }
            let currentHead = readGitHeadShortHash(repoRoot: repository.repoRoot)
            if currentHead != repository.gitHead {
                let cached = repository.gitHead ?? "unknown"
                let current = currentHead ?? "unknown"
                return "repository HEAD changed: \(repository.repoName) \(cached) -> \(current)"
            }
        }
        return nil
    }

    private static func readGitHeadShortHash(repoRoot: String) -> String? {
        guard let gitDirectory = gitDirectory(repoRoot: repoRoot) else { return nil }
        let headURL = gitDirectory.appendingPathComponent("HEAD", isDirectory: false)
        guard let head = try? String(contentsOf: headURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !head.isEmpty
        else {
            return nil
        }
        if let reference = head.stripPrefix("ref: ") {
            let refURL = gitDirectory.appendingPathComponent(reference, isDirectory: false)
            if let refHash = try? String(contentsOf: refURL, encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines),
                !refHash.isEmpty {
                return shortHash(refHash)
            }
            return packedRef(reference, gitDirectory: gitDirectory).map(shortHash)
        }
        return shortHash(head)
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

    private static func packedRef(_ reference: String, gitDirectory: URL) -> String? {
        let packedRefsURL = gitDirectory.appendingPathComponent("packed-refs", isDirectory: false)
        guard let content = try? String(contentsOf: packedRefsURL, encoding: .utf8) else {
            return nil
        }
        for line in content.lines where !line.hasPrefix("#") && !line.hasPrefix("^") {
            let parts = line.split(separator: " ", maxSplits: 1).map(String.init)
            if parts.count == 2, parts[1] == reference {
                return parts[0]
            }
        }
        return nil
    }

    private static func shortHash(_ value: String) -> String {
        String(value.prefix(12))
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}

private extension String {
    var lines: [String] {
        components(separatedBy: .newlines)
    }

    func stripPrefix(_ prefix: String) -> String? {
        guard hasPrefix(prefix) else { return nil }
        return String(dropFirst(prefix.count))
    }
}

struct StorageHygieneSummaryModel: Decodable {
    let itemCount: Int
    let totalReclaimableBytes: UInt64
    let safeCandidateCount: Int
    let reviewCandidateCount: Int
    let staleCandidateCount: Int
    let scannedDirectoryCount: UInt64
    let largestItemPath: String?
    let largestItemBytes: UInt64
    let attributedRepoCount: Int
}

struct StorageCleanupTierModel: Decodable, Identifiable {
    let tier: String
    let label: String
    let description: String
    let itemCount: Int
    let bytes: UInt64

    var id: String { tier }
}

struct StorageCleanupRecipeModel: Decodable, Identifiable {
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

struct StorageCleanupBundleModel: Decodable, Identifiable {
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

struct StorageCleanupBundleItemModel: Decodable, Identifiable {
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

    var id: String { path }
}

struct StorageBudgetGuardrailsModel: Decodable {
    let repoGrowthBudgetBytesPerDay: UInt64
    let repoArtifactBudgetBytes: UInt64
    let totalArtifactBudgetBytes: UInt64
    let status: String
    let violations: [StorageBudgetViolationModel]
}

struct StorageBudgetViolationModel: Decodable, Identifiable {
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

struct StorageAgentHygieneSummaryModel: Decodable {
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

struct StorageAgentArtifactSummaryModel: Decodable, Identifiable {
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

struct StorageAgentRepoSummaryModel: Decodable, Identifiable {
    let repoRoot: String
    let repoName: String
    let artifactBytes: UInt64
    let itemCount: Int

    var id: String { repoRoot }
}

struct StorageAgentItemSummaryModel: Decodable, Identifiable {
    let path: String
    let displayName: String
    let kind: String
    let cleanupTier: String
    let sizeBytes: UInt64
    let modifiedMillis: UInt64?

    var id: String { path }
}

struct StorageRepositoryInventoryModel: Decodable, Identifiable {
    let id: String
    let repoRoot: String
    let repoName: String
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

struct StorageAgentContractCoverageModel: Decodable, Identifiable {
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

struct StorageAgentGuidanceIssueModel: Decodable, Identifiable {
    let id: String
    let severity: String
    let title: String
    let detail: String
    let path: String
}

struct StorageRepoFootprintModel: Decodable, Identifiable {
    let id: String
    let repoRoot: String
    let repoName: String
    let currentSizeBytes: UInt64
    let artifactBytes: UInt64
    let itemCount: Int
    let topArtifactFolders: [StorageRepoArtifactFolderModel]
    let lastWriterProcess: String?
    let lastWriterPid: UInt32?
    let lastBranchTouched: String?
    let growthBytes: Int64?
    let growthWindow: String
    let estimatedRebuildCost: String
    let estimatedRebuildSeconds: UInt64?
    let caveats: [String]
}

struct StorageRepoArtifactFolderModel: Decodable, Identifiable {
    let path: String
    let displayName: String
    let kind: String
    let cleanupTier: String
    let sizeBytes: UInt64

    var id: String { path }
}

struct StorageHygieneItemModel: Decodable, Identifiable {
    let id: String
    let path: String
    let displayName: String
    let kind: String
    let safety: String
    let cleanupTier: String
    let sizeBytes: UInt64
    let sizeTruncated: Bool
    let modifiedMillis: UInt64?
    let ageDays: UInt64?
    let stale: Bool
    let reason: String
    let recommendation: String
    let commandHint: String
    let attribution: StorageArtifactAttributionModel
}

struct StorageArtifactAttributionModel: Decodable {
    let repoRoot: String?
    let repoName: String?
    let gitBranch: String?
    let gitHead: String?
    let command: String?
    let processTree: String?
    let aiAgentSession: String?
    let confidence: String
    let notes: [String]
}

struct StorageSkippedRootModel: Decodable, Identifiable {
    let path: String
    let reason: String

    var id: String { "\(path)|\(reason)" }
}
