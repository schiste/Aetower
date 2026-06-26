import Foundation
import AetowerBridge

actor SnapshotRefreshWorker {
    private let bridge: EngineBridge
    private var lastObservedSequence: UInt64

    init(bridge: EngineBridge, initialSequence: UInt64) {
        self.bridge = bridge
        self.lastObservedSequence = initialSequence
    }

    func refresh(force: Bool, includeOperatorState: Bool) throws -> SnapshotRefreshResult {
        let fetchStartedAt = CFAbsoluteTimeGetCurrent()
        let refreshedSnapshot: SystemSnapshot

        if let updatedSnapshot = try bridge.latestSnapshotIfNewer(since: lastObservedSequence) {
            refreshedSnapshot = updatedSnapshot
        } else if force {
            refreshedSnapshot = try bridge.latestSnapshot()
        } else {
            return .noChange
        }

        let bridgeFetchMillis = (CFAbsoluteTimeGetCurrent() - fetchStartedAt) * 1000.0
        lastObservedSequence = refreshedSnapshot.sequence

        let runtimeLagMetrics = bridge.latestRuntimeLagMetrics()
        let operatorState = includeOperatorState
            ? SnapshotOperatorRefreshPayload(
                diagnosticsOverview: bridge.diagnosticsOverview(),
                historyStoreSummary: bridge.historyRangeSummary(
                    startMillis: 0,
                    endMillis: max(
                        refreshedSnapshot.capturedAtMillis,
                        UInt64(Date().timeIntervalSince1970 * 1000)
                    )
                )
            )
            : nil

        return .updated(SnapshotRefreshPayload(
            snapshot: refreshedSnapshot,
            runtimeLagMetrics: runtimeLagMetrics,
            operatorState: operatorState,
            bridgeFetchMillis: bridgeFetchMillis
        ))
    }
}

enum SnapshotRefreshResult: Sendable {
    case noChange
    case updated(SnapshotRefreshPayload)
}

struct SnapshotRefreshPayload: Sendable {
    let snapshot: SystemSnapshot
    let runtimeLagMetrics: RuntimeLagMetrics
    let operatorState: SnapshotOperatorRefreshPayload?
    let bridgeFetchMillis: Double
}

struct SnapshotOperatorRefreshPayload: Sendable {
    let diagnosticsOverview: DiagnosticsOverview
    let historyStoreSummary: HistoryRangeSummary?
}
