import AppKit
import Darwin
import Foundation
import Observation
import AetowerBridge

private struct LocalMcpSocketProbe {
    let reachable: Bool
    let detail: String
}

@MainActor
@Observable
final class LocalMcpController {
    private(set) var clientStatuses: [LocalMcpClientRegistrationStatus] = []
    private(set) var registrationStatusMessage: String?
    private(set) var serverHealthy = false
    private(set) var lastHealthCheckDate: Date?
    private(set) var lastStartAttemptDate: Date?
    private(set) var lastStartSucceededDate: Date?
    private(set) var lastStartError: String?
    private(set) var lastProbeDetail: String?
    private(set) var restartCount = 0
    private(set) var consecutiveProbeFailures = 0
    private(set) var lastError: String?

    @ObservationIgnored
    private let bridge: EngineBridge
    @ObservationIgnored
    private var serverStarted = false
    @ObservationIgnored
    private var serverOperatorActionsEnabled: Bool?
    @ObservationIgnored
    private var lastHealthProbeDate = Date.distantPast
    @ObservationIgnored
    private var lastSocketProbeDate = Date.distantPast
    @ObservationIgnored
    private var cachedSocketProbe: LocalMcpSocketProbe?
    @ObservationIgnored
    private let healthyHealthCheckInterval: TimeInterval = 30
    @ObservationIgnored
    private let unhealthyHealthCheckInterval: TimeInterval = 5
    @ObservationIgnored
    private let restartFailureThreshold = 2
    @ObservationIgnored
    private let probeCacheInterval: TimeInterval = 2
    @ObservationIgnored
    private let socketPath = LocalMcpClientRegistrar.defaultSocketPath()
    @ObservationIgnored
    private var automaticRegistrationAttempted = false

    init(bridge: EngineBridge) {
        self.bridge = bridge
    }

    var socketPathDisplay: String {
        socketPath
    }

    func start(
        autoRegisterClients: Bool = false,
        operatorActionsEnabled: Bool = true
    ) {
        ensureServer(force: true, operatorActionsEnabled: operatorActionsEnabled)
        refreshClientStatuses()
        if autoRegisterClients {
            ensureAutomaticClientRegistration()
        }
    }

    func stop() {
        bridge.stopLocalMcpServer()
        serverStarted = false
        serverOperatorActionsEnabled = nil
        serverHealthy = false
        lastProbeDetail = "listener stopped"
        lastSocketProbeDate = Date()
        cachedSocketProbe = LocalMcpSocketProbe(
            reachable: false,
            detail: "listener stopped"
        )
    }

    func ensureServer(
        force: Bool = false,
        operatorActionsEnabled: Bool = true
    ) {
        let now = Date()
        let healthCheckInterval = serverHealthy
            ? healthyHealthCheckInterval
            : unhealthyHealthCheckInterval
        let modeChanged = serverOperatorActionsEnabled != nil
            && serverOperatorActionsEnabled != operatorActionsEnabled
        guard force || modeChanged || now.timeIntervalSince(lastHealthProbeDate) >= healthCheckInterval else {
            return
        }
        lastHealthProbeDate = now
        lastHealthCheckDate = now

        let probe = currentSocketProbe(forceFresh: force || modeChanged)
        lastProbeDetail = probe.detail
        if serverStarted && probe.reachable {
            serverHealthy = true
            consecutiveProbeFailures = 0
        } else if serverStarted {
            serverHealthy = false
            consecutiveProbeFailures += 1
        } else {
            serverHealthy = false
            consecutiveProbeFailures = 0
        }

        let shouldRestart = force
            || modeChanged
            || !serverStarted
            || (serverStarted && !probe.reachable && consecutiveProbeFailures >= restartFailureThreshold)
        guard shouldRestart else {
            return
        }

        let wasStarted = serverStarted
        lastStartAttemptDate = now
        if let error = bridge.startLocalMcpServer(operatorActionsEnabled: operatorActionsEnabled) {
            serverStarted = false
            serverOperatorActionsEnabled = nil
            serverHealthy = false
            lastStartError = error
            lastError = error
        } else {
            serverStarted = true
            serverOperatorActionsEnabled = operatorActionsEnabled
            if wasStarted || lastStartSucceededDate != nil {
                restartCount += 1
            }
            let postStartProbe = waitForSocketReachability()
            serverHealthy = postStartProbe.reachable
            lastProbeDetail = postStartProbe.detail
            consecutiveProbeFailures = postStartProbe.reachable
                ? 0
                : max(consecutiveProbeFailures, 1)
            lastStartError = postStartProbe.reachable
                ? nil
                : "listener did not become reachable after startup (\(postStartProbe.detail))"
            if postStartProbe.reachable {
                lastStartSucceededDate = now
            } else {
                lastError = lastStartError
            }
            refreshClientStatuses()
        }
    }

    func refreshHealthSnapshot() {
        let now = Date()
        let probe = currentSocketProbe()
        lastHealthCheckDate = now
        lastProbeDetail = probe.detail
        serverHealthy = serverStarted && probe.reachable
    }

    func refreshClientStatuses() {
        clientStatuses = LocalMcpClientRegistrar.inspectStatuses()
    }

    func registerSupportedClients() {
        let report = LocalMcpClientRegistrar.registerSupportedClients()
        clientStatuses = report.statuses
        if !report.updatedProviders.isEmpty {
            let names = report.updatedProviders.joined(separator: ", ")
            registrationStatusMessage = "Registered Aetower MCP in \(names)."
        } else if !report.errors.isEmpty {
            registrationStatusMessage = report.errors.joined(separator: " ")
            lastError = registrationStatusMessage
        } else {
            registrationStatusMessage = "No supported local AI client needed an MCP registration update."
        }
    }

