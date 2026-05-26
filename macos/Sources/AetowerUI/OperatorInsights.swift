import Foundation
import SwiftUI
import AetowerBridge

enum OperatorSeverity: Int, Comparable {
    case info = 0
    case warning = 1
    case critical = 2

    static func < (lhs: OperatorSeverity, rhs: OperatorSeverity) -> Bool {
        lhs.rawValue < rhs.rawValue
    }

    var label: String {
        switch self {
        case .info:
            return "Stable"
        case .warning:
            return "Watch"
        case .critical:
            return "Critical"
        }
    }

    var color: Color {
        switch self {
        case .info:
            return AetowerDesign.Status.ready
        case .warning:
            return AetowerDesign.Status.warning
        case .critical:
            return AetowerDesign.Status.error
        }
    }
}

struct HostIncidentSummary {
    let severity: OperatorSeverity
    let title: String
    let summary: String
    let action: String
}

struct BurdenLeaderSummary: Identifiable {
    let id: String
    let title: String
    let entityId: String
    let entityName: String
    let metricValue: String
    let detail: String
    let severity: OperatorSeverity
}

struct OperatorRecommendationSummary: Identifiable {
    let id: String
    let title: String
    let detail: String
    let action: String
    let severity: OperatorSeverity
}

struct OperatorHealthCheckSummary: Identifiable {
    let id: String
    let title: String
    let value: String
    let detail: String
    let footer: String
    let severity: OperatorSeverity
}

func buildHostIncident(
    snapshot: SystemSnapshot,
    history: HistoryRangeSummary?
) -> HostIncidentSummary? {
    let host = snapshot.host
    let memoryBand = operatorHostPressureBand(host)
    let wakeupBand = operatorWakeupBand(host.wakeupsPerSecond)
    let storeBand = history.map(operatorHistorySeverity) ?? .info
    let dominantSeverity = max(memoryBand, max(wakeupBand, storeBand))
    guard dominantSeverity != .info else {
        return nil
    }

    if memoryBand >= wakeupBand && memoryBand >= storeBand {
        let memoryLeaders = snapshot.entities
            .sorted { entityEffectiveMemoryBytes($0) > entityEffectiveMemoryBytes($1) }
            .prefix(3)
            .map(\.displayName)
            .joined(separator: ", ")
        return HostIncidentSummary(
            severity: memoryBand,
            title: "Machine under memory pressure",
            summary: "\(formatBytes(host.memoryUsedBytes)) used of \(formatBytes(host.memoryTotalBytes)), \(formatBytes(host.compressedMemoryBytes)) compressed, \(formatBytes(host.swapUsedBytes)) swap.",
            action: memoryLeaders.isEmpty
                ? "Reduce the top memory-heavy groups first."
                : "Reduce the top memory-heavy groups first: \(memoryLeaders)."
        )
    }

    if wakeupBand >= storeBand {
        let leader = snapshot.entities.max {
            $0.metrics.wakeupsPerSecond < $1.metrics.wakeupsPerSecond
        }
        return HostIncidentSummary(
            severity: wakeupBand,
            title: "Machine is wakeup-heavy",
            summary: "Host wakeups are \(formatWakeups(host.wakeupsPerSecond)); background polling is likely affecting battery life.",
            action: leader.map {
                "Start with \($0.displayName) at \(formatWakeups($0.metrics.wakeupsPerSecond)) and reduce watcher or polling churn."
            } ?? "Reduce timer-driven and polling-heavy workloads first."
        )
    }

    guard let history else {
        return nil
    }
    return HostIncidentSummary(
        severity: storeBand,
        title: "Persisted history store needs attention",
        summary: "\(formatBytes(history.storeBytes)) DB, \(formatBytes(history.walBytes)) WAL, \(history.quarantineCount) quarantined rows.",
        action: "Shorten retention or let Aetower compact the store before History becomes operator-hostile."
    )
}

