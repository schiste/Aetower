import AppKit
import Combine
import Foundation
import AetowerBridge

@MainActor
public final class AppState: ObservableObject {
    @Published public private(set) var snapshot: SystemSnapshot
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

    private let frontmostProbeInterval: TimeInterval = 1.0
    private let windowTitleProbeInterval: TimeInterval = 5.0

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
                thermalState: "nominal",
                onBattery: false,
                batteryChargePercent: nil,
                lowPowerMode: false,
                frontmostAppName: nil,
                frontmostWindowTitle: nil
            ),
            hostTrend: HostTrend(
                machineFriction: [],
                cpuPercent: [],
                memoryUsedBytes: [],
                diskActivityBps: [],
                networkActivityBps: [],
                wakeupsPerSecond: [],
                compressedMemoryBytes: []
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
            lastError = nil
        } catch {
            lastError = error.localizedDescription
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
}
