import SwiftUI
import AetowerBridge

public struct TimelineView: View {
    let events: [TimelineEvent]

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

                GroupBox("Recent timeline") {
                    if events.isEmpty {
                        ContentUnavailableView(
                            "No recent events",
                            systemImage: "clock.arrow.trianglehead.counterclockwise.rotate.90",
                            description: Text("Once apps spike or change state, those transitions will appear here.")
                        )
                        .frame(maxWidth: .infinity)
                    } else {
                        VStack(alignment: .leading, spacing: 14) {
                            ForEach(Array(events.reversed()), id: \.id) { event in
                                HStack(alignment: .top, spacing: 12) {
                                    Circle()
                                        .fill(color(for: event.severity))
                                        .frame(width: 10, height: 10)
                                        .padding(.top, 6)
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(event.title)
                                            .font(.headline)
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
}
