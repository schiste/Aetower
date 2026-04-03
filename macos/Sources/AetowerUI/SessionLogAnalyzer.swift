import Foundation

struct SessionLogSummary {
    let windowMinutes: Int
    let notificationLogEntries: Int
    let notificationAuthorizationFailures: Int
    let metalLoadFailures: Int
    let nonActiveWindowWarnings: Int

    var fingerprint: String {
        [
            String(windowMinutes),
            String(notificationLogEntries),
            String(notificationAuthorizationFailures),
            String(metalLoadFailures),
            String(nonActiveWindowWarnings),
        ].joined(separator: ":")
    }
}

enum SessionLogAnalyzer {
    static func analyzeCurrentProcess(lastMinutes: Int) throws -> SessionLogSummary {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/log")
        process.arguments = [
            "show",
            "--last",
            "\(max(lastMinutes, 1))m",
            "--style",
            "compact",
            "--predicate",
            "processIdentifier == \(ProcessInfo.processInfo.processIdentifier)",
        ]

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        try process.run()
        process.waitUntilExit()

        let outputData = stdout.fileHandleForReading.readDataToEndOfFile()
        let errorData = stderr.fileHandleForReading.readDataToEndOfFile()
        guard process.terminationStatus == 0 else {
            let errorMessage = String(data: errorData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
            throw NSError(
                domain: "Aetower.SessionLogAnalyzer",
                code: Int(process.terminationStatus),
                userInfo: [
                    NSLocalizedDescriptionKey: errorMessage?.isEmpty == false ? errorMessage! : "log show failed",
                ]
            )
        }

        let output = String(decoding: outputData, as: UTF8.self)
        let lines = output.split(separator: "\n", omittingEmptySubsequences: true)

        return SessionLogSummary(
            windowMinutes: max(lastMinutes, 1),
            notificationLogEntries: lines.filter {
                $0.contains("[com.apple.UserNotifications:Connections]")
            }.count,
            notificationAuthorizationFailures: lines.filter {
                $0.contains("Requested authorization [ didGrant: 0 hasError: 1")
            }.count,
            metalLoadFailures: lines.filter {
                $0.contains("Unable to open mach-O at path")
            }.count,
            nonActiveWindowWarnings: lines.filter {
                $0.contains("ordered front from a non-active application")
            }.count
        )
    }
}
