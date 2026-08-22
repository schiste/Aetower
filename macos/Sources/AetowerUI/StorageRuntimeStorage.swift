import Foundation

/// Runtime-owned storage cannot be represented as a filesystem path. Docker
/// owns the metadata and must be asked before any cleanup is attempted.
struct StorageRuntimeInventory: Equatable, Sendable {
    enum Availability: Equatable, Sendable {
        case unavailable(String)
        case available
    }

    let capturedAt: Date
    let availability: Availability
    let activeContainerCount: Int
    let totalImageCount: Int
    let danglingVolumeCount: Int
    let anonymousDanglingVolumeCount: Int
    let staleDanglingVolumeCount: Int
    let oldestDanglingVolumeAgeDays: Int?
    let safeReclaimableBytes: UInt64?
    let aggressiveReclaimableBytes: UInt64?
    let detail: String

    var isAvailable: Bool {
        if case .available = availability {
            return true
        }
        return false
    }
}

struct StorageRuntimeCleanupResult: Sendable {
    let succeeded: Bool
    let output: String
    let estimatedBytes: UInt64?
    let duration: TimeInterval
    let postCleanupInventory: StorageRuntimeInventory?
}

enum StorageRuntimeCleanupScope: Sendable {
    case safe
    case aggressive
}

enum StorageRuntimeStorageService {
    private static let limaHome = NSHomeDirectory() + "/.colima/_lima"
    private static let commandTimeout: TimeInterval = 8

    static func probe() async -> StorageRuntimeInventory {
        await Task.detached(priority: .utility) {
            probeSynchronously()
        }.value
    }

    static func clean(scope: StorageRuntimeCleanupScope) async -> StorageRuntimeCleanupResult {
        await Task.detached(priority: .utility) {
            cleanSynchronously(scope: scope)
        }.value
    }

    private static func probeSynchronously() -> StorageRuntimeInventory {
        let capturedAt = Date()
        guard FileManager.default.fileExists(atPath: limaHome) else {
            return unavailable(capturedAt, "Colima is not installed or has no Lima data directory.")
        }

        guard let counts = runDocker(arguments: ["volume", "ls", "-q", "--filter", "dangling=true"]),
              let containers = runDocker(arguments: ["ps", "-q"]),
              let images = runDocker(arguments: ["image", "ls", "-q"])
        else {
            return unavailable(capturedAt, "Docker/Colima did not answer within the storage probe timeout.")
        }

        let danglingNames = nonEmptyLines(counts.stdout)
        let anonymous = runDocker(arguments: [
            "volume", "ls", "-q", "--filter", "dangling=true",
            "--filter", "label=com.docker.volume.anonymous",
        ]).map { nonEmptyLines($0.stdout).count } ?? 0
        let df = runDocker(arguments: ["system", "df"])?.stdout
        let estimates = parseReclaimableBytes(df)
        let aging = inspectDanglingVolumes(names: danglingNames)
        return StorageRuntimeInventory(
            capturedAt: capturedAt,
            availability: .available,
            activeContainerCount: nonEmptyLines(containers.stdout).count,
            totalImageCount: Set(nonEmptyLines(images.stdout)).count,
            danglingVolumeCount: danglingNames.count,
            anonymousDanglingVolumeCount: anonymous,
            staleDanglingVolumeCount: aging.staleCount,
            oldestDanglingVolumeAgeDays: aging.oldestAgeDays,
            safeReclaimableBytes: estimates.safe,
            aggressiveReclaimableBytes: estimates.aggressive,
            detail: "Runtime inventory is live; filesystem indexing is not used for Docker-owned data."
        )
    }

    private static func cleanSynchronously(scope: StorageRuntimeCleanupScope) -> StorageRuntimeCleanupResult {
        let started = Date()
        var outputs: [String] = []
        var succeeded = true
        var estimated: UInt64?

        if let before = runDocker(arguments: ["system", "df"])?.stdout {
            let parsed = parseReclaimableBytes(before)
            estimated = switch scope {
            case .safe: parsed.safe
            case .aggressive: parsed.aggressive
            }
        }

        if case .aggressive = scope {
            guard let result = runDocker(arguments: ["volume", "prune", "--force"]) else {
                return StorageRuntimeCleanupResult(succeeded: false, output: "Docker volume prune timed out.", estimatedBytes: estimated, duration: Date().timeIntervalSince(started), postCleanupInventory: nil)
            }
            outputs.append(result.output)
            succeeded = succeeded && result.exitCode == 0
        }

        guard let result = runDocker(arguments: ["system", "prune", "--all", "--force"]) else {
            return StorageRuntimeCleanupResult(succeeded: false, output: outputs.joined(separator: "\n"), estimatedBytes: estimated, duration: Date().timeIntervalSince(started), postCleanupInventory: nil)
        }
        outputs.append(result.output)
        succeeded = succeeded && result.exitCode == 0

        // TRIM is best-effort: Docker deletion is still a valid success, but
        // the UI must distinguish logical deletion from host free-space gain.
        if let trim = runColima(arguments: ["sudo", "-n", "fstrim", "--all", "--verbose"]) {
            outputs.append(trim.output)
        }
        let postCleanupInventory = probeSynchronously()
        outputs.append("Verified remaining dangling volumes: \(postCleanupInventory.danglingVolumeCount).")
        return StorageRuntimeCleanupResult(
            succeeded: succeeded,
            output: outputs.joined(separator: "\n"),
            estimatedBytes: estimated,
            duration: Date().timeIntervalSince(started),
            postCleanupInventory: postCleanupInventory
        )
    }

