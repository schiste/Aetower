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

private struct SidePanelQuickStopSubmission: Equatable {
    let entityID: String
    let pid: UInt32
    let action: ProcessActionKind
    let actionID: String
    let submittedAt: Date
}

private struct SidePanelQuickStatusLine: View {
    let icon: String
    let color: Color
    let text: String

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            Image(systemName: icon)
                .foregroundStyle(color)
                .frame(width: 14)
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
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

private struct ProcessOriginChip: View {
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

private struct RowFrictionHighlights {
    let title: Bool
    let cpu: Bool
    let memory: Bool
    let disk: Bool
    let network: Bool
    let wakeups: Bool

    static let none = Self(title: false, cpu: false, memory: false, disk: false, network: false, wakeups: false)
}

private enum MonitorRowLayout {
    static let height: CGFloat = 30
}

private enum MonitorRowTrendDirection: Equatable {
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

private enum MonitorRowReputationTone: Equatable {
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

private struct MonitorEntityRowModel: Identifiable, Equatable {
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

private struct MonitorGroupRowModel: Identifiable, Equatable {
    let id: String
    let group: EntityGroup
    let origin: ProcessOriginSummary
    let iconName: String
    let displayName: String
    let memberOverflowText: String?
    let cpuText: String
    let memoryText: String
    let wakeupsText: String
    let processCountText: String
    let frictionText: String
    let frictionScore: Float
    let helpText: String

    init(group: EntityGroup, origin: ProcessOriginSummary) {
        self.id = group.id
        self.group = group
        self.origin = origin
        self.iconName = MonitorEntityRowModel.iconName(for: group.root.entityKind)
        self.displayName = group.root.displayName
        self.memberOverflowText = group.members.count > 1 ? "+\(group.members.count - 1)" : nil
        self.cpuText = String(format: "%.1f%%", group.cpuPercent)
        self.memoryText = formatBytes(group.memoryBytes)
        self.wakeupsText = formatWakeups(group.wakeupsPerSecond)
        self.processCountText = "\(group.processCount)"
        self.frictionText = String(format: "%.1f", group.frictionScore)
        self.frictionScore = group.frictionScore
        self.helpText = Self.helpText(for: group, origin: origin)
    }

    private static func helpText(for group: EntityGroup, origin: ProcessOriginSummary) -> String {
        let names = group.members.prefix(6).map(\.displayName).joined(separator: ", ")
        let user = group.userSummary.isEmpty ? "" : " · user \(group.userSummary)"
        let metrics = "\(formatWakeups(group.wakeupsPerSecond)) · \(formatRate(group.diskBps)) disk · \(formatRate(group.networkBps)) network · \(origin.subtitle)\(user)"
        if group.members.count > 6 {
            return "Collapsed entities: \(names), +\(group.members.count - 6) more · \(group.processCount) grouped processes · \(metrics)"
        }
        if group.members.count > 1 {
            return "Collapsed entities: \(names) · \(group.processCount) grouped processes · \(metrics)"
        }
        return "Entity footprint: \(group.processCount) grouped processes · \(metrics)"
    }
}

private struct EntityRow: View, Equatable {
    let row: MonitorEntityRowModel
    let isSelected: Bool
    @State private var isHovered = false

    nonisolated static func == (lhs: EntityRow, rhs: EntityRow) -> Bool {
        lhs.row == rhs.row && lhs.isSelected == rhs.isSelected
    }

    var body: some View {
        HStack(spacing: 6) {
            // Entity type icon
            Image(systemName: row.iconName)
                .font(.system(size: 11))
                .foregroundStyle(AetowerDesign.frictionColor(row.frictionScore).opacity(0.8))
                .frame(width: 16)

            // Entity name (compact)
            Text(row.displayName)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)

            ProcessOriginChip(summary: row.origin)

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

            // Metrics — centered, fixed width columns
            Text(row.cpuText)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 64, alignment: .center)

            Text(row.memoryText)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 84, alignment: .center)

            Text(row.wakeupsText)
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 72, alignment: .center)

            Label(row.processCountText, systemImage: "number")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.tertiary)
                .labelStyle(.titleAndIcon)
                .lineLimit(1)
                .frame(width: 56, alignment: .center)

            // Friction score — bold, colored
            HStack(spacing: 2) {
                Image(systemName: row.trendDirection.symbol)
                    .font(.system(size: 8))
                    .foregroundStyle(row.trendDirection.color)
                Text(row.frictionText)
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(row.frictionScore))
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
        .frame(height: MonitorRowLayout.height)
        .background(
            AetowerDesign.frictionColor(row.frictionScore)
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
        .help(row.helpText)
        .animation(AetowerDesign.Motion.quick, value: isHovered)
    }