    func applyClientRegistrationSettings(_ settings: SettingsStore) {
        if settings.autoRegisterLocalMcpClientsEnabled {
            ensureAutomaticClientRegistration()
        } else {
            refreshClientStatuses()
            registrationStatusMessage = "Automatic MCP client registration is off. Manual registration remains available."
        }
    }

    func copyConfigSnippet(providerId: String) {
        guard let snippet = LocalMcpClientRegistrar.manualSnippet(for: providerId), !snippet.isEmpty else {
            lastError = "No MCP config snippet is available for that client."
            return
        }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(snippet, forType: .string)
        if let provider = clientStatuses.first(where: { $0.id == providerId })?.displayName {
            registrationStatusMessage = "Copied an Aetower MCP config snippet for \(provider)."
        } else {
            registrationStatusMessage = "Copied an Aetower MCP config snippet."
        }
    }

    func consumeLastError() -> String? {
        let error = lastError
        lastError = nil
        return error
    }

    private func ensureAutomaticClientRegistration() {
        guard !automaticRegistrationAttempted else {
            return
        }
        automaticRegistrationAttempted = true
        let report = LocalMcpClientRegistrar.registerSupportedClients()
        clientStatuses = report.statuses
        if !report.updatedProviders.isEmpty {
            registrationStatusMessage = "Registered Aetower MCP in \(report.updatedProviders.joined(separator: ", "))."
        } else if !report.errors.isEmpty {
            registrationStatusMessage = report.errors.joined(separator: " ")
        }
    }

    private func currentSocketProbe(forceFresh: Bool = false) -> LocalMcpSocketProbe {
        let now = Date()
        if !forceFresh,
           let cachedProbe = cachedSocketProbe,
           now.timeIntervalSince(lastSocketProbeDate) < probeCacheInterval {
            return cachedProbe
        }
        let probe = probeSocket()
        lastSocketProbeDate = now
        cachedSocketProbe = probe
        return probe
    }

    private func probeSocket() -> LocalMcpSocketProbe {
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else {
            return LocalMcpSocketProbe(reachable: false, detail: "socket creation failed: \(socketErrorDetail(errno))")
        }
        defer { Darwin.close(fd) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = Array(socketPath.utf8)
        let maxLen = MemoryLayout.size(ofValue: addr.sun_path) - 1
        guard pathBytes.count <= maxLen else {
            return LocalMcpSocketProbe(reachable: false, detail: "socket path is too long")
        }
        withUnsafeMutablePointer(to: &addr.sun_path) { rawPath in
            rawPath.withMemoryRebound(to: CChar.self, capacity: pathBytes.count + 1) { dest in
                for (index, byte) in pathBytes.enumerated() {
                    dest[index] = CChar(bitPattern: byte)
                }
                dest[pathBytes.count] = 0
            }
        }
        let sockLen = socklen_t(MemoryLayout<sockaddr_un>.size)

        let flags = fcntl(fd, F_GETFL, 0)
        _ = fcntl(fd, F_SETFL, flags | O_NONBLOCK)

        let connectResult = withUnsafePointer(to: &addr) { ptr -> Int32 in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sap in
                Darwin.connect(fd, sap, sockLen)
            }
        }
        if connectResult == 0 {
            return LocalMcpSocketProbe(reachable: true, detail: "listener accepted a local socket connection")
        }
        let connectErrno = errno
        if connectErrno != EINPROGRESS {
            return LocalMcpSocketProbe(
                reachable: false,
                detail: "connect failed: \(socketErrorDetail(connectErrno))"
            )
        }
        var pfd = pollfd(fd: fd, events: Int16(POLLOUT), revents: 0)
        let timeoutMillis: Int32 = 120
        let polled = poll(&pfd, 1, timeoutMillis)
        if polled == 0 {
            return LocalMcpSocketProbe(
                reachable: false,
                detail: "connect timed out after \(timeoutMillis) ms"
            )
        }
        if polled < 0 {
            return LocalMcpSocketProbe(
                reachable: false,
                detail: "poll failed: \(socketErrorDetail(errno))"
            )
        }

        var soError: Int32 = 0
        var errLen = socklen_t(MemoryLayout<Int32>.size)
        guard getsockopt(fd, SOL_SOCKET, SO_ERROR, &soError, &errLen) == 0 else {
            return LocalMcpSocketProbe(
                reachable: false,
                detail: "getsockopt failed: \(socketErrorDetail(errno))"
            )
        }
        if soError == 0 {
            return LocalMcpSocketProbe(reachable: true, detail: "listener accepted a local socket connection")
        }
        return LocalMcpSocketProbe(
            reachable: false,
            detail: "connect completed with error: \(socketErrorDetail(soError))"
        )
    }

    private func waitForSocketReachability(attempts: Int = 3) -> LocalMcpSocketProbe {
        var probe = currentSocketProbe(forceFresh: true)
        guard !probe.reachable else {
            return probe
        }
        for _ in 1..<attempts {
            usleep(50_000)
            probe = currentSocketProbe(forceFresh: true)
            if probe.reachable {
                return probe
            }
        }
        return probe
    }

    private func socketErrorDetail(_ errorNumber: Int32) -> String {
        String(cString: strerror(errorNumber))
    }
}
