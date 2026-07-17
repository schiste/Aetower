import SwiftUI
import AetowerBridge

struct ProcessOriginChip: View {
    let summary: ProcessOriginSummary

    var body: some View {
        Label(summary.chipLabel, systemImage: summary.kind.systemImage)
            .font(.system(size: 9, weight: .bold))
            .foregroundStyle(summary.kind.tint)
            .lineLimit(1)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(summary.kind.tint.opacity(0.14), in: Capsule())
            .help(summary.detailLines.joined(separator: "\n"))
    }
}

enum MonitorRowLayout {
    static let height: CGFloat = 30
}

enum MonitorRowTrendDirection: Equatable {
    case up
    case down
    case flat

    var symbol: String {
        switch self {
        case .up: return "arrow.up"
        case .down: return "arrow.down"
        case .flat: return "minus"
        }
    }

    var color: Color {
        switch self {
        case .up: return .red
        case .down: return .green
        case .flat: return .secondary
        }
    }
}

enum MonitorRowReputationTone: Equatable {
    case error
    case warning
    case success

    var color: Color {
        switch self {
        case .error: return AetowerDesign.Status.error
        case .warning: return AetowerDesign.Status.warning
        case .success: return AetowerDesign.Status.success
        }
    }
}

struct MonitorEntityRowModel: Identifiable, Equatable {
    let id: String
    let entity: EntitySnapshot
    let origin: ProcessOriginSummary
    let iconName: String
    let displayName: String
    let cpuText: String
    let memoryText: String
    let wakeupsText: String
    let processCountText: String
    let frictionText: String
    let frictionScore: Float
    let trendDirection: MonitorRowTrendDirection
    let isNewlyLaunched: Bool
    let isForeground: Bool
    let anomalyDetected: Bool
    let remoteHostCount: Int
    let remoteHostsHelp: String?
    let reputationLabel: String?
    let reputationTone: MonitorRowReputationTone?
    let helpText: String

    init(entity: EntitySnapshot, origin: ProcessOriginSummary, nowMillis: UInt64) {
        self.id = entity.entityId
        self.entity = entity
        self.origin = origin
        self.iconName = Self.iconName(for: entity.entityKind)
        self.displayName = entity.displayName
        self.cpuText = String(format: "%.1f%%", entity.metrics.cpuPercent)
        self.memoryText = formatBytes(entityEffectiveMemoryBytes(entity))
        self.wakeupsText = formatWakeups(entity.metrics.wakeupsPerSecond)
        self.processCountText = "\(visibleProcessComponentCount(entity))"
        self.frictionText = String(format: "%.1f", entity.friction.totalScore)
        self.frictionScore = entity.friction.totalScore
        self.trendDirection = Self.trendDirection(for: entity.trend.friction)
        self.isNewlyLaunched = Self.isNewlyLaunched(entity: entity, nowMillis: nowMillis)
        self.isForeground = entity.metrics.isForeground
        self.anomalyDetected = entity.anomalyDetected

        let remoteHosts = Self.distinctRemoteHosts(for: entity)
        self.remoteHostCount = remoteHosts.count
        self.remoteHostsHelp = remoteHosts.isEmpty
            ? nil
            : "Talking to \(remoteHosts.prefix(5).joined(separator: ", "))"

        let reputation = Self.reputationChip(for: entity)
        self.reputationLabel = reputation?.label
        self.reputationTone = reputation?.tone
        self.helpText = Self.helpText(for: entity, origin: origin)
    }

    private static func isNewlyLaunched(entity: EntitySnapshot, nowMillis: UInt64) -> Bool {
        let start = entity.newestProcessStartMillis
        guard start > 0 else { return false }
        return nowMillis >= start && nowMillis - start <= 30_000
    }

