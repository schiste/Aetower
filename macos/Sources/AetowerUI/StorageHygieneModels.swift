import Foundation

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
    let budgetGuardrails: StorageBudgetGuardrailsModel
    let agentHygiene: StorageAgentHygieneSummaryModel
    let repositoryInventory: [StorageRepositoryInventoryModel]
    let repoFootprints: [StorageRepoFootprintModel]
    let duplicateGroups: [StorageDuplicateGroupModel]
    let appFootprints: [StorageAppFootprintModel]
    let systemDataBuckets: [StorageSystemDataBucketModel]
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
        case budgetGuardrails
        case agentHygiene
        case repositoryInventory
        case repoFootprints
        case duplicateGroups
        case appFootprints
        case systemDataBuckets
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
        cleanupRecipes = try container.decode([StorageCleanupRecipeModel].self, forKey: .cleanupRecipes)
        cleanupBundles = try container.decode([StorageCleanupBundleModel].self, forKey: .cleanupBundles)
        budgetGuardrails = try container.decode(StorageBudgetGuardrailsModel.self, forKey: .budgetGuardrails)
        agentHygiene = try container.decode(StorageAgentHygieneSummaryModel.self, forKey: .agentHygiene)
        repositoryInventory =
            try container.decodeIfPresent([StorageRepositoryInventoryModel].self, forKey: .repositoryInventory) ?? []
        repoFootprints = try container.decode([StorageRepoFootprintModel].self, forKey: .repoFootprints)
        duplicateGroups =
            try container.decodeIfPresent([StorageDuplicateGroupModel].self, forKey: .duplicateGroups) ?? []
        appFootprints = try container.decodeIfPresent([StorageAppFootprintModel].self, forKey: .appFootprints) ?? []
        systemDataBuckets =
            try container.decodeIfPresent([StorageSystemDataBucketModel].self, forKey: .systemDataBuckets) ?? []
        items = try container.decode([StorageHygieneItemModel].self, forKey: .items)
        roots = try container.decode([String].self, forKey: .roots)
        skippedRoots = try container.decode([StorageSkippedRootModel].self, forKey: .skippedRoots)
        sourceCoverage =
            try container.decodeIfPresent([StorageSourceCoverageModel].self, forKey: .sourceCoverage) ?? []
        volumeStates =
            try container.decodeIfPresent([StorageVolumeStateModel].self, forKey: .volumeStates) ?? []
        truncated = try container.decode(Bool.self, forKey: .truncated)
        caveats = try container.decode([String].self, forKey: .caveats)
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

struct StorageHygieneReportCacheHit: Sendable {
    let report: StorageHygieneReportModel
    let savedAtMillis: UInt64
    let repositoryCount: Int
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

private struct StorageHygieneRepositoryCacheFingerprint: Codable {
    let repoRoot: String
    let repoName: String
    let gitHead: String?
    let gitRef: String?
    let gitBranch: String?
    let gitRemoteKey: String?
    let gitDirtyStatus: String
    let gitDirtyFileCount: UInt64?
    let gitDirtyTruncated: Bool
    let gitConfigFingerprint: String?
    let gitIndexFingerprint: String?
    let agentReadinessScore: UInt8
    let agentReadinessStatus: String
    let agentGuidanceIssueCount: UInt64
    let agentContracts: [StorageHygieneContractCacheFingerprint]

