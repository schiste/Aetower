import SwiftUI
import AetowerBridge

private enum DiagnosticsLevelFilter: String, CaseIterable, Identifiable {
    case all
    case warn
    case error

    var id: String { rawValue }
}

public struct DiagnosticsView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var searchText = ""
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
            state.setDiagnosticsVisible(true)
            state.loadDiagnosticsQuery(currentQuery, force: true)
        }
        .onDisappear {
            isVisible = false
            state.setDiagnosticsVisible(false)
            searchFieldFocused = false
        }
        .task(id: "\(isVisible)-\(isLive)-\(includePersisted)-\(searchText)-\(levelFilter.rawValue)-\(subsystemFilter.map(subsystemLabel) ?? "all")") {
            guard isVisible && isLive else { return }
            state.loadDiagnosticsQuery(currentQuery, force: true)
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
                    subtitle: state.diagnosticsOverview.lastErrorMessage ?? "no recent errors"
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
                if let lastHistoryLoadFailure {
                    labeledPersistenceDetail("Last history issue", lastHistoryLoadFailure)
                }
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
            LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: 12) {
                diagnosticsMetric(
                    title: "Diagnostics",
                    value: diagnosticsHealthTitle,
                    subtitle: diagnosticsHealthSubtitle
                )
                diagnosticsMetric(
                    title: "Unified logs",
                    value: unifiedLogHealthTitle,
                    subtitle: unifiedLogHealthSubtitle
                )
                diagnosticsMetric(
                    title: "Permissions",
                    value: permissionHealthTitle,
                    subtitle: permissionHealthSubtitle
                )
                diagnosticsMetric(
                    title: "Telemetry",
                    value: telemetryHealthTitle,
                    subtitle: telemetryHealthSubtitle
                )
                diagnosticsMetric(
                    title: "Aetower overhead",
                    value: aetowerOverheadTitle,
                    subtitle: aetowerOverheadSubtitle
                )
            }
            .padding(.top, 4)
        }
    }

    private var eventStream: some View {
        GroupBox("Event stream") {
            if filteredEvents.isEmpty {
                ContentUnavailableView(
                    "No diagnostics match this filter",
                    systemImage: "waveform.path.ecg.rectangle",
                    description: Text("Try a broader filter or wait for more runtime activity.")
                )
                .frame(maxWidth: .infinity)
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(Array(filteredEvents.enumerated()), id: \.element.id) { _, event in
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
                                        if let sequence = event.sequence {
                                            Text("#\(sequence)")
                                                .font(.caption.monospacedDigit())
                                                .foregroundStyle(.secondary)
                                        }
                                    }
                                    Text("\(event.eventType) · \(formattedTimestamp(event.timestampMillis))")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    if let entityId = event.entityId {
                                        Text("entity \(entityId)")
                                            .font(.caption2.monospaced())
                                            .foregroundStyle(.tertiary)
                                    }
                                    if !event.fields.isEmpty {
                                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: 8)], alignment: .leading, spacing: 8) {
                                            ForEach(Array(event.fields.enumerated()), id: \.offset) { _, field in
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
                                    }
                                }
                                Spacer()
                            }
                        }
                        .padding(.vertical, 4)
                        if event.id != filteredEvents.last?.id {
                            Divider()
                        }
                    }
                }
                .padding(.top, 4)
            }
        }
    }

    private var filteredEvents: [DiagnosticsEvent] {
        state.diagnosticsEvents
    }

    private var currentQuery: DiagnosticsQuery {
        DiagnosticsQuery(
            limit: 500,
            minimumLevel: minimumLevel,
            subsystem: subsystemFilter,
            search: searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : searchText,
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
                || $0.eventType == "history-row-quarantined"
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
        if state.diagnosticsOverview.errorCount > 0 {
            return "Degraded"
        }
        if state.diagnosticsOverview.warnCount > 0 {
            return "Watch"
        }
        return "Clean"
    }

    private var diagnosticsHealthSubtitle: String {
        if let historyIssue = lastHistoryLoadFailure {
            return historyIssue
        }
        if let lastErrorMessage = state.diagnosticsOverview.lastErrorMessage {
            return lastErrorMessage
        }
        return "No recent diagnostics errors"
    }

    private var unifiedLogHealthTitle: String {
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
        guard let summary = state.sessionLogSummary else {
            return "Open Diagnostics to analyze the current session"
        }
        return "\(summary.cursorUiEntries) cursor updates, \(summary.metalLoadFailures) Metal errors, \(summary.viewBridgeCancellationCount) view bridge cancellations"
    }

    private var permissionHealthTitle: String {
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
        guard let summary = state.sessionLogSummary else {
            return "Session log analysis not loaded yet"
        }
        return "\(summary.tccAccessRequests) TCC checks, \(summary.notificationAuthorizationFailures) notification failures"
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
}

private func diagnosticsMetric(title: String, value: String, subtitle: String) -> some View {
    VStack(alignment: .leading, spacing: 4) {
        Text(title)
            .font(.caption2.weight(.medium))
            .foregroundStyle(.secondary)
        Text(value)
            .font(.system(size: 18, weight: .semibold, design: .rounded))
        Text(subtitle)
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .lineLimit(2)
    }
    .padding(10)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
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
