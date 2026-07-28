import Foundation
import SwiftUI
import AetowerBridge

enum SortKey: String, CaseIterable, Identifiable {
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

enum ListMode: String, CaseIterable, Identifiable {
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

struct EntityGroup: Equatable {
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

struct MonitorProcessComponentRef: Identifiable, Equatable {
    let owner: EntitySnapshot
    let ownerSortIndex: Int
    let pid: UInt32
    let parentPid: UInt32?
    let title: String
    let executablePath: String?
    let commandLine: String?
    let cwd: String?
    let user: String?
    let startTimeMillis: UInt64
    let cpuPercent: Float
    let memoryBytes: UInt64
    let memoryPhysicalFootprintBytes: UInt64
    let threadCount: UInt32
    let source: String
    let confidence: Float

    var id: String {
        "\(owner.entityId):\(pid)"
    }

    init(owner: EntitySnapshot, lineage: ProcessLineageNode, ownerSortIndex: Int) {
        self.owner = owner
        self.ownerSortIndex = ownerSortIndex
        self.pid = lineage.pid
        self.parentPid = lineage.parentPid
        self.title = lineage.title
        self.executablePath = lineage.executablePath
        self.commandLine = lineage.commandLine
        self.cwd = lineage.cwd
        self.user = lineage.user
        self.startTimeMillis = lineage.startTimeMillis
        self.cpuPercent = lineage.cpuPercent
        self.memoryBytes = lineage.memoryBytes
        self.memoryPhysicalFootprintBytes = lineage.memoryPhysicalFootprintBytes
        self.threadCount = lineage.threadCount
        self.source = lineage.source
        self.confidence = lineage.confidence
    }

    init(owner: EntitySnapshot, component: ComponentSnapshot, ownerSortIndex: Int) {
        self.owner = owner
        self.ownerSortIndex = ownerSortIndex
        self.pid = component.processId ?? 0
        self.parentPid = component.parentSummary.flatMap(extractParentPIDForGrouping)
        self.title = component.title
        self.executablePath = component.executablePath
        self.commandLine = component.commandLine
        self.cwd = component.cwd
        self.user = component.user
        self.startTimeMillis = component.startTimeMillis
        self.cpuPercent = component.cpuPercent
        self.memoryBytes = component.memoryBytes
        self.memoryPhysicalFootprintBytes = component.memoryPhysicalFootprintBytes
        self.threadCount = component.threadCount
        self.source = "component-parent-summary"
        self.confidence = 0.55
    }
}

struct GroupingCacheKey: Hashable {
    let sequence: UInt64
    let query: String
    let originFilter: ProcessOriginFilter
    let sortKey: SortKey
    let filterSignature: String
}

struct MonitorSectionCacheKey: Hashable {
    let sequence: UInt64
    let query: String
    let originFilter: ProcessOriginFilter
    let sortKey: SortKey
    let filterSignature: String
}

struct MonitorGroupRowCacheKey: Hashable {
    let groupingKey: GroupingCacheKey
    let burdenLeaderSignature: [String]
    let groupEntityIDs: [String]
}

struct MonitorEntitySections {
    let filteredEntities: [EntitySnapshot]
    let groupingEntities: [EntitySnapshot]
    let burdenLeaderRows: [MonitorEntityRowModel]
    let burdenLeaderEntityIDs: Set<String>
    let burdenLeaderSummariesByEntityID: [String: [BurdenLeaderSummary]]
    let allProcessRows: [MonitorEntityRowModel]
    let flatVisibleEntityIDs: [String]
    let burdenLeaderProcessCount: Int
    let flatVisibleProcessCount: Int
    let rowBuildDurationMillis: Double

