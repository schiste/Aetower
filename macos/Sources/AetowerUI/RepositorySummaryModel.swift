import AetowerBridge
import Foundation
import SwiftUI

// MARK: - Repository summary model
// Extracted from RepositoryView so the expensive report join is buildable and
// testable outside the view, and so the view can cache it: the join used to
// be recomputed 8-10x per body evaluation, each doing an O(repos x entities)
// live-context filter against the per-tick entities slice.

/// Outcome of moving a repository's artifact folders to the Trash, shown as an
/// inline result after the user confirms a cleanup.
struct RepositoryArtifactCleanupResult: Sendable {
    let movedCount: Int
    let failedCount: Int
    let reclaimedBytes: UInt64
    let firstError: String?

    var succeeded: Bool { failedCount == 0 }
}

struct RepositorySummary: Identifiable {
    let id: String
    let root: String
    let name: String
    let hasStorageFootprint: Bool
    let currentSizeBytes: UInt64
    let artifactBytes: UInt64
    let itemCount: Int
    let growthBytes: Int64?
    let growthWindow: String
    let estimatedRebuildCost: String
    let estimatedRebuildSeconds: UInt64?
    let topArtifactFolders: [StorageRepoArtifactFolderModel]
    let caveats: [String]
    let violationCount: Int
    let reviewItemCount: Int
    let safeItemCount: Int
    let staleItemCount: Int
    var liveSessionCount: Int
    var liveEntityCount: Int
    var liveMemoryBytes: UInt64
    var liveCPUPercent: Float
    var aiRunCount: UInt32 = 0
    var aiTotalTokens: UInt64 = 0
    var aiCostUsd: Float = 0
    var aiProviders: [String] = []
    let agentArtifactBytes: UInt64
    let agentCount: Int
    let scorecardReport: RepositoryScorecardReportModel?
    let gitDirtyStatus: String
    let gitDirtyFileCount: UInt64?
    let gitDirtyTruncated: Bool
    let notSeenInLatestScan: Bool
    let gitBranch: String?
    let gitHead: String?
    let gitRef: String?
    let gitDetachedHead: Bool
    let cloneGroupCount: UInt64
    let cloneGroupRoots: [String]
    let discoveredRoot: String?
    let project: RepositoryProjectModel?
    let gitRemoteOriginUrl: String?
    let gitRemoteKey: String?
    let gitRemoteHost: String?
    let gitRemoteOwner: String?
    let gitRemoteName: String?
    let inventoryCacheStatus: String
    let inventoryFingerprintChanged: Bool
    let inventoryLastSeenMillis: UInt64?
    let inventoryLastScanMillis: UInt64?
    let agentReadinessScore: UInt8
    let agentReadinessStatus: String
    let agentContractMissingCount: UInt64
    let agentContractCoverage: [StorageAgentContractCoverageModel]
    let agentGuidanceStatus: String
    let agentGuidanceIssueCount: UInt64
    let agentGuidanceIssues: [StorageAgentGuidanceIssueModel]
    let hasAgentsMd: Bool
    let hasClaudeMd: Bool
    let claudeMdBytes: UInt64?
    let claudeMdDelegationMaxBytes: UInt64
    let claudeMdDelegatesToAgentsMd: Bool
    let lastWriterProcess: String?
    let lastWriterPid: UInt32?
    let lastBranchTouched: String?
    /// Full footprint for the detail Storage tab (mix, rebuildable split);
    /// nil when the repo was indexed without a storage footprint.
    let footprint: StorageRepoFootprintModel?

