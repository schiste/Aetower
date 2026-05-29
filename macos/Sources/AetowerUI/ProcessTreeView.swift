import SwiftUI
import AetowerBridge

/// Visualizes grouped process hierarchies for an entity and folds in
/// runtime context such as Chau7 sessions when present.
public struct ProcessTreeView: View {
    let entity: EntitySnapshot
    let allEntities: [EntitySnapshot]
    let seedEntities: [EntitySnapshot]

    public init(
        entity: EntitySnapshot,
        allEntities: [EntitySnapshot],
        seedEntities: [EntitySnapshot]? = nil
    ) {
        self.entity = entity
        self.allEntities = allEntities
        self.seedEntities = seedEntities ?? [entity]
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "arrow.triangle.branch")
                    .foregroundStyle(.secondary)
                Text(headerTitle)
                    .font(.headline)
                Spacer()
                Text(summaryLabel)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
            .padding(.bottom, 8)

            ForEach(Array(treeNodes.enumerated()), id: \.offset) { index, node in
                ProcessTreeNode(
                    node: node,
                    isLast: index == treeNodes.count - 1
                )
            }
        }
    }

    private var treeNodes: [TreeNode] {
        buildTree(for: entity, allEntities: allEntities, seedEntities: seedEntities)
    }

    private var headerTitle: String {
        treeNodes.contains(where: { $0.adapterLabel == "chau7" }) ? "Runtime & Friction Groups" : "Process Friction Groups"
    }

    private var summaryLabel: String {
        let groupedProcessCount = seedEntities
            .flatMap(\.components)
            .filter { $0.kind != .adapterContext }
            .count
        let expandedProcessCount = relatedEntities(seedEntities: seedEntities, in: allEntities)
            .flatMap(\.components)
            .filter { $0.kind != .adapterContext }
            .count
        let rootGroupCount = treeNodes.filter { $0.depth == 0 }.count
        if expandedProcessCount > groupedProcessCount {
            return "\(rootGroupCount) groups · \(groupedProcessCount) grouped · \(expandedProcessCount) expanded"
        }
        return "\(rootGroupCount) groups · \(groupedProcessCount) grouped"
    }
}

private struct TreeNode {
    let title: String
    let pid: UInt32?
    let kind: ComponentKind
    let detail: String?
    let depth: Int
    let cpuPercent: Float
    let memoryBytes: UInt64
    let processCount: Int
    let ownerName: String?
    let isPrimaryEntity: Bool
    let user: String?
    let cwd: String?
    let provenance: String?
    let launchedBy: String?
    let adapterLabel: String?
    let statusLabel: String?
    let emphasis: NodeEmphasis
    let isRoot: Bool
}

private enum NodeEmphasis {
    case normal
    case warning
    case critical
}

private struct SubtreeAggregate {
    let cpuPercent: Float
    let memoryBytes: UInt64
    let processCount: Int
    let threadCount: UInt32
}

private struct RelatedComponent {
    let component: ComponentSnapshot
    let ownerName: String
    let isPrimaryEntity: Bool
}

