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

private enum SortKey: String, CaseIterable, Identifiable {
    case friction
    case cpu
    case memory
    case disk
    case alphabeticalAsc
    case alphabeticalDesc
    case oldestFirst
    case newestFirst

    var id: String { rawValue }

    var title: String {
        switch self {
        case .friction: return "Friction"
        case .cpu: return "CPU"
        case .memory: return "Memory"
        case .disk: return "Disk"
        case .alphabeticalAsc: return "A - Z"
        case .alphabeticalDesc: return "Z - A"
        case .oldestFirst: return "Oldest first"
        case .newestFirst: return "Newest first"
        }
    }

    var tone: Color {
        switch self {
        case .friction: return .orange
        case .cpu: return .blue
        case .memory: return .green
        case .disk: return .pink
        case .alphabeticalAsc, .alphabeticalDesc, .oldestFirst, .newestFirst:
            return .gray
        }
    }

    var usesMetricValue: Bool {
        switch self {
        case .friction, .cpu, .memory, .disk:
            return true
        case .alphabeticalAsc, .alphabeticalDesc, .oldestFirst, .newestFirst:
            return false
        }
    }
}

private struct SortChip: View {
    let title: String
    let tone: Color

    var body: some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.caption.weight(.semibold))
            Image(systemName: "chevron.down")
                .font(.caption2.weight(.semibold))
        }
        .foregroundStyle(.primary)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(Color.secondary.opacity(0.08), in: Capsule())
        .overlay(
            Capsule()
                .stroke(tone.opacity(0.16), lineWidth: 1)
        )
    }
}

private struct RowSignalBadge: View {
    let valueText: String?
    let title: String
    let tone: Color
    let showsForegroundDot: Bool

    var body: some View {
        HStack(spacing: 8) {
            if let valueText {
                Text(valueText)
                    .font(.caption.weight(.semibold))
                    .monospacedDigit()
                    .foregroundStyle(.white)
            }

            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(1)

            if showsForegroundDot {
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

}

private struct EntityRow: View {
    let entity: EntitySnapshot
    let isSelected: Bool
    let hostMemoryTotalBytes: UInt64
    let sortKey: SortKey
    let snapshotCapturedAtMillis: UInt64

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(alignment: .center, spacing: 6) {
                    RowSignalBadge(
                        valueText: badgeValueText,
                        title: entity.displayName,
                        tone: sortKey.tone,
                        showsForegroundDot: entity.metrics.isForeground
                    )
                    InlineMetric(title: "CPU", value: String(format: "%.1f%%", entity.metrics.cpuPercent))
                    InlineMetric(title: "Mem", value: String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: hostMemoryTotalBytes)))
                    InlineMetric(title: "Disk", value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps))
                    if let badge = entity.badges.first {
                        InlineMetric(title: "Tag", value: badge)
                    }
                }
            }

            Text(primaryNarrative)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
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

    private var badgeValueText: String? {
        switch sortKey {
        case .friction:
            return String(format: "%.1f", entity.friction.totalScore)
        case .cpu:
            return String(format: "%.1f%%", entity.metrics.cpuPercent)
        case .memory:
            return String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: hostMemoryTotalBytes))
        case .disk:
            return formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps)
        case .alphabeticalAsc, .alphabeticalDesc:
            return nil
        case .oldestFirst:
            return ageLabel(from: entity.oldestProcessStartMillis, now: snapshotCapturedAtMillis)
        case .newestFirst:
            return ageLabel(from: entity.newestProcessStartMillis, now: snapshotCapturedAtMillis)
        }
    }
}

