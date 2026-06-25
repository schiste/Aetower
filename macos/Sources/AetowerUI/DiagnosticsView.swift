import SwiftUI
import AetowerBridge

private enum DiagnosticsLevelFilter: String, CaseIterable, Identifiable {
    case all
    case warn
    case error

    var id: String { rawValue }
}

private enum DiagnosticsSection: String, CaseIterable, Identifiable {
    case health
    case crashReboot
    case mcp
    case uiPayloads
    case memory
    case eventStream

    var id: String { rawValue }

    var title: String {
        switch self {
        case .health: return "Health"
        case .crashReboot: return "Crash / Reboot"
        case .mcp: return "MCP"
        case .uiPayloads: return "UI Payloads"
        case .memory: return "Memory"
        case .eventStream: return "Event Stream"
        }
    }

    var systemImage: String {
        switch self {
        case .health: return "heart.text.square"
        case .crashReboot: return "bolt.trianglebadge.exclamationmark"
        case .mcp: return "point.3.connected.trianglepath.dotted"
        case .uiPayloads: return "rectangle.stack"
        case .memory: return "memorychip"
        case .eventStream: return "list.bullet.rectangle"
        }
    }

    var detail: String {
        switch self {
        case .health: return "Runtime health and operator recommendations."
        case .crashReboot: return "Boot session, panic/reset evidence, and pre-reboot pressure clues."
        case .mcp: return "Local server, helpers, clients, and transport pressure."
        case .uiPayloads: return "History and Timeline decode/filter cost."
        case .memory: return "Aetower self-memory and vmmap attribution."
        case .eventStream: return "Filtered diagnostics events and raw fields."
        }
    }
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
    @State private var selectedSection: DiagnosticsSection = .health
    @State private var showClearDiagnosticsConfirmation = false
    @FocusState private var searchFieldFocused: Bool

    private let overviewColumns = [GridItem(.adaptive(minimum: 160), spacing: 12)]

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    public var body: some View {
        let eventClusters = eventClusters

        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    Text("Diagnostics")
                        .font(.system(size: 28, weight: .semibold, design: .rounded))
                    Text("Bounded runtime diagnostics from the engine, adapters, persistence, telemetry, and the bridge. Use this to understand what Aetower is doing, not just what it is observing.")
                        .foregroundStyle(.secondary)
                }

