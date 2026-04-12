@_exported import AetowerBindings
import Foundation

public final class EngineBridge: @unchecked Sendable {
    private let engine: MonitorEngine

    public init() {
        self.engine = MonitorEngine()
    }

    public func latestSnapshot() throws -> SystemSnapshot {
        engine.latestSnapshot()
    }

    public func latestSnapshotIfNewer(since sequence: UInt64) throws -> SystemSnapshot? {
        engine.latestSnapshotIfNewer(lastSequence: sequence)
    }

    public func latestSequence() throws -> UInt64 {
        engine.latestSequence()
    }

    public func latestRuntimeLagMetrics() -> RuntimeLagMetrics {
        engine.latestRuntimeLagMetrics()
    }

    public func setCapability(_ kind: CapabilityKind, state: CapabilityState, detail: String? = nil) {
        engine.setCapabilityState(kind: kind, state: state, detailOverride: detail)
    }

    public func updateFrontmostAppState(
        appName: String,
        bundleId: String?,
        executablePath: String?,
        windowTitle: String?
    ) {
        let state = FrontmostAppState(
            appName: appName,
            bundleId: bundleId,
            executablePath: executablePath,
            windowTitle: windowTitle,
            capturedAtMillis: UInt64(Date().timeIntervalSince1970 * 1000)
        )
        engine.updateFrontmostAppState(state: state)
    }

    public func clearFrontmostAppState() {
        engine.clearFrontmostAppState()
    }

    public func configureChromiumEndpoint(_ endpoint: String?) {
        engine.configureChromiumEndpoint(endpoint: endpoint)
    }

    public func configureDockerSocketPath(_ socketPath: String) {
        engine.configureDockerSocketPath(socketPath: socketPath)
    }

    public func configurePrivilegedHelper(path: String?, enabled: Bool) {
        engine.configurePrivilegedHelper(helperPath: path, enabled: enabled)
    }

    public func configureChau7Endpoint(_ socketPath: String?) {
        engine.configureChau7Endpoint(socketPath: socketPath)
    }

    public func configureTelemetry(endpoint: String?, enabled: Bool, exportIntervalSeconds: UInt32) {
        engine.configureTelemetry(
            endpoint: endpoint,
            enabled: enabled,
            exportIntervalSecs: exportIntervalSeconds
        )
    }

    public func verifyTelemetryExport() -> String? {
        let result = engine.verifyTelemetryExport()
        return result.isEmpty ? nil : result
    }

    public func configureRuntimeCollection(
        fullCollection: Bool,
        adaptiveCadence: Bool,
        activeTickMillis: UInt64,
        idleTickMillis: UInt64,
        lowPowerTickMillis: UInt64,
        gpuSampleIntervalMillis: UInt64,
        gpuSampleLowPowerIntervalMillis: UInt64
    ) {
        engine.configureRuntimeCollection(settings: RuntimeCollectionSettingsInput(
            fullCollection: fullCollection,
            adaptiveCadence: adaptiveCadence,
            activeTickMillis: activeTickMillis,
            idleTickMillis: idleTickMillis,
            lowPowerTickMillis: lowPowerTickMillis,
            gpuSampleIntervalMillis: gpuSampleIntervalMillis,
            gpuSampleLowPowerIntervalMillis: gpuSampleLowPowerIntervalMillis
        ))
    }

    public func stopAgentSession(sessionId: String, force: Bool) -> String? {
        let result = engine.stopAgentSession(sessionId: sessionId, force: force)
        return result.isEmpty ? nil : result
    }

    /// Pin a fan to a minimum RPM via the privileged helper.
    ///
    /// Returns `nil` on success, or a human-readable error string on failure
    /// (helper not configured, SMC write blocked, etc.). The UI surfaces the
    /// string directly in an alert.
    public func setFanMinRpm(fanId: UInt8, rpm: Float) -> String? {
        let result = engine.setFanMinRpm(fanId: fanId, rpm: rpm)
        return result.isEmpty ? nil : result
    }

    /// Restore a fan to automatic (OS-controlled) mode.
    public func resetFanAuto(fanId: UInt8) -> String? {
        let result = engine.resetFanAuto(fanId: fanId)
        return result.isEmpty ? nil : result
    }

    public func exportSnapshotJSON() -> String {
        engine.exportSnapshotJson()
    }

    public func startLocalMcpServer(socketPath: String? = nil) -> String? {
        let result = engine.startLocalMcpServer(socketPath: socketPath)
        return result.isEmpty ? nil : result
    }

    public func refreshLocalMcpCache(cachePath: String? = nil) -> String? {
        let result = engine.refreshLocalMcpCache(cachePath: cachePath)
        return result.isEmpty ? nil : result
    }

    public func loadHistoryRange(startMillis: UInt64, endMillis: UInt64) -> [SystemSnapshot] {
        engine.loadHistoryRange(startMillis: startMillis, endMillis: endMillis)
    }

    public func loadHistoryPage(
        startMillis: UInt64,
        endMillis: UInt64,
        beforeMillisExclusive: UInt64?,
        limit: UInt32
    ) -> [SystemSnapshot] {
        engine.loadHistoryPage(
            startMillis: startMillis,
            endMillis: endMillis,
            beforeMillisExclusive: beforeMillisExclusive,
            limit: limit
        )
    }

    public func historyRangeSummary(startMillis: UInt64, endMillis: UInt64) -> HistoryRangeSummary? {
        engine.historyRangeSummary(startMillis: startMillis, endMillis: endMillis)
    }

    public func historyRangeSummaryResult(
        startMillis: UInt64,
        endMillis: UInt64
    ) -> HistoryRangeSummaryResult {
        engine.historyRangeSummaryResult(startMillis: startMillis, endMillis: endMillis)
    }

    public func loadHistoryPageResult(
        startMillis: UInt64,
        endMillis: UInt64,
        beforeMillisExclusive: UInt64?,
        limit: UInt32
    ) -> HistoryPageLoadResult {
        engine.loadHistoryPageResult(
            startMillis: startMillis,
            endMillis: endMillis,
            beforeMillisExclusive: beforeMillisExclusive,
            limit: limit
        )
    }

    public func maintainHistoryStore(aggressive: Bool) -> HistoryMaintenanceReport? {
        engine.maintainHistoryStore(aggressive: aggressive)
    }

    public func latestDiagnostics(limit: UInt32 = 500) -> [DiagnosticsEvent] {
        engine.latestDiagnostics(limit: limit)
    }

    public func queryDiagnostics(_ query: DiagnosticsQuery) -> [DiagnosticsEvent] {
        engine.queryDiagnostics(query: query)
    }

    public func diagnosticsOverview() -> DiagnosticsOverview {
        engine.diagnosticsOverview()
    }

    public func clearDiagnostics() -> String? {
        let result = engine.clearDiagnostics()
        return result.isEmpty ? nil : result
    }

    public func clearHistory() -> String? {
        let result = engine.clearHistory()
        return result.isEmpty ? nil : result
    }

    public func exportDiagnosticsJSON(limit: UInt32 = 1000) -> String {
        engine.exportDiagnosticsJson(limit: limit)
    }

    public func exportDiagnosticsQueryJSON(_ query: DiagnosticsQuery) -> String {
        engine.exportDiagnosticsQueryJson(query: query)
    }

    public func recordDiagnosticsEvent(_ event: DiagnosticsEvent) {
        engine.recordDiagnosticsEvent(event: event)
    }

    public func updateUiLagMetrics(_ metrics: UiLagMetrics) {
        engine.updateUiLagMetrics(metrics: metrics)
    }
}
