import SwiftUI
import AetowerBridge

private struct MetricCell: View {
    let label: String
    let value: String

    var body: some View {
        VStack(alignment: .trailing, spacing: 2) {
            Text(value)
                .font(.system(.body, design: .rounded).monospacedDigit())
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .frame(minWidth: 80, alignment: .trailing)
    }
}

private struct EntityRow: View {
    let entity: EntitySnapshot

    var body: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text(entity.displayName)
                    .font(.headline)
                Text(entity.friction.reasons.first ?? "No dominant reason")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if !entity.badges.isEmpty {
                    Text(entity.badges.joined(separator: " • "))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            Spacer()
            MetricCell(label: "Friction", value: String(format: "%.1f", entity.friction.totalScore))
            MetricCell(label: "CPU", value: String(format: "%.1f%%", entity.metrics.cpuPercent))
            MetricCell(label: "Memory", value: formatBytes(entity.metrics.memoryResidentBytes))
            MetricCell(label: "Disk", value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps))
        }
        .padding(.vertical, 6)
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
            VStack(spacing: 0) {
                header
                List {
                    ForEach(filteredEntities, id: \.entityId) { entity in
                        Button {
                            selectedEntityID = entity.entityId
                        } label: {
                            EntityRow(entity: entity)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .listRowBackground(rowBackground(for: entity.entityId))
                    }
                }
                .listStyle(.inset)
            }
            .navigationTitle("Aetower")
        } detail: {
            if let entity = selectedEntity {
                EntityDetailView(entity: entity)
            } else {
                ContentUnavailableView("No entity selected", systemImage: "rectangle.stack")
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("System pressure")
                        .font(.headline)
                    Text("Unified entity view")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                MetricCell(label: "Host CPU", value: String(format: "%.1f%%", state.snapshot.host.cpuPercent))
                MetricCell(label: "Memory", value: "\(formatBytes(state.snapshot.host.memoryUsedBytes)) / \(formatBytes(state.snapshot.host.memoryTotalBytes))")
                MetricCell(label: "Net", value: "\(formatRate(state.snapshot.host.networkReceiveBps)) ↓")
            }
            TextField("Filter apps, badges, or activity", text: $searchText)
                .textFieldStyle(.roundedBorder)
        }
        .padding()
        .background(.thinMaterial)
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

    private var selectedEntity: EntitySnapshot? {
        guard let selectedEntityID else {
            return filteredEntities.first
        }
        return filteredEntities.first(where: { $0.entityId == selectedEntityID }) ?? filteredEntities.first
    }

    private func rowBackground(for entityID: String) -> Color {
        if selectedEntityID == entityID {
            return Color(nsColor: .quaternaryLabelColor).opacity(0.12)
        }
        return .clear
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
