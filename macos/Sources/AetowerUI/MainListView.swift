import SwiftUI
import AetowerBridge

private struct SectionEyebrow: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.caption.weight(.semibold))
            .foregroundStyle(.tertiary)
            .tracking(0.8)
    }
}


private enum SortKey: String, CaseIterable, Identifiable {
    case friction
    case cpu
    case memory
    case wakeups
    case processCount
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
        case .memory: return "Resident"
        case .wakeups: return "Wakeups"
        case .processCount: return "Process count"
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
        case .wakeups: return .mint
        case .processCount: return .indigo
        case .disk: return .pink
        case .network: return .teal
        case .energy: return .yellow
        case .alphabeticalAsc, .alphabeticalDesc, .oldestFirst, .newestFirst:
            return .gray
        }
    }

    var usesMetricValue: Bool {
        switch self {
        case .friction, .cpu, .memory, .wakeups, .processCount, .disk, .network, .energy:
            return true
        case .alphabeticalAsc, .alphabeticalDesc, .oldestFirst, .newestFirst:
            return false
        }
    }
}

private enum MonitorSortColumn {
    case name
    case cpu
    case memory
    case wakeups
    case processCount
    case friction

    var title: String {
        switch self {
        case .name: return "Name"
        case .cpu: return "CPU"
        case .memory: return "MEM"
        case .wakeups: return "Wake"
        case .processCount: return "PIDs"
        case .friction: return "Friction"
        }
    }

    var width: CGFloat? {
        switch self {
        case .name: return nil
        case .cpu: return 64
        case .memory: return 84
        case .wakeups: return 72
        case .processCount: return 56
        case .friction: return 68
        }
    }

    var alignment: Alignment {
        switch self {
        case .name:
            return .leading
        case .cpu, .memory, .wakeups, .processCount, .friction:
            return .center
        }
    }
}

