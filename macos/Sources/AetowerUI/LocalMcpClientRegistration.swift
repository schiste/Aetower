import Foundation

public enum LocalMcpClientRegistrationState: String, Sendable {
    case registered
    case availableForAutomaticRegistration
    case manualConfigurationRequired
    case unavailable
    case notInstalled
}

public struct LocalMcpClientRegistrationStatus: Identifiable, Sendable {
    public let id: String
    public let displayName: String
    public let isInstalled: Bool
    public let state: LocalMcpClientRegistrationState
    public let detail: String
    public let configPath: String?
    public let supportsAutomaticRegistration: Bool
    public let manualSnippet: String?
}

struct LocalMcpClientRegistrationReport {
    let statuses: [LocalMcpClientRegistrationStatus]
    let updatedProviders: [String]
    let errors: [String]
}

struct LocalMcpClientRegistrar {
    private enum Client: String, CaseIterable {
        case claude
        case codex
        case chatgpt

        var displayName: String {
            switch self {
            case .claude:
                return "Claude"
            case .codex:
                return "Codex"
            case .chatgpt:
                return "ChatGPT"
            }
        }

        var supportsAutomaticRegistration: Bool {
            switch self {
            case .claude, .codex:
                return true
            case .chatgpt:
                return false
            }
        }

        var configPath: String? {
            switch self {
            case .claude:
                return homeRelativePath("Library/Application Support/Claude/claude_desktop_config.json")
            case .codex:
                return homeRelativePath(".codex/config.toml")
            case .chatgpt:
                return nil
            }
        }

        var supportMarkerPath: String? {
            switch self {
            case .claude:
                return configPath
            case .codex:
                return homeRelativePath(".codex/config.toml")
            case .chatgpt:
                return homeRelativePath("Library/Application Support/com.openai.chat")
            }
        }

        var appBundleNames: [String] {
            switch self {
            case .claude:
                return ["Claude.app"]
            case .codex:
                return ["Codex.app"]
            case .chatgpt:
                return ["ChatGPT.app"]
            }
        }
    }

    static func inspectStatuses(fileManager: FileManager = .default) -> [LocalMcpClientRegistrationStatus] {
        let commandPath = bundledProxyCommandPath(fileManager: fileManager)
        return Client.allCases.map { client in
            status(for: client, commandPath: commandPath, fileManager: fileManager)
        }
    }

    static func registerSupportedClients(fileManager: FileManager = .default) -> LocalMcpClientRegistrationReport {
        let commandPath = bundledProxyCommandPath(fileManager: fileManager)
        var updatedProviders: [String] = []
        var errors: [String] = []

        if let commandPath, isInstalled(.claude, fileManager: fileManager) {
            do {
                if try upsertClaudeRegistration(commandPath: commandPath, fileManager: fileManager) {
                    updatedProviders.append(Client.claude.displayName)
                }
            } catch {
                errors.append("Claude: \(error.localizedDescription)")
            }
        } else if isInstalled(.claude, fileManager: fileManager) {
            errors.append("Claude: Aetower MCP helper is missing from the app bundle.")
        }

        if let commandPath, isInstalled(.codex, fileManager: fileManager) {
            do {
                if try upsertCodexRegistration(commandPath: commandPath, fileManager: fileManager) {
                    updatedProviders.append(Client.codex.displayName)
                }
            } catch {
                errors.append("Codex: \(error.localizedDescription)")
            }
        } else if isInstalled(.codex, fileManager: fileManager) {
            errors.append("Codex: Aetower MCP helper is missing from the app bundle.")
        }

        return LocalMcpClientRegistrationReport(
            statuses: inspectStatuses(fileManager: fileManager),
            updatedProviders: updatedProviders,
            errors: errors
        )
    }

    static func manualSnippet(for providerId: String, fileManager: FileManager = .default) -> String? {
        guard Client(rawValue: providerId) != nil,
              let commandPath = bundledProxyCommandPath(fileManager: fileManager),
              let client = Client(rawValue: providerId)
        else {
            return nil
        }
        return configSnippet(for: client, commandPath: commandPath)
    }