    private static func unavailable(_ date: Date, _ detail: String) -> StorageRuntimeInventory {
        StorageRuntimeInventory(
            capturedAt: date,
            availability: .unavailable(detail),
            activeContainerCount: 0,
            totalImageCount: 0,
            danglingVolumeCount: 0,
            anonymousDanglingVolumeCount: 0,
            staleDanglingVolumeCount: 0,
            oldestDanglingVolumeAgeDays: nil,
            safeReclaimableBytes: nil,
            aggressiveReclaimableBytes: nil,
            detail: detail
        )
    }

    private struct CommandResult {
        let exitCode: Int32
        let stdout: String
        let output: String
    }

    private static func runDocker(arguments: [String]) -> CommandResult? {
        run(arguments: ["LIMA_HOME=\(limaHome)", "limactl", "shell", "colima", "docker"] + arguments)
    }

    private static func runColima(arguments: [String]) -> CommandResult? {
        run(arguments: ["LIMA_HOME=\(limaHome)", "limactl", "shell", "colima"] + arguments)
    }

    private static func run(arguments: [String]) -> CommandResult? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = arguments
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        do { try process.run() } catch { return nil }

        let deadline = Date().addingTimeInterval(commandTimeout)
        while process.isRunning, Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }
        if process.isRunning {
            process.terminate()
            return nil
        }
        let out = String(decoding: stdout.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        let err = String(decoding: stderr.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        return CommandResult(exitCode: process.terminationStatus, stdout: out, output: [out, err].filter { !$0.isEmpty }.joined(separator: "\n"))
    }

    private static func nonEmptyLines(_ value: String) -> [String] {
        value.split(whereSeparator: { $0.isNewline }).map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
    }

    private static func inspectDanglingVolumes(names: [String]) -> (staleCount: Int, oldestAgeDays: Int?) {
        guard !names.isEmpty,
              let result = runDocker(arguments: ["volume", "inspect", "--format", "{{.Name}}\t{{.CreatedAt}}"] + names),
              result.exitCode == 0
        else { return (0, nil) }
        let now = Date()
        let dates = nonEmptyLines(result.stdout).compactMap { line -> Date? in
            let parts = line.split(separator: "\t", maxSplits: 1).map(String.init)
            guard parts.count == 2 else { return nil }
            return parseDockerDate(parts[1])
        }
        let ages = dates.map { max(0, Int(now.timeIntervalSince($0) / 86400)) }
        return (ages.filter { $0 >= 90 }.count, ages.max())
    }

    private static func parseDockerDate(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        if let date = formatter.date(from: value) {
            return date
        }
        let fallback = DateFormatter()
        fallback.locale = Locale(identifier: "en_US_POSIX")
        fallback.timeZone = TimeZone(secondsFromGMT: 0)
        fallback.dateFormat = "yyyy-MM-dd HH:mm:ss ZZZ"
        return fallback.date(from: value)
    }

    private static func parseReclaimableBytes(_ value: String?) -> (safe: UInt64?, aggressive: UInt64?) {
        guard let value else { return (nil, nil) }
        let values = value.split(whereSeparator: { $0.isNewline }).compactMap { line -> UInt64? in
            guard line.contains("reclaimable") || line.contains("(0%)") else { return nil }
            let tokens = line.split(whereSeparator: { $0.isWhitespace })
            guard let token = tokens.dropFirst(3).first(where: { $0.contains("B") }) else { return nil }
            return parseByteToken(String(token))
        }
        let total = values.reduce(UInt64(0), &+)
        return (total > 0 ? total : nil, total > 0 ? total : nil)
    }

    private static func parseByteToken(_ token: String) -> UInt64? {
        let normalized = token.replacingOccurrences(of: ",", with: "").uppercased()
        let suffixes: [(String, Double)] = [("TB", 1e12), ("GB", 1e9), ("MB", 1e6), ("KB", 1e3), ("B", 1)]
        for (suffix, multiplier) in suffixes where normalized.hasSuffix(suffix) {
            guard let number = Double(normalized.dropLast(suffix.count)) else { return nil }
            return UInt64(number * multiplier)
        }
        return nil
    }
}
