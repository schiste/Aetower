import AetowerBridge
import Foundation
import SwiftUI

/// Flat Monitor projection backed by compact UiProcessRow deltas. Detailed
/// entity searches and sorts deliberately fall back to the full snapshot path.
struct MonitorLiveRowsProjection {
    static func filterAndSort(
        rows: [UiProcessRow],
        query: String,
        sortKey: SortKey,
        advancedFilterEntityIds: Set<String>?
    ) -> [UiProcessRow] {
        let tokens = tokenizeSearchQuery(query)

        return rows
            .filter { row in
                if let advancedFilterEntityIds,
                   !advancedFilterEntityIds.contains(row.entityId) {
                    return false
                }
                return tokens.allSatisfy { matches($0, row: row) }
            }
            .sorted { lhs, rhs in
                let leftValue = sortValue(lhs, key: sortKey)
                let rightValue = sortValue(rhs, key: sortKey)
                if leftValue != rightValue {
                    return leftValue > rightValue
                }
                let nameOrder = lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName)
                if nameOrder != .orderedSame {
                    return sortKey == .alphabeticalDesc
                        ? nameOrder == .orderedDescending
                        : nameOrder == .orderedAscending
                }
                return lhs.entityId < rhs.entityId
            }
    }

    static func supportsLiveSearch(_ query: String) -> Bool {
        tokenizeSearchQuery(query).allSatisfy { token in
            if token.hasPrefix("/") { return false }

            if let colon = token.firstIndex(of: ":") {
                let field = token[..<colon].localizedLowercase
                let value = token[token.index(after: colon)...]
                // `bundle:` maps exactly to the compact row's bundle ID. The
                // entity path also searches component fields/origins/badges,
                // which are intentionally not present in UiProcessRow.
                return field == "bundle" && !value.isEmpty
            }

            guard let operatorRange = numericOperatorRange(in: token),
                  let field = numericField(in: token, operatorRange: operatorRange),
                  supportedNumericFields.contains(field),
                  parseNumericToken(token) != nil
            else {
                // Unqualified text searches entity components, badges, command
                // lines, users and adapter context. Fall back to full details.
                return false
            }
            return true
        }
    }

    static func supportsLiveSort(_ sortKey: SortKey) -> Bool {
        switch sortKey {
        case .energy, .processCount, .oldestFirst, .newestFirst:
            // Energy sorts by Friction's impact score; process count uses the
            // live component count. Neither is equivalent to the compact row.
            false
        default:
            true
        }
    }

    private static let supportedNumericFields: Set<String> = [
        "cpu", "mem", "memory", "ram", "energy", "wakeups", "friction",
        "procs", "processes", "count", "threads",
    ]

    private static let numericOperators = [">=", "<=", ">", "<", "==", "="]

    private static func numericOperatorRange(in token: String) -> Range<String.Index>? {
        numericOperators.compactMap { token.range(of: $0) }.min {
            $0.lowerBound < $1.lowerBound
        }
    }

    private static func numericField(
        in token: String,
        operatorRange: Range<String.Index>
    ) -> String? {
        let field = String(token[..<operatorRange.lowerBound]).localizedLowercase
        return field.isEmpty ? nil : field
    }

    private static func matches(_ token: String, row: UiProcessRow) -> Bool {
        if let colon = token.firstIndex(of: ":") {
            let field = token[..<colon].localizedLowercase
            let value = token[token.index(after: colon)...].localizedLowercase
            guard field == "bundle", !value.isEmpty else { return false }
            return row.bundleId?.localizedLowercase.contains(value) == true
        }

        if let operatorRange = numericOperatorRange(in: token),
           let field = numericField(in: token, operatorRange: operatorRange) {
            let operand = String(token[operatorRange.upperBound...])
            guard let threshold = parseNumericValue(operand, field: field),
                  let value = numericValue(for: field, row: row)
            else {
                return false
            }
            switch token[operatorRange] {
            case ">": return value > threshold
            case "<": return value < threshold
            case ">=": return value >= threshold
            case "<=": return value <= threshold
            default: return value == threshold
            }
        }

        // This path is reached only for exact live-supported search tokens.
        // Free text is intentionally routed to the full entity search.
        return false
    }

    private static func numericValue(for field: String, row: UiProcessRow) -> Double? {
        switch field {
        case "cpu": Double(row.cpuPercent)
        case "mem", "memory", "ram": Double(row.memoryResidentBytes)
        case "friction": Double(row.frictionScore)
        case "wakeups": Double(row.wakeupsPerSecond)
        case "procs", "processes", "count": Double(row.processCount)
        case "threads": Double(row.threadCount)
        case "energy": row.energyNjPerS
        default: nil
        }
    }

    private static func sortValue(_ row: UiProcessRow, key: SortKey) -> Double {
        switch key {
        case .friction: Double(row.frictionScore)
        case .cpu: Double(row.cpuPercent)
        case .memory: Double(max(row.memoryResidentBytes, row.memoryPhysicalFootprintBytes))
        case .wakeups: Double(row.wakeupsPerSecond)
        case .processCount: Double(row.processCount)
        case .disk: Double(row.diskReadBps) + Double(row.diskWriteBps)
        case .network: Double(row.networkReceiveBps) + Double(row.networkSendBps)
        case .energy: row.energyNjPerS
        case .alphabeticalAsc, .alphabeticalDesc, .oldestFirst, .newestFirst: 0
        }
    }
}

