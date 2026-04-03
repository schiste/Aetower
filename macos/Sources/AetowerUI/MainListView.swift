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
    let isHighlighted: Bool

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
        .overlay(
            Capsule()
                .stroke(
                    isHighlighted ? Color.red.opacity(0.9) : Color.clear,
                    lineWidth: isHighlighted ? 1 : 0
                )
        )
    }
}

private enum SortKey: String, CaseIterable, Identifiable {
    case friction
    case cpu
    case memory
    case disk
    case network
    case energy
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
        case .network: return "Network"
        case .energy: return "Energy"
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
        case .network: return .teal
        case .energy: return .yellow
        case .alphabeticalAsc, .alphabeticalDesc, .oldestFirst, .newestFirst:
            return .gray
        }
    }

    var usesMetricValue: Bool {
        switch self {
        case .friction, .cpu, .memory, .disk, .network, .energy:
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
    let isHighlighted: Bool

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
        .overlay(
            Capsule()
                .stroke(
                    isHighlighted ? Color.red.opacity(0.9) : Color.clear,
                    lineWidth: isHighlighted ? 1 : 0
                )
        )
    }

}

private struct RowFrictionHighlights {
    let title: Bool
    let cpu: Bool
    let memory: Bool
    let disk: Bool
    let network: Bool
    let wakeups: Bool

    static let none = Self(title: false, cpu: false, memory: false, disk: false, network: false, wakeups: false)
}

private struct EntityRow: View {
    let entity: EntitySnapshot
    let isSelected: Bool
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: 6) {
            // Entity type icon
            Image(systemName: entityIcon)
                .font(.system(size: 11))
                .foregroundStyle(AetowerDesign.frictionColor(entity.friction.totalScore).opacity(0.8))
                .frame(width: 16)

            // Entity name (compact)
            Text(entity.displayName)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)

            // Badges
            if entity.entityKind == .aiAgent {
                Text(aiAgentProviderLabel)
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(aiAgentDotColor)
                    .padding(.horizontal, 4)
                    .padding(.vertical, 1)
                    .background(aiAgentDotColor.opacity(0.15), in: Capsule())
            }

            if entity.metrics.isForeground {
                Circle().fill(.blue).frame(width: 5, height: 5)
            }

            if entity.anomalyDetected {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 9))
                    .foregroundStyle(.orange)
                    .symbolEffect(.pulse.wholeSymbol, isActive: true)
            }

            // Metrics — right aligned, fixed width columns
            Text(String(format: "%.1f%%", entity.metrics.cpuPercent))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 48, alignment: .trailing)

            Text(formatBytes(entity.metrics.memoryResidentBytes))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 52, alignment: .trailing)

            // User — from first component
            Text(entityUser)
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: 60, alignment: .trailing)

            // Parent — from first component
            Text(entityParent)
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: 80, alignment: .trailing)

            // Friction score — bold, colored
            HStack(spacing: 2) {
                let trend = AetowerDesign.trendArrow(entity.trend.friction)
                Image(systemName: trend.symbol)
                    .font(.system(size: 8))
                    .foregroundStyle(trend.color)
                Text(String(format: "%.1f", entity.friction.totalScore))
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(entity.friction.totalScore))
                    .contentTransition(.numericText())
            }
            .frame(width: 50, alignment: .trailing)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity)
        .background(
            AetowerDesign.frictionColor(entity.friction.totalScore)
                .opacity(frictionBackgroundOpacity),
            in: RoundedRectangle(cornerRadius: 6, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .stroke(
                    isSelected ? Color.accentColor.opacity(0.5) : .clear,
                    lineWidth: 1
                )
        )
        .onHover { isHovered = $0 }
        .animation(AetowerDesign.Motion.quick, value: isHovered)
        .contextMenu {
            Button("Copy Process IDs") {
                let pids = entity.components.compactMap(\.processId).map(String.init).joined(separator: ", ")
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(pids, forType: .string)
            }
            if entity.executablePath?.contains(".app/") == true {
                Button("Show in Finder") {
                    if let path = entity.executablePath {
                        NSWorkspace.shared.selectFile(path, inFileViewerRootedAtPath: "")
                    }
                }
            }
            Button("Copy Name") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(entity.displayName, forType: .string)
            }
            Divider()
            Button("Terminate Processes", role: .destructive) {
                for component in entity.components {
                    if let pid = component.processId {
                        kill(Int32(pid), SIGTERM)
                    }
                }
            }
        }
    }

    private func metricPill(_ label: String, _ value: String) -> some View {
        HStack(spacing: 2) {
            Text(label)
                .font(.system(size: 9, weight: .medium))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption2.monospacedDigit().weight(.medium))
                .foregroundStyle(.secondary)
        }
    }

    private var aiAgentDotColor: Color {
        if entity.badges.contains("claude") { return .blue }
        if entity.badges.contains("codex") { return .green }
        if entity.badges.contains("chatgpt") { return .orange }
        return .purple
    }

    private var aiAgentProviderLabel: String {
        if entity.badges.contains("claude") { return "Claude" }
        if entity.badges.contains("codex") { return "Codex" }
        if entity.badges.contains("chatgpt") { return "ChatGPT" }
        return "AI"
    }

    private var entityUser: String {
        entity.components.first?.user ?? ""
    }

    private var entityParent: String {
        entity.components.first?.parentSummary ?? ""
    }

    private var entityIcon: String {
        switch entity.entityKind {
        case .app: return "app.fill"
        case .browser: return "globe"
        case .daemon: return "gearshape.2.fill"
        case .terminalSession: return "terminal.fill"
        case .aiAgent: return "cpu.fill"
        case .service: return "server.rack"
        case .unknown: return "questionmark.circle"
        }
    }

    private var frictionBackgroundOpacity: Double {
        let base = min(Double(entity.friction.totalScore) / 100.0, 1.0)
        if isSelected { return base * 0.12 + 0.04 }
        if isHovered { return base * 0.08 + 0.02 }
        return base * 0.05
    }
}

