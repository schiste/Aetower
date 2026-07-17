import SwiftUI
import AetowerBridge

private struct SectionEyebrow: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.caption.weight(.semibold))
            .foregroundStyle(.tertiary)
            .tracking(0.8)
    }
}

private enum MonitorSortColumn {
    case name
    case cpu
    case memory
    case wakeups
    case processCount
    case friction

    var title: String {
        switch self {
        case .name: return "Name"
        case .cpu: return "CPU"
        case .memory: return "MEM"
        case .wakeups: return "Wake"
        case .processCount: return "PIDs"
        case .friction: return "Friction"
        }
    }

    var width: CGFloat? {
        switch self {
        case .name: return nil
        case .cpu: return 64
        case .memory: return 84
        case .wakeups: return 72
        case .processCount: return 56
        case .friction: return 68
        }
    }

    var alignment: Alignment {
        switch self {
        case .name:
            return .leading
        case .cpu, .memory, .wakeups, .processCount, .friction:
            return .center
        }
    }
}

private struct MonitorHeaderButton: View {
    let column: MonitorSortColumn
    let isActive: Bool
    let indicatorSymbol: String?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 3) {
                Text(column.title)
                    .lineLimit(1)
                if let indicatorSymbol {
                    Image(systemName: indicatorSymbol)
                        .font(.system(size: 7, weight: .bold))
                }
            }
            .frame(
                maxWidth: column.width == nil ? .infinity : nil,
                alignment: column.alignment
            )
            .frame(width: column.width, alignment: column.alignment)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(isActive ? AnyShapeStyle(Color.primary) : AnyShapeStyle(.tertiary))
        .help(helpText)
    }

    private var helpText: String {
        switch column {
        case .name:
            return "Click to toggle alphabetical order."
        case .cpu:
            return "Sort by highest CPU usage."
        case .memory:
            return "Sort by highest resident memory."
        case .wakeups:
            return "Sort by highest wakeup rate."
        case .processCount:
            return "Sort by most live PIDs."
        case .friction:
            return "Sort by highest friction score."
        }
    }
}

private struct SidePanelQuickStopSubmission: Equatable {
    let entityID: String
    let pid: UInt32
    let action: ProcessActionKind
    let actionID: String
    let submittedAt: Date
}

private struct SidePanelQuickStatusLine: View {
    let icon: String
    let color: Color
    let text: String

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            Image(systemName: icon)
                .foregroundStyle(color)
                .frame(width: 14)
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct SidePanelSignalLine: View {
    let icon: String
    let tone: Color
    let title: String
    let detail: String?

    var body: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: icon)
                .font(AetowerDesign.Typography.compactData(size: 11, weight: .semibold))
                .foregroundStyle(AnyShapeStyle(tone), AnyShapeStyle(AetowerDesign.Ink.secondary))
                .frame(width: 16, height: 16)

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text(title)
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .foregroundStyle(AetowerDesign.Ink.primary)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                if let detail, !detail.isEmpty {
                    Text(detail)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            Spacer(minLength: AetowerDesign.Spacing.xs)
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
        HStack(spacing: AetowerDesign.Spacing.sm) {
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
                    isHighlighted ? AetowerDesign.Status.error.opacity(0.9) : .clear,
                    lineWidth: isHighlighted ? 1 : 0
                )
        )
    }
}

private struct MonitorMetricCardDescriptor: Identifiable {
    let id: MonitorMetricCardFocus
    let title: String
    let value: String
    let fixedScaleValue: String?
    let subtitle: String
    let samples: [Double]
    let style: TrendMetricStyle
    let valueAppearance: TrendMetricValueAppearance
    let sampleValueFormatter: (Double) -> String
    let fixedScaleSampleValueFormatter: ((Double) -> String)?
    /// Absolute top of the y-axis when the user enables fixed ring scaling.
    /// 0–100-native metrics use 100; unbounded ones reuse the danger
    /// thresholds in `MonitorRingCeiling`. A value of 0 means "no fixed
    /// ceiling available" (e.g. GPU memory) and falls back to relative.
    let fixedCeiling: Double
}

/// Fixed-mode ceilings for the unbounded Monitor rings, kept in lockstep with
/// the danger thresholds used for value coloring so the axis and the warning
/// bands can't drift apart.
private enum MonitorRingCeiling {
    /// Disk read+write danger threshold (also `monitorThroughputAppearance` danger).
    static let diskBps: Double = 250 * 1_024 * 1_024
    /// Network send+receive danger threshold.
    static let networkBps: Double = 100 * 1_024 * 1_024
    /// Wakeup "storm" band (`hostWakeupBand` severe, MonitorFormatters).
    static let wakeupsPerSecond: Double = 3_000
    /// 0–100-native metrics (friction, CPU %, memory pressure, GPU %).
    static let percent: Double = 100
}