    private var frictionBackgroundOpacity: Double {
        let base = min(Double(row.frictionScore) / 100.0, 1.0)
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

private struct EntityGroup: Equatable {
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
    let originFilter: ProcessOriginFilter
    let sortKey: SortKey
    let filterSignature: String
}

private struct MonitorSectionCacheKey: Hashable {
    let sequence: UInt64
    let query: String
    let originFilter: ProcessOriginFilter
    let sortKey: SortKey
    let filterSignature: String
}

private struct MonitorGroupRowCacheKey: Hashable {
    let groupingKey: GroupingCacheKey
    let excludedEntityIDs: [String]
    let groupEntityIDs: [String]
}

private struct MonitorEntitySections {
    let filteredEntities: [EntitySnapshot]
    let burdenLeaderRows: [MonitorEntityRowModel]
    let burdenLeaderEntityIDs: Set<String>
    let allProcessRows: [MonitorEntityRowModel]
    let flatVisibleEntityIDs: [String]
    let burdenLeaderProcessCount: Int
    let flatVisibleProcessCount: Int
    let rowBuildDurationMillis: Double

    static let empty = MonitorEntitySections(
        filteredEntities: [],
        burdenLeaderRows: [],
        burdenLeaderEntityIDs: [],
        allProcessRows: [],
        flatVisibleEntityIDs: [],
        burdenLeaderProcessCount: 0,
        flatVisibleProcessCount: 0,
        rowBuildDurationMillis: 0
    )
}

private struct MonitorGroupRowSection {
    let rows: [MonitorGroupRowModel]
    let rowBuildDurationMillis: Double

    static let empty = MonitorGroupRowSection(rows: [], rowBuildDurationMillis: 0)
}

@MainActor
private final class MonitorEntitySectionCacheStore: ObservableObject {
    private var key: MonitorSectionCacheKey?
    private var sections = MonitorEntitySections.empty

    func sections(
        for key: MonitorSectionCacheKey,
        snapshot: SystemSnapshot,
        originCache: ProcessOriginSnapshotCache,
        advancedFilterEntityIds: Set<String>?
    ) -> MonitorEntitySections {
        if self.key == key {
            return sections
        }

        let buildStartedAt = CFAbsoluteTimeGetCurrent()
        let advancedFilteredEntities: [EntitySnapshot]
        if let advancedFilterEntityIds {
            advancedFilteredEntities = snapshot.entities.filter {
                advancedFilterEntityIds.contains($0.entityId)
            }
        } else {
            advancedFilteredEntities = snapshot.entities
        }

        let originFilteredEntities: [EntitySnapshot]
        if key.originFilter == .all {
            originFilteredEntities = advancedFilteredEntities
        } else {
            originFilteredEntities = advancedFilteredEntities.filter {
                originCache.summary(for: $0).matches(key.originFilter)
            }
        }

        let filteredEntities = filterEntities(
            originFilteredEntities,
            query: key.query,
            originCache: originCache
        ).sorted {
            compareEntities($0, $1, by: key.sortKey)
        }
        let eligibleByID = Dictionary(
            uniqueKeysWithValues: filteredEntities.map { ($0.entityId, $0) }
        )
        var seenLeaderIDs = Set<String>()
        let burdenLeaderEntities = buildBurdenLeaders(snapshot: snapshot)
            .map(\.entityId)
            .filter { seenLeaderIDs.insert($0).inserted }
            .compactMap { eligibleByID[$0] }
        let burdenLeaderEntityIDs = Set(burdenLeaderEntities.map(\.entityId))
        let allProcessEntities = filteredEntities.filter {
            !burdenLeaderEntityIDs.contains($0.entityId)
        }
        let burdenLeaderProcessCount = burdenLeaderEntities.reduce(0) { total, entity in
            total + visibleProcessComponentCount(entity)
        }
        let flatVisibleProcessCount = burdenLeaderProcessCount + allProcessEntities.reduce(0) { total, entity in
            total + visibleProcessComponentCount(entity)
        }
        let nowMillis = UInt64(Date().timeIntervalSince1970 * 1000)
        let burdenLeaderRows = burdenLeaderEntities.map {
            MonitorEntityRowModel(entity: $0, origin: originCache.summary(for: $0), nowMillis: nowMillis)
        }
        let allProcessRows = allProcessEntities.map {
            MonitorEntityRowModel(entity: $0, origin: originCache.summary(for: $0), nowMillis: nowMillis)
        }

        let refreshed = MonitorEntitySections(
            filteredEntities: filteredEntities,
            burdenLeaderRows: burdenLeaderRows,
            burdenLeaderEntityIDs: burdenLeaderEntityIDs,
            allProcessRows: allProcessRows,
            flatVisibleEntityIDs: burdenLeaderEntities.map(\.entityId)
                + allProcessEntities.map(\.entityId),
            burdenLeaderProcessCount: burdenLeaderProcessCount,
            flatVisibleProcessCount: flatVisibleProcessCount,
            rowBuildDurationMillis: (CFAbsoluteTimeGetCurrent() - buildStartedAt) * 1000.0
        )
        self.key = key
        sections = refreshed
        return refreshed
    }
}

@MainActor
private final class MonitorGroupRowCacheStore: ObservableObject {
    private var key: MonitorGroupRowCacheKey?
    private var section = MonitorGroupRowSection.empty

    func section(
        for key: MonitorGroupRowCacheKey,
        groups: [EntityGroup],
        originCache: ProcessOriginSnapshotCache,
        excludedEntityIDs: Set<String>
    ) -> MonitorGroupRowSection {
        if self.key == key {
            return section
        }

        let buildStartedAt = CFAbsoluteTimeGetCurrent()
        let refreshed = groups
            .filter { !excludedEntityIDs.contains($0.root.entityId) }
            .map {
                MonitorGroupRowModel(group: $0, origin: originCache.summary(for: $0.members))
            }
        self.key = key
        section = MonitorGroupRowSection(
            rows: refreshed,
            rowBuildDurationMillis: (CFAbsoluteTimeGetCurrent() - buildStartedAt) * 1000.0
        )
        return section
    }
}

private struct MonitorMetricCardDescriptor: Identifiable {
    let id: MonitorMetricCardFocus
    let title: String
    let value: String
    let fixedScaleValue: String?
    let subtitle: String
    let samples: [Double]
    let style: TrendMetricStyle
    let valueAppearance: TrendMetricValueAppearance
    let sampleValueFormatter: (Double) -> String
    let fixedScaleSampleValueFormatter: ((Double) -> String)?
    /// Absolute top of the y-axis when the user enables fixed ring scaling.
    /// 0–100-native metrics use 100; unbounded ones reuse the danger
    /// thresholds in `MonitorRingCeiling`. A value of 0 means "no fixed
    /// ceiling available" (e.g. GPU memory) and falls back to relative.
    let fixedCeiling: Double
}

/// Fixed-mode ceilings for the unbounded Monitor rings, kept in lockstep with
/// the danger thresholds used for value coloring so the axis and the warning
/// bands can't drift apart.
private enum MonitorRingCeiling {
    /// Disk read+write danger threshold (also `monitorThroughputAppearance` danger).
    static let diskBps: Double = 250 * 1_024 * 1_024
    /// Network send+receive danger threshold.
    static let networkBps: Double = 100 * 1_024 * 1_024
    /// Wakeup "storm" band (`hostWakeupBand` severe, MonitorFormatters).
    static let wakeupsPerSecond: Double = 3_000
    /// 0–100-native metrics (friction, CPU %, memory pressure, GPU %).
    static let percent: Double = 100
}

private struct GroupedEntityRow: View, Equatable {
    let row: MonitorGroupRowModel
    let isSelected: Bool
    @State private var isHovered = false

    nonisolated static func == (lhs: GroupedEntityRow, rhs: GroupedEntityRow) -> Bool {
        lhs.row == rhs.row && lhs.isSelected == rhs.isSelected
    }

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: row.iconName)
                .font(.system(size: 11))
                .foregroundStyle(AetowerDesign.frictionColor(row.frictionScore).opacity(0.8))
                .frame(width: 16)

            Text(row.displayName)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)

            ProcessOriginChip(summary: row.origin)

            if let memberOverflowText = row.memberOverflowText {
                Text(memberOverflowText)
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(Color.secondary.opacity(0.12), in: Capsule())
            }

            Text(row.cpuText)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 64, alignment: .center)

            Text(row.memoryText)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 84, alignment: .center)