public struct MainListView: View {
    @ObservedObject private var state: AppState
    @State private var selectedEntityID: String?
    @State private var searchText = ""
    @State private var sortKey: SortKey = .friction
    @State private var focusedIndex: Int = 0

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        VStack(spacing: 0) {
            summaryHeader
            Divider()

            if let entity = selectedEntity {
                detailPanel(for: entity)
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            } else {
                rankingPanel
                    .transition(.opacity)
            }
        }
        .navigationTitle("Aetower")
        .modifier(KeyboardNavigationModifier(
            focusedIndex: $focusedIndex,
            selectedEntityID: $selectedEntityID,
            sortKey: $sortKey,
            entityCount: filteredEntities.count,
            entityIdAt: { index in filteredEntities[index].entityId }
        ))
    }

    private var summaryHeader: some View {
        HStack(spacing: 0) {
            // Friction score — large, colored
            HStack(spacing: 6) {
                Image(systemName: "bolt.fill")
                    .foregroundStyle(AetowerDesign.frictionColor(Float(machineFrictionScore(for: state.snapshot.host))))
                Text(String(format: "%.0f", machineFrictionScore(for: state.snapshot.host)))
                    .font(.system(size: 22, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(Float(machineFrictionScore(for: state.snapshot.host))))
                    .contentTransition(.numericText())
            }
            .padding(.horizontal, 16)

            Divider().frame(height: 24)

            // Compact metric pills
            ribbonMetric("CPU", String(format: "%.0f%%", state.snapshot.host.cpuPercent), AetowerDesign.Tone.cpu)
            ribbonMetric("MEM", formatBytes(state.snapshot.host.memoryUsedBytes), AetowerDesign.Tone.memory)
            ribbonMetric("DISK", formatRate(state.snapshot.host.diskReadBps + state.snapshot.host.diskWriteBps), AetowerDesign.Tone.disk)
            ribbonMetric("NET", formatRate(state.snapshot.host.networkReceiveBps + state.snapshot.host.networkSendBps), AetowerDesign.Tone.network)

            if state.snapshot.host.gpuPercent > 0 {
                ribbonMetric("GPU", String(format: "%.0f%%", state.snapshot.host.gpuPercent), AetowerDesign.Tone.gpu)
            }

            Spacer()

            // AI agent count (if any)
            let agentCount = state.snapshot.host.aiAgentCount
            if agentCount > 0 {
                HStack(spacing: 4) {
                    Circle().fill(.blue).frame(width: 6, height: 6)
                    Text("\(agentCount) agent\(agentCount == 1 ? "" : "s")")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, 12)
            }

            // Export + back buttons from old header
            Button {
                state.exportSnapshot()
            } label: {
                Label("Export", systemImage: "square.and.arrow.up")
                    .font(.caption.weight(.semibold))
            }
            .buttonStyle(.plain)
            .padding(.trailing, 8)

            if selectedEntity != nil {
                Button {
                    withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = nil }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "chevron.left")
                        Text("Back")
                    }
                    .font(.caption.weight(.medium))
                }
                .buttonStyle(.plain)
                .padding(.trailing, 12)
            }
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 16)
        .background(.ultraThinMaterial)
    }

    private func ribbonMetric(_ label: String, _ value: String, _ color: Color) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption.weight(.semibold).monospacedDigit())
                .foregroundStyle(color)
        }
        .padding(.horizontal, 10)
    }

    private var rankingPanel: some View {
        ScrollView {
            rankedEntitiesSection
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
        }
    }

    private func detailPanel(for entity: EntitySnapshot) -> some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .center, spacing: 12) {
                    Button {
                        withAnimation(AetowerDesign.Motion.standard) {
                            selectedEntityID = nil
                        }
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

            EntityDetailView(entity: entity, state: state)
        }
    }

    private var rankedEntitiesSection: some View {
        VStack(alignment: .leading, spacing: 2) {
            // Compact sort + search bar
            HStack(spacing: 6) {
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
                    HStack(spacing: 4) {
                        Text(sortKey.title)
                            .font(.caption.weight(.medium))
                        Image(systemName: "chevron.down")
                            .font(.system(size: 8, weight: .semibold))
                    }
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(Color.secondary.opacity(0.08), in: Capsule())
                }
                .menuStyle(.borderlessButton)
                .fixedSize()

                TextField("Search...", text: $searchText)
                    .textFieldStyle(.plain)
                    .font(.caption)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 5))
                    .frame(maxWidth: 160)

                Spacer()
            }
            .padding(.horizontal, 8)
            .padding(.bottom, 2)

            // Column headers
            HStack(spacing: 6) {
                Text("")
                    .frame(width: 16)
                Text("Name")
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text("CPU")
                    .frame(width: 48, alignment: .trailing)
                Text("MEM")
                    .frame(width: 52, alignment: .trailing)
                Text("User")
                    .frame(width: 60, alignment: .trailing)
                Text("Parent")
                    .frame(width: 80, alignment: .trailing)
                Text("Friction")
                    .frame(width: 50, alignment: .trailing)
            }
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(.tertiary)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)

            if filteredEntities.isEmpty {
                ContentUnavailableView(
                    "No apps match this filter",
                    systemImage: "magnifyingglass",
                    description: Text("Try a broader query.")
                )
            } else {
                LazyVStack(spacing: 2) {
                    ForEach(filteredEntities, id: \.entityId) { entity in
                        Button {
                            withAnimation(AetowerDesign.Motion.standard) {
                                selectedEntityID = entity.entityId
                            }
                        } label: {
                            EntityRow(
                                entity: entity,
                                isSelected: selectedEntityID == entity.entityId
                            )
                        }
                        .buttonStyle(.plain)
                        .contextMenu {
                            Button("Copy Process IDs") {
                                let pids = entity.components.compactMap { $0.processId }.map(String.init).joined(separator: ", ")
                                NSPasteboard.general.clearContents()
                                NSPasteboard.general.setString(pids, forType: .string)
                            }
                            if entity.executablePath?.contains(".app/") == true {
                                Button("Show in Finder") {
                                    if let path = entity.executablePath {
                                        NSWorkspace.shared.selectFile(path, inFileViewerRootedAtPath: "")
                                    }
                                }
                            }
                            Divider()
                            Button("Terminate Processes", role: .destructive) {
                                for component in entity.components {
                                    if let pid = component.processId {
                                        kill(Int32(pid), SIGTERM)
                                    }
                                }
                            }
                        }
                    }
                }
                .animation(AetowerDesign.Motion.standard, value: sortKey)
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
                    || entity.components.contains(where: { component in
                        component.title.localizedLowercase.contains(loweredQuery)
                            || component.detail.localizedLowercase.contains(loweredQuery)
                            || component.adapterContext?.status?.localizedLowercase.contains(loweredQuery) == true
                            || component.adapterContext?.url?.localizedLowercase.contains(loweredQuery) == true
                            || component.adapterContext?.workspacePath?.localizedLowercase.contains(loweredQuery) == true
                            || component.adapterContext?.repoRoot?.localizedLowercase.contains(loweredQuery) == true
                            || component.adapterContext?.imageName?.localizedLowercase.contains(loweredQuery) == true
                            || component.adapterContext?.sessionId?.localizedLowercase.contains(loweredQuery) == true
                            || component.adapterContext?.ports.joined(separator: " ").localizedLowercase.contains(loweredQuery) == true
                    })
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
            if entity.metrics.isForeground {
                return "\(entity.displayName) is frontmost and currently highest by friction score."
            }
            return "\(entity.displayName) is the highest-ranked background source by friction score."
        case .cpu:
            return "\(entity.displayName) is currently highest by CPU usage at \(String(format: "%.1f%%", entity.metrics.cpuPercent))."
        case .memory:
            return "\(entity.displayName) is currently highest by memory load at \(String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: state.snapshot.host.memoryTotalBytes)))."
        case .disk:
            return "\(entity.displayName) is currently highest by disk activity at \(formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps))."
        case .network:
            return "\(entity.displayName) is currently highest by network activity at \(formatRate(entity.metrics.networkReceiveBps + entity.metrics.networkSendBps))."
        case .energy:
            return "\(entity.displayName) is currently highest by energy impact at \(String(format: "%.1f", entity.friction.energyImpactScore))."
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
        case .network:
            return formatRate(entity.metrics.networkReceiveBps + entity.metrics.networkSendBps)
        case .energy:
            return String(format: "%.1f", entity.friction.energyImpactScore)
        case .alphabeticalAsc, .alphabeticalDesc:
            return nil
        case .oldestFirst:
            return ageLabel(from: entity.oldestProcessStartMillis, now: state.snapshot.capturedAtMillis)
        case .newestFirst:
            return ageLabel(from: entity.newestProcessStartMillis, now: state.snapshot.capturedAtMillis)
        }
    }

}