    private static func status(
        for client: Client,
        commandPath: String?,
        fileManager: FileManager
    ) -> LocalMcpClientRegistrationStatus {
        let installed = isInstalled(client, fileManager: fileManager)
        guard installed else {
            return LocalMcpClientRegistrationStatus(
                id: client.rawValue,
                displayName: client.displayName,
                isInstalled: false,
                state: .notInstalled,
                detail: "\(client.displayName) is not installed locally.",
                configPath: client.configPath,
                supportsAutomaticRegistration: client.supportsAutomaticRegistration,
                manualSnippet: nil
            )
        }

        guard let commandPath else {
            return LocalMcpClientRegistrationStatus(
                id: client.rawValue,
                displayName: client.displayName,
                isInstalled: true,
                state: .unavailable,
                detail: "Aetower MCP registration requires the bundled `aetower-mcp` helper inside the app bundle.",
                configPath: client.configPath,
                supportsAutomaticRegistration: client.supportsAutomaticRegistration,
                manualSnippet: nil
            )
        }

        switch client {
        case .claude:
            do {
                let registration = try claudeRegistrationState(commandPath: commandPath)
                return LocalMcpClientRegistrationStatus(
                    id: client.rawValue,
                    displayName: client.displayName,
                    isInstalled: true,
                    state: registration,
                    detail: registration == .registered
                        ? "Claude is configured to launch Aetower's bundled MCP proxy."
                        : "Claude has a stable user-owned MCP config file and can be registered automatically.",
                    configPath: client.configPath,
                    supportsAutomaticRegistration: true,
                    manualSnippet: configSnippet(for: client, commandPath: commandPath)
                )
            } catch {
                return LocalMcpClientRegistrationStatus(
                    id: client.rawValue,
                    displayName: client.displayName,
                    isInstalled: true,
                    state: .unavailable,
                    detail: "Claude config could not be read: \(error.localizedDescription)",
                    configPath: client.configPath,
                    supportsAutomaticRegistration: true,
                    manualSnippet: configSnippet(for: client, commandPath: commandPath)
                )
            }
        case .codex:
            do {
                let registration = try codexRegistrationState(commandPath: commandPath)
                return LocalMcpClientRegistrationStatus(
                    id: client.rawValue,
                    displayName: client.displayName,
                    isInstalled: true,
                    state: registration,
                    detail: registration == .registered
                        ? "Codex is configured to launch Aetower's bundled MCP proxy via ~/.codex/config.toml."
                        : "Codex has a stable user-owned MCP config surface and can be registered automatically.",
                    configPath: client.configPath,
                    supportsAutomaticRegistration: true,
                    manualSnippet: configSnippet(for: client, commandPath: commandPath)
                )
            } catch {
                return LocalMcpClientRegistrationStatus(
                    id: client.rawValue,
                    displayName: client.displayName,
                    isInstalled: true,
                    state: .unavailable,
                    detail: "Codex config could not be read: \(error.localizedDescription)",
                    configPath: client.configPath,
                    supportsAutomaticRegistration: true,
                    manualSnippet: configSnippet(for: client, commandPath: commandPath)
                )
            }
        case .chatgpt:
            return LocalMcpClientRegistrationStatus(
                id: client.rawValue,
                displayName: client.displayName,
                isInstalled: true,
                state: .manualConfigurationRequired,
                detail: "\(client.displayName) is installed, but Aetower does not know a stable writable MCP config surface for it yet. Use the generic MCP snippet when that client exposes MCP settings.",
                configPath: client.configPath ?? client.supportMarkerPath,
                supportsAutomaticRegistration: false,
                manualSnippet: configSnippet(for: client, commandPath: commandPath)
            )
        }
    }

    private static func isInstalled(_ client: Client, fileManager: FileManager) -> Bool {
        if let markerPath = client.supportMarkerPath, fileManager.fileExists(atPath: markerPath) {
            return true
        }

        let applicationRoots = [
            "/Applications",
            homeRelativePath("Applications"),
        ]

        for root in applicationRoots {
            for bundleName in client.appBundleNames where fileManager.fileExists(atPath: root + "/" + bundleName) {
                return true
            }
        }

        return false
    }

    private static func bundledProxyCommandPath(fileManager: FileManager) -> String? {
        let helperPath = Bundle.main.bundleURL
            .appendingPathComponent("Contents/Helpers/aetower-mcp")
            .path
        return fileManager.isExecutableFile(atPath: helperPath) ? helperPath : nil
    }

