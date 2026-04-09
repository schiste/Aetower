import AppKit
import Foundation
import Observation
import OSLog
import UniformTypeIdentifiers
import UserNotifications
import AetowerBridge

@MainActor
@Observable
public final class AppState {
    public private(set) var snapshot: SystemSnapshot
    public private(set) var historySnapshots: [SystemSnapshot] = []
    public private(set) var historyLoadError: String?
    public private(set) var diagnosticsEvents: [DiagnosticsEvent] = []
    public private(set) var sessionLogSummary: SessionLogSummary?
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
    public var lastError: String?

    @ObservationIgnored
    private let bridge: EngineBridge
    @ObservationIgnored
    private let permissionCoordinator: PermissionCoordinator
    @ObservationIgnored
    private let lagMonitor = LagMonitor()
    @ObservationIgnored
    private var refreshTask: Task<Void, Never>?
    @ObservationIgnored
    private var workspaceActivationTask: Task<Void, Never>?
    @ObservationIgnored
    private var historyLoadTask: Task<Void, Never>?
    @ObservationIgnored
    private var diagnosticsLoadTask: Task<Void, Never>?
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
    private let anomalyNotificationCooldown: TimeInterval = 300.0
    @ObservationIgnored
    private let suppressedAnomalySummaryInterval: TimeInterval = 300.0
    @ObservationIgnored
    private let suppressedAnomalySummaryMinimumCount = 10

