import Foundation

struct SnapshotRefreshPhaseTimings: Sendable {
    let fullSnapshotFetchMillis: Double?
    let evaluatorSnapshotFetchMillis: Double?
    let monitorPayloadFetchMillis: Double?
    let runtimeLagFetchMillis: Double?
    let operatorStateFetchMillis: Double?
    let workerTotalMillis: Double?
    let workerTotalAttempted: Bool
    let fullSnapshotFetchAttempted: Bool
    let evaluatorSnapshotFetchAttempted: Bool
    let monitorPayloadFetchAttempted: Bool
    let runtimeLagFetchAttempted: Bool
    let operatorStateFetchAttempted: Bool

    /// Sum attempted Bridge phases only when every attempted phase was timed.
    /// The existing Rust `bridge_fetch_millis` remains the full snapshot only.
    var bridgeCallMillis: Double? {
        let phases = [
            (fullSnapshotFetchAttempted, fullSnapshotFetchMillis),
            (evaluatorSnapshotFetchAttempted, evaluatorSnapshotFetchMillis),
            (monitorPayloadFetchAttempted, monitorPayloadFetchMillis),
            (runtimeLagFetchAttempted, runtimeLagFetchMillis),
            (operatorStateFetchAttempted, operatorStateFetchMillis)
        ]
        var total = 0.0
        var attemptedPhaseCount = 0
        for (attempted, duration) in phases where attempted {
            guard let duration else { return nil }
            total += duration
            attemptedPhaseCount += 1
        }
        return attemptedPhaseCount == 0 ? nil : total
    }

    static let zero = SnapshotRefreshPhaseTimings(
        fullSnapshotFetchMillis: nil,
        evaluatorSnapshotFetchMillis: nil,
        monitorPayloadFetchMillis: nil,
        runtimeLagFetchMillis: nil,
        operatorStateFetchMillis: nil,
        workerTotalMillis: nil
    )

    init(
        fullSnapshotFetchMillis: Double?,
        evaluatorSnapshotFetchMillis: Double?,
        monitorPayloadFetchMillis: Double?,
        runtimeLagFetchMillis: Double?,
        operatorStateFetchMillis: Double?,
        workerTotalMillis: Double?,
        workerTotalAttempted: Bool = false,
        fullSnapshotFetchAttempted: Bool = false,
        evaluatorSnapshotFetchAttempted: Bool = false,
        monitorPayloadFetchAttempted: Bool = false,
        runtimeLagFetchAttempted: Bool = false,
        operatorStateFetchAttempted: Bool = false
    ) {
        self.fullSnapshotFetchMillis = fullSnapshotFetchMillis
        self.evaluatorSnapshotFetchMillis = evaluatorSnapshotFetchMillis
        self.monitorPayloadFetchMillis = monitorPayloadFetchMillis
        self.runtimeLagFetchMillis = runtimeLagFetchMillis
        self.operatorStateFetchMillis = operatorStateFetchMillis
        self.workerTotalMillis = workerTotalMillis
        self.workerTotalAttempted = workerTotalAttempted
        self.fullSnapshotFetchAttempted = fullSnapshotFetchAttempted
        self.evaluatorSnapshotFetchAttempted = evaluatorSnapshotFetchAttempted
        self.monitorPayloadFetchAttempted = monitorPayloadFetchAttempted
        self.runtimeLagFetchAttempted = runtimeLagFetchAttempted
        self.operatorStateFetchAttempted = operatorStateFetchAttempted
    }
}

enum SnapshotRefreshOutcome: Sendable {
    case updated
    case noChange
    case failed
    case cancelled
}

struct UiRefreshTelemetryWindowSnapshot: Sendable {
    let refreshCount: Int
    let updatedCount: Int
    let noChangeCount: Int
    let failedCount: Int
    let cancelledCount: Int
    let slowCount: Int
    let fullSnapshotFetchCount: Int
    let fullSnapshotFetchTimingSampleCount: Int
    let fullSnapshotPublishedCount: Int
    let evaluatorSnapshotFetchCount: Int
    let evaluatorSnapshotFetchTimingSampleCount: Int
    let monitorPayloadFetchCount: Int
    let monitorPayloadFetchTimingSampleCount: Int
    let monitorDeltaCount: Int
    let monitorSnapshotCount: Int
    let runtimeLagFetchCount: Int
    let runtimeLagFetchTimingSampleCount: Int
    let operatorStateFetchCount: Int
    let operatorStateFetchTimingSampleCount: Int
    let workerTotalCount: Int
    let workerTotalTimingSampleCount: Int
    let meanFullSnapshotFetchMillis: Double?
    let meanEvaluatorSnapshotFetchMillis: Double?
    let meanMonitorPayloadFetchMillis: Double?
    let meanRuntimeLagFetchMillis: Double?
    let meanOperatorStateFetchMillis: Double?
    let meanWorkerTotalMillis: Double?
    let meanPublishMillis: Double?
    let meanUiRefreshMillis: Double?
    let maxUiRefreshMillis: Double
    let under50MillisCount: Int
    let from50To250MillisCount: Int
    let from250To1000MillisCount: Int
    let over1000MillisCount: Int
    let windowDurationMillis: UInt64
}

