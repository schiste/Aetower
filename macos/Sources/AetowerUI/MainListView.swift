import SwiftUI
import AetowerBridge

private struct ReasonPill: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(Color.secondary.opacity(0.08), in: Capsule())
    }
}

private struct StatusBadge: View {
    let score: Double

    var body: some View {
        Text(label)
            .font(.caption.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(color.opacity(0.14), in: Capsule())
    }

    private var label: String {
        switch score {
        case 75...:
            return "Critical"
        case 40...:
            return "High"
        case 15...:
            return "Watch"
        default:
            return "Stable"
        }
    }

    private var color: Color {
        switch score {
        case 75...:
            return .red
        case 40...:
            return .orange
        case 15...:
            return .yellow
        default:
            return .green
        }
    }
}

private struct SectionEyebrow: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.caption.weight(.semibold))
            .foregroundStyle(.tertiary)
            .tracking(0.8)
    }
}

private let dashboardColumns = [GridItem(.adaptive(minimum: 240), spacing: 12)]
private let metricColumns = [GridItem(.adaptive(minimum: 180), spacing: 8)]

private struct EntityRow: View {
    let entity: EntitySnapshot
    let isSelected: Bool
    let hostMemoryTotalBytes: UInt64

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 10) {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        Text(entity.displayName)
                            .font(.headline)
                            .lineLimit(1)
                        if entity.metrics.isForeground {
                            Text("Frontmost")
                                .font(.caption.weight(.medium))
                                .foregroundStyle(.blue)
                        }
                    }
                    Text(primaryNarrative)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer()
                StatusBadge(score: Double(entity.friction.totalScore))
            }

            LazyVGrid(columns: metricColumns, alignment: .leading, spacing: 8) {
                TrendMetricCard(
                    title: "Friction",
                    value: String(format: "%.1f", entity.friction.totalScore),
                    subtitle: frictionTrendSummary(entity),
                    samples: entity.trend.friction.map(Double.init),
                    style: .friction
                )
                TrendMetricCard(
                    title: "CPU",
                    value: String(format: "%.1f%%", entity.metrics.cpuPercent),
                    subtitle: cpuTrendSummary(entity),
                    samples: entity.trend.cpuPercent.map(Double.init),
                    style: .cpu
                )
                TrendMetricCard(
                    title: "Memory",
                    value: String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: hostMemoryTotalBytes)),
                    subtitle: memoryTrendSummary(entity),
                    samples: entityMemoryTrendPercents(entity, totalBytes: hostMemoryTotalBytes),
                    style: .memory
                )
                TrendMetricCard(
                    title: "Disk Activity",
                    value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps),
                    subtitle: diskTrendSummary(entity),
                    samples: entity.trend.diskActivityBps.map(Double.init),
                    style: .disk
                )
            }

            if let activeWindowTitle = entity.activeWindowTitle, !activeWindowTitle.isEmpty {
                Text("Window: \(activeWindowTitle)")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }

            if !entity.badges.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(entity.badges, id: \.self) { badge in
                            ReasonPill(text: badge)
                        }
                    }
                }
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(rowBackground, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(isSelected ? Color.accentColor.opacity(0.5) : Color.clear, lineWidth: 1)
        )
    }

    private var primaryNarrative: String {
        if let firstReason = entity.friction.reasons.first, !firstReason.isEmpty {
            return firstReason
        }
        return "Aetower is tracking this app, but it is not currently a top source of friction."
    }

    private var rowBackground: Color {
        isSelected ? Color.accentColor.opacity(0.08) : Color.secondary.opacity(0.04)
    }
}

