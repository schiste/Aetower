import Foundation

struct BrowserAutomationTabSnapshot: Codable, Equatable, Sendable {
    let browserBundleID: String
    let browserName: String
    let title: String
    let url: String
    let windowIndex: UInt32
    let tabIndex: UInt32
    let active: Bool
    let source: String
}

struct BrowserAutomationCollectionSummary: Equatable, Sendable {
    let running: Bool
    let tabs: [BrowserAutomationTabSnapshot]
    let capturedAtMillis: UInt64
    let errorMessage: String?

    var tabCount: Int {
        tabs.count
    }

    var runningBrowserNames: [String] {
        Array(Set(tabs.map(\.browserName))).sorted()
    }
}

enum BrowserTabAutomationError: Error, LocalizedError, Equatable, Sendable {
    case osascriptUnavailable
    case timedOut
    case commandFailed(String)
    case invalidOutput(String)

    var errorDescription: String? {
        switch self {
        case .osascriptUnavailable:
            return "macOS osascript is not available, so Aetower cannot request Chrome tab metadata."
        case .timedOut:
            return "Chrome tab discovery timed out. Try again after Chrome finishes responding."
        case .commandFailed(let detail):
            return detail
        case .invalidOutput(let detail):
            return "Chrome tab discovery returned data Aetower could not read. \(detail)"
        }
    }
}

enum BrowserTabAutomation {
    static let source = "apple-automation"
    static let maximumTabs = 50
    private static let timeoutSeconds: TimeInterval = 2.0

    static func collectCurrentChromeTabs(
        timeoutSeconds: TimeInterval = Self.timeoutSeconds
    ) throws -> BrowserAutomationCollectionSummary {
        guard FileManager.default.isExecutableFile(atPath: "/usr/bin/osascript") else {
            throw BrowserTabAutomationError.osascriptUnavailable
        }

        let process = Process()
        let stdout = Pipe()
        let stderr = Pipe()
        let completed = DispatchSemaphore(value: 0)

        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-l", "JavaScript", "-e", scriptSource]
        process.standardOutput = stdout
        process.standardError = stderr
        process.terminationHandler = { _ in completed.signal() }

        do {
            try process.run()
        } catch {
            throw BrowserTabAutomationError.commandFailed(error.localizedDescription)
        }

        let timeoutMillis = Int(timeoutSeconds * 1_000)
        if completed.wait(timeout: .now() + .milliseconds(timeoutMillis)) == .timedOut {
            process.terminate()
            _ = completed.wait(timeout: .now() + .milliseconds(250))
            throw BrowserTabAutomationError.timedOut
        }

        let outputData = stdout.fileHandleForReading.readDataToEndOfFile()
        let errorData = stderr.fileHandleForReading.readDataToEndOfFile()
        let errorText = String(data: errorData, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""

        guard process.terminationStatus == 0 else {
            throw BrowserTabAutomationError.commandFailed(
                friendlyFailureMessage(errorText.isEmpty ? "osascript exited with \(process.terminationStatus)." : errorText)
            )
        }

        do {
            return try parseCollectionOutput(
                outputData,
                capturedAtMillis: UInt64(Date().timeIntervalSince1970 * 1000)
            )
        } catch {
            let output = String(data: outputData, encoding: .utf8) ?? ""
            throw BrowserTabAutomationError.invalidOutput(output.isEmpty ? error.localizedDescription : output)
        }
    }

    static func parseCollectionOutput(
        _ data: Data,
        capturedAtMillis: UInt64
    ) throws -> BrowserAutomationCollectionSummary {
        let decoded = try JSONDecoder().decode(BrowserAutomationScriptOutput.self, from: data)
        let errorMessage = decoded.error
            .map(friendlyFailureMessage)
            .flatMap { $0.isEmpty ? nil : $0 }
        return BrowserAutomationCollectionSummary(
            running: decoded.running,
            tabs: Array(decoded.tabs.prefix(maximumTabs)),
            capturedAtMillis: capturedAtMillis,
            errorMessage: errorMessage
        )
    }

    private static func friendlyFailureMessage(_ raw: String) -> String {
        let normalized = raw.lowercased()
        if normalized.contains("-1743")
            || normalized.contains("not authorized")
            || normalized.contains("not permitted")
            || normalized.contains("automation")
        {
            return "macOS has not granted Aetower permission to read Chrome tabs. Click Connect Current Chrome, then allow Google Chrome automation when prompted."
        }
        return raw
    }

    private static let scriptSource = """
    const maximumTabs = \(maximumTabs);
    const maximumTitleLength = 180;
    const maximumURLLength = 700;
    const targets = [
      { name: "Google Chrome", bundleID: "com.google.Chrome" },
      { name: "Google Chrome Beta", bundleID: "com.google.Chrome.beta" },
      { name: "Google Chrome Dev", bundleID: "com.google.Chrome.dev" },
      { name: "Google Chrome Canary", bundleID: "com.google.Chrome.canary" }
    ];

    function clean(value, limit) {
      if (value === undefined || value === null) return "";
      return String(value).replace(/[\\u0000-\\u001f\\u007f]/g, " ").slice(0, limit);
    }

    const result = { running: false, tabs: [], error: null };
    let done = false;
    for (const target of targets) {
      if (done) break;
      let browser;
      try {
        browser = Application(target.name);
        if (!browser.running()) continue;
      } catch (_) {
        continue;
      }

      result.running = true;
      try {
        const windows = browser.windows();
        for (let windowOffset = 0; windowOffset < windows.length && !done; windowOffset += 1) {
          const window = windows[windowOffset];
          const activeTabIndex = Number(window.activeTabIndex());
          const tabs = window.tabs();
          for (let tabOffset = 0; tabOffset < tabs.length && !done; tabOffset += 1) {
            const tab = tabs[tabOffset];
            result.tabs.push({
              browserBundleID: target.bundleID,
              browserName: target.name,
              title: clean(tab.title(), maximumTitleLength),
              url: clean(tab.url(), maximumURLLength),
              windowIndex: windowOffset + 1,
              tabIndex: tabOffset + 1,
              active: activeTabIndex === tabOffset + 1,
              source: "\(source)"
            });
            if (result.tabs.length >= maximumTabs) {
              done = true;
            }
          }
        }
      } catch (error) {
        result.error = String(error);
      }
    }
    console.log(JSON.stringify(result));
    """
}

private struct BrowserAutomationScriptOutput: Decodable {
    let running: Bool
    let tabs: [BrowserAutomationTabSnapshot]
    let error: String?
}
