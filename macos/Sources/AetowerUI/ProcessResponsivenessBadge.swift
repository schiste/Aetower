import AetowerBridge
import SwiftUI

struct NotRespondingBadge: View {
    var body: some View {
        Label("Not responding", systemImage: "exclamationmark.circle.fill")
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(AetowerDesign.Status.error)
            .lineLimit(1)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(AetowerDesign.Status.error.opacity(0.12), in: Capsule())
            .help("macOS reports that this app is not responding to events. Updates automatically when it recovers.")
            .accessibilityLabel("Not responding")
    }
}

extension ProcessResponsivenessSnapshot {
    func isNotResponding(entity: EntitySnapshot) -> Bool {
        entity.components.contains { component in
            status(
                pid: component.processId,
                startTimeMillis: component.startTimeMillis,
                executablePath: component.executablePath
            ) == .notResponding
        }
    }

    func isNotResponding(row: UiProcessRow) -> Bool {
        if status(pid: row.primaryPid, executablePath: row.executablePath) == .notResponding {
            return true
        }
        // Compact app rows aggregate a bundle and their first PID can be a
        // helper. Match the actual app executable, never its helper's state.
        guard row.entityKind == .app || row.entityKind == .browser,
              let path = row.executablePath, !path.isEmpty else { return false }
        return observations.values.contains {
            $0.status == .notResponding && $0.executablePath == path
        }
    }
}