    private static func distinctRemoteHosts(for entity: EntitySnapshot) -> [String] {
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

    private static func reputationChip(
        for entity: EntitySnapshot
    ) -> (label: String, tone: MonitorRowReputationTone)? {
        guard let reputation = entity.binaryReputation else { return nil }
        let detections = reputation.malicious + reputation.suspicious
        switch reputation.verdict {
        case .malicious:
            return ("VT \(detections)/\(reputation.totalEngines)", .error)
        case .suspicious:
            return ("VT \(detections)/\(reputation.totalEngines)", .warning)
        case .clean:
            return ("VT clean", .success)
        case .unknown:
            return nil
        @unknown default:
            return nil
        }
    }

    private static func helpText(for entity: EntitySnapshot, origin: ProcessOriginSummary) -> String {
        let entityUser = entity.components
            .lazy
            .compactMap(\.user)
            .first(where: { !$0.isEmpty }) ?? ""
        let entityParent = entity.components.first?.parentSummary ?? ""

        return [
            entityUser.isEmpty ? nil : "User: \(entityUser)",
            entityParent.isEmpty ? nil : "Parent: \(entityParent)",
            "Processes: \(visibleProcessComponentCount(entity))",
            "Wakeups: \(formatWakeups(entity.metrics.wakeupsPerSecond))",
            entity.recentChangeSummary,
            origin.subtitle,
            entity.launcherSummary.map { "Launch lineage: \($0)" },
            entity.attributionNotes.first,
        ]
        .compactMap { $0 }
        .joined(separator: "\n")
    }

    private static func trendDirection(for samples: [Float]) -> MonitorRowTrendDirection {
        guard samples.count >= 3 else { return .flat }
        let recent = samples.suffix(3)
        let delta = (recent.last ?? 0) - (recent.first ?? 0)
        if delta > 2 { return .up }
        if delta < -2 { return .down }
        return .flat
    }

    static func iconName(for kind: EntityKind) -> String {
        switch kind {
        case .app: return "app.fill"
        case .browser: return "globe"
        case .daemon: return "gearshape.2.fill"
        case .terminalSession: return "terminal.fill"
        case .aiAgent: return "cpu.fill"
        case .service: return "server.rack"
        case .unknown: return "questionmark.circle"
        }
    }
}

struct MonitorGroupRowModel: Identifiable, Equatable {
    let id: String
    let group: EntityGroup
    let origin: ProcessOriginSummary
    let iconName: String
    let displayName: String
    let memberOverflowText: String?
    let burdenLeaderText: String?
    let burdenLeaderHelpText: String?
    let burdenLeaderSeverity: OperatorSeverity?
    let cpuText: String
    let memoryText: String
    let wakeupsText: String
    let processCountText: String
    let frictionText: String
    let frictionScore: Float
    let helpText: String

    init(group: EntityGroup, origin: ProcessOriginSummary, burdenLeaders: [BurdenLeaderSummary] = []) {
        let leaderAnnotation = Self.burdenLeaderAnnotation(for: burdenLeaders)
        self.id = group.id
        self.group = group
        self.origin = origin
        self.iconName = MonitorEntityRowModel.iconName(for: group.root.entityKind)
        self.displayName = group.root.displayName
        self.memberOverflowText = group.members.count > 1 ? "+\(group.members.count - 1)" : nil
        self.burdenLeaderText = leaderAnnotation.text
        self.burdenLeaderHelpText = leaderAnnotation.helpText
        self.burdenLeaderSeverity = leaderAnnotation.severity
        self.cpuText = String(format: "%.1f%%", group.cpuPercent)
        self.memoryText = formatBytes(group.memoryBytes)
        self.wakeupsText = formatWakeups(group.wakeupsPerSecond)
        self.processCountText = "\(group.processCount)"
        self.frictionText = String(format: "%.1f", group.frictionScore)
        self.frictionScore = group.frictionScore
        self.helpText = Self.helpText(for: group, origin: origin)
    }

    private static func burdenLeaderAnnotation(
        for burdenLeaders: [BurdenLeaderSummary]
    ) -> (text: String?, helpText: String?, severity: OperatorSeverity?) {
        var seen = Set<String>()
        let uniqueLeaders = burdenLeaders.filter { leader in
            seen.insert(leader.id).inserted
        }
        guard !uniqueLeaders.isEmpty else {
            return (nil, nil, nil)
        }

        let sorted = uniqueLeaders.sorted {
            if $0.severity != $1.severity {
                return $0.severity > $1.severity
            }
            return $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
        }
        let severity = sorted.map(\.severity).max()
        let text: String
        if let leader = sorted.first, sorted.count == 1 {
            text = "\(leader.title.replacingOccurrences(of: " leader", with: "")): \(leader.metricValue)"
        } else {
            text = "\(sorted.count) leaders"
        }
        let helpText = sorted.map { leader in
            "\(leader.title): \(leader.entityName) \(leader.metricValue). \(leader.detail)"
        }.joined(separator: "\n")
        return (text, helpText, severity)
    }

