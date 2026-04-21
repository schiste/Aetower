import SwiftUI
import AetowerBridge

private let detailMetricColumns = [GridItem(.adaptive(minimum: 170), spacing: 8)]

private struct ComponentMetadataLine: View {
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct ContributorRow: View {
    let contributor: FrictionContributor
    let totalScore: Double

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(contributor.label)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text(String(format: "%.1f", contributor.score))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                Text(shareLabel)
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.tertiary)
            }
            GeometryReader { geometry in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(Color.secondary.opacity(0.10))
                    Capsule()
                        .fill(contributorColor.opacity(0.85))
                        .frame(width: max(6, geometry.size.width * CGFloat(share)))
                }
            }
            .frame(height: 8)
            if !contributor.detail.isEmpty {
                Text(contributor.detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(12)
        .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private var share: Double {
        guard totalScore > 0 else { return 0 }
        return min(1.0, max(0.0, Double(contributor.score) / totalScore))
    }

    private var shareLabel: String {
        guard totalScore > 0 else { return "0%" }
        return String(format: "%.0f%% of score", share * 100.0)
    }

    private var contributorColor: Color {
        switch contributor.key {
        case "cpu":
            return .blue
        case "memory":
            return .green
        case "pressure":
            return .orange
        case "disk":
            return .pink
        case "network":
            return .teal
        case "wakeups":
            return .red
        case "foreground":
            return .purple
        default:
            return .secondary
        }
    }
}

private struct ComponentCard: View {
    let component: ComponentSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(component.title)
                        .font(.headline)
                    if let headlineDetail {
                        Text(headlineDetail)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer()
                Text(String(format: "%.1f%% CPU", component.cpuPercent))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .contentTransition(.numericText())
            }

            LazyVGrid(columns: detailMetricColumns, alignment: .leading, spacing: 12) {
                TrendMetricCard(
                    title: "Resident",
                    value: formatBytes(component.memoryBytes),
                    subtitle: "component resident memory",
                    samples: component.memoryBytes > 0 ? [Double(component.memoryBytes), Double(component.memoryBytes)] : [],
                    style: .memory,
                    minHeight: 124
                )
                if component.memoryPhysicalFootprintBytes > 0 {
                    TrendMetricCard(
                        title: "Footprint",
                        value: formatBytes(component.memoryPhysicalFootprintBytes),
                        subtitle: "macOS task footprint",
                        samples: [Double(component.memoryPhysicalFootprintBytes)],
                        style: .memory,
                        minHeight: 124
                    )
                }
                TrendMetricCard(
                    title: "CPU",
                    value: String(format: "%.1f%%", component.cpuPercent),
                    subtitle: component.kindLabel,
                    samples: component.cpuPercent > 0 ? [Double(component.cpuPercent), Double(component.cpuPercent)] : [],
                    style: .cpu,
                    minHeight: 124
                )
                if let adapterContext = component.adapterContext,
                   adapterContext.networkReceiveBps + adapterContext.networkSendBps > 0 {
                    TrendMetricCard(
                        title: "Network",
                        value: formatRate(adapterContext.networkReceiveBps + adapterContext.networkSendBps),
                        subtitle: "adapter-attributed traffic",
                        samples: [Double(adapterContext.networkReceiveBps + adapterContext.networkSendBps)],
                        style: .network,
                        minHeight: 124
                    )
                }
                if let adapterContext = component.adapterContext,
                   adapterContext.diskReadBps + adapterContext.diskWriteBps > 0 {
                    TrendMetricCard(
                        title: "Disk",
                        value: formatRate(adapterContext.diskReadBps + adapterContext.diskWriteBps),
                        subtitle: "adapter-attributed throughput",
                        samples: [Double(adapterContext.diskReadBps + adapterContext.diskWriteBps)],
                        style: .disk,
                        minHeight: 124
                    )
                }
            }

            if hasMetadata {
                Divider()

                VStack(alignment: .leading, spacing: 10) {
                    if let provenance = component.provenance {
                        ComponentMetadataLine(title: "Provenance", value: detailProvenanceLabel(provenance))
                        ComponentMetadataLine(title: "Attribution Confidence", value: attributionConfidenceLabel(provenance.confidence))
                        if !provenance.rule.isEmpty {
                            ComponentMetadataLine(title: "Attribution Rule", value: provenance.rule)
                        }
                    }
                    if let adapterContext = component.adapterContext {
                        if let status = adapterContext.status {
                            ComponentMetadataLine(title: "Status", value: status)
                        }
                        if let url = adapterContext.url {
                            ComponentMetadataLine(title: "URL", value: url)
                        }
                        if let workspacePath = adapterContext.workspacePath {
                            ComponentMetadataLine(title: "Workspace", value: workspacePath)
                        }
                        if let repoRoot = adapterContext.repoRoot {
                            ComponentMetadataLine(title: "Repo Root", value: repoRoot)
                        }
                        if let imageName = adapterContext.imageName {
                            ComponentMetadataLine(title: "Image", value: imageName)
                        }
                        if let sessionId = adapterContext.sessionId {
                            ComponentMetadataLine(title: "Session", value: sessionId)
                        }
                        if adapterContext.processCount != nil || adapterContext.connectionCount != nil {
                            let processText = adapterContext.processCount.map { "\($0) processes" }
                            let connectionText = adapterContext.connectionCount.map { "\($0) connections" }
                            let summary = [processText, connectionText].compactMap { $0 }.joined(separator: " · ")
                            if !summary.isEmpty {
                                ComponentMetadataLine(title: "Counts", value: summary)
                            }
                        }
                        if !adapterContext.ports.isEmpty {
                            ComponentMetadataLine(title: "Ports / Connections", value: adapterContext.ports.joined(separator: " · "))
                        }
                        if adapterContext.memoryLimitBytes > 0 {
                            ComponentMetadataLine(title: "Memory Limit", value: formatBytes(adapterContext.memoryLimitBytes))
                        }
                        if adapterContext.jsHeapTotalBytes > 0 {
                            ComponentMetadataLine(title: "JS Heap Capacity", value: formatBytes(adapterContext.jsHeapTotalBytes))
                        }
                        if adapterContext.domNodes > 0 || adapterContext.documents > 0 || adapterContext.frames > 0 {
                            ComponentMetadataLine(
                                title: "DOM Footprint",
                                value: "\(adapterContext.domNodes) nodes · \(adapterContext.documents) docs · \(adapterContext.frames) frames"
                            )
                        }
                    }
                    if let processId = component.processId {
                        ComponentMetadataLine(title: "PID", value: "\(processId)")
                    }
                    if let parentSummary = component.parentSummary {
                        ComponentMetadataLine(title: "Parent", value: parentSummary)
                    }
                    if let launchedBy = component.launchedBy {
                        ComponentMetadataLine(title: "Launched By", value: launchedBy)
                    }
                    if let executablePath = component.executablePath {
                        ComponentMetadataLine(title: "Executable", value: executablePath)
                    }
                    if let commandLine = component.commandLine {
                        ComponentMetadataLine(title: "Command", value: commandLine)
                    }
                    if let cwd = component.cwd {
                        ComponentMetadataLine(title: "Working Directory", value: cwd)
                    }
                }
            }
        }
        .padding(14)
        .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private var headlineDetail: String? {
        guard !component.detail.isEmpty else {
            return nil
        }
        if component.detail == component.commandLine {
            return nil
        }
        return component.detail
    }

    private var hasMetadata: Bool {
        component.provenance != nil
            || component.adapterContext != nil
            || component.processId != nil
            || component.parentSummary != nil
            || component.launchedBy != nil
            || component.executablePath != nil
            || component.commandLine != nil
            || component.cwd != nil
    }
}

