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
        collectPayloadDiagnostics: Bool = true,
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
            selectedEntityId: monitorSelectedEntityId,
            collectPayloadDiagnostics: collectPayloadDiagnostics
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
        selectedEntityId: String?,
        collectPayloadDiagnostics: Bool
    ) throws -> MonitorUiRefreshPayload {
        let fetchStartedAt = CFAbsoluteTimeGetCurrent()
        if force || lastMonitorUiSequence == 0 {
            let snapshot = try bridge.latestUiSnapshot(
                processLimit: processLimit,
                trendPoints: trendPoints,
                selectedEntityId: selectedEntityId
            )
            lastMonitorUiSequence = snapshot.sequence
            return .snapshot(
                snapshot,
                MonitorUiPayloadFetchDiagnostics(
                    ffiFetchMillis: elapsedMillis(since: fetchStartedAt),
                    snapshotBytes: collectPayloadDiagnostics ? estimatedByteCount(snapshot) : 0,
                    compactPayloadKind: "snapshot",
                    returnedRowCount: snapshot.processRows.count
                )
            )
        }

        let delta = try bridge.latestUiSnapshotDeltaSince(
            lastMonitorUiSequence,
            processLimit: processLimit,
            trendPoints: trendPoints,
            selectedEntityId: selectedEntityId
        )
        guard delta.updated else {
            return .delta(
                delta,
                MonitorUiPayloadFetchDiagnostics(
                    ffiFetchMillis: elapsedMillis(since: fetchStartedAt),
                    snapshotBytes: collectPayloadDiagnostics ? estimatedByteCount(delta) : 0,
                    compactPayloadKind: "delta-no-update",
                    returnedRowCount: delta.changedProcessRows.count
                )
            )
        }

        if !delta.baseAvailable {
            let snapshot = try bridge.latestUiSnapshot(
                processLimit: processLimit,
                trendPoints: trendPoints,
                selectedEntityId: selectedEntityId
            )
            lastMonitorUiSequence = snapshot.sequence
            return .snapshot(
                snapshot,
                MonitorUiPayloadFetchDiagnostics(
                    ffiFetchMillis: elapsedMillis(since: fetchStartedAt),
                    snapshotBytes: collectPayloadDiagnostics ? estimatedByteCount(snapshot) : 0,
                    compactPayloadKind: "snapshot-fallback",
                    returnedRowCount: snapshot.processRows.count
                )
            )
        }

        lastMonitorUiSequence = delta.sequence
        return .delta(
            delta,
            MonitorUiPayloadFetchDiagnostics(
                ffiFetchMillis: elapsedMillis(since: fetchStartedAt),
                snapshotBytes: collectPayloadDiagnostics ? estimatedByteCount(delta) : 0,
                compactPayloadKind: "delta",
                returnedRowCount: delta.changedProcessRows.count
            )
        )
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
    case snapshot(UiSnapshot, MonitorUiPayloadFetchDiagnostics)
    case delta(UiSnapshotDelta, MonitorUiPayloadFetchDiagnostics)
}

struct MonitorUiPayloadFetchDiagnostics: Sendable {
    let ffiFetchMillis: Double
    let snapshotBytes: UInt64
    let compactPayloadKind: String
    let returnedRowCount: Int
}

private func elapsedMillis(since startedAt: CFAbsoluteTime) -> Double {
    (CFAbsoluteTimeGetCurrent() - startedAt) * 1000.0
}

private func estimatedByteCount(_ snapshot: UiSnapshot) -> UInt64 {
    let metricBytes = snapshot.metricCards.reduce(UInt64(0)) { $0 + estimatedByteCount($1) }
    let rowBytes = snapshot.processRows.reduce(UInt64(0)) { $0 + estimatedByteCount($1) }
    let selectedBytes = snapshot.selectedEntity.map(estimatedByteCount(_:)) ?? 0
    return 256 + estimatedByteCount(snapshot.hostTrend) + metricBytes + rowBytes + selectedBytes
}

private func estimatedByteCount(_ delta: UiSnapshotDelta) -> UInt64 {
    let trendBytes = delta.hostTrend.map(estimatedByteCount(_:)) ?? 0
    let metricBytes = delta.changedMetricCards.reduce(UInt64(0)) { $0 + estimatedByteCount($1) }
    let rowBytes = delta.changedProcessRows.reduce(UInt64(0)) { $0 + estimatedByteCount($1) }
    let removedBytes = delta.removedEntityIds.reduce(UInt64(0)) { $0 + estimatedByteCount($1) }
    let selectedBytes = delta.selectedEntity.map(estimatedByteCount(_:)) ?? 0
    return 192 + trendBytes + metricBytes + rowBytes + removedBytes + selectedBytes
}

private func estimatedByteCount(_ trend: UiHostTrend) -> UInt64 {
    UInt64(
        trend.machineFriction.count
            + trend.cpuPercent.count
            + trend.memoryUsedBytes.count
            + trend.memoryPressureScore.count
            + trend.diskActivityBps.count
            + trend.networkActivityBps.count
            + trend.wakeupsPerSecond.count
            + trend.gpuPercent.count
            + trend.gpuMemoryBytes.count
            + trend.maxCpuTemperature.count
    ) * 8
}

private func estimatedByteCount(_ card: UiMetricCard) -> UInt64 {
    64
        + estimatedByteCount(card.id)
        + estimatedByteCount(card.title)
        + estimatedByteCount(card.unit)
        + estimatedByteCount(card.displayValue)
        + estimatedByteCount(card.detail)
        + UInt64(card.samples.count * 8)
}

private func estimatedByteCount(_ row: UiProcessRow) -> UInt64 {
    96
        + estimatedByteCount(row.entityId)
        + estimatedByteCount(row.displayName)
        + estimatedByteCount(row.bundleId)
        + estimatedByteCount(row.executablePath)
        + estimatedByteCount(row.activeWindowTitle)
        + estimatedByteCount(row.recentChangeSummary)
        + estimatedByteCount(row.signingClassification)
        + estimatedByteCount(row.appVersion)
}

private func estimatedByteCount(_ selected: UiSelectedEntity) -> UInt64 {
    estimatedByteCount(selected.row)
        + selected.components.reduce(0) { $0 + estimatedByteCount($1) }
        + selected.recommendations.reduce(0) { $0 + estimatedByteCount($1.title) + estimatedByteCount($1.detail) }
        + selected.networkConnections.reduce(0) {
            $0 + estimatedByteCount($1.local) + estimatedByteCount($1.remote)
        }
        + selected.badges.reduce(0) { $0 + estimatedByteCount($1) }
        + selected.attributionNotes.reduce(0) { $0 + estimatedByteCount($1) }
        + estimatedByteCount(selected.thermalContribution)
        + estimatedByteCount(selected.groupingSuggestion)
}

private func estimatedByteCount(_ component: UiProcessComponent) -> UInt64 {
    80
        + estimatedByteCount(component.title)
        + estimatedByteCount(component.detail)
        + estimatedByteCount(component.executablePath)
        + estimatedByteCount(component.commandLine)
        + estimatedByteCount(component.parentSummary)
        + estimatedByteCount(component.launchedBy)
        + estimatedByteCount(component.cwd)
        + estimatedByteCount(component.user)
}

private func estimatedByteCount(_ value: String?) -> UInt64 {
    value.map(estimatedByteCount(_:)) ?? 0
}

private func estimatedByteCount(_ value: String) -> UInt64 {
    UInt64(value.utf8.count)
}
