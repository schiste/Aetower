import XCTest
import AetowerBridge

@testable import AetowerUI

/// Pins the attention/status scoring that moved out of RepositoryView, and
/// covers the single-pass live join + summary cache introduced with it.
final class RepositorySummaryModelTests: XCTestCase {
    private func summary(
        artifactBytes: UInt64 = 0,
        growthBytes: Int64? = nil,
        violationCount: Int = 0,
        reviewItemCount: Int = 0,
        staleItemCount: Int = 0,
        liveSessionCount: Int = 0,
        liveEntityCount: Int = 0,
        scorecardReport: RepositoryScorecardReportModel? = nil,
        gitDirtyStatus: String = "clean",
        aiRunCount: UInt32 = 0,
        aiCostUsd: Float = 0,
        cloneGroupCount: UInt64 = 1,
        inventoryCacheStatus: String = "fresh",
        inventoryFingerprintChanged: Bool = false,
        notSeenInLatestScan: Bool = false,
        agentReadinessScore: UInt8 = 100,
        hasAgentsMd: Bool = true,
        hasClaudeMd: Bool = true,
        claudeMdDelegatesToAgentsMd: Bool = true
    ) -> RepositorySummary {
        RepositorySummary(
            id: "repo",
            root: "/tmp/repo",
            name: "repo",
            hasStorageFootprint: true,
            currentSizeBytes: 0,
            artifactBytes: artifactBytes,
            itemCount: 0,
            growthBytes: growthBytes,
            growthWindow: "24h",
            estimatedRebuildCost: "None",
            estimatedRebuildSeconds: 0,
            topArtifactFolders: [],
            caveats: [],
            violationCount: violationCount,
            reviewItemCount: reviewItemCount,
            safeItemCount: 0,
            staleItemCount: staleItemCount,
            liveSessionCount: liveSessionCount,
            liveEntityCount: liveEntityCount,
            liveMemoryBytes: 0,
            liveCPUPercent: 0,
            aiRunCount: aiRunCount,
            aiTotalTokens: 0,
            aiCostUsd: aiCostUsd,
            aiProviders: [],
            agentArtifactBytes: 0,
            agentCount: 0,
            scorecardReport: scorecardReport,
            gitDirtyStatus: gitDirtyStatus,
            gitDirtyFileCount: nil,
            gitDirtyTruncated: false,
            notSeenInLatestScan: notSeenInLatestScan,
            gitBranch: "main",
            gitHead: nil,
            gitRef: nil,
            gitDetachedHead: false,
            cloneGroupCount: cloneGroupCount,
            cloneGroupRoots: [],
            discoveredRoot: nil,
            project: nil,
            gitRemoteOriginUrl: nil,
            gitRemoteKey: nil,
            gitRemoteHost: nil,
            gitRemoteOwner: nil,
            gitRemoteName: nil,
            inventoryCacheStatus: inventoryCacheStatus,
            inventoryFingerprintChanged: inventoryFingerprintChanged,
            inventoryLastSeenMillis: nil,
            inventoryLastScanMillis: nil,
            agentReadinessScore: agentReadinessScore,
            agentReadinessStatus: "ready",
            agentContractMissingCount: 0,
            agentContractCoverage: [],
            agentGuidanceStatus: "ok",
            agentGuidanceIssueCount: 0,
            agentGuidanceIssues: [],
            hasAgentsMd: hasAgentsMd,
            hasClaudeMd: hasClaudeMd,
            claudeMdBytes: nil,
            claudeMdDelegationMaxBytes: 0,
            claudeMdDelegatesToAgentsMd: claudeMdDelegatesToAgentsMd,
            lastWriterProcess: nil,
            lastWriterPid: nil,
            lastBranchTouched: nil,
            footprint: nil
        )
    }

    // MARK: attention score pins (behavior moved verbatim from RepositoryView)

    func testQuietRepositoryScoresZero() {
        XCTAssertEqual(summary().attentionScore, 0, accuracy: 0.001)
        XCTAssertFalse(summary().requiresAttention)
    }

