import SwiftUI
import AetowerBridge

private struct OverviewHostAlertCard: View {
    let title: String
    let value: String
    let subtitle: String
    let tone: Color
    let samples: [Double]

    var body: some View {
        MetricCardSurface(tone: tone, samples: samples, fillOpacity: 0.08, minHeight: 124) {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .firstTextBaseline) {
                    Text(title)
                        .font(.headline)
                    Spacer()
                    Text(value)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(tone)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(tone.opacity(0.10), in: Capsule())
                }
                Spacer(minLength: 0)
                Text(subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
        } hoverOverlay: {
            EmptyView()
        }
    }
}

private struct OverviewIncidentBanner: View {
    let incident: HostIncidentSummary

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: incident.severity == .critical ? "exclamationmark.triangle.fill" : "waveform.path.ecg")
                    .foregroundStyle(incident.severity.color)
                Text(incident.title)
                    .font(.title3.weight(.semibold))
                Spacer()
                Text(incident.severity.label)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(incident.severity.color)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(incident.severity.color.opacity(0.10), in: Capsule())
            }
            Text(incident.summary)
                .font(.subheadline)
            Text(incident.action)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(incident.severity.color.opacity(0.08), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
                .stroke(incident.severity.color.opacity(0.18), lineWidth: 1)
        )
    }
}

private struct OverviewSectionHeader: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.title3.weight(.semibold))
            Text(subtitle)
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
    }
}

private struct OverviewPanel<Content: View>: View {
    let title: String
    let subtitle: String
    let content: Content

    init(title: String, subtitle: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.subtitle = subtitle
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            OverviewSectionHeader(title: title, subtitle: subtitle)
            content
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.lg, style: .continuous))
    }
}

private struct OverviewListCard: View {
    let title: String
    let value: String
    let detail: String
    let tone: Color
    let footer: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(title)
                    .font(.headline)
                Spacer()
                Text(value)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(tone)
            }
            Text(detail)
                .font(.subheadline.weight(.medium))
            Text(footer)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(14)
        .frame(maxWidth: .infinity, minHeight: 124, alignment: .leading)
        .contentShape(Rectangle())
        .overlay(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
                .stroke(Color.secondary.opacity(0.10), lineWidth: 1)
        )
    }
}

private struct OverviewHostAlert: Identifiable {
    let id: String
    let title: String
    let value: String
    let subtitle: String
    let tone: Color
    let samples: [Double]
}

public struct OverviewView: View {
    let state: AppState

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                summarySection

                if let machineIncident {
                    OverviewIncidentBanner(incident: machineIncident)
                }

                if !hostAlerts.isEmpty {
                    OverviewPanel(
                        title: "Host alerts",
                        subtitle: "Machine-level signals for memory pressure, wakeup storms, and thermal state."
                    ) {
                        LazyVGrid(columns: overviewGridColumns(minimum: 280), alignment: .leading, spacing: 12) {
                            ForEach(hostAlerts) { alert in
                                OverviewHostAlertCard(
                                    title: alert.title,
                                    value: alert.value,
                                    subtitle: alert.subtitle,
                                    tone: alert.tone,
                                    samples: alert.samples
                                )
                            }
                        }
                    }
                }

                if !burdenLeaders.isEmpty {
                    OverviewPanel(
                        title: "Burden leaders",
                        subtitle: "The groups currently carrying the largest memory, wakeup, disk, and network burden."
                    ) {
                        LazyVGrid(columns: overviewGridColumns(minimum: 260), alignment: .leading, spacing: 12) {
                            ForEach(burdenLeaders) { leader in
                                TrendMetricCard(
                                    title: leader.title,
                                    value: leader.metricValue,
                                    subtitle: "\(leader.entityName) · \(leader.detail)",
                                    samples: burdenLeaderSamples(for: leader),
                                    style: burdenLeaderStyle(for: leader),
                                    minHeight: 124
                                )
                            }
                        }
                    }
                }

                if !operatorRecommendations.isEmpty {
                    OverviewPanel(
                        title: "Recommended next actions",
                        subtitle: "Operator-grade remediation suggestions derived from the current host state."
                    ) {
                        LazyVGrid(columns: overviewGridColumns(minimum: 260), alignment: .leading, spacing: 12) {
                            ForEach(operatorRecommendations) { recommendation in
                                OverviewListCard(
                                    title: recommendation.title,
                                    value: recommendation.severity.label,
                                    detail: recommendation.detail,
                                    tone: recommendation.severity.color,
                                    footer: recommendation.action
                                )
                            }
                        }
                    }
                }

