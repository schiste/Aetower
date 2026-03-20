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
    private var lastObservedSequence: UInt64
    private var lastPublishedFrontmostSignature: String?

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
                diskReadBps: 0,
                diskWriteBps: 0,
                networkReceiveBps: 0,
                networkSendBps: 0,
                thermalState: "nominal",
                onBattery: false,
                frontmostAppName: nil,
                frontmostWindowTitle: nil
            ),
            hostTrend: HostTrend(
                machineFriction: [],
                cpuPercent: [],
                memoryUsedBytes: [],
                diskActivityBps: []
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
        refresh(force: true)
        timerCancellable = Timer.publish(every: max(1.0, refreshInterval), on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                self?.refresh()
            }
    }

    public func stop() {
        timerCancellable?.cancel()
        timerCancellable = nil
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
        refresh(force: true)
    }

    public func refresh(force: Bool = false) {
        publishFrontmostState()
        do {
            let latestSequence = try bridge.latestSequence()
            if !force && latestSequence == lastObservedSequence {
                return
            }
            snapshot = try bridge.latestSnapshot()
            lastObservedSequence = snapshot.sequence
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func publishFrontmostState() {
        guard let observation = permissionCoordinator.currentFrontmostAppObservation() else {
            if lastPublishedFrontmostSignature != nil {
                bridge.clearFrontmostAppState()
                lastPublishedFrontmostSignature = nil
            }
            return
        }

        let signature = [
            observation.appName,
            observation.bundleId ?? "",
            observation.executablePath ?? "",
            observation.windowTitle ?? ""
        ].joined(separator: "\u{1f}")

        guard signature != lastPublishedFrontmostSignature else {
            return
        }

        bridge.updateFrontmostAppState(
            appName: observation.appName,
            bundleId: observation.bundleId,
            executablePath: observation.executablePath,
            windowTitle: observation.windowTitle
        )
        lastPublishedFrontmostSignature = signature
    }
}