    func testDirtyRepositoryAddsTwo() {
        let dirty = summary(gitDirtyStatus: "dirty")
        XCTAssertEqual(dirty.attentionScore, 2.0, accuracy: 0.001)
        XCTAssertTrue(dirty.requiresAttention)
    }

    func testInventoryAttentionAddsNine() {
        let changed = summary(inventoryFingerprintChanged: true)
        XCTAssertEqual(changed.attentionScore, 9.0, accuracy: 0.001)
        XCTAssertTrue(changed.inventoryNeedsAttention)
    }

    func testOptimizationLayerRoutesFingerprintChangesToRefreshInventory() {
        let changed = summary(
            inventoryCacheStatus: "changed",
            inventoryFingerprintChanged: true
        )

        XCTAssertEqual(changed.primaryOptimizationActionKind, .refreshInventory)
        XCTAssertEqual(changed.optimizationSignals.first?.kind, .inventoryFreshness)
        XCTAssertTrue(changed.requiresAttention)
    }

    func testStaleRepositoryRemainsVisibleAndRefreshable() {
        let stale = summary(
            inventoryCacheStatus: "missing",
            notSeenInLatestScan: true
        )

        XCTAssertEqual(stale.statusLabel, "Missing")
        XCTAssertEqual(stale.primaryOptimizationActionKind, .refreshInventory)
        XCTAssertTrue(stale.inventoryNeedsAttention)
        XCTAssertTrue(stale.requiresAttention)
    }

    func testArtifactScoreCapsAtEighteen() {
        let huge = summary(artifactBytes: 100 * 1024 * 1024 * 1024)
        XCTAssertEqual(huge.attentionScore, 18.0, accuracy: 0.001)
    }

    func testViolationsWeighEight() {
        XCTAssertEqual(summary(violationCount: 2).attentionScore, 16.0, accuracy: 0.001)
    }

    func testGuidanceGapsScoreViaQualityCount() {
        // Missing AGENTS.md + missing CLAUDE.md = 2 quality issues * 2.5 each.
        let gaps = summary(hasAgentsMd: false, hasClaudeMd: false)
        XCTAssertEqual(gaps.attentionScore, 5.0, accuracy: 0.001)
        XCTAssertEqual(gaps.qualityIssueCount, 2)
    }

    func testAiSpendContributesCappedScore() {
        // $10 -> 2.0; cap at 8 regardless of spend.
        XCTAssertEqual(summary(aiRunCount: 3, aiCostUsd: 10).attentionScore, 2.0, accuracy: 0.001)
        XCTAssertEqual(summary(aiRunCount: 3, aiCostUsd: 500).attentionScore, 8.0, accuracy: 0.001)
    }

    func testActiveAgentOnDirtyTreeAddsBump() {
        // dirty(2.0) + spend(0.2) + active-agent-on-dirty bump(2.0)
        let active = summary(gitDirtyStatus: "dirty", aiRunCount: 1, aiCostUsd: 1)
        XCTAssertEqual(active.attentionScore, 4.2, accuracy: 0.001)
    }

    func testOptimizationSignalsUseExplicitPriorityOrder() {
        let repo = summary(
            growthBytes: 1024,
            violationCount: 1,
            reviewItemCount: 2,
            gitDirtyStatus: "dirty",
            inventoryFingerprintChanged: true,
            hasAgentsMd: false,
            hasClaudeMd: false
        )

        XCTAssertEqual(
            repo.optimizationSignals.map(\.kind),
            [
                .inventoryFreshness,
                .budget,
                .guidance,
                .dirtyWorktree,
                .storageGrowth,
                .reviewItems,
            ]
        )
    }

    func testMissingInventorySignalIsCriticalAndRefreshesFirst() {
        let repo = summary(inventoryCacheStatus: "missing")

        XCTAssertEqual(repo.optimizationSignals.first?.kind, .inventoryFreshness)
        XCTAssertEqual(repo.optimizationSignals.first?.severity, .critical)
        XCTAssertEqual(repo.primaryOptimizationActionKind, .refreshInventory)
        XCTAssertTrue(repo.requiresAttention)
    }