    var attentionScore: Double {
        let artifactScore = min(Double(artifactBytes) / Double(512 * 1024 * 1024), 18)
        let growthScore = growthBytes.map { max(0, Double($0) / Double(256 * 1024 * 1024)) } ?? 0
        let reviewScore = Double(reviewItemCount) * 2.2
        let staleScore = Double(staleItemCount) * 0.7
        let violationScore = Double(violationCount) * 8
        let liveScore = Double(liveSessionCount + liveEntityCount) * 1.4
        let readinessPenalty = Double(100 - Int(agentReadinessScore)) / 8.0
        let qualityScore = Double(qualityIssueCount + Int(agentGuidanceIssueCount)) * 2.5 + readinessPenalty
        let duplicateScore = cloneGroupCount > 1 ? Double(cloneGroupCount) * 3 : 0
        let dirtyScore = gitDirtyStatus == "dirty" ? 2.0 : 0.0
        let aiSpendScore = min(Double(aiCostUsd) / 5.0, 8)
        let aiActivityScore = aiRunCount > 0 && gitDirtyStatus == "dirty" ? 2.0 : 0.0
        let inventoryScore = inventoryNeedsAttention ? 9.0 : 0.0
        let storageScore = artifactScore + growthScore + reviewScore + staleScore
        let runtimeScore = violationScore + liveScore + qualityScore
        return storageScore
            + runtimeScore
            + duplicateScore
            + dirtyScore
            + aiSpendScore
            + aiActivityScore
            + inventoryScore
            + scorecardAttentionScore
            + githubProviderAttentionScore
            + cloudflareProviderAttentionScore
    }

    var githubProviderAttentionScore: Double {
        guard let status = project?.githubStatus else { return 0 }
        if status.failedLatestCIOnDefaultBranch {
            return status.latestCheckState == "failing" ? 10 : 8
        }
        if status.staleOpenPullRequestCount() > 0 {
            return 5
        }
        return 0
    }

    var scorecardAttentionScore: Double {
        guard let scorecardReport, scorecardReport.status == "ok" else { return 0 }
        let aggregateScore: Double
        if let score = scorecardReport.score {
            if score < 5 {
                aggregateScore = 12
            } else if score <= 7 {
                aggregateScore = 6
            } else {
                aggregateScore = 1
            }
        } else {
            aggregateScore = 0
        }
        let criticalFailureScore = scorecardCriticalFailureCount > 0 ? 10.0 : 0.0
        return max(aggregateScore, criticalFailureScore)
    }

    var scorecardCriticalFailureCount: Int {
        guard let scorecardReport else { return 0 }
        let highSeverityChecks = Set(scorecardReport.recommendations.compactMap { recommendation in
            ["critical", "high"].contains(recommendation.severity.lowercased())
                ? Self.normalizedScorecardCheckName(recommendation.checkName)
                : nil
        })
        return scorecardReport.failedChecks.filter { check in
            if let score = check.score, score <= 0 { return true }
            return highSeverityChecks.contains(Self.normalizedScorecardCheckName(check.name))
        }.count
    }

    var scorecardCaveat: String? {
        guard let scorecardReport else {
            return "OpenSSF Scorecard has not been run for this repository; supply-chain attention is unchanged."
        }
        guard scorecardReport.status == "ok" else {
            return "OpenSSF Scorecard is unavailable for this repository (\(scorecardReport.status)); supply-chain attention is unchanged."
        }
        guard scorecardReport.score != nil else {
            return "OpenSSF Scorecard completed without an aggregate score; supply-chain attention is unchanged."
        }
        return nil
    }

    var hasScorecardAttention: Bool {
        scorecardAttentionScore >= 5 || scorecardCriticalFailureCount > 0
    }

    var hasGitHubProviderAttention: Bool {
        githubProviderAttentionScore >= 5
    }

    var cloudflareProviderAttentionScore: Double {
        guard let project else { return 0 }
        let failedRank = project.cloudflareEnvironmentGroups.compactMap { group -> Int? in
            group.links.contains {
                project.cloudflareStatus(for: $0)?.hasFailedDeployment == true
            } ? group.rank : nil
        }.max() ?? 0
        if failedRank >= 80 { return 10 }
        if failedRank >= 50 { return 6 }
        if failedRank > 0 { return 3 }
        return 0
    }

    var hasCloudflareProviderAttention: Bool {
        cloudflareProviderAttentionScore >= 5
    }

    var requiresAttention: Bool {
        optimizationProfile.requiresAttention
    }

    var inventoryNeedsAttention: Bool {
        notSeenInLatestScan
            || inventoryFingerprintChanged
            || inventoryCacheStatus == "changed"
            || inventoryCacheStatus == "missing"
            || inventoryCacheStatus == "legacy"
    }

