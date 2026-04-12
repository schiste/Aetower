import SwiftUI
import AetowerBridge

private struct ReasonPill: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(Color.secondary.opacity(0.08), in: Capsule())
    }
}

private struct SectionEyebrow: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.caption.weight(.semibold))
            .foregroundStyle(.tertiary)
            .tracking(0.8)
    }
}

private struct MachineBandMetric: View {
    let title: String
    let value: String
    let tone: Color
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 16, weight: .semibold, design: .rounded))
                .lineLimit(1)
            Text(subtitle)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(minWidth: 138, alignment: .leading)
        .background(tone.opacity(0.08), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(tone.opacity(0.10), lineWidth: 1)
        )
    }
}

private struct OperatorIncidentBanner: View {
    let incident: HostIncidentSummary

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Image(systemName: incident.severity == .critical ? "exclamationmark.triangle.fill" : "waveform.path.ecg")
                    .foregroundStyle(incident.severity.color)
                Text(incident.title)
                    .font(.headline)
                Spacer()
                Text(incident.severity.label)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(incident.severity.color)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(incident.severity.color.opacity(0.12), in: Capsule())
            }
            Text(incident.summary)
                .font(.subheadline)
            Text(incident.action)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(incident.severity.color.opacity(0.08), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(incident.severity.color.opacity(0.18), lineWidth: 1)
        )
    }
}

private struct OperatorSectionCard<Content: View>: View {
    let title: String
    let content: Content

    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .font(.headline)
            content
        }
        .padding(14)
        .frame(width: 320, alignment: .leading)
        .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

private struct InlineMetric: View {
    let title: String
    let value: String
    let isHighlighted: Bool

    var body: some View {
        HStack(spacing: 5) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .background(Color.secondary.opacity(0.06), in: Capsule())
        .overlay(
            Capsule()
                .stroke(
                    isHighlighted ? AetowerDesign.Status.error.opacity(0.9) : .clear,
                    lineWidth: isHighlighted ? 1 : 0
                )
        )
    }
}


private enum SortKey: String, CaseIterable, Identifiable {
    case friction
    case cpu
    case memory
    case disk
    case network
    case energy
    case alphabeticalAsc
    case alphabeticalDesc
    case oldestFirst
    case newestFirst

    var id: String { rawValue }

    var title: String {
        switch self {
        case .friction: return "Friction"
        case .cpu: return "CPU"
        case .memory: return "Memory"
        case .disk: return "Disk"
        case .network: return "Network"
        case .energy: return "Energy"
        case .alphabeticalAsc: return "A - Z"
        case .alphabeticalDesc: return "Z - A"
        case .oldestFirst: return "Oldest first"
        case .newestFirst: return "Newest first"
        }
    }

    var tone: Color {
        switch self {
        case .friction: return .orange
        case .cpu: return .blue
        case .memory: return .green
        case .disk: return .pink
        case .network: return .teal
        case .energy: return .yellow
        case .alphabeticalAsc, .alphabeticalDesc, .oldestFirst, .newestFirst:
            return .gray
        }
    }

    /// Sort keys that have a meaningful grouped-mode sort implementation.
    /// Keys that return false silently degrade to friction sort in
    /// `sortGroups` — the UI should disable or badge them so the user
    /// knows their selection is not applied.
    var supportsGroupedMode: Bool {
        switch self {
        case .friction, .cpu, .memory, .alphabeticalAsc, .alphabeticalDesc:
            return true
        case .disk, .network, .energy, .oldestFirst, .newestFirst:
            return false
        }
    }

    var usesMetricValue: Bool {
        switch self {
        case .friction, .cpu, .memory, .disk, .network, .energy:
            return true
        case .alphabeticalAsc, .alphabeticalDesc, .oldestFirst, .newestFirst:
            return false
        }
    }
}

private enum ListMode: String, CaseIterable, Identifiable {
    case grouped
    case flat

    var id: String { rawValue }

    var title: String {
        switch self {
        case .grouped: return "Grouped"
        case .flat: return "Flat"
        }
    }

    var icon: String {
        switch self {
        case .grouped: return "square.grid.2x2"
        case .flat: return "list.bullet"
        }
    }
}

private struct SortChip: View {
    let title: String
    let tone: Color

    var body: some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.caption.weight(.semibold))
            Image(systemName: "chevron.down")
                .font(.caption2.weight(.semibold))
        }
        .foregroundStyle(.primary)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(Color.secondary.opacity(0.08), in: Capsule())
        .overlay(
            Capsule()
                .stroke(tone.opacity(0.16), lineWidth: 1)
        )
    }
}

private struct RowSignalBadge: View {
    let valueText: String?
    let title: String
    let tone: Color
    let showsForegroundDot: Bool
    let isHighlighted: Bool