    func testQuietRepositoryPrimaryActionCanStillRunScorecard() {
        let repo = summary()

        XCTAssertTrue(repo.optimizationSignals.isEmpty)
        XCTAssertEqual(repo.primaryOptimizationActionKind, .runScorecard)
        XCTAssertFalse(repo.requiresAttention)
    }

    func testPlannerAcceptsNeutralOptimizationInput() {
        let input = RepositoryOptimizationInput(
            identity: .init(
                id: "external-repo",
                root: "/workspace/external",
                name: "external",
                source: "external-tool"
            ),
            aggregateAttentionScore: 4,
            inventory: .init(status: "changed"),
            scorecard: .init(scanned: true)
        )

        let profile = RepositoryOptimizationPlanner.profile(for: input)

        XCTAssertEqual(input.identity.source, "external-tool")
        XCTAssertEqual(profile.signals.map(\.kind), [.inventoryFreshness])
        XCTAssertEqual(profile.primaryActionKind, .refreshInventory)
        XCTAssertTrue(profile.requiresAttention)
    }

    func testAggregateAttentionDoesNotCreateBudgetSignal() {
        let input = RepositoryOptimizationInput(
            aggregateAttentionScore: 9,
            scorecard: .init(scanned: true)
        )

        let profile = RepositoryOptimizationPlanner.profile(for: input)

        XCTAssertTrue(profile.signals.isEmpty)
        XCTAssertTrue(profile.requiresAttention)
    }

    func testOptimizationInputRoundTripsForExternalFeeds() throws {
        let input = RepositoryOptimizationInput(
            identity: .init(
                id: "external-repo",
                root: "/workspace/external",
                name: "external",
                source: "external-tool"
            ),
            aggregateAttentionScore: 12.5,
            budgetViolationCount: 1,
            inventory: .init(status: "current"),
            storage: .init(artifactBytes: 42, growthBytes: 7, reviewItemCount: 2),
            git: .init(dirtyStatus: "dirty", cloneGroupCount: 3),
            agentGuidance: .init(
                readinessStatus: "weak",
                guidanceStatus: "warning",
                guidanceIssueCount: 1,
                qualityIssueCount: 2
            ),
            scorecard: .init(scanned: true, attentionScore: 6),
            github: .init(scanned: true, attentionScore: 8),
            cloudflare: .init(scanned: true, attentionScore: 0)
        )

        let encoded = try JSONEncoder().encode(input)
        let decoded = try JSONDecoder().decode(RepositoryOptimizationInput.self, from: encoded)

        XCTAssertEqual(decoded, input)
    }

    func testAiUsageJoinAndOverlay() {
        let usage = AiRepoSummary(
            repoPath: "/tmp/repo", displayName: "repo", totalRuns: 7,
            totalTokens: 1_500_000, totalCostUsd: 12.5, providers: ["claude"]
        )
        let byRoot = RepositorySummaryBuilder.aiUsage(byRoot: [usage])
        let overlaid = RepositorySummaryBuilder.applyingLive([summary()], live: [:], aiUsageByRoot: byRoot)
        XCTAssertEqual(overlaid[0].aiRunCount, 7)
        XCTAssertEqual(overlaid[0].aiTotalTokens, 1_500_000)
        XCTAssertEqual(overlaid[0].aiCostUsd, 12.5, accuracy: 0.001)
        XCTAssertEqual(overlaid[0].aiProviders, ["claude"])
        XCTAssertTrue(overlaid[0].hasRepositoryUsage)
    }

