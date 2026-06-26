import Foundation
import AetowerBridge

private enum MonitorUiPayloadSizing {
    /// Fixed graph budget for compact Monitor payloads. Rust resamples each
    /// non-empty series to exactly this count, so Swift draw cost stays stable.
    static let graphPointCount: UInt32 = 60
}

actor SnapshotRefreshWorker {
    private let bridge: EngineBridge
    private var lastObservedSequence: UInt64
    private var lastMonitorUiSequence: UInt64

    init(bridge: EngineBridge, initialSequence: UInt64) {
        self.bridge = bridge
        self.lastObservedSequence = initialSequence
        self.lastMonitorUiSequence = initialSequence
    }

    func refresh(
        force: Bool,
        includeOperatorState: Bool,
        monitorSelectedEntityId: String? = nil,
        monitorProcessLimit: UInt32 = 160,
        monitorTrendPoints: UInt32 = MonitorUiPayloadSizing.graphPointCount
    ) throws -> SnapshotRefreshResult {
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
        let monitorPayload = try? fetchMonitorPayload(
            force: force,
            processLimit: monitorProcessLimit,
            trendPoints: monitorTrendPoints,
            selectedEntityId: monitorSelectedEntityId
        )

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
            monitorPayload: monitorPayload,
            bridgeFetchMillis: bridgeFetchMillis
        ))
    }

    private func fetchMonitorPayload(
        force: Bool,
        processLimit: UInt32,
        trendPoints: UInt32,
        selectedEntityId: String?
    ) throws -> MonitorUiRefreshPayload {
        if force || lastMonitorUiSequence == 0 {
            let snapshot = try bridge.latestUiSnapshot(
                processLimit: processLimit,
                trendPoints: trendPoints,
                selectedEntityId: selectedEntityId
            )
            lastMonitorUiSequence = snapshot.sequence
            return .snapshot(snapshot)
        }

        let delta = try bridge.latestUiSnapshotDeltaSince(
            lastMonitorUiSequence,
            processLimit: processLimit,
            trendPoints: trendPoints,
            selectedEntityId: selectedEntityId
        )
        guard delta.updated else {
            return .delta(delta)
        }

        if !delta.baseAvailable {
            let snapshot = try bridge.latestUiSnapshot(
                processLimit: processLimit,
                trendPoints: trendPoints,
                selectedEntityId: selectedEntityId
            )
            lastMonitorUiSequence = snapshot.sequence
            return .snapshot(snapshot)
        }

        lastMonitorUiSequence = delta.sequence
        return .delta(delta)
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
    let monitorPayload: MonitorUiRefreshPayload?
    let bridgeFetchMillis: Double
}

struct SnapshotOperatorRefreshPayload: Sendable {
    let diagnosticsOverview: DiagnosticsOverview
    let historyStoreSummary: HistoryRangeSummary?
}

enum MonitorUiRefreshPayload: Sendable {
    case snapshot(UiSnapshot)
    case delta(UiSnapshotDelta)
}