    var body: some View {
        HStack(spacing: 8) {
            if let valueText {
                Text(valueText)
                    .font(.caption.weight(.semibold))
                    .monospacedDigit()
                    .foregroundStyle(.white)
            }

            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(1)

            if showsForegroundDot {
                Circle()
                    .fill(.white.opacity(0.92))
                    .frame(width: 6, height: 6)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(
            LinearGradient(
                colors: [tone.opacity(0.95), tone.opacity(0.65)],
                startPoint: .leading,
                endPoint: .trailing
            ),
            in: Capsule()
        )
        .overlay(
            Capsule()
                .stroke(
                    isHighlighted ? AetowerDesign.Status.error.opacity(0.9) : .clear,
                    lineWidth: isHighlighted ? 1 : 0
                )
        )
    }
}

private struct RowFrictionHighlights {
    let title: Bool
    let cpu: Bool
    let memory: Bool
    let disk: Bool
    let network: Bool
    let wakeups: Bool

    static let none = Self(title: false, cpu: false, memory: false, disk: false, network: false, wakeups: false)
}

private struct EntityRow: View {
    let entity: EntitySnapshot
    let isSelected: Bool
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: 6) {
            // Entity type icon
            Image(systemName: entityIcon)
                .font(.system(size: 11))
                .foregroundStyle(AetowerDesign.frictionColor(entity.friction.totalScore).opacity(0.8))
                .frame(width: 16)

            // Entity name (compact)
            Text(entity.displayName)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)

            // Badges
            if entity.entityKind == .aiAgent {
                Text(aiAgentProviderLabel)
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(aiAgentDotColor)
                    .padding(.horizontal, 4)
                    .padding(.vertical, 1)
                    .background(aiAgentDotColor.opacity(0.15), in: Capsule())
            }

            if entity.metrics.isForeground {
                Circle().fill(.blue).frame(width: 5, height: 5)
            }

            if entity.anomalyDetected {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 9))
                    .foregroundStyle(.orange)
                    .symbolEffect(.pulse.wholeSymbol, isActive: true)
            }

            // Metrics — right aligned, fixed width columns
            Text(String(format: "%.1f%%", entity.metrics.cpuPercent))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 48, alignment: .trailing)

            Text(formatBytes(entity.metrics.memoryResidentBytes))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 52, alignment: .trailing)

            // User — from first component
            Text(entityUser)
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: 60, alignment: .trailing)

            // Parent — from first component
            Text(entityParent)
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: 80, alignment: .trailing)

            // Friction score — bold, colored
            HStack(spacing: 2) {
                let trend = AetowerDesign.trendArrow(entity.trend.friction)
                Image(systemName: trend.symbol)
                    .font(.system(size: 8))
                    .foregroundStyle(trend.color)
                Text(String(format: "%.1f", entity.friction.totalScore))
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(entity.friction.totalScore))
                    .contentTransition(.numericText())
            }
            .frame(width: 50, alignment: .trailing)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity)
        .background(
            AetowerDesign.frictionColor(entity.friction.totalScore)
                .opacity(frictionBackgroundOpacity),
            in: RoundedRectangle(cornerRadius: 6, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .stroke(
                    isSelected ? Color.accentColor.opacity(0.5) : .clear,
                    lineWidth: 1
                )
        )
        .onHover { isHovered = $0 }
        .help(rowHelpText)
        .animation(AetowerDesign.Motion.quick, value: isHovered)
        .contextMenu {
            Button("Copy Process IDs") {
                let pids = entity.components.compactMap(\.processId).map(String.init).joined(separator: ", ")
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(pids, forType: .string)
            }
            if entity.executablePath?.contains(".app/") == true {
                Button("Show in Finder") {
                    if let path = entity.executablePath {
                        NSWorkspace.shared.selectFile(path, inFileViewerRootedAtPath: "")
                    }
                }
            }
            Button("Copy Name") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(entity.displayName, forType: .string)
            }
            Divider()
            Button("Terminate Processes", role: .destructive) {
                for component in entity.components {
                    if let pid = component.processId {
                        kill(Int32(pid), SIGTERM)
                    }
                }
            }
        }
    }

    private var aiAgentDotColor: Color {
        if entity.badges.contains("claude") { return .blue }
        if entity.badges.contains("codex") { return .green }
        if entity.badges.contains("chatgpt") { return .orange }
        return .purple
    }

    private var aiAgentProviderLabel: String {
        if entity.badges.contains("claude") { return "Claude" }
        if entity.badges.contains("codex") { return "Codex" }
        if entity.badges.contains("chatgpt") { return "ChatGPT" }
        return "AI"
    }

    private var entityUser: String {
        entity.components
            .lazy
            .compactMap(\.user)
            .first(where: { !$0.isEmpty }) ?? ""
    }

    private var rowHelpText: String {
        [
            entity.recentChangeSummary,
            entity.launcherSummary.map { "Launch lineage: \($0)" },
            entity.attributionNotes.first,
        ]
        .compactMap { $0 }
        .joined(separator: "\n")
    }

    private var entityParent: String {
        entity.components.first?.parentSummary ?? ""
    }

    private var entityIcon: String {
        switch entity.entityKind {
        case .app: return "app.fill"
        case .browser: return "globe"
        case .daemon: return "gearshape.2.fill"
        case .terminalSession: return "terminal.fill"
        case .aiAgent: return "cpu.fill"
        case .service: return "server.rack"
        case .unknown: return "questionmark.circle"
        }
    }

    private var frictionBackgroundOpacity: Double {
        let base = min(Double(entity.friction.totalScore) / 100.0, 1.0)
        if isSelected { return base * 0.12 + 0.04 }
        if isHovered { return base * 0.08 + 0.02 }
        return base * 0.05
    }

    private func metricPill(_ label: String, _ value: String) -> some View {
        HStack(spacing: 2) {
            Text(label)
                .font(.system(size: 9, weight: .medium))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption2.monospacedDigit().weight(.medium))
                .foregroundStyle(.secondary)
        }
    }
}

private struct EntityGroup {
    let root: EntitySnapshot
    let members: [EntitySnapshot]
    let cpuPercent: Float
    let memoryBytes: UInt64
    let frictionScore: Float
    let processCount: Int
    let userSummary: String

    var id: String { root.entityId }
}

private struct GroupingCacheKey: Hashable {
    let sequence: UInt64
    let query: String
    let sortKey: SortKey
}

private struct GroupedEntityRow: View {
    let group: EntityGroup
    let isSelected: Bool
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: entityIcon)
                .font(.system(size: 11))
                .foregroundStyle(AetowerDesign.frictionColor(group.frictionScore).opacity(0.8))
                .frame(width: 16)

            Text(group.root.displayName)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)

            if group.members.count > 1 {
                Text("+\(group.members.count - 1)")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(Color.secondary.opacity(0.12), in: Capsule())
            }

            Text(String(format: "%.1f%%", group.cpuPercent))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 48, alignment: .trailing)

            Text(formatBytes(group.memoryBytes))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 52, alignment: .trailing)

            Text(group.userSummary)
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: 60, alignment: .trailing)

            Text(groupSummaryText)
                .font(.system(size: 10))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: 80, alignment: .trailing)

            HStack(spacing: 2) {
                Image(systemName: "arrow.triangle.branch")
                    .font(.system(size: 8))
                    .foregroundStyle(.secondary)
                Text(String(format: "%.1f", group.frictionScore))
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(group.frictionScore))
                    .contentTransition(.numericText())
            }
            .frame(width: 50, alignment: .trailing)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity)
        .background(
            AetowerDesign.frictionColor(group.frictionScore)
                .opacity(frictionBackgroundOpacity),
            in: RoundedRectangle(cornerRadius: 6, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .stroke(
                    isSelected ? Color.accentColor.opacity(0.5) : .clear,
                    lineWidth: 1
                )
        )
        .onHover { isHovered = $0 }
        .help(helpText)
        .animation(AetowerDesign.Motion.quick, value: isHovered)
    }

    private var entityIcon: String {
        switch group.root.entityKind {
        case .app: return "app.fill"
        case .browser: return "globe"
        case .daemon: return "gearshape.2.fill"
        case .terminalSession: return "terminal.fill"
        case .aiAgent: return "cpu.fill"
        case .service: return "server.rack"
        case .unknown: return "questionmark.circle"
        }
    }

    private var helpText: String {
        let names = group.members.prefix(6).map(\.displayName).joined(separator: ", ")
        if group.members.count > 6 {
            return "Collapsed entities: \(names), +\(group.members.count - 6) more · \(group.processCount) grouped processes"
        }
        if group.members.count > 1 {
            return "Collapsed entities: \(names) · \(group.processCount) grouped processes"
        }
        return "Entity footprint: \(group.processCount) grouped processes"
    }

    private var groupSummaryText: String {
        let entityCount = group.members.count
        let entityText = entityCount == 1 ? "1 ent" : "\(entityCount) ent"
        if entityCount <= 1 {
            return entityText
        }
        if group.processCount <= 1 {
            return entityText
        }
        let processText = group.processCount == 1 ? "1 proc" : "\(group.processCount) proc"
        return "\(entityText) · \(processText)"
    }

    private var frictionBackgroundOpacity: Double {
        let base = min(Double(group.frictionScore) / 100.0, 1.0)
        if isSelected { return base * 0.12 + 0.04 }
        if isHovered { return base * 0.08 + 0.02 }
        return base * 0.05
    }
}

