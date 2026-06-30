import CryptoKit
import Foundation

struct AethymeAgentContractKitInstall: Sendable, Equatable {
    let kitID: String
    let version: String
    let relativePath: String
    let absolutePath: String
    let fingerprint: String
    let installed: Bool
    let excludeUpdated: Bool

    var promptContext: AgentContractKitPromptContext {
        AgentContractKitPromptContext(
            kitID: kitID,
            version: version,
            relativePath: relativePath,
            absolutePath: absolutePath,
            fingerprint: fingerprint
        )
    }
}

enum AethymeAgentContractKitInstaller {
    static let kitID = "agent-contracts"
    static let kitVersion = "v1"
    static let preferredRelativePath = ".aethyme/agent-contracts/v1"

    private static let aethymeExcludeLine = ".aethyme/"

    static func install(
        in repositoryRoot: String,
        now: Date = Date(),
        fileManager: FileManager = .default
    ) throws -> AethymeAgentContractKitInstall {
        let rootURL = URL(fileURLWithPath: repositoryRoot, isDirectory: true)
            .standardizedFileURL
        var isDirectory = ObjCBool(false)
        guard fileManager.fileExists(atPath: rootURL.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw AethymeAgentContractKitError.repositoryMissing(repositoryRoot)
        }

        let sourceURL = try bundledKitURL()
        let fingerprint = try directoryFingerprint(sourceURL, fileManager: fileManager)
        let preferredDestination = rootURL.appendingPathComponent(preferredRelativePath, isDirectory: true)
        let destination = try resolvedDestination(
            preferredDestination: preferredDestination,
            fingerprint: fingerprint,
            fileManager: fileManager
        )
        let installed = try copyKitIfNeeded(
            from: sourceURL,
            to: destination,
            fingerprint: fingerprint,
            fileManager: fileManager
        )
        let relativePath = relativePath(from: rootURL, to: destination)
        try writeManifest(
            rootURL: rootURL,
            kitRelativePath: relativePath,
            fingerprint: fingerprint,
            now: now,
            fileManager: fileManager
        )
        let excludeUpdated = try ensureAethymeExcluded(
            rootURL: rootURL,
            fileManager: fileManager
        )

        return AethymeAgentContractKitInstall(
            kitID: kitID,
            version: kitVersion,
            relativePath: relativePath,
            absolutePath: destination.path,
            fingerprint: fingerprint,
            installed: installed,
            excludeUpdated: excludeUpdated
        )
    }

    static func requiredRelativePaths(
        forContractID contractID: String,
        contractPath: String,
        install: AethymeAgentContractKitInstall
    ) -> [String] {
        var paths = [
            ".aethyme/manifest.json",
            "\(install.relativePath)/README.md",
            "\(install.relativePath)/kit.json",
            "\(install.relativePath)/templates/\(templateFilename(contractID: contractID, contractPath: contractPath))",
        ]
        if let schemaFilename = schemaFilename(contractID: contractID) {
            paths.append("\(install.relativePath)/schemas/\(schemaFilename)")
        }
        return paths
    }

    static func requiredBaseRelativePaths(
        install: AethymeAgentContractKitInstall
    ) -> [String] {
        [
            ".aethyme/manifest.json",
            "\(install.relativePath)/README.md",
            "\(install.relativePath)/kit.json",
        ]
    }

    static func waitForInstalledFiles(
        in repositoryRoot: String,
        relativePaths: [String],
        timeoutSeconds: TimeInterval = 2,
        fileManager: FileManager = .default
    ) throws {
        let rootURL = URL(fileURLWithPath: repositoryRoot, isDirectory: true)
            .standardizedFileURL
        let deadline = Date().addingTimeInterval(timeoutSeconds)
        var missing = missingReadableFiles(
            rootURL: rootURL,
            relativePaths: relativePaths,
            fileManager: fileManager
        )
        while !missing.isEmpty, Date() < deadline {
            Thread.sleep(forTimeInterval: 0.05)
            missing = missingReadableFiles(
                rootURL: rootURL,
                relativePaths: relativePaths,
                fileManager: fileManager
            )
        }
        guard missing.isEmpty else {
            throw AethymeAgentContractKitError.installVerificationFailed(missing.joined(separator: ", "))
        }
    }

    private static func bundledKitURL() throws -> URL {
        if let url = Bundle.module.url(
            forResource: "v1",
            withExtension: nil,
            subdirectory: "Aethyme/agent-contracts"
        ) {
            return url
        }
        throw AethymeAgentContractKitError.resourceMissing("Aethyme/agent-contracts/v1")
    }

