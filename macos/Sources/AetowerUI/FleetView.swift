import SwiftUI
import AetowerBridge

public struct FleetView: View {
    var state: AppState
    @State private var fleet = FleetService()

    public init(state: AppState) { self.state = state }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Header with toggle
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Fleet Monitoring")
                        .font(.headline)
                    Text("Discover other Aetower instances on your local network.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Toggle("Enabled", isOn: Binding(
                    get: { fleet.isEnabled },
                    set: { newValue in
                        if newValue { fleet.start(state: state) } else { fleet.stop() }
                    }
                ))
                .toggleStyle(.switch)
                .labelsHidden()
            }
            .padding(.horizontal, 16)
            .padding(.top, 12)

            Divider()

            if !fleet.isEnabled {
                ContentUnavailableView(
                    "Fleet monitoring is off",
                    systemImage: "network",
                    description: Text("Enable the toggle above to discover peers via Bonjour and share your system snapshot on the local network.")
                )
            } else if fleet.peers.isEmpty {
                ContentUnavailableView(
                    "Searching for peers...",
                    systemImage: "antenna.radiowaves.left.and.right",
                    description: Text("Browsing for other Aetower instances on the local network. Make sure fleet is enabled on other Macs too.")
                )
            } else {
                // Peer list
                ScrollView {
                    LazyVStack(spacing: 4) {
                        // Column headers
                        HStack(spacing: 8) {
                            Text("Peer")
                                .frame(maxWidth: .infinity, alignment: .leading)
                            Text("CPU")
                                .frame(width: 50, alignment: .trailing)
                            Text("Entities")
                                .frame(width: 50, alignment: .trailing)
                            Text("Last seen")
                                .frame(width: 70, alignment: .trailing)
                        }
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 16)

                        ForEach(fleet.peers) { peer in
                            HStack(spacing: 8) {
                                Image(systemName: "desktopcomputer")
                                    .font(.system(size: 11))
                                    .foregroundStyle(.blue)
                                Text(peer.name)
                                    .font(.system(size: 12, weight: .medium))
                                    .lineLimit(1)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                Text(String(format: "%.0f%%", peer.cpuPercent))
                                    .font(.system(size: 11, design: .monospaced))
                                    .foregroundStyle(.secondary)
                                    .frame(width: 50, alignment: .trailing)
                                Text("\(peer.entityCount)")
                                    .font(.system(size: 11, design: .monospaced))
                                    .foregroundStyle(.secondary)
                                    .frame(width: 50, alignment: .trailing)
                                Text(peer.lastSeen, style: .relative)
                                    .font(.system(size: 10))
                                    .foregroundStyle(.tertiary)
                                    .frame(width: 70, alignment: .trailing)
                            }
                            .padding(.horizontal, 16)
                            .padding(.vertical, 6)
                            .background(Color.secondary.opacity(0.04), in: RoundedRectangle(cornerRadius: 6))
                            .padding(.horizontal, 8)
                        }
                    }
                }
            }

            Spacer()
        }
    }
}