private func buildEntityGroups(from entities: [EntitySnapshot]) -> [EntityGroup] {
    let entityByID = Dictionary(uniqueKeysWithValues: entities.map { ($0.entityId, $0) })
    let pidToEntityID = Dictionary(
        uniqueKeysWithValues: entities.flatMap { entity in
            entity.components.compactMap { component in
                component.processId.map { ($0, entity.entityId) }
            }
        }
    )

    func sessionIDs(for entity: EntitySnapshot) -> Set<String> {
        var ids = Set<String>()
        for badge in entity.badges where badge.hasPrefix("ai-session:") {
            ids.insert(String(badge.dropFirst("ai-session:".count)))
        }
        for component in entity.components {
            if let sessionID = component.adapterContext?.sessionId {
                ids.insert(sessionID)
            }
        }
        return ids
    }

    func workspaceHints(for entity: EntitySnapshot) -> Set<String> {
        var hints = Set<String>()
        for component in entity.components {
            if let value = component.adapterContext?.repoRoot { hints.insert(value) }
            if let value = component.adapterContext?.workspacePath { hints.insert(value) }
            if let value = component.cwd { hints.insert(value) }
        }
        if let executablePath = entity.executablePath {
            hints.insert((executablePath as NSString).deletingLastPathComponent)
        }
        return hints
    }

    func primaryUser(for entity: EntitySnapshot) -> String? {
        entity.components
            .lazy
            .compactMap(\.user)
            .first(where: { !$0.isEmpty })
    }

    func groupUserSummary(for members: [EntitySnapshot]) -> String {
        let users = members
            .compactMap { primaryUser(for: $0) }
            .filter { !$0.isEmpty }

        guard !users.isEmpty else { return "" }

        let counts = Dictionary(users.map { ($0, 1) }, uniquingKeysWith: +)
        let sorted = counts.sorted {
            if $0.value != $1.value {
                return $0.value > $1.value
            }
            return $0.key.localizedCaseInsensitiveCompare($1.key) == .orderedAscending
        }

        guard let dominant = sorted.first else { return "" }
        if sorted.count == 1 || dominant.value == users.count {
            return dominant.key
        }
        return "\(dominant.key)+"
    }

    func isGenericSystemRoot(_ entity: EntitySnapshot) -> Bool {
        let name = entity.displayName.localizedLowercase
        if [
            "launchd",
            "loginwindow",
            "xpcproxy",
            "kernel_task",
            "runningboardd",
            "distnoted",
            "cfprefsd"
        ].contains(name) {
            return true
        }
        return entity.entityKind == .daemon && entity.components.allSatisfy {
            ($0.parentSummary?.localizedLowercase.contains("launchd") ?? false)
        }
    }

    func sharesStrongContext(_ lhs: EntitySnapshot, _ rhs: EntitySnapshot) -> Bool {
        let sharedSessions = !sessionIDs(for: lhs).isDisjoint(with: sessionIDs(for: rhs))
        if sharedSessions {
            return true
        }

        let lhsHints = workspaceHints(for: lhs)
        let rhsHints = workspaceHints(for: rhs)
        let sharedHint = !lhsHints.isDisjoint(with: rhsHints)

        let lhsUser = primaryUser(for: lhs)
        let sameUser = lhsUser != nil && lhsUser == primaryUser(for: rhs)
        let launchDelta = abs(Int64(lhs.oldestProcessStartMillis) - Int64(rhs.oldestProcessStartMillis))
        let closeLaunch = launchDelta > 0 && launchDelta <= 120_000

        return sharedHint || (sameUser && closeLaunch)
    }

    let sessionRoots: [String: String] = {
        var roots: [String: String] = [:]
        for entity in entities {
            let ids = sessionIDs(for: entity)
            guard !ids.isEmpty else { continue }
            let isChau7Root =
                entity.displayName.localizedCaseInsensitiveContains("chau7")
                || (entity.badges.contains("chau7-live") && entity.entityKind != .aiAgent)
            guard isChau7Root else { continue }
            for id in ids {
                roots[id] = entity.entityId
            }
        }
        return roots
    }()

    func firstParentEntityID(for entity: EntitySnapshot) -> String? {
        let parentCandidates = entity.components.compactMap { component -> String? in
            guard
                let parentSummary = component.parentSummary,
                let parentPID = extractParentPIDForGrouping(from: parentSummary),
                let ownerID = pidToEntityID[parentPID],
                ownerID != entity.entityId
            else {
                return nil
            }
            return ownerID
        }
        for ownerID in parentCandidates {
            guard let owner = entityByID[ownerID] else { continue }
            if isGenericSystemRoot(owner) {
                continue
            }
            if sharesStrongContext(entity, owner) {
                return ownerID
            }
        }
        return nil
    }

    func rootID(for entity: EntitySnapshot) -> String {
        let ids = sessionIDs(for: entity)
        for id in ids {
            if let root = sessionRoots[id] {
                return root
            }
        }

        var visited = Set<String>()
        var current = entity.entityId
        while let currentEntity = entityByID[current],
              let parentID = firstParentEntityID(for: currentEntity),
              !visited.contains(parentID) {
            visited.insert(current)
            current = parentID
        }
        return current
    }

    let grouped = Dictionary(grouping: entities) { rootID(for: $0) }
    return grouped.compactMap { rootID, members in
        guard let root = entityByID[rootID] ?? members.first else { return nil }
        return EntityGroup(
            root: root,
            members: members.sorted { $0.friction.totalScore > $1.friction.totalScore },
            cpuPercent: members.reduce(0) { $0 + $1.metrics.cpuPercent },
            memoryBytes: members.reduce(0) { $0 + $1.metrics.memoryResidentBytes },
            frictionScore: members.reduce(0) { $0 + $1.friction.totalScore },
            processCount: members.reduce(0) { total, entity in
                total + entity.components.reduce(0) { componentTotal, component in
                    componentTotal + (component.kind == .adapterContext ? 0 : 1)
                }
            },
            userSummary: groupUserSummary(for: members)
        )
    }
}

private func filterEntities(
    _ entities: [EntitySnapshot],
    query: String
) -> [EntitySnapshot] {
    let normalizedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !normalizedQuery.isEmpty else {
        return entities
    }

    let loweredQuery = normalizedQuery.localizedLowercase
    return entities.filter { entity in
        entity.displayName.localizedLowercase.contains(loweredQuery)
            || entity.badges.joined(separator: " ").localizedLowercase.contains(loweredQuery)
            || entity.friction.reasons.joined(separator: " ").localizedLowercase.contains(loweredQuery)
            || entity.components.contains(where: { component in
                component.title.localizedLowercase.contains(loweredQuery)
                    || component.detail.localizedLowercase.contains(loweredQuery)
                    || component.adapterContext?.status?.localizedLowercase.contains(loweredQuery) == true
                    || component.adapterContext?.url?.localizedLowercase.contains(loweredQuery) == true
                    || component.adapterContext?.workspacePath?.localizedLowercase.contains(loweredQuery) == true
                    || component.adapterContext?.repoRoot?.localizedLowercase.contains(loweredQuery) == true
                    || component.adapterContext?.imageName?.localizedLowercase.contains(loweredQuery) == true
                    || component.adapterContext?.sessionId?.localizedLowercase.contains(loweredQuery) == true
                    || component.adapterContext?.ports.joined(separator: " ").localizedLowercase.contains(loweredQuery) == true
            })
    }
}