    func testResourceCostRollupAddsRepositoryEnergyAndCarbon() {
        let rollup = ResourceCostRollup(
            scope: .repository,
            id: "repository:/tmp/repo",
            label: "repo",
            entityId: nil,
            sessionId: nil,
            repositoryPath: "/tmp/repo",
            watts: 2.0,
            energyWattHours: 1.5,
            batteryMinutes: nil,
            dollars: 14.75,
            carbonGrams: 0.72,
            diskGrowthBytes: 0,
            thermalContribution: nil,
            source: "chau7-repo+estimated-energy",
            confidence: 0.72
        )

        let byRoot = RepositorySummaryBuilder.resourceCost(byRoot: [rollup])
        let overlaid = RepositorySummaryBuilder.applyingLive(
            [summary(aiCostUsd: 1.0)],
            live: [:],
            resourceCostByRoot: byRoot
        )

        XCTAssertEqual(overlaid[0].aiCostUsd, 14.75, accuracy: 0.001)
        XCTAssertEqual(overlaid[0].resourceEnergyWattHours, 1.5, accuracy: 0.001)
        XCTAssertEqual(overlaid[0].resourceCarbonGrams, 0.72, accuracy: 0.001)
        XCTAssertEqual(overlaid[0].resourceCostSource, "chau7-repo+estimated-energy")
        XCTAssertEqual(overlaid[0].resourceCostConfidence, 0.72, accuracy: 0.001)
        XCTAssertTrue(overlaid[0].hasRepositoryResourceEstimate)
        XCTAssertTrue(overlaid[0].hasRepositoryUsage)
    }

    func testStatusLabelPriority() {
        XCTAssertEqual(summary(inventoryCacheStatus: "missing").statusLabel, "Missing")
        XCTAssertEqual(summary(violationCount: 1).statusLabel, "Budget")
        XCTAssertEqual(summary(cloneGroupCount: 3).statusLabel, "Cloned")
        XCTAssertEqual(summary(growthBytes: 1024).statusLabel, "Growing")
        XCTAssertEqual(summary(gitDirtyStatus: "dirty").statusLabel, "Dirty")
        XCTAssertEqual(summary().statusLabel, "Stable")
    }

    func testScorecardRecommendationOrdering() {
        func rec(_ severity: String, _ actionability: String, _ title: String) -> RepositoryScorecardRecommendationModel {
            RepositoryScorecardRecommendationModel(
                category: "c", checkName: "check", severity: severity,
                actionability: actionability, title: title, detail: ""
            )
        }
        let ordered = orderedScorecardRecommendations([
            rec("medium", "remote", "m-remote"),
            rec("critical", "remote", "crit-remote"),
            rec("high", "local", "high-local"),
            rec("critical", "local", "crit-local"),
        ])
        XCTAssertEqual(ordered.map(\.title), ["crit-local", "crit-remote", "high-local", "m-remote"])
    }

    // MARK: live context single pass

    private func session(id: String, repoRoot: String?, workspacePath: String? = nil) -> Chau7SessionSummary {
        Chau7SessionSummary(
            id: id, tabId: nil, sessionId: nil, title: id, provider: "claude",
            status: "running", workspacePath: workspacePath, repoRoot: repoRoot,
            gitBranch: nil, activeApp: nil, windowId: 0, runCount: 1,
            lastActive: "", turnCount: 0, childSessionCount: 0,
            pendingApprovalDescription: nil, lastExitReason: nil,
            activeRunDurationMillis: 0, isAtPrompt: false, shellLoading: false,
            ctoActive: false, linkedEntityIds: []
        )
    }

    func testSessionMatchingCountsOncePerSession() {
        // repoRoot and workspacePath both matching must count the session once.
        let contexts = RepositorySummaryBuilder.liveContexts(
            roots: ["/tmp/repo"],
            sessions: [session(id: "s1", repoRoot: "/tmp/repo", workspacePath: "/tmp/repo")],
            entities: []
        )
        XCTAssertEqual(contexts["/tmp/repo"]?.sessionCount, 1)
    }

    func testWorkspacePathFallbackMatches() {
        let contexts = RepositorySummaryBuilder.liveContexts(
            roots: ["/tmp/repo"],
            sessions: [session(id: "s1", repoRoot: nil, workspacePath: "/tmp/repo")],
            entities: []
        )
        XCTAssertEqual(contexts["/tmp/repo"]?.sessionCount, 1)
    }

    func testUnmatchedRootsProduceNoContext() {
        let contexts = RepositorySummaryBuilder.liveContexts(
            roots: ["/tmp/other"],
            sessions: [session(id: "s1", repoRoot: "/tmp/repo")],
            entities: []
        )
        XCTAssertNil(contexts["/tmp/other"])
    }

