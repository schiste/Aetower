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
    let events: [TimelineEvent]
    @State private var categoryFilter: TimelineCategory?
    @State private var severityFilter: TimelineSeverityFilter = .all
    @State private var searchText = ""

    /// Computed once at init from the immutable events array rather
    /// than on every body evaluation. The category set cannot change
    /// without a new events prop being passed.
    private let sortedCategories: [TimelineCategory]

    public init(events: [TimelineEvent]) {
        self.events = events
        var seen: [TimelineCategory] = []
        for event in events {
            if !seen.contains(event.category) {
                seen.append(event.category)
            }
        }
        self.sortedCategories = seen.sorted { categoryLabel($0) < categoryLabel($1) }
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

                        Text("\(filteredEvents.count) events")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    .padding(.top, AetowerDesign.Spacing.xs)
                }

                GroupBox("Recent timeline") {
                    if filteredEvents.isEmpty {
                        ContentUnavailableView(
                            "No timeline events match this filter",
                            systemImage: "clock.arrow.trianglehead.counterclockwise.rotate.90",
                            description: Text("Try a broader filter or wait for more runtime changes.")
                        )
                        .frame(maxWidth: .infinity)
                    } else {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                            ForEach(filteredEvents.reversed(), id: \.id) { event in
                                EventRow(
                                    event: event,
                                    entityId: event.entityId
                                )
                            }
                        }
                    }
                }
            }
            .padding(AetowerDesign.Spacing.xxl)
        }
    }

    private var filteredEvents: [TimelineEvent] {
        events.filter { event in
            let severityOk: Bool = switch severityFilter {
            case .all:
                true
            case .warningAndAbove:
                event.severity == .warning || event.severity == .critical
            case .critical:
                event.severity == .critical
            }
            let categoryOk = categoryFilter.map { $0 == event.category } ?? true
            let searchOk = searchText.isEmpty || {
                let query = searchText.lowercased()
                return event.title.lowercased().contains(query)
                    || event.detail.lowercased().contains(query)
                    || (event.entityId?.lowercased().contains(query) ?? false)
            }()
            return severityOk && categoryOk && searchOk
        }
    }
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