private func sortGroups(_ groups: [EntityGroup], by sortKey: SortKey) -> [EntityGroup] {
    switch sortKey {
    case .friction:
        return groups.sorted { $0.frictionScore > $1.frictionScore }
    case .cpu:
        return groups.sorted { $0.cpuPercent > $1.cpuPercent }
    case .memory:
        return groups.sorted { $0.memoryBytes > $1.memoryBytes }
    case .alphabeticalAsc:
        return groups.sorted { $0.root.displayName.localizedCaseInsensitiveCompare($1.root.displayName) == .orderedAscending }
    case .alphabeticalDesc:
        return groups.sorted { $0.root.displayName.localizedCaseInsensitiveCompare($1.root.displayName) == .orderedDescending }
    default:
        return groups.sorted { $0.frictionScore > $1.frictionScore }
    }
}

private func buildGroupedEntities(
    from entities: [EntitySnapshot],
    query: String,
    sortKey: SortKey
) -> [EntityGroup] {
    sortGroups(buildEntityGroups(from: filterEntities(entities, query: query)), by: sortKey)
}

private func extractParentPIDForGrouping(from parentSummary: String) -> UInt32? {
    guard let pidRange = parentSummary.range(of: "pid ") else { return nil }
    let digits = parentSummary[pidRange.upperBound...].prefix(while: \.isNumber)
    return UInt32(digits)
}

