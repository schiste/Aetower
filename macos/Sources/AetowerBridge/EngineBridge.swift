@_exported import AetowerBindings
import Foundation

public final class EngineBridge {
    private let engine: MonitorEngine

    public init() {
        self.engine = MonitorEngine()
    }

    public func latestSnapshot() throws -> SystemSnapshot {
        engine.latestSnapshot()
    }

    public func latestSequence() throws -> UInt64 {
        engine.latestSequence()
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
}
