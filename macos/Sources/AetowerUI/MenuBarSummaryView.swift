import SwiftUI
import AetowerBridge

public struct MenuBarSummaryView: View {
    @ObservedObject private var state: AppState

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Aetower")
                .font(.headline)

            HStack {
                Text("Host CPU")
                Spacer()
                Text(String(format: "%.1f%%", state.snapshot.host.cpuPercent))
                    .monospacedDigit()
            }

            HStack {
                Text("Frontmost")
                Spacer()
                Text(state.snapshot.host.frontmostAppName ?? "n/a")
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            if let top = state.snapshot.entities.first {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Top friction")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(top.displayName)
                        .font(.subheadline.weight(.medium))
                    Text(top.friction.reasons.first ?? "No dominant reason")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }

            Divider()

            Button("Refresh now") {
                state.refresh()
            }
            .buttonStyle(.borderedProminent)

            Button("Open Settings") {
                NSApp.activate(ignoringOtherApps: true)
            }
            .buttonStyle(.bordered)
        }
        .padding(14)
        .frame(width: 280)
    }
}