private func frictionHighlights(for entity: EntitySnapshot) -> RowFrictionHighlights {
    let reasons = entity.friction.reasons.map { $0.localizedLowercase }
    let isBaselineOnly = !reasons.isEmpty && reasons.allSatisfy { $0.contains("baseline activity") }
    guard !isBaselineOnly else {
        return .none
    }

    return RowFrictionHighlights(
        title: reasons.contains(where: { $0.contains("foreground app") }),
        cpu: reasons.contains(where: { $0.contains("high cpu") }),
        memory: reasons.contains(where: { $0.contains("high memory") }),
        disk: reasons.contains(where: { $0.contains("heavy disk") }),
        network: reasons.contains(where: { $0.contains("heavy network") }),
        wakeups: reasons.contains(where: { $0.contains("wakeups") })
    )
}

private func provenanceSummaryLabel(_ provenance: ProvenanceSnapshot) -> String {
    switch provenance.kind {
    case .userLaunch:
        return provenance.label.isEmpty ? "user-launched app" : provenance.label.lowercased()
    case .appBundle:
        return "application bundle"
    case .helperTree:
        return provenance.label.isEmpty ? "grouped app helpers" : provenance.label.lowercased()
    case .shellSession:
        return provenance.label.isEmpty ? "interactive shell session" : provenance.label.lowercased()
    case .loginItem:
        return provenance.label.isEmpty ? "login item" : provenance.label.lowercased()
    case .serviceManager:
        return provenance.label.isEmpty ? "launchd-managed service" : provenance.label.lowercased()
    case .xpcService:
        return provenance.label.isEmpty ? "xpc service" : provenance.label.lowercased()
    case .browserContext:
        return provenance.label.isEmpty ? "browser context" : provenance.label.lowercased()
    case .containerWorkload:
        return provenance.label.isEmpty ? "container workload" : provenance.label.lowercased()
    case .parentProcess:
        return provenance.label.isEmpty ? "parent process" : provenance.label.lowercased()
    case .unknown:
        return provenance.label.isEmpty ? "unknown" : provenance.label.lowercased()
    }
}