    private static func normalizedScorecardCheckName(_ name: String) -> String {
        name
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
            .replacingOccurrences(of: "_", with: "-")
            .replacingOccurrences(of: " ", with: "-")
    }

    var qualityIssueCount: Int {
        [
            !hasAgentsMd,
            !hasClaudeMd,
            hasClaudeMd && !claudeMdDelegatesToAgentsMd,
        ].filter { $0 }.count
    }

    var qualityStatusLabel: String {
        let count = max(UInt64(qualityIssueCount), agentGuidanceIssueCount)
        return count == 0 ? "Guidance ok" : "\(count) guidance gap\(count == 1 ? "" : "s")"
    }

    var qualityStatusTone: Color {
        if agentGuidanceStatus == "error" { return AetowerDesign.Status.error }
        if agentGuidanceStatus == "warning" || qualityIssueCount > 0 { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.ready
    }

    var statusLabel: String {
        if inventoryCacheStatus == "missing" { return "Missing" }
        if inventoryCacheStatus == "changed" || inventoryFingerprintChanged { return "Changed" }
        if violationCount > 0 { return "Budget" }
        if agentReadinessStatus == "blocked" { return "Agent blocked" }
        if agentReadinessStatus == "weak" { return "Agent weak" }
        if agentGuidanceStatus == "error" { return "Guidance" }
        if scorecardAttentionScore >= 10 { return "Scorecard" }
        if githubProviderAttentionScore >= 8 { return "CI" }
        if cloudflareProviderAttentionScore >= 8 { return "Deploy" }
        if cloneGroupCount > 1 { return "Cloned" }
        if (growthBytes ?? 0) > 0 { return "Growing" }
        if gitDirtyStatus == "dirty" { return "Dirty" }
        if scorecardAttentionScore >= 5 { return "Scorecard" }
        if githubProviderAttentionScore >= 5 { return "GitHub" }
        if cloudflareProviderAttentionScore >= 5 { return "Cloudflare" }
        if reviewItemCount > 0 { return "Review" }
        if liveSessionCount > 0 || liveEntityCount > 0 { return "Active" }
        if notSeenInLatestScan { return "Cached" }
        if !hasStorageFootprint { return "Indexed" }
        return "Stable"
    }

    var statusTone: Color {
        if inventoryCacheStatus == "missing" { return AetowerDesign.Status.error }
        if inventoryCacheStatus == "changed" || inventoryFingerprintChanged { return AetowerDesign.Status.warning }
        if violationCount > 0 { return AetowerDesign.Status.error }
        if agentReadinessStatus == "blocked" { return AetowerDesign.Status.error }
        if agentReadinessStatus == "weak" { return AetowerDesign.Status.warning }
        if agentGuidanceStatus == "error" { return AetowerDesign.Status.error }
        if scorecardAttentionScore >= 10 { return AetowerDesign.Status.error }
        if githubProviderAttentionScore >= 8 { return AetowerDesign.Status.error }
        if cloudflareProviderAttentionScore >= 8 { return AetowerDesign.Status.error }
        if cloneGroupCount > 1 { return AetowerDesign.Status.warning }
        if (growthBytes ?? 0) > 512 * 1024 * 1024 { return AetowerDesign.Status.warning }
        if gitDirtyStatus == "dirty" { return AetowerDesign.Tone.energy }
        if scorecardAttentionScore >= 5 { return AetowerDesign.Status.warning }
        if githubProviderAttentionScore >= 5 { return AetowerDesign.Status.warning }
        if cloudflareProviderAttentionScore >= 5 { return AetowerDesign.Status.warning }
        if reviewItemCount > 0 { return AetowerDesign.Status.warning }
        if liveSessionCount > 0 || liveEntityCount > 0 { return AetowerDesign.Status.ready }
        return AetowerDesign.Status.neutral
    }
}


// MARK: - Live context (per-tick overlay)

struct RepositoryLiveContext: Equatable {
    var sessionCount = 0
    var entityCount = 0
    var memoryBytes: UInt64 = 0
    var cpuPercent: Float = 0
}

enum RepositorySummaryBuilder {
    /// One pass over sessions + entities producing per-root live aggregates,
    /// replacing the previous per-repo O(entities) filters.
    static func liveContexts(
        roots: some Collection<String>,
        sessions: [Chau7SessionSummary],
        entities: [EntitySnapshot]
    ) -> [String: RepositoryLiveContext] {
        let rootSet = Set(roots)
        var contexts: [String: RepositoryLiveContext] = [:]

        for session in sessions {
            for candidate in [session.repoRoot, session.workspacePath] {
                if let candidate, rootSet.contains(candidate) {
                    contexts[candidate, default: RepositoryLiveContext()].sessionCount += 1
                    break
                }
            }
        }

        for entity in entities {
            var matchedRoots: Set<String> = []
            for component in entity.components {
                if let repoRoot = component.adapterContext?.repoRoot, rootSet.contains(repoRoot) {
                    matchedRoots.insert(repoRoot)
                }
                if let cwd = component.cwd, rootSet.contains(cwd) {
                    matchedRoots.insert(cwd)
                }
            }
            guard !matchedRoots.isEmpty else { continue }
            let memory = entityEffectiveMemoryBytes(entity)
            let cpu = entity.metrics.cpuPercent
            for root in matchedRoots {
                var context = contexts[root, default: RepositoryLiveContext()]
                context.entityCount += 1
                context.memoryBytes = context.memoryBytes.addingReportingOverflow(memory).partialValue
                context.cpuPercent += cpu
                contexts[root] = context
            }
        }
        return contexts
    }

