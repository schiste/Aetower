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

func agentShortSessionId(_ sessionId: String) -> String {
    if sessionId.count <= 10 {
        return sessionId
    }
    return String(sessionId.prefix(8))
}

func agentStatusTone(_ status: String) -> Color {
    let normalized = status.localizedLowercase
    if normalized.contains("approval") || normalized.contains("error") {
        return AetowerDesign.Status.warning
    }
    if normalized.contains("idle") || normalized.contains("finished") || normalized.contains("prompt") {
        return AetowerDesign.Status.success
    }
    return AetowerDesign.Status.ready
}

func agentShortenPath(_ path: String) -> String {
    if let home = FileManager.default.homeDirectoryForCurrentUser.path.removingPercentEncoding,
       path.hasPrefix(home)
    {
        return "~" + path.dropFirst(home.count)
    }
    return path
}

func agentMetricPill(_ label: String, color: Color) -> some View {
    Text(label)
        .font(.caption)
        .fontWeight(.medium)
        .foregroundStyle(color)
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xxs)
        .background(color.opacity(0.08), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
}

func agentMetricPill(label: String, color: Color) -> some View {
    agentMetricPill(label, color: color)
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

@ViewBuilder
func wakeupDiagnosticsBody(_ attribution: WakeupAttributionReportModel) -> some View {
    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
        if let processWakeups = attribution.processWakeupsPerSecond {
            Text("Process wakeups: \(formatWakeups(processWakeups))")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }

        if let dataSources = attribution.dataSources, !dataSources.isEmpty {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.xs)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.xs
            ) {
                ForEach(dataSources) { source in
                    wakeupDataSourceBadge(source)
                }
            }
        }

        if let dominantCause = attribution.dominantCause {
            Text(dominantCause)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }

        if let counters = attribution.exactThreadWakeupCounters {
            wakeupStatusBlock(title: counters.title, status: counters.status, detail: counters.detail, nextAction: counters.nextAction)
        }

        if let timerInventory = attribution.timerInventory {
            wakeupStatusBlock(
                title: "Timer inventory",
                status: timerInventory.status,
                detail: timerInventory.detail,
                integrationFields: timerInventory.recommendedIntegrationFields
            )
            ForEach(timerInventory.inferredTimerThreads.prefix(3)) { stack in
                sampledStackBody(stack)
                    .padding(AetowerDesign.Spacing.sm)
                    .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
            }
        }

        if let displayLinkState = attribution.displayLinkState {
            wakeupStatusBlock(
                title: "Display-link / render state",
                status: displayLinkState.status,
                detail: displayLinkState.detail,
                integrationFields: displayLinkState.recommendedIntegrationFields
            )
            if let latest = displayLinkState.latestPipeline {
                Text("\(latest.liveViews) live views · \(latest.polls) polls · \(latest.draws) draws · \(String(format: "%.1f MiB", latest.syncMib)) sync")
                    .font(.caption2.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            ForEach(displayLinkState.views.suffix(4)) { view in
                wakeupRenderViewRow(view)
            }
        }

        ForEach((attribution.sampledThreadBreakdown ?? attribution.queueBreakdown).prefix(4)) { stack in
            sampledStackBody(stack)
                .padding(AetowerDesign.Spacing.sm)
                .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
        }

        if !attribution.caveats.isEmpty {
            Text(attribution.caveats.joined(separator: " "))
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private func wakeupDataSourceBadge(_ source: WakeupDataSourceStatusModel) -> some View {
    VStack(alignment: .leading, spacing: 2) {
        Text(source.title)
            .font(.caption2.weight(.medium))
            .lineLimit(1)
        Text(source.status)
            .font(.caption2.monospaced())
            .foregroundStyle(wakeupStatusColor(source.status))
            .lineLimit(1)
    }
    .padding(.horizontal, AetowerDesign.Spacing.sm)
    .padding(.vertical, AetowerDesign.Spacing.xs)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(wakeupStatusColor(source.status).opacity(0.08), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
}

private func wakeupStatusBlock(
    title: String,
    status: String,
    detail: String,
    nextAction: String? = nil,
    integrationFields: [String] = []
) -> some View {
    VStack(alignment: .leading, spacing: 4) {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            Text(title)
                .font(.caption.weight(.semibold))
            Text(status)
                .font(.caption2.monospaced())
                .foregroundStyle(wakeupStatusColor(status))
        }
        Text(detail)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        if let nextAction, !nextAction.isEmpty {
            Text("Next: \(nextAction)")
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
        if !integrationFields.isEmpty {
            Text("Integration fields: \(integrationFields.prefix(8).joined(separator: ", "))")
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private func wakeupRenderViewRow(_ view: Chau7RenderViewSampleModel) -> some View {
    VStack(alignment: .leading, spacing: 4) {
        HStack {
            Text("View \(view.viewId)")
                .font(.caption.weight(.semibold))
            Text(view.state)
                .font(.caption2.monospaced())
                .foregroundStyle(wakeupStatusColor(view.state))
            Spacer()
            Text("\(view.draws) draws · \(String(format: "%.1f MiB", view.syncMib))")
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        Text("\(view.tab) · \(view.session) · \(view.mode)")
            .font(.caption2.monospaced())
            .foregroundStyle(.secondary)
            .lineLimit(1)
        if !view.reasons.isEmpty {
            Text(view.reasons.prefix(4).joined(separator: " · "))
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(2)
        }
    }
    .padding(AetowerDesign.Spacing.sm)
    .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
}

private func wakeupStatusColor(_ status: String) -> Color {
    let lowered = status.lowercased()
    if lowered.contains("unavailable") || lowered.contains("requires") {
        return AetowerDesign.Status.warning
    }
    if lowered.contains("heuristic") || lowered.contains("inference") || lowered.contains("derived") {
        return AetowerDesign.Status.warning
    }
    if lowered.contains("available") || lowered == "active" {
        return AetowerDesign.Status.success
    }
    return .secondary
}
