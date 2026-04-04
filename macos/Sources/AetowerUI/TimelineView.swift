import SwiftUI
import AetowerBridge

private enum TimelineSeverityFilter: String, CaseIterable, Identifiable {
    case all
    case warning
    case critical

    var id: String { rawValue }
}

public struct TimelineView: View {
    let events: [TimelineEvent]
    @State private var categoryFilter: TimelineCategory?
    @State private var severityFilter: TimelineSeverityFilter = .all

    public init(events: [TimelineEvent]) {
        self.events = events
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("What changed recently")
                        .font(.system(size: 28, weight: .semibold, design: .rounded))
                    Text("The timeline records shifts that help explain why the ranking changed: spikes, new activity, and state transitions that matter to perceived system health.")
                        .foregroundStyle(.secondary)
                }

                GroupBox("Filters") {
                    HStack(spacing: 12) {
                        Picker("Severity", selection: $severityFilter) {
                            ForEach(TimelineSeverityFilter.allCases) { filter in
                                Text(filter.rawValue.capitalized).tag(filter)
                            }
                        }
                        .pickerStyle(.segmented)
                        .frame(maxWidth: 260)

                        Menu {
                            Button("All categories") {
                                categoryFilter = nil
                            }
                            ForEach(allCategories, id: \.self) { category in
                                Button(categoryLabel(category)) {
                                    categoryFilter = category
                                }
                            }
                        } label: {
                            HStack(spacing: 6) {
                                Text(categoryFilter.map(categoryLabel) ?? "All categories")
                                Image(systemName: "chevron.down")
                                    .font(.caption2)
                            }
                            .padding(.horizontal, 10)
                            .padding(.vertical, 6)
                            .background(Color.secondary.opacity(0.08), in: Capsule())
                        }
                        .buttonStyle(.plain)

                        Spacer()

                        Text("\(filteredEvents.count) events")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    .padding(.top, 4)
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
                        VStack(alignment: .leading, spacing: 14) {
                            ForEach(Array(filteredEvents.reversed()), id: \.id) { event in
                                HStack(alignment: .top, spacing: 12) {
                                    Circle()
                                        .fill(color(for: event.severity))
                                        .frame(width: 10, height: 10)
                                        .padding(.top, 6)
                                    VStack(alignment: .leading, spacing: 4) {
                                        HStack(spacing: 8) {
                                            Text(event.title)
                                                .font(.headline)
                                            Text(categoryLabel(event.category))
                                                .font(.caption2.weight(.medium))
                                                .foregroundStyle(.secondary)
                                                .padding(.horizontal, 8)
                                                .padding(.vertical, 4)
                                                .background(Color.secondary.opacity(0.08), in: Capsule())
                                        }
                                        Text(event.detail)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                        Text(Date(timeIntervalSince1970: TimeInterval(event.timestampMillis) / 1000), style: .time)
                                            .font(.caption2)
                                            .foregroundStyle(.tertiary)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            .padding(24)
        }
    }

    private var filteredEvents: [TimelineEvent] {
        events.filter { event in
            let severityMatches: Bool = switch severityFilter {
            case .all:
                true
            case .warning:
                event.severity == .warning || event.severity == .critical
            case .critical:
                event.severity == .critical
            }
            let categoryMatches = categoryFilter.map { $0 == event.category } ?? true
            return severityMatches && categoryMatches
        }
    }

    private var allCategories: [TimelineCategory] {
        let categories = events.reduce(into: [TimelineCategory]()) { partialResult, event in
            if !partialResult.contains(event.category) {
                partialResult.append(event.category)
            }
        }
        return categories.sorted { categoryLabel($0) < categoryLabel($1) }
    }

    private func color(for severity: TimelineSeverity) -> Color {
        switch severity {
        case .info:
            return .blue
        case .warning:
            return .orange
        case .critical:
            return .red
        }
    }

    private func categoryLabel(_ category: TimelineCategory) -> String {
        switch category {
        case .lifecycle:
            return "Lifecycle"
        case .friction:
            return "Friction"
        case .host:
            return "Host"
        case .thermal:
            return "Thermal"
        case .anomaly:
            return "Anomaly"
        }
    }
}
