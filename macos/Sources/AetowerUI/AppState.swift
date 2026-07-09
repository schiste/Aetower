import AppKit
import Foundation
import Observation
import OSLog
import UniformTypeIdentifiers
import UserNotifications
import AetowerBridge

/// Notification categories driven by the unified timeline evaluator. `rawValue`
/// is used in cooldown keys; `notificationTitle` is the user-facing heading.
private enum NotificationCategory: String {
    case thermal
    case regression
    case restartLoop = "restart-loop"
    case network

    var notificationTitle: String {
        switch self {
        case .thermal: return "Thermal Alert"
        case .regression: return "Regression Detected"
        case .restartLoop: return "Restart Loop"
        case .network: return "Network Activity"
        }
    }
}

struct PreparedStorageHygieneResult: Sendable {
    let report: StorageHygieneReportModel?
    let baseline: StorageHygieneBaselineModel?
    let rawJSON: String?
    let errorMessage: String?
    let payloadBytes: UInt64
    let decodeMillis: UInt64
    let cacheSaveMillis: UInt64
}

enum StorageEstimateConfidence: String, Sendable {
    case verified
    case estimated
    case stale
    case refreshing
    case needsFullScan
}

struct StorageEstimateStatus: Sendable {
    let confidence: StorageEstimateConfidence
    let title: String
    let detail: String
    let dirtyPathCount: Int
    let lastChangeMillis: UInt64?
    let lastRefreshMillis: UInt64?

    static let verified = StorageEstimateStatus(
        confidence: .verified,
        title: "Verified",
        detail: "Latest storage totals come from a completed scan.",
        dirtyPathCount: 0,
        lastChangeMillis: nil,
        lastRefreshMillis: nil
    )
}

struct RepositoryInventoryRefreshState: Equatable, Sendable {
    enum Phase: String, Sendable {
        case checkingFingerprints
        case refreshingChangedRepositories
        case scanningForNewRepositories
    }

    let phase: Phase
    let checkedRepositoryCount: Int
    let changedRepositoryCount: Int
    let missingRepositoryCount: Int
    let sampleRoots: [String]

    var title: String {
        switch phase {
        case .checkingFingerprints:
            return "Checking repo fingerprints"
        case .refreshingChangedRepositories:
            return "Refreshing changed repos"
        case .scanningForNewRepositories:
            return "Scanning for new repos"
        }
    }

    var detail: String {
        switch phase {
        case .checkingFingerprints:
            return "Using cached inventory while comparing \(checkedRepositoryCount) saved fingerprint\(checkedRepositoryCount == 1 ? "" : "s")."
        case .refreshingChangedRepositories:
            let changed = changedRepositoryCount + missingRepositoryCount
            let sample = sampleRoots.map(Self.shortPath).joined(separator: ", ")
            return "\(changed) repository fingerprint\(changed == 1 ? "" : "s") changed\(sample.isEmpty ? "." : ": \(sample).")"
        case .scanningForNewRepositories:
            return "Cached repositories are visible; a background root walk is looking for newly added clones."
        }
    }

    private static func shortPath(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        if path == home { return "~" }
        if path.hasPrefix(home + "/") {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }
}

enum StorageHygieneDecodePipeline {
    static func prepare(
        _ result: JsonQueryResult,
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32,
        mode: String,
        saveCache: Bool,
        saveBaseline: Bool = true
    ) -> PreparedStorageHygieneResult {
        guard let rawJSON = result.json else {
            return PreparedStorageHygieneResult(
                report: nil,
                baseline: nil,
                rawJSON: nil,
                errorMessage: result.errorMessage ?? "Storage hygiene scan failed.",
                payloadBytes: 0,
                decodeMillis: 0,
                cacheSaveMillis: 0
            )
        }

        let decodeStarted = ContinuousClock.now
        let decoder = AetowerJSON.snakeCaseDecoder()
        guard var report = try? decoder.decode(StorageHygieneReportModel.self, from: Data(rawJSON.utf8)) else {
            return PreparedStorageHygieneResult(
                report: nil,
                baseline: nil,
                rawJSON: rawJSON,
                errorMessage: "Storage hygiene scan could not be decoded.",
                payloadBytes: UInt64(rawJSON.utf8.count),
                decodeMillis: 0,
                cacheSaveMillis: 0
            )
        }
        let decodeMillis = durationMillis(decodeStarted.duration(to: .now))
        report.diagnostics.decodeMillis = decodeMillis
        report.volumeStates = StorageVolumeCapacityEnricher.enrich(report.volumeStates)
        let baseline = StorageHygieneBaselineModel(report: report)

        let cacheStarted = ContinuousClock.now
        if saveCache {
            StorageHygieneReportCacheStore.save(
                report: report,
                rawJSON: rawJSON,
                roots: roots,
                maxDepth: maxDepth,
                limit: limit,
                mode: mode
            )
        }
        if saveBaseline {
            StorageHygieneBaselineStore.save(baseline)
        }
        let cacheSaveMillis = durationMillis(cacheStarted.duration(to: .now))

        return PreparedStorageHygieneResult(
            report: report,
            baseline: baseline,
            rawJSON: rawJSON,
            errorMessage: nil,
            payloadBytes: UInt64(rawJSON.utf8.count),
            decodeMillis: decodeMillis,
            cacheSaveMillis: cacheSaveMillis
        )
    }

    private static func durationMillis(_ duration: Duration) -> UInt64 {
        let components = duration.components
        let seconds = max(components.seconds, 0)
        let millisFromAttoseconds = max(components.attoseconds, 0) / 1_000_000_000_000_000
        return UInt64(seconds) * 1000 + UInt64(millisFromAttoseconds)
    }
}

private final class StorageHygieneMainActorPublisher: @unchecked Sendable {
    weak var state: AppState?

    init(_ state: AppState) {
        self.state = state
    }

    @MainActor
    func publishCacheHit(_ cache: StorageHygieneReportCacheHit) {
        state?.publishStorageHygieneCacheHit(cache)
    }

    @MainActor
    func publishCacheStale(reason: String) {
        state?.publishStorageHygieneCacheStale(reason: reason)
    }

    @MainActor
    func publishVerificationStarted() {
        state?.publishStorageHygieneVerificationStarted()
    }

    @MainActor
    func publishInventoryVerification(
        _ inventory: RepositoryInventoryReportModel,
        signalOnly: Bool = false
    ) {
        state?.publishStorageRepositoryInventoryVerification(inventory, signalOnly: signalOnly)
    }

    @MainActor
    func publishRepositoryInventoryRefreshState(_ refreshState: RepositoryInventoryRefreshState?) {
        state?.publishRepositoryInventoryRefreshState(refreshState)
    }

    @MainActor
    func publishVerificationFinished(message: String?) {
        state?.publishStorageHygieneVerificationFinished(message: message)
    }

    @MainActor
    func publishPrepared(_ prepared: PreparedStorageHygieneResult) {
        state?.publishPreparedStorageHygieneResult(prepared)
    }

    @MainActor
    func startScan(
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32,
        mode: String
    ) {
        state?.startStorageScanJob(
            roots: roots,
            maxDepth: maxDepth,
            limit: limit,
            mode: mode
        )
    }
}

private final class RepositoryDetailMainActorPublisher: @unchecked Sendable {
    weak var state: AppState?

    init(_ state: AppState) {
        self.state = state
    }

    @MainActor
    func publish(key: String, runID: String, detail: StorageRepoDetailModel?, errorMessage: String?) {
        state?.publishRepositoryStorageDetail(
            key: key,
            runID: runID,
            detail: detail,
            errorMessage: errorMessage
        )
    }
}

private final class StorageItemsPageMainActorPublisher: @unchecked Sendable {
    weak var state: AppState?

    init(_ state: AppState) {
        self.state = state
    }

    @MainActor
    func publish(runID: String, page: StorageHygieneItemsPageModel?, errorMessage: String?) {
        state?.publishStorageItemsPage(
            runID: runID,
            page: page,
            errorMessage: errorMessage
        )
    }
}

private final class RepositoryScorecardMainActorPublisher: @unchecked Sendable {
    weak var state: AppState?

    init(_ state: AppState) {
        self.state = state
    }

    @MainActor
    func publishResult(
        key: String,
        runID: String,
        requestedMode: String,
        result: JsonQueryResult
    ) {
        state?.publishRepositoryScorecardResult(
            key: key,
            runID: runID,
            requestedMode: requestedMode,
            result: result
        )
    }
}

private final class RepositoryGitHubProviderMainActorPublisher: @unchecked Sendable {
    weak var state: AppState?

    init(_ state: AppState) {
        self.state = state
    }

    @MainActor
    func publishResult(
        key: String,
        runID: String,
        status: RepositoryGitHubProviderStatusModel
    ) {
        state?.publishRepositoryGitHubProviderStatus(
            key: key,
            runID: runID,
            status: status
        )
    }

    @MainActor
    func finishWorkflowRerun(repoRoot: String, error: String?) {
        state?.applyWorkflowRerunResult(repoRoot: repoRoot, error: error)
    }
}

private final class RepositoryCloudflareProviderMainActorPublisher: @unchecked Sendable {
    weak var state: AppState?

    init(_ state: AppState) {
        self.state = state
    }

    @MainActor
    func publishResult(
        key: String,
        linkKey: String,
        runID: String,
        status: RepositoryCloudflareProviderStatusModel
    ) {
        state?.publishRepositoryCloudflareProviderStatus(
            key: key,
            linkKey: linkKey,
            runID: runID,
            status: status
        )
    }

    @MainActor
    func finishRedeploy(repoRoot: String, link: RepositoryProjectLinkModel, error: String?) {
        state?.applyCloudflareRedeployResult(repoRoot: repoRoot, link: link, error: error)
    }
}

@MainActor
@Observable
public final class AppState {
    public private(set) var snapshot: SystemSnapshot
    // MARK: Snapshot slices
    // `snapshot` is reassigned wholesale every tick, so every reader is
    // invalidated once per tick regardless of what it looks at. Views should
    // read these stored slices instead: the hot ones (host/entities) change
    // every tick anyway, while the rare-change ones are only reassigned when
    // their content actually differs, which prunes per-tick invalidation for
    // timeline-, capability-, and agent-driven views.
    public private(set) var hostState: HostSnapshot
    public private(set) var hostTrendState: HostTrend
    public private(set) var entitiesState: [EntitySnapshot] = []
    public private(set) var timelineState: [TimelineEvent] = []
    public private(set) var capabilitiesState: [CapabilitySnapshot] = []
    public private(set) var agentContextState = AgentContextSlice()
    public private(set) var thermalForecastState: ThermalForecast?
    public private(set) var snapshotSequence: UInt64 = 0
    public private(set) var snapshotCapturedAtMillis: UInt64 = 0
    private(set) var monitorViewModel = MonitorViewModel.empty
    public private(set) var historySnapshots: [SystemSnapshot] = []
    public private(set) var historyLoadError: String?
    public private(set) var historyRangeSummary: HistoryRangeSummary?
    public private(set) var historyStoreSummary: HistoryRangeSummary?
    public private(set) var historyMaintenanceReport: HistoryMaintenanceReport?
    public private(set) var historyIsLoading = false
    public private(set) var historyIsLoadingMore = false
    public private(set) var historyLoadStatus: String?
    public private(set) var historyHasMore = false
    public private(set) var historyLastLoadDurationMillis = 0.0
    public private(set) var historyUiDiagnostics = HistoryUiDiagnosticsSummary.empty
    public private(set) var historySnapshotDiffIsLoading = false
    private(set) var historySnapshotDiff: SnapshotDiffReportModel?
    private(set) var historySnapshotDiffError: String?
    public private(set) var historyCompareBeforeMillis: UInt64?
    public private(set) var historyCompareAfterMillis: UInt64?
    public private(set) var monitorFocusEntityID: String?
    public private(set) var diagnosticsEvents: [DiagnosticsEvent] = []
    public private(set) var diagnosticsRecentWarningCount = 0
    public private(set) var diagnosticsRecentErrorCount = 0
    public private(set) var sessionLogSummary: SessionLogSummary?
    public private(set) var sessionLogAnalysisError: String?
    public private(set) var lastDiagnosticsQueryDate: Date?
    public private(set) var lastSessionLogAnalysisCompletedDate: Date?
    public private(set) var runtimeLagMetrics = RuntimeLagMetrics(
        updatedAtMillis: 0,
        engineTickMillis: 0,
        collectMillis: 0,
        identityMillis: 0,
        attributionMillis: 0,
        frictionMillis: 0,
        enrichMillis: 0,
        historyMillis: 0,
        persistMillis: 0,
        gpuSampleMillis: 0,
        targetTickMillis: 0,
        historyQueueDepth: 0,
        diagnosticsQueueDepth: 0,
        mcpHelperCount: 0,
        staleMcpHelperCount: 0,
        oldestMcpHelperAgeMillis: 0,
        selfCpuPercent: 0,
        selfMemoryBytes: 0,
        selfMemoryPhysicalFootprintBytes: 0,
        selfWakeupsPerSecond: 0,
        selfEnergyNjPerS: 0,
        mcpTotalConnections: 0,
        mcpActiveClientCount: 0,
        mcpTotalRequests: 0,
        mcpRequestsPerSecond: 0,
        mcpObservedAtMillis: 0,
        bridgeFetchMillis: 0,
        uiRefreshMillis: 0,
        snapshotToUiMillis: 0,
        snapshotToRenderMillis: 0,
        renderCommitMillis: 0,
        displayFrameIntervalMillis: 0,
        displayRefreshHz: 0,
        displayDroppedFrames: 0,
        inputAvgLatencyMillis: 0,
        inputMaxLatencyMillis: 0,
        inputSampleCount: 0
    )
    public private(set) var diagnosticsOverview = DiagnosticsOverview(
        ringCapacity: 0,
        currentSize: 0,
        droppedEvents: 0,
        errorCount: 0,
        warnCount: 0,
        lastEventMillis: nil,
        lastErrorMillis: nil,
        lastErrorMessage: nil,
        persistedEvents: 0,
        persistedPath: nil,
        persistedBytes: 0,
        persistenceError: nil
    )
    public private(set) var diagnosticsLoadError: String?
    public private(set) var notificationAuthorizationStatus = "unknown"
    public private(set) var telemetryEnabled = false
    public private(set) var telemetryEndpoint = "http://localhost:4318/v1/metrics"
    public private(set) var telemetryVerificationStatus: String?
    public var localMcpClientStatuses: [LocalMcpClientRegistrationStatus] {
        localMcpController.clientStatuses
    }
    public var localMcpRegistrationStatusMessage: String? {
        localMcpController.registrationStatusMessage
    }
    public var localMcpServerHealthy: Bool {
        localMcpController.serverHealthy
    }
    public var localMcpLastHealthCheckDate: Date? {
        localMcpController.lastHealthCheckDate
    }
    public var localMcpLastStartAttemptDate: Date? {
        localMcpController.lastStartAttemptDate
    }
    public var localMcpLastStartSucceededDate: Date? {
        localMcpController.lastStartSucceededDate
    }
    public var localMcpLastStartError: String? {
        localMcpController.lastStartError
    }
    public var localMcpLastProbeDetail: String? {
        localMcpController.lastProbeDetail
    }
    public var localMcpRestartCount: Int {
        localMcpController.restartCount
    }
    public var localMcpConsecutiveProbeFailures: Int {
        localMcpController.consecutiveProbeFailures
    }
    private(set) var entityAnomalyExplanations: [String: AnomalyExplanationReport] = [:]
    private(set) var entityProcessTreeReports: [String: EntityProcessTreeReportModel] = [:]
    private(set) var entityMemoryBreakdowns: [String: EntityMemoryBreakdownReportModel] = [:]
    private(set) var entityProfiles: [String: EntityProfileReportModel] = [:]
    private(set) var entityWakeupAttributions: [String: WakeupAttributionReportModel] = [:]
    private(set) var selfMemoryAttribution: SelfRuntimeMemoryAttributionReportModel?
    private(set) var selfMemoryAttributionError: String?
    public private(set) var selfMemoryAttributionIsLoading = false
    public private(set) var selfMemoryAttributionUpdatedAt: Date?
    public private(set) var timelinePayloadDiagnostics = TimelinePayloadDiagnosticsSummary.empty
    public private(set) var uiPerformanceBudgetDiagnostics = UiPerformanceBudgetDiagnosticsSummary.empty
    private(set) var processInspections: [UInt32: ProcessInspectionReportModel] = [:]
    private(set) var persistenceScanReport: PersistenceScanReportModel?
    private(set) var persistenceScanIsLoading = false
    private(set) var persistenceScanError: String?
    private(set) var persistenceScanCompletedAt: Date?
    private(set) var persistenceChangedItemIds: Set<String> = []
    private(set) var storageHygieneReport: StorageHygieneReportModel?
    private(set) var previousStorageHygieneReport: StorageHygieneReportModel?
    private(set) var persistedStorageHygieneBaseline: StorageHygieneBaselineModel? = StorageHygieneBaselineStore.load()
    private(set) var storageCleanupMovedPaths: Set<String> = []
    private(set) var storageScanJob: StorageScanJobResponseModel?
    private(set) var storageEstimateStatus = StorageEstimateStatus.verified
    private(set) var storageHygieneIsLoading = false {
        didSet {
            guard storageHygieneIsLoading != oldValue else { return }
            restartStorageHygieneLoadWatchdog()
        }
    }
    /// Set by the load watchdog when an in-flight hygiene load has been
    /// running past its budget (~30s). Re-enables the manual Rescan button so
    /// an explicit user action can supersede the stuck load —
    /// `runStorageHygieneScan` cancels the previous task, whose results are
    /// then dropped at its next cancellation checkpoint.
    private(set) var storageHygieneLoadExceededBudget = false
    @ObservationIgnored private var storageHygieneLoadWatchdogTask: Task<Void, Never>?
    private(set) var storageHygieneIsVerifyingCache = false
    private(set) var storageHygieneError: String?
    private(set) var storageHygieneCompletedAt: Date?
    private(set) var repositoryInventoryRefreshState: RepositoryInventoryRefreshState?
    /// Server-paged Storage Explorer table state. The page is fetched on
    /// demand from `storage_hygiene_items_page_json` (index-backed, sorted
    /// server-side); the offset/sort properties record the most recent
    /// *request* so the view can render sort indicators before the reply.
    private(set) var storageItemsPage: StorageHygieneItemsPageModel?
    private(set) var storageItemsPageIsLoading = false
    private(set) var storageItemsPageError: String?
    private(set) var storageItemsPageOffset = 0
    private(set) var storageItemsPageSortKey = "size"
    private(set) var storageItemsPageSortDescending = true
    /// Bumped whenever any input of the repository summary join changes
    /// (report publish/merge, scorecard results, project links). Views key
    /// their summary caches on this instead of re-deriving per render.
    private(set) var repositorySummaryInputsGeneration: UInt64 = 0
    private(set) var repositoryScorecardReportsByRoot: [String: RepositoryScorecardReportModel] = [:]
    private(set) var repositoryScorecardLoadingRoots: Set<String> = []
    private(set) var repositoryDetailReportsByRoot: [String: StorageRepoDetailModel] = [:]
    private(set) var repositoryDetailLoadingRoots: Set<String> = []
    private(set) var repositoryDetailErrorsByRoot: [String: String] = [:]
    private(set) var repositoryScorecardErrorsByRoot: [String: String] = [:]
    private(set) var repositoryCleanupResultByRoot: [String: RepositoryArtifactCleanupResult] = [:]
    private(set) var repositoryBulkLabel: String?
    private(set) var repositoryBulkRoots: Set<String> = []
    private(set) var repositoryProjects: [RepositoryProjectModel] = RepositoryProjectStore.load()
    private(set) var repositoryProjectGitHubLoadingRoots: Set<String> = []
    private(set) var repositoryProjectGitHubErrorsByRoot: [String: String] = [:]
    private(set) var repositoryProjectCloudflareLoadingKeys: Set<String> = []
    private(set) var repositoryProjectCloudflareErrorsByKey: [String: String] = [:]
    private(set) var processOpenResources: [UInt32: ProcessOpenResourcesReportModel] = [:]
    /// Result of the most recent reverse resource-holder lookup (which process
    /// holds a given file/port). Not pid-keyed — it's a system-wide query.
    private(set) var resourceHolders: ResourceHoldersReportModel?
    private(set) var resourceHoldersIsLoading = false
    private(set) var resourceHoldersError: String?
    private(set) var processSamples: [UInt32: ProcessSampleReportModel] = [:]
    private(set) var recentlyFinished: [FinishedProcessModel] = []
    /// Entity ids matched by the active advanced (Rhai) filter; nil = no filter.
    private(set) var advancedFilterEntityIds: Set<String>?
    private(set) var advancedFilterError: String?
    private(set) var advancedFilterSummary: String?
    private(set) var automationRules: [AutomationRule] = AutomationStore.load()
    private var seenAutomationEventIds: Set<String> = []
    private var automationSeeded = false
    /// Two-pass grace bookkeeping for pruning the on-demand report caches: a
    /// key must be absent from the live snapshot on two consecutive passes
    /// (operator-state cadence, ~30s apart) before its cached report is dropped.
    private var stalePruneEntityIds: Set<String> = []
    private var stalePrunePids: Set<UInt32> = []
    /// Live tokens from views that need the full SystemSnapshot decode.
    /// While empty, the expensive full fetch drops to the evaluator floor
    /// cadence (every `fullSnapshotFloorTicks` refreshes) so automation,
    /// notification, and anomaly evaluators keep running on fresh data.
    @ObservationIgnored private var fullSnapshotDemandTokens: Set<UUID> = []
    @ObservationIgnored private var frontmostTitleProbeTask: Task<Void, Never>?
    @ObservationIgnored private var lastInventorySignalRefreshMillis: UInt64 = 0
    @ObservationIgnored private var lastRepositoryInventoryFingerprintAuditMillis: UInt64 = 0
    @ObservationIgnored private var lastStorageEstimateRefreshMillis: UInt64 = 0
    @ObservationIgnored private var lastStorageEstimateDecisionMillis: UInt64 = 0
    @ObservationIgnored private var ticksSinceFullSnapshot = 0
    @ObservationIgnored private let fullSnapshotFloorTicks = 5
    var processActionPreviewReports: [String: ProcessActionReportModel] {
        processActionController.processActionPreviewReports
    }
    var processActionReports: [UInt32: ProcessActionReportModel] {
        processActionController.processActionReports
    }
    var processActionHistory: ProcessActionHistoryReportModel? {
        processActionController.processActionHistory
    }
    public var lastError: String?

    @ObservationIgnored
    private let bridge: EngineBridge
    @ObservationIgnored
    private let permissionCoordinator: PermissionCoordinator
    private let processActionController: ProcessActionController
    private let localMcpController: LocalMcpController
    private let exportController: ExportController
    private let storageScanController: StorageScanController
    @ObservationIgnored
    private let snapshotRefreshWorker: SnapshotRefreshWorker
    @ObservationIgnored
    private let lagMonitor = LagMonitor()
    @ObservationIgnored
    private var refreshTask: Task<Void, Never>?
    @ObservationIgnored
    private var refreshFetchTask: Task<Void, Never>?
    @ObservationIgnored
    private var refreshInFlight = false
    @ObservationIgnored
    private var pendingForcedRefresh = false
    @ObservationIgnored
    private var workspaceActivationTask: Task<Void, Never>?
    @ObservationIgnored
    private var appBecameActiveTask: Task<Void, Never>?
    @ObservationIgnored
    private var appResignedActiveTask: Task<Void, Never>?
    /// Whether any Aetower window is actually on screen (occlusion `.visible`).
    /// A window that is merely unfocused is still visible and must keep its
    /// rings updating — so the refresh loop throttles on occlusion, not on
    /// app-active/frontmost state. Only when every window is minimized, fully
    /// covered, hidden, or on another Space does the cadence stretch (see
    /// `currentRefreshIntervalNanos`). Internal control state, not UI-bound.
    @ObservationIgnored
    private var windowsVisible = true
    /// Base refresh cadence in nanoseconds, captured from the configured
    /// foreground interval in `start`. The background cadence is derived from
    /// this so a single source drives both.
    @ObservationIgnored
    private var baseRefreshIntervalNanos: UInt64 = 1_000_000_000
    /// How much to stretch the refresh interval while backgrounded. At the
    /// default 1 s foreground cadence this makes the occluded app pull a
    /// snapshot every 5 s instead of every second.
    @ObservationIgnored
    private let backgroundRefreshMultiplier: UInt64 = 5
    @ObservationIgnored
    private var historyLoadTask: Task<Void, Never>?
    @ObservationIgnored
    private var historyDiffTask: Task<Void, Never>?
    @ObservationIgnored
    private var selfMemoryAttributionTask: Task<Void, Never>?
    @ObservationIgnored
    private var diagnosticsLoadTask: Task<Void, Never>?
    @ObservationIgnored
    private var persistenceScanTask: Task<Void, Never>?
    @ObservationIgnored
    private var storageHygieneTask: Task<Void, Never>?
    @ObservationIgnored
    private var storageScheduledScanTask: Task<Void, Never>?
    @ObservationIgnored
    private var storageScheduledScansEnabled = false
    @ObservationIgnored
    private var storageScheduledScanIntervalSeconds: TimeInterval = 86_400
    @ObservationIgnored
    private var repositoryScorecardTasks: [String: Task<Void, Never>] = [:]
    @ObservationIgnored private var repositoryDetailTasks: [String: Task<Void, Never>] = [:]
    @ObservationIgnored private var repositoryInventorySignalTask: Task<Void, Never>?
    @ObservationIgnored private var repositoryDetailRunIDs: [String: String] = [:]
    @ObservationIgnored private var storageItemsPageTask: Task<Void, Never>?
    @ObservationIgnored private var storageItemsPageRunID: String?
    /// Capture stamp of the hygiene report the current page request was made
    /// against; a newer report invalidates the dedupe so the page refreshes.
    @ObservationIgnored private var storageItemsPageReportCaptureMillis: UInt64?
    @ObservationIgnored
    private var repositoryScorecardRunIDs: [String: String] = [:]
    @ObservationIgnored
    var repositoryGitHubProviderClient = RepositoryGitHubProviderClient.live
    @ObservationIgnored
    private var repositoryGitHubProviderTasks: [String: Task<Void, Never>] = [:]
    @ObservationIgnored
    private var repositoryGitHubProviderRunIDs: [String: String] = [:]
    @ObservationIgnored
    var repositoryCloudflareProviderClient = RepositoryCloudflareProviderClient.live
    @ObservationIgnored
    private var repositoryCloudflareProviderTasks: [String: Task<Void, Never>] = [:]
    @ObservationIgnored
    private var repositoryCloudflareProviderRunIDs: [String: String] = [:]
    @ObservationIgnored
    private let storageRootChangeMonitor = StorageRootChangeMonitor()
    @ObservationIgnored
    private var lastObservedSequence: UInt64
    @ObservationIgnored
    private var lastPublishedFrontmostBaseSignature: String?
    @ObservationIgnored
    private var lastPublishedFrontmostSignature: String?
    @ObservationIgnored
    private var lastPublishedFrontmostAppName: String?
    @ObservationIgnored
    private var lastPublishedWindowTitle: String?
    @ObservationIgnored
    private var lastFrontmostProbeDate = Date.distantPast
    @ObservationIgnored
    private var lastWindowTitleProbeDate = Date.distantPast
    @ObservationIgnored
    private var previousAnomalyStates: [String: Bool] = [:]
    @ObservationIgnored
    private var lastAnomalyNotificationDates: [String: Date] = [:]
    @ObservationIgnored
    private var lastBudgetBreachDates: [String: Date] = [:]
    @ObservationIgnored
    private var lastTimelineNotificationDates: [String: Date] = [:]
    @ObservationIgnored
    private var seenNotificationEventIds: Set<String> = []
    @ObservationIgnored
    private var notificationsSeeded = false
    /// Per-category notification enable flags, mirrored from SettingsStore.
    @ObservationIgnored
    private var notifyThermal = true
    @ObservationIgnored
    private var notifyRegression = true
    @ObservationIgnored
    private var notifyRestartLoop = true
    @ObservationIgnored
    private var notifyNetwork = false
    @ObservationIgnored
    private var notifyAgentBudget = true
    /// Active per-bundle notification snoozes, loaded from NotificationSnoozeStore.
    @ObservationIgnored
    private var notificationSnoozes: [NotificationSnooze] = NotificationSnoozeStore.load()
    @ObservationIgnored
    private var suppressedAnomalyNotificationCount = 0
    @ObservationIgnored
    private var suppressedAnomalyEntityKeys = Set<String>()
    @ObservationIgnored
    private var notificationsEnabled = false
    @ObservationIgnored
    private var frictionNotificationThreshold = 60.0
    @ObservationIgnored
    private var mirroredDiagnosticsSignatures = Set<String>()
    @ObservationIgnored
    private var localMcpOperatorActionsEnabled = false
    @ObservationIgnored
    private var historyWindowSeconds: TimeInterval = 3600
    @ObservationIgnored
    private var lastHistoryLoadDate = Date.distantPast
    @ObservationIgnored
    private var lastDiagnosticsLoadDate = Date.distantPast
    @ObservationIgnored
    private var lastSessionLogAnalysisDate = Date.distantPast
    @ObservationIgnored
    private var lastSessionLogFingerprint: String?
    @ObservationIgnored
    private var lastSuppressedAnomalySummaryDate = Date.distantPast
    @ObservationIgnored
    private var historyVisible = false
    @ObservationIgnored
    private var diagnosticsVisible = false
    @ObservationIgnored
    private var lagMonitoringActive = false
    @ObservationIgnored
    private var historyRangeStartMillis: UInt64 = 0
    @ObservationIgnored
    private var historyRangeEndMillis: UInt64 = 0
    @ObservationIgnored
    private var historyRangeEndOverrideMillis: UInt64?
    private var lastOperatorStateRefreshDate = Date.distantPast
    @ObservationIgnored
    private var entityAnalysisLoadingKeys = Set<String>()
    @ObservationIgnored
    private var entityAnalysisErrorMessages: [String: String] = [:]
    @ObservationIgnored
    private var entityAnalysisUpdatedAtByKey: [String: Date] = [:]

