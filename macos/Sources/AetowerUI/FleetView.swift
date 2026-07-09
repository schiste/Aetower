import AetowerBridge
import SwiftUI

public struct FleetView: View {
    var state: AppState
    var settings: SettingsStore
    @State private var fleet = FleetService()

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            header
            Divider()
            if !settings.fleetEnabled && !fleet.isEnabled {
                setupFirstView
            } else {
                fleetContent
            }
            Spacer()
        }
        .task {
            if settings.fleetEnabled {
                fleet.start(state: state)
            }
        }
        .onChange(of: settings.fleetEnabled) { _, isEnabled in
            if isEnabled {
                fleet.start(state: state)
            } else {
                fleet.stop()
            }
        }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text("Fleet")
                    .font(AetowerDesign.Typography.sectionTitle)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                Text(statusCopy)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }
            Spacer()
            Button {
                settings.fleetEnabled.toggle()
            } label: {
                AetowerBadge(statusLabel, systemImage: statusSystemImage, tone: statusTone)
            }
            .buttonStyle(.plain)
            .animation(AetowerDesign.Motion.quick, value: fleet.isEnabled)
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
        .padding(.top, AetowerDesign.Spacing.md)
    }

    private var statusLabel: String {
        if fleet.isEnabled { return "On" }
        if settings.fleetEnabled { return "Starting" }
        return "Off"
    }

    private var statusTone: Color {
        if fleet.isEnabled { return AetowerDesign.Status.success }
        if settings.fleetEnabled { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.neutral
    }

    private var statusSystemImage: String {
        if fleet.isEnabled { return "checkmark.circle.fill" }
        if settings.fleetEnabled { return "clock" }
        return "power"
    }

    private var statusCopy: String {
        if fleet.isEnabled {
            return "This Mac is advertising current Aetower snapshots on the local network."
        }
        if settings.fleetEnabled {
            return "Fleet is enabled in settings and waiting for local advertising to start."
        }
        return "Off by default. Enable only on a trusted local network."
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
                        warningLine("Trusted network required: peers on this LAN can discover Aetower when Fleet is on.")
                        privacyLine("Only enable this on trusted networks.")
                        privacyLine("Other Aetower instances can see this Mac's current host and process snapshot while Fleet is on.")
                        privacyLine("Fleet is useful for labs and multi-Mac debugging; it is intentionally opt-in for personal machines.")
                    }
                    .padding(.top, AetowerDesign.Spacing.xs)
                }

                Button {
                    settings.fleetEnabled = true
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
                advertisingStatus
                ContentUnavailableView(
                    fleet.isEnabled ? "Searching for peers..." : "Waiting for Fleet listener...",
                    systemImage: "antenna.radiowaves.left.and.right",
                    description: Text(emptyStateDescription)
                )
                Text("While Fleet is enabled, this Mac is advertising Aetower on the local network and can serve its current snapshot to peers.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(spacing: AetowerDesign.Spacing.xs) {
                    advertisingStatus
                        .padding(.horizontal, AetowerDesign.Spacing.lg)
                        .padding(.bottom, AetowerDesign.Spacing.xs)

                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text("Peer")
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Text("Confidence")
                            .frame(width: 88, alignment: .trailing)
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

    private var emptyStateDescription: String {
        if fleet.isEnabled {
            return "Browsing for nearby Aetower instances on the local network. Make sure Fleet is enabled on other Macs too."
        }
        return "The remembered setting is on, but this Mac is not advertising snapshots until the listener is ready."
    }

    private var advertisingStatus: some View {
        AetowerSurface(level: fleet.isEnabled ? .card : .warning, padding: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: fleet.isEnabled ? "dot.radiowaves.left.and.right" : "pause.circle")
                    .foregroundStyle(fleet.isEnabled ? AetowerDesign.Status.success : AetowerDesign.Status.warning)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text(fleet.isEnabled ? "This Mac is advertising snapshots" : "Fleet is not advertising yet")
                        .font(AetowerDesign.Typography.metadataStrong)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text(fleet.isEnabled
                        ? "Nearby Macs running Aetower with Fleet enabled can discover this Mac and request the current snapshot."
                        : "The remembered setting is on, but peers cannot discover this Mac until the listener is ready.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 0)
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

    private func warningLine(_ text: String) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(AetowerDesign.Status.warning)
            Text(text)
                .font(AetowerDesign.Typography.caption.weight(.semibold))
                .foregroundStyle(AetowerDesign.Ink.primary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

// MARK: - Extracted peer row

private struct PeerRow: View {
    let peer: FleetService.FleetPeer

    var body: some View {
        AetowerOperationalListRow(tone: peer.isLocal ? AetowerDesign.Status.success : AetowerDesign.Status.neutral) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: peer.isLocal ? "laptopcomputer" : "desktopcomputer")
                    .font(AetowerDesign.Typography.compactData(size: 11))
                    .foregroundStyle(peer.isLocal ? AetowerDesign.Status.success : AetowerDesign.Tone.cpu)
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Text(peer.name)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                    if peer.isLocal {
                        Text("(you)")
                            .font(AetowerDesign.Typography.metadataStrong)
                            .foregroundStyle(AetowerDesign.Status.success)
                    }
                    if peer.isStale {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .font(AetowerDesign.Typography.metadataStrong)
                            .foregroundStyle(AetowerDesign.Status.warning)
                            .help("Peer has not responded in over 30 seconds")
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                confidenceBadge
                    .frame(width: 88, alignment: .trailing)
                Text(String(format: "%.0f%%", peer.cpuPercent))
                    .font(AetowerDesign.Typography.compactData(size: 11))
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .frame(width: 50, alignment: .trailing)
                    .contentTransition(.numericText())
                Text(String(format: "%.1f", peer.friction))
                    .font(AetowerDesign.Typography.compactData(size: 11))
                    .foregroundStyle(AetowerDesign.frictionColor(peer.friction))
                    .frame(width: 55, alignment: .trailing)
                    .contentTransition(.numericText())
                Text("\(peer.entityCount)")
                    .font(AetowerDesign.Typography.compactData(size: 11))
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .frame(width: 50, alignment: .trailing)
                    .contentTransition(.numericText())
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    if peer.unsignedCount > 0 {
                        Image(systemName: "exclamationmark.shield.fill")
                            .font(AetowerDesign.Typography.metadataStrong)
                    }
                    Text("\(peer.unsignedCount)")
                        .font(AetowerDesign.Typography.compactData(size: 11))
                }
                .foregroundStyle(peer.unsignedCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Ink.tertiary)
                .frame(width: 60, alignment: .trailing)
                .contentTransition(.numericText())
                .help(peer.unsignedCount > 0
                    ? "\(peer.unsignedCount) running \(peer.unsignedCount == 1 ? "binary is" : "binaries are") unsigned or ad-hoc signed"
                    : "No unsigned or ad-hoc binaries detected")
                Text(peer.lastSeen, style: .relative)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(peer.isStale ? AetowerDesign.Status.warning : AetowerDesign.Ink.tertiary)
                    .frame(width: 70, alignment: .trailing)
            }
        }
        .opacity(peer.isStale ? 0.6 : 1.0)
    }

    private var confidenceBadge: some View {
        let confidence = PeerConfidence(peer: peer)
        return AetowerBadge(confidence.label, tone: confidence.color)
            .help(confidence.help)
    }
}

private struct PeerConfidence {
    let label: String
    let color: Color
    let help: String

    init(peer: FleetService.FleetPeer, now: Date = .now) {
        let age = now.timeIntervalSince(peer.lastSeen)
        if peer.isLocal {
            label = "This Mac"
            color = AetowerDesign.Status.success
            help = "Local identity from this Aetower instance; metrics are read directly from AppState."
        } else if age <= 10 {
            label = "Fresh"
            color = AetowerDesign.Status.success
            help = "Peer identity is Bonjour-discovered and the snapshot was refreshed less than 10 seconds ago."
        } else if age <= FleetService.stalenessThreshold {
            label = "Recent"
            color = AetowerDesign.Status.warning
            help = "Peer identity is Bonjour-discovered, but the last snapshot is aging."
        } else {
            label = "Stale"
            color = AetowerDesign.Status.warning
            help = "Peer identity remains visible, but Aetower has not refreshed its snapshot in over 30 seconds."
        }
    }
}
