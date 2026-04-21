import Foundation
import OSLog

public struct SessionLogSummary {
    public static let invalidDisplayIdentifierWarningThreshold = 3

    public let windowMinutes: Int
    public let notificationLogEntries: Int
    public let notificationAuthorizationFailures: Int
    public let metalLoadFailures: Int
    public let nonActiveWindowWarnings: Int
    public let cursorUiEntries: Int
    public let tccAccessRequests: Int
    public let viewBridgeCancellationCount: Int
    public let invalidDisplayIdentifierCount: Int

    var fingerprint: String {
        [
            String(windowMinutes),
            String(notificationLogEntries),
            String(notificationAuthorizationFailures),
            String(metalLoadFailures),
            String(nonActiveWindowWarnings),
            String(cursorUiEntries),
            String(tccAccessRequests),
            String(viewBridgeCancellationCount),
            String(invalidDisplayIdentifierCount),
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
        var cursorUiEntries = 0
        var tccAccessRequests = 0
        var viewBridgeCancellationCount = 0
        var invalidDisplayIdentifierCount = 0

        for case let entry as OSLogEntryLog in entries {
            if entry.subsystem == "com.apple.TextInputUI" && entry.category == "CursorUI" {
                cursorUiEntries += 1
                continue
            }
            let message = entry.composedMessage
            if entry.subsystem == "com.apple.SkyLight"
                && message.localizedCaseInsensitiveContains("invalid display identifier")
            {
                invalidDisplayIdentifierCount += 1
            }
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
            if message.contains("TCCAccessRequest() IPC") {
                tccAccessRequests += 1
            }
            if message.contains("NSViewBridgeErrorCanceled") {
                viewBridgeCancellationCount += 1
            }
        }

        return SessionLogSummary(
            windowMinutes: windowMinutes,
            notificationLogEntries: notificationLogEntries,
            notificationAuthorizationFailures: notificationAuthorizationFailures,
            metalLoadFailures: metalLoadFailures,
            nonActiveWindowWarnings: nonActiveWindowWarnings,
            cursorUiEntries: cursorUiEntries,
            tccAccessRequests: tccAccessRequests,
            viewBridgeCancellationCount: viewBridgeCancellationCount,
            invalidDisplayIdentifierCount: invalidDisplayIdentifierCount
        )
    }
}