            Text(row.wakeupsText)
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 72, alignment: .center)

            Label(row.processCountText, systemImage: "number")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.tertiary)
                .labelStyle(.titleAndIcon)
                .lineLimit(1)
                .frame(width: 56, alignment: .center)

            HStack(spacing: 2) {
                Image(systemName: "arrow.triangle.branch")
                    .font(.system(size: 8))
                    .foregroundStyle(.secondary)
                Text(row.frictionText)
                    .font(.system(size: 12, weight: .bold, design: .rounded))
                    .foregroundStyle(AetowerDesign.frictionColor(row.frictionScore))
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
        .frame(height: MonitorRowLayout.height)
        .background(
            AetowerDesign.frictionColor(row.frictionScore)
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
        .help(row.helpText)
        .animation(AetowerDesign.Motion.quick, value: isHovered)
    }

    private var frictionBackgroundOpacity: Double {
        let base = min(Double(row.frictionScore) / 100.0, 1.0)
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
    query: String,
    originCache: ProcessOriginSnapshotCache
) -> [EntitySnapshot] {
    let predicates = parseSearchQuery(query, originCache: originCache)
    guard !predicates.isEmpty else {
        return entities
    }
    // Space-separated tokens are ANDed: an entity must satisfy every predicate.
    return entities.filter { entity in predicates.allSatisfy { $0(entity) } }
}

/// All free-text haystacks for an entity (lowercased), used by bare-term and
/// `field`-less matching.
private func entityTextHaystacks(
    _ entity: EntitySnapshot,
    originCache: ProcessOriginSnapshotCache
) -> [String] {
    var haystacks = [
        entity.displayName,
        entity.badges.joined(separator: " "),
        entity.friction.reasons.joined(separator: " "),
    ]
    haystacks.append(contentsOf: originCache.summary(for: entity).searchTokens)
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

private func entityMatchesSubstring(
    _ entity: EntitySnapshot,
    _ needle: String,
    originCache: ProcessOriginSnapshotCache
) -> Bool {
    entityTextHaystacks(entity, originCache: originCache).contains { $0.contains(needle) }
}

/// Parse a search query into a list of AND-ed predicates. Supports bare
/// substrings (default), `/regex/flags`, `field:value` text filters, and
/// `field>n` / `field<n` / `field=n` numeric comparisons.
private func parseSearchQuery(
    _ query: String,
    originCache: ProcessOriginSnapshotCache
) -> [(EntitySnapshot) -> Bool] {
    tokenizeSearchQuery(query).compactMap { token in
        searchPredicate(for: token, originCache: originCache)
    }
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

private func searchPredicate(
    for token: String,
    originCache: ProcessOriginSnapshotCache
) -> ((EntitySnapshot) -> Bool)? {
    guard !token.isEmpty else { return nil }

    // /regex/flags
    if token.hasPrefix("/"), let regex = compileRegexToken(token) {
        return { entity in
            entityTextHaystacks(entity, originCache: originCache).contains { haystack in
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
        if !value.isEmpty, let predicate = textFieldPredicate(field: field, value: value, originCache: originCache) {
            return predicate
        }
    }

    // bare substring
    let needle = token.localizedLowercase
    return { entityMatchesSubstring($0, needle, originCache: originCache) }
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

private func textFieldPredicate(
    field: String,
    value: String,
    originCache: ProcessOriginSnapshotCache
) -> ((EntitySnapshot) -> Bool)? {
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
    case "origin", "source", "kind", "host":
        return { entity in processOriginMatchesSearch(originCache.summary(for: entity), value: value) }
    default:
        return nil
    }
}

private func processOriginMatchesSearch(_ origin: ProcessOriginSummary, value: String) -> Bool {
    let normalizedValue = normalizedProcessOriginSearchValue(value)
    if origin.kind.rawValue == normalizedValue {
        return true
    }
    return origin.searchTokens.contains { token in
        token.localizedLowercase.contains(normalizedValue)
    }
}

private func normalizedProcessOriginSearchValue(_ value: String) -> String {
    switch value {
    case "apps", "applications", "application": return "app"
    case "helpers": return "helper"
    case "systems": return "system"
    case "services", "daemons", "daemon": return "service"
    case "term", "terminal", "shell": return "cli"
    default: return value
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
    sortKey: SortKey,
    originCache: ProcessOriginSnapshotCache
) -> [EntityGroup] {
    sortGroups(buildEntityGroups(from: filterEntities(entities, query: query, originCache: originCache)), by: sortKey)
}

private func extractParentPIDForGrouping(from parentSummary: String) -> UInt32? {
    guard let pidRange = parentSummary.range(of: "pid ") else { return nil }
    let digits = parentSummary[pidRange.upperBound...].prefix(while: \.isNumber)
    return UInt32(digits)
}

public struct MainListView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var selectedEntityID: String?
    @State private var searchText = ""
    @State private var originFilter: ProcessOriginFilter = .all
    @State private var sortKey: SortKey = .friction
    @State private var focusedIndex: Int = 0
    @State private var listMode: ListMode = .grouped
    @State private var groupedEntitiesCache: [GroupingCacheKey: [EntityGroup]] = [:]
    @State private var displayedGroupedEntities: [EntityGroup] = []
    @State private var groupingTask: Task<[EntityGroup], Never>?
    @State private var isGrouping = false
    @State private var processOperatorRequest: ProcessOperatorRequest?
    @State private var quickStopSubmission: SidePanelQuickStopSubmission?
    @State private var advancedFilterText = ""
    @State private var showAdvancedFilter = false
    @StateObject private var processOriginCacheStore = ProcessOriginSnapshotCacheStore()
    @StateObject private var monitorSectionCacheStore = MonitorEntitySectionCacheStore()
    @StateObject private var monitorGroupRowCacheStore = MonitorGroupRowCacheStore()
    @FocusState private var searchFieldFocused: Bool

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    private var processOriginCache: ProcessOriginSnapshotCache {
        processOriginCacheStore.cache(for: state.snapshot)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.none) {
            monitorTabToolBand
            Divider()
            thermalForecastBanner
            monitorContentLayout
                .padding(.horizontal, AetowerDesign.Spacing.sm)
                .padding(.vertical, AetowerDesign.Spacing.sm)
        }
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
        .task(id: monitorUiPerformanceBudgetToken) {
            state.recordMonitorRowBuildDiagnostics(
                rowBuildMillis: monitorVisibleRowBuildMillis,
                visibleRowCount: monitorVisibleRowCount
            )
        }
        .onDisappear {
            groupingTask?.cancel()
            groupingTask = nil
            searchFieldFocused = false
        }
    }

    @ViewBuilder
    private var monitorContentLayout: some View {
        switch settings.monitorMetricCardPlacement {
        case .top:
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                monitorOverviewSummary
                monitorSplitView
            }
        case .bottom:
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                monitorSplitView
                monitorOverviewSummary
            }
        case .left:
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.lg) {
                monitorMetricRail
                monitorSplitView
            }
        case .right:
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.lg) {
                monitorSplitView
                monitorMetricRail
            }
        }
    }

    private var monitorMetricRail: some View {
        ScrollView {
            monitorOverviewSummary
        }
        .frame(minWidth: 190, idealWidth: 220, maxWidth: 250, maxHeight: .infinity, alignment: .top)
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

    private var monitorTabToolBand: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search processes, origin:cli, cpu>50",
            searchWidth: 320
        ) {
            Picker(selection: $listMode) {
                ForEach(ListMode.allCases) { mode in
                    Image(systemName: mode.icon)
                        .tag(mode)
                }
            } label: {
                EmptyView()
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityLabel("Layout")
            .frame(width: 86)
            .onChange(of: listMode) { _, _ in
                focusedIndex = 0
                if let selectedEntityID, !visibleEntityIDs.contains(selectedEntityID) {
                    self.selectedEntityID = nil
                }
            }
        } filterTools: {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                sortMenu
                originMenu
                advancedFilterButton
            }
        } badges: {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerToolBadge(
                    isGroupedMode ? "Groups" : "Entities",
                    value: "\(visibleEntityIDs.count)",
                    systemImage: isGroupedMode ? "square.grid.2x2" : "list.bullet",
                    tone: AetowerDesign.Tone.friction
                )
                AetowerToolBadge(
                    "PIDs",
                    value: "\(visibleProcessCount)",
                    systemImage: "number",
                    tone: AetowerDesign.Tone.cpu
                )
                if isGroupedMode && isGrouping {
                    AetowerToolBadge(
                        "Grouping",
                        value: "Running",
                        systemImage: "arrow.triangle.2.circlepath",
                        tone: AetowerDesign.Status.warning
                    )
                }
            }
        }
    }

    private var sortMenu: some View {
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
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Text(sortKey.title)
                    .font(AetowerDesign.Typography.caption.weight(.medium))
                Image(systemName: "chevron.down")
                    .font(AetowerDesign.Typography.compactData(size: 8, weight: .semibold))
            }
            .foregroundStyle(AetowerDesign.Ink.secondary)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .aetowerControlChrome()
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    private var originMenu: some View {
        Menu {
            ForEach(ProcessOriginFilter.allCases) { filter in
                Button {
                    originFilter = filter
                    focusedIndex = 0
                    if let selectedEntityID, !visibleEntityIDs.contains(selectedEntityID) {
                        self.selectedEntityID = nil
                    }
                } label: {
                    HStack {
                        Text(filter.label)
                        if filter == originFilter {
                            Spacer()
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: "point.3.connected.trianglepath.dotted")
                    .font(AetowerDesign.Typography.compactData(size: 9, weight: .semibold))
                Text(originFilter.menuLabel)
                    .font(AetowerDesign.Typography.caption.weight(.medium))
                Image(systemName: "chevron.down")
                    .font(AetowerDesign.Typography.compactData(size: 8, weight: .semibold))
            }
            .foregroundStyle(originFilter == .all ? AnyShapeStyle(AetowerDesign.Ink.secondary) : AnyShapeStyle(Color.accentColor))
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .aetowerControlChrome()
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    private func detailPanel(for entity: EntitySnapshot) -> some View {
        let processTreeEntities = selectedProcessTreeEntities(for: entity)
        let quickStopPID = primaryProcessID(in: processTreeEntities)
        let quickStopDisplayPID = quickStopPID ?? retainedQuickStopPID(for: entity.entityId)

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

                sidePanelOriginSummary(processOriginCache.summary(for: processTreeEntities))

                if let quickStopDisplayPID {
                    sidePanelQuickStop(
                        entity: entity,
                        pid: quickStopDisplayPID,
                        targetVisible: quickStopPID != nil
                    )
                }
            }
            .padding(.horizontal, AetowerDesign.Spacing.lg)
            .padding(.vertical, AetowerDesign.Spacing.md)

            Divider()

            EntityDetailView(
                entity: entity,
                state: state,
                settings: settings,
                processTreeSeedEntities: processTreeEntities,
                processOperatorRequest: processOperatorRequest
            )
        }
    }

    private func sidePanelOriginSummary(_ origin: ProcessOriginSummary) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 8) {
                ProcessOriginChip(summary: origin)
                Text(origin.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer()
            }
            ForEach(Array(origin.detailLines.dropFirst().prefix(3)), id: \.self) { line in
                Text(line)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(Color.secondary.opacity(0.05), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
        .help(origin.detailLines.joined(separator: "\n"))
    }

    private func sidePanelQuickStop(
        entity: EntitySnapshot,
        pid: UInt32,
        targetVisible: Bool
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Label("Quick stop", systemImage: "hand.raised.fill")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)

                Text("PID \(pid)")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.tertiary)

                Spacer()

                sidePanelStopButton(.terminate, entity: entity, pid: pid, targetVisible: targetVisible)
                sidePanelStopButton(.forceKill, entity: entity, pid: pid, targetVisible: targetVisible)
            }

            sidePanelQuickStopStatus(entity: entity, pid: pid, targetVisible: targetVisible)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
    }

    private func sidePanelStopButton(
        _ action: ProcessActionKind,
        entity: EntitySnapshot,
        pid: UInt32,
        targetVisible: Bool
    ) -> some View {
        Button(role: action.isDestructive ? .destructive : nil) {
            runSidePanelQuickStop(action, entity: entity, pid: pid)
        } label: {
            Label(action.label, systemImage: action.systemImage)
                .font(.caption.weight(.semibold))
                .foregroundStyle(action == .forceKill ? AetowerDesign.Status.error : AetowerDesign.Status.warning)
        }
        .buttonStyle(.borderless)
        .disabled(!targetVisible || state.entityAnalysisIsLoading(processAnalysisKey(pid), kind: .processAction))
        .help(targetVisible ? "Validate the target, send \(action.label.lowercased()), and verify the result." : "Target PID is no longer visible.")
    }

    @ViewBuilder
    private func sidePanelQuickStopStatus(
        entity: EntitySnapshot,
        pid: UInt32,
        targetVisible: Bool
    ) -> some View {
        let submission = quickStopSubmissionMatching(entityID: entity.entityId, pid: pid)

        if !targetVisible, submission != nil {
            SidePanelQuickStatusLine(
                icon: "checkmark.seal.fill",
                color: AetowerDesign.Status.success,
                text: "PID \(pid) is no longer visible in the current snapshot."
            )
        }

        if let submission {
            sidePanelQuickStopSubmissionStatus(submission)
        }

        if submission != nil,
           let error = state.entityAnalysisError(processAnalysisKey(pid), kind: .processAction) {
            SidePanelQuickStatusLine(
                icon: "exclamationmark.triangle.fill",
                color: AetowerDesign.Status.error,
                text: error
            )
        }
    }

    @ViewBuilder
    private func sidePanelQuickStopSubmissionStatus(_ submission: SidePanelQuickStopSubmission) -> some View {
        if state.entityAnalysisIsLoading(processAnalysisKey(submission.pid), kind: .processAction) {
            SidePanelQuickStatusLine(
                icon: "paperplane.fill",
                color: AetowerDesign.Status.ready,
                text: "\(submission.action.label) \(shortActionID(submission.actionID)) started; Aetower is validating the target, sending the signal, and waiting for verification."
            )
        }

        if let report = state.processActionReports[submission.pid],
           report.actionId == submission.actionID {
            VStack(alignment: .leading, spacing: 6) {
                SidePanelQuickStatusLine(
                    icon: sidePanelQuickStopResultIcon(report),
                    color: sidePanelQuickStopResultColor(report),
                    text: "\(report.action) \(shortActionID(submission.actionID)): \(report.message)"
                )
                if let verification = sidePanelQuickStopEffectiveVerification(report) {
                    Text("Verification: \(sidePanelQuickStopVerificationLabel(verification))")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                sidePanelQuickStopFollowUpStatus(report)
                if let stderr = report.commandResult?.stderr, !stderr.isEmpty {
                    Text("macOS stderr: \(stderr)")
                        .font(.caption2)
                        .foregroundStyle(AetowerDesign.Status.warning)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Button("Clear quick-stop status") {
                    quickStopSubmission = nil
                }
                .buttonStyle(.borderless)
                .controlSize(.small)
            }
            .padding(.top, 2)
        }
    }

    @ViewBuilder
    private func sidePanelQuickStopFollowUpStatus(_ report: ProcessActionReportModel) -> some View {
        if let checks = report.followUpChecks, !checks.isEmpty {
            ForEach(checks) { check in
                Text("\(check.delayMillis / 1_000)s follow-up: \(sidePanelQuickStopVerificationLabel(check.verification))")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func runSidePanelQuickStop(
        _ action: ProcessActionKind,
        entity: EntitySnapshot,
        pid: UInt32
    ) {
        let actionID = UUID().uuidString
        quickStopSubmission = SidePanelQuickStopSubmission(
            entityID: entity.entityId,
            pid: pid,
            action: action,
            actionID: actionID,
            submittedAt: Date()
        )
        state.runVerifiedProcessAction(
            pid: pid,
            action: action,
            reason: "Quick stop from Monitor side panel.",
            actionID: actionID
        )
    }

    private func quickStopSubmissionMatching(entityID: String, pid: UInt32) -> SidePanelQuickStopSubmission? {
        guard let quickStopSubmission,
              quickStopSubmission.entityID == entityID,
              quickStopSubmission.pid == pid
        else {
            return nil
        }
        return quickStopSubmission
    }

    private func retainedQuickStopPID(for entityID: String) -> UInt32? {
        if let quickStopSubmission, quickStopSubmission.entityID == entityID {
            return quickStopSubmission.pid
        }
        return nil
    }

    private func shortActionID(_ actionID: String) -> String {
        "#\(actionID.suffix(8))"
    }

    private func processAnalysisKey(_ pid: UInt32) -> String {
        "pid:\(pid)"
    }

    private func sidePanelQuickStopResultIcon(_ report: ProcessActionReportModel) -> String {
        guard report.success else { return "xmark.octagon.fill" }
        return sidePanelQuickStopVerificationIsConfirmed(sidePanelQuickStopEffectiveVerification(report))
            ? "checkmark.circle.fill"
            : "exclamationmark.triangle.fill"
    }

    private func sidePanelQuickStopResultColor(_ report: ProcessActionReportModel) -> Color {
        guard report.success else { return AetowerDesign.Status.error }
        return sidePanelQuickStopVerificationIsConfirmed(sidePanelQuickStopEffectiveVerification(report))
            ? AetowerDesign.Status.success
            : AetowerDesign.Status.warning
    }

    private func sidePanelQuickStopEffectiveVerification(_ report: ProcessActionReportModel) -> String? {
        report.followUpChecks?.last?.verification ?? report.verification
    }

    private func sidePanelQuickStopVerificationIsConfirmed(_ verification: String?) -> Bool {
        switch verification {
        case "verified-exited", "verified-suspended", "verified-running",
            "verified-priority", "command-accepted", "preview":
            return true
        default:
            return false
        }
    }

    private func sidePanelQuickStopVerificationLabel(_ verification: String) -> String {
        switch verification {
        case "verified-exited", "exited": return "Exited / no longer running"
        case "targets-still-visible", "still-visible": return "Still visible"
        case "command-failed": return "Command failed"
        case "preview": return "Preview only"
        case "target-visible": return "Target visible"
        case "target-not-visible": return "Target missing"
        default: return verification.replacingOccurrences(of: "-", with: " ")
        }
    }

    private var rankedEntitiesSection: some View {
        VStack(alignment: .leading, spacing: 6) {
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
                    if !burdenLeaderRows.isEmpty {
                        listSectionHeader("Burden leaders")
                        ForEach(burdenLeaderRows) { row in
                            Button {
                                searchFieldFocused = false
                                withAnimation(AetowerDesign.Motion.standard) {
                                    selectedEntityID = row.id
                                }
                            } label: {
                                EntityRow(
                                    row: row,
                                    isSelected: selectedEntityID == row.id
                                )
                                .equatable()
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                monitorContextMenu(for: row.entity, members: [row.entity])
                            }
                        }
                    }

                    listSectionHeader("All processes")
                    if isGroupedMode {
                        ForEach(allProcessGroupRows) { row in
                            Button {
                                searchFieldFocused = false
                                withAnimation(AetowerDesign.Motion.standard) {
                                    selectedEntityID = row.id
                                }
                            } label: {
                                GroupedEntityRow(
                                    row: row,
                                    isSelected: selectedEntityID == row.id
                                )
                                .equatable()
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                monitorContextMenu(for: row.group.root, members: row.group.members)
                            }
                        }
                    } else {
                        ForEach(allProcessRows) { row in
                            Button {
                                searchFieldFocused = false
                                withAnimation(AetowerDesign.Motion.standard) {
                                    selectedEntityID = row.id
                                }
                            } label: {
                                EntityRow(
                                    row: row,
                                    isSelected: selectedEntityID == row.id
                                )
                                .equatable()
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                monitorContextMenu(for: row.entity, members: [row.entity])
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
        processOperatorRequest = ProcessOperatorRequest(pid: pid, operation: operation)
        selectEntity(entityID)
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

    @ViewBuilder
    private var thermalForecastBanner: some View {
        if let forecast = state.snapshot.thermalForecast {
            let tone =
                forecast.state == .nominal || forecast.state == .fair
                ? AetowerDesign.Status.warning : AetowerDesign.Status.error
            HStack(spacing: 8) {
                Image(systemName: "thermometer.sun.fill")
                    .foregroundStyle(tone)
                Text(thermalBannerText(forecast))
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(tone)
                Spacer()
                Text("Sensors tab for the temperature trend")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .background(tone.opacity(0.12), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm))
            .padding(.horizontal, AetowerDesign.Spacing.sm)
        }
    }

    private func thermalBannerText(_ forecast: ThermalForecast) -> String {
        let lead =
            forecast.minutesToThrottle.map { String(format: "Throttle likely in ~%.0f min", $0) }
            ?? "Throttling now"
        if let label = forecast.topContributorLabel {
            return "\(lead) · top contributor: \(label)"
        }
        return lead
    }

    private var monitorOverviewSummary: some View {
        let compactCards = monitorMetricCardDescriptors(from: state.monitorViewModel)
        if !compactCards.isEmpty {
            return monitorMetricCardsLayout(compactCards)
        }

        let snapshot = state.snapshot
        let host = snapshot.host
        let frictionScore = machineFrictionScore(for: host)
        let frictionSamples = monitorFallbackTrendSamples(frictionScore)
        let cpuSamples = monitorFallbackTrendSamples(Double(host.cpuPercent))
        let memorySamples = monitorFallbackTrendSamples(hostMemoryPressureScore(host))
        let diskSamples = monitorFallbackTrendSamples(Double(host.diskReadBps + host.diskWriteBps))
        let networkSamples = monitorFallbackTrendSamples(Double(host.networkReceiveBps + host.networkSendBps))
        let wakeupSamples = monitorFallbackTrendSamples(Double(host.wakeupsPerSecond))
        let gpuPercentSamples = monitorFallbackTrendSamples(Double(host.gpuPercent))
        let gpuMemorySamples = monitorFallbackTrendSamples(Double(host.gpuMemoryBytes))

        let usingGpuPercent = host.gpuPercent > 0 || host.gpuMemoryBytes == 0
        let gpuSamples = usingGpuPercent ? gpuPercentSamples : gpuMemorySamples
        let gpuValue = usingGpuPercent
            ? String(format: "%.0f%%", host.gpuPercent)
            : formatBytes(host.gpuMemoryBytes)
        let memoryPressure = hostMemoryPressureScore(host)

        let cards = [
            MonitorMetricCardDescriptor(
                id: .friction,
                title: "Friction",
                value: String(format: "%.0f", frictionScore),
                fixedScaleValue: String(format: "%.0f%%", frictionScore),
                subtitle: "overall machine score · \(trendWindowLabel(sampleCount: frictionSamples.count))",
                samples: frictionSamples,
                style: .friction,
                valueAppearance: monitorFrictionAppearance(frictionScore),
                sampleValueFormatter: { String(format: "%.0f", $0) },
                fixedScaleSampleValueFormatter: { String(format: "%.0f%%", $0) },
                fixedCeiling: MonitorRingCeiling.percent
            ),
            MonitorMetricCardDescriptor(
                id: .cpu,
                title: "CPU",
                value: String(format: "%.0f%%", host.cpuPercent),
                fixedScaleValue: nil,
                subtitle: "thermal \(thermalStateLabel(host.thermalState)) · \(trendWindowLabel(sampleCount: cpuSamples.count))",
                samples: cpuSamples,
                style: .cpu,
                valueAppearance: monitorCPUAppearance(host),
                sampleValueFormatter: { String(format: "%.0f%%", $0) },
                fixedScaleSampleValueFormatter: nil,
                fixedCeiling: MonitorRingCeiling.percent
            ),
            MonitorMetricCardDescriptor(
                id: .memory,
                title: "Memory",
                value: formatBytes(host.memoryUsedBytes),
                fixedScaleValue: String(format: "%.0f%%", memoryPressure),
                subtitle: "\(formatBytes(host.memoryUsedBytes)) used · \(formatBytes(host.compressedMemoryBytes)) compressed · \(formatBytes(host.swapUsedBytes)) swap · pressure trend",
                samples: memorySamples,
                style: .memory,
                valueAppearance: monitorMemoryAppearance(host),
                // Samples are the 0–100 memory pressure score, not bytes — label
                // the hover stats with /100 so they don't read as bytes/percent.
                sampleValueFormatter: { String(format: "%.0f/100", $0) },
                fixedScaleSampleValueFormatter: { String(format: "%.0f%%", $0) },
                fixedCeiling: MonitorRingCeiling.percent
            ),
            MonitorMetricCardDescriptor(
                id: .disk,
                title: "Disk",
                value: formatRate(host.diskReadBps + host.diskWriteBps),
                fixedScaleValue: nil,
                subtitle: "read + write · \(trendWindowLabel(sampleCount: diskSamples.count))",
                samples: diskSamples,
                style: .disk,
                valueAppearance: monitorThroughputAppearance(host.diskReadBps + host.diskWriteBps, warning: 50 * 1_024 * 1_024, danger: 250 * 1_024 * 1_024),
                sampleValueFormatter: { formatRate(UInt64(max($0, 0))) },
                fixedScaleSampleValueFormatter: nil,
                fixedCeiling: MonitorRingCeiling.diskBps
            ),
            MonitorMetricCardDescriptor(
                id: .network,
                title: "Network",
                value: formatRate(host.networkReceiveBps + host.networkSendBps),
                fixedScaleValue: nil,
                subtitle: "send + receive · \(trendWindowLabel(sampleCount: networkSamples.count))",
                samples: networkSamples,
                style: .network,
                valueAppearance: monitorThroughputAppearance(host.networkReceiveBps + host.networkSendBps, warning: 10 * 1_024 * 1_024, danger: 100 * 1_024 * 1_024),
                sampleValueFormatter: { formatRate(UInt64(max($0, 0))) },
                fixedScaleSampleValueFormatter: nil,
                fixedCeiling: MonitorRingCeiling.networkBps
            ),
            MonitorMetricCardDescriptor(
                id: .wakeups,
                title: "Wakeups",
                value: formatWakeups(host.wakeupsPerSecond),
                fixedScaleValue: nil,
                subtitle: hostWakeupBand(host.wakeupsPerSecond) == .severe
                    ? "wakeup storm band · \(trendWindowLabel(sampleCount: wakeupSamples.count))"
                    : "current host wakeup rate · \(trendWindowLabel(sampleCount: wakeupSamples.count))",
                samples: wakeupSamples,
                style: .friction,
                valueAppearance: monitorWakeupAppearance(host),
                sampleValueFormatter: { formatWakeups(Float($0)) },
                fixedScaleSampleValueFormatter: nil,
                fixedCeiling: MonitorRingCeiling.wakeupsPerSecond
            ),
            MonitorMetricCardDescriptor(
                id: .gpu,
                title: "GPU",
                value: gpuValue,
                fixedScaleValue: usingGpuPercent ? String(format: "%.0f%%", host.gpuPercent) : nil,
                subtitle: "\(hostGPUSummary(host)) · \(trendWindowLabel(sampleCount: gpuSamples.count))",
                samples: gpuSamples,
                style: .energy,
                valueAppearance: monitorGPUAppearance(host),
                sampleValueFormatter: usingGpuPercent
                    ? { String(format: "%.0f%%", $0) }
                    : { formatBytes(UInt64(max($0, 0))) },
                fixedScaleSampleValueFormatter: usingGpuPercent
                    ? { String(format: "%.0f%%", $0) }
                    : nil,
                // GPU % has a true 0–100 axis; the GPU-memory fallback has no
                // fixed ceiling, so 0 keeps that case on relative scaling.
                fixedCeiling: usingGpuPercent ? MonitorRingCeiling.percent : 0
            ),
        ]

        return monitorMetricCardsLayout(cards)
    }

    private func monitorMetricCardDescriptors(
        from viewModel: MonitorViewModel
    ) -> [MonitorMetricCardDescriptor] {
        let order: [MonitorMetricCardFocus] = [.friction, .cpu, .memory, .disk, .network, .wakeups, .gpu]
        let orderIndex = Dictionary(uniqueKeysWithValues: order.enumerated().map { ($0.element, $0.offset) })
        return viewModel.metricCards
            .compactMap(monitorMetricCardDescriptor(from:))
            .sorted {
                (orderIndex[$0.id] ?? Int.max) < (orderIndex[$1.id] ?? Int.max)
            }
    }

    private func monitorMetricCardDescriptor(
        from card: UiMetricCard
    ) -> MonitorMetricCardDescriptor? {
        guard let focus = MonitorMetricCardFocus(rawValue: card.id), focus != .all else {
            return nil
        }

        let samples = card.samples.isEmpty ? [card.value] : card.samples
        return MonitorMetricCardDescriptor(
            id: focus,
            title: card.title,
            value: card.displayValue,
            fixedScaleValue: monitorMetricFixedScaleValue(for: card, focus: focus),
            subtitle: "\(card.detail) · \(trendWindowLabel(sampleCount: samples.count))",
            samples: samples,
            style: monitorMetricStyle(for: focus),
            valueAppearance: monitorMetricValueAppearance(for: card.severity),
            sampleValueFormatter: monitorMetricSampleFormatter(for: card, focus: focus),
            fixedScaleSampleValueFormatter: monitorMetricFixedScaleSampleFormatter(for: card, focus: focus),
            fixedCeiling: monitorMetricFixedCeiling(for: card, focus: focus)
        )
    }

    private func monitorMetricStyle(for focus: MonitorMetricCardFocus) -> TrendMetricStyle {
        switch focus {
        case .friction:
            return .friction
        case .cpu:
            return .cpu
        case .memory:
            return .memory
        case .disk:
            return .disk
        case .network:
            return .network
        case .wakeups:
            return .friction
        case .gpu:
            return .energy
        case .all:
            return .friction
        }
    }

    private func monitorMetricValueAppearance(for severity: UiMetricSeverity) -> TrendMetricValueAppearance {
        switch severity {
        case .normal:
            return .ok
        case .warning:
            return .warning
        case .critical:
            return .danger
        }
    }

    private func monitorMetricFixedScaleValue(
        for card: UiMetricCard,
        focus: MonitorMetricCardFocus
    ) -> String? {
        switch card.unit {
        case "percent":
            return String(format: "%.0f%%", card.value)
        case "score":
            return String(format: "%.0f%%", card.value)
        case "bytes":
            guard focus == .memory, let ceiling = card.fixedCeiling, ceiling > 0 else { return nil }
            return String(format: "%.0f%%", monitorMetricPercent(card.value, ceiling: ceiling))
        default:
            return nil
        }
    }

    private func monitorMetricSampleFormatter(
        for card: UiMetricCard,
        focus: MonitorMetricCardFocus
    ) -> (Double) -> String {
        switch card.unit {
        case "percent":
            return { String(format: "%.0f%%", $0) }
        case "score":
            return { String(format: "%.0f", $0) }
        case "bytes":
            return { formatBytes(UInt64(max($0, 0))) }
        case "bytes_per_second":
            return { formatRate(UInt64(max($0, 0))) }
        case "wakeups_per_second":
            return { formatWakeups(Float($0)) }
        default:
            switch focus {
            case .disk, .network:
                return { formatRate(UInt64(max($0, 0))) }
            case .wakeups:
                return { formatWakeups(Float($0)) }
            default:
                return { String(format: "%.0f", $0) }
            }
        }
    }

    private func monitorMetricFixedScaleSampleFormatter(
        for card: UiMetricCard,
        focus: MonitorMetricCardFocus
    ) -> ((Double) -> String)? {
        switch card.unit {
        case "percent":
            return { String(format: "%.0f%%", $0) }
        case "score":
            return { String(format: "%.0f%%", $0) }
        case "bytes":
            guard focus == .memory, let ceiling = card.fixedCeiling, ceiling > 0 else { return nil }
            return { String(format: "%.0f%%", monitorMetricPercent($0, ceiling: ceiling)) }
        default:
            return nil
        }
    }

    private func monitorMetricFixedCeiling(
        for card: UiMetricCard,
        focus: MonitorMetricCardFocus
    ) -> Double {
        if let fixedCeiling = card.fixedCeiling, fixedCeiling > 0 {
            return fixedCeiling
        }

        switch focus {
        case .friction, .cpu, .memory, .gpu:
            return MonitorRingCeiling.percent
        case .disk:
            return MonitorRingCeiling.diskBps
        case .network:
            return MonitorRingCeiling.networkBps
        case .wakeups:
            return MonitorRingCeiling.wakeupsPerSecond
        case .all:
            return 0
        }
    }

    private func monitorMetricPercent(_ value: Double, ceiling: Double) -> Double {
        guard ceiling > 0 else { return 0 }
        return min(100, max(0, (value / ceiling) * 100))
    }

    @ViewBuilder
    private func monitorMetricCardsLayout(_ cards: [MonitorMetricCardDescriptor]) -> some View {
        let visibleCards = visibleMonitorMetricCards(cards)
        switch settings.monitorMetricCardPlacement {
        case .left, .right:
            LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(visibleCards) { card in
                    monitorMetricCard(card, minHeight: 106)
                }
            }
        case .top, .bottom:
            if settings.monitorMetricCardFocus != .all, visibleCards.count == 1, let card = visibleCards.first {
                monitorMetricCard(card, minHeight: 168, allowsPinning: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                LazyVGrid(columns: monitorSummaryGridColumns, alignment: .leading, spacing: 12) {
                    ForEach(visibleCards) { card in
                        monitorMetricCard(card, minHeight: 124, allowsPinning: true)
                    }
                }
            }
        }
    }

    private func monitorMetricCard(
        _ card: MonitorMetricCardDescriptor,
        minHeight: CGFloat,
        allowsPinning: Bool = false
    ) -> some View {
        let fixedScaling = settings.metricRingsFixedScaling
        let fixedCeiling = fixedScaling && card.fixedCeiling > 0 ? card.fixedCeiling : nil
        return TrendMetricCard(
            title: card.title,
            value: monitorMetricValue(card, fixedScaling: fixedScaling),
            subtitle: monitorMetricSubtitle(card, fixedScaling: fixedScaling),
            samples: card.samples,
            style: card.style,
            valueAppearance: card.valueAppearance,
            sampleValueFormatter: monitorMetricSampleFormatter(card, fixedScaling: fixedScaling),
            minHeight: minHeight,
            fixedCeiling: fixedCeiling
        )
        .id("\(card.id.rawValue)-scale-\(fixedCeiling ?? -1)")
        .overlay(alignment: .topTrailing) {
            if allowsPinning {
                monitorMetricPinButton(card)
            }
        }
    }

    private func monitorMetricValue(
        _ card: MonitorMetricCardDescriptor,
        fixedScaling: Bool
    ) -> String {
        if fixedScaling, let fixedScaleValue = card.fixedScaleValue {
            return fixedScaleValue
        }
        return card.value
    }

    private func monitorMetricSampleFormatter(
        _ card: MonitorMetricCardDescriptor,
        fixedScaling: Bool
    ) -> (Double) -> String {
        if fixedScaling, let fixedScaleSampleValueFormatter = card.fixedScaleSampleValueFormatter {
            return fixedScaleSampleValueFormatter
        }
        return card.sampleValueFormatter
    }

    private func monitorMetricSubtitle(
        _ card: MonitorMetricCardDescriptor,
        fixedScaling: Bool
    ) -> String {
        guard fixedScaling, card.fixedCeiling > 0 else { return card.subtitle }
        return "\(card.subtitle) · \(monitorMetricScaleLabel(card))"
    }

    private func monitorMetricScaleLabel(_ card: MonitorMetricCardDescriptor) -> String {
        switch card.id {
        case .friction, .cpu, .memory, .gpu:
            return "fixed 0–100"
        case .disk:
            return "fixed 0–\(formatRate(UInt64(card.fixedCeiling)))"
        case .network:
            return "fixed 0–\(formatRate(UInt64(card.fixedCeiling)))"
        case .wakeups:
            return "fixed 0–\(formatWakeups(Float(card.fixedCeiling)))"
        case .all:
            return "fixed scale"
        }
    }

    private func monitorMetricPinButton(_ card: MonitorMetricCardDescriptor) -> some View {
        let isFocused = settings.monitorMetricCardFocus == card.id
        return Button {
            withAnimation(AetowerDesign.Motion.standard) {
                settings.monitorMetricCardFocus = isFocused ? .all : card.id
            }
        } label: {
            Image(systemName: isFocused ? "pin.slash.fill" : "pin.fill")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(isFocused ? Color.accentColor : Color.secondary)
                .frame(width: 22, height: 22)
                .background(.regularMaterial, in: Circle())
                .overlay(
                    Circle()
                        .stroke(Color.secondary.opacity(0.18), lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
        .padding(7)
        .help(isFocused ? "Unpin \(card.title) and show all rings" : "Pin \(card.title) as the full-width ring")
        .accessibilityLabel(isFocused ? "Unpin \(card.title)" : "Pin \(card.title)")
    }

    private func visibleMonitorMetricCards(_ cards: [MonitorMetricCardDescriptor]) -> [MonitorMetricCardDescriptor] {
        guard settings.monitorMetricCardPlacement.supportsFocusedMetric else { return cards }
        let focus = settings.monitorMetricCardFocus
        guard focus != .all else { return cards }
        let focused = cards.filter { $0.id == focus }
        return focused.isEmpty ? cards : focused
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
        monitorSections.filteredEntities
    }

    private var burdenLeaderRows: [MonitorEntityRowModel] {
        monitorSections.burdenLeaderRows
    }

    private var burdenLeaderEntityIDs: Set<String> {
        monitorSections.burdenLeaderEntityIDs
    }

    private var allProcessRows: [MonitorEntityRowModel] {
        monitorSections.allProcessRows
    }

    private var allProcessGroupRows: [MonitorGroupRowModel] {
        allProcessGroupSection.rows
    }

    private var allProcessGroupSection: MonitorGroupRowSection {
        guard let groupingKey = currentGroupingCacheKey else { return .empty }
        let groups = groupedEntities
        let excludedIDs = burdenLeaderEntityIDs
        let key = MonitorGroupRowCacheKey(
            groupingKey: groupingKey,
            excludedEntityIDs: excludedIDs.sorted(),
            groupEntityIDs: groups.map(\.id)
        )
        return monitorGroupRowCacheStore.section(
            for: key,
            groups: groups,
            originCache: processOriginCache,
            excludedEntityIDs: excludedIDs
        )
    }

    private var monitorVisibleRowCount: Int {
        if isGroupedMode {
            return burdenLeaderRows.count + allProcessGroupRows.count
        }
        return burdenLeaderRows.count + allProcessRows.count
    }

    private var monitorVisibleRowBuildMillis: Double {
        monitorSections.rowBuildDurationMillis
            + (isGroupedMode ? allProcessGroupSection.rowBuildDurationMillis : 0)
    }

    private var monitorUiPerformanceBudgetToken: String {
        let durationBucket = Int((monitorVisibleRowBuildMillis * 10).rounded())
        return [
            "\(state.snapshot.sequence)",
            listMode.rawValue,
            "\(monitorVisibleRowCount)",
            "\(durationBucket)",
            normalizedSearchQuery,
            originFilter.rawValue,
            sortKey.rawValue,
            advancedFilterSignature,
        ].joined(separator: "|")
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
        let sections = monitorSections
        if isGroupedMode {
            return sections.burdenLeaderRows.map(\.id) + allProcessGroupRows.map(\.id)
        }
        return sections.flatVisibleEntityIDs
    }

    private var visibleProcessCount: Int {
        let sections = monitorSections
        if isGroupedMode {
            return sections.burdenLeaderProcessCount + allProcessGroupRows.reduce(0) {
                $0 + $1.group.processCount
            }
        }
        return sections.flatVisibleProcessCount
    }

    private var groupedEntities: [EntityGroup] {
        guard let key = currentGroupingCacheKey else { return [] }
        return groupedEntitiesCache[key] ?? displayedGroupedEntities
    }

    private var monitorSections: MonitorEntitySections {
        guard let key = currentMonitorSectionCacheKey else { return .empty }
        return monitorSectionCacheStore.sections(
            for: key,
            snapshot: state.snapshot,
            originCache: processOriginCache,
            advancedFilterEntityIds: state.advancedFilterEntityIds
        )
    }

    private func selectedProcessTreeEntities(for entity: EntitySnapshot) -> [EntitySnapshot] {
        if let selectedEntityGroup, selectedEntityGroup.root.entityId == entity.entityId {
            return selectedEntityGroup.members
        }
        return [entity]
    }

    private var normalizedSearchQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var currentGroupingCacheKey: GroupingCacheKey? {
        guard isGroupedMode else { return nil }
        return GroupingCacheKey(
            sequence: state.snapshot.sequence,
            query: normalizedSearchQuery,
            originFilter: originFilter,
            sortKey: sortKey,
            filterSignature: advancedFilterSignature
        )
    }

    private var currentMonitorSectionCacheKey: MonitorSectionCacheKey? {
        MonitorSectionCacheKey(
            sequence: state.snapshot.sequence,
            query: normalizedSearchQuery,
            originFilter: originFilter,
            sortKey: sortKey,
            filterSignature: advancedFilterSignature
        )
    }

    /// Stable signature of the active advanced filter so the grouping cache
    /// invalidates when the filter set changes ("" = no filter).
    private var advancedFilterSignature: String {
        guard let ids = state.advancedFilterEntityIds else { return "" }
        guard !ids.isEmpty else { return "<empty>" }
        return ids.sorted().joined(separator: ",")
    }

    private var isGroupedMode: Bool {
        listMode == .grouped
    }

    private var groupingTaskToken: String {
        guard let key = currentGroupingCacheKey else { return "flat" }
        return "\(key.sequence)|\(key.query)|\(key.originFilter.rawValue)|\(key.sortKey.rawValue)|\(key.filterSignature)"
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

    private var monitorSummaryGridColumns: [GridItem] {
        [GridItem(.adaptive(minimum: 180, maximum: 420), spacing: AetowerDesign.Spacing.md, alignment: .top)]
    }

    private func monitorFallbackTrendSamples(_ value: Double) -> [Double] {
        [value, value]
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
            originFilter = .all
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

        let originCache = processOriginCache
        let entities = monitorSections.filteredEntities
        let sortKey = key.sortKey
        isGrouping = true

        let task = Task.detached(priority: .utility) {
            buildGroupedEntities(from: entities, query: "", sortKey: sortKey, originCache: originCache)
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

private func visibleProcessComponentCount(_ entity: EntitySnapshot) -> Int {
    entity.components.reduce(0) { total, component in
        total + (component.kind != .adapterContext && component.processId != nil ? 1 : 0)
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
