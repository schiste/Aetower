import XCTest
@testable import AetowerUI

final class SnapshotRefreshTelemetryTests: XCTestCase {
    func testWindowCountsAttemptsSeparatelyFromMeasuredTimings() {
        var window = UiRefreshTelemetryWindow()
        window.record(
            outcome: .updated,
            timings: SnapshotRefreshPhaseTimings(
                fullSnapshotFetchMillis: 100,
                evaluatorSnapshotFetchMillis: 0,
                monitorPayloadFetchMillis: 30,
                runtimeLagFetchMillis: 2,
                operatorStateFetchMillis: 0,
                workerTotalMillis: 140,
                workerTotalAttempted: true,
                fullSnapshotFetchAttempted: true,
                evaluatorSnapshotFetchAttempted: false,
                monitorPayloadFetchAttempted: true,
                runtimeLagFetchAttempted: true,
                operatorStateFetchAttempted: false
            ),
            fullSnapshotPublished: true,
            monitorPayloadKind: "delta",
            publishMillis: 25,
            uiRefreshMillis: 300,
            atMillis: 1_000
        )
        window.record(
            outcome: .noChange,
            timings: SnapshotRefreshPhaseTimings(
                fullSnapshotFetchMillis: nil,
                evaluatorSnapshotFetchMillis: nil,
                monitorPayloadFetchMillis: 0,
                runtimeLagFetchMillis: 0,
                operatorStateFetchMillis: nil,
                workerTotalMillis: 20,
                workerTotalAttempted: true,
                fullSnapshotFetchAttempted: false,
                evaluatorSnapshotFetchAttempted: false,
                monitorPayloadFetchAttempted: true,
                runtimeLagFetchAttempted: true,
                operatorStateFetchAttempted: false
            ),
            fullSnapshotPublished: false,
            monitorPayloadKind: "delta-no-update",
            publishMillis: 0,
            uiRefreshMillis: 20,
            atMillis: 1_010
        )
        let failedTimings = SnapshotRefreshPhaseTimings(
            fullSnapshotFetchMillis: nil,
            evaluatorSnapshotFetchMillis: nil,
            monitorPayloadFetchMillis: nil,
            runtimeLagFetchMillis: nil,
            operatorStateFetchMillis: nil,
            workerTotalMillis: 80,
            workerTotalAttempted: true,
            fullSnapshotFetchAttempted: true
        )
        XCTAssertNil(failedTimings.bridgeCallMillis)
        window.record(
            outcome: .failed,
            timings: failedTimings,
            fullSnapshotPublished: false,
            monitorPayloadKind: "none",
            publishMillis: 0,
            uiRefreshMillis: 80,
            atMillis: 1_020
        )

        let sample = window.snapshot
        XCTAssertEqual(sample.refreshCount, 3)
        XCTAssertEqual(sample.updatedCount, 1)
        XCTAssertEqual(sample.noChangeCount, 1)
        XCTAssertEqual(sample.failedCount, 1)
        XCTAssertEqual(sample.slowCount, 1)
        XCTAssertEqual(sample.fullSnapshotFetchCount, 2)
        XCTAssertEqual(sample.fullSnapshotFetchTimingSampleCount, 1)
        XCTAssertEqual(sample.fullSnapshotPublishedCount, 1)
        XCTAssertEqual(sample.monitorPayloadFetchCount, 2)
        XCTAssertEqual(sample.monitorPayloadFetchTimingSampleCount, 2)
        XCTAssertEqual(sample.monitorDeltaCount, 2)
        XCTAssertEqual(sample.runtimeLagFetchCount, 2)
        XCTAssertEqual(sample.runtimeLagFetchTimingSampleCount, 2)
        XCTAssertEqual(sample.operatorStateFetchCount, 0)
        XCTAssertEqual(sample.operatorStateFetchTimingSampleCount, 0)
        XCTAssertEqual(sample.workerTotalCount, 3)
        XCTAssertEqual(sample.workerTotalTimingSampleCount, 3)
        XCTAssertEqual(sample.meanFullSnapshotFetchMillis ?? -1, 100, accuracy: 0.001)
        XCTAssertNil(sample.meanEvaluatorSnapshotFetchMillis)
        XCTAssertEqual(sample.meanMonitorPayloadFetchMillis ?? -1, 15, accuracy: 0.001)
        XCTAssertEqual(sample.meanRuntimeLagFetchMillis ?? -1, 1, accuracy: 0.001)
        XCTAssertNil(sample.meanOperatorStateFetchMillis)
        XCTAssertEqual(sample.meanWorkerTotalMillis ?? -1, 80, accuracy: 0.001)
        XCTAssertEqual(sample.under50MillisCount, 1)
        XCTAssertEqual(sample.from50To250MillisCount, 1)
        XCTAssertEqual(sample.from250To1000MillisCount, 1)
        XCTAssertEqual(sample.over1000MillisCount, 0)
        XCTAssertEqual(sample.windowDurationMillis, 20)
    }

    func testResetStartsANewIndependentWindow() {
        var window = UiRefreshTelemetryWindow()
        window.record(
            outcome: .updated,
            timings: .zero,
            fullSnapshotPublished: false,
            monitorPayloadKind: "delta",
            publishMillis: 1,
            uiRefreshMillis: 40,
            atMillis: 1_000
        )

        window.reset()

        XCTAssertEqual(window.snapshot.refreshCount, 0)
        XCTAssertEqual(window.snapshot.slowCount, 0)
        XCTAssertNil(window.snapshot.meanUiRefreshMillis)
        XCTAssertEqual(window.snapshot.windowDurationMillis, 0)
    }
}
