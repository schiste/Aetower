import SwiftUI
import AetowerBridge

public struct CompactHUDView: View {
    @ObservedObject private var state: AppState

    public init(state: AppState) { self.state = state }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
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
                        .foregroundStyle(.orange)
                }
            }
        }
        .padding(12)
        .frame(width: 200)
        .background(.ultraThinMaterial)
    }
}
