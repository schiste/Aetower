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

enum StorageHygieneDecodePipeline {
    static func prepare(
        _ result: JsonQueryResult,
        roots: [String],
        maxDepth: UInt32,
        limit: UInt32,
        mode: String,
        saveCache: Bool
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
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
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
        StorageHygieneBaselineStore.save(baseline)
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

@MainActor
@Observable
public final class AppState {
    public private(set) var snapshot: SystemSnapshot
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
    private(set) var storageScanJob: StorageScanJobResponseModel?
    private(set) var storageHygieneIsLoading = false
    private(set) var storageHygieneError: String?
    private(set) var storageHygieneCompletedAt: Date?
    private(set) var repositoryScorecardReportsByRoot: [String: RepositoryScorecardReportModel] = [:]
    private(set) var repositoryScorecardLoadingRoots: Set<String> = []
    private(set) var repositoryScorecardErrorsByRoot: [String: String] = [:]
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
    /// Whether Aetower is the active application. When it is not, its windows
    /// are not on screen, so the refresh loop stretches its cadence (see
    /// `currentRefreshIntervalNanos`). Internal control state, not UI-bound.
    @ObservationIgnored
    private var appIsActive = true
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
    private var repositoryScorecardTasks: [String: Task<Void, Never>] = [:]
    @ObservationIgnored
    private var repositoryScorecardRunIDs: [String: String] = [:]
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
    private let historyMaxRetainedSamples = 240
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
        self.lastObservedSequence = initialSnapshot.sequence
        self.storageScanController.attach(self)
    }

    private static func emptySnapshot() -> SystemSnapshot {
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
            thermalForecast: nil
        )
    }

    public func start() {
        start(refreshInterval: 1.0)
    }

    public var localMcpSocketPathDisplay: String {
        localMcpController.socketPathDisplay
    }

