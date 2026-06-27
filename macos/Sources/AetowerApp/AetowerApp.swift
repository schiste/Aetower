import AetowerBridge
import AetowerUI
import Sparkle
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
        false
    }

    func applicationShouldSaveApplicationState(_ app: NSApplication) -> Bool {
        false
    }

    func applicationShouldRestoreApplicationState(_ app: NSApplication) -> Bool {
        false
    }
}

private enum ActivityWorkspaceTab: String, CaseIterable, Hashable, Identifiable {
    case overview
    case history
    case timeline
    case storage

    var id: Self { self }

    var title: String {
        switch self {
        case .overview: return "Overview"
        case .history: return "History"
        case .timeline: return "Timeline"
        case .storage: return "Storage"
        }
    }

    var role: String {
        switch self {
        case .overview: return "Time-domain summary"
        case .history: return "Persisted snapshots"
        case .timeline: return "Recent events"
        case .storage: return "Store health"
        }
    }

    var summary: String {
        switch self {
        case .overview:
            return "Recent changes, event pressure, history coverage, and load status."
        case .history:
            return "Stored snapshots, trends, recurring entities, and before/after comparisons."
        case .timeline:
            return "Live events, alerts, anomalies, regressions, and recently finished processes."
        case .storage:
            return "History DB/WAL size, retention coverage, quarantine, and maintenance state."
        }
    }

    var systemImage: String {
        switch self {
        case .overview: return "rectangle.grid.2x2"
        case .history: return "clock.arrow.trianglehead.counterclockwise.rotate.90"
        case .timeline: return "timeline.selection"
        case .storage: return "externaldrive"
        }
    }
}

private enum AgentsWorkspaceTab: String, CaseIterable, Hashable, Identifiable {
    case chau7
    case aiAgents

    var id: Self { self }

    var title: String {
        switch self {
        case .chau7: return "Chau7"
        case .aiAgents: return "AI Agents"
        }
    }

    var systemImage: String {
        switch self {
        case .chau7: return "terminal"
        case .aiAgents: return "cpu"
        }
    }
}

private enum SystemWorkspaceTab: String, CaseIterable, Hashable, Identifiable {
    case sensors
    case persistence
    case diagnostics
    case fleet

    var id: Self { self }

    var title: String {
        switch self {
        case .sensors: return "Sensors"
        case .persistence: return "Startup"
        case .diagnostics: return "Diagnostics"
        case .fleet: return "Fleet"
        }
    }

    var groupTitle: String {
        switch self {
        case .sensors: return "Live Machine"
        case .persistence: return "Trust & Startup"
        case .diagnostics: return "Aetower Ops"
        case .fleet: return "Network"
        }
    }

    var role: String {
        switch self {
        case .sensors: return "Hardware health"
        case .persistence: return "Startup inventory"
        case .diagnostics: return "Self-observability"
        case .fleet: return "Peer discovery"
        }
    }

    var summary: String {
        switch self {
        case .sensors:
            return "Thermals, fans, power, storage, battery, Bluetooth, and per-core load."
        case .persistence:
            return "What starts on this Mac, what is active now, and what deserves review."
        case .diagnostics:
            return "Aetower runtime health, MCP state, payload sizes, memory, and logs."
        case .fleet:
            return "Optional local-network visibility across other Aetower instances."
        }
    }

    var detailTitle: String {
        switch self {
        case .persistence: return "Startup & persistence"
        default: return title
        }
    }

    var systemImage: String {
        switch self {
        case .sensors: return "thermometer.medium"
        case .persistence: return "lock.shield"
        case .diagnostics: return "waveform.path.ecg.rectangle"
        case .fleet: return "network"
        }
    }
}

private struct AgentsWorkspaceView: View {
    let state: AppState
    @State private var selectedTab: AgentsWorkspaceTab = .chau7
    @State private var searchText = ""

    private enum Chau7Status: Hashable {
        case enriched
        case running
        case configured
        case unavailable
    }