    static let empty = MonitorEntitySections(
        filteredEntities: [],
        groupingEntities: [],
        burdenLeaderRows: [],
        burdenLeaderEntityIDs: [],
        burdenLeaderSummariesByEntityID: [:],
        allProcessRows: [],
        flatVisibleEntityIDs: [],
        burdenLeaderProcessCount: 0,
        flatVisibleProcessCount: 0,
        rowBuildDurationMillis: 0
    )
}

struct MonitorGroupRowSection {
    let rows: [MonitorGroupRowModel]
    let rowBuildDurationMillis: Double

    static let empty = MonitorGroupRowSection(rows: [], rowBuildDurationMillis: 0)
}

@MainActor
final class MonitorEntitySectionCacheStore: ObservableObject {
    private var key: MonitorSectionCacheKey?
    private var sections = MonitorEntitySections.empty

    func sections(
        for key: MonitorSectionCacheKey,
        entities: [EntitySnapshot],
        host: HostSnapshot,
        originCache: ProcessOriginSnapshotCache,
        advancedFilterEntityIds: Set<String>?
    ) -> MonitorEntitySections {
        if self.key == key {
            return sections
        }

        let buildStartedAt = CFAbsoluteTimeGetCurrent()
        let advancedFilteredEntities: [EntitySnapshot]
        if let advancedFilterEntityIds {
            advancedFilteredEntities = entities.filter {
                advancedFilterEntityIds.contains($0.entityId)
            }
        } else {
            advancedFilteredEntities = entities
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
        let burdenLeaderSummaries = buildBurdenLeaders(entities: originFilteredEntities, host: host)
        let burdenLeaderEntities = burdenLeaderSummaries
            .map(\.entityId)
            .filter { seenLeaderIDs.insert($0).inserted }
            .compactMap { eligibleByID[$0] }
        let burdenLeaderEntityIDs = Set(burdenLeaderEntities.map(\.entityId))
        let burdenLeaderSummariesByEntityID = Dictionary(grouping: burdenLeaderSummaries, by: \.entityId)
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
            groupingEntities: originFilteredEntities,
            burdenLeaderRows: burdenLeaderRows,
            burdenLeaderEntityIDs: burdenLeaderEntityIDs,
            burdenLeaderSummariesByEntityID: burdenLeaderSummariesByEntityID,
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
final class MonitorGroupRowCacheStore: ObservableObject {
    private var key: MonitorGroupRowCacheKey?
    private var section = MonitorGroupRowSection.empty

    func section(
        for key: MonitorGroupRowCacheKey,
        groups: [EntityGroup],
        originCache: ProcessOriginSnapshotCache,
        burdenLeaderSummariesByEntityID: [String: [BurdenLeaderSummary]]
    ) -> MonitorGroupRowSection {
        if self.key == key {
            return section
        }

        let buildStartedAt = CFAbsoluteTimeGetCurrent()
        let refreshed = groups.map { group in
            let summaries = group.members.flatMap { member in
                burdenLeaderSummariesByEntityID[member.entityId] ?? []
            }
            return MonitorGroupRowModel(
                group: group,
                origin: originCache.summary(for: group.members),
                burdenLeaders: summaries
            )
        }
        self.key = key
        section = MonitorGroupRowSection(
            rows: refreshed,
            rowBuildDurationMillis: (CFAbsoluteTimeGetCurrent() - buildStartedAt) * 1000.0
        )
        return section
    }
}

func buildEntityGroups(from entities: [EntitySnapshot]) -> [EntityGroup] {
    let entityByID = Dictionary(uniqueKeysWithValues: entities.map { ($0.entityId, $0) })
    let pidToEntityID = pidOwnerMap(for: entities)

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

    func chau7RootRank(_ entity: EntitySnapshot) -> Int? {
        let entityID = entity.entityId.localizedLowercase
        let name = entity.displayName.localizedLowercase
        let executablePath = entity.executablePath?.localizedLowercase ?? ""

        if entityID == "bundle-path:/applications/chau7.app"
            || executablePath == "/applications/chau7.app/contents/macos/chau7" {
            return 0
        }
        if entity.entityKind == .app && name == "chau7" {
            return 1
        }
        if entity.entityKind == .app && name.contains("chau7") {
            return 2
        }
        if name == "chau7" {
            return 3
        }
        if entity.entityKind != .aiAgent
            && (name.contains("chau7") || entity.badges.contains("chau7-live")) {
            return 10
        }
        return nil
    }

    func isChau7ProxyRoot(_ entity: EntitySnapshot) -> Bool {
        chau7RootRank(entity) != nil
    }

    let canonicalChau7RootID = entities
        .compactMap { entity -> (rank: Int, id: String)? in
            guard let rank = chau7RootRank(entity) else { return nil }
            return (rank, entity.entityId)
        }
        .sorted {
            if $0.rank != $1.rank {
                return $0.rank < $1.rank
            }
            return $0.id < $1.id
        }
        .first?
        .id

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
            guard isChau7ProxyRoot(entity) else { continue }
            let rootID = canonicalChau7RootID ?? entity.entityId
            for id in ids {
                roots[id] = rootID
            }
        }
        return roots
    }()

    func parentEntityIDCandidates(for entity: EntitySnapshot) -> [String] {
        var seen = Set<String>()
        let lineageCandidates = entity.processLineage.compactMap { node -> String? in
            guard
                let parentPID = node.parentPid,
                let ownerID = pidToEntityID[parentPID],
                ownerID != entity.entityId,
                seen.insert(ownerID).inserted
            else {
                return nil
            }
            return ownerID
        }
        if !lineageCandidates.isEmpty {
            return lineageCandidates
        }

        return entity.components.compactMap { component -> String? in
            guard
                let parentSummary = component.parentSummary,
                let parentPID = extractParentPIDForGrouping(from: parentSummary),
                let ownerID = pidToEntityID[parentPID],
                ownerID != entity.entityId,
                seen.insert(ownerID).inserted
            else {
                return nil
            }
            return ownerID
        }
    }

    func firstParentEntityID(for entity: EntitySnapshot) -> String? {
        let parentCandidates = parentEntityIDCandidates(for: entity)
        for ownerID in parentCandidates {
            guard let owner = entityByID[ownerID] else { continue }
            if isGenericSystemRoot(owner) {
                continue
            }
            if !entity.processLineage.isEmpty || isChau7ProxyRoot(owner) || sharesStrongContext(entity, owner) {
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
        let sortedMembers = sortEntities(members, by: .friction)
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
                total + visibleProcessComponentCount(entity)
            },
            userSummary: groupUserSummary(for: members),
            oldestStartMillis: startTimes.min() ?? 0,
            newestStartMillis: members.map(\.newestProcessStartMillis).max() ?? 0
        )
    }
}

func filterEntities(
    _ entities: [EntitySnapshot],
    query: String,
    originCache: ProcessOriginSnapshotCache
) -> [EntitySnapshot] {
    let predicates = parseSearchQuery(query, originCache: originCache)
    guard !predicates.isEmpty else {
        return entities
    }
    return entities.filter { entity in predicates.allSatisfy { $0(entity) } }
}

func entityTextHaystacks(
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

func entityMatchesSubstring(
    _ entity: EntitySnapshot,
    _ needle: String,
    originCache: ProcessOriginSnapshotCache
) -> Bool {
    entityTextHaystacks(entity, originCache: originCache).contains { $0.contains(needle) }
}

func parseSearchQuery(
    _ query: String,
    originCache: ProcessOriginSnapshotCache
) -> [(EntitySnapshot) -> Bool] {
    tokenizeSearchQuery(query).compactMap { token in
        searchPredicate(for: token, originCache: originCache)
    }
}

func tokenizeSearchQuery(_ query: String) -> [String] {
    var tokens: [String] = []
    var current = ""
    var inRegex = false
    let chars = Array(query)
    var index = 0
    while index < chars.count {
        let character = chars[index]
        if inRegex {
            current.append(character)
            if character == "\\", index + 1 < chars.count {
                index += 1
                current.append(chars[index])
                index += 1
                continue
            }
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

func searchPredicate(
    for token: String,
    originCache: ProcessOriginSnapshotCache
) -> ((EntitySnapshot) -> Bool)? {
    guard !token.isEmpty else { return nil }

    if token.hasPrefix("/"), let regex = compileRegexToken(token) {
        return { entity in
            entityTextHaystacks(entity, originCache: originCache).contains { haystack in
                regex.firstMatch(in: haystack, range: NSRange(haystack.startIndex..., in: haystack)) != nil
            }
        }
    }

    if let numeric = parseNumericToken(token) {
        return numeric
    }

    if let colon = token.firstIndex(of: ":") {
        let field = String(token[..<colon]).localizedLowercase
        let value = String(token[token.index(after: colon)...]).localizedLowercase
        if !value.isEmpty, let predicate = textFieldPredicate(field: field, value: value, originCache: originCache) {
            return predicate
        }
    }

    let needle = token.localizedLowercase
    return { entityMatchesSubstring($0, needle, originCache: originCache) }
}

func compileRegexToken(_ token: String) -> NSRegularExpression? {
    let body = token.dropFirst()
    guard let closing = body.lastIndex(of: "/") else { return nil }
    let pattern = String(body[..<closing])
    let flags = String(body[body.index(after: closing)...])
    guard !pattern.isEmpty else { return nil }
    var options: NSRegularExpression.Options = []
    if flags.contains("i") { options.insert(.caseInsensitive) }
    return try? NSRegularExpression(pattern: pattern, options: options)
}

func textFieldPredicate(
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

func processOriginMatchesSearch(_ origin: ProcessOriginSummary, value: String) -> Bool {
    let normalizedValue = normalizedProcessOriginSearchValue(value)
    if origin.kind.rawValue == normalizedValue {
        return true
    }
    return origin.searchTokens.contains { token in
        token.localizedLowercase.contains(normalizedValue)
    }
}

func normalizedProcessOriginSearchValue(_ value: String) -> String {
    switch value {
    case "apps", "applications", "application": return "app"
    case "helpers": return "helper"
    case "systems": return "system"
    case "services", "daemons", "daemon": return "service"
    case "term", "terminal", "shell": return "cli"
    default: return value
    }
}

func parseNumericToken(_ token: String) -> ((EntitySnapshot) -> Bool)? {
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

func parseNumericValue(_ raw: String, field: String) -> Double? {
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
        return number * 1024 * 1024
    }
    return number
}

func sortGroups(_ groups: [EntityGroup], by sortKey: SortKey) -> [EntityGroup] {
    groups.sorted { compareGroups($0, $1, by: sortKey) }
}

func sortEntities(_ entities: [EntitySnapshot], by sortKey: SortKey) -> [EntitySnapshot] {
    entities.sorted { compareEntities($0, $1, by: sortKey) }
}

func expandedMemberEntities(for group: EntityGroup, by sortKey: SortKey) -> [EntitySnapshot] {
    sortEntities(
        group.members.filter { $0.entityId != group.root.entityId },
        by: sortKey
    )
}

func expandedProcessComponents(for group: EntityGroup, by sortKey: SortKey) -> [MonitorProcessComponentRef] {
    sortEntities(group.members, by: sortKey)
        .enumerated()
        .flatMap { ownerIndex, owner in
            let lineageRefs = owner.processLineage.map {
                MonitorProcessComponentRef(owner: owner, lineage: $0, ownerSortIndex: ownerIndex)
            }
            if !lineageRefs.isEmpty {
                return lineageRefs
            }

            return owner.components.compactMap { component -> MonitorProcessComponentRef? in
                guard component.kind != .adapterContext, component.processId != nil else {
                    return nil
                }
                return MonitorProcessComponentRef(owner: owner, component: component, ownerSortIndex: ownerIndex)
            }
        }
        .sorted { compareProcessComponents($0, $1, by: sortKey) }
}

func pidOwnerMap(for entities: [EntitySnapshot]) -> [UInt32: String] {
    var owners: [UInt32: String] = [:]
    for entity in entities {
        for node in entity.processLineage {
            owners[node.pid] = entity.entityId
        }
    }
    if !owners.isEmpty {
        return owners
    }

    for entity in entities {
        for component in entity.components {
            if let pid = component.processId {
                owners[pid] = entity.entityId
            }
        }
    }
    return owners
}

func uniqueEntityIDsPreservingOrder(_ entityIDs: [String]) -> [String] {
    var seen = Set<String>()
    return entityIDs.filter { seen.insert($0).inserted }
}

func buildGroupedEntities(
    from entities: [EntitySnapshot],
    query: String,
    sortKey: SortKey,
    originCache: ProcessOriginSnapshotCache
) -> [EntityGroup] {
    let groups = buildEntityGroups(from: entities)
    let predicates = parseSearchQuery(query, originCache: originCache)
    guard !predicates.isEmpty else {
        return sortGroups(groups, by: sortKey)
    }
    let visibleGroups = groups.filter { group in
        group.members.contains { entity in
            predicates.allSatisfy { $0(entity) }
        }
    }
    return sortGroups(visibleGroups, by: sortKey)
}

func extractParentPIDForGrouping(from parentSummary: String) -> UInt32? {
    guard let pidRange = parentSummary.range(of: "pid ") else { return nil }
    let digits = parentSummary[pidRange.upperBound...].prefix(while: \.isNumber)
    return UInt32(digits)
}

func visibleProcessComponentCount(_ entity: EntitySnapshot) -> Int {
    if !entity.processLineage.isEmpty {
        return entity.processLineage.count
    }
    return entity.components.reduce(0) { total, component in
        total + (component.kind != .adapterContext && component.processId != nil ? 1 : 0)
    }
}

func liveProcessCount(for entity: EntitySnapshot) -> Int {
    if !entity.processLineage.isEmpty {
        return entity.processLineage.count
    }
    return entity.components.filter { $0.kind != .adapterContext && $0.processId != nil }.count
}

func compareEntities(_ left: EntitySnapshot, _ right: EntitySnapshot, by sortKey: SortKey) -> Bool {
    switch sortKey {
    case .friction:
        return descending(left.friction.totalScore, right.friction.totalScore) {
            entityTieBreak(left, right)
        }
    case .cpu:
        return descending(left.metrics.cpuPercent, right.metrics.cpuPercent) {
            entityTieBreak(left, right)
        }
    case .memory:
        return descending(entityEffectiveMemoryBytes(left), entityEffectiveMemoryBytes(right)) {
            entityTieBreak(left, right)
        }
    case .wakeups:
        return descending(left.metrics.wakeupsPerSecond, right.metrics.wakeupsPerSecond) {
            entityTieBreak(left, right)
        }
    case .processCount:
        return descending(liveProcessCount(for: left), liveProcessCount(for: right)) {
            entityTieBreak(left, right)
        }
    case .disk:
        return descending(left.metrics.diskReadBps + left.metrics.diskWriteBps, right.metrics.diskReadBps + right.metrics.diskWriteBps) {
            entityTieBreak(left, right)
        }
    case .network:
        return descending(left.metrics.networkReceiveBps + left.metrics.networkSendBps, right.metrics.networkReceiveBps + right.metrics.networkSendBps) {
            entityTieBreak(left, right)
        }
    case .energy:
        return descending(left.friction.energyImpactScore, right.friction.energyImpactScore) {
            entityTieBreak(left, right)
        }
    case .alphabeticalAsc:
        return entityNameAscending(left, right)
    case .alphabeticalDesc:
        return entityNameDescending(left, right)
    case .oldestFirst:
        let leftStart = left.oldestProcessStartMillis == 0 ? UInt64.max : left.oldestProcessStartMillis
        let rightStart = right.oldestProcessStartMillis == 0 ? UInt64.max : right.oldestProcessStartMillis
        return ascending(leftStart, rightStart) {
            entityTieBreak(left, right)
        }
    case .newestFirst:
        return descending(left.newestProcessStartMillis, right.newestProcessStartMillis) {
            entityTieBreak(left, right)
        }
    }
}

func compareGroups(_ left: EntityGroup, _ right: EntityGroup, by sortKey: SortKey) -> Bool {
    switch sortKey {
    case .friction:
        return descending(left.frictionScore, right.frictionScore) {
            groupTieBreak(left, right)
        }
    case .cpu:
        return descending(left.cpuPercent, right.cpuPercent) {
            groupTieBreak(left, right)
        }
    case .memory:
        return descending(left.memoryBytes, right.memoryBytes) {
            groupTieBreak(left, right)
        }
    case .wakeups:
        return descending(left.wakeupsPerSecond, right.wakeupsPerSecond) {
            groupTieBreak(left, right)
        }
    case .processCount:
        return descending(left.processCount, right.processCount) {
            groupTieBreak(left, right)
        }
    case .disk:
        return descending(left.diskBps, right.diskBps) {
            groupTieBreak(left, right)
        }
    case .network:
        return descending(left.networkBps, right.networkBps) {
            groupTieBreak(left, right)
        }
    case .energy:
        return descending(left.energyScore, right.energyScore) {
            groupTieBreak(left, right)
        }
    case .alphabeticalAsc:
        return groupNameAscending(left, right)
    case .alphabeticalDesc:
        return groupNameDescending(left, right)
    case .oldestFirst:
        let leftStart = left.oldestStartMillis == 0 ? UInt64.max : left.oldestStartMillis
        let rightStart = right.oldestStartMillis == 0 ? UInt64.max : right.oldestStartMillis
        return ascending(leftStart, rightStart) {
            groupTieBreak(left, right)
        }
    case .newestFirst:
        return descending(left.newestStartMillis, right.newestStartMillis) {
            groupTieBreak(left, right)
        }
    }
}

func compareProcessComponents(
    _ left: MonitorProcessComponentRef,
    _ right: MonitorProcessComponentRef,
    by sortKey: SortKey
) -> Bool {
    switch sortKey {
    case .friction:
        return descending(left.owner.friction.totalScore, right.owner.friction.totalScore) {
            processComponentTieBreak(left, right)
        }
    case .cpu:
        return descending(left.cpuPercent, right.cpuPercent) {
            processComponentTieBreak(left, right)
        }
    case .memory:
        return descending(componentEffectiveMemoryBytes(left), componentEffectiveMemoryBytes(right)) {
            processComponentTieBreak(left, right)
        }
    case .wakeups:
        return descending(left.owner.metrics.wakeupsPerSecond, right.owner.metrics.wakeupsPerSecond) {
            processComponentTieBreak(left, right)
        }
    case .processCount:
        return descending(liveProcessCount(for: left.owner), liveProcessCount(for: right.owner)) {
            processComponentTieBreak(left, right)
        }
    case .disk:
        let leftDisk = left.owner.metrics.diskReadBps + left.owner.metrics.diskWriteBps
        let rightDisk = right.owner.metrics.diskReadBps + right.owner.metrics.diskWriteBps
        return descending(leftDisk, rightDisk) {
            processComponentTieBreak(left, right)
        }
    case .network:
        let leftNetwork = left.owner.metrics.networkReceiveBps + left.owner.metrics.networkSendBps
        let rightNetwork = right.owner.metrics.networkReceiveBps + right.owner.metrics.networkSendBps
        return descending(leftNetwork, rightNetwork) {
            processComponentTieBreak(left, right)
        }
    case .energy:
        return descending(left.owner.friction.energyImpactScore, right.owner.friction.energyImpactScore) {
            processComponentTieBreak(left, right)
        }
    case .alphabeticalAsc:
        return processComponentNameAscending(left, right)
    case .alphabeticalDesc:
        return processComponentNameDescending(left, right)
    case .oldestFirst:
        let leftStart = left.startTimeMillis == 0 ? UInt64.max : left.startTimeMillis
        let rightStart = right.startTimeMillis == 0 ? UInt64.max : right.startTimeMillis
        return ascending(leftStart, rightStart) {
            processComponentTieBreak(left, right)
        }
    case .newestFirst:
        return descending(left.startTimeMillis, right.startTimeMillis) {
            processComponentTieBreak(left, right)
        }
    }
}

private func descending<T: Comparable>(_ left: T, _ right: T, tieBreak: () -> Bool) -> Bool {
    if left != right {
        return left > right
    }
    return tieBreak()
}

private func ascending<T: Comparable>(_ left: T, _ right: T, tieBreak: () -> Bool) -> Bool {
    if left != right {
        return left < right
    }
    return tieBreak()
}

private func entityTieBreak(_ left: EntitySnapshot, _ right: EntitySnapshot) -> Bool {
    let nameOrder = left.displayName.localizedCaseInsensitiveCompare(right.displayName)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedAscending
    }
    return left.entityId < right.entityId
}

private func entityNameAscending(_ left: EntitySnapshot, _ right: EntitySnapshot) -> Bool {
    let nameOrder = left.displayName.localizedCaseInsensitiveCompare(right.displayName)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedAscending
    }
    return left.entityId < right.entityId
}

private func entityNameDescending(_ left: EntitySnapshot, _ right: EntitySnapshot) -> Bool {
    let nameOrder = left.displayName.localizedCaseInsensitiveCompare(right.displayName)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedDescending
    }
    return left.entityId < right.entityId
}

private func groupTieBreak(_ left: EntityGroup, _ right: EntityGroup) -> Bool {
    let nameOrder = left.root.displayName.localizedCaseInsensitiveCompare(right.root.displayName)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedAscending
    }
    return left.id < right.id
}

private func groupNameAscending(_ left: EntityGroup, _ right: EntityGroup) -> Bool {
    let nameOrder = left.root.displayName.localizedCaseInsensitiveCompare(right.root.displayName)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedAscending
    }
    return left.id < right.id
}

private func groupNameDescending(_ left: EntityGroup, _ right: EntityGroup) -> Bool {
    let nameOrder = left.root.displayName.localizedCaseInsensitiveCompare(right.root.displayName)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedDescending
    }
    return left.id < right.id
}

private func componentEffectiveMemoryBytes(_ component: MonitorProcessComponentRef) -> UInt64 {
    max(component.memoryPhysicalFootprintBytes, component.memoryBytes)
}

private func processComponentTieBreak(
    _ left: MonitorProcessComponentRef,
    _ right: MonitorProcessComponentRef
) -> Bool {
    if left.ownerSortIndex != right.ownerSortIndex {
        return left.ownerSortIndex < right.ownerSortIndex
    }

    let nameOrder = left.title.localizedCaseInsensitiveCompare(right.title)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedAscending
    }

    if left.owner.entityId != right.owner.entityId {
        return left.owner.entityId < right.owner.entityId
    }

    return left.pid < right.pid
}

private func processComponentNameAscending(
    _ left: MonitorProcessComponentRef,
    _ right: MonitorProcessComponentRef
) -> Bool {
    let nameOrder = left.title.localizedCaseInsensitiveCompare(right.title)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedAscending
    }
    return processComponentTieBreak(left, right)
}

private func processComponentNameDescending(
    _ left: MonitorProcessComponentRef,
    _ right: MonitorProcessComponentRef
) -> Bool {
    let nameOrder = left.title.localizedCaseInsensitiveCompare(right.title)
    if nameOrder != .orderedSame {
        return nameOrder == .orderedDescending
    }
    return processComponentTieBreak(left, right)
}
