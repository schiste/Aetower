import SwiftUI
import AetowerBridge

public struct MenuBarSummaryView: View {
    let state: AppState

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        let host = state.monitorViewModel.host
        let rows = state.monitorViewModel.processRows
        VStack(alignment: .leading, spacing: 12) {
            Text("Aetower")
                .font(.headline)

            sparklineRow

            HStack {
                Text("Host CPU")
                Spacer()
                Text(String(format: "%.1f%%", host?.cpuPercent ?? state.hostState.cpuPercent))
                    .monospacedDigit()
                    .foregroundStyle(cpuColor(host?.cpuPercent ?? state.hostState.cpuPercent))
            }

            HStack {
                Text("Frontmost")
                Spacer()
                Text(host?.frontmostAppName ?? state.hostState.frontmostAppName ?? "n/a")
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            HStack {
                Text("Power")
                Spacer()
                Text(host.map(menuBarPowerSummary) ?? menuBarPowerSummary(state.hostState))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            HStack {
                Text("Thermal")
                Spacer()
                HStack(spacing: 4) {
                    Image(systemName: thermalIcon(host?.thermalState ?? state.hostState.thermalState))
                        .foregroundStyle(thermalColor(host?.thermalState ?? state.hostState.thermalState))
                    Text(menuBarThermalSummary(host?.thermalState ?? state.hostState.thermalState))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            HStack {
                Text("GPU")
                Spacer()
                Text(
                    (host?.gpuPercent ?? state.hostState.gpuPercent) > 0
                        || (host?.gpuMemoryBytes ?? state.hostState.gpuMemoryBytes) > 0
                        ? "\(String(format: "%.1f%%", host?.gpuPercent ?? state.hostState.gpuPercent)) · \(formatBytes(host?.gpuMemoryBytes ?? state.hostState.gpuMemoryBytes))"
                        : "idle"
                )
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }

            let agentRows = rows.filter { $0.entityKind == .aiAgent }
            let agentCount = Int(host?.aiAgentCount ?? UInt32(agentRows.count))
            if agentCount > 0 {
                let running = agentRows.filter { $0.cpuPercent > 0.1 || $0.wakeupsPerSecond > 0.1 }.count
                let agentSummary = running > 0 ? "\(agentCount) (\(running) active)" : "\(agentCount) idle"
                HStack {
                    Text("AI agents")
                    Spacer()
                    Text(agentSummary)
                        .foregroundStyle(.secondary)
                }
            }

            if let top = rows.first {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Top friction")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(top.displayName)
                        .font(.subheadline.weight(.medium))
                    Text(top.recentChangeSummary ?? "No dominant reason")
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
        let trend = state.monitorViewModel.hostTrend
        let host = state.monitorViewModel.host
        return HStack(spacing: 8) {
            menuSparkline(
                "CPU",
                samples: trend?.cpuPercent ?? state.hostTrendState.cpuPercent.map { Double($0) },
                value: String(format: "%.0f%%", host?.cpuPercent ?? state.hostState.cpuPercent),
                tone: AetowerDesign.Tone.cpu
            )
            menuSparkline(
                "Memory",
                samples: trend?.memoryUsedBytes ?? state.hostTrendState.memoryUsedBytes.map { Double($0) },
                value: formatBytes(host?.memoryUsedBytes ?? state.hostState.memoryUsedBytes),
                tone: AetowerDesign.Tone.memory
            )
            menuSparkline(
                "Network",
                samples: trend?.networkActivityBps
                    ?? state.hostTrendState.networkActivityBps.map { Double($0) },
                value: "\(formatBytes((host?.networkReceiveBps ?? state.hostState.networkReceiveBps) + (host?.networkSendBps ?? state.hostState.networkSendBps)))/s",
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

private func menuBarPowerSummary(_ host: UiHostSummary) -> String {
    if host.onBattery {
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
