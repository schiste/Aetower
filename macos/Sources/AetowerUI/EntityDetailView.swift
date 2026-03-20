import SwiftUI
import AetowerBridge

public struct EntityDetailView: View {
    let entity: EntitySnapshot

    public init(entity: EntitySnapshot) {
        self.entity = entity
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                summary
                friction
                components
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .navigationTitle(entity.displayName)
    }

    private var summary: some View {
        GroupBox("Summary") {
            VStack(alignment: .leading, spacing: 10) {
                LabeledContent("Entity type", value: entity.entityKind.rawValue)
                LabeledContent("Bundle", value: entity.bundleId ?? "n/a")
                LabeledContent("Executable", value: entity.executablePath ?? "n/a")
                LabeledContent("Processes", value: "\(entity.metrics.processCount)")
                LabeledContent("Foreground", value: entity.metrics.isForeground ? "yes" : "no")
                LabeledContent("Badges", value: entity.badges.isEmpty ? "none" : entity.badges.joined(separator: ", "))
            }
        }
    }

    private var friction: some View {
        GroupBox("Friction") {
            VStack(alignment: .leading, spacing: 10) {
                Text(String(format: "%.1f", entity.friction.totalScore))
                    .font(.system(size: 34, weight: .semibold, design: .rounded))
                ForEach(entity.friction.reasons, id: \.self) { reason in
                    Text("• \(reason)")
                        .foregroundStyle(.secondary)
                }
                Divider()
                HStack {
                    StatPill(title: "CPU", value: String(format: "%.1f%%", entity.metrics.cpuPercent))
                    StatPill(title: "Memory", value: formatBytes(entity.metrics.memoryResidentBytes))
                    StatPill(title: "Disk", value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps))
                }
            }
        }
    }

    private var components: some View {
        GroupBox("Components") {
            LazyVStack(alignment: .leading, spacing: 12) {
                ForEach(entity.components) { component in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(component.title)
                                .font(.headline)
                            Spacer()
                            Text(String(format: "%.1f%% CPU", component.cpuPercent))
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                        }
                        Text(component.detail)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                        if component.memoryBytes > 0 {
                            Text(formatBytes(component.memoryBytes))
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .padding(.vertical, 6)
                    Divider()
                }
            }
        }
    }
}

private struct StatPill: View {
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.body.monospacedDigit())
        }
        .padding(10)
        .background(Color.secondary.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}
