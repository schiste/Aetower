import AetowerBridge
import SwiftUI

/// Dedicated tab for AI agent hardware impact — surfaces GPU attribution,
/// energy draw, session costs, and VRAM pressure alongside per-repository
/// token/cost breakdowns from the Chau7 adapter.
public struct Chau7View: View {
    let state: AppState

    public init(state: AppState) {
        self.state = state
    }

    // MARK: - Computed data

    private var aiAgents: [EntitySnapshot] {
        state.snapshot.entities.filter { $0.entityKind == .aiAgent }
    }

    private var host: HostSnapshot { state.snapshot.host }

    private var totalEnergy: Double {
        aiAgents.reduce(0) { $0 + $1.metrics.energyNjPerS }
    }

    private var totalCost: Float {
        aiAgents.compactMap(\.agentCost?.costUsd).reduce(0, +)
    }

    private var totalSessionEnergyNj: UInt64 {
        aiAgents.compactMap(\.agentCost?.sessionEnergyNj).reduce(0, +)
    }

    private var vramPercent: Double {
        guard host.memoryTotalBytes > 0 else { return 0 }
        return Double(host.gpuMemoryBytes) / Double(host.memoryTotalBytes) * 100
    }

    private var aiTimelineEvents: [TimelineEvent] {
        state.snapshot.timeline
            .filter { event in
                event.entityId.map { id in
                    aiAgents.contains { $0.entityId == id }
                } ?? (event.title.contains("GPU memory")
                    || event.title.contains("session ended")
                    || event.title.contains("energy"))
            }
            .suffix(10)
            .reversed()
    }

    // MARK: - Body

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                Text("AI & Agents")
                    .font(.title2)
                    .fontWeight(.semibold)
                    .padding(.horizontal, AetowerDesign.Spacing.lg)
                    .padding(.top, AetowerDesign.Spacing.md)

                if aiAgents.isEmpty && state.snapshot.aiRepoSummaries.isEmpty {
                    emptyState
                } else {
                    summaryStrip
                    activeAgentsSection
                    if !state.snapshot.aiRepoSummaries.isEmpty {
                        projectCostsSection
                    }
                    if !aiTimelineEvents.isEmpty {
                        recentActivitySection
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
            Text("Start an AI coding agent (Claude Code, Codex, Aider) or a local LLM server (Ollama, MLX, llama.cpp) and it will appear here.")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
        .padding(.top, AetowerDesign.Spacing.xxl)
    }

    // MARK: - Summary strip

    private var summaryStrip: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            summaryChip(
                label: "\(aiAgents.count) agent\(aiAgents.count == 1 ? "" : "s")",
                icon: "cpu",
                color: .blue
            )
            summaryChip(
                label: "GPU \(Int(host.gpuPercent))%",
                icon: "gpu",
                color: AetowerDesign.Tone.gpu
            )
            summaryChip(
                label: formatEnergy(njPerS: totalEnergy),
                icon: "bolt.fill",
                color: AetowerDesign.Tone.energy
            )
            summaryChip(
                label: "VRAM \(Int(vramPercent))%",
                icon: "memorychip",
                color: vramPercent >= 90 ? .red : vramPercent >= 75 ? .orange : .green
            )
            if totalCost > 0 {
                summaryChip(
                    label: String(format: "$%.2f", totalCost),
                    icon: "dollarsign.circle",
                    color: .green
                )
            }
            if totalSessionEnergyNj > 0 {
                summaryChip(
                    label: formatSessionEnergy(nj: totalSessionEnergyNj),
                    icon: "battery.25percent",
                    color: .orange
                )
            }
            if host.onBattery {
                summaryChip(label: "On Battery", icon: "bolt.slash.fill", color: .red)
            }
            Spacer()
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

    private var activeAgentsSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Active Agents")
            ForEach(aiAgents, id: \.entityId) { entity in
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
                if let workspace = entity.components.first(where: {
                    $0.adapterContext?.kind == .chau7Session
                })?.adapterContext?.repoRoot {
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
                            color: .green
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
                            color: .orange
                        )
                    }
                }
            }

            // Latest session marker
            if let latest = entity.sessionMarkers.last {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: latest.kind == .runStart ? "play.fill" : "stop.fill")
                        .font(.caption2)
                        .foregroundStyle(latest.kind == .runStart ? .green : .secondary)
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

    private var projectCostsSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Project Costs")
            ForEach(state.snapshot.aiRepoSummaries, id: \.repoPath) { repo in
                HStack {
                    Text(repo.displayName)
                        .font(.subheadline)
                        .lineLimit(1)
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
                            .foregroundStyle(.green)
                    }
                }
                .padding(.vertical, AetowerDesign.Spacing.xs)
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    // MARK: - Recent activity

    private var recentActivitySection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Recent Activity")
            ForEach(Array(aiTimelineEvents), id: \.id) { event in
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
        case .critical: return .red
        case .warning: return .orange
        case .info: return .blue
        }
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
        let formatter = DateFormatter()
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }
}