private func hostStatusSummary(_ host: HostSnapshot) -> String {
    var parts = [powerStatusSummary(host)]
    parts.append("thermal \(thermalStateLabel(host.thermalState))")
    if host.compressedMemoryBytes > 0 {
        parts.append("compressed \(formatBytes(host.compressedMemoryBytes))")
    }
    if host.gpuPercent > 0 || host.anePercent > 0 {
        parts.append(hostGPUSummary(host))
    }
    if host.wakeupsPerSecond > 0 {
        parts.append("\(formatWakeups(host.wakeupsPerSecond)) wakeups")
    }
    if host.lowPowerMode {
        parts.append("low power mode")
    }
    return parts.joined(separator: " · ")
}

private func powerStatusSummary(_ host: HostSnapshot) -> String {
    if host.onBattery {
        if let batteryChargePercent = host.batteryChargePercent {
            return "battery \(batteryChargePercent)%"
        }
        return "battery power"
    }
    return "AC power"
}

private func hostGPUSummary(_ host: HostSnapshot) -> String {
    if host.anePercent > 0 {
        return "ANE \(String(format: "%.1f%%", host.anePercent)) · \(formatBytes(host.gpuMemoryBytes)) GPU memory"
    }
    if host.gpuMemoryBytes > 0 {
        return "\(formatBytes(host.gpuMemoryBytes)) GPU memory"
    }
    return "render/compute activity"
}

