import AetowerBridge
import SwiftUI

private enum TimelineSeverityFilter: String, CaseIterable, Identifiable {
    case all
    case warningAndAbove = "warning+"
    case critical

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: return "All"
        case .warningAndAbove: return "Warning+"
        case .critical: return "Critical"
        }
    }
}

// MARK: - Event row (extracted from inline ForEach body)

private struct EventRow: View {
    let event: TimelineEvent
    /// The raw entity identifier (e.g. "ai-agent:1234") — not a
    /// human-readable display name. TimelineEvent does not carry the
    /// entity's display name, so we show the technical ID as a
    /// tertiary hint. This is honest about what data is available
    /// and still useful for cross-referencing with the Monitor tab.
    let entityId: String?

    var body: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
            Circle()
                .fill(severityColor(event.severity))
                .frame(width: 10, height: 10)
                .padding(.top, 6)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Text(event.title)
                        .font(.headline)
                    categoryBadge(event.category)
                    if let entityId {
                        Text(entityId)
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                    }
                }
                if !event.detail.isEmpty {
                    Text(event.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Text(formatEventTimestamp(event.timestampMillis))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func categoryBadge(_ category: TimelineCategory) -> some View {
        Text(categoryLabel(category))
            .font(.caption2.weight(.medium))
            .foregroundStyle(.secondary)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xxs)
            .background(Color.secondary.opacity(0.08), in: Capsule())
    }
}

// MARK: - Main view

public struct TimelineView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var categoryFilter: TimelineCategory?
    @State private var severityFilter: TimelineSeverityFilter = .all
    @State private var searchText = ""
    @State private var filteredEventsCache: [TimelineEvent] = []
    @State private var filterTask: Task<[TimelineEvent], Never>?
    @State private var isFiltering = false
    @State private var visibleEventLimit: Int
    @State private var expandTimelineRowsInSafeMode = false

    /// Computed once at init from the immutable events array rather
    /// than on every body evaluation. The category set cannot change
    /// without a new events prop being passed.
    private var sortedCategories: [TimelineCategory] {
        var seen: [TimelineCategory] = []
        for event in events {
            if !seen.contains(event.category) {
                seen.append(event.category)
            }
        }
        return seen.sorted { categoryLabel($0) < categoryLabel($1) }
    }

    private var events: [TimelineEvent] {
        state.snapshot.timeline
    }

    private var defaultVisibleEventLimit: Int {
        settings.operatorSafeModeEnabled ? 100 : 200
    }

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
        _visibleEventLimit = State(initialValue: settings.operatorSafeModeEnabled ? 100 : 200)
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("What changed recently")
                        .font(.system(size: 28, weight: .semibold, design: .rounded))
                    Text("The timeline records shifts that help explain why the ranking changed: spikes, new activity, and state transitions that matter to perceived system health.")
                        .foregroundStyle(.secondary)
                }

                GroupBox("Filters") {
                    HStack(spacing: AetowerDesign.Spacing.md) {
                        Picker("Severity", selection: $severityFilter) {
                            ForEach(TimelineSeverityFilter.allCases) { filter in
                                Text(filter.label).tag(filter)
                            }
                        }
                        .pickerStyle(.segmented)
                        .frame(maxWidth: 280)

                        Menu {
                            Button("All categories") {
                                categoryFilter = nil
                            }
                            ForEach(sortedCategories, id: \.self) { category in
                                Button(categoryLabel(category)) {
                                    categoryFilter = category
                                }
                            }
                        } label: {
                            HStack(spacing: AetowerDesign.Spacing.xs) {
                                Text(categoryFilter.map(categoryLabel) ?? "All categories")
                                Image(systemName: "chevron.down")
                                    .font(.caption2)
                            }
                            .padding(.horizontal, AetowerDesign.Spacing.sm)
                            .padding(.vertical, AetowerDesign.Spacing.xs)
                            .background(
                                Color.secondary.opacity(0.08),
                                in: Capsule()
                            )
                        }
                        .buttonStyle(.plain)

                        TextField("Search...", text: $searchText)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                            .frame(maxWidth: 180)

                        Spacer()

                        Text("\(filteredEventsCache.count) events")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    .padding(.top, AetowerDesign.Spacing.xs)

                    if settings.operatorSafeModeEnabled {
                        Text("Operator-safe mode keeps large timeline lists collapsed by default and starts from a smaller visible window.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                GroupBox("Recent timeline") {
                    if isFiltering && filteredEventsCache.isEmpty {
                        ContentUnavailableView {
                            Label("Preparing timeline", systemImage: "timeline.selection")
                        } description: {
                            ProgressView()
                                .controlSize(.large)
                        }
                        .frame(maxWidth: .infinity)
                    } else if filteredEventsCache.isEmpty {
                        ContentUnavailableView(
                            "No timeline events match this filter",
                            systemImage: "clock.arrow.trianglehead.counterclockwise.rotate.90",
                            description: Text("Try a broader filter or wait for more runtime changes.")
                        )
                        .frame(maxWidth: .infinity)
                    } else {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                            HStack {
                                Text("\(visibleEvents.count) of \(filteredEventsCache.count) visible")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Spacer()
                                if isFiltering {
                                    ProgressView()
                                        .controlSize(.small)
                                }
                            }

                            if settings.operatorSafeModeEnabled && !expandTimelineRowsInSafeMode {
                                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                                    Text("Detailed timeline rows stay collapsed until you explicitly expand them.")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    Button {
                                        expandTimelineRowsInSafeMode = true
                                    } label: {
                                        Label("Expand timeline rows", systemImage: "arrow.down.circle")
                                    }
                                    .buttonStyle(.bordered)
                                }
                            } else {
                                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                                    ForEach(visibleEvents, id: \.id) { event in
                                        EventRow(
                                            event: event,
                                            entityId: event.entityId
                                        )
                                    }
                                }

                                if visibleEventLimit < filteredEventsCache.count {
                                    Button {
                                        visibleEventLimit = min(
                                            visibleEventLimit + defaultVisibleEventLimit,
                                            filteredEventsCache.count
                                        )
                                    } label: {
                                        Label("Load older timeline events", systemImage: "arrow.down.circle")
                                    }
                                    .buttonStyle(.bordered)
                                }
                            }
                        }
                    }
                }
            }
            .padding(AetowerDesign.Spacing.xxl)
        }
        .task(id: filterCacheToken) {
            await recomputeFilteredEvents()
        }
        .onChange(of: settings.operatorSafeModeEnabled) { _, _ in
            visibleEventLimit = defaultVisibleEventLimit
            expandTimelineRowsInSafeMode = false
        }
        .onDisappear {
            filterTask?.cancel()
            filterTask = nil
        }
    }

    private var filterCacheToken: String {
        let newestEventID = events.last?.id ?? ""
        let oldestEventID = events.first?.id ?? ""
        let categoryLabelValue = categoryFilter.map(categoryLabel) ?? "all"
        return "\(events.count)|\(oldestEventID)|\(newestEventID)|\(severityFilter.rawValue)|\(categoryLabelValue)|\(searchText.lowercased())"
    }

    private var visibleEvents: [TimelineEvent] {
        Array(filteredEventsCache.prefix(visibleEventLimit))
    }

    @MainActor
    private func recomputeFilteredEvents() async {
        filterTask?.cancel()
        isFiltering = true
        let filterStarted = CFAbsoluteTimeGetCurrent()
        let token = filterCacheToken
        let events = self.events
        let severityFilter = self.severityFilter
        let categoryFilter = self.categoryFilter
        let query = self.searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let task = Task.detached(priority: .utility) {
            buildFilteredTimelineEvents(
                from: events,
                severityFilter: severityFilter,
                categoryFilter: categoryFilter,
                query: query
            )
        }
        filterTask = task
        let filtered = await task.value
        guard !Task.isCancelled, filterCacheToken == token else { return }
        filteredEventsCache = filtered
        visibleEventLimit = defaultVisibleEventLimit
        expandTimelineRowsInSafeMode = false
        isFiltering = false
        state.recordTimelinePayloadDiagnostics(
            totalEventCount: events.count,
            filteredEventCount: filtered.count,
            visibleEventCount: min(defaultVisibleEventLimit, filtered.count),
            filterDurationMillis: (CFAbsoluteTimeGetCurrent() - filterStarted) * 1000.0,
            safeModeEnabled: settings.operatorSafeModeEnabled
        )
    }
}

