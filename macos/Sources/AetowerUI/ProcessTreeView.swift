import SwiftUI
import AetowerBridge

/// Visualizes the parent→child process hierarchy for an entity,
/// enriched with adapter context from Chau7, Docker, Chromium, etc.
public struct ProcessTreeView: View {
    let entity: EntitySnapshot

    public init(entity: EntitySnapshot) { self.entity = entity }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 8) {
                Image(systemName: "arrow.triangle.branch")
                    .foregroundStyle(.secondary)
                Text("Process Tree")
                    .font(.headline)
                Spacer()
                Text("\(entity.components.count) processes")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
            .padding(.bottom, 8)

            // Tree nodes
            ForEach(Array(treeNodes.enumerated()), id: \.offset) { index, node in
                ProcessTreeNode(
                    node: node,
                    isLast: index == treeNodes.count - 1
                )
            }
        }
    }

    private var treeNodes: [TreeNode] {
        buildTree(from: entity.components)
    }
}

// MARK: - Tree Data Model

private struct TreeNode {
    let title: String
    let pid: UInt32?
    let kind: ComponentKind
    let detail: String
    let depth: Int
    let cpuPercent: Float
    let memoryBytes: UInt64
    let user: String?
    let cwd: String?
    let provenance: String?
    let adapterLabel: String?
    let isRoot: Bool
}

private func buildTree(from components: [ComponentSnapshot]) -> [TreeNode] {
    // Build a map of PID → component for parent lookup
    let pidMap: [UInt32: ComponentSnapshot] = Dictionary(
        components.compactMap { c in c.processId.map { ($0, c) } },
        uniquingKeysWith: { first, _ in first }
    )

    // Sort: root processes first (those whose parent isn't in the list),
    // then by CPU descending
    var roots: [ComponentSnapshot] = []
    var children: [UInt32: [ComponentSnapshot]] = [:]

    for component in components {
        if component.kind == .adapterContext {
            // Adapter contexts attach to their parent entity, shown as enrichment
            continue
        }

        let parentPid = extractParentPid(from: component.parentSummary)
        if let parentPid, pidMap[parentPid] != nil {
            children[parentPid, default: []].append(component)
        } else {
            roots.append(component)
        }
    }

    // Build flat tree with depth tracking
    var nodes: [TreeNode] = []
    for root in roots.sorted(by: { $0.cpuPercent > $1.cpuPercent }) {
        appendNode(component: root, depth: 0, isRoot: true, components: components, children: children, into: &nodes)
    }

    // Append adapter contexts as enrichment at the end
    for component in components where component.kind == .adapterContext {
        nodes.append(TreeNode(
            title: component.title,
            pid: component.processId,
            kind: component.kind,
            detail: component.detail,
            depth: 1,
            cpuPercent: component.cpuPercent,
            memoryBytes: component.memoryBytes,
            user: component.user,
            cwd: component.cwd,
            provenance: component.provenance.map { "\($0.kind) — \($0.label)" },
            adapterLabel: "adapter",
            isRoot: false
        ))
    }

    return nodes
}

private func appendNode(
    component: ComponentSnapshot,
    depth: Int,
    isRoot: Bool,
    components: [ComponentSnapshot],
    children: [UInt32: [ComponentSnapshot]],
    into nodes: inout [TreeNode]
) {
    let adapterContext = components.first {
        $0.kind == .adapterContext && $0.processId == component.processId
    }

    nodes.append(TreeNode(
        title: component.title,
        pid: component.processId,
        kind: component.kind,
        detail: component.detail,
        depth: depth,
        cpuPercent: component.cpuPercent,
        memoryBytes: component.memoryBytes,
        user: component.user,
        cwd: component.cwd,
        provenance: component.provenance.map { "\($0.kind) — \($0.label)" },
        adapterLabel: adapterContext?.title,
        isRoot: isRoot
    ))

    if let pid = component.processId, let kids = children[pid] {
        for child in kids.sorted(by: { $0.cpuPercent > $1.cpuPercent }) {
            appendNode(
                component: child,
                depth: depth + 1,
                isRoot: false,
                components: components,
                children: children,
                into: &nodes
            )
        }
    }
}

private func extractParentPid(from parentSummary: String?) -> UInt32? {
    // parentSummary format: "launchd (pid 1)" or "zsh (pid 12345)"
    guard let summary = parentSummary else { return nil }
    guard let pidRange = summary.range(of: "pid ") else { return nil }
    let afterPid = summary[pidRange.upperBound...]
    let digits = afterPid.prefix(while: \.isNumber)
    return UInt32(digits)
}

// MARK: - Tree Node View

private struct ProcessTreeNode: View {
    let node: TreeNode
    let isLast: Bool

    var body: some View {
        HStack(spacing: 0) {
            // Indentation with tree lines
            ForEach(0..<node.depth, id: \.self) { _ in
                HStack(spacing: 0) {
                    Rectangle()
                        .fill(Color.secondary.opacity(0.15))
                        .frame(width: 1)
                    Spacer()
                }
                .frame(width: 20)
            }

            // Branch connector
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

            // Node icon
            Image(systemName: nodeIcon)
                .font(.system(size: 10))
                .foregroundStyle(nodeColor)
                .frame(width: 16)

            // Node content
            VStack(alignment: .leading, spacing: 1) {
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

                    Spacer()

                    // Metrics
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

                // Detail line: user, cwd, provenance
                if let detail = detailLine {
                    Text(detail)
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
            }
            .padding(.leading, 4)
            .padding(.vertical, 3)
        }
        .padding(.horizontal, 4)
    }

    private var nodeIcon: String {
        switch node.kind {
        case .process: return "circle.fill"
        case .command: return "terminal.fill"
        case .adapterContext: return "link"
        }
    }

    private var nodeColor: Color {
        switch node.kind {
        case .adapterContext: return .blue
        case .command: return .purple
        case .process:
            if node.cpuPercent > 50 { return .red }
            if node.cpuPercent > 10 { return .orange }
            return .secondary
        }
    }

    private var detailLine: String? {
        var parts: [String] = []
        if let user = node.user { parts.append(user) }
        if let cwd = node.cwd {
            let short = cwd.split(separator: "/").last.map(String.init) ?? cwd
            parts.append(short)
        }
        if let provenance = node.provenance { parts.append(provenance) }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

private func formatBytesCompact(_ bytes: UInt64) -> String {
    if bytes >= 1_073_741_824 {
        return String(format: "%.1fG", Double(bytes) / 1_073_741_824)
    } else if bytes >= 1_048_576 {
        return String(format: "%.0fM", Double(bytes) / 1_048_576)
    } else {
        return String(format: "%.0fK", Double(bytes) / 1024)
    }
}
