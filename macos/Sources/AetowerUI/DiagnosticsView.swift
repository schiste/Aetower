import SwiftUI
import AetowerBridge

private enum DiagnosticsLevelFilter: String, CaseIterable, Identifiable {
    case all
    case warn
    case error

    var id: String { rawValue }
}

private struct DiagnosticsEventCluster: Identifiable {
    let representative: DiagnosticsEvent
    let events: [DiagnosticsEvent]

    var id: String {
        "\(representative.id)-\(events.count)"
    }

    var count: Int {
        events.count
    }

    var newestTimestampMillis: UInt64 {
        events.first?.timestampMillis ?? representative.timestampMillis
    }

    var oldestTimestampMillis: UInt64 {
        events.last?.timestampMillis ?? representative.timestampMillis
    }

    var latestFields: [DiagnosticsField] {
        events.first?.fields ?? representative.fields
    }

    func canMerge(_ other: DiagnosticsEvent) -> Bool {
        representative.level == other.level
            && representative.subsystem == other.subsystem
            && representative.eventType == other.eventType
            && representative.message == other.message
            && representative.entityId == other.entityId
            && representative.adapter == other.adapter
            && representative.capability == other.capability
    }
}

public struct DiagnosticsView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var searchText = ""
    @State private var debouncedSearchText = ""
    @State private var subsystemFilter: DiagnosticsSubsystem?
    @State private var levelFilter: DiagnosticsLevelFilter = .all
    @State private var isLive = true
    @State private var isVisible = false
    @State private var includePersisted = true
    @State private var showClearDiagnosticsConfirmation = false
    @FocusState private var searchFieldFocused: Bool

    private let overviewColumns = [GridItem(.adaptive(minimum: 160), spacing: 12)]

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Diagnostics")
                        .font(.system(size: 28, weight: .semibold, design: .rounded))
                    Text("Bounded runtime diagnostics from the engine, adapters, persistence, telemetry, and the bridge. Use this to understand what Aetower is doing, not just what it is observing.")
                        .foregroundStyle(.secondary)
                }

                controls
                sessionHealth
                overview
                eventStream
            }
            .padding(24)
        }
        .navigationTitle("Diagnostics")
        .onAppear {
            isVisible = true
            debouncedSearchText = searchText
            state.setDiagnosticsVisible(true)
        }
        .onDisappear {
            isVisible = false
            state.setDiagnosticsVisible(false)
            searchFieldFocused = false
        }
        .task(id: searchText) {
            let candidate = searchText
            try? await Task.sleep(nanoseconds: 300_000_000)
            guard !Task.isCancelled, candidate == searchText else { return }
            debouncedSearchText = candidate
        }
        .task(id: "\(isVisible)-\(isLive)-\(includePersisted)-\(debouncedSearchText)-\(levelFilter.rawValue)-\(subsystemFilter.map(subsystemLabel) ?? "all")") {
            guard isVisible else { return }
            state.loadDiagnosticsQuery(currentQuery, force: true)
            guard isLive else { return }
            while !Task.isCancelled && isVisible && isLive {
                try? await Task.sleep(nanoseconds: 2_000_000_000)
                guard !Task.isCancelled && isVisible && isLive else { break }
                state.loadDiagnosticsQuery(currentQuery)
            }
        }
        .alert("Clear diagnostics?", isPresented: $showClearDiagnosticsConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Clear", role: .destructive) {
                state.clearDiagnostics()
            }
        } message: {
            Text("This removes in-memory and persisted diagnostics events from local storage.")
        }
    }

    private var controls: some View {
        GroupBox("Controls") {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 12) {
                    Picker("Severity", selection: $levelFilter) {
                        ForEach(DiagnosticsLevelFilter.allCases) { filter in
                            Text(filter.rawValue.capitalized).tag(filter)
                        }
                    }
                    .pickerStyle(.segmented)
                    .frame(maxWidth: 260)

                    Menu {
                        Button("All subsystems") {
                            subsystemFilter = nil
                        }
                        ForEach(allSubsystems, id: \.self) { subsystem in
                            Button(subsystemLabel(subsystem)) {
                                subsystemFilter = subsystem
                            }
                        }
                    } label: {
                        HStack(spacing: 6) {
                            Text(subsystemFilter.map(subsystemLabel) ?? "All subsystems")
                            Image(systemName: "chevron.down")
                                .font(.caption2)
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Color.secondary.opacity(0.08), in: Capsule())
                    }
                    .buttonStyle(.plain)

                    Toggle("Live", isOn: $isLive)
                        .toggleStyle(.switch)

                    Toggle("Persisted", isOn: $includePersisted)
                        .toggleStyle(.switch)

                    Spacer()

                    Button("Reload") {
                        state.loadDiagnosticsQuery(currentQuery, force: true)
                    }
                    .buttonStyle(.bordered)

                    Button("Export JSON") {
                        state.exportDiagnostics()
                    }
                    .buttonStyle(.borderedProminent)

                    Button("Export support bundle") {
                        state.exportSupportBundle(settings)
                    }
                    .buttonStyle(.bordered)

                    Button("Clear diagnostics", role: .destructive) {
                        showClearDiagnosticsConfirmation = true
                    }
                    .buttonStyle(.bordered)
                }

                TextField("Search messages, fields, entity ids, adapters, or capabilities", text: $searchText)
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .focused($searchFieldFocused)
                    .onSubmit { searchFieldFocused = false }

                if let diagnosticsLoadError = state.diagnosticsLoadError {
                    Text(diagnosticsLoadError)
                        .font(.caption)
                        .foregroundStyle(.orange)
                }

                Text(diagnosticsLoadStatus)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(.top, 4)
        }
    }

    private var overview: some View {
        GroupBox("Overview") {
            LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: 12) {
                diagnosticsMetric(
                    title: "Buffered events",
                    value: "\(state.diagnosticsOverview.currentSize)",
                    subtitle: "capacity \(state.diagnosticsOverview.ringCapacity)"
                )
                diagnosticsMetric(
                    title: "Dropped",
                    value: "\(state.diagnosticsOverview.droppedEvents)",
                    subtitle: "ring overflow since launch"
                )
                diagnosticsMetric(
                    title: "Warnings",
                    value: "\(state.diagnosticsOverview.warnCount)",
                    subtitle: "current buffer"
                )
                diagnosticsMetric(
                    title: "Errors",
                    value: "\(state.diagnosticsOverview.errorCount)",
                    subtitle: diagnosticsErrorSubtitle
                )
                diagnosticsMetric(
                    title: "Persisted events",
                    value: "\(state.diagnosticsOverview.persistedEvents)",
                    subtitle: state.diagnosticsOverview.persistedPath ?? "persistence disabled"
                )
                diagnosticsMetric(
                    title: "Persisted size",
                    value: byteCount(state.diagnosticsOverview.persistedBytes),
                    subtitle: "diagnostics store"
                )
                diagnosticsMetric(
                    title: "Engine tick",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.engineTickMillis),
                    subtitle: "current self-observed pipeline latency"
                )
                diagnosticsMetric(
                    title: "Target cadence",
                    value: String(format: "%.1f s", state.runtimeLagMetrics.targetTickMillis / 1000),
                    subtitle: "active adaptive target tick"
                )
                diagnosticsMetric(
                    title: "UI render",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.snapshotToRenderMillis),
                    subtitle: "snapshot to visible render"
                )
                diagnosticsMetric(
                    title: "Display / Input",
                    value: String(
                        format: "%.0f Hz / %.1f ms",
                        state.runtimeLagMetrics.displayRefreshHz,
                        state.runtimeLagMetrics.inputAvgLatencyMillis
                    ),
                    subtitle: "refresh and avg input latency"
                )
            }
            .padding(.top, 4)
            VStack(alignment: .leading, spacing: 6) {
                if let persistedPath = state.diagnosticsOverview.persistedPath {
                    labeledPersistenceDetail("Diagnostics file", persistedPath)
                }
                if let historySummary = state.historyStoreSummary ?? state.historyRangeSummary {
                    labeledPersistenceDetail(
                        "History store",
                        "\(byteCount(historySummary.storeBytes)) db · \(byteCount(historySummary.walBytes)) wal · \(historySummary.snapshotCount) persisted"
                    )
                    labeledPersistenceDetail(
                        "Last history load",
                        "\(Int(state.historyLastLoadDurationMillis.rounded())) ms · \(state.historySnapshots.count)/\(historySummary.rangeCount) loaded"
                    )
                }
                if let maintenance = state.historyMaintenanceReport,
                   maintenance.prunedRows > 0 || maintenance.vacuumed || maintenance.checkpointed {
                    labeledPersistenceDetail(
                        "Last history maintenance",
                        [
                            maintenance.prunedRows > 0 ? "trimmed \(maintenance.prunedRows) rows" : nil,
                            maintenance.vacuumed ? "vacuumed" : nil,
                            maintenance.checkpointed ? "checkpointed" : nil,
                            maintenance.aggressiveReason
                        ]
                        .compactMap { $0 }
                        .joined(separator: " · ")
                    )
                }
                if let lastHistoryLoadFailure {
                    labeledPersistenceDetail("Last history issue", lastHistoryLoadFailure)
                }
                labeledPersistenceDetail(
                    "Signal policy",
                    "Current errors stay prominent, stale retained errors are downgraded, and repeated persistence churn is summarized instead of retained row-by-row."
                )
            }
            .padding(.top, 8)
            if let persistenceError = state.diagnosticsOverview.persistenceError {
                Text(persistenceError)
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .padding(.top, 8)
            }
        }
    }

    private var sessionHealth: some View {
        GroupBox("Session health") {
            LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                diagnosticsMetric(
                    title: "Diagnostics",
                    value: diagnosticsHealthTitle,
                    subtitle: diagnosticsHealthSubtitle,
                    valueColor: healthColor(diagnosticsHealthTitle)
                )
                diagnosticsMetric(
                    title: "Unified logs",
                    value: unifiedLogHealthTitle,
                    subtitle: unifiedLogHealthSubtitle,
                    valueColor: healthColor(unifiedLogHealthTitle)
                )
                diagnosticsMetric(
                    title: "Permissions",
                    value: permissionHealthTitle,
                    subtitle: permissionHealthSubtitle,
                    valueColor: healthColor(permissionHealthTitle)
                )
                diagnosticsMetric(
                    title: "Telemetry",
                    value: telemetryHealthTitle,
                    subtitle: telemetryHealthSubtitle,
                    valueColor: healthColor(telemetryHealthTitle)
                )
                diagnosticsMetric(
                    title: "Aetower overhead",
                    value: aetowerOverheadTitle,
                    subtitle: aetowerOverheadSubtitle,
                    valueColor: healthColor(aetowerOverheadTitle)
                )
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private var eventStream: some View {
        let clusters = eventClusters
        return GroupBox("Event stream") {
            if clusters.isEmpty {
                ContentUnavailableView(
                    "No diagnostics match this filter",
                    systemImage: "waveform.path.ecg.rectangle",
                    description: Text("Try a broader filter or wait for more runtime activity.")
                )
                .frame(maxWidth: .infinity)
            } else {
                LazyVStack(alignment: .leading, spacing: 10) {
                    Text(eventStreamSummaryText(clusterCount: clusters.count))
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    ForEach(clusters) { cluster in
                        let event = cluster.representative
                        VStack(alignment: .leading, spacing: 6) {
                            HStack(alignment: .top, spacing: 8) {
                                Circle()
                                    .fill(levelColor(event.level))
                                    .frame(width: 8, height: 8)
                                    .padding(.top, 6)
                                VStack(alignment: .leading, spacing: 4) {
                                    HStack(spacing: 8) {
                                        Text(event.message)
                                            .font(.headline)
                                        Text(subsystemLabel(event.subsystem))
                                            .font(.caption.monospaced())
                                            .padding(.horizontal, 8)
                                            .padding(.vertical, 4)
                                            .background(Color.secondary.opacity(0.08), in: Capsule())
                                        if cluster.count > 1 {
                                            Text("\(cluster.count)x")
                                                .font(.caption.monospaced())
                                                .padding(.horizontal, 8)
                                                .padding(.vertical, 4)
                                                .background(Color.secondary.opacity(0.06), in: Capsule())
                                        }
                                        if let sequence = event.sequence {
                                            Text("#\(sequence)")
                                                .font(.caption.monospacedDigit())
                                                .foregroundStyle(.secondary)
                                        }
                                    }
                                    Text(eventMetadataLine(cluster))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    if let entityId = event.entityId {
                                        Text("entity \(entityId)")
                                            .font(.caption2.monospaced())
                                            .foregroundStyle(.tertiary)
                                    }
                                    if !cluster.latestFields.isEmpty {
                                        DisclosureGroup(cluster.count > 1 ? "Latest fields" : "Fields") {
                                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: 8)], alignment: .leading, spacing: 8) {
                                                ForEach(Array(cluster.latestFields.enumerated()), id: \.offset) { _, field in
                                                    VStack(alignment: .leading, spacing: 2) {
                                                        Text(field.key)
                                                            .font(.caption2.weight(.medium))
                                                            .foregroundStyle(.tertiary)
                                                        Text(field.value)
                                                            .font(.caption.monospaced())
                                                            .foregroundStyle(.secondary)
                                                            .textSelection(.enabled)
                                                    }
                                                    .padding(8)
                                                    .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                                                }
                                            }
                                            .padding(.top, 4)
                                        }
                                    }
                                }
                                Spacer()
                            }
                        }
                        .padding(.vertical, 4)
                        if cluster.id != clusters.last?.id {
                            Divider()
                        }
                    }
                }
                .padding(.top, 4)
            }
        }
    }

    private var eventClusters: [DiagnosticsEventCluster] {
        var clusters: [DiagnosticsEventCluster] = []
        for event in state.diagnosticsEvents {
            if let last = clusters.last, last.canMerge(event) {
                let mergedEvents = last.events + [event]
                clusters[clusters.count - 1] = DiagnosticsEventCluster(
                    representative: last.representative,
                    events: mergedEvents
                )
            } else {
                clusters.append(DiagnosticsEventCluster(representative: event, events: [event]))
            }
        }
        return clusters
    }

    private var currentQuery: DiagnosticsQuery {
        DiagnosticsQuery(
            limit: 500,
            minimumLevel: minimumLevel,
            subsystem: subsystemFilter,
            search: debouncedSearchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : debouncedSearchText,
            sinceMillis: nil,
            includePersisted: includePersisted
        )
    }

    private var minimumLevel: DiagnosticsLevel? {
        switch levelFilter {
        case .all:
            return nil
        case .warn:
            return .warn
        case .error:
            return .error
        }
    }

    private var allSubsystems: [DiagnosticsSubsystem] {
        var seen = Set(state.diagnosticsEvents.map(\.subsystem))
        if seen.isEmpty {
            seen = [
                .engine, .persistence, .telemetry, .adapterChromium, .adapterDocker,
                .adapterHelper, .adapterChau7, .gpu, .ffi, .ui
            ]
        }
        return Array(seen).sorted { subsystemLabel($0) < subsystemLabel($1) }
    }

    private var lastHistoryLoadFailure: String? {
        state.diagnosticsEvents.first(where: {
            $0.eventType == "history-load-failed"
                || $0.eventType == "history-loaded-with-quarantine"
        }).map { event in
            if let errorField = event.fields.first(where: { $0.key == "error" })?.value,
               !errorField.isEmpty {
                return errorField
            }
            if let quarantinedRows = event.fields.first(where: { $0.key == "quarantined_rows" })?.value,
               quarantinedRows != "0" {
                return "quarantined \(quarantinedRows) incompatible row(s)"
            }
            return event.message
        }
    }

    private var diagnosticsHealthTitle: String {
        if recentDiagnosticsErrorMessage != nil {
            return "Degraded"
        }
        if state.diagnosticsOverview.errorCount > 0 {
            return "Stale"
        }
        if state.diagnosticsRecentWarningCount >= 25 {
            return "Watch"
        }
        if state.diagnosticsOverview.warnCount > 0 {
            return "Quiet"
        }
        return "Clean"
    }

    private var diagnosticsHealthSubtitle: String {
        if let historyIssue = lastHistoryLoadFailure {
            return historyIssue
        }
        if let lastErrorMessage = recentDiagnosticsErrorMessage {
            return lastErrorMessage
        }
        if state.diagnosticsOverview.errorCount > 0 {
            return "No active diagnostics error, but stale retained errors still exist."
        }
        if state.diagnosticsRecentWarningCount >= 25 {
            return "\(state.diagnosticsRecentWarningCount) warning-level events in the recent diagnostics window"
        }
        if let lastErrorMillis = state.diagnosticsOverview.lastErrorMillis {
            return "Most recent diagnostics error was \(relativeTimeLabel(from: lastErrorMillis))"
        }
        return "No recent diagnostics errors · \(diagnosticsFreshnessLabel)"
    }

    private var unifiedLogHealthTitle: String {
        if state.sessionLogAnalysisError != nil {
            return "Degraded"
        }
        guard let summary = state.sessionLogSummary else {
            return "Pending"
        }
        if summary.metalLoadFailures > 0 {
            return "Investigate"
        }
        if summary.cursorUiEntries >= 120 || summary.viewBridgeCancellationCount > 0 {
            return "Noisy"
        }
        return "Quiet"
    }

    private var unifiedLogHealthSubtitle: String {
        if let error = state.sessionLogAnalysisError {
            let freshness = state.lastSessionLogAnalysisCompletedDate.map {
                "Last successful analysis \(relativeTimeLabel(from: $0))"
            } ?? "No successful analysis yet"
            return "\(error) · \(freshness)"
        }
        guard let summary = state.sessionLogSummary else {
            return "Open Diagnostics to analyze the current session"
        }
        return "\(summary.cursorUiEntries) cursor updates, \(summary.metalLoadFailures) Metal errors, \(summary.viewBridgeCancellationCount) view bridge cancellations · \(sessionLogFreshnessLabel)"
    }

    private var permissionHealthTitle: String {
        if state.sessionLogAnalysisError != nil {
            return "Unknown"
        }
        guard let summary = state.sessionLogSummary else {
            return "Pending"
        }
        if summary.notificationAuthorizationFailures > 0 {
            return "Denied"
        }
        if summary.tccAccessRequests > 4 {
            return "Chatty"
        }
        return "Stable"
    }

    private var permissionHealthSubtitle: String {
        if state.sessionLogAnalysisError != nil {
            return sessionLogFreshnessLabel
        }
        guard let summary = state.sessionLogSummary else {
            return "Session log analysis not loaded yet"
        }
        return "\(summary.tccAccessRequests) TCC checks, \(summary.notificationAuthorizationFailures) notification failures · \(sessionLogFreshnessLabel)"
    }

    private var telemetryHealthTitle: String {
        if let status = state.telemetryVerificationStatus {
            return status.contains("failed") ? "Failed" : "Verified"
        }
        return state.telemetryEnabled ? "Enabled" : "Disabled"
    }

    private var telemetryHealthSubtitle: String {
        if let status = state.telemetryVerificationStatus {
            return status
        }
        return state.telemetryEnabled ? state.telemetryEndpoint : "Run verification from Settings"
    }

    private var aetowerOverheadTitle: String {
        if state.runtimeLagMetrics.engineTickMillis >= 100 || state.runtimeLagMetrics.snapshotToRenderMillis >= 120 {
            return "Hot"
        }
        if state.runtimeLagMetrics.engineTickMillis >= 40 || state.runtimeLagMetrics.snapshotToRenderMillis >= 60 {
            return "Watch"
        }
        return "Bounded"
    }

    private var aetowerOverheadSubtitle: String {
        String(
            format: "tick %.1f ms · collect %.1f ms · render %.1f ms",
            state.runtimeLagMetrics.engineTickMillis,
            state.runtimeLagMetrics.collectMillis,
            state.runtimeLagMetrics.snapshotToRenderMillis
        )
    }

    private var diagnosticsErrorSubtitle: String {
        if let message = recentDiagnosticsErrorMessage {
            return message
        }
        if state.diagnosticsOverview.errorCount > 0 {
            return "stale retained errors exist, but none are active"
        }
        if let lastErrorMillis = state.diagnosticsOverview.lastErrorMillis {
            return "last error \(relativeTimeLabel(from: lastErrorMillis))"
        }
        return "no recent errors · \(diagnosticsFreshnessLabel)"
    }

    private var recentDiagnosticsErrorMessage: String? {
        guard let lastErrorMessage = state.diagnosticsOverview.lastErrorMessage,
              let lastErrorMillis = state.diagnosticsOverview.lastErrorMillis
        else {
            return nil
        }
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        let ageMillis = nowMillis >= lastErrorMillis ? nowMillis - lastErrorMillis : 0
        if ageMillis <= 10 * 60 * 1000 {
            return lastErrorMessage
        }
        return nil
    }

    private var diagnosticsLoadStatus: String {
        if searchText != debouncedSearchText {
            return "Updating search…"
        }
        return "\(diagnosticsFreshnessLabel) · \(sessionLogFreshnessLabel)"
    }

    private var diagnosticsFreshnessLabel: String {
        if let lastDiagnosticsQueryDate = state.lastDiagnosticsQueryDate {
            return "Diagnostics updated \(relativeTimeLabel(from: lastDiagnosticsQueryDate))"
        }
        return "Diagnostics not loaded yet"
    }

    private var sessionLogFreshnessLabel: String {
        if let lastSessionLogAnalysisCompletedDate = state.lastSessionLogAnalysisCompletedDate {
            return "Unified logs analyzed \(relativeTimeLabel(from: lastSessionLogAnalysisCompletedDate)) over last 6m"
        }
        return "Unified logs not analyzed yet"
    }

    private func eventStreamSummaryText(clusterCount: Int) -> String {
        let eventCountLabel = clusterCount == state.diagnosticsEvents.count
            ? "\(state.diagnosticsEvents.count) loaded event(s)"
            : "\(state.diagnosticsEvents.count) loaded event(s) condensed into \(clusterCount) groups"
        return "\(eventCountLabel) · \(diagnosticsFreshnessLabel)"
    }

    private func eventMetadataLine(_ cluster: DiagnosticsEventCluster) -> String {
        if cluster.count == 1 {
            return "\(cluster.representative.eventType) · \(formattedTimestamp(cluster.newestTimestampMillis))"
        }
        return "\(cluster.representative.eventType) · \(formattedTimestamp(cluster.newestTimestampMillis)) · repeated \(cluster.count)x over \(clusterTimespanLabel(cluster))"
    }

    private func clusterTimespanLabel(_ cluster: DiagnosticsEventCluster) -> String {
        guard cluster.newestTimestampMillis > cluster.oldestTimestampMillis else {
            return "one sample"
        }
        let deltaSeconds = (cluster.newestTimestampMillis - cluster.oldestTimestampMillis) / 1000
        if deltaSeconds < 60 {
            return "\(deltaSeconds)s"
        }
        if deltaSeconds < 3600 {
            return "\(deltaSeconds / 60)m"
        }
        return "\(deltaSeconds / 3600)h"
    }
}

