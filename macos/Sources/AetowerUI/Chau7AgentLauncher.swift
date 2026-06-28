import Darwin
import Foundation

struct Chau7AgentLaunchRequest: Sendable {
    let socketPath: String
    let repositoryRoot: String
    let agentCommand: String
    let prompt: String
}

struct Chau7AgentLaunchResult: Sendable, Equatable {
    let launchedCount: Int
    let tabIDs: [String]
    let promptStatus: String?
    let summary: String
}

enum Chau7AgentLauncher {
    static let defaultSocketPath = "~/.chau7/mcp.sock"

    static func resolvedSocketPath(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let path = trimmed.isEmpty ? defaultSocketPath : trimmed
        if path == "~" {
            return FileManager.default.homeDirectoryForCurrentUser.path
        }
        if path.hasPrefix("~/") {
            return FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(path.dropFirst(2)))
                .path
        }
        return path
    }

    static func launch(_ request: Chau7AgentLaunchRequest) async throws -> Chau7AgentLaunchResult {
        try await Task.detached(priority: .userInitiated) {
            try launchSync(request)
        }.value
    }

    private static func launchSync(_ request: Chau7AgentLaunchRequest) throws -> Chau7AgentLaunchResult {
        let agentCommand = SettingsStore.normalizedChau7AgentCommand(request.agentCommand)
        let socketPath = resolvedSocketPath(request.socketPath)
        let client = try Chau7JSONRPCClient(socketPath: socketPath)

        _ = try client.call(
            method: "initialize",
            params: [
                "protocolVersion": "2024-11-05",
                "capabilities": [:],
                "clientInfo": [
                    "name": "Aetower",
                    "version": "dev",
                ],
            ],
            timeoutSeconds: 8
        )
        try client.notify(method: "notifications/initialized", params: [:])

        let result = try client.call(
            method: "tools/call",
            params: [
                "name": "agent_launch",
                "arguments": [
                    "directory": request.repositoryRoot,
                    "agent_command": agentCommand,
                    "prompt": request.prompt,
                    "count": 1,
                    "ready_timeout_ms": 30_000,
                ],
            ],
            timeoutSeconds: 45
        )
        return try parseLaunchResult(result)
    }

    private static func parseLaunchResult(_ result: [String: Any]) throws -> Chau7AgentLaunchResult {
        if let isError = result["isError"] as? Bool, isError {
            throw Chau7AgentLaunchError.toolError(extractToolText(result) ?? "Chau7 returned an error.")
        }

        let payload = extractStructuredContent(result) ?? parseToolTextPayload(result) ?? [:]
        if let error = payload["error"] as? String {
            throw Chau7AgentLaunchError.toolError(error)
        }

        let agents = payload["agents"] as? [[String: Any]] ?? []
        let launchedAgents = agents.filter { ($0["status"] as? String) == "launched" }
        let launchedCount = payload["launched"] as? Int ?? launchedAgents.count
        let tabIDs = launchedAgents.compactMap { $0["tab_id"] as? String }
        let promptStatus = launchedAgents.compactMap { $0["prompt"] as? String }.first

        let summary: String
        if !tabIDs.isEmpty {
            summary = "Launched \(launchedCount) Chau7 tab\(launchedCount == 1 ? "" : "s"): \(tabIDs.joined(separator: ", "))"
        } else if launchedCount > 0 {
            summary = "Launched \(launchedCount) Chau7 tab\(launchedCount == 1 ? "" : "s")."
        } else if let firstError = agents.compactMap({ $0["error"] as? String }).first {
            throw Chau7AgentLaunchError.toolError(firstError)
        } else {
            summary = extractToolText(result) ?? "Chau7 accepted the launch request."
        }

        return Chau7AgentLaunchResult(
            launchedCount: launchedCount,
            tabIDs: tabIDs,
            promptStatus: promptStatus,
            summary: summary
        )
    }

    private static func extractStructuredContent(_ result: [String: Any]) -> [String: Any]? {
        result["structuredContent"] as? [String: Any]
    }

    private static func parseToolTextPayload(_ result: [String: Any]) -> [String: Any]? {
        guard let text = extractToolText(result),
              let data = text.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        return object
    }

    private static func extractToolText(_ result: [String: Any]) -> String? {
        guard let content = result["content"] as? [[String: Any]] else {
            return result["text"] as? String
        }
        return content.compactMap { item in
            item["text"] as? String
        }.joined(separator: "\n")
    }
}