public struct MainListView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var selectedEntityID: String?
    @State private var selectedProcessRowID: String?
    @State private var searchText = ""
    @State private var originFilter: ProcessOriginFilter = .all
    @State private var sortKey: SortKey = .friction
    @State private var focusedIndex: Int = 0
    @State private var listMode: ListMode = .grouped
    @State private var groupedEntitiesCache: [GroupingCacheKey: [EntityGroup]] = [:]
    @State private var expandedGroupIDs = Set<String>()
    @State private var groupingTask: Task<[EntityGroup], Never>?
    @State private var isGrouping = false
    @State private var processOperatorRequest: ProcessOperatorRequest?
    @State private var quickStopSubmission: SidePanelQuickStopSubmission?
    @State private var advancedFilterText = ""
    @State private var showAdvancedFilter = false
    @State private var sidePanelWhyExpanded = false
    @State private var sidePanelMembersExpanded = false
    @State private var sidePanelWatchExpanded = false
    @StateObject private var processOriginCacheStore = ProcessOriginSnapshotCacheStore()
    @StateObject private var monitorSectionCacheStore = MonitorEntitySectionCacheStore()
    @StateObject private var monitorGroupRowCacheStore = MonitorGroupRowCacheStore()
    @FocusState private var searchFieldFocused: Bool

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    private var processOriginCache: ProcessOriginSnapshotCache {
        processOriginCacheStore.cache(sequence: state.snapshotSequence, entities: state.entitiesState)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.none) {
            monitorTabToolBand
            Divider()
            thermalForecastBanner
            monitorContentLayout
                .padding(.horizontal, AetowerDesign.Spacing.sm)
                .padding(.vertical, AetowerDesign.Spacing.sm)
        }
        .navigationTitle("Monitor")
        .modifier(KeyboardNavigationModifier(
            focusedIndex: $focusedIndex,
            selectedEntityID: $selectedEntityID,
            sortKey: $sortKey,
            entityCount: visibleEntityIDs.count,
            entityIdAt: { index in
                let ids = visibleEntityIDs
                guard index >= 0, index < ids.count else { return nil }
                return ids[index]
            }
        ))
        .task(id: groupingTaskToken) {
            await refreshGroupingCache()
        }
        .onChange(of: selectedEntityID) { _, newValue in
            if newValue != nil {
                searchFieldFocused = false
            }
            if let selectedProcessRowID,
               let newValue,
               !selectedProcessRowID.hasPrefix("\(newValue):") {
                self.selectedProcessRowID = nil
            }
            if newValue == nil {
                selectedProcessRowID = nil
            }
        }
        .onChange(of: state.monitorFocusEntityID) { _, _ in
            applyPendingMonitorFocusIfNeeded()
        }
        .task {
            applyPendingMonitorFocusIfNeeded()
        }
        .task(id: monitorUiPerformanceBudgetToken) {
            state.recordMonitorRowBuildDiagnostics(
                rowBuildMillis: monitorVisibleRowBuildMillis,
                visibleRowCount: monitorVisibleRowCount
            )
        }
        .onDisappear {
            groupingTask?.cancel()
            groupingTask = nil
            searchFieldFocused = false
        }
    }

    @ViewBuilder
    private var monitorContentLayout: some View {
        switch settings.monitorMetricCardPlacement {
        case .top:
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                monitorOverviewSummary
                monitorSplitView
            }
        case .bottom:
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                monitorSplitView
                monitorOverviewSummary
            }
        case .left:
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.lg) {
                monitorMetricRail
                monitorSplitView
            }
        case .right:
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.lg) {
                monitorSplitView
                monitorMetricRail
            }
        }
    }

    private var monitorMetricRail: some View {
        ScrollView {
            monitorOverviewSummary
        }
        .frame(minWidth: 190, idealWidth: 220, maxWidth: 250, maxHeight: .infinity, alignment: .top)
    }

    private var rankingPanel: some View {
        ScrollView {
            rankedEntitiesSection
        }
    }

    private var monitorSplitView: some View {
        Group {
            if let entity = selectedEntity {
                HSplitView {
                    rankingPanel
                        .frame(
                            minWidth: 420,
                            idealWidth: 760,
                            maxWidth: .infinity,
                            maxHeight: .infinity
                        )
                        .layoutPriority(2)

                    detailPanel(for: entity)
                        .transition(.opacity)
                        .frame(
                            minWidth: 360,
                            idealWidth: 760,
                            maxWidth: .infinity,
                            maxHeight: .infinity,
                            alignment: .topLeading
                        )
                        .layoutPriority(1)
                }
            } else {
                rankingPanel
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .animation(AetowerDesign.Motion.standard, value: selectedEntity?.entityId)
    }

    private var monitorTabToolBand: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search processes, origin:cli, cpu>50",
            searchWidth: 320
        ) {
            Picker(selection: $listMode) {
                ForEach(ListMode.allCases) { mode in
                    Image(systemName: mode.icon)
                        .tag(mode)
                }
            } label: {
                EmptyView()
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityLabel("Layout")
            .frame(width: 86)
            .onChange(of: listMode) { _, _ in
                focusedIndex = 0
                if listMode == .flat {
                    expandedGroupIDs.removeAll()
                    selectedProcessRowID = nil
                }
                if let selectedEntityID, !visibleEntityIDs.contains(selectedEntityID) {
                    self.selectedEntityID = nil
                    selectedProcessRowID = nil
                }
            }
        } filterTools: {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                sortMenu
                originMenu
                advancedFilterButton
            }
        } badges: {
            AetowerToolBadgeGroup(monitorHeaderBadges, visibleCount: 2)
        }
    }

    private var monitorHeaderBadges: [AetowerToolBadgeItem] {
        var items = [
            AetowerToolBadgeItem(
                isGroupedMode ? "Groups" : "Entities",
                value: "\(visiblePrimaryRowCount)",
                systemImage: isGroupedMode ? "square.grid.2x2" : "list.bullet",
                tone: AetowerDesign.Tone.friction
            ),
            AetowerToolBadgeItem(
                "PIDs",
                value: "\(visibleProcessCount)",
                systemImage: "number",
                tone: AetowerDesign.Tone.cpu
            ),
        ]
        if isGroupedMode && isGrouping {
            items.append(
                AetowerToolBadgeItem(
                    "Grouping",
                    value: "Running",
                    systemImage: "arrow.triangle.2.circlepath",
                    tone: AetowerDesign.Status.warning
                )
            )
        }
        return items
    }

    private var sortMenu: some View {
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
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Text(sortKey.title)
                    .font(AetowerDesign.Typography.caption.weight(.medium))
                Image(systemName: "chevron.down")
                    .font(AetowerDesign.Typography.compactData(size: 8, weight: .semibold))
            }
            .foregroundStyle(AetowerDesign.Ink.secondary)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .aetowerControlChrome()
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    private var originMenu: some View {
        Menu {
            ForEach(ProcessOriginFilter.allCases) { filter in
                Button {
                    originFilter = filter
                    focusedIndex = 0
                    if let selectedEntityID, !visibleEntityIDs.contains(selectedEntityID) {
                        self.selectedEntityID = nil
                        selectedProcessRowID = nil
                    }
                } label: {
                    HStack {
                        Text(filter.label)
                        if filter == originFilter {
                            Spacer()
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: "point.3.connected.trianglepath.dotted")
                    .font(AetowerDesign.Typography.compactData(size: 9, weight: .semibold))
                Text(originFilter.menuLabel)
                    .font(AetowerDesign.Typography.caption.weight(.medium))
                Image(systemName: "chevron.down")
                    .font(AetowerDesign.Typography.compactData(size: 8, weight: .semibold))
            }
            .foregroundStyle(originFilter == .all ? AnyShapeStyle(AetowerDesign.Ink.secondary) : AnyShapeStyle(Color.accentColor))
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .aetowerControlChrome()
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    private func detailPanel(for entity: EntitySnapshot) -> some View {
        let processTreeEntities = selectedProcessTreeEntities(for: entity)
        let quickStopPID = primaryProcessID(in: processTreeEntities)
        let quickStopDisplayPID = quickStopPID ?? retainedQuickStopPID(for: entity.entityId)
        let origin = processOriginCache.summary(for: processTreeEntities)
        let browserTabs = sidePanelBrowserTabComponents(for: entity)

        return VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                sidePanelInspectorHeader(
                    entity: entity,
                    origin: origin,
                    processTreeEntities: processTreeEntities
                )

                if let quickStopDisplayPID {
                    sidePanelActionStrip(
                        entity: entity,
                        pid: quickStopDisplayPID,
                        targetVisible: quickStopPID != nil
                    )
                }
            }
            .padding(.horizontal, AetowerDesign.Spacing.lg)
            .padding(.vertical, AetowerDesign.Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                    sidePanelCurrentRead(
                        entity: entity,
                        origin: origin,
                        processTreeEntities: processTreeEntities
                    )

                    sidePanelOperationPanel(
                        entity: entity,
                        processTreeEntities: processTreeEntities
                    )

                    sidePanelMembers(
                        entity: entity,
                        processTreeEntities: processTreeEntities,
                        browserTabs: browserTabs
                    )

                    sidePanelWhy(entity)

                    sidePanelWatch(entity)
                }
                .padding(.horizontal, AetowerDesign.Spacing.lg)
                .padding(.vertical, AetowerDesign.Spacing.md)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .animation(nil, value: state.snapshotSequence)
    }

    private func sidePanelSection<Content: View>(
        _ title: String,
        systemImage: String,
        badge: String? = nil,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Label(title, systemImage: systemImage)
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                if let badge {
                    AetowerBadge(
                        badge,
                        tone: AetowerDesign.Status.neutral,
                        style: .outline
                    )
                }
                Spacer(minLength: AetowerDesign.Spacing.xs)
            }

            content()
        }
    }

    private func sidePanelDisclosureSection<Content: View>(
        _ title: String,
        systemImage: String,
        badge: String? = nil,
        isExpanded: Binding<Bool>,
        @ViewBuilder content: @escaping () -> Content
    ) -> some View {
        DisclosureGroup(isExpanded: isExpanded) {
            content()
                .padding(.top, AetowerDesign.Spacing.xs)
        } label: {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Label(title, systemImage: systemImage)
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                if let badge {
                    AetowerBadge(
                        badge,
                        tone: AetowerDesign.Status.neutral,
                        style: .outline
                    )
                }
                Spacer(minLength: AetowerDesign.Spacing.xs)
            }
        }
        .disclosureGroupStyle(.automatic)
    }

    private func sidePanelCurrentRead(
        entity: EntitySnapshot,
        origin: ProcessOriginSummary,
        processTreeEntities: [EntitySnapshot]
    ) -> some View {
        sidePanelSection("Current Read", systemImage: "waveform.path.ecg") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                sidePanelDominantIssue(entity)

                sidePanelMetricStrip(for: entity)

                if let recentChangeSummary = entity.recentChangeSummary, !recentChangeSummary.isEmpty {
                    SidePanelSignalLine(
                        icon: "clock.arrow.circlepath",
                        tone: AetowerDesign.Tone.network,
                        title: "Recent change",
                        detail: recentChangeSummary
                    )
                }

                if let activeWindowTitle = entity.activeWindowTitle, !activeWindowTitle.isEmpty {
                    SidePanelSignalLine(
                        icon: "macwindow",
                        tone: AetowerDesign.Status.neutral,
                        title: "Active window",
                        detail: activeWindowTitle
                    )
                }

                sidePanelOriginSummary(origin, processTreeEntities: processTreeEntities)
            }
        }
    }

    @ViewBuilder
    private func sidePanelDominantIssue(_ entity: EntitySnapshot) -> some View {
        if let recommendation = sidePanelPrimaryRecommendation(for: entity) {
            SidePanelSignalLine(
                icon: sidePanelRecommendationIcon(recommendation.severity),
                tone: sidePanelRecommendationTone(recommendation.severity),
                title: recommendation.title,
                detail: recommendation.detail
            )
        } else if let firstReason = entity.friction.reasons.first, !firstReason.isEmpty {
            SidePanelSignalLine(
                icon: "target",
                tone: AetowerDesign.frictionColor(entity.friction.totalScore),
                title: "Dominant issue",
                detail: firstReason
            )
        } else if entity.anomalyDetected {
            SidePanelSignalLine(
                icon: "exclamationmark.triangle.fill",
                tone: AetowerDesign.Status.warning,
                title: "Anomaly detected",
                detail: "This entity is behaving outside its recent baseline."
            )
        } else {
            SidePanelSignalLine(
                icon: "checkmark.circle",
                tone: AetowerDesign.Status.ready,
                title: "No strong issue attached",
                detail: "Current signals are informational unless the list sort is highlighting a specific metric."
            )
        }
    }

    private func sidePanelWhy(_ entity: EntitySnapshot) -> some View {
        sidePanelDisclosureSection(
            "Why",
            systemImage: "questionmark.circle",
            badge: sidePanelFrictionTrendBadge(for: entity),
            isExpanded: $sidePanelWhyExpanded
        ) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                SidePanelSignalLine(
                    icon: "chart.line.uptrend.xyaxis",
                    tone: AetowerDesign.frictionColor(entity.friction.totalScore),
                    title: sidePanelFrictionSummary(for: entity),
                    detail: sidePanelFrictionTrendDetail(for: entity)
                )

                if entity.friction.reasons.isEmpty {
                    SidePanelSignalLine(
                        icon: "info.circle",
                        tone: AetowerDesign.Status.neutral,
                        title: "No specific reason attached",
                        detail: "The score is currently driven by the live metrics and list sort."
                    )
                } else {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        ForEach(Array(entity.friction.reasons.prefix(3).enumerated()), id: \.offset) { _, reason in
                            SidePanelSignalLine(
                                icon: "smallcircle.filled.circle",
                                tone: AetowerDesign.frictionColor(entity.friction.totalScore),
                                title: reason,
                                detail: nil
                            )
                        }
                    }
                }

                if !entity.friction.contributors.isEmpty {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        Text("Top contributors")
                            .font(AetowerDesign.Typography.metadataStrong)
                            .foregroundStyle(AetowerDesign.Ink.tertiary)
                        ForEach(Array(entity.friction.contributors.prefix(3).enumerated()), id: \.offset) { _, contributor in
                            sidePanelContributorRow(contributor, totalScore: entity.friction.totalScore)
                        }
                    }
                }
            }
        }
    }

    private func sidePanelContributorRow(
        _ contributor: FrictionContributor,
        totalScore: Float
    ) -> some View {
        let share = sidePanelContributorShare(contributor, totalScore: totalScore)
        let tone = sidePanelContributorTone(contributor)

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                Text(contributor.label)
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .foregroundStyle(AetowerDesign.Ink.primary)
                    .lineLimit(1)
                Spacer(minLength: AetowerDesign.Spacing.xs)
                Text(String(format: "%.1f", contributor.score))
                    .font(AetowerDesign.Typography.dataSmall)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }

            ProgressView(value: Double(share))
                .progressViewStyle(.linear)
                .tint(tone)
                .controlSize(.mini)

            if !contributor.detail.isEmpty {
                Text(contributor.detail)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func sidePanelMembers(
        entity: EntitySnapshot,
        processTreeEntities: [EntitySnapshot],
        browserTabs: [ComponentSnapshot]
    ) -> some View {
        let processRefs = sidePanelProcessRefs(for: processTreeEntities)
        let componentCount = processTreeEntities.reduce(0) { $0 + $1.components.count }
        let memberBadge = "\(sidePanelLiveProcessCount(processTreeEntities)) PIDs"

        return sidePanelDisclosureSection(
            "Members",
            systemImage: "list.bullet.indent",
            badge: memberBadge,
            isExpanded: $sidePanelMembersExpanded
        ) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                if processTreeEntities.count > 1 {
                    SidePanelSignalLine(
                        icon: "square.stack.3d.up",
                        tone: AetowerDesign.Tone.cpu,
                        title: "\(processTreeEntities.count) grouped entities",
                        detail: "\(sortKey.title) order"
                    )
                }

                if processRefs.isEmpty {
                    SidePanelSignalLine(
                        icon: "eye.slash",
                        tone: AetowerDesign.Status.neutral,
                        title: "No live process details",
                        detail: "The entity is visible, but this snapshot has no process rows to expand."
                    )
                } else {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        ForEach(Array(processRefs.prefix(6).enumerated()), id: \.element.id) { _, processRef in
                            sidePanelProcessRefRow(processRef)
                        }
                    }

                    if processRefs.count > 6 {
                        Text("+\(processRefs.count - 6) more process rows follow the same sort order.")
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.tertiary)
                    }
                }

                sidePanelBrowserTabs(browserTabs)

                if componentCount > browserTabs.count {
                    SidePanelSignalLine(
                        icon: "puzzlepiece.extension",
                        tone: AetowerDesign.Status.neutral,
                        title: "\(componentCount) component snapshots",
                        detail: entity.components.isEmpty ? "Grouped members provide the component context." : "Process and adapter components are folded into this inspector."
                    )
                }
            }
        }
    }

    private func sidePanelProcessRefRow(_ processRef: MonitorProcessComponentRef) -> some View {
        Button {
            selectProcessRow(MonitorProcessRowModel(reference: processRef))
        } label: {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: "terminal")
                    .font(AetowerDesign.Typography.compactData(size: 11, weight: .semibold))
                    .foregroundStyle(AetowerDesign.Tone.cpu)
                    .frame(width: 16, height: 16)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text(processRef.title.isEmpty ? processRef.owner.displayName : processRef.title)
                        .font(AetowerDesign.Typography.caption.weight(.medium))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)

                    Text(sidePanelProcessRefDetail(processRef))
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                }

                Spacer(minLength: AetowerDesign.Spacing.xs)

                Text(verbatim: processPIDLabel(processRef.pid))
                    .font(AetowerDesign.Typography.compactData(size: 10, weight: .medium))
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(MonitorProcessRowModel(reference: processRef).helpText)
    }

    private func sidePanelWatch(_ entity: EntitySnapshot) -> some View {
        sidePanelDisclosureSection(
            "Watch",
            systemImage: "bell.badge",
            badge: sidePanelWatchBadge(for: entity),
            isExpanded: $sidePanelWatchExpanded
        ) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                if let bundleId = entity.bundleId, !bundleId.isEmpty {
                    SidePanelSignalLine(
                        icon: "bell.slash",
                        tone: AetowerDesign.Status.neutral,
                        title: "Snooze notifications",
                        detail: "Mute alerts for this app without changing Monitor visibility."
                    )

                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        sidePanelSnoozeButton("1h", entity: entity, bundleId: bundleId, hours: 1)
                        sidePanelSnoozeButton("4h", entity: entity, bundleId: bundleId, hours: 4)
                        sidePanelSnoozeButton("24h", entity: entity, bundleId: bundleId, hours: 24)
                    }
                } else {
                    SidePanelSignalLine(
                        icon: "bell",
                        tone: AetowerDesign.Status.neutral,
                        title: "No app bundle target",
                        detail: "Snooze controls need a bundle identifier; this entity may be a process, service, or grouped runtime."
                    )
                }

                SidePanelSignalLine(
                    icon: "slider.horizontal.3",
                    tone: AetowerDesign.Tone.memory,
                    title: "Automation rules live in Settings",
                    detail: "Use Settings > Automation for durable rules; this panel stays focused on the current selection."
                )
            }
        }
    }

    @ViewBuilder
    private func sidePanelOperationPanel(
        entity: EntitySnapshot,
        processTreeEntities: [EntitySnapshot]
    ) -> some View {
        if processOperatorRequest != nil {
            sidePanelSection("Operation", systemImage: "wrench.and.screwdriver") {
                ProcessOperatorPanel(
                    entity: entity,
                    state: state,
                    processEntities: processTreeEntities,
                    processSortKey: selectedEntityGroup?.root.entityId == entity.entityId ? sortKey : nil,
                    quickRequest: processOperatorRequest
                )
            }
        }
    }

    private func sidePanelPrimaryRecommendation(for entity: EntitySnapshot) -> Recommendation? {
        entity.recommendations.sorted {
            let leftRank = sidePanelRecommendationRank($0.severity)
            let rightRank = sidePanelRecommendationRank($1.severity)
            if leftRank != rightRank {
                return leftRank > rightRank
            }
            return $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
        }
        .first
    }

    private func sidePanelRecommendationRank(_ severity: RecommendationSeverity) -> Int {
        switch severity {
        case .urgent: return 3
        case .suggested: return 2
        case .info: return 1
        @unknown default: return 0
        }
    }

    private func sidePanelRecommendationTone(_ severity: RecommendationSeverity) -> Color {
        switch severity {
        case .urgent: return AetowerDesign.Status.error
        case .suggested: return AetowerDesign.Status.warning
        case .info: return AetowerDesign.Status.neutral
        @unknown default: return AetowerDesign.Status.neutral
        }
    }

    private func sidePanelRecommendationIcon(_ severity: RecommendationSeverity) -> String {
        switch severity {
        case .urgent: return "exclamationmark.triangle.fill"
        case .suggested: return "lightbulb"
        case .info: return "info.circle"
        @unknown default: return "info.circle"
        }
    }

    private func sidePanelFrictionTrendBadge(for entity: EntitySnapshot) -> String {
        trendLabel(samples: entity.trend.friction.map(Double.init), stableText: "stable")
    }

    private func sidePanelFrictionSummary(for entity: EntitySnapshot) -> String {
        "Friction \(String(format: "%.1f", entity.friction.totalScore))"
    }

    private func sidePanelFrictionTrendDetail(for entity: EntitySnapshot) -> String {
        let trend = sidePanelFrictionTrendBadge(for: entity)
        return "\(trend.capitalized) over \(trendWindowLabel(sampleCount: entity.trend.friction.count)); current sort is \(sortKey.title)."
    }

    private func sidePanelContributorShare(
        _ contributor: FrictionContributor,
        totalScore: Float
    ) -> CGFloat {
        guard totalScore > 0 else { return 0 }
        return CGFloat(min(1, max(0, contributor.score / totalScore)))
    }

    private func sidePanelContributorTone(_ contributor: FrictionContributor) -> Color {
        let key = contributor.key.lowercased()
        if key.contains("cpu") { return AetowerDesign.Tone.cpu }
        if key.contains("memory") || key.contains("pressure") { return AetowerDesign.Tone.memory }
        if key.contains("disk") { return AetowerDesign.Tone.disk }
        if key.contains("network") { return AetowerDesign.Tone.network }
        if key.contains("wake") { return AetowerDesign.Tone.wakeups }
        if key.contains("energy") { return AetowerDesign.Tone.energy }
        return AetowerDesign.frictionColor(contributor.score)
    }

    private func sidePanelProcessRefs(for entities: [EntitySnapshot]) -> [MonitorProcessComponentRef] {
        sortEntities(entities, by: sortKey)
            .enumerated()
            .flatMap { ownerIndex, owner in
                let lineageRefs = owner.processLineage.map {
                    MonitorProcessComponentRef(owner: owner, lineage: $0, ownerSortIndex: ownerIndex)
                }
                if !lineageRefs.isEmpty {
                    return lineageRefs
                }

                return owner.components.compactMap { component -> MonitorProcessComponentRef? in
                    guard component.kind != .adapterContext, component.processId != nil else {
                        return nil
                    }
                    return MonitorProcessComponentRef(owner: owner, component: component, ownerSortIndex: ownerIndex)
                }
            }
            .sorted { compareProcessComponents($0, $1, by: sortKey) }
    }

    private func sidePanelProcessRefDetail(_ processRef: MonitorProcessComponentRef) -> String {
        let memoryBytes = max(processRef.memoryPhysicalFootprintBytes, processRef.memoryBytes)
        var parts = [
            String(format: "%.1f%% CPU", processRef.cpuPercent),
            formatBytes(memoryBytes),
            "\(processRef.threadCount) threads",
        ]
        if let user = processRef.user, !user.isEmpty {
            parts.append(user)
        }
        return parts.joined(separator: " · ")
    }

    private func sidePanelWatchBadge(for entity: EntitySnapshot) -> String {
        entity.bundleId?.isEmpty == false ? "app" : "runtime"
    }

    private func sidePanelSnoozeButton(
        _ label: String,
        entity: EntitySnapshot,
        bundleId: String,
        hours: Double
    ) -> some View {
        Button {
            state.snoozeNotifications(
                bundleId: bundleId,
                displayName: entity.displayName,
                hours: hours
            )
        } label: {
            Label(label, systemImage: "bell.slash")
                .font(AetowerDesign.Typography.caption.weight(.semibold))
        }
        .buttonStyle(.borderless)
        .help("Snooze \(entity.displayName) notifications for \(label).")
    }

    private func sidePanelInspectorHeader(
        entity: EntitySnapshot,
        origin: ProcessOriginSummary,
        processTreeEntities: [EntitySnapshot]
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: sidePanelEntityIconName(for: entity))
                    .font(AetowerDesign.Typography.metricValue(size: 18, weight: .semibold))
                    .foregroundStyle(AetowerDesign.frictionColor(entity.friction.totalScore))
                    .frame(width: 24, height: 24)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                        Text(entity.displayName)
                            .font(AetowerDesign.Typography.sectionTitle.weight(.semibold))
                            .foregroundStyle(AetowerDesign.Ink.primary)
                            .lineLimit(1)
                            .minimumScaleFactor(0.8)

                        FrictionStatusBadge(score: Double(entity.friction.totalScore))
                    }

                    Text(topConcernSummary(for: entity, sortKey: sortKey))
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }

                Spacer(minLength: AetowerDesign.Spacing.sm)

                Button {
                    withAnimation(AetowerDesign.Motion.standard) {
                        selectedEntityID = nil
                        selectedProcessRowID = nil
                    }
                } label: {
                    Image(systemName: "xmark")
                        .font(AetowerDesign.Typography.compactData(size: 12, weight: .semibold))
                        .frame(width: 24, height: 24)
                }
                .buttonStyle(.plain)
                .foregroundStyle(AetowerDesign.Ink.tertiary)
                .help("Close inspector")
            }

            HStack(spacing: AetowerDesign.Spacing.xs) {
                ProcessOriginChip(summary: origin)
                AetowerBadge(
                    entity.metrics.isForeground ? "Frontmost" : "Background",
                    systemImage: entity.metrics.isForeground ? "macwindow.on.rectangle" : "rectangle.dashed",
                    tone: entity.metrics.isForeground ? AetowerDesign.Status.ready : AetowerDesign.Status.neutral
                )
                AetowerBadge(
                    "\(sidePanelLiveProcessCount(processTreeEntities)) PIDs",
                    systemImage: "number",
                    tone: AetowerDesign.Tone.cpu
                )
            }
        }
    }

    private func sidePanelMetricStrip(for entity: EntitySnapshot) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 92), spacing: AetowerDesign.Spacing.md)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.sm
        ) {
            AetowerMetricReadout(
                label: "CPU",
                value: String(format: "%.1f%%", entity.metrics.cpuPercent),
                detail: trendLabel(samples: entity.trend.cpuPercent.map(Double.init), stableText: "current"),
                tone: AetowerDesign.Tone.cpu
            )
            AetowerMetricReadout(
                label: "Memory",
                value: formatBytes(entityEffectiveMemoryBytes(entity)),
                detail: entity.metrics.memoryPhysicalFootprintBytes > 0 ? "charged" : "resident",
                tone: AetowerDesign.Tone.memory
            )
            AetowerMetricReadout(
                label: "Wakeups",
                value: formatWakeups(entity.metrics.wakeupsPerSecond),
                detail: "timer churn",
                tone: AetowerDesign.Tone.wakeups
            )
            AetowerMetricReadout(
                label: "Network",
                value: formatRate(entity.metrics.networkReceiveBps + entity.metrics.networkSendBps),
                detail: "in + out",
                tone: AetowerDesign.Tone.network
            )
        }
    }

    private func sidePanelOriginSummary(
        _ origin: ProcessOriginSummary,
        processTreeEntities: [EntitySnapshot]
    ) -> some View {
        HStack(spacing: 8) {
            Label("Attribution", systemImage: "point.3.connected.trianglepath.dotted")
                .font(AetowerDesign.Typography.caption.weight(.semibold))
                .foregroundStyle(AetowerDesign.Ink.secondary)
            ProcessOriginChip(summary: origin)
            Text(origin.subtitle)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(1)
            if processTreeEntities.count > 1 {
                AetowerBadge(
                    "\(processTreeEntities.count) entities",
                    tone: AetowerDesign.Status.neutral,
                    style: .outline
                )
            }
            Spacer(minLength: AetowerDesign.Spacing.xs)
        }
        .help(origin.detailLines.joined(separator: "\n"))
    }

    private func sidePanelActionStrip(
        entity: EntitySnapshot,
        pid: UInt32,
        targetVisible: Bool
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Label("Process actions", systemImage: "wrench.and.screwdriver")
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .foregroundStyle(AetowerDesign.Ink.secondary)

                Text(verbatim: processPIDLabel(pid))
                    .font(AetowerDesign.Typography.compactData(size: 10, weight: .medium))
                    .foregroundStyle(AetowerDesign.Ink.tertiary)

                Spacer(minLength: AetowerDesign.Spacing.xs)
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: AetowerDesign.Size.actionTileMinWidth), spacing: AetowerDesign.Spacing.xs)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.xs
            ) {
                sidePanelProcessActionButton(
                    title: "Inspect",
                    systemImage: "info.circle",
                    isEnabled: targetVisible
                ) {
                    requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .inspect)
                }

                sidePanelProcessActionButton(
                    title: "Files",
                    systemImage: "folder",
                    isEnabled: targetVisible
                ) {
                    requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .resources)
                }

                sidePanelProcessActionButton(
                    title: "Sample",
                    systemImage: "waveform.path.ecg",
                    isEnabled: targetVisible
                ) {
                    requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .sample)
                }

                sidePanelStopButton(.terminate, entity: entity, pid: pid, targetVisible: targetVisible)
                sidePanelStopButton(.forceKill, entity: entity, pid: pid, targetVisible: targetVisible)
            }

            sidePanelQuickStopStatus(entity: entity, pid: pid, targetVisible: targetVisible)
        }
    }

    private func sidePanelProcessActionButton(
        title: String,
        systemImage: String,
        isEnabled: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: systemImage)
                    .font(AetowerDesign.Typography.compactData(size: 12, weight: .semibold))
                    .frame(width: AetowerDesign.Size.iconSlot)
                Text(title)
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .lineLimit(1)
                Spacer(minLength: AetowerDesign.Spacing.xs)
            }
            .foregroundStyle(isEnabled ? AetowerDesign.Ink.primary : AetowerDesign.Ink.tertiary)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .frame(maxWidth: .infinity, alignment: .leading)
            .aetowerControlChrome(minHeight: AetowerDesign.Size.minTouchTarget)
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
        .help(isEnabled ? title : "Target PID is no longer visible.")
    }

    private func sidePanelStopButton(
        _ action: ProcessActionKind,
        entity: EntitySnapshot,
        pid: UInt32,
        targetVisible: Bool
    ) -> some View {
        Button(role: action.isDestructive ? .destructive : nil) {
            runSidePanelQuickStop(action, entity: entity, pid: pid)
        } label: {
            AetowerSurface(
                level: action == .forceKill ? .critical : .warning,
                padding: AetowerDesign.Spacing.none,
                cornerRadius: AetowerDesign.Radius.sm
            ) {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: action.systemImage)
                        .font(AetowerDesign.Typography.compactData(size: 12, weight: .semibold))
                        .frame(width: AetowerDesign.Size.iconSlot)
                    Text(action.label)
                        .font(AetowerDesign.Typography.caption.weight(.semibold))
                        .lineLimit(1)
                    Spacer(minLength: AetowerDesign.Spacing.xs)
                }
                .foregroundStyle(action == .forceKill ? AetowerDesign.Status.error : AetowerDesign.Status.warning)
                .padding(.horizontal, AetowerDesign.Spacing.sm)
                .frame(maxWidth: .infinity, minHeight: AetowerDesign.Size.minTouchTarget, alignment: .leading)
            }
        }
        .buttonStyle(.plain)
        .disabled(!targetVisible || state.entityAnalysisIsLoading(processAnalysisKey(pid), kind: .processAction))
        .help(targetVisible ? "Validate the target, send \(action.label.lowercased()), and verify the result." : "Target PID is no longer visible.")
    }

    @ViewBuilder
    private func sidePanelBrowserTabs(_ components: [ComponentSnapshot]) -> some View {
        if !components.isEmpty {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Label("Browser tabs", systemImage: "globe")
                        .font(AetowerDesign.Typography.caption.weight(.semibold))
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                    AetowerBadge(
                        "\(components.count)",
                        tone: AetowerDesign.Tone.network,
                        style: .outline
                    )
                    Spacer()
                    Text("Chromium debug")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                }

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    ForEach(Array(components.prefix(4).enumerated()), id: \.offset) { _, component in
                        sidePanelBrowserTabRow(component)
                    }
                }

                if components.count > 4 {
                    Text("+\(components.count - 4) more in Components")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                }
            }
        }
    }

    private func sidePanelBrowserTabRow(_ component: ComponentSnapshot) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "safari")
                .font(AetowerDesign.Typography.compactData(size: 11, weight: .semibold))
                .foregroundStyle(AetowerDesign.Tone.network)
                .frame(width: 16, height: 16)

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text(sidePanelBrowserTabTitle(component.title))
                    .font(AetowerDesign.Typography.caption.weight(.medium))
                    .foregroundStyle(AetowerDesign.Ink.primary)
                    .lineLimit(1)

                if let url = component.adapterContext?.url, !url.isEmpty {
                    Text(url)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                }

                if let signal = sidePanelBrowserTabSignal(component) {
                    Text(signal)
                        .font(AetowerDesign.Typography.metadataStrong)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                        .lineLimit(1)
                }
            }

            Spacer(minLength: AetowerDesign.Spacing.xs)
        }
        .padding(.vertical, AetowerDesign.Spacing.xxs)
    }

    private func sidePanelBrowserTabComponents(for entity: EntitySnapshot) -> [ComponentSnapshot] {
        entity.components.filter { component in
            component.adapterContext?.kind == .chromiumTab
        }
    }

    private func sidePanelBrowserTabTitle(_ title: String) -> String {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "Untitled tab" }

        let separator = " · "
        if trimmed.hasPrefix("Tab "), let range = trimmed.range(of: separator) {
            let tabTitle = trimmed[range.upperBound...].trimmingCharacters(in: .whitespacesAndNewlines)
            return tabTitle.isEmpty ? "Untitled tab" : tabTitle
        }

        return trimmed
    }

    private func sidePanelBrowserTabSignal(_ component: ComponentSnapshot) -> String? {
        guard let adapterContext = component.adapterContext else { return nil }

        var parts: [String] = []
        if component.memoryBytes > 0 || adapterContext.jsHeapTotalBytes > 0 {
            let used = component.memoryBytes > 0 ? formatBytes(component.memoryBytes) : "unknown"
            if adapterContext.jsHeapTotalBytes > 0 {
                parts.append("heap \(used) / \(formatBytes(adapterContext.jsHeapTotalBytes))")
            } else {
                parts.append("heap \(used)")
            }
        }

        let networkBps = adapterContext.networkReceiveBps + adapterContext.networkSendBps
        if networkBps > 0 {
            parts.append("net \(formatRate(networkBps))")
        }

        if adapterContext.domNodes > 0 {
            parts.append("\(adapterContext.domNodes) DOM nodes")
        } else if adapterContext.documents > 0 || adapterContext.frames > 0 {
            parts.append("\(adapterContext.documents) docs · \(adapterContext.frames) frames")
        }

        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func sidePanelLiveProcessCount(_ entities: [EntitySnapshot]) -> Int {
        entities.reduce(0) { count, entity in
            count + liveProcessCount(for: entity)
        }
    }

    private func sidePanelEntityIconName(for entity: EntitySnapshot) -> String {
        switch entity.entityKind {
        case .app:
            return "app"
        case .browser:
            return "globe"
        case .daemon, .service:
            return "gearshape.2"
        case .terminalSession:
            return "terminal"
        case .aiAgent:
            return "sparkles"
        case .unknown:
            return "square.stack.3d.up"
        }
    }

    @ViewBuilder
    private func sidePanelQuickStopStatus(
        entity: EntitySnapshot,
        pid: UInt32,
        targetVisible: Bool
    ) -> some View {
        let submission = quickStopSubmissionMatching(entityID: entity.entityId, pid: pid)

        if !targetVisible, submission != nil {
            SidePanelQuickStatusLine(
                icon: "checkmark.seal.fill",
                color: AetowerDesign.Status.success,
                text: "\(processPIDLabel(pid)) is no longer visible in the current snapshot."
            )
        }

        if let submission {
            sidePanelQuickStopSubmissionStatus(submission, targetVisible: targetVisible)
        }

        if submission != nil,
           let error = state.entityAnalysisError(processAnalysisKey(pid), kind: .processAction) {
            SidePanelQuickStatusLine(
                icon: "exclamationmark.triangle.fill",
                color: AetowerDesign.Status.error,
                text: error
            )
        }
    }

    @ViewBuilder
    private func sidePanelQuickStopSubmissionStatus(
        _ submission: SidePanelQuickStopSubmission,
        targetVisible: Bool
    ) -> some View {
        if state.entityAnalysisIsLoading(processAnalysisKey(submission.pid), kind: .processAction) {
            SidePanelQuickStatusLine(
                icon: "paperplane.fill",
                color: AetowerDesign.Status.ready,
                text: "\(submission.action.label) \(shortActionID(submission.actionID)) started; Aetower is validating the target, sending the signal, and waiting for verification."
            )
        }

        if let report = state.processActionReports[submission.pid],
           report.actionId == submission.actionID {
            VStack(alignment: .leading, spacing: 6) {
                SidePanelQuickStatusLine(
                    icon: sidePanelQuickStopResultIcon(report),
                    color: sidePanelQuickStopResultColor(report),
                    text: "\(report.action) \(shortActionID(submission.actionID)): \(report.message)"
                )
                if let verification = sidePanelQuickStopEffectiveVerification(report) {
                    Text("Verification: \(sidePanelQuickStopVerificationLabel(verification))")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                if targetVisible,
                   sidePanelQuickStopVerificationIsConfirmed(sidePanelQuickStopEffectiveVerification(report)),
                   sidePanelQuickStopActionRemovesProcess(submission.action) {
                    SidePanelQuickStatusLine(
                        icon: "arrow.clockwise",
                        color: AetowerDesign.Status.ready,
                        text: "Monitor is refreshing; the current snapshot still includes \(processPIDLabel(submission.pid))."
                    )
                }
                sidePanelQuickStopFollowUpStatus(report)
                if let stderr = report.commandResult?.stderr, !stderr.isEmpty {
                    Text("macOS stderr: \(stderr)")
                        .font(.caption2)
                        .foregroundStyle(AetowerDesign.Status.warning)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Button("Clear quick-stop status") {
                    quickStopSubmission = nil
                }
                .buttonStyle(.borderless)
                .controlSize(.small)
            }
            .padding(.top, 2)
        }
    }

    @ViewBuilder
    private func sidePanelQuickStopFollowUpStatus(_ report: ProcessActionReportModel) -> some View {
        if let checks = report.followUpChecks, !checks.isEmpty {
            ForEach(checks) { check in
                Text("\(check.delayMillis / 1_000)s follow-up: \(sidePanelQuickStopVerificationLabel(check.verification))")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func runSidePanelQuickStop(
        _ action: ProcessActionKind,
        entity: EntitySnapshot,
        pid: UInt32
    ) {
        let actionID = UUID().uuidString
        quickStopSubmission = SidePanelQuickStopSubmission(
            entityID: entity.entityId,
            pid: pid,
            action: action,
            actionID: actionID,
            submittedAt: Date()
        )
        state.runVerifiedProcessAction(
            pid: pid,
            action: action,
            reason: "Quick stop from Monitor side panel.",
            actionID: actionID
        )
    }

    private func quickStopSubmissionMatching(entityID: String, pid: UInt32) -> SidePanelQuickStopSubmission? {
        guard let quickStopSubmission,
              quickStopSubmission.entityID == entityID,
              quickStopSubmission.pid == pid
        else {
            return nil
        }
        return quickStopSubmission
    }

    private func retainedQuickStopPID(for entityID: String) -> UInt32? {
        if let quickStopSubmission, quickStopSubmission.entityID == entityID {
            return quickStopSubmission.pid
        }
        return nil
    }

    private func shortActionID(_ actionID: String) -> String {
        "#\(actionID.suffix(8))"
    }

    private func processAnalysisKey(_ pid: UInt32) -> String {
        "pid:\(pid)"
    }

    private func sidePanelQuickStopResultIcon(_ report: ProcessActionReportModel) -> String {
        guard report.success else { return "xmark.octagon.fill" }
        return sidePanelQuickStopVerificationIsConfirmed(sidePanelQuickStopEffectiveVerification(report))
            ? "checkmark.circle.fill"
            : "exclamationmark.triangle.fill"
    }

    private func sidePanelQuickStopResultColor(_ report: ProcessActionReportModel) -> Color {
        guard report.success else { return AetowerDesign.Status.error }
        return sidePanelQuickStopVerificationIsConfirmed(sidePanelQuickStopEffectiveVerification(report))
            ? AetowerDesign.Status.success
            : AetowerDesign.Status.warning
    }

    private func sidePanelQuickStopEffectiveVerification(_ report: ProcessActionReportModel) -> String? {
        report.followUpChecks?.last?.verification ?? report.verification
    }

    private func sidePanelQuickStopVerificationIsConfirmed(_ verification: String?) -> Bool {
        switch verification {
        case "verified-exited", "verified-suspended", "verified-running",
            "verified-priority", "command-accepted", "preview":
            return true
        default:
            return false
        }
    }

    private func sidePanelQuickStopActionRemovesProcess(_ action: ProcessActionKind) -> Bool {
        switch action {
        case .terminate, .forceKill, .terminateTree, .forceKillTree:
            return true
        case .suspend, .resume, .lowerPriority, .normalPriority:
            return false
        }
    }

    private func sidePanelQuickStopVerificationLabel(_ verification: String) -> String {
        switch verification {
        case "verified-exited", "exited": return "Exited / no longer running"
        case "targets-still-visible", "still-visible": return "Still visible"
        case "command-failed": return "Command failed"
        case "preview": return "Preview only"
        case "target-visible": return "Target visible"
        case "target-not-visible": return "Target missing"
        default: return verification.replacingOccurrences(of: "-", with: " ")
        }
    }

    private var rankedEntitiesSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Text("")
                    .frame(width: 16)
                MonitorHeaderButton(
                    column: .name,
                    isActive: isActiveSortColumn(.name),
                    indicatorSymbol: sortIndicatorSymbol(for: .name),
                    action: { applySort(for: .name) }
                )
                MonitorHeaderButton(
                    column: .cpu,
                    isActive: isActiveSortColumn(.cpu),
                    indicatorSymbol: sortIndicatorSymbol(for: .cpu),
                    action: { applySort(for: .cpu) }
                )
                MonitorHeaderButton(
                    column: .memory,
                    isActive: isActiveSortColumn(.memory),
                    indicatorSymbol: sortIndicatorSymbol(for: .memory),
                    action: { applySort(for: .memory) }
                )
                MonitorHeaderButton(
                    column: .wakeups,
                    isActive: isActiveSortColumn(.wakeups),
                    indicatorSymbol: sortIndicatorSymbol(for: .wakeups),
                    action: { applySort(for: .wakeups) }
                )
                MonitorHeaderButton(
                    column: .processCount,
                    isActive: isActiveSortColumn(.processCount),
                    indicatorSymbol: sortIndicatorSymbol(for: .processCount),
                    action: { applySort(for: .processCount) }
                )
                MonitorHeaderButton(
                    column: .friction,
                    isActive: isActiveSortColumn(.friction),
                    indicatorSymbol: sortIndicatorSymbol(for: .friction),
                    action: { applySort(for: .friction) }
                )
                Text("")
                    .frame(width: 16)
            }
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(.tertiary)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, 2)

            if filteredEntities.isEmpty {
                ContentUnavailableView(
                    "No apps match this filter",
                    systemImage: "magnifyingglass",
                    description: Text("Try a broader query.")
                )
            } else {
                LazyVStack(spacing: 2) {
                    if !isGroupedMode && !burdenLeaderRows.isEmpty {
                        listSectionHeader("Burden leaders")
                        ForEach(burdenLeaderRows) { row in
                            Button {
                                selectEntity(row.id)
                            } label: {
                                EntityRow(
                                    row: row,
                                    isSelected: selectedEntityID == row.id
                                )
                                .equatable()
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                monitorContextMenu(for: row.entity, members: [row.entity])
                            }
                        }
                    }

                    listSectionHeader("All processes")
                    if isGroupedMode {
                        if allProcessGroupRows.isEmpty && isGrouping {
                            HStack(spacing: 8) {
                                ProgressView()
                                    .controlSize(.small)
                                Text("Grouping processes…")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, AetowerDesign.Spacing.sm)
                            .padding(.vertical, AetowerDesign.Spacing.xs)
                        }
                        ForEach(allProcessGroupRows) { row in
                            let isExpanded = expandedGroupIDs.contains(row.id)
                            GroupedEntityRow(
                                row: row,
                                isSelected: selectedEntityID == row.id,
                                isExpanded: isExpanded,
                                canExpand: canExpandGroup(row.group),
                                onToggleExpansion: {
                                    toggleGroupExpansion(row.group)
                                }
                            )
                            .equatable()
                            .contentShape(Rectangle())
                            .onTapGesture {
                                selectEntity(row.id)
                            }
                            .contextMenu {
                                if canExpandGroup(row.group) {
                                    Button(isExpanded ? "Collapse Group" : "Expand Group") {
                                        toggleGroupExpansion(row.group)
                                    }
                                    Divider()
                                }
                                monitorContextMenu(for: row.group.root, members: row.group.members)
                            }

                            if isExpanded {
                                ForEach(expandedProcessRows(for: row.group)) { processRow in
                                    Button {
                                        selectProcessRow(processRow)
                                    } label: {
                                        MonitorProcessRow(
                                            row: processRow,
                                            isSelected: selectedProcessRowID == processRow.id
                                        )
                                        .equatable()
                                    }
                                    .buttonStyle(.plain)
                                    .padding(.leading, AetowerDesign.Spacing.lg)
                                    .transition(.opacity.combined(with: .move(edge: .top)))
                                    .contextMenu {
                                        processComponentContextMenu(for: processRow)
                                    }
                                }
                            }
                        }
                    } else {
                        ForEach(allProcessRows) { row in
                            Button {
                                selectEntity(row.id)
                            } label: {
                                EntityRow(
                                    row: row,
                                    isSelected: selectedEntityID == row.id
                                )
                                .equatable()
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                monitorContextMenu(for: row.entity, members: [row.entity])
                            }
                        }
                    }
                }
                .animation(AetowerDesign.Motion.standard, value: sortKey)
                .animation(AetowerDesign.Motion.standard, value: listMode)
                .animation(nil, value: state.snapshotSequence)
            }
        }
    }

    @ViewBuilder
    private func monitorContextMenu(for entity: EntitySnapshot, members: [EntitySnapshot]) -> some View {
        Button("Open Detail") {
            selectEntity(entity.entityId)
        }

        if let pid = primaryProcessID(in: members) {
            Divider()
            Button("Inspect \(processPIDLabel(pid))") {
                requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .inspect)
            }
            Button("Open files & sockets") {
                requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .resources)
            }
            Button("Run 3s sample") {
                requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .sample)
            }
            Divider()
            Button("Terminate \(processPIDLabel(pid))…", role: .destructive) {
                requestProcessOperation(
                    entityID: entity.entityId,
                    pid: pid,
                    operation: .previewAction(.terminate)
                )
            }
            Button("Force kill \(processPIDLabel(pid))…", role: .destructive) {
                requestProcessOperation(
                    entityID: entity.entityId,
                    pid: pid,
                    operation: .previewAction(.forceKill)
                )
            }
            Menu("Actions") {
                ForEach(contextPreviewActions) { action in
                    Button(action.label, role: action.isDestructive ? .destructive : nil) {
                        requestProcessOperation(
                            entityID: entity.entityId,
                            pid: pid,
                            operation: .previewAction(action)
                        )
                    }
                }
            }
        }

        Divider()
        Button("Copy Process IDs") {
            let pids = processIDs(in: members)
                .map(String.init)
                .joined(separator: ", ")
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(pids, forType: .string)
        }
        Button("Copy Name") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(entity.displayName, forType: .string)
        }
        if entity.executablePath?.contains(".app/") == true {
            Button("Show in Finder") {
                if let path = entity.executablePath {
                    NSWorkspace.shared.selectFile(path, inFileViewerRootedAtPath: "")
                }
            }
        }
    }

    @ViewBuilder
    private func processComponentContextMenu(for row: MonitorProcessRowModel) -> some View {
        Button("Open Detail") {
            selectProcessRow(row)
        }

        Divider()
        Button("Inspect \(processPIDLabel(row.pid))") {
            requestProcessOperation(entityID: row.ownerEntityID, pid: row.pid, operation: .inspect)
            selectedProcessRowID = row.id
        }
        Button("Open files & sockets") {
            requestProcessOperation(entityID: row.ownerEntityID, pid: row.pid, operation: .resources)
            selectedProcessRowID = row.id
        }
        Button("Run 3s sample") {
            requestProcessOperation(entityID: row.ownerEntityID, pid: row.pid, operation: .sample)
            selectedProcessRowID = row.id
        }

        Divider()
        Button("Copy Process ID") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(String(row.pid), forType: .string)
        }
        Button("Copy Name") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(row.displayName, forType: .string)
        }
        if let commandLine = row.commandLine, !commandLine.isEmpty {
            Button("Copy Command") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(commandLine, forType: .string)
            }
        }
    }

    private var contextPreviewActions: [ProcessActionKind] {
        [.suspend, .resume, .lowerPriority, .normalPriority, .terminate, .forceKill, .terminateTree, .forceKillTree]
    }

    private func selectEntity(_ entityID: String) {
        searchFieldFocused = false
        withAnimation(AetowerDesign.Motion.standard) {
            selectedEntityID = entityID
            selectedProcessRowID = nil
        }
    }

    private func selectProcessRow(_ row: MonitorProcessRowModel) {
        searchFieldFocused = false
        withAnimation(AetowerDesign.Motion.standard) {
            selectedEntityID = row.ownerEntityID
            selectedProcessRowID = row.id
        }
    }

    private func requestProcessOperation(
        entityID: String,
        pid: UInt32,
        operation: ProcessOperatorQuickOperation
    ) {
        processOperatorRequest = ProcessOperatorRequest(pid: pid, operation: operation)
        selectEntity(entityID)
    }

    private func primaryProcessID(in members: [EntitySnapshot]) -> UInt32? {
        let lineageCandidates = members
            .flatMap(\.processLineage)
            .map { node in
                (
                    pid: node.pid,
                    cpu: node.cpuPercent,
                    memory: max(node.memoryPhysicalFootprintBytes, node.memoryBytes)
                )
            }
        if let pid = primaryProcessID(from: lineageCandidates) {
            return pid
        }

        let componentCandidates = members
            .flatMap(\.components)
            .filter { $0.kind != .adapterContext }
            .compactMap { component -> (pid: UInt32, cpu: Float, memory: UInt64)? in
                guard let pid = component.processId else { return nil }
                return (pid, component.cpuPercent, max(component.memoryPhysicalFootprintBytes, component.memoryBytes))
            }
        return primaryProcessID(from: componentCandidates)
    }

    private func primaryProcessID(from candidates: [(pid: UInt32, cpu: Float, memory: UInt64)]) -> UInt32? {
        candidates
            .sorted {
                if $0.cpu != $1.cpu {
                    return $0.cpu > $1.cpu
                }
                return $0.memory > $1.memory
            }
            .first?
            .pid
    }

    private func processIDs(in members: [EntitySnapshot]) -> [UInt32] {
        var seen = Set<UInt32>()
        return members.flatMap { entity -> [UInt32] in
            let lineagePIDs = entity.processLineage.map(\.pid)
            let pids = lineagePIDs.isEmpty
                ? entity.components.compactMap(\.processId)
                : lineagePIDs
            return pids.filter { seen.insert($0).inserted }
        }
    }

    @ViewBuilder
    private var thermalForecastBanner: some View {
        if let forecast = state.thermalForecastState {
            let tone =
                forecast.state == .nominal || forecast.state == .fair
                ? AetowerDesign.Status.warning : AetowerDesign.Status.error
            HStack(spacing: 8) {
                Image(systemName: "thermometer.sun.fill")
                    .foregroundStyle(tone)
                Text(thermalBannerText(forecast))
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(tone)
                Spacer()
                Text("Sensors tab for the temperature trend")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .background(tone.opacity(0.12), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
            .padding(.horizontal, AetowerDesign.Spacing.sm)
        }
    }

    private func thermalBannerText(_ forecast: ThermalForecast) -> String {
        let lead =
            forecast.minutesToThrottle.map { String(format: "Throttle likely in ~%.0f min", $0) }
            ?? "Throttling now"
        if let label = forecast.topContributorLabel {
            return "\(lead) · top contributor: \(label)"
        }
        return lead
    }

    private var monitorOverviewSummary: some View {
        let compactCards = monitorMetricCardDescriptors(from: state.monitorViewModel)
        if !compactCards.isEmpty {
            return monitorMetricCardsLayout(compactCards)
        }

        let host = state.hostState
        let frictionScore = machineFrictionScore(for: host)
        let frictionSamples = monitorFallbackTrendSamples(frictionScore)
        let cpuSamples = monitorFallbackTrendSamples(Double(host.cpuPercent))
        let memorySamples = monitorFallbackTrendSamples(hostMemoryPressureScore(host))
        let diskSamples = monitorFallbackTrendSamples(Double(host.diskReadBps + host.diskWriteBps))
        let networkSamples = monitorFallbackTrendSamples(Double(host.networkReceiveBps + host.networkSendBps))
        let wakeupSamples = monitorFallbackTrendSamples(Double(host.wakeupsPerSecond))
        let gpuPercentSamples = monitorFallbackTrendSamples(Double(host.gpuPercent))
        let gpuMemorySamples = monitorFallbackTrendSamples(Double(host.gpuMemoryBytes))

        let usingGpuPercent = host.gpuPercent > 0 || host.gpuMemoryBytes == 0
        let gpuSamples = usingGpuPercent ? gpuPercentSamples : gpuMemorySamples
        let gpuValue = usingGpuPercent
            ? String(format: "%.0f%%", host.gpuPercent)
            : formatBytes(host.gpuMemoryBytes)
        let memoryPressure = hostMemoryPressureScore(host)

        let cards = [
            MonitorMetricCardDescriptor(
                id: .friction,
                title: "Friction",
                value: String(format: "%.0f", frictionScore),
                fixedScaleValue: String(format: "%.0f%%", frictionScore),
                subtitle: "overall machine score · \(trendWindowLabel(sampleCount: frictionSamples.count))",
                samples: frictionSamples,
                style: .friction,
                valueAppearance: monitorFrictionAppearance(frictionScore),
                sampleValueFormatter: { String(format: "%.0f", $0) },
                fixedScaleSampleValueFormatter: { String(format: "%.0f%%", $0) },
                fixedCeiling: MonitorRingCeiling.percent
            ),
            MonitorMetricCardDescriptor(
                id: .cpu,
                title: "CPU",
                value: String(format: "%.0f%%", host.cpuPercent),
                fixedScaleValue: nil,
                subtitle: "thermal \(thermalStateLabel(host.thermalState)) · \(trendWindowLabel(sampleCount: cpuSamples.count))",
                samples: cpuSamples,
                style: .cpu,
                valueAppearance: monitorCPUAppearance(host),
                sampleValueFormatter: { String(format: "%.0f%%", $0) },
                fixedScaleSampleValueFormatter: nil,
                fixedCeiling: MonitorRingCeiling.percent
            ),
            MonitorMetricCardDescriptor(
                id: .memory,
                title: "Memory",
                value: formatBytes(host.memoryUsedBytes),
                fixedScaleValue: String(format: "%.0f%%", memoryPressure),
                subtitle: "\(formatBytes(host.memoryUsedBytes)) used · \(formatBytes(host.compressedMemoryBytes)) compressed · \(formatBytes(host.swapUsedBytes)) swap · pressure trend",
                samples: memorySamples,
                style: .memory,
                valueAppearance: monitorMemoryAppearance(host),
                // Samples are the 0–100 memory pressure score, not bytes — label
                // the hover stats with /100 so they don't read as bytes/percent.
                sampleValueFormatter: { String(format: "%.0f/100", $0) },
                fixedScaleSampleValueFormatter: { String(format: "%.0f%%", $0) },
                fixedCeiling: MonitorRingCeiling.percent
            ),
            MonitorMetricCardDescriptor(
                id: .disk,
                title: "Disk",
                value: formatRate(host.diskReadBps + host.diskWriteBps),
                fixedScaleValue: nil,
                subtitle: "read + write · \(trendWindowLabel(sampleCount: diskSamples.count))",
                samples: diskSamples,
                style: .disk,
                valueAppearance: monitorThroughputAppearance(host.diskReadBps + host.diskWriteBps, warning: 50 * 1_024 * 1_024, danger: 250 * 1_024 * 1_024),
                sampleValueFormatter: { formatRate(UInt64(max($0, 0))) },
                fixedScaleSampleValueFormatter: nil,
                fixedCeiling: MonitorRingCeiling.diskBps
            ),
            MonitorMetricCardDescriptor(
                id: .network,
                title: "Network",
                value: formatRate(host.networkReceiveBps + host.networkSendBps),
                fixedScaleValue: nil,
                subtitle: "send + receive · \(trendWindowLabel(sampleCount: networkSamples.count))",
                samples: networkSamples,
                style: .network,
                valueAppearance: monitorThroughputAppearance(host.networkReceiveBps + host.networkSendBps, warning: 10 * 1_024 * 1_024, danger: 100 * 1_024 * 1_024),
                sampleValueFormatter: { formatRate(UInt64(max($0, 0))) },
                fixedScaleSampleValueFormatter: nil,
                fixedCeiling: MonitorRingCeiling.networkBps
            ),
            MonitorMetricCardDescriptor(
                id: .wakeups,
                title: "Wakeups",
                value: formatWakeups(host.wakeupsPerSecond),
                fixedScaleValue: nil,
                subtitle: hostWakeupBand(host.wakeupsPerSecond) == .severe
                    ? "wakeup storm band · \(trendWindowLabel(sampleCount: wakeupSamples.count))"
                    : "current host wakeup rate · \(trendWindowLabel(sampleCount: wakeupSamples.count))",
                samples: wakeupSamples,
                style: .friction,
                valueAppearance: monitorWakeupAppearance(host),
                sampleValueFormatter: { formatWakeups(Float($0)) },
                fixedScaleSampleValueFormatter: nil,
                fixedCeiling: MonitorRingCeiling.wakeupsPerSecond
            ),
            MonitorMetricCardDescriptor(
                id: .gpu,
                title: "GPU",
                value: gpuValue,
                fixedScaleValue: usingGpuPercent ? String(format: "%.0f%%", host.gpuPercent) : nil,
                subtitle: "\(hostGPUSummary(host)) · \(trendWindowLabel(sampleCount: gpuSamples.count))",
                samples: gpuSamples,
                style: .energy,
                valueAppearance: monitorGPUAppearance(host),
                sampleValueFormatter: usingGpuPercent
                    ? { String(format: "%.0f%%", $0) }
                    : { formatBytes(UInt64(max($0, 0))) },
                fixedScaleSampleValueFormatter: usingGpuPercent
                    ? { String(format: "%.0f%%", $0) }
                    : nil,
                // GPU % has a true 0–100 axis; the GPU-memory fallback has no
                // fixed ceiling, so 0 keeps that case on relative scaling.
                fixedCeiling: usingGpuPercent ? MonitorRingCeiling.percent : 0
            ),
        ]

        return monitorMetricCardsLayout(cards)
    }

    private func monitorMetricCardDescriptors(
        from viewModel: MonitorViewModel
    ) -> [MonitorMetricCardDescriptor] {
        let order: [MonitorMetricCardFocus] = [.friction, .cpu, .memory, .disk, .network, .wakeups, .gpu]
        let orderIndex = Dictionary(uniqueKeysWithValues: order.enumerated().map { ($0.element, $0.offset) })
        return viewModel.metricCards
            .compactMap(monitorMetricCardDescriptor(from:))
            .sorted {
                (orderIndex[$0.id] ?? Int.max) < (orderIndex[$1.id] ?? Int.max)
            }
    }

    private func monitorMetricCardDescriptor(
        from card: UiMetricCard
    ) -> MonitorMetricCardDescriptor? {
        guard let focus = MonitorMetricCardFocus(rawValue: card.id), focus != .all else {
            return nil
        }

        let samples = card.samples.isEmpty ? [card.value] : card.samples
        return MonitorMetricCardDescriptor(
            id: focus,
            title: card.title,
            value: card.displayValue,
            fixedScaleValue: monitorMetricFixedScaleValue(for: card, focus: focus),
            subtitle: "\(card.detail) · \(trendWindowLabel(sampleCount: samples.count))",
            samples: samples,
            style: monitorMetricStyle(for: focus),
            valueAppearance: monitorMetricValueAppearance(for: card.severity),
            sampleValueFormatter: monitorMetricSampleFormatter(for: card, focus: focus),
            fixedScaleSampleValueFormatter: monitorMetricFixedScaleSampleFormatter(for: card, focus: focus),
            fixedCeiling: monitorMetricFixedCeiling(for: card, focus: focus)
        )
    }

    private func monitorMetricStyle(for focus: MonitorMetricCardFocus) -> TrendMetricStyle {
        switch focus {
        case .friction:
            return .friction
        case .cpu:
            return .cpu
        case .memory:
            return .memory
        case .disk:
            return .disk
        case .network:
            return .network
        case .wakeups:
            return .friction
        case .gpu:
            return .energy
        case .all:
            return .friction
        }
    }

    private func monitorMetricValueAppearance(for severity: UiMetricSeverity) -> TrendMetricValueAppearance {
        switch severity {
        case .normal:
            return .ok
        case .warning:
            return .warning
        case .critical:
            return .danger
        }
    }

    private func monitorMetricFixedScaleValue(
        for card: UiMetricCard,
        focus: MonitorMetricCardFocus
    ) -> String? {
        switch card.unit {
        case "percent":
            return String(format: "%.0f%%", card.value)
        case "score":
            return String(format: "%.0f%%", card.value)
        case "bytes":
            guard focus == .memory, let ceiling = card.fixedCeiling, ceiling > 0 else { return nil }
            return String(format: "%.0f%%", monitorMetricPercent(card.value, ceiling: ceiling))
        default:
            return nil
        }
    }

    private func monitorMetricSampleFormatter(
        for card: UiMetricCard,
        focus: MonitorMetricCardFocus
    ) -> (Double) -> String {
        switch card.unit {
        case "percent":
            return { String(format: "%.0f%%", $0) }
        case "score":
            return { String(format: "%.0f", $0) }
        case "bytes":
            return { formatBytes(UInt64(max($0, 0))) }
        case "bytes_per_second":
            return { formatRate(UInt64(max($0, 0))) }
        case "wakeups_per_second":
            return { formatWakeups(Float($0)) }
        default:
            switch focus {
            case .disk, .network:
                return { formatRate(UInt64(max($0, 0))) }
            case .wakeups:
                return { formatWakeups(Float($0)) }
            default:
                return { String(format: "%.0f", $0) }
            }
        }
    }

    private func monitorMetricFixedScaleSampleFormatter(
        for card: UiMetricCard,
        focus: MonitorMetricCardFocus
    ) -> ((Double) -> String)? {
        switch card.unit {
        case "percent":
            return { String(format: "%.0f%%", $0) }
        case "score":
            return { String(format: "%.0f%%", $0) }
        case "bytes":
            guard focus == .memory, let ceiling = card.fixedCeiling, ceiling > 0 else { return nil }
            return { String(format: "%.0f%%", monitorMetricPercent($0, ceiling: ceiling)) }
        default:
            return nil
        }
    }

    private func monitorMetricFixedCeiling(
        for card: UiMetricCard,
        focus: MonitorMetricCardFocus
    ) -> Double {
        if let fixedCeiling = card.fixedCeiling, fixedCeiling > 0 {
            return fixedCeiling
        }

        switch focus {
        case .friction, .cpu, .memory, .gpu:
            return MonitorRingCeiling.percent
        case .disk:
            return MonitorRingCeiling.diskBps
        case .network:
            return MonitorRingCeiling.networkBps
        case .wakeups:
            return MonitorRingCeiling.wakeupsPerSecond
        case .all:
            return 0
        }
    }

    private func monitorMetricPercent(_ value: Double, ceiling: Double) -> Double {
        guard ceiling > 0 else { return 0 }
        return min(100, max(0, (value / ceiling) * 100))
    }

    @ViewBuilder
    private func monitorMetricCardsLayout(_ cards: [MonitorMetricCardDescriptor]) -> some View {
        let visibleCards = visibleMonitorMetricCards(cards)
        switch settings.monitorMetricCardPlacement {
        case .left, .right:
            LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(visibleCards) { card in
                    monitorMetricCard(card, minHeight: 106)
                }
            }
        case .top, .bottom:
            if settings.monitorMetricCardFocus != .all, visibleCards.count == 1, let card = visibleCards.first {
                monitorMetricCard(card, minHeight: 168, allowsPinning: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                LazyVGrid(columns: monitorSummaryGridColumns, alignment: .leading, spacing: 12) {
                    ForEach(visibleCards) { card in
                        monitorMetricCard(card, minHeight: 124, allowsPinning: true)
                    }
                }
            }
        }
    }

    private func monitorMetricCard(
        _ card: MonitorMetricCardDescriptor,
        minHeight: CGFloat,
        allowsPinning: Bool = false
    ) -> some View {
        let fixedScaling = settings.metricRingsFixedScaling
        let fixedCeiling = fixedScaling && card.fixedCeiling > 0 ? card.fixedCeiling : nil
        return TrendMetricCard(
            title: card.title,
            value: monitorMetricValue(card, fixedScaling: fixedScaling),
            subtitle: monitorMetricSubtitle(card, fixedScaling: fixedScaling),
            samples: card.samples,
            style: card.style,
            valueAppearance: card.valueAppearance,
            sampleValueFormatter: monitorMetricSampleFormatter(card, fixedScaling: fixedScaling),
            minHeight: minHeight,
            fixedCeiling: fixedCeiling
        )
        .id("\(card.id.rawValue)-scale-\(fixedCeiling ?? -1)")
        .overlay(alignment: .topTrailing) {
            if allowsPinning {
                monitorMetricPinButton(card)
            }
        }
    }

    private func monitorMetricValue(
        _ card: MonitorMetricCardDescriptor,
        fixedScaling: Bool
    ) -> String {
        if fixedScaling, let fixedScaleValue = card.fixedScaleValue {
            return fixedScaleValue
        }
        return card.value
    }

    private func monitorMetricSampleFormatter(
        _ card: MonitorMetricCardDescriptor,
        fixedScaling: Bool
    ) -> (Double) -> String {
        if fixedScaling, let fixedScaleSampleValueFormatter = card.fixedScaleSampleValueFormatter {
            return fixedScaleSampleValueFormatter
        }
        return card.sampleValueFormatter
    }

    private func monitorMetricSubtitle(
        _ card: MonitorMetricCardDescriptor,
        fixedScaling: Bool
    ) -> String {
        guard fixedScaling, card.fixedCeiling > 0 else { return card.subtitle }
        return "\(card.subtitle) · \(monitorMetricScaleLabel(card))"
    }

    private func monitorMetricScaleLabel(_ card: MonitorMetricCardDescriptor) -> String {
        switch card.id {
        case .friction, .cpu, .memory, .gpu:
            return "fixed 0–100"
        case .disk:
            return "fixed 0–\(formatRate(UInt64(card.fixedCeiling)))"
        case .network:
            return "fixed 0–\(formatRate(UInt64(card.fixedCeiling)))"
        case .wakeups:
            return "fixed 0–\(formatWakeups(Float(card.fixedCeiling)))"
        case .all:
            return "fixed scale"
        }
    }

    private func monitorMetricPinButton(_ card: MonitorMetricCardDescriptor) -> some View {
        let isFocused = settings.monitorMetricCardFocus == card.id
        return Button {
            withAnimation(AetowerDesign.Motion.standard) {
                settings.monitorMetricCardFocus = isFocused ? .all : card.id
            }
        } label: {
            Image(systemName: isFocused ? "pin.slash.fill" : "pin.fill")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(isFocused ? Color.accentColor : Color.secondary)
                .frame(width: 22, height: 22)
                .background(.regularMaterial, in: Circle())
                .overlay(
                    Circle()
                        .stroke(Color.secondary.opacity(0.18), lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
        .padding(7)
        .help(isFocused ? "Unpin \(card.title) and show all rings" : "Pin \(card.title) as the full-width ring")
        .accessibilityLabel(isFocused ? "Unpin \(card.title)" : "Pin \(card.title)")
    }

    private func visibleMonitorMetricCards(_ cards: [MonitorMetricCardDescriptor]) -> [MonitorMetricCardDescriptor] {
        guard settings.monitorMetricCardPlacement.supportsFocusedMetric else { return cards }
        let focus = settings.monitorMetricCardFocus
        guard focus != .all else { return cards }
        let focused = cards.filter { $0.id == focus }
        return focused.isEmpty ? cards : focused
    }

    private var advancedFilterButton: some View {
        Button {
            showAdvancedFilter.toggle()
        } label: {
            Image(systemName: state.advancedFilterEntityIds == nil
                ? "line.3.horizontal.decrease.circle"
                : "line.3.horizontal.decrease.circle.fill")
                .font(.system(size: 12))
                .foregroundStyle(state.advancedFilterEntityIds == nil ? Color.secondary : Color.accentColor)
        }
        .buttonStyle(.plain)
        .help("Advanced filter (Rhai expression)")
        .popover(isPresented: $showAdvancedFilter, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 10) {
                Text("Advanced filter")
                    .font(.headline)
                Text("Boolean expression evaluated per process. Fields: name, path, cmd, user, cwd, entity, bundle, badges, pid, cpu, mem, mem_mb, threads, energy, friction.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                TextField("cpu > 20 && name.contains(\"Helper\")", text: $advancedFilterText, axis: .vertical)
                    .textFieldStyle(.roundedBorder)
                    .lineLimit(2...4)
                    .font(.system(size: 11, design: .monospaced))
                    .aetowerUtilityTextInput()
                if let error = state.advancedFilterError {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                } else if let summary = state.advancedFilterSummary {
                    Text(summary)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                HStack {
                    Button("Apply") {
                        state.applyAdvancedFilter(advancedFilterText)
                    }
                    .keyboardShortcut(.return, modifiers: [])
                    Button("Clear") {
                        advancedFilterText = ""
                        state.clearAdvancedFilter()
                    }
                    .disabled(state.advancedFilterEntityIds == nil && advancedFilterText.isEmpty)
                    Spacer()
                }
            }
            .padding(14)
            .frame(width: 320)
        }
    }

    private var filteredEntities: [EntitySnapshot] {
        monitorSections.filteredEntities
    }

    private var burdenLeaderRows: [MonitorEntityRowModel] {
        monitorSections.burdenLeaderRows
    }

    private var allProcessRows: [MonitorEntityRowModel] {
        monitorSections.allProcessRows
    }

    private var allProcessGroupRows: [MonitorGroupRowModel] {
        allProcessGroupSection.rows
    }

    private var allProcessGroupSection: MonitorGroupRowSection {
        guard let groupingKey = currentGroupingCacheKey else { return .empty }
        let groups = groupedEntities
        let key = MonitorGroupRowCacheKey(
            groupingKey: groupingKey,
            burdenLeaderSignature: burdenLeaderSignature,
            groupEntityIDs: groups.map(\.id)
        )
        return monitorGroupRowCacheStore.section(
            for: key,
            groups: groups,
            originCache: processOriginCache,
            burdenLeaderSummariesByEntityID: monitorSections.burdenLeaderSummariesByEntityID
        )
    }

    private var burdenLeaderSignature: [String] {
        monitorSections.burdenLeaderSummariesByEntityID
            .flatMap { entityID, summaries in
                summaries.map { summary in
                    "\(entityID):\(summary.id):\(summary.metricValue):\(summary.severity.rawValue)"
                }
            }
            .sorted()
    }

    private var monitorVisibleRowCount: Int {
        if isGroupedMode {
            return allProcessGroupRows.count + visibleExpandedProcessRowCount
        }
        return burdenLeaderRows.count + allProcessRows.count
    }

    private var visiblePrimaryRowCount: Int {
        if isGroupedMode {
            return allProcessGroupRows.count
        }
        return visibleEntityIDs.count
    }

    private var monitorVisibleRowBuildMillis: Double {
        monitorSections.rowBuildDurationMillis
            + (isGroupedMode ? allProcessGroupSection.rowBuildDurationMillis : 0)
    }

    private var monitorUiPerformanceBudgetToken: String {
        let durationBucket = Int((monitorVisibleRowBuildMillis * 10).rounded())
        return [
            "\(state.snapshotSequence)",
            listMode.rawValue,
            "\(monitorVisibleRowCount)",
            "\(durationBucket)",
            normalizedSearchQuery,
            originFilter.rawValue,
            sortKey.rawValue,
            advancedFilterSignature,
        ].joined(separator: "|")
    }

    private func applySort(for column: MonitorSortColumn) {
        switch column {
        case .name:
            sortKey = sortKey == .alphabeticalAsc ? .alphabeticalDesc : .alphabeticalAsc
        case .cpu:
            sortKey = .cpu
        case .memory:
            sortKey = .memory
        case .wakeups:
            sortKey = .wakeups
        case .processCount:
            sortKey = .processCount
        case .friction:
            sortKey = .friction
        }
    }

    private func isActiveSortColumn(_ column: MonitorSortColumn) -> Bool {
        switch column {
        case .name:
            return sortKey == .alphabeticalAsc || sortKey == .alphabeticalDesc
        case .cpu:
            return sortKey == .cpu
        case .memory:
            return sortKey == .memory
        case .wakeups:
            return sortKey == .wakeups
        case .processCount:
            return sortKey == .processCount
        case .friction:
            return sortKey == .friction
        }
    }

    private func sortIndicatorSymbol(for column: MonitorSortColumn) -> String? {
        guard isActiveSortColumn(column) else { return nil }
        switch column {
        case .name:
            return sortKey == .alphabeticalAsc ? "arrow.up" : "arrow.down"
        case .cpu, .memory, .wakeups, .processCount, .friction:
            return "arrow.down"
        }
    }

    // Resolves from the full snapshot so the detail pane persists even
    // when the entity is filtered out by search. Eliminates one
    // redundant filteredEntities evaluation per render cycle.
    private var selectedEntity: EntitySnapshot? {
        guard let selectedEntityID else { return nil }
        return state.entitiesState.first { $0.entityId == selectedEntityID }
    }

    private var selectedEntityGroup: EntityGroup? {
        guard let selectedEntityID, isGroupedMode else { return nil }
        return groupedEntities.first(where: { $0.root.entityId == selectedEntityID })
    }

    private var visibleEntityIDs: [String] {
        let sections = monitorSections
        if isGroupedMode {
            return visibleGroupedEntityIDs
        }
        return sections.flatVisibleEntityIDs
    }

    private var visibleGroupedEntityIDs: [String] {
        uniqueEntityIDsPreservingOrder(allProcessGroupRows.flatMap { row in
            var ids = [row.id]
            if expandedGroupIDs.contains(row.id) {
                ids.append(contentsOf: expandedProcessComponents(for: row.group, by: sortKey).map(\.owner.entityId))
            }
            return ids
        })
    }

    private var visibleExpandedProcessRowCount: Int {
        allProcessGroupRows.reduce(0) { total, row in
            guard expandedGroupIDs.contains(row.id) else { return total }
            return total + expandedProcessComponents(for: row.group, by: sortKey).count
        }
    }

    private var visibleProcessCount: Int {
        let sections = monitorSections
        if isGroupedMode {
            return allProcessGroupRows.reduce(0) {
                $0 + $1.group.processCount
            }
        }
        return sections.flatVisibleProcessCount
    }

    private var groupedEntities: [EntityGroup] {
        guard let key = currentGroupingCacheKey else { return [] }
        if let exact = groupedEntitiesCache[key] {
            return exact
        }
        return latestCompatibleGroupedEntities(for: key)
    }

    private func latestCompatibleGroupedEntities(for key: GroupingCacheKey) -> [EntityGroup] {
        groupedEntitiesCache
            .filter { cachedKey, _ in
                cachedKey.query == key.query
                    && cachedKey.originFilter == key.originFilter
                    && cachedKey.sortKey == key.sortKey
                    && cachedKey.filterSignature == key.filterSignature
            }
            .max { left, right in
                left.key.sequence < right.key.sequence
            }
            .map(\.value) ?? []
    }

    private var monitorSections: MonitorEntitySections {
        guard let key = currentMonitorSectionCacheKey else { return .empty }
        return monitorSectionCacheStore.sections(
            for: key,
            entities: state.entitiesState,
            host: state.hostState,
            originCache: processOriginCache,
            advancedFilterEntityIds: state.advancedFilterEntityIds
        )
    }

    private func selectedProcessTreeEntities(for entity: EntitySnapshot) -> [EntitySnapshot] {
        if let selectedEntityGroup, selectedEntityGroup.root.entityId == entity.entityId {
            return sortEntities(selectedEntityGroup.members, by: sortKey)
        }
        return [entity]
    }

    private func canExpandGroup(_ group: EntityGroup) -> Bool {
        expandedProcessComponents(for: group, by: sortKey).count > 1
    }

    private func expandedProcessRows(for group: EntityGroup) -> [MonitorProcessRowModel] {
        expandedProcessComponents(for: group, by: sortKey).map(MonitorProcessRowModel.init)
    }

    private func toggleGroupExpansion(_ group: EntityGroup) {
        let processRefs = expandedProcessComponents(for: group, by: sortKey)
        let childIDs = Set(processRefs.map(\.owner.entityId))
        let processRowIDs = Set(processRefs.map(\.id))
        withAnimation(AetowerDesign.Motion.standard) {
            if expandedGroupIDs.contains(group.id) {
                expandedGroupIDs.remove(group.id)
                if let selectedProcessRowID, processRowIDs.contains(selectedProcessRowID) {
                    self.selectedEntityID = group.id
                    self.selectedProcessRowID = nil
                } else if let selectedEntityID, childIDs.contains(selectedEntityID) {
                    self.selectedEntityID = group.id
                }
            } else {
                expandedGroupIDs.insert(group.id)
            }
        }
    }

    private var normalizedSearchQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var currentGroupingCacheKey: GroupingCacheKey? {
        guard isGroupedMode else { return nil }
        return GroupingCacheKey(
            sequence: state.snapshotSequence,
            query: normalizedSearchQuery,
            originFilter: originFilter,
            sortKey: sortKey,
            filterSignature: advancedFilterSignature
        )
    }

    private var currentMonitorSectionCacheKey: MonitorSectionCacheKey? {
        MonitorSectionCacheKey(
            sequence: state.snapshotSequence,
            query: normalizedSearchQuery,
            originFilter: originFilter,
            sortKey: sortKey,
            filterSignature: advancedFilterSignature
        )
    }

    /// Stable signature of the active advanced filter so the grouping cache
    /// invalidates when the filter set changes ("" = no filter).
    private var advancedFilterSignature: String {
        guard let ids = state.advancedFilterEntityIds else { return "" }
        guard !ids.isEmpty else { return "<empty>" }
        return ids.sorted().joined(separator: ",")
    }

    private var isGroupedMode: Bool {
        listMode == .grouped
    }

    private var groupingTaskToken: String {
        guard let key = currentGroupingCacheKey else { return "flat" }
        return "\(key.sequence)|\(key.query)|\(key.originFilter.rawValue)|\(key.sortKey.rawValue)|\(key.filterSignature)"
    }

    private func listSectionHeader(_ title: String) -> some View {
        HStack(spacing: 8) {
            SectionEyebrow(text: title)
            Rectangle()
                .fill(Color.secondary.opacity(0.12))
                .frame(height: 1)
        }
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    private var monitorSummaryGridColumns: [GridItem] {
        [GridItem(.adaptive(minimum: 180, maximum: 420), spacing: AetowerDesign.Spacing.md, alignment: .top)]
    }

    private func monitorFallbackTrendSamples(_ value: Double) -> [Double] {
        [value, value]
    }

    private func monitorFrictionAppearance(_ score: Double) -> TrendMetricValueAppearance {
        if score >= 60 { return .danger }
        if score >= 30 { return .warning }
        return .ok
    }

    private func monitorCPUAppearance(_ host: HostSnapshot) -> TrendMetricValueAppearance {
        if host.cpuPercent >= 85 || host.thermalState == .critical { return .danger }
        if host.cpuPercent >= 50 || host.thermalState == .serious { return .warning }
        return .ok
    }

    private func monitorMemoryAppearance(_ host: HostSnapshot) -> TrendMetricValueAppearance {
        switch hostPressureBand(host) {
        case .severe: return .danger
        case .elevated: return .warning
        case .nominal: return .ok
        }
    }

    private func monitorThroughputAppearance(_ bytesPerSecond: UInt64, warning: UInt64, danger: UInt64) -> TrendMetricValueAppearance {
        if bytesPerSecond >= danger { return .danger }
        if bytesPerSecond >= warning { return .warning }
        return .ok
    }

    private func monitorWakeupAppearance(_ host: HostSnapshot) -> TrendMetricValueAppearance {
        switch hostWakeupBand(host.wakeupsPerSecond) {
        case .severe: return .danger
        case .elevated: return .warning
        case .nominal: return .ok
        }
    }

    private func monitorGPUAppearance(_ host: HostSnapshot) -> TrendMetricValueAppearance {
        if host.gpuPercent >= 85 || host.gpuMemoryBytes >= 8 * 1_024 * 1_024 * 1_024 {
            return .danger
        }
        if host.gpuPercent >= 40 || host.gpuMemoryBytes >= 2 * 1_024 * 1_024 * 1_024 {
            return .warning
        }
        return .ok
    }

    private func applyPendingMonitorFocusIfNeeded() {
        guard let requestedEntityID = state.consumeMonitorFocusEntityID() else {
            return
        }
        searchText = ""
        focusedIndex = 0
        searchFieldFocused = false

        if isGroupedMode,
           let group = groupedEntities.first(where: { group in
               group.members.contains(where: { $0.entityId == requestedEntityID })
           }) {
            selectedEntityID = group.root.entityId
            selectedProcessRowID = nil
            return
        }

        if state.entitiesState.contains(where: { $0.entityId == requestedEntityID }) {
            originFilter = .all
            listMode = .flat
            selectedEntityID = requestedEntityID
            selectedProcessRowID = nil
        }
    }

    @MainActor
    private func refreshGroupingCache() async {
        guard let key = currentGroupingCacheKey else {
            groupingTask?.cancel()
            groupingTask = nil
            isGrouping = false
            return
        }

        if groupedEntitiesCache[key] != nil {
            isGrouping = false
            return
        }

        groupingTask?.cancel()

        let originCache = processOriginCache
        let entities = monitorSections.groupingEntities
        let sortKey = key.sortKey
        let query = key.query
        isGrouping = true

        let task = Task.detached(priority: .utility) {
            buildGroupedEntities(from: entities, query: query, sortKey: sortKey, originCache: originCache)
        }
        groupingTask = task

        let groups = await task.value
        guard !Task.isCancelled else { return }
        guard currentGroupingCacheKey == key else { return }

        let compatiblePrevious = groupedEntitiesCache.filter { cachedKey, _ in
            cachedKey.query == key.query
                && cachedKey.originFilter == key.originFilter
                && cachedKey.sortKey == key.sortKey
                && cachedKey.filterSignature == key.filterSignature
        }
        let previousSequenceFloor = key.sequence > 0 ? key.sequence - 1 : key.sequence
        groupedEntitiesCache = compatiblePrevious
            .filter { $0.key.sequence >= previousSequenceFloor }
        groupedEntitiesCache[key] = groups
        isGrouping = false

        if let selectedEntityID, !visibleEntityIDs.contains(selectedEntityID) {
            self.selectedEntityID = nil
            selectedProcessRowID = nil
        }
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
            return "\(entity.displayName) is currently highest by charged-memory load at \(String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: state.hostState.memoryTotalBytes)))."
        case .wakeups:
            return "\(entity.displayName) is currently highest by wakeup rate at \(formatWakeups(entity.metrics.wakeupsPerSecond))."
        case .processCount:
            return "\(entity.displayName) currently owns the widest live process footprint at \(liveProcessCount(for: entity)) PIDs."
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
            return "\(entity.displayName) has the oldest process group in the current list at \(ageLabel(from: entity.oldestProcessStartMillis, now: state.snapshotCapturedAtMillis))."
        case .newestFirst:
            return "\(entity.displayName) has the newest process group in the current list at \(ageLabel(from: entity.newestProcessStartMillis, now: state.snapshotCapturedAtMillis))."
        }
    }

}


/// Vim-style keyboard navigation for the entity list.
///
/// `entityIdAt` returns `nil` when the index is out of bounds so that
/// a stale `focusedIndex` from before a search/sort/filter change
/// cannot crash the view. The index is clamped on every movement, but
/// the closure is the last line of defense against the race between
/// the `@State` update and the next key event.
private struct KeyboardNavigationModifier: ViewModifier {
    @Binding var focusedIndex: Int
    @Binding var selectedEntityID: String?
    @Binding var sortKey: SortKey
    let entityCount: Int
    let entityIdAt: (Int) -> String?

    func body(content: Content) -> some View {
        content
            .onKeyPress("j") { moveDown() }
            .onKeyPress("k") { moveUp() }
            .onKeyPress(.return) { selectCurrent() }
            .onKeyPress(.escape) {
                withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = nil }
                return .handled
            }
            .onChange(of: entityCount) { _, newCount in
                // Clamp the cursor whenever the list size changes so a
                // search that narrows from 40 to 3 entities doesn't leave
                // focusedIndex at 39.
                if newCount > 0 {
                    focusedIndex = min(focusedIndex, newCount - 1)
                } else {
                    focusedIndex = 0
                }
            }
    }

    /// Clamp the index to valid bounds before using it.
    private var safeIndex: Int {
        guard entityCount > 0 else { return 0 }
        return min(max(focusedIndex, 0), entityCount - 1)
    }

    private func moveDown() -> KeyPress.Result {
        guard entityCount > 0 else { return .ignored }
        focusedIndex = min(safeIndex + 1, entityCount - 1)
        if let id = entityIdAt(focusedIndex) {
            withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = id }
        }
        return .handled
    }

    private func moveUp() -> KeyPress.Result {
        guard entityCount > 0 else { return .ignored }
        focusedIndex = max(safeIndex - 1, 0)
        if let id = entityIdAt(focusedIndex) {
            withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = id }
        }
        return .handled
    }

    private func selectCurrent() -> KeyPress.Result {
        guard selectedEntityID == nil, entityCount > 0 else { return .handled }
        focusedIndex = safeIndex
        if let id = entityIdAt(focusedIndex) {
            withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = id }
        }
        return .handled
    }
}