public struct MainListView: View {
    @ObservedObject private var state: AppState
    @State private var selectedEntityID: String?
    @State private var searchText = ""

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                summaryHeader
                insightsSection
                focusedEntitySection
                filterSection
                rankedEntitiesSection
            }
            .padding(16)
        }
        .navigationTitle("Aetower")
    }

    private var summaryHeader: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionEyebrow(text: "Machine")
            VStack(alignment: .leading, spacing: 6) {
                Text("Machine state")
                    .font(.system(size: 28, weight: .semibold, design: .rounded))
                Text("Start with system pressure, then drill into the app causing it.")
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            LazyVGrid(columns: dashboardColumns, alignment: .leading, spacing: 12) {
                TrendMetricCard(
                    title: "Machine Friction",
                    value: String(format: "%.1f", machineFrictionScore(for: state.snapshot.host)),
                    subtitle: "\(trendLabel(samples: state.snapshot.hostTrend.machineFriction.map(Double.init), stableText: "overall pressure")) · \(trendWindowLabel(sampleCount: state.snapshot.hostTrend.machineFriction.count))",
                    samples: state.snapshot.hostTrend.machineFriction.map(Double.init),
                    style: .friction
                )
                TrendMetricCard(
                    title: "CPU",
                    value: String(format: "%.1f%%", state.snapshot.host.cpuPercent),
                    subtitle: "\(foregroundEntities.count) foreground-tracked apps · \(trendWindowLabel(sampleCount: state.snapshot.hostTrend.cpuPercent.count))",
                    samples: state.snapshot.hostTrend.cpuPercent.map(Double.init),
                    style: .cpu
                )
                TrendMetricCard(
                    title: "Memory Load",
                    value: String(format: "%.1f%%", hostMemoryLoadPercent),
                    subtitle: "\(formatBytes(state.snapshot.host.memoryUsedBytes)) / \(formatBytes(state.snapshot.host.memoryTotalBytes)) · \(trendWindowLabel(sampleCount: state.snapshot.hostTrend.memoryUsedBytes.count))",
                    samples: hostMemoryTrendPercents,
                    style: .memory
                )
                TrendMetricCard(
                    title: "Disk Activity",
                    value: formatRate(state.snapshot.host.diskReadBps + state.snapshot.host.diskWriteBps),
                    subtitle: "\(trendLabel(samples: state.snapshot.hostTrend.diskActivityBps.map(Double.init), stableText: "host throughput")) · \(trendWindowLabel(sampleCount: state.snapshot.hostTrend.diskActivityBps.count))",
                    samples: state.snapshot.hostTrend.diskActivityBps.map(Double.init),
                    style: .disk
                )
            }
        }
    }

    private var insightsSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionEyebrow(text: "Read")
            Text("Immediate read")
                .font(.headline)

            if let topConcern {
                VStack(alignment: .leading, spacing: 10) {
                    HStack(spacing: 10) {
                        StatusBadge(score: Double(topConcern.friction.totalScore))
                        Text(topConcern.displayName)
                            .font(.title3.weight(.semibold))
                    }
                    Text(topConcernSummary(for: topConcern))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)

                    LazyVGrid(columns: metricColumns, alignment: .leading, spacing: 8) {
                        TrendMetricCard(
                            title: "Friction",
                            value: String(format: "%.1f", topConcern.friction.totalScore),
                            subtitle: frictionTrendSummary(topConcern),
                            samples: topConcern.trend.friction.map(Double.init),
                            style: .friction
                        )
                        TrendMetricCard(
                            title: "CPU",
                            value: String(format: "%.1f%%", topConcern.metrics.cpuPercent),
                            subtitle: cpuTrendSummary(topConcern),
                            samples: topConcern.trend.cpuPercent.map(Double.init),
                            style: .cpu
                        )
                        TrendMetricCard(
                            title: "Memory",
                            value: String(format: "%.1f%%", entityMemoryLoadPercent(topConcern, totalBytes: state.snapshot.host.memoryTotalBytes)),
                            subtitle: memoryTrendSummary(topConcern),
                            samples: entityMemoryTrendPercents(topConcern, totalBytes: state.snapshot.host.memoryTotalBytes),
                            style: .memory
                        )
                        TrendMetricCard(
                            title: "Disk Activity",
                            value: formatRate(topConcern.metrics.diskReadBps + topConcern.metrics.diskWriteBps),
                            subtitle: diskTrendSummary(topConcern),
                            samples: topConcern.trend.diskActivityBps.map(Double.init),
                            style: .disk
                        )
                    }
                }
                .padding(12)
                .background(Color.accentColor.opacity(0.07), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                Text("No entity is currently producing enough friction to be highlighted.")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var filterSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionEyebrow(text: "Filter")
            Text("Filter apps")
                .font(.headline)
            Text("Search by app name, badge, or ranking reason.")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Search apps, reasons, or badges", text: $searchText)
                .textFieldStyle(.roundedBorder)
        }
    }

    private var focusedEntitySection: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionEyebrow(text: "Likely culprit")
            Text("Focused app")
                .font(.headline)

            if let entity = selectedEntity ?? topConcern {
                VStack(alignment: .leading, spacing: 12) {
                    HStack(alignment: .top, spacing: 12) {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(entity.displayName)
                                .font(.system(size: 24, weight: .semibold, design: .rounded))
                            Text(topConcernSummary(for: entity))
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        Spacer()
                        StatusBadge(score: Double(entity.friction.totalScore))
                    }

                    LazyVGrid(columns: metricColumns, alignment: .leading, spacing: 8) {
                        TrendMetricCard(
                            title: "Friction",
                            value: String(format: "%.1f", entity.friction.totalScore),
                            subtitle: frictionTrendSummary(entity),
                            samples: entity.trend.friction.map(Double.init),
                            style: .friction
                        )
                        TrendMetricCard(
                            title: "CPU",
                            value: String(format: "%.1f%%", entity.metrics.cpuPercent),
                            subtitle: cpuTrendSummary(entity),
                            samples: entity.trend.cpuPercent.map(Double.init),
                            style: .cpu
                        )
                        TrendMetricCard(
                            title: "Memory",
                            value: String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: state.snapshot.host.memoryTotalBytes)),
                            subtitle: memoryTrendSummary(entity),
                            samples: entityMemoryTrendPercents(entity, totalBytes: state.snapshot.host.memoryTotalBytes),
                            style: .memory
                        )
                        TrendMetricCard(
                            title: "Disk Activity",
                            value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps),
                            subtitle: diskTrendSummary(entity),
                            samples: entity.trend.diskActivityBps.map(Double.init),
                            style: .disk
                        )
                    }

                    if !entity.friction.reasons.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Why this app matters")
                                .font(.subheadline.weight(.semibold))
                            ForEach(entity.friction.reasons.prefix(3), id: \.self) { reason in
                                Text("• \(reason)")
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                .padding(14)
                .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            } else {
                ContentUnavailableView(
                    "No focused app",
                    systemImage: "square.stack.3d.up.slash",
                    description: Text("Aetower will surface the most important app here.")
                )
            }
        }
    }

    private var rankedEntitiesSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionEyebrow(text: "Ranking")
            Text("Apps ranked by current friction")
                .font(.headline)

            if filteredEntities.isEmpty {
                ContentUnavailableView(
                    "No apps match this filter",
                    systemImage: "magnifyingglass",
                    description: Text("Try a broader query.")
                )
            } else {
                LazyVStack(spacing: 10) {
                    ForEach(filteredEntities, id: \.entityId) { entity in
                        Button {
                            selectedEntityID = entity.entityId
                        } label: {
                            EntityRow(
                                entity: entity,
                                isSelected: selectedEntityID == entity.entityId,
                                hostMemoryTotalBytes: state.snapshot.host.memoryTotalBytes
                            )
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }

    private var filteredEntities: [EntitySnapshot] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            return state.snapshot.entities
        }

        let loweredQuery = query.localizedLowercase
        return state.snapshot.entities.filter { entity in
            entity.displayName.localizedLowercase.contains(loweredQuery)
                || entity.badges.joined(separator: " ").localizedLowercase.contains(loweredQuery)
                || entity.friction.reasons.joined(separator: " ").localizedLowercase.contains(loweredQuery)
        }
    }

    private var topConcern: EntitySnapshot? {
        filteredEntities.first
    }

    private var foregroundEntities: [EntitySnapshot] {
        state.snapshot.entities.filter(\.metrics.isForeground)
    }

    private var hostMemoryLoadPercent: Double {
        memoryLoadPercent(bytes: state.snapshot.host.memoryUsedBytes, totalBytes: state.snapshot.host.memoryTotalBytes)
    }

    private var hostMemoryTrendPercents: [Double] {
        state.snapshot.hostTrend.memoryUsedBytes.map {
            memoryLoadPercent(bytes: $0, totalBytes: state.snapshot.host.memoryTotalBytes)
        }
    }

    private var selectedEntity: EntitySnapshot? {
        if let selectedEntityID {
            return filteredEntities.first(where: { $0.entityId == selectedEntityID })
                ?? state.snapshot.entities.first(where: { $0.entityId == selectedEntityID })
        }
        return nil
    }

    private func topConcernSummary(for entity: EntitySnapshot) -> String {
        let reason = entity.friction.reasons.first ?? "No single dominant reason was recorded."
        if entity.metrics.isForeground {
            return "\(entity.displayName) is frontmost and currently ranked highest because \(reason.lowercased())"
        }
        return "\(entity.displayName) is the highest-ranked background source of friction because \(reason.lowercased())"
    }

}