    var body: some View {
        VStack(spacing: 0) {
            mergedTabHeader(selection: $selectedTab)
            Divider()
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .task(id: chau7Status) {
            selectPreferredAgentView(for: chau7Status)
        }
    }

    private var chau7Status: Chau7Status {
        if !state.snapshot.chau7Sessions.isEmpty || !chau7LinkedEntities.isEmpty {
            return .enriched
        }
        if chau7Entity != nil {
            return .running
        }
        if let capability = chau7Capability,
           capability.health == .configured || capability.health == .cached || capability.health == .live
        {
            return .configured
        }
        return .unavailable
    }

    private var chau7Capability: CapabilitySnapshot? {
        state.snapshot.capabilities.first { $0.kind == .chau7 }
    }

    private var chau7Entity: EntitySnapshot? {
        state.snapshot.entities.first { entity in
            entity.bundleId == "local.chau7"
                || entity.entityId == "bundle-path:/applications/chau7.app"
                || entity.executablePath == "/Applications/Chau7.app/Contents/MacOS/Chau7"
                || entity.displayName.localizedCaseInsensitiveContains("Chau7")
        }
    }

    private var chau7LinkedEntities: [EntitySnapshot] {
        state.snapshot.entities.filter { entity in
            entity.badges.contains("chau7-live")
                || entity.components.contains { $0.adapterContext?.kind == .chau7Session }
        }
    }

    @ViewBuilder
    private var content: some View {
        if !agentSearchQuery.isEmpty {
            agentSearchResults
        } else {
            switch selectedTab {
            case .chau7:
                Chau7View(state: state)
            case .aiAgents:
                AIAgentsView(state: state)
            }
        }
    }

    private var agentSearchQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var matchingChau7Sessions: [Chau7SessionSummary] {
        let query = agentSearchQuery
        guard !query.isEmpty else { return [] }
        return state.snapshot.chau7Sessions.filter { session in
            [
                session.title,
                session.provider,
                session.status,
                session.workspacePath ?? "",
                session.repoRoot ?? "",
                session.gitBranch ?? "",
                session.activeApp ?? "",
            ]
            .contains { $0.localizedCaseInsensitiveContains(query) }
        }
    }

    private var matchingAgentEntities: [EntitySnapshot] {
        let query = agentSearchQuery
        guard !query.isEmpty else { return [] }
        return state.snapshot.entities.filter { entity in
            entity.entityKind == .aiAgent
                && [
                    entity.displayName,
                    entity.executablePath ?? "",
                    entity.bundleId ?? "",
                    entity.activeWindowTitle ?? "",
                    entity.recentChangeSummary ?? "",
                    entity.badges.joined(separator: " "),
                ]
                .contains { $0.localizedCaseInsensitiveContains(query) }
        }
    }

    private var agentSearchResults: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                if matchingChau7Sessions.isEmpty && matchingAgentEntities.isEmpty {
                    ContentUnavailableView(
                        "No agent matches",
                        systemImage: "magnifyingglass",
                        description: Text("Search tab titles, providers, repositories, branches, apps, and agent names.")
                    )
                    .frame(maxWidth: .infinity, minHeight: 220)
                } else {
                    ForEach(matchingChau7Sessions, id: \.id) { session in
                        agentSearchSessionRow(session)
                    }
                    ForEach(matchingAgentEntities, id: \.entityId) { entity in
                        agentSearchEntityRow(entity)
                    }
                }
            }
            .padding(AetowerDesign.Spacing.xxl)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func agentSearchSessionRow(_ session: Chau7SessionSummary) -> some View {
        AetowerSurface {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "terminal")
                    .foregroundStyle(AetowerDesign.Tone.cpu)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text(session.title)
                        .font(AetowerDesign.Typography.controlLabel)
                    Text([session.provider, session.gitBranch, session.repoRoot]
                        .compactMap { $0 }
                        .filter { !$0.isEmpty }
                        .joined(separator: " · "))
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                }
                Spacer()
                AetowerBadge(session.status, tone: session.isAtPrompt ? .green : .orange)
            }
        }
    }

    private func agentSearchEntityRow(_ entity: EntitySnapshot) -> some View {
        AetowerSurface {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "cpu")
                    .foregroundStyle(AetowerDesign.Tone.energy)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text(entity.displayName)
                        .font(AetowerDesign.Typography.controlLabel)
                    Text(entity.executablePath ?? entity.bundleId ?? entity.entityId)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                }
                Spacer()
                AetowerBadge(String(format: "%.0f friction", entity.friction.totalScore), tone: AetowerDesign.frictionColor(entity.friction.totalScore))
            }
        }
    }

    private func mergedTabHeader(selection: Binding<AgentsWorkspaceTab>) -> some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search agents, sessions, repos",
            searchWidth: 280
        ) {
            Picker("", selection: selection) {
                ForEach(AgentsWorkspaceTab.allCases) { tab in
                    Label(tab.title, systemImage: tab.systemImage)
                        .tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityLabel("Agents section")
            .frame(width: 260)
        } badges: {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerToolBadge(
                    "Sessions",
                    value: "\(state.snapshot.chau7Sessions.count)",
                    systemImage: "terminal",
                    tone: chau7Status == .enriched ? .green : .orange
                )
                AetowerToolBadge(
                    "Agents",
                    value: "\(state.snapshot.entities.filter { $0.entityKind == .aiAgent }.count)",
                    systemImage: "cpu",
                    tone: AetowerDesign.Tone.cpu
                )
                AetowerToolBadge(
                    "Linked",
                    value: "\(chau7LinkedEntities.count)",
                    systemImage: "link",
                    tone: chau7LinkedEntities.isEmpty ? .secondary : .green
                )
            }
        }
    }

    private func capabilityStateLabel(_ capability: CapabilitySnapshot) -> String {
        switch capability.state {
        case .granted:
            return "Granted"
        case .denied:
            return "Denied"
        case .requested:
            return "Pending"
        case .unknown:
            return "Not checked"
        case .unavailable:
            return "Unavailable"
        }
    }

    private func capabilityHealthLabel(_ health: CapabilityHealth) -> String {
        switch health {
        case .configured:
            return "Configured"
        case .live:
            return "Live"
        case .cached:
            return "Cached"
        case .degraded:
            return "Degraded"
        }
    }

    private func selectPreferredAgentView(for status: Chau7Status) {
        switch status {
        case .enriched, .running:
            selectedTab = .chau7
        case .configured, .unavailable:
            if selectedTab == .chau7 {
                selectedTab = .aiAgents
            }
        }
    }
}