private func thermalStateLabel(_ state: ThermalState) -> String {
    switch state {
    case .nominal:
        return "nominal"
    case .fair:
        return "fair"
    case .serious:
        return "serious"
    case .critical:
        return "critical"
    }
}

func formatBytes(_ bytes: UInt64) -> String {
    ByteFormatters.binary.string(fromByteCount: Int64(bytes))
}

func formatRate(_ bytesPerSecond: UInt64) -> String {
    "\(formatBytes(bytesPerSecond))/s"
}

func formatWakeups(_ wakeupsPerSecond: Float) -> String {
    if wakeupsPerSecond >= 100 {
        return String(format: "%.0f/s", wakeupsPerSecond)
    }
    return String(format: "%.1f/s", wakeupsPerSecond)
}

private enum ByteFormatters {
    nonisolated(unsafe) static let binary: ByteCountFormatter = {
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

private func networkTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.networkActivityBps.map(Double.init), stableText: "recent throughput")) · \(trendWindowLabel(sampleCount: entity.trend.networkActivityBps.count))"
}

private func wakeupsTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.wakeupsPerSecond.map(Double.init), stableText: "recent wakeups")) · \(trendWindowLabel(sampleCount: entity.trend.wakeupsPerSecond.count))"
}

private func machineFrictionScore(for host: HostSnapshot) -> Double {
    let cpuScore = min(Double(host.cpuPercent), 100.0) * 0.5
    let memoryRatio = host.memoryTotalBytes == 0 ? 0.0 : Double(host.memoryUsedBytes) / Double(host.memoryTotalBytes)
    let memoryScore = min(memoryRatio, 1.0) * 35.0
    let swapScore = host.swapUsedBytes == 0 ? 0.0 : (min(Double(host.swapUsedBytes) / 1_073_741_824.0, 8.0) / 8.0) * 15.0
    let compressedScore = host.memoryTotalBytes == 0 ? 0.0 : min(Double(host.compressedMemoryBytes) / Double(host.memoryTotalBytes), 1.0) * 12.0
    let networkScore = min(Double(host.networkReceiveBps + host.networkSendBps) / 8_388_608.0, 1.0) * 10.0
    let wakeupsScore = min(Double(host.wakeupsPerSecond) / 500.0, 1.0) * 8.0
    return min(cpuScore + memoryScore + swapScore + compressedScore + networkScore + wakeupsScore, 100.0)
}

private func hostCompressedSummary(_ host: HostSnapshot) -> String {
    if host.compressedMemoryBytes > 0 {
        return "compressed \(formatBytes(host.compressedMemoryBytes))"
    }
    return "memory pressure nominal"
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
    case .network:
        return (left.metrics.networkReceiveBps + left.metrics.networkSendBps) > (right.metrics.networkReceiveBps + right.metrics.networkSendBps)
    case .energy:
        return Double(left.friction.energyImpactScore) > Double(right.friction.energyImpactScore)
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

private struct KeyboardNavigationModifier: ViewModifier {
    @Binding var focusedIndex: Int
    @Binding var selectedEntityID: String?
    @Binding var sortKey: SortKey
    let entityCount: Int
    let entityIdAt: (Int) -> String

    func body(content: Content) -> some View {
        content
            .onKeyPress("j") { moveDown() }
            .onKeyPress("k") { moveUp() }
            .onKeyPress(.return) { selectCurrent() }
            .onKeyPress(.escape) { withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = nil }; return .handled }
    }

    private func moveDown() -> KeyPress.Result {
        guard entityCount > 0 else { return .ignored }
        focusedIndex = min(focusedIndex + 1, entityCount - 1)
        withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = entityIdAt(focusedIndex) }
        return .handled
    }

    private func moveUp() -> KeyPress.Result {
        guard entityCount > 0 else { return .ignored }
        focusedIndex = max(focusedIndex - 1, 0)
        withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = entityIdAt(focusedIndex) }
        return .handled
    }

    private func selectCurrent() -> KeyPress.Result {
        guard selectedEntityID == nil, entityCount > 0 else { return .handled }
        withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = entityIdAt(focusedIndex) }
        return .handled
    }
}
