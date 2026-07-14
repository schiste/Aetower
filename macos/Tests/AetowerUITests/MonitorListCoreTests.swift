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

    func testExpandedProcessComponentsIncludeRootPIDsAndFollowProcessSort() {
        let root = entity(
            id: "chau7",
            name: "Chau7",
            components: [
                component(title: "Chau7", pid: 100, cpu: 4, memory: 10),
                component(title: "codex", pid: 101, cpu: 12, memory: 20),
                component(title: "zsh", pid: 102, cpu: 1, memory: 30),
            ]
        )
        let group = EntityGroup(
            root: root,
            members: [root],
            cpuPercent: 17,
            memoryBytes: 60,
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

        XCTAssertEqual(expandedProcessComponents(for: group, by: .cpu).map(\.pid), [101, 100, 102])
    }

    func testStructuredLineageGroupsAgentGrandchildrenWithoutSharedWorkspaceContext() throws {
        let terminal = entity(
            id: "terminal",
            name: "Terminal Host",
            kind: .terminalSession,
            executablePath: "/Applications/Terminal Host.app/Contents/MacOS/Terminal Host",
            processLineage: [
                lineage(title: "Terminal Host", pid: 700, entityID: "terminal", executablePath: "/Applications/Terminal Host.app/Contents/MacOS/Terminal Host"),
            ]
        )
        let codex = entity(
            id: "codex",
            name: "codex",
            kind: .aiAgent,
            executablePath: "/opt/homebrew/bin/codex",
            processLineage: [
                lineage(title: "codex", pid: 701, parentPid: 702, entityID: "codex", executablePath: "/opt/homebrew/bin/codex", cwd: "/Users/me/ProjectA"),
            ]
        )
        let zsh = entity(
            id: "zsh",
            name: "zsh",
            kind: .terminalSession,
            executablePath: "/bin/zsh",
            processLineage: [
                lineage(title: "zsh", pid: 702, parentPid: 700, entityID: "zsh", executablePath: "/bin/zsh", cwd: "/Users/me/ProjectB"),
            ]
        )

        let groups = buildEntityGroups(from: [codex, terminal, zsh])
        let group = try XCTUnwrap(groups.first { $0.root.entityId == "terminal" })

        XCTAssertEqual(Set(group.members.map(\.entityId)), Set(["terminal", "codex", "zsh"]))
        XCTAssertEqual(group.processCount, 3)
    }

    func testGroupedSearchPreservesAncestorsForMatchingLineageChild() throws {
        let terminal = entity(
            id: "terminal",
            name: "Terminal Host",
            kind: .terminalSession,
            processLineage: [
                lineage(title: "Terminal Host", pid: 700, entityID: "terminal"),
            ]
        )
        let shell = entity(
            id: "zsh",
            name: "zsh",
            kind: .terminalSession,
            processLineage: [
                lineage(title: "zsh", pid: 702, parentPid: 700, entityID: "zsh"),
            ]
        )
        let agent = entity(
            id: "codex",
            name: "codex",
            kind: .aiAgent,
            processLineage: [
                lineage(title: "codex", pid: 701, parentPid: 702, entityID: "codex"),
            ]
        )
        let originCache = ProcessOriginSnapshotCache(sequence: 1, entities: [terminal, shell, agent])

        let groups = buildGroupedEntities(
            from: [agent, terminal, shell],
            query: "codex",
            sortKey: .friction,
            originCache: originCache
        )
        let group = try XCTUnwrap(groups.first)

        XCTAssertEqual(groups.count, 1)
        XCTAssertEqual(group.root.entityId, "terminal")
        XCTAssertEqual(Set(group.members.map(\.entityId)), Set(["terminal", "zsh", "codex"]))
    }

    @MainActor
    func testGroupRowAnnotatesBurdenLeaderWithoutDroppingGroup() throws {
        let root = entity(id: "root", name: "Root", memory: 1_024)
        let child = entity(id: "child", name: "Child", memory: 512)
        let group = EntityGroup(
            root: root,
            members: [root, child],
            cpuPercent: 0,
            memoryBytes: 1_536,
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
        let key = MonitorGroupRowCacheKey(
            groupingKey: GroupingCacheKey(
                sequence: 1,
                query: "",
                originFilter: .all,
                sortKey: .friction,
                filterSignature: ""
            ),
            burdenLeaderSignature: ["root:memory:1 KB:1"],
            groupEntityIDs: ["root"]
        )
        let leader = BurdenLeaderSummary(
            id: "memory",
            title: "Memory leader",
            entityId: "root",
            entityName: "Root",
            metricValue: "1 KB",
            detail: "Largest charged memory footprint in the current snapshot.",
            severity: .warning
        )
        let cache = ProcessOriginSnapshotCache(sequence: 1, entities: [root, child])
        let section = MonitorGroupRowCacheStore().section(
            for: key,
            groups: [group],
            originCache: cache,
            burdenLeaderSummariesByEntityID: ["root": [leader]]
        )

        XCTAssertEqual(section.rows.map(\.id), ["root"])
        XCTAssertEqual(section.rows.first?.burdenLeaderText, "Memory: 1 KB")
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
        badges: [String] = [],
        executablePath: String? = nil,
        components: [ComponentSnapshot]? = nil,
        processLineage: [ProcessLineageNode]? = nil
    ) -> EntitySnapshot {
        let resolvedComponents = components ?? [
            component(title: name, pid: UInt32(abs(id.hashValue % 10_000) + 1), cpu: cpu, memory: memory),
        ]
        let resolvedLineage = processLineage ?? []
        let resolvedCPU = components == nil ? cpu : resolvedComponents.reduce(0) { $0 + $1.cpuPercent }
        let resolvedMemory = components == nil ? memory : resolvedComponents.reduce(0) {
            $0 + max($1.memoryPhysicalFootprintBytes, $1.memoryBytes)
        }
        let processCount = resolvedComponents.filter {
            $0.kind != .adapterContext && $0.processId != nil
        }.count
        let threadCount = resolvedComponents.reduce(UInt32(0)) { $0 + $1.threadCount }
        let starts = resolvedComponents.map(\.startTimeMillis).filter { $0 > 0 }
        let resolvedExecutablePath = executablePath ?? "/usr/bin/\(name)"

        return EntitySnapshot(
            entityId: id,
            displayName: name,
            primaryProvenance: nil,
            launcherSummary: nil,
            attributionNotes: [],
            bundleId: nil,
            executablePath: resolvedExecutablePath,
            oldestProcessStartMillis: starts.min() ?? 1_000,
            newestProcessStartMillis: starts.max() ?? 1_000,
            entityKind: kind,
            metrics: AggregateMetrics(
                cpuPercent: resolvedCPU,
                memoryResidentBytes: resolvedMemory,
                memoryPhysicalFootprintBytes: resolvedMemory,
                diskReadBps: 0,
                diskWriteBps: 0,
                networkReceiveBps: 0,
                networkSendBps: 0,
                wakeupsPerSecond: 0,
                energyNjPerS: 0,
                estimatedGpuPercent: 0,
                processCount: UInt32(processCount),
                threadCount: threadCount,
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
            components: resolvedComponents,
            processLineage: resolvedLineage,
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

    private func component(
        title: String,
        pid: UInt32,
        cpu: Float = 0,
        memory: UInt64 = 0,
        parentSummary: String? = nil,
        executablePath: String? = nil,
        cwd: String? = nil
    ) -> ComponentSnapshot {
        ComponentSnapshot(
            kind: .process,
            title: title,
            detail: "",
            adapterContext: nil,
            provenance: nil,
            processId: pid,
            startTimeMillis: 1_000,
            executablePath: executablePath ?? "/usr/bin/\(title)",
            commandLine: nil,
            parentSummary: parentSummary,
            launchedBy: nil,
            cpuPercent: cpu,
            memoryBytes: memory,
            memoryPhysicalFootprintBytes: memory,
            cwd: cwd,
            user: nil,
            threadCount: 1
        )
    }

    private func lineage(
        title: String,
        pid: UInt32,
        parentPid: UInt32? = nil,
        entityID: String,
        cpu: Float = 0,
        memory: UInt64 = 0,
        executablePath: String? = nil,
        cwd: String? = nil,
        source: String = "test-lineage",
        confidence: Float = 1.0
    ) -> ProcessLineageNode {
        ProcessLineageNode(
            pid: pid,
            parentPid: parentPid,
            entityId: entityID,
            title: title,
            startTimeMillis: 1_000,
            executablePath: executablePath ?? "/usr/bin/\(title)",
            commandLine: nil,
            cwd: cwd,
            user: nil,
            sessionId: nil,
            workspace: cwd,
            cpuPercent: cpu,
            memoryBytes: memory,
            memoryPhysicalFootprintBytes: memory,
            threadCount: 1,
            source: source,
            confidence: confidence
        )
    }
}
