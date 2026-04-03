import AppKit
import Combine
import Foundation
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
        lastErrorMessage: nil
    )
    @Published public private(set) var diagnosticsLoadError: String?
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
    private var historyWindowSeconds: TimeInterval = 3600
    private var lastHistoryLoadDate = Date.distantPast
    private var lastDiagnosticsLoadDate = Date.distantPast

    private let frontmostProbeInterval: TimeInterval = 1.0
    private let windowTitleProbeInterval: TimeInterval = 5.0
    private let historyReloadInterval: TimeInterval = 20.0
    private let diagnosticsReloadInterval: TimeInterval = 2.0

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

    public func requestNotificationPermission() {
        guard Bundle.main.bundleIdentifier != nil else { return }
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
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
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let events = bridge.latestDiagnostics(limit: limit)
            let overview = bridge.diagnosticsOverview()
            DispatchQueue.main.async {
                guard let self else { return }
                self.diagnosticsEvents = events
                self.diagnosticsOverview = overview
                self.diagnosticsLoadError = nil
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
            newStates[entity.entityId] = entity.anomalyDetected
            let wasAnomaly = previousAnomalyStates[entity.entityId] ?? false
            if entity.anomalyDetected && !wasAnomaly {
                fireAnomalyNotification(for: entity)
            }
        }
        previousAnomalyStates = newStates
    }

    private func fireAnomalyNotification(for entity: EntitySnapshot) {
        guard Bundle.main.bundleIdentifier != nil else { return }
        let content = UNMutableNotificationContent()
        content.title = "Anomaly Detected"
        content.body = "\(entity.displayName) friction is unusually high (\(String(format: "%.1f", entity.friction.totalScore)))."
        content.sound = .default
        let request = UNNotificationRequest(
            identifier: "anomaly-\(entity.entityId)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }
}
