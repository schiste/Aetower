import Foundation

public struct SelfRuntimeMemoryAttributionReportModel: Codable {
    let currentResidentBytes: UInt64
    let currentPhysicalFootprintBytes: UInt64
    let peakResidentBytes: UInt64
    let peakPhysicalFootprintBytes: UInt64
    let peakAtMillis: UInt64?
    let peakDeltaFromCurrentBytes: Int64
    let selfProcessId: UInt32
    let selfEntityId: String?
    let selfDisplayName: String?
    let processIds: [UInt32]
    let regions: [MemoryRegionBreakdownReportModel]
    let note: String
    let caveats: [String]
}

public struct HistoryUiDiagnosticsSummary: Sendable {
    let updatedAt: Date?
    let pageDecodeDurationMillis: Double
    let snapshotCount: Int
    let entityCount: Int
    let derivedSummaryBuildDurationMillis: Double
    let recurringEntityCount: Int
    let changeSummaryCount: Int

    static let empty = HistoryUiDiagnosticsSummary(
        updatedAt: nil,
        pageDecodeDurationMillis: 0,
        snapshotCount: 0,
        entityCount: 0,
        derivedSummaryBuildDurationMillis: 0,
        recurringEntityCount: 0,
        changeSummaryCount: 0
    )
}

public struct TimelinePayloadDiagnosticsSummary: Sendable {
    let updatedAt: Date?
    let totalEventCount: Int
    let filteredEventCount: Int
    let visibleEventCount: Int
    let filterDurationMillis: Double
    let safeModeEnabled: Bool

    static let empty = TimelinePayloadDiagnosticsSummary(
        updatedAt: nil,
        totalEventCount: 0,
        filteredEventCount: 0,
        visibleEventCount: 0,
        filterDurationMillis: 0,
        safeModeEnabled: false
    )
}