    private static func resolvedDestination(
        preferredDestination: URL,
        fingerprint: String,
        fileManager: FileManager
    ) throws -> URL {
        guard fileManager.fileExists(atPath: preferredDestination.path) else {
            return preferredDestination
        }
        if try directoryFingerprint(preferredDestination, fileManager: fileManager) == fingerprint {
            return preferredDestination
        }

        let suffix = String(fingerprint.prefix(12))
        let alternate = preferredDestination
            .deletingLastPathComponent()
            .appendingPathComponent("\(kitVersion)-\(suffix)", isDirectory: true)
        guard fileManager.fileExists(atPath: alternate.path) else {
            return alternate
        }
        if try directoryFingerprint(alternate, fileManager: fileManager) == fingerprint {
            return alternate
        }
        throw AethymeAgentContractKitError.destinationConflict(alternate.path)
    }

    private static func copyKitIfNeeded(
        from sourceURL: URL,
        to destinationURL: URL,
        fingerprint: String,
        fileManager: FileManager
    ) throws -> Bool {
        if fileManager.fileExists(atPath: destinationURL.path) {
            guard try directoryFingerprint(destinationURL, fileManager: fileManager) == fingerprint else {
                throw AethymeAgentContractKitError.destinationConflict(destinationURL.path)
            }
            return false
        }

        try fileManager.createDirectory(
            at: destinationURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try fileManager.copyItem(at: sourceURL, to: destinationURL)
        return true
    }

    private static func writeManifest(
        rootURL: URL,
        kitRelativePath: String,
        fingerprint: String,
        now: Date,
        fileManager: FileManager
    ) throws {
        let aethymeURL = rootURL.appendingPathComponent(".aethyme", isDirectory: true)
        try fileManager.createDirectory(at: aethymeURL, withIntermediateDirectories: true)
        let manifestURL = aethymeURL.appendingPathComponent("manifest.json")
        let timestamp = isoTimestamp(now)

        let existingManifest: AethymeManifest
        if let data = try? Data(contentsOf: manifestURL),
           let decoded = try? JSONDecoder().decode(AethymeManifest.self, from: data) {
            existingManifest = decoded
        } else {
            existingManifest = AethymeManifest(version: 1, updated_at: timestamp, kits: [:])
        }

        var manifest = existingManifest
        manifest.updated_at = timestamp
        manifest.kits[kitID] = AethymeManifestKit(
            version: kitVersion,
            path: kitRelativePath,
            fingerprint: fingerprint,
            installed_at: timestamp,
            source: "Aethyme bundled resources"
        )

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(manifest)
        try data.write(to: manifestURL, options: .atomic)
    }

    private static func ensureAethymeExcluded(
        rootURL: URL,
        fileManager: FileManager
    ) throws -> Bool {
        guard let gitDirectory = gitMetadataDirectory(for: rootURL, fileManager: fileManager) else {
            return false
        }

        let infoURL = gitDirectory.appendingPathComponent("info", isDirectory: true)
        try fileManager.createDirectory(at: infoURL, withIntermediateDirectories: true)
        let excludeURL = infoURL.appendingPathComponent("exclude")
        let existing = (try? String(contentsOf: excludeURL, encoding: .utf8)) ?? ""
        let hasLine = existing
            .split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .contains { $0 == ".aethyme" || $0 == aethymeExcludeLine }
        guard !hasLine else {
            return false
        }

        var updated = existing
        if !updated.isEmpty, !updated.hasSuffix("\n") {
            updated.append("\n")
        }
        updated.append("# Aethyme local repo intelligence/cache\n")
        updated.append("\(aethymeExcludeLine)\n")
        try updated.write(to: excludeURL, atomically: true, encoding: .utf8)
        return true
    }

    private static func gitMetadataDirectory(
        for rootURL: URL,
        fileManager: FileManager
    ) -> URL? {
        let dotGitURL = rootURL.appendingPathComponent(".git")
        var isDirectory = ObjCBool(false)
        guard fileManager.fileExists(atPath: dotGitURL.path, isDirectory: &isDirectory) else {
            return nil
        }
        if isDirectory.boolValue {
            return dotGitURL
        }

        guard let content = try? String(contentsOf: dotGitURL, encoding: .utf8) else {
            return nil
        }
        let trimmed = content.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let gitDir = trimmed.stripPrefix("gitdir:")?.trimmingCharacters(in: .whitespaces),
              !gitDir.isEmpty else {
            return nil
        }
        if gitDir.hasPrefix("/") {
            return URL(fileURLWithPath: gitDir, isDirectory: true).standardizedFileURL
        }
        return rootURL.appendingPathComponent(gitDir, isDirectory: true).standardizedFileURL
    }

    private static func directoryFingerprint(
        _ directoryURL: URL,
        fileManager: FileManager
    ) throws -> String {
        var isDirectory = ObjCBool(false)
        guard fileManager.fileExists(atPath: directoryURL.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw AethymeAgentContractKitError.resourceMissing(directoryURL.path)
        }

        let urls = try fileURLs(in: directoryURL, fileManager: fileManager)
        var hasher = SHA256()
        for url in urls {
            let relative = relativePath(from: directoryURL, to: url)
            hasher.update(data: Data(relative.utf8))
            hasher.update(data: Data([0]))
            hasher.update(data: try Data(contentsOf: url))
            hasher.update(data: Data([0]))
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func fileURLs(
        in directoryURL: URL,
        fileManager: FileManager
    ) throws -> [URL] {
        guard let enumerator = fileManager.enumerator(
            at: directoryURL,
            includingPropertiesForKeys: [.isRegularFileKey],
            options: []
        ) else {
            return []
        }
        var urls: [URL] = []
        for case let url as URL in enumerator {
            let values = try url.resourceValues(forKeys: [.isRegularFileKey])
            if values.isRegularFile == true {
                urls.append(url)
            }
        }
        return urls.sorted { relativePath(from: directoryURL, to: $0) < relativePath(from: directoryURL, to: $1) }
    }

    private static func missingReadableFiles(
        rootURL: URL,
        relativePaths: [String],
        fileManager: FileManager
    ) -> [String] {
        relativePaths.filter { relativePath in
            let url = rootURL.appendingPathComponent(relativePath, isDirectory: false)
            var isDirectory = ObjCBool(false)
            guard fileManager.fileExists(atPath: url.path, isDirectory: &isDirectory),
                  !isDirectory.boolValue,
                  fileManager.isReadableFile(atPath: url.path) else {
                return true
            }
            return false
        }
    }

    private static func templateFilename(contractID: String, contractPath: String) -> String {
        switch contractID {
        case "agents_md":
            return "AGENTS.md"
        case "repo_map":
            return "repo-map.yaml"
        default:
            return contractPath.split(separator: "/").last.map(String.init) ?? "\(contractID).yaml"
        }
    }

    private static func schemaFilename(contractID: String) -> String? {
        switch contractID {
        case "manifest":
            return "manifest.schema.json"
        case "tasks":
            return "tasks.schema.json"
        case "repo_map":
            return "repo-map.schema.json"
        case "contracts":
            return "contracts.schema.json"
        case "commands":
            return "commands.schema.json"
        case "validation":
            return "validation.schema.json"
        case "boundaries":
            return "boundaries.schema.json"
        case "risks":
            return "risks.schema.json"
        case "references":
            return "references.schema.json"
        default:
            return nil
        }
    }

    private static func relativePath(from baseURL: URL, to url: URL) -> String {
        let basePath = baseURL.standardizedFileURL.path
        let path = url.standardizedFileURL.path
        guard path.hasPrefix(basePath) else {
            return path
        }
        let start = path.index(path.startIndex, offsetBy: basePath.count)
        return String(path[start...]).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    private static func isoTimestamp(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.string(from: date)
    }
}

private struct AethymeManifest: Codable {
    var version: Int
    var updated_at: String
    var kits: [String: AethymeManifestKit]
}

private struct AethymeManifestKit: Codable {
    var version: String
    var path: String
    var fingerprint: String
    var installed_at: String
    var source: String
}

enum AethymeAgentContractKitError: LocalizedError, Equatable {
    case repositoryMissing(String)
    case resourceMissing(String)
    case destinationConflict(String)
    case installVerificationFailed(String)

    var errorDescription: String? {
        switch self {
        case let .repositoryMissing(path):
            return "Aethyme kit install failed: repository root does not exist at \(path)."
        case let .resourceMissing(path):
            return "Aethyme kit install failed: bundled resource is missing at \(path)."
        case let .destinationConflict(path):
            return "Aethyme kit install failed: local kit path differs from bundled resources at \(path)."
        case let .installVerificationFailed(paths):
            return "Aethyme kit install failed: copied files were not readable before launch: \(paths)."
        }
    }
}

private extension String {
    func stripPrefix(_ prefix: String) -> String? {
        guard hasPrefix(prefix) else {
            return nil
        }
        return String(dropFirst(prefix.count))
    }
}
