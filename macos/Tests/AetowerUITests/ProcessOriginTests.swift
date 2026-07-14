import XCTest
import AetowerBridge
@testable import AetowerUI

final class ProcessOriginTests: XCTestCase {
    private func entity(
        id: String,
        kind: EntityKind,
        name: String,
        bundleId: String? = nil,
        executablePath: String?,
        parentSummary: String? = nil
    ) -> EntitySnapshot {
        let component = ComponentSnapshot(
            kind: .process,
            title: name,
            detail: "",
            adapterContext: nil,
            provenance: nil,
            processId: nil,
            startTimeMillis: 0,
            executablePath: executablePath,
            commandLine: nil,
            parentSummary: parentSummary,
            launchedBy: nil,
            cpuPercent: 0,
            memoryBytes: 0,
            memoryPhysicalFootprintBytes: 0,
            cwd: nil,
            user: nil,
            threadCount: 0
        )
        return EntitySnapshot(
            entityId: id,
            displayName: name,
            primaryProvenance: nil,
            launcherSummary: nil,
            attributionNotes: [],
            bundleId: bundleId,
            executablePath: executablePath,
            oldestProcessStartMillis: 0,
            newestProcessStartMillis: 0,
            entityKind: kind,
            metrics: AggregateMetrics(
                cpuPercent: 0,
                memoryResidentBytes: 0,
                memoryPhysicalFootprintBytes: 0,
                diskReadBps: 0,
                diskWriteBps: 0,
                networkReceiveBps: 0,
                networkSendBps: 0,
                wakeupsPerSecond: 0,
                energyNjPerS: 0,
                estimatedGpuPercent: 0,
                processCount: 1,
                threadCount: 0,
                isForeground: false
            ),
            friction: FrictionBreakdown(
                totalScore: 0,
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
            processLineage: [],
            trend: MetricTrend(
                friction: [],
                cpuPercent: [],
                memoryResidentBytes: [],
                diskActivityBps: [],
                networkActivityBps: [],
                wakeupsPerSecond: []
            ),
            badges: [],
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

    func testShellParentClassifiesClaudeCodeAsCli() {
        let origin = processOrigin(from: ProcessOriginSignals(
            entityKind: .aiAgent,
            displayName: "Claude Code",
            bundleId: nil,
            executablePath: "/opt/homebrew/bin/claude",
            componentExecutablePaths: ["/opt/homebrew/bin/claude"],
            commandLines: ["claude --resume abc"],
            parentSummaries: ["zsh (pid 30341)"],
            launchedByValues: [],
            provenanceLabels: ["Shell session"],
            provenanceKindNames: ["shellSession"],
            adapterKindNames: ["chau7Session"],
            users: ["christophehenner"]
        ))

        XCTAssertEqual(origin.kind, .cli)
        XCTAssertTrue(origin.subtitle.contains("CLI"))
        XCTAssertTrue(origin.subtitle.contains("Chau7"))
        XCTAssertTrue(origin.searchTokens.contains("origin:cli"))
        XCTAssertTrue(origin.searchTokens.contains("host:chau7-session"))
    }

    func testApplicationBundleClassifiesClaudeDesktopAsApp() {
        let origin = processOrigin(from: ProcessOriginSignals(
            entityKind: .app,
            displayName: "Claude",
            bundleId: "com.anthropic.claudefordesktop",
            executablePath: "/Applications/Claude.app/Contents/MacOS/Claude",
            componentExecutablePaths: ["/Applications/Claude.app/Contents/MacOS/Claude"],
            commandLines: [],
            parentSummaries: ["launchd (pid 1)"],
            launchedByValues: [],
            provenanceLabels: ["Application bundle"],
            provenanceKindNames: ["appBundle"],
            adapterKindNames: [],
            users: ["christophehenner"]
        ))

        XCTAssertEqual(origin.kind, .app)
        XCTAssertTrue(origin.subtitle.contains("com.anthropic.claudefordesktop"))
        XCTAssertTrue(origin.searchTokens.contains("origin:app"))
    }

    func testBundledHelperClassifiesAsHelperBeforeApp() {
        let origin = processOrigin(from: ProcessOriginSignals(
            entityKind: .unknown,
            displayName: "Chrome Helper",
            bundleId: nil,
            executablePath: "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper",
            componentExecutablePaths: [
                "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper"
            ],
            commandLines: [],
            parentSummaries: ["Google Chrome (pid 700)"],
            launchedByValues: [],
            provenanceLabels: ["Helper tree"],
            provenanceKindNames: ["helperTree"],
            adapterKindNames: [],
            users: ["christophehenner"]
        ))

        XCTAssertEqual(origin.kind, .helper)
        XCTAssertTrue(origin.subtitle.contains("Helper"))
        XCTAssertTrue(origin.searchTokens.contains("origin:helper"))
    }

    func testShellSessionWinsOverHelperParentText() {
        let origin = processOrigin(from: ProcessOriginSignals(
            entityKind: .terminalSession,
            displayName: "zsh",
            bundleId: nil,
            executablePath: "/bin/zsh",
            componentExecutablePaths: ["/bin/zsh"],
            commandLines: ["zsh"],
            parentSummaries: ["Google Chrome Helper (pid 41421)"],
            launchedByValues: [],
            provenanceLabels: ["Helper tree", "Shell session"],
            provenanceKindNames: ["helperTree", "shellSession"],
            adapterKindNames: [],
            users: ["christophehenner"]
        ))

        XCTAssertEqual(origin.kind, .cli)
        XCTAssertTrue(origin.subtitle.contains("Command-line"))
        XCTAssertTrue(origin.searchTokens.contains("origin:cli"))
    }

    func testBrowserMainProcessWithShellParentRemainsApp() {
        let origin = processOrigin(from: ProcessOriginSignals(
            entityKind: .browser,
            displayName: "Google Chrome",
            bundleId: "local.google-chrome",
            executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            componentExecutablePaths: ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"],
            commandLines: ["Google Chrome --remote-debugging-pipe"],
            parentSummaries: ["zsh (pid 42717)"],
            launchedByValues: [],
            provenanceLabels: ["Application bundle"],
            provenanceKindNames: ["appBundle"],
            adapterKindNames: [],
            users: ["christophehenner"]
        ))

        XCTAssertEqual(origin.kind, .app)
        XCTAssertTrue(origin.searchTokens.contains("origin:app"))
    }

    func testSnapshotCacheReusesClassificationBySequenceAndEntityId() {
        let appEntity = entity(
            id: "same",
            kind: .app,
            name: "Claude",
            bundleId: "com.anthropic.claudefordesktop",
            executablePath: "/Applications/Claude.app/Contents/MacOS/Claude"
        )
        let cliEntityWithSameId = entity(
            id: "same",
            kind: .aiAgent,
            name: "Claude Code",
            executablePath: "/opt/homebrew/bin/claude",
            parentSummary: "zsh (pid 30341)"
        )

        let cache = ProcessOriginSnapshotCache(sequence: 42, entities: [appEntity])
        XCTAssertEqual(cache.summary(for: cliEntityWithSameId).kind, .app)

        let refreshed = ProcessOriginSnapshotCache(sequence: 43, entities: [cliEntityWithSameId])
        XCTAssertEqual(refreshed.summary(for: cliEntityWithSameId).kind, .cli)
    }

    func testSnapshotCacheBuildsGroupedOriginFromCachedMembers() {
        let appEntity = entity(
            id: "app",
            kind: .app,
            name: "Claude",
            bundleId: "com.anthropic.claudefordesktop",
            executablePath: "/Applications/Claude.app/Contents/MacOS/Claude"
        )
        let cliEntity = entity(
            id: "cli",
            kind: .aiAgent,
            name: "Claude Code",
            executablePath: "/opt/homebrew/bin/claude",
            parentSummary: "zsh (pid 30341)"
        )

        let cache = ProcessOriginSnapshotCache(sequence: 7, entities: [appEntity, cliEntity])
        let grouped = cache.summary(for: [appEntity, cliEntity])

        XCTAssertEqual(grouped.kind, .mixed)
        XCTAssertTrue(grouped.searchTokens.contains("origin:app"))
        XCTAssertTrue(grouped.searchTokens.contains("origin:cli"))
    }
}
