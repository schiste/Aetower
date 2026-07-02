import AetowerBridge
import SwiftUI

/// Dedicated tab for AI agent hardware impact — surfaces GPU attribution,
/// energy draw, session costs, and unified GPU memory pressure alongside
/// per-repository token and cost breakdowns from local AI runtimes.
package struct AIAgentsView: View {
    let state: AppState

    package init(state: AppState) {
        self.state = state
    }

    // MARK: - Derived data

    private struct RuntimeGroup: Identifiable {
        let provider: String
        let workspace: String?
        let agents: [EntitySnapshot]
        let totalCpuPercent: Float
        let totalMemoryBytes: UInt64
        let totalEnergyNjPerS: Double
        let totalCostUsd: Float
        let approvalCount: Int
        let delegatingCount: Int

        var id: String { "\(provider)|\(workspace ?? "-")" }
    }

    private struct BurdenLeader: Identifiable {
        let id: String
        let title: String
        let entityName: String
        let valueLabel: String
        let color: Color
        let trend: [Double]
    }

    private struct HistoricalGroupTrend: Identifiable {
        let id: String
        let provider: String
        let workspace: String?
        let latestCpuPercent: Double
        let latestMemoryBytes: UInt64
        let cpuSamples: [Double]
        let memorySamples: [Double]
    }

    private struct AgentNarrative: Identifiable {
        let id: String
        let title: String
        let detail: String
        let timestampMillis: UInt64
        let tone: Color
    }

    private struct DataBadge: Identifiable {
        let label: String
        let color: Color

        var id: String { label }
    }

    private struct Chau7SessionInfo {
        let status: String?
        let sessionId: String?
        let detail: String
        let appVersion: String?
        let buildSha: String?
        let buildTimestamp: String?
        let buildChannel: String?
    }

    private struct ApprovalItem: Identifiable {
        let entityId: String
        let displayName: String
        let sessionId: String?
        let workspace: String?
        let detail: String
        let impact: String

        var id: String { entityId }
    }

    private struct DelegationItem: Identifiable {
        let entityId: String
        let displayName: String
        let sessionId: String?
        let workspace: String?
        let childSessionCount: Int
        let detail: String
        let note: String

        var id: String { entityId }
    }

    private struct DerivedData {
        let aiAgents: [EntitySnapshot]
        let aiAgentIDs: Set<String>
        let aiLifecycleTitles: Set<String>
        let sortedAiAgents: [EntitySnapshot]
        let sortedRepoSummaries: [AiRepoSummary]
        let aiTimelineEvents: [TimelineEvent]
        let runtimeGroups: [RuntimeGroup]
        let burdenLeaders: [BurdenLeader]
        let historicalGroupTrends: [HistoricalGroupTrend]
        let approvalQueue: [ApprovalItem]
        let delegationItems: [DelegationItem]
        let narratives: [AgentNarrative]
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

    private var host: HostSnapshot { state.hostState }

    private var derived: DerivedData {
        let aiAgents = state.entitiesState.filter { $0.entityKind == .aiAgent }
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
        let sortedRepoSummaries = state.agentContextState.aiRepoSummaries.sorted {
            if $0.totalCostUsd != $1.totalCostUsd {
                return $0.totalCostUsd > $1.totalCostUsd
            }
            if $0.totalTokens != $1.totalTokens {
                return $0.totalTokens > $1.totalTokens
            }
            return $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
        }
        let aiTimelineEvents = Array(
            state.timelineState
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

        // Compute runtime groups once and pass to both the struct and
        // the historical trend builder — previously buildRuntimeGroups
        // was called twice per render.
        let runtimeGroups = buildRuntimeGroups(from: sortedAiAgents)

        return DerivedData(
            aiAgents: aiAgents,
            aiAgentIDs: aiAgentIDs,
            aiLifecycleTitles: aiLifecycleTitles,
            sortedAiAgents: sortedAiAgents,
            sortedRepoSummaries: sortedRepoSummaries,
            aiTimelineEvents: aiTimelineEvents,
            runtimeGroups: runtimeGroups,
            burdenLeaders: buildBurdenLeaders(from: sortedAiAgents),
            historicalGroupTrends: buildHistoricalGroupTrends(groups: runtimeGroups),
            approvalQueue: buildApprovalQueue(from: sortedAiAgents),
            delegationItems: buildDelegationItems(from: sortedAiAgents),
            narratives: buildNarratives(from: sortedAiAgents, timeline: aiTimelineEvents),
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
                if derived.aiAgents.isEmpty && derived.sortedRepoSummaries.isEmpty {
                    emptyState
                } else {
                    summaryStrip(derived)
                    if !derived.burdenLeaders.isEmpty {
                        burdenLeadersSection(derived.burdenLeaders)
                    }
                    historicalTrendsSection(derived.historicalGroupTrends)
                    if !derived.approvalQueue.isEmpty {
                        approvalQueueSection(derived.approvalQueue)
                    }
                    if !derived.delegationItems.isEmpty {
                        delegatedSessionsSection(derived.delegationItems)
                    }
                    if !derived.runtimeGroups.isEmpty {
                        runtimeGroupsSection(derived.runtimeGroups)
                    }
                    if !derived.sortedAiAgents.isEmpty {
                        activeAgentsSection(derived.sortedAiAgents)
                    } else {
                        inactiveAgentsState
                    }
                    if !derived.sortedRepoSummaries.isEmpty {
                        projectCostsSection(derived.sortedRepoSummaries)
                    }
                    if !derived.narratives.isEmpty {
                        recentChangesSection(derived.narratives)
                    }
                    if !derived.aiTimelineEvents.isEmpty {
                        recentActivitySection(derived.aiTimelineEvents)
                    }
                }
            }
            .padding(.bottom, AetowerDesign.Spacing.lg)
        }
        .task {
            if state.historySnapshots.isEmpty && !state.historyIsLoading {
                state.loadHistory(force: true)
            }
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
                label: "Host GPU \(Int(host.gpuPercent))%",
                icon: "gpu",
                color: AetowerDesign.Tone.gpu
            )
            summaryChip(
                label: agentFormatEnergy(njPerS: derived.totalEnergy),
                icon: "bolt.fill",
                color: AetowerDesign.Tone.energy
            )
            summaryChip(
                label: "Unified mem \(Int(derived.gpuMemoryUnifiedPercent))%",
                icon: "memorychip",
                color: gpuMemoryTone(derived.gpuMemoryUnifiedPercent)
            )
            if derived.totalCost > 0 {
                summaryChip(
                    label: String(format: "$%.2f", derived.totalCost),
                    icon: "dollarsign.circle",
                    color: AetowerDesign.Status.neutral
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
                    color: AetowerDesign.Status.warning
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

    // MARK: - Operator sections

    private func burdenLeadersSection(_ leaders: [BurdenLeader]) -> some View {
        let columns = [GridItem(.adaptive(minimum: 168), spacing: AetowerDesign.Spacing.sm)]

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Burden Leaders")
            LazyVGrid(columns: columns, alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(leaders) { leader in
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text(leader.title)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(leader.entityName)
                            .font(.subheadline)
                            .fontWeight(.semibold)
                            .lineLimit(1)
                        HStack(alignment: .firstTextBaseline) {
                            Text(leader.valueLabel)
                                .font(.caption)
                                .foregroundStyle(leader.color)
                            Spacer()
                            MiniTrendStrip(samples: leader.trend, color: leader.color)
                                .frame(width: 54, height: 18)
                        }
                    }
                    .padding(AetowerDesign.Spacing.md)
                    .background(
                        AetowerDesign.Surface.card,
                        in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                    )
                }
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    @ViewBuilder
    private func historicalTrendsSection(_ trends: [HistoricalGroupTrend]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Historical Group Trends")
            if !trends.isEmpty {
                let columns = [GridItem(.adaptive(minimum: 220), spacing: AetowerDesign.Spacing.sm)]
                LazyVGrid(columns: columns, alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(trends) { trend in
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                            HStack(alignment: .firstTextBaseline) {
                                Text(trend.provider)
                                    .font(.subheadline)
                                    .fontWeight(.semibold)
                                Spacer()
                                Text(String(format: "%.0f%% CPU", trend.latestCpuPercent))
                                    .font(.caption)
                                    .foregroundStyle(AetowerDesign.Tone.cpu)
                            }
                            if let workspace = trend.workspace {
                                Text(agentShortenPath(workspace))
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                            HStack(spacing: AetowerDesign.Spacing.md) {
                                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                                    Text("CPU")
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                    MiniTrendStrip(samples: trend.cpuSamples, color: AetowerDesign.Tone.cpu)
                                        .frame(height: 18)
                                }
                                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                                    Text(formatBytes(trend.latestMemoryBytes))
                                        .font(.caption2)
                                        .foregroundStyle(AetowerDesign.Tone.memory)
                                    MiniTrendStrip(samples: trend.memorySamples, color: AetowerDesign.Tone.memory)
                                        .frame(height: 18)
                                }
                            }
                        }
                        .padding(AetowerDesign.Spacing.md)
                        .background(
                            AetowerDesign.Surface.card,
                            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                        )
                    }
                }
            } else if state.historyIsLoading {
                loadingInfo("Loading recent persisted AI history…")
            } else {
                loadingInfo("Open the AI tab long enough for recent persisted history to load. Trend cards appear when recent AI runtime samples exist.")
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    private func approvalQueueSection(_ approvals: [ApprovalItem]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Approval Queue")
            ForEach(approvals) { item in
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack {
                        Text(item.displayName)
                            .font(.subheadline)
                            .fontWeight(.semibold)
                        Spacer()
                        if let sessionId = item.sessionId {
                            agentStateBadge("Session \(agentShortSessionId(sessionId))", color: AetowerDesign.Status.warning)
                        }
                    }
                    if let workspace = item.workspace {
                        Text(agentShortenPath(workspace))
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Text(item.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(item.impact)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                .padding(AetowerDesign.Spacing.md)
                .background(
                    AetowerDesign.Surface.card,
                    in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                )
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    private func delegatedSessionsSection(_ delegations: [DelegationItem]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Delegated Sessions")
            ForEach(delegations) { item in
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack {
                        Text(item.displayName)
                            .font(.subheadline)
                            .fontWeight(.semibold)
                        Spacer()
                        if let sessionId = item.sessionId {
                            agentStateBadge("Session \(agentShortSessionId(sessionId))", color: AetowerDesign.Status.ready)
                        }
                        agentStateBadge("\(item.childSessionCount) child", color: AetowerDesign.Status.ready)
                    }
                    if let workspace = item.workspace {
                        Text(agentShortenPath(workspace))
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Text(item.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(item.note)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                .padding(AetowerDesign.Spacing.md)
                .background(
                    AetowerDesign.Surface.card,
                    in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                )
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    private func runtimeGroupsSection(_ groups: [RuntimeGroup]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Runtime Groups")
            ForEach(groups) { group in
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(group.provider)
                            .font(.subheadline)
                            .fontWeight(.semibold)
                        if let workspace = group.workspace {
                            Text("—")
                                .foregroundStyle(.tertiary)
                            Text(agentShortenPath(workspace))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                        Spacer()
                        Text("\(group.agents.count) runtime\(group.agents.count == 1 ? "" : "s")")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        agentMetricPill(
                            label: String(format: "CPU %.0f%%", group.totalCpuPercent),
                            color: AetowerDesign.Tone.cpu
                        )
                        agentMetricPill(label: formatBytes(group.totalMemoryBytes), color: AetowerDesign.Tone.memory)
                        agentMetricPill(label: agentFormatEnergy(njPerS: group.totalEnergyNjPerS), color: AetowerDesign.Tone.energy)
                        if group.totalCostUsd > 0 {
                            agentMetricPill(
                                label: String(format: "$%.2f", group.totalCostUsd),
                                color: AetowerDesign.Status.neutral
                            )
                        }
                    }

                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        if group.approvalCount > 0 {
                            agentStateBadge("Approvals \(group.approvalCount)", color: AetowerDesign.Status.warning)
                        }
                        if group.delegatingCount > 0 {
                            agentStateBadge("Delegating \(group.delegatingCount)", color: AetowerDesign.Status.ready)
                        }
                    }
                }
                .padding(AetowerDesign.Spacing.md)
                .background(
                    AetowerDesign.Surface.card,
                    in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                )
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
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
            HStack {
                Text(entity.displayName)
                    .font(.headline)
                if let provider = agentProviderLabel(for: entity) {
                    Text("·")
                        .foregroundStyle(.tertiary)
                    Text(provider)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                if let workspace = agentProjectContext(for: entity) {
                    Text("—")
                        .foregroundStyle(.tertiary)
                    Text(agentShortenPath(workspace))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer()
            }

            dataQualityBadgeRow(for: entity)

            HStack(spacing: AetowerDesign.Spacing.md) {
                if entity.metrics.estimatedGpuPercent > 0 {
                    agentMetricPill(
                        label: "GPU \(Int(entity.metrics.estimatedGpuPercent))%",
                        color: AetowerDesign.Tone.gpu
                    )
                }
                agentMetricPill(
                    label: agentFormatEnergy(njPerS: entity.metrics.energyNjPerS),
                    color: AetowerDesign.Tone.energy
                )
                agentMetricPill(
                    label: String(format: "CPU %.0f%%", entity.metrics.cpuPercent),
                    color: AetowerDesign.Tone.cpu
                )
                agentMetricPill(
                    label: formatBytes(entity.metrics.memoryResidentBytes),
                    color: AetowerDesign.Tone.memory
                )
                if entity.metrics.wakeupsPerSecond > 0 {
                    agentMetricPill(
                        label: String(format: "%.0f wake/s", entity.metrics.wakeupsPerSecond),
                        color: AetowerDesign.Status.warning
                    )
                }
            }

            if let cost = entity.agentCost {
                HStack(spacing: AetowerDesign.Spacing.md) {
                    if cost.costUsd > 0 {
                        agentMetricPill(
                            label: String(format: "$%.2f", cost.costUsd),
                            color: AetowerDesign.Status.success
                        )
                    }
                    if cost.totalInputTokens > 0 {
                        agentMetricPill(
                            label: "In \(formatTokens(cost.totalInputTokens))",
                            color: AetowerDesign.Status.ready
                        )
                    }
                    if cost.totalOutputTokens > 0 {
                        agentMetricPill(
                            label: "Out \(formatTokens(cost.totalOutputTokens))",
                            color: AetowerDesign.Status.ready
                        )
                    }
                    if cost.totalRuns > 0 {
                        agentMetricPill(
                            label: "\(cost.totalRuns) run\(cost.totalRuns == 1 ? "" : "s")",
                            color: AetowerDesign.Status.ready
                        )
                    }
                    if cost.sessionEnergyNj > 0 {
                        agentMetricPill(
                            label: formatSessionEnergy(nj: cost.sessionEnergyNj),
                            color: AetowerDesign.Status.warning
                        )
                    }
                }
            }

            if let info = chau7SessionInfo(for: entity) {
                chau7SessionInsights(info, entity: entity)
            }

            hostBudgetSection(for: entity)

            trendStripSection(for: entity)

            if let summary = entity.recentChangeSummary, !summary.isEmpty {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: "sparkle")
                        .font(.caption2)
                        .foregroundStyle(AetowerDesign.Status.ready)
                    Text(summary)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if !entity.recommendations.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    ForEach(Array(entity.recommendations.prefix(2).enumerated()), id: \.offset) { _, recommendation in
                        HStack(alignment: .top, spacing: AetowerDesign.Spacing.xs) {
                            Image(systemName: "arrow.up.right.circle.fill")
                                .font(.caption2)
                                .foregroundStyle(AetowerDesign.Status.warning)
                                .padding(.top, AetowerDesign.Spacing.xxs)
                            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                                Text(recommendation.title)
                                    .font(.caption)
                                    .fontWeight(.medium)
                                Text(recommendation.detail)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }

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
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
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
                            .foregroundStyle(AetowerDesign.Status.neutral)
                    }
                }
                .padding(.vertical, AetowerDesign.Spacing.xs)
            }
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
    }

    // MARK: - Recent change narratives

    private func recentChangesSection(_ narratives: [AgentNarrative]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            sectionHeader("Recent Changes")
            ForEach(narratives) { narrative in
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                    Circle()
                        .fill(narrative.tone)
                        .frame(width: 7, height: 7)
                        .padding(.top, AetowerDesign.Spacing.xs)
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                        Text(narrative.title)
                            .font(.caption)
                            .fontWeight(.medium)
                        Text(narrative.detail)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text(formatTimestamp(narrative.timestampMillis))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
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
                        .frame(width: 7, height: 7)
                        .padding(.top, AetowerDesign.Spacing.xs)
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
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

    private func loadingInfo(_ message: String) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            ProgressView()
                .scaleEffect(0.8)
            Text(message)
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(AetowerDesign.Spacing.md)
        .background(
            AetowerDesign.Surface.card,
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
        )
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

    private func dataBadges(for entity: EntitySnapshot) -> [DataBadge] {
        let sessionLinked = agentSessionComponent(for: entity)?.adapterContext?.sessionId != nil
        let projectLinked = agentProjectContext(for: entity) != nil
        let gpuEstimated = entity.metrics.estimatedGpuPercent > 0

        var badges: [DataBadge] = [
            DataBadge(
                label: sessionLinked ? "Session linked" : "Partial context",
                color: sessionLinked ? AetowerDesign.Status.success : AetowerDesign.Status.warning
            ),
        ]

        if projectLinked {
            badges.append(DataBadge(label: "Project linked", color: AetowerDesign.Status.ready))
        }
        if gpuEstimated {
            badges.append(DataBadge(label: "GPU inferred", color: AetowerDesign.Tone.gpu))
        }
        if entity.badges.contains("approval-needed") {
            badges.append(DataBadge(label: "Approval needed", color: AetowerDesign.Status.warning))
        }
        if entity.badges.contains("delegating") {
            badges.append(DataBadge(label: "Delegating", color: AetowerDesign.Status.ready))
        }
        if entity.badges.contains("agent-error") {
            badges.append(DataBadge(label: "Recent error", color: AetowerDesign.Status.error))
        }

        return badges
    }

    @ViewBuilder
    private func dataQualityBadgeRow(for entity: EntitySnapshot) -> some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            ForEach(dataBadges(for: entity).prefix(4)) { badge in
                agentStateBadge(badge.label, color: badge.color)
            }
        }
    }

    private func chau7SessionInfo(for entity: EntitySnapshot) -> Chau7SessionInfo? {
        guard let component = agentSessionComponent(for: entity) else {
            return nil
        }
        return Chau7SessionInfo(
            status: component.adapterContext?.status,
            sessionId: component.adapterContext?.sessionId,
            detail: component.detail,
            appVersion: component.adapterContext?.appVersion,
            buildSha: component.adapterContext?.buildSha,
            buildTimestamp: component.adapterContext?.buildTimestamp,
            buildChannel: component.adapterContext?.buildChannel
        )
    }

    @ViewBuilder
    private func chau7SessionInsights(_ info: Chau7SessionInfo, entity: EntitySnapshot) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                if let status = info.status, !status.isEmpty {
                    agentStateBadge(status.replacingOccurrences(of: "-", with: " ").capitalized, color: agentStatusTone(status))
                }
                if let sessionId = info.sessionId, !sessionId.isEmpty {
                    agentStateBadge("Session \(agentShortSessionId(sessionId))", color: AetowerDesign.Status.ready)
                }
                if entity.badges.contains("cto-active") {
                    agentStateBadge("CTO active", color: AetowerDesign.Status.ready)
                }
                if entity.badges.contains("at-prompt") {
                    agentStateBadge("At prompt", color: AetowerDesign.Status.success)
                }
                if entity.badges.contains("shell-loading") {
                    agentStateBadge("Shell loading", color: AetowerDesign.Status.warning)
                }
                if let version = info.appVersion, !version.isEmpty {
                    agentStateBadge("v\(version)", color: AetowerDesign.Tone.memory)
                }
                if let channel = info.buildChannel, !channel.isEmpty {
                    agentStateBadge(channel.capitalized, color: AetowerDesign.Status.ready)
                }
                if let buildSha = info.buildSha, !buildSha.isEmpty {
                    agentStateBadge("#\(shortBuildSha(buildSha))", color: AetowerDesign.Status.warning)
                }
            }

            if !info.detail.isEmpty {
                Text(info.detail)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            if let buildTimestamp = info.buildTimestamp, !buildTimestamp.isEmpty {
                Text("Observed Chau7 build: \(buildTimestamp)")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }

            if !entity.attributionNotes.isEmpty {
                Text(entity.attributionNotes.prefix(2).joined(separator: " "))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func shortBuildSha(_ value: String) -> String {
        String(value.prefix(7))
    }

    private func trendStripSection(for entity: EntitySnapshot) -> some View {
        HStack(spacing: AetowerDesign.Spacing.md) {
            trendMetric(
                title: "CPU",
                value: String(format: "%.0f%%", entity.metrics.cpuPercent),
                samples: entity.trend.cpuPercent.map { Double($0) },
                color: AetowerDesign.Tone.cpu
            )
            trendMetric(
                title: "Memory",
                value: formatBytes(entity.metrics.memoryResidentBytes),
                samples: entity.trend.memoryResidentBytes.map { Double($0) },
                color: AetowerDesign.Tone.memory
            )
            trendMetric(
                title: "Wakeups",
                value: String(format: "%.0f/s", entity.metrics.wakeupsPerSecond),
                samples: entity.trend.wakeupsPerSecond.map { Double($0) },
                color: AetowerDesign.Status.warning
            )
        }
    }

    private func trendMetric(title: String, value: String, samples: [Double], color: Color) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.secondary)
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Text(value)
                    .font(.caption)
                    .foregroundStyle(color)
                MiniTrendStrip(samples: samples, color: color)
                    .frame(width: 44, height: 14)
            }
        }
    }

    private func hostBudgetSection(for entity: EntitySnapshot) -> some View {
        let cpuShare = host.cpuPercent > 0 ? Double(entity.metrics.cpuPercent) / Double(host.cpuPercent) * 100 : 0
        let memoryShare = host.memoryUsedBytes > 0 ? Double(entity.metrics.memoryResidentBytes) / Double(host.memoryUsedBytes) * 100 : 0
        let gpuShare = host.gpuPercent > 0 ? Double(entity.metrics.estimatedGpuPercent) / Double(host.gpuPercent) * 100 : 0
        let wakeupShare = host.wakeupsPerSecond > 0 ? Double(entity.metrics.wakeupsPerSecond) / Double(host.wakeupsPerSecond) * 100 : 0

        return HStack(spacing: AetowerDesign.Spacing.md) {
            budgetMetric(title: "Host CPU", value: cpuShare)
            budgetMetric(title: "Host mem", value: memoryShare)
            if entity.metrics.estimatedGpuPercent > 0 {
                budgetMetric(title: "Host GPU", value: gpuShare)
            }
            if entity.metrics.wakeupsPerSecond > 0 {
                budgetMetric(title: "Wakeups", value: wakeupShare)
            }
        }
    }

    private func budgetMetric(title: String, value: Double) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(value > 0 ? String(format: "%.0f%%", value) : "—")
                .font(.caption)
                .foregroundStyle(value >= 40 ? AetowerDesign.Status.warning : .secondary)
        }
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

    private func buildRuntimeGroups(from agents: [EntitySnapshot]) -> [RuntimeGroup] {
        let grouped = Dictionary(grouping: agents) { entity in
            let provider = agentProviderLabel(for: entity) ?? "Unknown Provider"
            let workspace = agentProjectContext(for: entity)
            return "\(provider)|\(workspace ?? "-")"
        }

        return grouped.values.compactMap { members in
            guard let first = members.first else { return nil }
            let provider = agentProviderLabel(for: first) ?? "Unknown Provider"
            let workspace = agentProjectContext(for: first)
            return RuntimeGroup(
                provider: provider,
                workspace: workspace,
                agents: members.sorted { $0.friction.totalScore > $1.friction.totalScore },
                totalCpuPercent: members.reduce(0) { $0 + $1.metrics.cpuPercent },
                totalMemoryBytes: members.reduce(0) { $0 + $1.metrics.memoryResidentBytes },
                totalEnergyNjPerS: members.reduce(0) { $0 + $1.metrics.energyNjPerS },
                totalCostUsd: members.compactMap(\.agentCost?.costUsd).reduce(0, +),
                approvalCount: members.filter { $0.badges.contains("approval-needed") }.count,
                delegatingCount: members.filter { $0.badges.contains("delegating") }.count
            )
        }
        .sorted {
            if $0.totalEnergyNjPerS != $1.totalEnergyNjPerS {
                return $0.totalEnergyNjPerS > $1.totalEnergyNjPerS
            }
            if $0.totalCpuPercent != $1.totalCpuPercent {
                return $0.totalCpuPercent > $1.totalCpuPercent
            }
            return $0.provider.localizedCaseInsensitiveCompare($1.provider) == .orderedAscending
        }
    }

    /// Build sparkline trends for the top runtime groups using persisted
    /// history snapshots. Accepts pre-computed groups to avoid calling
    /// buildRuntimeGroups a second time per render.
    private func buildHistoricalGroupTrends(groups: [RuntimeGroup]) -> [HistoricalGroupTrend] {
        guard !state.historySnapshots.isEmpty else {
            return []
        }

        return groups.prefix(4).compactMap { group in
            let samples: [(cpu: Double, memory: UInt64)] = state.historySnapshots.suffix(24).map { snapshot in
                let matching = snapshot.entities.filter { entity in
                    entity.entityKind == .aiAgent
                        && agentProviderLabel(for: entity) == group.provider
                        && agentProjectContext(for: entity) == group.workspace
                }
                return (
                    cpu: matching.reduce(0.0) { $0 + Double($1.metrics.cpuPercent) },
                    memory: matching.reduce(0) { $0 + $1.metrics.memoryResidentBytes }
                )
            }

            let latest = samples.last ?? (0, 0)
            if samples.allSatisfy({ $0.cpu == 0 && $0.memory == 0 }) {
                return nil
            }

            return HistoricalGroupTrend(
                id: group.id,
                provider: group.provider,
                workspace: group.workspace,
                latestCpuPercent: latest.cpu,
                latestMemoryBytes: latest.memory,
                cpuSamples: samples.map(\.cpu),
                memorySamples: samples.map { Double($0.memory) }
            )
        }
    }

    private func buildApprovalQueue(from agents: [EntitySnapshot]) -> [ApprovalItem] {
        agents.compactMap { entity in
            guard entity.badges.contains("approval-needed") else {
                return nil
            }

            let detail = entity.recommendations.first(where: {
                $0.title == "Resolve pending agent approval"
            })?.detail ?? entity.recentChangeSummary ?? "This runtime is waiting for approval before it can continue."

            return ApprovalItem(
                entityId: entity.entityId,
                displayName: entity.displayName,
                sessionId: agentSessionComponent(for: entity)?.adapterContext?.sessionId,
                workspace: agentProjectContext(for: entity),
                detail: detail,
                impact: impactSummary(for: entity)
            )
        }
    }

    private func buildDelegationItems(from agents: [EntitySnapshot]) -> [DelegationItem] {
        agents.compactMap { entity in
            guard entity.badges.contains("delegating") else {
                return nil
            }

            let detail = chau7SessionInfo(for: entity)?.detail ?? entity.recentChangeSummary ?? "This runtime is delegating work to child sessions."
            let childCount = extractChildSessionCount(from: detail)
            guard childCount > 0 else {
                return nil
            }

            return DelegationItem(
                entityId: entity.entityId,
                displayName: entity.displayName,
                sessionId: agentSessionComponent(for: entity)?.adapterContext?.sessionId,
                workspace: agentProjectContext(for: entity),
                childSessionCount: childCount,
                detail: detail,
                note: "\(childCount) child session\(childCount == 1 ? "" : "s") delegated from this agent."
            )
        }
    }

    private func buildBurdenLeaders(from agents: [EntitySnapshot]) -> [BurdenLeader] {
        var leaders: [BurdenLeader] = []

        if let cpu = agents.max(by: { $0.metrics.cpuPercent < $1.metrics.cpuPercent }), cpu.metrics.cpuPercent > 0 {
            leaders.append(BurdenLeader(
                id: "cpu",
                title: "Top CPU",
                entityName: cpu.displayName,
                valueLabel: String(format: "%.0f%%", cpu.metrics.cpuPercent),
                color: AetowerDesign.Tone.cpu,
                trend: cpu.trend.cpuPercent.map { Double($0) }
            ))
        }
        if let memory = agents.max(by: { $0.metrics.memoryResidentBytes < $1.metrics.memoryResidentBytes }), memory.metrics.memoryResidentBytes > 0 {
            leaders.append(BurdenLeader(
                id: "memory",
                title: "Top Memory",
                entityName: memory.displayName,
                valueLabel: formatBytes(memory.metrics.memoryResidentBytes),
                color: AetowerDesign.Tone.memory,
                trend: memory.trend.memoryResidentBytes.map { Double($0) }
            ))
        }
        if let energy = agents.max(by: { $0.metrics.energyNjPerS < $1.metrics.energyNjPerS }), energy.metrics.energyNjPerS > 0 {
            leaders.append(BurdenLeader(
                id: "energy",
                title: "Top Energy",
                entityName: energy.displayName,
                valueLabel: agentFormatEnergy(njPerS: energy.metrics.energyNjPerS),
                color: AetowerDesign.Tone.energy,
                // No dedicated energy trend array exists in MetricTrend.
                // CPU is the best physical proxy on Apple Silicon —
                // energy consumption correlates most directly with CPU
                // utilisation, not the composite friction score.
                trend: energy.trend.cpuPercent.map { Double($0) }
            ))
        }
        if let gpu = agents.max(by: { $0.metrics.estimatedGpuPercent < $1.metrics.estimatedGpuPercent }), gpu.metrics.estimatedGpuPercent > 0 {
            leaders.append(BurdenLeader(
                id: "gpu",
                title: "Top GPU",
                entityName: gpu.displayName,
                valueLabel: String(format: "%.0f%%", gpu.metrics.estimatedGpuPercent),
                color: AetowerDesign.Tone.gpu,
                // MetricTrend has no GPU-specific trend array. Friction
                // is a reasonable proxy — it blends CPU, memory, disk,
                // and wakeups into a composite "system impact" score
                // that tracks with GPU-heavy workloads' total footprint.
                trend: gpu.trend.cpuPercent.map { Double($0) }
            ))
        }
        if let wakeups = agents.max(by: { $0.metrics.wakeupsPerSecond < $1.metrics.wakeupsPerSecond }), wakeups.metrics.wakeupsPerSecond > 0 {
            leaders.append(BurdenLeader(
                id: "wakeups",
                title: "Top Wakeups",
                entityName: wakeups.displayName,
                valueLabel: String(format: "%.0f/s", wakeups.metrics.wakeupsPerSecond),
                color: AetowerDesign.Status.warning,
                trend: wakeups.trend.wakeupsPerSecond.map { Double($0) }
            ))
        }

        return leaders
    }

    private func buildNarratives(from agents: [EntitySnapshot], timeline: [TimelineEvent]) -> [AgentNarrative] {
        var items: [AgentNarrative] = agents.compactMap { entity in
            guard let summary = entity.recentChangeSummary, !summary.isEmpty else {
                return nil
            }

            let timestamp = entity.sessionMarkers.map(\.timestampMillis).max() ?? state.snapshotCapturedAtMillis
            let tone: Color
            if entity.badges.contains("agent-error") {
                tone = AetowerDesign.Status.error
            } else if entity.badges.contains("approval-needed") {
                tone = AetowerDesign.Status.warning
            } else {
                tone = AetowerDesign.Status.ready
            }

            return AgentNarrative(
                id: "entity-\(entity.entityId)",
                title: entity.displayName,
                detail: summary,
                timestampMillis: timestamp,
                tone: tone
            )
        }

        items.append(contentsOf: timeline.prefix(4).map { event in
            AgentNarrative(
                id: "timeline-\(event.id)",
                title: event.title,
                detail: event.detail.isEmpty ? "Aetower observed AI runtime activity." : event.detail,
                timestampMillis: event.timestampMillis,
                tone: severityColor(event.severity)
            )
        })

        var deduped: [String: AgentNarrative] = [:]
        deduped.reserveCapacity(items.count)
        for item in items {
            deduped[item.id] = item
        }

        return Array(
            deduped.values
                .sorted { $0.timestampMillis > $1.timestampMillis }
                .prefix(6)
        )
    }

    /// Cached regex — NSRegularExpression is expensive to compile and
    /// was previously allocated on every call to this function.
    private static let childSessionRegex: NSRegularExpression? =
        try? NSRegularExpression(pattern: #"(\d+)\s+child sessions"#, options: [.caseInsensitive])

    private func extractChildSessionCount(from detail: String) -> Int {
        guard let regex = Self.childSessionRegex else { return 0 }
        let nsRange = NSRange(detail.startIndex..<detail.endIndex, in: detail)
        guard let match = regex.firstMatch(in: detail, options: [], range: nsRange),
              let range = Range(match.range(at: 1), in: detail)
        else {
            return 0
        }
        return Int(detail[range]) ?? 0
    }

    private func impactSummary(for entity: EntitySnapshot) -> String {
        var parts: [String] = []
        if entity.metrics.cpuPercent > 0 {
            parts.append(String(format: "%.0f%% CPU", entity.metrics.cpuPercent))
        }
        if entity.metrics.wakeupsPerSecond > 0 {
            parts.append(String(format: "%.0f wake/s", entity.metrics.wakeupsPerSecond))
        }
        if entity.metrics.memoryResidentBytes > 0 {
            parts.append(formatBytes(entity.metrics.memoryResidentBytes))
        }
        if parts.isEmpty {
            parts.append(String(format: "friction %.1f", entity.friction.totalScore))
        }
        return parts.joined(separator: " · ")
    }

    // MARK: - Formatters

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

    // formatBytes: uses the shared MonitorFormatters.formatBytes()
    // which caches its ByteCountFormatter. The private fork that was
    // here allocated a fresh formatter per call with different precision.

    private func formatTokens(_ tokens: UInt64) -> String {
        if tokens >= 1_000_000 {
            return String(format: "%.1fM tok", Double(tokens) / 1_000_000)
        } else if tokens >= 1000 {
            return String(format: "%.1fk tok", Double(tokens) / 1000)
        }
        return "\(tokens) tok"
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

private struct MiniTrendStrip: View {
    let samples: [Double]
    let color: Color

    var body: some View {
        GeometryReader { geometry in
            let rect = CGRect(origin: .zero, size: geometry.size)
            ZStack {
                Capsule()
                    .fill(color.opacity(0.08))
                trendPath(in: rect)
                    .stroke(
                        color,
                        style: StrokeStyle(lineWidth: 1.5, lineCap: .round, lineJoin: .round)
                    )
            }
        }
    }

    private func trendPath(in rect: CGRect) -> Path {
        let filtered = samples.filter { $0.isFinite }
        guard filtered.count > 1 else {
            return Path()
        }

        let minValue = filtered.min() ?? 0
        let maxValue = filtered.max() ?? minValue
        let span = max(maxValue - minValue, 0.0001)
        let stepX = rect.width / CGFloat(filtered.count - 1)

        return Path { path in
            for (index, value) in filtered.enumerated() {
                let x = CGFloat(index) * stepX
                let normalized = (value - minValue) / span
                let y = rect.maxY - CGFloat(normalized) * rect.height
                if index == 0 {
                    path.move(to: CGPoint(x: x, y: y))
                } else {
                    path.addLine(to: CGPoint(x: x, y: y))
                }
            }
        }
    }
}
