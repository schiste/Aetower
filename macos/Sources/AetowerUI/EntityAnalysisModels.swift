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
    case processInspect
    case processResources
    case processSample
    case processAction
    case processActionHistory

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
        case .processInspect:
            return AnalysisDescriptor(
                label: "Process inspection",
                confidence: .measured,
                overhead: .low,
                detail: "Combines current Aetower attribution with live ps state for one PID."
            )
        case .processResources:
            return AnalysisDescriptor(
                label: "Open files & sockets",
                confidence: .sampled,
                overhead: .medium,
                detail: "Uses lsof for one process with a bounded result limit."
            )
        case .processSample:
            return AnalysisDescriptor(
                label: "Process sample",
                confidence: .sampled,
                overhead: .high,
                detail: "Runs a short live sample for one PID and summarizes hot stacks."
            )
        case .processAction:
            return AnalysisDescriptor(
                label: "Process action",
                confidence: .measured,
                overhead: .low,
                detail: "Sends an explicit operator-approved signal to one process."
            )
        case .processActionHistory:
            return AnalysisDescriptor(
                label: "Action history",
                confidence: .persisted,
                overhead: .low,
                detail: "Reads recent process actions from diagnostics history."
            )
        }
    }
}

public enum ProcessActionKind: String, CaseIterable, Identifiable {
    case terminate
    case suspend
    case resume
    case forceKill = "force-kill"
    case terminateTree = "terminate-tree"
    case forceKillTree = "force-kill-tree"
    case lowerPriority = "lower-priority"
    case normalPriority = "normal-priority"

    public var id: String { rawValue }

    var label: String {
        switch self {
        case .terminate: return "Terminate"
        case .suspend: return "Suspend"
        case .resume: return "Resume"
        case .forceKill: return "Force kill"
        case .terminateTree: return "Terminate tree"
        case .forceKillTree: return "Force kill tree"
        case .lowerPriority: return "Lower priority"
        case .normalPriority: return "Normal priority"
        }
    }

    var systemImage: String {
        switch self {
        case .terminate: return "xmark.circle"
        case .suspend: return "pause.circle"
        case .resume: return "play.circle"
        case .forceKill: return "exclamationmark.octagon"
        case .terminateTree: return "xmark.circle.fill"
        case .forceKillTree: return "exclamationmark.octagon.fill"
        case .lowerPriority: return "speedometer"
        case .normalPriority: return "arrow.counterclockwise.circle"
        }
    }

    var isDestructive: Bool {
        self == .terminate || self == .forceKill || self == .terminateTree || self == .forceKillTree
    }

    var confirmationDetail: String {
        switch self {
        case .terminate:
            return "Send SIGTERM to the selected process."
        case .forceKill:
            return "Send SIGKILL to the selected process. Unsaved work can be lost."
        case .suspend:
            return "Pause the selected process with SIGSTOP."
        case .resume:
            return "Resume the selected process with SIGCONT."
        case .terminateTree:
            return "Send SIGTERM to the selected process and known child processes."
        case .forceKillTree:
            return "Send SIGKILL to the selected process and known child processes. Unsaved work can be lost."
        case .lowerPriority:
            return "Renice the selected process to a lower scheduling priority."
        case .normalPriority:
            return "Request normal scheduling priority for the selected process. macOS may require privileges."
        }
    }
}

public enum ProcessOperatorQuickOperation: Equatable {
    case inspect
    case resources
    case sample
    case previewAction(ProcessActionKind)
}

public struct ProcessOperatorRequest: Identifiable, Equatable {
    public let id = UUID()
    let createdAt = Date()
    let pid: UInt32
    let operation: ProcessOperatorQuickOperation
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

struct WakeupDataSourceStatusModel: Codable, Identifiable {
    let key: String
    let title: String
    let status: String
    let detail: String
    let nextAction: String?