func formatBytes(_ bytes: UInt64) -> String {
    ByteFormatters.binary.string(fromByteCount: Int64(bytes))
}

func formatRate(_ bytesPerSecond: UInt64) -> String {
    "\(formatBytes(bytesPerSecond))/s"
}

private enum ByteFormatters {
    static let binary: ByteCountFormatter = {
        let formatter = ByteCountFormatter()
        formatter.allowedUnits = [.useMB, .useGB]
        formatter.countStyle = .binary
        return formatter
    }()
}

private func trendLabel(samples: [Double], stableText: String) -> String {
    guard let first = samples.first, let last = samples.last, samples.count >= 2 else {
        return stableText
    }

    let baseline = max(abs(first), 1.0)
    let deltaRatio = (last - first) / baseline
    if deltaRatio > 0.12 {
        return "rising"
    }
    if deltaRatio < -0.12 {
        return "falling"
    }
    return stableText
}

private func frictionTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.friction.map(Double.init), stableText: "recent score")) · \(trendWindowLabel(sampleCount: entity.trend.friction.count))"
}

private func cpuTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.cpuPercent.map(Double.init), stableText: "recent load")) · \(trendWindowLabel(sampleCount: entity.trend.cpuPercent.count))"
}

private func memoryTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(formatBytes(entity.metrics.memoryResidentBytes)) · \(trendWindowLabel(sampleCount: entity.trend.memoryResidentBytes.count))"
}

