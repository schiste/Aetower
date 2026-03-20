import SwiftUI
import AetowerBridge

public struct MenuBarSummaryView: View {
    @ObservedObject private var state: AppState
    @ObservedObject private var settings: SettingsStore

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
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

            Toggle("Show menu bar extra", isOn: $settings.showMenuBarExtra)

            Button("Refresh now") {
                state.refresh()
            }
            .buttonStyle(.borderedProminent)
        }
        .padding(14)
        .frame(width: 280)
        .onChange(of: settings.showMenuBarExtra) { _, _ in
            settings.persist()
        }
    }
}