enum Chau7AgentLaunchError: LocalizedError {
    case socket(String)
    case json(String)
    case rpc(String)
    case toolError(String)
    case timeout(String)
    case disconnected

    var errorDescription: String? {
        switch self {
        case let .socket(detail):
            return "Chau7 socket error: \(detail)"
        case let .json(detail):
            return "Chau7 MCP JSON error: \(detail)"
        case let .rpc(detail):
            return "Chau7 MCP RPC error: \(detail)"
        case let .toolError(detail):
            return "Chau7 launch failed: \(detail)"
        case let .timeout(detail):
            return "Chau7 MCP timed out: \(detail)"
        case .disconnected:
            return "Chau7 closed the MCP connection before replying."
        }
    }
}

private final class Chau7JSONRPCClient {
    private let fd: Int32
    private var nextID = 1
    private var buffer = Data()

    init(socketPath: String) throws {
        fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else {
            throw Chau7AgentLaunchError.socket(Self.socketErrorDetail(errno))
        }
        do {
            try Self.connect(fd: fd, socketPath: socketPath)
        } catch {
            Darwin.close(fd)
            throw error
        }
    }

    deinit {
        Darwin.close(fd)
    }

    func call(method: String, params: [String: Any], timeoutSeconds: TimeInterval) throws -> [String: Any] {
        let id = nextID
        nextID += 1
        try send([
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        ])
        return try readResponse(id: id, timeoutSeconds: timeoutSeconds)
    }

    func notify(method: String, params: [String: Any]) throws {
        try send([
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        ])
    }

    private func send(_ object: [String: Any]) throws {
        guard JSONSerialization.isValidJSONObject(object) else {
            throw Chau7AgentLaunchError.json("invalid JSON-RPC request")
        }
        var data = try JSONSerialization.data(withJSONObject: object)
        data.append(0x0A)
        try data.withUnsafeBytes { rawBuffer in
            guard let baseAddress = rawBuffer.baseAddress else { return }
            var written = 0
            while written < data.count {
                let count = Darwin.write(fd, baseAddress.advanced(by: written), data.count - written)
                if count < 0 {
                    if errno == EINTR { continue }
                    throw Chau7AgentLaunchError.socket("write failed: \(Self.socketErrorDetail(errno))")
                }
                written += count
            }
        }
    }

    private func readResponse(id: Int, timeoutSeconds: TimeInterval) throws -> [String: Any] {
        let deadline = Date().addingTimeInterval(timeoutSeconds)
        while Date() < deadline {
            if let object = try nextBufferedObject(matching: id) {
                return try Self.extractResult(from: object)
            }

            var pollDescriptor = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
            let remainingMillis = max(1, Int32(deadline.timeIntervalSinceNow * 1000))
            let polled = poll(&pollDescriptor, 1, min(remainingMillis, 250))
            if polled < 0 {
                if errno == EINTR { continue }
                throw Chau7AgentLaunchError.socket("poll failed: \(Self.socketErrorDetail(errno))")
            }
            guard polled > 0 else {
                continue
            }

            var chunk = [UInt8](repeating: 0, count: 8192)
            let readCount = Darwin.read(fd, &chunk, chunk.count)
            if readCount < 0 {
                if errno == EINTR { continue }
                throw Chau7AgentLaunchError.socket("read failed: \(Self.socketErrorDetail(errno))")
            }
            if readCount == 0 {
                throw Chau7AgentLaunchError.disconnected
            }
            buffer.append(contentsOf: chunk.prefix(readCount))
        }
        throw Chau7AgentLaunchError.timeout("no response for request \(id) after \(Int(timeoutSeconds))s")
    }