private struct MonitorHeaderButton: View {
    let column: MonitorSortColumn
    let isActive: Bool
    let indicatorSymbol: String?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 3) {
                Text(column.title)
                    .lineLimit(1)
                if let indicatorSymbol {
                    Image(systemName: indicatorSymbol)
                        .font(.system(size: 7, weight: .bold))
                }
            }
            .frame(
                maxWidth: column.width == nil ? .infinity : nil,
                alignment: column.alignment
            )
            .frame(width: column.width, alignment: column.alignment)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(isActive ? AnyShapeStyle(Color.primary) : AnyShapeStyle(.tertiary))
        .help(helpText)
    }

    private var helpText: String {
        switch column {
        case .name:
            return "Click to toggle alphabetical order."
        case .cpu:
            return "Sort by highest CPU usage."
        case .memory:
            return "Sort by highest resident memory."
        case .wakeups:
            return "Sort by highest wakeup rate."
        case .processCount:
            return "Sort by most live PIDs."
        case .friction:
            return "Sort by highest friction score."
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

    /// A process group is "new" when its most recently started process began
    /// within the last 30 seconds — surfaces freshly launched apps at a glance.
    private var isNewlyLaunched: Bool {
        let start = entity.newestProcessStartMillis
        guard start > 0 else { return false }
        let now = UInt64(Date().timeIntervalSince1970 * 1000)
        return now >= start && now - start <= 30_000
    }

    /// Distinct remote hosts this entity is connected to (host without port,
    /// excluding listening wildcards) — powers the "talking to" indicator.
    private var distinctRemoteHosts: [String] {
        var seen = Set<String>()
        var hosts: [String] = []
        for connection in entity.networkConnections {
            guard let remote = connection.remote else { continue }
            let host = String(remote.split(separator: ":").first ?? Substring(remote))
            if host.isEmpty || host == "*" { continue }
            if seen.insert(host).inserted { hosts.append(host) }
        }
        return hosts
    }

    /// Compact VirusTotal chip for the row: detections/total colored by verdict.
    /// Hidden for the `unknown` verdict (binary not in VirusTotal's corpus) to
    /// avoid cluttering the row with non-signal.
    private var reputationChip: (label: String, color: Color)? {
        guard let reputation = entity.binaryReputation else { return nil }
        let detections = reputation.malicious + reputation.suspicious
        switch reputation.verdict {
        case .malicious:
            return ("VT \(detections)/\(reputation.totalEngines)", AetowerDesign.Status.error)
        case .suspicious:
            return ("VT \(detections)/\(reputation.totalEngines)", AetowerDesign.Status.warning)
        case .clean:
            return ("VT clean", AetowerDesign.Status.success)
        case .unknown:
            return nil
        @unknown default:
            return nil
        }
    }

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

            if isNewlyLaunched {
                Text("NEW")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(AetowerDesign.Status.success)
                    .padding(.horizontal, 4)
                    .padding(.vertical, 1)
                    .background(AetowerDesign.Status.success.opacity(0.15), in: Capsule())
            }

            // Badges
            if entity.entityKind == .aiAgent {
                HStack(spacing: 3) {
                    ForEach(agentRuntimeBadges(for: entity)) { badge in
                        Text(badge.label)
                            .font(.system(size: 9, weight: .bold))
                            .foregroundStyle(badge.color)
                            .lineLimit(1)
                            .padding(.horizontal, 4)
                            .padding(.vertical, 1)
                            .background(badge.color.opacity(0.15), in: Capsule())
                    }
                }
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

            if !distinctRemoteHosts.isEmpty {
                HStack(spacing: 2) {
                    Image(systemName: "network")
                        .font(.system(size: 8))
                    Text("\(distinctRemoteHosts.count)")
                        .font(.system(size: 9, weight: .medium, design: .monospaced))
                }
                .foregroundStyle(AetowerDesign.Tone.network)
                .help("Talking to \(distinctRemoteHosts.prefix(5).joined(separator: ", "))")
            }

            if let chip = reputationChip {
                Text(chip.label)
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(chip.color)
                    .lineLimit(1)
                    .padding(.horizontal, 4)
                    .padding(.vertical, 1)
                    .background(chip.color.opacity(0.15), in: Capsule())
                    .help("VirusTotal reputation for this binary.")
            }

            // Metrics — centered, fixed width columns
            Text(String(format: "%.1f%%", entity.metrics.cpuPercent))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 64, alignment: .center)

            Text(formatBytes(entityEffectiveMemoryBytes(entity)))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 84, alignment: .center)

            Text(formatWakeups(entity.metrics.wakeupsPerSecond))
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 72, alignment: .center)

            Label("\(entityProcessCount)", systemImage: "number")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.tertiary)
                .labelStyle(.titleAndIcon)
                .lineLimit(1)
                .frame(width: 56, alignment: .center)

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
            .frame(width: 68, alignment: .center)

            Image(systemName: "sidebar.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(isSelected || isHovered ? Color.accentColor : Color.secondary.opacity(0.55))
                .frame(width: 16)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .frame(maxWidth: .infinity)
        .background(
            AetowerDesign.frictionColor(entity.friction.totalScore)
                .opacity(frictionBackgroundOpacity),
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm, style: .continuous)
                .stroke(
                    isSelected ? Color.accentColor.opacity(0.5) : .clear,
                    lineWidth: 1
                )
        )
        .onHover { isHovered = $0 }
        .help(rowHelpText)
        .animation(AetowerDesign.Motion.quick, value: isHovered)
    }

    private var entityUser: String {
        entity.components
            .lazy
            .compactMap(\.user)
            .first(where: { !$0.isEmpty }) ?? ""
    }

    private var rowHelpText: String {
        [
            entityUser.isEmpty ? nil : "User: \(entityUser)",
            entityParent.isEmpty ? nil : "Parent: \(entityParent)",
            "Processes: \(entityProcessCount)",
            "Wakeups: \(formatWakeups(entity.metrics.wakeupsPerSecond))",
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

    private var entityProcessCount: Int {
        entity.components.filter { $0.kind != .adapterContext && $0.processId != nil }.count
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
    let wakeupsPerSecond: Float
    let diskBps: UInt64
    let networkBps: UInt64
    let energyScore: Float
    let frictionScore: Float
    let processCount: Int
    let userSummary: String
    let oldestStartMillis: UInt64
    let newestStartMillis: UInt64

    var id: String { root.entityId }
}

private struct GroupingCacheKey: Hashable {
    let sequence: UInt64
    let query: String
    let sortKey: SortKey
    let filterSignature: String
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
                .lineLimit(1)
                .frame(width: 64, alignment: .center)

            Text(formatBytes(group.memoryBytes))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 84, alignment: .center)

            Text(formatWakeups(group.wakeupsPerSecond))
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 72, alignment: .center)

            Label("\(group.processCount)", systemImage: "number")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.tertiary)
                .labelStyle(.titleAndIcon)
                .lineLimit(1)
                .frame(width: 56, alignment: .center)

            HStack(spacing: 2) {
                Image(systemName: "arrow.triangle.branch")
                    .font(.system(size: 8))
                    .foregroundStyle(.secondary)
                Text(String(format: "%.1f", group.frictionScore))
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(group.frictionScore))
                    .contentTransition(.numericText())
            }
            .frame(width: 68, alignment: .center)

            Image(systemName: "sidebar.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(isSelected || isHovered ? Color.accentColor : Color.secondary.opacity(0.55))
                .frame(width: 16)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .frame(maxWidth: .infinity)
        .background(
            AetowerDesign.frictionColor(group.frictionScore)
                .opacity(frictionBackgroundOpacity),
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm, style: .continuous)
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
        let user = group.userSummary.isEmpty ? "" : " · user \(group.userSummary)"
        let metrics = "\(formatWakeups(group.wakeupsPerSecond)) · \(formatRate(group.diskBps)) disk · \(formatRate(group.networkBps)) network\(user)"
        if group.members.count > 6 {
            return "Collapsed entities: \(names), +\(group.members.count - 6) more · \(group.processCount) grouped processes · \(metrics)"
        }
        if group.members.count > 1 {
            return "Collapsed entities: \(names) · \(group.processCount) grouped processes · \(metrics)"
        }
        return "Entity footprint: \(group.processCount) grouped processes · \(metrics)"
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
        let sortedMembers = members.sorted { $0.friction.totalScore > $1.friction.totalScore }
        let startTimes = members
            .map(\.oldestProcessStartMillis)
            .filter { $0 > 0 }
        return EntityGroup(
            root: root,
            members: sortedMembers,
            cpuPercent: members.reduce(0) { $0 + $1.metrics.cpuPercent },
            memoryBytes: members.reduce(0) { $0 + entityEffectiveMemoryBytes($1) },
            wakeupsPerSecond: members.reduce(0) { $0 + $1.metrics.wakeupsPerSecond },
            diskBps: members.reduce(0) { $0 + $1.metrics.diskReadBps + $1.metrics.diskWriteBps },
            networkBps: members.reduce(0) { $0 + $1.metrics.networkReceiveBps + $1.metrics.networkSendBps },
            energyScore: members.reduce(0) { $0 + $1.friction.energyImpactScore },
            frictionScore: members.reduce(0) { $0 + $1.friction.totalScore },
            processCount: members.reduce(0) { total, entity in
                total + entity.components.reduce(0) { componentTotal, component in
                    componentTotal + (component.kind == .adapterContext ? 0 : 1)
                }
            },
            userSummary: groupUserSummary(for: members),
            oldestStartMillis: startTimes.min() ?? 0,
            newestStartMillis: members.map(\.newestProcessStartMillis).max() ?? 0
        )
    }
}

private func filterEntities(
    _ entities: [EntitySnapshot],
    query: String
) -> [EntitySnapshot] {
    let predicates = parseSearchQuery(query)
    guard !predicates.isEmpty else {
        return entities
    }
    // Space-separated tokens are ANDed: an entity must satisfy every predicate.
    return entities.filter { entity in predicates.allSatisfy { $0(entity) } }
}