public struct EntityDetailView: View {
    let entity: EntitySnapshot
    let state: AppState
    let processTreeSeedEntities: [EntitySnapshot]

    public init(
        entity: EntitySnapshot,
        state: AppState,
        processTreeSeedEntities: [EntitySnapshot]? = nil
    ) {
        self.entity = entity
        self.state = state
        self.processTreeSeedEntities = processTreeSeedEntities ?? [entity]
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                if entity.anomalyDetected {
                    HStack(spacing: 8) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                        Text("Anomaly detected — friction is unusually high for this entity right now.")
                            .font(.callout)
                    }
                    .padding(8)
                    .background(Color.orange.opacity(0.1), in: RoundedRectangle(cornerRadius: 8))
                    .transition(.push(from: .top))
                }
                if let thermal = entity.thermalContribution {
                    HStack(spacing: 8) {
                        Image(systemName: "flame.fill")
                            .foregroundStyle(.red)
                        Text(thermal)
                            .font(.callout)
                    }
                    .padding(8)
                    .background(Color.red.opacity(0.1), in: RoundedRectangle(cornerRadius: 8))
                    .transition(.push(from: .top))
                }
                if let grouping = entity.groupingSuggestion {
                    HStack(spacing: 8) {
                        Image(systemName: "arrow.triangle.merge")
                            .foregroundStyle(.blue)
                        Text(grouping)
                            .font(.callout)
                    }
                    .padding(8)
                    .background(Color.blue.opacity(0.1), in: RoundedRectangle(cornerRadius: 8))
                    .transition(.push(from: .top))
                }
                if let recentChangeSummary = entity.recentChangeSummary {
                    HStack(spacing: 8) {
                        Image(systemName: "clock.arrow.circlepath")
                            .foregroundStyle(.teal)
                        Text(recentChangeSummary)
                            .font(.callout)
                    }
                    .padding(8)
                    .background(Color.teal.opacity(0.1), in: RoundedRectangle(cornerRadius: 8))
                    .transition(.push(from: .top))
                }
                anomalyExplanation
                hero
                memorySemantics
                if let cost = entity.agentCost {
                    GroupBox("AI Agent Cost") {
                        VStack(alignment: .leading, spacing: 8) {
                            LabeledContent("Total tokens", value: "\(cost.totalInputTokens + cost.totalOutputTokens)")
                            LabeledContent("Estimated cost", value: String(format: "$%.2f", cost.costUsd))
                            LabeledContent("Total runs", value: "\(cost.totalRuns)")
                        }
                        .padding(.top, 4)
                    }
                }
                if entity.entityKind == .aiAgent { aiAgentSession }
                whyItMatters
                recommendedNextActions
                attributionCaveats
                whatAetowerSees
                processTree
                ProcessOperatorPanel(entity: entity, state: state)
                deepAnalysis
                components
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .navigationTitle(entity.displayName)
        .task(id: entity.entityId) {
            state.loadEntityStaticAnalysis(entityID: entity.entityId)
        }
    }

    private var hero: some View {
        VStack(alignment: .leading, spacing: 8) {
            LazyVGrid(columns: detailMetricColumns, alignment: .leading, spacing: 8) {
                TrendMetricCard(
                    title: "Friction",
                    value: String(format: "%.1f", entity.friction.totalScore),
                    subtitle: "recent score · \(trendWindowLabel(sampleCount: entity.trend.friction.count))",
                    samples: entity.trend.friction.map(Double.init),
                    style: .friction,
                    minHeight: 124
                )
                TrendMetricCard(
                    title: "CPU",
                    value: String(format: "%.1f%%", entity.metrics.cpuPercent),
                    subtitle: "\(entity.metrics.isForeground ? "frontmost app" : "backgrounded app") · \(trendWindowLabel(sampleCount: entity.trend.cpuPercent.count))",
                    samples: entity.trend.cpuPercent.map(Double.init),
                    style: .cpu,
                    minHeight: 124
                )
                TrendMetricCard(
                    title: "Resident Memory",
                    value: formatBytes(entity.metrics.memoryResidentBytes),
                    subtitle: "\(entity.metrics.processCount) grouped processes · \(trendWindowLabel(sampleCount: entity.trend.memoryResidentBytes.count))",
                    samples: entity.trend.memoryResidentBytes.map(Double.init),
                    style: .memory,
                    minHeight: 124
                )
                if entity.metrics.memoryPhysicalFootprintBytes > 0 {
                    TrendMetricCard(
                        title: "Physical Footprint",
                        value: formatBytes(entity.metrics.memoryPhysicalFootprintBytes),
                        subtitle: "macOS task footprint across grouped processes",
                        samples: [Double(entity.metrics.memoryPhysicalFootprintBytes)],
                        style: .memory,
                        minHeight: 124
                    )
                }
                TrendMetricCard(
                    title: "Disk Activity",
                    value: formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps),
                    subtitle: "read + write throughput · \(trendWindowLabel(sampleCount: entity.trend.diskActivityBps.count))",
                    samples: entity.trend.diskActivityBps.map(Double.init),
                    style: .disk,
                    minHeight: 124
                )
                TrendMetricCard(
                    title: "Energy Impact",
                    value: String(format: "%.1f", entity.friction.energyImpactScore),
                    subtitle: "estimated battery drain",
                    samples: [],
                    style: .energy,
                    minHeight: 124
                )
                TrendMetricCard(
                    title: "Network",
                    value: formatRate(entity.metrics.networkReceiveBps + entity.metrics.networkSendBps),
                    subtitle: "read + send throughput · \(trendWindowLabel(sampleCount: entity.trend.networkActivityBps.count))",
                    samples: entity.trend.networkActivityBps.map(Double.init),
                    style: .network,
                    minHeight: 124
                )
                TrendMetricCard(
                    title: "Wakeups",
                    value: formatWakeups(entity.metrics.wakeupsPerSecond),
                    subtitle: "timer and interrupt churn · \(trendWindowLabel(sampleCount: entity.trend.wakeupsPerSecond.count))",
                    samples: entity.trend.wakeupsPerSecond.map(Double.init),
                    style: .friction,
                    minHeight: 124
                )
            }
        }
    }

    private var memorySemantics: some View {
        GroupBox("Memory semantics") {
            VStack(alignment: .leading, spacing: 8) {
                LabeledContent("Resident", value: formatBytes(entity.metrics.memoryResidentBytes))
                if entity.metrics.memoryPhysicalFootprintBytes > 0 {
                    LabeledContent("Physical footprint", value: formatBytes(entity.metrics.memoryPhysicalFootprintBytes))
                }
                Text(
                    memoryMetricNote(
                        residentBytes: entity.metrics.memoryResidentBytes,
                        footprintBytes: entity.metrics.memoryPhysicalFootprintBytes
                    )
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.top, 4)
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

                if !entity.friction.contributors.isEmpty {
                    Divider()
                    VStack(alignment: .leading, spacing: 10) {
                        Text("Top score contributors")
                            .font(.headline)
                        ForEach(Array(entity.friction.contributors.enumerated()), id: \.offset) { _, contributor in
                            ContributorRow(
                                contributor: contributor,
                                totalScore: Double(entity.friction.totalScore)
                            )
                        }
                    }
                }

            }
            .padding(.top, 4)
        }
    }

    private var recommendedNextActions: some View {
        GroupBox("Recommended next actions") {
            VStack(alignment: .leading, spacing: 12) {
                if entity.recommendations.isEmpty {
                    Text("No action recommendation is attached right now. The current signals look more informational than urgent.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(Array(entity.recommendations.enumerated()), id: \.offset) { _, recommendation in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(recommendation.title)
                                .font(.headline)
                            Text(recommendation.detail)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
            .padding(.top, 4)
        }
    }

    private var attributionCaveats: some View {
        GroupBox("Attribution caveats") {
            VStack(alignment: .leading, spacing: 10) {
                if entity.attributionNotes.isEmpty {
                    Text("Aetower did not detect attribution caveats for this entity in the current snapshot.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(entity.attributionNotes, id: \.self) { note in
                        HStack(alignment: .top, spacing: 10) {
                            Image(systemName: "scope")
                                .foregroundStyle(.orange)
                                .padding(.top, 2)
                            Text(note)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
            .padding(.top, 4)
        }
    }

    @ViewBuilder
    private var aiAgentSession: some View {
        let agentComponents = entity.components.filter { $0.kind == .adapterContext && $0.title.contains(" · ") }
        if entity.badges.contains("chau7-live"), !agentComponents.isEmpty {
            GroupBox("AI Agent Session") {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(Array(agentComponents.enumerated()), id: \.offset) { _, component in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(component.title)
                                    .font(.headline)
                                Spacer()
                                if let status = component.adapterContext?.status {
                                    Text(status)
                                        .font(.caption.monospaced())
                                        .padding(.horizontal, 8)
                                        .padding(.vertical, 4)
                                        .background(Color.secondary.opacity(0.1), in: Capsule())
                                }
                            }
                            Text(component.detail)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            if let adapterContext = component.adapterContext,
                               let buildLine = chau7BuildLine(adapterContext)
                            {
                                Text(buildLine)
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                            if let repoRoot = component.adapterContext?.repoRoot {
                                Text(repoRoot)
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                        }
                    }
                    HStack(spacing: 12) {
                        Button("Stop Session") {
                            if let sessionId = extractSessionId() {
                                state.stopAgentSession(sessionId: sessionId, force: false)
                            }
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(.orange)

                        Button("Force Stop") {
                            if let sessionId = extractSessionId() {
                                state.stopAgentSession(sessionId: sessionId, force: true)
                            }
                        }
                        .buttonStyle(.bordered)
                        .tint(.red)
                    }
                }
                .padding(.top, 4)
            }
        }
    }

    private func extractSessionId() -> String? {
        entity.badges.first(where: { $0.hasPrefix("ai-session:") })?.replacingOccurrences(of: "ai-session:", with: "")
    }

    private func chau7BuildLine(_ context: AdapterContextSnapshot) -> String? {
        var parts = [String]()
        if let version = context.appVersion, !version.isEmpty {
            parts.append("v\(version)")
        }
        if let channel = context.buildChannel, !channel.isEmpty {
            parts.append(channel)
        }
        if let buildSha = context.buildSha, !buildSha.isEmpty {
            parts.append("#\(String(buildSha.prefix(7)))")
        }
        if let buildTimestamp = context.buildTimestamp, !buildTimestamp.isEmpty {
            parts.append(buildTimestamp)
        }
        return parts.isEmpty ? nil : "Chau7 build: \(parts.joined(separator: " · "))"
    }

    private var whatAetowerSees: some View {
        GroupBox("What Aetower sees") {
            VStack(alignment: .leading, spacing: 12) {
                LabeledContent("Entity type", value: String(describing: entity.entityKind))
                LabeledContent(
                    "Provenance",
                    value: entity.primaryProvenance.map(detailProvenanceLabel) ?? "Unknown"
                )
                LabeledContent(
                    "Attribution confidence",
                    value: entity.primaryProvenance.map { attributionConfidenceLabel($0.confidence) } ?? "Unknown"
                )
                if let provenance = entity.primaryProvenance, !provenance.rule.isEmpty {
                    LabeledContent("Attribution rule", value: provenance.rule)
                }
                if let launcherSummary = entity.launcherSummary {
                    LabeledContent("Launch lineage", value: launcherSummary)
                }
                LabeledContent("Executable", value: entity.executablePath ?? "Unknown")
                LabeledContent("Frontmost", value: entity.metrics.isForeground ? "Yes" : "No")
                LabeledContent("Active window", value: entity.activeWindowTitle ?? "None detected")
                LabeledContent("Network", value: formatRate(entity.metrics.networkReceiveBps + entity.metrics.networkSendBps))
                LabeledContent("Wakeups", value: formatWakeups(entity.metrics.wakeupsPerSecond))

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

    @ViewBuilder
    private var anomalyExplanation: some View {
        GroupBox("What changed and why") {
            VStack(alignment: .leading, spacing: 12) {
                AnalysisMetadataStrip(
                    descriptor: EntityAnalysisKind.anomalyExplanation.descriptor,
                    updatedAt: state.entityAnalysisUpdatedAt(entity.entityId, kind: .anomalyExplanation)
                )

                if state.entityAnalysisIsLoading(entity.entityId, kind: .anomalyExplanation) {
                    ProgressView("Preparing anomaly explanation…")
                } else if let explanation = state.entityAnomalyExplanations[entity.entityId] {
                    Text(explanation.summary)
                        .font(.subheadline)
                        .fixedSize(horizontal: false, vertical: true)

                    if !explanation.drivers.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Driver metrics")
                                .font(.headline)
                            ForEach(explanation.drivers) { driver in
                                VStack(alignment: .leading, spacing: 4) {
                                    HStack {
                                        Text(driver.metric.replacingOccurrences(of: "_", with: " ").capitalized)
                                            .font(.subheadline.weight(.semibold))
                                        Spacer()
                                        Text(String(format: "%+.1f", driver.delta))
                                            .font(.caption.monospacedDigit())
                                            .foregroundStyle(.secondary)
                                    }
                                    Text(driver.summary)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .fixedSize(horizontal: false, vertical: true)
                                }
                                .padding(10)
                                .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                            }
                        }
                    }

                    if !explanation.supportingEvents.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Supporting events")
                                .font(.headline)
                            ForEach(explanation.supportingEvents.prefix(3)) { event in
                                VStack(alignment: .leading, spacing: 4) {
                                    HStack {
                                        Text(event.title)
                                            .font(.caption.weight(.semibold))
                                        Spacer()
                                        Text(entityDetailTimestamp(event.timestampMillis))
                                            .font(.caption2)
                                            .foregroundStyle(.tertiary)
                                    }
                                    Text(event.detail)
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                        .fixedSize(horizontal: false, vertical: true)
                                }
                            }
                        }
                    }
                } else if let error = state.entityAnalysisError(entity.entityId, kind: .anomalyExplanation) {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Text("No active anomaly explanation is available for this entity.")
                        .foregroundStyle(.secondary)
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

    private var processTree: some View {
        GroupBox("Process Tree") {
            VStack(alignment: .leading, spacing: 12) {
                AnalysisMetadataStrip(
                    descriptor: EntityAnalysisKind.processTree.descriptor,
                    updatedAt: state.entityAnalysisUpdatedAt(entity.entityId, kind: .processTree)
                )

                if let report = state.entityProcessTreeReports[entity.entityId] {
                    HStack(spacing: 8) {
                        Text("\(report.groupedProcessCount) grouped")
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 8)
                            .padding(.vertical, 5)
                            .background(Color.secondary.opacity(0.08), in: Capsule())
                        Text("\(report.expandedProcessCount) expanded")
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 8)
                            .padding(.vertical, 5)
                            .background(Color.secondary.opacity(0.08), in: Capsule())
                        Spacer()
                    }

                    if !report.groupingReasons.isEmpty {
                        Text(report.groupingReasons.joined(separator: " · "))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    if !report.roots.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Burdened branches")
                                .font(.headline)
                            ForEach(report.roots.prefix(4)) { root in
                                ProcessTreeAnalysisRootCard(root: root)
                            }
                        }
                    }
                } else if let error = state.entityAnalysisError(entity.entityId, kind: .processTree) {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Text("Grouped by parent-child hierarchy and aggregated friction footprint so you can see which root groups are actually carrying the load.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if entity.components.isEmpty {
                    ContentUnavailableView(
                        "No process tree yet",
                        systemImage: "arrow.triangle.branch",
                        description: Text("Aetower does not have enough component attribution for this entity yet.")
                    )
                } else {
                    ProcessTreeView(
                        entity: entity,
                        allEntities: state.snapshot.entities,
                        seedEntities: processTreeSeedEntities
                    )
                }
            }
            .padding(.top, 4)
        }
    }

    private var deepAnalysis: some View {
        GroupBox("Deep analysis") {
            VStack(alignment: .leading, spacing: 14) {
                analysisActionSection(
                    title: "Memory breakdown",
                    descriptor: .memoryBreakdown,
                    isLoading: state.entityAnalysisIsLoading(entity.entityId, kind: .memoryBreakdown),
                    error: state.entityAnalysisError(entity.entityId, kind: .memoryBreakdown),
                    updatedAt: state.entityAnalysisUpdatedAt(entity.entityId, kind: .memoryBreakdown),
                    buttonTitle: "Capture vmmap breakdown"
                ) {
                    state.runEntityMemoryBreakdown(entityID: entity.entityId)
                } content: {
                    if let breakdown = state.entityMemoryBreakdowns[entity.entityId] {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(
                                breakdown.physicalFootprintBytes > 0
                                    ? "Resident \(formatBytes(breakdown.residentBytes)) · footprint \(formatBytes(breakdown.physicalFootprintBytes)) across \(breakdown.processIds.count) processes."
                                    : "Resident \(formatBytes(breakdown.residentBytes)) across \(breakdown.processIds.count) processes."
                            )
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text(breakdown.memoryMetricNote)
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .fixedSize(horizontal: false, vertical: true)
                            ForEach(breakdown.regions.prefix(6)) { region in
                                HStack {
                                    Text(region.regionType)
                                        .font(.caption.weight(.semibold))
                                    Spacer()
                                    Text(formatBytes(region.residentBytes))
                                        .font(.caption.monospacedDigit())
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }

                analysisActionSection(
                    title: "Sampled profile",
                    descriptor: .profile,
                    isLoading: state.entityAnalysisIsLoading(entity.entityId, kind: .profile),
                    error: state.entityAnalysisError(entity.entityId, kind: .profile),
                    updatedAt: state.entityAnalysisUpdatedAt(entity.entityId, kind: .profile),
                    buttonTitle: "Run 5s sample"
                ) {
                    state.runEntityProfile(entityID: entity.entityId)
                } content: {
                    if let profile = state.entityProfiles[entity.entityId] {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(profile.summary)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                            ForEach(profile.topStacks.prefix(4)) { stack in
                                SampledStackCard(stack: stack)
                            }
                        }
                    }
                }

                analysisActionSection(
                    title: "Wakeup attribution",
                    descriptor: .wakeupAttribution,
                    isLoading: state.entityAnalysisIsLoading(entity.entityId, kind: .wakeupAttribution),
                    error: state.entityAnalysisError(entity.entityId, kind: .wakeupAttribution),
                    updatedAt: state.entityAnalysisUpdatedAt(entity.entityId, kind: .wakeupAttribution),
                    buttonTitle: "Sample wakeups"
                ) {
                    state.runEntityWakeupAttribution(entityID: entity.entityId)
                } content: {
                    if let attribution = state.entityWakeupAttributions[entity.entityId] {
                        VStack(alignment: .leading, spacing: 8) {
                            if let dominantCause = attribution.dominantCause {
                                Text(dominantCause)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                            ForEach(attribution.queueBreakdown.prefix(4)) { stack in
                                SampledStackCard(stack: stack)
                            }
                            if !attribution.caveats.isEmpty {
                                Text(attribution.caveats.joined(separator: " "))
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                }
            }
            .padding(.top, 4)
        }
    }

    @ViewBuilder
    private func analysisActionSection<Content: View>(
        title: String,
        descriptor kind: EntityAnalysisKind,
        isLoading: Bool,
        error: String?,
        updatedAt: Date?,
        buttonTitle: String,
        action: @escaping () -> Void,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(title)
                    .font(.headline)
                Spacer()
                Button(buttonTitle, action: action)
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(isLoading)
            }
            AnalysisMetadataStrip(
                descriptor: kind.descriptor,
                updatedAt: updatedAt
            )
            if isLoading {
                ProgressView("Collecting \(title.lowercased())…")
            } else if let error {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                content()
            }
        }
        .padding(12)
        .background(Color.secondary.opacity(0.04), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

private struct ProcessTreeAnalysisRootCard: View {
    let root: ProcessTreeNodeReportModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(root.title)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text(String(format: "%.1f%% CPU", root.subtreeCpuPercent))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            Text("\(root.subtreeProcessCount) processes · \(formatBytes(root.subtreeMemoryBytes)) subtree memory")
                .font(.caption)
                .foregroundStyle(.secondary)
            if !root.badges.isEmpty {
                Text(root.badges.joined(separator: " · "))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(10)
        .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

private struct SampledStackCard: View {
    let stack: SampledStackReportModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(stack.queueLabel ?? stack.threadLabel)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text("\(stack.sampleCount) samples")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            Text(stack.classification)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.tertiary)
            if !stack.topFrames.isEmpty {
                Text(stack.topFrames.prefix(3).joined(separator: " → "))
                    .font(.caption2.monospaced())
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(10)
        .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
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

private func detailProvenanceLabel(_ provenance: ProvenanceSnapshot) -> String {
    switch provenance.kind {
    case .userLaunch:
        return provenance.label.isEmpty ? "User-launched app" : provenance.label
    case .appBundle:
        return provenance.label.isEmpty ? "Application bundle" : provenance.label
    case .helperTree:
        return provenance.label.isEmpty ? "Grouped app helpers" : provenance.label
    case .shellSession:
        return provenance.label.isEmpty ? "Interactive shell session" : provenance.label
    case .loginItem:
        return provenance.label.isEmpty ? "Login item" : provenance.label
    case .serviceManager:
        return provenance.label.isEmpty ? "launchd-managed service" : provenance.label
    case .xpcService:
        return provenance.label.isEmpty ? "XPC service" : provenance.label
    case .browserContext:
        return provenance.label.isEmpty ? "Browser context" : provenance.label
    case .containerWorkload:
        return provenance.label.isEmpty ? "Container workload" : provenance.label
    case .parentProcess:
        return provenance.label.isEmpty ? "Parent process" : provenance.label
    case .unknown:
        return provenance.label.isEmpty ? "Unknown" : provenance.label
    }
}

private func attributionConfidenceLabel(_ confidence: AttributionConfidence) -> String {
    switch confidence {
    case .high:
        return "High"
    case .medium:
        return "Medium"
    case .low:
        return "Low"
    }
}

private func entityDetailTimestamp(_ millis: UInt64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(millis) / 1000)
    return date.formatted(date: .abbreviated, time: .shortened)
}