struct UiRefreshTelemetryWindow {
    // The event stream retains the refresh denominator, including quick no-change
    // cycles, so slow-event counts can be interpreted rather than guessed.
    private(set) var refreshCount = 0
    private(set) var updatedCount = 0
    private(set) var noChangeCount = 0
    private(set) var failedCount = 0
    private(set) var cancelledCount = 0
    private(set) var slowCount = 0
    private(set) var fullSnapshotFetchCount = 0
    private(set) var fullSnapshotFetchTimingSampleCount = 0
    private(set) var fullSnapshotPublishedCount = 0
    private(set) var evaluatorSnapshotFetchCount = 0
    private(set) var evaluatorSnapshotFetchTimingSampleCount = 0
    private(set) var monitorPayloadFetchCount = 0
    private(set) var monitorPayloadFetchTimingSampleCount = 0
    private(set) var runtimeLagFetchCount = 0
    private(set) var runtimeLagFetchTimingSampleCount = 0
    private(set) var operatorStateFetchCount = 0
    private(set) var operatorStateFetchTimingSampleCount = 0
    private(set) var workerTotalCount = 0
    private(set) var workerTotalTimingSampleCount = 0
    private(set) var monitorDeltaCount = 0
    private(set) var monitorSnapshotCount = 0
    private var fullSnapshotFetchTotalMillis = 0.0
    private var evaluatorSnapshotFetchTotalMillis = 0.0
    private var monitorPayloadFetchTotalMillis = 0.0
    private var runtimeLagFetchTotalMillis = 0.0
    private var operatorStateFetchTotalMillis = 0.0
    private var workerTotalMillis = 0.0
    private var publishTotalMillis = 0.0
    private var uiRefreshTotalMillis = 0.0
    private(set) var maxUiRefreshMillis = 0.0
    private(set) var under50MillisCount = 0
    private(set) var from50To250MillisCount = 0
    private(set) var from250To1000MillisCount = 0
    private(set) var over1000MillisCount = 0
    private var startedAtMillis: UInt64?
    private var endedAtMillis: UInt64?

    var snapshot: UiRefreshTelemetryWindowSnapshot {
        UiRefreshTelemetryWindowSnapshot(
            refreshCount: refreshCount,
            updatedCount: updatedCount,
            noChangeCount: noChangeCount,
            failedCount: failedCount,
            cancelledCount: cancelledCount,
            slowCount: slowCount,
            fullSnapshotFetchCount: fullSnapshotFetchCount,
            fullSnapshotFetchTimingSampleCount: fullSnapshotFetchTimingSampleCount,
            fullSnapshotPublishedCount: fullSnapshotPublishedCount,
            evaluatorSnapshotFetchCount: evaluatorSnapshotFetchCount,
            evaluatorSnapshotFetchTimingSampleCount: evaluatorSnapshotFetchTimingSampleCount,
            monitorPayloadFetchCount: monitorPayloadFetchCount,
            monitorPayloadFetchTimingSampleCount: monitorPayloadFetchTimingSampleCount,
            monitorDeltaCount: monitorDeltaCount,
            monitorSnapshotCount: monitorSnapshotCount,
            runtimeLagFetchCount: runtimeLagFetchCount,
            runtimeLagFetchTimingSampleCount: runtimeLagFetchTimingSampleCount,
            operatorStateFetchCount: operatorStateFetchCount,
            operatorStateFetchTimingSampleCount: operatorStateFetchTimingSampleCount,
            workerTotalCount: workerTotalCount,
            workerTotalTimingSampleCount: workerTotalTimingSampleCount,
            meanFullSnapshotFetchMillis: mean(fullSnapshotFetchTotalMillis, count: fullSnapshotFetchTimingSampleCount),
            meanEvaluatorSnapshotFetchMillis: mean(evaluatorSnapshotFetchTotalMillis, count: evaluatorSnapshotFetchTimingSampleCount),
            meanMonitorPayloadFetchMillis: mean(monitorPayloadFetchTotalMillis, count: monitorPayloadFetchTimingSampleCount),
            meanRuntimeLagFetchMillis: mean(runtimeLagFetchTotalMillis, count: runtimeLagFetchTimingSampleCount),
            meanOperatorStateFetchMillis: mean(operatorStateFetchTotalMillis, count: operatorStateFetchTimingSampleCount),
            meanWorkerTotalMillis: mean(workerTotalMillis, count: workerTotalTimingSampleCount),
            meanPublishMillis: mean(publishTotalMillis, count: refreshCount),
            meanUiRefreshMillis: mean(uiRefreshTotalMillis, count: refreshCount),
            maxUiRefreshMillis: maxUiRefreshMillis,
            under50MillisCount: under50MillisCount,
            from50To250MillisCount: from50To250MillisCount,
            from250To1000MillisCount: from250To1000MillisCount,
            over1000MillisCount: over1000MillisCount,
            windowDurationMillis: windowDurationMillis
        )
    }

