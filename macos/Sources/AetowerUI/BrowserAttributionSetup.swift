import AppKit
import Foundation

struct BrowserAttributionEndpointSummary: Equatable, Sendable {
    let endpoint: String
    let pageTargetCount: Int
    let totalTargetCount: Int
}

enum BrowserAttributionSetupError: Error, LocalizedError, Equatable, Sendable {
    case chromeNotFound
    case invalidEndpoint(String)
    case invalidResponse(String)
    case launchFailed(String)
    case endpointUnavailable(String)
    case endpointDidNotBecomeReady(String)

    var errorDescription: String? {
        switch self {
        case .chromeNotFound:
            return "Google Chrome was not found in Applications."
        case .invalidEndpoint(let endpoint):
            return "Use a full http:// or https:// browser debug endpoint. Current value: \(endpoint)"
        case .invalidResponse(let detail):
            return "The endpoint responded, but it did not look like Chromium debug JSON. \(detail)"
        case .launchFailed(let detail):
            return "Chrome could not be opened. \(detail)"
        case .endpointUnavailable(let detail):
            return "The browser debug endpoint is not reachable. \(detail)"
        case .endpointDidNotBecomeReady(let detail):
            return "Chrome opened, but the debug endpoint did not become ready. \(detail)"
        }
    }
}

enum BrowserAttributionSetup {
    static let defaultPort = 9222
    static let defaultEndpoint = "http://127.0.0.1:\(defaultPort)/json/list"
    private static let requestTimeoutSeconds = 2.0
    private static let launchReadinessTimeoutNanoseconds: UInt64 = 6_000_000_000
    private static let launchReadinessPollNanoseconds: UInt64 = 350_000_000

    static var dedicatedProfileDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first ?? FileManager.default.homeDirectoryForCurrentUser
        return base
            .appendingPathComponent("Aetower", isDirectory: true)
            .appendingPathComponent("BrowserProfiles", isDirectory: true)
            .appendingPathComponent("ChromeDebug", isDirectory: true)
    }

    static var dedicatedProfileDisplayPath: String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let path = dedicatedProfileDirectory.path
        if path.hasPrefix(home) {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }

    static func launchArguments(profileDirectory: URL = dedicatedProfileDirectory) -> [String] {
        [
            "--remote-debugging-port=\(defaultPort)",
            "--user-data-dir=\(profileDirectory.path)",
            "--no-first-run",
            "--new-window",
            "about:blank",
        ]
    }

    @MainActor
    static func enableDedicatedChrome() async throws -> BrowserAttributionEndpointSummary {
        do {
            return try await probeEndpoint(defaultEndpoint)
        } catch BrowserAttributionSetupError.endpointUnavailable {
            // Expected when the dedicated browser has not been launched yet.
        } catch BrowserAttributionSetupError.invalidResponse(let detail) {
            throw BrowserAttributionSetupError.invalidResponse(
                "Port \(defaultPort) is already in use, but it is not exposing /json/list. \(detail)"
            )
        } catch {
            throw error
        }

        let executableURL = try chromeExecutableURL()
        do {
            try FileManager.default.createDirectory(
                at: dedicatedProfileDirectory,
                withIntermediateDirectories: true
            )
        } catch {
            throw BrowserAttributionSetupError.launchFailed(
                "Could not create \(dedicatedProfileDisplayPath): \(error.localizedDescription)"
            )
        }

        let process = Process()
        process.executableURL = executableURL
        process.arguments = launchArguments()
        do {
            try process.run()
        } catch {
            throw BrowserAttributionSetupError.launchFailed(error.localizedDescription)
        }

        return try await waitForEndpoint(defaultEndpoint)
    }

    static func probeEndpoint(_ endpoint: String) async throws -> BrowserAttributionEndpointSummary {
        guard let url = URL(string: endpoint),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else {
            throw BrowserAttributionSetupError.invalidEndpoint(endpoint)
        }

        var request = URLRequest(url: url)
        request.timeoutInterval = requestTimeoutSeconds
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            if let httpResponse = response as? HTTPURLResponse,
               !(200..<300).contains(httpResponse.statusCode) {
                throw BrowserAttributionSetupError.invalidResponse(
                    "HTTP \(httpResponse.statusCode) from \(endpoint)."
                )
            }
            return try parseTargetSummary(from: data, endpoint: endpoint)
        } catch let setupError as BrowserAttributionSetupError {
            throw setupError
        } catch {
            throw BrowserAttributionSetupError.endpointUnavailable(error.localizedDescription)
        }
    }

    static func parseTargetSummary(
        from data: Data,
        endpoint: String
    ) throws -> BrowserAttributionEndpointSummary {
        do {
            let targets = try JSONDecoder().decode([ChromiumDebugTarget].self, from: data)
            let pageCount = targets.filter { $0.targetType == "page" }.count
            return BrowserAttributionEndpointSummary(
                endpoint: endpoint,
                pageTargetCount: pageCount,
                totalTargetCount: targets.count
            )
        } catch {
            throw BrowserAttributionSetupError.invalidResponse(error.localizedDescription)
        }
    }

    @MainActor
    private static func chromeExecutableURL() throws -> URL {
        let bundleCandidates = [
            "com.google.Chrome",
            "com.google.Chrome.beta",
            "com.google.Chrome.dev",
            "com.google.Chrome.canary",
        ]
        for bundleID in bundleCandidates {
            if let appURL = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID),
               let executableURL = executableURL(in: appURL) {
                return executableURL
            }
        }

        let fallbackAppPaths = [
            "/Applications/Google Chrome.app",
            FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Applications/Google Chrome.app")
                .path,
        ]
        for path in fallbackAppPaths {
            if let executableURL = executableURL(in: URL(fileURLWithPath: path)) {
                return executableURL
            }
        }
        throw BrowserAttributionSetupError.chromeNotFound
    }

    private static func executableURL(in appURL: URL) -> URL? {
        let candidates = [
            "Google Chrome",
            "Google Chrome Beta",
            "Google Chrome Dev",
            "Google Chrome Canary",
        ]
        for name in candidates {
            let url = appURL
                .appendingPathComponent("Contents", isDirectory: true)
                .appendingPathComponent("MacOS", isDirectory: true)
                .appendingPathComponent(name)
            if FileManager.default.isExecutableFile(atPath: url.path) {
                return url
            }
        }
        return nil
    }

    private static func waitForEndpoint(_ endpoint: String) async throws -> BrowserAttributionEndpointSummary {
        let started = DispatchTime.now().uptimeNanoseconds
        var lastError: Error?
        while DispatchTime.now().uptimeNanoseconds - started < launchReadinessTimeoutNanoseconds {
            try? await Task.sleep(nanoseconds: launchReadinessPollNanoseconds)
            do {
                return try await probeEndpoint(endpoint)
            } catch {
                lastError = error
            }
        }
        throw BrowserAttributionSetupError.endpointDidNotBecomeReady(
            lastError?.localizedDescription ?? "Timed out waiting for \(endpoint)."
        )
    }
}

private struct ChromiumDebugTarget: Decodable {
    let targetType: String?

    private enum CodingKeys: String, CodingKey {
        case targetType = "type"
    }
}
