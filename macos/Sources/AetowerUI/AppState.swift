import AppKit
import Combine
import Foundation
import OSLog
import UniformTypeIdentifiers
import UserNotifications
import AetowerBridge

@MainActor
public final class AppState: ObservableObject {
    @Published public private(set) var snapshot: SystemSnapshot
    @Published public private(set) var historySnapshots: [SystemSnapshot] = []
    @Published public private(set) var historyLoadError: String?
    @Published public private(set) var diagnosticsEvents: [DiagnosticsEvent] = []
    @Published public private(set) var diagnosticsOverview = DiagnosticsOverview(
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
    @Published public private(set) var diagnosticsLoadError: String?
    @Published public private(set) var notificationAuthorizationStatus = "unknown"
    @Published public var lastError: String?

    private let bridge: EngineBridge
    private let permissionCoordinator: PermissionCoordinator
    private var timerCancellable: AnyCancellable?
    private var workspaceActivationCancellable: AnyCancellable?
    private var lastObservedSequence: UInt64
    private var lastPublishedFrontmostBaseSignature: String?
    private var lastPublishedFrontmostSignature: String?
    private var lastPublishedFrontmostAppName: String?
    private var lastPublishedWindowTitle: String?
    private var lastFrontmostProbeDate = Date.distantPast
    private var lastWindowTitleProbeDate = Date.distantPast
    private var previousAnomalyStates: [String: Bool] = [:]
    private var lastAnomalyNotificationDates: [String: Date] = [:]
    private var suppressedAnomalyNotificationCount = 0
    private var suppressedAnomalyEntityKeys = Set<String>()
    private var notificationsEnabled = false
    private var frictionNotificationThreshold = 60.0
    private var mirroredDiagnosticsSignatures = Set<String>()
    private var historyWindowSeconds: TimeInterval = 3600
    private var lastHistoryLoadDate = Date.distantPast
    private var lastDiagnosticsLoadDate = Date.distantPast
    private var lastSessionLogAnalysisDate = Date.distantPast
    private var lastSessionLogFingerprint: String?
    private var lastSuppressedAnomalySummaryDate = Date.distantPast

    private let frontmostProbeInterval: TimeInterval = 1.0
    private let windowTitleProbeInterval: TimeInterval = 5.0
    private let historyReloadInterval: TimeInterval = 20.0
    private let diagnosticsReloadInterval: TimeInterval = 2.0
    private let sessionLogAnalysisInterval: TimeInterval = 45.0
    private let anomalyNotificationCooldown: TimeInterval = 300.0
    private let suppressedAnomalySummaryInterval: TimeInterval = 30.0

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
        loadHistory(force: true)
        timerCancellable = Timer.publish(every: max(1.0, refreshInterval), on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                self?.publishFrontmostState()
                self?.refresh()
            }
    }

    public func stop() {
        timerCancellable?.cancel()
        timerCancellable = nil
        workspaceActivationCancellable?.cancel()
        workspaceActivationCancellable = nil
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

    public func exportSnapshot() {
        let json = bridge.exportSnapshotJSON()
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "aetower-snapshot.json"
        if panel.runModal() == .OK, let url = panel.url {
            try? json.write(to: url, atomically: true, encoding: .utf8)
        }
    }

    public func exportDiagnostics(limit: UInt32 = 1000) {
        let json = bridge.exportDiagnosticsJSON(limit: limit)
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "aetower-diagnostics.json"
        if panel.runModal() == .OK, let url = panel.url {
            try? json.write(to: url, atomically: true, encoding: .utf8)
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
        bridge.configureTelemetry(
            endpoint: telemetryEndpoint.isEmpty ? nil : telemetryEndpoint,
            enabled: settings.telemetryEnabled,
            exportIntervalSeconds: UInt32(max(5, Int(settings.telemetryExportIntervalSeconds.rounded())))
        )

        refresh(force: true)
    }

    public func refresh(force: Bool = false) {
        do {
            if let updatedSnapshot = try bridge.latestSnapshotIfNewer(since: lastObservedSequence) {
                snapshot = updatedSnapshot
                lastObservedSequence = updatedSnapshot.sequence
            } else if force {
                let latestSnapshot = try bridge.latestSnapshot()
                snapshot = latestSnapshot
                lastObservedSequence = latestSnapshot.sequence
            } else {
                return
            }
            applyLocalFrontmostState(
                appName: lastPublishedFrontmostAppName,
                windowTitle: lastPublishedWindowTitle
            )
            loadHistory(force: force)
            diffAnomalyStates()
            flushSuppressedAnomalySummaryIfNeeded()
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
    }

    public func setHistoryWindow(seconds: TimeInterval) {
        historyWindowSeconds = max(seconds, 300)
        loadHistory(force: true)
    }

    public func loadHistory(force: Bool = false) {
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

        DispatchQueue.global(qos: .utility).async { [weak self] in
            let snapshots = bridge.loadHistoryRange(startMillis: startMillis, endMillis: endMillis)
            DispatchQueue.main.async {
                guard let self else { return }
                self.historySnapshots = snapshots
                self.historyLoadError = snapshots.isEmpty ? "No persisted history in the selected range yet." : nil
            }
        }
    }

    public func loadDiagnostics(force: Bool = false, limit: UInt32 = 500) {
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
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let events = bridge.latestDiagnostics(limit: limit)
            let overview = bridge.diagnosticsOverview()
            let sessionLogSummary = shouldAnalyzeSessionLogs ? Result { try SessionLogAnalyzer.analyzeCurrentProcess(lastMinutes: 6) } : nil
            DispatchQueue.main.async {
                guard let self else { return }
                self.diagnosticsEvents = events
                self.diagnosticsOverview = overview
                self.diagnosticsLoadError = nil
                self.mirrorDiagnosticsToUnifiedLog(events)
                if let sessionLogSummary {
                    self.applySessionLogSummary(sessionLogSummary)
                }
                self.flushSuppressedAnomalySummaryIfNeeded()
            }
        }
    }

    private func observeWorkspaceActivation() {
        workspaceActivationCancellable = NSWorkspace.shared.notificationCenter
            .publisher(for: NSWorkspace.didActivateApplicationNotification)
            .sink { [weak self] _ in
                self?.publishFrontmostState(force: true)
                self?.refresh()
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
        if shouldProbeWindowTitle {
            windowTitle = permissionCoordinator
                .currentFrontmostAppObservation(includeWindowTitle: true)?
                .windowTitle
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
            if summary.metalLoadFailures > 0 {
                recordLocalDiagnosticsEvent(
                    level: .error,
                    subsystem: .ui,
                    eventType: "session-log-metal-error",
                    message: "Unified logs recorded a Metal-side load failure in this session.",
                    fields: [
                        DiagnosticsField(key: "count", value: String(summary.metalLoadFailures)),
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
}
