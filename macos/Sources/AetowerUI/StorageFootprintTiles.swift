import SwiftUI

// MARK: - Shared storage-footprint presentation
// Both StorageView (repo footprint card) and RepositoryView (detail Storage
// tab) render the same StorageRepoFootprintModel; the tier tone/icon mapping
// and the artifact-mix list live here so the two stay consistent.

func storageCleanupTierTone(_ tier: String) -> Color {
    switch tier {
    case "safe":
        return AetowerDesign.Status.ready
    case "rebuildable":
        return AetowerDesign.Tone.disk
    case "expensive":
        return AetowerDesign.Status.warning
    case "risky":
        return AetowerDesign.Status.error
    default:
        return .secondary
    }
}

func storageCleanupTierIcon(_ tier: String) -> String {
    switch tier {
    case "safe":
        return "checkmark.shield"
    case "rebuildable":
        return "hammer"
    case "expensive":
        return "clock.badge.exclamationmark"
    case "risky":
        return "exclamationmark.triangle"
    default:
        return "folder"
    }
}

func storageFormatPercent(_ value: Double) -> String {
    if value.rounded() == value {
        return "\(Int(value))%"
    }
    return String(format: "%.1f%%", value)
}

/// Compact per-tier composition rows for a repo footprint's artifact mix.
struct StorageArtifactMixList: View {
    let artifactMix: [StorageRepoArtifactMixModel]
    var limit: Int = 4

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            Text("Artifact mix")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(artifactMix.prefix(limit)) { artifact in
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Image(systemName: storageCleanupTierIcon(artifact.cleanupTier))
                        .foregroundStyle(storageCleanupTierTone(artifact.cleanupTier))
                        .frame(width: 16)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(artifact.label)
                            .font(.caption.weight(.semibold))
                        Text(artifact.rebuildCommand ?? "No regenerate command")
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: 2) {
                        Text(formatBytes(artifact.bytes))
                            .font(.caption2.weight(.semibold))
                        Text("\(artifact.itemCount) item\(artifact.itemCount == 1 ? "" : "s") · \(artifact.estimatedRebuildCost)")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }
}