/// All free-text haystacks for an entity (lowercased), used by bare-term and
/// `field`-less matching.
private func entityTextHaystacks(_ entity: EntitySnapshot) -> [String] {
    var haystacks = [
        entity.displayName,
        entity.badges.joined(separator: " "),
        entity.friction.reasons.joined(separator: " "),
    ]
    for component in entity.components {
        haystacks.append(component.title)
        haystacks.append(component.detail)
        if let path = component.executablePath { haystacks.append(path) }
        if let command = component.commandLine { haystacks.append(command) }
        if let cwd = component.cwd { haystacks.append(cwd) }
        if let user = component.user { haystacks.append(user) }
        if let context = component.adapterContext {
            for value in [context.status, context.url, context.workspacePath, context.repoRoot, context.imageName, context.sessionId] {
                if let value { haystacks.append(value) }
            }
        }
    }
    return haystacks.map { $0.localizedLowercase }
}

private func entityMatchesSubstring(_ entity: EntitySnapshot, _ needle: String) -> Bool {
    entityTextHaystacks(entity).contains { $0.contains(needle) }
}

/// Parse a search query into a list of AND-ed predicates. Supports bare
/// substrings (default), `/regex/flags`, `field:value` text filters, and
/// `field>n` / `field<n` / `field=n` numeric comparisons.
private func parseSearchQuery(_ query: String) -> [(EntitySnapshot) -> Bool] {
    tokenizeSearchQuery(query).compactMap(searchPredicate(for:))
}

/// Split on whitespace, but keep a `/.../`‑delimited regex (which may contain
/// spaces) together as a single token.
private func tokenizeSearchQuery(_ query: String) -> [String] {
    var tokens: [String] = []
    var current = ""
    var inRegex = false
    let chars = Array(query)
    var index = 0
    while index < chars.count {
        let character = chars[index]
        if inRegex {
            current.append(character)
            if character == "/" {
                index += 1
                while index < chars.count, chars[index].isLetter {
                    current.append(chars[index])
                    index += 1
                }
                tokens.append(current)
                current = ""
                inRegex = false
                continue
            }
            index += 1
            continue
        }
        if character == "/", current.isEmpty {
            inRegex = true
            current.append(character)
            index += 1
            continue
        }
        if character == " " || character == "\t" {
            if !current.isEmpty {
                tokens.append(current)
                current = ""
            }
            index += 1
            continue
        }
        current.append(character)
        index += 1
    }
    if !current.isEmpty {
        tokens.append(current)
    }
    return tokens
}

private func searchPredicate(for token: String) -> ((EntitySnapshot) -> Bool)? {
    guard !token.isEmpty else { return nil }

    // /regex/flags
    if token.hasPrefix("/"), let regex = compileRegexToken(token) {
        return { entity in
            entityTextHaystacks(entity).contains { haystack in
                regex.firstMatch(in: haystack, range: NSRange(haystack.startIndex..., in: haystack)) != nil
            }
        }
    }

    // field>n / field<n / field>=n / field<=n / field=n  (numeric)
    if let numeric = parseNumericToken(token) {
        return numeric
    }

    // field:value  (text)
    if let colon = token.firstIndex(of: ":") {
        let field = String(token[..<colon]).localizedLowercase
        let value = String(token[token.index(after: colon)...]).localizedLowercase
        if !value.isEmpty, let predicate = textFieldPredicate(field: field, value: value) {
            return predicate
        }
    }

    // bare substring
    let needle = token.localizedLowercase
    return { entityMatchesSubstring($0, needle) }
}

private func compileRegexToken(_ token: String) -> NSRegularExpression? {
    // token looks like /pattern/flags ; fall back to substring (nil) if malformed.
    let body = token.dropFirst()
    guard let closing = body.lastIndex(of: "/") else { return nil }
    let pattern = String(body[..<closing])
    let flags = String(body[body.index(after: closing)...])
    guard !pattern.isEmpty else { return nil }
    var options: NSRegularExpression.Options = []
    if flags.contains("i") { options.insert(.caseInsensitive) }
    return try? NSRegularExpression(pattern: pattern, options: options)
}

private func textFieldPredicate(field: String, value: String) -> ((EntitySnapshot) -> Bool)? {
    switch field {
    case "name":
        return { entity in
            entity.displayName.localizedLowercase.contains(value)
                || entity.components.contains { $0.title.localizedLowercase.contains(value) }
        }
    case "path":
        return { entity in
            entity.executablePath?.localizedLowercase.contains(value) == true
                || entity.components.contains { $0.executablePath?.localizedLowercase.contains(value) == true }
        }
    case "cmd", "command":
        return { entity in
            entity.components.contains { $0.commandLine?.localizedLowercase.contains(value) == true }
        }
    case "user":
        return { entity in
            entity.components.contains { $0.user?.localizedLowercase.contains(value) == true }
        }
    case "bundle":
        return { entity in entity.bundleId?.localizedLowercase.contains(value) == true }
    case "badge":
        return { entity in entity.badges.contains { $0.localizedLowercase.contains(value) } }
    default:
        return nil
    }
}

private func parseNumericToken(_ token: String) -> ((EntitySnapshot) -> Bool)? {
    let operators = [">=", "<=", ">", "<", "==", "="]
    guard let op = operators.first(where: { token.contains($0) }),
          let range = token.range(of: op)
    else { return nil }
    let field = String(token[..<range.lowerBound]).localizedLowercase
    let rawValue = String(token[range.upperBound...])
    guard !field.isEmpty, !rawValue.isEmpty,
          let value = parseNumericValue(rawValue, field: field)
    else { return nil }

    func compare(_ lhs: Double) -> Bool {
        switch op {
        case ">": return lhs > value
        case "<": return lhs < value
        case ">=": return lhs >= value
        case "<=": return lhs <= value
        default: return lhs == value
        }
    }

    switch field {
    case "cpu":
        return { compare(Double($0.metrics.cpuPercent)) }
    case "mem", "memory", "ram":
        return { compare(Double($0.metrics.memoryResidentBytes)) }
    case "energy":
        return { compare($0.metrics.energyNjPerS) }
    case "wakeups":
        return { compare(Double($0.metrics.wakeupsPerSecond)) }
    case "friction":
        return { compare(Double($0.friction.totalScore)) }
    case "procs", "processes", "count":
        return { compare(Double($0.metrics.processCount)) }
    case "threads":
        return { compare(Double($0.metrics.threadCount)) }
    case "pid":
        let target = UInt32(value)
        return { entity in entity.components.contains { $0.processId == target } }
    default:
        return nil
    }
}