    private static func helpText(for group: EntityGroup, origin: ProcessOriginSummary) -> String {
        let names = group.members.prefix(6).map(\.displayName).joined(separator: ", ")
        let user = group.userSummary.isEmpty ? "" : " · user \(group.userSummary)"
        let metrics = "\(formatWakeups(group.wakeupsPerSecond)) · \(formatRate(group.diskBps)) disk · \(formatRate(group.networkBps)) network · \(origin.subtitle)\(user)"
        let aggregate = "Aggregate burden: \(String(format: "%.1f", group.frictionScore))"
        if group.members.count > 6 {
            return "\(aggregate)\nCollapsed entities: \(names), +\(group.members.count - 6) more · \(group.processCount) grouped processes · \(metrics)"
        }
        if group.members.count > 1 {
            return "\(aggregate)\nCollapsed entities: \(names) · \(group.processCount) grouped processes · \(metrics)"
        }
        return "\(aggregate)\nEntity footprint: \(group.processCount) grouped processes · \(metrics)"
    }
}

struct MonitorProcessRowModel: Identifiable, Equatable {
    let id: String
    let ownerEntityID: String
    let owner: EntitySnapshot
    let commandLine: String?
    let pid: UInt32
    let displayName: String
    let subtitle: String
    let cpuText: String
    let memoryText: String
    let threadsText: String
    let pidText: String
    let frictionScore: Float
    let helpText: String

    init(reference: MonitorProcessComponentRef) {
        self.id = reference.id
        self.ownerEntityID = reference.owner.entityId
        self.owner = reference.owner
        self.commandLine = reference.commandLine
        self.pid = reference.pid
        self.displayName = reference.title.isEmpty
            ? reference.owner.displayName
            : reference.title
        self.subtitle = Self.subtitle(for: reference)
        self.cpuText = String(format: "%.1f%%", reference.cpuPercent)
        self.memoryText = formatBytes(
            max(reference.memoryPhysicalFootprintBytes, reference.memoryBytes)
        )
        self.threadsText = "\(reference.threadCount) th"
        self.pidText = processPIDLabel(reference.pid)
        self.frictionScore = reference.owner.friction.totalScore
        self.helpText = Self.helpText(for: reference)
    }

    private static func subtitle(for reference: MonitorProcessComponentRef) -> String {
        if let cwd = reference.cwd, !cwd.isEmpty {
            return "\(reference.owner.displayName) · \(cwd)"
        }
        if let user = reference.user, !user.isEmpty {
            return "\(reference.owner.displayName) · \(user)"
        }
        return reference.owner.displayName
    }

    private static func helpText(for reference: MonitorProcessComponentRef) -> String {
        return [
            "Owner: \(reference.owner.displayName)",
            processPIDLabel(reference.pid),
            reference.parentPid.map { "Parent \(processPIDLabel($0))" },
            reference.user.map { "User: \($0)" },
            reference.cwd.map { "CWD: \($0)" },
            reference.commandLine.map { "Command: \($0)" },
            "Lineage: \(reference.source) · \(Int((reference.confidence * 100).rounded()))% confidence",
        ]
        .compactMap { $0 }
        .joined(separator: "\n")
    }
}

struct EntityRow: View, Equatable {
    let row: MonitorEntityRowModel
    let isSelected: Bool

    nonisolated static func == (lhs: EntityRow, rhs: EntityRow) -> Bool {
        lhs.row == rhs.row && lhs.isSelected == rhs.isSelected
    }

    var body: some View {
        MonitorRowChrome(
            frictionScore: row.frictionScore,
            isSelected: isSelected,
            helpText: row.helpText
        ) { _ in
            Image(systemName: row.iconName)
                .font(.system(size: 11))
                .foregroundStyle(AetowerDesign.frictionColor(row.frictionScore).opacity(0.8))
                .frame(width: 16)

            Text(row.displayName)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)

            ProcessOriginChip(summary: row.origin)

            entityBadges

            MonitorMetricColumns(
                cpuText: row.cpuText,
                memoryText: row.memoryText,
                wakeupsText: row.wakeupsText,
                processCountText: row.processCountText
            ) {
                HStack(spacing: 2) {
                    Image(systemName: row.trendDirection.symbol)
                        .font(.system(size: 8))
                        .foregroundStyle(row.trendDirection.color)
                    Text(row.frictionText)
                        .font(.system(size: 12, weight: .bold, design: .rounded))
                        .foregroundStyle(AetowerDesign.frictionColor(row.frictionScore))
                        .contentTransition(.numericText())
                }
            }
        }
    }

