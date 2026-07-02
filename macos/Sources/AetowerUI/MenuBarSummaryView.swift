import SwiftUI
import AetowerBridge

public struct MenuBarSummaryView: View {
    let state: AppState

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Aetower")
                .font(.headline)

            sparklineRow

            HStack {
                Text("Host CPU")
                Spacer()
                Text(String(format: "%.1f%%", state.snapshot.host.cpuPercent))
                    .monospacedDigit()
                    .foregroundStyle(cpuColor(state.snapshot.host.cpuPercent))
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
                HStack(spacing: 4) {
                    Image(systemName: thermalIcon(state.snapshot.host.thermalState))
                        .foregroundStyle(thermalColor(state.snapshot.host.thermalState))
                    Text(menuBarThermalSummary(state.snapshot.host.thermalState))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            HStack {
                Text("GPU")
                Spacer()
                Text(
                    state.snapshot.host.gpuPercent > 0 || state.snapshot.host.gpuMemoryBytes > 0
                        ? "\(String(format: "%.1f%%", state.snapshot.host.gpuPercent)) · \(formatBytes(state.snapshot.host.gpuMemoryBytes))"
                        : "idle"
                )
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

            Button("Export Processes (CSV)") {
                state.exportSnapshotCSV()
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

    private var sparklineRow: some View {
        let trend = state.snapshot.hostTrend
        let host = state.snapshot.host
        return HStack(spacing: 8) {
            menuSparkline(
                "CPU",
                samples: trend.cpuPercent.map { Double($0) },
                value: String(format: "%.0f%%", host.cpuPercent),
                tone: AetowerDesign.Tone.cpu
            )
            menuSparkline(
                "Memory",
                samples: trend.memoryUsedBytes.map { Double($0) },
                value: formatBytes(host.memoryUsedBytes),
                tone: AetowerDesign.Tone.memory
            )
            menuSparkline(
                "Network",
                samples: trend.networkActivityBps.map { Double($0) },
                value: "\(formatBytes(host.networkReceiveBps + host.networkSendBps))/s",
                tone: AetowerDesign.Tone.network
            )
        }
    }

    private func menuSparkline(_ title: String, samples: [Double], value: String, tone: Color) -> some View {
        MetricCardSurface(tone: tone, samples: samples, minHeight: 46) {
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.secondary)
                Text(value)
                    .font(.system(size: 11, weight: .semibold, design: .rounded))
                    .foregroundStyle(tone)
            }
        } hoverOverlay: {
            EmptyView()
        }
        .frame(maxWidth: .infinity)
    }
}

private func cpuColor(_ percent: Float) -> Color {
    switch percent {
    case 70...: return .red
    case 30...: return .orange
    default: return .green
    }
}

private func thermalIcon(_ state: ThermalState) -> String {
    switch state {
    case .critical: return "flame.fill"
    case .serious, .fair: return "thermometer.high"
    case .nominal: return "thermometer.medium"
    }
}

private func thermalColor(_ state: ThermalState) -> Color {
    switch state {
    case .critical: return .red
    case .serious: return .red
    case .fair: return .orange
    case .nominal: return .green
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

private func menuBarThermalSummary(_ state: ThermalState) -> String {
    switch state {
    case .nominal: return "nominal"
    case .fair: return "fair"
    case .serious: return "serious"
    case .critical: return "critical"
    }
}