public struct MainListView: View {
    let state: AppState
    @State private var selectedEntityID: String?
    @State private var searchText = ""
    @State private var sortKey: SortKey = .friction
    @State private var focusedIndex: Int = 0
    @State private var listMode: ListMode = .grouped
    @State private var groupedEntitiesCache: [GroupingCacheKey: [EntityGroup]] = [:]
    @State private var displayedGroupedEntities: [EntityGroup] = []
    @State private var groupingTask: Task<[EntityGroup], Never>?
    @State private var isGrouping = false
    @FocusState private var searchFieldFocused: Bool

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        VStack(spacing: 0) {
            summaryHeader
            if shouldShowOperatorPanel {
                Divider()
                operatorOverviewPanel
            }
            Divider()
            monitorSplitView
        }
        .navigationTitle("Aetower")
        .modifier(KeyboardNavigationModifier(
            focusedIndex: $focusedIndex,
            selectedEntityID: $selectedEntityID,
            sortKey: $sortKey,
            entityCount: visibleEntityIDs.count,
            entityIdAt: { index in
                let ids = visibleEntityIDs
                guard index >= 0, index < ids.count else { return nil }
                return ids[index]
            }
        ))
        .task(id: groupingTaskToken) {
            await refreshGroupingCache()
        }
        .onChange(of: selectedEntityID) { _, newValue in
            if newValue != nil {
                searchFieldFocused = false
            }
        }
        .onDisappear {
            groupingTask?.cancel()
            groupingTask = nil
            searchFieldFocused = false
        }
    }

    private var summaryHeader: some View {
        let frictionScore = machineFrictionScore(for: state.snapshot.host)
        let frictionColor = AetowerDesign.frictionColor(Float(frictionScore))
        return HStack(spacing: 0) {
            // Friction score — large, colored
            HStack(spacing: 6) {
                Image(systemName: "bolt.fill")
                    .foregroundStyle(frictionColor)
                Text(String(format: "%.0f", frictionScore))
                    .font(.system(size: 22, weight: .bold, design: .rounded))
                    .foregroundStyle(frictionColor)
                    .contentTransition(.numericText())
            }
            .padding(.horizontal, 16)

            Divider().frame(height: 24)

            // Compact metric pills
            ribbonMetric("CPU", String(format: "%.0f%%", state.snapshot.host.cpuPercent), AetowerDesign.Tone.cpu)
            ribbonMetric("MEM", formatBytes(state.snapshot.host.memoryUsedBytes), AetowerDesign.Tone.memory)
            ribbonMetric("DISK", formatRate(state.snapshot.host.diskReadBps + state.snapshot.host.diskWriteBps), AetowerDesign.Tone.disk)
            ribbonMetric("NET", formatRate(state.snapshot.host.networkReceiveBps + state.snapshot.host.networkSendBps), AetowerDesign.Tone.network)

            if state.snapshot.host.gpuPercent > 0 {
                ribbonMetric("GPU", String(format: "%.0f%%", state.snapshot.host.gpuPercent), AetowerDesign.Tone.gpu)
            }

            if state.runtimeLagMetrics.targetTickMillis > 0 {
                ribbonMetric(
                    "TICK",
                    String(format: "%.1fs", state.runtimeLagMetrics.targetTickMillis / 1000),
                    .secondary
                )
            }

            Spacer()

            // AI agent count (if any)
            let agentCount = state.snapshot.host.aiAgentCount
            if agentCount > 0 {
                HStack(spacing: 4) {
                    Circle().fill(.blue).frame(width: 6, height: 6)
                    Text("\(agentCount) agent\(agentCount == 1 ? "" : "s")")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, 12)
            }

            // Export
            Button {
                state.exportSnapshot()
            } label: {
                Label("Export", systemImage: "square.and.arrow.up")
                    .font(.caption.weight(.semibold))
            }
            .buttonStyle(.plain)
            .padding(.trailing, 12)
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 16)
        .background(.ultraThinMaterial)
    }

    private var hostAlertsPanel: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 10) {
                ForEach(Array(hostAlerts.enumerated()), id: \.offset) { _, alert in
                    MachineBandMetric(
                        title: alert.title,
                        value: alert.value,
                        tone: alert.tone,
                        subtitle: alert.subtitle
                    )
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
        }
        .background(Color.secondary.opacity(0.04))
    }

    private var operatorOverviewPanel: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let machineIncident {
                OperatorIncidentBanner(incident: machineIncident)
                    .padding(.horizontal, 16)
                    .padding(.top, 12)
            }

            if !hostAlerts.isEmpty {
                hostAlertsPanel
            }

            if !burdenLeaders.isEmpty || !operatorRecommendations.isEmpty || !selfHealthChecks.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(alignment: .top, spacing: 12) {
                        if !burdenLeaders.isEmpty {
                            OperatorSectionCard(title: "Burden leaders") {
                                VStack(alignment: .leading, spacing: 10) {
                                    ForEach(burdenLeaders) { leader in
                                        VStack(alignment: .leading, spacing: 3) {
                                            HStack {
                                                Text(leader.title)
                                                    .font(.caption.weight(.semibold))
                                                    .foregroundStyle(.secondary)
                                                Spacer()
                                                Text(leader.metricValue)
                                                    .font(.caption.monospacedDigit())
                                                    .foregroundStyle(leader.severity.color)
                                            }
                                            Text(leader.entityName)
                                                .font(.subheadline.weight(.semibold))
                                            Text(leader.detail)
                                                .font(.caption)
                                                .foregroundStyle(.secondary)
                                        }
                                    }
                                }
                            }
                        }

                        if !operatorRecommendations.isEmpty {
                            OperatorSectionCard(title: "Next actions") {
                                VStack(alignment: .leading, spacing: 10) {
                                    ForEach(operatorRecommendations) { recommendation in
                                        VStack(alignment: .leading, spacing: 3) {
                                            HStack {
                                                Text(recommendation.title)
                                                    .font(.subheadline.weight(.semibold))
                                                Spacer()
                                                Text(recommendation.severity.label)
                                                    .font(.caption2.weight(.semibold))
                                                    .foregroundStyle(recommendation.severity.color)
                                            }
                                            Text(recommendation.detail)
                                                .font(.caption)
                                                .foregroundStyle(.secondary)
                                            Text(recommendation.action)
                                                .font(.caption2)
                                                .foregroundStyle(.tertiary)
                                        }
                                    }
                                }
                            }
                        }

                        if !selfHealthChecks.isEmpty {
                            OperatorSectionCard(title: "Aetower self-health") {
                                VStack(alignment: .leading, spacing: 10) {
                                    ForEach(selfHealthChecks) { check in
                                        VStack(alignment: .leading, spacing: 3) {
                                            HStack {
                                                Text(check.title)
                                                    .font(.caption.weight(.semibold))
                                                    .foregroundStyle(.secondary)
                                                Spacer()
                                                Text(check.value)
                                                    .font(.caption.weight(.semibold))
                                                    .foregroundStyle(check.severity.color)
                                            }
                                            Text(check.detail)
                                                .font(.caption2)
                                                .foregroundStyle(.secondary)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 12)
                }
            }
        }
        .background(Color.secondary.opacity(0.04))
    }

    private func ribbonMetric(_ label: String, _ value: String, _ color: Color) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption.weight(.semibold).monospacedDigit())
                .foregroundStyle(color)
        }
        .padding(.horizontal, 10)
    }

    private var rankingPanel: some View {
        ScrollView {
            rankedEntitiesSection
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
        }
    }

    private var monitorSplitView: some View {
        HSplitView {
            rankingPanel
                .frame(
                    minWidth: 420,
                    idealWidth: selectedEntity == nil ? 980 : 760,
                    maxWidth: .infinity,
                    maxHeight: .infinity
                )
                .layoutPriority(2)

            Group {
                if let entity = selectedEntity {
                    detailPanel(for: entity)
                        .transition(.opacity)
                } else {
                    detailPlaceholder
                }
            }
            .frame(
                minWidth: selectedEntity == nil ? 240 : 360,
                idealWidth: selectedEntity == nil ? 280 : 760,
                maxWidth: selectedEntity == nil ? 360 : .infinity,
                maxHeight: .infinity,
                alignment: .topLeading
            )
            .layoutPriority(1)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .animation(AetowerDesign.Motion.standard, value: selectedEntity?.entityId)
    }

    private func detailPanel(for entity: EntitySnapshot) -> some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .center, spacing: 12) {
                    SectionEyebrow(text: "Detail")

                    Text(entity.displayName)
                        .font(.title2.weight(.semibold))

                    FrictionStatusBadge(score: Double(entity.friction.totalScore))

                    Spacer()

                    Button {
                        withAnimation(AetowerDesign.Motion.standard) {
                            selectedEntityID = nil
                        }
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .font(.title3)
                            .foregroundStyle(.tertiary)
                    }
                    .buttonStyle(.plain)
                }

                Text(topConcernSummary(for: entity, sortKey: sortKey))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)

            Divider()

            EntityDetailView(
                entity: entity,
                state: state,
                processTreeSeedEntities: selectedProcessTreeEntities(for: entity)
            )
        }
    }

    private var detailPlaceholder: some View {
        VStack(alignment: .leading, spacing: 16) {
            ContentUnavailableView(
                "Select an Entity",
                systemImage: "arrow.triangle.branch",
                description: Text("Choose an app or process group from the monitor list to inspect its grouped process tree, attribution, and friction drivers.")
            )
        }
        .frame(maxWidth: 320, maxHeight: .infinity, alignment: .topLeading)
        .padding(.horizontal, 20)
        .padding(.top, 28)
    }

    private var rankedEntitiesSection: some View {
        VStack(alignment: .leading, spacing: 2) {
            // Compact sort + search bar
            HStack(spacing: 6) {
                Picker(selection: $listMode) {
                    ForEach(ListMode.allCases) { mode in
                        Image(systemName: mode.icon)
                            .tag(mode)
                    }
                } label: {
                    EmptyView()
                }
                .pickerStyle(.segmented)
                .frame(width: 80)
                .onChange(of: listMode) { _, newMode in
                    focusedIndex = 0
                    if let selectedEntityID, !visibleEntityIDs.contains(selectedEntityID) {
                        self.selectedEntityID = nil
                    }
                    // Reset unsupported sort keys when switching to grouped
                    // mode so the user doesn't silently get friction sort
                    // while the menu shows "Disk".
                    if newMode == .grouped && !sortKey.supportsGroupedMode {
                        sortKey = .friction
                    }
                }

                Menu {
                    ForEach(SortKey.allCases) { key in
                        Button {
                            sortKey = key
                        } label: {
                            HStack {
                                Text(key.title)
                                if isGroupedMode && !key.supportsGroupedMode {
                                    Text("(flat only)")
                                        .font(.caption2)
                                        .foregroundStyle(.tertiary)
                                }
                                if key == sortKey {
                                    Spacer()
                                    Image(systemName: "checkmark")
                                }
                            }
                        }
                        .disabled(isGroupedMode && !key.supportsGroupedMode)
                    }
                } label: {
                    HStack(spacing: 4) {
                        Text(sortKey.title)
                            .font(.caption.weight(.medium))
                        Image(systemName: "chevron.down")
                            .font(.system(size: 8, weight: .semibold))
                    }
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 5))
                }
                .menuStyle(.borderlessButton)
                .fixedSize()

                HStack(spacing: 4) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                    TextField("Search...", text: $searchText)
                        .textFieldStyle(.plain)
                        .aetowerUtilityTextInput()
                        .focused($searchFieldFocused)
                        .onSubmit { searchFieldFocused = false }
                        .font(.caption)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 5))
                .frame(maxWidth: 160)

                if isGroupedMode && isGrouping {
                    HStack(spacing: 5) {
                        ProgressView()
                            .controlSize(.mini)
                        Text("Grouping…")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                    .transition(.opacity)
                }

                Spacer()
            }
            .padding(.horizontal, 8)
            .padding(.bottom, 2)

            // Column headers
            HStack(spacing: 6) {
                Text("")
                    .frame(width: 16)
                Text("Name")
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text("CPU")
                    .frame(width: 48, alignment: .trailing)
                Text("MEM")
                    .frame(width: 52, alignment: .trailing)
                Text("User")
                    .frame(width: 60, alignment: .trailing)
                Text("Parent")
                    .frame(width: 80, alignment: .trailing)
                Text("Friction")
                    .frame(width: 50, alignment: .trailing)
            }
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(.tertiary)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)

            if filteredEntities.isEmpty {
                ContentUnavailableView(
                    "No apps match this filter",
                    systemImage: "magnifyingglass",
                    description: Text("Try a broader query.")
                )
            } else {
                LazyVStack(spacing: 2) {
                    if isGroupedMode {
                        ForEach(groupedEntities, id: \.id) { group in
                            Button {
                                searchFieldFocused = false
                                withAnimation(AetowerDesign.Motion.standard) {
                                    selectedEntityID = group.root.entityId
                                }
                            } label: {
                                GroupedEntityRow(
                                    group: group,
                                    isSelected: selectedEntityID == group.root.entityId
                                )
                            }
                            .buttonStyle(.plain)
                        }
                    } else {
                        ForEach(filteredEntities, id: \.entityId) { entity in
                            Button {
                                searchFieldFocused = false
                                withAnimation(AetowerDesign.Motion.standard) {
                                    selectedEntityID = entity.entityId
                                }
                            } label: {
                                EntityRow(
                                    entity: entity,
                                    isSelected: selectedEntityID == entity.entityId
                                )
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                Button("Copy Process IDs") {
                                    let pids = entity.components.compactMap { $0.processId }.map(String.init).joined(separator: ", ")
                                    NSPasteboard.general.clearContents()
                                    NSPasteboard.general.setString(pids, forType: .string)
                                }
                                if entity.executablePath?.contains(".app/") == true {
                                    Button("Show in Finder") {
                                        if let path = entity.executablePath {
                                            NSWorkspace.shared.selectFile(path, inFileViewerRootedAtPath: "")
                                        }
                                    }
                                }
                                Divider()
                                Button("Terminate Processes", role: .destructive) {
                                    for component in entity.components {
                                        if let pid = component.processId {
                                            kill(Int32(pid), SIGTERM)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                .animation(AetowerDesign.Motion.standard, value: sortKey)
                .animation(AetowerDesign.Motion.standard, value: listMode)
                .animation(nil, value: state.snapshot.sequence)
            }
        }
    }

    private var filteredEntities: [EntitySnapshot] {
        filterEntities(state.snapshot.entities, query: normalizedSearchQuery).sorted {
            compareEntities($0, $1, by: sortKey)
        }
    }

    // Lazy short-circuit: evaluate cheapest check first, stop as soon
    // as any section has content. Previously, all five expensive
    // computed properties were evaluated unconditionally just to test
    // isEmpty, which sorted/scanned the entity list multiple times.
    private var shouldShowOperatorPanel: Bool {
        if !hostAlerts.isEmpty { return true }
        if machineIncident != nil { return true }
        if !burdenLeaders.isEmpty { return true }
        if !operatorRecommendations.isEmpty { return true }
        if !selfHealthChecks.isEmpty { return true }
        return false
    }

    private var machineIncident: HostIncidentSummary? {
        buildHostIncident(snapshot: state.snapshot, history: state.historyStoreSummary ?? state.historyRangeSummary)
    }

    private var burdenLeaders: [BurdenLeaderSummary] {
        Array(buildBurdenLeaders(snapshot: state.snapshot).prefix(4))
    }

    private var operatorRecommendations: [OperatorRecommendationSummary] {
        buildOperatorRecommendations(
            snapshot: state.snapshot,
            diagnostics: state.diagnosticsOverview,
            history: state.historyStoreSummary ?? state.historyRangeSummary,
            runtime: state.runtimeLagMetrics
        )
    }

    private var selfHealthChecks: [OperatorHealthCheckSummary] {
        buildSelfHealthChecks(
            snapshot: state.snapshot,
            diagnostics: state.diagnosticsOverview,
            history: state.historyStoreSummary ?? state.historyRangeSummary,
            runtime: state.runtimeLagMetrics,
            localMcpServerHealthy: state.localMcpServerHealthy
        )
    }

    private var hostAlerts: [HostAlertCard] {
        var alerts: [HostAlertCard] = []
        let host = state.snapshot.host
        let memoryBand = hostPressureBand(host)
        if memoryBand != .nominal {
            let leaders = state.snapshot.entities
                .sorted { $0.metrics.memoryResidentBytes > $1.metrics.memoryResidentBytes }
                .prefix(3)
                .map(\.displayName)
                .joined(separator: ", ")
            let pressureText = memoryBand == .severe ? "Severe" : "Elevated"
            alerts.append(
                HostAlertCard(
                    title: "Memory pressure",
                    value: pressureText,
                    tone: memoryBand == .severe ? .red : .orange,
                    subtitle: [
                        "compressed \(formatBytes(host.compressedMemoryBytes))",
                        host.swapUsedBytes > 0 ? "swap \(formatBytes(host.swapUsedBytes))" : nil,
                        leaders.isEmpty ? nil : "leaders \(leaders)"
                    ]
                    .compactMap { $0 }
                    .joined(separator: " · ")
                )
            )
        }

        let wakeupBand = hostWakeupBand(host.wakeupsPerSecond)
        if wakeupBand != .nominal,
           let leader = state.snapshot.entities.max(by: { $0.metrics.wakeupsPerSecond < $1.metrics.wakeupsPerSecond })
        {
            alerts.append(
                HostAlertCard(
                    title: "Wakeup leader",
                    value: leader.displayName,
                    tone: wakeupBand == .severe ? .red : .yellow,
                    subtitle: "\(formatWakeups(leader.metrics.wakeupsPerSecond)) entity · \(formatWakeups(host.wakeupsPerSecond)) host · \(wakeupsLeaderSubtitle(for: leader))"
                )
            )
        }

        return alerts
    }

    // Resolves from the full snapshot so the detail pane persists even
    // when the entity is filtered out by search. Eliminates one
    // redundant filteredEntities evaluation per render cycle.
    private var selectedEntity: EntitySnapshot? {
        guard let selectedEntityID else { return nil }
        return state.snapshot.entities.first { $0.entityId == selectedEntityID }
    }

    private var selectedEntityGroup: EntityGroup? {
        guard let selectedEntityID, isGroupedMode else { return nil }
        return groupedEntities.first(where: { $0.root.entityId == selectedEntityID })
    }

    private var visibleEntityIDs: [String] {
        isGroupedMode ? groupedEntities.map(\.root.entityId) : filteredEntities.map(\.entityId)
    }

    private var groupedEntities: [EntityGroup] {
        guard let key = currentGroupingCacheKey else { return [] }
        return groupedEntitiesCache[key] ?? displayedGroupedEntities
    }

    private func selectedProcessTreeEntities(for entity: EntitySnapshot) -> [EntitySnapshot] {
        if let selectedEntityGroup, selectedEntityGroup.root.entityId == entity.entityId {
            return selectedEntityGroup.members
        }
        return state.snapshot.entities
    }

    private var normalizedSearchQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var currentGroupingCacheKey: GroupingCacheKey? {
        guard isGroupedMode else { return nil }
        return GroupingCacheKey(
            sequence: state.snapshot.sequence,
            query: normalizedSearchQuery,
            sortKey: sortKey
        )
    }

    private var isGroupedMode: Bool {
        listMode == .grouped
    }

    private var groupingTaskToken: String {
        guard let key = currentGroupingCacheKey else { return "flat" }
        return "\(key.sequence)|\(key.query)|\(key.sortKey.rawValue)"
    }

    @MainActor
    private func refreshGroupingCache() async {
        guard let key = currentGroupingCacheKey else {
            groupingTask?.cancel()
            groupingTask = nil
            isGrouping = false
            return
        }

        if let cached = groupedEntitiesCache[key] {
            displayedGroupedEntities = cached
            isGrouping = false
            return
        }

        groupingTask?.cancel()

        let entities = state.snapshot.entities
        let query = key.query
        let sortKey = key.sortKey
        isGrouping = true

        let task = Task.detached(priority: .utility) {
            buildGroupedEntities(from: entities, query: query, sortKey: sortKey)
        }
        groupingTask = task

        let groups = await task.value
        guard !Task.isCancelled else { return }
        guard currentGroupingCacheKey == key else { return }

        groupedEntitiesCache = groupedEntitiesCache
            .filter { $0.key.sequence == key.sequence }
        groupedEntitiesCache[key] = groups
        displayedGroupedEntities = groups
        isGrouping = false

        if let selectedEntityID, !visibleEntityIDs.contains(selectedEntityID) {
            self.selectedEntityID = nil
        }
    }

    private func topConcernSummary(for entity: EntitySnapshot, sortKey: SortKey) -> String {
        switch sortKey {
        case .friction:
            if entity.metrics.isForeground {
                return "\(entity.displayName) is frontmost and currently highest by friction score."
            }
            return "\(entity.displayName) is the highest-ranked background source by friction score."
        case .cpu:
            return "\(entity.displayName) is currently highest by CPU usage at \(String(format: "%.1f%%", entity.metrics.cpuPercent))."
        case .memory:
            return "\(entity.displayName) is currently highest by memory load at \(String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: state.snapshot.host.memoryTotalBytes)))."
        case .disk:
            return "\(entity.displayName) is currently highest by disk activity at \(formatRate(entity.metrics.diskReadBps + entity.metrics.diskWriteBps))."
        case .network:
            return "\(entity.displayName) is currently highest by network activity at \(formatRate(entity.metrics.networkReceiveBps + entity.metrics.networkSendBps))."
        case .energy:
            return "\(entity.displayName) is currently highest by energy impact at \(String(format: "%.1f", entity.friction.energyImpactScore))."
        case .alphabeticalAsc:
            return "\(entity.displayName) is first in the current alphabetical A-Z sort."
        case .alphabeticalDesc:
            return "\(entity.displayName) is first in the current alphabetical Z-A sort."
        case .oldestFirst:
            return "\(entity.displayName) has the oldest process group in the current list at \(ageLabel(from: entity.oldestProcessStartMillis, now: state.snapshot.capturedAtMillis))."
        case .newestFirst:
            return "\(entity.displayName) has the newest process group in the current list at \(ageLabel(from: entity.newestProcessStartMillis, now: state.snapshot.capturedAtMillis))."
        }
    }

}

private struct HostAlertCard {
    let title: String
    let value: String
    let tone: Color
    let subtitle: String
}

private enum HostBand {
    case nominal
    case elevated
    case severe
}

private func hostStatusSummary(_ host: HostSnapshot) -> String {
    var parts = [powerStatusSummary(host)]
    parts.append("thermal \(thermalStateLabel(host.thermalState))")
    if host.compressedMemoryBytes > 0 {
        parts.append("compressed \(formatBytes(host.compressedMemoryBytes))")
    }
    if host.gpuPercent > 0 || host.anePercent > 0 {
        parts.append(hostGPUSummary(host))
    }
    if host.wakeupsPerSecond > 0 {
        parts.append("\(formatWakeups(host.wakeupsPerSecond)) wakeups")
    }
    if host.lowPowerMode {
        parts.append("low power mode")
    }
    return parts.joined(separator: " · ")
}

private func powerStatusSummary(_ host: HostSnapshot) -> String {
    if host.onBattery {
        if let batteryChargePercent = host.batteryChargePercent {
            return "battery \(batteryChargePercent)%"
        }
        return "battery power"
    }
    return "AC power"
}

private func hostGPUSummary(_ host: HostSnapshot) -> String {
    if host.anePercent > 0 {
        return "ANE \(String(format: "%.1f%%", host.anePercent)) · \(formatBytes(host.gpuMemoryBytes)) GPU memory"
    }
    if host.gpuMemoryBytes > 0 {
        return "\(formatBytes(host.gpuMemoryBytes)) GPU memory"
    }
    return "render/compute activity"
}

private func thermalStateLabel(_ state: ThermalState) -> String {
    switch state {
    case .nominal:
        return "nominal"
    case .fair:
        return "fair"
    case .serious:
        return "serious"
    case .critical:
        return "critical"
    }
}

func formatBytes(_ bytes: UInt64) -> String {
    ByteFormatters.binary.string(fromByteCount: Int64(bytes))
}

func formatRate(_ bytesPerSecond: UInt64) -> String {
    "\(formatBytes(bytesPerSecond))/s"
}

func formatWakeups(_ wakeupsPerSecond: Float) -> String {
    if wakeupsPerSecond >= 100 {
        return String(format: "%.0f/s", wakeupsPerSecond)
    }
    return String(format: "%.1f/s", wakeupsPerSecond)
}

private enum ByteFormatters {
    nonisolated(unsafe) static let binary: ByteCountFormatter = {
        let formatter = ByteCountFormatter()
        formatter.allowedUnits = [.useMB, .useGB]
        formatter.countStyle = .binary
        return formatter
    }()
}

private func trendLabel(samples: [Double], stableText: String) -> String {
    guard let first = samples.first, let last = samples.last, samples.count >= 2 else {
        return stableText
    }

    let baseline = max(abs(first), 1.0)
    let deltaRatio = (last - first) / baseline
    if deltaRatio > 0.12 {
        return "rising"
    }
    if deltaRatio < -0.12 {
        return "falling"
    }
    return stableText
}

private func frictionTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.friction.map(Double.init), stableText: "recent score")) · \(trendWindowLabel(sampleCount: entity.trend.friction.count))"
}

private func cpuTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.cpuPercent.map(Double.init), stableText: "recent load")) · \(trendWindowLabel(sampleCount: entity.trend.cpuPercent.count))"
}

