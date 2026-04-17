import Foundation

enum AnalysisConfidenceLabel: String {
    case measured = "Measured"
    case persisted = "Persisted"
    case inferred = "Inferred"
    case sampled = "Sampled"
    case heuristic = "Heuristic"
}

enum AnalysisOverheadLabel: String {
    case low = "Low overhead"
    case medium = "Medium overhead"
    case high = "High overhead"
}

struct AnalysisDescriptor {
    let label: String
    let confidence: AnalysisConfidenceLabel
    let overhead: AnalysisOverheadLabel
    let detail: String
}

enum EntityAnalysisKind: String, CaseIterable, Hashable {
    case anomalyExplanation
    case processTree
    case memoryBreakdown
    case profile
    case wakeupAttribution

    var descriptor: AnalysisDescriptor {
        switch self {
        case .anomalyExplanation:
            return AnalysisDescriptor(
                label: "Anomaly explanation",
                confidence: .inferred,
                overhead: .low,
                detail: "Built from the current snapshot plus recent timeline changes."
            )
        case .processTree:
            return AnalysisDescriptor(
                label: "Process-tree analysis",
                confidence: .measured,
                overhead: .low,
                detail: "Built from the current process tree and grouping logic for this snapshot."
            )
        case .memoryBreakdown:
            return AnalysisDescriptor(
                label: "Memory breakdown",
                confidence: .sampled,
                overhead: .medium,
                detail: "Uses a live vmmap-style region breakdown from the running app."
            )
        case .profile:
            return AnalysisDescriptor(
                label: "Sampled profile",
                confidence: .sampled,
                overhead: .high,
                detail: "Uses a short live sample and should be run only when you need deeper attribution."
            )
        case .wakeupAttribution:
            return AnalysisDescriptor(
                label: "Wakeup attribution",
                confidence: .heuristic,
                overhead: .high,
                detail: "Heuristic sampled attribution. Useful for direction, not exact kernel wakeup accounting."
            )
        }
    }
}

struct SnapshotMetricDeltaReport: Codable {
    let before: Double
    let after: Double
    let delta: Double
    let percentChange: Double?
}

struct SnapshotEntityDeltaReport: Codable, Identifiable {
    let entityId: String
    let displayName: String
    let beforePresent: Bool
    let afterPresent: Bool
    let friction: SnapshotMetricDeltaReport
    let cpuPercent: SnapshotMetricDeltaReport
    let memoryBytes: SnapshotMetricDeltaReport
    let memoryPhysicalFootprintBytes: SnapshotMetricDeltaReport
    let wakeupsPerSecond: SnapshotMetricDeltaReport
    let processCount: SnapshotMetricDeltaReport
    let recentChangeSummary: String?

    var id: String { entityId }
}

struct SnapshotDiffSummaryReport: Codable {
    let hostSummary: String
    let entitySummary: String
}

struct SnapshotDiffReportModel: Codable {
    let beforeSnapshotMillis: UInt64
    let afterSnapshotMillis: UInt64
    let summary: SnapshotDiffSummaryReport
    let host: [String: SnapshotMetricDeltaReport]
    let entities: [SnapshotEntityDeltaReport]
}

struct AnalysisRecentChangeReport: Codable, Identifiable {
    let timestampMillis: UInt64
    let severity: String
    let source: String
    let entityId: String?
    let title: String
    let detail: String

    var id: String {
        "\(timestampMillis)-\(source)-\(title)"
    }
}

struct AnomalyDriverReport: Codable, Identifiable {
    let metric: String
    let before: Double
    let after: Double
    let delta: Double
    let summary: String

    var id: String { metric }
}

struct AnomalyExplanationReport: Codable, Identifiable {
    let entityId: String
    let displayName: String
    let severity: String
    let summary: String
    let recentChangeSummary: String?
    let drivers: [AnomalyDriverReport]
    let supportingEvents: [AnalysisRecentChangeReport]

    var id: String { entityId }
}

struct ProcessTreeNodeReportModel: Codable, Identifiable {
    let title: String
    let pid: UInt32?
    let relation: String
    let ownerEntityId: String
    let ownerDisplayName: String
    let selfCpuPercent: Float
    let subtreeCpuPercent: Float
    let selfMemoryBytes: UInt64
    let subtreeMemoryBytes: UInt64
    let subtreeProcessCount: UInt32
    let badges: [String]
    let user: String?
    let cwd: String?
    let provenance: String?
    let launchedBy: String?
    let adapterLabel: String?
    let statusLabel: String?
    let children: [ProcessTreeNodeReportModel]

    var id: String {
        "\(ownerEntityId)-\(pid.map(String.init) ?? title)-\(relation)"
    }
}

struct EntityProcessTreeReportModel: Codable {
    let capturedAtMillis: UInt64
    let rootEntityId: String
    let rootDisplayName: String
    let seedEntityIds: [String]
    let expandedEntityIds: [String]
    let groupedProcessCount: UInt32
    let expandedProcessCount: UInt32
    let groupingReasons: [String]
    let roots: [ProcessTreeNodeReportModel]
}

struct MemoryRegionBreakdownReportModel: Codable, Identifiable {
    let regionType: String
    let virtualBytes: UInt64
    let residentBytes: UInt64
    let dirtyBytes: UInt64
    let swapBytes: UInt64

    var id: String { regionType }
}

struct EntityMemoryBreakdownReportModel: Codable {
    let capturedAtMillis: UInt64
    let entityId: String
    let displayName: String
    let processIds: [UInt32]
    let residentBytes: UInt64
    let physicalFootprintBytes: UInt64
    let memoryMetricNote: String
    let regions: [MemoryRegionBreakdownReportModel]
}

struct SampledStackReportModel: Codable, Identifiable {
    let threadLabel: String
    let queueLabel: String?
    let sampleCount: UInt32
    let topFrames: [String]
    let classification: String

    var id: String {
        "\(queueLabel ?? threadLabel)-\(classification)"
    }
}

struct EntityProfileReportModel: Codable {
    let capturedAtMillis: UInt64
    let entityId: String
    let displayName: String
    let durationSeconds: UInt64
    let sampledProcessIds: [UInt32]
    let threadCount: Int
    let topStacks: [SampledStackReportModel]
    let summary: String
}

struct WakeupAttributionReportModel: Codable {
    let capturedAtMillis: UInt64
    let entityId: String
    let displayName: String
    let durationSeconds: UInt64
    let sampledProcessIds: [UInt32]
    let queueBreakdown: [SampledStackReportModel]
    let dominantCause: String?
    let attributionMode: String
    let caveats: [String]
}
