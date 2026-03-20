import Combine
import Foundation
import AetowerBridge

@MainActor
public final class AppState: ObservableObject {
    @Published public private(set) var snapshot: SystemSnapshot
    @Published public var selectedEntityID: String?
    @Published public var lastError: String?
    @Published public var searchText: String = ""

    private let bridge: EngineBridge
    private let permissionCoordinator: PermissionCoordinator
    private var timerCancellable: AnyCancellable?

    public init(
        bridge: EngineBridge = EngineBridge(),
        permissionCoordinator: PermissionCoordinator = PermissionCoordinator()
    ) {
        self.bridge = bridge
        self.permissionCoordinator = permissionCoordinator
        self.snapshot = (try? bridge.latestSnapshot()) ?? SystemSnapshot(
            sequence: 0,
            capturedAtMillis: 0,
            host: HostSnapshot(
                cpuPercent: 0,
                memoryUsedBytes: 0,
                memoryTotalBytes: 0,
                swapUsedBytes: 0,
                networkReceiveBps: 0,
                networkSendBps: 0,
                thermalState: "nominal",
                onBattery: false,
                frontmostAppName: nil,
                frontmostWindowTitle: nil
            ),
            capabilities: [],
            entities: [],
            timeline: []
        )
    }

    public func start() {
        start(refreshInterval: 1.0)
    }

    public func start(refreshInterval: Double) {
        stop()
        refresh()
        timerCancellable = Timer.publish(every: max(0.25, refreshInterval), on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                self?.refresh()
            }
    }

    public func stop() {
        timerCancellable?.cancel()
        timerCancellable = nil
    }

    public var visibleEntities: [EntitySnapshot] {
        guard !searchText.isEmpty else { return snapshot.entities }
        return snapshot.entities.filter {
            $0.displayName.localizedCaseInsensitiveContains(searchText)
                || $0.badges.joined(separator: " ").localizedCaseInsensitiveContains(searchText)
        }
    }

    public var selectedEntity: EntitySnapshot? {
        let source = visibleEntities.isEmpty ? snapshot.entities : visibleEntities
        if let selectedEntityID {
            return source.first(where: { $0.entityId == selectedEntityID }) ?? snapshot.entities.first(where: { $0.entityId == selectedEntityID })
        }
        return source.first
    }

    public func requestCapability(_ capability: CapabilitySnapshot) {
        let result = permissionCoordinator.request(capability.kind)
        bridge.setCapability(capability.kind, state: result.state, detail: result.detail)
        refresh()
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
        refresh()
    }

    public func refresh() {
        publishFrontmostState()
        do {
            snapshot = try bridge.latestSnapshot()
            if selectedEntityID == nil {
                selectedEntityID = snapshot.entities.first?.entityId
            }
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func publishFrontmostState() {
        guard let observation = permissionCoordinator.currentFrontmostAppObservation() else {
            bridge.clearFrontmostAppState()
            return
        }

        bridge.updateFrontmostAppState(
            appName: observation.appName,
            bundleId: observation.bundleId,
            executablePath: observation.executablePath,
            windowTitle: observation.windowTitle
        )
    }
}