private func memoryTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(formatBytes(entity.metrics.memoryResidentBytes)) · \(trendWindowLabel(sampleCount: entity.trend.memoryResidentBytes.count))"
}

private func diskTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.diskActivityBps.map(Double.init), stableText: "recent throughput")) · \(trendWindowLabel(sampleCount: entity.trend.diskActivityBps.count))"
}

private func networkTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.networkActivityBps.map(Double.init), stableText: "recent throughput")) · \(trendWindowLabel(sampleCount: entity.trend.networkActivityBps.count))"
}

private func wakeupsTrendSummary(_ entity: EntitySnapshot) -> String {
    "\(trendLabel(samples: entity.trend.wakeupsPerSecond.map(Double.init), stableText: "recent wakeups")) · \(trendWindowLabel(sampleCount: entity.trend.wakeupsPerSecond.count))"
}

private func hostCompressedSummary(_ host: HostSnapshot) -> String {
    if host.compressedMemoryBytes > 0 {
        return "compressed \(formatBytes(host.compressedMemoryBytes))"
    }
    return "memory pressure nominal"
}

private func hostPressureBand(_ host: HostSnapshot) -> HostBand {
    let totalBytes = max(Double(host.memoryTotalBytes), 1)
    let compressedRatio = Double(host.compressedMemoryBytes) / totalBytes
    let swapRatio = Double(host.swapUsedBytes) / totalBytes
    if compressedRatio >= 0.12 || swapRatio >= 0.08 {
        return .severe
    }
    if compressedRatio >= 0.05 || swapRatio >= 0.02 {
        return .elevated
    }
    return .nominal
}