    @ObservationIgnored
    private let frontmostProbeInterval: TimeInterval = 1.0
    @ObservationIgnored
    private let windowTitleProbeInterval: TimeInterval = 5.0
    @ObservationIgnored
    private let historyReloadInterval: TimeInterval = 20.0
    @ObservationIgnored
    private let diagnosticsReloadInterval: TimeInterval = 2.0
    @ObservationIgnored
    private let sessionLogAnalysisInterval: TimeInterval = 45.0
    @ObservationIgnored
    private let diagnosticsHealthWindowSeconds: TimeInterval = 600.0
    @ObservationIgnored
    private let anomalyNotificationCooldown: TimeInterval = 300.0
    @ObservationIgnored
    private let suppressedAnomalySummaryInterval: TimeInterval = 300.0
    @ObservationIgnored
    private let suppressedAnomalySummaryMinimumCount = 10
    @ObservationIgnored
    private let historyInitialPageSize: UInt32 = 48
    @ObservationIgnored
    private let historyLoadMorePageSize: UInt32 = 96
    @ObservationIgnored
    private let diagnosticsMaxRetainedEvents: UInt32 = 500
    @ObservationIgnored
    private let operatorStateRefreshInterval: TimeInterval = 30.0
    @ObservationIgnored
    private let entityStaticAnalysisReloadInterval: TimeInterval = 15.0

    public init(
        bridge: EngineBridge = EngineBridge(),
        permissionCoordinator: PermissionCoordinator = PermissionCoordinator()
    ) {
        let initialSnapshot = Self.emptySnapshot()
        self.bridge = bridge
        self.permissionCoordinator = permissionCoordinator
        self.processActionController = ProcessActionController(bridge: bridge)
        self.localMcpController = LocalMcpController(bridge: bridge)
        self.exportController = ExportController(bridge: bridge)
        self.storageScanController = StorageScanController(bridge: bridge)
        self.snapshotRefreshWorker = SnapshotRefreshWorker(
            bridge: bridge,
            initialSequence: initialSnapshot.sequence
        )
        self.snapshot = initialSnapshot
        self.hostState = initialSnapshot.host
        self.hostTrendState = initialSnapshot.hostTrend
        self.lastObservedSequence = initialSnapshot.sequence
        self.storageScanController.attach(self)
    }

    static func emptySnapshot() -> SystemSnapshot {
        SystemSnapshot(
            sequence: 0,
            capturedAtMillis: 0,
            host: HostSnapshot(
                cpuPercent: 0,
                memoryUsedBytes: 0,
                memoryTotalBytes: 0,
                swapUsedBytes: 0,
                compressedMemoryBytes: 0,
                diskReadBps: 0,
                diskWriteBps: 0,
                networkReceiveBps: 0,
                networkSendBps: 0,
                wakeupsPerSecond: 0,
                thermalState: .nominal,
                onBattery: false,
                batteryChargePercent: nil,
                lowPowerMode: false,
                frontmostAppName: nil,
                frontmostWindowTitle: nil,
                aiAgentFriction: 0,
                aiAgentCount: 0,
                gpuPercent: 0,
                anePercent: 0,
                gpuMemoryBytes: 0,
                gpuTemperatureCelsius: nil,
                fans: [],
                cpuTemperatures: [],
                powerReadings: [],
                batteryHealth: nil,
                bootSession: nil,
                networkInterfaces: [],
                disks: [],
                bluetoothDevices: [],
                perCoreCpu: []
            ),
            hostTrend: HostTrend(
                machineFriction: [],
                cpuPercent: [],
                memoryUsedBytes: [],
                memoryPressureScore: [],
                diskActivityBps: [],
                networkActivityBps: [],
                wakeupsPerSecond: [],
                compressedMemoryBytes: [],
                aiAgentFriction: [],
                gpuPercent: [],
                gpuMemoryBytes: [],
                maxCpuTemperature: []
            ),
            capabilities: [],
            entities: [],
            timeline: [],
            aiRepoSummaries: [],
            chau7Sessions: [],
            resourceCostRollups: [],
            thermalForecast: nil
        )
    }

    public func start() {
        start(refreshInterval: 1.0)
    }

    public var localMcpSocketPathDisplay: String {
        localMcpController.socketPathDisplay
    }

    public func startLocalMcpServer(
        autoRegisterClients: Bool = false,
        operatorActionsEnabled: Bool = false
    ) {
        localMcpOperatorActionsEnabled = operatorActionsEnabled
        localMcpController.start(
            autoRegisterClients: autoRegisterClients,
            operatorActionsEnabled: operatorActionsEnabled
        )
        consumeLocalMcpError()
    }

    /// Tear down the in-process MCP server explicitly so the Unix socket
    /// file at `localMcpSocketPath` is unlinked before the process exits.
    /// Safe to call multiple times; subsequent calls are no-ops.
    public func stopLocalMcpServer() {
        localMcpController.stop()
    }