    public init(
        bridge: EngineBridge = EngineBridge(),
        permissionCoordinator: PermissionCoordinator = PermissionCoordinator()
    ) {
        let initialSnapshot = (try? bridge.latestSnapshot()) ?? SystemSnapshot(
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
                gpuMemoryBytes: 0
            ),
            hostTrend: HostTrend(
                machineFriction: [],
                cpuPercent: [],
                memoryUsedBytes: [],
                diskActivityBps: [],
                networkActivityBps: [],
                wakeupsPerSecond: [],
                compressedMemoryBytes: [],
                aiAgentFriction: []
            ),
            capabilities: [],
            entities: [],
            timeline: []
        )
        self.bridge = bridge
        self.permissionCoordinator = permissionCoordinator
        self.snapshot = initialSnapshot
        self.lastObservedSequence = initialSnapshot.sequence
    }

    public func start() {
        start(refreshInterval: 1.0)
    }

    public func start(refreshInterval: Double) {
        stop()
        observeWorkspaceActivation()
        publishFrontmostState(force: true)
        refresh(force: true)
        updateLagMonitoringState()
        let intervalNanos = UInt64(max(1.0, refreshInterval) * 1_000_000_000)
        refreshTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: intervalNanos)
                guard !Task.isCancelled else { break }
                await MainActor.run {
                    self.publishFrontmostState()
                    self.refresh()
                }
            }
        }
    }

    public func stop() {
        refreshTask?.cancel()
        refreshTask = nil
        workspaceActivationTask?.cancel()
        workspaceActivationTask = nil
        historyLoadTask?.cancel()
        historyLoadTask = nil
        diagnosticsLoadTask?.cancel()
        diagnosticsLoadTask = nil
        lagMonitor.stop()
        lagMonitoringActive = false
    }

    public func requestCapability(_ capability: CapabilitySnapshot) {
        let result = permissionCoordinator.request(capability.kind)
        bridge.setCapability(capability.kind, state: result.state, detail: result.detail)
        refresh(force: true)
    }

    public func stopAgentSession(sessionId: String, force: Bool) {
        if let error = bridge.stopAgentSession(sessionId: sessionId, force: force) {
            lastError = error
        }
        refresh(force: true)
    }

    public func exportSnapshotJSON() -> String {
        bridge.exportSnapshotJSON()
    }

    public func exportSnapshot() {
        let json = exportControlledJson(
            bridge.exportSnapshotJSON(),
            privacyTier: exportPrivacyTier
        )
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "aetower-snapshot.json"
        if panel.runModal() == .OK, let url = panel.url {
            try? json.write(to: url, atomically: true, encoding: .utf8)
        }
    }

    public func exportDiagnostics(limit: UInt32 = 1000) {
        let json = exportControlledJson(
            bridge.exportDiagnosticsQueryJSON(
                DiagnosticsQuery(
                    limit: limit,
                    minimumLevel: nil,
                    subsystem: nil,
                    search: nil,
                    sinceMillis: nil,
                    includePersisted: true
                )
            ),
            privacyTier: exportPrivacyTier
        )
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "aetower-diagnostics.json"
        if panel.runModal() == .OK, let url = panel.url {
            try? json.write(to: url, atomically: true, encoding: .utf8)
        }
    }

    public func clearDiagnostics() {
        if let error = bridge.clearDiagnostics() {
            lastError = error
            return
        }
        diagnosticsEvents = []
        diagnosticsOverview = bridge.diagnosticsOverview()
        diagnosticsLoadError = nil
    }

    public func clearHistory() {
        if let error = bridge.clearHistory() {
            lastError = error
            return
        }
        historySnapshots = []
        historyLoadError = "Persisted history was cleared."
        loadDiagnostics(force: true)
    }

    public func setHistoryVisible(_ visible: Bool) {
        historyVisible = visible
        if visible {
            loadHistory(force: true)
        }
    }

    public func setDiagnosticsVisible(_ visible: Bool) {
        diagnosticsVisible = visible
        updateLagMonitoringState()
    }

    public func exportSupportBundle(_ settings: SettingsStore, diagnosticsLimit: UInt32 = 1_500) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = supportBundleDirectoryName()
        guard panel.runModal() == .OK, let url = panel.url else {
            return
        }

        do {
            let packageURL = normalizedSupportBundleURL(url)
            try writeSupportBundlePackage(
                to: packageURL,
                settings: settings,
                diagnosticsLimit: diagnosticsLimit
            )
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
        } catch {
            let message = "Support bundle export failed: \(error.localizedDescription)"
            lastError = message
            recordLocalDiagnosticsEvent(
                level: .error,
                subsystem: .ui,
                eventType: "support-bundle-export-failed",
                message: "Failed to export a support bundle.",
                fields: [
                    DiagnosticsField(key: "error", value: error.localizedDescription),
                ]
            )
        }
    }

    public func applyNotificationSettings(_ settings: SettingsStore) {
        notificationsEnabled = settings.notificationsEnabled
        frictionNotificationThreshold = settings.frictionNotificationThreshold
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

    public func applyIntegrationSettings(_ settings: SettingsStore) {
        let chromiumEndpoint = settings.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        let dockerSocketPath = settings.dockerSocketPath.trimmingCharacters(in: .whitespacesAndNewlines)
        let privilegedHelperPath = settings.privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines)

        bridge.configureChromiumEndpoint(chromiumEndpoint.isEmpty ? nil : chromiumEndpoint)
        bridge.configureDockerSocketPath(
            dockerSocketPath.isEmpty ? "/var/run/docker.sock" : dockerSocketPath
        )
        bridge.configurePrivilegedHelper(
            path: privilegedHelperPath.isEmpty ? nil : privilegedHelperPath,
            enabled: settings.privilegedHelperEnabled
        )

        let chau7Endpoint = settings.chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        bridge.configureChau7Endpoint(chau7Endpoint.isEmpty ? nil : chau7Endpoint)
        let telemetryEndpoint = settings.telemetryEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        self.telemetryEnabled = settings.telemetryEnabled
        self.telemetryEndpoint = telemetryEndpoint.isEmpty ? "http://localhost:4318/v1/metrics" : telemetryEndpoint
        telemetryVerificationStatus = nil
        bridge.configureTelemetry(
            endpoint: telemetryEndpoint.isEmpty ? nil : telemetryEndpoint,
            enabled: settings.telemetryEnabled,
            exportIntervalSeconds: UInt32(max(5, Int(settings.telemetryExportIntervalSeconds.rounded())))
        )
        bridge.configureRuntimeCollection(
            fullCollection: settings.collectionProfile == .full,
            adaptiveCadence: settings.adaptiveCadenceEnabled,
            activeTickMillis: UInt64(max(500, Int((settings.engineActiveIntervalSeconds * 1000).rounded()))),
            idleTickMillis: UInt64(max(500, Int((settings.engineIdleIntervalSeconds * 1000).rounded()))),
            lowPowerTickMillis: UInt64(max(500, Int((settings.engineLowPowerIntervalSeconds * 1000).rounded()))),
            gpuSampleIntervalMillis: UInt64(max(5_000, Int((settings.gpuSampleIntervalSeconds * 1000).rounded()))),
            gpuSampleLowPowerIntervalMillis: UInt64(max(5_000, Int((settings.gpuSampleLowPowerIntervalSeconds * 1000).rounded())))
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

    public func refresh(force: Bool = false) {
        let refreshStartedAt = CFAbsoluteTimeGetCurrent()
        let fetchStartedAt = CFAbsoluteTimeGetCurrent()
        do {
            var bridgeFetchMillis = 0.0
            var updatedSnapshotValue: SystemSnapshot?
            if let updatedSnapshot = try bridge.latestSnapshotIfNewer(since: lastObservedSequence) {
                bridgeFetchMillis = (CFAbsoluteTimeGetCurrent() - fetchStartedAt) * 1000.0
                snapshot = updatedSnapshot
                lastObservedSequence = updatedSnapshot.sequence
                updatedSnapshotValue = updatedSnapshot
            } else if force {
                let latestSnapshot = try bridge.latestSnapshot()
                bridgeFetchMillis = (CFAbsoluteTimeGetCurrent() - fetchStartedAt) * 1000.0
                snapshot = latestSnapshot
                lastObservedSequence = latestSnapshot.sequence
                updatedSnapshotValue = latestSnapshot
            } else {
                return
            }
            applyLocalFrontmostState(
                appName: lastPublishedFrontmostAppName,
                windowTitle: lastPublishedWindowTitle
            )
            runtimeLagMetrics = bridge.latestRuntimeLagMetrics()
            if let updatedSnapshotValue, lagMonitoringActive {
                publishUiLagMetrics(
                    snapshot: updatedSnapshotValue,
                    bridgeFetchMillis: bridgeFetchMillis,
                    uiRefreshMillis: (CFAbsoluteTimeGetCurrent() - refreshStartedAt) * 1000.0,
                    refreshStartedAt: refreshStartedAt
                )
            }
            if historyVisible {
                loadHistory(force: force)
            }
            diffAnomalyStates()
            flushSuppressedAnomalySummaryIfNeeded()
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
    }

    public func setHistoryWindow(seconds: TimeInterval) {
        historyWindowSeconds = max(seconds, 300)
        if historyVisible {
            loadHistory(force: true)
        }
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
        let bridge = self.bridge

        historyLoadTask?.cancel()
        historyLoadTask = Task(priority: .utility) { [weak self] in
            let snapshots = bridge.loadHistoryRange(startMillis: startMillis, endMillis: endMillis)
            guard !Task.isCancelled else { return }
            await MainActor.run {
                guard let self else { return }
                self.historySnapshots = snapshots
                self.historyLoadError = snapshots.isEmpty ? "No persisted history in the selected range yet." : nil
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
        let shouldAnalyzeSessionLogs = force || now.timeIntervalSince(lastSessionLogAnalysisDate) >= sessionLogAnalysisInterval
        if shouldAnalyzeSessionLogs {
            lastSessionLogAnalysisDate = now
        }
        diagnosticsLoadTask?.cancel()
        diagnosticsLoadTask = Task(priority: .utility) { [weak self] in
            let events = bridge.queryDiagnostics(query)
            let overview = bridge.diagnosticsOverview()
            let runtimeLagMetrics = bridge.latestRuntimeLagMetrics()
            let sessionLogSummary = shouldAnalyzeSessionLogs ? Result { try SessionLogAnalyzer.analyzeCurrentProcess(lastMinutes: 6) } : nil
            guard !Task.isCancelled else { return }
            await MainActor.run {
                guard let self else { return }
                self.diagnosticsEvents = events
                self.diagnosticsOverview = overview
                self.runtimeLagMetrics = runtimeLagMetrics
                self.diagnosticsLoadError = nil
                self.mirrorDiagnosticsToUnifiedLog(events)
                if let sessionLogSummary {
                    self.sessionLogSummary = try? sessionLogSummary.get()
                    self.applySessionLogSummary(sessionLogSummary)
                }
                self.flushSuppressedAnomalySummaryIfNeeded()
            }
        }
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

    private func supportBundlePayload(
        _ settings: SettingsStore,
        diagnosticsLimit: UInt32
    ) throws -> [String: Any] {
        let privacyTier = settings.exportPrivacyTier
        let snapshotJson = exportControlledJson(
            bridge.exportSnapshotJSON(),
            privacyTier: privacyTier
        )
        let diagnosticsJson = exportControlledJson(
            bridge.exportDiagnosticsQueryJSON(
                DiagnosticsQuery(
                    limit: diagnosticsLimit,
                    minimumLevel: nil,
                    subsystem: nil,
                    search: nil,
                    sinceMillis: nil,
                    includePersisted: true
                )
            ),
            privacyTier: privacyTier
        )

        return [
            "generatedAtMillis": UInt64(Date().timeIntervalSince1970 * 1000),
            "privacyTier": privacyTier.rawValue,
            "redacted": privacyTier != .full,
            "app": appMetadata(),
            "settings": settingsSummary(settings),
            "diagnosticsOverview": diagnosticsOverviewSummary(),
            "runtimeLagMetrics": runtimeLagSummary(),
            "sessionHealth": sessionHealthSummary(),
            "historySummary": historySummary(),
            "snapshot": try parseJsonObject(snapshotJson),
            "diagnostics": try parseJsonValue(diagnosticsJson),
        ]
    }

    private func appMetadata() -> [String: Any] {
        let info = Bundle.main.infoDictionary ?? [:]
        return [
            "bundleIdentifier": Bundle.main.bundleIdentifier ?? "unknown",
            "buildVersion": info["CFBundleVersion"] as? String ?? "unknown",
            "shortVersion": info["CFBundleShortVersionString"] as? String ?? "unknown",
            "processIdentifier": ProcessInfo.processInfo.processIdentifier,
            "processName": ProcessInfo.processInfo.processName,
        ]
    }

    private func settingsSummary(_ settings: SettingsStore) -> [String: Any] {
        let privacyTier = settings.exportPrivacyTier
        return [
            "refreshIntervalSeconds": settings.refreshIntervalSeconds,
            "showMenuBarExtra": settings.showMenuBarExtra,
            "notificationsEnabled": settings.notificationsEnabled,
            "frictionNotificationThreshold": settings.frictionNotificationThreshold,
            "appearanceMode": settings.appearanceMode,
            "chromiumEndpointConfigured": !settings.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            "dockerSocketPath": exportControlledValue(
                settings.dockerSocketPath,
                privacyTier: privacyTier,
                key: "dockerSocketPath"
            ),
            "privilegedHelperEnabled": settings.privilegedHelperEnabled,
            "privilegedHelperPathConfigured": !settings.privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            "chau7EndpointConfigured": !settings.chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            "telemetryEnabled": settings.telemetryEnabled,
            "telemetryEndpoint": exportControlledValue(
                settings.telemetryEndpoint,
                privacyTier: privacyTier,
                key: "telemetryEndpoint"
            ),
            "telemetryExportIntervalSeconds": settings.telemetryExportIntervalSeconds,
            "telemetryVerificationStatus": telemetryVerificationStatus ?? "not-run",
            "collectionProfile": settings.collectionProfile.rawValue,
            "adaptiveCadenceEnabled": settings.adaptiveCadenceEnabled,
            "engineActiveIntervalSeconds": settings.engineActiveIntervalSeconds,
            "engineIdleIntervalSeconds": settings.engineIdleIntervalSeconds,
            "engineLowPowerIntervalSeconds": settings.engineLowPowerIntervalSeconds,
            "gpuSampleIntervalSeconds": settings.gpuSampleIntervalSeconds,
            "gpuSampleLowPowerIntervalSeconds": settings.gpuSampleLowPowerIntervalSeconds,
            "exportPrivacyTier": settings.exportPrivacyTier.rawValue,
        ]
    }

    private func diagnosticsOverviewSummary() -> [String: Any] {
        [
            "ringCapacity": diagnosticsOverview.ringCapacity,
            "currentSize": diagnosticsOverview.currentSize,
            "droppedEvents": diagnosticsOverview.droppedEvents,
            "warnCount": diagnosticsOverview.warnCount,
            "errorCount": diagnosticsOverview.errorCount,
            "lastEventMillis": jsonOptional(diagnosticsOverview.lastEventMillis),
            "lastErrorMessage": jsonOptional(diagnosticsOverview.lastErrorMessage),
            "persistedEvents": diagnosticsOverview.persistedEvents,
            "persistedPath": jsonOptional(diagnosticsOverview.persistedPath),
            "persistedBytes": diagnosticsOverview.persistedBytes,
            "persistenceError": jsonOptional(diagnosticsOverview.persistenceError),
        ]
    }

    private func runtimeLagSummary() -> [String: Any] {
        [
            "updatedAtMillis": runtimeLagMetrics.updatedAtMillis,
            "engineTickMillis": runtimeLagMetrics.engineTickMillis,
            "collectMillis": runtimeLagMetrics.collectMillis,
            "identityMillis": runtimeLagMetrics.identityMillis,
            "attributionMillis": runtimeLagMetrics.attributionMillis,
            "frictionMillis": runtimeLagMetrics.frictionMillis,
            "enrichMillis": runtimeLagMetrics.enrichMillis,
            "historyMillis": runtimeLagMetrics.historyMillis,
            "persistMillis": runtimeLagMetrics.persistMillis,
            "bridgeFetchMillis": runtimeLagMetrics.bridgeFetchMillis,
            "uiRefreshMillis": runtimeLagMetrics.uiRefreshMillis,
            "snapshotToUiMillis": runtimeLagMetrics.snapshotToUiMillis,
            "snapshotToRenderMillis": runtimeLagMetrics.snapshotToRenderMillis,
            "renderCommitMillis": runtimeLagMetrics.renderCommitMillis,
            "displayFrameIntervalMillis": runtimeLagMetrics.displayFrameIntervalMillis,
            "displayRefreshHz": runtimeLagMetrics.displayRefreshHz,
            "displayDroppedFrames": runtimeLagMetrics.displayDroppedFrames,
            "inputAvgLatencyMillis": runtimeLagMetrics.inputAvgLatencyMillis,
            "inputMaxLatencyMillis": runtimeLagMetrics.inputMaxLatencyMillis,
            "inputSampleCount": runtimeLagMetrics.inputSampleCount,
        ]
    }

    private func sessionHealthSummary() -> [String: Any] {
        [
            "notificationAuthorizationStatus": notificationAuthorizationStatus,
            "sessionLogSummary": jsonOptional(sessionLogSummary.map { summary in
                [
                    "windowMinutes": summary.windowMinutes,
                    "notificationLogEntries": summary.notificationLogEntries,
                    "notificationAuthorizationFailures": summary.notificationAuthorizationFailures,
                    "metalLoadFailures": summary.metalLoadFailures,
                    "nonActiveWindowWarnings": summary.nonActiveWindowWarnings,
                    "cursorUiEntries": summary.cursorUiEntries,
                    "tccAccessRequests": summary.tccAccessRequests,
                    "viewBridgeCancellationCount": summary.viewBridgeCancellationCount,
                ]
            }),
        ]
    }

    private func historySummary() -> [String: Any] {
        let recentSnapshots = historySnapshots.suffix(24).map { snapshot in
            [
                "sequence": snapshot.sequence,
                "capturedAtMillis": snapshot.capturedAtMillis,
                "entityCount": snapshot.entities.count,
                "topEntity": jsonOptional(snapshot.entities.first?.displayName),
                "topFriction": jsonOptional(snapshot.entities.first.map { Double($0.friction.totalScore) }),
            ]
        }
        return [
            "windowSeconds": historyWindowSeconds,
            "loadedSnapshots": historySnapshots.count,
            "historyLoadError": jsonOptional(historyLoadError),
            "recentSnapshots": recentSnapshots,
        ]
    }

    private func jsonOptional(_ value: Any?) -> Any {
        value ?? NSNull()
    }

    private func parseJsonObject(_ json: String) throws -> [String: Any] {
        guard let object = try parseJsonValue(json) as? [String: Any] else {
            throw NSError(
                domain: "AetowerSupportBundle",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Expected JSON object while building support bundle."]
            )
        }
        return object
    }

    private func parseJsonValue(_ json: String) throws -> Any {
        try JSONSerialization.jsonObject(with: Data(json.utf8))
    }

    private var exportPrivacyTier: ExportPrivacyTier {
        let defaults = UserDefaults.standard
        if let raw = defaults.string(forKey: SettingsStore.exportPrivacyTierKey),
           let tier = ExportPrivacyTier(rawValue: raw) {
            return tier
        }
        let legacySensitive = defaults.object(forKey: SettingsStore.includeSensitiveExportsKey) as? Bool ?? false
        return legacySensitive ? .full : .redacted
    }

    private func exportControlledJson(_ json: String, privacyTier: ExportPrivacyTier) -> String {
        guard privacyTier != .full else {
            return json
        }
        guard let value = try? parseJsonValue(json) else {
            return json
        }
        let redacted = exportControlledValue(value, privacyTier: privacyTier)
        guard let data = try? JSONSerialization.data(
            withJSONObject: redacted,
            options: [.prettyPrinted, .sortedKeys]
        ),
              let encoded = String(data: data, encoding: .utf8)
        else {
            return json
        }
        return encoded
    }

    private func exportControlledValue(
        _ value: Any,
        privacyTier: ExportPrivacyTier,
        key: String? = nil
    ) -> Any {
        let normalizedKey = key?.lowercased() ?? ""
        if privacyTier == .redacted && isSensitiveExportKey(normalizedKey) {
            return "<redacted>"
        }

        if let dictionary = value as? [String: Any] {
            var redacted = [String: Any](minimumCapacity: dictionary.count)
            let eventIsSensitive = (dictionary["sensitive"] as? Bool) ?? false
            for (childKey, childValue) in dictionary {
                if eventIsSensitive, childKey == "message" {
                    redacted[childKey] = privacyTier == .operatorMode
                        ? "<sensitive event>"
                        : "<redacted sensitive event>"
                    continue
                }
                if eventIsSensitive, childKey == "fields" {
                    redacted[childKey] = privacyTier == .operatorMode
                        ? redactSensitiveFieldsOperatorMode(dictionary["fields"])
                        : []
                    continue
                }
                redacted[childKey] = exportControlledValue(
                    childValue,
                    privacyTier: privacyTier,
                    key: childKey
                )
            }
            return redacted
        }

        if let array = value as? [Any] {
            return array.map { child in
                exportControlledValue(child, privacyTier: privacyTier, key: key)
            }
        }

        if let string = value as? String {
            switch privacyTier {
            case .full:
                return string
            case .redacted:
                if shouldRedactStringValue(string, key: normalizedKey) {
                    return "<redacted>"
                }
            case .operatorMode:
                if isSensitiveExportKey(normalizedKey) || shouldRedactStringValue(string, key: normalizedKey) {
                    return sanitizedOperatorStringValue(string, key: normalizedKey)
                }
            }
        }

        return value
    }

    private func isSensitiveExportKey(_ key: String) -> Bool {
        if key.isEmpty {
            return false
        }
        let exactMatches: Set<String> = [
            "commandline",
            "cwd",
            "executablepath",
            "frontmostwindowtitle",
            "activewindowtitle",
            "windowtitle",
            "workspacepath",
            "reporoot",
            "sessionid",
            "telemetryendpoint",
            "path",
            "url",
            "ports",
            "entities_preview",
        ]
        if exactMatches.contains(key) {
            return true
        }
        return key.contains("command")
            || key.contains("windowtitle")
            || key.contains("workspace")
            || key.contains("repo")
            || key.contains("endpoint")
            || key.contains("executable")
            || key.contains("path")
    }

    private func shouldRedactStringValue(_ value: String, key: String) -> Bool {
        guard !value.isEmpty else {
            return false
        }
        if isSensitiveExportKey(key) {
            return true
        }
        if value.hasPrefix("/") || value.hasPrefix("file://") || value.hasPrefix("http://") || value.hasPrefix("https://") {
            return true
        }
        if value.contains("/Users/") || value.contains("/var/") || value.contains(".sock") {
            return true
        }
        return false
    }

    private func redactSensitiveFieldsOperatorMode(_ value: Any?) -> Any {
        guard let fields = value as? [[String: Any]] else {
            return []
        }
        return fields.map { field in
            let key = (field["key"] as? String) ?? "field"
            let rawValue = (field["value"] as? String) ?? ""
            return [
                "key": key,
                "value": sanitizedOperatorStringValue(rawValue, key: key.lowercased()),
            ]
        }
    }

    private func sanitizedOperatorStringValue(_ value: String, key: String) -> String {
        if value.isEmpty {
            return value
        }
        if key.contains("windowtitle") || key == "message" {
            return "<redacted>"
        }
        if key.contains("command") {
            let executable = value
                .split(separator: " ")
                .first
                .map(String.init)
                .map { URL(fileURLWithPath: $0).lastPathComponent }
                ?? "command"
            return executable
        }
        if key.contains("url") || key.contains("endpoint") {
            return sanitizedHostString(value)
        }
        if key.contains("path") || key.contains("cwd") || key.contains("workspace") || key.contains("repo") {
            return sanitizedPathString(value)
        }
        if key == "ports" || key == "sessionid" {
            return "<redacted>"
        }
        if value.hasPrefix("http://") || value.hasPrefix("https://") {
            return sanitizedHostString(value)
        }
        if value.hasPrefix("/") || value.hasPrefix("file://") || value.contains("/Users/") {
            return sanitizedPathString(value)
        }
        return value
    }

    private func sanitizedHostString(_ value: String) -> String {
        guard let components = URLComponents(string: value), let host = components.host else {
            return "<redacted host>"
        }
        let scheme = components.scheme ?? "https"
        return "\(scheme)://\(host)"
    }

    private func sanitizedPathString(_ value: String) -> String {
        let cleaned = value.replacingOccurrences(of: "file://", with: "")
        let lastPath = URL(fileURLWithPath: cleaned).lastPathComponent
        return lastPath.isEmpty ? "<redacted path>" : "…/\(lastPath)"
    }

    private func normalizedSupportBundleURL(_ url: URL) -> URL {
        if url.pathExtension == "aetower-supportbundle" {
            return url
        }
        return url.appendingPathExtension("aetower-supportbundle")
    }

    private func writeSupportBundlePackage(
        to packageURL: URL,
        settings: SettingsStore,
        diagnosticsLimit: UInt32
    ) throws {
        let payload = try supportBundlePayload(settings, diagnosticsLimit: diagnosticsLimit)
        let fileManager = FileManager.default
        if fileManager.fileExists(atPath: packageURL.path) {
            try fileManager.removeItem(at: packageURL)
        }
        try fileManager.createDirectory(at: packageURL, withIntermediateDirectories: true)

        let snapshotJson = exportControlledJson(
            bridge.exportSnapshotJSON(),
            privacyTier: settings.exportPrivacyTier
        )
        let diagnosticsJson = exportControlledJson(
            bridge.exportDiagnosticsQueryJSON(
                DiagnosticsQuery(
                    limit: diagnosticsLimit,
                    minimumLevel: nil,
                    subsystem: nil,
                    search: nil,
                    sinceMillis: nil,
                    includePersisted: true
                )
            ),
            privacyTier: settings.exportPrivacyTier
        )
        let manifest: [String: Any] = [
            "format": "aetower-supportbundle/v2",
            "generatedAtMillis": UInt64(Date().timeIntervalSince1970 * 1000),
            "privacyTier": settings.exportPrivacyTier.rawValue,
            "files": [
                "bundle.json",
                "snapshot.json",
                "diagnostics.json",
            ],
        ]

        try writeJsonObject(payload, to: packageURL.appendingPathComponent("bundle.json"))
        try snapshotJson.write(
            to: packageURL.appendingPathComponent("snapshot.json"),
            atomically: true,
            encoding: .utf8
        )
        try diagnosticsJson.write(
            to: packageURL.appendingPathComponent("diagnostics.json"),
            atomically: true,
            encoding: .utf8
        )
        try writeJsonObject(manifest, to: packageURL.appendingPathComponent("manifest.json"))
    }

    private func writeJsonObject(_ value: Any, to url: URL) throws {
        let data = try JSONSerialization.data(
            withJSONObject: value,
            options: [.prettyPrinted, .sortedKeys]
        )
        try data.write(to: url, options: .atomic)
    }

    private func supportBundleDirectoryName() -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone.current
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return "aetower-support-bundle-\(formatter.string(from: Date())).aetower-supportbundle"
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
