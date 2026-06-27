import AppKit
import AetowerBridge
import SwiftUI

private enum PersistenceSection: String, CaseIterable, Identifiable {
    case summary
    case attention
    case activeNow
    case allItems
    case locations

    var id: String { rawValue }

    var title: String {
        switch self {
        case .summary: return "Summary"
        case .attention: return "Attention"
        case .activeNow: return "Active Now"
        case .allItems: return "All Items"
        case .locations: return "Locations"
        }
    }

    var systemImage: String {
        switch self {
        case .summary: return "rectangle.grid.2x2"
        case .attention: return "exclamationmark.triangle"
        case .activeNow: return "waveform.path.ecg"
        case .allItems: return "list.bullet.rectangle"
        case .locations: return "folder"
        }
    }

    var detail: String {
        switch self {
        case .summary: return "Inventory, risk, scan mode, and cost."
        case .attention: return "Risky, changed, or currently active items."
        case .activeNow: return "Startup entries linked to live processes."
        case .allItems: return "Full inventory with filter and search."
        case .locations: return "Scanned paths and unavailable sources."
        }
    }
}

/// Fast, operator-oriented startup/persistence inventory. The default path is a
/// cached metadata scan; code signing and deeper process inspection are explicit
/// actions so this tab does not stall Aetower while opening.
public struct PersistenceScannerView: View {
    let state: AppState
    let settings: SettingsStore

