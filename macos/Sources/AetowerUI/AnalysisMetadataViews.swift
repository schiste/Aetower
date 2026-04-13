import SwiftUI

struct AnalysisMetadataStrip: View {
    let descriptor: AnalysisDescriptor
    let updatedAt: Date?
    let trailing: String?

    init(
        descriptor: AnalysisDescriptor,
        updatedAt: Date? = nil,
        trailing: String? = nil
    ) {
        self.descriptor = descriptor
        self.updatedAt = updatedAt
        self.trailing = trailing
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                metadataBadge(descriptor.label, tone: .accentColor)
                metadataBadge(descriptor.confidence.rawValue, tone: confidenceTone)
                metadataBadge(descriptor.overhead.rawValue, tone: overheadTone)
                if let trailing, !trailing.isEmpty {
                    metadataBadge(trailing, tone: .secondary)
                }
            }
            Text(detailLine)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var detailLine: String {
        if let updatedAt {
            return "\(descriptor.detail) Updated \(updatedAt.formatted(date: .omitted, time: .shortened))."
        }
        return descriptor.detail
    }

    private var confidenceTone: Color {
        switch descriptor.confidence {
        case .measured: return AetowerDesign.Tone.cpu
        case .persisted: return AetowerDesign.Tone.network
        case .inferred: return AetowerDesign.Tone.energy
        case .sampled: return AetowerDesign.Tone.memory
        case .heuristic: return AetowerDesign.Tone.friction
        }
    }

    private var overheadTone: Color {
        switch descriptor.overhead {
        case .low: return AetowerDesign.Tone.cpu
        case .medium: return AetowerDesign.Tone.energy
        case .high: return AetowerDesign.Tone.friction
        }
    }

    private func metadataBadge(_ text: String, tone: Color) -> some View {
        Text(text)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(tone)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(tone.opacity(0.10), in: Capsule())
    }
}