    var id: String { key }
}

struct TimerInventoryReportModel: Codable {
    let status: String
    let detail: String
    let inferredTimerThreads: [SampledStackReportModel]
    let recommendedIntegrationFields: [String]
}

struct Chau7RenderPipelineSampleModel: Codable, Identifiable {
    let timestamp: String
    let liveViews: UInt32
    let polls: UInt32
    let changed: UInt32
    let draws: UInt32
    let syncCalls: UInt32
    let syncMib: Float
    let fullRefresh: UInt32
    let physicalMb: UInt32
    let peakMb: UInt32

    var id: String { timestamp }
}

struct Chau7RenderViewSampleModel: Codable, Identifiable {
    let timestamp: String
    let viewId: String
    let state: String
    let tab: String
    let session: String
    let mode: String
    let reasons: [String]
    let polls: UInt32
    let changed: UInt32
    let draws: UInt32
    let syncCalls: UInt32
    let syncMib: Float

    var id: String {
        "\(timestamp)-\(viewId)"
    }
}

struct DisplayLinkStateReportModel: Codable {
    let status: String
    let detail: String
    let source: String?
    let latestPipeline: Chau7RenderPipelineSampleModel?
    let recentPipeline: [Chau7RenderPipelineSampleModel]
    let views: [Chau7RenderViewSampleModel]
    let recommendedIntegrationFields: [String]
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
    let processWakeupsPerSecond: Float?
    let sampledThreadBreakdown: [SampledStackReportModel]?
    let exactThreadWakeupCounters: WakeupDataSourceStatusModel?
    let timerInventory: TimerInventoryReportModel?
    let displayLinkState: DisplayLinkStateReportModel?
    let dataSources: [WakeupDataSourceStatusModel]?
    let caveats: [String]
}

struct ProcessPsSummaryModel: Codable {
    let parentPid: UInt32?
    let user: String?
    let status: String?
    let cpuPercent: Float?
    let residentBytes: UInt64?
    let command: String?
}

struct ProcessInspectionReportModel: Codable {
    let capturedAtMillis: UInt64
    let pid: UInt32
    let alive: Bool
    let entityId: String?
    let displayName: String?
    let componentTitle: String?
    let componentKind: String?
    let executablePath: String?
    let commandLine: String?
    let cwd: String?
    let user: String?
    let parentPid: UInt32?
    let parentSummary: String?
    let cpuPercent: Float?
    let memoryBytes: UInt64?
    let memoryPhysicalFootprintBytes: UInt64?
    let startTimeMillis: UInt64?
    let childPids: [UInt32]
    let siblingProcessCount: UInt32
    let ps: ProcessPsSummaryModel?
    let bundle: ProcessBundleInfoModel?
    let signature: ProcessSignatureInfoModel?
    let entitlements: [String]?
    let environment: [ProcessEnvVarModel]?
    let environmentNote: String?
    let startupEntry: ProcessStartupEntryModel?
    let loadedDylibs: [ProcessDylibModel]?
    let dylibSummary: DylibSummaryModel?
    let safetyNotes: [String]
}

struct ProcessDylibModel: Codable, Identifiable {
    let path: String
    let name: String
    /// system | third_party
    let category: String
    let injected: Bool

    var id: String { path }
    var isSystem: Bool { category == "system" }
}

struct DylibSummaryModel: Codable {
    let total: UInt32
    let thirdParty: UInt32
    let injected: UInt32
}

/// Result of a sandboxed Rhai advanced filter evaluated by the engine.
struct EntityFilterReportModel: Codable {
    let expression: String
    let matchedEntityIds: [String]
    let matchedPids: [UInt32]
    let matchedCount: UInt32
    let evaluatedEntities: UInt32
    let error: String?
}

/// A process observed in recent history that is no longer alive — reconstructed
/// by diffing the live snapshot against `loadHistoryRange`. Carries the last
/// metrics seen before the process exited.
struct FinishedProcessModel: Identifiable {
    let pid: UInt32
    let name: String
    let entityName: String
    let lastCpuPercent: Float
    let lastMemoryBytes: UInt64
    let user: String?
    let startTimeMillis: UInt64
    let lastSeenMillis: UInt64

