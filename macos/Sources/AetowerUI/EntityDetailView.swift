import SwiftUI
import AetowerBridge

private struct DetailStatusBadge: View {
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

private let detailMetricColumns = [GridItem(.adaptive(minimum: 160), spacing: 12)]

private struct ComponentCard: View {
    let component: ComponentSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(component.title)
                        .font(.headline)
                    Text(component.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                Spacer()
                Text(String(format: "%.1f%% CPU", component.cpuPercent))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }

            LazyVGrid(columns: detailMetricColumns, alignment: .leading, spacing: 12) {
                TrendMetricCard(
                    title: "Memory",
                    value: formatBytes(component.memoryBytes),
                    subtitle: "component footprint",
                    samples: component.memoryBytes > 0 ? [Double(component.memoryBytes), Double(component.memoryBytes)] : [],
                    style: .memory
                )
                TrendMetricCard(
                    title: "CPU",
                    value: String(format: "%.1f%%", component.cpuPercent),
                    subtitle: component.kindLabel,
                    samples: component.cpuPercent > 0 ? [Double(component.cpuPercent), Double(component.cpuPercent)] : [],
                    style: .cpu
                )
            }
        }
        .padding(14)
        .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

public struct EntityDetailView: View {
    let entity: EntitySnapshot

    public init(entity: EntitySnapshot) {
        self.entity = entity
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                hero
                whyItMatters
                whatAetowerSees
                components
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .navigationTitle(entity.displayName)
    }

    private var hero: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                Text(entity.displayName)
                    .font(.system(size: 30, weight: .semibold, design: .rounded))
                Spacer()
                DetailStatusBadge(score: Double(entity.friction.totalScore))
            }

            Text(heroNarrative)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            LazyVGrid(columns: detailMetricColumns, alignment: .leading, spacing: 12) {
                TrendMetricCard(
                    title: "Friction",
                    value: String(format: "%.1f", entity.friction.totalScore),
                    subtitle: "recent score",
                    samples: entity.trend.friction.map(Double.init),
                    style: .friction
                )
                TrendMetricCard(
                    title: "CPU",
                    value: String(format: "%.1f%%", entity.metrics.cpuPercent),
                    subtitle: entity.metrics.isForeground ? "frontmost app" : "backgrounded app",
                    samples: entity.trend.cpuPercent.map(Double.init),
                    style: .cpu
                )
                TrendMetricCard(
                    title: "Memory",
                    value: formatBytes(entity.metrics.memoryResidentBytes),
                    subtitle: "\(entity.metrics.processCount) grouped processes",
                    samples: entity.trend.memoryResidentBytes.map(Double.init),
                    style: .memory
                )
                TrendMetricCard(
                    title: "Disk Activity",
                    value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps),
                    subtitle: "read + write throughput",
                    samples: entity.trend.diskActivityBps.map(Double.init),
                    style: .disk
                )
            }
        }
    }

    private var whyItMatters: some View {
        GroupBox("Why Aetower ranked this app") {
            VStack(alignment: .leading, spacing: 12) {
                if entity.friction.reasons.isEmpty {
                    Text("No strong friction reason is currently attached to this app.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(entity.friction.reasons, id: \.self) { reason in
                        HStack(alignment: .top, spacing: 10) {
                            Circle()
                                .fill(Color.accentColor)
                                .frame(width: 8, height: 8)
                                .padding(.top, 6)
                            Text(reason)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }

            }
            .padding(.top, 4)
        }
    }

    private var whatAetowerSees: some View {
        GroupBox("What Aetower sees") {
            VStack(alignment: .leading, spacing: 12) {
                LabeledContent("Entity type", value: String(describing: entity.entityKind))
                LabeledContent("Executable", value: entity.executablePath ?? "Unknown")
                LabeledContent("Frontmost", value: entity.metrics.isForeground ? "Yes" : "No")
                LabeledContent("Active window", value: entity.activeWindowTitle ?? "None detected")

                if !entity.badges.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Tags")
                            .font(.headline)
                        FlowBadgeRow(badges: entity.badges)
                    }
                }
            }
            .padding(.top, 4)
        }
    }

    private var components: some View {
        GroupBox("What is inside this app") {
            VStack(alignment: .leading, spacing: 12) {
                Text("Components are the concrete pieces Aetower could attribute to this app: helper processes, tabs, extensions, commands, or containers.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                if entity.components.isEmpty {
                    ContentUnavailableView(
                        "No component breakdown yet",
                        systemImage: "square.stack.3d.forward.dottedline",
                        description: Text("Aetower has the app-level view, but no deeper component attribution for this entity yet.")
                    )
                } else {
                    ForEach(Array(entity.components.enumerated()), id: \.offset) { _, component in
                        ComponentCard(component: component)
                    }
                }
            }
            .padding(.top, 4)
        }
    }

    private var heroNarrative: String {
        if let firstReason = entity.friction.reasons.first {
            return "\(entity.displayName) is currently ranked here because \(firstReason.lowercased())"
        }
        return "\(entity.displayName) is currently tracked, but Aetower does not yet have a dominant friction explanation for it."
    }
}

private struct FlowBadgeRow: View {
    let badges: [String]

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 8)], alignment: .leading, spacing: 8) {
            ForEach(badges, id: \.self) { badge in
                Text(badge)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.secondary.opacity(0.08), in: Capsule())
            }
        }
    }
}

private extension ComponentSnapshot {
    var kindLabel: String {
        String(describing: kind)
            .replacingOccurrences(of: "_", with: " ")
            .capitalized
    }
}