func buildBurdenLeaders(snapshot: SystemSnapshot) -> [BurdenLeaderSummary] {
    var leaders: [BurdenLeaderSummary] = []

    if let memoryLeader = snapshot.entities.max(by: { entityEffectiveMemoryBytes($0) < entityEffectiveMemoryBytes($1) }),
       entityEffectiveMemoryBytes(memoryLeader) > 0
    {
        leaders.append(
            BurdenLeaderSummary(
                id: "memory",
                title: "Memory leader",
                entityId: memoryLeader.entityId,
                entityName: memoryLeader.displayName,
                metricValue: formatBytes(entityEffectiveMemoryBytes(memoryLeader)),
                detail: "Largest charged memory footprint in the current snapshot.",
                severity: operatorHostPressureBand(snapshot.host)
            )
        )
    }

    if let wakeupLeader = snapshot.entities.max(by: { $0.metrics.wakeupsPerSecond < $1.metrics.wakeupsPerSecond }),
       wakeupLeader.metrics.wakeupsPerSecond >= 100
    {
        leaders.append(
            BurdenLeaderSummary(
                id: "wakeups",
                title: "Wakeup leader",
                entityId: wakeupLeader.entityId,
                entityName: wakeupLeader.displayName,
                metricValue: formatWakeups(wakeupLeader.metrics.wakeupsPerSecond),
                detail: "Top timer and interrupt churn source right now.",
                severity: operatorWakeupBand(snapshot.host.wakeupsPerSecond)
            )
        )
    }

    if let diskLeader = snapshot.entities.max(by: {
        ($0.metrics.diskReadBps + $0.metrics.diskWriteBps) < ($1.metrics.diskReadBps + $1.metrics.diskWriteBps)
    }) {
        let throughput = diskLeader.metrics.diskReadBps + diskLeader.metrics.diskWriteBps
        if throughput > 0 {
            leaders.append(
                BurdenLeaderSummary(
                    id: "disk",
                    title: "Disk leader",
                    entityId: diskLeader.entityId,
                    entityName: diskLeader.displayName,
                    metricValue: formatRate(throughput),
                    detail: "Highest read/write throughput among attributed groups.",
                    severity: throughput >= 50 * 1024 * 1024 ? .warning : .info
                )
            )
        }
    }

    if let networkLeader = snapshot.entities.max(by: {
        ($0.metrics.networkReceiveBps + $0.metrics.networkSendBps) < ($1.metrics.networkReceiveBps + $1.metrics.networkSendBps)
    }) {
        let throughput = networkLeader.metrics.networkReceiveBps + networkLeader.metrics.networkSendBps
        if throughput > 0 {
            leaders.append(
                BurdenLeaderSummary(
                    id: "network",
                    title: "Network leader",
                    entityId: networkLeader.entityId,
                    entityName: networkLeader.displayName,
                    metricValue: formatRate(throughput),
                    detail: "Largest current network mover among attributed groups.",
                    severity: throughput >= 20 * 1024 * 1024 ? .warning : .info
                )
            )
        }
    }

    return leaders
}

func buildOperatorRecommendations(
    snapshot: SystemSnapshot,
    diagnostics: DiagnosticsOverview,
    history: HistoryRangeSummary?,
    runtime: RuntimeLagMetrics
) -> [OperatorRecommendationSummary] {
    var recommendations: [OperatorRecommendationSummary] = []

    if operatorHostPressureBand(snapshot.host) != .info {
        recommendations.append(
            OperatorRecommendationSummary(
                id: "host-memory",
                title: "Reduce memory pressure",
                detail: "Compression and swap are already active. Work the biggest resident groups down first.",
                action: topEntityNames(snapshot: snapshot, metric: entityEffectiveMemoryBytes),
                severity: operatorHostPressureBand(snapshot.host)
            )
        )
    }

    if operatorWakeupBand(snapshot.host.wakeupsPerSecond) != .info {
        recommendations.append(
            OperatorRecommendationSummary(
                id: "host-wakeups",
                title: "Reduce wakeup-heavy workloads",
                detail: "Wakeups are high enough to hurt battery life and foreground responsiveness.",
                action: topEntityNames(snapshot: snapshot) { $0.metrics.wakeupsPerSecond },
                severity: operatorWakeupBand(snapshot.host.wakeupsPerSecond)
            )
        )
    }

    if let history, operatorHistorySeverity(history) != .info {
        recommendations.append(
            OperatorRecommendationSummary(
                id: "history-store",
                title: "Keep the history store bounded",
                detail: "\(formatBytes(history.storeBytes)) DB, \(formatBytes(history.walBytes)) WAL, \(history.quarantineCount) quarantined rows.",
                action: "Trim retention or let Aetower compact the store before History becomes slow again.",
                severity: operatorHistorySeverity(history)
            )
        )
    }

    let diagnosticsSeverity = operatorDiagnosticsSeverity(diagnostics)
    if diagnosticsSeverity != .info {
        recommendations.append(
            OperatorRecommendationSummary(
                id: "diagnostics",
                title: "Reduce diagnostics noise",
                detail: "\(diagnostics.warnCount) warnings and \(diagnostics.errorCount) errors are currently retained.",
                action: "Collapse repeated warnings into counters or health state so the retained stream stays operator-grade.",
                severity: diagnosticsSeverity
            )
        )
    }

    let runtimeSeverity = operatorRuntimeSeverity(runtime)
    if runtimeSeverity != .info {
        recommendations.append(
            OperatorRecommendationSummary(
                id: "runtime",
                title: "Investigate Aetower self-overhead",
                detail: String(
                    format: "tick %.1f ms · collect %.1f ms · history %.1f ms",
                    runtime.engineTickMillis,
                    runtime.collectMillis,
                    runtime.historyMillis
                ),
                action: "Keep history maintenance and collection work under the current target cadence.",
                severity: runtimeSeverity
            )
        )
    }

    for entity in snapshot.entities {
        for recommendation in entity.recommendations.prefix(1) {
            recommendations.append(
                OperatorRecommendationSummary(
                    id: "entity-\(entity.entityId)-\(recommendation.title)",
                    title: recommendation.title,
                    detail: recommendation.detail,
                    action: entity.displayName,
                    severity: entity.anomalyDetected ? .warning : .info
                )
            )
        }
    }

    recommendations.sort {
        if $0.severity != $1.severity {
            return $0.severity > $1.severity
        }
        return $0.title < $1.title
    }
    return Array(recommendations.prefix(5))
}