private struct ActivityWorkspaceView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var selectedTab: ActivityWorkspaceTab = .overview
    @State private var searchText = ""

    var body: some View {
        VStack(spacing: 0) {
            activityHeader
            Divider()
            HStack(spacing: 0) {
                navigationRail
                Divider()
                content
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private var activityHeader: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search activity, events, history",
            searchWidth: 280
        ) {
            Picker("", selection: $selectedTab) {
                ForEach(ActivityWorkspaceTab.allCases) { tab in
                    Label(tab.title, systemImage: tab.systemImage)
                        .tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityLabel("Activity section")
            .frame(width: 360)
        } badges: {
            HStack(spacing: 8) {
                AetowerToolBadge(
                    "Events",
                    value: "\(state.snapshot.timeline.count)",
                    systemImage: "timeline.selection",
                    tone: timelineTint
                )
                AetowerToolBadge(
                    "Snapshots",
                    value: historySnapshotLabel,
                    systemImage: "clock",
                    tone: historyTint
                )
                AetowerToolBadge(
                    "Store",
                    value: historyStoreLabel,
                    systemImage: "externaldrive",
                    tone: storageTint
                )
            }
        }
    }

    private var navigationRail: some View {
        AetowerNavigationRail {
            AetowerRailGroup("Activity") {
                ForEach(visibleActivityTabs) { tab in
                    moduleButton(tab)
                }
            }
        }
    }

    private func moduleButton(_ tab: ActivityWorkspaceTab) -> some View {
        AetowerRailButton(
            title: tab.title,
            role: tab.role,
            summary: tab.summary,
            signal: moduleSignal(for: tab),
            systemImage: tab.systemImage,
            signalTone: signalTint(for: tab),
            isSelected: selectedTab == tab
        ) {
            selectedTab = tab
        }
    }

    @ViewBuilder
    private var content: some View {
        switch selectedTab {
        case .overview:
            overview
        case .history:
            HistoryView(state: state, settings: settings)
        case .timeline:
            TimelineView(state: state, settings: settings)
        case .storage:
            storage
        }
    }

    private var overview: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 170), spacing: 12)],
                    alignment: .leading,
                    spacing: 12
                ) {
                    activityMetric(
                        "Timeline events",
                        value: "\(state.snapshot.timeline.count)",
                        detail: timelinePressureDetail,
                        systemImage: "timeline.selection",
                        tone: timelineTint
                    )
                    activityMetric(
                        "Critical",
                        value: "\(criticalEventCount)",
                        detail: "\(warningEventCount) warning event(s)",
                        systemImage: "exclamationmark.triangle",
                        tone: criticalEventCount > 0 ? .red : timelineTint
                    )
                    activityMetric(
                        "Loaded history",
                        value: "\(state.historySnapshots.count)",
                        detail: state.historyIsLoading ? "loading now" : "snapshots in memory",
                        systemImage: "clock",
                        tone: historyTint
                    )
                    activityMetric(
                        "Persisted range",
                        value: historyRangeCountLabel,
                        detail: historyCoverageLabel,
                        systemImage: "tray.full",
                        tone: storageTint
                    )
                    activityMetric(
                        "Last load",
                        value: String(format: "%.0f ms", state.historyLastLoadDurationMillis),
                        detail: state.historyLoadStatus ?? "history loader idle",
                        systemImage: "speedometer",
                        tone: state.historyLastLoadDurationMillis > 750 ? .orange : .green
                    )
                    activityMetric(
                        "Safe mode",
                        value: settings.operatorSafeModeEnabled ? "On" : "Off",
                        detail: settings.operatorSafeModeEnabled ? "bounded heavy views" : "full detail by default",
                        systemImage: "shield.lefthalf.filled",
                        tone: settings.operatorSafeModeEnabled ? .green : .orange
                    )
                }

                recentTimelinePreview
                historyStatusPanel
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(24)
        }
    }

    private var recentTimelinePreview: some View {
        GroupBox("Recent timeline") {
            if recentEvents.isEmpty {
                ContentUnavailableView(
                    "No timeline events yet",
                    systemImage: "timeline.selection",
                    description: Text("Aetower will show spikes, anomalies, and state changes here as they arrive.")
                )
                .frame(maxWidth: .infinity)
            } else {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(recentEvents, id: \.id) { event in
                        HStack(alignment: .top, spacing: 10) {
                            Circle()
                                .fill(timelineSeverityColor(event.severity))
                                .frame(width: 9, height: 9)
                                .padding(.top, 6)
                            VStack(alignment: .leading, spacing: 3) {
                                HStack(spacing: 7) {
                                    Text(event.title)
                                        .font(.subheadline.weight(.semibold))
                                    Text(timelineCategoryLabel(event.category))
                                        .font(.caption2.weight(.medium))
                                        .foregroundStyle(.secondary)
                                        .padding(.horizontal, 7)
                                        .padding(.vertical, 3)
                                        .background(Color.secondary.opacity(0.08), in: Capsule())
                                }
                                if !event.detail.isEmpty {
                                    Text(event.detail)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .fixedSize(horizontal: false, vertical: true)
                                }
                                Text(relativeActivityTime(event.timestampMillis))
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                        }
                    }
                }
                .padding(.top, 6)
            }
        }
    }

    private var historyStatusPanel: some View {
        GroupBox("History status") {
            VStack(alignment: .leading, spacing: 10) {
                if let historyLoadError = state.historyLoadError {
                    Label(historyLoadError, systemImage: "exclamationmark.triangle")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                if let summary = historySummary {
                    activityDetailLine("Snapshots", "\(summary.snapshotCount) persisted · \(summary.rangeCount) in selected range")
                    activityDetailLine("Coverage", historyCoverageLabel)
                    activityDetailLine("Store", "\(formatActivityBytes(summary.storeBytes)) DB · \(formatActivityBytes(summary.walBytes)) WAL")
                    if summary.quarantineCount > 0 {
                        activityDetailLine("Quarantine", "\(summary.quarantineCount) incompatible row(s)")
                    }
                    if summary.pendingWrites > 0 {
                        activityDetailLine("Pending writes", "\(summary.pendingWrites)")
                    }
                } else {
                    Text("No persisted history summary is loaded yet. Open History to load a selected range.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.top, 6)
        }
    }

    private var storage: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 180), spacing: 12)],
                    alignment: .leading,
                    spacing: 12
                ) {
                    activityMetric(
                        "Database",
                        value: formatActivityBytes(historySummary?.storeBytes ?? 0),
                        detail: "snapshot store",
                        systemImage: "externaldrive",
                        tone: storageTint
                    )
                    activityMetric(
                        "WAL",
                        value: formatActivityBytes(historySummary?.walBytes ?? 0),
                        detail: "write-ahead log",
                        systemImage: "arrow.triangle.2.circlepath",
                        tone: walTint
                    )
                    activityMetric(
                        "Snapshots",
                        value: historySummary.map { "\($0.snapshotCount)" } ?? "0",
                        detail: "\(state.historySnapshots.count) loaded in UI",
                        systemImage: "tray.full",
                        tone: historyTint
                    )
                    activityMetric(
                        "Quarantine",
                        value: historySummary.map { "\($0.quarantineCount)" } ?? "0",
                        detail: "incompatible persisted rows",
                        systemImage: "cross.case",
                        tone: (historySummary?.quarantineCount ?? 0) > 0 ? .orange : .green
                    )
                    activityMetric(
                        "Pending writes",
                        value: historySummary.map { "\($0.pendingWrites)" } ?? "0",
                        detail: "history queue backlog",
                        systemImage: "hourglass",
                        tone: (historySummary?.pendingWrites ?? 0) > 0 ? .orange : .green
                    )
                    activityMetric(
                        "Last maintenance",
                        value: maintenanceStatusLabel,
                        detail: maintenanceDetailLabel,
                        systemImage: "wrench.and.screwdriver",
                        tone: state.historyMaintenanceReport?.cancelled == true ? .orange : .green
                    )
                }

                historyStatusPanel
                maintenancePanel
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(24)
        }
    }

    @ViewBuilder
    private var maintenancePanel: some View {
        if let maintenance = state.historyMaintenanceReport {
            GroupBox("Last maintenance pass") {
                VStack(alignment: .leading, spacing: 10) {
                    activityDetailLine("Elapsed", "\(maintenance.elapsedMillis) ms")
                    activityDetailLine("Checkpointed", maintenance.checkpointed ? "yes" : "no")
                    activityDetailLine("Vacuumed", maintenance.vacuumed ? "yes" : "no")
                    activityDetailLine("Pruned rows", "\(maintenance.prunedRows)")
                    activityDetailLine(
                        "Before",
                        "\(formatActivityBytes(maintenance.storeBytesBefore)) DB · \(formatActivityBytes(maintenance.walBytesBefore)) WAL"
                    )
                    activityDetailLine(
                        "After",
                        "\(formatActivityBytes(maintenance.storeBytesAfter)) DB · \(formatActivityBytes(maintenance.walBytesAfter)) WAL"
                    )
                    if let aggressiveReason = maintenance.aggressiveReason {
                        activityDetailLine("Aggressive reason", aggressiveReason)
                    }
                }
                .padding(.top, 6)
            }
        }
    }

    private func activityMetric(
        _ title: String,
        value: String,
        detail: String,
        systemImage: String,
        tone: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: systemImage)
                    .foregroundStyle(tone)
                Text(title.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Text(value)
                .font(.system(size: 20, weight: .semibold, design: .rounded))
                .foregroundStyle(tone)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(detail)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(2)
        }
        .padding(12)
        .frame(maxWidth: .infinity, minHeight: 112, alignment: .topLeading)
        .background(Color.secondary.opacity(0.055), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private func activityDetailLine(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
    }

    private var historySummary: HistoryRangeSummary? {
        state.historyStoreSummary ?? state.historyRangeSummary
    }

    private var historySnapshotLabel: String {
        if state.historyIsLoading {
            return "Loading"
        }
        return "\(state.historySnapshots.count)"
    }

    private var historyStoreLabel: String {
        guard let historySummary else {
            return "No summary"
        }
        return formatActivityBytes(historySummary.storeBytes + historySummary.walBytes)
    }

    private var historyRangeCountLabel: String {
        guard let historySummary else {
            return "0"
        }
        return "\(historySummary.rangeCount)"
    }

    private var historyCoverageLabel: String {
        guard let historySummary,
              let oldest = historySummary.oldestMillis,
              let newest = historySummary.newestMillis
        else {
            return "No coverage window loaded"
        }
        return "\(relativeActivityTime(oldest)) to \(relativeActivityTime(newest))"
    }

    private var timelinePressureDetail: String {
        "\(warningEventCount) warning · \(criticalEventCount) critical"
    }

    private var recentEvents: [TimelineEvent] {
        let query = activitySearchQuery
        let events = state.snapshot.timeline
            .filter { event in
                guard !query.isEmpty else { return true }
                return event.title.localizedCaseInsensitiveContains(query)
                    || event.detail.localizedCaseInsensitiveContains(query)
                    || timelineCategoryLabel(event.category).localizedCaseInsensitiveContains(query)
            }
            .sorted { $0.timestampMillis > $1.timestampMillis }
        return Array(events.prefix(5))
    }

    private var activitySearchQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var visibleActivityTabs: [ActivityWorkspaceTab] {
        let query = activitySearchQuery
        guard !query.isEmpty else {
            return ActivityWorkspaceTab.allCases
        }
        return ActivityWorkspaceTab.allCases.filter { tab in
            tab.title.localizedCaseInsensitiveContains(query)
                || tab.role.localizedCaseInsensitiveContains(query)
                || tab.summary.localizedCaseInsensitiveContains(query)
                || moduleSignal(for: tab).localizedCaseInsensitiveContains(query)
        }
    }

    private var warningEventCount: Int {
        state.snapshot.timeline.filter { $0.severity == .warning }.count
    }

    private var criticalEventCount: Int {
        state.snapshot.timeline.filter { $0.severity == .critical }.count
    }

    private var timelineTint: Color {
        if criticalEventCount > 0 { return .red }
        if warningEventCount > 0 { return .orange }
        return .green
    }

    private var historyTint: Color {
        if state.historyLoadError != nil { return .orange }
        if state.historyIsLoading { return .blue }
        return state.historySnapshots.isEmpty ? .orange : .green
    }

    private var storageTint: Color {
        let summary = historySummary
        if (summary?.quarantineCount ?? 0) > 0 || (summary?.pendingWrites ?? 0) > 0 {
            return .orange
        }
        return summary == nil ? .secondary : .green
    }

    private var walTint: Color {
        guard let summary = historySummary else {
            return .secondary
        }
        if summary.walBytes > max(summary.storeBytes / 2, 64 * 1024 * 1024) {
            return .orange
        }
        return .green
    }

    private var maintenanceStatusLabel: String {
        guard let maintenance = state.historyMaintenanceReport else {
            return "None"
        }
        if maintenance.cancelled {
            return "Cancelled"
        }
        if maintenance.vacuumed || maintenance.checkpointed || maintenance.prunedRows > 0 {
            return "Ran"
        }
        return "No-op"
    }

    private var maintenanceDetailLabel: String {
        guard let maintenance = state.historyMaintenanceReport else {
            return "no maintenance report this session"
        }
        return "\(maintenance.elapsedMillis) ms · \(maintenance.prunedRows) pruned"
    }

    private func moduleSignal(for tab: ActivityWorkspaceTab) -> String {
        switch tab {
        case .overview:
            return "\(state.snapshot.timeline.count) events · \(state.historySnapshots.count) loaded"
        case .history:
            return state.historyIsLoading ? "loading persisted snapshots" : "\(state.historySnapshots.count) loaded snapshots"
        case .timeline:
            return timelinePressureDetail
        case .storage:
            return "\(historyStoreLabel) · \(historySummary?.pendingWrites ?? 0) pending"
        }
    }

    private func signalTint(for tab: ActivityWorkspaceTab) -> Color {
        switch tab {
        case .overview:
            return timelineTint
        case .history:
            return historyTint
        case .timeline:
            return timelineTint
        case .storage:
            return storageTint
        }
    }

    private func timelineSeverityColor(_ severity: TimelineSeverity) -> Color {
        switch severity {
        case .info: return .blue
        case .warning: return .orange
        case .critical: return .red
        }
    }

    private func timelineCategoryLabel(_ category: TimelineCategory) -> String {
        switch category {
        case .lifecycle: return "lifecycle"
        case .friction: return "friction"
        case .host: return "host"
        case .thermal: return "thermal"
        case .anomaly: return "anomaly"
        case .network: return "network"
        case .regression: return "regression"
        }
    }

    private func relativeActivityTime(_ millis: UInt64) -> String {
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        guard nowMillis >= millis else {
            return "just now"
        }
        let seconds = (nowMillis - millis) / 1000
        if seconds < 60 { return "\(seconds)s ago" }
        if seconds < 3_600 { return "\(seconds / 60)m ago" }
        if seconds < 86_400 { return "\(seconds / 3_600)h ago" }
        return "\(seconds / 86_400)d ago"
    }

    private func formatActivityBytes(_ bytes: UInt64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .binary
        formatter.allowedUnits = [.useKB, .useMB, .useGB]
        formatter.isAdaptive = true
        return formatter.string(fromByteCount: Int64(bytes))
    }
}