private func buildFilteredTimelineEvents(
    from events: [TimelineEvent],
    severityFilter: TimelineSeverityFilter,
    categoryFilter: TimelineCategory?,
    query: String
) -> [TimelineEvent] {
    var filtered: [TimelineEvent] = []
    filtered.reserveCapacity(min(events.count, 256))
    for event in events.reversed() {
        let severityOk: Bool = switch severityFilter {
        case .all:
            true
        case .warningAndAbove:
            event.severity == .warning || event.severity == .critical
        case .critical:
            event.severity == .critical
        }
        guard severityOk else { continue }
        guard categoryFilter.map({ $0 == event.category }) ?? true else { continue }
        if !query.isEmpty {
            let searchOk = event.title.lowercased().contains(query)
                || event.detail.lowercased().contains(query)
                || (event.entityId?.lowercased().contains(query) ?? false)
            guard searchOk else { continue }
        }
        filtered.append(event)
    }
    return filtered
}

// MARK: - Shared helpers

private func severityColor(_ severity: TimelineSeverity) -> Color {
    switch severity {
    case .info: return AetowerDesign.Status.ready
    case .warning: return AetowerDesign.Status.warning
    case .critical: return AetowerDesign.Status.error
    }
}

private func categoryLabel(_ category: TimelineCategory) -> String {
    switch category {
    case .lifecycle: return "Lifecycle"
    case .friction: return "Friction"
    case .host: return "Host"
    case .thermal: return "Thermal"
    case .anomaly: return "Anomaly"
    }
}

/// Formats event timestamps with both date context and time.
/// Shows relative time for recent events ("3m ago"), date+time for
/// older ones ("Mon 3:42 PM"). The built-in `.time` style only showed
/// time-of-day which was ambiguous for events hours or days apart.
/// Cached formatter for event timestamps older than 24 hours.
/// DateFormatter is expensive to allocate — creating one per event
/// per render would mean 120 allocations per pass for an old timeline.
private let eventDateFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateFormat = "EEE h:mm a"
    return formatter
}()

private func formatEventTimestamp(_ millis: UInt64) -> String {
    let date = Date(timeIntervalSince1970: Double(millis) / 1000)
    let elapsed = Date.now.timeIntervalSince(date)
    if elapsed < 60 { return "just now" }
    if elapsed < 3600 { return "\(Int(elapsed / 60))m ago" }
    if elapsed < 86400 { return "\(Int(elapsed / 3600))h ago" }
    return eventDateFormatter.string(from: date)
}