func buildSelfHealthChecks(
    snapshot: SystemSnapshot,
    diagnostics: DiagnosticsOverview,
    history: HistoryRangeSummary?,
    runtime: RuntimeLagMetrics,
    localMcpServerHealthy: Bool
) -> [OperatorHealthCheckSummary] {
    var checks = [
        OperatorHealthCheckSummary(
            id: "runtime",
            title: "Runtime",
            value: operatorRuntimeSeverity(runtime).label,
            detail: String(
                format: "tick %.1f ms vs target %.1f ms · helpers %u (%u stale)",
                runtime.engineTickMillis,
                runtime.targetTickMillis,
                runtime.mcpHelperCount,
                runtime.staleMcpHelperCount
            ),
            footer: "Keep collection, persistence, and helper churn under the active tick budget.",
            severity: operatorRuntimeSeverity(runtime)
        ),
        OperatorHealthCheckSummary(
            id: "diagnostics",
            title: "Diagnostics",
            value: operatorDiagnosticsSeverity(diagnostics).label,
            detail: operatorDiagnosticsDetail(diagnostics),
            footer: "Reduce retained warning/error noise so the stream stays operator-grade.",
            severity: operatorDiagnosticsSeverity(diagnostics)
        ),
        OperatorHealthCheckSummary(
            id: "capabilities",
            title: "Capabilities",
            value: capabilityHealthValue(snapshot.capabilities),
            detail: capabilityHealthDetail(snapshot.capabilities),
            footer: "Grant or configure missing capabilities before trusting attribution depth.",
            severity: capabilityOverallSeverity(snapshot.capabilities)
        ),
        OperatorHealthCheckSummary(
            id: "mcp",
            title: "Local MCP",
            value: localMcpServerHealthy ? "Live" : "Down",
            detail: localMcpServerHealthy
                ? mcpHelperHealthDetail(runtime)
                : "The in-app MCP socket is not currently reachable.",
            footer: localMcpServerHealthy
                ? "Watch helper count and stale sessions so local agent access remains cheap and reliable."
                : "Bring the local socket back before relying on agent-facing tools.",
            severity: localMcpServerHealthy ? mcpHelperHealthSeverity(runtime) : .warning
        ),
    ]

    if let history {
        checks.insert(
            OperatorHealthCheckSummary(
                id: "history",
                title: "History store",
                value: operatorHistorySeverity(history).label,
                detail: "\(formatBytes(history.storeBytes)) DB · \(formatBytes(history.walBytes)) WAL · \(history.snapshotCount) snapshots",
                footer: "Trim retention or let compaction run before heavy tabs become expensive again.",
                severity: operatorHistorySeverity(history)
            ),
            at: 2
        )
    }

    return checks
}