private func buildTree(
    for entity: EntitySnapshot,
    allEntities: [EntitySnapshot],
    seedEntities: [EntitySnapshot]
) -> [TreeNode] {
    let related = relatedEntities(seedEntities: seedEntities, in: allEntities)
    let relatedComponents = related.flatMap { relatedEntity in
        relatedEntity.components.map {
            RelatedComponent(
                component: $0,
                ownerName: relatedEntity.displayName,
                isPrimaryEntity: relatedEntity.entityId == entity.entityId
            )
        }
    }

    let processComponents = relatedComponents.filter { $0.component.kind != .adapterContext }
    let adapterComponents = entity.components.filter { $0.kind == .adapterContext }
    let chau7Components = adapterComponents.filter { $0.adapterContext?.kind == .chau7Session }
    let otherAdapterComponents = adapterComponents.filter { $0.adapterContext?.kind != .chau7Session }

    let pidMap: [UInt32: RelatedComponent] = Dictionary(
        processComponents.compactMap { related in related.component.processId.map { ($0, related) } },
        uniquingKeysWith: { first, _ in first }
    )

    var roots: [RelatedComponent] = []
    var children: [UInt32: [RelatedComponent]] = [:]

    for relatedComponent in processComponents {
        let parentPid = extractParentPid(from: relatedComponent.component.parentSummary)
        if let parentPid, pidMap[parentPid] != nil {
            children[parentPid, default: []].append(relatedComponent)
        } else {
            roots.append(relatedComponent)
        }
    }

    let rootAggregates: [UInt32: SubtreeAggregate] = Dictionary(
        uniqueKeysWithValues: roots.compactMap { root in
            guard let pid = root.component.processId else { return nil }
            return (pid, subtreeAggregate(for: root, children: children))
        }
    )

    roots.sort { lhs, rhs in
        let left = lhs.component.processId.flatMap { rootAggregates[$0] } ?? subtreeAggregate(for: lhs, children: children)
        let right = rhs.component.processId.flatMap { rootAggregates[$0] } ?? subtreeAggregate(for: rhs, children: children)
        return processSort(lhs, left: left, rhs, right: right)
    }

    var nodes: [TreeNode] = []

    if !chau7Components.isEmpty {
        let totalAggregate = aggregateTotals(rootAggregates.values)
        for session in chau7Components {
            nodes.append(makeAdapterNode(
                session,
                aggregate: totalAggregate,
                depth: 0,
                isRoot: true
            ))
            nodes.append(contentsOf: syntheticChau7Nodes(for: session, depth: 1))

            for root in roots {
                appendProcessNode(
                    relatedComponent: root,
                    depth: 1,
                    isRoot: false,
                    children: children,
                    into: &nodes
                )
            }
        }
    } else {
        for root in roots {
            appendProcessNode(
                relatedComponent: root,
                depth: 0,
                isRoot: true,
                children: children,
                into: &nodes
            )
        }
    }

    for component in otherAdapterComponents {
        nodes.append(
            makeAdapterNode(
                component,
                aggregate: SubtreeAggregate(cpuPercent: 0, memoryBytes: 0, processCount: 0, threadCount: 0),
                depth: chau7Components.isEmpty ? 0 : 1,
                isRoot: chau7Components.isEmpty
            )
        )
    }

    return nodes
}

private func appendProcessNode(
    relatedComponent: RelatedComponent,
    depth: Int,
    isRoot: Bool,
    children: [UInt32: [RelatedComponent]],
    into nodes: inout [TreeNode]
) {
    let aggregate = subtreeAggregate(for: relatedComponent, children: children)
    nodes.append(makeProcessNode(relatedComponent, aggregate: aggregate, depth: depth, isRoot: isRoot))

    if let pid = relatedComponent.component.processId, let kids = children[pid] {
        let sortedKids = kids.sorted {
            processSort(
                $0,
                left: subtreeAggregate(for: $0, children: children),
                $1,
                right: subtreeAggregate(for: $1, children: children)
            )
        }
        for child in sortedKids {
            appendProcessNode(
                relatedComponent: child,
                depth: depth + 1,
                isRoot: false,
                children: children,
                into: &nodes
            )
        }
    }
}

private func subtreeAggregate(
    for relatedComponent: RelatedComponent,
    children: [UInt32: [RelatedComponent]]
) -> SubtreeAggregate {
    var totalCPU = relatedComponent.component.cpuPercent
    var totalMemory = relatedComponent.component.memoryBytes
    var totalCount = 1
    var totalThreads = relatedComponent.component.threadCount

    if let pid = relatedComponent.component.processId, let kids = children[pid] {
        for child in kids {
            let aggregate = subtreeAggregate(for: child, children: children)
            totalCPU += aggregate.cpuPercent
            totalMemory += aggregate.memoryBytes
            totalCount += aggregate.processCount
            totalThreads += aggregate.threadCount
        }
    }

    return SubtreeAggregate(
        cpuPercent: totalCPU,
        memoryBytes: totalMemory,
        processCount: totalCount,
        threadCount: totalThreads
    )
}

private func aggregateTotals<S: Sequence>(_ values: S) -> SubtreeAggregate where S.Element == SubtreeAggregate {
    values.reduce(into: SubtreeAggregate(cpuPercent: 0, memoryBytes: 0, processCount: 0, threadCount: 0)) { result, value in
        result = SubtreeAggregate(
            cpuPercent: result.cpuPercent + value.cpuPercent,
            memoryBytes: result.memoryBytes + value.memoryBytes,
            processCount: result.processCount + value.processCount,
            threadCount: result.threadCount + value.threadCount
        )
    }
}

private func makeProcessNode(
    _ relatedComponent: RelatedComponent,
    aggregate: SubtreeAggregate,
    depth: Int,
    isRoot: Bool
) -> TreeNode {
    let component = relatedComponent.component
    return TreeNode(
        title: component.title,
        pid: component.processId,
        kind: component.kind,
        detail: processDetail(component, aggregate: aggregate),
        depth: depth,
        cpuPercent: aggregate.cpuPercent,
        memoryBytes: aggregate.memoryBytes,
        processCount: aggregate.processCount,
        ownerName: relatedComponent.ownerName,
        isPrimaryEntity: relatedComponent.isPrimaryEntity,
        user: component.user,
        cwd: component.cwd,
        provenance: component.provenance.map { "\($0.kind) — \($0.label)" },
        launchedBy: component.launchedBy,
        adapterLabel: nil,
        statusLabel: aggregate.processCount > 1 ? "group" : nil,
        emphasis: processEmphasis(aggregate),
        isRoot: isRoot
    )
}

