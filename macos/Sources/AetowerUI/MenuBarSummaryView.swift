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

            HStack {
                Text("Power")
                Spacer()
                Text(menuBarPowerSummary(state.snapshot.host))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            HStack {
                Text("Thermal")
                Spacer()
                Text(state.snapshot.host.thermalState)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            let agentEntities = state.snapshot.entities.filter { $0.entityKind == .aiAgent }
            if !agentEntities.isEmpty {
                let running = agentEntities.filter { $0.badges.contains(where: { $0 == "running" }) }.count
                let agentSummary = running > 0
                    ? "\(agentEntities.count) (\(running) active)"
                    : "\(agentEntities.count) idle"
                HStack {
                    Text("AI agents")
                    Spacer()
                    Text(agentSummary)
                        .foregroundStyle(.secondary)
                }
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

            Button("Export Snapshot") {
                state.exportSnapshot()
            }
            .buttonStyle(.bordered)

            Button("Open Settings") {
                NSApp.activate(ignoringOtherApps: true)
            }
            .buttonStyle(.bordered)
        }
        .padding(14)
        .frame(width: 280)
    }
}

private func menuBarPowerSummary(_ host: HostSnapshot) -> String {
    if host.onBattery {
        if let batteryChargePercent = host.batteryChargePercent {
            return host.lowPowerMode ? "Battery \(batteryChargePercent)% · Low Power" : "Battery \(batteryChargePercent)%"
        }
        return host.lowPowerMode ? "Battery · Low Power" : "Battery"
    }
    return host.lowPowerMode ? "AC Power · Low Power" : "AC Power"
}
