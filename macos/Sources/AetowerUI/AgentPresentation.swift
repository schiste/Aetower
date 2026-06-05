import AetowerBridge
import SwiftUI

// Presentation helpers shared by AIAgentsView, Chau7View, and EntityDetailView.
// These were previously copy-pasted into each view; they are centralised here
// so there is a single source of truth. Where the per-view copies had drifted,
// the shared version takes the superset behavior (noted inline).

private let agentNonRuntimeBadges: Set<String> = [
    "ai-agent",
    "ai-model",
    "shell-tree",
    "daemon",
    "interactive",
    "chau7-live",
    "command-attributed",
    "privileged-helper",
    "approval-needed",
    "delegating",
    "cto-active",
    "at-prompt",
    "shell-loading",
    "waiting-input",
    "agent-error",
    "agent-idle",
    "agent-finished",
    "recent-process-exit",
]

/// The Chau7 session component for an entity, if one is attached.
func agentSessionComponent(for entity: EntitySnapshot) -> ComponentSnapshot? {
    entity.components.first { $0.adapterContext?.kind == .chau7Session }
}

private func agentRuntimeBadgeIDs(for entity: EntitySnapshot) -> [String] {
    var ids: [String] = []
    var seen = Set<String>()
    for badge in entity.badges {
        guard isRuntimeBadgeCandidate(badge) else {
            continue
        }
        if seen.insert(badge).inserted {
            ids.append(badge)
        }
    }
    return ids
}

private func isRuntimeBadgeCandidate(_ badge: String) -> Bool {
    !badge.isEmpty
        && !badge.hasPrefix("ai-session:")
        && !agentNonRuntimeBadges.contains(badge)
}

/// Human-readable provider/runtime label for an AI agent entity.
///
/// Prefers the session component title; otherwise derives it from the entity's
/// badges, excluding lifecycle/status badges. The exclusion list is the union of
/// the previous AIAgentsView and Chau7View copies (it includes `agent-finished`,
/// which AIAgentsView formerly omitted) so a terminal-status badge is never
/// mistaken for a provider name.
func agentProviderLabel(for entity: EntitySnapshot) -> String? {
    if let title = agentSessionComponent(for: entity)?.title,
       let prefix = title.components(separatedBy: " · ").first {
        let trimmed = prefix.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            return AetowerDesign.agentLabel(trimmed)
        }
    }

    return agentRuntimeBadgeIDs(for: entity).first.map(AetowerDesign.agentLabel)
}

/// Best project/workspace path for an entity, ranked by source reliability.
///
/// Ranking is the superset of the previous copies: repo roots win, then VS Code
/// workspaces (a refinement Chau7View formerly lacked), then other workspace
/// paths, then cwd. Ties break toward the shorter, then lexicographically.
func agentProjectContext(for entity: EntitySnapshot) -> String? {
    func rankedPath(_ component: ComponentSnapshot) -> (Int, String)? {
        if let repoRoot = component.adapterContext?.repoRoot, !repoRoot.isEmpty {
            let rank = component.adapterContext?.kind == .chau7Session ? 0 : 1
            return (rank, repoRoot)
        }

        if let workspacePath = component.adapterContext?.workspacePath, !workspacePath.isEmpty {
            let kind = component.adapterContext?.kind
            let rank = (kind == .vsCodeWorkspace || kind == .vsCodeRuntime) ? 0 : 2
            return (rank, workspacePath)
        }

        if let cwd = component.cwd, !cwd.isEmpty {
            return (3, cwd)
        }

        return nil
    }

    return entity.components
        .compactMap(rankedPath)
        .sorted { lhs, rhs in
            if lhs.0 != rhs.0 {
                return lhs.0 < rhs.0
            }
            if lhs.1.count != rhs.1.count {
                return lhs.1.count < rhs.1.count
            }
            return lhs.1.localizedCaseInsensitiveCompare(rhs.1) == .orderedAscending
        }
        .first?
        .1
}

/// Format an instantaneous energy rate (nanojoules/second) as W/mW.
func agentFormatEnergy(njPerS: Double) -> String {
    let mw = njPerS / 1_000_000
    if mw >= 1000 {
        return String(format: "%.1f W", mw / 1000)
    } else if mw >= 1 {
        return String(format: "%.0f mW", mw)
    } else {
        return "0 mW"
    }
}

/// Small capsule status badge used across the agent views.
func agentStateBadge(_ label: String, color: Color) -> some View {
    Text(label)
        .font(.caption2)
        .fontWeight(.medium)
        .foregroundStyle(color)
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xxs)
        .background(color.opacity(0.08), in: Capsule())
}

/// Shared inner content for a sampled-stack card (queue/thread, sample count,
/// classification, and top frames). Callers wrap this with their own padding
/// and background so each surface keeps its existing chrome.
@ViewBuilder
func sampledStackBody(_ stack: SampledStackReportModel) -> some View {
    VStack(alignment: .leading, spacing: 6) {
        HStack {
            Text(stack.queueLabel ?? stack.threadLabel)
                .font(.subheadline.weight(.semibold))
            Spacer()
            Text("\(stack.sampleCount) samples")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        Text(stack.classification)
            .font(.caption2.weight(.medium))
            .foregroundStyle(.tertiary)
        if !stack.topFrames.isEmpty {
            Text(stack.topFrames.prefix(3).joined(separator: " → "))
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}