    /// Report-derived (static) summaries with live fields zeroed; the live
    /// overlay is applied per tick via `applyingLive`.
    static func staticSummaries(
        report: StorageHygieneReportModel,
        scorecardsByRoot: [String: RepositoryScorecardReportModel],
        projectForRoot: (String) -> RepositoryProjectModel?,
        previousSizeByRoot: [String: UInt64],
        baselineSizeByRoot: [String: UInt64],
        hasBaseline: Bool
    ) -> [RepositorySummary] {
        let violationCountsByRoot = report.budgetGuardrails.violations.reduce(into: [String: Int]()) {
            counts, violation in
            guard let repoRoot = violation.repoRoot else { return }
            counts[repoRoot, default: 0] += 1
        }
        let itemsByRoot = Dictionary(grouping: report.items.compactMap { item -> (String, StorageHygieneItemModel)? in
            guard let repoRoot = item.attribution.repoRoot else { return nil }
            return (repoRoot, item)
        }, by: \.0)
        let footprintsByRoot = Dictionary(uniqueKeysWithValues: report.repoFootprints.map { ($0.repoRoot, $0) })

        return report.repositoryInventory.map { inventory in
            let root = inventory.repoRoot
            let footprint = footprintsByRoot[root]
            let items = itemsByRoot[root]?.map(\.1) ?? []
            let agent = agentContext(for: root, report: report)
            let scorecardReport = scorecardsByRoot[root]
            let project = projectForRoot(root)
            var baseCaveats = footprint?.caveats ?? [
                "Indexed from Git repository discovery. No tracked storage artifacts were found in the bounded hygiene scan."
            ]
            if inventory.notSeenInLatestScan {
                baseCaveats.append(
                    "Repository was restored from the persistent inventory cache and was not seen in the latest scan."
                )
            }
            return RepositorySummary(
                id: inventory.id,
                root: root,
                name: inventory.repoName,
                hasStorageFootprint: footprint != nil,
                currentSizeBytes: footprint?.currentSizeBytes ?? 0,
                artifactBytes: footprint?.artifactBytes ?? 0,
                itemCount: footprint?.itemCount ?? 0,
                growthBytes: footprint.flatMap {
                    storageGrowthDelta(
                        for: $0,
                        previousSizeByRoot: previousSizeByRoot,
                        baselineSizeByRoot: baselineSizeByRoot
                    )
                },
                growthWindow: footprint.map { storageGrowthWindow(for: $0, hasBaseline: hasBaseline) }
                    ?? "no storage footprint baseline",
                estimatedRebuildCost: footprint?.estimatedRebuildCost ?? "None",
                estimatedRebuildSeconds: footprint?.estimatedRebuildSeconds ?? 0,
                topArtifactFolders: footprint?.topArtifactFolders ?? [],
                caveats: caveats(base: baseCaveats, scorecardReport: scorecardReport, project: project),
                violationCount: violationCountsByRoot[root] ?? 0,
                reviewItemCount: items.filter { $0.safety != "safe" }.count,
                safeItemCount: items.filter { $0.safety == "safe" }.count,
                staleItemCount: items.filter(\.stale).count,
                liveSessionCount: 0,
                liveEntityCount: 0,
                liveMemoryBytes: 0,
                liveCPUPercent: 0,
                agentArtifactBytes: agent.artifactBytes,
                agentCount: agent.agentCount,
                scorecardReport: scorecardReport,
                gitDirtyStatus: inventory.gitDirtyStatus,
                gitDirtyFileCount: inventory.gitDirtyFileCount,
                gitDirtyTruncated: inventory.gitDirtyTruncated,
                notSeenInLatestScan: inventory.notSeenInLatestScan,
                gitBranch: inventory.gitBranch,
                gitHead: inventory.gitHead,
                gitRef: inventory.gitRef,
                gitDetachedHead: inventory.gitDetachedHead,
                cloneGroupCount: inventory.cloneGroupCount,
                cloneGroupRoots: inventory.cloneGroupRoots,
                discoveredRoot: inventory.discoveredRoot,
                project: project,
                gitRemoteOriginUrl: inventory.gitRemoteOriginUrl,
                gitRemoteKey: inventory.gitRemoteKey,
                gitRemoteHost: inventory.gitRemoteHost,
                gitRemoteOwner: inventory.gitRemoteOwner,
                gitRemoteName: inventory.gitRemoteName,
                inventoryCacheStatus: inventory.inventoryCacheStatus,
                inventoryFingerprintChanged: inventory.inventoryFingerprintChanged,
                inventoryLastSeenMillis: inventory.inventoryLastSeenMillis,
                inventoryLastScanMillis: inventory.inventoryLastScanMillis,
                agentReadinessScore: inventory.agentReadinessScore,
                agentReadinessStatus: inventory.agentReadinessStatus,
                agentContractMissingCount: inventory.agentContractMissingCount,
                agentContractCoverage: inventory.agentContractCoverage,
                agentGuidanceStatus: inventory.agentGuidanceStatus,
                agentGuidanceIssueCount: inventory.agentGuidanceIssueCount,
                agentGuidanceIssues: inventory.agentGuidanceIssues,
                hasAgentsMd: inventory.hasAgentsMd,
                hasClaudeMd: inventory.hasClaudeMd,
                claudeMdBytes: inventory.claudeMdBytes,
                claudeMdDelegationMaxBytes: inventory.claudeMdDelegationMaxBytes,
                claudeMdDelegatesToAgentsMd: inventory.claudeMdDelegatesToAgentsMd,
                lastWriterProcess: footprint?.lastWriterProcess,
                lastWriterPid: footprint?.lastWriterPid,
                lastBranchTouched: footprint?.lastBranchTouched ?? inventory.gitBranch ?? inventory.gitHead,
                footprint: footprint
            )
        }
    }

