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

private struct MachineBandMetric: View {
    let title: String
    let value: String
    let tone: Color
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 16, weight: .semibold, design: .rounded))
                .lineLimit(1)
            Text(subtitle)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(minWidth: 138, alignment: .leading)
        .background(tone.opacity(0.08), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(tone.opacity(0.10), lineWidth: 1)
        )
    }
}

private struct InlineMetric: View {
    let title: String
    let value: String

    var body: some View {
        HStack(spacing: 5) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .background(Color.secondary.opacity(0.06), in: Capsule())
    }
}

private struct RowSignalBadge: View {
    let score: Double
    let title: String
    let isForeground: Bool

    var body: some View {
        HStack(spacing: 8) {
            Text(String(format: "%.1f", score))
                .font(.caption.weight(.semibold))
                .monospacedDigit()
                .foregroundStyle(.white)

            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(1)

            if isForeground {
                Circle()
                    .fill(.white.opacity(0.92))
                    .frame(width: 6, height: 6)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(
            LinearGradient(
                colors: [tone.opacity(0.95), tone.opacity(0.65)],
                startPoint: .leading,
                endPoint: .trailing
            ),
            in: Capsule()
        )
    }

    private var tone: Color {
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

private struct EntityRow: View {
    let entity: EntitySnapshot
    let isSelected: Bool
    let hostMemoryTotalBytes: UInt64

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            RowSignalBadge(
                score: Double(entity.friction.totalScore),
                title: entity.displayName,
                isForeground: entity.metrics.isForeground
            )

            HStack(alignment: .top, spacing: 10) {
                Text(primaryNarrative)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                Spacer()
            }

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    InlineMetric(title: "CPU", value: String(format: "%.1f%%", entity.metrics.cpuPercent))
                    InlineMetric(title: "Mem", value: String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: hostMemoryTotalBytes)))
                    InlineMetric(title: "Disk", value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps))
                    if let badge = entity.badges.first {
                        InlineMetric(title: "Tag", value: badge)
                    }
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
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
        VStack(spacing: 0) {
            summaryHeader
            Divider()

            if let entity = selectedEntity {
                detailPanel(for: entity)
            } else {
                rankingPanel
            }
        }
        .navigationTitle("Aetower")
    }

    private var summaryHeader: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .center) {
                SectionEyebrow(text: "Machine")
                Spacer()
                if selectedEntity != nil {
                    Button("Back to ranking") {
                        selectedEntityID = nil
                    }
                    .buttonStyle(.plain)
                    .font(.caption.weight(.semibold))
                }
            }

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    MachineBandMetric(
                        title: "Friction",
                        value: String(format: "%.1f", machineFrictionScore(for: state.snapshot.host)),
                        tone: .orange,
                        subtitle: trendWindowLabel(sampleCount: state.snapshot.hostTrend.machineFriction.count)
                    )
                    MachineBandMetric(
                        title: "CPU",
                        value: String(format: "%.1f%%", state.snapshot.host.cpuPercent),
                        tone: .blue,
                        subtitle: trendWindowLabel(sampleCount: state.snapshot.hostTrend.cpuPercent.count)
                    )
                    MachineBandMetric(
                        title: "Memory",
                        value: String(format: "%.1f%%", hostMemoryLoadPercent),
                        tone: .green,
                        subtitle: "\(formatBytes(state.snapshot.host.memoryUsedBytes)) used"
                    )
                    MachineBandMetric(
                        title: "Disk",
                        value: formatRate(state.snapshot.host.diskReadBps + state.snapshot.host.diskWriteBps),
                        tone: .pink,
                        subtitle: trendWindowLabel(sampleCount: state.snapshot.hostTrend.diskActivityBps.count)
                    )
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private var rankingPanel: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                if let topConcern {
                    VStack(alignment: .leading, spacing: 6) {
                        SectionEyebrow(text: "Read")
                        HStack(spacing: 8) {
                            StatusBadge(score: Double(topConcern.friction.totalScore))
                            Text(topConcernSummary(for: topConcern))
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                    }
                }

                VStack(alignment: .leading, spacing: 10) {
                    SectionEyebrow(text: "Filter")
                    TextField("Search apps, reasons, or badges", text: $searchText)
                        .textFieldStyle(.roundedBorder)
                }

                rankedEntitiesSection
            }
            .padding(16)
        }
    }

    private func detailPanel(for entity: EntitySnapshot) -> some View {
        VStack(spacing: 0) {
            HStack(alignment: .center) {
                Button {
                    selectedEntityID = nil
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.left")
                        Text("Back to ranking")
                    }
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .background(Color.secondary.opacity(0.08), in: Capsule())
                }
                .buttonStyle(.plain)

                VStack(alignment: .leading, spacing: 4) {
                    SectionEyebrow(text: "Detail")
                    Text(entity.displayName)
                        .font(.title2.weight(.semibold))
                }
                Spacer()
                StatusBadge(score: Double(entity.friction.totalScore))
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)

            Divider()

            EntityDetailView(entity: entity)
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
                LazyVStack(spacing: 8) {
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