    @ViewBuilder
    private var entityBadges: some View {
        if row.isNewlyLaunched {
            Text("NEW")
                .font(.system(size: 8, weight: .bold))
                .foregroundStyle(AetowerDesign.Status.success)
                .padding(.horizontal, 4)
                .padding(.vertical, 1)
                .background(AetowerDesign.Status.success.opacity(0.15), in: Capsule())
        }

        if row.isForeground {
            Circle().fill(.blue).frame(width: 5, height: 5)
        }

        if row.anomalyDetected {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 9))
                .foregroundStyle(.orange)
                .symbolEffect(.pulse.wholeSymbol, isActive: true)
        }

        if row.remoteHostCount > 0 {
            HStack(spacing: 2) {
                Image(systemName: "network")
                    .font(.system(size: 8))
                Text("\(row.remoteHostCount)")
                    .font(.system(size: 9, weight: .medium, design: .monospaced))
            }
            .foregroundStyle(AetowerDesign.Tone.network)
            .help(row.remoteHostsHelp ?? "")
        }

        if let reputationLabel = row.reputationLabel, let reputationTone = row.reputationTone {
            Text(reputationLabel)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(reputationTone.color)
                .lineLimit(1)
                .padding(.horizontal, 4)
                .padding(.vertical, 1)
                .background(reputationTone.color.opacity(0.15), in: Capsule())
                .help("VirusTotal reputation for this binary.")
        }
    }
}

struct MonitorProcessRow: View, Equatable {
    let row: MonitorProcessRowModel
    let isSelected: Bool

    nonisolated static func == (lhs: MonitorProcessRow, rhs: MonitorProcessRow) -> Bool {
        lhs.row == rhs.row && lhs.isSelected == rhs.isSelected
    }

    var body: some View {
        MonitorRowChrome(
            frictionScore: row.frictionScore,
            isSelected: isSelected,
            helpText: row.helpText
        ) { _ in
            Image(systemName: "cpu")
                .font(AetowerDesign.Typography.compactData(size: 11))
                .foregroundStyle(AetowerDesign.Ink.tertiary)
                .frame(width: 16)

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text(row.displayName)
                    .font(AetowerDesign.Typography.caption.weight(.medium))
                    .foregroundStyle(AetowerDesign.Ink.primary)
                    .lineLimit(1)

                Text(row.subtitle)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            Text(row.cpuText)
                .font(AetowerDesign.Typography.data)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(1)
                .frame(width: 64, alignment: .center)

            Text(row.memoryText)
                .font(AetowerDesign.Typography.data)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(1)
                .frame(width: 84, alignment: .center)

            Text(row.threadsText)
                .font(AetowerDesign.Typography.compactData())
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(1)
                .frame(width: 72, alignment: .center)

            Text(row.pidText)
                .font(AetowerDesign.Typography.compactData())
                .foregroundStyle(AetowerDesign.Ink.tertiary)
                .lineLimit(1)
                .frame(width: 72, alignment: .center)
        }
    }
}

struct GroupedEntityRow: View, Equatable {
    let row: MonitorGroupRowModel
    let isSelected: Bool
    let isExpanded: Bool
    let canExpand: Bool
    let onToggleExpansion: () -> Void

    nonisolated static func == (lhs: GroupedEntityRow, rhs: GroupedEntityRow) -> Bool {
        lhs.row == rhs.row
            && lhs.isSelected == rhs.isSelected
            && lhs.isExpanded == rhs.isExpanded
            && lhs.canExpand == rhs.canExpand
    }