enum MonitorSnapshotPolicy {
    static func requiresFullSnapshot(
        isGroupedMode: Bool,
        hasSelectedEntity: Bool,
        hasDetailedFilter: Bool,
        hasCompleteLiveRows: Bool
    ) -> Bool {
        isGroupedMode || hasSelectedEntity || hasDetailedFilter || !hasCompleteLiveRows
    }
}

struct MonitorLiveRowsSnapshot {
    let rows: [UiProcessRow]
    let buildDurationMillis: Double
}

@MainActor
final class MonitorLiveRowsStore: ObservableObject {
    private struct Key: Hashable {
        let sequence: UInt64
        let query: String
        let sortKey: String
        let advancedFilterEntityIds: Set<String>?
    }

    private var key: Key?
    private var cached = MonitorLiveRowsSnapshot(rows: [], buildDurationMillis: 0)

    func snapshot(
        rows: [UiProcessRow],
        sequence: UInt64,
        query: String,
        sortKey: SortKey,
        advancedFilterEntityIds: Set<String>?
    ) -> MonitorLiveRowsSnapshot {
        let nextKey = Key(
            sequence: sequence,
            query: query,
            sortKey: sortKey.rawValue,
            advancedFilterEntityIds: advancedFilterEntityIds
        )
        guard key != nextKey else { return cached }

        let startedAt = CFAbsoluteTimeGetCurrent()
        let projected = MonitorLiveRowsProjection.filterAndSort(
            rows: rows,
            query: query,
            sortKey: sortKey,
            advancedFilterEntityIds: advancedFilterEntityIds
        )
        key = nextKey
        cached = MonitorLiveRowsSnapshot(
            rows: projected,
            buildDurationMillis: (CFAbsoluteTimeGetCurrent() - startedAt) * 1_000
        )
        return cached
    }
}

struct MonitorLiveEntityRow: View {
    let row: UiProcessRow
    let isSelected: Bool

    var body: some View {
        HStack(spacing: AetowerDesign.Spacing.md) {
            Circle()
                .fill(row.anomalyDetected ? AetowerDesign.Status.error : AetowerDesign.Tone.friction)
                .frame(width: 7, height: 7)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Text(row.displayName)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                    if row.isForeground {
                        AetowerBadge("Foreground", tone: AetowerDesign.Tone.cpu)
                    }
                    if row.anomalyDetected {
                        AetowerBadge("Anomaly", tone: AetowerDesign.Status.error)
                    }
                }
                Text(row.executablePath ?? row.bundleId ?? row.entityId)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            metric(String(format: "%.1f%%", row.cpuPercent), label: "CPU")
            metric(formatBytes(max(row.memoryResidentBytes, row.memoryPhysicalFootprintBytes)), label: "Memory")
            metric(formatWakeups(row.wakeupsPerSecond), label: "Wakeups")
            metric(String(row.processCount), label: "PIDs")
            metric(String(format: "%.1f", row.frictionScore), label: "Friction")
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .background(
            isSelected ? AetowerDesign.Surface.rowSelected : Color.clear,
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm)
        )
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(row.displayName), CPU \(String(format: "%.1f", row.cpuPercent)) percent, memory \(formatBytes(max(row.memoryResidentBytes, row.memoryPhysicalFootprintBytes)) )")
    }

    private func metric(_ value: String, label: String) -> some View {
        VStack(alignment: .trailing, spacing: 2) {
            Text(value)
                .font(AetowerDesign.Typography.compactData(size: 11, weight: .medium))
                .foregroundStyle(AetowerDesign.Ink.primary)
                .monospacedDigit()
            Text(label)
                .font(AetowerDesign.Typography.metadata)
                .foregroundStyle(AetowerDesign.Ink.tertiary)
        }
        .frame(minWidth: 56, alignment: .trailing)
    }
}