    public func startLocalMcpServer(autoRegisterClients: Bool = false) {
        localMcpController.start(autoRegisterClients: autoRegisterClients)
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

    /// Sleep interval for the refresh loop: the foreground cadence when Aetower
    /// is active, stretched by `backgroundRefreshMultiplier` when it is not.
    /// Regaining focus forces an immediate refresh (see `observeAppActivation`),
    /// so the longer background sleep never delays what the user sees on focus.
    private func currentRefreshIntervalNanos() -> UInt64 {
        appIsActive
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
        repositoryScorecardTasks.values.forEach { $0.cancel() }
        repositoryScorecardTasks.removeAll()
        repositoryScorecardRunIDs.removeAll()
        repositoryScorecardLoadingRoots.removeAll()
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
        let events = snapshot.timeline
        guard automationSeeded else {
            seenAutomationEventIds = Set(events.map(\.id))
            automationSeeded = true
            return
        }
        let activeRules = automationRules.filter(\.enabled)
        if !activeRules.isEmpty {
            for event in events where !seenAutomationEventIds.contains(event.id) {
                for rule in activeRules where automationRuleMatches(rule, event, in: snapshot) {
                    executeAutomationAction(rule, event: event)
                }
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
        let events = snapshot.timeline
        // Seed-guard: ignore the backlog present on the first snapshot so we
        // don't alert for events that predate this session.
        guard notificationsSeeded else {
            seenNotificationEventIds = Set(events.map(\.id))
            notificationsSeeded = true
            return
        }
        if notificationsEnabled {
            for event in events where !seenNotificationEventIds.contains(event.id) {
                considerTimelineNotification(event, in: snapshot)
            }
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
        let worker = snapshotRefreshWorker
        refreshFetchTask = Task(priority: .utility) { [weak self] in
            do {
                let result = try await worker.refresh(
                    force: force,
                    includeOperatorState: includeOperatorState
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
        localMcpController.ensureServer(force: force)
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
            snapshot = payload.snapshot
            lastObservedSequence = payload.snapshot.sequence
            applyLocalFrontmostState(
                appName: lastPublishedFrontmostAppName,
                windowTitle: lastPublishedWindowTitle
            )
            runtimeLagMetrics = payload.runtimeLagMetrics
            if let operatorState = payload.operatorState {
                diagnosticsOverview = operatorState.diagnosticsOverview
                historyStoreSummary = operatorState.historyStoreSummary
                lastOperatorStateRefreshDate = Date()
                localMcpController.refreshHealthSnapshot()
            }

            evaluateAutomationRules(snapshot: payload.snapshot)
            evaluateAgentBudgetRules(snapshot: payload.snapshot)
            evaluateTimelineNotifications(snapshot: payload.snapshot)

            if lagMonitoringActive {
                publishUiLagMetrics(
                    snapshot: payload.snapshot,
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
            diffAnomalyStates()
            flushSuppressedAnomalySummaryIfNeeded()
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

    private func completeSnapshotRefresh() {
        refreshFetchTask = nil
        refreshInFlight = false
        if pendingForcedRefresh {
            pendingForcedRefresh = false
            refresh(force: true)
        }
    }

    public func setHistoryWindow(seconds: TimeInterval) {
        historyWindowSeconds = max(seconds, 300)
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
    func ensureStorageHygieneScan() {
        guard storageHygieneReport == nil, !storageHygieneIsLoading else { return }
        storageHygieneTask?.cancel()
        storageHygieneIsLoading = true
        storageHygieneError = nil
        let bridge = self.bridge
        let publisher = StorageHygieneMainActorPublisher(self)
        storageHygieneTask = Task.detached(priority: .background) {
            let roots: [String] = []
            let maxDepth: UInt32 = 5
            let limit: UInt32 = 80
            let cacheResult = StorageHygieneReportCacheStore.loadIfValid(
                roots: roots,
                maxDepth: maxDepth,
                limit: limit
            )

            switch cacheResult {
            case let .hit(cache):
                guard !Task.isCancelled else { return }
                await publisher.publishCacheHit(cache)
                return
            case let .stale(reason):
                guard !Task.isCancelled else { return }
                await publisher.publishCacheStale(reason: reason)
            case .miss:
                break
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
                saveCache: false
            )
            if indexedPrepared.report != nil {
                await publisher.publishPrepared(indexedPrepared)
            }

            guard !Task.isCancelled else { return }
            await publisher.startScan(
                roots: roots,
                maxDepth: maxDepth,
                limit: limit,
                mode: "fast_changed_only"
            )
        }
    }

    /// On-demand read-only storage hygiene scan for build artifacts, logs,
    /// caches, and dependency trees. This never deletes files.
    func runStorageHygieneScan(
        roots: [String] = [],
        maxDepth: UInt32 = 5,
        limit: UInt32 = 80,
        mode: String = "fast_changed_only"
    ) {
        storageHygieneTask?.cancel()
        startStorageScanJob(
            roots: roots,
            maxDepth: maxDepth,
            limit: limit,
            mode: mode
        )
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
            let message = jsonQueryErrorMessage(
                result,
                fallback: "Repository Scorecard scan could not be collected."
            ) ?? "Repository Scorecard scan failed."
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
        storageHygieneError = nil
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
        storageHygieneReport = cache.report
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

    func publishStorageScanJob(_ job: StorageScanJobResponseModel) {
        storageScanJob = job
        storageScanController.setActiveJobId(job.isActive ? job.jobId : nil)
        switch job.status {
        case "queued", "running", "paused":
            storageHygieneIsLoading = true
            storageHygieneError = nil
        case "complete":
            storageHygieneIsLoading = true
            storageHygieneError = nil
        case "cancelled":
            storageHygieneIsLoading = false
            storageHygieneError = "Storage scan cancelled."
        case "failed":
            storageHygieneIsLoading = false
            storageHygieneError = job.errorMessage ?? "Storage scan failed."
        default:
            storageHygieneIsLoading = false
        }
    }

    func publishStorageScanFailure(_ message: String) {
        storageHygieneIsLoading = false
        storageHygieneError = message
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
        if let report = prepared.report {
            storageScanController.setActiveJobId(nil)
            previousStorageHygieneReport = storageHygieneReport
            storageHygieneReport = report
            persistedStorageHygieneBaseline = prepared.baseline
            storageRootChangeMonitor.startWatching(roots: report.roots)
            if report.scanMode != "instant_cached" {
                StorageRootChangeJournal.clearDirtyPaths()
            }
            storageHygieneCompletedAt = Date()
            storageHygieneError = nil
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
        pid: UInt32,
        action: ProcessActionKind,
        reason: String? = nil,
        actionID: String? = nil,
        expectedTargets: [ProcessActionTargetIdentityModel] = [],
        restoreNiceValue: Int? = nil,
        privilegedHelperApproved: Bool = false
    ) {
        processActionController.runProcessAction(
            pid: pid,
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

        let endMillis = max(
            snapshot.capturedAtMillis,
            UInt64(Date().timeIntervalSince1970 * 1000)
        )
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
                let cappedSnapshots = Array(snapshots.suffix(self.historyMaxRetainedSamples))
                let entityCount = cappedSnapshots.reduce(into: 0) { count, sample in
                    count += sample.entities.count
                }
                self.historyRangeSummary = summary
                self.historyStoreSummary = summary
                self.historySnapshots = cappedSnapshots
                self.historyLastLoadDurationMillis = durationMillis
                self.historyUiDiagnostics = HistoryUiDiagnosticsSummary(
                    updatedAt: Date(),
                    pageDecodeDurationMillis: pageDecodeDurationMillis,
                    snapshotCount: cappedSnapshots.count,
                    entityCount: entityCount,
                    derivedSummaryBuildDurationMillis: self.historyUiDiagnostics.derivedSummaryBuildDurationMillis,
                    recurringEntityCount: self.historyUiDiagnostics.recurringEntityCount,
                    changeSummaryCount: self.historyUiDiagnostics.changeSummaryCount
                )
                self.historyHasMore = UInt64(cappedSnapshots.count) < (summary?.rangeCount ?? 0)
                    && cappedSnapshots.count < self.historyMaxRetainedSamples
                self.historyLoadError = self.historyLoadErrorMessage(
                    summaryError: summaryResult.errorMessage,
                    pageError: pageResult.errorMessage,
                    rangeSummary: summary,
                    loadedCount: cappedSnapshots.count
                )
                self.resetHistoryComparisonIfNeeded()
                self.refreshHistorySnapshotDiff()
                self.historyLoadStatus = self.historyStatusMessage(
                    summary: summary,
                    loadedCount: cappedSnapshots.count,
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
                        DiagnosticsField(key: "loaded_count", value: String(cappedSnapshots.count)),
                        DiagnosticsField(key: "entity_count", value: String(entityCount)),
                        DiagnosticsField(key: "range_count", value: String(summary?.rangeCount ?? 0)),
                        DiagnosticsField(key: "store_bytes", value: String(summary?.storeBytes ?? 0)),
                        DiagnosticsField(key: "wal_bytes", value: String(summary?.walBytes ?? 0)),
                        DiagnosticsField(key: "retained_cap", value: String(self.historyMaxRetainedSamples)),
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
        let remainingCapacity = max(0, historyMaxRetainedSamples - historySnapshots.count)
        guard remainingCapacity > 0 else {
            historyHasMore = false
            historyLoadStatus = "Loaded history is capped at \(historyMaxRetainedSamples) retained samples to protect UI memory."
            return
        }
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
                limit: min(self?.historyLoadMorePageSize ?? 96, UInt32(remainingCapacity))
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
                    && self.historySnapshots.count < self.historyMaxRetainedSamples
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
                if self.historySnapshots.count >= self.historyMaxRetainedSamples {
                    self.historyLoadStatus = "Loaded history is capped at \(self.historyMaxRetainedSamples) retained samples to protect UI memory."
                } else if olderSnapshots.isEmpty {
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

    /// Tracks whether Aetower itself is the active application so the refresh
    /// loop can throttle while our windows are occluded. `NSApplication` posts
    /// these on `NotificationCenter.default`; we seed `appIsActive` from the
    /// current state in case the app launches in the background.
    private func observeAppActivation() {
        appIsActive = NSApplication.shared.isActive
        appResignedActiveTask = Task { [weak self] in
            guard let self else { return }
            for await _ in NotificationCenter.default.notifications(
                named: NSApplication.didResignActiveNotification
            ) {
                guard !Task.isCancelled else { break }
                await MainActor.run { self.appIsActive = false }
            }
        }
        appBecameActiveTask = Task { [weak self] in
            guard let self else { return }
            for await _ in NotificationCenter.default.notifications(
                named: NSApplication.didBecomeActiveNotification
            ) {
                guard !Task.isCancelled else { break }
                await MainActor.run {
                    self.appIsActive = true
                    // Pull immediately so focus shows fresh data rather than
                    // whatever was last sampled on the slow background cadence.
                    self.publishFrontmostState(force: true)
                    self.refresh(force: true)
                }
            }
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
        let windowTitle: String?
        if shouldProbeWindowTitle,
           permissionCoordinator.canReadFocusedWindowTitle(bundleId: observation.bundleId)
        {
            windowTitle = permissionCoordinator.currentFocusedWindowTitle(
                processIdentifier: observation.processIdentifier,
                bundleId: observation.bundleId
            )
            lastWindowTitleProbeDate = now
        } else if shouldProbeWindowTitle {
            windowTitle = nil
            lastWindowTitleProbeDate = now
        } else {
            windowTitle = lastPublishedWindowTitle
        }

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

    private func applyLocalFrontmostState(appName: String?, windowTitle: String?) {
        snapshot.host.frontmostAppName = appName
        snapshot.host.frontmostWindowTitle = windowTitle
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

    private func jsonDecoder() -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }

    private func decodeJsonQueryResult<T: Decodable>(_ result: JsonQueryResult, as type: T.Type) -> T? {
        guard let json = result.json, let data = json.data(using: .utf8) else {
            return nil
        }
        return try? jsonDecoder().decode(T.self, from: data)
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
