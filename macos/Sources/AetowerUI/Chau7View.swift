import AetowerBridge
import SwiftUI

/// Dedicated tab for AI agent hardware impact — surfaces GPU attribution,
/// energy draw, session costs, and unified GPU memory pressure alongside
/// per-repository token/cost breakdowns from local AI runtimes.
public struct Chau7View: View {
    let state: AppState

    public init(state: AppState) {
        self.state = state
    }

    // MARK: - Derived data

    private struct DerivedData {
        let aiAgents: [EntitySnapshot]
        let aiAgentIDs: Set<String>
        let aiLifecycleTitles: Set<String>
        let sortedAiAgents: [EntitySnapshot]
        let sortedRepoSummaries: [AiRepoSummary]
        let aiTimelineEvents: [TimelineEvent]
        let totalEnergy: Double
        let totalCost: Float
        let totalSessionEnergyNj: UInt64
        let gpuMemoryUnifiedPercent: Double
    }

    private static let fallbackTimeFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.timeStyle = .short
        return formatter
    }()

    private var host: HostSnapshot { state.snapshot.host }

    private var derived: DerivedData {
        let aiAgents = state.snapshot.entities.filter { $0.entityKind == .aiAgent }
        let aiAgentIDs = Set(aiAgents.map(\.entityId))
        let aiLifecycleTitles = Set(
            aiAgents.map { "\($0.displayName) session ended".localizedLowercase }
        )
        let sortedAiAgents = aiAgents.sorted {
            if $0.friction.totalScore != $1.friction.totalScore {
                return $0.friction.totalScore > $1.friction.totalScore
            }
            if $0.metrics.energyNjPerS != $1.metrics.energyNjPerS {
                return $0.metrics.energyNjPerS > $1.metrics.energyNjPerS
            }
            if $0.metrics.cpuPercent != $1.metrics.cpuPercent {
                return $0.metrics.cpuPercent > $1.metrics.cpuPercent
            }
            return $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
        }
        let sortedRepoSummaries = state.snapshot.aiRepoSummaries.sorted {
            if $0.totalCostUsd != $1.totalCostUsd {
                return $0.totalCostUsd > $1.totalCostUsd
            }
            if $0.totalTokens != $1.totalTokens {
                return $0.totalTokens > $1.totalTokens
            }
            return $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
        }
        let aiTimelineEvents = Array(
            state.snapshot.timeline
                .filter { isRelevantAiTimelineEvent($0, aiAgentIDs: aiAgentIDs, aiLifecycleTitles: aiLifecycleTitles) }
                .suffix(10)
                .reversed()
        )
        let totalEnergy = aiAgents.reduce(0) { $0 + $1.metrics.energyNjPerS }
        let totalCost = aiAgents.compactMap(\.agentCost?.costUsd).reduce(0, +)
        let totalSessionEnergyNj = aiAgents.compactMap(\.agentCost?.sessionEnergyNj).reduce(0, +)
        let gpuMemoryUnifiedPercent = host.memoryTotalBytes > 0
            ? Double(host.gpuMemoryBytes) / Double(host.memoryTotalBytes) * 100
            : 0

        return DerivedData(
            aiAgents: aiAgents,
            aiAgentIDs: aiAgentIDs,
            aiLifecycleTitles: aiLifecycleTitles,
            sortedAiAgents: sortedAiAgents,
            sortedRepoSummaries: sortedRepoSummaries,
            aiTimelineEvents: aiTimelineEvents,
            totalEnergy: totalEnergy,
            totalCost: totalCost,
            totalSessionEnergyNj: totalSessionEnergyNj,
            gpuMemoryUnifiedPercent: gpuMemoryUnifiedPercent
        )
    }

    // MARK: - Body

    public var body: some View {
        let derived = derived

        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                Text("AI & Agents")
                    .font(.title2)
                    .fontWeight(.semibold)
                    .padding(.horizontal, AetowerDesign.Spacing.lg)
                    .padding(.top, AetowerDesign.Spacing.md)

                if derived.aiAgents.isEmpty && derived.sortedRepoSummaries.isEmpty {
                    emptyState
                } else {
                    summaryStrip(derived)
                    if !derived.sortedAiAgents.isEmpty {
                        activeAgentsSection(derived.sortedAiAgents)
                    } else {
                        inactiveAgentsState
                    }
                    if !derived.sortedRepoSummaries.isEmpty {
                        projectCostsSection(derived.sortedRepoSummaries)
                    }
                    if !derived.aiTimelineEvents.isEmpty {
                        recentActivitySection(derived.aiTimelineEvents)
                    }
                }
            }
            .padding(.bottom, AetowerDesign.Spacing.lg)
        }
    }

    // MARK: - Empty state

    private var emptyState: some View {
        VStack(spacing: AetowerDesign.Spacing.md) {
            Image(systemName: "cpu")
                .font(.system(size: 36))
                .foregroundStyle(.secondary)
            Text("No AI agents detected")
                .font(.headline)
                .foregroundStyle(.secondary)
            Text("Start a supported AI coding agent or runtime and Aetower will surface its local hardware impact here.")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
        .padding(.top, AetowerDesign.Spacing.xxl)
    }

    private var inactiveAgentsState: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "moon.zzz")
                .font(.caption)
                .foregroundStyle(AetowerDesign.Status.ready)
            Text("No live AI runtimes right now. Historical repo cost and activity remain available below.")
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(AetowerDesign.Spacing.md)
        .background(
            AetowerDesign.Surface.card,
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
        )
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    // MARK: - Summary strip

    private func summaryStrip(_ derived: DerivedData) -> some View {
        let columns = [GridItem(.adaptive(minimum: 118), spacing: AetowerDesign.Spacing.sm)]

        return LazyVGrid(columns: columns, alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            summaryChip(
                label: "\(derived.aiAgents.count) agent\(derived.aiAgents.count == 1 ? "" : "s")",
                icon: "cpu",
                color: AetowerDesign.Tone.cpu
            )
            summaryChip(
                label: "GPU \(Int(host.gpuPercent))%",
                icon: "gpu",
                color: AetowerDesign.Tone.gpu
            )
            summaryChip(
                label: formatEnergy(njPerS: derived.totalEnergy),
                icon: "bolt.fill",
                color: AetowerDesign.Tone.energy
            )
            summaryChip(
                label: "GPU mem \(Int(derived.gpuMemoryUnifiedPercent))%",
                icon: "memorychip",
                color: gpuMemoryTone(derived.gpuMemoryUnifiedPercent)
            )
            if derived.totalCost > 0 {
                summaryChip(
                    label: String(format: "$%.2f", derived.totalCost),
                    icon: "dollarsign.circle",
                    color: AetowerDesign.Status.success
                )
            }
            if derived.totalSessionEnergyNj > 0 {
                summaryChip(
                    label: formatSessionEnergy(nj: derived.totalSessionEnergyNj),
                    icon: "battery.25percent",
                    color: AetowerDesign.Status.warning
                )
            }
            if host.onBattery {
                summaryChip(
                    label: "On Battery",
                    icon: "bolt.slash.fill",
                    color: AetowerDesign.Status.error
                )
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    private func summaryChip(label: String, icon: String, color: Color) -> some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            Image(systemName: icon)
                .font(.caption2)
                .foregroundStyle(color)
            Text(label)
                .font(.caption)
                .fontWeight(.medium)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .background(color.opacity(0.08), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
    }

    // MARK: - Active agents

    private func activeAgentsSection(_ agents: [EntitySnapshot]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("AI Runtimes")
            ForEach(agents, id: \.entityId) { entity in
                agentCard(entity)
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    private func agentCard(_ entity: EntitySnapshot) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            // Header row: name + workspace
            HStack {
                Text(entity.displayName)
                    .font(.headline)
                if let workspace = projectContext(for: entity) {
                    Text("—")
                        .foregroundStyle(.tertiary)
                    Text(shortenPath(workspace))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            // Metrics row
            HStack(spacing: AetowerDesign.Spacing.md) {
                if entity.metrics.estimatedGpuPercent > 0 {
                    metricPill(
                        label: "GPU \(Int(entity.metrics.estimatedGpuPercent))%",
                        color: AetowerDesign.Tone.gpu
                    )
                }
                metricPill(
                    label: formatEnergy(njPerS: entity.metrics.energyNjPerS),
                    color: AetowerDesign.Tone.energy
                )
                metricPill(
                    label: String(format: "CPU %.0f%%", entity.metrics.cpuPercent),
                    color: AetowerDesign.Tone.cpu
                )
                metricPill(
                    label: formatBytes(entity.metrics.memoryResidentBytes),
                    color: AetowerDesign.Tone.memory
                )
            }

            // Cost row (if available)
            if let cost = entity.agentCost {
                HStack(spacing: AetowerDesign.Spacing.md) {
                    if cost.costUsd > 0 {
                        metricPill(
                            label: String(format: "$%.2f", cost.costUsd),
                            color: AetowerDesign.Status.success
                        )
                    }
                    if cost.totalInputTokens + cost.totalOutputTokens > 0 {
                        metricPill(
                            label: formatTokens(cost.totalInputTokens + cost.totalOutputTokens),
                            color: .secondary
                        )
                    }
                    if cost.totalRuns > 0 {
                        metricPill(
                            label: "\(cost.totalRuns) run\(cost.totalRuns == 1 ? "" : "s")",
                            color: .secondary
                        )
                    }
                    if cost.sessionEnergyNj > 0 {
                        metricPill(
                            label: formatSessionEnergy(nj: cost.sessionEnergyNj),
                            color: AetowerDesign.Status.warning
                        )
                    }
                }
            }

            // Latest session marker
            if let latest = entity.sessionMarkers.max(by: { $0.timestampMillis < $1.timestampMillis }) {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: latest.kind == .runStart ? "play.fill" : "stop.fill")
                        .font(.caption2)
                        .foregroundStyle(
                            latest.kind == .runStart ? AetowerDesign.Status.success : .secondary
                        )
                    Text(latest.label)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(formatTimestamp(latest.timestampMillis))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md))
    }

    // MARK: - Project costs

    private func projectCostsSection(_ repoSummaries: [AiRepoSummary]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Project Costs")
            ForEach(repoSummaries, id: \.repoPath) { repo in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(repo.displayName)
                            .font(.subheadline)
                            .lineLimit(1)
                        if !repo.providers.isEmpty {
                            Text(repo.providers.joined(separator: ", "))
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                        }
                    }
                    Spacer()
                    Text("\(repo.totalRuns) run\(repo.totalRuns == 1 ? "" : "s")")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(formatTokens(repo.totalTokens))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if repo.totalCostUsd > 0 {
                        Text(String(format: "$%.2f", repo.totalCostUsd))
                            .font(.caption)
                            .fontWeight(.medium)
                            .foregroundStyle(AetowerDesign.Status.success)
                    }
                }
                .padding(.vertical, AetowerDesign.Spacing.xs)
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    // MARK: - Recent activity

    private func recentActivitySection(_ events: [TimelineEvent]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Recent Activity")
            ForEach(events, id: \.id) { event in
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                    Circle()
                        .fill(severityColor(event.severity))
                        .frame(width: 6, height: 6)
                        .padding(.top, 5)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(event.title)
                            .font(.caption)
                        if !event.detail.isEmpty {
                            Text(event.detail)
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .lineLimit(2)
                        }
                    }
                    Spacer()
                    Text(formatTimestamp(event.timestampMillis))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    // MARK: - Helpers

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .font(.subheadline)
            .fontWeight(.semibold)
            .foregroundStyle(.secondary)
    }

    private func metricPill(label: String, color: Color) -> some View {
        Text(label)
            .font(.caption)
            .fontWeight(.medium)
            .foregroundStyle(color)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xxs)
            .background(color.opacity(0.08), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
    }

    private func severityColor(_ severity: TimelineSeverity) -> Color {
        switch severity {
        case .critical: return AetowerDesign.Status.error
        case .warning: return AetowerDesign.Status.warning
        case .info: return AetowerDesign.Status.ready
        }
    }

    private func gpuMemoryTone(_ gpuMemoryUnifiedPercent: Double) -> Color {
        if gpuMemoryUnifiedPercent >= 90 {
            return AetowerDesign.Status.error
        }
        if gpuMemoryUnifiedPercent >= 75 {
            return AetowerDesign.Status.warning
        }
        return AetowerDesign.Status.success
    }

    private func projectContext(for entity: EntitySnapshot) -> String? {
        func rankedPath(_ component: ComponentSnapshot) -> (Int, String)? {
            if let repoRoot = component.adapterContext?.repoRoot, !repoRoot.isEmpty {
                let rank = component.adapterContext?.kind == .chau7Session ? 0 : 1
                return (rank, repoRoot)
            }

            if let workspacePath = component.adapterContext?.workspacePath, !workspacePath.isEmpty {
                let kind = component.adapterContext?.kind
                let rank = (kind == .vsCodeWorkspace || kind == .vsCodeRuntime) ? 0 : 2
                return (rank, workspacePath)
            }

            if let cwd = component.cwd, !cwd.isEmpty {
                return (3, cwd)
            }

            return nil
        }

        return entity.components
            .compactMap(rankedPath)
            .sorted { lhs, rhs in
                if lhs.0 != rhs.0 {
                    return lhs.0 < rhs.0
                }
                if lhs.1.count != rhs.1.count {
                    return lhs.1.count < rhs.1.count
                }
                return lhs.1.localizedCaseInsensitiveCompare(rhs.1) == .orderedAscending
            }
            .first?
            .1
    }

    private func isRelevantAiTimelineEvent(
        _ event: TimelineEvent,
        aiAgentIDs: Set<String>,
        aiLifecycleTitles: Set<String>
    ) -> Bool {
        if let entityId = event.entityId, aiAgentIDs.contains(entityId) {
            return true
        }

        if event.category == .host && event.title.hasPrefix("GPU memory") {
            return true
        }

        if event.category == .lifecycle,
           aiLifecycleTitles.contains(event.title.localizedLowercase) {
            return true
        }

        return false
    }

    // MARK: - Formatters

    private func formatEnergy(njPerS: Double) -> String {
        let mw = njPerS / 1_000_000
        if mw >= 1000 {
            return String(format: "%.1f W", mw / 1000)
        } else if mw >= 1 {
            return String(format: "%.0f mW", mw)
        } else {
            return "0 mW"
        }
    }

    private func formatSessionEnergy(nj: UInt64) -> String {
        let wh = Double(nj) / 3.6e12
        let mah = wh / 3.7 * 1000
        if mah >= 100 {
            return String(format: "%.0f mAh", mah)
        } else if mah >= 1 {
            return String(format: "%.1f mAh", mah)
        } else {
            let mwh = wh * 1000
            return String(format: "%.1f mWh", mwh)
        }
    }

    private func formatBytes(_ bytes: UInt64) -> String {
        let gb = Double(bytes) / (1024 * 1024 * 1024)
        if gb >= 1 {
            return String(format: "%.1f GB", gb)
        }
        let mb = Double(bytes) / (1024 * 1024)
        return String(format: "%.0f MB", mb)
    }

    private func formatTokens(_ tokens: UInt64) -> String {
        if tokens >= 1_000_000 {
            return String(format: "%.1fM tok", Double(tokens) / 1_000_000)
        } else if tokens >= 1000 {
            return String(format: "%.1fk tok", Double(tokens) / 1000)
        }
        return "\(tokens) tok"
    }

    private func shortenPath(_ path: String) -> String {
        if let home = FileManager.default.homeDirectoryForCurrentUser.path
            .removingPercentEncoding,
           path.hasPrefix(home) {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }

    private func formatTimestamp(_ millis: UInt64) -> String {
        let date = Date(timeIntervalSince1970: Double(millis) / 1000)
        let now = Date()
        let delta = now.timeIntervalSince(date)
        if delta < 60 { return "just now" }
        if delta < 3600 { return "\(Int(delta / 60))m ago" }
        if delta < 86400 { return "\(Int(delta / 3600))h ago" }
        return Self.fallbackTimeFormatter.string(from: date)
    }
}
