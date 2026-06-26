import Foundation

struct StorageHygieneReportModel: Decodable {
    let capturedAtMillis: UInt64
    let scanDurationMillis: UInt64
    let summary: StorageHygieneSummaryModel
    let cleanupTiers: [StorageCleanupTierModel]
    let cleanupRecipes: [StorageCleanupRecipeModel]
    let budgetGuardrails: StorageBudgetGuardrailsModel
    let agentHygiene: StorageAgentHygieneSummaryModel
    let repoFootprints: [StorageRepoFootprintModel]
    let items: [StorageHygieneItemModel]
    let roots: [String]
    let skippedRoots: [StorageSkippedRootModel]
    let truncated: Bool
    let caveats: [String]
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