private func hostWakeupBand(_ wakeupsPerSecond: Float) -> HostBand {
    if wakeupsPerSecond >= 3_000 {
        return .severe
    }
    if wakeupsPerSecond >= 1_500 {
        return .elevated
    }
    return .nominal
}

private func wakeupsLeaderSubtitle(for entity: EntitySnapshot) -> String {
    let components = entity.components.filter { $0.kind == .process }
    if components.count <= 1 {
        return "single-process hotspot"
    }
    return "\(components.count) processes in group"
}

private func memoryLoadPercent(bytes: UInt64, totalBytes: UInt64) -> Double {
    guard totalBytes > 0 else { return 0 }
    return (Double(bytes) / Double(totalBytes)) * 100.0
}

private func entityMemoryLoadPercent(_ entity: EntitySnapshot, totalBytes: UInt64) -> Double {
    memoryLoadPercent(bytes: entity.metrics.memoryResidentBytes, totalBytes: totalBytes)
}

private func entityMemoryTrendPercents(_ entity: EntitySnapshot, totalBytes: UInt64) -> [Double] {
    entity.trend.memoryResidentBytes.map {
        memoryLoadPercent(bytes: $0, totalBytes: totalBytes)
    }
}

private func trendWindowLabel(sampleCount: Int) -> String {
    let seconds = min(sampleCount * 2, 300)
    return "last \(seconds)s"
}