    init(repository: StorageRepositoryInventoryModel) {
        repoRoot = repository.repoRoot
        repoName = repository.repoName
        gitHead = repository.gitHead
        gitRef = repository.gitRef
        gitBranch = repository.gitBranch
        gitRemoteKey = repository.gitRemoteKey
        gitDirtyStatus = repository.gitDirtyStatus
        gitDirtyFileCount = repository.gitDirtyFileCount
        gitDirtyTruncated = repository.gitDirtyTruncated
        let gitDirectory = Self.gitDirectory(repoRoot: repository.repoRoot)
        gitConfigFingerprint = Self.fileFingerprint(gitDirectory?.appendingPathComponent("config"))
        gitIndexFingerprint = Self.fileFingerprint(gitDirectory?.appendingPathComponent("index"))
        agentReadinessScore = repository.agentReadinessScore
        agentReadinessStatus = repository.agentReadinessStatus
        agentGuidanceIssueCount = repository.agentGuidanceIssueCount
        agentContracts = Self.agentContractFingerprints(repoRoot: repository.repoRoot)
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
    private static let schemaVersion: UInt8 = 4
    private static let fileName = "storage-hygiene-report-cache-v1.json"
    private static let cacheMaxAgeMillis: UInt64 = 5 * 60 * 1000

    static func loadIfValid(
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32,
        mode: String = "fast_changed_only"
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
        guard record.maxDepth == maxDepth else {
            return .stale("scan max depth changed")
        }
        guard record.limit == limit else {
            return .stale("scan result limit changed")
        }
        guard record.scanMode == mode else {
            return .stale("scan mode changed")
        }
        if let lastRootChangeMillis = StorageRootChangeJournal.lastChangeMillis(),
            lastRootChangeMillis > record.savedAtMillis
        {
            return .stale("storage roots changed after cache save")
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
            "Downloads",
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
            let currentRef = readGitRef(repoRoot: repository.repoRoot)
            if currentRef.reference != repository.gitRef || currentRef.branch != repository.gitBranch {
                return "repository ref changed: \(repository.repoName)"
            }
            let gitDirectory = gitDirectory(repoRoot: repository.repoRoot)
            if fileFingerprint(gitDirectory?.appendingPathComponent("config")) != repository.gitConfigFingerprint {
                return "repository Git config changed: \(repository.repoName)"
            }
            if fileFingerprint(gitDirectory?.appendingPathComponent("index")) != repository.gitIndexFingerprint {
                return "repository Git index changed: \(repository.repoName)"
            }
            let currentContracts = agentContractFingerprints(repoRoot: repository.repoRoot)
            if currentContracts != repository.agentContracts {
                return "repository agent contracts changed: \(repository.repoName)"
            }
        }
        return nil
    }

    private static func agentContractFingerprints(repoRoot: String) -> [StorageHygieneContractCacheFingerprint] {
        storageAgentContractCachePaths.map { relativePath in
            let url = URL(fileURLWithPath: repoRoot, isDirectory: true)
                .appendingPathComponent(relativePath, isDirectory: false)
            return StorageHygieneContractCacheFingerprint(path: relativePath, fileURL: url)
        }
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

    private static func readGitRef(repoRoot: String) -> (reference: String?, branch: String?) {
        guard let gitDirectory = gitDirectory(repoRoot: repoRoot) else { return (nil, nil) }
        let headURL = gitDirectory.appendingPathComponent("HEAD", isDirectory: false)
        guard let head = try? String(contentsOf: headURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !head.isEmpty
        else {
            return (nil, nil)
        }
        guard let reference = head.stripPrefix("ref: ") else {
            return (head, nil)
        }
        let branchPrefix = "refs/heads/"
        let branch = reference.hasPrefix(branchPrefix)
            ? String(reference.dropFirst(branchPrefix.count))
            : nil
        return (reference, branch)
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

    private static func fileFingerprint(_ url: URL?) -> String? {
        guard let url else { return nil }
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path) else {
            return nil
        }
        let size = (attributes[.size] as? NSNumber)?.uint64Value ?? 0
        let modifiedMillis = ((attributes[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0) * 1000
        return "\(size):\(UInt64(modifiedMillis))"
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

struct StorageHygieneSummaryModel: Decodable, Sendable {
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
        topKRetained: false
    )
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
        cleanupAllowed = try container.decodeIfPresent(Bool.self, forKey: .cleanupAllowed) ?? true
        cleanupBlockers = try container.decodeIfPresent([String].self, forKey: .cleanupBlockers) ?? []
        defaultCleanupAction = try container.decodeIfPresent(String.self, forKey: .defaultCleanupAction) ?? "trash"
    }
}

struct StorageBudgetGuardrailsModel: Decodable, Sendable {
    let repoGrowthBudgetBytesPerDay: UInt64
    let repoArtifactBudgetBytes: UInt64
    let totalArtifactBudgetBytes: UInt64
    let status: String
    let violations: [StorageBudgetViolationModel]
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
    let confirmed: Bool
    let confidenceScore: UInt8
    let fileCount: Int
    let totalBytes: UInt64
    let reclaimableBytes: UInt64
    let paths: [StorageDuplicateItemModel]
    let recommendation: String
    let caveat: String
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

struct StorageRepositoryInventoryModel: Decodable, Identifiable, Sendable {
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
        topArtifactFolders = try container.decode([StorageRepoArtifactFolderModel].self, forKey: .topArtifactFolders)
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

    var id: String { path }
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
    let evidence: [String]
    let cleanupAllowed: Bool
    let cleanupBlockers: [String]
    let defaultCleanupAction: String
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
        case evidence
        case cleanupAllowed
        case cleanupBlockers
        case defaultCleanupAction
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
        case "app-support-data", "app-container", "app-launch-item":
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
    let detail: String
    let localBytes: UInt64?
    let logicalBytes: UInt64?
    let reclaimableBytes: UInt64?
    let cloudPlaceholder: Bool
    let network: Bool
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