/// Parse a numeric literal with optional byte units. Memory fields default to
/// MB when unit-less; other fields treat the bare number literally.
private func parseNumericValue(_ raw: String, field: String) -> Double? {
    let lowered = raw.localizedLowercase
    let unitMultipliers: [(String, Double)] = [
        ("gb", 1024 * 1024 * 1024), ("g", 1024 * 1024 * 1024),
        ("mb", 1024 * 1024), ("m", 1024 * 1024),
        ("kb", 1024), ("k", 1024),
        ("b", 1),
    ]
    for (suffix, multiplier) in unitMultipliers where lowered.hasSuffix(suffix) {
        let numberPart = String(lowered.dropLast(suffix.count))
        if let number = Double(numberPart) {
            return number * multiplier
        }
    }
    guard let number = Double(lowered) else { return nil }
    if field == "mem" || field == "memory" || field == "ram" {
        return number * 1024 * 1024 // bare memory number means MB
    }
    return number
}

private func sortGroups(_ groups: [EntityGroup], by sortKey: SortKey) -> [EntityGroup] {
    switch sortKey {
    case .friction:
        return groups.sorted { $0.frictionScore > $1.frictionScore }
    case .cpu:
        return groups.sorted { $0.cpuPercent > $1.cpuPercent }
    case .memory:
        return groups.sorted { $0.memoryBytes > $1.memoryBytes }
    case .wakeups:
        return groups.sorted { $0.wakeupsPerSecond > $1.wakeupsPerSecond }
    case .processCount:
        return groups.sorted { $0.processCount > $1.processCount }
    case .disk:
        return groups.sorted { $0.diskBps > $1.diskBps }
    case .network:
        return groups.sorted { $0.networkBps > $1.networkBps }
    case .energy:
        return groups.sorted { $0.energyScore > $1.energyScore }
    case .alphabeticalAsc:
        return groups.sorted { $0.root.displayName.localizedCaseInsensitiveCompare($1.root.displayName) == .orderedAscending }
    case .alphabeticalDesc:
        return groups.sorted { $0.root.displayName.localizedCaseInsensitiveCompare($1.root.displayName) == .orderedDescending }
    case .oldestFirst:
        return groups.sorted {
            let leftStart = $0.oldestStartMillis == 0 ? UInt64.max : $0.oldestStartMillis
            let rightStart = $1.oldestStartMillis == 0 ? UInt64.max : $1.oldestStartMillis
            return leftStart < rightStart
        }
    case .newestFirst:
        return groups.sorted { $0.newestStartMillis > $1.newestStartMillis }
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
    @State private var processOperatorRequest: ProcessOperatorRequest?
    @State private var advancedFilterText = ""
    @State private var showAdvancedFilter = false
    @FocusState private var searchFieldFocused: Bool

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            monitorOverviewSummary
            monitorSplitView
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, 6)
        .navigationTitle("Monitor")
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
        .onChange(of: state.monitorFocusEntityID) { _, _ in
            applyPendingMonitorFocusIfNeeded()
        }
        .task {
            applyPendingMonitorFocusIfNeeded()
        }
        .onDisappear {
            groupingTask?.cancel()
            groupingTask = nil
            searchFieldFocused = false
        }
    }

    private var rankingPanel: some View {
        ScrollView {
            rankedEntitiesSection
        }
    }

    private var monitorSplitView: some View {
        Group {
            if let entity = selectedEntity {
                HSplitView {
                    rankingPanel
                        .frame(
                            minWidth: 420,
                            idealWidth: 760,
                            maxWidth: .infinity,
                            maxHeight: .infinity
                        )
                        .layoutPriority(2)

                    detailPanel(for: entity)
                        .transition(.opacity)
                        .frame(
                            minWidth: 360,
                            idealWidth: 760,
                            maxWidth: .infinity,
                            maxHeight: .infinity,
                            alignment: .topLeading
                        )
                        .layoutPriority(1)
                }
            } else {
                rankingPanel
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .animation(AetowerDesign.Motion.standard, value: selectedEntity?.entityId)
    }

    private func detailPanel(for entity: EntitySnapshot) -> some View {
        let processTreeEntities = selectedProcessTreeEntities(for: entity)
        let quickStopPID = primaryProcessID(in: processTreeEntities)

        return VStack(spacing: 0) {
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

                if let quickStopPID {
                    sidePanelQuickStop(entity: entity, pid: quickStopPID)
                }
            }
            .padding(.horizontal, AetowerDesign.Spacing.lg)
            .padding(.vertical, AetowerDesign.Spacing.md)

            Divider()

            EntityDetailView(
                entity: entity,
                state: state,
                processTreeSeedEntities: processTreeEntities,
                processOperatorRequest: processOperatorRequest
            )
        }
    }

    private func sidePanelQuickStop(entity: EntitySnapshot, pid: UInt32) -> some View {
        HStack(spacing: 8) {
            Label("Quick stop", systemImage: "hand.raised.fill")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)

            Text("PID \(pid)")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.tertiary)

            Spacer()

            sidePanelStopButton(.terminate, entity: entity, pid: pid)
            sidePanelStopButton(.forceKill, entity: entity, pid: pid)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
    }

    private func sidePanelStopButton(_ action: ProcessActionKind, entity: EntitySnapshot, pid: UInt32) -> some View {
        Button(role: action.isDestructive ? .destructive : nil) {
            requestProcessOperation(
                entityID: entity.entityId,
                pid: pid,
                operation: .previewAction(action)
            )
        } label: {
            Label(action.label, systemImage: action.systemImage)
                .font(.caption.weight(.semibold))
                .foregroundStyle(action == .forceKill ? AetowerDesign.Status.error : AetowerDesign.Status.warning)
        }
        .buttonStyle(.borderless)
    }

    private var rankedEntitiesSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            monitorHeader

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
                }

                Menu {
                    ForEach(SortKey.allCases) { key in
                        Button {
                            sortKey = key
                        } label: {
                            HStack {
                                Text(key.title)
                                if key == sortKey {
                                    Spacer()
                                    Image(systemName: "checkmark")
                                }
                            }
                        }
                    }
                } label: {
                    HStack(spacing: 4) {
                        Text(sortKey.title)
                            .font(.caption.weight(.medium))
                        Image(systemName: "chevron.down")
                            .font(.system(size: 8, weight: .semibold))
                    }
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, AetowerDesign.Spacing.sm)
                    .padding(.vertical, AetowerDesign.Spacing.xs)
                    .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
                }
                .menuStyle(.borderlessButton)
                .fixedSize()

                HStack(spacing: 4) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                    TextField("Search…  cpu>50  path:/Applications  /helper/i", text: $searchText)
                        .textFieldStyle(.plain)
                        .aetowerUtilityTextInput()
                        .focused($searchFieldFocused)
                        .onSubmit { searchFieldFocused = false }
                        .font(.caption)
                }
                .padding(.horizontal, AetowerDesign.Spacing.sm)
                .padding(.vertical, AetowerDesign.Spacing.xs)
                .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
                .frame(maxWidth: 160)

                advancedFilterButton

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
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.bottom, 2)

            HStack(spacing: 6) {
                Text("")
                    .frame(width: 16)
                MonitorHeaderButton(
                    column: .name,
                    isActive: isActiveSortColumn(.name),
                    indicatorSymbol: sortIndicatorSymbol(for: .name),
                    action: { applySort(for: .name) }
                )
                MonitorHeaderButton(
                    column: .cpu,
                    isActive: isActiveSortColumn(.cpu),
                    indicatorSymbol: sortIndicatorSymbol(for: .cpu),
                    action: { applySort(for: .cpu) }
                )
                MonitorHeaderButton(
                    column: .memory,
                    isActive: isActiveSortColumn(.memory),
                    indicatorSymbol: sortIndicatorSymbol(for: .memory),
                    action: { applySort(for: .memory) }
                )
                MonitorHeaderButton(
                    column: .wakeups,
                    isActive: isActiveSortColumn(.wakeups),
                    indicatorSymbol: sortIndicatorSymbol(for: .wakeups),
                    action: { applySort(for: .wakeups) }
                )
                MonitorHeaderButton(
                    column: .processCount,
                    isActive: isActiveSortColumn(.processCount),
                    indicatorSymbol: sortIndicatorSymbol(for: .processCount),
                    action: { applySort(for: .processCount) }
                )
                MonitorHeaderButton(
                    column: .friction,
                    isActive: isActiveSortColumn(.friction),
                    indicatorSymbol: sortIndicatorSymbol(for: .friction),
                    action: { applySort(for: .friction) }
                )
                Text("")
                    .frame(width: 16)
            }
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(.tertiary)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, 2)

            if filteredEntities.isEmpty {
                ContentUnavailableView(
                    "No apps match this filter",
                    systemImage: "magnifyingglass",
                    description: Text("Try a broader query.")
                )
            } else {
                LazyVStack(spacing: 2) {
                    if !burdenLeaderEntities.isEmpty {
                        listSectionHeader("Burden leaders")
                        ForEach(burdenLeaderEntities, id: \.entityId) { entity in
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
                                monitorContextMenu(for: entity, members: [entity])
                            }
                        }
                    }

                    listSectionHeader("All processes")
                    if isGroupedMode {
                        ForEach(allProcessGroups, id: \.id) { group in
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
                            .contextMenu {
                                monitorContextMenu(for: group.root, members: group.members)
                            }
                        }
                    } else {
                        ForEach(allProcessEntities, id: \.entityId) { entity in
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
                                monitorContextMenu(for: entity, members: [entity])
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

    @ViewBuilder
    private func monitorContextMenu(for entity: EntitySnapshot, members: [EntitySnapshot]) -> some View {
        Button("Open Detail") {
            selectEntity(entity.entityId)
        }

        if let pid = primaryProcessID(in: members) {
            Divider()
            Button("Inspect PID \(pid)") {
                requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .inspect)
            }
            Button("Open files & sockets") {
                requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .resources)
            }
            Button("Run 3s sample") {
                requestProcessOperation(entityID: entity.entityId, pid: pid, operation: .sample)
            }
            Divider()
            Button("Terminate PID \(pid)…", role: .destructive) {
                requestProcessOperation(
                    entityID: entity.entityId,
                    pid: pid,
                    operation: .previewAction(.terminate)
                )
            }
            Button("Force kill PID \(pid)…", role: .destructive) {
                requestProcessOperation(
                    entityID: entity.entityId,
                    pid: pid,
                    operation: .previewAction(.forceKill)
                )
            }
            Menu("Actions") {
                ForEach(contextPreviewActions) { action in
                    Button(action.label, role: action.isDestructive ? .destructive : nil) {
                        requestProcessOperation(
                            entityID: entity.entityId,
                            pid: pid,
                            operation: .previewAction(action)
                        )
                    }
                }
            }
        }

        Divider()
        Button("Copy Process IDs") {
            let pids = members
                .flatMap(\.components)
                .compactMap(\.processId)
                .map(String.init)
                .joined(separator: ", ")
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(pids, forType: .string)
        }
        Button("Copy Name") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(entity.displayName, forType: .string)
        }
        if entity.executablePath?.contains(".app/") == true {
            Button("Show in Finder") {
                if let path = entity.executablePath {
                    NSWorkspace.shared.selectFile(path, inFileViewerRootedAtPath: "")
                }
            }
        }
    }

    private var contextPreviewActions: [ProcessActionKind] {
        [.suspend, .resume, .lowerPriority, .normalPriority, .terminate, .forceKill, .terminateTree, .forceKillTree]
    }

    private func selectEntity(_ entityID: String) {
        searchFieldFocused = false
        withAnimation(AetowerDesign.Motion.standard) {
            selectedEntityID = entityID
        }
    }

    private func requestProcessOperation(
        entityID: String,
        pid: UInt32,
        operation: ProcessOperatorQuickOperation
    ) {
        selectEntity(entityID)
        processOperatorRequest = ProcessOperatorRequest(pid: pid, operation: operation)
    }

    private func primaryProcessID(in members: [EntitySnapshot]) -> UInt32? {
        members
            .flatMap(\.components)
            .filter { $0.kind != .adapterContext }
            .compactMap { component -> (pid: UInt32, cpu: Float, memory: UInt64)? in
                guard let pid = component.processId else { return nil }
                return (pid, component.cpuPercent, component.memoryBytes)
            }
            .sorted {
                if $0.cpu != $1.cpu {
                    return $0.cpu > $1.cpu
                }
                return $0.memory > $1.memory
            }
            .first?
            .pid
    }

    private var monitorHeader: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Live control center")
                    .font(.headline)
                Text("Monitor host load, then drill straight into burden leaders and live process groups.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Text("\(visibleEntityIDs.count) \(isGroupedMode ? "groups" : "entities")")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)

            Text("\(visibleProcessCount) PIDs")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.top, AetowerDesign.Spacing.xs)
    }

    private var monitorOverviewSummary: some View {
        let snapshot = state.snapshot
        let hostTrend = snapshot.hostTrend
        let host = snapshot.host
        let frictionScore = machineFrictionScore(for: host)
        let frictionSamples = monitorTrendSamples(
            hostTrend.machineFriction.map { Double($0) },
            history: monitorHostHistorySamples { machineFrictionScore(for: $0) },
            fallback: frictionScore
        )
        let cpuSamples = monitorTrendSamples(
            hostTrend.cpuPercent.map { Double($0) },
            history: monitorHostHistorySamples { Double($0.cpuPercent) },
            fallback: Double(host.cpuPercent)
        )
        let memorySamples = monitorTrendSamples(
            hostTrend.memoryPressureScore.map { Double($0) },
            history: monitorHostHistorySamples { hostMemoryPressureScore($0) },
            fallback: hostMemoryPressureScore(host)
        )
        let diskSamples = monitorTrendSamples(
            hostTrend.diskActivityBps.map { Double($0) },
            history: monitorHostHistorySamples { Double($0.diskReadBps + $0.diskWriteBps) },
            fallback: Double(host.diskReadBps + host.diskWriteBps)
        )
        let networkSamples = monitorTrendSamples(
            hostTrend.networkActivityBps.map { Double($0) },
            history: monitorHostHistorySamples { Double($0.networkReceiveBps + $0.networkSendBps) },
            fallback: Double(host.networkReceiveBps + host.networkSendBps)
        )
        let wakeupSamples = monitorTrendSamples(
            hostTrend.wakeupsPerSecond.map { Double($0) },
            history: monitorHostHistorySamples { Double($0.wakeupsPerSecond) },
            fallback: Double(host.wakeupsPerSecond)
        )
        let gpuPercentSamples = monitorTrendSamples(
            hostTrend.gpuPercent.map { Double($0) },
            history: monitorHostHistorySamples { Double($0.gpuPercent) },
            fallback: Double(host.gpuPercent)
        )
        let gpuMemorySamples = monitorTrendSamples(
            hostTrend.gpuMemoryBytes.map { Double($0) },
            history: monitorHostHistorySamples { Double($0.gpuMemoryBytes) },
            fallback: Double(host.gpuMemoryBytes)
        )

        return LazyVGrid(columns: monitorSummaryGridColumns, alignment: .leading, spacing: 12) {
            TrendMetricCard(
                title: "Friction",
                value: String(format: "%.0f", frictionScore),
                subtitle: "overall machine score · \(trendWindowLabel(sampleCount: frictionSamples.count))",
                samples: frictionSamples,
                style: .friction,
                valueAppearance: monitorFrictionAppearance(frictionScore),
                sampleValueFormatter: { String(format: "%.0f", $0) },
                minHeight: 124
            )
            TrendMetricCard(
                title: "CPU",
                value: String(format: "%.0f%%", host.cpuPercent),
                subtitle: "thermal \(thermalStateLabel(host.thermalState)) · \(trendWindowLabel(sampleCount: cpuSamples.count))",
                samples: cpuSamples,
                style: .cpu,
                valueAppearance: monitorCPUAppearance(host),
                sampleValueFormatter: { String(format: "%.0f%%", $0) },
                minHeight: 124
            )
            TrendMetricCard(
                title: "Memory",
                value: formatBytes(host.memoryUsedBytes),
                subtitle: "\(formatBytes(host.compressedMemoryBytes)) compressed · \(formatBytes(host.swapUsedBytes)) swap · pressure trend",
                samples: memorySamples,
                style: .memory,
                valueAppearance: monitorMemoryAppearance(host),
                // Samples are the 0–100 memory pressure score, not bytes — label
                // the hover stats with /100 so they don't read as bytes/percent.
                sampleValueFormatter: { String(format: "%.0f/100", $0) },
                minHeight: 124
            )
            TrendMetricCard(
                title: "Disk",
                value: formatRate(host.diskReadBps + host.diskWriteBps),
                subtitle: "read + write · \(trendWindowLabel(sampleCount: diskSamples.count))",
                samples: diskSamples,
                style: .disk,
                valueAppearance: monitorThroughputAppearance(host.diskReadBps + host.diskWriteBps, warning: 50 * 1_024 * 1_024, danger: 250 * 1_024 * 1_024),
                sampleValueFormatter: { formatRate(UInt64(max($0, 0))) },
                minHeight: 124
            )
            TrendMetricCard(
                title: "Network",
                value: formatRate(host.networkReceiveBps + host.networkSendBps),
                subtitle: "send + receive · \(trendWindowLabel(sampleCount: networkSamples.count))",
                samples: networkSamples,
                style: .network,
                valueAppearance: monitorThroughputAppearance(host.networkReceiveBps + host.networkSendBps, warning: 10 * 1_024 * 1_024, danger: 100 * 1_024 * 1_024),
                sampleValueFormatter: { formatRate(UInt64(max($0, 0))) },
                minHeight: 124
            )
            TrendMetricCard(
                title: "Wakeups",
                value: formatWakeups(host.wakeupsPerSecond),
                subtitle: hostWakeupBand(host.wakeupsPerSecond) == .severe
                    ? "wakeup storm band · \(trendWindowLabel(sampleCount: wakeupSamples.count))"
                    : "current host wakeup rate · \(trendWindowLabel(sampleCount: wakeupSamples.count))",
                samples: wakeupSamples,
                style: .friction,
                valueAppearance: monitorWakeupAppearance(host),
                sampleValueFormatter: { formatWakeups(Float($0)) },
                minHeight: 124
            )
            if host.gpuPercent > 0 || host.gpuMemoryBytes > 0 {
                let usingGpuPercent = host.gpuPercent > 0
                TrendMetricCard(
                    title: "GPU",
                    value: usingGpuPercent
                        ? String(format: "%.0f%%", host.gpuPercent)
                        : formatBytes(host.gpuMemoryBytes),
                    subtitle: "\(hostGPUSummary(host)) · \(trendWindowLabel(sampleCount: usingGpuPercent ? gpuPercentSamples.count : gpuMemorySamples.count))",
                    samples: usingGpuPercent ? gpuPercentSamples : gpuMemorySamples,
                    style: .energy,
                    valueAppearance: monitorGPUAppearance(host),
                    sampleValueFormatter: usingGpuPercent
                        ? { String(format: "%.0f%%", $0) }
                        : { formatBytes(UInt64(max($0, 0))) },
                    minHeight: 124
                )
            }
        }
    }

    private var advancedFilterButton: some View {
        Button {
            showAdvancedFilter.toggle()
        } label: {
            Image(systemName: state.advancedFilterEntityIds == nil
                ? "line.3.horizontal.decrease.circle"
                : "line.3.horizontal.decrease.circle.fill")
                .font(.system(size: 12))
                .foregroundStyle(state.advancedFilterEntityIds == nil ? Color.secondary : Color.accentColor)
        }
        .buttonStyle(.plain)
        .help("Advanced filter (Rhai expression)")
        .popover(isPresented: $showAdvancedFilter, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 10) {
                Text("Advanced filter")
                    .font(.headline)
                Text("Boolean expression evaluated per process. Fields: name, path, cmd, user, cwd, entity, bundle, badges, pid, cpu, mem, mem_mb, threads, energy, friction.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                TextField("cpu > 20 && name.contains(\"Helper\")", text: $advancedFilterText, axis: .vertical)
                    .textFieldStyle(.roundedBorder)
                    .lineLimit(2...4)
                    .font(.system(size: 11, design: .monospaced))
                    .aetowerUtilityTextInput()
                if let error = state.advancedFilterError {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                } else if let summary = state.advancedFilterSummary {
                    Text(summary)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                HStack {
                    Button("Apply") {
                        state.applyAdvancedFilter(advancedFilterText)
                    }
                    .keyboardShortcut(.return, modifiers: [])
                    Button("Clear") {
                        advancedFilterText = ""
                        state.clearAdvancedFilter()
                    }
                    .disabled(state.advancedFilterEntityIds == nil && advancedFilterText.isEmpty)
                    Spacer()
                }
            }
            .padding(14)
            .frame(width: 320)
        }
    }

    private var filteredEntities: [EntitySnapshot] {
        filterEntities(applyAdvancedFilter(state.snapshot.entities), query: normalizedSearchQuery).sorted {
            compareEntities($0, $1, by: sortKey)
        }
    }

    private var burdenLeaderEntities: [EntitySnapshot] {
        var seen = Set<String>()
        let ids = buildBurdenLeaders(snapshot: state.snapshot).map(\.entityId).filter { seen.insert($0).inserted }
        return ids.compactMap { id in
            state.snapshot.entities.first(where: { $0.entityId == id })
        }
    }

    private var burdenLeaderEntityIDs: Set<String> {
        Set(burdenLeaderEntities.map(\.entityId))
    }

    private var allProcessEntities: [EntitySnapshot] {
        filteredEntities.filter { !burdenLeaderEntityIDs.contains($0.entityId) }
    }

    private var allProcessGroups: [EntityGroup] {
        groupedEntities.filter { !burdenLeaderEntityIDs.contains($0.root.entityId) }
    }

    private func applySort(for column: MonitorSortColumn) {
        switch column {
        case .name:
            sortKey = sortKey == .alphabeticalAsc ? .alphabeticalDesc : .alphabeticalAsc
        case .cpu:
            sortKey = .cpu
        case .memory:
            sortKey = .memory
        case .wakeups:
            sortKey = .wakeups
        case .processCount:
            sortKey = .processCount
        case .friction:
            sortKey = .friction
        }
    }

    private func isActiveSortColumn(_ column: MonitorSortColumn) -> Bool {
        switch column {
        case .name:
            return sortKey == .alphabeticalAsc || sortKey == .alphabeticalDesc
        case .cpu:
            return sortKey == .cpu
        case .memory:
            return sortKey == .memory
        case .wakeups:
            return sortKey == .wakeups
        case .processCount:
            return sortKey == .processCount
        case .friction:
            return sortKey == .friction
        }
    }

    private func sortIndicatorSymbol(for column: MonitorSortColumn) -> String? {
        guard isActiveSortColumn(column) else { return nil }
        switch column {
        case .name:
            return sortKey == .alphabeticalAsc ? "arrow.up" : "arrow.down"
        case .cpu, .memory, .wakeups, .processCount, .friction:
            return "arrow.down"
        }
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
        let burdenLeaderIDs = burdenLeaderEntities.map(\.entityId)
        if isGroupedMode {
            return burdenLeaderIDs + allProcessGroups.map(\.root.entityId)
        }
        return burdenLeaderIDs + allProcessEntities.map(\.entityId)
    }

    private var visibleProcessCount: Int {
        if isGroupedMode {
            let burdenLeaderProcessCount = burdenLeaderEntities.reduce(0) { total, entity in
                total + entity.components.filter { $0.kind != .adapterContext && $0.processId != nil }.count
            }
            return burdenLeaderProcessCount + allProcessGroups.reduce(0) { $0 + $1.processCount }
        }
        return burdenLeaderEntities.reduce(0) { total, entity in
            total + entity.components.filter { $0.kind != .adapterContext && $0.processId != nil }.count
        } + allProcessEntities.reduce(0) { total, entity in
            total + entity.components.filter { $0.kind != .adapterContext && $0.processId != nil }.count
        }
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
            sortKey: sortKey,
            filterSignature: advancedFilterSignature
        )
    }

    /// Stable signature of the active advanced filter so the grouping cache
    /// invalidates when the filter set changes ("" = no filter).
    private var advancedFilterSignature: String {
        guard let ids = state.advancedFilterEntityIds else { return "" }
        return ids.sorted().joined(separator: ",")
    }

    private func applyAdvancedFilter(_ entities: [EntitySnapshot]) -> [EntitySnapshot] {
        guard let ids = state.advancedFilterEntityIds else { return entities }
        return entities.filter { ids.contains($0.entityId) }
    }

    private var isGroupedMode: Bool {
        listMode == .grouped
    }

    private var groupingTaskToken: String {
        guard let key = currentGroupingCacheKey else { return "flat" }
        return "\(key.sequence)|\(key.query)|\(key.sortKey.rawValue)"
    }

    private func listSectionHeader(_ title: String) -> some View {
        HStack(spacing: 8) {
            SectionEyebrow(text: title)
            Rectangle()
                .fill(Color.secondary.opacity(0.12))
                .frame(height: 1)
        }
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    private var monitorTrendSampleLimit: Int {
        150
    }

    private var monitorSummaryGridColumns: [GridItem] {
        [GridItem(.adaptive(minimum: 180, maximum: 420), spacing: AetowerDesign.Spacing.md, alignment: .top)]
    }

    private func monitorHostHistorySamples(_ extractor: (HostSnapshot) -> Double) -> [Double] {
        let samples = state.historySnapshots.suffix(monitorTrendSampleLimit).map { extractor($0.host) }
        return samples.isEmpty ? [extractor(state.snapshot.host)] : samples
    }

    private func monitorTrendSamples(_ live: [Double], history: [Double], fallback: Double) -> [Double] {
        let trimmedLive = Array(live.suffix(monitorTrendSampleLimit))
        if monitorHasUsefulVariation(trimmedLive) {
            return trimmedLive
        }
        let trimmedHistory = Array(history.suffix(monitorTrendSampleLimit))
        if monitorHasUsefulVariation(trimmedHistory) {
            return trimmedHistory
        }
        if trimmedLive.count >= 2 {
            return trimmedLive
        }
        if let sample = trimmedLive.first {
            return [sample, sample]
        }
        if let sample = trimmedHistory.first {
            return [sample, sample]
        }
        return [fallback, fallback]
    }

    private func monitorHasUsefulVariation(_ samples: [Double]) -> Bool {
        guard samples.count >= 2, let minimum = samples.min(), let maximum = samples.max() else {
            return false
        }
        let baseline = max(abs(samples.last ?? 0), 1.0)
        return (maximum - minimum) / baseline > 0.01
    }

    private func monitorFrictionAppearance(_ score: Double) -> TrendMetricValueAppearance {
        if score >= 60 { return .danger }
        if score >= 30 { return .warning }
        return .ok
    }

    private func monitorCPUAppearance(_ host: HostSnapshot) -> TrendMetricValueAppearance {
        if host.cpuPercent >= 85 || host.thermalState == .critical { return .danger }
        if host.cpuPercent >= 50 || host.thermalState == .serious { return .warning }
        return .ok
    }

    private func monitorMemoryAppearance(_ host: HostSnapshot) -> TrendMetricValueAppearance {
        switch hostPressureBand(host) {
        case .severe: return .danger
        case .elevated: return .warning
        case .nominal: return .ok
        }
    }

    private func monitorThroughputAppearance(_ bytesPerSecond: UInt64, warning: UInt64, danger: UInt64) -> TrendMetricValueAppearance {
        if bytesPerSecond >= danger { return .danger }
        if bytesPerSecond >= warning { return .warning }
        return .ok
    }

    private func monitorWakeupAppearance(_ host: HostSnapshot) -> TrendMetricValueAppearance {
        switch hostWakeupBand(host.wakeupsPerSecond) {
        case .severe: return .danger
        case .elevated: return .warning
        case .nominal: return .ok
        }
    }

    private func monitorGPUAppearance(_ host: HostSnapshot) -> TrendMetricValueAppearance {
        if host.gpuPercent >= 85 || host.gpuMemoryBytes >= 8 * 1_024 * 1_024 * 1_024 {
            return .danger
        }
        if host.gpuPercent >= 40 || host.gpuMemoryBytes >= 2 * 1_024 * 1_024 * 1_024 {
            return .warning
        }
        return .ok
    }

    private func applyPendingMonitorFocusIfNeeded() {
        guard let requestedEntityID = state.consumeMonitorFocusEntityID() else {
            return
        }
        searchText = ""
        focusedIndex = 0
        searchFieldFocused = false

        if isGroupedMode,
           let group = groupedEntities.first(where: { group in
               group.members.contains(where: { $0.entityId == requestedEntityID })
           }) {
            selectedEntityID = group.root.entityId
            return
        }

        if state.snapshot.entities.contains(where: { $0.entityId == requestedEntityID }) {
            listMode = .flat
            selectedEntityID = requestedEntityID
        }
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

        let entities = applyAdvancedFilter(state.snapshot.entities)
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
            return "\(entity.displayName) is currently highest by charged-memory load at \(String(format: "%.1f%%", entityMemoryLoadPercent(entity, totalBytes: state.snapshot.host.memoryTotalBytes)))."
        case .wakeups:
            return "\(entity.displayName) is currently highest by wakeup rate at \(formatWakeups(entity.metrics.wakeupsPerSecond))."
        case .processCount:
            return "\(entity.displayName) currently owns the widest live process footprint at \(liveProcessCount(for: entity)) PIDs."
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

private func compareEntities(_ left: EntitySnapshot, _ right: EntitySnapshot, by sortKey: SortKey) -> Bool {
    switch sortKey {
    case .friction:
        return Double(left.friction.totalScore) > Double(right.friction.totalScore)
    case .cpu:
        return Double(left.metrics.cpuPercent) > Double(right.metrics.cpuPercent)
    case .memory:
        return entityEffectiveMemoryBytes(left) > entityEffectiveMemoryBytes(right)
    case .wakeups:
        return Double(left.metrics.wakeupsPerSecond) > Double(right.metrics.wakeupsPerSecond)
    case .processCount:
        return liveProcessCount(for: left) > liveProcessCount(for: right)
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

private func liveProcessCount(for entity: EntitySnapshot) -> Int {
    entity.components.filter { $0.kind != .adapterContext && $0.processId != nil }.count
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