private func compareEntities(_ left: EntitySnapshot, _ right: EntitySnapshot, by sortKey: SortKey) -> Bool {
    switch sortKey {
    case .friction:
        return Double(left.friction.totalScore) > Double(right.friction.totalScore)
    case .cpu:
        return Double(left.metrics.cpuPercent) > Double(right.metrics.cpuPercent)
    case .memory:
        return left.metrics.memoryResidentBytes > right.metrics.memoryResidentBytes
    case .disk:
        return (left.metrics.diskReadBps + left.metrics.diskWriteBps) > (right.metrics.diskReadBps + right.metrics.diskWriteBps)
    case .network:
        return (left.metrics.networkReceiveBps + left.metrics.networkSendBps) > (right.metrics.networkReceiveBps + right.metrics.networkSendBps)
    case .energy:
        return Double(left.friction.energyImpactScore) > Double(right.friction.energyImpactScore)
    case .alphabeticalAsc:
        return left.displayName.localizedCaseInsensitiveCompare(right.displayName) == .orderedAscending
    case .alphabeticalDesc:
        return left.displayName.localizedCaseInsensitiveCompare(right.displayName) == .orderedDescending
    case .oldestFirst:
        let leftStart = left.oldestProcessStartMillis == 0 ? UInt64.max : left.oldestProcessStartMillis
        let rightStart = right.oldestProcessStartMillis == 0 ? UInt64.max : right.oldestProcessStartMillis
        return leftStart < rightStart
    case .newestFirst:
        return left.newestProcessStartMillis > right.newestProcessStartMillis
    }
}

private func ageLabel(from startMillis: UInt64, now capturedAtMillis: UInt64) -> String {
    guard startMillis > 0, capturedAtMillis >= startMillis else {
        return "n/a"
    }

    let elapsedSeconds = Int((capturedAtMillis - startMillis) / 1_000)
    if elapsedSeconds < 60 {
        return "\(elapsedSeconds)s"
    }
    if elapsedSeconds < 3_600 {
        return "\(elapsedSeconds / 60)m"
    }
    if elapsedSeconds < 86_400 {
        return "\(elapsedSeconds / 3_600)h"
    }
    return "\(elapsedSeconds / 86_400)d"
}

/// Vim-style keyboard navigation for the entity list.
///
/// `entityIdAt` returns `nil` when the index is out of bounds so that
/// a stale `focusedIndex` from before a search/sort/filter change
/// cannot crash the view. The index is clamped on every movement, but
/// the closure is the last line of defense against the race between
/// the `@State` update and the next key event.
private struct KeyboardNavigationModifier: ViewModifier {
    @Binding var focusedIndex: Int
    @Binding var selectedEntityID: String?
    @Binding var sortKey: SortKey
    let entityCount: Int
    let entityIdAt: (Int) -> String?

    func body(content: Content) -> some View {
        content
            .onKeyPress("j") { moveDown() }
            .onKeyPress("k") { moveUp() }
            .onKeyPress(.return) { selectCurrent() }
            .onKeyPress(.escape) {
                withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = nil }
                return .handled
            }
            .onChange(of: entityCount) { _, newCount in
                // Clamp the cursor whenever the list size changes so a
                // search that narrows from 40 to 3 entities doesn't leave
                // focusedIndex at 39.
                if newCount > 0 {
                    focusedIndex = min(focusedIndex, newCount - 1)
                } else {
                    focusedIndex = 0
                }
            }
    }

    /// Clamp the index to valid bounds before using it.
    private var safeIndex: Int {
        guard entityCount > 0 else { return 0 }
        return min(max(focusedIndex, 0), entityCount - 1)
    }

    private func moveDown() -> KeyPress.Result {
        guard entityCount > 0 else { return .ignored }
        focusedIndex = min(safeIndex + 1, entityCount - 1)
        if let id = entityIdAt(focusedIndex) {
            withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = id }
        }
        return .handled
    }

    private func moveUp() -> KeyPress.Result {
        guard entityCount > 0 else { return .ignored }
        focusedIndex = max(safeIndex - 1, 0)
        if let id = entityIdAt(focusedIndex) {
            withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = id }
        }
        return .handled
    }

    private func selectCurrent() -> KeyPress.Result {
        guard selectedEntityID == nil, entityCount > 0 else { return .handled }
        focusedIndex = safeIndex
        if let id = entityIdAt(focusedIndex) {
            withAnimation(AetowerDesign.Motion.standard) { selectedEntityID = id }
        }
        return .handled
    }
}