    static func applyingLive(
        _ summaries: [RepositorySummary],
        live: [String: RepositoryLiveContext],
        aiUsageByRoot: [String: AiRepoSummary] = [:]
    ) -> [RepositorySummary] {
        summaries.map { summary in
            var updated = summary
            if let context = live[summary.root] {
                updated.liveSessionCount = context.sessionCount
                updated.liveEntityCount = context.entityCount
                updated.liveMemoryBytes = context.memoryBytes
                updated.liveCPUPercent = context.cpuPercent
            }
            if let usage = aiUsageByRoot[summary.root] {
                updated.aiRunCount = usage.totalRuns
                updated.aiTotalTokens = usage.totalTokens
                updated.aiCostUsd = usage.totalCostUsd
                updated.aiProviders = usage.providers
            }
            return updated
        }
    }

    /// Exact-root join, matching the liveContexts convention (Chau7 reports
    /// repo_root/workspace_path in the same path domain as the inventory).
    static func aiUsage(byRoot summaries: [AiRepoSummary]) -> [String: AiRepoSummary] {
        Dictionary(summaries.map { ($0.repoPath, $0) }) { first, _ in first }
    }

    static func caveats(
        base: [String],
        scorecardReport: RepositoryScorecardReportModel?,
        project: RepositoryProjectModel?
    ) -> [String] {
        var caveats = base
        if let scorecardReport {
            if scorecardReport.status != "ok" {
                caveats.append(
                    "OpenSSF Scorecard is unavailable for this repository (\(scorecardReport.status)); supply-chain attention is unchanged."
                )
            } else if scorecardReport.score == nil {
                caveats.append(
                    "OpenSSF Scorecard completed without an aggregate score; supply-chain attention is unchanged."
                )
            }
        } else {
            caveats.append(
                "OpenSSF Scorecard has not been run for this repository; supply-chain attention is unchanged."
            )
        }
        if project?.githubStatus?.hasAuthCaveat == true {
            caveats.append(
                "GitHub project status needs authentication or broader read access; external attention is unchanged."
            )
        }
        if project?.cloudflareStatuses?.contains(where: \.hasAuthCaveat) == true {
            caveats.append(
                "Cloudflare project status needs an API token with read access; external attention is unchanged."
            )
        }
        return caveats
    }