private func makeAdapterNode(
    _ component: ComponentSnapshot,
    aggregate: SubtreeAggregate,
    depth: Int,
    isRoot: Bool
) -> TreeNode {
    let status = component.adapterContext?.status
    return TreeNode(
        title: component.title,
        pid: component.processId,
        kind: component.kind,
        detail: component.detail.isEmpty ? nil : component.detail,
        depth: depth,
        cpuPercent: aggregate.cpuPercent,
        memoryBytes: aggregate.memoryBytes,
        processCount: aggregate.processCount,
        ownerName: nil,
        isPrimaryEntity: true,
        user: component.user,
        cwd: component.adapterContext?.workspacePath ?? component.cwd,
        provenance: nil,
        launchedBy: nil,
        adapterLabel: adapterLabel(for: component),
        statusLabel: status,
        emphasis: adapterEmphasis(status),
        isRoot: isRoot
    )
}

private func processDetail(_ component: ComponentSnapshot, aggregate: SubtreeAggregate) -> String? {
    var parts: [String] = []
    if aggregate.processCount > 1 {
        parts.append("\(aggregate.processCount) processes in group")
    }
    if aggregate.threadCount > 0 {
        parts.append("\(aggregate.threadCount) threads")
    }
    if !component.detail.isEmpty {
        parts.append(component.detail)
    }
    return parts.isEmpty ? nil : parts.joined(separator: " · ")
}

private func syntheticChau7Nodes(for component: ComponentSnapshot, depth: Int) -> [TreeNode] {
    var nodes: [TreeNode] = []
    let detail = component.detail
    let sessionId = component.adapterContext?.sessionId ?? "session"

    if let activeDuration = capture(detail, matching: #"active ([^·]+)"#) {
        nodes.append(
            TreeNode(
                title: "Active run",
                pid: nil,
                kind: .adapterContext,
                detail: "Current agent run has been active for \(activeDuration).",
                depth: depth,
                cpuPercent: 0,
                memoryBytes: 0,
                processCount: 0,
                ownerName: nil,
                isPrimaryEntity: true,
                user: nil,
                cwd: nil,
                provenance: nil,
                launchedBy: nil,
                adapterLabel: "chau7",
                statusLabel: nil,
                emphasis: .normal,
                isRoot: false
            )
        )
    }

    if let turnCount = capture(detail, matching: #"(\d+) turns"#) {
        nodes.append(
            TreeNode(
                title: "Turn history",
                pid: nil,
                kind: .adapterContext,
                detail: "\(turnCount) recorded turns for session \(sessionId).",
                depth: depth,
                cpuPercent: 0,
                memoryBytes: 0,
                processCount: 0,
                ownerName: nil,
                isPrimaryEntity: true,
                user: nil,
                cwd: nil,
                provenance: nil,
                launchedBy: nil,
                adapterLabel: "chau7",
                statusLabel: nil,
                emphasis: .normal,
                isRoot: false
            )
        )
    }

    if let childSessions = capture(detail, matching: #"(\d+) child sessions"#) {
        nodes.append(
            TreeNode(
                title: "Delegated child sessions",
                pid: nil,
                kind: .adapterContext,
                detail: "\(childSessions) Chau7 child session(s) are currently linked to this runtime.",
                depth: depth,
                cpuPercent: 0,
                memoryBytes: 0,
                processCount: Int(childSessions) ?? 0,
                ownerName: nil,
                isPrimaryEntity: true,
                user: nil,
                cwd: nil,
                provenance: nil,
                launchedBy: nil,
                adapterLabel: "chau7",
                statusLabel: "delegating",
                emphasis: .warning,
                isRoot: false
            )
        )
    }

    if let approval = statusApprovalDetail(from: component.detail) {
        nodes.append(
            TreeNode(
                title: "Pending approval",
                pid: nil,
                kind: .adapterContext,
                detail: approval,
                depth: depth,
                cpuPercent: 0,
                memoryBytes: 0,
                processCount: 0,
                ownerName: nil,
                isPrimaryEntity: true,
                user: nil,
                cwd: nil,
                provenance: nil,
                launchedBy: nil,
                adapterLabel: "chau7",
                statusLabel: "approval-needed",
                emphasis: .critical,
                isRoot: false
            )
        )
    }

    return nodes
}