    public func start(refreshInterval: Double) {
        stop()
        ensureLocalMcpServer()
        refreshLocalPermissionCapabilities()
        observeWorkspaceActivation()
        observeAppActivation()
        publishFrontmostState(force: true)
        refresh(force: true)
        updateLagMonitoringState()
        baseRefreshIntervalNanos = UInt64(max(1.0, refreshInterval) * 1_000_000_000)
        refreshTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                // Read the cadence each iteration so it tracks foreground /
                // background transitions without restarting the loop.
                let sleepNanos = await MainActor.run { self.currentRefreshIntervalNanos() }
                try? await Task.sleep(nanoseconds: sleepNanos)
                guard !Task.isCancelled else { break }
                await MainActor.run {
                    self.publishFrontmostState()
                    self.refresh()
                }
            }
        }
    }

    /// Sleep interval for the refresh loop: the foreground cadence whenever any
    /// window is visible on screen, stretched by `backgroundRefreshMultiplier`
    /// only when every window is off screen. Becoming visible again forces an
    /// immediate refresh (see `observeAppActivation`), so the longer off-screen
    /// sleep never delays what the user sees when a window returns.
    private func currentRefreshIntervalNanos() -> UInt64 {
        windowsVisible
            ? baseRefreshIntervalNanos
            : baseRefreshIntervalNanos * backgroundRefreshMultiplier
    }

    public func stop() {
        refreshTask?.cancel()
        refreshTask = nil
        refreshFetchTask?.cancel()
        refreshFetchTask = nil
        refreshInFlight = false
        pendingForcedRefresh = false
        workspaceActivationTask?.cancel()
        workspaceActivationTask = nil
        appBecameActiveTask?.cancel()
        appBecameActiveTask = nil
        appResignedActiveTask?.cancel()
        appResignedActiveTask = nil
        historyLoadTask?.cancel()
        historyLoadTask = nil
        historyDiffTask?.cancel()
        historyDiffTask = nil
        selfMemoryAttributionTask?.cancel()
        selfMemoryAttributionTask = nil
        diagnosticsLoadTask?.cancel()
        diagnosticsLoadTask = nil
        storageHygieneTask?.cancel()
        storageHygieneTask = nil
        storageHygieneIsVerifyingCache = false
        storageScheduledScanTask?.cancel()
        storageScheduledScanTask = nil
        repositoryScorecardTasks.values.forEach { $0.cancel() }
        repositoryScorecardTasks.removeAll()
        repositoryScorecardRunIDs.removeAll()
        repositoryScorecardLoadingRoots.removeAll()
        // Provider and detail fetches outlived teardown before: cancel them all
        // so no network work leaks past stop().
        repositoryDetailTasks.values.forEach { $0.cancel() }
        repositoryDetailTasks.removeAll()
        repositoryGitHubProviderTasks.values.forEach { $0.cancel() }
        repositoryGitHubProviderTasks.removeAll()
        repositoryCloudflareProviderTasks.values.forEach { $0.cancel() }
        repositoryCloudflareProviderTasks.removeAll()
        repositoryInventorySignalTask?.cancel()
        repositoryInventorySignalTask = nil
        repositoryInventoryRefreshState = nil
        storageScanController.stop()
        storageRootChangeMonitor.stop()
        lagMonitor.stop()
        lagMonitoringActive = false
    }

    public func requestCapability(_ capability: CapabilitySnapshot) {
        let result = permissionCoordinator.request(capability.kind)
        bridge.setCapability(capability.kind, state: result.state, detail: result.detail)
        refresh(force: true)
    }

    public func performCapabilityAction(_ capability: CapabilitySnapshot, settings: SettingsStore) {
        switch capability.kind {
        case .accessibility, .fullDiskAccess, .appleAutomation:
            requestCapability(capability)
        case .chromiumDebug, .dockerSocket, .privilegedHelper, .endpointSecurity, .chau7:
            applyIntegrationSettings(settings)
        }
    }

    private func refreshLocalPermissionCapabilities() {
        for kind in [CapabilityKind.accessibility, .fullDiskAccess, .appleAutomation] {
            let result = permissionCoordinator.currentStatus(kind)
            bridge.setCapability(kind, state: result.state, detail: result.detail)
        }
    }

    public func stopAgentSession(sessionId: String, force: Bool) {
        if let error = bridge.stopAgentSession(sessionId: sessionId, force: force) {
            lastError = error
        }
        refresh(force: true)
    }

    public func exportSnapshotJSON() -> String {
        exportController.exportSnapshotJSON()
    }

    public func exportSnapshot() {
        exportController.exportSnapshot(privacyTier: exportPrivacyTier)
    }

    /// Export the current snapshot as a per-process CSV (one row per process
    /// component). Mirrors `exportSnapshot` but emits spreadsheet-friendly rows.
    public func exportSnapshotCSV() {
        exportController.exportSnapshotCSV(snapshot: snapshot)
    }

    func snapshotCSV() -> String {
        exportController.snapshotCSV(snapshot: snapshot)
    }

    /// Reconstruct recently-finished processes by diffing the live snapshot
    /// against the last `windowMinutes` of history: a PID seen in history but
    /// not alive now is "finished", carrying its most recent observed metrics.
    public func refreshRecentlyFinished(windowMinutes: UInt64 = 5) {
        let bridge = self.bridge
        let current = self.snapshot
        Task(priority: .utility) { [weak self] in
            let endMillis = current.capturedAtMillis
            let windowMillis = windowMinutes * 60_000
            let startMillis = endMillis > windowMillis ? endMillis - windowMillis : 0
            let history = bridge.loadHistoryRange(startMillis: startMillis, endMillis: endMillis)

            let alivePids = Set(
                current.entities.flatMap { entity in
                    entity.components.compactMap(\.processId)
                }
            )

            var lastSeen: [UInt32: FinishedProcessModel] = [:]
            for snap in history {
                for entity in snap.entities {
                    for component in entity.components where component.kind != .adapterContext {
                        guard let pid = component.processId else { continue }
                        if let existing = lastSeen[pid], existing.lastSeenMillis >= snap.capturedAtMillis {
                            continue
                        }
                        lastSeen[pid] = FinishedProcessModel(
                            pid: pid,
                            name: component.title,
                            entityName: entity.displayName,
                            lastCpuPercent: component.cpuPercent,
                            lastMemoryBytes: component.memoryBytes,
                            user: component.user,
                            startTimeMillis: component.startTimeMillis,
                            lastSeenMillis: snap.capturedAtMillis
                        )
                    }
                }
            }

            let finished = lastSeen.values
                .filter { !alivePids.contains($0.pid) }
                .sorted { $0.lastSeenMillis > $1.lastSeenMillis }
                .prefix(25)
            let result = Array(finished)
            await MainActor.run { self?.recentlyFinished = result }
        }
    }

    /// Evaluate a sandboxed Rhai filter expression in the engine and retain the
    /// matched entity ids so the list view can intersect against them.
    public func applyAdvancedFilter(_ expression: String) {
        let trimmed = expression.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            clearAdvancedFilter()
            return
        }
        let result = bridge.filterEntitiesJSON(expression: trimmed)
        guard let report = decodeJsonQueryResult(result, as: EntityFilterReportModel.self) else {
            advancedFilterEntityIds = []
            advancedFilterError = jsonQueryErrorMessage(result, fallback: "Filter could not be evaluated.")
            advancedFilterSummary = nil
            return
        }
        advancedFilterEntityIds = Set(report.matchedEntityIds)
        advancedFilterError = report.error
        advancedFilterSummary = "\(report.matchedCount) of \(report.evaluatedEntities) match"
    }

    public func clearAdvancedFilter() {
        advancedFilterEntityIds = nil
        advancedFilterError = nil
        advancedFilterSummary = nil
    }

    public func updateAutomationRules(_ rules: [AutomationRule]) {
        automationRules = rules
        AutomationStore.save(rules)
    }

    /// Fire automation rules for timeline events that appeared since the last
    /// snapshot. On the first snapshot we only seed the seen-set so we don't
    /// fire on the historical backlog at launch. Rules run only while the app
    /// is running (documented in the Automation settings UI).
    private func evaluateAutomationRules(snapshot: SystemSnapshot) {
        let activeRules = automationRules.filter(\.enabled)
        // With no active rules, skip the per-tick seen-set rebuild entirely and
        // drop the seed so re-enabling reseeds (never fires on the backlog).
        guard !activeRules.isEmpty else {
            automationSeeded = false
            seenAutomationEventIds.removeAll()
            return
        }
        let events = snapshot.timeline
        guard automationSeeded else {
            seenAutomationEventIds = Set(events.map(\.id))
            automationSeeded = true
            return
        }
        for event in events where !seenAutomationEventIds.contains(event.id) {
            for rule in activeRules where automationRuleMatches(rule, event, in: snapshot) {
                executeAutomationAction(rule, event: event)
            }
        }
        // Reset to the current (bounded) window so the set never grows without bound.
        seenAutomationEventIds = Set(events.map(\.id))
    }

    /// Unified timeline-driven notification evaluator: posts a user
    /// notification for each newly-observed timeline event whose category is
    /// enabled, severity is high enough, and whose app is not snoozed — gated by
    /// the master toggle and the existing per-key cooldown. Friction and
    /// agent-budget alerts are threshold/rate-driven and handled separately.
    private func evaluateTimelineNotifications(snapshot: SystemSnapshot) {
        // While notifications are off, skip the per-tick seen-set rebuild and
        // drop the seed so re-enabling reseeds (never alerts on the backlog).
        guard notificationsEnabled else {
            notificationsSeeded = false
            seenNotificationEventIds.removeAll()
            return
        }
        let events = snapshot.timeline
        // Seed-guard: ignore the backlog present on the first snapshot so we
        // don't alert for events that predate this session.
        guard notificationsSeeded else {
            seenNotificationEventIds = Set(events.map(\.id))
            notificationsSeeded = true
            return
        }
        for event in events where !seenNotificationEventIds.contains(event.id) {
            considerTimelineNotification(event, in: snapshot)
        }
        seenNotificationEventIds = Set(events.map(\.id))
    }

    private func considerTimelineNotification(_ event: TimelineEvent, in snapshot: SystemSnapshot) {
        guard let category = notificationCategory(for: event) else { return }
        // Severity floor: only Warning/Critical events notify.
        guard event.severity == .warning || event.severity == .critical else { return }

        let bundleId = event.entityId
            .flatMap { id in snapshot.entities.first { $0.entityId == id } }
            .flatMap { $0.bundleId }
        if let bundleId, isSnoozed(bundleId: bundleId) { return }

        let cooldownKey = "\(category.rawValue):\(bundleId ?? event.entityId ?? "system")"
        let now = Date()
        if let last = lastTimelineNotificationDates[cooldownKey],
            now.timeIntervalSince(last) < anomalyNotificationCooldown
        {
            return
        }
        lastTimelineNotificationDates[cooldownKey] = now
        postUserNotification(
            identifier: "timeline-\(cooldownKey)-\(event.timestampMillis)",
            title: category.notificationTitle,
            body: event.detail.isEmpty ? event.title : event.detail
        )
    }

    /// Map a timeline event to its notification category, honoring the
    /// per-category enable toggles. Returns nil when the category is off or not
    /// a notifiable kind. Restart loops are Lifecycle events distinguished by
    /// title (the engine emits "restart loop" lifecycle events).
    private func notificationCategory(for event: TimelineEvent) -> NotificationCategory? {
        switch event.category {
        case .thermal: return notifyThermal ? .thermal : nil
        case .regression: return notifyRegression ? .regression : nil
        case .network: return notifyNetwork ? .network : nil
        case .lifecycle:
            let isRestartLoop = event.title.lowercased().contains("restart")
                || event.detail.lowercased().contains("restart loop")
            return (isRestartLoop && notifyRestartLoop) ? .restartLoop : nil
        default:
            return nil
        }
    }

    private func isSnoozed(bundleId: String) -> Bool {
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        return notificationSnoozes.contains { $0.bundleId == bundleId && $0.untilMillis > nowMillis }
    }

    /// Snooze all notifications for an app (by bundle id) for `hours`. Replaces
    /// any existing entry for the same bundle and persists.
    public func snoozeNotifications(bundleId: String, displayName: String, hours: Double) {
        let untilMillis = UInt64((Date().timeIntervalSince1970 + hours * 3600) * 1000)
        notificationSnoozes.removeAll { $0.bundleId == bundleId }
        notificationSnoozes.append(
            NotificationSnooze(
                bundleId: bundleId, displayName: displayName, untilMillis: untilMillis))
        persistSnoozes()
    }

    /// Active (non-expired) snoozes, for the settings list.
    public var activeNotificationSnoozes: [NotificationSnooze] {
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        return notificationSnoozes.filter { $0.untilMillis > nowMillis }
    }

    public func clearSnooze(bundleId: String) {
        notificationSnoozes.removeAll { $0.bundleId == bundleId }
        persistSnoozes()
    }

    private func persistSnoozes() {
        // Drop expired entries opportunistically so the store stays bounded.
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        notificationSnoozes.removeAll { $0.untilMillis <= nowMillis }
        NotificationSnoozeStore.save(notificationSnoozes)
    }

    /// Post a user notification, reusing the master-toggle + bundle-id guards.
    private func postUserNotification(identifier: String, title: String, body: String) {
        guard notificationsEnabled, Bundle.main.bundleIdentifier != nil else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        let request = UNNotificationRequest(identifier: identifier, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request) { _ in }
    }

    /// Per-agent FinOps guardrail: derive each AI agent's cost/token rate from
    /// recent persisted history and fire the rule's action (+ a notification)
    /// when it exceeds the budget threshold. Rates can't be metered per call, so
    /// they're computed by diffing the cumulative agent_cost totals over time.
    private func evaluateAgentBudgetRules(snapshot: SystemSnapshot) {
        let budgetRules = automationRules.filter {
            $0.enabled && $0.event == .agentBudget && $0.budgetThreshold > 0
        }
        guard !budgetRules.isEmpty else { return }
        let nowMillis = snapshot.capturedAtMillis
        // Baseline window: the last hour of persisted samples.
        let windowStartMillis = nowMillis >= 3_600_000 ? nowMillis - 3_600_000 : 0
        let now = Date()

        for rule in budgetRules {
            let metric = AgentBudgetMetric(rawValue: rule.budgetMetric) ?? .costPerHour
            let scope = rule.titleContains
                .trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            for entity in snapshot.entities {
                guard entity.entityKind == .aiAgent, let cost = entity.agentCost else { continue }
                if !scope.isEmpty, !entity.displayName.lowercased().contains(scope) { continue }
                guard
                    let baseline = agentBudgetBaseline(
                        entityId: entity.entityId,
                        metric: metric,
                        sinceMillis: windowStartMillis
                    )
                else { continue }
                let elapsedSeconds = Double(nowMillis - baseline.millis) / 1000.0
                // Need a meaningful window before a rate is trustworthy.
                guard elapsedSeconds >= 300 else { continue }
                let delta = agentMetricValue(cost, metric) - baseline.value
                guard delta > 0 else { continue }
                let ratePerHour = delta / (elapsedSeconds / 3600.0)
                guard ratePerHour > rule.budgetThreshold else { continue }

                let cooldownKey = "\(rule.id.uuidString):\(entity.entityId)"
                if let last = lastBudgetBreachDates[cooldownKey],
                    now.timeIntervalSince(last) < anomalyNotificationCooldown
                {
                    continue
                }
                lastBudgetBreachDates[cooldownKey] = now

                fireBudgetNotification(
                    entity: entity, rule: rule, metric: metric, rate: ratePerHour)
                // The configured action runs regardless of notification settings.
                let event = TimelineEvent(
                    id: "agent-budget:\(cooldownKey):\(nowMillis)",
                    timestampMillis: nowMillis,
                    category: .anomaly,
                    severity: .warning,
                    entityId: entity.entityId,
                    title: "AI agent budget exceeded: \(entity.displayName)",
                    detail: budgetBreachDetail(
                        metric: metric, rate: ratePerHour, threshold: rule.budgetThreshold)
                )
                executeAutomationAction(rule, event: event)
            }
        }
    }

    /// The cumulative value of the watched metric on an agent-cost summary.
    private func agentMetricValue(_ cost: AgentCostSummary, _ metric: AgentBudgetMetric) -> Double {
        switch metric {
        case .costPerHour: return Double(cost.costUsd)
        case .tokensPerHour: return Double(cost.totalInputTokens + cost.totalOutputTokens)
        }
    }

    /// Oldest persisted sample of this agent within the window that carries an
    /// agent_cost, used as the rate baseline. `historySnapshots` is ascending,
    /// so the first match is the oldest.
    private func agentBudgetBaseline(
        entityId: String,
        metric: AgentBudgetMetric,
        sinceMillis: UInt64
    ) -> (millis: UInt64, value: Double)? {
        for snapshot in historySnapshots where snapshot.capturedAtMillis >= sinceMillis {
            if let entity = snapshot.entities.first(where: { $0.entityId == entityId }),
                let cost = entity.agentCost
            {
                return (snapshot.capturedAtMillis, agentMetricValue(cost, metric))
            }
        }
        return nil
    }

    private func budgetRateLabel(metric: AgentBudgetMetric, rate: Double) -> String {
        switch metric {
        case .costPerHour: return String(format: "$%.2f/hr", rate)
        case .tokensPerHour: return String(format: "%.0f tokens/hr", rate)
        }
    }

    private func budgetBreachDetail(metric: AgentBudgetMetric, rate: Double, threshold: Double)
        -> String
    {
        "Rate \(budgetRateLabel(metric: metric, rate: rate)) exceeds the \(budgetRateLabel(metric: metric, rate: threshold)) limit (derived from recent history)."
    }

    private func fireBudgetNotification(
        entity: EntitySnapshot,
        rule: AutomationRule,
        metric: AgentBudgetMetric,
        rate: Double
    ) {
        recordLocalDiagnosticsEvent(
            level: .warn,
            subsystem: .ui,
            eventType: "agent-budget-breach",
            message: "AI agent \(entity.displayName) exceeded its budget.",
            entityId: entity.entityId,
            fields: [
                DiagnosticsField(key: "rule", value: rule.name),
                DiagnosticsField(key: "metric", value: metric.rawValue),
                DiagnosticsField(key: "rate", value: budgetRateLabel(metric: metric, rate: rate)),
                DiagnosticsField(
                    key: "threshold",
                    value: budgetRateLabel(metric: metric, rate: rule.budgetThreshold)),
            ]
        )
        guard notifyAgentBudget else { return }
        if let bundleId = entity.bundleId, isSnoozed(bundleId: bundleId) { return }
        postUserNotification(
            identifier: "agent-budget-\(rule.id.uuidString)-\(entity.entityId)",
            title: "AI Agent Budget Alert",
            body:
                "\(entity.displayName) is at \(budgetRateLabel(metric: metric, rate: rate)) — over the \(budgetRateLabel(metric: metric, rate: rule.budgetThreshold)) limit."
        )
    }

    private func automationRuleMatches(
        _ rule: AutomationRule,
        _ event: TimelineEvent,
        in snapshot: SystemSnapshot
    ) -> Bool {
        // Category gate. `.networkConnection` targets the `.network` category;
        // every other concrete event matches a category whose key equals its
        // raw value.
        if let requiredCategory = requiredCategoryKey(for: rule.event),
            requiredCategory != timelineCategoryKey(event.category)
        {
            return false
        }

        // Title/detail substring gate (shared by all rules).
        let needle = rule.titleContains.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if !needle.isEmpty,
            !(event.title.lowercased().contains(needle)
                || event.detail.lowercased().contains(needle))
        {
            return false
        }

        // Connection-specific predicates (signing class + remote host) resolve
        // against the connecting entity in the snapshot — a pure lookup, no
        // codesign at match time.
        if rule.event == .networkConnection {
            return connectionPredicatesMatch(rule, event, in: snapshot)
        }
        return true
    }

    /// The timeline-category key a rule's event must match, or `nil` for `.any`
    /// (no category constraint).
    private func requiredCategoryKey(for event: AutomationEvent) -> String? {
        switch event {
        case .any: return nil
        case .networkConnection: return "network"
        default: return event.rawValue
        }
    }

    /// Evaluate the signing-class and remote-host predicates for a
    /// network-connection rule against the event's entity. An empty predicate
    /// means "any"; a rule with neither predicate matches every connection
    /// event that passed the category/title gates.
    private func connectionPredicatesMatch(
        _ rule: AutomationRule,
        _ event: TimelineEvent,
        in snapshot: SystemSnapshot
    ) -> Bool {
        if rule.signingClasses.isEmpty, rule.remoteHostContains.isEmpty {
            return true
        }
        // Both predicates require the entity; if it's gone from the snapshot we
        // can't verify, so don't fire (avoids false positives on stale events).
        guard let entityId = event.entityId,
            let entity = snapshot.entities.first(where: { $0.entityId == entityId })
        else {
            return false
        }
        if !rule.signingClasses.isEmpty,
            !rule.signingClasses.contains(entity.signingClassification)
        {
            return false
        }
        if !rule.remoteHostContains.isEmpty {
            let host = rule.remoteHostContains
                .trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            let hit = entity.networkConnections.contains { connection in
                connection.remote?.lowercased().contains(host) ?? false
            }
            if !hit { return false }
        }
        return true
    }

    private func timelineCategoryKey(_ category: TimelineCategory) -> String {
        switch category {
        case .lifecycle: return "lifecycle"
        case .friction: return "friction"
        case .host: return "host"
        case .thermal: return "thermal"
        case .anomaly: return "anomaly"
        case .network: return "network"
        case .regression: return "regression"
        @unknown default: return "any"
        }
    }

    private func executeAutomationAction(_ rule: AutomationRule, event: TimelineEvent) {
        let value = rule.actionValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { return }
        let process = Process()
        switch rule.actionKind {
        case .shortcut:
            process.executableURL = URL(fileURLWithPath: "/usr/bin/shortcuts")
            process.arguments = ["run", value]
        case .shell:
            process.executableURL = URL(fileURLWithPath: "/bin/sh")
            process.arguments = ["-c", value]
        }
        var launchError: String?
        do {
            try process.run()
        } catch {
            launchError = error.localizedDescription
        }
        recordLocalDiagnosticsEvent(
            level: launchError == nil ? .info : .warn,
            subsystem: .ui,
            eventType: "automation-rule",
            message: launchError.map { "Automation '\(rule.name)' failed to launch: \($0)" }
                ?? "Automation '\(rule.name)' triggered \(rule.actionKind.label.lowercased()) for: \(event.title)",
            entityId: event.entityId,
            fields: [
                DiagnosticsField(key: "rule", value: rule.name),
                DiagnosticsField(key: "action_kind", value: rule.actionKind.rawValue),
                DiagnosticsField(key: "action_value", value: value),
                DiagnosticsField(key: "event", value: event.title),
            ]
        )
    }

    public func exportDiagnostics(limit: UInt32 = 1000) {
        exportController.exportDiagnostics(privacyTier: exportPrivacyTier, limit: limit)
    }

    public func clearDiagnostics() {
        if let error = bridge.clearDiagnostics() {
            lastError = error
            return
        }
        diagnosticsEvents = []
        diagnosticsRecentWarningCount = 0
        diagnosticsRecentErrorCount = 0
        diagnosticsOverview = bridge.diagnosticsOverview()
        diagnosticsLoadError = nil
        sessionLogAnalysisError = nil
        uiPerformanceBudgetDiagnostics = .empty
    }

    public func clearHistory() {
        if let error = bridge.clearHistory() {
            lastError = error
            return
        }
        historySnapshots = []
        historyLoadError = "Persisted history was cleared."
        historyRangeSummary = nil
        historyStoreSummary = nil
        historyMaintenanceReport = nil
        historyHasMore = false
        historyLoadStatus = nil
        historyLastLoadDurationMillis = 0
        historyUiDiagnostics = .empty
        historySnapshotDiffIsLoading = false
        historySnapshotDiff = nil
        historySnapshotDiffError = nil
        historyCompareBeforeMillis = nil
        historyCompareAfterMillis = nil
        historyDiffTask?.cancel()
        historyDiffTask = nil
        timelinePayloadDiagnostics = .empty
        uiPerformanceBudgetDiagnostics = .empty
        loadDiagnostics(force: true)
    }

    public func setHistoryVisible(_ visible: Bool) {
        historyVisible = visible
        if visible {
            loadHistory(force: true)
        } else {
            historyLoadTask?.cancel()
            historyDiffTask?.cancel()
            historyIsLoading = false
            historyIsLoadingMore = false
            historySnapshotDiffIsLoading = false
        }
    }

    public func setDiagnosticsVisible(_ visible: Bool) {
        diagnosticsVisible = visible
        updateLagMonitoringState()
    }

    public func exportSupportBundle(_ settings: SettingsStore, diagnosticsLimit: UInt32 = 1_500) {
        let context = ExportSupportBundleContext(
            diagnosticsOverview: diagnosticsOverview,
            diagnosticsRecentWarningCount: diagnosticsRecentWarningCount,
            diagnosticsRecentErrorCount: diagnosticsRecentErrorCount,
            runtimeLagMetrics: runtimeLagMetrics,
            uiPerformanceBudgetDiagnostics: uiPerformanceBudgetDiagnostics,
            notificationAuthorizationStatus: notificationAuthorizationStatus,
            lastDiagnosticsQueryDate: lastDiagnosticsQueryDate,
            lastSessionLogAnalysisCompletedDate: lastSessionLogAnalysisCompletedDate,
            sessionLogAnalysisError: sessionLogAnalysisError,
            sessionLogSummary: sessionLogSummary,
            historyWindowSeconds: historyWindowSeconds,
            historySnapshots: historySnapshots,
            historyLoadError: historyLoadError,
            historyRangeSummary: historyRangeSummary,
            historyMaintenanceReport: historyMaintenanceReport,
            telemetryVerificationStatus: telemetryVerificationStatus
        )
        switch exportController.exportSupportBundle(
            settings,
            diagnosticsLimit: diagnosticsLimit,
            context: context
        ) {
        case .success(let packageURL):
            recordLocalDiagnosticsEvent(
                level: .info,
                subsystem: .ui,
                eventType: "support-bundle-exported",
                message: "Exported a support bundle.",
                fields: [
                    DiagnosticsField(key: "path", value: packageURL.path),
                    DiagnosticsField(key: "history_snapshot_count", value: String(historySnapshots.count)),
                    DiagnosticsField(key: "diagnostics_event_count", value: String(diagnosticsEvents.count)),
                ]
                ,
                sensitive: true
            )
        case .cancelled:
            return
        case .failure(let message):
            lastError = message
            recordLocalDiagnosticsEvent(
                level: .error,
                subsystem: .ui,
                eventType: "support-bundle-export-failed",
                message: "Failed to export a support bundle.",
                fields: [
                    DiagnosticsField(key: "error", value: message),
                ]
            )
        }
    }

    public func applyNotificationSettings(_ settings: SettingsStore) {
        notificationsEnabled = settings.notificationsEnabled
        frictionNotificationThreshold = settings.frictionNotificationThreshold
        notifyThermal = settings.notifyThermal
        notifyRegression = settings.notifyRegression
        notifyRestartLoop = settings.notifyRestartLoop
        notifyNetwork = settings.notifyNetwork
        notifyAgentBudget = settings.notifyAgentBudget
        if settings.notificationsEnabled {
            flushSuppressedAnomalySummaryIfNeeded(force: true)
            requestNotificationPermissionIfNeeded(trigger: "notifications-enabled")
        } else {
            notificationAuthorizationStatus = "disabled"
            recordLocalDiagnosticsEvent(
                level: .info,
                subsystem: .ui,
                eventType: "notification-settings-disabled",
                message: "Notifications are disabled in settings."
            )
        }
    }

    public func applyRuntimeCollectionSettings(_ settings: SettingsStore) {
        configureRuntimeCollection(settings)
        updateLagMonitoringState()
        refresh(force: true)
    }

    public func applyStoragePolicySettings(_ settings: SettingsStore) {
        storageScheduledScansEnabled = settings.storageScheduledScansEnabled
        storageScheduledScanIntervalSeconds =
            SettingsStore.normalizedStorageScheduledScanIntervalHours(
                settings.storageScheduledScanIntervalHours
            ) * 3600

        storageScheduledScanTask?.cancel()
        storageScheduledScanTask = nil

        guard storageScheduledScansEnabled else {
            return
        }

        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "storage-scheduled-scans-enabled",
            message: "Storage scheduled scans are enabled.",
            fields: [
                DiagnosticsField(
                    key: "interval_hours",
                    value: String(format: "%.1f", storageScheduledScanIntervalSeconds / 3600)
                )
            ]
        )
        storageScheduledScanTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                let sleepNanos = await MainActor.run {
                    self.storageScheduledScanSleepNanos()
                }
                try? await Task.sleep(nanoseconds: sleepNanos)
                guard !Task.isCancelled else { break }
                await MainActor.run {
                    self.runScheduledStorageHygieneScanIfDue()
                }
            }
        }
    }

    public func applyIntegrationSettings(_ settings: SettingsStore) {
        let chromiumEndpoint = settings.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        let privilegedHelperPath = settings.privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines)

        bridge.configureChromiumEndpoint(chromiumEndpoint.isEmpty ? nil : chromiumEndpoint)
        bridge.configureDockerSocketPath(SettingsStore.normalizedDockerSocketPath(settings.dockerSocketPath))
        bridge.configurePrivilegedHelper(
            path: privilegedHelperPath.isEmpty ? nil : privilegedHelperPath,
            enabled: settings.privilegedHelperEnabled
        )

        let chau7Endpoint = settings.chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        bridge.configureChau7Endpoint(chau7Endpoint.isEmpty ? nil : chau7Endpoint)

        // Binary reputation: only forward the key when the feature is enabled,
        // and read it from the Keychain (never UserDefaults). An empty key
        // disables lookups engine-side.
        let virusTotalKey =
            settings.binaryReputationEnabled
            ? (KeychainHelper.retrieve(account: KeychainHelper.binaryReputationAccount) ?? "")
            : ""
        bridge.setBinaryReputationConfig(
            enabled: settings.binaryReputationEnabled,
            apiKey: virusTotalKey
        )
        let telemetryEndpoint = SettingsStore.normalizedTelemetryEndpoint(settings.telemetryEndpoint)
        self.telemetryEnabled = settings.telemetryEnabled
        self.telemetryEndpoint = telemetryEndpoint
        telemetryVerificationStatus = nil
        bridge.configureTelemetry(
            endpoint: telemetryEndpoint,
            enabled: settings.telemetryEnabled,
            exportIntervalSeconds: SettingsStore.normalizedTelemetryExportIntervalSeconds(
                settings.telemetryExportIntervalSeconds
            )
        )

        updateLagMonitoringState()
        refresh(force: true)
    }

    public func verifyTelemetryExport(_ settings: SettingsStore) {
        applyIntegrationSettings(settings)
        telemetryVerificationStatus = "Verifying..."
        let bridge = self.bridge
        Task(priority: .utility) { [weak self] in
            let result = bridge.verifyTelemetryExport()
            await MainActor.run {
                guard let self else { return }
                if let result, !result.isEmpty {
                    self.telemetryVerificationStatus = "Verification failed: \(result)"
                    self.lastError = result
                } else {
                    self.telemetryVerificationStatus = "Verification succeeded."
                }
                self.loadDiagnostics(force: true)
            }
        }
    }

    private func configureRuntimeCollection(_ settings: SettingsStore) {
        bridge.configureRuntimeCollection(
            fullCollection: settings.collectionProfile == .full,
            adaptiveCadence: settings.adaptiveCadenceEnabled,
            activeTickMillis: SettingsStore.milliseconds(
                from: settings.engineActiveIntervalSeconds,
                minimumSeconds: SettingsStore.minimumEngineTickSeconds
            ),
            idleTickMillis: SettingsStore.milliseconds(
                from: settings.engineIdleIntervalSeconds,
                minimumSeconds: SettingsStore.minimumEngineTickSeconds
            ),
            lowPowerTickMillis: SettingsStore.milliseconds(
                from: settings.engineLowPowerIntervalSeconds,
                minimumSeconds: SettingsStore.minimumEngineTickSeconds
            ),
            gpuSampleIntervalMillis: SettingsStore.milliseconds(
                from: settings.gpuSampleIntervalSeconds,
                minimumSeconds: SettingsStore.minimumGPUSampleIntervalSeconds
            ),
            gpuSampleLowPowerIntervalMillis: SettingsStore.milliseconds(
                from: settings.gpuSampleLowPowerIntervalSeconds,
                minimumSeconds: SettingsStore.minimumGPUSampleIntervalSeconds
            )
        )
    }

    public func refreshLocalMcpClientStatuses() {
        localMcpController.refreshClientStatuses()
    }

    public func registerSupportedLocalMcpClients() {
        localMcpController.registerSupportedClients()
        consumeLocalMcpError()
    }

    public func applyLocalMcpClientRegistrationSettings(_ settings: SettingsStore) {
        let modeChanged = localMcpOperatorActionsEnabled != settings.localMcpOperatorActionsEnabled
        localMcpOperatorActionsEnabled = settings.localMcpOperatorActionsEnabled
        if modeChanged {
            ensureLocalMcpServer(force: true)
        }
        localMcpController.applyClientRegistrationSettings(settings)
        consumeLocalMcpError()
    }

    public func copyLocalMcpConfigSnippet(providerId: String) {
        localMcpController.copyConfigSnippet(providerId: providerId)
        consumeLocalMcpError()
    }

    public func refresh(force: Bool = false) {
        ensureLocalMcpServer()
        let refreshStartedAt = CFAbsoluteTimeGetCurrent()
        if refreshInFlight {
            if force {
                pendingForcedRefresh = true
            }
            return
        }

        refreshInFlight = true
        let includeOperatorState = shouldRefreshOperatorState(force: force)
        // The per-payload byte-count walk only feeds the Diagnostics tab and
        // telemetry mirror; skip it when neither is watching.
        let collectPayloadDiagnostics = diagnosticsVisible || telemetryEnabled
        let fetchFullSnapshot = force
            || !fullSnapshotDemandTokens.isEmpty
            || ticksSinceFullSnapshot >= fullSnapshotFloorTicks
        let worker = snapshotRefreshWorker
        refreshFetchTask = Task(priority: .utility) { [weak self] in
            do {
                let result = try await worker.refresh(
                    force: force,
                    includeOperatorState: includeOperatorState,
                    collectPayloadDiagnostics: collectPayloadDiagnostics,
                    fetchFullSnapshot: fetchFullSnapshot
                )
                let wasCancelled = Task.isCancelled
                await MainActor.run {
                    guard let self else { return }
                    if wasCancelled {
                        self.completeSnapshotRefresh()
                    } else {
                        self.applySnapshotRefreshResult(
                            result,
                            refreshStartedAt: refreshStartedAt
                        )
                    }
                }
            } catch {
                let wasCancelled = Task.isCancelled
                await MainActor.run {
                    guard let self else { return }
                    if wasCancelled {
                        self.completeSnapshotRefresh()
                    } else {
                        self.lastError = error.localizedDescription
                        self.completeSnapshotRefresh()
                    }
                }
            }
        }
    }

    private func ensureLocalMcpServer(force: Bool = false) {
        localMcpController.ensureServer(
            force: force,
            operatorActionsEnabled: localMcpOperatorActionsEnabled
        )
        consumeLocalMcpError()
    }

    private func consumeLocalMcpError() {
        if let error = localMcpController.consumeLastError() {
            lastError = error
        }
    }

    private func shouldRefreshOperatorState(force: Bool) -> Bool {
        force || Date().timeIntervalSince(lastOperatorStateRefreshDate) >= operatorStateRefreshInterval
    }

    private func applySnapshotRefreshResult(
        _ result: SnapshotRefreshResult,
        refreshStartedAt: CFAbsoluteTime
    ) {
        switch result {
        case .noChange:
            completeSnapshotRefresh()
            return
        case let .updated(payload):
            let decodeStartedAt = CFAbsoluteTimeGetCurrent()
            var monitorFetchDiagnostics: MonitorUiPayloadFetchDiagnostics?
            if let monitorPayload = payload.monitorPayload {
                monitorFetchDiagnostics = applyMonitorUiPayload(monitorPayload)
            }
            let monitorDecodeMillis = (CFAbsoluteTimeGetCurrent() - decodeStartedAt) * 1000.0
            let publishStartedAt = CFAbsoluteTimeGetCurrent()
            if let refreshedSnapshot = payload.snapshot {
                snapshot = refreshedSnapshot
                publishSnapshotSlices(refreshedSnapshot)
                lastObservedSequence = refreshedSnapshot.sequence
                ticksSinceFullSnapshot = 0
                applyLocalFrontmostState(
                    appName: lastPublishedFrontmostAppName,
                    windowTitle: lastPublishedWindowTitle
                )
            } else {
                ticksSinceFullSnapshot += 1
            }
            runtimeLagMetrics = payload.runtimeLagMetrics
            if let operatorState = payload.operatorState {
                diagnosticsOverview = operatorState.diagnosticsOverview
                historyStoreSummary = operatorState.historyStoreSummary
                lastOperatorStateRefreshDate = Date()
                localMcpController.refreshHealthSnapshot()
                refreshRepositoryInventorySignalsIfQuiescent()
                refreshStorageEstimateIfQuiescent()
                if let refreshedSnapshot = payload.snapshot {
                    pruneOnDemandReportCaches(snapshot: refreshedSnapshot)
                }
            }

            if let refreshedSnapshot = payload.snapshot {
                evaluateAutomationRules(snapshot: refreshedSnapshot)
                evaluateAgentBudgetRules(snapshot: refreshedSnapshot)
                evaluateTimelineNotifications(snapshot: refreshedSnapshot)
            }

            if lagMonitoringActive, let refreshedSnapshot = payload.snapshot {
                publishUiLagMetrics(
                    snapshot: refreshedSnapshot,
                    bridgeFetchMillis: payload.bridgeFetchMillis,
                    uiRefreshMillis: (CFAbsoluteTimeGetCurrent() - refreshStartedAt) * 1000.0,
                    refreshStartedAt: refreshStartedAt
                )
            }
            if let monitorFetchDiagnostics {
                let renderPublishMillis = (CFAbsoluteTimeGetCurrent() - publishStartedAt) * 1000.0
                recordUiPerformancePayloadDiagnostics(
                    fetchDiagnostics: monitorFetchDiagnostics,
                    decodeMillis: monitorDecodeMillis,
                    renderPublishMillis: renderPublishMillis
                )
            }
            if payload.snapshot != nil {
                diffAnomalyStates()
                flushSuppressedAnomalySummaryIfNeeded()
            }
            lastError = nil
            completeSnapshotRefresh()
        }
    }

    private func applyMonitorUiPayload(_ payload: MonitorUiRefreshPayload) -> MonitorUiPayloadFetchDiagnostics {
        switch payload {
        case let .snapshot(snapshot, diagnostics):
            monitorViewModel = MonitorViewModel(snapshot: snapshot)
            return diagnostics
        case let .delta(delta, diagnostics):
            monitorViewModel.apply(delta: delta)
            return diagnostics
        }
    }

    /// Register a view's need for the full SystemSnapshot decode. The first
    /// live token forces an immediate refresh so a newly-shown tab never
    /// waits out the floor cadence on stale data.
    public func beginFullSnapshotDemand() -> UUID {
        let token = UUID()
        let wasEmpty = fullSnapshotDemandTokens.isEmpty
        fullSnapshotDemandTokens.insert(token)
        if wasEmpty {
            refresh(force: true)
        }
        return token
    }

    public func endFullSnapshotDemand(_ token: UUID) {
        fullSnapshotDemandTokens.remove(token)
    }

    private func completeSnapshotRefresh() {
        refreshFetchTask = nil
        refreshInFlight = false
        if pendingForcedRefresh {
            pendingForcedRefresh = false
            refresh(force: true)
        }
    }

    /// Event-driven git-signal refresh: when FSEvents recorded changes under
    /// watched roots and the filesystem has been quiet for a beat, re-derive
    /// the lightweight repository inventory (branch/dirty/fingerprints) and
    /// merge it into the displayed report — without a full storage scan.
    /// Checked on the operator-state cadence (~30s), so worst-case freshness
    /// is quiescence window + one cadence tick.
    private func refreshRepositoryInventorySignalsIfQuiescent() {
        guard let report = storageHygieneReport else { return }
        guard !storageHygieneIsLoading, !storageHygieneIsVerifyingCache, storageScanJob == nil else { return }
        guard let lastChange = StorageRootChangeJournal.lastChangeMillis() else { return }
        guard lastChange > lastInventorySignalRefreshMillis else { return }
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        // Require quiescence, not just an event: builds fire FSEvents storms.
        guard nowMillis >= lastChange + 45_000 else { return }
        lastInventorySignalRefreshMillis = nowMillis

        let roots = report.roots
        // Same bounded depth the scheduled verification pass uses.
        let maxDepth: UInt32 = 5
        let bridge = self.bridge
        let publisher = StorageHygieneMainActorPublisher(self)
        repositoryInventorySignalTask?.cancel()
        repositoryInventorySignalTask = Task.detached(priority: .utility) { [bridge, publisher] in
            await publisher.publishRepositoryInventoryRefreshState(
                RepositoryInventoryRefreshState(
                    phase: .refreshingChangedRepositories,
                    checkedRepositoryCount: 0,
                    changedRepositoryCount: 0,
                    missingRepositoryCount: 0,
                    sampleRoots: []
                )
            )
            let inventory = bridge.repositoryInventoryJSON(roots: roots, maxDepth: maxDepth)
            guard !Task.isCancelled else { return }
            guard let decoded = Self.decodeRepositoryInventoryReport(inventory) else {
                await publisher.publishRepositoryInventoryRefreshState(nil)
                return
            }
            await publisher.publishInventoryVerification(decoded, signalOnly: true)
            await publisher.publishRepositoryInventoryRefreshState(nil)
        }
    }

    private static let storageEstimateQuietMillis: UInt64 = 45_000
    private static let storageEstimateRefreshCooldownMillis: UInt64 = 120_000
    private static let storageEstimateFullScanDirtyPathThreshold = 220

    /// Low-impact storage freshness loop. FSEvents only marks roots dirty; this
    /// waits for quiescence and starts the existing changed-only Rust scan job.
    /// That keeps totals fresh after cleanup/build activity without rescanning
    /// on every write burst or blocking the visible Storage report.
    private func refreshStorageEstimateIfQuiescent() {
        guard let report = storageHygieneReport else { return }
        updateStorageEstimateStatus(report: report)
        guard !storageHygieneIsLoading, !storageHygieneIsVerifyingCache, storageScanJob == nil else { return }

        let summary = StorageRootChangeJournal.summary(sampleLimit: 4)
        guard summary.hasChanges, let lastChange = summary.lastChangeMillis else { return }
        guard lastChange > report.capturedAtMillis else { return }

        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        guard nowMillis >= lastChange + Self.storageEstimateQuietMillis else { return }
        guard nowMillis >= lastStorageEstimateDecisionMillis + Self.storageEstimateRefreshCooldownMillis else { return }
        lastStorageEstimateDecisionMillis = nowMillis

        guard summary.dirtyPathCount < Self.storageEstimateFullScanDirtyPathThreshold else {
            storageEstimateStatus = StorageEstimateStatus(
                confidence: .needsFullScan,
                title: "Full Scan Needed",
                detail: "\(summary.dirtyPathCount) changed paths recorded; run a full scan to avoid underestimating broad filesystem churn.",
                dirtyPathCount: summary.dirtyPathCount,
                lastChangeMillis: lastChange,
                lastRefreshMillis: lastStorageEstimateRefreshMillis == 0
                    ? report.capturedAtMillis
                    : lastStorageEstimateRefreshMillis
            )
            recordLocalDiagnosticsEvent(
                level: .info,
                subsystem: .ui,
                eventType: "storage-estimate-refresh-deferred",
                message: "Deferred automatic storage re-estimation because too many paths changed.",
                fields: [
                    DiagnosticsField(key: "dirty_path_count", value: String(summary.dirtyPathCount)),
                    DiagnosticsField(
                        key: "dirty_path_threshold",
                        value: String(Self.storageEstimateFullScanDirtyPathThreshold)
                    ),
                    DiagnosticsField(key: "sample_paths", value: summary.samplePaths.joined(separator: " | ")),
                ]
            )
            return
        }

        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "storage-estimate-refresh-started",
            message: "Started changed-only storage re-estimation after filesystem quiescence.",
            fields: [
                DiagnosticsField(key: "dirty_path_count", value: String(summary.dirtyPathCount)),
                DiagnosticsField(key: "root_count", value: String(report.roots.count)),
                DiagnosticsField(key: "mode", value: "fast_changed_only"),
                DiagnosticsField(key: "sample_paths", value: summary.samplePaths.joined(separator: " | ")),
            ]
        )
        startStorageScanJob(
            roots: report.roots,
            maxDepth: 5,
            limit: 200,
            mode: "fast_changed_only"
        )
    }

    private func updateStorageEstimateStatus(report: StorageHygieneReportModel? = nil) {
        if let job = storageScanJob, job.isActive {
            storageEstimateStatus = StorageEstimateStatus(
                confidence: .refreshing,
                title: "Refreshing",
                detail: "\(job.progress.phase) · \(job.progress.currentPathHint ?? "checking changed storage paths")",
                dirtyPathCount: StorageRootChangeJournal.summary().dirtyPathCount,
                lastChangeMillis: StorageRootChangeJournal.lastChangeMillis(),
                lastRefreshMillis: lastStorageEstimateRefreshMillis == 0 ? nil : lastStorageEstimateRefreshMillis
            )
            return
        }

        let currentReport = report ?? storageHygieneReport
        let summary = StorageRootChangeJournal.summary(sampleLimit: 3)
        let lastRefresh = currentReport?.capturedAtMillis
            ?? (lastStorageEstimateRefreshMillis == 0 ? nil : lastStorageEstimateRefreshMillis)

        guard summary.hasChanges, let lastChange = summary.lastChangeMillis else {
            storageEstimateStatus = StorageEstimateStatus(
                confidence: .verified,
                title: "Verified",
                detail: "Latest storage totals come from a completed scan.",
                dirtyPathCount: 0,
                lastChangeMillis: nil,
                lastRefreshMillis: lastRefresh
            )
            return
        }

        if let currentReport, lastChange <= currentReport.capturedAtMillis {
            storageEstimateStatus = StorageEstimateStatus(
                confidence: .verified,
                title: "Verified",
                detail: "Latest storage totals include the recorded filesystem changes.",
                dirtyPathCount: 0,
                lastChangeMillis: lastChange,
                lastRefreshMillis: currentReport.capturedAtMillis
            )
            return
        }

        if summary.dirtyPathCount >= Self.storageEstimateFullScanDirtyPathThreshold {
            storageEstimateStatus = StorageEstimateStatus(
                confidence: .needsFullScan,
                title: "Full Scan Needed",
                detail: "\(summary.dirtyPathCount) changed paths are queued; this is too broad for a cheap estimate.",
                dirtyPathCount: summary.dirtyPathCount,
                lastChangeMillis: lastChange,
                lastRefreshMillis: lastRefresh
            )
            return
        }

        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        let isQuiet = nowMillis >= lastChange + Self.storageEstimateQuietMillis
        storageEstimateStatus = StorageEstimateStatus(
            confidence: isQuiet ? .estimated : .stale,
            title: isQuiet ? "Estimating" : "Watching",
            detail: isQuiet
                ? "\(summary.dirtyPathCount) changed paths are ready for a changed-only refresh."
                : "\(summary.dirtyPathCount) changed paths recorded; waiting for filesystem activity to quiet.",
            dirtyPathCount: summary.dirtyPathCount,
            lastChangeMillis: lastChange,
            lastRefreshMillis: lastRefresh
        )
    }

    /// Drop cached on-demand reports whose entity/pid has left the live
    /// snapshot. Reports are refetchable, so bounding the caches to live
    /// subjects (plus one grace pass) caps long-session memory growth.
    private func pruneOnDemandReportCaches(snapshot: SystemSnapshot) {
        let liveEntityIds = Set(snapshot.entities.map(\.entityId))
        let livePids = Set(snapshot.entities.flatMap { $0.components.compactMap(\.processId) })

        let entityKeys = Set(entityAnomalyExplanations.keys)
            .union(entityProcessTreeReports.keys)
            .union(entityMemoryBreakdowns.keys)
            .union(entityProfiles.keys)
            .union(entityWakeupAttributions.keys)
        let missingEntityIds = entityKeys.subtracting(liveEntityIds)
        for entityId in stalePruneEntityIds.intersection(missingEntityIds) {
            entityAnomalyExplanations.removeValue(forKey: entityId)
            entityProcessTreeReports.removeValue(forKey: entityId)
            entityMemoryBreakdowns.removeValue(forKey: entityId)
            entityProfiles.removeValue(forKey: entityId)
            entityWakeupAttributions.removeValue(forKey: entityId)
        }
        stalePruneEntityIds = missingEntityIds

        let pidKeys = Set(processInspections.keys)
            .union(processOpenResources.keys)
            .union(processSamples.keys)
        let missingPids = pidKeys.subtracting(livePids)
        for pid in stalePrunePids.intersection(missingPids) {
            processInspections.removeValue(forKey: pid)
            processOpenResources.removeValue(forKey: pid)
            processSamples.removeValue(forKey: pid)
        }
        stalePrunePids = missingPids
    }

    public func setHistoryWindow(seconds: TimeInterval) {
        historyWindowSeconds = max(seconds, 300)
        if historyVisible {
            loadHistory(force: true)
        }
    }

    public func setHistoryRangeEnd(date: Date) {
        let requestedMillis = UInt64(max(0.0, date.timeIntervalSince1970) * 1000)
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        historyRangeEndOverrideMillis = min(requestedMillis, nowMillis)
        if historyVisible {
            loadHistory(force: true)
        }
    }

    public func clearHistoryRangeEndOverride() {
        historyRangeEndOverrideMillis = nil
        if historyVisible {
            loadHistory(force: true)
        }
    }

    public func focusEntityInMonitor(_ entityID: String) {
        monitorFocusEntityID = entityID
    }

    public func consumeMonitorFocusEntityID() -> String? {
        let entityID = monitorFocusEntityID
        monitorFocusEntityID = nil
        return entityID
    }

    func entityAnalysisIsLoading(_ entityID: String, kind: EntityAnalysisKind) -> Bool {
        if kind == .processAction || kind == .processActionHistory {
            return processActionController.isLoading(entityID, kind: kind)
        }
        return entityAnalysisLoadingKeys.contains(entityAnalysisKey(entityID, kind: kind))
    }

    func entityAnalysisError(_ entityID: String, kind: EntityAnalysisKind) -> String? {
        if kind == .processAction || kind == .processActionHistory {
            return processActionController.error(entityID, kind: kind)
        }
        return entityAnalysisErrorMessages[entityAnalysisKey(entityID, kind: kind)]
    }

    func entityAnalysisUpdatedAt(_ entityID: String, kind: EntityAnalysisKind) -> Date? {
        if kind == .processAction || kind == .processActionHistory {
            return processActionController.updatedAt(entityID, kind: kind)
        }
        return entityAnalysisUpdatedAtByKey[entityAnalysisKey(entityID, kind: kind)]
    }

    func loadEntityStaticAnalysis(entityID: String, force: Bool = false) {
        guard snapshot.entities.contains(where: { $0.entityId == entityID }) else {
            return
        }
        if !force,
           let processUpdatedAt = entityAnalysisUpdatedAt(entityID, kind: .processTree),
           let anomalyUpdatedAt = entityAnalysisUpdatedAt(entityID, kind: .anomalyExplanation),
           Date().timeIntervalSince(processUpdatedAt) < entityStaticAnalysisReloadInterval,
           Date().timeIntervalSince(anomalyUpdatedAt) < entityStaticAnalysisReloadInterval {
            return
        }

        setEntityAnalysisLoading(entityID, kind: .processTree, isLoading: true)
        setEntityAnalysisLoading(entityID, kind: .anomalyExplanation, isLoading: true)

        let bridge = self.bridge
        Task(priority: .utility) { [weak self] in
            let processTreeResult = bridge.entityProcessTreeJSON(entityId: entityID)
            let anomalyResult = bridge.explainAnomaliesJSON(entityIds: [entityID], limit: 1, windowMinutes: 20)
            await MainActor.run {
                guard let self else { return }
                self.entityProcessTreeReports[entityID] = self.decodeJsonQueryResult(
                    processTreeResult,
                    as: EntityProcessTreeReportModel.self
                )
                self.finishEntityAnalysis(
                    entityID,
                    kind: .processTree,
                    result: processTreeResult,
                    fallback: "Process-tree analysis is unavailable for this entity."
                )
                self.entityAnomalyExplanations[entityID] = self.decodeJsonQueryResult(
                    anomalyResult,
                    as: [AnomalyExplanationReport].self
                )?.first
                self.finishEntityAnalysis(
                    entityID,
                    kind: .anomalyExplanation,
                    result: anomalyResult,
                    fallback: "Anomaly explanation is unavailable for this entity."
                )
            }
        }
    }

    func runEntityMemoryBreakdown(entityID: String, topRegions: UInt32 = 8) {
        let bridge = self.bridge
        setEntityAnalysisLoading(entityID, kind: .memoryBreakdown, isLoading: true)
        Task(priority: .utility) { [weak self] in
            let result = bridge.memoryBreakdownJSON(entityId: entityID, topRegions: topRegions)
            await MainActor.run {
                guard let self else { return }
                self.entityMemoryBreakdowns[entityID] = self.decodeJsonQueryResult(
                    result,
                    as: EntityMemoryBreakdownReportModel.self
                )
                self.finishEntityAnalysis(
                    entityID,
                    kind: .memoryBreakdown,
                    result: result,
                    fallback: "Memory breakdown could not be collected."
                )
            }
        }
    }

    func runEntityProfile(
        entityID: String,
        durationSeconds: UInt32 = 5,
        topStacks: UInt32 = 6
    ) {
        let bridge = self.bridge
        setEntityAnalysisLoading(entityID, kind: .profile, isLoading: true)
        Task(priority: .utility) { [weak self] in
            let result = bridge.profileEntityJSON(
                entityId: entityID,
                durationSeconds: durationSeconds,
                topStacks: topStacks
            )
            await MainActor.run {
                guard let self else { return }
                self.entityProfiles[entityID] = self.decodeJsonQueryResult(
                    result,
                    as: EntityProfileReportModel.self
                )
                self.finishEntityAnalysis(
                    entityID,
                    kind: .profile,
                    result: result,
                    fallback: "Sampled profile could not be collected."
                )
            }
        }
    }

    func runEntityWakeupAttribution(
        entityID: String,
        durationSeconds: UInt32 = 5,
        topStacks: UInt32 = 6
    ) {
        let bridge = self.bridge
        setEntityAnalysisLoading(entityID, kind: .wakeupAttribution, isLoading: true)
        Task(priority: .utility) { [weak self] in
            let result = bridge.wakeupAttributionJSON(
                entityId: entityID,
                durationSeconds: durationSeconds,
                topStacks: topStacks
            )
            await MainActor.run {
                guard let self else { return }
                self.entityWakeupAttributions[entityID] = self.decodeJsonQueryResult(
                    result,
                    as: WakeupAttributionReportModel.self
                )
                self.finishEntityAnalysis(
                    entityID,
                    kind: .wakeupAttribution,
                    result: result,
                    fallback: "Wakeup attribution could not be collected."
                )
            }
        }
    }

    func loadSelfMemoryAttribution(force: Bool = false, topRegions: UInt32 = 8) {
        if !force,
           let updatedAt = selfMemoryAttributionUpdatedAt,
           Date().timeIntervalSince(updatedAt) < 60
        {
            return
        }

        let bridge = self.bridge
        selfMemoryAttributionTask?.cancel()
        selfMemoryAttributionIsLoading = true
        selfMemoryAttributionError = nil
        selfMemoryAttributionTask = Task(priority: .utility) { [weak self] in
            let started = CFAbsoluteTimeGetCurrent()
            let result = bridge.selfMemoryAttributionJSON(topRegions: topRegions)
            await MainActor.run {
                guard let self else { return }
                let durationMillis = (CFAbsoluteTimeGetCurrent() - started) * 1000.0
                self.selfMemoryAttribution = self.decodeJsonQueryResult(
                    result,
                    as: SelfRuntimeMemoryAttributionReportModel.self
                )
                self.selfMemoryAttributionError = self.jsonQueryErrorMessage(
                    result,
                    fallback: "Self memory attribution could not be collected."
                )
                self.selfMemoryAttributionIsLoading = false
                if self.selfMemoryAttributionError == nil {
                    self.selfMemoryAttributionUpdatedAt = Date()
                    self.recordLocalDiagnosticsEvent(
                        level: .info,
                        subsystem: .ui,
                        eventType: "self-memory-attribution-completed",
                        message: "Captured Aetower self memory attribution.",
                        fields: [
                            DiagnosticsField(key: "duration_millis", value: String(format: "%.1f", durationMillis)),
                            DiagnosticsField(key: "top_regions", value: String(topRegions)),
                            DiagnosticsField(key: "resident_bytes", value: String(self.selfMemoryAttribution?.currentResidentBytes ?? 0)),
                            DiagnosticsField(key: "footprint_bytes", value: String(self.selfMemoryAttribution?.currentPhysicalFootprintBytes ?? 0)),
                        ]
                    )
                }
            }
        }
    }

    func recordHistoryDerivedDiagnostics(
        durationMillis: Double,
        snapshotCount: Int,
        recurringEntityCount: Int,
        changeSummaryCount: Int
    ) {
        historyUiDiagnostics = HistoryUiDiagnosticsSummary(
            updatedAt: Date(),
            pageDecodeDurationMillis: historyUiDiagnostics.pageDecodeDurationMillis,
            snapshotCount: snapshotCount,
            entityCount: historyUiDiagnostics.entityCount,
            derivedSummaryBuildDurationMillis: durationMillis,
            recurringEntityCount: recurringEntityCount,
            changeSummaryCount: changeSummaryCount
        )
        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "history-ui-derived-completed",
            message: "Built History tab derived summaries off the main render path.",
            fields: [
                DiagnosticsField(key: "duration_millis", value: String(format: "%.1f", durationMillis)),
                DiagnosticsField(key: "snapshot_count", value: String(snapshotCount)),
                DiagnosticsField(key: "recurring_entity_count", value: String(recurringEntityCount)),
                DiagnosticsField(key: "change_summary_count", value: String(changeSummaryCount)),
            ]
        )
    }

    func recordTimelinePayloadDiagnostics(
        totalEventCount: Int,
        filteredEventCount: Int,
        visibleEventCount: Int,
        filterDurationMillis: Double,
        safeModeEnabled: Bool
    ) {
        timelinePayloadDiagnostics = TimelinePayloadDiagnosticsSummary(
            updatedAt: Date(),
            totalEventCount: totalEventCount,
            filteredEventCount: filteredEventCount,
            visibleEventCount: visibleEventCount,
            filterDurationMillis: filterDurationMillis,
            safeModeEnabled: safeModeEnabled
        )
        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "timeline-ui-filter-completed",
            message: "Computed the Timeline tab payload on a background task.",
            fields: [
                DiagnosticsField(key: "duration_millis", value: String(format: "%.1f", filterDurationMillis)),
                DiagnosticsField(key: "total_event_count", value: String(totalEventCount)),
                DiagnosticsField(key: "filtered_event_count", value: String(filteredEventCount)),
                DiagnosticsField(key: "visible_event_count", value: String(visibleEventCount)),
                DiagnosticsField(key: "safe_mode_enabled", value: safeModeEnabled ? "true" : "false"),
            ]
        )
    }

    func recordMonitorRowBuildDiagnostics(
        rowBuildMillis: Double,
        visibleRowCount: Int
    ) {
        uiPerformanceBudgetDiagnostics = UiPerformanceBudgetDiagnosticsSummary(
            updatedAt: Date(),
            ffiFetchMillis: uiPerformanceBudgetDiagnostics.ffiFetchMillis,
            decodeMillis: uiPerformanceBudgetDiagnostics.decodeMillis,
            rowBuildMillis: rowBuildMillis,
            renderPublishMillis: uiPerformanceBudgetDiagnostics.renderPublishMillis,
            visibleRowCount: visibleRowCount,
            snapshotBytes: uiPerformanceBudgetDiagnostics.snapshotBytes,
            compactPayloadKind: uiPerformanceBudgetDiagnostics.compactPayloadKind
        )
    }

    private func recordUiPerformancePayloadDiagnostics(
        fetchDiagnostics: MonitorUiPayloadFetchDiagnostics,
        decodeMillis: Double,
        renderPublishMillis: Double
    ) {
        let visibleRowCount = uiPerformanceBudgetDiagnostics.visibleRowCount == 0
            ? fetchDiagnostics.returnedRowCount
            : uiPerformanceBudgetDiagnostics.visibleRowCount
        uiPerformanceBudgetDiagnostics = UiPerformanceBudgetDiagnosticsSummary(
            updatedAt: Date(),
            ffiFetchMillis: fetchDiagnostics.ffiFetchMillis,
            decodeMillis: decodeMillis,
            rowBuildMillis: uiPerformanceBudgetDiagnostics.rowBuildMillis,
            renderPublishMillis: renderPublishMillis,
            visibleRowCount: visibleRowCount,
            snapshotBytes: fetchDiagnostics.snapshotBytes,
            compactPayloadKind: fetchDiagnostics.compactPayloadKind
        )
    }

    /// Load the lightweight startup/persistence inventory once. The default tab
    /// path should be fast and cached; explicit rescans/deep audits call
    /// `runPersistenceScan` directly.
    func ensurePersistenceScan() {
        guard persistenceScanReport == nil, !persistenceScanIsLoading else { return }
        runPersistenceScan(deep: false)
    }

    /// On-demand persistence/startup scan (launchd, login items, cron). The
    /// default path is a lightweight metadata inventory; deep mode explicitly
    /// enriches entries with code-signing data.
    func runPersistenceScan(deep: Bool = false) {
        let bridge = self.bridge
        persistenceScanTask?.cancel()
        persistenceScanIsLoading = true
        persistenceScanError = nil
        persistenceScanTask = Task(priority: .utility) { [weak self] in
            let result = deep ? bridge.persistenceDeepScanJSON() : bridge.persistenceScanJSON()
            await MainActor.run {
                guard let self else { return }
                guard !Task.isCancelled else { return }
                self.persistenceScanIsLoading = false
                if let report = self.decodeJsonQueryResult(
                    result, as: PersistenceScanReportModel.self)
                {
                    self.persistenceChangedItemIds = Self.changedPersistenceItemIds(
                        before: self.persistenceScanReport?.items ?? [],
                        after: report.items
                    )
                    self.persistenceScanReport = report
                    self.persistenceScanCompletedAt = Date()
                    self.persistenceScanError = nil
                } else {
                    self.persistenceScanError =
                        result.errorMessage ?? "Persistence scan could not be collected."
                }
            }
        }
    }

    /// Load the developer storage hygiene report once. The backend scan is
    /// bounded and read-only; explicit refreshes call `runStorageHygieneScan`.
    func ensureRepositoryInventoryResponsiveLoad(roots: [String] = []) {
        ensureStorageHygieneScan(roots: roots)
        refreshRepositoryInventoryForVisibleCache(roots: roots)
    }

    func ensureStorageHygieneScan(roots: [String] = []) {
        guard !storageHygieneIsLoading, !storageHygieneIsVerifyingCache else { return }
        if let storageHygieneReport,
           (roots.isEmpty || Self.storageHygieneReportMatchesRequestedRoots(storageHygieneReport, roots: roots))
        {
            return
        }
        storageHygieneTask?.cancel()

        // Read the small JSON cache synchronously first so a hit paints
        // instantly. The old flow flipped storageHygieneIsLoading true here and
        // deferred the cache read onto a .background-priority task, so even a
        // guaranteed hit flashed "Starting storage scan" for the deserialize
        // window on every launch. Loading is now shown only on a real miss.
        var paintedFromCache = false
        var cacheSavedAtMillis: UInt64 = 0
        if storageHygieneReport == nil {
            if case let .hit(cache) = StorageHygieneReportCacheStore.loadIfValid(roots: roots) {
                publishStorageHygieneCacheHit(cache)
                paintedFromCache = true
                cacheSavedAtMillis = cache.savedAtMillis
            }
        }

        storageHygieneIsLoading = storageHygieneReport == nil
        storageHygieneIsVerifyingCache = false
        storageHygieneError = nil
        let bridge = self.bridge
        let publisher = StorageHygieneMainActorPublisher(self)
        storageHygieneTask = Task.detached(priority: paintedFromCache ? .utility : .background) {
            let maxDepth: UInt32 = 5
            let limit: UInt32 = 200
            var publishedReport = paintedFromCache

            // Only re-read the cache in the background if we didn't already
            // paint it synchronously above; either way the index refresh below
            // still runs to keep the displayed data current.
            if !paintedFromCache {
                switch StorageHygieneReportCacheStore.loadIfValid(roots: roots) {
                case let .hit(cache):
                    guard !Task.isCancelled else { return }
                    await publisher.publishCacheHit(cache)
                    publishedReport = true
                case let .stale(reason):
                    guard !Task.isCancelled else { return }
                    await publisher.publishCacheStale(reason: reason)
                case .miss:
                    break
                }
            }

            if !publishedReport {
                guard !Task.isCancelled else { return }
                let overview = bridge.storageHygieneOverviewJSON(
                    roots: roots,
                    maxDepth: maxDepth,
                    mode: "instant_cached"
                )
                let overviewPrepared = StorageHygieneDecodePipeline.prepare(
                    overview,
                    roots: roots,
                    maxDepth: maxDepth,
                    limit: limit,
                    mode: "instant_cached",
                    saveCache: false,
                    saveBaseline: false
                )
                if overviewPrepared.report != nil {
                    await publisher.publishPrepared(overviewPrepared)
                    publishedReport = true
                }
            }

            guard !Task.isCancelled else { return }
            let indexed = bridge.storageHygieneIndexedJSON(
                roots: roots,
                maxDepth: maxDepth,
                limit: limit
            )
            let indexedPrepared = StorageHygieneDecodePipeline.prepare(
                indexed,
                roots: roots,
                maxDepth: maxDepth,
                limit: limit,
                mode: "instant_cached",
                // Persist the authoritative indexed report so the NEXT launch
                // paints instantly from the synchronous cache read. Previously
                // only the manual full-scan button saved the cache, so every
                // launch missed and paid the multi-second index query with a
                // "Starting storage scan" flash.
                saveCache: true,
                saveBaseline: true
            )
            if indexedPrepared.report != nil {
                await publisher.publishPrepared(indexedPrepared)
                publishedReport = true
            } else if !publishedReport {
                await publisher.publishVerificationFinished(message: indexedPrepared.errorMessage)
            }

            guard !Task.isCancelled else { return }
            guard publishedReport else { return }

            // The repository inventory walk is a full live re-walk of every repo
            // (~25-30s). Running it on every launch is the "full scan after every
            // rebuild" cost. When we painted from a fresh cache and the on-disk
            // change journal (persisted across launches) shows nothing changed
            // since that cache was written, the walk would only reproduce what we
            // already show — so skip it. Any real change (lastChange > cacheSaved,
            // or no cache paint) still triggers a full verify.
            //
            // FSEvents only records while the app runs, so a change made while it
            // was closed would be missed by the journal; the age bound below
            // guarantees a periodic re-verify anyway, so staleness is capped at
            // reverifyIntervalMillis rather than "until the next real event".
            let reverifyIntervalMillis: UInt64 = 6 * 60 * 60 * 1000 // 6 hours
            if paintedFromCache {
                let lastChange = StorageRootChangeJournal.lastChangeMillis() ?? 0
                let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
                let cacheAge = nowMillis &- cacheSavedAtMillis
                if lastChange <= cacheSavedAtMillis, cacheAge < reverifyIntervalMillis {
                    return
                }
            }

            await publisher.publishVerificationStarted()

            let inventory = bridge.repositoryInventoryJSON(
                roots: roots,
                maxDepth: maxDepth
            )
            guard !Task.isCancelled else { return }
            if let verifiedInventory = Self.decodeRepositoryInventoryReport(inventory) {
                await publisher.publishInventoryVerification(verifiedInventory)
            } else {
                await publisher.publishVerificationFinished(
                    message: inventory.errorMessage ?? "Repository inventory verification failed."
                )
            }
        }
    }

    private static let repositoryInventoryFingerprintAuditCooldownMillis: UInt64 = 15_000

    private func refreshRepositoryInventoryForVisibleCache(roots: [String]) {
        guard let report = storageHygieneReport else { return }
        guard !storageHygieneIsVerifyingCache, storageScanJob == nil else { return }
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        guard nowMillis >= lastRepositoryInventoryFingerprintAuditMillis
            + Self.repositoryInventoryFingerprintAuditCooldownMillis
        else {
            return
        }
        lastRepositoryInventoryFingerprintAuditMillis = nowMillis

        let bridge = self.bridge
        let publisher = StorageHygieneMainActorPublisher(self)
        let discoveryRoots = roots.isEmpty ? report.roots : roots
        repositoryInventorySignalTask?.cancel()
        repositoryInventorySignalTask = Task.detached(priority: .utility) {
            [report, roots, bridge, publisher, discoveryRoots] in
            let audit = StorageHygieneReportCacheStore.auditRepositoryFingerprints(
                report: report,
                roots: roots
            )
            guard !Task.isCancelled else { return }
            await publisher.publishRepositoryInventoryRefreshState(
                RepositoryInventoryRefreshState(
                    phase: .checkingFingerprints,
                    checkedRepositoryCount: audit.checkedRepositoryCount,
                    changedRepositoryCount: audit.changedRepositoryCount,
                    missingRepositoryCount: audit.missingRepositoryCount,
                    sampleRoots: audit.sampleRoots
                )
            )

            if audit.hasChangedRepositories {
                await publisher.publishRepositoryInventoryRefreshState(
                    RepositoryInventoryRefreshState(
                        phase: .refreshingChangedRepositories,
                        checkedRepositoryCount: audit.checkedRepositoryCount,
                        changedRepositoryCount: audit.changedRepositoryCount,
                        missingRepositoryCount: audit.missingRepositoryCount,
                        sampleRoots: audit.sampleRoots
                    )
                )
                let targetRoots = audit.missingRepositoryRoots.isEmpty
                    ? audit.changedRepositoryRoots
                    : report.roots
                let maxDepth: UInt32 = audit.missingRepositoryRoots.isEmpty ? 1 : 5
                let inventory = bridge.repositoryInventoryJSON(roots: targetRoots, maxDepth: maxDepth)
                guard !Task.isCancelled else { return }
                if let decoded = Self.decodeRepositoryInventoryReport(inventory) {
                    await publisher.publishInventoryVerification(decoded, signalOnly: true)
                }
                if !audit.missingRepositoryRoots.isEmpty {
                    await publisher.publishRepositoryInventoryRefreshState(nil)
                    return
                }
            }

            let knownRoots = Set(report.repositoryInventory.map(\.repoRoot))
            let newRepositoryRoots = Self.discoverUnknownRepositoryRoots(
                roots: discoveryRoots,
                knownRoots: knownRoots,
                maxDepth: 5
            )
            if !newRepositoryRoots.isEmpty {
                await publisher.publishRepositoryInventoryRefreshState(
                    RepositoryInventoryRefreshState(
                        phase: .scanningForNewRepositories,
                        checkedRepositoryCount: audit.checkedRepositoryCount,
                        changedRepositoryCount: newRepositoryRoots.count,
                        missingRepositoryCount: 0,
                        sampleRoots: Array(newRepositoryRoots.prefix(3))
                    )
                )
                let inventory = bridge.repositoryInventoryJSON(roots: newRepositoryRoots, maxDepth: 1)
                guard !Task.isCancelled else { return }
                if let decoded = Self.decodeRepositoryInventoryReport(inventory) {
                    await publisher.publishInventoryVerification(decoded, signalOnly: true)
                }
            }

            await publisher.publishRepositoryInventoryRefreshState(nil)
        }
    }

    nonisolated private static func discoverUnknownRepositoryRoots(
        roots: [String],
        knownRoots: Set<String>,
        maxDepth: Int
    ) -> [String] {
        let fileManager = FileManager.default
        let normalizedKnownRoots = Set(
            knownRoots.flatMap { root in
                [root, normalizedScanRoot(root)]
            }
        )
        let rootURLs = roots
            .map(normalizedScanRoot)
            .filter { !$0.isEmpty }
            .map { URL(fileURLWithPath: $0, isDirectory: true).standardizedFileURL }
        let maxDepth = Swift.max(1, Swift.min(maxDepth, 12))
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        let maxScannedDirectories = 25_000
        var scannedDirectories = 0
        var discoveredRoots: [String] = []
        var discoveredSet: Set<String> = []
        var queue = rootURLs.map { ($0, 0) }
        var cursor = 0

        while cursor < queue.count,
              scannedDirectories < maxScannedDirectories,
              ProcessInfo.processInfo.systemUptime < deadline
        {
            let (url, depth) = queue[cursor]
            cursor += 1
            guard Self.repositoryDiscoveryIsReadableDirectory(url) else { continue }

            let path = url.path
            let isRepository = Self.repositoryDiscoveryIsGitRoot(url)
            if isRepository,
               !normalizedKnownRoots.contains(path),
               discoveredSet.insert(path).inserted
            {
                discoveredRoots.append(path)
            }

            guard depth < maxDepth else { continue }
            if !isRepository, depth > 0, Self.repositoryDiscoverySkipReason(url.lastPathComponent) != nil {
                continue
            }

            scannedDirectories += 1
            guard let children = try? fileManager.contentsOfDirectory(
                at: url,
                includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
                options: []
            ) else {
                continue
            }

            for child in children.sorted(by: { $0.path < $1.path }) {
                if child.lastPathComponent == ".git" { continue }
                guard Self.repositoryDiscoveryIsReadableDirectory(child) else { continue }
                queue.append((child.standardizedFileURL, depth + 1))
            }
        }

        return discoveredRoots
    }

    nonisolated private static func repositoryDiscoveryIsGitRoot(_ url: URL) -> Bool {
        FileManager.default.fileExists(
            atPath: url.appendingPathComponent(".git", isDirectory: false).path
        )
    }

    nonisolated private static func repositoryDiscoveryIsReadableDirectory(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]),
              values.isDirectory == true,
              values.isSymbolicLink != true
        else {
            return false
        }
        return true
    }

    nonisolated private static func repositoryDiscoverySkipReason(_ name: String) -> String? {
        switch name {
        case ".git",
             "node_modules",
             "target",
             ".build",
             "DerivedData",
             ".cache",
             "Library",
             ".docker",
             ".npm",
             ".pnpm-store",
             ".cargo",
             ".gradle",
             ".venv",
             "venv",
             ".tox",
             "__pycache__",
             ".next",
             ".turbo",
             "Pods":
            return "skipped"
        default:
            return nil
        }
    }

    nonisolated private static func decodeRepositoryInventoryReport(
        _ result: JsonQueryResult
    ) -> RepositoryInventoryReportModel? {
        guard let json = result.json, let data = json.data(using: .utf8) else {
            return nil
        }
        let decoder = AetowerJSON.snakeCaseDecoder()
        return try? decoder.decode(RepositoryInventoryReportModel.self, from: data)
    }

    private static func storageHygieneReportMatchesRequestedRoots(
        _ report: StorageHygieneReportModel,
        roots: [String]
    ) -> Bool {
        let requested = normalizedScanRootSet(roots)
        guard !requested.isEmpty else { return true }
        return normalizedScanRootSet(report.repositoryInventoryRoots) == requested
    }

    nonisolated private static func normalizedScanRootSet(_ roots: [String]) -> Set<String> {
        Set(roots.map(normalizedScanRoot).filter { !$0.isEmpty })
    }

    nonisolated private static func normalizedScanRoot(_ root: String) -> String {
        let trimmed = root.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        let expanded: String
        if trimmed == "~" {
            expanded = FileManager.default.homeDirectoryForCurrentUser.path
        } else if trimmed.hasPrefix("~/") {
            expanded = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(trimmed.dropFirst(2)))
                .path
        } else {
            expanded = trimmed
        }
        return URL(fileURLWithPath: expanded, isDirectory: true).standardizedFileURL.path
    }

    /// Observability for stuck hygiene loads: the FFI report calls have no
    /// timeout, so if one wedges (e.g. a pathological index query) the
    /// loading flag would otherwise pin the UI in "Scanning" forever with a
    /// silently disabled Rescan button. After the budget elapses this records
    /// a diagnostics event and flips `storageHygieneLoadExceededBudget` so an
    /// explicit user rescan can supersede the stuck load. It never cancels
    /// the in-flight FFI call.
    private static let storageHygieneLoadBudgetSeconds: TimeInterval = 30

    private func restartStorageHygieneLoadWatchdog() {
        storageHygieneLoadWatchdogTask?.cancel()
        storageHygieneLoadWatchdogTask = nil
        storageHygieneLoadExceededBudget = false
        guard storageHygieneIsLoading else { return }
        storageHygieneLoadWatchdogTask = Task { [weak self] in
            try? await Task.sleep(
                nanoseconds: UInt64(Self.storageHygieneLoadBudgetSeconds * 1_000_000_000)
            )
            guard !Task.isCancelled else { return }
            self?.reportStorageHygieneLoadExceededBudget()
        }
    }

    private func reportStorageHygieneLoadExceededBudget() {
        guard storageHygieneIsLoading, !storageHygieneLoadExceededBudget else { return }
        storageHygieneLoadExceededBudget = true
        recordLocalDiagnosticsEvent(
            level: .warn,
            subsystem: .ui,
            eventType: "storage-hygiene-load-slow",
            message: "Storage hygiene load has been running for over "
                + "\(Int(Self.storageHygieneLoadBudgetSeconds))s; "
                + "the Rescan button was re-enabled so a manual rescan can supersede it.",
            fields: [
                DiagnosticsField(
                    key: "budget_seconds",
                    value: String(Int(Self.storageHygieneLoadBudgetSeconds))
                )
            ]
        )
    }

    /// On-demand read-only storage hygiene scan for build artifacts, logs,
    /// caches, and dependency trees. This never deletes files.
    func runStorageHygieneScan(
        roots: [String] = [],
        maxDepth: UInt32 = 5,
        limit: UInt32 = 200,
        mode: String = "fast_changed_only"
    ) {
        storageHygieneTask?.cancel()
        storageHygieneIsLoading = true
        storageHygieneIsVerifyingCache = false
        storageHygieneError = nil
        let bridge = self.bridge
        let publisher = StorageHygieneMainActorPublisher(self)
        storageHygieneTask = Task.detached(priority: .background) {
            let overview = bridge.storageHygieneOverviewJSON(
                roots: roots,
                maxDepth: maxDepth,
                mode: "instant_cached"
            )
            let overviewPrepared = StorageHygieneDecodePipeline.prepare(
                overview,
                roots: roots,
                maxDepth: maxDepth,
                limit: limit,
                mode: "instant_cached",
                saveCache: false,
                saveBaseline: false
            )
            if overviewPrepared.report != nil {
                guard !Task.isCancelled else { return }
                await publisher.publishPrepared(overviewPrepared)
            }

            let indexed = bridge.storageHygieneIndexedJSON(
                roots: roots,
                maxDepth: maxDepth,
                limit: limit
            )
            let indexedPrepared = StorageHygieneDecodePipeline.prepare(
                indexed,
                roots: roots,
                maxDepth: maxDepth,
                limit: limit,
                mode: "instant_cached",
                saveCache: false,
                saveBaseline: true
            )
            if indexedPrepared.report != nil {
                guard !Task.isCancelled else { return }
                await publisher.publishPrepared(indexedPrepared)
            }

            guard !Task.isCancelled else { return }
            await publisher.startScan(
                roots: roots,
                maxDepth: maxDepth,
                limit: limit,
                mode: mode
            )
        }
    }

    func storagePathWasMovedToTrash(_ path: String) -> Bool {
        storageCleanupMovedPaths.contains(Self.normalizedStorageCleanupPath(path))
    }

    func markStoragePathsMovedToTrash(_ paths: [String], refresh: Bool = true) {
        let normalizedPaths = Set(paths.map(Self.normalizedStorageCleanupPath).filter { !$0.isEmpty })
        guard !normalizedPaths.isEmpty else { return }
        storageCleanupMovedPaths.formUnion(normalizedPaths)
        StorageHygieneReportCacheStore.invalidate()
        repositorySummaryInputsGeneration += 1
        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "storage-cleanup-paths-reconciled",
            message: "Hid \(normalizedPaths.count) moved storage path(s) pending the next scan refresh.",
            fields: [
                DiagnosticsField(key: "moved_path_count", value: String(normalizedPaths.count)),
            ]
        )
        guard refresh, !storageHygieneIsLoading else { return }
        runStorageHygieneScan(roots: storageHygieneReport?.roots ?? [], mode: "fast_changed_only")
    }

    private static func normalizedStorageCleanupPath(_ path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        return URL(fileURLWithPath: trimmed).standardizedFileURL.path
    }

    private func storageScheduledScanSleepNanos() -> UInt64 {
        guard storageScheduledScansEnabled else {
            return 900_000_000_000
        }
        let now = Date()
        let interval = max(3600, storageScheduledScanIntervalSeconds)
        let lastScanDate = storageHygieneCompletedAt
        let secondsUntilDue: TimeInterval
        if let lastScanDate {
            secondsUntilDue = max(0, interval - now.timeIntervalSince(lastScanDate))
        } else {
            secondsUntilDue = 30
        }
        // Wake at least every 15 minutes so setting changes, cancellations, and
        // manual scans are reflected without keeping a hot timer alive.
        let boundedSeconds = min(max(5, secondsUntilDue), 900)
        return UInt64(boundedSeconds * 1_000_000_000)
    }

    private func runScheduledStorageHygieneScanIfDue() {
        guard storageScheduledScansEnabled else { return }
        guard !storageHygieneIsLoading else { return }

        let interval = max(3600, storageScheduledScanIntervalSeconds)
        if let completedAt = storageHygieneCompletedAt,
           Date().timeIntervalSince(completedAt) < interval
        {
            return
        }

        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "storage-scheduled-scan-started",
            message: "Started an opt-in scheduled storage prevention scan.",
            fields: [
                DiagnosticsField(
                    key: "interval_hours",
                    value: String(format: "%.1f", interval / 3600)
                )
            ]
        )
        runStorageHygieneScan(mode: "fast_changed_only")
    }

    func upsertRepositoryProject(_ project: RepositoryProjectModel) {
        repositoryProjects = RepositoryProjectStore.upsert(project, into: repositoryProjects)
        repositorySummaryInputsGeneration += 1
        persistRepositoryProjects()
    }

    func removeRepositoryProject(id: String) {
        repositoryProjects = RepositoryProjectStore.remove(id: id, from: repositoryProjects)
        repositorySummaryInputsGeneration += 1
        persistRepositoryProjects()
    }

    /// Move a repository's reclaimable artifact folders to the Finder Trash
    /// through the shared TrashService — the same reversible path the Storage
    /// tab uses. Only safe/rebuildable tiers are eligible; the caller confirms
    /// first. Closes the loop on the page's headline reclaimable metric, which
    /// previously only ever copied a text brief.
    func trashRepositoryArtifacts(
        repoRoot: String,
        folders: [StorageRepoArtifactFolderModel]
    ) {
        let eligible = folders.filter(Self.repositoryArtifactFolderIsTrashActionable)
        guard !eligible.isEmpty else { return }
        let paths = eligible.map(\.path)
        let bytesByPath = Dictionary(uniqueKeysWithValues: eligible.map { ($0.path, $0.sizeBytes) })
        let activeWriterProbe = cleanupActiveWriterProbe()

        Task.detached(priority: .utility) { [bytesByPath, paths, activeWriterProbe] in
            let outcome = TrashService.trash(paths: paths, activeWriterProbe: activeWriterProbe)
            let reclaimed = outcome.movedPaths.reduce(UInt64(0)) { total, path in
                total.addingReportingOverflow(bytesByPath[path] ?? 0).partialValue
            }
            await MainActor.run {
                self.repositoryCleanupResultByRoot[repoRoot] = RepositoryArtifactCleanupResult(
                    movedCount: outcome.movedPaths.count,
                    failedCount: outcome.failedPaths.count,
                    reclaimedBytes: reclaimed,
                    firstError: outcome.failedPaths.values.first
                )
                self.markStoragePathsMovedToTrash(outcome.movedPaths)
                self.recordLocalDiagnosticsEvent(
                    level: outcome.succeeded ? .info : .warn,
                    subsystem: .ui,
                    eventType: outcome.succeeded ? "repository-artifacts-trashed" : "repository-artifacts-trash-partial",
                    message: "Moved \(outcome.movedPaths.count) repository artifact folder(s) to Trash.",
                    fields: [
                        DiagnosticsField(key: "repo_root", value: repoRoot),
                        DiagnosticsField(key: "moved", value: String(outcome.movedPaths.count)),
                        DiagnosticsField(key: "failed", value: String(outcome.failedPaths.count)),
                        DiagnosticsField(key: "reclaimed_bytes", value: String(reclaimed)),
                    ]
                )
                // Nudge a storage-signal refresh so freed space is reflected.
                self.repositorySummaryInputsGeneration += 1
                self.refreshRepositoryInventorySignalsIfQuiescent()
            }
        }
    }

    private static func repositoryArtifactFolderIsTrashActionable(
        _ folder: StorageRepoArtifactFolderModel
    ) -> Bool {
        ["safe", "rebuildable"].contains(folder.cleanupTier)
            && folder.cleanupAllowed
            && folder.defaultCleanupAction == "trash"
            && folder.cleanupBlockers.isEmpty
            && !folder.sizeTruncated
            && !folder.cloudPlaceholder
            && !folder.hasHardlinks
    }

    func clearRepositoryCleanupResult(repoRoot: String) {
        repositoryCleanupResultByRoot[repoRoot] = nil
    }

    func recordStorageCleanupDiagnostics(
        action: String,
        path: String,
        detail: String,
        bytes: UInt64,
        cleanupTier: String?,
        safety: String?,
        blockerCount: Int,
        succeeded: Bool?,
        auditPersisted: Bool
    ) {
        var fields = [
            DiagnosticsField(key: "action", value: action),
            DiagnosticsField(key: "path", value: path),
            DiagnosticsField(key: "bytes", value: String(bytes)),
            DiagnosticsField(key: "blocker_count", value: String(blockerCount)),
            DiagnosticsField(key: "audit_persisted", value: auditPersisted ? "true" : "false"),
        ]
        if let cleanupTier {
            fields.append(DiagnosticsField(key: "cleanup_tier", value: cleanupTier))
        }
        if let safety {
            fields.append(DiagnosticsField(key: "safety", value: safety))
        }
        if let succeeded {
            fields.append(DiagnosticsField(key: "succeeded", value: succeeded ? "true" : "false"))
        }
        recordLocalDiagnosticsEvent(
            level: succeeded == false || !auditPersisted ? .warn : .info,
            subsystem: .ui,
            eventType: "storage-cleanup-\(action)",
            message: detail,
            fields: fields,
            sensitive: true
        )
    }

    func recordStorageSimilarityActionDiagnostics(
        action: String,
        groupKind: String,
        groupFingerprint: String,
        detectorKind: String,
        actionability: String,
        confidenceBand: String,
        confidenceScore: UInt8,
        itemCount: Int,
        totalBytes: UInt64,
        reclaimableBytes: UInt64,
        extraFields: [(key: String, value: String)] = [],
        warning: Bool = false
    ) {
        var fields = [
            DiagnosticsField(key: "action", value: action),
            DiagnosticsField(key: "group_kind", value: groupKind),
            DiagnosticsField(key: "group_fingerprint", value: groupFingerprint),
            DiagnosticsField(key: "detector_kind", value: detectorKind),
            DiagnosticsField(key: "actionability", value: actionability),
            DiagnosticsField(key: "confidence_band", value: confidenceBand),
            DiagnosticsField(key: "confidence_score", value: String(confidenceScore)),
            DiagnosticsField(key: "item_count", value: String(itemCount)),
            DiagnosticsField(key: "total_bytes", value: String(totalBytes)),
            DiagnosticsField(key: "reclaimable_bytes", value: String(reclaimableBytes)),
        ]
        fields.append(contentsOf: extraFields.map { DiagnosticsField(key: $0.key, value: $0.value) })
        recordLocalDiagnosticsEvent(
            level: warning ? .warn : .info,
            subsystem: .ui,
            eventType: "storage-similarity-\(action)",
            message: "Storage similarity group \(action).",
            fields: fields
        )
    }

    func cleanupActiveWriterProbe() -> TrashService.ActiveWriterProbe {
        let bridge = self.bridge
        return { path in
            let result = bridge.resourceHoldersByFileJSON(path: path)
            if let error = result.errorMessage, !error.isEmpty {
                return .unavailable(error)
            }
            guard let payload = result.json?.data(using: .utf8) else {
                return .unavailable("resource holder query returned no payload")
            }
            do {
                let report = try AetowerJSON.snakeCaseDecoder().decode(
                    ResourceHoldersReportModel.self,
                    from: payload
                )
                return .checked(
                    report.holders.map { holder in
                        TrashService.ActiveWriterHolder(
                            pid: holder.pid,
                            command: holder.command,
                            fd: holder.fd,
                            name: holder.name
                        )
                    }
                )
            } catch {
                return .unavailable("resource holder query decode failed: \(error.localizedDescription)")
            }
        }
    }

    // MARK: - Bulk (fleet-scale) operations
    //
    // At ~2800 repos, per-row buttons don't scale. These fan the existing
    // per-repo async methods across a selection and derive progress from the
    // per-repo loading sets those methods already maintain — no duplicated
    // orchestration, no new task bookkeeping.

    private static func normalizedRoots(_ roots: [String]) -> Set<String> {
        Set(roots.map { RepositoryProjectModel.normalizedRepoRoot($0) }.filter { !$0.isEmpty })
    }

    func bulkRunScorecard(roots: [String]) {
        let normalized = Self.normalizedRoots(roots)
        guard !normalized.isEmpty else { return }
        repositoryBulkLabel = "Running Scorecard"
        repositoryBulkRoots = normalized
        for root in normalized {
            runRepositoryScorecard(repoRoot: root, mode: "auto", refresh: false)
        }
    }

    func bulkRefreshProviders(roots: [String]) {
        let normalized = Self.normalizedRoots(roots)
        guard !normalized.isEmpty else { return }
        repositoryBulkLabel = "Refreshing providers"
        repositoryBulkRoots = normalized
        for root in normalized {
            refreshRepositoryGitHubStatus(repoRoot: root, force: true)
            if let project = repositoryProject(forRepoRoot: root) {
                for link in project.cloudflareLinks {
                    refreshRepositoryCloudflareStatus(repoRoot: root, link: link, force: true)
                }
            }
        }
    }

    /// (completed, total) derived from the relevant per-repo loading set, or nil
    /// when idle. Reaching completed == total means the batch has drained.
    var repositoryBulkProgress: (completed: Int, total: Int)? {
        guard let label = repositoryBulkLabel, !repositoryBulkRoots.isEmpty else { return nil }
        let pending: Int
        if label.contains("Scorecard") {
            pending = repositoryBulkRoots.intersection(repositoryScorecardLoadingRoots).count
        } else {
            pending = repositoryBulkRoots.intersection(repositoryProjectGitHubLoadingRoots).count
        }
        return (repositoryBulkRoots.count - pending, repositoryBulkRoots.count)
    }

    func clearRepositoryBulk() {
        repositoryBulkLabel = nil
        repositoryBulkRoots = []
    }

    /// Persist repository project links, surfacing a diagnostics warning on
    /// failure. The store also backs up a corrupt file on load, so a failed
    /// write no longer silently loses every configured link.
    private func persistRepositoryProjects() {
        guard !RepositoryProjectStore.save(repositoryProjects) else { return }
        recordLocalDiagnosticsEvent(
            level: .warn,
            subsystem: .ui,
            eventType: "repository-projects-save-failed",
            message: "Could not persist repository project links to disk.",
            fields: [DiagnosticsField(key: "project_count", value: String(repositoryProjects.count))]
        )
    }

    func repositoryProject(forRepoRoot repoRoot: String) -> RepositoryProjectModel? {
        RepositoryProjectStore.project(forRepoRoot: repoRoot, in: repositoryProjects)
    }

    /// Re-run a GitHub Actions workflow from the provider tile, then refresh
    /// status. Turns the read-only CI readout into a control surface.
    func rerunRepositoryWorkflow(repoRoot: String, runId: UInt64) {
        let key = RepositoryProjectModel.normalizedRepoRoot(repoRoot)
        guard let project = repositoryProject(forRepoRoot: key),
              let link = project.githubRepositoryLink,
              let owner = link.owner, let repo = link.repo,
              !owner.isEmpty, !repo.isEmpty
        else {
            repositoryProjectGitHubErrorsByRoot[key] = "Link a GitHub repository before re-running a workflow."
            return
        }
        let client = repositoryGitHubProviderClient
        let token = ProviderCredentialStore().resolvedAccessToken(for: .github)
        let publisher = RepositoryGitHubProviderMainActorPublisher(self)
        Task.detached(priority: .userInitiated) { [client, publisher] in
            let error = await client.rerunWorkflow(owner: owner, repo: repo, runId: runId, token: token)
            await publisher.finishWorkflowRerun(repoRoot: key, error: error)
        }
    }

    fileprivate func applyWorkflowRerunResult(repoRoot: String, error: String?) {
        if let error {
            repositoryProjectGitHubErrorsByRoot[repoRoot] = error
            recordLocalDiagnosticsEvent(
                level: .warn,
                subsystem: .ui,
                eventType: "repository-workflow-rerun-failed",
                message: error,
                fields: [DiagnosticsField(key: "repo_root", value: repoRoot)]
            )
            return
        }
        repositoryProjectGitHubErrorsByRoot[repoRoot] = nil
        // Give GitHub a moment to register the queued run, then refresh.
        refreshRepositoryGitHubStatus(repoRoot: repoRoot, force: true)
    }

    func refreshRepositoryGitHubStatus(
        repoRoot: String,
        currentBranch: String? = nil,
        currentHead: String? = nil,
        force: Bool = false
    ) {
        let key = RepositoryProjectModel.normalizedRepoRoot(repoRoot)
        guard !key.isEmpty else { return }
        guard let project = repositoryProject(forRepoRoot: key) else {
            repositoryProjectGitHubErrorsByRoot[key] = "Create or link a project before refreshing GitHub status."
            return
        }
        guard let link = project.githubRepositoryLink,
              let owner = link.owner,
              let repo = link.repo,
              !owner.isEmpty,
              !repo.isEmpty
        else {
            repositoryProjectGitHubErrorsByRoot[key] = "Link a GitHub repository before refreshing status."
            return
        }
        if !force,
           let cached = project.githubStatus,
           cached.isFresh()
        {
            repositoryProjectGitHubErrorsByRoot[key] = nil
            return
        }

        repositoryGitHubProviderTasks[key]?.cancel()
        let runID = beginRepositoryGitHubProviderRun(key: key)
        let client = repositoryGitHubProviderClient
        let publisher = RepositoryGitHubProviderMainActorPublisher(self)
        let token = ProviderCredentialStore().resolvedAccessToken(for: .github)

        repositoryGitHubProviderTasks[key] = Task.detached(priority: .utility) { [client, publisher] in
            let status = await client.fetchStatus(
                RepositoryGitHubStatusRequest(
                    owner: owner,
                    repo: repo,
                    currentBranch: currentBranch,
                    currentHead: currentHead,
                    token: token
                )
            )
            guard !Task.isCancelled else { return }
            await publisher.publishResult(key: key, runID: runID, status: status)
        }
    }

    @discardableResult
    private func beginRepositoryGitHubProviderRun(key: String) -> String {
        let runID = UUID().uuidString
        repositoryGitHubProviderRunIDs[key] = runID
        repositoryProjectGitHubLoadingRoots.insert(key)
        repositoryProjectGitHubErrorsByRoot[key] = nil
        return runID
    }

    func publishRepositoryGitHubProviderStatus(
        key: String,
        runID: String,
        status: RepositoryGitHubProviderStatusModel
    ) {
        guard repositoryGitHubProviderRunIDs[key] == runID else { return }
        repositoryGitHubProviderRunIDs[key] = nil
        repositoryGitHubProviderTasks[key] = nil
        repositoryProjectGitHubLoadingRoots.remove(key)
        repositoryProjectGitHubErrorsByRoot[key] = nil

        guard var project = repositoryProject(forRepoRoot: key) else {
            repositoryProjectGitHubErrorsByRoot[key] = "Project was removed before GitHub status finished."
            return
        }
        project.githubStatus = status
        repositoryProjects = RepositoryProjectStore.upsert(project, into: repositoryProjects)
        repositorySummaryInputsGeneration += 1
        persistRepositoryProjects()
    }

    func repositoryCloudflareProviderKey(
        repoRoot: String,
        link: RepositoryProjectLinkModel
    ) -> String {
        "\(RepositoryProjectModel.normalizedRepoRoot(repoRoot))::\(link.identityKey)"
    }

    /// Retry a failed Cloudflare Pages deployment from the provider tile, then
    /// refresh status. Workers use a different deploy mechanism and are not
    /// retryable this way, so the caller only offers this for pages links.
    func redeployRepositoryCloudflare(
        repoRoot: String,
        link: RepositoryProjectLinkModel,
        deploymentId: String
    ) {
        let key = RepositoryProjectModel.normalizedRepoRoot(repoRoot)
        let loadingKey = repositoryCloudflareProviderKey(repoRoot: key, link: link)
        guard link.kind == .pages,
              let accountID = link.accountId, !accountID.isEmpty,
              let projectName = link.projectName, !projectName.isEmpty
        else {
            repositoryProjectCloudflareErrorsByKey[loadingKey] =
                "Only linked Cloudflare Pages projects can be redeployed here."
            return
        }
        let client = repositoryCloudflareProviderClient
        let token = ProviderCredentialStore().resolvedAccessToken(for: .cloudflare)
        let publisher = RepositoryCloudflareProviderMainActorPublisher(self)
        Task.detached(priority: .userInitiated) { [client, publisher] in
            let error = await client.redeployPages(
                accountID: accountID,
                projectName: projectName,
                deploymentId: deploymentId,
                token: token
            )
            await publisher.finishRedeploy(repoRoot: key, link: link, error: error)
        }
    }

    fileprivate func applyCloudflareRedeployResult(
        repoRoot: String,
        link: RepositoryProjectLinkModel,
        error: String?
    ) {
        let loadingKey = repositoryCloudflareProviderKey(repoRoot: repoRoot, link: link)
        if let error {
            repositoryProjectCloudflareErrorsByKey[loadingKey] = error
            recordLocalDiagnosticsEvent(
                level: .warn,
                subsystem: .ui,
                eventType: "repository-cloudflare-redeploy-failed",
                message: error,
                fields: [DiagnosticsField(key: "repo_root", value: repoRoot)]
            )
            return
        }
        repositoryProjectCloudflareErrorsByKey[loadingKey] = nil
        refreshRepositoryCloudflareStatus(repoRoot: repoRoot, link: link, force: true)
    }

    func refreshRepositoryCloudflareStatus(
        repoRoot: String,
        link: RepositoryProjectLinkModel,
        force: Bool = false
    ) {
        let key = RepositoryProjectModel.normalizedRepoRoot(repoRoot)
        let loadingKey = repositoryCloudflareProviderKey(repoRoot: key, link: link)
        guard !key.isEmpty else { return }
        guard let project = repositoryProject(forRepoRoot: key) else {
            repositoryProjectCloudflareErrorsByKey[loadingKey] =
                "Create or link a project before refreshing Cloudflare status."
            return
        }
        guard project.containsCloudflareLink(link),
              link.provider == .cloudflare
        else {
            repositoryProjectCloudflareErrorsByKey[loadingKey] =
                "Link Cloudflare before refreshing deployment status."
            return
        }
        guard link.accountId?.isEmpty == false else {
            repositoryProjectCloudflareErrorsByKey[loadingKey] =
                "Cloudflare link is missing an account ID."
            return
        }
        if !force,
           let cached = project.cloudflareStatus(for: link),
           cached.isFresh()
        {
            repositoryProjectCloudflareErrorsByKey[loadingKey] = nil
            return
        }

        repositoryCloudflareProviderTasks[loadingKey]?.cancel()
        let runID = beginRepositoryCloudflareProviderRun(loadingKey: loadingKey)
        let client = repositoryCloudflareProviderClient
        let publisher = RepositoryCloudflareProviderMainActorPublisher(self)
        let token = ProviderCredentialStore().resolvedAccessToken(for: .cloudflare)

        repositoryCloudflareProviderTasks[loadingKey] = Task.detached(priority: .utility) {
            [client, publisher] in
            let status = await client.fetchStatus(
                RepositoryCloudflareStatusRequest(
                    link: link,
                    token: token
                )
            )
            guard !Task.isCancelled else { return }
            await publisher.publishResult(
                key: key,
                linkKey: loadingKey,
                runID: runID,
                status: status
            )
        }
    }

    @discardableResult
    private func beginRepositoryCloudflareProviderRun(loadingKey: String) -> String {
        let runID = UUID().uuidString
        repositoryCloudflareProviderRunIDs[loadingKey] = runID
        repositoryProjectCloudflareLoadingKeys.insert(loadingKey)
        repositoryProjectCloudflareErrorsByKey[loadingKey] = nil
        return runID
    }

    func publishRepositoryCloudflareProviderStatus(
        key: String,
        linkKey: String,
        runID: String,
        status: RepositoryCloudflareProviderStatusModel
    ) {
        guard repositoryCloudflareProviderRunIDs[linkKey] == runID else { return }
        repositoryCloudflareProviderRunIDs[linkKey] = nil
        repositoryCloudflareProviderTasks[linkKey] = nil
        repositoryProjectCloudflareLoadingKeys.remove(linkKey)
        repositoryProjectCloudflareErrorsByKey[linkKey] = nil

        guard var project = repositoryProject(forRepoRoot: key) else {
            repositoryProjectCloudflareErrorsByKey[linkKey] =
                "Project was removed before Cloudflare status finished."
            return
        }
        var statuses = project.cloudflareStatuses ?? []
        statuses.removeAll { $0.linkIdentityKey == status.linkIdentityKey }
        statuses.append(status)
        project.cloudflareStatuses = statuses.sorted { left, right in
            left.resourceName.localizedCaseInsensitiveCompare(right.resourceName) == .orderedAscending
        }
        repositoryProjects = RepositoryProjectStore.upsert(project, into: repositoryProjects)
        repositorySummaryInputsGeneration += 1
        persistRepositoryProjects()
    }

    /// On-demand per-repo storage drill-down. instant_cached serves from the
    /// index; the endpoint can fall back to a real projection scan on a cold
    /// index, so this always runs detached with a loading state.
    func loadRepositoryStorageDetail(
        repoRoot: String,
        mode: String = "instant_cached",
        force: Bool = false
    ) {
        let key = repoRoot.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { return }
        if !force, repositoryDetailReportsByRoot[key] != nil || repositoryDetailLoadingRoots.contains(key) {
            return
        }

        let runID = UUID().uuidString
        repositoryDetailTasks[key]?.cancel()
        repositoryDetailRunIDs[key] = runID
        repositoryDetailLoadingRoots.insert(key)
        repositoryDetailErrorsByRoot[key] = nil

        let bridge = self.bridge
        let publisher = RepositoryDetailMainActorPublisher(self)
        repositoryDetailTasks[key] = Task.detached(priority: .utility) { [bridge, publisher] in
            let result = bridge.storageHygieneRepoDetailJSON(repoRoot: key, mode: mode)
            let decoded: StorageRepoDetailModel?
            let errorMessage: String?
            if let json = result.json, result.errorMessage == nil, let data = json.data(using: .utf8) {
                do {
                    decoded = try AetowerJSON.snakeCaseDecoder().decode(StorageRepoDetailModel.self, from: data)
                    errorMessage = nil
                } catch {
                    decoded = nil
                    errorMessage = "Repository detail decode failed: \(error.localizedDescription)"
                }
            } else {
                decoded = nil
                errorMessage = result.errorMessage ?? "Repository detail returned no payload."
            }
            guard !Task.isCancelled else { return }
            await publisher.publish(key: key, runID: runID, detail: decoded, errorMessage: errorMessage)
        }
    }

    func publishRepositoryStorageDetail(
        key: String,
        runID: String,
        detail: StorageRepoDetailModel?,
        errorMessage: String?
    ) {
        // Stale run guard: a newer request for the same root supersedes this one.
        guard repositoryDetailRunIDs[key] == runID else { return }
        repositoryDetailLoadingRoots.remove(key)
        repositoryDetailTasks[key] = nil
        if let detail {
            repositoryDetailReportsByRoot[key] = detail
            repositoryDetailErrorsByRoot[key] = nil
        } else {
            repositoryDetailErrorsByRoot[key] = errorMessage ?? "Repository detail unavailable."
        }
    }

    /// On-demand Storage Explorer page fetch. instant_cached serves from the
    /// SQLite index (sorted + paged server-side), so this stays cheap, but the
    /// endpoint can still fall back to a projection scan on a cold index, so
    /// it always runs detached with a loading state. A newer request cancels
    /// and supersedes any in-flight one.
    func loadStorageItemsPage(
        offset: Int,
        limit: Int = 100,
        sortKey: String,
        sortDescending: Bool,
        force: Bool = false
    ) {
        let clampedOffset = max(0, offset)
        let clampedLimit = max(1, limit)
        let reportStamp = storageHygieneReport?.capturedAtMillis
        if !force,
           storageItemsPageOffset == clampedOffset,
           storageItemsPageSortKey == sortKey,
           storageItemsPageSortDescending == sortDescending,
           storageItemsPageReportCaptureMillis == reportStamp,
           storageItemsPage != nil || storageItemsPageIsLoading
        {
            return
        }

        let runID = UUID().uuidString
        storageItemsPageTask?.cancel()
        storageItemsPageRunID = runID
        storageItemsPageIsLoading = true
        storageItemsPageError = nil
        storageItemsPageOffset = clampedOffset
        storageItemsPageSortKey = sortKey
        storageItemsPageSortDescending = sortDescending
        storageItemsPageReportCaptureMillis = reportStamp

        let roots = storageHygieneReport?.roots ?? []
        let bridge = self.bridge
        let publisher = StorageItemsPageMainActorPublisher(self)
        storageItemsPageTask = Task.detached(priority: .utility) { [bridge, publisher] in
            let result = bridge.storageHygieneItemsPageJSON(
                roots: roots,
                maxDepth: 5,
                offset: UInt32(clampedOffset),
                limit: UInt32(clampedLimit),
                mode: "instant_cached",
                sortKey: sortKey,
                sortDescending: sortDescending
            )
            let decoded: StorageHygieneItemsPageModel?
            let errorMessage: String?
            if let json = result.json, result.errorMessage == nil, let data = json.data(using: .utf8) {
                do {
                    decoded = try AetowerJSON.snakeCaseDecoder().decode(StorageHygieneItemsPageModel.self, from: data)
                    errorMessage = nil
                } catch {
                    decoded = nil
                    errorMessage = "Storage items page decode failed: \(error.localizedDescription)"
                }
            } else {
                decoded = nil
                errorMessage = result.errorMessage ?? "Storage items page returned no payload."
            }
            guard !Task.isCancelled else { return }
            await publisher.publish(runID: runID, page: decoded, errorMessage: errorMessage)
        }
    }

    func publishStorageItemsPage(
        runID: String,
        page: StorageHygieneItemsPageModel?,
        errorMessage: String?
    ) {
        // Stale run guard: a newer page request supersedes this one.
        guard storageItemsPageRunID == runID else { return }
        storageItemsPageIsLoading = false
        storageItemsPageTask = nil
        if let page {
            storageItemsPage = page
            storageItemsPageError = nil
        } else {
            // Keep the previous page (if any) so the table stays usable while
            // the banner explains the failed refresh.
            storageItemsPageError = errorMessage ?? "Storage items page unavailable."
        }
    }

    func runRepositoryScorecard(
        repoRoot: String,
        mode: String = "auto",
        refresh: Bool = false
    ) {
        let key = repoRoot.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { return }

        let requestedMode = Self.normalizedRepositoryScorecardMode(mode)
        let bridge = self.bridge
        let publisher = RepositoryScorecardMainActorPublisher(self)

        let runID = beginRepositoryScorecardRun(key: key)

        repositoryScorecardTasks[key] = Task.detached(priority: .utility) { [bridge, publisher] in
            let result = bridge.scorecardJSON(
                repoRoot: key,
                mode: requestedMode,
                timeoutSeconds: 30,
                refresh: refresh
            )
            guard !Task.isCancelled else { return }
            await publisher.publishResult(
                key: key,
                runID: runID,
                requestedMode: requestedMode,
                result: result
            )
        }
    }

    @discardableResult
    func beginRepositoryScorecardRun(
        key: String,
        runID: String = UUID().uuidString
    ) -> String {
        repositoryScorecardTasks[key]?.cancel()
        repositoryScorecardRunIDs[key] = runID
        repositoryScorecardLoadingRoots.insert(key)
        repositoryScorecardErrorsByRoot[key] = nil
        return runID
    }

    func publishRepositoryScorecardResult(
        key: String,
        runID: String,
        requestedMode: String,
        result: JsonQueryResult
    ) {
        publishRepositoryScorecardJSONResult(
            key: key,
            runID: runID,
            requestedMode: requestedMode,
            json: result.json,
            errorMessage: result.errorMessage
        )
    }

    func publishRepositoryScorecardJSONResult(
        key: String,
        runID: String,
        requestedMode: String,
        json: String?,
        errorMessage: String?
    ) {
        guard repositoryScorecardRunIDs[key] == runID else { return }
        repositoryScorecardLoadingRoots.remove(key)
        repositoryScorecardTasks[key] = nil
        repositoryScorecardRunIDs[key] = nil

        let result = JsonQueryResult(json: json, errorMessage: errorMessage)
        if let report = decodeJsonQueryResult(
            result,
            as: RepositoryScorecardReportModel.self
        ) {
            repositoryScorecardReportsByRoot[key] = report
            repositorySummaryInputsGeneration += 1
            repositoryScorecardErrorsByRoot[key] = nil
            recordLocalDiagnosticsEvent(
                level: report.status == "ok" ? .info : .warn,
                subsystem: .ui,
                eventType: "repository-scorecard-completed",
                message: "Completed repository OpenSSF Scorecard request.",
                fields: [
                    DiagnosticsField(key: "repo_root", value: report.repoRoot),
                    DiagnosticsField(key: "status", value: report.status),
                    DiagnosticsField(key: "mode", value: report.mode),
                    DiagnosticsField(key: "requested_mode", value: report.requestedMode),
                    DiagnosticsField(key: "cache_hit", value: report.cacheHit ? "true" : "false"),
                    DiagnosticsField(
                        key: "failed_check_count",
                        value: String(report.failedChecks.count)
                    ),
                    DiagnosticsField(
                        key: "unavailable_check_count",
                        value: String(report.unavailableChecks.count)
                    ),
                ]
            )
        } else {
            // Distinguish a real scan failure (engine returned an error) from a
            // scan that SUCCEEDED but whose payload could not be decoded — the
            // latter previously showed a misleading "scan failed".
            let message: String
            if let engineError = jsonQueryErrorMessage(
                result,
                fallback: "Repository Scorecard scan could not be collected."
            ) {
                message = engineError
            } else if result.json != nil {
                message = "Scorecard scan completed but its data could not be read (see diagnostics)."
            } else {
                message = "Repository Scorecard scan returned no data."
            }
            repositoryScorecardErrorsByRoot[key] = message
            recordLocalDiagnosticsEvent(
                level: .warn,
                subsystem: .ui,
                eventType: "repository-scorecard-failed",
                message: message,
                fields: [
                    DiagnosticsField(key: "repo_root", value: key),
                    DiagnosticsField(key: "mode", value: requestedMode),
                ]
            )
        }
    }

    func startStorageScanJob(
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32,
        mode: String
    ) {
        storageHygieneIsLoading = true
        storageHygieneIsVerifyingCache = false
        storageHygieneError = nil
        repositoryInventoryRefreshState = nil
        storageScanJob = nil
        storageScanController.start(
            roots: roots,
            maxDepth: maxDepth,
            limit: limit,
            mode: mode,
            throttleHint: storageScanThrottleHint,
            dirtyPaths: StorageRootChangeJournal.dirtyPaths()
        )
    }

    fileprivate func publishStorageHygieneCacheHit(_ cache: StorageHygieneReportCacheHit) {
        guard !Task.isCancelled else { return }
        storageHygieneIsLoading = false
        storageScanJob = nil
        storageHygieneReport = cache.report
        updateStorageEstimateStatus(report: cache.report)
        repositorySummaryInputsGeneration += 1
        storageRootChangeMonitor.startWatching(roots: cache.report.roots)
        storageHygieneCompletedAt =
            Date(timeIntervalSince1970: Double(cache.savedAtMillis) / 1000.0)
        storageHygieneError = nil
        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "storage-hygiene-cache-hit",
            message: "Loaded cached repository/storage hygiene report.",
            fields: [
                DiagnosticsField(key: "repository_count", value: String(cache.repositoryCount)),
                DiagnosticsField(key: "saved_at_millis", value: String(cache.savedAtMillis)),
                DiagnosticsField(key: "captured_at_millis", value: String(cache.report.capturedAtMillis)),
            ]
        )
    }

    fileprivate func publishStorageHygieneCacheStale(reason: String) {
        guard !Task.isCancelled else { return }
        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "storage-hygiene-cache-stale",
            message: "Cached repository/storage hygiene report needs refresh.",
            fields: [DiagnosticsField(key: "reason", value: reason)]
        )
    }

    fileprivate func publishStorageHygieneVerificationStarted() {
        guard !Task.isCancelled else { return }
        storageHygieneIsLoading = false
        storageHygieneIsVerifyingCache = true
        storageHygieneError = nil
        repositoryInventoryRefreshState = RepositoryInventoryRefreshState(
            phase: .scanningForNewRepositories,
            checkedRepositoryCount: storageHygieneReport?.repositoryInventory.count ?? 0,
            changedRepositoryCount: 0,
            missingRepositoryCount: 0,
            sampleRoots: []
        )
        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "storage-hygiene-cache-verification-started",
            message: "Started lightweight repository inventory verification."
        )
    }

    fileprivate func publishRepositoryInventoryRefreshState(_ refreshState: RepositoryInventoryRefreshState?) {
        guard !Task.isCancelled else { return }
        repositoryInventoryRefreshState = refreshState
    }

    fileprivate func publishStorageRepositoryInventoryVerification(
        _ inventory: RepositoryInventoryReportModel,
        signalOnly: Bool = false
    ) {
        guard !Task.isCancelled else { return }
        if !signalOnly {
            storageHygieneIsLoading = false
            storageHygieneIsVerifyingCache = false
            storageHygieneTask = nil
        }
        guard var report = storageHygieneReport else {
            repositoryInventoryRefreshState = nil
            recordLocalDiagnosticsEvent(
                level: .info,
                subsystem: .ui,
                eventType: "storage-hygiene-cache-verification-completed",
                message: "Repository inventory verification completed without a display report.",
                fields: [
                    DiagnosticsField(
                        key: "repository_inventory_count",
                        value: String(inventory.repositoryInventory.count)
                    ),
                ]
            )
            return
        }

        if !signalOnly {
            // Signal-only refreshes must not rotate the growth baseline:
            // previousStorageHygieneReport feeds repo growth deltas and would
            // flap if every git-status refresh replaced it.
            previousStorageHygieneReport = storageHygieneReport
        }
        let displayedRoots = Set(report.repositoryInventory.map(\.repoRoot))
        let verifiedRoots = Set(inventory.repositoryInventory.map(\.repoRoot))
        if signalOnly, verifiedRoots.isEmpty {
            repositoryInventoryRefreshState = nil
            recordLocalDiagnosticsEvent(
                level: .info,
                subsystem: .ui,
                eventType: "storage-hygiene-inventory-signal-empty",
                message: "Skipped empty signal-only repository inventory refresh.",
                fields: [
                    DiagnosticsField(key: "displayed_count", value: String(displayedRoots.count)),
                ]
            )
            return
        }

        repositorySummaryInputsGeneration += 1
        let isPartialSignalRefresh = signalOnly
            && !verifiedRoots.isEmpty
            && !displayedRoots.isSubset(of: verifiedRoots)

        if isPartialSignalRefresh {
            var replacements = Dictionary(
                uniqueKeysWithValues: inventory.repositoryInventory.map { ($0.repoRoot, $0) }
            )
            report.repositoryInventory = report.repositoryInventory.map { repository in
                replacements.removeValue(forKey: repository.repoRoot) ?? repository
            }
            report.repositoryInventory.append(contentsOf: replacements.values)
            report.repositoryInventory.sort {
                $0.repoName.localizedCaseInsensitiveCompare($1.repoName) == .orderedAscending
            }
            storageHygieneReport = report
            storageRootChangeMonitor.startWatching(roots: report.roots)
            storageHygieneError = nil
            repositoryInventoryRefreshState = nil
            recordLocalDiagnosticsEvent(
                level: .info,
                subsystem: .ui,
                eventType: "storage-hygiene-inventory-partial-merged",
                message: "Merged changed repository inventory rows into the cached report.",
                fields: [
                    DiagnosticsField(key: "updated_count", value: String(inventory.repositoryInventory.count)),
                ]
            )
            return
        }

        // An incomplete verification walk (budget-truncated under load) must
        // not replace a richer inventory already on screen.
        let displayedCount = report.repositoryInventory.count
        let verifiedIsRicher = inventory.repositoryInventoryComplete
            || inventory.repositoryInventory.count >= displayedCount
        if verifiedIsRicher {
            report.repositoryInventory = inventory.repositoryInventory
            report.repositoryInventoryComplete = inventory.repositoryInventoryComplete
            report.repositoryInventoryTruncated = inventory.repositoryInventoryTruncated
            report.repositoryInventoryRoots = inventory.repositoryInventoryRoots
            report.repositoryInventoryPartialRoots = inventory.repositoryInventoryPartialRoots
            report.repositoryInventoryCoverage = inventory.repositoryInventoryCoverage
            storageHygieneReport = report
        } else {
            recordLocalDiagnosticsEvent(
                level: .warn,
                subsystem: .ui,
                eventType: "storage-hygiene-verification-skipped-thin",
                message: "Skipped merging truncated inventory verification over a richer displayed inventory.",
                fields: [
                    DiagnosticsField(key: "verified_count", value: String(inventory.repositoryInventory.count)),
                    DiagnosticsField(key: "displayed_count", value: String(displayedCount)),
                ]
            )
        }
        storageRootChangeMonitor.startWatching(roots: report.roots)
        storageHygieneError = nil
        repositoryInventoryRefreshState = nil

        recordLocalDiagnosticsEvent(
            level: inventory.repositoryInventoryComplete ? .info : .warn,
            subsystem: .ui,
            eventType: "storage-hygiene-cache-verification-completed",
            message: "Merged lightweight repository inventory verification into cached report.",
            fields: [
                DiagnosticsField(
                    key: "repository_inventory_count",
                    value: String(inventory.repositoryInventory.count)
                ),
                DiagnosticsField(
                    key: "repository_inventory_complete",
                    value: inventory.repositoryInventoryComplete ? "true" : "false"
                ),
                DiagnosticsField(
                    key: "repository_walk_millis",
                    value: String(inventory.diagnostics.repositoryWalkMillis)
                ),
            ]
        )
    }

    fileprivate func publishStorageHygieneVerificationFinished(message: String?) {
        guard !Task.isCancelled else { return }
        storageHygieneIsLoading = false
        storageHygieneIsVerifyingCache = false
        storageHygieneTask = nil
        repositoryInventoryRefreshState = nil
        if let message, storageHygieneReport != nil {
            recordLocalDiagnosticsEvent(
                level: .warn,
                subsystem: .ui,
                eventType: "storage-hygiene-cache-verification-failed",
                message: message
            )
        } else if let message {
            recordLocalDiagnosticsEvent(
                level: .info,
                subsystem: .ui,
                eventType: "storage-hygiene-cache-miss",
                message: message
            )
        }
    }

    func publishStorageScanJob(_ job: StorageScanJobResponseModel) {
        storageHygieneIsVerifyingCache = false
        storageScanJob = job
        storageScanController.setActiveJobId(job.isActive ? job.jobId : nil)
        switch job.status {
        case "queued", "running", "paused":
            storageHygieneIsLoading = true
            storageHygieneError = nil
            storageEstimateStatus = StorageEstimateStatus(
                confidence: .refreshing,
                title: "Refreshing",
                detail: "\(job.progress.phase) · \(job.progress.currentPathHint ?? "checking changed storage paths")",
                dirtyPathCount: StorageRootChangeJournal.summary().dirtyPathCount,
                lastChangeMillis: StorageRootChangeJournal.lastChangeMillis(),
                lastRefreshMillis: lastStorageEstimateRefreshMillis == 0 ? nil : lastStorageEstimateRefreshMillis
            )
        case "complete":
            storageHygieneIsLoading = true
            storageHygieneError = nil
        case "cancelled":
            storageHygieneIsLoading = false
            storageHygieneError = "Storage scan cancelled."
            updateStorageEstimateStatus()
        case "failed":
            storageHygieneIsLoading = false
            storageHygieneError = job.errorMessage ?? "Storage scan failed."
            updateStorageEstimateStatus()
        default:
            storageHygieneIsLoading = false
            updateStorageEstimateStatus()
        }
    }

    func publishStorageScanFailure(_ message: String) {
        storageHygieneIsLoading = false
        storageHygieneIsVerifyingCache = false
        storageHygieneError = message
        storageScanJob = nil
        storageScanController.setActiveJobId(nil)
        updateStorageEstimateStatus()
        recordLocalDiagnosticsEvent(
            level: .warn,
            subsystem: .ui,
            eventType: "storage-scan-job-failed",
            message: message
        )
    }

    func publishPreparedStorageHygieneResult(
        _ prepared: PreparedStorageHygieneResult
    ) {
        guard !Task.isCancelled else { return }
        storageHygieneIsLoading = false
        storageHygieneIsVerifyingCache = false
        if let report = prepared.report {
            storageScanController.setActiveJobId(nil)
            let storagePublishStartedAt = CFAbsoluteTimeGetCurrent()
            previousStorageHygieneReport = storageHygieneReport
            storageHygieneReport = report
            if report.scanMode != "instant_cached" {
                storageCleanupMovedPaths.removeAll()
            }
            lastStorageEstimateRefreshMillis = report.capturedAtMillis
            updateStorageEstimateStatus(report: report)
            repositorySummaryInputsGeneration += 1
            persistedStorageHygieneBaseline = prepared.baseline
            storageRootChangeMonitor.startWatching(roots: report.roots)
            if report.scanMode != "instant_cached" {
                StorageRootChangeJournal.clearDirtyPaths()
            }
            storageHygieneCompletedAt = Date()
            storageHygieneError = nil
            let storagePublishMillis = UInt64(
                max(0, (CFAbsoluteTimeGetCurrent() - storagePublishStartedAt) * 1000)
            )
            recordLocalDiagnosticsEvent(
                level: report.truncated ? .warn : .info,
                subsystem: .ui,
                eventType: report.truncated
                    ? "storage-hygiene-scan-truncated"
                    : "storage-hygiene-scan-completed",
                message: "Completed read-only developer storage hygiene scan.",
                fields: [
                    DiagnosticsField(
                        key: "duration_millis",
                        value: String(report.scanDurationMillis)
                    ),
                    DiagnosticsField(
                        key: "item_count",
                        value: String(report.summary.itemCount)
                    ),
                    DiagnosticsField(
                        key: "reclaimable_bytes",
                        value: String(report.summary.totalReclaimableBytes)
                    ),
                    DiagnosticsField(
                        key: "repository_inventory_count",
                        value: String(report.repositoryInventory.count)
                    ),
                    DiagnosticsField(
                        key: "payload_bytes",
                        value: String(prepared.payloadBytes)
                    ),
                    DiagnosticsField(
                        key: "storage_budget_status",
                        value: report.diagnostics.performanceBudget?.status ?? "unknown"
                    ),
                    DiagnosticsField(
                        key: "storage_scan_latency_millis",
                        value: String(
                            report.diagnostics.performanceBudget?.scanJobLatencyMillis
                                ?? report.scanDurationMillis
                        )
                    ),
                    DiagnosticsField(
                        key: "storage_payload_budget_bytes",
                        value: String(report.diagnostics.performanceBudget?.payloadBudgetBytes ?? 0)
                    ),
                    DiagnosticsField(
                        key: "storage_table_page_millis",
                        value: String(report.diagnostics.performanceBudget?.tablePageMillis ?? 0)
                    ),
                    DiagnosticsField(
                        key: "storage_render_publish_millis",
                        value: String(storagePublishMillis)
                    ),
                    DiagnosticsField(
                        key: "decode_millis",
                        value: String(prepared.decodeMillis)
                    ),
                    DiagnosticsField(
                        key: "cache_save_millis",
                        value: String(prepared.cacheSaveMillis)
                    ),
                    DiagnosticsField(
                        key: "scan_mode",
                        value: report.scanMode
                    ),
                    DiagnosticsField(
                        key: "root_walk_millis",
                        value: String(report.diagnostics.rootWalkMillis)
                    ),
                    DiagnosticsField(
                        key: "size_walk_millis",
                        value: String(report.diagnostics.sizeWalkMillis)
                    ),
                    DiagnosticsField(
                        key: "git_millis",
                        value: String(report.diagnostics.gitMillis)
                    ),
                    DiagnosticsField(
                        key: "sized_entry_count",
                        value: String(report.diagnostics.sizedEntryCount)
                    ),
                    DiagnosticsField(
                        key: "storage_index_hits",
                        value: String(report.diagnostics.storageIndexHits)
                    ),
                    DiagnosticsField(
                        key: "storage_index_misses",
                        value: String(report.diagnostics.storageIndexMisses)
                    ),
                    DiagnosticsField(
                        key: "lazy_git_status",
                        value: report.diagnostics.lazyGitStatus ? "true" : "false"
                    ),
                    DiagnosticsField(
                        key: "repo_storage_footprint_count",
                        value: String(report.repoFootprints.count)
                    ),
                    DiagnosticsField(
                        key: "persisted_baseline_available",
                        value: persistedStorageHygieneBaseline == nil ? "false" : "true"
                    ),
                    DiagnosticsField(
                        key: "budget_violation_count",
                        value: String(report.budgetGuardrails.violations.count)
                    ),
                    DiagnosticsField(
                        key: "cleanup_bundle_count",
                        value: String(report.cleanupBundles.count)
                    ),
                    DiagnosticsField(
                        key: "cleanup_bundle_bytes",
                        value: String(
                            report.cleanupBundles.reduce(UInt64(0)) {
                                $0 + $1.estimatedReclaimableBytes
                            }
                        )
                    ),
                    DiagnosticsField(
                        key: "agent_hygiene_count",
                        value: String(report.agentHygiene.agentCount)
                    ),
                    DiagnosticsField(
                        key: "agent_hygiene_bytes",
                        value: String(report.agentHygiene.totalAgentArtifactBytes)
                    ),
                    DiagnosticsField(
                        key: "truncated",
                        value: report.truncated ? "true" : "false"
                    ),
                ]
            )
        } else {
            storageScanController.setActiveJobId(nil)
            storageHygieneError =
                prepared.errorMessage ?? "Storage hygiene scan could not be collected."
            recordLocalDiagnosticsEvent(
                level: .warn,
                subsystem: .ui,
                eventType: "storage-hygiene-scan-failed",
                message: storageHygieneError ?? "Storage hygiene scan failed."
            )
        }
    }

    func pauseStorageHygieneScan() {
        storageScanController.pause()
    }

    func resumeStorageHygieneScan() {
        storageScanController.resume()
    }

    func cancelStorageHygieneScan() {
        storageScanController.cancel()
    }

    private var storageScanThrottleHint: String {
        var hints: [String] = []
        if snapshot.host.onBattery || snapshot.host.lowPowerMode {
            hints.append("battery")
        }
        switch snapshot.host.thermalState {
        case .serious, .critical:
            hints.append("thermal-pressure")
        case .fair:
            hints.append("thermal")
        case .nominal:
            break
        }
        return hints.isEmpty ? "normal" : hints.joined(separator: ",")
    }

    private static func changedPersistenceItemIds(
        before oldItems: [PersistenceItemModel],
        after newItems: [PersistenceItemModel]
    ) -> Set<String> {
        var oldById: [String: String] = [:]
        for item in oldItems {
            oldById[item.id] = persistenceItemFingerprint(item)
        }
        return Set(
            newItems.compactMap { item in
                let fingerprint = persistenceItemFingerprint(item)
                return oldById[item.id].map { $0 == fingerprint ? nil : item.id } ?? item.id
            }
        )
    }

    private static func persistenceItemFingerprint(_ item: PersistenceItemModel) -> String {
        var parts: [String] = []
        parts.append(item.kind)
        parts.append(item.path)
        parts.append(item.program ?? "")
        parts.append(item.disabled.map(String.init) ?? "")
        parts.append(item.sourceModifiedAtMillis.map(String.init) ?? "")
        parts.append(item.sourceSizeBytes.map(String.init) ?? "")
        parts.append(item.programExists.map(String.init) ?? "")
        parts.append(item.programModifiedAtMillis.map(String.init) ?? "")
        parts.append(item.programSizeBytes.map(String.init) ?? "")
        parts.append(item.signature?.classification ?? "")
        return parts.joined(separator: "|")
    }

    func runProcessInspection(pid: UInt32) {
        let bridge = self.bridge
        setEntityAnalysisLoading(processAnalysisKey(pid), kind: .processInspect, isLoading: true)
        Task(priority: .utility) { [weak self] in
            let result = bridge.processInspectJSON(pid: pid)
            await MainActor.run {
                guard let self else { return }
                self.processInspections[pid] = self.decodeJsonQueryResult(
                    result,
                    as: ProcessInspectionReportModel.self
                )
                self.finishEntityAnalysis(
                    self.processAnalysisKey(pid),
                    kind: .processInspect,
                    result: result,
                    fallback: "Process inspection could not be collected."
                )
            }
        }
    }

    func runProcessOpenResources(pid: UInt32, limit: UInt32 = 80) {
        let bridge = self.bridge
        setEntityAnalysisLoading(processAnalysisKey(pid), kind: .processResources, isLoading: true)
        Task(priority: .utility) { [weak self] in
            let result = bridge.processOpenResourcesJSON(pid: pid, limit: limit)
            await MainActor.run {
                guard let self else { return }
                self.processOpenResources[pid] = self.decodeJsonQueryResult(
                    result,
                    as: ProcessOpenResourcesReportModel.self
                )
                self.finishEntityAnalysis(
                    self.processAnalysisKey(pid),
                    kind: .processResources,
                    result: result,
                    fallback: "Open files and sockets could not be collected."
                )
            }
        }
    }

    /// Target of a reverse resource-holder lookup.
    enum ResourceHolderQuery {
        case port(UInt32)
        case file(String)
    }

    /// Reverse pivot: list every process holding a given file or port, via lsof.
    /// A successful lookup that finds nothing decodes to an empty holder list
    /// (not an error); only a malformed/failed query sets resourceHoldersError.
    func runResourceHolders(_ query: ResourceHolderQuery) {
        let bridge = self.bridge
        resourceHoldersIsLoading = true
        resourceHoldersError = nil
        Task(priority: .utility) { [weak self] in
            let result: JsonQueryResult
            switch query {
            case let .port(port):
                result = bridge.resourceHoldersByPortJSON(port: port)
            case let .file(path):
                result = bridge.resourceHoldersByFileJSON(path: path)
            }
            await MainActor.run {
                guard let self else { return }
                self.resourceHolders = self.decodeJsonQueryResult(
                    result,
                    as: ResourceHoldersReportModel.self
                )
                self.resourceHoldersIsLoading = false
                if self.resourceHolders == nil {
                    self.resourceHoldersError =
                        result.errorMessage ?? "Could not look up resource holders."
                }
            }
        }
    }

    func runProcessSample(pid: UInt32, durationSeconds: UInt32 = 3, topStacks: UInt32 = 6) {
        let bridge = self.bridge
        setEntityAnalysisLoading(processAnalysisKey(pid), kind: .processSample, isLoading: true)
        Task(priority: .utility) { [weak self] in
            let result = bridge.processSampleJSON(
                pid: pid,
                durationSeconds: durationSeconds,
                topStacks: topStacks
            )
            await MainActor.run {
                guard let self else { return }
                self.processSamples[pid] = self.decodeJsonQueryResult(
                    result,
                    as: ProcessSampleReportModel.self
                )
                self.finishEntityAnalysis(
                    self.processAnalysisKey(pid),
                    kind: .processSample,
                    result: result,
                    fallback: "Process sample could not be collected."
                )
            }
        }
    }

    func runProcessAction(
        pid targetPid: UInt32,
        action: ProcessActionKind,
        reason: String? = nil,
        actionID: String? = nil,
        expectedTargets: [ProcessActionTargetIdentityModel] = [],
        restoreNiceValue: Int? = nil,
        privilegedHelperApproved: Bool = false
    ) {
        processActionController.runProcessAction(
            pid: targetPid,
            action: action,
            reason: reason,
            actionID: actionID,
            expectedTargets: expectedTargets,
            restoreNiceValue: restoreNiceValue,
            privilegedHelperApproved: privilegedHelperApproved
        )
    }

    func runVerifiedProcessAction(
        pid: UInt32,
        action: ProcessActionKind,
        reason: String? = nil,
        actionID: String? = nil,
        privilegedHelperApproved: Bool = false
    ) {
        processActionController.runVerifiedProcessAction(
            pid: pid,
            action: action,
            reason: reason,
            actionID: actionID,
            privilegedHelperApproved: privilegedHelperApproved
        )
    }

    func runProcessActionPreview(
        pid: UInt32,
        action: ProcessActionKind,
        reason: String? = nil,
        actionID: String? = nil
    ) {
        processActionController.runProcessActionPreview(
            pid: pid,
            action: action,
            reason: reason,
            actionID: actionID
        )
    }

    func processActionPreview(
        pid: UInt32,
        action: ProcessActionKind,
        matchingActionID: String? = nil
    ) -> ProcessActionReportModel? {
        processActionController.processActionPreview(
            pid: pid,
            action: action,
            matchingActionID: matchingActionID
        )
    }

    func clearProcessActionPreview(
        pid: UInt32,
        action: ProcessActionKind,
        cancelLoading: Bool = false
    ) {
        processActionController.clearProcessActionPreview(
            pid: pid,
            action: action,
            cancelLoading: cancelLoading
        )
    }

    func refreshProcessActionHistory(windowMinutes: UInt32 = 60, limit: UInt32 = 25) {
        processActionController.refreshProcessActionHistory(
            windowMinutes: windowMinutes,
            limit: limit
        )
    }

    public func loadHistory(force: Bool = false) {
        guard historyVisible || force else {
            return
        }
        let now = Date()
        if !force && now.timeIntervalSince(lastHistoryLoadDate) < historyReloadInterval {
            return
        }
        lastHistoryLoadDate = now

        let liveEndMillis = max(
            snapshot.capturedAtMillis,
            UInt64(Date().timeIntervalSince1970 * 1000)
        )
        let endMillis = historyRangeEndOverrideMillis.map { min($0, liveEndMillis) } ?? liveEndMillis
        let rangeMillis = UInt64(historyWindowSeconds * 1000)
        let startMillis = endMillis >= rangeMillis ? endMillis - rangeMillis : 0
        historyRangeStartMillis = startMillis
        historyRangeEndMillis = endMillis
        let bridge = self.bridge

        historyLoadTask?.cancel()
        historyIsLoading = true
        historyIsLoadingMore = false
        historyLoadStatus = "Preparing local history…"
        historyLoadError = nil
        historyLoadTask = Task(priority: .utility) { [weak self] in
            let loadStarted = CFAbsoluteTimeGetCurrent()
            let summaryResult = bridge.historyRangeSummaryResult(startMillis: startMillis, endMillis: endMillis)
            guard !Task.isCancelled else { return }
            let summary = summaryResult.summary
            let pageResult: HistoryPageLoadResult
            let pageDecodeDurationMillis: Double
            if summaryResult.errorMessage == nil, let summary, summary.rangeCount > 0 {
                let pageLoadStarted = CFAbsoluteTimeGetCurrent()
                pageResult = bridge.loadHistoryPageResult(
                    startMillis: startMillis,
                    endMillis: endMillis,
                    beforeMillisExclusive: nil,
                    limit: self?.historyInitialPageSize ?? 48
                )
                pageDecodeDurationMillis = (CFAbsoluteTimeGetCurrent() - pageLoadStarted) * 1000.0
            } else {
                pageResult = HistoryPageLoadResult(snapshots: [], errorMessage: nil)
                pageDecodeDurationMillis = 0
            }
            let snapshots = pageResult.snapshots.sorted { $0.capturedAtMillis < $1.capturedAtMillis }
            await MainActor.run {
                guard let self else { return }
                let durationMillis = (CFAbsoluteTimeGetCurrent() - loadStarted) * 1000.0
                let entityCount = snapshots.reduce(into: 0) { count, sample in
                    count += sample.entities.count
                }
                self.historyRangeSummary = summary
                self.historyStoreSummary = summary
                self.historySnapshots = snapshots
                self.historyLastLoadDurationMillis = durationMillis
                self.historyUiDiagnostics = HistoryUiDiagnosticsSummary(
                    updatedAt: Date(),
                    pageDecodeDurationMillis: pageDecodeDurationMillis,
                    snapshotCount: snapshots.count,
                    entityCount: entityCount,
                    derivedSummaryBuildDurationMillis: self.historyUiDiagnostics.derivedSummaryBuildDurationMillis,
                    recurringEntityCount: self.historyUiDiagnostics.recurringEntityCount,
                    changeSummaryCount: self.historyUiDiagnostics.changeSummaryCount
                )
                self.historyHasMore = UInt64(snapshots.count) < (summary?.rangeCount ?? 0)
                self.historyLoadError = self.historyLoadErrorMessage(
                    summaryError: summaryResult.errorMessage,
                    pageError: pageResult.errorMessage,
                    rangeSummary: summary,
                    loadedCount: snapshots.count
                )
                self.resetHistoryComparisonIfNeeded()
                self.refreshHistorySnapshotDiff()
                self.historyLoadStatus = self.historyStatusMessage(
                    summary: summary,
                    loadedCount: snapshots.count,
                    durationMillis: durationMillis,
                    maintenance: nil,
                    isLoadMore: false
                )
                self.historyIsLoading = false
                self.recordLocalDiagnosticsEvent(
                    level: .info,
                    subsystem: .persistence,
                    eventType: "history-ui-load-completed",
                    message: "Loaded persisted history into the History view.",
                    fields: [
                        DiagnosticsField(key: "duration_millis", value: String(format: "%.1f", durationMillis)),
                        DiagnosticsField(key: "page_decode_millis", value: String(format: "%.1f", pageDecodeDurationMillis)),
                        DiagnosticsField(key: "loaded_count", value: String(snapshots.count)),
                        DiagnosticsField(key: "entity_count", value: String(entityCount)),
                        DiagnosticsField(key: "range_count", value: String(summary?.rangeCount ?? 0)),
                        DiagnosticsField(key: "store_bytes", value: String(summary?.storeBytes ?? 0)),
                        DiagnosticsField(key: "wal_bytes", value: String(summary?.walBytes ?? 0)),
                    ]
                )
            }
        }
    }

    public func loadMoreHistory() {
        guard historyVisible,
              !historyIsLoading,
              !historyIsLoadingMore,
              historyHasMore,
              let oldestMillis = historySnapshots.first?.capturedAtMillis
        else {
            return
        }
        let startMillis = historyRangeStartMillis
        let endMillis = historyRangeEndMillis
        let bridge = self.bridge
        historyIsLoadingMore = true
        historyLoadStatus = "Loading older persisted samples…"
        historyLoadTask?.cancel()
        historyLoadTask = Task(priority: .utility) { [weak self] in
            let pageLoadStarted = CFAbsoluteTimeGetCurrent()
            let loadStarted = CFAbsoluteTimeGetCurrent()
            let pageResult = bridge.loadHistoryPageResult(
                startMillis: startMillis,
                endMillis: endMillis,
                beforeMillisExclusive: oldestMillis,
                limit: self?.historyLoadMorePageSize ?? 96
            )
            let pageDecodeDurationMillis = (CFAbsoluteTimeGetCurrent() - pageLoadStarted) * 1000.0
            let olderSnapshots = pageResult.snapshots
            guard !Task.isCancelled else { return }
            await MainActor.run {
                guard let self else { return }
                let durationMillis = (CFAbsoluteTimeGetCurrent() - loadStarted) * 1000.0
                if let errorMessage = pageResult.errorMessage {
                    self.historyLoadError = "Failed to load older persisted samples: \(errorMessage)"
                    self.historyLoadStatus = "Older persisted samples could not be loaded."
                    self.historyIsLoadingMore = false
                    return
                }
                let existing = Set(self.historySnapshots.map(\.sequence))
                let uniqueOlder = olderSnapshots.filter { !existing.contains($0.sequence) }
                self.historySnapshots.insert(contentsOf: uniqueOlder, at: 0)
                self.historySnapshots.sort { $0.capturedAtMillis < $1.capturedAtMillis }
                self.historyLastLoadDurationMillis = durationMillis
                let entityCount = self.historySnapshots.reduce(into: 0) { count, sample in
                    count += sample.entities.count
                }
                self.historyUiDiagnostics = HistoryUiDiagnosticsSummary(
                    updatedAt: Date(),
                    pageDecodeDurationMillis: pageDecodeDurationMillis,
                    snapshotCount: self.historySnapshots.count,
                    entityCount: entityCount,
                    derivedSummaryBuildDurationMillis: self.historyUiDiagnostics.derivedSummaryBuildDurationMillis,
                    recurringEntityCount: self.historyUiDiagnostics.recurringEntityCount,
                    changeSummaryCount: self.historyUiDiagnostics.changeSummaryCount
                )
                let rangeCount = self.historyRangeSummary?.rangeCount ?? 0
                let appendedUniqueSamples = !uniqueOlder.isEmpty
                self.historyHasMore = UInt64(self.historySnapshots.count) < rangeCount
                    && appendedUniqueSamples
                self.historyIsLoadingMore = false
                self.historyLoadError = nil
                self.resetHistoryComparisonIfNeeded()
                self.refreshHistorySnapshotDiff()
                self.recordLocalDiagnosticsEvent(
                    level: .info,
                    subsystem: .persistence,
                    eventType: "history-ui-load-more-completed",
                    message: "Appended older persisted history samples into the History view.",
                    fields: [
                        DiagnosticsField(key: "duration_millis", value: String(format: "%.1f", durationMillis)),
                        DiagnosticsField(key: "page_decode_millis", value: String(format: "%.1f", pageDecodeDurationMillis)),
                        DiagnosticsField(key: "loaded_count", value: String(self.historySnapshots.count)),
                        DiagnosticsField(key: "entity_count", value: String(entityCount)),
                        DiagnosticsField(key: "added_count", value: String(uniqueOlder.count)),
                    ]
                )
                if olderSnapshots.isEmpty {
                    self.historyLoadStatus = "Reached the beginning of readable persisted history for this range."
                } else if !appendedUniqueSamples {
                    self.historyLoadStatus = "No additional unique persisted samples were returned for this range."
                } else {
                    self.historyLoadStatus = self.historyStatusMessage(
                        summary: self.historyRangeSummary,
                        loadedCount: self.historySnapshots.count,
                        durationMillis: durationMillis,
                        maintenance: nil,
                        isLoadMore: true
                    )
                }
            }
        }
    }

    public func setHistoryComparison(beforeMillis: UInt64?, afterMillis: UInt64?) {
        historyCompareBeforeMillis = beforeMillis
        historyCompareAfterMillis = afterMillis
        refreshHistorySnapshotDiff()
    }

    private func resetHistoryComparisonIfNeeded() {
        let available = Set(historySnapshots.map(\.capturedAtMillis))
        if historyCompareBeforeMillis == nil || !available.contains(historyCompareBeforeMillis ?? 0) {
            historyCompareBeforeMillis = historySnapshots.first?.capturedAtMillis
        }
        if historyCompareAfterMillis == nil || !available.contains(historyCompareAfterMillis ?? 0) {
            historyCompareAfterMillis = historySnapshots.last?.capturedAtMillis
        }
        if let before = historyCompareBeforeMillis,
           let after = historyCompareAfterMillis,
           before > after
        {
            historyCompareBeforeMillis = after
            historyCompareAfterMillis = before
        }
    }

    private func refreshHistorySnapshotDiff(limit: UInt32 = 12) {
        historyDiffTask?.cancel()
        historyDiffTask = nil
        guard let beforeMillis = historyCompareBeforeMillis,
              let afterMillis = historyCompareAfterMillis,
              beforeMillis != afterMillis
        else {
            historySnapshotDiffIsLoading = false
            historySnapshotDiff = nil
            historySnapshotDiffError = historySnapshots.count >= 2
                ? "Select two different persisted samples to compare."
                : nil
            return
        }

        let normalizedBefore = min(beforeMillis, afterMillis)
        let normalizedAfter = max(beforeMillis, afterMillis)
        let bridge = self.bridge
        historySnapshotDiffIsLoading = true
        historySnapshotDiffError = nil
        historyDiffTask = Task(priority: .utility) { [weak self] in
            let result = bridge.diffSnapshotsJSON(
                beforeMillis: normalizedBefore,
                afterMillis: normalizedAfter,
                entityIds: [],
                limit: limit
            )
            guard !Task.isCancelled else { return }
            await MainActor.run {
                guard let self else { return }
                let currentBefore = self.historyCompareBeforeMillis.map { min($0, self.historyCompareAfterMillis ?? $0) }
                let currentAfter = self.historyCompareAfterMillis.map { max($0, self.historyCompareBeforeMillis ?? $0) }
                guard currentBefore == normalizedBefore, currentAfter == normalizedAfter else {
                    return
                }
                self.historySnapshotDiff = self.decodeJsonQueryResult(result, as: SnapshotDiffReportModel.self)
                self.historySnapshotDiffError = self.jsonQueryErrorMessage(
                    result,
                    fallback: self.historySnapshots.count >= 2 ? "Persisted diff analysis could not be prepared." : nil
                )
                self.historySnapshotDiffIsLoading = false
            }
        }
    }

    public func loadDiagnostics(force: Bool = false, limit: UInt32 = 500) {
        loadDiagnosticsQuery(
            DiagnosticsQuery(
                limit: limit,
                minimumLevel: nil,
                subsystem: nil,
                search: nil,
                sinceMillis: nil,
                includePersisted: false
            ),
            force: force
        )
    }

    public func loadDiagnosticsQuery(_ query: DiagnosticsQuery, force: Bool = false) {
        let now = Date()
        if !force && now.timeIntervalSince(lastDiagnosticsLoadDate) < diagnosticsReloadInterval {
            return
        }
        lastDiagnosticsLoadDate = now
        let bridge = self.bridge
        let boundedQuery = cappedDiagnosticsQuery(query)
        let shouldAnalyzeSessionLogs = force || now.timeIntervalSince(lastSessionLogAnalysisDate) >= sessionLogAnalysisInterval
        if shouldAnalyzeSessionLogs {
            lastSessionLogAnalysisDate = now
        }
        let healthWindowStartMillis = UInt64(
            max(
                0,
                Int64(now.timeIntervalSince1970 * 1000) - Int64(diagnosticsHealthWindowSeconds * 1000)
            )
        )
        let healthQuery = DiagnosticsQuery(
            limit: 500,
            minimumLevel: .warn,
            subsystem: nil,
            search: nil,
            sinceMillis: healthWindowStartMillis,
            includePersisted: false
        )
        diagnosticsLoadTask?.cancel()
        diagnosticsLoadTask = Task(priority: .utility) { [weak self] in
            let events = bridge.queryDiagnostics(boundedQuery)
            let overview = bridge.diagnosticsOverview()
            let runtimeLagMetrics = bridge.latestRuntimeLagMetrics()
            let healthWindowEvents = bridge.queryDiagnostics(healthQuery)
            let sessionLogSummary = shouldAnalyzeSessionLogs ? Result { try SessionLogAnalyzer.analyzeCurrentProcess(lastMinutes: 6) } : nil
            guard !Task.isCancelled else { return }
            await MainActor.run {
                guard let self else { return }
                self.diagnosticsEvents = Array(events.prefix(Int(self.diagnosticsMaxRetainedEvents)))
                self.diagnosticsRecentWarningCount = healthWindowEvents.filter { $0.level == .warn }.count
                self.diagnosticsRecentErrorCount = healthWindowEvents.filter { $0.level == .error }.count
                self.diagnosticsOverview = overview
                self.runtimeLagMetrics = runtimeLagMetrics
                self.lastDiagnosticsQueryDate = Date()
                self.diagnosticsLoadError = nil
                if let sessionLogSummary {
                    self.applySessionLogSummary(sessionLogSummary)
                    switch sessionLogSummary {
                    case let .success(summary):
                        self.sessionLogSummary = summary
                        self.sessionLogAnalysisError = nil
                        self.lastSessionLogAnalysisCompletedDate = Date()
                    case let .failure(error):
                        self.sessionLogAnalysisError = error.localizedDescription
                        self.diagnosticsLoadError = "Unified log analysis failed: \(error.localizedDescription)"
                    }
                }
                self.flushSuppressedAnomalySummaryIfNeeded()
            }
        }
    }

    private func cappedDiagnosticsQuery(_ query: DiagnosticsQuery) -> DiagnosticsQuery {
        DiagnosticsQuery(
            limit: min(query.limit, diagnosticsMaxRetainedEvents),
            minimumLevel: query.minimumLevel,
            subsystem: query.subsystem,
            search: query.search,
            sinceMillis: query.sinceMillis,
            includePersisted: query.includePersisted
        )
    }

    private func updateLagMonitoringState() {
        let shouldMonitorLag = diagnosticsVisible || telemetryEnabled
        guard shouldMonitorLag != lagMonitoringActive else {
            return
        }
        lagMonitoringActive = shouldMonitorLag
        if shouldMonitorLag {
            lagMonitor.start()
        } else {
            lagMonitor.stop()
        }
    }

    private func historyStatusMessage(
        summary: HistoryRangeSummary?,
        loadedCount: Int,
        durationMillis: Double,
        maintenance: HistoryMaintenanceReport?,
        isLoadMore: Bool
    ) -> String {
        let total = summary?.rangeCount ?? 0
        let base = total > 0
            ? "Loaded \(loadedCount) of \(total) persisted samples in \(String(format: "%.0f", durationMillis)) ms."
            : "History checked in \(String(format: "%.0f", durationMillis)) ms."
        if let maintenance, maintenance.prunedRows > 0 {
            if let aggressiveReason = maintenance.aggressiveReason, !aggressiveReason.isEmpty {
                return "\(base) Retention trimmed \(maintenance.prunedRows) rows (\(aggressiveReason))."
            }
            return "\(base) Retention trimmed \(maintenance.prunedRows) rows."
        }
        if maintenance?.vacuumed == true {
            return "\(base) Store was compacted."
        }
        if maintenance?.checkpointed == true {
            return "\(base) Store checkpointed before loading."
        }
        if isLoadMore {
            return "\(base) Older samples appended."
        }
        return base
    }

    private func historyLoadErrorMessage(
        summaryError: String?,
        pageError: String?,
        rangeSummary: HistoryRangeSummary?,
        loadedCount: Int
    ) -> String? {
        if let summaryError, !summaryError.isEmpty {
            return "Failed to summarize persisted history: \(summaryError)"
        }
        if let pageError, !pageError.isEmpty {
            return "Failed to load persisted history: \(pageError)"
        }
        guard loadedCount == 0 else {
            return nil
        }
        if rangeSummary?.rangeCount == 0 {
            return "No persisted history in the selected range yet."
        }
        if rangeSummary != nil {
            return "Persisted history exists in this range, but no readable snapshots were returned."
        }
        return "Persisted history is unavailable for the selected range."
    }

    private func observeWorkspaceActivation() {
        workspaceActivationTask = Task { [weak self] in
            guard let self else { return }
            for await _ in NSWorkspace.shared.notificationCenter.notifications(
                named: NSWorkspace.didActivateApplicationNotification
            ) {
                guard !Task.isCancelled else { break }
                await MainActor.run {
                    self.publishFrontmostState(force: true)
                    self.refresh()
                }
            }
        }
    }

    /// Tracks whether any Aetower window is on screen so the refresh loop only
    /// throttles when nothing is visible. Keyed on window occlusion, NOT app
    /// activation: an unfocused window is still visible and its rings must keep
    /// updating. `NSWindow` posts occlusion changes on `NotificationCenter
    /// .default`; we also re-evaluate on app-activation and miniaturize, which
    /// do not always emit an occlusion change.
    private func observeAppActivation() {
        updateWindowsVisible()
        appResignedActiveTask = Task { [weak self] in
            guard let self else { return }
            for await _ in NotificationCenter.default.notifications(
                named: NSWindow.didChangeOcclusionStateNotification
            ) {
                guard !Task.isCancelled else { break }
                await MainActor.run { self.updateWindowsVisible() }
            }
        }
        appBecameActiveTask = Task { [weak self] in
            guard let self else { return }
            for await _ in NotificationCenter.default.notifications(
                named: NSApplication.didBecomeActiveNotification
            ) {
                guard !Task.isCancelled else { break }
                await MainActor.run { self.updateWindowsVisible(forceRefreshOnAppear: true) }
            }
        }
    }

    /// A window counts as visible when AppKit reports it on screen (occlusion
    /// `.visible`). This stays true for a visible-but-unfocused window and only
    /// drops when every window is minimized, fully covered, hidden, or on
    /// another Space. Erring toward "visible" is deliberate: over-updating a
    /// barely-visible window is cheap; under-updating a visible one is the bug.
    private func updateWindowsVisible(forceRefreshOnAppear: Bool = false) {
        let visible = NSApplication.shared.windows.contains {
            $0.isVisible && $0.occlusionState.contains(.visible)
        }
        let becameVisible = visible && !windowsVisible
        windowsVisible = visible
        if visible && (becameVisible || forceRefreshOnAppear) {
            // Pull immediately so a returning window shows fresh data rather
            // than whatever was last sampled on the slow off-screen cadence.
            publishFrontmostState(force: true)
            refresh(force: true)
        }
    }

    private func publishFrontmostState(force: Bool = false) {
        let now = Date()
        if !force && now.timeIntervalSince(lastFrontmostProbeDate) < frontmostProbeInterval {
            return
        }
        lastFrontmostProbeDate = now

        guard let observation = permissionCoordinator.currentFrontmostAppObservation(includeWindowTitle: false) else {
            if lastPublishedFrontmostSignature != nil {
                bridge.clearFrontmostAppState()
                applyLocalFrontmostState(appName: nil, windowTitle: nil)
                lastPublishedFrontmostBaseSignature = nil
                lastPublishedFrontmostSignature = nil
                lastPublishedFrontmostAppName = nil
                lastPublishedWindowTitle = nil
            }
            return
        }

        let baseSignature = [
            observation.appName,
            observation.bundleId ?? "",
            observation.executablePath ?? ""
        ].joined(separator: "\u{1f}")

        let shouldProbeWindowTitle =
            force
            || baseSignature != lastPublishedFrontmostBaseSignature
            || now.timeIntervalSince(lastWindowTitleProbeDate) >= windowTitleProbeInterval

        // Publish immediately with the last-known title (nil when the app
        // changed) so the base state never waits on AX. The title probe is
        // synchronous IPC that can stall on an unresponsive target, so it
        // runs off the main actor and re-publishes when it lands.
        let staleTitle = baseSignature == lastPublishedFrontmostBaseSignature
            ? lastPublishedWindowTitle
            : nil
        publishFrontmost(
            observation: observation,
            baseSignature: baseSignature,
            windowTitle: staleTitle
        )

        guard shouldProbeWindowTitle else { return }
        lastWindowTitleProbeDate = now
        guard permissionCoordinator.canReadFocusedWindowTitle(bundleId: observation.bundleId)
        else {
            publishFrontmost(observation: observation, baseSignature: baseSignature, windowTitle: nil)
            return
        }
        let pid = observation.processIdentifier
        frontmostTitleProbeTask?.cancel()
        frontmostTitleProbeTask = Task.detached(priority: .utility) { [weak self] in
            let title = PermissionCoordinator.probeFocusedWindowTitle(processIdentifier: pid)
            if Task.isCancelled { return }
            await self?.finishFrontmostTitleProbe(
                observation: observation,
                baseSignature: baseSignature,
                windowTitle: title
            )
        }
    }

    private func finishFrontmostTitleProbe(
        observation: FrontmostAppObservation,
        baseSignature: String,
        windowTitle: String?
    ) {
        // Out-of-order guard: drop the result if the frontmost app moved on
        // while the probe ran off-main.
        guard baseSignature == lastPublishedFrontmostBaseSignature else { return }
        publishFrontmost(
            observation: observation,
            baseSignature: baseSignature,
            windowTitle: windowTitle
        )
    }

    private func publishFrontmost(
        observation: FrontmostAppObservation,
        baseSignature: String,
        windowTitle: String?
    ) {
        let signature = [
            baseSignature,
            windowTitle ?? ""
        ].joined(separator: "\u{1f}")

        guard signature != lastPublishedFrontmostSignature else {
            return
        }

        bridge.updateFrontmostAppState(
            appName: observation.appName,
            bundleId: observation.bundleId,
            executablePath: observation.executablePath,
            windowTitle: windowTitle
        )
        applyLocalFrontmostState(appName: observation.appName, windowTitle: windowTitle)
        lastPublishedFrontmostBaseSignature = baseSignature
        lastPublishedFrontmostSignature = signature
        lastPublishedFrontmostAppName = observation.appName
        lastPublishedWindowTitle = windowTitle
    }

    /// Publish the per-slice projections of a freshly-applied snapshot. Hot
    /// slices change every engine tick, so a deep == there would be an O(n)
    /// walk that almost always fails; rare-change slices are equality-gated,
    /// which is where the invalidation pruning comes from.
    func publishSnapshotSlices(_ snapshot: SystemSnapshot) {
        hostState = snapshot.host
        hostTrendState = snapshot.hostTrend
        entitiesState = snapshot.entities
        snapshotSequence = snapshot.sequence
        snapshotCapturedAtMillis = snapshot.capturedAtMillis
        if timelineState != snapshot.timeline {
            timelineState = snapshot.timeline
        }
        if capabilitiesState != snapshot.capabilities {
            capabilitiesState = snapshot.capabilities
        }
        let agentContext = AgentContextSlice(
            chau7Sessions: snapshot.chau7Sessions,
            aiRepoSummaries: snapshot.aiRepoSummaries,
            resourceCostRollups: snapshot.resourceCostRollups
        )
        if agentContextState != agentContext {
            agentContextState = agentContext
        }
        if thermalForecastState != snapshot.thermalForecast {
            thermalForecastState = snapshot.thermalForecast
        }
    }

    private func applyLocalFrontmostState(appName: String?, windowTitle: String?) {
        // In-place snapshot mutation invalidates every snapshot reader; skip
        // the write when nothing changed.
        guard snapshot.host.frontmostAppName != appName
            || snapshot.host.frontmostWindowTitle != windowTitle
        else { return }
        snapshot.host.frontmostAppName = appName
        snapshot.host.frontmostWindowTitle = windowTitle
        hostState.frontmostAppName = appName
        hostState.frontmostWindowTitle = windowTitle
    }

    private func diffAnomalyStates() {
        var newStates: [String: Bool] = [:]
        for entity in snapshot.entities {
            let notificationKey = anomalyNotificationKey(for: entity)
            newStates[notificationKey] = entity.anomalyDetected
            let wasAnomaly = previousAnomalyStates[notificationKey] ?? false
            if entity.anomalyDetected && !wasAnomaly {
                fireAnomalyNotification(for: entity)
            }
        }
        previousAnomalyStates = newStates
    }

    private func fireAnomalyNotification(for entity: EntitySnapshot) {
        let notificationKey = anomalyNotificationKey(for: entity)
        guard notificationsEnabled else {
            suppressedAnomalyNotificationCount += 1
            suppressedAnomalyEntityKeys.insert(notificationKey)
            flushSuppressedAnomalySummaryIfNeeded()
            return
        }
        guard entity.friction.totalScore >= Float(frictionNotificationThreshold) else {
            return
        }
        guard Bundle.main.bundleIdentifier != nil else { return }
        let now = Date()
        if let lastFire = lastAnomalyNotificationDates[notificationKey],
           now.timeIntervalSince(lastFire) < anomalyNotificationCooldown {
            recordLocalDiagnosticsEvent(
                level: .info,
                subsystem: .ui,
                eventType: "anomaly-notification-suppressed",
                message: "Suppressed a repeated anomaly notification within the cooldown window.",
                entityId: entity.entityId,
                fields: [
                    DiagnosticsField(key: "notification_key", value: notificationKey),
                    DiagnosticsField(key: "cooldown_seconds", value: String(Int(anomalyNotificationCooldown))),
                    DiagnosticsField(key: "friction", value: String(format: "%.1f", entity.friction.totalScore)),
                ]
            )
            return
        }
        let content = UNMutableNotificationContent()
        content.title = "Anomaly Detected"
        content.body = "\(entity.displayName) friction is unusually high (\(String(format: "%.1f", entity.friction.totalScore)))."
        content.sound = .default
        let request = UNNotificationRequest(
            identifier: "anomaly-\(notificationKey)",
            content: content,
            trigger: nil
        )
        lastAnomalyNotificationDates[notificationKey] = now
        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "anomaly-notification-enqueued",
            message: "Enqueued an anomaly notification.",
            entityId: entity.entityId,
            fields: [
                DiagnosticsField(key: "notification_key", value: notificationKey),
                DiagnosticsField(key: "friction", value: String(format: "%.1f", entity.friction.totalScore)),
                DiagnosticsField(key: "threshold", value: String(format: "%.1f", frictionNotificationThreshold)),
            ]
        )
        UNUserNotificationCenter.current().add(request) { [weak self] error in
            guard let self else { return }
            Task { @MainActor in
                if let error {
                    self.recordLocalDiagnosticsEvent(
                        level: .error,
                        subsystem: .ui,
                        eventType: "anomaly-notification-failed",
                        message: "Failed to enqueue an anomaly notification.",
                        entityId: entity.entityId,
                        fields: [
                            DiagnosticsField(key: "notification_key", value: notificationKey),
                            DiagnosticsField(key: "error", value: error.localizedDescription),
                        ]
                    )
                }
            }
        }
    }

    private func requestNotificationPermissionIfNeeded(trigger: String) {
        guard Bundle.main.bundleIdentifier != nil else { return }
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { [weak self] settings in
            guard let self else { return }
            let authorizationStatus = settings.authorizationStatus
            Task { @MainActor in
                switch authorizationStatus {
                case .authorized, .provisional, .ephemeral:
                    notificationAuthorizationStatus = "authorized"
                    recordLocalDiagnosticsEvent(
                        level: .info,
                        subsystem: .ui,
                        eventType: "notification-permission-ready",
                        message: "Notification permission is already available.",
                        fields: [
                            DiagnosticsField(key: "trigger", value: trigger),
                            DiagnosticsField(key: "status", value: authorizationStatusLabel(authorizationStatus)),
                        ]
                    )
                case .denied:
                    notificationAuthorizationStatus = "denied"
                    recordLocalDiagnosticsEvent(
                        level: .warn,
                        subsystem: .ui,
                        eventType: "notification-permission-denied",
                        message: "Notification permission is denied for this app.",
                        fields: [
                            DiagnosticsField(key: "trigger", value: trigger),
                            DiagnosticsField(key: "status", value: authorizationStatusLabel(authorizationStatus)),
                        ]
                    )
                case .notDetermined:
                    Task { @MainActor in
                        do {
                            let granted = try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
                            self.notificationAuthorizationStatus = granted ? "authorized" : "denied"
                            self.recordLocalDiagnosticsEvent(
                                level: granted ? .info : .warn,
                                subsystem: .ui,
                                eventType: "notification-permission-requested",
                                message: granted
                                    ? "Notification permission granted."
                                    : "Notification permission was not granted.",
                                fields: [
                                    DiagnosticsField(key: "trigger", value: trigger),
                                    DiagnosticsField(key: "granted", value: granted ? "true" : "false"),
                                ]
                            )
                        } catch {
                            self.notificationAuthorizationStatus = "error"
                            self.recordLocalDiagnosticsEvent(
                                level: .warn,
                                subsystem: .ui,
                                eventType: "notification-permission-request-failed",
                                message: "Notification permission request failed.",
                                fields: [
                                    DiagnosticsField(key: "trigger", value: trigger),
                                    DiagnosticsField(key: "error", value: error.localizedDescription),
                                ]
                            )
                        }
                    }
                @unknown default:
                    notificationAuthorizationStatus = "unknown"
                    recordLocalDiagnosticsEvent(
                        level: .warn,
                        subsystem: .ui,
                        eventType: "notification-permission-unknown",
                        message: "Notification permission returned an unknown authorization state.",
                        fields: [
                            DiagnosticsField(key: "trigger", value: trigger),
                        ]
                    )
                }
            }
        }
    }

    private func anomalyNotificationKey(for entity: EntitySnapshot) -> String {
        let normalizedName = entity.displayName
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        return normalizedName.isEmpty ? entity.entityId : normalizedName
    }

    private func flushSuppressedAnomalySummaryIfNeeded(force: Bool = false) {
        guard suppressedAnomalyNotificationCount > 0 else {
            return
        }
        let now = Date()
        if !force && now.timeIntervalSince(lastSuppressedAnomalySummaryDate) < suppressedAnomalySummaryInterval {
            return
        }
        if !force && suppressedAnomalyNotificationCount < suppressedAnomalySummaryMinimumCount {
            return
        }
        let entities = suppressedAnomalyEntityKeys.sorted().prefix(5).joined(separator: ", ")
        recordLocalDiagnosticsEvent(
            level: .info,
            subsystem: .ui,
            eventType: "anomaly-notifications-suppressed",
            message: "Suppressed anomaly notifications while notifications are disabled.",
            fields: [
                DiagnosticsField(key: "suppressed_count", value: String(suppressedAnomalyNotificationCount)),
                DiagnosticsField(key: "unique_entity_count", value: String(suppressedAnomalyEntityKeys.count)),
                DiagnosticsField(key: "entities_preview", value: entities),
            ]
        )
        suppressedAnomalyNotificationCount = 0
        suppressedAnomalyEntityKeys.removeAll(keepingCapacity: true)
        lastSuppressedAnomalySummaryDate = now
    }

    private func applySessionLogSummary(_ result: Result<SessionLogSummary, Error>) {
        switch result {
        case let .success(summary):
            guard summary.fingerprint != lastSessionLogFingerprint else {
                return
            }
            lastSessionLogFingerprint = summary.fingerprint
            if summary.notificationLogEntries >= 60 {
                recordLocalDiagnosticsEvent(
                    level: .warn,
                    subsystem: .ui,
                    eventType: "session-log-notification-churn",
                    message: "Session logs show repeated notification scheduling chatter.",
                    fields: [
                        DiagnosticsField(key: "window_minutes", value: String(summary.windowMinutes)),
                        DiagnosticsField(key: "notification_log_entries", value: String(summary.notificationLogEntries)),
                    ]
                )
            }
            if summary.notificationAuthorizationFailures > 0 {
                recordLocalDiagnosticsEvent(
                    level: .warn,
                    subsystem: .ui,
                    eventType: "session-log-notification-permission-failure",
                    message: "Unified logs recorded a notification authorization failure in this session.",
                    fields: [
                        DiagnosticsField(key: "count", value: String(summary.notificationAuthorizationFailures)),
                    ]
                )
            }
            if summary.tccAccessRequests > 4 {
                recordLocalDiagnosticsEvent(
                    level: .info,
                    subsystem: .ui,
                    eventType: "session-log-tcc-churn",
                    message: "Unified logs recorded repeated TCC access requests in this session.",
                    fields: [
                        DiagnosticsField(key: "count", value: String(summary.tccAccessRequests)),
                    ]
                )
            }
            if summary.cursorUiEntries >= 120 {
                recordLocalDiagnosticsEvent(
                    level: .info,
                    subsystem: .ui,
                    eventType: "session-log-cursor-noise",
                    message: "Unified logs recorded heavy TextInputUI cursor noise in this session.",
                    fields: [
                        DiagnosticsField(key: "count", value: String(summary.cursorUiEntries)),
                    ]
                )
            }
            if summary.metalLoadFailures > 0 {
                recordLocalDiagnosticsEvent(
                    level: summary.cursorUiEntries > 0 ? .warn : .error,
                    subsystem: .ui,
                    eventType: "session-log-metal-error",
                    message: summary.cursorUiEntries > 0
                        ? "Unified logs recorded a Metal-side load failure while text-input services were active."
                        : "Unified logs recorded a Metal-side load failure in this session.",
                    fields: [
                        DiagnosticsField(key: "count", value: String(summary.metalLoadFailures)),
                    ]
                )
            }
            if summary.viewBridgeCancellationCount > 0 {
                recordLocalDiagnosticsEvent(
                    level: .info,
                    subsystem: .ui,
                    eventType: "session-log-view-bridge-cancelled",
                    message: "Unified logs recorded cancelled TextInputUI view-bridge connections in this session.",
                    fields: [
                        DiagnosticsField(key: "count", value: String(summary.viewBridgeCancellationCount)),
                    ]
                )
            }
            if summary.invalidDisplayIdentifierCount >= SessionLogSummary.invalidDisplayIdentifierWarningThreshold {
                recordLocalDiagnosticsEvent(
                    level: .warn,
                    subsystem: .ui,
                    eventType: "session-log-invalid-display-identifier",
                    message: "Unified logs recorded repeated SkyLight invalid display identifier errors.",
                    fields: [
                        DiagnosticsField(key: "count", value: String(summary.invalidDisplayIdentifierCount)),
                        DiagnosticsField(key: "window_minutes", value: String(summary.windowMinutes)),
                        DiagnosticsField(key: "log_subsystem", value: "com.apple.SkyLight"),
                    ]
                )
            }
            if summary.nonActiveWindowWarnings > 0 {
                recordLocalDiagnosticsEvent(
                    level: .info,
                    subsystem: .ui,
                    eventType: "session-log-window-noise",
                    message: "Unified logs recorded non-active window ordering noise in this session.",
                    fields: [
                        DiagnosticsField(key: "count", value: String(summary.nonActiveWindowWarnings)),
                    ]
                )
            }
        case let .failure(error):
            recordLocalDiagnosticsEvent(
                level: .warn,
                subsystem: .ui,
                eventType: "session-log-analysis-failed",
                message: "Failed to inspect unified logs for the current app session.",
                fields: [
                    DiagnosticsField(key: "error", value: error.localizedDescription),
                ]
            )
        }
    }

    private func recordLocalDiagnosticsEvent(
        level: DiagnosticsLevel,
        subsystem: DiagnosticsSubsystem,
        eventType: String,
        message: String,
        sequence: UInt64? = nil,
        entityId: String? = nil,
        adapter: String? = nil,
        capability: String? = nil,
        fields: [DiagnosticsField] = [],
        sensitive: Bool = false
    ) {
        let event = DiagnosticsEvent(
            id: "ui-\(UUID().uuidString)",
            timestampMillis: UInt64(Date().timeIntervalSince1970 * 1000),
            level: level,
            subsystem: subsystem,
            eventType: eventType,
            sequence: sequence,
            entityId: entityId,
            adapter: adapter,
            capability: capability,
            message: message,
            fields: fields,
            sensitive: sensitive
        )
        bridge.recordDiagnosticsEvent(event)
        mirrorDiagnosticsToUnifiedLog([event])
    }

    private func mirrorDiagnosticsToUnifiedLog(_ events: [DiagnosticsEvent]) {
        let orderedEvents = events.reversed()
        for event in orderedEvents {
            let signature = diagnosticsMirrorSignature(for: event)
            guard mirroredDiagnosticsSignatures.insert(signature).inserted else {
                continue
            }
            logDiagnosticsEvent(event)
        }
        if mirroredDiagnosticsSignatures.count > 4_000 {
            let retained = Set(events.prefix(2_000).map(diagnosticsMirrorSignature))
            mirroredDiagnosticsSignatures = retained
        }
    }

    private func logDiagnosticsEvent(_ event: DiagnosticsEvent) {
        let logger = Logger(
            subsystem: Bundle.main.bundleIdentifier ?? "com.aetower.app",
            category: "diag.\(diagnosticsSubsystemCategory(event.subsystem))"
        )
        let fieldSummary = event.fields.map { "\($0.key)=\($0.value)" }.joined(separator: " ")
        if event.sensitive {
            switch event.level {
            case .trace, .debug:
                logger.debug("\(event.eventType, privacy: .public) \(event.message, privacy: .public) \(fieldSummary, privacy: .private)")
            case .info:
                logger.info("\(event.eventType, privacy: .public) \(event.message, privacy: .public) \(fieldSummary, privacy: .private)")
            case .warn:
                logger.warning("\(event.eventType, privacy: .public) \(event.message, privacy: .public) \(fieldSummary, privacy: .private)")
            case .error:
                logger.error("\(event.eventType, privacy: .public) \(event.message, privacy: .public) \(fieldSummary, privacy: .private)")
            }
        } else {
            switch event.level {
            case .trace, .debug:
                logger.debug("\(event.eventType, privacy: .public) \(event.message, privacy: .public) \(fieldSummary, privacy: .public)")
            case .info:
                logger.info("\(event.eventType, privacy: .public) \(event.message, privacy: .public) \(fieldSummary, privacy: .public)")
            case .warn:
                logger.warning("\(event.eventType, privacy: .public) \(event.message, privacy: .public) \(fieldSummary, privacy: .public)")
            case .error:
                logger.error("\(event.eventType, privacy: .public) \(event.message, privacy: .public) \(fieldSummary, privacy: .public)")
            }
        }
    }

    private func diagnosticsMirrorSignature(for event: DiagnosticsEvent) -> String {
        [
            String(event.timestampMillis),
            event.eventType,
            event.message,
            event.entityId ?? "",
            event.adapter ?? "",
            event.capability ?? "",
            event.fields.map { "\($0.key)=\($0.value)" }.joined(separator: "|"),
        ].joined(separator: "\u{1f}")
    }

    private func entityAnalysisKey(_ entityID: String, kind: EntityAnalysisKind) -> String {
        "\(entityID)|\(kind.rawValue)"
    }

    private func processAnalysisKey(_ pid: UInt32) -> String {
        "pid:\(pid)"
    }

    private func setEntityAnalysisLoading(_ entityID: String, kind: EntityAnalysisKind, isLoading: Bool) {
        let key = entityAnalysisKey(entityID, kind: kind)
        if isLoading {
            entityAnalysisLoadingKeys.insert(key)
            entityAnalysisErrorMessages[key] = nil
        } else {
            entityAnalysisLoadingKeys.remove(key)
        }
    }

    private func finishEntityAnalysis(
        _ entityID: String,
        kind: EntityAnalysisKind,
        result: JsonQueryResult,
        fallback: String
    ) {
        let key = entityAnalysisKey(entityID, kind: kind)
        entityAnalysisLoadingKeys.remove(key)
        entityAnalysisErrorMessages[key] = jsonQueryErrorMessage(result, fallback: fallback)
        if entityAnalysisErrorMessages[key] == nil {
            entityAnalysisUpdatedAtByKey[key] = Date()
        }
    }

    private static func normalizedRepositoryScorecardMode(_ mode: String) -> String {
        let normalized = mode.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        switch normalized {
        case "public_api", "live_cli", "auto":
            return normalized
        default:
            return "auto"
        }
    }

    private func appStateJSONDecoder() -> JSONDecoder {
        return AetowerJSON.snakeCaseDecoder()
    }

    private func decodeJsonQueryResult<T: Decodable>(_ result: JsonQueryResult, as type: T.Type) -> T? {
        guard let payload = result.json?.data(using: .utf8) else {
            return nil
        }
        do {
            return try appStateJSONDecoder().decode(type, from: payload)
        } catch {
            // A silent `try?` here masked real shape drift as an empty result —
            // and made a *successful* scan look "failed". Surface the exact
            // offending key so a Rust field rename is diagnosable, not invisible.
            recordLocalDiagnosticsEvent(
                level: .warn,
                subsystem: .ui,
                eventType: "report-decode-failed",
                message: "Could not decode \(type): \(Self.decodeErrorSummary(error))",
                fields: [DiagnosticsField(key: "model", value: "\(type)")]
            )
            return nil
        }
    }

    /// Extracts the offending coding path from a `DecodingError` so a decode
    /// failure names the exact field that drifted rather than a generic error.
    static func decodeErrorSummary(_ error: Error) -> String {
        guard let decodingError = error as? DecodingError else {
            return error.localizedDescription
        }
        func path(_ context: DecodingError.Context) -> String {
            context.codingPath.map(\.stringValue).joined(separator: ".")
        }
        switch decodingError {
        case let .keyNotFound(key, context):
            return "missing key '\(key.stringValue)' at \(path(context))"
        case let .typeMismatch(_, context):
            return "type mismatch at \(path(context)): \(context.debugDescription)"
        case let .valueNotFound(_, context):
            return "null value at \(path(context))"
        case let .dataCorrupted(context):
            return "corrupted at \(path(context)): \(context.debugDescription)"
        @unknown default:
            return decodingError.localizedDescription
        }
    }

    private func jsonQueryErrorMessage(_ result: JsonQueryResult, fallback: String?) -> String? {
        if let error = result.errorMessage, !error.isEmpty {
            return error
        }
        guard let json = result.json, !json.isEmpty else {
            return fallback
        }
        return nil
    }

    private var exportPrivacyTier: ExportPrivacyTier {
        SettingsStore.persistedExportPrivacyTier()
    }

    private func diagnosticsSubsystemCategory(_ subsystem: DiagnosticsSubsystem) -> String {
        switch subsystem {
        case .engine:
            return "engine"
        case .collector:
            return "collector"
        case .identity:
            return "identity"
        case .attribution:
            return "attribution"
        case .friction:
            return "friction"
        case .history:
            return "history"
        case .persistence:
            return "persistence"
        case .telemetry:
            return "telemetry"
        case .gpu:
            return "gpu"
        case .ffi:
            return "ffi"
        case .ui:
            return "ui"
        case .adapterChromium:
            return "adapter-chromium"
        case .adapterDocker:
            return "adapter-docker"
        case .adapterHelper:
            return "adapter-helper"
        case .adapterChau7:
            return "adapter-chau7"
        case .adapterVsCode:
            return "adapter-vscode"
        }
    }

    private func authorizationStatusLabel(_ status: UNAuthorizationStatus) -> String {
        switch status {
        case .notDetermined:
            return "not-determined"
        case .denied:
            return "denied"
        case .authorized:
            return "authorized"
        case .provisional:
            return "provisional"
        case .ephemeral:
            return "ephemeral"
        @unknown default:
            return "unknown"
        }
    }

    private func publishUiLagMetrics(
        snapshot: SystemSnapshot,
        bridgeFetchMillis: Double,
        uiRefreshMillis: Double,
        refreshStartedAt: CFAbsoluteTime
    ) {
        let sampledAtMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        let snapshotToUiMillis = sampledAtMillis >= snapshot.capturedAtMillis
            ? Double(sampledAtMillis - snapshot.capturedAtMillis)
            : 0
        let sample = lagMonitor.sample()
        bridge.updateUiLagMetrics(
            UiLagMetrics(
                updatedAtMillis: sampledAtMillis,
                bridgeFetchMillis: Float(bridgeFetchMillis),
                uiRefreshMillis: Float(uiRefreshMillis),
                snapshotToUiMillis: Float(snapshotToUiMillis),
                snapshotToRenderMillis: 0,
                renderCommitMillis: 0,
                displayFrameIntervalMillis: Float(sample.displayFrameIntervalMillis),
                displayRefreshHz: Float(sample.displayRefreshHz),
                displayDroppedFrames: sample.displayDroppedFrames,
                inputAvgLatencyMillis: Float(sample.inputAvgLatencyMillis),
                inputMaxLatencyMillis: Float(sample.inputMaxLatencyMillis),
                inputSampleCount: sample.inputSampleCount
            )
        )

        Task { @MainActor [weak self] in
            guard let self, !Task.isCancelled else { return }
            let renderSampledAtMillis = UInt64(Date().timeIntervalSince1970 * 1000)
            let renderCommitMillis = (CFAbsoluteTimeGetCurrent() - refreshStartedAt) * 1000.0
            let snapshotToRenderMillis = renderSampledAtMillis >= snapshot.capturedAtMillis
                ? Double(renderSampledAtMillis - snapshot.capturedAtMillis)
                : 0
            let renderSample = self.lagMonitor.sample()
            self.bridge.updateUiLagMetrics(
                UiLagMetrics(
                    updatedAtMillis: renderSampledAtMillis,
                    bridgeFetchMillis: Float(bridgeFetchMillis),
                    uiRefreshMillis: Float(uiRefreshMillis),
                    snapshotToUiMillis: Float(snapshotToUiMillis),
                    snapshotToRenderMillis: Float(snapshotToRenderMillis),
                    renderCommitMillis: Float(renderCommitMillis),
                    displayFrameIntervalMillis: Float(renderSample.displayFrameIntervalMillis),
                    displayRefreshHz: Float(renderSample.displayRefreshHz),
                    displayDroppedFrames: renderSample.displayDroppedFrames,
                    inputAvgLatencyMillis: Float(renderSample.inputAvgLatencyMillis),
                    inputMaxLatencyMillis: Float(renderSample.inputMaxLatencyMillis),
                    inputSampleCount: renderSample.inputSampleCount
                )
            )
        }
    }
}
