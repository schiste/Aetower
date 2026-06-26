import XCTest
import AetowerBridge
@testable import AetowerUI

final class MonitorViewModelTests: XCTestCase {
    func testDeltaMergesCardsRowsAndRemovals() {
        var model = MonitorViewModel()
        model.metricCards = [
            metricCard(id: "cpu", displayValue: "10%"),
            metricCard(id: "memory", displayValue: "4 GB"),
        ]
        model.processRows = [
            processRow(id: "pid:1", name: "old"),
            processRow(id: "pid:2", name: "remove-me"),
        ]

        model.apply(delta: UiSnapshotDelta(
            updated: true,
            sequence: 42,
            baseSequence: 41,
            capturedAtMillis: 1_000,
            processLimit: 160,
            trendPoints: 120,
            baseAvailable: true,
            host: nil,
            hostTrend: nil,
            changedMetricCards: [metricCard(id: "cpu", displayValue: "25%")],
            changedProcessRows: [processRow(id: "pid:3", name: "new")],
            removedEntityIds: ["pid:2"],
            selectedEntity: nil,
            selectedEntityChanged: false,
            selectedEntityRemoved: false,
            totalEntityCount: 2,
            returnedEntityCount: 2,
            totalProcessCount: 3,
            timelineWarningCount: 4,
            timelineCriticalCount: 5
        ))

        XCTAssertEqual(model.sequence, 42)
        XCTAssertEqual(model.metricCards.map(\.displayValue), ["25%", "4 GB"])
        XCTAssertEqual(model.processRows.map(\.entityId), ["pid:1", "pid:3"])
        XCTAssertEqual(model.lastDeltaChangedMetricCount, 1)
        XCTAssertEqual(model.lastDeltaChangedRowCount, 1)
        XCTAssertEqual(model.lastDeltaRemovedRowCount, 1)
        XCTAssertTrue(model.lastDeltaBaseAvailable)
        XCTAssertEqual(model.timelineWarningCount, 4)
        XCTAssertEqual(model.timelineCriticalCount, 5)
    }

    func testNoUpdateDeltaOnlyRefreshesDiagnosticsCounters() {
        var model = MonitorViewModel()
        model.sequence = 7
        model.metricCards = [metricCard(id: "cpu", displayValue: "10%")]

        model.apply(delta: UiSnapshotDelta(
            updated: false,
            sequence: 7,
            baseSequence: 7,
            capturedAtMillis: 1_000,
            processLimit: 160,
            trendPoints: 120,
            baseAvailable: true,
            host: nil,
            hostTrend: nil,
            changedMetricCards: [metricCard(id: "cpu", displayValue: "99%")],
            changedProcessRows: [processRow(id: "pid:99", name: "ignored")],
            removedEntityIds: ["pid:1"],
            selectedEntity: nil,
            selectedEntityChanged: false,
            selectedEntityRemoved: false,
            totalEntityCount: 0,
            returnedEntityCount: 0,
            totalProcessCount: 0,
            timelineWarningCount: 0,
            timelineCriticalCount: 0
        ))

        XCTAssertEqual(model.sequence, 7)
        XCTAssertEqual(model.metricCards.map(\.displayValue), ["10%"])
        XCTAssertEqual(model.lastDeltaChangedMetricCount, 0)
        XCTAssertEqual(model.lastDeltaChangedRowCount, 0)
        XCTAssertEqual(model.lastDeltaRemovedRowCount, 0)
    }

    private func metricCard(id: String, displayValue: String) -> UiMetricCard {
        UiMetricCard(
            id: id,
            title: id.capitalized,
            value: 0,
            unit: "percent",
            displayValue: displayValue,
            detail: "detail",
            severity: .normal,
            samples: [0, 1],
            fixedCeiling: 100
        )
    }

    private func processRow(id: String, name: String) -> UiProcessRow {
        UiProcessRow(
            entityId: id,
            displayName: name,
            entityKind: .unknown,
            bundleId: nil,
            executablePath: nil,
            primaryPid: nil,
            processCount: 1,
            threadCount: 1,
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
            frictionScore: 0,
            isForeground: false,
            anomalyDetected: false,
            activeWindowTitle: nil,
            recentChangeSummary: nil,
            signingClassification: "unknown",
            isAdhoc: false,
            appVersion: nil
        )
    }
}