func capabilityConsequenceText(_ capability: CapabilitySnapshot) -> String {
    switch capability.kind {
    case .accessibility:
        switch capability.state {
        case .granted:
            return "Focused app and window context can be attributed directly."
        case .denied:
            return "Frontmost app and window-title attribution stay degraded."
        case .requested:
            return "Aetower is waiting for macOS to grant Accessibility access."
        case .unknown, .unavailable:
            return "Without Accessibility, active window context and deeper UI attribution remain weaker."
        }
    case .fullDiskAccess:
        switch capability.state {
        case .granted:
            return "Disk-backed inspection and richer local context can work without sandboxed blind spots."
        case .denied:
            return "Some persisted files and local developer context may remain partially opaque."
        case .requested:
            return "Aetower is waiting for Full Disk Access so persisted context can be read consistently."
        case .unknown, .unavailable:
            return "Without Full Disk Access, some repo, browser, and local-state inspection paths stay limited."
        }
    case .appleAutomation:
        switch capability.state {
        case .granted:
            return "Automation-backed app context and richer browser/app detail are available."
        case .denied:
            return "Automation-driven app context will stay unavailable until macOS grants access."
        case .requested:
            return "Aetower is waiting for Apple Automation consent."
        case .unknown, .unavailable:
            return "Without Apple Automation, some app-specific context remains shallow."
        }
    case .chromiumDebug:
        return capability.state == .granted
            ? "Chromium pages and tabs can be attributed directly."
            : "Browser tab and DOM attribution will stay coarse until Chromium debug access is available."
    case .dockerSocket:
        return capability.state == .granted
            ? "Container workloads can be grouped and attributed more precisely."
            : "Container attribution will stay best-effort until the Docker socket is reachable."
    case .privilegedHelper:
        return capability.state == .granted
            ? "Privileged helper metrics are available."
            : "Kernel-adjacent and privileged metrics stay reduced without the helper."
    case .chau7:
        return capability.state == .granted
            ? "AI session grouping and agent-runtime overlays are live."
            : "Agentic development sessions will be grouped heuristically until Chau7 is reachable."
    case .endpointSecurity:
        return capability.state == .granted
            ? "Process lifecycle and short-lived helper lineage are backed by ESF."
            : "Short-lived process lineage and exec/fork/exit attribution will stay weaker without ESF."
    }
}

func capabilityRemediationText(_ capability: CapabilitySnapshot) -> String {
    switch capability.state {
    case .granted:
        return "No action needed."
    case .requested:
        return "Finish the pending macOS prompt, then refresh this capability."
    case .denied:
        return "Grant this permission in macOS Settings, then refresh it here."
    case .unknown:
        return "Request or refresh this capability so Aetower can determine whether richer attribution is available."
    case .unavailable:
        if capability.detail.localizedLowercase.contains("configure") || capability.detail.localizedLowercase.contains("set ") {
            return "Configure the local integration endpoint or helper path, then refresh."
        }
        if capability.detail.localizedLowercase.contains("not found") || capability.detail.localizedLowercase.contains("not detected") {
            return "Install or launch the backing integration, then refresh."
        }
        return "This capability is not available on the current system or configuration."
    }
}

private func topEntityNames<T: Comparable>(snapshot: SystemSnapshot, metric: (EntitySnapshot) -> T) -> String {
    let leaders = snapshot.entities
        .sorted { metric($0) > metric($1) }
        .prefix(3)
        .map(\.displayName)
    if leaders.isEmpty {
        return "No dominant group is currently identified."
    }
    return leaders.joined(separator: ", ")
}

private func operatorDiagnosticsDetail(_ diagnostics: DiagnosticsOverview) -> String {
    if let lastErrorMillis = diagnostics.lastErrorMillis {
        let ageSeconds = max(0, Int((Date().timeIntervalSince1970 * 1000 - Double(lastErrorMillis)) / 1000))
        if ageSeconds < 900 {
            return "Most recent retained error is current."
        }
        return "Most recent retained error is stale (\(ageSeconds / 60)m ago)."
    }
    return "No retained diagnostics errors."
}

private func capabilityOverallSeverity(_ capabilities: [CapabilitySnapshot]) -> OperatorSeverity {
    capabilities
        .map { capability in
            switch capability.state {
            case .denied:
                return OperatorSeverity.warning
            case .requested:
                return OperatorSeverity.info
            case .unknown:
                return OperatorSeverity.info
            case .unavailable:
                return capabilityRemediationText(capability) == "This capability is not available on the current system or configuration."
                    ? .info
                    : .warning
            case .granted:
                return capability.health == .degraded ? .warning : .info
            }
        }
        .max() ?? .info
}