    static func agentContext(
        for repoRoot: String,
        report: StorageHygieneReportModel
    ) -> (artifactBytes: UInt64, agentCount: Int) {
        let agents = report.agentHygiene.agents.filter { agent in
            agent.topRepositories.contains { $0.repoRoot == repoRoot }
        }
        let artifactBytes = agents.reduce(UInt64(0)) { total, agent in
            let repoBytes = agent.topRepositories
                .filter { $0.repoRoot == repoRoot }
                .reduce(UInt64(0)) { subtotal, repo in
                    subtotal.addingReportingOverflow(repo.artifactBytes).partialValue
                }
            return total.addingReportingOverflow(repoBytes).partialValue
        }
        return (artifactBytes, agents.count)
    }

    static func storageGrowthDelta(
        for footprint: StorageRepoFootprintModel,
        previousSizeByRoot: [String: UInt64],
        baselineSizeByRoot: [String: UInt64]
    ) -> Int64? {
        if let growth = footprint.growthBytes {
            return growth
        }
        if let previous = previousSizeByRoot[footprint.repoRoot] {
            return Int64(footprint.currentSizeBytes) - Int64(previous)
        }
        if let baseline = baselineSizeByRoot[footprint.repoRoot] {
            return Int64(footprint.currentSizeBytes) - Int64(baseline)
        }
        return nil
    }

    static func storageGrowthWindow(
        for footprint: StorageRepoFootprintModel,
        hasBaseline: Bool
    ) -> String {
        if !footprint.growthWindow.isEmpty {
            return footprint.growthWindow
        }
        return hasBaseline ? "since saved baseline" : "no baseline yet"
    }
}

// MARK: - Cache

/// Single-slot cache: the static (report-derived) join recomputes only when a
/// summary input generation moves; the live overlay recomputes per snapshot
/// sequence, as one pass over the entities slice.
@MainActor
final class RepositorySummaryCacheStore {
    private var staticKey: UInt64?
    private var staticSummaries: [RepositorySummary] = []
    private var liveKey: UInt64?
    private var liveSummaries: [RepositorySummary] = []

    func summaries(
        inputsGeneration: UInt64,
        snapshotSequence: UInt64,
        buildStatic: () -> [RepositorySummary],
        buildLive: ([RepositorySummary]) -> [RepositorySummary]
    ) -> [RepositorySummary] {
        if staticKey != inputsGeneration {
            staticSummaries = buildStatic()
            staticKey = inputsGeneration
            liveKey = nil
        }
        if liveKey != snapshotSequence {
            liveSummaries = buildLive(staticSummaries)
            liveKey = snapshotSequence
        }
        return liveSummaries
    }
}
