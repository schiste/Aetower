import AetowerBridge
import Foundation

struct MonitorViewModel: Equatable, Sendable {
    var sequence: UInt64
    var capturedAtMillis: UInt64
    var host: UiHostSummary?
    var hostTrend: UiHostTrend?
    var metricCards: [UiMetricCard]
    var processRows: [UiProcessRow]
    var selectedEntity: UiSelectedEntity?
    var totalEntityCount: UInt32
    var returnedEntityCount: UInt32
    var totalProcessCount: UInt32
    var timelineWarningCount: UInt32
    var timelineCriticalCount: UInt32
    var lastDeltaChangedMetricCount: Int
    var lastDeltaChangedRowCount: Int
    var lastDeltaRemovedRowCount: Int
    var lastDeltaBaseAvailable: Bool

    static let empty = MonitorViewModel()

    init() {
        sequence = 0
        capturedAtMillis = 0
        host = nil
        hostTrend = nil
        metricCards = []
        processRows = []
        selectedEntity = nil
        totalEntityCount = 0
        returnedEntityCount = 0
        totalProcessCount = 0
        timelineWarningCount = 0
        timelineCriticalCount = 0
        lastDeltaChangedMetricCount = 0
        lastDeltaChangedRowCount = 0
        lastDeltaRemovedRowCount = 0
        lastDeltaBaseAvailable = true
    }

    init(snapshot: UiSnapshot) {
        sequence = snapshot.sequence
        capturedAtMillis = snapshot.capturedAtMillis
        host = snapshot.host
        hostTrend = snapshot.hostTrend
        metricCards = snapshot.metricCards
        processRows = snapshot.processRows
        selectedEntity = snapshot.selectedEntity
        totalEntityCount = snapshot.totalEntityCount
        returnedEntityCount = snapshot.returnedEntityCount
        totalProcessCount = snapshot.totalProcessCount
        timelineWarningCount = snapshot.timelineWarningCount
        timelineCriticalCount = snapshot.timelineCriticalCount
        lastDeltaChangedMetricCount = snapshot.metricCards.count
        lastDeltaChangedRowCount = snapshot.processRows.count
        lastDeltaRemovedRowCount = 0
        lastDeltaBaseAvailable = true
    }

    mutating func apply(delta: UiSnapshotDelta) {
        guard delta.updated else {
            lastDeltaChangedMetricCount = 0
            lastDeltaChangedRowCount = 0
            lastDeltaRemovedRowCount = 0
            lastDeltaBaseAvailable = delta.baseAvailable
            return
        }

        sequence = delta.sequence
        capturedAtMillis = delta.capturedAtMillis
        lastDeltaChangedMetricCount = delta.changedMetricCards.count
        lastDeltaChangedRowCount = delta.changedProcessRows.count
        lastDeltaRemovedRowCount = delta.removedEntityIds.count
        lastDeltaBaseAvailable = delta.baseAvailable

        if let updatedHost = delta.host {
            host = updatedHost
        }
        if let updatedHostTrend = delta.hostTrend {
            hostTrend = updatedHostTrend
        }
        totalEntityCount = delta.totalEntityCount
        returnedEntityCount = delta.returnedEntityCount
        totalProcessCount = delta.totalProcessCount
        timelineWarningCount = delta.timelineWarningCount
        timelineCriticalCount = delta.timelineCriticalCount

        if !delta.changedMetricCards.isEmpty {
            metricCards.mergeReplacing(delta.changedMetricCards, id: \.id)
        }

        if !delta.removedEntityIds.isEmpty {
            let removed = Set(delta.removedEntityIds)
            processRows.removeAll { removed.contains($0.entityId) }
        }
        if !delta.changedProcessRows.isEmpty {
            processRows.mergeReplacing(delta.changedProcessRows, id: \.entityId)
        }

        if delta.selectedEntityRemoved {
            selectedEntity = nil
        } else if delta.selectedEntityChanged {
            selectedEntity = delta.selectedEntity
        }
    }
}

private extension Array {
    mutating func mergeReplacing<ID: Hashable>(_ updates: [Element], id: (Element) -> ID) {
        guard !updates.isEmpty else { return }
        var indexesById: [ID: Int] = [:]
        indexesById.reserveCapacity(count)
        for (index, element) in enumerated() {
            indexesById[id(element)] = index
        }

        for update in updates {
            let updateId = id(update)
            if let index = indexesById[updateId] {
                self[index] = update
            } else {
                indexesById[updateId] = count
                append(update)
            }
        }
    }
}