    var id: UInt32 { pid }
}

struct ProcessBundleInfoModel: Codable {
    let bundleId: String?
    let bundlePath: String?
    let name: String?
    let shortVersion: String?
    let version: String?

    /// Human-friendly "1.2.3 (456)" version label when either component exists.
    var versionLabel: String? {
        switch (shortVersion, version) {
        case let (short?, build?) where short != build: return "\(short) (\(build))"
        case let (short?, _): return short
        case let (_, build?): return build
        default: return nil
        }
    }
}

struct ProcessSignatureInfoModel: Codable {
    let signed: Bool
    let signingId: String?
    let teamId: String?
    let authority: [String]
    let notarized: Bool?
    let classification: String?
    let isAdhoc: Bool?
    let note: String?

    /// Human label for the classification string.
    var classificationLabel: String {
        switch classification {
        case "apple": return "Apple"
        case "mac_app_store": return "Mac App Store"
        case "developer_id": return "Developer ID"
        case "adhoc": return "Ad-hoc"
        case "unsigned": return "Unsigned"
        case "other": return "Other"
        default: return "Unknown"
        }
    }
}

// MARK: - Persistence scanner

struct PersistenceScanReportModel: Codable {
    let capturedAtMillis: UInt64
    let scanMode: String
    let scanDurationMillis: UInt64
    let cacheKey: String
    let deepScanAvailable: Bool
    let truncated: Bool
    let summary: PersistenceScanSummaryModel
    let items: [PersistenceItemModel]
    let scannedLocations: [String]
    let degraded: [DegradedSourceModel]
}

struct PersistenceScanSummaryModel: Codable {
    let totalItems: Int
    let expectedCount: Int
    let reviewCount: Int
    let warningCount: Int
    let dangerCount: Int
    let recentlyModifiedCount: Int
    let userScopeCount: Int
    let systemScopeCount: Int
    let missingTargetCount: Int
    let unknownSignatureCount: Int
    let unsignedOrAdhocCount: Int
}

struct PersistenceItemModel: Codable, Identifiable {
    let kind: String
    let mechanism: String
    let owner: String
    let label: String?
    let path: String
    let program: String?
    let programName: String?
    let runAtLoad: Bool?
    let disabled: Bool?
    let sourceModifiedAtMillis: UInt64?
    let sourceSizeBytes: UInt64?
    let programExists: Bool?
    let programModifiedAtMillis: UInt64?
    let programSizeBytes: UInt64?
    let signature: ProcessSignatureInfoModel?
    let isApple: Bool
    let riskLevel: String
    let riskReasons: [String]
    let actionHints: [String]
    let hiddenLocation: Bool
    let recentlyModified: Bool
    let isUserScope: Bool
    let notes: [String]

    var id: String { "\(kind):\(path):\(program ?? "")" }

    /// Human label for the persistence source kind.
    var kindLabel: String {
        switch kind {
        case "launch-agent": return "Launch Agent (/Library)"
        case "launch-daemon": return "Launch Daemon (/Library)"
        case "user-launch-agent": return "Launch Agent (user)"
        case "system-launch-agent": return "Launch Agent (/System)"
        case "system-launch-daemon": return "Launch Daemon (/System)"
        case "login-item": return "Login Item"
        case "cron": return "Cron"
        default: return kind
        }
    }

    var displayTitle: String {
        label ?? programName ?? program ?? path
    }

    var riskLabel: String {
        switch riskLevel {
        case "danger": return "Danger"
        case "warning": return "Warning"
        case "review": return "Review"
        default: return "Expected"
        }
    }
}

struct DegradedSourceModel: Codable, Identifiable {
    let source: String
    let reason: String

    var id: String { source }
}

struct ProcessEnvVarModel: Codable, Identifiable {
    let key: String
    let value: String

    var id: String { key }
}

struct ProcessStartupEntryModel: Codable {
    let kind: String
    let label: String?
    let plistPath: String
}

struct ProcessOpenResourceModel: Codable, Identifiable {
    let fd: String
    let resourceType: String
    let name: String
    let detail: String?
    let isSocket: Bool

