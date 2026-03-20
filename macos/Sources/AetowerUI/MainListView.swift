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

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        NavigationSplitView {
            VStack(spacing: 0) {
                header
                List(selection: $state.selectedEntityID) {
                    ForEach(state.visibleEntities) { entity in
                        EntityRow(entity: entity)
                            .tag(entity.entityId)
                    }
                }
                .listStyle(.inset)
            }
            .navigationTitle("Aetower")
        } detail: {
            if let entity = state.selectedEntity {
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
            TextField("Filter apps, badges, or activity", text: $state.searchText)
                .textFieldStyle(.roundedBorder)
        }
        .padding()
        .background(.thinMaterial)
    }
}

func formatBytes(_ bytes: UInt64) -> String {
    let formatter = ByteCountFormatter()
    formatter.allowedUnits = [.useMB, .useGB]
    formatter.countStyle = .binary
    return formatter.string(fromByteCount: Int64(bytes))
}

func formatRate(_ bytesPerSecond: UInt64) -> String {
    "\(formatBytes(bytesPerSecond))/s"
}
