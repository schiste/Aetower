import SwiftUI
import AetowerBridge

private struct OverviewCard: View {
    let title: String
    let value: String
    let detail: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(.title2, design: .rounded).weight(.semibold))
            Text(detail)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(Color.secondary.opacity(0.08), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

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

private struct MetricPill: View {
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.body.monospacedDigit())
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(Color.secondary.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

private let dashboardColumns = [GridItem(.adaptive(minimum: 220), spacing: 14)]
private let metricColumns = [GridItem(.adaptive(minimum: 120), spacing: 10)]

private struct EntityRow: View {
    let entity: EntitySnapshot
    let isSelected: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 5) {
                    HStack(spacing: 8) {
                        Text(entity.displayName)
                            .font(.headline)
                        if entity.metrics.isForeground {
                            Text("Frontmost")
                                .font(.caption.weight(.medium))
                                .foregroundStyle(.blue)
                        }
                    }
                    Text(primaryNarrative)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer()
                StatusBadge(score: Double(entity.friction.totalScore))
            }

            LazyVGrid(columns: metricColumns, alignment: .leading, spacing: 10) {
                MetricPill(title: "Friction", value: String(format: "%.1f", entity.friction.totalScore))
                MetricPill(title: "CPU", value: String(format: "%.1f%%", entity.metrics.cpuPercent))
                MetricPill(title: "Memory", value: formatBytes(entity.metrics.memoryResidentBytes))
                MetricPill(title: "Processes", value: "\(entity.metrics.processCount)")
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
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(rowBackground, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
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
        NavigationSplitView {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    summaryHeader
                    insightsSection
                    filterSection
                    rankedEntitiesSection
                }
                .padding(20)
            }
            .navigationTitle("Aetower")
        } detail: {
            if let entity = selectedEntity {
                EntityDetailView(entity: entity)
            } else {
                ContentUnavailableView(
                    "No matching apps",
                    systemImage: "line.3.horizontal.decrease.circle",
                    description: Text("Adjust the filter to see ranked apps again.")
                )
            }
        }
    }

    private var summaryHeader: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 6) {
                Text("What is affecting this Mac right now")
                    .font(.system(size: 30, weight: .semibold, design: .rounded))
                Text("Aetower ranks apps by friction: a blend of CPU, memory, disk activity, and whether the app is actively in your way. The first rows are the ones most likely to explain slowness, heat, noise, or battery drain.")
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            LazyVGrid(columns: dashboardColumns, alignment: .leading, spacing: 14) {
                OverviewCard(
                    title: "Top concern",
                    value: topConcern?.displayName ?? "Nothing urgent",
                    detail: topConcern?.friction.reasons.first ?? "No app currently stands out as a strong source of friction."
                )
                OverviewCard(
                    title: "Host CPU",
                    value: String(format: "%.1f%%", state.snapshot.host.cpuPercent),
                    detail: "\(foregroundEntities.count) frontmost or foreground-tracked apps right now"
                )
                OverviewCard(
                    title: "Memory in use",
                    value: "\(formatBytes(state.snapshot.host.memoryUsedBytes)) / \(formatBytes(state.snapshot.host.memoryTotalBytes))",
                    detail: "Ranked list shows which apps are actually carrying that footprint"
                )
            }
        }
    }

    private var insightsSection: some View {
        VStack(alignment: .leading, spacing: 12) {
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

                    LazyVGrid(columns: metricColumns, alignment: .leading, spacing: 10) {
                        MetricPill(title: "CPU", value: String(format: "%.1f%%", topConcern.metrics.cpuPercent))
                        MetricPill(title: "Memory", value: formatBytes(topConcern.metrics.memoryResidentBytes))
                        MetricPill(title: "Disk", value: formatRate(topConcern.metrics.diskReadBps + topConcern.metrics.diskWriteBps))
                    }
                }
                .padding(16)
                .background(Color.accentColor.opacity(0.07), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            } else {
                Text("No entity is currently producing enough friction to be highlighted.")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var filterSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Filter the ranking")
                .font(.headline)
            Text("Search by app name, badges, or the reason Aetower is giving for the ranking.")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Search apps, reasons, or badges", text: $searchText)
                .textFieldStyle(.roundedBorder)
        }
    }

    private var rankedEntitiesSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Ranked apps")
                .font(.headline)

            if filteredEntities.isEmpty {
                ContentUnavailableView(
                    "No apps match this filter",
                    systemImage: "magnifyingglass",
                    description: Text("Try a broader query.")
                )
            } else {
                LazyVStack(spacing: 12) {
                    ForEach(filteredEntities, id: \.entityId) { entity in
                        Button {
                            selectedEntityID = entity.entityId
                        } label: {
                            EntityRow(entity: entity, isSelected: selectedEntityID == entity.entityId)
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

    private var selectedEntity: EntitySnapshot? {
        if let selectedEntityID {
            return filteredEntities.first(where: { $0.entityId == selectedEntityID })
                ?? state.snapshot.entities.first(where: { $0.entityId == selectedEntityID })
        }
        return filteredEntities.first
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
