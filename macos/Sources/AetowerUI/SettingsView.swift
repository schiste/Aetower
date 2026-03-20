import SwiftUI
import AetowerBridge

public struct SettingsView: View {
    @ObservedObject private var state: AppState

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("Capabilities")
                    .font(.largeTitle.weight(.semibold))
                Text("Aetower keeps core monitoring useful without invasive access, and exposes richer integrations behind explicit capability gates.")
                    .foregroundStyle(.secondary)

                ForEach(state.snapshot.capabilities) { capability in
                    GroupBox {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack {
                                Text(capability.kind.rawValue)
                                    .font(.headline)
                                Spacer()
                                Text(capability.state.rawValue)
                                    .font(.caption.monospaced())
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 4)
                                    .background(Color.secondary.opacity(0.1), in: Capsule())
                            }
                            Text(capability.detail)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            HStack {
                                Button("Request / Refresh") {
                                    state.requestCapability(capability)
                                }
                                .buttonStyle(.borderedProminent)

                                Text(Date(timeIntervalSince1970: TimeInterval(capability.lastUpdatedMillis) / 1000), style: .time)
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                        }
                    }
                }
            }
            .padding(24)
        }
        .navigationTitle("Settings")
    }
}
