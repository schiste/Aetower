import AetowerBridge
import SwiftUI

public struct FleetView: View {
    var state: AppState
    @State private var fleet = FleetService()

    public init(state: AppState) { self.state = state }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            // Header with toggle
            HStack {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text("Fleet Monitoring")
                        .font(.headline)
                    Text("Discover other Aetower instances on your local network.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    if fleet.isEnabled { fleet.stop() } else { fleet.start(state: state) }
                } label: {
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        Circle()
                            .fill(fleet.isEnabled ? AetowerDesign.Status.success : AetowerDesign.Status.neutral.opacity(0.3))
                            .frame(width: 8, height: 8)
                        Text(fleet.isEnabled ? "On" : "Off")
                            .font(.caption.weight(.medium))
                    }
                    .padding(.horizontal, AetowerDesign.Spacing.sm)
                    .padding(.vertical, AetowerDesign.Spacing.xs)
                    .background(
                        fleet.isEnabled ? AetowerDesign.Status.success.opacity(0.12) : AetowerDesign.Surface.badge,
                        in: Capsule()
                    )
                    .overlay(
                        Capsule().stroke(
                            fleet.isEnabled ? AetowerDesign.Status.success.opacity(0.3) : AetowerDesign.Status.neutral.opacity(0.15),
                            lineWidth: 1
                        )
                    )
                }
                .buttonStyle(.plain)
                .animation(AetowerDesign.Motion.quick, value: fleet.isEnabled)
            }
            .padding(.horizontal, AetowerDesign.Spacing.lg)
            .padding(.top, AetowerDesign.Spacing.md)

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
                ScrollView {
                    LazyVStack(spacing: AetowerDesign.Spacing.xs) {
                        // Column headers
                        HStack(spacing: AetowerDesign.Spacing.sm) {
                            Text("Peer")
                                .frame(maxWidth: .infinity, alignment: .leading)
                            Text("CPU")
                                .frame(width: 50, alignment: .trailing)
                            Text("Friction")
                                .frame(width: 55, alignment: .trailing)
                            Text("Entities")
                                .frame(width: 50, alignment: .trailing)
                            Text("Unsigned")
                                .frame(width: 60, alignment: .trailing)
                            Text("Last seen")
                                .frame(width: 70, alignment: .trailing)
                        }
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, AetowerDesign.Spacing.lg)

                        ForEach(fleet.peers) { peer in
                            PeerRow(peer: peer)
                                .padding(.horizontal, AetowerDesign.Spacing.sm)
                        }
                    }
                }
            }

            Spacer()
        }
    }
}

// MARK: - Extracted peer row

private struct PeerRow: View {
    let peer: FleetService.FleetPeer

    var body: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: peer.isLocal ? "laptopcomputer" : "desktopcomputer")
                .font(.system(size: 11))
                .foregroundStyle(peer.isLocal ? AetowerDesign.Status.success : AetowerDesign.Tone.cpu)
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Text(peer.name)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
                if peer.isLocal {
                    Text("(you)")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(AetowerDesign.Status.success)
                }
                if peer.isStale {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .font(.system(size: 9))
                        .foregroundStyle(AetowerDesign.Status.warning)
                        .help("Peer has not responded in over 30 seconds")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Text(String(format: "%.0f%%", peer.cpuPercent))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 50, alignment: .trailing)
                .contentTransition(.numericText())
            Text(String(format: "%.1f", peer.friction))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(AetowerDesign.frictionColor(peer.friction))
                .frame(width: 55, alignment: .trailing)
                .contentTransition(.numericText())
            Text("\(peer.entityCount)")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 50, alignment: .trailing)
                .contentTransition(.numericText())
            HStack(spacing: 3) {
                if peer.unsignedCount > 0 {
                    Image(systemName: "exclamationmark.shield.fill")
                        .font(.system(size: 9))
                }
                Text("\(peer.unsignedCount)")
                    .font(.system(size: 11, design: .monospaced))
            }
            .foregroundStyle(peer.unsignedCount > 0 ? AetowerDesign.Status.warning : Color.secondary.opacity(0.5))
            .frame(width: 60, alignment: .trailing)
            .contentTransition(.numericText())
            .help(peer.unsignedCount > 0
                ? "\(peer.unsignedCount) running \(peer.unsignedCount == 1 ? "binary is" : "binaries are") unsigned or ad-hoc signed"
                : "No unsigned or ad-hoc binaries detected")
            Text(peer.lastSeen, style: .relative)
                .font(.system(size: 10))
                .foregroundStyle(peer.isStale ? AetowerDesign.Status.warning : Color.secondary.opacity(0.5))
                .frame(width: 70, alignment: .trailing)
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .opacity(peer.isStale ? 0.6 : 1.0)
        .background(
            peer.isLocal
                ? AetowerDesign.Status.success.opacity(0.04)
                : AetowerDesign.Surface.rowIdle,
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm)
        )
    }
}