public struct MainListView: View {
    @ObservedObject private var state: AppState
    @State private var selectedEntityID: String?
    @State private var searchText = ""
    @State private var sortKey: SortKey = .friction

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
                        HStack(spacing: 8) {
                            RowSignalBadge(
                                valueText: badgeValueText(for: topConcern),
                                title: topConcern.displayName,
                                tone: sortKey.tone,
                                showsForegroundDot: topConcern.metrics.isForeground
                            )
                            Text(topConcernSummary(for: topConcern, sortKey: sortKey))
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                    }
                }

                rankedEntitiesSection
            }
            .padding(16)
        }
    }

    private func detailPanel(for entity: EntitySnapshot) -> some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .center, spacing: 12) {
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

                    SectionEyebrow(text: "Detail")

                    Text(entity.displayName)
                        .font(.title2.weight(.semibold))

                    StatusBadge(score: Double(entity.friction.totalScore))

                    Spacer()
                }

                Text(topConcernSummary(for: entity, sortKey: sortKey))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)

            Divider()

            EntityDetailView(entity: entity)
        }
    }

    private var rankedEntitiesSection: some View {
            VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .center, spacing: 8) {
                Text("Apps sorted by")
                    .font(.headline)

                Menu {
                    ForEach(SortKey.allCases) { key in
                        Button {
                            sortKey = key
                        } label: {
                            HStack {
                                Text(key.title)
                                if key == sortKey {
                                    Spacer()
                                    Image(systemName: "checkmark")
                                }
                            }
                        }
                    }
                } label: {
                    SortChip(title: sortKey.title, tone: sortKey.tone)
                }
            }

            TextField("Search apps, reasons, or badges", text: $searchText)
                .textFieldStyle(.roundedBorder)

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
                                hostMemoryTotalBytes: state.snapshot.host.memoryTotalBytes,
                                sortKey: sortKey,
                                snapshotCapturedAtMillis: state.snapshot.capturedAtMillis
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
        let filtered: [EntitySnapshot]
        if query.isEmpty {
            filtered = state.snapshot.entities
        } else {
            let loweredQuery = query.localizedLowercase
            filtered = state.snapshot.entities.filter { entity in
                entity.displayName.localizedLowercase.contains(loweredQuery)
                    || entity.badges.joined(separator: " ").localizedLowercase.contains(loweredQuery)
                    || entity.friction.reasons.joined(separator: " ").localizedLowercase.contains(loweredQuery)
            }
        }

        return filtered.sorted {
            compareEntities($0, $1, by: sortKey)
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

    private func topConcernSummary(for entity: EntitySnapshot, sortKey: SortKey) -> String {
        switch sortKey {
        case .friction:
            let reason = entity.friction.reasons.first ?? "no single dominant reason was recorded."
            if entity.metrics.isForeground {
                return "\(entity.displayName) is frontmost and currently ranked highest because \(reason.lowercased())"
            }
            return "\(entity.displayName) is the highest-ranked background source of friction because \(reason.lowercased())"
        case .cpu:
            return "\(entity.displayName) is currently highest by CPU usage at \(String(format: "%.1f%%", entity.metrics.cpuPercent))."
        case .memory:
            return "\(entity.displayName) is currently highest by memory load at \(String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: state.snapshot.host.memoryTotalBytes)))."
        case .disk:
            return "\(entity.displayName) is currently highest by disk activity at \(formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps))."
        case .alphabeticalAsc:
            return "\(entity.displayName) is first in the current alphabetical A-Z sort."
        case .alphabeticalDesc:
            return "\(entity.displayName) is first in the current alphabetical Z-A sort."
        case .oldestFirst:
            return "\(entity.displayName) has the oldest process group in the current list at \(ageLabel(from: entity.oldestProcessStartMillis, now: state.snapshot.capturedAtMillis))."
        case .newestFirst:
            return "\(entity.displayName) has the newest process group in the current list at \(ageLabel(from: entity.newestProcessStartMillis, now: state.snapshot.capturedAtMillis))."
        }
    }

    private func badgeValueText(for entity: EntitySnapshot) -> String? {
        switch sortKey {
        case .friction:
            return String(format: "%.1f", entity.friction.totalScore)
        case .cpu:
            return String(format: "%.1f%%", entity.metrics.cpuPercent)
        case .memory:
            return String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: state.snapshot.host.memoryTotalBytes))
        case .disk:
            return formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps)
        case .alphabeticalAsc, .alphabeticalDesc:
            return nil
        case .oldestFirst:
            return ageLabel(from: entity.oldestProcessStartMillis, now: state.snapshot.capturedAtMillis)
        case .newestFirst:
            return ageLabel(from: entity.newestProcessStartMillis, now: state.snapshot.capturedAtMillis)
        }
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

private func compareEntities(_ left: EntitySnapshot, _ right: EntitySnapshot, by sortKey: SortKey) -> Bool {
    switch sortKey {
    case .friction:
        return Double(left.friction.totalScore) > Double(right.friction.totalScore)
    case .cpu:
        return Double(left.metrics.cpuPercent) > Double(right.metrics.cpuPercent)
    case .memory:
        return left.metrics.memoryResidentBytes > right.metrics.memoryResidentBytes
    case .disk:
        return (left.metrics.diskReadBps + left.metrics.diskWriteBps) > (right.metrics.diskReadBps + right.metrics.diskWriteBps)
    case .alphabeticalAsc:
        return left.displayName.localizedCaseInsensitiveCompare(right.displayName) == .orderedAscending
    case .alphabeticalDesc:
        return left.displayName.localizedCaseInsensitiveCompare(right.displayName) == .orderedDescending
    case .oldestFirst:
        let leftStart = left.oldestProcessStartMillis == 0 ? UInt64.max : left.oldestProcessStartMillis
        let rightStart = right.oldestProcessStartMillis == 0 ? UInt64.max : right.oldestProcessStartMillis
        return leftStart < rightStart
    case .newestFirst:
        return left.newestProcessStartMillis > right.newestProcessStartMillis
    }
}

private func ageLabel(from startMillis: UInt64, now capturedAtMillis: UInt64) -> String {
    guard startMillis > 0, capturedAtMillis >= startMillis else {
        return "n/a"
    }

    let elapsedSeconds = Int((capturedAtMillis - startMillis) / 1_000)
    if elapsedSeconds < 60 {
        return "\(elapsedSeconds)s"
    }
    if elapsedSeconds < 3_600 {
        return "\(elapsedSeconds / 60)m"
    }
    if elapsedSeconds < 86_400 {
        return "\(elapsedSeconds / 3_600)h"
    }
    return "\(elapsedSeconds / 86_400)d"
}