    var body: some View {
        MonitorRowChrome(
            frictionScore: row.frictionScore,
            isSelected: isSelected,
            helpText: row.helpText,
            trailingAction: disclosureAction
        ) { _ in
            Image(systemName: row.iconName)
                .font(.system(size: 11))
                .foregroundStyle(AetowerDesign.frictionColor(row.frictionScore).opacity(0.8))
                .frame(width: 16)

            Text(row.displayName)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)

            ProcessOriginChip(summary: row.origin)

            if let burdenLeaderText = row.burdenLeaderText {
                AetowerBadge(
                    burdenLeaderText,
                    tone: row.burdenLeaderSeverity?.color ?? AetowerDesign.Status.neutral,
                    size: .compact
                )
                    .help(row.burdenLeaderHelpText ?? "Burden leader in this grouped process tree.")
            }

            if let memberOverflowText = row.memberOverflowText {
                Text(memberOverflowText)
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(Color.secondary.opacity(0.12), in: Capsule())
            }

            MonitorMetricColumns(
                cpuText: row.cpuText,
                memoryText: row.memoryText,
                wakeupsText: row.wakeupsText,
                processCountText: row.processCountText
            ) {
                HStack(spacing: 2) {
                    Image(systemName: "arrow.triangle.branch")
                        .font(.system(size: 8))
                        .foregroundStyle(.secondary)
                    Text(row.frictionText)
                        .font(.system(size: 12, weight: .bold, design: .rounded))
                        .foregroundStyle(AetowerDesign.frictionColor(row.frictionScore))
                        .contentTransition(.numericText())
                }
                .help("Aggregate burden for grouped processes.")
            }
        }
    }

    private var disclosureAction: MonitorRowTrailingAction? {
        guard canExpand else { return nil }
        return MonitorRowTrailingAction(
            systemImage: isExpanded ? "chevron.down" : "chevron.right",
            accessibilityLabel: isExpanded ? "Collapse grouped processes" : "Expand grouped processes",
            help: isExpanded ? "Collapse grouped processes" : "Show grouped processes inline",
            action: onToggleExpansion
        )
    }
}

private struct MonitorRowTrailingAction {
    let systemImage: String
    let accessibilityLabel: String
    let help: String
    let action: () -> Void
}

private struct MonitorRowChrome<Content: View>: View {
    let frictionScore: Float
    let isSelected: Bool
    let helpText: String
    let trailingAction: MonitorRowTrailingAction?
    @ViewBuilder var content: (Bool) -> Content
    @State private var isHovered = false

    init(
        frictionScore: Float,
        isSelected: Bool,
        helpText: String,
        trailingAction: MonitorRowTrailingAction? = nil,
        @ViewBuilder content: @escaping (Bool) -> Content
    ) {
        self.frictionScore = frictionScore
        self.isSelected = isSelected
        self.helpText = helpText
        self.trailingAction = trailingAction
        self.content = content
    }

    var body: some View {
        HStack(spacing: 6) {
            content(isSelected || isHovered)

            trailingControl(isActive: isSelected || isHovered)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .frame(maxWidth: .infinity)
        .frame(height: MonitorRowLayout.height)
        .background(
            AetowerDesign.frictionColor(frictionScore)
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

    @ViewBuilder
    private func trailingControl(isActive: Bool) -> some View {
        if let trailingAction {
            Button(action: trailingAction.action) {
                Image(systemName: trailingAction.systemImage)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(isActive ? Color.accentColor : Color.secondary.opacity(0.65))
                    .frame(width: 16)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(trailingAction.accessibilityLabel)
            .help(trailingAction.help)
        } else {
            Image(systemName: "sidebar.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(isActive ? Color.accentColor : Color.secondary.opacity(0.55))
                .frame(width: 16)
        }
    }

    private var frictionBackgroundOpacity: Double {
        let base = min(Double(frictionScore) / 100.0, 1.0)
        if isSelected { return base * 0.12 + 0.04 }
        if isHovered { return base * 0.08 + 0.02 }
        return base * 0.05
    }
}

private struct MonitorMetricColumns<Friction: View>: View {
    let cpuText: String
    let memoryText: String
    let wakeupsText: String
    let processCountText: String
    @ViewBuilder var friction: () -> Friction

    var body: some View {
        Text(cpuText)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: 64, alignment: .center)

        Text(memoryText)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: 84, alignment: .center)

        Text(wakeupsText)
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: 72, alignment: .center)

        Label(processCountText, systemImage: "number")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(.tertiary)
            .labelStyle(.titleAndIcon)
            .lineLimit(1)
            .frame(width: 56, alignment: .center)

        friction()
            .frame(width: 68, alignment: .center)
    }
}
