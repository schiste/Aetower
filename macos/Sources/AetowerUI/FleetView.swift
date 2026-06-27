import AetowerBridge
import SwiftUI

public struct FleetView: View {
    var state: AppState
    @State private var fleet = FleetService()

    public init(state: AppState) { self.state = state }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            header
            Divider()
            if !fleet.isEnabled {
                setupFirstView
            } else {
                fleetContent
            }
            Spacer()
        }
    }

    private var header: some View {
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
    }

    private var setupFirstView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text("Set up local fleet visibility")
                        .font(.system(size: 26, weight: .semibold, design: .rounded))
                    Text("Fleet is off by default. Enable it only when you want nearby Macs running Aetower to discover this instance and exchange current system snapshots.")
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 210), spacing: AetowerDesign.Spacing.md)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.md
                ) {
                    setupCard(
                        "Local network only",
                        detail: "Uses Bonjour/mDNS and TCP on the local network. This is not an internet telemetry feature.",
                        systemImage: "network",
                        tone: AetowerDesign.Tone.network
                    )
                    setupCard(
                        "Shares current snapshot",
                        detail: "Peers can request the current Aetower snapshot while Fleet is enabled, including host metrics and entity summaries.",
                        systemImage: "rectangle.stack",
                        tone: AetowerDesign.Status.warning
                    )
                    setupCard(
                        "Stops cleanly",
                        detail: "Turning Fleet off cancels the listener, browser, timer, in-flight connections, and visible peers.",
                        systemImage: "power",
                        tone: AetowerDesign.Status.success
                    )
                }

                GroupBox("Before enabling") {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        privacyLine("Only enable this on trusted networks.")
                        privacyLine("Other Aetower instances can see this Mac's current host and process snapshot while Fleet is on.")
                        privacyLine("Fleet is useful for labs and multi-Mac debugging; it is intentionally opt-in for personal machines.")
                    }
                    .padding(.top, AetowerDesign.Spacing.xs)
                }

                Button {
                    fleet.start(state: state)
                } label: {
                    Label("Enable local Fleet", systemImage: "antenna.radiowaves.left.and.right")
                }
                .buttonStyle(.borderedProminent)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
    }

    @ViewBuilder
    private var fleetContent: some View {
        if fleet.peers.isEmpty {
            VStack(spacing: AetowerDesign.Spacing.md) {
                ContentUnavailableView(
                    "Searching for peers...",
                    systemImage: "antenna.radiowaves.left.and.right",
                    description: Text("Browsing for other Aetower instances on the local network. Make sure fleet is enabled on other Macs too.")
                )
                Text("While Fleet is enabled, this Mac is advertising Aetower on the local network and can serve its current snapshot to peers.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(spacing: AetowerDesign.Spacing.xs) {
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
                .padding(.top, AetowerDesign.Spacing.md)
            }
        }
    }

    private func setupCard(_ title: String, detail: String, systemImage: String, tone: Color) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: systemImage)
                .font(.title3)
                .foregroundStyle(tone)
            Text(title)
                .font(.subheadline.weight(.semibold))
            Text(detail)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 142, alignment: .topLeading)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous))
    }

    private func privacyLine(_ text: String) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "checkmark.circle")
                .foregroundStyle(AetowerDesign.Status.success)
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
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
