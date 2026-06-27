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
        switch selectedTab {
        case .chau7:
            Chau7View(state: state)
        case .aiAgents:
            AIAgentsView(state: state)
        }
    }

    private func mergedTabHeader(selection: Binding<AgentsWorkspaceTab>) -> some View {
        HStack {
            Spacer()

            Picker("Agents section", selection: selection) {
                ForEach(AgentsWorkspaceTab.allCases) { tab in
                    Label(tab.title, systemImage: tab.systemImage)
                        .tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 260)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.bar)
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
        HStack(alignment: .top, spacing: 18) {
            Spacer()

            HStack(spacing: 8) {
                activityStatusPill(
                    title: "Events",
                    value: "\(state.snapshot.timeline.count)",
                    systemImage: "timeline.selection",
                    tint: timelineTint
                )
                activityStatusPill(
                    title: "Snapshots",
                    value: historySnapshotLabel,
                    systemImage: "clock",
                    tint: historyTint
                )
                activityStatusPill(
                    title: "Store",
                    value: historyStoreLabel,
                    systemImage: "externaldrive",
                    tint: storageTint
                )
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(.bar)
    }

    private var navigationRail: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 7) {
                Text("ACTIVITY")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 12)

                ForEach(ActivityWorkspaceTab.allCases) { tab in
                    moduleButton(tab)
                }

                Spacer(minLength: 0)
            }
            .padding(12)
        }
        .frame(width: 292)
        .background(.regularMaterial)
    }

    private func moduleButton(_ tab: ActivityWorkspaceTab) -> some View {
        let isSelected = selectedTab == tab
        return Button {
            selectedTab = tab
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: tab.systemImage)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                        .frame(width: 22)

                    VStack(alignment: .leading, spacing: 2) {
                        Text(tab.title)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.primary)
                            .lineLimit(1)
                        Text(tab.role)
                            .font(.caption2)
                            .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                    }

                    Spacer()
                }

                Text(tab.summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                Text(moduleSignal(for: tab))
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(signalTint(for: tab))
                    .lineLimit(1)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                isSelected ? Color.accentColor.opacity(0.11) : Color.secondary.opacity(0.055),
                in: RoundedRectangle(cornerRadius: 12, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(isSelected ? Color.accentColor.opacity(0.32) : Color.clear, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
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

    private func activityStatusPill(title: String, value: String, systemImage: String, tint: Color) -> some View {
        HStack(spacing: 7) {
            Image(systemName: systemImage)
                .foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 1) {
                Text(title.uppercased())
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                Text(value)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(tint.opacity(0.08), in: Capsule())
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
        Array(state.snapshot.timeline.sorted { $0.timestampMillis > $1.timestampMillis }.prefix(5))
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
        HStack(alignment: .top, spacing: 18) {
            Spacer()

            HStack(spacing: 8) {
                statusPill(
                    title: "Thermal",
                    value: thermalLabel,
                    systemImage: "thermometer.medium",
                    tint: thermalTint
                )
                statusPill(
                    title: "Sensors",
                    value: sensorCoverageLabel,
                    systemImage: "sensor",
                    tint: sensorCoverageTint
                )
                statusPill(
                    title: "Diagnostics",
                    value: diagnosticsLabel,
                    systemImage: "waveform.path.ecg.rectangle",
                    tint: diagnosticsTint
                )
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(.bar)
    }

    private var navigationRail: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                ForEach(groupedTabs, id: \.title) { group in
                    VStack(alignment: .leading, spacing: 7) {
                        Text(group.title.uppercased())
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(.tertiary)
                            .padding(.horizontal, 12)

                        ForEach(group.tabs) { tab in
                            moduleButton(tab)
                        }
                    }
                }

                Spacer(minLength: 0)
            }
            .padding(12)
        }
        .frame(width: 292)
        .background(.regularMaterial)
    }

    private func moduleButton(_ tab: SystemWorkspaceTab) -> some View {
        let isSelected = selectedTab == tab
        return Button {
            selectedTab = tab
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: tab.systemImage)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                        .frame(width: 22)

                    VStack(alignment: .leading, spacing: 2) {
                        Text(tab.detailTitle)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.primary)
                            .lineLimit(1)
                        Text(tab.role)
                            .font(.caption2)
                            .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                    }

                    Spacer()
                }

                Text(tab.summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                Text(moduleSignal(for: tab))
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(signalTint(for: tab))
                    .lineLimit(1)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                isSelected ? Color.accentColor.opacity(0.11) : Color.secondary.opacity(0.055),
                in: RoundedRectangle(cornerRadius: 12, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(isSelected ? Color.accentColor.opacity(0.32) : Color.clear, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }

    private func statusPill(title: String, value: String, systemImage: String, tint: Color) -> some View {
        HStack(spacing: 7) {
            Image(systemName: systemImage)
                .foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 1) {
                Text(title.uppercased())
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                Text(value)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(tint.opacity(0.08), in: Capsule())
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
