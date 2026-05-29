import SwiftUI

/// A host-history graph with timestamp-aware hover-scrub (iStat-style). Wraps
/// the shared `MetricCardSurface` sparkline and adds a synchronized cursor whose
/// readout shows the value AND the time at the hovered point. Dense windows
/// (e.g. 7 days of snapshots) are downsampled to a bounded point count so the
/// path stays legible and cheap to draw.
struct HorizonGraph: View {
    let title: String
    let subtitle: String
    let samples: [Double]
    let timestamps: [UInt64]
    let tone: Color
    let format: (Double) -> String

    @State private var hoverX: CGFloat?
    @State private var hoverIndex: Int?

    private let maxPoints = 240

    var body: some View {
        let series = downsampled
        let displayValue = hoverIndex
            .flatMap { series.samples.indices.contains($0) ? series.samples[$0] : nil }
            ?? series.samples.last
            ?? 0
        let displayTime = hoverIndex.flatMap { index in
            series.timestamps.indices.contains(index) ? series.timestamps[index] : nil
        }

        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(title)
                        .font(.caption.weight(.semibold))
                    Text(subtitle)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 1) {
                    Text(format(displayValue))
                        .font(.system(size: 13, weight: .semibold, design: .rounded))
                        .foregroundStyle(tone)
                        .contentTransition(.numericText())
                    Text(displayTime.map(Self.timeLabel) ?? "now")
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundStyle(.tertiary)
                }
            }

            MetricCardSurface(
                tone: tone,
                samples: series.samples,
                minHeight: 88,
                hoverX: hoverX
            ) {
                EmptyView()
            } hoverOverlay: {
                GeometryReader { geometry in
                    Rectangle()
                        .fill(Color.clear)
                        .contentShape(Rectangle())
                        .onContinuousHover { phase in
                            switch phase {
                            case .active(let location):
                                hoverX = location.x
                                let width = geometry.size.width
                                guard series.samples.count > 1, width > 0 else {
                                    hoverIndex = nil
                                    return
                                }
                                let ratio = max(0, min(1, location.x / width))
                                hoverIndex = Int((ratio * Double(series.samples.count - 1)).rounded())
                            case .ended:
                                hoverX = nil
                                hoverIndex = nil
                            }
                        }
                }
            }
        }
    }

    /// Bucket the raw series into at most `maxPoints` points (mean per bucket),
    /// pairing each with the latest timestamp in its bucket.
    private var downsampled: (samples: [Double], timestamps: [UInt64]) {
        let count = samples.count
        guard count > maxPoints else { return (samples, timestamps) }
        var outSamples: [Double] = []
        var outTimestamps: [UInt64] = []
        outSamples.reserveCapacity(maxPoints)
        outTimestamps.reserveCapacity(maxPoints)
        for bucket in 0..<maxPoints {
            let start = bucket * count / maxPoints
            let end = max(start + 1, (bucket + 1) * count / maxPoints)
            let slice = samples[start..<end]
            outSamples.append(slice.reduce(0, +) / Double(slice.count))
            if timestamps.indices.contains(end - 1) {
                outTimestamps.append(timestamps[end - 1])
            }
        }
        return (outSamples, outTimestamps)
    }

    private static let formatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "MMM d HH:mm"
        return formatter
    }()

    private static func timeLabel(_ millis: UInt64) -> String {
        formatter.string(from: Date(timeIntervalSince1970: TimeInterval(millis) / 1000))
    }
}
