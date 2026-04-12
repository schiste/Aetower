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

struct TrendMetricCard: View {
    let title: String
    let value: String
    let subtitle: String
    let samples: [Double]
    let style: TrendMetricStyle
    @State private var isHovered = false
    @State private var hoverX: CGFloat? = nil

    var body: some View {
        ZStack {
            // Background
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(style.color.opacity(0.12))
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(style.color.opacity(0.20), lineWidth: 1)

            // Sparkline
            TrendSparkline(samples: samples, color: style.color, hoverX: hoverX)
                .padding(6)
                .opacity(0.85)

            // Content
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.secondary)

                Spacer(minLength: 0)

                Text(value)
                    .font(.system(size: 28, weight: .bold, design: .rounded))
                    .minimumScaleFactor(0.7)
                    .lineLimit(1)
                    .contentTransition(.numericText())

                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .padding(10)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

            // Hover overlay: min/max/current + tooltip
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
                        Text(formatSampleValue(sampleValue))
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
        .frame(minHeight: 104, maxHeight: 104)
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
            Text(formatSampleValue(value))
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

    private func formatSampleValue(_ value: Double) -> String {
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

// MARK: - Interactive Sparkline

private struct TrendSparkline: View {
    let samples: [Double]
    let color: Color
    let hoverX: CGFloat?
    @State private var drawProgress: CGFloat = 0

    var body: some View {
        GeometryReader { geometry in
            let rect = geometry.frame(in: .local)
            ZStack {
                if samples.count >= 2 {
                    // Fill gradient
                    fillPath(in: rect)
                        .fill(
                            LinearGradient(
                                colors: [color.opacity(0.18), color.opacity(0.02)],
                                startPoint: .top,
                                endPoint: .bottom
                            )
                        )
                        .opacity(drawProgress)

                    // Stroke line
                    sparklinePath(in: rect)
                        .trim(from: 0, to: drawProgress)
                        .stroke(color.opacity(0.6), style: StrokeStyle(lineWidth: 2.0, lineCap: .round, lineJoin: .round))

                    // Hover cursor line
                    if let hoverX {
                        let clampedX = min(max(hoverX, 0), rect.width)
                        Path { path in
                            path.move(to: CGPoint(x: clampedX, y: rect.minY))
                            path.addLine(to: CGPoint(x: clampedX, y: rect.maxY))
                        }
                        .stroke(color.opacity(0.4), style: StrokeStyle(lineWidth: 1, dash: [3, 3]))

                        // Dot at intersection
                        let normalized = normalizedSamples
                        if !normalized.isEmpty {
                            let ratio = max(0, min(1, clampedX / rect.width))
                            let index = Int(ratio * Double(normalized.count - 1))
                            let clamped = max(0, min(normalized.count - 1, index))
                            let y = rect.maxY - CGFloat(normalized[clamped]) * rect.height
                            Circle()
                                .fill(color)
                                .frame(width: 6, height: 6)
                                .position(x: clampedX, y: y)
                        }
                    }
                } else {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(color.opacity(0.06))
                }
            }
        }
        .onAppear {
            withAnimation(AetowerDesign.Motion.slow) {
                drawProgress = 1.0
            }
        }
    }

    private func sparklinePath(in rect: CGRect) -> Path {
        let normalized = normalizedSamples
        return Path { path in
            guard let first = normalized.first else { return }
            path.move(to: point(for: first, index: 0, count: normalized.count, in: rect))
            for (index, value) in normalized.enumerated().dropFirst() {
                path.addLine(to: point(for: value, index: index, count: normalized.count, in: rect))
            }
        }
    }

    private func fillPath(in rect: CGRect) -> Path {
        let normalized = normalizedSamples
        return Path { path in
            guard let first = normalized.first else { return }
            let firstPoint = point(for: first, index: 0, count: normalized.count, in: rect)
            path.move(to: CGPoint(x: firstPoint.x, y: rect.maxY))
            path.addLine(to: firstPoint)
            for (index, value) in normalized.enumerated().dropFirst() {
                path.addLine(to: point(for: value, index: index, count: normalized.count, in: rect))
            }
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
            path.closeSubpath()
        }
    }

    private var normalizedSamples: [Double] {
        guard let min = samples.min(), let max = samples.max() else { return [] }
        let range = max - min
        guard range > 0.000_1 else { return samples.map { _ in 0.5 } }
        return samples.map { ($0 - min) / range }
    }

    private func point(for value: Double, index: Int, count: Int, in rect: CGRect) -> CGPoint {
        let x = count <= 1
            ? rect.midX
            : rect.minX + (CGFloat(index) / CGFloat(count - 1)) * rect.width
        let y = rect.maxY - CGFloat(value) * rect.height
        return CGPoint(x: x, y: y)
    }
}
