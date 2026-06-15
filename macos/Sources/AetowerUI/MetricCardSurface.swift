import SwiftUI

struct MetricCardSurface<OverlayContent: View, HoverOverlay: View>: View {
    let tone: Color
    let samples: [Double]
    let strokeOpacity: Double
    let fillOpacity: Double
    let sparklineOpacity: Double
    let contentPadding: CGFloat
    let minHeight: CGFloat
    let hoverX: CGFloat?
    /// When set (> 0), the sparkline is drawn on a fixed `0...fixedCeiling`
    /// axis instead of auto-scaling to the data window. `nil` keeps the
    /// relative behavior, so only callers that opt in (the Monitor rings)
    /// are affected.
    let fixedCeiling: Double?
    let overlayContent: OverlayContent
    let hoverOverlay: HoverOverlay

    init(
        tone: Color,
        samples: [Double],
        strokeOpacity: Double = 0.20,
        fillOpacity: Double = 0.12,
        sparklineOpacity: Double = 0.85,
        contentPadding: CGFloat = 10,
        minHeight: CGFloat = 104,
        hoverX: CGFloat? = nil,
        fixedCeiling: Double? = nil,
        @ViewBuilder overlayContent: () -> OverlayContent,
        @ViewBuilder hoverOverlay: () -> HoverOverlay
    ) {
        self.tone = tone
        self.samples = samples
        self.strokeOpacity = strokeOpacity
        self.fillOpacity = fillOpacity
        self.sparklineOpacity = sparklineOpacity
        self.contentPadding = contentPadding
        self.minHeight = minHeight
        self.hoverX = hoverX
        self.fixedCeiling = fixedCeiling
        self.overlayContent = overlayContent()
        self.hoverOverlay = hoverOverlay()
    }

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(tone.opacity(fillOpacity))
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(tone.opacity(strokeOpacity), lineWidth: 1)

            MetricCardSparkline(samples: samples, color: tone, hoverX: hoverX, fixedCeiling: fixedCeiling)
                .padding(6)
                .opacity(sparklineOpacity)

            overlayContent
                .padding(contentPadding)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)

            hoverOverlay
        }
        .frame(minHeight: minHeight, maxHeight: minHeight)
    }
}

private struct MetricCardSparkline: View {
    let samples: [Double]
    let color: Color
    let hoverX: CGFloat?
    var fixedCeiling: Double? = nil
    @State private var drawProgress: CGFloat = 0

    var body: some View {
        GeometryReader { geometry in
            let rect = geometry.frame(in: .local)
            ZStack {
                if samples.count >= 2 {
                    fillPath(in: rect)
                        .fill(
                            LinearGradient(
                                colors: [color.opacity(0.18), color.opacity(0.02)],
                                startPoint: .top,
                                endPoint: .bottom
                            )
                        )
                        .opacity(drawProgress)

                    sparklinePath(in: rect)
                        .trim(from: 0, to: drawProgress)
                        .stroke(color.opacity(0.6), style: StrokeStyle(lineWidth: 2.0, lineCap: .round, lineJoin: .round))

                    if let hoverX {
                        let clampedX = min(max(hoverX, 0), rect.width)
                        Path { path in
                            path.move(to: CGPoint(x: clampedX, y: rect.minY))
                            path.addLine(to: CGPoint(x: clampedX, y: rect.maxY))
                        }
                        .stroke(color.opacity(0.4), style: StrokeStyle(lineWidth: 1, dash: [3, 3]))

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
        // Fixed mode: draw on an absolute 0...ceiling axis (zero-baselined),
        // so graph height means the same thing every refresh. Values above the
        // ceiling peg at the top. Used by the Monitor rings when the user
        // enables fixed scaling.
        if let ceiling = fixedCeiling, ceiling > 0 {
            return samples.map { Swift.max(0, Swift.min(1, $0 / ceiling)) }
        }
        // Relative mode (default): normalize linearly, but clamp the top of the
        // range at the 95th percentile so a single burst (common in disk/network
        // series) doesn't squash all the ordinary variation into a flat sliver.
        // Fall back to the true max when that percentile collapses onto the floor
        // (e.g. a mostly idle/zero series) so a sparse signal still shows.
        // Bounded metrics (CPU %, 0–100 scores) have no outliers, so the
        // percentile ≈ max and this stays equivalent to plain min/max scaling.
        guard let minimum = samples.min(), let maximum = samples.max() else { return [] }
        let sorted = samples.sorted()
        let p95 = sorted[Int((Double(sorted.count) - 1.0) * 0.95)]
        let ceiling = p95 > minimum ? p95 : maximum
        let range = ceiling - minimum
        guard range > 0.000_1 else { return samples.map { _ in 0.5 } }
        return samples.map { Swift.min(1.0, ($0 - minimum) / range) }
    }

    private func point(for value: Double, index: Int, count: Int, in rect: CGRect) -> CGPoint {
        let x = count <= 1
            ? rect.midX
            : rect.minX + (CGFloat(index) / CGFloat(count - 1)) * rect.width
        let y = rect.maxY - CGFloat(value) * rect.height
        return CGPoint(x: x, y: y)
    }
}