private func processSort(
    _ lhs: RelatedComponent,
    left: SubtreeAggregate,
    _ rhs: RelatedComponent,
    right: SubtreeAggregate
) -> Bool {
    if left.cpuPercent != right.cpuPercent {
        return left.cpuPercent > right.cpuPercent
    }
    if left.memoryBytes != right.memoryBytes {
        return left.memoryBytes > right.memoryBytes
    }
    return lhs.component.title.localizedCaseInsensitiveCompare(rhs.component.title) == .orderedAscending
}

private func relatedEntities(
    seedEntities: [EntitySnapshot],
    in allEntities: [EntitySnapshot]
) -> [EntitySnapshot] {
    var includedIDs = Set(seedEntities.map(\.entityId))
    var includedPIDs = Set(seedEntities.flatMap { $0.components.compactMap(\.processId) })

    func sessionIDs(for candidate: EntitySnapshot) -> Set<String> {
        var result = Set<String>()
        for badge in candidate.badges where badge.hasPrefix("ai-session:") {
            result.insert(String(badge.dropFirst("ai-session:".count)))
        }
        for component in candidate.components {
            if let sessionID = component.adapterContext?.sessionId {
                result.insert(sessionID)
            }
        }
        return result
    }

    func repoRoots(for candidate: EntitySnapshot) -> [String] {
        candidate.components.compactMap {
            $0.adapterContext?.repoRoot ?? $0.adapterContext?.workspacePath
        }
    }

    let selectedSessionIDs = seedEntities.reduce(into: Set<String>()) { result, entity in
        result.formUnion(sessionIDs(for: entity))
    }
    let selectedRepoRoots = seedEntities.flatMap(repoRoots(for:))

    var changed = true
    while changed {
        changed = false
        for candidate in allEntities where !includedIDs.contains(candidate.entityId) {
            let isChildByPID = candidate.components.contains { component in
                guard let parentPID = extractParentPid(from: component.parentSummary) else { return false }
                return includedPIDs.contains(parentPID)
            }

            let sharesChau7Context =
                candidate.badges.contains("chau7-live")
                && (
                    !selectedSessionIDs.isDisjoint(with: sessionIDs(for: candidate))
                    || candidate.components.contains { component in
                        let paths = [component.cwd, component.executablePath, component.adapterContext?.workspacePath, component.adapterContext?.repoRoot]
                            .compactMap { $0 }
                        return paths.contains { path in
                            selectedRepoRoots.contains { root in path.hasPrefix(root) }
                        }
                    }
                )

            if isChildByPID || sharesChau7Context {
                includedIDs.insert(candidate.entityId)
                includedPIDs.formUnion(candidate.components.compactMap(\.processId))
                changed = true
            }
        }
    }

    return allEntities.filter { includedIDs.contains($0.entityId) }
}

private func processEmphasis(_ aggregate: SubtreeAggregate) -> NodeEmphasis {
    if aggregate.cpuPercent >= 50 || aggregate.memoryBytes >= 1_500_000_000 {
        return .critical
    }
    if aggregate.cpuPercent >= 10 || aggregate.memoryBytes >= 400_000_000 {
        return .warning
    }
    return .normal
}

private func adapterEmphasis(_ status: String?) -> NodeEmphasis {
    let normalized = status?.lowercased() ?? ""
    if normalized.contains("approval") || normalized.contains("error") {
        return .critical
    }
    if normalized.contains("delegat") || normalized.contains("waiting") || normalized.contains("loading") {
        return .warning
    }
    return .normal
}

private func adapterLabel(for component: ComponentSnapshot) -> String? {
    switch component.adapterContext?.kind {
    case .chau7Session:
        return "chau7"
    case .chromiumTab:
        return "chromium"
    case .dockerContainer:
        return "docker"
    case .privilegedSocket:
        return "helper"
    case .vsCodeWorkspace, .vsCodeRuntime:
        return "vscode"
    case .unknown, .none:
        return component.kind == .adapterContext ? "adapter" : nil
    }
}

private func capture(_ source: String?, matching pattern: String) -> String? {
    guard let source, let regex = try? NSRegularExpression(pattern: pattern) else { return nil }
    let range = NSRange(source.startIndex..<source.endIndex, in: source)
    guard
        let match = regex.firstMatch(in: source, options: [], range: range),
        match.numberOfRanges > 1,
        let captureRange = Range(match.range(at: 1), in: source)
    else {
        return nil
    }
    return String(source[captureRange])
}