    @State private var hideAppleSigned = true
    @State private var filter: PersistenceFilter = .all
    @State private var selectedSection: PersistenceSection = .summary
    @State private var searchText = ""
    @State private var expandedItemID: String?
    @State private var runtimeIndex: PersistenceRuntimeIndex
    @State private var runtimeIndexSequence: UInt64

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
        _runtimeIndex = State(initialValue: PersistenceRuntimeIndex(snapshot: state.snapshot))
        _runtimeIndexSequence = State(initialValue: state.snapshot.sequence)
    }

    private var report: PersistenceScanReportModel? { state.persistenceScanReport }

    private struct DerivedInventory {
        let attentionItems: [PersistenceItemModel]
        let activeNowItems: [PersistenceItemModel]
        let allItems: [PersistenceItemModel]
        let activeSummaryCount: Int
        let attentionSummaryCount: Int
        let changedSummaryCount: Int

        func items(for section: PersistenceSection) -> [PersistenceItemModel] {
            switch section {
            case .attention:
                return attentionItems
            case .activeNow:
                return activeNowItems
            case .allItems:
                return allItems
            case .summary, .locations:
                return []
            }
        }
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                header
                if let report {
                    reportContent(report)
                } else if state.persistenceScanIsLoading {
                    loadingState
                } else {
                    emptyState
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
        .onAppear {
            state.ensurePersistenceScan()
        }
        .task(id: state.snapshot.sequence) {
            refreshRuntimeIndexIfNeeded(snapshot: state.snapshot)
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            Text("Startup & persistence")
                .font(.system(size: 28, weight: .semibold, design: .rounded))
            Text("What starts on this Mac, where it comes from, whether it is active, and what deserves review.")
                .foregroundStyle(.secondary)
        }
    }

    private var loadingState: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            ProgressView()
            Text("Building lightweight startup inventory...")
                .foregroundStyle(.secondary)
        }
        .padding(.top, AetowerDesign.Spacing.xl)
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            ContentUnavailableView(
                "No scan yet",
                systemImage: "lock.shield",
                description: Text("Run a fast scan to enumerate this Mac's startup and persistence surface.")
            )
            Button {
                state.runPersistenceScan()
            } label: {
                Label("Scan now", systemImage: "magnifyingglass")
            }
            .buttonStyle(.borderedProminent)
            errorText
        }
    }

    private func reportContent(_ report: PersistenceScanReportModel) -> some View {
        let runtimeIndex = runtimeIndex
        let inventory = derivedInventory(report, runtimeIndex: runtimeIndex)
        return LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            controls(report)
            sectionSwitcher(report, inventory: inventory)
            if let error = state.persistenceScanError {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.warning)
            }
            if report.truncated {
                warningBanner("Inventory reached the safety cap. Results are truncated to keep Aetower responsive.")
            }
            sectionContent(report, runtimeIndex: runtimeIndex, inventory: inventory)
        }
    }

    private func sectionSwitcher(
        _ report: PersistenceScanReportModel,
        inventory: DerivedInventory
    ) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 170), spacing: AetowerDesign.Spacing.sm)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.sm
        ) {
            ForEach(PersistenceSection.allCases) { section in
                sectionButton(section, count: sectionCount(section, report: report, inventory: inventory))
            }
        }
    }

    private func sectionButton(_ section: PersistenceSection, count: Int) -> some View {
        let isSelected = selectedSection == section
        return Button {
            withAnimation(AetowerDesign.Motion.quick) {
                selectedSection = section
                expandedItemID = nil
            }
        } label: {
            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 7) {
                    Image(systemName: section.systemImage)
                        .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                    Text(section.title)
                        .font(.subheadline.weight(.semibold))
                    Spacer()
                    Text("\(count)")
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
                Text(section.detail)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .padding(AetowerDesign.Spacing.sm)
            .frame(maxWidth: .infinity, minHeight: 86, alignment: .topLeading)
            .background(
                isSelected ? Color.accentColor.opacity(0.11) : AetowerDesign.Surface.card,
                in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
                    .stroke(isSelected ? Color.accentColor.opacity(0.32) : Color.clear, lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func sectionContent(
        _ report: PersistenceScanReportModel,
        runtimeIndex: PersistenceRuntimeIndex,
        inventory: DerivedInventory
    ) -> some View {
        switch selectedSection {
        case .summary:
            summaryStrip(report, inventory: inventory)
            summaryGuidance(report, inventory)
        case .attention:
            itemList(
                title: "Attention",
                emptyTitle: "No startup items need attention",
                emptyDescription: "Aetower found no risky, changed, or active startup entries with the current search settings.",
                items: inventory.items(for: .attention),
                runtimeIndex: runtimeIndex
            )
        case .activeNow:
            itemList(
                title: "Active now",
                emptyTitle: "No active startup items",
                emptyDescription: "No startup entries are currently linked to live process metrics.",
                items: inventory.items(for: .activeNow),
                runtimeIndex: runtimeIndex
            )
        case .allItems:
            itemList(
                title: "All startup items",
                emptyTitle: "No matching startup items",
                emptyDescription: "Change the filter or search text to broaden the inventory.",
                items: inventory.items(for: .allItems),
                runtimeIndex: runtimeIndex
            )
        case .locations:
            locationsSection(report)
        }
    }

    private func itemList(
        title: String,
        emptyTitle: String,
        emptyDescription: String,
        items: [PersistenceItemModel],
        runtimeIndex: PersistenceRuntimeIndex
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            Text(title)
                .font(.title3.weight(.semibold))

            if items.isEmpty {
                ContentUnavailableView(
                    emptyTitle,
                    systemImage: "line.3.horizontal.decrease.circle",
                    description: Text(emptyDescription)
                )
                .frame(maxWidth: .infinity)
            } else {
                let itemsByKind = Dictionary(grouping: items, by: \.kind)
                ForEach(groupedKinds(items), id: \.self) { kind in
                    section(
                        kind: kind,
                        items: itemsByKind[kind] ?? [],
                        runtimeIndex: runtimeIndex
                    )
                }
            }
        }
    }

    private func summaryGuidance(
        _ report: PersistenceScanReportModel,
        _ inventory: DerivedInventory
    ) -> some View {
        let attentionCount = inventory.attentionItems.count
        let activeCount = inventory.activeNowItems.count
        return GroupBox("How to use this scan") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                guidanceLine(
                    "Start with Attention",
                    detail: attentionCount == 0
                        ? "No risky, changed, or active entries require review right now."
                        : "\(attentionCount) entry or entries deserve review before browsing the full inventory.",
                    systemImage: "exclamationmark.triangle"
                )
                guidanceLine(
                    "Check Active Now",
                    detail: "\(activeCount) startup entries are linked to live process metrics in the current Monitor snapshot.",
                    systemImage: "waveform.path.ecg"
                )
                guidanceLine(
                    "Use Deep audit selectively",
                    detail: report.scanMode == "deep"
                        ? "This scan includes signing detail for targets."
                        : "Fast scan avoids code-signing every target. Run deep audit when an entry is unexpected.",
                    systemImage: "signature"
                )
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
    }

    private func guidanceLine(_ title: String, detail: String, systemImage: String) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: systemImage)
                .foregroundStyle(.secondary)
                .frame(width: 18)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.caption.weight(.semibold))
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func locationsSection(_ report: PersistenceScanReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            scannedLocations(report)
            if report.degraded.isEmpty {
                ContentUnavailableView(
                    "All sources available",
                    systemImage: "checkmark.shield",
                    description: Text("Aetower did not report unavailable startup sources for this scan.")
                )
                .frame(maxWidth: .infinity)
            } else {
                degradedSection(report.degraded)
            }
        }
    }

    private func sectionCount(
        _ section: PersistenceSection,
        report: PersistenceScanReportModel,
        inventory: DerivedInventory
    ) -> Int {
        switch section {
        case .summary:
            return Int(report.summary.totalItems)
        case .locations:
            return report.scannedLocations.count + report.degraded.count
        case .attention, .activeNow, .allItems:
            return inventory.items(for: section).count
        }
    }

    private func summaryStrip(
        _ report: PersistenceScanReportModel,
        inventory: DerivedInventory
    ) -> some View {
        return LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 160), spacing: AetowerDesign.Spacing.sm)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.sm
        ) {
            summaryCard(
                "Entries",
                value: "\(report.summary.totalItems)",
                detail: "\(report.summary.userScopeCount) user · \(report.summary.systemScopeCount) system",
                tone: AetowerDesign.Status.neutral
            )
            summaryCard(
                "Attention",
                value: "\(inventory.attentionSummaryCount)",
                detail: "\(report.summary.dangerCount) danger · \(report.summary.warningCount) warning",
                tone: inventory.attentionSummaryCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
            summaryCard(
                "Active now",
                value: "\(inventory.activeSummaryCount)",
                detail: "Linked to live process metrics",
                tone: inventory.activeSummaryCount > 0 ? AetowerDesign.Tone.cpu : AetowerDesign.Status.neutral
            )
            summaryCard(
                "Changed",
                value: "\(inventory.changedSummaryCount)",
                detail: "Since last scan or last 7 days",
                tone: inventory.changedSummaryCount > 0 ? AetowerDesign.Tone.network : AetowerDesign.Status.neutral
            )
            summaryCard(
                report.scanMode == "deep" ? "Signing" : "Fast scan",
                value: report.scanMode == "deep" ? "\(report.summary.unsignedOrAdhocCount)" : "\(report.summary.unknownSignatureCount)",
                detail: report.scanMode == "deep" ? "unsigned/ad-hoc targets" : "targets not signed yet",
                tone: report.scanMode == "deep" ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
            )
            summaryCard(
                "Scan cost",
                value: "\(report.scanDurationMillis) ms",
                detail: report.scanMode == "deep" ? "deep audit" : "cached metadata path",
                tone: report.scanDurationMillis > 1500 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
        }
    }

    private func summaryCard(_ title: String, value: String, detail: String, tone: Color) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 24, weight: .semibold, design: .rounded))
                .monospacedDigit()
            Text(detail)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 104, alignment: .leading)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(tone.opacity(0.55))
                .frame(height: 2)
        }
        .background(AetowerDesign.Surface.card)
    }

    private func controls(_ report: PersistenceScanReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.md) {
                Button {
                    state.runPersistenceScan()
                } label: {
                    Label("Fast rescan", systemImage: "arrow.clockwise")
                }
                .buttonStyle(.bordered)
                .disabled(state.persistenceScanIsLoading)

                Button {
                    state.runPersistenceScan(deep: true)
                } label: {
                    Label("Run deep audit", systemImage: "signature")
                }
                .buttonStyle(.borderedProminent)
                .disabled(state.persistenceScanIsLoading)
                .help("Adds code-signing detail. Cached by startup item path, mtime, size, and executable target.")

                Toggle("Hide Apple/System", isOn: $hideAppleSigned)
                    .toggleStyle(.switch)

                Spacer()

                if state.persistenceScanIsLoading {
                    ProgressView()
                        .scaleEffect(0.75)
                }
                if let completed = state.persistenceScanCompletedAt {
                    Text("scanned \(completed.formatted(date: .omitted, time: .shortened))")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }

            HStack(spacing: AetowerDesign.Spacing.md) {
                if selectedSection == .allItems {
                    Picker("Filter", selection: $filter) {
                        ForEach(PersistenceFilter.allCases) { option in
                            Text(option.label).tag(option)
                        }
                    }
                    .pickerStyle(.segmented)
                }

                TextField("Search label, path, executable, reason", text: $searchText)
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .frame(maxWidth: selectedSection == .allItems ? 360 : 520)
            }

            Text(report.scanMode == "deep"
                ? "Deep audit includes signing. Runtime impact is joined from the live Monitor snapshot."
                : "Fast scan avoids code-signing every target. Use Deep audit only when an entry is unexpected.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func refreshRuntimeIndexIfNeeded(snapshot: SystemSnapshot) {
        guard runtimeIndexSequence != snapshot.sequence else { return }
        runtimeIndex = PersistenceRuntimeIndex(snapshot: snapshot)
        runtimeIndexSequence = snapshot.sequence
    }

    private func derivedInventory(
        _ report: PersistenceScanReportModel,
        runtimeIndex: PersistenceRuntimeIndex
    ) -> DerivedInventory {
        DerivedInventory(
            attentionItems: filteredItems(report, runtimeIndex: runtimeIndex, section: .attention),
            activeNowItems: filteredItems(report, runtimeIndex: runtimeIndex, section: .activeNow),
            allItems: filteredItems(report, runtimeIndex: runtimeIndex, section: .allItems),
            activeSummaryCount: report.items.filter { !runtimeIndex.matches(for: $0).isEmpty }.count,
            attentionSummaryCount: report.items.filter { itemNeedsAttention($0, runtimeIndex: runtimeIndex) }.count,
            changedSummaryCount: report.items.filter { isChanged($0) }.count
        )
    }

    private func filteredItems(
        _ report: PersistenceScanReportModel,
        runtimeIndex: PersistenceRuntimeIndex,
        section: PersistenceSection
    ) -> [PersistenceItemModel] {
        let needle = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return report.items
            .filter { item in
                if hideAppleSigned && item.isApple { return false }
                if !needle.isEmpty && !searchCorpus(item).contains(needle) { return false }
                switch section {
                case .summary, .locations:
                    return true
                case .attention:
                    return itemNeedsAttention(item, runtimeIndex: runtimeIndex)
                case .activeNow:
                    return !runtimeIndex.matches(for: item).isEmpty
                case .allItems:
                    return matchesAllItemsFilter(item, runtimeIndex: runtimeIndex)
                }
            }
            .sorted { lhs, rhs in
                let leftRank = riskRank(lhs, runtimeIndex: runtimeIndex)
                let rightRank = riskRank(rhs, runtimeIndex: runtimeIndex)
                if leftRank != rightRank { return leftRank > rightRank }
                return lhs.displayTitle.localizedCaseInsensitiveCompare(rhs.displayTitle) == .orderedAscending
            }
    }

    private func matchesAllItemsFilter(
        _ item: PersistenceItemModel,
        runtimeIndex: PersistenceRuntimeIndex
    ) -> Bool {
        switch filter {
        case .attention:
            return itemNeedsAttention(item, runtimeIndex: runtimeIndex)
        case .active:
            return !runtimeIndex.matches(for: item).isEmpty
        case .changed:
            return isChanged(item)
        case .unsigned:
            return item.signature?.classification == "unsigned"
                || item.signature?.classification == "adhoc"
                || item.signature?.classification == "other"
                || (!item.isApple && item.signature == nil && item.kind != "cron")
        case .user:
            return item.isUserScope
        case .system:
            return !item.isUserScope
        case .all:
            return true
        }
    }

    private func itemNeedsAttention(
        _ item: PersistenceItemModel,
        runtimeIndex: PersistenceRuntimeIndex
    ) -> Bool {
        item.riskLevel != "expected"
            || item.recentlyModified
            || state.persistenceChangedItemIds.contains(item.id)
            || !runtimeIndex.matches(for: item).isEmpty
    }

    private func isChanged(_ item: PersistenceItemModel) -> Bool {
        item.recentlyModified || state.persistenceChangedItemIds.contains(item.id)
    }

    private func riskRank(_ item: PersistenceItemModel, runtimeIndex: PersistenceRuntimeIndex) -> Int {
        let activeBonus = runtimeIndex.matches(for: item).isEmpty ? 0 : 1
        switch item.riskLevel {
        case "danger": return 8 + activeBonus
        case "warning": return 6 + activeBonus
        case "review": return 4 + activeBonus
        default: return activeBonus
        }
    }

    private func searchCorpus(_ item: PersistenceItemModel) -> String {
        [
            item.kind,
            item.mechanism,
            item.owner,
            item.label ?? "",
            item.path,
            item.program ?? "",
            item.programName ?? "",
            item.riskReasons.joined(separator: " "),
            item.notes.joined(separator: " "),
        ]
        .joined(separator: " ")
        .lowercased()
    }

    private func groupedKinds(_ items: [PersistenceItemModel]) -> [String] {
        let order = [
            "user-launch-agent", "login-item", "cron", "launch-agent", "launch-daemon",
            "system-launch-agent", "system-launch-daemon",
        ]
        let present = Set(items.map(\.kind))
        return order.filter { present.contains($0) }
    }

    private func section(
        kind: String,
        items: [PersistenceItemModel],
        runtimeIndex: PersistenceRuntimeIndex
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack {
                Text(items.first?.kindLabel ?? kind)
                    .font(.headline)
                Text("\(items.count)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                Spacer()
            }
            LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(items) { item in
                    itemRow(item, runtimeIndex: runtimeIndex)
                }
            }
        }
    }

    private func itemRow(
        _ item: PersistenceItemModel,
        runtimeIndex: PersistenceRuntimeIndex
    ) -> some View {
        let matches = runtimeIndex.matches(for: item)
        let expanded = expandedItemID == item.id
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Button {
                withAnimation(AetowerDesign.Motion.standard) {
                    expandedItemID = expanded ? nil : item.id
                }
            } label: {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(spacing: 8) {
                            riskChip(item)
                            if !matches.isEmpty {
                                activeChip(matches)
                            }
                            if isChanged(item) {
                                changedChip
                            }
                        }
                        Text(item.displayTitle)
                            .font(.system(size: 13, weight: .semibold))
                            .lineLimit(1)
                        Text(item.program ?? item.path)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)

                    rowColumn("Source", item.mechanism)
                    rowColumn("Owner", item.owner)
                    rowColumn("Runtime", runtimeSummary(matches))
                    rowColumn("Target", targetStatus(item))

                    Image(systemName: expanded ? "chevron.up" : "chevron.down")
                        .foregroundStyle(.secondary)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if expanded {
                expandedCards(item, matches: matches)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(riskTone(item).opacity(0.65))
                .frame(width: 3)
        }
    }

    private func rowColumn(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .frame(width: 118, alignment: .leading)
    }

    private func expandedCards(
        _ item: PersistenceItemModel,
        matches: [PersistenceRuntimeMatch]
    ) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 245), spacing: AetowerDesign.Spacing.sm)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.sm
        ) {
            detailCard("Identity", systemImage: "person.text.rectangle") {
                detailLine("Name", item.displayTitle)
                detailLine("Mechanism", item.mechanism)
                detailLine("Owner", item.owner)
                if let runAtLoad = item.runAtLoad {
                    detailLine("Run at load", runAtLoad ? "yes" : "no")
                }
                if let disabled = item.disabled {
                    detailLine("Disabled", disabled ? "yes" : "no")
                }
            }
            detailCard("Persistence Source", systemImage: "doc.text.magnifyingglass") {
                detailLine("Path", item.path, monospaced: true)
                detailLine("Modified", dateLabel(item.sourceModifiedAtMillis))
                if let bytes = item.sourceSizeBytes {
                    detailLine("Size", formatBytes(bytes))
                }
                Button("Reveal source") { reveal(path: item.path) }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            }
            detailCard("Runtime Impact", systemImage: "waveform.path.ecg") {
                if matches.isEmpty {
                    Text("Not linked to a live process right now.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(matches.prefix(3)) { match in
                        detailLine(
                            "PID \(match.pid)",
                            "\(String(format: "%.1f%%", match.cpuPercent)) CPU · \(formatBytes(match.memoryBytes)) · \(formatWakeups(match.wakeupsPerSecond))"
                        )
                    }
                    if let first = matches.first {
                        Button("Inspect live process") {
                            state.runProcessInspection(pid: first.pid)
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                    }
                }
            }
            detailCard("Signature", systemImage: "signature") {
                if let signature = item.signature {
                    detailLine("Class", signature.classificationLabel)
                    detailLine("Team", signature.teamId ?? "unknown")
                    detailLine("Signing ID", signature.signingId ?? "unknown")
                    if let note = signature.note {
                        Text(note)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                } else if item.kind == "cron" {
                    Text("Cron entries are shell commands, not a single signable executable.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Text("Signature is intentionally skipped in fast scan.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Button("Run deep audit") {
                        state.runPersistenceScan(deep: true)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }
            }
            detailCard("Children", systemImage: "point.3.connected.trianglepath.dotted") {
                if let pid = matches.first?.pid, let inspection = state.processInspections[pid] {
                    detailLine("Parent", inspection.parentSummary ?? inspection.parentPid.map { "PID \($0)" } ?? "unknown")
                    detailLine("Children", "\(inspection.childPids.count)")
                    detailLine("User", inspection.user ?? "unknown")
                } else if let pid = matches.first?.pid {
                    Text("Load process details to see parent and children.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Button("Load details") {
                        state.runProcessInspection(pid: pid)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                } else {
                    Text("No active process tree is available for inactive startup items.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            detailCard("Actions", systemImage: "bolt.horizontal") {
                Button("Reveal target") {
                    if let program = item.program {
                        reveal(path: program)
                    } else {
                        reveal(path: item.path)
                    }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                Button("Open source file") {
                    open(path: item.path)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                Button("Copy diagnostic") {
                    copyDiagnostic(item, matches: matches)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                if let entityID = matches.first?.entityId {
                    Button("Focus in Monitor") {
                        state.focusEntityInMonitor(entityID)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }
            }
        }
        .padding(.top, AetowerDesign.Spacing.xs)
    }

    private func detailCard<Content: View>(
        _ title: String,
        systemImage: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: 6) {
                Image(systemName: systemImage)
                    .foregroundStyle(.secondary)
                Text(title)
                    .font(.caption.weight(.semibold))
                Spacer()
            }
            content()
            Spacer(minLength: 0)
        }
        .padding(AetowerDesign.Spacing.sm)
        .frame(maxWidth: .infinity, minHeight: 145, alignment: .topLeading)
        .background(AetowerDesign.Surface.badge)
    }

    private func detailLine(_ title: String, _ value: String, monospaced: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(title.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(monospaced ? .system(size: 11, design: .monospaced) : .caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
    }

    private func riskChip(_ item: PersistenceItemModel) -> some View {
        Text(item.riskLabel)
            .font(.caption2.weight(.bold))
            .foregroundStyle(riskTone(item))
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(riskTone(item).opacity(0.14), in: Capsule())
    }

    private func activeChip(_ matches: [PersistenceRuntimeMatch]) -> some View {
        Text("\(matches.count) active")
            .font(.caption2.weight(.bold))
            .foregroundStyle(AetowerDesign.Tone.cpu)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(AetowerDesign.Tone.cpu.opacity(0.14), in: Capsule())
    }

    private var changedChip: some View {
        Text("Changed")
            .font(.caption2.weight(.bold))
            .foregroundStyle(AetowerDesign.Tone.network)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(AetowerDesign.Tone.network.opacity(0.14), in: Capsule())
    }

    private func riskTone(_ item: PersistenceItemModel) -> Color {
        switch item.riskLevel {
        case "danger": return AetowerDesign.Status.error
        case "warning": return AetowerDesign.Status.warning
        case "review": return AetowerDesign.Tone.network
        default: return AetowerDesign.Status.ready
        }
    }

    private func runtimeSummary(_ matches: [PersistenceRuntimeMatch]) -> String {
        guard !matches.isEmpty else { return "inactive" }
        let cpu = matches.reduce(Float(0)) { $0 + $1.cpuPercent }
        let memory = matches.reduce(UInt64(0)) { $0 + $1.memoryBytes }
        return "\(String(format: "%.1f%%", cpu)) · \(formatBytes(memory))"
    }

    private func targetStatus(_ item: PersistenceItemModel) -> String {
        if item.kind == "cron" { return "shell line" }
        if item.programExists == false { return "missing" }
        if item.signature != nil { return item.signature?.classificationLabel ?? "signed" }
        return "not checked"
    }

    private func warningBanner(_ text: String) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "exclamationmark.triangle")
            Text(text)
                .font(.caption)
            Spacer()
        }
        .foregroundStyle(AetowerDesign.Status.warning)
        .padding(AetowerDesign.Spacing.sm)
        .background(AetowerDesign.Status.warning.opacity(0.12))
    }

    private var errorText: some View {
        Group {
            if let error = state.persistenceScanError {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.warning)
            }
        }
    }

    private func degradedSection(_ degraded: [DegradedSourceModel]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text("Unavailable sources")
                .font(.headline)
            ForEach(degraded) { source in
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "lock.slash")
                        .foregroundStyle(.secondary)
                    Text(source.reason)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card)
    }

    private func scannedLocations(_ report: PersistenceScanReportModel) -> some View {
        DisclosureGroup("Scanned locations") {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(report.scannedLocations, id: \.self) { location in
                    Text(location)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
            .padding(.top, AetowerDesign.Spacing.xs)
        }
        .font(.caption)
        .foregroundStyle(.secondary)
    }

    private func dateLabel(_ millis: UInt64?) -> String {
        guard let millis else { return "unknown" }
        let date = Date(timeIntervalSince1970: Double(millis) / 1000.0)
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    private func reveal(path: String) {
        let url = URL(fileURLWithPath: path)
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    private func open(path: String) {
        NSWorkspace.shared.open(URL(fileURLWithPath: path))
    }

    private func copyDiagnostic(_ item: PersistenceItemModel, matches: [PersistenceRuntimeMatch]) {
        let diagnostic = [
            "Aetower startup/persistence diagnostic",
            "Title: \(item.displayTitle)",
            "Kind: \(item.kind)",
            "Mechanism: \(item.mechanism)",
            "Owner: \(item.owner)",
            "Risk: \(item.riskLabel)",
            "Source: \(item.path)",
            "Program: \(item.program ?? "unknown")",
            "Program exists: \(item.programExists.map(String.init) ?? "unknown")",
            "Signature: \(item.signature?.classificationLabel ?? "not checked")",
            "Active PIDs: \(matches.map { String($0.pid) }.joined(separator: ", "))",
            "Reasons: \(item.riskReasons.joined(separator: " | "))",
        ].joined(separator: "\n")
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(diagnostic, forType: .string)
    }
}

private enum PersistenceFilter: String, CaseIterable, Identifiable {
    case attention
    case active
    case changed
    case unsigned
    case user
    case system
    case all

    var id: String { rawValue }

    var label: String {
        switch self {
        case .attention: return "Attention"
        case .active: return "Active"
        case .changed: return "Changed"
        case .unsigned: return "Signing"
        case .user: return "User"
        case .system: return "System"
        case .all: return "All"
        }
    }
}

private struct PersistenceRuntimeMatch: Identifiable {
    let entityId: String
    let entityName: String
    let pid: UInt32
    let cpuPercent: Float
    let memoryBytes: UInt64
    let wakeupsPerSecond: Float

    var id: String { "\(entityId):\(pid)" }
}

private struct PersistenceRuntimeIndex {
    private let byExecutable: [String: [PersistenceRuntimeMatch]]
    private let byProcessName: [String: [PersistenceRuntimeMatch]]

    init(snapshot: SystemSnapshot) {
        var executable: [String: [PersistenceRuntimeMatch]] = [:]
        var processName: [String: [PersistenceRuntimeMatch]] = [:]
        for entity in snapshot.entities {
            for component in entity.components {
                guard let pid = component.processId else { continue }
                let match = PersistenceRuntimeMatch(
                    entityId: entity.entityId,
                    entityName: entity.displayName,
                    pid: pid,
                    cpuPercent: component.cpuPercent,
                    memoryBytes: component.memoryBytes,
                    wakeupsPerSecond: entity.metrics.wakeupsPerSecond
                )
                if let path = component.executablePath, !path.isEmpty {
                    executable[path, default: []].append(match)
                    if let name = URL(fileURLWithPath: path).lastPathComponent.nilIfEmpty {
                        processName[name.lowercased(), default: []].append(match)
                    }
                }
                processName[component.title.lowercased(), default: []].append(match)
            }
        }
        self.byExecutable = executable
        self.byProcessName = processName
    }

    func matches(for item: PersistenceItemModel) -> [PersistenceRuntimeMatch] {
        if let program = item.program, let exact = byExecutable[program] {
            return exact
        }
        if let name = item.programName?.lowercased(), let named = byProcessName[name] {
            return named
        }
        if let label = item.label?.lowercased(), let named = byProcessName[label] {
            return named
        }
        return []
    }
}

private extension String {
    var nilIfEmpty: String? {
        isEmpty ? nil : self
    }
}
