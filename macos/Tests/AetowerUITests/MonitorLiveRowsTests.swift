import XCTest
import AetowerBridge
@testable import AetowerUI

final class MonitorLiveRowsTests: XCTestCase {
    func testLiveProjectionFiltersSupportedBundleAndNumericTokensThenSortsByMetric() {
        let rows = [
            processRow(id: "z", name: "Zulu Agent", cpu: 72, friction: 10),
            processRow(id: "a", name: "Alpha Agent", cpu: 80, friction: 2),
            processRow(id: "b", name: "Beta Helper", cpu: 30, friction: 30),
        ]

        let projected = MonitorLiveRowsProjection.filterAndSort(
            rows: rows,
            query: "bundle:org.example cpu>50",
            sortKey: .cpu,
            advancedFilterEntityIds: nil
        )

        XCTAssertEqual(projected.map(\.entityId), ["a", "z"])
    }

    func testMemoryFilterPreservesEntitySearchBinaryUnitsAndResidentSemantics() {
        let rows = [
            processRow(
                id: "resident-under",
                name: "Under Resident Threshold",
                cpu: 1,
                friction: 1,
                memoryResidentBytes: 100_000_000,
                memoryPhysicalFootprintBytes: 200_000_000
            ),
            processRow(
                id: "resident-over",
                name: "Over Resident Threshold",
                cpu: 1,
                friction: 1,
                memoryResidentBytes: 110_000_000,
                memoryPhysicalFootprintBytes: 110_000_000
            ),
        ]

        let projected = MonitorLiveRowsProjection.filterAndSort(
            rows: rows,
            query: "memory>100MB",
            sortKey: .cpu,
            advancedFilterEntityIds: nil
        )

        XCTAssertEqual(projected.map(\.entityId), ["resident-over"])
    }

    func testLiveProjectionHonorsAdvancedFilterAndStableNameTieBreak() {
        let rows = [
            processRow(id: "z", name: "Same", cpu: 20, friction: 5),
            processRow(id: "a", name: "Same", cpu: 20, friction: 5),
            processRow(id: "b", name: "Other", cpu: 20, friction: 5),
        ]

        let projected = MonitorLiveRowsProjection.filterAndSort(
            rows: rows,
            query: "",
            sortKey: .friction,
            advancedFilterEntityIds: ["z", "a"]
        )

        XCTAssertEqual(projected.map(\.entityId), ["a", "z"])
    }

    func testLiveProjectionRetainsTheFullFiveHundredRowBridgeLimit() {
        let rows = (0..<500).map { index in
            processRow(id: "pid:\(index)", name: "Process \(index)", cpu: Float(index), friction: Float(index % 100))
        }

        let projected = MonitorLiveRowsProjection.filterAndSort(
            rows: rows,
            query: "",
            sortKey: .cpu,
            advancedFilterEntityIds: nil
        )

        XCTAssertEqual(projected.count, 500)
        XCTAssertEqual(projected.first?.entityId, "pid:499")
    }

    func testUnsupportedSearchAndSortFallBackToFullSnapshotPath() {
        XCTAssertFalse(MonitorLiveRowsProjection.supportsLiveSearch("origin:cli"))
        XCTAssertFalse(MonitorLiveRowsProjection.supportsLiveSearch("name:agent"))
        XCTAssertFalse(MonitorLiveRowsProjection.supportsLiveSearch("Agent"))
        XCTAssertFalse(MonitorLiveRowsProjection.supportsLiveSearch("/agent.*/"))
        XCTAssertFalse(MonitorLiveRowsProjection.supportsLiveSearch("pid=42"))
        XCTAssertTrue(MonitorLiveRowsProjection.supportsLiveSearch("bundle:org.example"))
        XCTAssertTrue(MonitorLiveRowsProjection.supportsLiveSearch("cpu>=50 memory>100MB"))
        XCTAssertFalse(MonitorLiveRowsProjection.supportsLiveSort(.processCount))
        XCTAssertFalse(MonitorLiveRowsProjection.supportsLiveSort(.energy))
        XCTAssertFalse(MonitorLiveRowsProjection.supportsLiveSort(.newestFirst))
        XCTAssertTrue(MonitorLiveRowsProjection.supportsLiveSort(.cpu))
    }

    func testFlatCompleteUnfilteredMonitorDoesNotDemandFullSnapshot() {
        XCTAssertFalse(MonitorSnapshotPolicy.requiresFullSnapshot(
            isGroupedMode: false,
            hasSelectedEntity: false,
            hasDetailedFilter: false,
            hasCompleteLiveRows: true
        ))
    }

    func testGroupedDetailFilteredOrIncompleteMonitorDemandsFullSnapshot() {
        let cases: [(Bool, Bool, Bool, Bool)] = [
            (true, false, false, true),
            (false, true, false, true),
            (false, false, true, true),
            (false, false, false, false),
        ]

        for (isGrouped, hasSelection, hasFilter, hasCompleteRows) in cases {
            XCTAssertTrue(MonitorSnapshotPolicy.requiresFullSnapshot(
                isGroupedMode: isGrouped,
                hasSelectedEntity: hasSelection,
                hasDetailedFilter: hasFilter,
                hasCompleteLiveRows: hasCompleteRows
            ))
        }
    }

    private func processRow(
        id: String,
        name: String,
        cpu: Float,
        friction: Float,
        memoryResidentBytes: UInt64 = 10_000,
        memoryPhysicalFootprintBytes: UInt64 = 12_000
    ) -> UiProcessRow {
        UiProcessRow(
            entityId: id,
            displayName: name,
            entityKind: .unknown,
            bundleId: "org.example.\(id)",
            executablePath: "/Applications/\(name).app/Contents/MacOS/\(name)",
            primaryPid: nil,
            processCount: 1,
            threadCount: 2,
            cpuPercent: cpu,
            memoryResidentBytes: memoryResidentBytes,
            memoryPhysicalFootprintBytes: memoryPhysicalFootprintBytes,
            diskReadBps: 0,
            diskWriteBps: 0,
            networkReceiveBps: 0,
            networkSendBps: 0,
            wakeupsPerSecond: 0,
            energyNjPerS: 0,
            estimatedGpuPercent: 0,
            frictionScore: friction,
            isForeground: false,
            anomalyDetected: false,
            activeWindowTitle: nil,
            recentChangeSummary: nil,
            signingClassification: "signed",
            isAdhoc: false,
            appVersion: nil
        )
    }
}