private func statusApprovalDetail(from detail: String?) -> String? {
    guard let detail else { return nil }
    let marker = "permission to"
    if let range = detail.range(of: marker, options: .caseInsensitive) {
        return String(detail[range.lowerBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
    }
    return nil
}

private func extractParentPid(from parentSummary: String?) -> UInt32? {
    guard let summary = parentSummary else { return nil }
    guard let pidRange = summary.range(of: "pid ") else { return nil }
    let afterPid = summary[pidRange.upperBound...]
    let digits = afterPid.prefix(while: \.isNumber)
    return UInt32(digits)
}

private func formatBytesCompact(_ bytes: UInt64) -> String {
    let formatter = ByteCountFormatter()
    formatter.allowedUnits = [.useKB, .useMB, .useGB]
    formatter.countStyle = .binary
    formatter.includesUnit = true
    formatter.isAdaptive = true
    return formatter.string(fromByteCount: Int64(bytes))
}

private struct ProcessTreeNode: View {
    let node: TreeNode
    let isLast: Bool

    var body: some View {
        HStack(spacing: 0) {
            ForEach(0..<node.depth, id: \.self) { _ in
                HStack(spacing: 0) {
                    Rectangle()
                        .fill(Color.secondary.opacity(0.15))
                        .frame(width: 1)
                    Spacer()
                }
                .frame(width: 20)
            }

            if node.depth > 0 {
                HStack(spacing: 0) {
                    Path { path in
                        path.move(to: CGPoint(x: 0, y: 0))
                        path.addLine(to: CGPoint(x: 0, y: 10))
                        path.addLine(to: CGPoint(x: 10, y: 10))
                    }
                    .stroke(Color.secondary.opacity(0.2), lineWidth: 1)
                    .frame(width: 12, height: 20)
                }
            }

            Image(systemName: nodeIcon)
                .font(.system(size: 10))
                .foregroundStyle(nodeColor)
                .frame(width: 16)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(node.title)
                        .font(.system(size: 11, weight: node.isRoot ? .semibold : .regular))
                        .lineLimit(1)

                    if let pid = node.pid {
                        Text("\(pid)")
                            .font(.system(size: 9, design: .monospaced))
                            .foregroundStyle(.tertiary)
                    }

                    if let adapter = node.adapterLabel {
                        Text(adapter)
                            .font(.system(size: 8, weight: .medium))
                            .foregroundStyle(.blue)
                            .padding(.horizontal, 4)
                            .padding(.vertical, 1)
                            .background(.blue.opacity(0.1), in: Capsule())
                    }

                    if let status = node.statusLabel {
                        Text(status)
                            .font(.system(size: 8, weight: .medium, design: .monospaced))
                            .foregroundStyle(statusColor)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 1)
                            .background(statusColor.opacity(0.1), in: Capsule())
                    }

                    Spacer()

                    if node.cpuPercent > 0.1 {
                        Text(String(format: "%.1f%%", node.cpuPercent))
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }

                    if node.memoryBytes > 0 {
                        Text(formatBytesCompact(node.memoryBytes))
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                }

                if let metadata = metadataLine {
                    Text(metadata)
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }

                if let detail = node.detail, !detail.isEmpty {
                    Text(detail)
                        .font(.system(size: 9))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.leading, 4)
            .padding(.vertical, 4)
        }
        .padding(.horizontal, 4)
    }

    private var nodeIcon: String {
        switch node.kind {
        case .process: return "circle.fill"
        case .command: return "terminal.fill"
        case .adapterContext:
            if node.adapterLabel == "chau7" {
                return "sparkles.rectangle.stack"
            }
            return "link"
        }
    }

    private var nodeColor: Color {
        switch node.emphasis {
        case .critical: return .red
        case .warning: return .orange
        case .normal:
            switch node.kind {
            case .adapterContext: return .blue
            case .command: return .purple
            case .process: return .secondary
            }
        }
    }

    private var statusColor: Color {
        switch node.emphasis {
        case .critical: return .red
        case .warning: return .orange
        case .normal: return .secondary
        }
    }

    private var metadataLine: String? {
        var parts: [String] = []
        if node.processCount > 1 {
            parts.append("\(node.processCount) proc group")
        }
        if let ownerName = node.ownerName, !node.isPrimaryEntity {
            parts.append(ownerName)
        }
        if let user = node.user { parts.append(user) }
        if let cwd = node.cwd {
            let short = cwd.split(separator: "/").last.map(String.init) ?? cwd
            parts.append(short)
        }
        if let provenance = node.provenance { parts.append(provenance) }
        if let launchedBy = node.launchedBy { parts.append("by \(launchedBy)") }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}
