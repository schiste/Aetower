import XCTest
import AetowerBridge
@testable import AetowerUI

final class MonitorListCoreTests: XCTestCase {
    func testMetricSortUsesStableNameTieBreaker() {
        let zulu = entity(id: "z", name: "Zulu", cpu: 10)
        let alpha = entity(id: "a", name: "Alpha", cpu: 10)

        XCTAssertEqual(sortEntities([zulu, alpha], by: .cpu).map(\.entityId), ["a", "z"])
    }

    func testGroupedFrictionIsAggregateBurden() throws {
        let root = entity(
            id: "chau7",
            name: "Chau7",
            kind: .terminalSession,
            friction: 12,
            badges: ["ai-session:s1", "chau7-live"]
        )
        let agent = entity(
            id: "agent",
            name: "Claude Code",
            kind: .aiAgent,
            friction: 21,
            badges: ["ai-session:s1"]
        )

        let groups = buildEntityGroups(from: [agent, root])
        let group = try XCTUnwrap(groups.first { $0.root.entityId == "chau7" })

        XCTAssertEqual(group.members.map(\.entityId), ["agent", "chau7"])
        XCTAssertEqual(group.frictionScore, 33)
    }

    func testGroupedMembersCanUseParentSortKey() {
        let small = entity(id: "small", name: "Small", memory: 10)
        let large = entity(id: "large", name: "Large", memory: 40)
        let group = EntityGroup(
            root: large,
            members: [small, large],
            cpuPercent: 0,
            memoryBytes: 50,
            wakeupsPerSecond: 0,
            diskBps: 0,
            networkBps: 0,
            energyScore: 0,
            frictionScore: 0,
            processCount: 2,
            userSummary: "",
            oldestStartMillis: 0,
            newestStartMillis: 0
        )

        XCTAssertEqual(sortEntities(group.members, by: .memory).map(\.entityId), ["large", "small"])
    }

    func testExpandedGroupMembersExcludeRootAndFollowParentSort() {
        let root = entity(id: "root", name: "Root", memory: 20)
        let small = entity(id: "small", name: "Small", memory: 10)
        let large = entity(id: "large", name: "Large", memory: 40)
        let group = EntityGroup(
            root: root,
            members: [small, root, large],
            cpuPercent: 0,
            memoryBytes: 70,
            wakeupsPerSecond: 0,
            diskBps: 0,
            networkBps: 0,
            energyScore: 0,
            frictionScore: 0,
            processCount: 3,
            userSummary: "",
            oldestStartMillis: 0,
            newestStartMillis: 0
        )

        XCTAssertEqual(expandedMemberEntities(for: group, by: .memory).map(\.entityId), ["large", "small"])
    }

    func testRegexTokenizerKeepsEscapedSlashInsideRegexToken() {
        XCTAssertEqual(
            tokenizeSearchQuery(#"/Users\/me\/Project/i cpu>10"#),
            [#"/Users\/me\/Project/i"#, "cpu>10"]
        )
    }

    func testFilterEntitiesCombinesTextAndNumericPredicates() {
        let match = entity(id: "match", name: "Claude Code", cpu: 25)
        let belowThreshold = entity(id: "low", name: "Claude Helper", cpu: 5)
        let wrongName = entity(id: "other", name: "Safari", cpu: 50)
        let cache = ProcessOriginSnapshotCache(sequence: 1, entities: [match, belowThreshold, wrongName])

        let filtered = filterEntities(
            [match, belowThreshold, wrongName],
            query: "claude cpu>=10",
            originCache: cache
        )

        XCTAssertEqual(filtered.map(\.entityId), ["match"])
    }

    private func entity(
        id: String,
        name: String,
        kind: EntityKind = .unknown,
        cpu: Float = 0,
        memory: UInt64 = 0,
        friction: Float = 0,
        badges: [String] = []
    ) -> EntitySnapshot {
        let component = ComponentSnapshot(
            kind: .process,
            title: name,
            detail: "",
            adapterContext: nil,
            provenance: nil,
            processId: UInt32(abs(id.hashValue % 10_000) + 1),
            startTimeMillis: 1_000,
            executablePath: "/usr/bin/\(name)",
            commandLine: nil,
            parentSummary: nil,
            launchedBy: nil,
            cpuPercent: cpu,
            memoryBytes: memory,
            memoryPhysicalFootprintBytes: memory,
            cwd: nil,
            user: nil,
            threadCount: 1
        )
        return EntitySnapshot(
            entityId: id,
            displayName: name,
            primaryProvenance: nil,
            launcherSummary: nil,
            attributionNotes: [],
            bundleId: nil,
            executablePath: "/usr/bin/\(name)",
            oldestProcessStartMillis: 1_000,
            newestProcessStartMillis: 1_000,
            entityKind: kind,
            metrics: AggregateMetrics(
                cpuPercent: cpu,
                memoryResidentBytes: memory,
                memoryPhysicalFootprintBytes: memory,
                diskReadBps: 0,
                diskWriteBps: 0,
                networkReceiveBps: 0,
                networkSendBps: 0,
                wakeupsPerSecond: 0,
                energyNjPerS: 0,
                estimatedGpuPercent: 0,
                processCount: 1,
                threadCount: 1,
                isForeground: false
            ),
            friction: FrictionBreakdown(
                totalScore: friction,
                cpuScore: 0,
                memoryScore: 0,
                diskScore: 0,
                networkScore: 0,
                wakeupsScore: 0,
                pressureScore: 0,
                foregroundBonus: 0,
                energyImpactScore: 0,
                reasons: [],
                contributors: []
            ),
            components: [component],
            trend: MetricTrend(
                friction: [],
                cpuPercent: [],
                memoryResidentBytes: [],
                diskActivityBps: [],
                networkActivityBps: [],
                wakeupsPerSecond: []
            ),
            badges: badges,
            activeWindowTitle: nil,
            recentChangeSummary: nil,
            anomalyDetected: false,
            thermalContribution: nil,
            groupingSuggestion: nil,
            agentCost: nil,
            sessionMarkers: [],
            recommendations: [],
            networkConnections: [],
            signingClassification: "unknown",
            isAdhoc: false,
            binaryReputation: nil,
            appVersion: nil
        )
    }
}