    var id: String { "\(fd)-\(resourceType)-\(name)" }
}

struct ProcessOpenResourcesReportModel: Codable {
    let capturedAtMillis: UInt64
    let pid: UInt32
    let resourceCount: Int
    let returned: Int
    let fileCount: Int
    let socketCount: Int
    let resources: [ProcessOpenResourceModel]
}

/// One process holding a queried file or port — the reverse "who holds this?"
/// pivot row.
struct ResourceHolderModel: Codable, Identifiable {
    let pid: UInt32
    let command: String
    let user: String
    let fd: String
    let resourceType: String
    let name: String

    var id: String { "\(pid)-\(fd)-\(name)" }
}

struct ResourceHoldersReportModel: Codable {
    let capturedAtMillis: UInt64
    /// The looked-up target, echoed back (":443" or a file path).
    let query: String
    /// "port" or "file".
    let kind: String
    let holderCount: Int
    let returned: Int
    let holders: [ResourceHolderModel]
}

struct ProcessSampleReportModel: Codable {
    let capturedAtMillis: UInt64
    let pid: UInt32
    let durationSeconds: UInt64
    let threadCount: Int
    let topStacks: [SampledStackReportModel]
    let summary: String
}

struct ProcessActionReportModel: Codable {
    let capturedAtMillis: UInt64
    let actionId: String?
    let pid: UInt32
    let targetPids: [UInt32]
    let targetIdentities: [ProcessActionTargetIdentityModel]?
    let blastRadius: ProcessActionBlastRadiusModel?
    let action: String
    let signal: String
    let dryRun: Bool
    let executed: Bool
    let success: Bool
    let command: String
    let verification: String?
    let targetOutcomes: [ProcessActionTargetOutcomeModel]?
    let followUpChecks: [ProcessActionFollowUpCheckModel]?
    let commandResult: ProcessActionCommandResultModel?
    let privilegedHelperStatus: String?
    let reason: String?
    let entityId: String?
    let displayName: String?
    let message: String
    let safetyNotes: [String]
}

struct ProcessActionTargetIdentityModel: Codable, Identifiable {
    let pid: UInt32
    let startTimeMillis: UInt64?
    let executablePath: String?
    let displayName: String?
    let niceValue: Int?

    var id: UInt32 { pid }
}

struct ProcessActionBlastRadiusModel: Codable {
    let targetCount: Int
    let treeAction: Bool
    let confirmationRequired: Bool
    let summary: String
}

struct ProcessActionTargetOutcomeModel: Codable, Identifiable {
    let pid: UInt32
    let visibleBefore: Bool
    let visibleAfter: Bool?
    let niceBefore: Int?
    let statusAfter: String?
    let niceAfter: Int?
    let verification: String
    let detail: String

    var id: UInt32 { pid }
}

struct ProcessActionCommandResultModel: Codable {
    let exitStatus: Int?
    let stdout: String
    let stderr: String
}

struct ProcessActionFollowUpCheckModel: Codable, Identifiable {
    let delayMillis: UInt64
    let verification: String
    let targetOutcomes: [ProcessActionTargetOutcomeModel]

    var id: UInt64 { delayMillis }
}

struct ProcessActionHistoryItemModel: Codable, Identifiable {
    let timestampMillis: UInt64
    let actionId: String?
    let pid: UInt32?
    let targetPids: [UInt32]
    let action: String?
    let signal: String?
    let success: Bool
    let verification: String?
    let exitStatus: Int?
    let stderr: String?
    let blastRadius: Int?
    let privilegedHelperStatus: String?
    let reason: String?
    let entityId: String?
    let displayName: String?
    let message: String

    var id: String {
        "\(timestampMillis)-\(pid.map(String.init) ?? "unknown")-\(action ?? "action")"
    }
}

struct ProcessActionHistoryReportModel: Codable {
    let windowMinutes: UInt64
    let returned: Int
    let actions: [ProcessActionHistoryItemModel]
}
