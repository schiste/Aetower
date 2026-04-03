import SwiftUI
import AetowerBridge

public struct CompactHUDView: View {
    @ObservedObject private var state: AppState

    public init(state: AppState) { self.state = state }

    private var machineFriction: Float {
        let entities = state.snapshot.entities
        guard !entities.isEmpty else { return 0 }
        return entities.prefix(5).map(\.friction.totalScore).reduce(0, +) / Float(min(entities.count, 5))
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Machine Friction")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                Text(String(format: "%.1f", machineFriction))
                    .font(.system(size: 18, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(machineFriction))
            }

            Divider()

            Text("Top Friction")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(Array(state.snapshot.entities.prefix(3).enumerated()), id: \.offset) { _, entity in
                HStack {
                    Text(entity.displayName)
                        .font(.caption)
                        .lineLimit(1)
                    Spacer()
                    Text(String(format: "%.1f", entity.friction.totalScore))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(AetowerDesign.frictionColor(entity.friction.totalScore))
                }
            }
        }
        .padding(12)
        .frame(width: 200)
        .background(.ultraThinMaterial)
    }
}