    func testLiveContextsAggregateCompactRuntimeEntityContext() {
        let contexts = RepositorySummaryBuilder.liveContexts(
            roots: ["/tmp/repo"],
            sessions: [],
            entities: [
                RepositoryRuntimeEntityContext(
                    entityId: "entity-1",
                    candidateRoots: ["/tmp/repo", "/tmp/other"],
                    memoryBytes: 42,
                    cpuPercent: 3.5
                )
            ]
        )
        XCTAssertEqual(contexts["/tmp/repo"]?.entityCount, 1)
        XCTAssertEqual(contexts["/tmp/repo"]?.memoryBytes, 42)
        XCTAssertEqual(contexts["/tmp/repo"]?.cpuPercent ?? 0, 3.5, accuracy: 0.001)
    }

    func testApplyingLiveOverlaysOnlyMatchedRoots() {
        let base = summary()
        let overlaid = RepositorySummaryBuilder.applyingLive(
            [base],
            live: ["/tmp/repo": RepositoryLiveContext(sessionCount: 2, entityCount: 1, memoryBytes: 42, cpuPercent: 3.5)]
        )
        XCTAssertEqual(overlaid[0].liveSessionCount, 2)
        XCTAssertEqual(overlaid[0].liveEntityCount, 1)
        XCTAssertEqual(overlaid[0].liveMemoryBytes, 42)
        XCTAssertEqual(overlaid[0].liveCPUPercent, 3.5, accuracy: 0.001)
    }

    // MARK: cache store

    @MainActor
    func testCacheRebuildsStaticOnlyOnGenerationChange() {
        let store = RepositorySummaryCacheStore()
        var staticBuilds = 0
        var liveBuilds = 0

        func fetch(generation: UInt64, runtimeGeneration: UInt64) {
            _ = store.summaries(
                inputsGeneration: generation,
                runtimeGeneration: runtimeGeneration,
                buildStatic: {
                    staticBuilds += 1
                    return [summary()]
                },
                buildLive: { staticSummaries in
                    liveBuilds += 1
                    return staticSummaries
                }
            )
        }

        fetch(generation: 1, runtimeGeneration: 1)
        fetch(generation: 1, runtimeGeneration: 1)
        XCTAssertEqual(staticBuilds, 1)
        XCTAssertEqual(liveBuilds, 1)

        fetch(generation: 1, runtimeGeneration: 2)
        XCTAssertEqual(staticBuilds, 1)
        XCTAssertEqual(liveBuilds, 2)

        fetch(generation: 2, runtimeGeneration: 2)
        XCTAssertEqual(staticBuilds, 2)
        XCTAssertEqual(liveBuilds, 3)
    }

    // MARK: repository refresh state

    func testRepositoryInventoryRefreshStateDrivesBackgroundScanToast() {
        let state = RepositoryInventoryRefreshState(
            phase: .scanningForNewRepositories,
            checkedRepositoryCount: 12,
            changedRepositoryCount: 2,
            missingRepositoryCount: 0,
            sampleRoots: ["/tmp/NewRepo", "/tmp/OtherRepo"]
        )

        XCTAssertEqual(state.title, "Scanning for new repos")
        XCTAssertTrue(state.detail.contains("Cached repositories are visible"))
        XCTAssertTrue(state.detail.contains("2 new repository candidates"))
        XCTAssertTrue(state.detail.contains("/tmp/NewRepo"))
        XCTAssertEqual(
            state.accessibilityIdentifier,
            "repository.inventory.refresh.scanningForNewRepositories"
        )
    }

    func testRepositoryInventoryRefreshStateSummarizesFingerprintAudit() {
        let state = RepositoryInventoryRefreshState(
            phase: .refreshingChangedRepositories,
            checkedRepositoryCount: 8,
            changedRepositoryCount: 1,
            missingRepositoryCount: 1,
            sampleRoots: ["/tmp/ChangedRepo"]
        )

        XCTAssertEqual(state.title, "Refreshing changed repos")
        XCTAssertTrue(state.detail.contains("2 repository fingerprints changed"))
        XCTAssertTrue(state.detail.contains("/tmp/ChangedRepo"))
        XCTAssertEqual(
            state.accessibilityIdentifier,
            "repository.inventory.refresh.refreshingChangedRepositories"
        )
    }
}
