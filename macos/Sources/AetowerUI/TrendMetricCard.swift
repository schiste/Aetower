import SwiftUI

enum TrendMetricStyle {
    case friction
    case cpu
    case memory
    case disk
    case network
    case energy

    var color: Color {
        switch self {
        case .friction: return AetowerDesign.Tone.friction
        case .cpu: return AetowerDesign.Tone.cpu
        case .memory: return AetowerDesign.Tone.memory
        case .disk: return AetowerDesign.Tone.disk
        case .network: return AetowerDesign.Tone.network
        case .energy: return AetowerDesign.Tone.energy
        }
    }
}

enum TrendMetricValueAppearance {
    case neutral
    case ok
    case warning
    case danger

    var color: Color {
        switch self {
        case .neutral:
            return .primary
        case .ok:
            return AetowerDesign.Status.success
        case .warning:
            return AetowerDesign.Status.warning
        case .danger:
            return AetowerDesign.Status.error
        }
    }

    var weight: Font.Weight {
        switch self {
        case .neutral:
            return .bold
        case .ok:
            return .regular
        case .warning:
            return .medium
        case .danger:
            return .bold
        }
    }

    var italic: Bool {
        if case .warning = self {
            return true
        }
        return false
    }
}

struct TrendMetricCard: View {
    let title: String
    let value: String
    let subtitle: String
    let samples: [Double]
    let style: TrendMetricStyle
    let valueAppearance: TrendMetricValueAppearance
    let minHeight: CGFloat
    let sampleValueFormatter: (Double) -> String
    @State private var isHovered = false
    @State private var hoverX: CGFloat? = nil

    init(
        title: String,
        value: String,
        subtitle: String,
        samples: [Double],
        style: TrendMetricStyle,
        valueAppearance: TrendMetricValueAppearance = .neutral,
        sampleValueFormatter: @escaping (Double) -> String = TrendMetricCard.defaultSampleFormatter,
        minHeight: CGFloat = 104
    ) {
        self.title = title
        self.value = value
        self.subtitle = subtitle
        self.samples = samples
        self.style = style
        self.valueAppearance = valueAppearance
        self.sampleValueFormatter = sampleValueFormatter
        self.minHeight = minHeight
    }

    var body: some View {
        MetricCardSurface(
            tone: style.color,
            samples: samples,
            minHeight: minHeight,
            hoverX: hoverX
        ) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.secondary)

                Spacer(minLength: 0)

                Text(value)
                    .font(.system(size: 28, weight: valueAppearance.weight, design: .rounded))
                    .foregroundStyle(valueAppearance.color)
                    .italic(valueAppearance.italic)
                    .minimumScaleFactor(0.7)
                    .lineLimit(1)
                    .contentTransition(.numericText())

                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        } hoverOverlay: {
            if isHovered && samples.count >= 2 {
                VStack {
                    HStack(spacing: 8) {
                        Spacer()
                        miniStat("min", samples.min() ?? 0)
                        miniStat("max", samples.max() ?? 0)
                        miniStat("cur", samples.last ?? 0)
                    }
                    .padding(.horizontal, 8)
                    .padding(.top, 6)
                    Spacer()
                }

                // Hover tooltip at cursor position
                if let hoverX, let sampleValue = sampleAtPosition(hoverX) {
                    GeometryReader { geo in
                        let clampedX = min(max(hoverX, 30), geo.size.width - 30)
                        Text(sampleValueFormatter(sampleValue))
                            .font(.system(size: 10, weight: .semibold, design: .monospaced))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 3)
                            .background(style.color.opacity(0.85), in: Capsule())
                            .position(x: clampedX, y: geo.size.height - 14)
                    }
                }
            }
        }
        .shadow(color: style.color.opacity(isHovered ? 0.2 : 0), radius: 8)
        .onContinuousHover { phase in
            switch phase {
            case .active(let location):
                isHovered = true
                hoverX = location.x - 16 // account for padding
            case .ended:
                isHovered = false
                hoverX = nil
            }
        }
        .animation(AetowerDesign.Motion.quick, value: isHovered)
    }

    private func miniStat(_ label: String, _ value: Double) -> some View {
        HStack(spacing: 2) {
            Text(label)
                .font(.system(size: 8, weight: .medium))
                .foregroundStyle(.tertiary)
            Text(sampleValueFormatter(value))
                .font(.system(size: 8, weight: .semibold, design: .monospaced))
                .foregroundStyle(.secondary)
        }
    }

    private func sampleAtPosition(_ x: CGFloat) -> Double? {
        guard !samples.isEmpty else { return nil }
        // Card inner width is roughly frame width - 2*padding(16)
        let cardWidth: CGFloat = 160 // approximate
        let ratio = max(0, min(1, x / cardWidth))
        let index = Int(ratio * Double(samples.count - 1))
        let clampedIndex = max(0, min(samples.count - 1, index))
        return samples[clampedIndex]
    }

    nonisolated private static func defaultSampleFormatter(_ value: Double) -> String {
        if value >= 1_000_000_000 {
            return String(format: "%.1fG", value / 1_000_000_000)
        } else if value >= 1_000_000 {
            return String(format: "%.1fM", value / 1_000_000)
        } else if value >= 1000 {
            return String(format: "%.0f", value)
        } else {
            return String(format: "%.1f", value)
        }
    }
}
