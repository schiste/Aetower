import Foundation
import OSLog

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
        let windowMinutes = max(lastMinutes, 1)
        let store = try OSLogStore(scope: .currentProcessIdentifier)
        let position = store.position(date: Date().addingTimeInterval(TimeInterval(-windowMinutes * 60)))
        let predicate = NSPredicate(
            format: "processIdentifier == %d",
            ProcessInfo.processInfo.processIdentifier
        )
        let entries = try store.getEntries(at: position, matching: predicate)

        var notificationLogEntries = 0
        var notificationAuthorizationFailures = 0
        var metalLoadFailures = 0
        var nonActiveWindowWarnings = 0

        for case let entry as OSLogEntryLog in entries {
            if isBenignFrameworkNoise(entry) {
                continue
            }
            let message = entry.composedMessage
            if entry.subsystem == "com.apple.UserNotifications"
                && entry.category == "Connections"
            {
                notificationLogEntries += 1
            }
            if message.contains("Requested authorization [ didGrant: 0 hasError: 1") {
                notificationAuthorizationFailures += 1
            }
            if message.contains("Unable to open mach-O at path") {
                metalLoadFailures += 1
            }
            if message.contains("ordered front from a non-active application") {
                nonActiveWindowWarnings += 1
            }
        }

        return SessionLogSummary(
            windowMinutes: windowMinutes,
            notificationLogEntries: notificationLogEntries,
            notificationAuthorizationFailures: notificationAuthorizationFailures,
            metalLoadFailures: metalLoadFailures,
            nonActiveWindowWarnings: nonActiveWindowWarnings
        )
    }

    private static func isBenignFrameworkNoise(_ entry: OSLogEntryLog) -> Bool {
        entry.subsystem == "com.apple.TextInputUI" && entry.category == "CursorUI"
    }
}