private struct SystemWorkspaceView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var selectedTab: SystemWorkspaceTab = .sensors
    @State private var searchText = ""

    private let groupedTabs: [(title: String, tabs: [SystemWorkspaceTab])] = [
        ("Live Machine", [.sensors]),
        ("Trust & Startup", [.persistence]),
        ("Aetower Ops", [.diagnostics]),
        ("Network", [.fleet]),
    ]

    var body: some View {
        VStack(spacing: 0) {
            systemHeader
            Divider()
            HStack(spacing: 0) {
                navigationRail
                Divider()
                content
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    private var systemHeader: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search system, startup, diagnostics",
            searchWidth: 280
        ) {
            Picker("", selection: $selectedTab) {
                ForEach(SystemWorkspaceTab.allCases) { tab in
                    Label(tab.title, systemImage: tab.systemImage)
                        .tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityLabel("System section")
            .frame(width: 360)
        } badges: {
            HStack(spacing: 8) {
                AetowerToolBadge(
                    "Thermal",
                    value: thermalLabel,
                    systemImage: "thermometer.medium",
                    tone: thermalTint
                )
                AetowerToolBadge(
                    "Sensors",
                    value: sensorCoverageLabel,
                    systemImage: "sensor",
                    tone: sensorCoverageTint
                )
                AetowerToolBadge(
                    "Diagnostics",
                    value: diagnosticsLabel,
                    systemImage: "waveform.path.ecg.rectangle",
                    tone: diagnosticsTint
                )
            }
        }
    }

    private var navigationRail: some View {
        AetowerNavigationRail {
            ForEach(groupedTabs, id: \.title) { group in
                AetowerRailGroup(group.title) {
                    ForEach(group.tabs.filter(systemSearchMatches)) { tab in
                        moduleButton(tab)
                    }
                }
            }
        }
    }

    private func moduleButton(_ tab: SystemWorkspaceTab) -> some View {
        AetowerRailButton(
            title: tab.detailTitle,
            role: tab.role,
            summary: tab.summary,
            signal: moduleSignal(for: tab),
            systemImage: tab.systemImage,
            signalTone: signalTint(for: tab),
            isSelected: selectedTab == tab
        ) {
            selectedTab = tab
        }
    }

    @ViewBuilder
    private var content: some View {
        switch selectedTab {
        case .sensors:
            SensorDashboardView(state: state, settings: settings)
        case .persistence:
            PersistenceScannerView(state: state, settings: settings)
        case .diagnostics:
            DiagnosticsView(state: state, settings: settings)
        case .fleet:
            FleetView(state: state)
        }
    }

    private var thermalLabel: String {
        switch state.snapshot.host.thermalState {
        case .nominal: return "Nominal"
        case .fair: return "Fair"
        case .serious: return "Serious"
        case .critical: return "Critical"
        }
    }

    private var thermalTint: Color {
        switch state.snapshot.host.thermalState {
        case .nominal: return .green
        case .fair: return .orange
        case .serious, .critical: return .red
        }
    }

    private var sensorCoverageLabel: String {
        let host = state.snapshot.host
        let channels = [
            !host.perCoreCpu.isEmpty,
            !host.fans.isEmpty,
            !host.cpuTemperatures.isEmpty || host.gpuTemperatureCelsius != nil,
            !host.powerReadings.isEmpty,
            !host.disks.isEmpty,
            host.batteryHealth != nil,
            !host.bluetoothDevices.isEmpty,
        ].filter { $0 }.count
        return channels == 0 ? "Waiting" : "\(channels)/7 live"
    }

    private var sensorCoverageTint: Color {
        sensorCoverageLabel == "Waiting" ? .orange : .green
    }

    private var diagnosticsLabel: String {
        let overview = state.diagnosticsOverview
        if overview.errorCount > 0 {
            return "\(overview.errorCount) errors"
        }
        if overview.warnCount > 0 {
            return "\(overview.warnCount) warnings"
        }
        return "Quiet"
    }

    private var diagnosticsTint: Color {
        let overview = state.diagnosticsOverview
        if overview.errorCount > 0 { return .red }
        if overview.warnCount > 0 { return .orange }
        return .green
    }

    private func moduleSignal(for tab: SystemWorkspaceTab) -> String {
        switch tab {
        case .sensors:
            return "\(thermalLabel) thermal · \(sensorCoverageLabel)"
        case .persistence:
            return "Fast scan opens lazily · deep audit on demand"
        case .diagnostics:
            return "\(diagnosticsLabel) · \(state.diagnosticsOverview.currentSize) buffered events"
        case .fleet:
            return "Opt-in Bonjour discovery"
        }
    }

    private var systemSearchQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func systemSearchMatches(_ tab: SystemWorkspaceTab) -> Bool {
        let query = systemSearchQuery
        guard !query.isEmpty else { return true }
        return tab.title.localizedCaseInsensitiveContains(query)
            || tab.detailTitle.localizedCaseInsensitiveContains(query)
            || tab.role.localizedCaseInsensitiveContains(query)
            || tab.summary.localizedCaseInsensitiveContains(query)
            || tab.groupTitle.localizedCaseInsensitiveContains(query)
            || moduleSignal(for: tab).localizedCaseInsensitiveContains(query)
    }

    private func signalTint(for tab: SystemWorkspaceTab) -> Color {
        switch tab {
        case .sensors:
            return thermalTint
        case .persistence:
            return .orange
        case .diagnostics:
            return diagnosticsTint
        case .fleet:
            return .blue
        }
    }
}

private struct NotificationSettingsSignature: Equatable {
    let enabled: Bool
    let threshold: Double
    let thermal: Bool
    let regression: Bool
    let restartLoop: Bool
    let network: Bool
    let agentBudget: Bool

    @MainActor
    init(_ settings: SettingsStore) {
        enabled = settings.notificationsEnabled
        threshold = settings.frictionNotificationThreshold
        thermal = settings.notifyThermal
        regression = settings.notifyRegression
        restartLoop = settings.notifyRestartLoop
        network = settings.notifyNetwork
        agentBudget = settings.notifyAgentBudget
    }
}

private struct CollectionSettingsSignature: Equatable {
    let profile: CollectionProfile
    let adaptiveCadence: Bool
    let engineActiveInterval: Double
    let engineIdleInterval: Double
    let engineLowPowerInterval: Double
    let gpuSampleInterval: Double
    let gpuSampleLowPowerInterval: Double

    @MainActor
    init(_ settings: SettingsStore) {
        profile = settings.collectionProfile
        adaptiveCadence = settings.adaptiveCadenceEnabled
        engineActiveInterval = settings.engineActiveIntervalSeconds
        engineIdleInterval = settings.engineIdleIntervalSeconds
        engineLowPowerInterval = settings.engineLowPowerIntervalSeconds
        gpuSampleInterval = settings.gpuSampleIntervalSeconds
        gpuSampleLowPowerInterval = settings.gpuSampleLowPowerIntervalSeconds
    }
}

@main
struct AetowerApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var state = AppState()
    @State private var settings = SettingsStore()
    @State private var updater = UpdaterController()
    @State private var menuBarExtraInserted = false
    @State private var hudPanel: CompactHUDPanel?
    @State private var menuBarDisplayTitle = "Aetower"
    @State private var lastMenuBarTitleUpdate = Date.distantPast

    private var resolvedColorScheme: ColorScheme? {
        switch settings.appearanceMode {
        case "light": return .light
        case "dark": return .dark
        default: return nil
        }
    }

    private var notificationSettingsSignature: NotificationSettingsSignature {
        NotificationSettingsSignature(settings)
    }

    private var collectionSettingsSignature: CollectionSettingsSignature {
        CollectionSettingsSignature(settings)
    }

    private var computedMenuBarTitle: String {
        let entities = state.snapshot.entities
        if entities.isEmpty { return "Aetower" }
        let topFriction = entities.first?.friction.totalScore ?? 0
        return String(format: "%.0f", topFriction)
    }

    init() {
        UserDefaults.standard.set(false, forKey: "NSQuitAlwaysKeepsWindows")
        UserDefaults.standard.set(true, forKey: "ApplePersistenceIgnoreState")
    }

    var body: some Scene {
        WindowGroup {
            TabView {
                MainListView(state: state, settings: settings)
                    .tabItem {
                        Label("Monitor", systemImage: "gauge.with.needle")
                    }

                ActivityWorkspaceView(state: state, settings: settings)
                    .tabItem {
                        Label("Activity", systemImage: "timeline.selection")
                    }

                StorageView(state: state)
                    .tabItem {
                        Label("Storage", systemImage: "externaldrive")
                    }

                RepositoryView(state: state)
                    .tabItem {
                        Label("Repos", systemImage: "folder.badge.gearshape")
                    }

                AgentsWorkspaceView(state: state)
                    .tabItem {
                        Label("Agents", systemImage: "cpu")
                    }

                SystemWorkspaceView(state: state, settings: settings)
                    .tabItem {
                        Label("System", systemImage: "wrench.and.screwdriver")
                    }

                SettingsView(state: state, settings: settings)
                    .environment(updater)
                    .tabItem {
                        Label("Settings", systemImage: "slider.horizontal.3")
                    }
            }
            .frame(minWidth: 940, minHeight: 680)
            .preferredColorScheme(resolvedColorScheme)
            .task {
                menuBarExtraInserted = settings.showMenuBarExtra
                refreshMenuBarTitle(force: true)
                state.startLocalMcpServer(autoRegisterClients: settings.autoRegisterLocalMcpClientsEnabled)
                state.applyNotificationSettings(settings)
                state.applyRuntimeCollectionSettings(settings)
                state.applyIntegrationSettings(settings)
                // NSApplication.terminate exits before SwiftUI @State deinit,
                // so rely on the will-terminate notification to tear down the
                // MCP socket explicitly — otherwise the socket file lingers
                // between runs and the next launch must rebind over a stale
                // entry.
                NotificationCenter.default.addObserver(
                    forName: NSApplication.willTerminateNotification,
                    object: nil,
                    queue: .main
                ) { _ in
                    MainActor.assumeIsolated {
                        state.stopLocalMcpServer()
                    }
                }
                try? await Task.sleep(nanoseconds: 1_000_000_000)
                state.start(refreshInterval: settings.refreshIntervalSeconds)
            }
            .onChange(of: state.snapshot.sequence) { _, _ in
                refreshMenuBarTitle()
            }
            .onChange(of: settings.refreshIntervalSeconds) { _, newValue in
                state.start(refreshInterval: newValue)
            }
            .onChange(of: settings.showMenuBarExtra) { _, newValue in
                if menuBarExtraInserted != newValue {
                    menuBarExtraInserted = newValue
                }
                refreshMenuBarTitle(force: true)
            }
            .onChange(of: notificationSettingsSignature) { _, _ in
                state.applyNotificationSettings(settings)
            }
            .onChange(of: collectionSettingsSignature) { _, _ in
                state.applyRuntimeCollectionSettings(settings)
            }
            .onChange(of: settings.autoRegisterLocalMcpClientsEnabled) { _, _ in
                state.applyLocalMcpClientRegistrationSettings(settings)
            }
            .onDisappear {
                state.stop()
            }
        }
        .commands {
            CommandMenu("View") {
                Button("Toggle Compact HUD") {
                    if hudPanel == nil { hudPanel = CompactHUDPanel(state: state) }
                    hudPanel?.toggle()
                }
                .keyboardShortcut("h", modifiers: [.command, .shift])
            }
            CommandMenu("Updates") {
                Button("Check for Updates…") {
                    updater.checkForUpdates()
                }
                .disabled(!updater.isConfigured)
            }
        }
        MenuBarExtra(menuBarDisplayTitle, systemImage: "bolt.fill", isInserted: $menuBarExtraInserted) {
            MenuBarSummaryView(state: state)
        }
        .onChange(of: menuBarExtraInserted) { _, newValue in
            if settings.showMenuBarExtra != newValue {
                settings.showMenuBarExtra = newValue
            }
        }
        .windowStyle(.titleBar)
    }

    private func refreshMenuBarTitle(force: Bool = false) {
        if !menuBarExtraInserted && !force {
            return
        }
        let now = Date()
        let nextTitle = computedMenuBarTitle
        let previousTitle = menuBarDisplayTitle
        let elapsed = now.timeIntervalSince(lastMenuBarTitleUpdate)

        if !force {
            if previousTitle == nextTitle {
                return
            }
            if elapsed < 20,
               let previousValue = Double(previousTitle),
               let nextValue = Double(nextTitle),
               abs(previousValue - nextValue) < 5
            {
                return
            }
        }

        menuBarDisplayTitle = nextTitle
        lastMenuBarTitleUpdate = now
    }
}
