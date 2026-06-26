import AppKit
import SwiftUI

public struct StorageView: View {
    let state: AppState
    @State private var selectedFilter: StorageFilter = .attention
    @State private var searchText = ""
    @State private var customRoot = ""
    @State private var maxDepth = 5.0

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                header
                controls
                safetyBanner

                if let error = state.storageHygieneError {
                    warningBanner(error)
                }

                if let report = state.storageHygieneReport {
                    summaryGrid(report)
                    if report.truncated {
                        warningBanner("The scan hit a cap or time budget. Results are partial; narrow the root or refresh when the machine is idle.")
                    }
                    itemSection(report)
                    rootsSection(report)
                    caveatsSection(report)
                } else if state.storageHygieneIsLoading {
                    loadingSection
                } else {
                    emptySection
                }
            }
            .frame(maxWidth: 1120, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
        .task {
            state.ensureStorageHygieneScan()
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.lg) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Storage")
                    .font(.system(size: 28, weight: .semibold, design: .rounded))
                Text("Find local build artifacts, logs, caches, and dependency trees that make development machines drift over time.")
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer()
            if state.storageHygieneIsLoading {
                ProgressView()
                    .controlSize(.small)
            }
        }
    }

    private var controls: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                TextField("Optional root, for example ~/Repositories", text: $customRoot)
                    .aetowerUtilityTextInput()
                    .textFieldStyle(.roundedBorder)
                Stepper(
                    "Depth \(Int(maxDepth))",
                    value: $maxDepth,
                    in: 1...12,
                    step: 1
                )
                .frame(width: 130)
                Button {
                    runScan()
                } label: {
                    Label("Scan", systemImage: "arrow.clockwise")
                }
                .buttonStyle(.borderedProminent)
                .disabled(state.storageHygieneIsLoading)
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Picker("Filter", selection: $selectedFilter) {
                    ForEach(StorageFilter.allCases) { filter in
                        Text(filter.label).tag(filter)
                    }
                }
                .pickerStyle(.segmented)
                .frame(maxWidth: 520)

                TextField("Search paths or kinds", text: $searchText)
                    .aetowerUtilityTextInput()
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 260)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private var safetyBanner: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "shield.checkered")
                .foregroundStyle(AetowerDesign.Status.ready)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Read-only inventory")
                    .font(.headline)
                Text("Aetower does not delete files from this view. It estimates size, age, and cleanup confidence, then gives reveal/copy actions so operators stay in control.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Status.ready.opacity(0.10))
    }

    private func summaryGrid(_ report: StorageHygieneReportModel) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 170), spacing: AetowerDesign.Spacing.md)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.md
        ) {
            summaryCard(
                "Reclaimable",
                value: formatBytes(report.summary.totalReclaimableBytes),
                detail: "bounded estimate",
                systemImage: "externaldrive.badge.minus",
                tone: AetowerDesign.Tone.disk
            )
            summaryCard(
                "Candidates",
                value: "\(report.summary.itemCount)",
                detail: "\(report.summary.safeCandidateCount) expected artifacts",
                systemImage: "shippingbox",
                tone: AetowerDesign.Tone.cpu
            )
            summaryCard(
                "Review",
                value: "\(report.summary.reviewCandidateCount)",
                detail: "needs operator decision",
                systemImage: "eye",
                tone: report.summary.reviewCandidateCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
            summaryCard(
                "Stale",
                value: "\(report.summary.staleCandidateCount)",
                detail: "older than 7 days",
                systemImage: "calendar.badge.clock",
                tone: report.summary.staleCandidateCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
            summaryCard(
                "Scanned",
                value: "\(report.summary.scannedDirectoryCount)",
                detail: "\(report.scanDurationMillis) ms",
                systemImage: "folder.badge.gearshape",
                tone: AetowerDesign.Tone.network
            )
            summaryCard(
                "Largest",
                value: formatBytes(report.summary.largestItemBytes),
                detail: report.summary.largestItemPath.map(lastPathComponent) ?? "none",
                systemImage: "arrow.up.left.and.arrow.down.right",
                tone: AetowerDesign.Tone.energy
            )
        }
    }

    private func itemSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Artifacts")
                        .font(.headline)
                    Text("\(filteredItems(from: report).count) visible of \(report.items.count) reported candidate(s).")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            if filteredItems(from: report).isEmpty {
                ContentUnavailableView(
                    "No matching artifacts",
                    systemImage: "line.3.horizontal.decrease.circle",
                    description: Text("Change the filter, search text, root, or depth and scan again.")
                )
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(filteredItems(from: report)) { item in
                        artifactRow(item)
                    }
                }
            }
        }
    }

    private func artifactRow(_ item: StorageHygieneItemModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: icon(for: item))
                    .foregroundStyle(tone(for: item))
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(item.displayName)
                            .font(.subheadline.weight(.semibold))
                        safetyBadge(item)
                        if item.stale {
                            Text("Stale")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Status.warning)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Status.warning.opacity(0.12), in: Capsule())
                        }
                        if item.sizeTruncated {
                            Text("Partial size")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Surface.badge, in: Capsule())
                        }
                    }

                    Text(item.path)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Text(item.reason)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(item.recommendation)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(item.sizeBytes))
                        .font(.system(size: 18, weight: .semibold, design: .rounded))
                    Text(ageLabel(item))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Reveal") { reveal(path: item.path) }
                Button("Copy path") { copy(item.path) }
                Button("Copy command") { copy(item.commandHint) }
                Spacer()
                Text(item.kind)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func rootsSection(_ report: StorageHygieneReportModel) -> some View {
        DisclosureGroup("Scanned and skipped roots") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(report.roots, id: \.self) { root in
                    rootLine(root, detail: "scanned", systemImage: "checkmark.circle")
                }
                ForEach(report.skippedRoots) { root in
                    rootLine(root.path, detail: root.reason, systemImage: "exclamationmark.triangle")
                }
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        }
        .font(.caption)
    }

    private func caveatsSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text("Caveats")
                .font(.headline)
            ForEach(report.caveats, id: \.self) { caveat in
                Label(caveat, systemImage: "info.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private var loadingSection: some View {
        VStack(spacing: AetowerDesign.Spacing.md) {
            ProgressView()
            Text("Scanning bounded developer storage roots...")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 220)
    }

    private var emptySection: some View {
        ContentUnavailableView(
            "No storage report yet",
            systemImage: "externaldrive",
            description: Text("Run a read-only scan to inventory local development artifacts.")
        )
        .frame(maxWidth: .infinity, minHeight: 220)
    }

    private func summaryCard(
        _ title: String,
        value: String,
        detail: String,
        systemImage: String,
        tone: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: systemImage)
                    .foregroundStyle(tone)
                Text(title.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Text(value)
                .font(.system(size: 22, weight: .semibold, design: .rounded))
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(detail)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(2)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 112, alignment: .topLeading)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func safetyBadge(_ item: StorageHygieneItemModel) -> some View {
        Text(item.safety == "safe" ? "Expected artifact" : "Review")
            .font(.caption2.weight(.semibold))
            .foregroundStyle(tone(for: item))
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(tone(for: item).opacity(0.12), in: Capsule())
    }

    private func rootLine(_ path: String, detail: String, systemImage: String) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: systemImage)
                .foregroundStyle(detail == "scanned" ? AetowerDesign.Status.ready : AetowerDesign.Status.warning)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text(path)
                    .font(.system(size: 11, design: .monospaced))
                    .textSelection(.enabled)
                Text(detail)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func warningBanner(_ text: String) -> some View {
        Label(text, systemImage: "exclamationmark.triangle")
            .font(.caption)
            .foregroundStyle(AetowerDesign.Status.warning)
            .padding(AetowerDesign.Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(AetowerDesign.Status.warning.opacity(0.12))
    }

    private func filteredItems(from report: StorageHygieneReportModel) -> [StorageHygieneItemModel] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return report.items.filter { item in
            selectedFilter.matches(item)
                && (
                    query.isEmpty
                        || item.path.lowercased().contains(query)
                        || item.kind.lowercased().contains(query)
                        || item.reason.lowercased().contains(query)
                )
        }
    }

    private func runScan() {
        let root = customRoot.trimmingCharacters(in: .whitespacesAndNewlines)
        state.runStorageHygieneScan(
            roots: root.isEmpty ? [] : [root],
            maxDepth: UInt32(maxDepth),
            limit: 120
        )
    }

    private func tone(for item: StorageHygieneItemModel) -> Color {
        item.safety == "safe" ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
    }

    private func icon(for item: StorageHygieneItemModel) -> String {
        switch item.kind {
        case "log-file", "logs": return "doc.text"
        case "node-dependencies", "python-environment": return "shippingbox"
        case let kind where kind.contains("cache"): return "tray"
        default: return "folder"
        }
    }

    private func ageLabel(_ item: StorageHygieneItemModel) -> String {
        if let ageDays = item.ageDays {
            return ageDays == 0 ? "modified today" : "\(ageDays)d old"
        }
        if let modifiedMillis = item.modifiedMillis {
            let date = Date(timeIntervalSince1970: Double(modifiedMillis) / 1000.0)
            return date.formatted(date: .abbreviated, time: .omitted)
        }
        return "age unknown"
    }

    private func formatBytes(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
    }

    private func lastPathComponent(_ path: String) -> String {
        URL(fileURLWithPath: path).lastPathComponent
    }

    private func reveal(path: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
    }

    private func copy(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }
}

private enum StorageFilter: String, CaseIterable, Identifiable {
    case attention
    case safe
    case review
    case stale
    case all

    var id: String { rawValue }

    var label: String {
        switch self {
        case .attention: return "Attention"
        case .safe: return "Expected"
        case .review: return "Review"
        case .stale: return "Stale"
        case .all: return "All"
        }
    }

    func matches(_ item: StorageHygieneItemModel) -> Bool {
        switch self {
        case .attention:
            item.stale || item.safety != "safe" || item.sizeTruncated || item.sizeBytes >= 100 * 1024 * 1024
        case .safe:
            item.safety == "safe"
        case .review:
            item.safety != "safe"
        case .stale:
            item.stale
        case .all:
            true
        }
    }
}