private func diskTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.diskActivityBps.map(Double.init), stableText: "recent throughput")) · \(trendWindowLabel(sampleCount: entity.trend.diskActivityBps.count))"
}

private func machineFrictionScore(for host: HostSnapshot) -> Double {
    let cpuScore = min(Double(host.cpuPercent), 100.0) * 0.5
    let memoryRatio = host.memoryTotalBytes == 0 ? 0.0 : Double(host.memoryUsedBytes) / Double(host.memoryTotalBytes)
    let memoryScore = min(memoryRatio, 1.0) * 35.0
    let swapScore = host.swapUsedBytes == 0 ? 0.0 : (min(Double(host.swapUsedBytes) / 1_073_741_824.0, 8.0) / 8.0) * 15.0
    return min(cpuScore + memoryScore + swapScore, 100.0)
}

private func memoryLoadPercent(bytes: UInt64, totalBytes: UInt64) -> Double {
    guard totalBytes > 0 else { return 0 }
    return (Double(bytes) / Double(totalBytes)) * 100.0
}

private func entityMemoryLoadPercent(_ entity: EntitySnapshot, totalBytes: UInt64) -> Double {
    memoryLoadPercent(bytes: entity.metrics.memoryResidentBytes, totalBytes: totalBytes)
}

private func entityMemoryTrendPercents(_ entity: EntitySnapshot, totalBytes: UInt64) -> [Double] {
    entity.trend.memoryResidentBytes.map {
        memoryLoadPercent(bytes: $0, totalBytes: totalBytes)
    }
}

private func trendWindowLabel(sampleCount: Int) -> String {
    let seconds = min(sampleCount * 2, 300)
    return "last \(seconds)s"
}