private func diagnosticsMetric(
    title: String,
    value: String,
    subtitle: String,
    valueColor: Color? = nil
) -> some View {
    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
        Text(title)
            .font(.caption2.weight(.medium))
            .foregroundStyle(.secondary)
        Text(value)
            .font(.system(size: 18, weight: .semibold, design: .rounded))
            .foregroundStyle(valueColor ?? .primary)
        Text(subtitle)
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .lineLimit(2)
    }
    .padding(AetowerDesign.Spacing.sm)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous))
}

private func labeledPersistenceDetail(_ title: String, _ value: String) -> some View {
    VStack(alignment: .leading, spacing: 2) {
        Text(title)
            .font(.caption2.weight(.medium))
            .foregroundStyle(.secondary)
        Text(value)
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
            .textSelection(.enabled)
    }
}

private func relativeTimeLabel(from timestampMillis: UInt64) -> String {
    let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
    guard nowMillis >= timestampMillis else {
        return "just now"
    }
    let deltaSeconds = (nowMillis - timestampMillis) / 1000
    if deltaSeconds < 60 {
        return "\(deltaSeconds)s ago"
    }
    if deltaSeconds < 3600 {
        return "\(deltaSeconds / 60)m ago"
    }
    if deltaSeconds < 86_400 {
        return "\(deltaSeconds / 3600)h ago"
    }
    return "\(deltaSeconds / 86_400)d ago"
}

