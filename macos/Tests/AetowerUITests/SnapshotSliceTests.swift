import Observation
import XCTest
import AetowerBridge

@testable import AetowerUI

/// Proves the change-gating contract of AppState's snapshot slices: hot
/// slices republish every tick, while rare-change slices are only reassigned
/// (and therefore only invalidate their observers) when content changed.
/// Sendable flag for observing invalidation from the @Sendable onChange closure.
private final class InvalidationFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var raised = false
    var value: Bool {
        lock.lock()
        defer { lock.unlock() }
        return raised
    }
    func raise() {
        lock.lock()
        raised = true
        lock.unlock()
    }
}

@MainActor
final class SnapshotSliceTests: XCTestCase {
    private func makeState() -> AppState {
        AppState()
    }

    private func snapshot(
        sequence: UInt64,
        cpuPercent: Float = 0,
        timeline: [TimelineEvent] = []
    ) -> SystemSnapshot {
        var snapshot = AppState.emptySnapshot()
        snapshot.sequence = sequence
        snapshot.capturedAtMillis = sequence * 1_000
        snapshot.host.cpuPercent = cpuPercent
        snapshot.timeline = timeline
        return snapshot
    }

    private func timelineEvent(id: String) -> TimelineEvent {
        TimelineEvent(
            id: id,
            timestampMillis: 1,
            category: .anomaly,
            severity: .info,
            entityId: nil,
            title: id,
            detail: ""
        )
    }

    func testUnchangedTimelineIsNotRepublished() {
        let state = makeState()
        let events = [timelineEvent(id: "a")]
        state.publishSnapshotSlices(snapshot(sequence: 1, timeline: events))

        let timelineInvalidated = InvalidationFlag()
        withObservationTracking {
            _ = state.timelineState
        } onChange: {
            timelineInvalidated.raise()
        }

        // Same timeline content, new host data: hot slices move, timeline must not.
        state.publishSnapshotSlices(snapshot(sequence: 2, cpuPercent: 50, timeline: events))
        XCTAssertFalse(timelineInvalidated.value, "timeline slice republished without a content change")
        XCTAssertEqual(state.snapshotSequence, 2)
        XCTAssertEqual(state.hostState.cpuPercent, 50)
    }

    func testChangedTimelineIsRepublished() {
        let state = makeState()
        state.publishSnapshotSlices(snapshot(sequence: 1, timeline: [timelineEvent(id: "a")]))

        let timelineInvalidated = InvalidationFlag()
        withObservationTracking {
            _ = state.timelineState
        } onChange: {
            timelineInvalidated.raise()
        }

        state.publishSnapshotSlices(
            snapshot(sequence: 2, timeline: [timelineEvent(id: "a"), timelineEvent(id: "b")])
        )
        XCTAssertTrue(timelineInvalidated.value, "timeline slice must republish when events change")
        XCTAssertEqual(state.timelineState.count, 2)
    }

    func testHotSlicesRepublishEveryTick() {
        let state = makeState()
        state.publishSnapshotSlices(snapshot(sequence: 1))

        let hostInvalidated = InvalidationFlag()
        withObservationTracking {
            _ = state.hostState
        } onChange: {
            hostInvalidated.raise()
        }

        state.publishSnapshotSlices(snapshot(sequence: 2, cpuPercent: 10))
        XCTAssertTrue(hostInvalidated.value, "host slice must republish per tick")
    }
}