                diagnosticsActions
                sectionSwitcher(eventClusters: eventClusters)
                selectedSectionContent(eventClusters: eventClusters)
            }
            .padding(AetowerDesign.Spacing.xxl)
        }
        .navigationTitle("Diagnostics")
        .onAppear {
            isVisible = true
            debouncedSearchText = searchText
            state.setDiagnosticsVisible(true)
            if state.selfMemoryAttribution == nil {
                state.loadSelfMemoryAttribution()
            }
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
        .task(id: "\(isVisible)-\(selectedSection.rawValue)-\(isLive)-\(includePersisted)-\(debouncedSearchText)-\(levelFilter.rawValue)-\(subsystemFilter.map(subsystemLabel) ?? "all")") {
            guard isVisible else { return }
            guard selectedSection == .eventStream || selectedSection == .crashReboot else { return }
            state.loadDiagnosticsQuery(activeDiagnosticsQuery, force: true)
            guard isLive else { return }
            while !Task.isCancelled && isVisible && isLive
                && (selectedSection == .eventStream || selectedSection == .crashReboot) {
                try? await Task.sleep(nanoseconds: 2_000_000_000)
                guard !Task.isCancelled && isVisible && isLive
                    && (selectedSection == .eventStream || selectedSection == .crashReboot) else { break }
                state.loadDiagnosticsQuery(activeDiagnosticsQuery)
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

    private var diagnosticsActions: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Button("Reload events") {
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

            Spacer()

            Text(diagnosticsLoadStatus)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func sectionSwitcher(eventClusters: [DiagnosticsEventCluster]) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 170), spacing: AetowerDesign.Spacing.sm)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.sm
        ) {
            ForEach(DiagnosticsSection.allCases) { section in
                diagnosticsSectionButton(section, eventClusters: eventClusters)
            }
        }
    }

    private func diagnosticsSectionButton(
        _ section: DiagnosticsSection,
        eventClusters: [DiagnosticsEventCluster]
    ) -> some View {
        let isSelected = selectedSection == section
        return Button {
            withAnimation(AetowerDesign.Motion.quick) {
                selectedSection = section
            }
        } label: {
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 7) {
                    Image(systemName: section.systemImage)
                        .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                    Text(section.title)
                        .font(.subheadline.weight(.semibold))
                    Spacer()
                }
                Text(sectionSignal(section, eventClusters: eventClusters))
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(sectionSignalColor(section))
                    .lineLimit(1)
                Text(section.detail)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .padding(AetowerDesign.Spacing.sm)
            .frame(maxWidth: .infinity, minHeight: 96, alignment: .topLeading)
            .background(
                isSelected ? Color.accentColor.opacity(0.11) : AetowerDesign.Surface.card,
                in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
                    .stroke(isSelected ? Color.accentColor.opacity(0.32) : Color.clear, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func selectedSectionContent(eventClusters: [DiagnosticsEventCluster]) -> some View {
        switch selectedSection {
        case .health:
            sessionHealth
            recommendations
            overview
        case .crashReboot:
            crashRebootInvestigation
        case .mcp:
            localMcpServerState
            mcpLifecycle
        case .uiPayloads:
            uiPayloadDiagnostics
        case .memory:
            selfMemoryAttribution
        case .eventStream:
            controls
            eventStream(eventClusters)
        }
    }

    private var controls: some View {
        GroupBox("Event stream filters") {
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
            .padding(.top, AetowerDesign.Spacing.xs)
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
                    title: "Collect",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.collectMillis),
                    subtitle: "process scan + host sample"
                )
                diagnosticsMetric(
                    title: "Identity",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.identityMillis),
                    subtitle: "process → entity resolution"
                )
                diagnosticsMetric(
                    title: "Attribution",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.attributionMillis),
                    subtitle: "entity grouping + metrics"
                )
                diagnosticsMetric(
                    title: "Friction",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.frictionMillis),
                    subtitle: "scoring + recommendations"
                )
                diagnosticsMetric(
                    title: "Enrich",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.enrichMillis),
                    subtitle: "adapter context merge"
                )
                diagnosticsMetric(
                    title: "History",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.historyMillis),
                    subtitle: "timeline + trend update"
                )
                diagnosticsMetric(
                    title: "Persist",
                    value: String(format: "%.1f ms", state.runtimeLagMetrics.persistMillis),
                    subtitle: "SQLite write queue"
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
            .padding(.top, AetowerDesign.Spacing.xs)
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
            .padding(.top, AetowerDesign.Spacing.sm)
            if let persistenceError = state.diagnosticsOverview.persistenceError {
                Text(persistenceError)
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .padding(.top, AetowerDesign.Spacing.sm)
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

    private var recommendations: some View {
        let items = buildOperatorRecommendations(
            snapshot: state.snapshot,
            diagnostics: state.diagnosticsOverview,
            history: state.historyStoreSummary ?? state.historyRangeSummary,
            runtime: state.runtimeLagMetrics
        )

        return GroupBox("Recommended next actions") {
            if items.isEmpty {
                Text("No immediate operator actions are currently recommended.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, AetowerDesign.Spacing.xs)
            } else {
                LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    ForEach(items) { recommendation in
                        diagnosticsRecommendationCard(recommendation)
                    }
                }
                .padding(.top, AetowerDesign.Spacing.xs)
            }
        }
    }

    private var mcpLifecycle: some View {
        let runtime = state.runtimeLagMetrics
        return GroupBox("MCP helper lifecycle") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    diagnosticsMetric(
                        title: "Active clients",
                        value: "\(runtime.mcpActiveClientCount)",
                        subtitle: "\(runtime.mcpHelperCount) helper processes · \(runtime.staleMcpHelperCount) stale",
                        valueColor: mcpLifecycleColor
                    )
                    diagnosticsMetric(
                        title: "Oldest helper",
                        value: durationLabel(runtime.oldestMcpHelperAgeMillis),
                        subtitle: "stale threshold \(durationLabel(15 * 60 * 1000))"
                    )
                    diagnosticsMetric(
                        title: "Request rate",
                        value: String(format: "%.1f/s", runtime.mcpRequestsPerSecond),
                        subtitle: "\(runtime.mcpTotalRequests) total requests · \(runtime.mcpTotalConnections) connections"
                    )
                    diagnosticsMetric(
                        title: "Owner attribution",
                        value: "Unavailable",
                        subtitle: "MCP protocol exposes counts, not stable client identities yet"
                    )
                    diagnosticsMetric(
                        title: "Aetower self",
                        value: String(format: "%.1f%% CPU", runtime.selfCpuPercent),
                        subtitle: "\(byteCount(runtime.selfMemoryBytes)) resident · \(String(format: "%.0f/s", runtime.selfWakeupsPerSecond)) wakeups"
                    )
                    diagnosticsMetric(
                        title: "Transport strategy",
                        value: transportStrategyTitle,
                        subtitle: transportStrategySubtitle,
                        valueColor: transportStrategyColor
                    )
                    diagnosticsMetric(
                        title: "Last MCP observation",
                        value: runtime.mcpObservedAtMillis == 0 ? "Pending" : relativeTimeLabel(from: runtime.mcpObservedAtMillis),
                        subtitle: "updated by in-app MCP runtime stats"
                    )
                }

                Text(mcpLifecycleGuidance)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private var localMcpServerState: some View {
        let statuses = state.localMcpClientStatuses
        let installedAutomaticClients = statuses.filter { $0.isInstalled && $0.supportsAutomaticRegistration }
        let registeredAutomaticClients = installedAutomaticClients.filter { $0.state == .registered }
        let runtime = state.runtimeLagMetrics
        let listenerValue: String = {
            if state.localMcpServerHealthy {
                return "Live"
            }
            if let lastStartError = state.localMcpLastStartError, !lastStartError.isEmpty {
                return "Failed"
            }
            if state.localMcpLastStartAttemptDate != nil {
                return "Starting"
            }
            return "Down"
        }()

        let listenerSubtitle: String = {
            if let detail = state.localMcpLastProbeDetail, !detail.isEmpty {
                return detail
            }
            if let lastStartError = state.localMcpLastStartError, !lastStartError.isEmpty {
                return lastStartError
            }
            return "No listener probe has completed yet."
        }()

        return GroupBox("Local MCP server") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    diagnosticsMetric(
                        title: "Listener",
                        value: listenerValue,
                        subtitle: listenerSubtitle,
                        valueColor: healthColor(listenerValue)
                    )
                    diagnosticsMetric(
                        title: "Socket path",
                        value: "Configured",
                        subtitle: state.localMcpSocketPathDisplay
                    )
                    diagnosticsMetric(
                        title: "Registrations",
                        value: "\(registeredAutomaticClients.count)/\(installedAutomaticClients.count)",
                        subtitle: "\(statuses.count) known local clients inspected"
                    )
                    diagnosticsMetric(
                        title: "Clients now",
                        value: "\(runtime.mcpActiveClientCount)",
                        subtitle: "\(runtime.mcpHelperCount) helper processes · \(runtime.mcpTotalConnections) total connections"
                    )
                    diagnosticsMetric(
                        title: "Last start",
                        value: state.localMcpLastStartSucceededDate.map(relativeTimeLabel(from:)) ?? "Pending",
                        subtitle: state.localMcpLastStartAttemptDate.map(relativeTimeLabel(from:)) ?? "no start attempt yet"
                    )
                    diagnosticsMetric(
                        title: "Probe failures",
                        value: "\(state.localMcpConsecutiveProbeFailures)",
                        subtitle: "\(state.localMcpRestartCount) restart attempts since launch"
                    )
                    diagnosticsMetric(
                        title: "Last accepted client",
                        value: runtime.mcpObservedAtMillis == 0 ? "Unknown" : relativeTimeLabel(from: runtime.mcpObservedAtMillis),
                        subtitle: "Protocol currently exposes counts and timestamps, not stable client identities"
                    )
                }

                if let lastStartError = state.localMcpLastStartError, !lastStartError.isEmpty {
                    Text(lastStartError)
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .textSelection(.enabled)
                }

                if !statuses.isEmpty {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        Text("Client registration state")
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                        ForEach(statuses) { status in
                            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                                Text(status.displayName)
                                    .font(.caption.weight(.medium))
                                    .frame(width: 110, alignment: .leading)
                                Text(status.state.rawValue.replacingOccurrences(of: "([A-Z])", with: " $1", options: .regularExpression).capitalized)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(status.state == .registered ? .green : .secondary)
                                    .frame(width: 150, alignment: .leading)
                                Text(status.detail)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private var selfMemoryAttribution: some View {
        GroupBox("Self memory attribution") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack {
                    Text("Point-in-time vmmap attribution for the Aetower app process or its attributed self entity.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Capture vmmap attribution") {
                        state.loadSelfMemoryAttribution(force: true)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(state.selfMemoryAttributionIsLoading)
                }

                LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    diagnosticsMetric(
                        title: "Resident",
                        value: byteCount(state.selfMemoryAttribution?.currentResidentBytes ?? state.runtimeLagMetrics.selfMemoryBytes),
                        subtitle: "current Aetower resident memory"
                    )
                    diagnosticsMetric(
                        title: "Footprint",
                        value: byteCount(state.selfMemoryAttribution?.currentPhysicalFootprintBytes ?? state.runtimeLagMetrics.selfMemoryPhysicalFootprintBytes),
                        subtitle: "current physical footprint"
                    )
                    diagnosticsMetric(
                        title: "Peak footprint",
                        value: byteCount(state.selfMemoryAttribution?.peakPhysicalFootprintBytes ?? state.runtimeLagMetrics.selfMemoryPhysicalFootprintBytes),
                        subtitle: selfPeakSubtitle
                    )
                    diagnosticsMetric(
                        title: "Attributed PIDs",
                        value: "\(state.selfMemoryAttribution?.processIds.count ?? 0)",
                        subtitle: state.selfMemoryAttribution?.selfDisplayName ?? "capture vmmap attribution for grouped self context"
                    )
                }

                if state.selfMemoryAttributionIsLoading {
                    ProgressView("Capturing Aetower self vmmap breakdown…")
                } else if let error = state.selfMemoryAttributionError {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.orange)
                } else if let attribution = state.selfMemoryAttribution {
                    Text(attribution.note)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)

                    ForEach(attribution.regions.prefix(6)) { region in
                        HStack {
                            Text(region.regionType)
                                .font(.subheadline.weight(.semibold))
                            Spacer()
                            Text(byteCount(region.residentBytes))
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 4)
                    }

                    if !attribution.caveats.isEmpty {
                        Text(attribution.caveats.joined(separator: " "))
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                } else {
                    Text("Use the capture button to collect a current vmmap-style region breakdown for Aetower itself.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private var uiPayloadDiagnostics: some View {
        GroupBox("Heavy-tab payloads") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    diagnosticsMetric(
                        title: "History decode",
                        value: String(format: "%.1f ms", state.historyUiDiagnostics.pageDecodeDurationMillis),
                        subtitle: "\(state.historyUiDiagnostics.snapshotCount) snapshots · \(state.historyUiDiagnostics.entityCount) entities"
                    )
                    diagnosticsMetric(
                        title: "History derive",
                        value: String(format: "%.1f ms", state.historyUiDiagnostics.derivedSummaryBuildDurationMillis),
                        subtitle: "\(state.historyUiDiagnostics.recurringEntityCount) recurring entities · \(state.historyUiDiagnostics.changeSummaryCount) changes"
                    )
                    diagnosticsMetric(
                        title: "Timeline filter",
                        value: String(format: "%.1f ms", state.timelinePayloadDiagnostics.filterDurationMillis),
                        subtitle: "\(state.timelinePayloadDiagnostics.filteredEventCount) of \(state.timelinePayloadDiagnostics.totalEventCount) events"
                    )
                    diagnosticsMetric(
                        title: "Timeline visible",
                        value: "\(state.timelinePayloadDiagnostics.visibleEventCount)",
                        subtitle: state.timelinePayloadDiagnostics.safeModeEnabled
                            ? "operator-safe mode window"
                            : "standard visible window"
                    )
                }

                Text(uiPayloadGuidance)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private var crashRebootInvestigation: some View {
        let events = crashRebootEvents
        let diagnosticReportEvents = events.filter {
            $0.eventType == "system-reset-counter-report"
                || $0.eventType == "system-panic-manifest"
                || $0.eventType == "system-panic-file-observed"
        }
        let incidentEvents = events.filter { $0.eventType == "host-incident-snapshot" }
        let overBudgetEvents = events.filter {
            $0.eventType == "tick-over-budget"
                || $0.eventType == "history-maintenance-over-budget"
                || $0.eventType == "long-state-lock-wait"
        }
        let bootEvent = events.first { $0.eventType == "boot-session-observed" }
        let shutdownEvent = events.first { $0.eventType == "system-previous-shutdown-cause" }

        return GroupBox("Crash / reboot investigator") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                LazyVGrid(columns: overviewColumns, alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    diagnosticsMetric(
                        title: "Current boot",
                        value: bootEvent.flatMap { bootTimeLabel($0) } ?? "Unknown",
                        subtitle: bootEvent.map { eventMetadataLine(DiagnosticsEventCluster(representative: $0, events: [$0])) } ?? "No boot-session diagnostic loaded yet",
                        valueColor: bootEvent == nil ? AetowerDesign.Status.warning : AetowerDesign.Status.success
                    )
                    diagnosticsMetric(
                        title: "Shutdown marker",
                        value: shutdownEvent.flatMap { fieldValue("previous_shutdown_code", in: $0) ?? fieldValue("detail", in: $0) } ?? "Unavailable",
                        subtitle: shutdownEvent?.message ?? "No previous shutdown marker in loaded diagnostics",
                        valueColor: shutdownEvent == nil ? AetowerDesign.Status.warning : nil
                    )
                    diagnosticsMetric(
                        title: "Crash artifacts",
                        value: "\(diagnosticReportEvents.count)",
                        subtitle: "ResetCounter, panic manifest, or panic files observed",
                        valueColor: diagnosticReportEvents.isEmpty ? AetowerDesign.Status.warning : AetowerDesign.Status.error
                    )
                    diagnosticsMetric(
                        title: "Pressure clues",
                        value: "\(incidentEvents.count)",
                        subtitle: "memory, wakeup, or thermal incident breadcrumbs",
                        valueColor: incidentEvents.isEmpty ? AetowerDesign.Status.success : AetowerDesign.Status.warning
                    )
                    diagnosticsMetric(
                        title: "Runtime stalls",
                        value: "\(overBudgetEvents.count)",
                        subtitle: "engine tick, lock wait, or history maintenance over budget",
                        valueColor: overBudgetEvents.isEmpty ? AetowerDesign.Status.success : AetowerDesign.Status.warning
                    )
                }

                Text(crashRebootGuidance(events: events, diagnosticReportEvents: diagnosticReportEvents))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                if events.isEmpty {
                    ContentUnavailableView(
                        "No crash or reboot diagnostics loaded",
                        systemImage: "bolt.trianglebadge.exclamationmark",
                        description: Text("Aetower will load persisted diagnostics here. If the Mac rebooted before Aetower started, missing kernel panic files may still need Full Disk Access or a manual support bundle.")
                    )
                    .frame(maxWidth: .infinity)
                } else {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        Text("Evidence timeline")
                            .font(.subheadline.weight(.semibold))
                        ForEach(events.prefix(10), id: \.id) { event in
                            crashRebootEvidenceRow(event)
                            if event.id != events.prefix(10).last?.id {
                                Divider()
                            }
                        }
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private func eventStream(_ eventClusters: [DiagnosticsEventCluster]) -> some View {
        GroupBox("Event stream") {
            if eventClusters.isEmpty {
                ContentUnavailableView(
                    "No diagnostics match this filter",
                    systemImage: "waveform.path.ecg.rectangle",
                    description: Text("Try a broader filter or wait for more runtime activity.")
                )
                .frame(maxWidth: .infinity)
            } else {
                LazyVStack(alignment: .leading, spacing: 10) {
                    Text(eventStreamSummaryText(clusterCount: eventClusters.count))
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    ForEach(eventClusters) { cluster in
                        let event = cluster.representative
                        VStack(alignment: .leading, spacing: 6) {
                            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                                Circle()
                                    .fill(levelColor(event.level))
                                    .frame(width: 8, height: 8)
                                    .padding(.top, 6)
                                VStack(alignment: .leading, spacing: 4) {
                                    HStack(spacing: AetowerDesign.Spacing.sm) {
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
                                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: AetowerDesign.Spacing.sm)], alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
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
                                            .padding(.top, AetowerDesign.Spacing.xs)
                                        }
                                    }
                                }
                                Spacer()
                            }
                        }
                        .padding(.vertical, 4)
                        if cluster.id != eventClusters.last?.id {
                            Divider()
                        }
                    }
                }
                .padding(.top, AetowerDesign.Spacing.xs)
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

    private func sectionSignal(
        _ section: DiagnosticsSection,
        eventClusters: [DiagnosticsEventCluster]
    ) -> String {
        switch section {
        case .health:
            return "\(diagnosticsHealthTitle) · \(aetowerOverheadTitle)"
        case .crashReboot:
            let count = crashRebootEvents.count
            return count == 0 ? "no evidence loaded" : "\(count) evidence events"
        case .mcp:
            return "\(state.runtimeLagMetrics.mcpActiveClientCount) clients · \(state.runtimeLagMetrics.mcpHelperCount) helpers"
        case .uiPayloads:
            return "\(state.timelinePayloadDiagnostics.filteredEventCount)/\(state.timelinePayloadDiagnostics.totalEventCount) timeline events"
        case .memory:
            return byteCount(state.selfMemoryAttribution?.currentResidentBytes ?? state.runtimeLagMetrics.selfMemoryBytes)
        case .eventStream:
            return "\(state.diagnosticsEvents.count) loaded · \(eventClusters.count) groups"
        }
    }

    private func sectionSignalColor(_ section: DiagnosticsSection) -> Color {
        switch section {
        case .health:
            return healthColor(diagnosticsHealthTitle) ?? .secondary
        case .crashReboot:
            return crashRebootEvents.contains(where: { $0.level == .error || $0.eventType.contains("panic") || $0.eventType.contains("reset") })
                ? AetowerDesign.Status.error
                : .secondary
        case .mcp:
            return mcpLifecycleColor ?? .secondary
        case .uiPayloads:
            return state.timelinePayloadDiagnostics.safeModeEnabled ? AetowerDesign.Status.success : AetowerDesign.Status.warning
        case .memory:
            return AetowerDesign.Tone.memory
        case .eventStream:
            return levelFilter == .error ? AetowerDesign.Status.error : .secondary
        }
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

    private var crashRebootQuery: DiagnosticsQuery {
        DiagnosticsQuery(
            limit: 750,
            minimumLevel: nil,
            subsystem: nil,
            search: nil,
            sinceMillis: nil,
            includePersisted: true
        )
    }

    private var activeDiagnosticsQuery: DiagnosticsQuery {
        selectedSection == .crashReboot ? crashRebootQuery : currentQuery
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
                .engine, .collector, .identity, .attribution, .friction,
                .history, .persistence, .telemetry, .gpu, .ffi, .ui,
                .adapterChromium, .adapterDocker, .adapterHelper, .adapterChau7,
                .adapterVsCode
            ]
        }
        return Array(seen).sorted { subsystemLabel($0) < subsystemLabel($1) }
    }

    private var crashRebootEvents: [DiagnosticsEvent] {
        state.diagnosticsEvents.filter { event in
            switch event.eventType {
            case "boot-session-observed",
                 "system-previous-shutdown-cause",
                 "system-panic-marker",
                 "system-sleep-marker",
                 "system-wake-marker",
                 "system-thermal-marker",
                 "system-power-marker",
                 "system-reset-counter-report",
                 "system-panic-manifest",
                 "system-panic-file-observed",
                 "host-incident-snapshot",
                 "tick-over-budget",
                 "history-maintenance-over-budget",
                 "long-state-lock-wait":
                return true
            default:
                return false
            }
        }
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
        if summary.metalLoadFailures > 0
            || summary.invalidDisplayIdentifierCount >= SessionLogSummary.invalidDisplayIdentifierWarningThreshold
        {
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
            return "Unified log analysis is starting…"
        }
        return "\(summary.cursorUiEntries) cursor updates, \(summary.metalLoadFailures) Metal errors, \(summary.viewBridgeCancellationCount) view bridge cancellations, \(summary.invalidDisplayIdentifierCount) invalid display identifiers · \(sessionLogFreshnessLabel)"
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

    private var mcpLifecycleColor: Color? {
        if state.runtimeLagMetrics.staleMcpHelperCount > 0 || state.runtimeLagMetrics.mcpHelperCount >= 8 {
            return AetowerDesign.Status.error
        }
        if state.runtimeLagMetrics.mcpHelperCount >= 4 || state.runtimeLagMetrics.mcpRequestsPerSecond >= 10 {
            return AetowerDesign.Status.warning
        }
        return AetowerDesign.Status.success
    }

    private var mcpLifecycleGuidance: String {
        let runtime = state.runtimeLagMetrics
        if runtime.staleMcpHelperCount > 0 {
            return "\(runtime.staleMcpHelperCount) helper process(es) crossed the stale threshold. Check agents that kept stdio sessions open after disconnect."
        }
        if runtime.mcpHelperCount >= 8 && runtime.mcpHelperCount >= runtime.mcpActiveClientCount {
            return "Helper count tracks active clients one-for-one at a high steady state. If this is normal, the next runtime step is a pooled or persistent transport so idle helper overhead does not scale linearly with client count."
        }
        if runtime.mcpRequestsPerSecond >= 10 {
            return "MCP request pressure is elevated. Prefer persistent clients and avoid reconnect/poll loops when agents run longer audits."
        }
        if runtime.mcpHelperCount > 0 {
            return "Helpers are expected while clients are connected. They should drain when agents disconnect; Aetower can currently count clients but cannot identify individual owners."
        }
        return "No helper process is currently visible. Local agents should use the bundled helper, which proxies to this app-owned engine."
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

    private var transportStrategyTitle: String {
        let runtime = state.runtimeLagMetrics
        if runtime.mcpHelperCount >= 8 && runtime.mcpHelperCount >= runtime.mcpActiveClientCount {
            return "Pool candidate"
        }
        if runtime.mcpHelperCount > 0 {
            return "Per-client"
        }
        return "Idle"
    }

    private var transportStrategySubtitle: String {
        let runtime = state.runtimeLagMetrics
        if runtime.mcpHelperCount >= 8 && runtime.mcpHelperCount >= runtime.mcpActiveClientCount {
            return "\(runtime.mcpHelperCount) helpers for \(runtime.mcpActiveClientCount) clients; pooled or persistent transport would cut steady overhead"
        }
        if runtime.mcpHelperCount > 0 {
            return "\(runtime.mcpHelperCount) helpers for \(runtime.mcpActiveClientCount) connected clients"
        }
        return "No active helper process"
    }

    private var transportStrategyColor: Color? {
        let runtime = state.runtimeLagMetrics
        if runtime.mcpHelperCount >= 8 && runtime.mcpHelperCount >= runtime.mcpActiveClientCount {
            return AetowerDesign.Status.warning
        }
        if runtime.mcpHelperCount > 0 {
            return AetowerDesign.Status.neutral
        }
        return AetowerDesign.Status.success
    }

    private var selfPeakSubtitle: String {
        guard let attribution = state.selfMemoryAttribution else {
            return "current self telemetry"
        }
        let delta = attribution.peakDeltaFromCurrentBytes
        let deltaLabel = delta > 0 ? "+\(byteCount(UInt64(delta)))" : byteCount(UInt64(abs(delta)))
        if let peakAtMillis = attribution.peakAtMillis {
            return "\(relativeTimeLabel(from: peakAtMillis)) · delta \(deltaLabel)"
        }
        return "delta \(deltaLabel)"
    }

    private var uiPayloadGuidance: String {
        if state.timelinePayloadDiagnostics.safeModeEnabled {
            return "Operator-safe mode is on. Heavy tabs start with bounded visible windows and collapsed large lists so Diagnostics can show payload cost without forcing the UI to render the full payload immediately."
        }
        return "Use operator-safe mode when these timings start climbing; it keeps History and Timeline summary-first while still letting operators expand deeper lists on demand."
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

    private func crashRebootEvidenceRow(_ event: DiagnosticsEvent) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                Circle()
                    .fill(levelColor(event.level))
                    .frame(width: 8, height: 8)
                Text(event.message)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text(event.eventType)
                    .font(.caption2.monospaced())
                    .foregroundStyle(.secondary)
            }
            Text(formattedTimestamp(event.timestampMillis))
                .font(.caption)
                .foregroundStyle(.secondary)
            if let detail = fieldValue("detail", in: event)
                ?? fieldValue("summary", in: event)
                ?? fieldValue("boot_faults", in: event)
                ?? fieldValue("top_entity_names", in: event) {
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, 3)
    }

    private func crashRebootGuidance(
        events: [DiagnosticsEvent],
        diagnosticReportEvents: [DiagnosticsEvent]
    ) -> String {
        if diagnosticReportEvents.contains(where: { $0.eventType == "system-reset-counter-report" }) {
            return "A ResetCounter artifact means macOS recorded a reboot-level fault. Correlate the timestamp with pressure clues and any panic manifest before assigning blame to a process."
        }
        if diagnosticReportEvents.contains(where: { $0.eventType.contains("panic") }) {
            return "A panic artifact was found. If the referenced panic file is missing, collect a support bundle quickly because macOS may rotate DiagnosticReports."
        }
        if events.contains(where: { $0.eventType == "host-incident-snapshot" }) {
            return "Aetower has pre-reboot pressure breadcrumbs. Use the timeline below to identify whether memory pressure, wakeups, thermal pressure, or Aetower maintenance overlapped the reboot window."
        }
        return "No direct panic/reset artifact is loaded yet. Missing evidence usually means the file was outside the scan window, rotated by macOS, or inaccessible without broader DiagnosticReports access."
    }

    private func bootTimeLabel(_ event: DiagnosticsEvent) -> String? {
        guard let bootTime = fieldValue("boot_time_millis", in: event).flatMap(UInt64.init) else {
            return nil
        }
        return relativeTimeLabel(from: bootTime)
    }

    private func fieldValue(_ key: String, in event: DiagnosticsEvent) -> String? {
        event.fields.first { $0.key == key }?.value
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

private func diagnosticsRecommendationCard(_ recommendation: OperatorRecommendationSummary) -> some View {
    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
        HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
            Text(recommendation.title)
                .font(.subheadline.weight(.semibold))
            Spacer()
            Text(recommendation.severity.label)
                .font(.caption.weight(.semibold))
                .foregroundStyle(recommendation.severity.color)
        }
        Text(recommendation.detail)
            .font(.caption)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        Text(recommendation.action)
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .fixedSize(horizontal: false, vertical: true)
    }
    .padding(AetowerDesign.Spacing.sm)
    .frame(maxWidth: .infinity, minHeight: 112, alignment: .topLeading)
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

private func durationLabel(_ millis: UInt64) -> String {
    let seconds = millis / 1000
    if seconds < 60 {
        return "\(seconds)s"
    }
    if seconds < 3600 {
        return "\(seconds / 60)m"
    }
    return "\(seconds / 3600)h \(seconds % 3600 / 60)m"
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
    case "Clean", "Quiet", "Stable", "Bounded", "Verified", "Enabled", "Live":
        return AetowerDesign.Status.success
    case "Watch", "Noisy", "Chatty", "Stale", "Pending", "Unknown", "Starting", "Down":
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

/// Cached file-style byte formatter. The class method
/// ByteCountFormatter.string(fromByteCount:countStyle:) allocates a
/// throwaway formatter on every call. Caching avoids that overhead
/// when called multiple times per render (persistence detail rows).
nonisolated(unsafe) private let diagnosticsByteFormatter: ByteCountFormatter = {
    let formatter = ByteCountFormatter()
    formatter.countStyle = .file
    return formatter
}()

private func byteCount(_ bytes: UInt64) -> String {
    diagnosticsByteFormatter.string(fromByteCount: Int64(bytes))
}