    private func nextBufferedObject(matching id: Int) throws -> [String: Any]? {
        while let newline = buffer.firstIndex(of: 0x0A) {
            let line = buffer[..<newline]
            buffer.removeSubrange(...newline)
            guard !line.isEmpty else { continue }
            guard let object = try JSONSerialization.jsonObject(with: Data(line)) as? [String: Any] else {
                throw Chau7AgentLaunchError.json("expected JSON-RPC object")
            }
            if Self.objectID(object) == id {
                return object
            }
        }
        return nil
    }

    private static func extractResult(from object: [String: Any]) throws -> [String: Any] {
        if let error = object["error"] as? [String: Any] {
            let message = error["message"] as? String ?? "\(error)"
            throw Chau7AgentLaunchError.rpc(message)
        }
        guard let result = object["result"] as? [String: Any] else {
            throw Chau7AgentLaunchError.rpc("missing result object")
        }
        return result
    }

    private static func objectID(_ object: [String: Any]) -> Int? {
        if let id = object["id"] as? Int { return id }
        if let id = object["id"] as? Double { return Int(id) }
        if let id = object["id"] as? String { return Int(id) }
        return nil
    }

    private static func connect(fd: Int32, socketPath: String) throws {
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = Array(socketPath.utf8)
        let maxLen = MemoryLayout.size(ofValue: addr.sun_path) - 1
        guard pathBytes.count <= maxLen else {
            throw Chau7AgentLaunchError.socket("socket path is too long")
        }
        withUnsafeMutablePointer(to: &addr.sun_path) { rawPath in
            rawPath.withMemoryRebound(to: CChar.self, capacity: pathBytes.count + 1) { dest in
                for (index, byte) in pathBytes.enumerated() {
                    dest[index] = CChar(bitPattern: byte)
                }
                dest[pathBytes.count] = 0
            }
        }

        let flags = fcntl(fd, F_GETFL, 0)
        _ = fcntl(fd, F_SETFL, flags | O_NONBLOCK)
        let sockLen = socklen_t(MemoryLayout<sockaddr_un>.size)
        let connectResult = withUnsafePointer(to: &addr) { ptr -> Int32 in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sap in
                Darwin.connect(fd, sap, sockLen)
            }
        }
        if connectResult == 0 {
            _ = fcntl(fd, F_SETFL, flags)
            return
        }
        let connectErrno = errno
        guard connectErrno == EINPROGRESS else {
            throw Chau7AgentLaunchError.socket("connect failed: \(socketErrorDetail(connectErrno))")
        }

        var pollDescriptor = pollfd(fd: fd, events: Int16(POLLOUT), revents: 0)
        let polled = poll(&pollDescriptor, 1, 1_500)
        if polled == 0 {
            throw Chau7AgentLaunchError.timeout("connect to \(socketPath)")
        }
        if polled < 0 {
            throw Chau7AgentLaunchError.socket("connect poll failed: \(socketErrorDetail(errno))")
        }

        var soError: Int32 = 0
        var errLen = socklen_t(MemoryLayout<Int32>.size)
        guard getsockopt(fd, SOL_SOCKET, SO_ERROR, &soError, &errLen) == 0 else {
            throw Chau7AgentLaunchError.socket("getsockopt failed: \(socketErrorDetail(errno))")
        }
        guard soError == 0 else {
            throw Chau7AgentLaunchError.socket("connect completed with error: \(socketErrorDetail(soError))")
        }
        _ = fcntl(fd, F_SETFL, flags)
    }

    private static func socketErrorDetail(_ errorNumber: Int32) -> String {
        String(cString: strerror(errorNumber))
    }
}
