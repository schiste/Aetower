import SwiftUI
import AetowerBridge

public struct CompactHUDView: View {
    let state: AppState

    public init(state: AppState) { self.state = state }

    public var body: some View {
        let host = state.monitorViewModel.host
        let rows = state.monitorViewModel.processRows
        VStack(alignment: .leading, spacing: 6) {
            // Machine friction header
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Image(systemName: "bolt.fill")
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.frictionColor(machineFriction))
                Text(String(format: "%.0f", machineFriction))
                    .font(.system(size: 24, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(machineFriction))
                    .contentTransition(.numericText())

                Spacer()

                Text(String(format: "CPU %.0f%%", host?.cpuPercent ?? state.hostState.cpuPercent))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }

            // Top entities with friction bars
            ForEach(Array(rows.prefix(5).enumerated()), id: \.offset) { _, entity in
                HStack(spacing: 6) {
                    // Mini friction bar
                    ZStack(alignment: .leading) {
                        RoundedRectangle(cornerRadius: 2)
                            .fill(Color.secondary.opacity(0.1))
                            .frame(width: 40, height: 4)
                        RoundedRectangle(cornerRadius: 2)
                            .fill(AetowerDesign.frictionColor(entity.frictionScore))
                            .frame(width: AetowerDesign.frictionBarWidth(entity.frictionScore, maxWidth: 40), height: 4)
                    }

                    if entity.entityKind == .aiAgent {
                        Circle()
                            .fill(.blue)
                            .frame(width: 5, height: 5)
                    }

                    Text(entity.displayName)
                        .font(.system(size: 11))
                        .lineLimit(1)

                    Spacer()

                    Text(String(format: "%.0f", entity.frictionScore))
                        .font(.system(size: 11, weight: .medium, design: .rounded).monospacedDigit())
                        .foregroundStyle(AetowerDesign.frictionColor(entity.frictionScore))
                }
            }

            let aiAgentCount = Int(host?.aiAgentCount ?? state.hostState.aiAgentCount)
            if aiAgentCount > 0 {
                Divider()
                HStack(spacing: 4) {
                    Circle().fill(.blue).frame(width: 5, height: 5)
                    Text("\(aiAgentCount) AI agent\(aiAgentCount == 1 ? "" : "s")")
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(14)
        .frame(width: 220)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private var machineFriction: Float {
        Float(state.monitorViewModel.hostTrend?.machineFriction.last ?? Double(state.hostTrendState.machineFriction.last ?? 0))
    }
}