                if !selfHealthChecks.isEmpty {
                    OverviewPanel(
                        title: "Aetower self-health",
                        subtitle: "The health of Aetower’s own runtime, diagnostics, history store, capabilities, and MCP surface."
                    ) {
                        LazyVGrid(columns: overviewGridColumns(minimum: 240), alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                            ForEach(selfHealthChecks) { check in
                                OverviewListCard(
                                    title: check.title,
                                    value: check.value,
                                    detail: check.detail,
                                    tone: check.severity.color,
                                    footer: check.severity == .info ? "No immediate operator action required." : "Investigate this subsystem before trusting the rest of the output."
                                )
                            }
                        }
                    }
                }

                // When the machine is healthy and no sections below the
                // summary rendered, show a positive confirmation so the
                // user knows the blank space is intentional.
                if machineIncident == nil && hostAlerts.isEmpty
                    && burdenLeaders.isEmpty && operatorRecommendations.isEmpty
                {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Image(systemName: "checkmark.circle.fill")
                            .foregroundStyle(AetowerDesign.Status.success)
                        Text("System is healthy — no alerts, incidents, or recommendations.")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.top, AetowerDesign.Spacing.sm)
                }
            }
            .padding(.horizontal, AetowerDesign.Spacing.xl)
            .padding(.vertical, AetowerDesign.Spacing.lg)
        }
        .background(Color.secondary.opacity(0.03))
        .navigationTitle("Overview")
    }

    private var summarySection: some View {
        let host = state.snapshot.host
        let frictionScore = machineFrictionScore(for: host)
        let agentCount = host.aiAgentCount
        return OverviewPanel(
            title: "Machine overview",
            subtitle: hostStatusSummary(host)
        ) {
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .top, spacing: 16) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Host friction")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        Text(String(format: "%.0f", frictionScore))
                            .font(.system(size: 48, weight: .bold, design: .rounded))
                            .foregroundStyle(AetowerDesign.frictionColor(Float(frictionScore)))
                        Text("\(state.snapshot.entities.count) ranked groups · \(agentCount) AI agent\(agentCount == 1 ? "" : "s")")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                }

                LazyVGrid(columns: overviewGridColumns(minimum: 180), alignment: .leading, spacing: 12) {
                    TrendMetricCard(
                        title: "CPU",
                        value: String(format: "%.0f%%", host.cpuPercent),
                        subtitle: "Thermal \(thermalStateLabel(host.thermalState))",
                        samples: hostHistorySamples { Double($0.cpuPercent) },
                        style: .cpu,
                        minHeight: 124
                    )
                    TrendMetricCard(
                        title: "Memory",
                        value: formatBytes(host.memoryUsedBytes),
                        subtitle: "\(formatBytes(host.compressedMemoryBytes)) compressed · \(formatBytes(host.swapUsedBytes)) swap",
                        samples: hostHistorySamples { Double($0.memoryUsedBytes) },
                        style: .memory,
                        minHeight: 124
                    )
                    TrendMetricCard(
                        title: "Disk",
                        value: formatRate(host.diskReadBps + host.diskWriteBps),
                        subtitle: "read + write · \(trendWindowLabel(sampleCount: min(state.historySnapshots.count, 24)))",
                        samples: hostHistorySamples { Double($0.diskReadBps + $0.diskWriteBps) },
                        style: .disk,
                        minHeight: 124
                    )
                    TrendMetricCard(
                        title: "Network",
                        value: formatRate(host.networkReceiveBps + host.networkSendBps),
                        subtitle: "send + receive · \(trendWindowLabel(sampleCount: min(state.historySnapshots.count, 24)))",
                        samples: hostHistorySamples { Double($0.networkReceiveBps + $0.networkSendBps) },
                        style: .network,
                        minHeight: 124
                    )
                    TrendMetricCard(
                        title: "Wakeups",
                        value: formatWakeups(host.wakeupsPerSecond),
                        subtitle: hostWakeupBand(host.wakeupsPerSecond) == .severe ? "Wakeup storm band" : "Current host wakeup rate",
                        samples: hostHistorySamples { Double($0.wakeupsPerSecond) },
                        style: .friction,
                        minHeight: 124
                    )
                    TrendMetricCard(
                        title: "AI agents",
                        value: "\(agentCount)",
                        subtitle: "recent local AI runtime count",
                        samples: hostHistorySamples { Double($0.aiAgentCount) },
                        style: .energy,
                        minHeight: 124
                    )
                    if host.gpuPercent > 0 || host.gpuMemoryBytes > 0 {
                        TrendMetricCard(
                            title: "GPU",
                            value: host.gpuPercent > 0
                                ? String(format: "%.0f%%", host.gpuPercent)
                                : formatBytes(host.gpuMemoryBytes),
                            subtitle: hostGPUSummary(host),
                            samples: host.gpuPercent > 0
                                ? hostHistorySamples { Double($0.gpuPercent) }
                                : hostHistorySamples { Double($0.gpuMemoryBytes) },
                            style: .energy,
                            minHeight: 124
                        )
                    }
                }
            }
        }
    }

    private var machineIncident: HostIncidentSummary? {
        buildHostIncident(snapshot: state.snapshot, history: state.historyStoreSummary ?? state.historyRangeSummary)
    }

    private var burdenLeaders: [BurdenLeaderSummary] {
        Array(buildBurdenLeaders(snapshot: state.snapshot).prefix(4))
    }

    private var operatorRecommendations: [OperatorRecommendationSummary] {
        buildOperatorRecommendations(
            snapshot: state.snapshot,
            diagnostics: state.diagnosticsOverview,
            history: state.historyStoreSummary ?? state.historyRangeSummary,
            runtime: state.runtimeLagMetrics
        )
    }

    private var selfHealthChecks: [OperatorHealthCheckSummary] {
        buildSelfHealthChecks(
            snapshot: state.snapshot,
            diagnostics: state.diagnosticsOverview,
            history: state.historyStoreSummary ?? state.historyRangeSummary,
            runtime: state.runtimeLagMetrics,
            localMcpServerHealthy: state.localMcpServerHealthy
        )
    }

    private var hostAlerts: [OverviewHostAlert] {
        var alerts: [OverviewHostAlert] = []
        let host = state.snapshot.host
        let memoryBand = hostPressureBand(host)
        if memoryBand != .nominal {
            let leaders = state.snapshot.entities
                .sorted { $0.metrics.memoryResidentBytes > $1.metrics.memoryResidentBytes }
                .prefix(3)
                .map(\.displayName)
                .joined(separator: ", ")
            let pressureText = memoryBand == .severe ? "Severe" : "Elevated"
            alerts.append(
                OverviewHostAlert(
                    id: "memory",
                    title: "Memory pressure",
                    value: pressureText,
                    subtitle: [
                        "compressed \(formatBytes(host.compressedMemoryBytes))",
                        host.swapUsedBytes > 0 ? "swap \(formatBytes(host.swapUsedBytes))" : nil,
                        leaders.isEmpty ? nil : "leaders \(leaders)"
                    ]
                    .compactMap { $0 }
                    .joined(separator: " · "),
                    tone: memoryBand == .severe ? AetowerDesign.Status.error : AetowerDesign.Status.warning,
                    samples: hostHistorySamples { Double($0.compressedMemoryBytes + $0.swapUsedBytes) }
                )
            )
        }

        let wakeupBand = hostWakeupBand(host.wakeupsPerSecond)
        if wakeupBand != .nominal,
           let leader = state.snapshot.entities.max(by: { $0.metrics.wakeupsPerSecond < $1.metrics.wakeupsPerSecond })
        {
            alerts.append(
                OverviewHostAlert(
                    id: "wakeups",
                    title: "Wakeup leader",
                    value: leader.displayName,
                    subtitle: "\(formatWakeups(leader.metrics.wakeupsPerSecond)) entity · \(formatWakeups(host.wakeupsPerSecond)) host · \(wakeupsLeaderSubtitle(for: leader))",
                    tone: wakeupBand == .severe ? AetowerDesign.Status.error : AetowerDesign.Status.warning,
                    samples: leader.trend.wakeupsPerSecond.map(Double.init)
                )
            )
        }

        return alerts
    }

    private func burdenLeaderStyle(for leader: BurdenLeaderSummary) -> TrendMetricStyle {
        switch leader.id {
        case "memory":
            return .memory
        case "wakeups":
            return .friction
        case "disk":
            return .disk
        case "network":
            return .network
        default:
            return .energy
        }
    }

    private func burdenLeaderSamples(for leader: BurdenLeaderSummary) -> [Double] {
        guard let entity = state.snapshot.entities.first(where: { $0.entityId == leader.entityId }) else {
            return []
        }
        switch leader.id {
        case "memory":
            return entity.trend.memoryResidentBytes.map(Double.init)
        case "wakeups":
            return entity.trend.wakeupsPerSecond.map(Double.init)
        case "disk":
            return entity.trend.diskActivityBps.map(Double.init)
        case "network":
            return entity.trend.networkActivityBps.map(Double.init)
        default:
            return []
        }
    }

    /// Unified history sample extractor. The previous three type-specific
    /// helpers (Float, UInt64, Double) were nearly identical and each
    /// iterated the same .suffix(24) slice independently.
    private func hostHistorySamples(_ extractor: (HostSnapshot) -> Double) -> [Double] {
        let samples = state.historySnapshots.suffix(24).map { extractor($0.host) }
        return samples.isEmpty ? [extractor(state.snapshot.host)] : samples
    }
}

// MARK: - Grid column presets (allocated once, not per render)

private func overviewGridColumns(minimum: CGFloat) -> [GridItem] {
    [GridItem(.adaptive(minimum: minimum, maximum: 420), spacing: AetowerDesign.Spacing.md, alignment: .top)]
}
