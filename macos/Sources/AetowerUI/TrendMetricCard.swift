import SwiftUI

enum TrendMetricStyle {
    case friction
    case cpu
    case memory
    case disk

    var color: Color {
        switch self {
        case .friction:
            return .orange
        case .cpu:
            return .blue
        case .memory:
            return .green
        case .disk:
            return .pink
        }
    }
}

struct TrendMetricCard: View {
    let title: String
    let value: String
    let subtitle: String
    let samples: [Double]
    let style: TrendMetricStyle

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(style.color.opacity(0.08))

            TrendSparkline(samples: samples, color: style.color)
                .padding(10)
                .opacity(0.9)

            VStack(alignment: .leading, spacing: 8) {
                Text(title)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)

                Spacer(minLength: 0)

                Text(value)
                    .font(.system(size: 24, weight: .semibold, design: .rounded))
                    .minimumScaleFactor(0.7)
                    .lineLimit(1)

                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .padding(14)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .aspectRatio(1, contentMode: .fit)
    }
}

private struct TrendSparkline: View {
    let samples: [Double]
    let color: Color

    var body: some View {
        GeometryReader { geometry in
            let rect = geometry.frame(in: .local)
            ZStack {
                if samples.count >= 2 {
                    sparklinePath(in: rect)
                        .stroke(color.opacity(0.6), style: StrokeStyle(lineWidth: 2.0, lineCap: .round, lineJoin: .round))

                    fillPath(in: rect)
                        .fill(
                            LinearGradient(
                                colors: [color.opacity(0.18), color.opacity(0.02)],
                                startPoint: .top,
                                endPoint: .bottom
                            )
                        )
                } else {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(color.opacity(0.06))
                }
            }
        }
        .allowsHitTesting(false)
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
        guard let min = samples.min(), let max = samples.max() else {
            return []
        }
        let range = max - min
        guard range > 0.000_1 else {
            return samples.map { _ in 0.5 }
        }
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