private func relativeTimeLabel(from date: Date) -> String {
    relativeTimeLabel(from: UInt64(date.timeIntervalSince1970 * 1000))
}

private func subsystemLabel(_ subsystem: DiagnosticsSubsystem) -> String {
    switch subsystem {
    case .engine: return "engine"
    case .collector: return "collector"
    case .identity: return "identity"
    case .attribution: return "attribution"
    case .friction: return "friction"
    case .history: return "history"
    case .persistence: return "persistence"
    case .telemetry: return "telemetry"
    case .gpu: return "gpu"
    case .ffi: return "ffi"
    case .ui: return "ui"
    case .adapterChromium: return "adapter.chromium"
    case .adapterDocker: return "adapter.docker"
    case .adapterHelper: return "adapter.helper"
    case .adapterChau7: return "adapter.chau7"
    case .adapterVsCode: return "adapter.vscode"
    }
}

/// Map health status strings to semantic colors so "Degraded" stands out
/// visually from "Clean". Previously all health values rendered as plain
/// text with no color signal.
private func healthColor(_ title: String) -> Color? {
    switch title {
    case "Clean", "Quiet", "Stable", "Bounded", "Verified", "Enabled":
        return AetowerDesign.Status.success
    case "Watch", "Noisy", "Chatty", "Stale", "Pending", "Unknown":
        return AetowerDesign.Status.warning
    case "Degraded", "Denied", "Failed", "Hot", "Investigate":
        return AetowerDesign.Status.error
    default:
        return nil
    }
}

private func levelColor(_ level: DiagnosticsLevel) -> Color {
    switch level {
    case .trace, .debug:
        return .secondary
    case .info:
        return .blue
    case .warn:
        return .orange
    case .error:
        return .red
    }
}

private func formattedTimestamp(_ millis: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(millis) / 1000)
        .formatted(date: .omitted, time: .standard)
}

private func byteCount(_ bytes: UInt64) -> String {
    ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
}