    private static func upsertClaudeRegistration(commandPath: String, fileManager: FileManager) throws -> Bool {
        let path = try requiredConfigPath(for: .claude)
        let url = URL(fileURLWithPath: path)
        let parentURL = url.deletingLastPathComponent()
        try fileManager.createDirectory(at: parentURL, withIntermediateDirectories: true, attributes: nil)

        var root = try loadJsonObject(at: url)
        var mcpServers = root["mcpServers"] as? [String: Any] ?? [:]
        let existingCommand = (mcpServers["aetower"] as? [String: Any])?["command"] as? String
        if existingCommand == commandPath {
            return false
        }
        mcpServers["aetower"] = [
            "command": commandPath,
        ]
        root["mcpServers"] = mcpServers

        let data = try JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: url, options: .atomic)
        return true
    }

    private static func upsertCodexRegistration(commandPath: String, fileManager: FileManager) throws -> Bool {
        let path = try requiredConfigPath(for: .codex)
        let url = URL(fileURLWithPath: path)
        let parentURL = url.deletingLastPathComponent()
        try fileManager.createDirectory(at: parentURL, withIntermediateDirectories: true, attributes: nil)

        var content = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
        let sectionHeader = "[mcp_servers.aetower]"
        let desiredBlock = """

[mcp_servers.aetower]
command = "\(commandPath)"
"""

        if let sectionRange = content.range(of: sectionHeader) {
            let remainder = content[sectionRange.upperBound...]
            let nextSectionRange = remainder.range(of: "\n[")
            let blockEnd = nextSectionRange.map { $0.lowerBound } ?? content.endIndex
            let replacementRange = sectionRange.lowerBound..<blockEnd
            let existingBlock = String(content[replacementRange]).trimmingCharacters(in: .whitespacesAndNewlines)
            let normalizedDesired = desiredBlock.trimmingCharacters(in: .whitespacesAndNewlines)
            if existingBlock == normalizedDesired {
                return false
            }
            content.replaceSubrange(replacementRange, with: normalizedDesired)
        } else if let featuresRange = content.range(of: "\n[features]") {
            content.insert(contentsOf: desiredBlock + "\n", at: featuresRange.lowerBound)
        } else {
            if !content.isEmpty, !content.hasSuffix("\n") {
                content.append("\n")
            }
            content.append(desiredBlock)
            content.append("\n")
        }

        try content.write(to: url, atomically: true, encoding: .utf8)
        return true
    }

    private static func claudeRegistrationState(commandPath: String) throws -> LocalMcpClientRegistrationState {
        let path = try requiredConfigPath(for: .claude)
        let url = URL(fileURLWithPath: path)
        let root = try loadJsonObject(at: url)
        guard let mcpServers = root["mcpServers"] as? [String: Any],
              let aetower = mcpServers["aetower"] as? [String: Any],
              let configuredCommand = aetower["command"] as? String
        else {
            return .availableForAutomaticRegistration
        }
        return configuredCommand == commandPath ? .registered : .availableForAutomaticRegistration
    }

    private static func codexRegistrationState(commandPath: String) throws -> LocalMcpClientRegistrationState {
        let path = try requiredConfigPath(for: .codex)
        let url = URL(fileURLWithPath: path)
        guard FileManager.default.fileExists(atPath: url.path) else {
            return .availableForAutomaticRegistration
        }
        let content = try String(contentsOf: url, encoding: .utf8)
        guard let sectionRange = content.range(of: "[mcp_servers.aetower]") else {
            return .availableForAutomaticRegistration
        }
        let remainder = content[sectionRange.upperBound...]
        let nextSectionRange = remainder.range(of: "\n[")
        let blockEnd = nextSectionRange.map { $0.lowerBound } ?? content.endIndex
        let block = String(content[sectionRange.lowerBound..<blockEnd])
        let expectedLine = "command = \"\(commandPath)\""
        return block.contains(expectedLine) ? .registered : .availableForAutomaticRegistration
    }

    private static func loadJsonObject(at url: URL) throws -> [String: Any] {
        guard FileManager.default.fileExists(atPath: url.path) else {
            return [:]
        }
        let data = try Data(contentsOf: url)
        if data.trimmingAsciiWhitespaceAndNewlines().isEmpty {
            return [:]
        }
        let object = try JSONSerialization.jsonObject(with: data)
        guard let dictionary = object as? [String: Any] else {
            throw RegistrationError.invalidJsonRoot(url.path)
        }
        return dictionary
    }

    private static func requiredConfigPath(for client: Client) throws -> String {
        guard let path = client.configPath else {
            throw RegistrationError.missingConfigPath(client.displayName)
        }
        return path
    }

    private static func configSnippet(for client: Client, commandPath: String) -> String {
        switch client {
        case .claude, .chatgpt:
            let payload: [String: Any] = [
                "mcpServers": [
                    "aetower": [
                        "command": commandPath,
                    ],
                ],
            ]
            let data = try? JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys])
            return String(data: data ?? Data(), encoding: .utf8) ?? ""
        case .codex:
            return """
            [mcp_servers.aetower]
            command = "\(commandPath)"
            """
        }
    }

    private static func homeRelativePath(_ suffix: String) -> String {
        URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(suffix).path
    }

    private enum RegistrationError: LocalizedError {
        case invalidJsonRoot(String)
        case missingConfigPath(String)

        var errorDescription: String? {
            switch self {
            case .invalidJsonRoot(let path):
                return "Expected a JSON object at \(path)."
            case .missingConfigPath(let displayName):
                return "No config path is known for \(displayName)."
            }
        }
    }
}

private extension Data {
    func trimmingAsciiWhitespaceAndNewlines() -> Data {
        let allowed = CharacterSet.whitespacesAndNewlines.inverted
        guard let string = String(data: self, encoding: .utf8),
              let range = string.rangeOfCharacter(from: allowed)
        else {
            return Data()
        }
        let trimmed = string[range.lowerBound...]
        guard let lastRange = trimmed.rangeOfCharacter(from: allowed, options: .backwards) else {
            return Data()
        }
        return Data(trimmed[...lastRange.lowerBound].utf8)
    }
}