    private var windowDurationMillis: UInt64 {
        guard let startedAtMillis, let endedAtMillis, endedAtMillis >= startedAtMillis else { return 0 }
        return endedAtMillis - startedAtMillis
    }

    mutating func record(
        outcome: SnapshotRefreshOutcome,
        timings: SnapshotRefreshPhaseTimings,
        fullSnapshotPublished: Bool,
        monitorPayloadKind: String,
        publishMillis: Double,
        uiRefreshMillis: Double,
        atMillis: UInt64
    ) {
        if startedAtMillis == nil { startedAtMillis = atMillis }
        endedAtMillis = atMillis
        refreshCount += 1
        switch outcome {
        case .updated: updatedCount += 1
        case .noChange: noChangeCount += 1
        case .failed: failedCount += 1
        case .cancelled: cancelledCount += 1
        }
        if uiRefreshMillis >= 250 || publishMillis >= 120 { slowCount += 1 }
        if fullSnapshotPublished { fullSnapshotPublishedCount += 1 }
        if timings.fullSnapshotFetchAttempted {
            fullSnapshotFetchCount += 1
            if let duration = timings.fullSnapshotFetchMillis {
                fullSnapshotFetchTimingSampleCount += 1
                fullSnapshotFetchTotalMillis += duration
            }
        }
        if timings.evaluatorSnapshotFetchAttempted {
            evaluatorSnapshotFetchCount += 1
            if let duration = timings.evaluatorSnapshotFetchMillis {
                evaluatorSnapshotFetchTimingSampleCount += 1
                evaluatorSnapshotFetchTotalMillis += duration
            }
        }
        if timings.monitorPayloadFetchAttempted {
            monitorPayloadFetchCount += 1
            if let duration = timings.monitorPayloadFetchMillis {
                monitorPayloadFetchTimingSampleCount += 1
                monitorPayloadFetchTotalMillis += duration
            }
        }
        if timings.runtimeLagFetchAttempted {
            runtimeLagFetchCount += 1
            if let duration = timings.runtimeLagFetchMillis {
                runtimeLagFetchTimingSampleCount += 1
                runtimeLagFetchTotalMillis += duration
            }
        }
        if timings.operatorStateFetchAttempted {
            operatorStateFetchCount += 1
            if let duration = timings.operatorStateFetchMillis {
                operatorStateFetchTimingSampleCount += 1
                operatorStateFetchTotalMillis += duration
            }
        }
        if timings.workerTotalAttempted {
            workerTotalCount += 1
            if let duration = timings.workerTotalMillis {
                workerTotalTimingSampleCount += 1
                workerTotalMillis += duration
            }
        }
        if monitorPayloadKind.hasPrefix("delta") { monitorDeltaCount += 1 }
        if monitorPayloadKind.hasPrefix("snapshot") { monitorSnapshotCount += 1 }
        publishTotalMillis += publishMillis
        uiRefreshTotalMillis += uiRefreshMillis
        maxUiRefreshMillis = max(maxUiRefreshMillis, uiRefreshMillis)
        if uiRefreshMillis < 50 {
            under50MillisCount += 1
        } else if uiRefreshMillis < 250 {
            from50To250MillisCount += 1
        } else if uiRefreshMillis < 1_000 {
            from250To1000MillisCount += 1
        } else {
            over1000MillisCount += 1
        }
    }

    mutating func reset() {
        self = UiRefreshTelemetryWindow()
    }

    private func mean(_ total: Double, count: Int) -> Double? {
        guard count > 0 else { return nil }
        return total / Double(count)
    }
}