private func capabilityHealthValue(_ capabilities: [CapabilitySnapshot]) -> String {
    let grantedCount = capabilities.filter { $0.state == .granted }.count
    return "\(grantedCount)/\(capabilities.count) live"
}

private func capabilityHealthDetail(_ capabilities: [CapabilitySnapshot]) -> String {
    let degraded = capabilities.filter { $0.health == .degraded || $0.state == .denied || $0.state == .unavailable }
    if degraded.isEmpty {
        return "No blocking capability gaps are currently detected."
    }
    return degraded.prefix(3).map { capabilityKindDisplayName($0.kind) }.joined(separator: ", ")
}

private func operatorHostPressureBand(_ host: HostSnapshot) -> OperatorSeverity {
    let totalBytes = max(Double(host.memoryTotalBytes), 1)
    let usedRatio = Double(host.memoryUsedBytes) / totalBytes
    let compressedRatio = Double(host.compressedMemoryBytes) / totalBytes
    let swapRatio = Double(host.swapUsedBytes) / totalBytes
    if usedRatio >= 0.88 || compressedRatio >= 0.12 || swapRatio >= 0.08 {
        return .critical
    }
    if usedRatio >= 0.75 || compressedRatio >= 0.05 || swapRatio >= 0.02 {
        return .warning
    }
    return .info
}

private func operatorWakeupBand(_ wakeupsPerSecond: Float) -> OperatorSeverity {
    if wakeupsPerSecond >= 3_000 {
        return .critical
    }
    if wakeupsPerSecond >= 1_500 {
        return .warning
    }
    return .info
}

private func operatorHistorySeverity(_ history: HistoryRangeSummary) -> OperatorSeverity {
    if history.storeBytes >= 1024 * 1024 * 1024
        || history.walBytes >= 128 * 1024 * 1024
        || history.quarantineCount >= 128
    {
        return .critical
    }
    if history.storeBytes >= 512 * 1024 * 1024
        || history.walBytes >= 32 * 1024 * 1024
        || history.quarantineCount >= 64
    {
        return .warning
    }
    return .info
}

private func operatorDiagnosticsSeverity(_ diagnostics: DiagnosticsOverview) -> OperatorSeverity {
    if diagnostics.errorCount > 0 {
        if let lastErrorMillis = diagnostics.lastErrorMillis,
           Date().timeIntervalSince1970 * 1000 - Double(lastErrorMillis) > 30 * 60 * 1000
        {
            return .warning
        }
        return .critical
    }
    if diagnostics.warnCount >= 100 {
        return .warning
    }
    return .info
}

private func operatorRuntimeSeverity(_ runtime: RuntimeLagMetrics) -> OperatorSeverity {
    if runtime.engineTickMillis >= 100
        || runtime.snapshotToRenderMillis >= 120
        || runtime.staleMcpHelperCount > 0
    {
        return .critical
    }
    if runtime.engineTickMillis >= 40
        || runtime.snapshotToRenderMillis >= 60
        || runtime.mcpHelperCount >= 4
    {
        return .warning
    }
    return .info
}

private func mcpHelperHealthSeverity(_ runtime: RuntimeLagMetrics) -> OperatorSeverity {
    if runtime.staleMcpHelperCount > 0 || runtime.mcpHelperCount >= 8 {
        return .critical
    }
    if runtime.mcpHelperCount >= 4 {
        return .warning
    }
    return .info
}

private func mcpHelperHealthDetail(_ runtime: RuntimeLagMetrics) -> String {
    if runtime.staleMcpHelperCount > 0 {
        return "\(runtime.staleMcpHelperCount) helper process(es) have been alive for more than 15 minutes. Helpers should exit when clients disconnect."
    }
    if runtime.mcpHelperCount > 0 {
        return "\(runtime.mcpHelperCount) helper process(es) are currently attached to local MCP clients."
    }
    return "The in-app MCP socket is available for local agents."
}

// operatorFormatBytes/Rate/Wakeups removed — use the shared
// formatBytes/formatRate/formatWakeups from MonitorFormatters.swift
// which cache the ByteCountFormatter instead of allocating per call.
