import AppKit
import SwiftUI

private struct StorageGrowthTimelineEvent: Identifiable {
    let id: String
    let timestampMillis: UInt64?
    let repoName: String?
    let repoRoot: String?
    let branch: String?
    let displayName: String
    let path: String
    let cleanupTier: String
    let deltaBytes: Int64
    let previousBytes: UInt64
    let currentBytes: UInt64
    let command: String?
    let processTree: String?
    let aiAgentSession: String?
    let writerSource: String?
    let writerDisplay: String?
    let matchedWriterCount: UInt64
    let matchedFilesystemEventCount: UInt64
    let attributionSources: [String]
    let confidence: String
    let confidenceScore: UInt8
    let ambiguous: Bool
    let attributionSummary: String
    let attributionEvidence: [String]
}

private struct StorageTopOffender: Identifiable {
    let id: String
    let title: String
    let value: String
    let detail: String
    let systemImage: String
    let tone: Color
}

private struct StorageHomeAction: Identifiable {
    let id: String
    let title: String
    let detail: String
    let consequence: String
    let systemImage: String
    let tone: Color
    let bytes: UInt64
    let itemCount: Int
    let confidence: UInt8
    let sampleItems: [StorageHygieneItemModel]
    let stageItems: [StorageHygieneItemModel]
    let growthEvents: [StorageGrowthTimelineEvent]
    let cleanupBundle: StorageCleanupBundleModel?

    var hasStageableItems: Bool {
        cleanupBundle != nil || !stageItems.isEmpty
    }
}

private struct StorageReclaimFolderRow: Identifiable {
    let id: String
    let path: String
    let displayName: String
    let kind: String
    let cleanupTier: String
    let safety: String
    let sizeBytes: UInt64
    let itemCount: Int
    let cleanupAllowed: Bool
    let cleanupBlockers: [String]
    let defaultCleanupAction: String
    let sizeTruncated: Bool
    let cloudPlaceholder: Bool
    let hasHardlinks: Bool
    let source: String
    let stageItems: [StorageHygieneItemModel]
}

private struct StorageClassificationExplanation: Identifiable {
    let id: String
    let title: String
    let path: String
    let classification: String
    let consequence: String
    let evidence: [String]
    let blockers: [String]
}

private struct StorageRecommendationDecision: Identifiable {
    let id: String
    let title: String
    let value: String
    let detail: String
    let systemImage: String
    let tone: Color
}

private struct StorageCleanupExecutionRequest: Identifiable, Sendable {
    let id = UUID()
    let title: String
    let subtitle: String
    let targetPaths: [String]
    let estimatedBytes: UInt64
    let requiresReview: Bool
    let prerequisites: [String]

    var targetPath: String? { targetPaths.first }
}

private struct StorageCleanupBasketItem: Identifiable, Sendable {
    let id: String
    let title: String
    let path: String
    let source: String
    let cleanupTier: String
    let safety: String
    let estimatedBytes: UInt64
    let reason: String
    let consequence: String
    let evidence: [String]
    let requiresReview: Bool
    let blockers: [String]
    let prerequisites: [String]
}

private struct StorageCleanupExecutionResult: Sendable {
    let exitCode: Int32
    let output: String
    let durationSeconds: Double
    /// Paths actually moved to Trash, and paths that failed with the reason.
    /// Batch outcomes are per-path: one root-owned folder failing must not
    /// report the other 41 successful moves as failures.
    let movedPaths: [String]
    let movedTrashURLs: [String: URL]
    let failedPaths: [String: String]

    var succeeded: Bool { exitCode == 0 }
    var partiallySucceeded: Bool { !movedPaths.isEmpty && !failedPaths.isEmpty }
}

private struct StorageCleanupAuditEvent: Codable, Identifiable, Sendable {
    let id: String
    let timestampMillis: UInt64
    let action: String
    let path: String
    let detail: String
    let bytes: UInt64
    let cleanupTier: String?
    let safety: String?
    let blockers: [String]?
    let succeeded: Bool?
}

private enum StorageCleanupAuditLog {
    private static let fileName = "storage-cleanup-audit.ndjson"

    static func append(_ event: StorageCleanupAuditEvent) -> Bool {
        guard let url = auditURL(createDirectory: true),
            let data = try? JSONEncoder().encode(event)
        else { return false }
        var line = data
        line.append(0x0A)
        if FileManager.default.fileExists(atPath: url.path),
            let handle = try? FileHandle(forWritingTo: url)
        {
            do {
                try handle.seekToEnd()
                try handle.write(contentsOf: line)
                try handle.close()
                return true
            } catch {
                try? handle.close()
                return false
            }
        } else {
            do {
                try line.write(to: url, options: [.atomic])
                return true
            } catch {
                return false
            }
        }
    }

    static func loadRecent(limit: Int = 40) -> [StorageCleanupAuditEvent] {
        guard let url = auditURL(createDirectory: false),
            let text = try? String(contentsOf: url, encoding: .utf8)
        else { return [] }
        let decoder = JSONDecoder()
        return text
            .split(separator: "\n")
            .suffix(limit)
            .reversed()
            .compactMap { line in
                try? decoder.decode(StorageCleanupAuditEvent.self, from: Data(line.utf8))
            }
    }

    private static func auditURL(createDirectory: Bool) -> URL? {
        storageSupportFileURL(fileName: fileName, createDirectory: createDirectory)
    }
}

private struct StorageTrackedTrashItem: Codable, Sendable {
    let originalPath: String
    let trashPath: String
    let bytes: UInt64
    let timestampMillis: UInt64
}

private enum StorageTrackedTrashStore {
    private static let fileName = "storage-tracked-trash-v1.json"

    static func loadItems(pruneMissing: Bool = true) -> [String: StorageTrackedTrashItem] {
        guard let url = storeURL(createDirectory: false),
            let data = try? Data(contentsOf: url),
            let decoded = try? JSONDecoder().decode([StorageTrackedTrashItem].self, from: data)
        else { return [:] }
        let items = Dictionary(uniqueKeysWithValues: decoded.map { ($0.originalPath, $0) })
        guard pruneMissing else { return items }
        let retained = items.filter { FileManager.default.fileExists(atPath: $0.value.trashPath) }
        if retained.count != items.count {
            saveItems(retained)
        }
        return retained
    }

    static func loadURLsByOriginalPath() -> [String: URL] {
        loadItems().mapValues { URL(fileURLWithPath: $0.trashPath) }
    }

    static func loadPendingBytes() -> UInt64 {
        loadItems().values.reduce(UInt64(0)) { total, item in
            let (sum, overflow) = total.addingReportingOverflow(item.bytes)
            return overflow ? UInt64.max : sum
        }
    }

    static func upsert(originalPath: String, trashURL: URL, bytes: UInt64) {
        var items = loadItems(pruneMissing: false)
        items[originalPath] = StorageTrackedTrashItem(
            originalPath: originalPath,
            trashPath: trashURL.path,
            bytes: bytes,
            timestampMillis: UInt64(Date().timeIntervalSince1970 * 1000)
        )
        saveItems(items)
    }

    static func remove(originalPath: String) {
        var items = loadItems(pruneMissing: false)
        items.removeValue(forKey: originalPath)
        saveItems(items)
    }

    static func clear() {
        saveItems([:])
    }

    static func reconcileExisting() -> (urls: [String: URL], pendingBytes: UInt64) {
        let items = loadItems(pruneMissing: true)
        let urls = items.mapValues { URL(fileURLWithPath: $0.trashPath) }
        let pendingBytes = items.values.reduce(UInt64(0)) { total, item in
            let (sum, overflow) = total.addingReportingOverflow(item.bytes)
            return overflow ? UInt64.max : sum
        }
        return (urls, pendingBytes)
    }

    private static func saveItems(_ items: [String: StorageTrackedTrashItem]) {
        guard let url = storeURL(createDirectory: true) else { return }
        let payload = Array(items.values).sorted { $0.originalPath < $1.originalPath }
        guard let data = try? JSONEncoder().encode(payload) else { return }
        try? data.write(to: url, options: [.atomic])
    }

    private static func storeURL(createDirectory: Bool) -> URL? {
        storageSupportFileURL(fileName: fileName, createDirectory: createDirectory)
    }
}

/// Pending undo for a one-click direct-to-Trash action. `trashURL` is where
/// Finder placed the item; nil means the trash attempt failed and the toast
/// is informational only.
struct StorageDirectTrashUndo {
    let message: String
    let originalPath: String
    let trashURL: URL?
    let bytes: UInt64
    let succeeded: Bool
}

public struct StorageView: View {
    let state: AppState
    @State private var selectedFilter: StorageFilter = .attention
    @State private var artifactScope: StorageArtifactScope = .all
    @State private var artifactSort: StorageArtifactSort = .largest
    @State private var searchText = ""
    @State private var customRoot = ""
    @State private var maxDepth = 5.0
    @State private var scanMode: StorageScanModeSelection = .fast
    @State private var copiedCleanupBundleID: String?
    @State private var copiedCleanupRecipeID: String?
    @State private var candidateCommandPreviewBundle: StorageCleanupBundleModel?
    @State private var selectedSection: StorageSection = .reclaim
    @State private var showCustomScanSettings = false
    @State private var showCleanupRecipes = false
    @State private var showRawArtifacts = false
    @State private var showScannedRoots = false
    @State private var showCaveats = false
    @State private var pendingCleanupExecutionRequest: StorageCleanupExecutionRequest?
    @State private var cleanupExecutionResult: StorageCleanupExecutionResult?
    @State private var cleanupExecutionIsRunning = false
    @State private var cleanupBasket: [StorageCleanupBasketItem] = []
    @State private var showCleanupBasket = false
    @State private var directTrashUndo: StorageDirectTrashUndo?
    @State private var directTrashUndoDismissTask: Task<Void, Never>?
    @State private var directTrashInFlightPaths: Set<String> = []
    @State private var trashedItemURLsByOriginalPath = StorageTrackedTrashStore
        .loadURLsByOriginalPath()
    /// Bytes this session has moved into Finder Trash but not yet freed —
    /// reclaim is a two-step operation (Trash, then empty) and the second
    /// step must stay visible until it happens.
    @State private var trashPendingBytes = StorageTrackedTrashStore.loadPendingBytes()
    @State private var confirmEmptyTrash = false
    @State private var emptyTrashInFlight = false
    @State private var cleanupAuditEvents = StorageCleanupAuditLog.loadRecent()
    @State private var classificationExplanation: StorageClassificationExplanation?
    @State private var storageVisualExplorerMode: StorageVisualExplorerMode = .table
    @State private var showStorageTreemap = false
    @State private var selectedTreemapNodeID: String?
    @State private var storageExplorerPage = 0
    @State private var selectedExplorePane: StorageExplorePane = .browse
    @State private var reclaimListMode: StorageReclaimListMode = .files
    @State private var selectedReclaimFilePath: String?
    @State private var selectedReclaimFolderPath: String?
    @State private var coldDataSort: StorageColdDataSort = .recommended
    @State private var selectedSimilarityFilter: StorageSimilarityFilter = .exactDuplicates
    @State private var reviewedSimilarityGroupIDs: Set<String> = []
    @State private var ignoredSimilarityGroupIDs: Set<String> = []
    @State private var showIgnoredSimilarityGroups = false
    @State private var duplicateCanonicalPathByGroupID: [String: String] = [:]
    @State private var expandedSimilarityGroupKeys: Set<String> = []
    @State private var similarityTelemetryViewedGroupKeys: Set<String> = []
    @State private var similarityTelemetryViewedSurfaceKeys: Set<String> = []

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        VStack(spacing: AetowerDesign.Spacing.none) {
            storageTabToolBand
            Divider()
            HStack(spacing: AetowerDesign.Spacing.none) {
                storageNavigationRail(report: state.storageHygieneReport)
                Divider()
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                        if let error = state.storageHygieneError {
                            warningBanner(error)
                        }

                        if let report = state.storageHygieneReport {
                            storageSectionContent(report)
                        } else if state.storageHygieneIsLoading {
                            loadingSection
                        } else {
                            emptySection
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(AetowerDesign.Spacing.xxl)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .confirmationDialog(
            "Delete Aetower-tracked Trash items?",
            isPresented: $confirmEmptyTrash,
            titleVisibility: .visible
        ) {
            Button("Delete Aetower-tracked items", role: .destructive) { emptyTrash() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This permanently deletes only items Aetower moved to the Trash and still tracks. Unrelated Finder Trash contents are left alone.")
        }
        .overlay(alignment: .bottom) {
            // Floating stack: transient undo toast above the persistent
            // cleanup pill. Both hover over content rather than reserving a
            // full-width bar, so they never push the layout around.
            VStack(spacing: AetowerDesign.Spacing.sm) {
                if let undo = directTrashUndo {
                    directTrashUndoToast(undo)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                if !cleanupBasket.isEmpty || trashPendingBytes > 0 {
                    cleanupActionPill
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .padding(.bottom, AetowerDesign.Spacing.xl)
            .animation(AetowerDesign.Motion.smooth, value: cleanupBasket.count)
            .animation(AetowerDesign.Motion.smooth, value: trashPendingBytes)
            .animation(AetowerDesign.Motion.quick, value: directTrashUndo == nil)
        }
        .task {
            state.ensureStorageHygieneScan()
        }
        .sheet(item: $candidateCommandPreviewBundle) { bundle in
            cleanupCommandPreviewSheet(bundle)
        }
        .sheet(item: $pendingCleanupExecutionRequest) { request in
            cleanupExecutionSheet(request)
        }
        .sheet(isPresented: $showCleanupBasket) {
            cleanupBasketSheet
        }
        .sheet(isPresented: $showCustomScanSettings) {
            storageCustomScanSettingsSheet
        }
        .sheet(item: $classificationExplanation) { explanation in
            classificationExplanationSheet(explanation)
        }
    }

    private var storageTabToolBand: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search storage artifacts and paths",
            searchWidth: 300
        ) {
            EmptyView()
        } filterTools: {
            EmptyView()
        } badges: {
            AetowerToolBadgeGroup(storageHeaderBadges, visibleCount: 3)
        } actions: {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                storageScanActionGroup
                if !cleanupBasket.isEmpty {
                    Button {
                        showCleanupBasket = true
                    } label: {
                        Label("\(cleanupBasket.count) staged", systemImage: "tray.full")
                    }
                    .buttonStyle(.bordered)
                }
            }
        }
    }

    private var storageScanActionGroup: some View {
        HStack(spacing: AetowerDesign.Spacing.none) {
            // A load that blew its watchdog budget re-enables the button
            // so an explicit quick scan can supersede the stuck load.
            AetowerScanButton(
                isRunning: state.storageHygieneIsLoading
                    && !state.storageHygieneLoadExceededBudget
            ) {
                runQuickScan()
            }
            storageScanOptionsMenu
        }
        .fixedSize()
    }

    private var storageScanOptionsMenu: some View {
        Menu {
            Section("Attention list") {
                ForEach(StorageFilter.allCases) { filter in
                    Button {
                        selectedFilter = filter
                    } label: {
                        HStack {
                            Text(filter.label)
                            if selectedFilter == filter {
                                Spacer()
                                Image(systemName: "checkmark")
                            }
                        }
                    }
                }
            }

            Divider()

            Button {
                showCustomScanSettings = true
            } label: {
                Label("Custom scan", systemImage: "slider.horizontal.3")
            }
        } label: {
            Image(systemName: "chevron.down")
                .font(AetowerDesign.Typography.compactData(size: 9, weight: .bold))
                .frame(width: AetowerDesign.Size.controlHeight, height: AetowerDesign.Size.controlHeight)
        }
        .menuStyle(.borderlessButton)
        .buttonStyle(.borderedProminent)
        .controlSize(.small)
        .help("Choose the visible reclaim list or open custom scan settings")
    }

    private var storageHeaderBadges: [AetowerToolBadgeItem] {
        [
            AetowerToolBadgeItem(
                    "Reclaim",
                    value: storageReclaimableLabel,
                    systemImage: "externaldrive.badge.minus",
                    tone: AetowerDesign.Tone.disk
            ),
            AetowerToolBadgeItem(
                    "Items",
                    value: storageItemCountLabel,
                    systemImage: "shippingbox",
                    tone: AetowerDesign.Tone.memory
            ),
            AetowerToolBadgeItem(
                    "Estimate",
                    value: state.storageEstimateStatus.title,
                    systemImage: storageEstimateSystemImage,
                    tone: storageEstimateTone
            ),
        ]
    }

    private var storageEstimateSystemImage: String {
        switch state.storageEstimateStatus.confidence {
        case .verified: return "checkmark.seal"
        case .estimated: return "waveform.path.ecg"
        case .stale: return "eye"
        case .refreshing: return "arrow.triangle.2.circlepath"
        case .needsFullScan: return "exclamationmark.triangle"
        }
    }

    private var storageEstimateTone: Color {
        switch state.storageEstimateStatus.confidence {
        case .verified: return AetowerDesign.Status.ready
        case .estimated: return AetowerDesign.Tone.disk
        case .stale: return AetowerDesign.Status.warning
        case .refreshing: return AetowerDesign.Status.ready
        case .needsFullScan: return AetowerDesign.Status.error
        }
    }

    private var storageScanRootField: some View {
        TextField("Optional root, for example ~/Repositories", text: $customRoot)
            .aetowerUtilityTextInput()
            .textFieldStyle(.plain)
            .font(AetowerDesign.Typography.caption)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .frame(minWidth: 180, idealWidth: 280, maxWidth: .infinity)
            .aetowerControlChrome()
    }

    private var storageScanDepthStepper: some View {
        Stepper(
            "Depth \(Int(maxDepth))",
            value: $maxDepth,
            in: 1...12,
            step: 1
        )
        .font(AetowerDesign.Typography.caption)
        .fixedSize()
    }

    private var storageScanModePicker: some View {
        Picker("Mode", selection: $scanMode) {
            ForEach(StorageScanModeSelection.allCases) { mode in
                Text(mode.label).tag(mode)
            }
        }
        .pickerStyle(.segmented)
        .frame(width: 190)
    }

    @ViewBuilder
    private var storageScanLoadingStatus: some View {
        if state.storageHygieneIsLoading {
            Text(storageScanLoadingTitle)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
        } else if state.storageEstimateStatus.confidence != .verified {
            Text(state.storageEstimateStatus.detail)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

    private var storageCustomScanSettingsSheet: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                Label("Custom scan", systemImage: "slider.horizontal.3")
                    .font(AetowerDesign.Typography.sectionTitle)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                Spacer()
                Button {
                    showCustomScanSettings = false
                } label: {
                    Image(systemName: "xmark")
                        .font(AetowerDesign.Typography.caption.weight(.semibold))
                        .frame(width: AetowerDesign.Size.controlHeight, height: AetowerDesign.Size.controlHeight)
                }
                .buttonStyle(.plain)
                .help("Close")
            }

            AetowerSurface(level: .card, padding: AetowerDesign.Spacing.md, cornerRadius: AetowerDesign.Radius.lg) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    storageScanRootField
                    HStack(spacing: AetowerDesign.Spacing.md) {
                        storageScanDepthStepper
                        storageScanModePicker
                        AetowerBadge(
                            "\(scanMode.resultLimit) rows",
                            systemImage: "list.number",
                            tone: AetowerDesign.Tone.disk
                        )
                        Spacer(minLength: AetowerDesign.Spacing.none)
                    }
                }
            }

            AetowerSurface(level: .card, padding: AetowerDesign.Spacing.md, cornerRadius: AetowerDesign.Radius.lg) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    HStack(spacing: AetowerDesign.Spacing.md) {
                        Picker("Open list", selection: $selectedFilter) {
                            ForEach(StorageFilter.allCases) { filter in
                                Text(filter.label).tag(filter)
                            }
                        }
                        .pickerStyle(.menu)
                        .frame(width: 180)

                        Picker("Table scope", selection: $artifactScope) {
                            ForEach(StorageArtifactScope.allCases) { scope in
                                Text(scope.label).tag(scope)
                            }
                        }
                        .pickerStyle(.menu)
                        .frame(width: 180)

                        Picker("Sort", selection: $artifactSort) {
                            ForEach(StorageArtifactSort.allCases) { sort in
                                Text(sort.label).tag(sort)
                            }
                        }
                        .pickerStyle(.menu)
                        .frame(width: 180)
                    }

                    storageScanLoadingStatus
                }
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Quick defaults") {
                    customRoot = ""
                    maxDepth = 5
                    scanMode = .fast
                    selectedFilter = .attention
                }
                .buttonStyle(.bordered)

                Spacer()

                Button("Cancel") {
                    showCustomScanSettings = false
                }
                .buttonStyle(.bordered)

                Button {
                    runScan()
                    showCustomScanSettings = false
                } label: {
                    Label("Run custom scan", systemImage: "arrow.clockwise")
                }
                .buttonStyle(.borderedProminent)
                .disabled(state.storageHygieneIsLoading && !state.storageHygieneLoadExceededBudget)
            }
        }
        .padding(AetowerDesign.Spacing.xxl)
        .frame(width: 660)
    }

    private func storageNavigationRail(report: StorageHygieneReportModel?) -> some View {
        AetowerNavigationRail(width: 228) {
            AetowerRailGroup {
                ForEach(StorageSection.allCases) { section in
                    storageNavigationButton(section, report: report)
                }
            }
        }
    }

    private func storageNavigationButton(
        _ section: StorageSection,
        report: StorageHygieneReportModel?
    ) -> some View {
        AetowerRailButton(
            title: section.label,
            role: section.role,
            summary: section.summary,
            signal: storageSectionSignal(section, report: report),
            systemImage: section.systemImage,
            signalTone: storageSectionTone(section, report: report),
            isSelected: selectedSection == section
        ) {
            selectedSection = section
        }
    }

    private func storageSectionSignal(
        _ section: StorageSection,
        report: StorageHygieneReportModel?
    ) -> String? {
        guard let report else {
            if state.storageHygieneIsVerifyingCache { return "Verifying" }
            return state.storageHygieneIsLoading ? "Scanning" : "No scan"
        }
        switch section {
        case .reclaim:
            return formatBytes(report.summary.totalReclaimableBytes)
        case .explore:
            let similaritySummary = similarityReviewSummary(for: report)
            if similaritySummary.groupCount > 0 {
                return "\(similaritySummary.groupCount) groups - \(formatBytes(similaritySummary.reviewableBytes))"
            }
            return "\(report.summary.itemCount) item\(report.summary.itemCount == 1 ? "" : "s")"
        case .insights:
            let growthCount = report.growthDeltas.filter { $0.deltaBytes > 0 }.count
            return "\(growthCount) growth signal\(growthCount == 1 ? "" : "s")"
        }
    }

    private func storageSectionTone(
        _ section: StorageSection,
        report: StorageHygieneReportModel?
    ) -> Color {
        guard let report else {
            if state.storageHygieneIsVerifyingCache { return AetowerDesign.Tone.disk }
            return state.storageHygieneIsLoading ? AetowerDesign.Tone.disk : AetowerDesign.Status.neutral
        }
        switch section {
        case .reclaim:
            return report.summary.totalReclaimableBytes > 0 ? AetowerDesign.Tone.disk : AetowerDesign.Status.ready
        case .explore:
            if similarityReviewSummary(for: report).groupCount > 0 {
                return AetowerDesign.Status.warning
            }
            return AetowerDesign.Tone.memory
        case .insights:
            if !report.budgetGuardrails.violations.isEmpty
                || report.growthDeltas.contains(where: { $0.deltaBytes > 0 }) {
                return AetowerDesign.Status.warning
            }
            return AetowerDesign.Status.neutral
        }
    }

    private var storageReclaimableLabel: String {
        guard let report = state.storageHygieneReport else {
            return state.storageHygieneIsLoading ? "Loading" : "No scan"
        }
        return formatBytes(report.summary.totalReclaimableBytes)
    }

    private var storageItemCountLabel: String {
        guard let report = state.storageHygieneReport else {
            return "0"
        }
        return "\(report.summary.itemCount)"
    }

    private var storageScanStatusLabel: String {
        if state.storageHygieneIsVerifyingCache {
            return "Verifying"
        }
        if state.storageHygieneIsLoading {
            return "Running"
        }
        if state.storageHygieneError != nil {
            return "Error"
        }
        guard let report = state.storageHygieneReport else {
            return "Idle"
        }
        return "\(report.scanDurationMillis) ms"
    }

    @ViewBuilder
    private func storageSectionContent(_ report: StorageHygieneReportModel) -> some View {
        switch selectedSection {
        case .reclaim:
            storageReclaimHome(report)
        case .explore:
            storageExploreWorkspace(report)
        case .insights:
            storageInsightsWorkspace(report)
        }
    }

    /// Primary surface: disk pressure up top, then the staged-cleanup workflow.
    private func storageReclaimHome(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            storageReclaimSummaryBand(report)
            storageReclaimTableSection(report)
            storageReclaimSupportingData(report)
        }
    }

    private func storageReclaimSummaryBand(_ report: StorageHygieneReportModel) -> some View {
        let actions = storageHomeActions(from: report)
        let safeBytes = actions.first { $0.id == "safe-reclaim" }?.bytes ?? 0
        let developerBytes = actions.first { $0.id == "developer-artifacts" }?.bytes ?? 0
        let riskyBytes = actions.first { $0.id == "risky-review" }?.bytes ?? 0
        let safeAction = actions.first { $0.id == "safe-reclaim" }
        let developerAction = actions.first { $0.id == "developer-artifacts" }
        let directReclaimItems = directCleanItems(from: actions.flatMap(\.stageItems))
        let directSafeItems = safeAction.map { directCleanItems(from: $0.stageItems) } ?? []
        let directDeveloperItems = developerAction.map { directCleanItems(from: $0.stageItems) } ?? []
        let volume = primaryVolume(report)

        return AetowerSurface(level: .card, padding: AetowerDesign.Spacing.md, cornerRadius: 16) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                if let volume, volume.totalBytes > 0 {
                    let free = volume.availableBytes > 0 ? volume.availableBytes : volume.freeNowBytes
                    HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                        Label(volumeDisplayName(volume), systemImage: "internaldrive.fill")
                            .font(AetowerDesign.Typography.caption.weight(.semibold))
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                        Spacer()
                        Text("\(formatBytes(free)) free")
                            .font(AetowerDesign.Typography.caption.weight(.semibold))
                            .foregroundStyle(AetowerDesign.Tone.disk)
                        Text("of \(formatBytes(volume.totalBytes))")
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                    }
                    diskCapacityBar(
                        total: volume.totalBytes,
                        free: free,
                        reclaimable: min(report.summary.totalReclaimableBytes, volume.totalBytes),
                        tone: AetowerDesign.Tone.disk
                    )
                }

                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 168), spacing: AetowerDesign.Spacing.sm)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.sm
                ) {
                    storageReclaimMetric(
                        "Reclaimable",
                        value: formatBytes(report.summary.totalReclaimableBytes),
                        detail: "bounded estimate",
                        systemImage: "externaldrive.badge.minus",
                        tone: AetowerDesign.Tone.disk,
                        primaryActionKind: .clean,
                        reviewEnabled: report.summary.itemCount > 0,
                        primaryEnabled: !directReclaimItems.isEmpty,
                        reviewAction: {
                            focusReclaimTable(filter: .all, scope: .all, sort: .recommended)
                        },
                        primaryAction: {
                            trashStorageItemsDirectly(directReclaimItems, sourceTitle: "Reclaimable")
                        }
                    )
                    storageReclaimMetric(
                        "Safe",
                        value: formatBytes(safeBytes),
                        detail: "\(report.summary.safeCandidateCount) candidate\(report.summary.safeCandidateCount == 1 ? "" : "s")",
                        systemImage: "checkmark.shield",
                        tone: AetowerDesign.Status.ready,
                        primaryActionKind: .clean,
                        reviewEnabled: report.summary.safeCandidateCount > 0 || safeBytes > 0,
                        primaryEnabled: !directSafeItems.isEmpty,
                        reviewAction: {
                            focusReclaimTable(filter: .safe, scope: .all, sort: .recommended)
                        },
                        primaryAction: {
                            trashStorageItemsDirectly(directSafeItems, sourceTitle: "Safe")
                        }
                    )
                    storageReclaimMetric(
                        "Dev Artifacts",
                        value: formatBytes(developerBytes),
                        detail: "builds, caches, deps",
                        systemImage: "hammer",
                        tone: AetowerDesign.Tone.cpu,
                        primaryActionKind: .clean,
                        reviewEnabled: developerBytes > 0,
                        primaryEnabled: !directDeveloperItems.isEmpty,
                        reviewAction: {
                            focusReclaimTable(filter: .all, scope: .repoLinked, sort: .recommended)
                        },
                        primaryAction: {
                            trashStorageItemsDirectly(directDeveloperItems, sourceTitle: "Developer Artifacts")
                        }
                    )
                    storageReclaimMetric(
                        "Review",
                        value: formatBytes(riskyBytes),
                        detail: "\(report.summary.reviewCandidateCount) blocked/manual",
                        systemImage: "exclamationmark.triangle",
                        tone: AetowerDesign.Status.warning,
                        primaryActionKind: .clean,
                        reviewEnabled: report.summary.reviewCandidateCount > 0 || riskyBytes > 0,
                        primaryEnabled: false,
                        reviewAction: {
                            focusReclaimTable(filter: .risky, scope: .all, sort: .recommended)
                        },
                        primaryAction: {}
                    )
                    storageReclaimMetric(
                        "Tracked Trash",
                        value: formatBytes(trashPendingBytes),
                        detail: emptyTrashInFlight ? "emptying" : "pending delete",
                        systemImage: "trash",
                        tone: trashPendingBytes > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral,
                        primaryActionKind: .clean,
                        reviewEnabled: trashPendingBytes > 0,
                        primaryEnabled: trashPendingBytes > 0 && !emptyTrashInFlight,
                        reviewAction: {
                            openTrash()
                        },
                        primaryAction: {
                            emptyTrash()
                        }
                    )
                    storageReclaimMetric(
                        "Scan",
                        value: storageScanFreshnessLabel(report),
                        detail: "\(report.scanDurationMillis) ms",
                        systemImage: "clock.arrow.circlepath",
                        tone: AetowerDesign.Status.neutral,
                        primaryActionKind: .scan,
                        reviewEnabled: true,
                        primaryEnabled: !state.storageHygieneIsLoading,
                        reviewAction: {
                            selectedSection = .insights
                        },
                        primaryAction: {
                            runScan()
                        }
                    )
                }
            }
        }
    }

    private func storageReclaimMetric(
        _ title: String,
        value: String,
        detail: String,
        systemImage: String,
        tone: Color,
        primaryActionKind: StorageDataCardActionKind,
        reviewEnabled: Bool = true,
        primaryEnabled: Bool = true,
        reviewAction: @escaping () -> Void,
        primaryAction: @escaping () -> Void
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: systemImage)
                    .foregroundStyle(tone)
                Text(title.uppercased())
                    .font(AetowerDesign.Typography.metadataStrong)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
            }
            Text(value)
                .font(AetowerDesign.Typography.metricValue(size: 15, weight: .semibold))
                .foregroundStyle(tone)
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(detail)
                .font(AetowerDesign.Typography.metadata)
                .foregroundStyle(AetowerDesign.Ink.tertiary)
                .lineLimit(2)
            Spacer(minLength: AetowerDesign.Spacing.xs)
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Button {
                    reviewAction()
                } label: {
                    Label(StorageDataCardActionKind.review.title, systemImage: StorageDataCardActionKind.review.systemImage)
                        .labelStyle(.titleAndIcon)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .disabled(!reviewEnabled)

                Button {
                    primaryAction()
                } label: {
                    storageDataCardActionLabel(primaryActionKind)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.mini)
                .disabled(!primaryEnabled)
            }
        }
        .padding(AetowerDesign.Spacing.sm)
        .frame(maxWidth: .infinity, minHeight: 112, alignment: .topLeading)
        .background(
            AetowerDesign.Surface.card,
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md, style: .continuous)
        )
    }

    @ViewBuilder
    private func storageDataCardActionLabel(_ actionKind: StorageDataCardActionKind) -> some View {
        if actionKind == .scan && state.storageHygieneIsLoading {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                ProgressView()
                    .controlSize(.mini)
                Text("Scanning")
            }
        } else {
            Label(actionKind.title, systemImage: actionKind.systemImage)
                .labelStyle(.titleAndIcon)
        }
    }

    private func storageScanFreshnessLabel(_ report: StorageHygieneReportModel) -> String {
        let capturedAt = Date(timeIntervalSince1970: Double(report.capturedAtMillis) / 1_000)
        let seconds = max(0, Int(Date().timeIntervalSince(capturedAt)))
        if seconds < 60 {
            return "\(seconds)s ago"
        }
        let minutes = seconds / 60
        if minutes < 60 {
            return "\(minutes)m ago"
        }
        let hours = minutes / 60
        if hours < 48 {
            return "\(hours)h ago"
        }
        return capturedAt.formatted(date: .abbreviated, time: .omitted)
    }

    private func storageReclaimTableSection(_ report: StorageHygieneReportModel) -> some View {
        let visibleItems = filteredItems(from: report)
        let visibleFolders = storageReclaimFolderRows(report: report, visibleItems: visibleItems)
        return AetowerSurface(level: .card, padding: AetowerDesign.Spacing.md, cornerRadius: 16) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Picker("Reclaim list", selection: $reclaimListMode) {
                        ForEach(StorageReclaimListMode.allCases) { mode in
                            Text(mode.label).tag(mode)
                        }
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 190)

                    Picker("Tier", selection: $selectedFilter) {
                        ForEach(StorageFilter.allCases) { filter in
                            Text(filter.label).tag(filter)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(maxWidth: 150)

                    Picker("Scope", selection: $artifactScope) {
                        ForEach(StorageArtifactScope.allCases) { scope in
                            Text(scope.label).tag(scope)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(maxWidth: 160)

                    Picker("Sort", selection: $artifactSort) {
                        ForEach(StorageArtifactSort.allCases) { sort in
                            Text(sort.label).tag(sort)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(maxWidth: 180)

                    Spacer()

                    AetowerBadge(
                        reclaimListMode == .files
                            ? "\(visibleItems.count) file candidate\(visibleItems.count == 1 ? "" : "s")"
                            : "\(visibleFolders.count) folder candidate\(visibleFolders.count == 1 ? "" : "s")",
                        systemImage: "list.bullet.rectangle",
                        tone: AetowerDesign.Tone.disk
                    )
                    if !cleanupBasket.isEmpty {
                        Button {
                            showCleanupBasket = true
                        } label: {
                            Label(basketSummaryLabel, systemImage: "tray.full")
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                    }
                }

                switch reclaimListMode {
                case .files:
                    storageReclaimFilesTable(visibleItems)
                case .folders:
                    storageReclaimFoldersTable(visibleFolders)
                }

                storageReclaimSelectionInspector(
                    visibleItems: visibleItems,
                    visibleFolders: visibleFolders
                )
            }
        }
    }

    private func storageReclaimFilesTable(_ visibleItems: [StorageHygieneItemModel]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            if visibleItems.isEmpty {
                ContentUnavailableView(
                    "No matching files",
                    systemImage: "doc.text.magnifyingglass",
                    description: Text("Change the tier, scope, sort, search text, or run a fresh scan.")
                )
            } else {
                storageExplorerTableHeader
                ForEach(visibleItems.prefix(80)) { item in
                    storageExplorerTableRow(item)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            selectedReclaimFilePath = item.path
                            selectedReclaimFolderPath = nil
                        }
                }
                if visibleItems.count > 80 {
                    AetowerInfoBanner(
                        "Showing the first 80 matching files. Narrow the filter or use Explore for paged server-side browsing.",
                        systemImage: "line.3.horizontal.decrease.circle",
                        tone: AetowerDesign.Status.neutral,
                        level: .card
                    )
                }
            }
        }
    }

    private func storageReclaimFoldersTable(_ folders: [StorageReclaimFolderRow]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            if folders.isEmpty {
                ContentUnavailableView(
                    "No matching folders",
                    systemImage: "folder.badge.questionmark",
                    description: Text("Change the filters or run a deeper scan to surface folder-level cleanup candidates.")
                )
            } else {
                storageReclaimFolderTableHeader
                ForEach(folders.prefix(80)) { folder in
                    storageReclaimFolderTableRow(folder)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            selectedReclaimFolderPath = folder.path
                            selectedReclaimFilePath = nil
                        }
                }
                if folders.count > 80 {
                    AetowerInfoBanner(
                        "Showing the first 80 matching folders. Narrow the filters to focus the cleanup plan.",
                        systemImage: "line.3.horizontal.decrease.circle",
                        tone: AetowerDesign.Status.neutral,
                        level: .card
                    )
                }
            }
        }
    }

    @ViewBuilder
    private func storageReclaimSelectionInspector(
        visibleItems: [StorageHygieneItemModel],
        visibleFolders: [StorageReclaimFolderRow]
    ) -> some View {
        if let selectedReclaimFilePath,
           let item = visibleItems.first(where: { $0.path == selectedReclaimFilePath }) {
            storageReclaimFileInspector(item)
        } else if let selectedReclaimFolderPath,
                  let folder = visibleFolders.first(where: { $0.path == selectedReclaimFolderPath }) {
            storageReclaimFolderInspector(folder)
        }
    }

    private func storageReclaimFileInspector(_ item: StorageHygieneItemModel) -> some View {
        AetowerSurface(level: .selected, padding: AetowerDesign.Spacing.md, cornerRadius: AetowerDesign.Radius.lg) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                    Label(item.displayName, systemImage: icon(for: item))
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                    Spacer()
                    storageItemPrimaryAction(item)
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                    Button("Reveal") { reveal(path: item.path) }
                    Button("Quick Look") { quickLook(path: item.path) }
                    Button("Explain") { classificationExplanation = explanation(for: item) }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                HStack(spacing: AetowerDesign.Spacing.xs) {
                    AetowerBadge(cleanupTierLabel(item.cleanupTier), tone: tone(forCleanupTier: item.cleanupTier))
                    AetowerBadge(
                        storageItemIsTrashActionable(item) ? "Trash-ready" : "Review required",
                        systemImage: storageItemIsTrashActionable(item) ? "checkmark.shield" : "exclamationmark.triangle",
                        tone: storageItemIsTrashActionable(item) ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
                    )
                    AetowerBadge(formatBytes(item.sizeBytes), systemImage: "externaldrive", tone: AetowerDesign.Tone.disk)
                    if item.sizeTruncated {
                        AetowerBadge("Partial size", systemImage: "scalemass", tone: AetowerDesign.Status.warning)
                    }
                    Spacer(minLength: 0)
                }

                Text(item.path)
                    .font(AetowerDesign.Typography.compactData(size: 10))
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)

                Text(item.cleanupConsequence.isEmpty ? item.recommendation : item.cleanupConsequence)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                if !item.cleanupBlockers.isEmpty {
                    storageReclaimBlockerText(item.cleanupBlockers)
                }
            }
        }
    }

    private func storageReclaimFolderInspector(_ folder: StorageReclaimFolderRow) -> some View {
        AetowerSurface(level: .selected, padding: AetowerDesign.Spacing.md, cornerRadius: AetowerDesign.Radius.lg) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                    Label(folder.displayName, systemImage: "folder")
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                    Spacer()
                    Button {
                        stageReclaimFolder(folder)
                    } label: {
                        Label("Stage", systemImage: "tray.and.arrow.down")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(!storageReclaimFolderHasStageableContent(folder))
                    Button("Reveal") { reveal(path: folder.path) }
                    Button("Quick Look") { quickLook(path: folder.path) }
                    Button("Copy Plan") { copy(storageReclaimFolderPlan(folder)) }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                HStack(spacing: AetowerDesign.Spacing.xs) {
                    AetowerBadge(cleanupTierLabel(folder.cleanupTier), tone: tone(forCleanupTier: folder.cleanupTier))
                    AetowerBadge(
                        storageReclaimFolderIsTrashActionable(folder) ? "Folder Trash-ready" : "Stages child items",
                        systemImage: storageReclaimFolderIsTrashActionable(folder) ? "checkmark.shield" : "tray",
                        tone: storageReclaimFolderIsTrashActionable(folder) ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
                    )
                    AetowerBadge(formatBytes(folder.sizeBytes), systemImage: "externaldrive", tone: AetowerDesign.Tone.disk)
                    AetowerBadge("\(folder.itemCount) item\(folder.itemCount == 1 ? "" : "s")", systemImage: "number", tone: AetowerDesign.Status.neutral)
                    AetowerBadge(folder.source, systemImage: "link", tone: AetowerDesign.Tone.memory)
                    Spacer(minLength: 0)
                }

                Text(folder.path)
                    .font(AetowerDesign.Typography.compactData(size: 10))
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)

                Text(storageReclaimFolderIsTrashActionable(folder)
                    ? "Aetower can stage this folder as one Trash target because the folder projection and current preflight agree."
                    : "Aetower will not delete this folder as a unit. It can stage known safe child items or produce a manual cleanup plan.")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                if !folder.cleanupBlockers.isEmpty {
                    storageReclaimBlockerText(folder.cleanupBlockers)
                }
            }
        }
    }

    private func storageReclaimBlockerText(_ blockers: [String]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            ForEach(Array(blockers.prefix(4)), id: \.self) { blocker in
                Label(blocker, systemImage: "exclamationmark.triangle")
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Status.error)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var storageReclaimFolderTableHeader: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Text("Folder")
                .frame(maxWidth: .infinity, alignment: .leading)
            Text("Tier")
                .frame(width: 88, alignment: .center)
            Text("Source")
                .frame(width: 92, alignment: .center)
            Text("Items")
                .frame(width: 52, alignment: .trailing)
            Text("Size")
                .frame(width: 80, alignment: .trailing)
            Text("Actions")
                .frame(width: 84, alignment: .trailing)
        }
        .font(AetowerDesign.Typography.metadataStrong)
        .foregroundStyle(AetowerDesign.Ink.secondary)
        .padding(.horizontal, AetowerDesign.Spacing.sm)
    }

    private func storageReclaimFolderTableRow(_ folder: StorageReclaimFolderRow) -> some View {
        AetowerSurface(level: .card, padding: 0, cornerRadius: AetowerDesign.Radius.md) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text(folder.displayName)
                        .font(AetowerDesign.Typography.caption.weight(.semibold))
                        .lineLimit(1)
                    Text(folder.path)
                        .font(AetowerDesign.Typography.compactData(size: 10))
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if !folder.cleanupBlockers.isEmpty {
                        Text(folder.cleanupBlockers.prefix(2).joined(separator: "; "))
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Status.error)
                            .lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                AetowerBadge(
                    cleanupTierLabel(folder.cleanupTier),
                    tone: tone(forCleanupTier: folder.cleanupTier)
                )
                .frame(width: 88)

                Text(folder.source)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                    .frame(width: 92)

                Text("\(folder.itemCount)")
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .frame(width: 52, alignment: .trailing)

                Text(formatBytes(folder.sizeBytes))
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .frame(width: 80, alignment: .trailing)

                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Button {
                        stageReclaimFolder(folder)
                    } label: {
                        Label("Stage", systemImage: "tray.and.arrow.down")
                    }
                    .disabled(!storageReclaimFolderHasStageableContent(folder))

                    Menu {
                        Button("Reveal in Finder") { reveal(path: folder.path) }
                        Button("Quick Look") { quickLook(path: folder.path) }
                        Button("Copy cleanup plan") { copy(storageReclaimFolderPlan(folder)) }
                        Button("Copy path") { copy(folder.path) }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                            .font(AetowerDesign.Typography.caption.weight(.semibold))
                    }
                    .menuStyle(.borderlessButton)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .frame(width: 84, alignment: .trailing)
            }
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
        }
    }

    private func storageReclaimFolderRows(
        report: StorageHygieneReportModel,
        visibleItems: [StorageHygieneItemModel]
    ) -> [StorageReclaimFolderRow] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        var rowsByPath: [String: StorageReclaimFolderRow] = [:]

        for footprint in report.repoFootprints {
            for folder in footprint.topArtifactFolders where !state.storagePathWasMovedToTrash(folder.path) {
                let row = storageReclaimFolderRow(from: folder, repoName: footprint.repoName)
                guard storageReclaimFolderMatches(row, query: query),
                      storageReclaimFolderMatchesFilters(row)
                else { continue }
                rowsByPath[row.path] = row
            }
        }

        let groupedItems = Dictionary(grouping: visibleItems, by: storageReclaimParentFolderPath)
        for (path, items) in groupedItems where !path.isEmpty && rowsByPath[path] == nil {
            let row = storageReclaimFolderRow(path: path, items: items)
            guard storageReclaimFolderMatches(row, query: query) else { continue }
            rowsByPath[path] = row
        }

        return rowsByPath.values.sorted(by: storageReclaimFolderSort)
    }

    private func storageReclaimFolderRow(
        from folder: StorageRepoArtifactFolderModel,
        repoName: String
    ) -> StorageReclaimFolderRow {
        StorageReclaimFolderRow(
            id: "repo-folder|\(folder.path)",
            path: folder.path,
            displayName: folder.displayName,
            kind: folder.kind,
            cleanupTier: folder.cleanupTier,
            safety: folder.cleanupAllowed ? "safe" : "review",
            sizeBytes: folder.sizeBytes,
            itemCount: 1,
            cleanupAllowed: folder.cleanupAllowed,
            cleanupBlockers: folder.cleanupBlockers,
            defaultCleanupAction: folder.defaultCleanupAction,
            sizeTruncated: folder.sizeTruncated,
            cloudPlaceholder: folder.cloudPlaceholder,
            hasHardlinks: folder.hasHardlinks,
            source: repoName,
            stageItems: []
        )
    }

    private func storageReclaimFolderRow(
        path: String,
        items: [StorageHygieneItemModel]
    ) -> StorageReclaimFolderRow {
        let sortedItems = items.sorted(by: storageItemSizeSort)
        let actionableItems = sortedItems.filter(storageItemIsTrashActionable)
        let cleanupTier = strongestCleanupTier(in: sortedItems)
        let blockers = sortedItems.flatMap(\.cleanupBlockers)
        return StorageReclaimFolderRow(
            id: "aggregate-folder|\(path)",
            path: path,
            displayName: URL(fileURLWithPath: path).lastPathComponent.isEmpty ? path : URL(fileURLWithPath: path).lastPathComponent,
            kind: "folder",
            cleanupTier: cleanupTier,
            safety: actionableItems.isEmpty ? "review" : "safe",
            sizeBytes: sumItemBytes(sortedItems),
            itemCount: sortedItems.count,
            cleanupAllowed: !actionableItems.isEmpty,
            cleanupBlockers: blockers.isEmpty && !actionableItems.isEmpty
                ? []
                : blockers.isEmpty ? ["Folder aggregate stages safe child items only."] : Array(Set(blockers)).sorted(),
            defaultCleanupAction: "stage_children",
            sizeTruncated: sortedItems.contains { $0.sizeTruncated },
            cloudPlaceholder: sortedItems.contains { $0.cloudPlaceholder },
            hasHardlinks: sortedItems.contains { $0.hasHardlinks },
            source: "items",
            stageItems: actionableItems
        )
    }

    private func storageReclaimParentFolderPath(_ item: StorageHygieneItemModel) -> String {
        let url = URL(fileURLWithPath: item.path).standardizedFileURL
        let parent = url.deletingLastPathComponent().path
        return parent == "." ? "" : parent
    }

    private func strongestCleanupTier(in items: [StorageHygieneItemModel]) -> String {
        let tiers = Set(items.map(\.cleanupTier))
        if tiers.contains("risky") { return "risky" }
        if tiers.contains("expensive") { return "expensive" }
        if tiers.contains("rebuildable") { return "rebuildable" }
        if tiers.contains("safe") { return "safe" }
        return items.first?.cleanupTier ?? "review"
    }

    private func storageReclaimFolderMatches(_ folder: StorageReclaimFolderRow, query: String) -> Bool {
        query.isEmpty
            || folder.displayName.lowercased().contains(query)
            || folder.path.lowercased().contains(query)
            || folder.kind.lowercased().contains(query)
            || folder.cleanupTier.lowercased().contains(query)
            || folder.source.lowercased().contains(query)
    }

    private func storageReclaimFolderMatchesFilters(_ folder: StorageReclaimFolderRow) -> Bool {
        switch selectedFilter {
        case .attention:
            return folder.cleanupTier == "risky"
                || folder.cleanupTier == "expensive"
                || folder.safety != "safe"
                || folder.sizeTruncated
                || folder.sizeBytes >= 100 * 1024 * 1024
        case .safe:
            return folder.cleanupTier == "safe"
        case .rebuildable:
            return folder.cleanupTier == "rebuildable"
        case .expensive:
            return folder.cleanupTier == "expensive"
        case .risky:
            return folder.cleanupTier == "risky"
        case .all:
            return true
        }
    }

    private func storageReclaimFolderSort(
        _ left: StorageReclaimFolderRow,
        _ right: StorageReclaimFolderRow
    ) -> Bool {
        switch artifactSort {
        case .smallest:
            return left.sizeBytes == right.sizeBytes ? left.path < right.path : left.sizeBytes < right.sizeBytes
        case .path:
            return left.path.localizedStandardCompare(right.path) == .orderedAscending
        case .tier:
            let leftKey = "\(left.cleanupTier)|\(left.safety)|\(left.path)"
            let rightKey = "\(right.cleanupTier)|\(right.safety)|\(right.path)"
            return leftKey < rightKey
        case .recommended, .largest, .newest, .oldest:
            return left.sizeBytes == right.sizeBytes ? left.path < right.path : left.sizeBytes > right.sizeBytes
        }
    }

    private func storageReclaimFolderIsTrashActionable(_ folder: StorageReclaimFolderRow) -> Bool {
        folder.cleanupAllowed
            && folder.defaultCleanupAction == "trash"
            && folder.cleanupBlockers.isEmpty
            && folder.cleanupTier != "risky"
            && !folder.sizeTruncated
            && !folder.cloudPlaceholder
            && storageCleanupPathExists(folder.path)
            && storagePrivilegedCleanupBlocker(for: folder.path) == nil
    }

    private func storageReclaimFolderHasStageableContent(_ folder: StorageReclaimFolderRow) -> Bool {
        storageReclaimFolderIsTrashActionable(folder) || !folder.stageItems.isEmpty
    }

    private func storageReclaimSupportingData(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            similarFilesOverviewSection(report)
            if let coldData = report.coldData, coldData.bands.contains(where: { $0.itemCount > 0 }) {
                coldDataLaneSection(coldData)
            }
            cleanupRecipesSection(report)
            cleanupAuditSection
            if report.truncated {
                warningBanner("The scan hit a cap or time budget. Results are partial; open Insights to inspect coverage or narrow the root.")
            }
        }
    }

    /// Hunt-for-space surface: the visual treemap and the full item list.
    private func storageExploreWorkspace(_ report: StorageHygieneReportModel) -> some View {
        LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            storageExplorePaneHeader
            storageExplorePaneContent(report)
        }
    }

    private var storageExplorePaneHeader: some View {
        AetowerSurface(level: .card, padding: AetowerDesign.Spacing.lg, cornerRadius: AetowerDesign.Radius.lg) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                    Label("Explore", systemImage: "square.grid.3x3.topleft.filled")
                        .font(AetowerDesign.Typography.sectionTitle)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Spacer()
                    Picker("Explore focus", selection: $selectedExplorePane) {
                        ForEach(StorageExplorePane.allCases) { pane in
                            Text(pane.label).tag(pane)
                        }
                    }
                    .labelsHidden()
                    .pickerStyle(.segmented)
                    .frame(width: 360)
                }

                Text(selectedExplorePane.detail)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    @ViewBuilder
    private func storageExplorePaneContent(_ report: StorageHygieneReportModel) -> some View {
        switch selectedExplorePane {
        case .browse:
            visualExplorationSection(report)
        case .optimize:
            wholeComputerOptimizationSection(report)
            storageInvestigationSection(report)
        case .similar:
            similarFilesReviewSection(report)
        case .raw:
            itemSection(report)
        }
    }

    /// Analytical surface: everything that explains rather than acts.
    private func storageInsightsWorkspace(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            storageHomeActionsSection(report)
            if let insights = report.growthInsights {
                storageGrowthInsightsSection(insights)
                if let diff = insights.sinceLastScan {
                    storageSinceLastScanSection(diff)
                }
            }
            storageGrowthTimeline(report)
            repoFootprintDashboard(report)
            budgetGuardrailsSection(report)
            if shouldShowAgentHygieneOverview(report) {
                agentHygieneSection(report)
            }
            storageCoverageOverview(report)
            summaryGrid(report)
            storageAdvanced(report)
        }
    }

    private func storageActionHome(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            storageActionPanel(report)
            storageHomeActionsSection(report)
            topOffenderCallout(report)
            if report.truncated {
                warningBanner("The scan hit a cap or time budget. Results are partial; use Sources to inspect coverage or narrow the root.")
            }
        }
    }

    private func storageReclaimWorkspace(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            storageActionPanel(report)
            cleanupPreviewSection(report)
            cleanupBundlesSection(report)
            reclaimSpaceSection(report)
            if let coldData = report.coldData, coldData.bands.contains(where: { $0.itemCount > 0 }) {
                coldDataLaneSection(coldData)
            }
            cleanupRecipesSection(report)
            cleanupAuditSection
        }
    }

    private func storageGrowthWorkspace(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            topOffenderCallout(report)
            if let insights = report.growthInsights {
                storageGrowthInsightsSection(insights)
                if let diff = insights.sinceLastScan {
                    storageSinceLastScanSection(diff)
                }
            }
            storageGrowthTimeline(report)
            repoFootprintDashboard(report)
            budgetGuardrailsSection(report)
        }
    }

    private func storageExplorerWorkspace(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            visualExplorationSection(report)
            itemSection(report)
        }
    }

    private func storageInventoryWorkspace(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            wholeComputerOptimizationSection(report)
            if shouldShowAgentHygieneOverview(report) {
                agentHygieneSection(report)
            }
            storageInvestigationSection(report)
            summaryGrid(report)
        }
    }

    private func storageSourcesWorkspace(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            storageCoverageOverview(report)
            volumeStateSection(report)
            rootsSection(report)
            caveatsSection(report)
        }
    }

    private func storagePoliciesWorkspace(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            budgetGuardrailsSection(report)
            cleanupAuditSection
            cleanupPreviewSection(report)
        }
    }

    private func storageOverview(_ report: StorageHygieneReportModel) -> some View {
        storageActionHome(report)
    }

    private func storageHomeActionsSection(_ report: StorageHygieneReportModel) -> some View {
        let actions = storageHomeActions(from: report)
        let maxBytes = actions.map(\.bytes).max() ?? 0
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Where the space is")
                        .font(.title3.weight(.semibold))
                    Text("Grouped by what it is and how safe it is to remove. Bars are relative to the largest group.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
                if !cleanupBasket.isEmpty {
                    Button {
                        showCleanupBasket = true
                    } label: {
                        Label(basketSummaryLabel, systemImage: "tray.full")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 260), spacing: AetowerDesign.Spacing.md)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.md
            ) {
                ForEach(actions) { action in
                    storageHomeActionCard(action, maxBytes: maxBytes)
                }
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func storageHomeActionCard(_ action: StorageHomeAction, maxBytes: UInt64) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: action.systemImage)
                    .foregroundStyle(action.tone)
                    .frame(width: 22)
                VStack(alignment: .leading, spacing: 3) {
                    Text(action.title)
                        .font(.subheadline.weight(.semibold))
                    Text(action.detail)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: AetowerDesign.Spacing.sm)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(formatBytes(action.bytes))
                        .font(.system(size: 20, weight: .semibold, design: .rounded))
                    Text(
                        action.itemCount == 0
                            ? "\(action.confidence)% confidence"
                            : "\(action.confidence)% · \(action.itemCount) item\(action.itemCount == 1 ? "" : "s")"
                    )
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                }
            }

            // Share bar echoing the disk header: this group's size relative to
            // the largest group, so the big offenders read at a glance.
            GeometryReader { geo in
                let ratio = maxBytes > 0 ? CGFloat(Double(action.bytes) / Double(maxBytes)) : 0
                ZStack(alignment: .leading) {
                    Capsule().fill(AetowerDesign.Surface.badgeStrong)
                    Capsule().fill(action.tone.opacity(0.85))
                        .frame(width: max(2, geo.size.width * ratio))
                }
            }
            .frame(height: 5)

            storageActionMetaChips(action)

            storageHomeActionSamples(action)
            if let bundle = action.cleanupBundle {
                Text("Manifest: \(bundle.title) · \(bundle.itemCount) item\(bundle.itemCount == 1 ? "" : "s") · \(bundle.confidenceScore)% confidence")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .lineLimit(1)
            }

            Spacer(minLength: 0)

            HStack(spacing: AetowerDesign.Spacing.sm) {
                let dataActionKind = storageHomeActionKind(action)
                if dataActionKind == .clean {
                    let decision = storageReclaimActionDecision(for: action)
                    Button {
                        if decision == .moveToTrash {
                            stageStorageHomeAction(action, presentExecution: true)
                        } else {
                            stageStorageHomeAction(action)
                        }
                    } label: {
                        Label(
                            decision.title,
                            systemImage: decision.systemImage
                        )
                    }
                    .buttonStyle(.borderedProminent)
                } else {
                    Button {
                        reviewStorageHomeAction(action)
                    } label: {
                        Label(dataActionKind.title, systemImage: dataActionKind.systemImage)
                    }
                    .buttonStyle(.borderedProminent)
                }

                Button {
                    classificationExplanation = explanation(for: action)
                } label: {
                    Label("Explain", systemImage: "info.circle")
                }
                Spacer()
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 240, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func storageHomeActionKind(_ action: StorageHomeAction) -> StorageDataCardActionKind {
        switch action.id {
        case "safe-reclaim", "developer-artifacts":
            return action.hasStageableItems ? .clean : .review
        default:
            return .review
        }
    }

    private func reviewStorageHomeAction(_ action: StorageHomeAction) {
        switch action.id {
        case "safe-reclaim":
            focusReclaimTable(filter: .safe, scope: .all, sort: .recommended)
        case "developer-artifacts":
            focusReclaimTable(filter: .all, scope: .repoLinked, sort: .recommended)
        case "largest-offenders":
            focusReclaimTable(filter: .all, scope: .all, sort: .largest)
        case "recently-grew":
            focusReclaimTable(filter: .all, scope: .all, sort: .newest)
        case "old-unused":
            focusReclaimTable(filter: .attention, scope: .cold, sort: .recommended)
        case "risky-review":
            focusReclaimTable(filter: .risky, scope: .all, sort: .recommended)
        default:
            focusReclaimTable(filter: .all, scope: .all, sort: .recommended)
        }

        if let firstItem = action.sampleItems.first {
            selectedReclaimFilePath = firstItem.path
            selectedReclaimFolderPath = nil
            reclaimListMode = .files
        } else {
            classificationExplanation = explanation(for: action)
        }
    }

    private func focusReclaimTable(
        filter: StorageFilter,
        scope: StorageArtifactScope,
        sort: StorageArtifactSort
    ) {
        selectedSection = .reclaim
        selectedFilter = filter
        artifactScope = scope
        artifactSort = sort
        reclaimListMode = .files
        selectedReclaimFilePath = nil
        selectedReclaimFolderPath = nil
        searchText = ""
    }

    /// Compact one-line replacement for the old six-tile decision grid. The
    /// header already carries the title, size, and confidence; the sample rows
    /// below carry per-item detail. This strip keeps only the three signals a
    /// user actually decides on: is it safe, what does rebuilding cost, and
    /// how reversible is it.
    private func storageActionMetaChips(_ action: StorageHomeAction) -> some View {
        let relevantItems = action.stageItems.isEmpty ? action.sampleItems : action.stageItems
        return HStack(spacing: AetowerDesign.Spacing.xs) {
            storageMetaChip(
                systemImage: action.hasStageableItems ? "checkmark.shield.fill" : "exclamationmark.triangle.fill",
                text: action.hasStageableItems ? "Trash-ready" : "Needs review",
                tone: action.hasStageableItems ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
            )
            if action.hasStageableItems {
                storageMetaChip(
                    systemImage: action.cleanupBundle == nil ? "arrow.uturn.backward" : "shippingbox.fill",
                    text: action.cleanupBundle == nil ? "Undoable" : "Manifest",
                    tone: action.cleanupBundle == nil ? AetowerDesign.Status.neutral : AetowerDesign.Tone.disk
                )
            }
            let rebuild = rebuildCostSummary(for: relevantItems)
            if !["n/a", "Unknown", "None expected"].contains(rebuild) {
                storageMetaChip(
                    systemImage: "hammer.fill",
                    text: "Rebuild \(rebuild)",
                    tone: AetowerDesign.Tone.cpu
                )
            }
            Spacer(minLength: 0)
        }
    }

    private func storageMetaChip(systemImage: String, text: String, tone: Color) -> some View {
        HStack(spacing: 4) {
            Image(systemName: systemImage)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(tone)
            Text(text)
                .font(.caption2.weight(.medium))
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(1)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(tone.opacity(0.12), in: Capsule())
    }

    private func storageHomeActionSamples(_ action: StorageHomeAction) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            if !action.sampleItems.isEmpty {
                ForEach(Array(action.sampleItems.prefix(3))) { item in
                    storageHomeItemSample(item)
                }
            } else if !action.growthEvents.isEmpty {
                ForEach(Array(action.growthEvents.prefix(3))) { event in
                    storageHomeGrowthSample(event)
                }
            } else {
                Label("No current candidates in this lane.", systemImage: "checkmark.circle")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func storageHomeItemSample(_ item: StorageHygieneItemModel) -> some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            Image(systemName: icon(for: item))
                .foregroundStyle(tone(for: item))
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.displayName)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                Text(item.path)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let accounting = storageByteAccountingSummary(item) {
                    Text(accounting)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: AetowerDesign.Spacing.xs)
            Text(formatBytes(item.sizeBytes))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
            storageItemActionMenu(item)
        }
    }

    private func storageHomeGrowthSample(_ event: StorageGrowthTimelineEvent) -> some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            Image(systemName: cleanupTierIcon(event.cleanupTier))
                .foregroundStyle(tone(forCleanupTier: event.cleanupTier))
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(event.displayName)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                Text(storageGrowthCorrelationDetail(event))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
            Spacer(minLength: AetowerDesign.Spacing.xs)
            Text("+\(formatBytes(UInt64(event.deltaBytes)))")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(AetowerDesign.Status.warning)
            storageGrowthActionMenu(event)
        }
    }

    /// Single primary action per row: safe items trash in one click (with an
    /// undo toast); anything needing review stages silently into the batch bar.
    @ViewBuilder
    private func storageItemPrimaryAction(_ item: StorageHygieneItemModel) -> some View {
        let directTrashInFlight = directTrashInFlightPaths.contains(item.path)
        if storageItemIsTrashActionable(item) && item.safety == "safe" {
            Button {
                trashItemDirectly(item)
            } label: {
                if directTrashInFlight {
                    Label("Moving", systemImage: "hourglass")
                } else {
                    Label("Trash", systemImage: "trash")
                }
            }
            .disabled(directTrashInFlight)
            .help("Move to Finder Trash now (undoable)")
        } else {
            Button {
                _ = stageCleanupItem(item)
            } label: {
                Label("Stage", systemImage: "tray.and.arrow.down")
            }
            .disabled(!storageItemIsTrashActionable(item))
            .help(
                storageItemIsTrashActionable(item)
                    ? "Add to the cleanup batch for review"
                    : "Not eligible for Trash cleanup"
            )
        }
    }

    private func storageItemActionMenu(_ item: StorageHygieneItemModel) -> some View {
        let directTrashInFlight = directTrashInFlightPaths.contains(item.path)
        return Menu {
            if storageItemIsTrashActionable(item) && item.safety == "safe" {
                Button("Move to Trash now") { trashItemDirectly(item) }
                    .disabled(directTrashInFlight)
            }
            Button("Stage cleanup") { _ = stageCleanupItem(item) }
                .disabled(!storageItemIsTrashActionable(item))
            Divider()
            Button("Quick Look") { quickLook(path: item.path) }
            Button("Reveal in Finder") { reveal(path: item.path) }
            Button("Explain classification") { classificationExplanation = explanation(for: item) }
            Divider()
            Button("Copy path") { copy(item.path) }
            Button("Copy command reference") { copy(item.commandHint) }
        } label: {
            Image(systemName: "ellipsis.circle")
                .font(AetowerDesign.Typography.caption.weight(.semibold))
        }
        .menuStyle(.borderlessButton)
        .controlSize(.mini)
        .help("Storage item actions")
    }

    private func storageGrowthActionMenu(_ event: StorageGrowthTimelineEvent) -> some View {
        Menu {
            Button("Quick Look") { quickLook(path: event.path) }
            Button("Reveal in Finder") { reveal(path: event.path) }
            Button("Explain classification") { classificationExplanation = explanation(for: event) }
        } label: {
            Image(systemName: "ellipsis.circle")
                .font(AetowerDesign.Typography.caption.weight(.semibold))
        }
        .menuStyle(.borderlessButton)
        .controlSize(.mini)
        .help("Storage growth actions")
    }

    private func classificationExplanationSheet(_ explanation: StorageClassificationExplanation) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "questionmark.folder")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text(explanation.title)
                        .font(.title3.weight(.semibold))
                    Text(explanation.classification)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            ScrollView {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Path")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        Text(explanation.path.isEmpty ? "No single path." : explanation.path)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .padding(AetowerDesign.Spacing.sm)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    }

                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Consequence")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        Text(explanation.consequence)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    if !explanation.blockers.isEmpty {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                            Text("Cleanup blockers")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Status.error)
                            ForEach(explanation.blockers, id: \.self) { blocker in
                                Label(blocker, systemImage: "hand.raised")
                                    .font(.caption2)
                                    .foregroundStyle(AetowerDesign.Status.error)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }

                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Evidence")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        ForEach(explanation.evidence, id: \.self) { evidence in
                            Label(evidence, systemImage: "smallcircle.filled.circle")
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                .padding(.trailing, AetowerDesign.Spacing.sm)
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Close") {
                    classificationExplanation = nil
                }
                Spacer()
                if !explanation.path.isEmpty {
                    Button("Quick Look") {
                        quickLook(path: explanation.path)
                    }
                    Button("Reveal in Finder") {
                        reveal(path: explanation.path)
                    }
                }
                Button("Copy explanation") {
                    copy(classificationExplanationMarkdown(explanation))
                }
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.xl)
        .frame(width: 720, height: 560, alignment: .topLeading)
    }

    private func explanation(for action: StorageHomeAction) -> StorageClassificationExplanation {
        return StorageClassificationExplanation(
            id: "action|\(action.id)",
            title: action.title,
            path: action.sampleItems.first?.path ?? action.growthEvents.first?.path ?? "",
            classification: "\(action.itemCount) item(s) · \(formatBytes(action.bytes)) · \(action.confidence)% confidence",
            consequence: action.consequence,
            evidence: recommendationDecisionEvidence(actionRecommendationDecisions(action)) + actionEvidence(action),
            blockers: action.sampleItems.flatMap(\.cleanupBlockers).prefix(8).map { $0 }
        )
    }

    private func explanation(for item: StorageHygieneItemModel) -> StorageClassificationExplanation {
        var evidence = recommendationDecisionEvidence(itemRecommendationDecisions(item))
        evidence.append(contentsOf: item.evidence.isEmpty ? [item.reason, item.recommendation] : item.evidence)
        if let rebuildCommand = item.rebuildCommand, !rebuildCommand.isEmpty {
            evidence.append("Rebuild command: \(rebuildCommand).")
        }
        evidence.append(
            "Estimated rebuild cost: \(item.estimatedRebuildCost) (\(rebuildTimeLabel(item.estimatedRebuildSeconds)))."
        )
        return StorageClassificationExplanation(
            id: "item|\(item.id)",
            title: item.displayName,
            path: item.path,
            classification: "\(cleanupTierLabel(item.cleanupTier)) · \(storageRoleLabel(item.storageRole)) · \(gitStatusLabel(item.gitStatus))",
            consequence: item.cleanupConsequence.isEmpty ? item.nextStep : item.cleanupConsequence,
            evidence: evidence,
            blockers: item.cleanupBlockers
        )
    }

    private func explanation(for item: StorageCleanupBundleItemModel) -> StorageClassificationExplanation {
        StorageClassificationExplanation(
            id: "bundle-item|\(item.id)",
            title: item.displayName,
            path: item.path,
            classification: "\(cleanupTierLabel(item.cleanupTier)) · \(item.safety) · \(item.confidenceScore)% confidence",
            consequence: item.rollbackNote,
            evidence: [
                "What: \(item.displayName).",
                "Why: \(item.reason)",
                "Safe?: \(cleanupBundleItemIsActionable(item) ? cleanupTierLabel(item.cleanupTier) : "Review").",
                "Reclaim: \(formatBytes(item.sizeBytes)).",
                "Rebuild cost: \(item.consequence)",
                "Undo path: \(item.rollbackNote)",
                cleanupBundleItemIsActionable(item)
                    ? "Default action is Finder Trash."
                    : "Default action is manual review.",
            ],
            blockers: item.cleanupBlockers
        )
    }

    private func explanation(for event: StorageGrowthTimelineEvent) -> StorageClassificationExplanation {
        StorageClassificationExplanation(
            id: "growth|\(event.id)",
            title: event.displayName,
            path: event.path,
            classification: "Recently grew · \(cleanupTierLabel(event.cleanupTier)) · \(storageGrowthConfidenceLabel(event))",
            consequence: storageGrowthCorrelationDetail(event),
            evidence: [
                "Current size is \(formatBytes(event.currentBytes)); previous baseline was \(formatBytes(event.previousBytes)).",
                "Positive delta is \(formatBytes(UInt64(event.deltaBytes))).",
                "Observed around \(storageGrowthEventTime(event)).",
                event.attributionSources.isEmpty ? "" : "Attribution sources: \(event.attributionSources.joined(separator: ", ")).",
                event.attributionSummary,
            ].filter { !$0.isEmpty } + event.attributionEvidence,
            blockers: []
        )
    }

    private func explanation(for finding: StorageInvestigationFindingModel) -> StorageClassificationExplanation {
        StorageClassificationExplanation(
            id: "finding|\(finding.id)",
            title: finding.title,
            path: finding.path,
            classification: "\(cleanupTierLabel(finding.cleanupTier)) · \(storageRoleLabel(finding.storageRole)) · \(finding.confidenceScore)% confidence",
            consequence: finding.recommendedAction,
            evidence: finding.evidence.isEmpty ? [finding.detail] : finding.evidence,
            blockers: finding.safety == "safe" ? [] : ["Finding requires manual review because safety is \(finding.safety)."]
        )
    }

    private func explanation(for recipe: StorageCleanupRecipeModel) -> StorageClassificationExplanation {
        StorageClassificationExplanation(
            id: "recipe|\(recipe.id)",
            title: recipe.title,
            path: recipe.affectedPath,
            classification: "\(cleanupTierLabel(recipe.safety)) · \(recipe.requiresReview ? "manual review" : "ready")",
            consequence: recipe.reason,
            evidence: recommendationDecisionEvidence(recipeRecommendationDecisions(recipe))
                + (recipe.prerequisites.isEmpty ? [recipe.command] : recipe.prerequisites),
            blockers: recipe.requiresReview ? ["Recipe requires review before cleanup."] : []
        )
    }

    private func explanation(for item: StorageAgentItemSummaryModel) -> StorageClassificationExplanation {
        StorageClassificationExplanation(
            id: "agent-item|\(item.id)",
            title: item.displayName,
            path: item.path,
            classification: "\(cleanupTierLabel(item.cleanupTier)) · AI-agent attributed · \(item.kind)",
            consequence: "This artifact is attributed to an AI-agent storage lane. Review the owning session or repository before cleanup.",
            evidence: [
                "Estimated size: \(formatBytes(item.sizeBytes)).",
                "Kind: \(item.kind).",
                "Cleanup tier: \(cleanupTierLabel(item.cleanupTier)).",
            ],
            blockers: item.cleanupTier == "risky" ? ["Risky tier requires manual review."] : []
        )
    }

    private func explanation(for item: StorageCleanupBasketItem) -> StorageClassificationExplanation {
        StorageClassificationExplanation(
            id: "basket|\(item.id)",
            title: item.title,
            path: item.path,
            classification: "\(cleanupTierLabel(item.cleanupTier)) · \(item.safety) · \(item.source)",
            consequence: item.consequence,
            evidence: uniqueStrings(
                [
                    "What: \(item.title).",
                    "Why: \(item.reason)",
                    "Safe?: \(item.requiresReview ? "Review" : cleanupTierLabel(item.cleanupTier)).",
                    "Reclaim: \(formatBytes(item.estimatedBytes)).",
                    "Rebuild cost: \(item.consequence)",
                    "Undo path: Finder Trash.",
                ]
                    + item.evidence
                    + item.prerequisites
            ),
            blockers: item.blockers
        )
    }

    private func actionEvidence(_ action: StorageHomeAction) -> [String] {
        var evidence = [
            "Estimated bytes: \(formatBytes(action.bytes)).",
            "Matched items: \(action.itemCount).",
            "Confidence: \(action.confidence)%.",
        ]
        evidence.append(contentsOf: action.sampleItems.prefix(4).map {
            "\($0.displayName): \($0.reason)"
        })
        evidence.append(contentsOf: action.growthEvents.prefix(4).map(storageGrowthEventTitle))
        return evidence
    }

    private func recommendationDecisionEvidence(_ decisions: [StorageRecommendationDecision]) -> [String] {
        decisions.map { decision in
            "\(decision.title): \(decision.value) - \(decision.detail)"
        }
    }

    private func classificationExplanationMarkdown(_ explanation: StorageClassificationExplanation) -> String {
        var lines = [
            "# \(explanation.title)",
            "",
            "- Path: \(explanation.path.isEmpty ? "n/a" : explanation.path)",
            "- Classification: \(explanation.classification)",
            "- Consequence: \(explanation.consequence)",
        ]
        if !explanation.blockers.isEmpty {
            lines.append(contentsOf: ["", "## Blockers"])
            lines.append(contentsOf: explanation.blockers.map { "- \($0)" })
        }
        lines.append(contentsOf: ["", "## Evidence"])
        lines.append(contentsOf: explanation.evidence.map { "- \($0)" })
        return lines.joined(separator: "\n")
    }

    private func storageHomeActions(from report: StorageHygieneReportModel) -> [StorageHomeAction] {
        let allItems = visibleStorageItems(from: report).sorted { left, right in
            left.sizeBytes == right.sizeBytes ? left.path < right.path : left.sizeBytes > right.sizeBytes
        }
        let stageable = allItems.filter(storageItemIsTrashActionable)
        let safe = stageable.filter {
            $0.safety == "safe" && ($0.cleanupTier == "safe" || $0.cleanupTier == "rebuildable")
        }
        let developer = stageable.filter(isDeveloperStorageArtifact)
        let largest = Array(allItems.prefix(8))
        let recentlyGrew = storageGrowthEvents(from: report)
        let oldUnused = allItems.filter(isOldUnusedStorageItem)
        let risky = allItems.filter {
            $0.cleanupTier == "risky" || $0.safety != "safe" || !$0.cleanupAllowed || !$0.cleanupBlockers.isEmpty
        }
        let primaryCleanupBundle = report.cleanupBundles.first(where: cleanupBundleHasActionableCommands)

        return [
            StorageHomeAction(
                id: "safe-reclaim",
                title: "Safe Reclaim",
                detail: "High-confidence logs, caches, and rebuildable artifacts.",
                consequence: "Moves eligible local artifacts to Finder Trash. Reversal is normally possible from Trash or by rebuilding.",
                systemImage: "checkmark.shield",
                tone: AetowerDesign.Status.ready,
                bytes: primaryCleanupBundle?.estimatedReclaimableBytes ?? sumItemBytes(safe),
                itemCount: primaryCleanupBundle?.itemCount ?? safe.count,
                confidence: primaryCleanupBundle?.confidenceScore ?? (safe.isEmpty ? 0 : 94),
                sampleItems: Array(safe.prefix(4)),
                stageItems: safe,
                growthEvents: [],
                cleanupBundle: primaryCleanupBundle
            ),
            StorageHomeAction(
                id: "developer-artifacts",
                title: "Developer Artifacts",
                detail: "Build outputs, dependency trees, caches, and local environments.",
                consequence: "Reclaiming can cost rebuild time, package downloads, or simulator/toolchain regeneration.",
                systemImage: "hammer",
                tone: AetowerDesign.Tone.cpu,
                bytes: sumItemBytes(developer),
                itemCount: developer.count,
                confidence: developer.isEmpty ? 0 : 86,
                sampleItems: Array(developer.prefix(4)),
                stageItems: developer,
                growthEvents: [],
                cleanupBundle: nil
            ),
            StorageHomeAction(
                id: "largest-offenders",
                title: "Largest Offenders",
                detail: "Biggest physical-space candidates in the current scan.",
                consequence: "Largest is not the same as safe. Inspect classification before staging anything.",
                systemImage: "chart.bar.xaxis",
                tone: AetowerDesign.Tone.disk,
                bytes: sumItemBytes(largest),
                itemCount: largest.count,
                confidence: largest.isEmpty ? 0 : 58,
                sampleItems: Array(largest.prefix(4)),
                stageItems: largest.filter(storageItemIsTrashActionable),
                growthEvents: [],
                cleanupBundle: nil
            ),
            StorageHomeAction(
                id: "recently-grew",
                title: "Recently Grew",
                detail: "Largest positive deltas since the previous scan or saved baseline.",
                consequence: "Use this to explain sudden disk pressure; attribution may be partial without a live file-event ledger.",
                systemImage: "timeline.selection",
                tone: AetowerDesign.Status.warning,
                bytes: sumGrowthBytes(recentlyGrew),
                itemCount: recentlyGrew.count,
                confidence: recentlyGrew.isEmpty ? 0 : 72,
                sampleItems: itemsMatchingGrowthEvents(recentlyGrew, in: allItems),
                stageItems: itemsMatchingGrowthEvents(recentlyGrew, in: stageable),
                growthEvents: recentlyGrew,
                cleanupBundle: nil
            ),
            StorageHomeAction(
                id: "old-unused",
                title: "Old Unused",
                detail: "Cold or long-unmodified files that may be forgotten.",
                consequence: "Old files can still be important. Aetower stages only policy-approved items and leaves source-like work blocked.",
                systemImage: "snowflake",
                tone: AetowerDesign.Tone.memory,
                bytes: sumItemBytes(oldUnused),
                itemCount: oldUnused.count,
                confidence: oldUnused.isEmpty ? 0 : 52,
                sampleItems: Array(oldUnused.prefix(4)),
                stageItems: oldUnused.filter(storageItemIsTrashActionable),
                growthEvents: [],
                cleanupBundle: nil
            ),
            StorageHomeAction(
                id: "risky-review",
                title: "Risky Review",
                detail: "Source-like, protected, dirty, recent, or manually-blocked candidates.",
                consequence: "No unattended cleanup. Review blockers, reveal paths, and decide outside automatic reclaim.",
                systemImage: "exclamationmark.triangle",
                tone: AetowerDesign.Status.error,
                bytes: sumItemBytes(risky),
                itemCount: risky.count,
                confidence: risky.isEmpty ? 0 : 30,
                sampleItems: Array(risky.prefix(4)),
                stageItems: [],
                growthEvents: [],
                cleanupBundle: nil
            ),
        ]
    }

    private func sumItemBytes(_ items: [StorageHygieneItemModel]) -> UInt64 {
        items.reduce(UInt64(0)) { total, item in
            let sum = total.addingReportingOverflow(item.sizeBytes)
            return sum.overflow ? UInt64.max : sum.partialValue
        }
    }

    private func sumBytes(_ left: UInt64, _ right: UInt64) -> UInt64 {
        let sum = left.addingReportingOverflow(right)
        return sum.overflow ? UInt64.max : sum.partialValue
    }

    private func similarityReviewSummary(
        for report: StorageHygieneReportModel
    ) -> StorageSimilarityReviewSummary {
        let otherRedundancyGroups = otherRedundancyGroups(from: report)
        let activeDuplicateGroups = report.duplicateGroups.filter {
            !ignoredSimilarityGroupIDs.contains(duplicateGroupActionKey($0))
        }
        let exactGroups = duplicateGroups(for: .exactDuplicates, in: report)
        let fuzzyGroups = activeDuplicateGroups.filter { !$0.confirmed && $0.detectorKind != .exact }
        let duplicateBytes = sumDuplicateGroupBytes(activeDuplicateGroups)
        let otherRedundancyBytes = sumRedundancyGroupBytes(otherRedundancyGroups)
        return StorageSimilarityReviewSummary(
            duplicateGroupCount: activeDuplicateGroups.count,
            otherRedundancyGroupCount: otherRedundancyGroups.count,
            exactGroupCount: exactGroups.count,
            fuzzyGroupCount: fuzzyGroups.count,
            exactBytes: sumDuplicateGroupBytes(exactGroups),
            otherRedundancyBytes: otherRedundancyBytes,
            reviewableBytes: sumBytes(duplicateBytes, otherRedundancyBytes)
        )
    }

    private func duplicateGroups(
        for filter: StorageSimilarityFilter,
        in report: StorageHygieneReportModel,
        includeIgnored: Bool = false
    ) -> [StorageDuplicateGroupModel] {
        let groups = report.duplicateGroups.filter { group in
            includeIgnored || !ignoredSimilarityGroupIDs.contains(duplicateGroupActionKey(group))
        }
        switch filter {
        case .exactDuplicates:
            return groups.filter { $0.confirmed || $0.detectorKind == .exact }
        case .similarImages:
            return groups.filter { $0.detectorKind == .imageSimilarity }
        case .similarDocumentsText:
            return groups.filter {
                $0.detectorKind == .documentSimilarity || $0.detectorKind == .textSimilarity
            }
        case .similarVideos:
            return groups.filter { $0.detectorKind == .videoSimilarity }
        case .similarBinaries:
            return groups.filter { $0.detectorKind == .binarySimilarity }
        case .otherRedundancy:
            return []
        }
    }

    private func otherRedundancyGroups(
        from report: StorageHygieneReportModel,
        includeIgnored: Bool = false
    ) -> [StorageRedundancyGroupModel] {
        report.redundancyGroups.filter { group in
            group.redundancyClass != "byte-duplicates"
                && !group.id.hasPrefix("byte-duplicates|")
                && (includeIgnored || !ignoredSimilarityGroupIDs.contains(redundancyGroupActionKey(group)))
        }
    }

    private func sortedDuplicateGroups(
        _ groups: [StorageDuplicateGroupModel]
    ) -> [StorageDuplicateGroupModel] {
        groups.sorted { left, right in
            if left.reclaimableBytes != right.reclaimableBytes {
                return left.reclaimableBytes > right.reclaimableBytes
            }
            if left.totalBytes != right.totalBytes {
                return left.totalBytes > right.totalBytes
            }
            if left.confidenceScore != right.confidenceScore {
                return left.confidenceScore > right.confidenceScore
            }
            return left.id < right.id
        }
    }

    private func sortedRedundancyGroups(
        _ groups: [StorageRedundancyGroupModel]
    ) -> [StorageRedundancyGroupModel] {
        groups.sorted { left, right in
            if left.reclaimableBytes != right.reclaimableBytes {
                return left.reclaimableBytes > right.reclaimableBytes
            }
            if left.totalBytes != right.totalBytes {
                return left.totalBytes > right.totalBytes
            }
            if left.confidenceScore != right.confidenceScore {
                return left.confidenceScore > right.confidenceScore
            }
            return left.id < right.id
        }
    }

    private func similarityFilterCount(
        _ filter: StorageSimilarityFilter,
        in report: StorageHygieneReportModel
    ) -> Int {
        if filter == .otherRedundancy {
            return otherRedundancyGroups(from: report).count
        }
        return duplicateGroups(for: filter, in: report).count
    }

    private func similarityFilterBytes(
        _ filter: StorageSimilarityFilter,
        in report: StorageHygieneReportModel
    ) -> UInt64 {
        if filter == .otherRedundancy {
            return sumRedundancyGroupBytes(otherRedundancyGroups(from: report))
        }
        return sumDuplicateGroupBytes(duplicateGroups(for: filter, in: report))
    }

    private func preferredSimilarityFilter(
        for report: StorageHygieneReportModel
    ) -> StorageSimilarityFilter {
        StorageSimilarityFilter.allCases.first { filter in
            similarityFilterCount(filter, in: report) > 0
        } ?? .exactDuplicates
    }

    private func sumDuplicateGroupBytes(_ groups: [StorageDuplicateGroupModel]) -> UInt64 {
        groups.reduce(UInt64(0)) { total, group in
            sumBytes(total, group.reclaimableBytes)
        }
    }

    private func sumRedundancyGroupBytes(_ groups: [StorageRedundancyGroupModel]) -> UInt64 {
        groups.reduce(UInt64(0)) { total, group in
            sumBytes(total, group.reclaimableBytes)
        }
    }

    private func duplicateGroupTitle(_ group: StorageDuplicateGroupModel) -> String {
        switch group.detectorKind {
        case .exact:
            return "Exact duplicates"
        case .imageSimilarity:
            return "Similar images"
        case .textSimilarity:
            return "Similar text files"
        case .documentSimilarity:
            return "Similar documents"
        case .videoSimilarity:
            return "Similar videos"
        case .binarySimilarity:
            return group.confirmed ? "Exact duplicates" : "Similar binaries"
        case .unknown:
            return group.confirmed ? "Exact duplicates" : "Similar files"
        }
    }

    private func duplicateDetectorLabel(_ detectorKind: StorageDuplicateDetectorKindModel) -> String {
        switch detectorKind {
        case .exact:
            return "Exact"
        case .imageSimilarity:
            return "Image"
        case .textSimilarity:
            return "Text"
        case .documentSimilarity:
            return "Document"
        case .videoSimilarity:
            return "Video"
        case .binarySimilarity:
            return "Binary"
        case .unknown:
            return "Unknown"
        }
    }

    private func duplicateDetectorIcon(_ detectorKind: StorageDuplicateDetectorKindModel) -> String {
        switch detectorKind {
        case .exact:
            return "equal.square"
        case .imageSimilarity:
            return "photo.on.rectangle"
        case .textSimilarity:
            return "doc.text"
        case .documentSimilarity:
            return "doc.richtext"
        case .videoSimilarity:
            return "film"
        case .binarySimilarity:
            return "shippingbox"
        case .unknown:
            return "questionmark.square"
        }
    }

    private func duplicateGroupIcon(_ group: StorageDuplicateGroupModel) -> String {
        duplicateDetectorIcon(group.detectorKind)
    }

    private func duplicateGroupTone(_ group: StorageDuplicateGroupModel) -> Color {
        if group.actionability == .cleanableExact || group.confirmed {
            return AetowerDesign.Status.ready
        }
        switch group.confidenceBand {
        case .high:
            return AetowerDesign.Status.warning
        case .medium:
            return AetowerDesign.Tone.energy
        case .low:
            return AetowerDesign.Status.error
        case .confirmed:
            return AetowerDesign.Status.ready
        case .unknown:
            return AetowerDesign.Status.neutral
        }
    }

    private func redundancyGroupTone(_ group: StorageRedundancyGroupModel) -> Color {
        switch group.safety.lowercased() {
        case "safe":
            return AetowerDesign.Status.ready
        case "review":
            return AetowerDesign.Status.warning
        case "risky":
            return AetowerDesign.Status.error
        default:
            return group.confidenceScore >= 80 ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
        }
    }

    private func duplicateConfidenceLabel(
        _ confidenceBand: StorageDuplicateConfidenceBandModel
    ) -> String {
        switch confidenceBand {
        case .confirmed:
            return "Confirmed"
        case .high:
            return "High confidence"
        case .medium:
            return "Medium confidence"
        case .low:
            return "Low confidence"
        case .unknown:
            return "Unknown confidence"
        }
    }

    private func duplicateGroupReviewWarning(_ group: StorageDuplicateGroupModel) -> String? {
        switch group.detectorKind {
        case .videoSimilarity:
            return "Lower-confidence video match: metadata and sampled bytes are not decoded-frame proof."
        case .binarySimilarity:
            if group.confirmed {
                return nil
            }
            return "Lower-confidence binary match: fuzzy chunk fingerprints can produce false positives."
        default:
            if group.confidenceBand == .low {
                return "Low-confidence match. Confirm the files manually before cleanup."
            }
            return nil
        }
    }

    private func duplicateGroupHasTextDocumentAffordances(_ group: StorageDuplicateGroupModel) -> Bool {
        group.detectorKind == .textSimilarity || group.detectorKind == .documentSimilarity
    }

    private func duplicateGroupActionKey(_ group: StorageDuplicateGroupModel) -> String {
        "duplicate|\(group.id)"
    }

    private func redundancyGroupActionKey(_ group: StorageRedundancyGroupModel) -> String {
        "redundancy|\(group.id)"
    }

    private func recordDuplicateGroupImpression(_ group: StorageDuplicateGroupModel) {
        let key = duplicateGroupActionKey(group)
        if similarityTelemetryViewedGroupKeys.insert(key).inserted {
            recordSimilarityAction("group-viewed", duplicateGroup: group)
        }
    }

    private func recordRedundancyGroupImpression(_ group: StorageRedundancyGroupModel) {
        let key = redundancyGroupActionKey(group)
        if similarityTelemetryViewedGroupKeys.insert(key).inserted {
            recordSimilarityAction("group-viewed", redundancyGroup: group)
        }
    }

    private func similarityGroupIsExpanded(_ key: String) -> Bool {
        expandedSimilarityGroupKeys.contains(key)
    }

    private func toggleDuplicateGroupExpansion(_ group: StorageDuplicateGroupModel) {
        let key = duplicateGroupActionKey(group)
        if expandedSimilarityGroupKeys.contains(key) {
            expandedSimilarityGroupKeys.remove(key)
        } else {
            expandedSimilarityGroupKeys.insert(key)
            recordSimilarityAction("group-expanded", duplicateGroup: group)
        }
    }

    private func toggleRedundancyGroupExpansion(_ group: StorageRedundancyGroupModel) {
        let key = redundancyGroupActionKey(group)
        if expandedSimilarityGroupKeys.contains(key) {
            expandedSimilarityGroupKeys.remove(key)
        } else {
            expandedSimilarityGroupKeys.insert(key)
            recordSimilarityAction("group-expanded", redundancyGroup: group)
        }
    }

    private func recordSimilarityOverviewImpression(
        _ report: StorageHygieneReportModel,
        summary: StorageSimilarityReviewSummary
    ) {
        let key = "overview|\(report.capturedAtMillis)"
        if similarityTelemetryViewedSurfaceKeys.insert(key).inserted {
            recordSimilaritySurfaceAction("overview-viewed", summary: summary)
        }
    }

    private func recordSimilarityReviewSectionImpression(
        _ report: StorageHygieneReportModel,
        summary: StorageSimilarityReviewSummary
    ) {
        let key = "review|\(report.capturedAtMillis)"
        if similarityTelemetryViewedSurfaceKeys.insert(key).inserted {
            recordSimilaritySurfaceAction(
                "review-section-viewed",
                summary: summary,
                extraFields: [(key: "selected_filter", value: selectedSimilarityFilter.rawValue)]
            )
        }
    }

    private func recordSimilaritySurfaceAction(
        _ action: String,
        summary: StorageSimilarityReviewSummary,
        extraFields: [(key: String, value: String)] = []
    ) {
        state.recordStorageSimilarityActionDiagnostics(
            action: action,
            groupKind: "surface",
            groupFingerprint: similarityTelemetryFingerprint(
                "surface|\(summary.groupCount)|\(summary.reviewableBytes)"
            ),
            detectorKind: "mixed",
            actionability: summary.exactGroupCount > 0 ? "mixed" : "review_only",
            confidenceBand: "mixed",
            confidenceScore: 0,
            itemCount: summary.groupCount,
            totalBytes: summary.reviewableBytes,
            reclaimableBytes: summary.reviewableBytes,
            extraFields: [
                (key: "exact_group_count", value: String(summary.exactGroupCount)),
                (key: "fuzzy_group_count", value: String(summary.fuzzyGroupCount)),
                (key: "redundancy_group_count", value: String(summary.otherRedundancyGroupCount)),
            ] + extraFields
        )
    }

    private func recordSimilarityAction(
        _ action: String,
        duplicateGroup group: StorageDuplicateGroupModel,
        extraFields: [(key: String, value: String)] = [],
        warning: Bool = false
    ) {
        state.recordStorageSimilarityActionDiagnostics(
            action: action,
            groupKind: "duplicate",
            groupFingerprint: similarityTelemetryFingerprint(duplicateGroupActionKey(group)),
            detectorKind: group.detectorKind.rawValue,
            actionability: group.actionability.rawValue,
            confidenceBand: group.confidenceBand.rawValue,
            confidenceScore: group.confidenceScore,
            itemCount: group.fileCount,
            totalBytes: group.totalBytes,
            reclaimableBytes: group.reclaimableBytes,
            extraFields: extraFields,
            warning: warning
        )
    }

    private func recordSimilarityAction(
        _ action: String,
        redundancyGroup group: StorageRedundancyGroupModel,
        extraFields: [(key: String, value: String)] = [],
        warning: Bool = false
    ) {
        state.recordStorageSimilarityActionDiagnostics(
            action: action,
            groupKind: "redundancy",
            groupFingerprint: similarityTelemetryFingerprint(redundancyGroupActionKey(group)),
            detectorKind: "other_redundancy:\(similarityTelemetryToken(group.redundancyClass))",
            actionability: "review_only",
            confidenceBand: similarityConfidenceBand(score: group.confidenceScore),
            confidenceScore: group.confidenceScore,
            itemCount: group.itemCount,
            totalBytes: group.totalBytes,
            reclaimableBytes: group.reclaimableBytes,
            extraFields: extraFields,
            warning: warning
        )
    }

    private func similarityConfidenceBand(score: UInt8) -> String {
        if score >= 80 {
            return "high"
        }
        if score >= 60 {
            return "medium"
        }
        return "low"
    }

    private func similarityTelemetryToken(_ value: String) -> String {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !trimmed.isEmpty else { return "unknown" }
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "_-"))
        return trimmed.unicodeScalars.map { scalar in
            allowed.contains(scalar) ? String(scalar) : "_"
        }.joined()
    }

    private func similarityTelemetryFingerprint(_ value: String) -> String {
        var hash: UInt64 = 14_695_981_039_346_656_037
        for byte in value.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 1_099_511_628_211
        }
        return String(format: "%016llx", CUnsignedLongLong(hash))
    }

    private func markSimilarityGroupReviewed(_ key: String) {
        reviewedSimilarityGroupIDs.insert(key)
    }

    private func unmarkSimilarityGroupReviewed(_ key: String) {
        reviewedSimilarityGroupIDs.remove(key)
    }

    private func ignoreSimilarityGroup(_ key: String) {
        ignoredSimilarityGroupIDs.insert(key)
    }

    private func restoreSimilarityGroup(_ key: String) {
        ignoredSimilarityGroupIDs.remove(key)
    }

    private func copyPaths(_ paths: [String]) {
        copy(paths.joined(separator: "\n"))
    }

    private func reveal(paths: [String]) {
        let urls = paths.map { URL(fileURLWithPath: NSString(string: $0).expandingTildeInPath) }
        guard !urls.isEmpty else { return }
        NSWorkspace.shared.activateFileViewerSelecting(urls)
    }

    private func duplicateGroupCanStageExactCleanup(_ group: StorageDuplicateGroupModel) -> Bool {
        group.actions.canStageCleanup
            && group.paths.count >= 2
            && group.confirmed
            && group.detectorKind == .exact
            && group.actionability == .cleanableExact
    }

    private func duplicateGroupCanonicalItem(_ group: StorageDuplicateGroupModel) -> StorageDuplicateItemModel? {
        if let path = duplicateCanonicalPathByGroupID[group.id],
           let item = group.paths.first(where: { $0.path == path }) {
            return item
        }
        return group.paths.first
    }

    private func duplicateCanonicalSelection(
        for group: StorageDuplicateGroupModel
    ) -> Binding<String> {
        Binding(
            get: {
                duplicateCanonicalPathByGroupID[group.id] ?? group.paths.first?.path ?? ""
            },
            set: { newValue in
                duplicateCanonicalPathByGroupID[group.id] = newValue
            }
        )
    }

    private func duplicateGroupStageableItems(
        _ group: StorageDuplicateGroupModel,
        canonical: StorageDuplicateItemModel
    ) -> [StorageDuplicateItemModel] {
        group.paths.filter { $0.path != canonical.path }
    }

    private func duplicateGroupReviewOnlyReason(_ group: StorageDuplicateGroupModel) -> String {
        if !group.actions.canStageCleanup,
           let blockReason = group.actions.blockReason,
           !blockReason.isEmpty {
            return "Review-only: \(blockReason)"
        }
        switch group.detectorKind {
        case .imageSimilarity:
            return "Review-only: perceptual image hashes can group edited, cropped, or different originals."
        case .textSimilarity, .documentSimilarity:
            return "Review-only: text similarity ignores formatting and may miss important appended, reordered, or scanned content."
        case .videoSimilarity:
            return "Review-only: video matching uses metadata and sampled bytes, not decoded-frame equivalence."
        case .binarySimilarity:
            return "Review-only: binary similarity uses fuzzy chunk fingerprints and can produce false positives."
        case .exact:
            return "Exact cleanup is unavailable until at least one canonical and one duplicate path are present."
        case .unknown:
            return "Review-only: detector type is unknown, so Aetower will not stage cleanup automatically."
        }
    }

    private func stageExactDuplicateGroup(
        _ group: StorageDuplicateGroupModel,
        canonical: StorageDuplicateItemModel
    ) {
        guard duplicateGroupCanStageExactCleanup(group) else {
            recordSimilarityAction(
                "deletion-blocked",
                duplicateGroup: group,
                extraFields: [(key: "blocked_reason", value: "not-cleanable-exact")],
                warning: true
            )
            return
        }
        var staged = 0
        let stageableItems = duplicateGroupStageableItems(group, canonical: canonical)
        guard !stageableItems.isEmpty else {
            recordSimilarityAction(
                "deletion-blocked",
                duplicateGroup: group,
                extraFields: [(key: "blocked_reason", value: "no-non-canonical-copy")],
                warning: true
            )
            return
        }
        for item in stageableItems {
            let basketItem = StorageCleanupBasketItem(
                id: "exact-duplicate|\(group.id)|\(item.path)",
                title: item.displayName,
                path: item.path,
                source: "exact duplicate",
                cleanupTier: normalizedDuplicateCleanupTier(item.cleanupTier),
                safety: normalizedDuplicateSafety(item.safety),
                estimatedBytes: item.sizeBytes,
                reason: "Confirmed byte-identical duplicate. Canonical retained: \(canonical.path)",
                consequence: "Aetower will move only the non-canonical duplicate to Finder Trash. The retained canonical path is not staged.",
                evidence: [
                    "Detector: \(duplicateDetectorLabel(group.detectorKind))",
                    "Candidate key: \(group.candidateKey)",
                    "Confidence: \(group.confidenceScore)%",
                    "Canonical retained: \(canonical.path)",
                    "Duplicate staged: \(item.path)",
                ],
                requiresReview: true,
                blockers: [],
                prerequisites: [
                    "Quick Look the canonical file and each staged duplicate before moving the basket to Trash.",
                    "Confirm no application or workflow depends on the duplicate path."
                ]
            )
            if stageBasketItem(basketItem) {
                staged += 1
            }
        }
        if staged > 0 {
            recordSimilarityAction(
                "exact-duplicate-staged",
                duplicateGroup: group,
                extraFields: [
                    (key: "stage_attempted_count", value: String(stageableItems.count)),
                    (key: "stage_count", value: String(staged)),
                    (key: "canonical_selected", value: "true"),
                ]
            )
        }
        let blocked = stageableItems.count - staged
        if blocked > 0 {
            recordSimilarityAction(
                "deletion-blocked",
                duplicateGroup: group,
                extraFields: [
                    (key: "blocked_reason", value: "stage-rejected"),
                    (key: "blocked_count", value: String(blocked)),
                    (key: "stage_attempted_count", value: String(stageableItems.count)),
                    (key: "stage_count", value: String(staged)),
                ],
                warning: true
            )
        }
        if staged > 0 {
            presentCleanupExecution(basketTrashExecutionRequest())
        }
    }

    private func normalizedDuplicateCleanupTier(_ cleanupTier: String) -> String {
        cleanupTier.isEmpty ? "review" : cleanupTier
    }

    private func normalizedDuplicateSafety(_ safety: String) -> String {
        safety.isEmpty ? "review" : safety
    }

    private func similarDuplicateItemMetadata(_ item: StorageDuplicateItemModel) -> String {
        [
            fileKindLabel(item.path),
            modifiedDateLabel(item.modifiedMillis),
            cleanupTierLabel(item.cleanupTier),
        ]
        .filter { !$0.isEmpty }
        .joined(separator: " - ")
    }

    private func similarRedundancyItemMetadata(_ item: StorageRedundancyItemModel) -> String {
        [
            item.kind.replacingOccurrences(of: "-", with: " "),
            item.role.replacingOccurrences(of: "-", with: " "),
            cleanupTierLabel(item.cleanupTier),
            item.safety,
        ]
        .filter { !$0.isEmpty }
        .joined(separator: " - ")
    }

    private func fileKindLabel(_ path: String) -> String {
        let ext = URL(fileURLWithPath: path).pathExtension
        return ext.isEmpty ? "no extension" : ext.uppercased()
    }

    private func modifiedDateLabel(_ modifiedMillis: UInt64?) -> String {
        guard let modifiedMillis else {
            return "modified unknown"
        }
        let date = Date(timeIntervalSince1970: Double(modifiedMillis) / 1000.0)
        return "modified \(date.formatted(date: .abbreviated, time: .omitted))"
    }

    private func openDuplicateGroupFirstPair(_ group: StorageDuplicateGroupModel) {
        for item in group.paths.prefix(2) {
            openPath(path: item.path)
        }
    }

    private func compareDuplicateGroupFirstPair(_ group: StorageDuplicateGroupModel) {
        comparePaths(Array(group.paths.prefix(2)).map(\.path))
    }

    private func diffCommand(for paths: [String]) -> String {
        guard paths.count >= 2 else {
            return ""
        }
        return "diff -u \(shellQuotedPath(paths[0])) \(shellQuotedPath(paths[1]))"
    }

    private func sumGrowthBytes(_ events: [StorageGrowthTimelineEvent]) -> UInt64 {
        events.reduce(UInt64(0)) { total, event in
            let sum = total.addingReportingOverflow(UInt64(max(0, event.deltaBytes)))
            return sum.overflow ? UInt64.max : sum.partialValue
        }
    }

    private func storageItemSizeSort(_ left: StorageHygieneItemModel, _ right: StorageHygieneItemModel) -> Bool {
        left.sizeBytes == right.sizeBytes ? left.path < right.path : left.sizeBytes > right.sizeBytes
    }

    private func isDeveloperStorageArtifact(_ item: StorageHygieneItemModel) -> Bool {
        matches(
            item.storageRole,
            anyOf: ["build-artifact", "cache", "dependency-tree", "environment", "temporary", "log"]
        ) || matches(
            item.kind,
            anyOf: [
                "rust", "swift", "xcode", "node", "python", "frontend", "coverage", "tool-cache",
                "docker", "npm", "pnpm", "yarn", "test-output", "release-artifact",
            ]
        )
    }

    private func isOldUnusedStorageItem(_ item: StorageHygieneItemModel) -> Bool {
        item.cold || (item.accessAgeDays ?? 0) >= 365 || (item.ageDays ?? 0) >= 365
    }

    private func itemsMatchingGrowthEvents(
        _ events: [StorageGrowthTimelineEvent],
        in items: [StorageHygieneItemModel]
    ) -> [StorageHygieneItemModel] {
        let eventIDs = Set(events.map(\.id))
        let eventPaths = Set(events.map(\.path))
        return items.filter { eventIDs.contains($0.id) || eventPaths.contains($0.path) }
    }

    private func matches(_ value: String, anyOf needles: [String]) -> Bool {
        let lowered = value.lowercased()
        return needles.contains { lowered.contains($0) }
    }

    private func storageAdvanced(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            visualExplorationSection(report)
            itemSection(report)
            storageScanDiagnosticsSection(report)
            storageCoverageOverview(report)
            volumeStateSection(report)
            wholeComputerOptimizationSection(report)
            repoFootprintDashboard(report)
            storageGrowthTimeline(report)
            cleanupPreviewSection(report)
            cleanupBundlesSection(report)
            cleanupRecipesSection(report)
            cleanupAuditSection
            summaryGrid(report)
            if report.truncated {
                warningBanner("The scan hit a cap or time budget. Results are partial; narrow the root or refresh when the machine is idle.")
            }
            rootsSection(report)
            caveatsSection(report)
        }
    }

    private func storageScanDiagnosticsSection(_ report: StorageHygieneReportModel) -> some View {
        let budget = report.diagnostics.performanceBudget
        let budgetTone = storageBudgetTone(budget?.status)
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            advancedSectionLabel(
                title: "Scan diagnostics",
                detail: "Latency, payload, table-page, index, and budget signals for the current Storage projection.",
                systemImage: "waveform.path.ecg.rectangle"
            )

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 170), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "Budget",
                    value: (budget?.status ?? "unknown").uppercased(),
                    detail: budget?.notes.first ?? "No budget data in this scan"
                )
                footprintMetric(
                    "Scan latency",
                    value: "\(budget?.scanJobLatencyMillis ?? report.scanDurationMillis) ms",
                    detail: "job duration"
                )
                footprintMetric(
                    "Payload",
                    value: formatBytes(budget?.payloadBytes ?? report.diagnostics.payloadBytes),
                    detail: "Swift decode \(report.diagnostics.decodeMillis) ms"
                )
                footprintMetric(
                    "Table page",
                    value: "\(budget?.tablePageMillis ?? 0) ms",
                    detail: "sort/page budget \(budget?.tablePageBudgetMillis ?? 0) ms"
                )
                footprintMetric(
                    "Root walk",
                    value: "\(report.diagnostics.rootWalkMillis) ms",
                    detail: "\(report.diagnostics.scannedDirectoryCount) dirs"
                )
                footprintMetric(
                    "Size walk",
                    value: "\(report.diagnostics.sizeWalkMillis) ms",
                    detail: "\(report.diagnostics.sizedEntryCount) entries"
                )
                footprintMetric(
                    "Index",
                    value: report.diagnostics.storageIndexStatus,
                    detail: "\(report.diagnostics.storageIndexHits) hit / \(report.diagnostics.storageIndexMisses) miss"
                )
                footprintMetric(
                    "Retained",
                    value: "\(report.diagnostics.candidateRetainedCount)",
                    detail: "\(report.diagnostics.candidateSeenCount) candidates seen"
                )
            }

            if let budget, !budget.notes.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(budget.notes.prefix(4)), id: \.self) { note in
                        Label(note, systemImage: "speedometer")
                            .font(.caption2)
                            .foregroundStyle(budgetTone)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func wholeComputerOptimizationSection(_ report: StorageHygieneReportModel) -> some View {
        let items = visibleStorageItems(from: report)
        let largeFiles = items
            .filter { $0.kind == "large-file" || $0.kind == "release-artifact" }
            .sorted(by: storageItemSizeSort)
        let oldUnused = items
            .filter(isOldUnusedStorageItem)
            .sorted(by: storageItemSizeSort)
        let systemBytes = report.systemDataBuckets.reduce(UInt64(0)) { total, bucket in
            sumBytes(total, bucket.sizeBytes)
        }
        let similaritySummary = similarityReviewSummary(for: report)

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "sparkles.square.filled.on.square")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Whole-computer optimization")
                        .font(.headline)
                    Text("Large files, cold data, potentially similar files, app footprints, and macOS System Data buckets from the current bounded scan.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                AetowerBadge(
                    "\(largeFiles.count + oldUnused.count + similaritySummary.groupCount + report.appFootprints.count) leads",
                    tone: AetowerDesign.Tone.disk
                )
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 145), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric("Large files", value: "\(largeFiles.count)", detail: largeFiles.first.map { formatBytes($0.sizeBytes) } ?? "none")
                footprintMetric("Old unused", value: "\(oldUnused.count)", detail: oldUnused.first.map { formatBytes($0.sizeBytes) } ?? "none")
                footprintMetric("Similar", value: "\(similaritySummary.groupCount)", detail: formatBytes(similaritySummary.reviewableBytes))
                footprintMetric("Apps", value: "\(report.appFootprints.count)", detail: report.appFootprints.first.map { formatBytes($0.totalBytes) } ?? "none")
                footprintMetric("System Data", value: formatBytes(systemBytes), detail: "\(report.systemDataBuckets.filter { $0.sizeBytes > 0 }.count) active bucket\(report.systemDataBuckets.filter { $0.sizeBytes > 0 }.count == 1 ? "" : "s")")
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 300), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                wholeComputerItemCard(
                    title: "Large file finder",
                    detail: "Largest standalone files and release packages. Large does not mean safe.",
                    empty: "No large standalone files in the retained scan candidates.",
                    items: Array(largeFiles.prefix(4))
                )
                wholeComputerItemCard(
                    title: "Old unused finder",
                    detail: "Cold or long-unmodified files. Review personal data before reclaiming.",
                    empty: "No cold files in the retained scan candidates.",
                    items: Array(oldUnused.prefix(4))
                )
                appFootprintsCard(report.appFootprints)
                systemDataBucketsCard(report.systemDataBuckets)
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func wholeComputerItemCard(
        title: String,
        detail: String,
        empty: String,
        items: [StorageHygieneItemModel]
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text(title)
                .font(.subheadline.weight(.semibold))
            Text(detail)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if items.isEmpty {
                Label(empty, systemImage: "checkmark.circle")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            } else {
                ForEach(items) { item in
                    compactStorageItemActionRow(item)
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 210, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    @ViewBuilder
    private func similarFilesOverviewSection(_ report: StorageHygieneReportModel) -> some View {
        let summary = similarityReviewSummary(for: report)
        if summary.groupCount > 0 {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                    Image(systemName: "doc.on.doc")
                        .foregroundStyle(AetowerDesign.Status.warning)
                        .frame(width: 24)
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Similar files review")
                            .font(.headline)
                        Text("Detector and redundancy groups need a human decision before cleanup.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer(minLength: AetowerDesign.Spacing.md)
                    AetowerBadge(
                        "\(summary.groupCount) group\(summary.groupCount == 1 ? "" : "s")",
                        tone: AetowerDesign.Status.warning
                    )
                }

                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 145), spacing: AetowerDesign.Spacing.sm)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.sm
                ) {
                    footprintMetric("Reviewable", value: formatBytes(summary.reviewableBytes), detail: "across all groups")
                    footprintMetric("Exact", value: "\(summary.exactGroupCount)", detail: formatBytes(summary.exactBytes))
                    footprintMetric("Fuzzy", value: "\(summary.fuzzyGroupCount)", detail: "detector groups")
                    footprintMetric("Redundancy", value: "\(summary.otherRedundancyGroupCount)", detail: formatBytes(summary.otherRedundancyBytes))
                }

                HStack {
                    Button {
                        recordSimilaritySurfaceAction(
                            "overview-review-clicked",
                            summary: summary,
                            extraFields: [
                                (
                                    key: "next_filter",
                                    value: preferredSimilarityFilter(for: report).rawValue
                                ),
                            ]
                        )
                        selectedSimilarityFilter = preferredSimilarityFilter(for: report)
                        selectedSection = .explore
                    } label: {
                        Label("Review groups", systemImage: "magnifyingglass")
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    Spacer()
                }
            }
            .padding(AetowerDesign.Spacing.lg)
            .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .onAppear {
                recordSimilarityOverviewImpression(report, summary: summary)
            }
        }
    }

    private func similarFilesReviewSection(_ report: StorageHygieneReportModel) -> some View {
        let summary = similarityReviewSummary(for: report)
        let selectedCount = similarityFilterCount(selectedSimilarityFilter, in: report)
        let selectedBytes = similarityFilterBytes(selectedSimilarityFilter, in: report)

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "rectangle.stack.badge.person.crop")
                    .foregroundStyle(AetowerDesign.Status.warning)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Similar files review")
                        .font(.title3.weight(.semibold))
                    Text("Exact duplicates, fuzzy media/document matches, and non-duplicate redundancy findings separated by detector.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                VStack(alignment: .trailing, spacing: 4) {
                    AetowerBadge(
                        "\(summary.groupCount) group\(summary.groupCount == 1 ? "" : "s")",
                        tone: summary.groupCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
                    )
                    Text(formatBytes(summary.reviewableBytes))
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 145), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric("Shown", value: "\(selectedCount)", detail: selectedSimilarityFilter.label)
                footprintMetric("Reviewable", value: formatBytes(selectedBytes), detail: "selected tab")
                footprintMetric("Exact groups", value: "\(summary.exactGroupCount)", detail: formatBytes(summary.exactBytes))
                footprintMetric("Other redundancy", value: "\(summary.otherRedundancyGroupCount)", detail: formatBytes(summary.otherRedundancyBytes))
            }

            Picker("Similar files", selection: $selectedSimilarityFilter) {
                ForEach(StorageSimilarityFilter.allCases) { filter in
                    Text(filter.label).tag(filter)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Toggle("Show ignored", isOn: $showIgnoredSimilarityGroups)
                    .toggleStyle(.checkbox)
                    .font(.caption)
                if !ignoredSimilarityGroupIDs.isEmpty {
                    AetowerBadge(
                        "\(ignoredSimilarityGroupIDs.count) ignored",
                        tone: AetowerDesign.Status.neutral
                    )
                }
                Spacer()
            }

            similarFilesFilteredContent(report, filter: selectedSimilarityFilter)
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .onAppear {
            recordSimilarityReviewSectionImpression(report, summary: summary)
        }
    }

    @ViewBuilder
    private func similarFilesFilteredContent(
        _ report: StorageHygieneReportModel,
        filter: StorageSimilarityFilter
    ) -> some View {
        let groupDisplayLimit = 40
        if filter == .otherRedundancy {
            let groups = sortedRedundancyGroups(
                otherRedundancyGroups(from: report, includeIgnored: showIgnoredSimilarityGroups)
            )
            if groups.isEmpty {
                similarFilesEmptyState(filter)
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(groups.prefix(groupDisplayLimit)) { group in
                        similarRedundancyGroupRow(group)
                    }
                    if groups.count > groupDisplayLimit {
                        similarFilesLimitBanner(
                            shown: groupDisplayLimit,
                            total: groups.count,
                            filter: filter
                        )
                    }
                }
            }
        } else {
            let groups = sortedDuplicateGroups(
                duplicateGroups(for: filter, in: report, includeIgnored: showIgnoredSimilarityGroups)
            )
            if groups.isEmpty {
                similarFilesEmptyState(filter)
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(groups.prefix(groupDisplayLimit)) { group in
                        similarDuplicateGroupRow(group)
                    }
                    if groups.count > groupDisplayLimit {
                        similarFilesLimitBanner(
                            shown: groupDisplayLimit,
                            total: groups.count,
                            filter: filter
                        )
                    }
                }
            }
        }
    }

    private func similarFilesLimitBanner(
        shown: Int,
        total: Int,
        filter: StorageSimilarityFilter
    ) -> some View {
        AetowerInfoBanner(
            "Showing the first \(shown) \(filter.emptyStateLabel). Narrow the scan root or hide reviewed groups to inspect the remaining \(max(0, total - shown)).",
            systemImage: "rectangle.stack.badge.plus",
            tone: AetowerDesign.Status.neutral,
            level: .card
        )
    }

    private func similarDuplicateGroupRow(_ group: StorageDuplicateGroupModel) -> some View {
        let tone = duplicateGroupTone(group)
        let groupKey = duplicateGroupActionKey(group)
        let isExpanded = similarityGroupIsExpanded(groupKey)
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: duplicateGroupIcon(group))
                    .foregroundStyle(tone)
                    .frame(width: 22)
                VStack(alignment: .leading, spacing: 3) {
                    Text(duplicateGroupTitle(group))
                        .font(.subheadline.weight(.semibold))
                    Text("\(group.fileCount) file\(group.fileCount == 1 ? "" : "s") - \(group.confidenceScore)% confidence")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(formatBytes(group.reclaimableBytes))
                        .font(.system(size: 18, weight: .semibold, design: .rounded))
                    Text("reclaimable estimate")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Text(formatBytes(group.totalBytes))
                        .font(.caption.weight(.semibold))
                    Text("total bytes")
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                }
            }

            HStack(spacing: AetowerDesign.Spacing.xs) {
                AetowerBadge(duplicateDetectorLabel(group.detectorKind), systemImage: duplicateDetectorIcon(group.detectorKind), tone: tone)
                AetowerBadge(duplicateConfidenceLabel(group.confidenceBand), tone: tone)
                AetowerBadge(group.actionability == .cleanableExact ? "Cleanable exact" : "Review-only", tone: tone)
                if group.confirmed {
                    AetowerBadge("Confirmed", tone: AetowerDesign.Status.ready)
                }
                if reviewedSimilarityGroupIDs.contains(groupKey) {
                    AetowerBadge("Reviewed", tone: AetowerDesign.Status.ready)
                }
                if ignoredSimilarityGroupIDs.contains(groupKey) {
                    AetowerBadge("Ignored", tone: AetowerDesign.Status.neutral)
                }
                Spacer(minLength: 0)
            }

            similarDuplicateGroupActions(group)

            if isExpanded {
                if !group.recommendation.isEmpty {
                    Text(group.recommendation)
                        .font(.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                if !group.caveat.isEmpty {
                    Text(group.caveat)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                if let warning = duplicateGroupReviewWarning(group) {
                    Label(warning, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(duplicateGroupTone(group))
                        .fixedSize(horizontal: false, vertical: true)
                }

                if group.detectorKind == .imageSimilarity {
                    similarImageContactSheet(group)
                }

                if duplicateGroupHasTextDocumentAffordances(group) {
                    similarTextDocumentActions(group)
                }

                similarDuplicateCleanupControls(group)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    ForEach(group.paths) { item in
                        similarDuplicateItemRow(item, group: group)
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .onAppear {
            recordDuplicateGroupImpression(group)
        }
    }

    private func similarRedundancyGroupRow(_ group: StorageRedundancyGroupModel) -> some View {
        let tone = redundancyGroupTone(group)
        let groupKey = redundancyGroupActionKey(group)
        let isExpanded = similarityGroupIsExpanded(groupKey)
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: "square.stack.3d.up")
                    .foregroundStyle(tone)
                    .frame(width: 22)
                VStack(alignment: .leading, spacing: 3) {
                    Text(group.title)
                        .font(.subheadline.weight(.semibold))
                    Text("\(group.itemCount) item\(group.itemCount == 1 ? "" : "s") - \(group.confidenceScore)% confidence")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(formatBytes(group.reclaimableBytes))
                        .font(.system(size: 18, weight: .semibold, design: .rounded))
                    Text("reclaimable estimate")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Text(formatBytes(group.totalBytes))
                        .font(.caption.weight(.semibold))
                    Text("total bytes")
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                }
            }

            HStack(spacing: AetowerDesign.Spacing.xs) {
                AetowerBadge(group.redundancyClass.replacingOccurrences(of: "-", with: " "), tone: tone)
                AetowerBadge("Other redundancy", tone: AetowerDesign.Tone.memory)
                if reviewedSimilarityGroupIDs.contains(groupKey) {
                    AetowerBadge("Reviewed", tone: AetowerDesign.Status.ready)
                }
                if ignoredSimilarityGroupIDs.contains(groupKey) {
                    AetowerBadge("Ignored", tone: AetowerDesign.Status.neutral)
                }
                Spacer(minLength: 0)
            }

            similarRedundancyGroupActions(group)

            if isExpanded {
                if !group.recommendation.isEmpty {
                    Text(group.recommendation)
                        .font(.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                if !group.caveat.isEmpty {
                    Text(group.caveat)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                if !group.evidence.isEmpty {
                    VStack(alignment: .leading, spacing: 3) {
                        ForEach(group.evidence.prefix(3), id: \.self) { evidence in
                            Label(evidence, systemImage: "checkmark.seal")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    ForEach(group.items) { item in
                        similarRedundancyItemRow(item, group: group)
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .onAppear {
            recordRedundancyGroupImpression(group)
        }
    }

    private func similarImageContactSheet(_ group: StorageDuplicateGroupModel) -> some View {
        let items = group.paths
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: "photo.on.rectangle.angled")
                    .foregroundStyle(AetowerDesign.Status.warning)
                Text("Image contact sheet")
                    .font(.caption.weight(.semibold))
                Spacer()
                Text("\(items.count) image\(items.count == 1 ? "" : "s")")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 112), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                ForEach(items) { item in
                    StorageSimilarityImageThumbnail(item: item)
                        .onTapGesture {
                            recordSimilarityAction(
                                "quick-look-used",
                                duplicateGroup: group,
                                extraFields: [(key: "target_scope", value: "thumbnail")]
                            )
                            quickLook(path: item.path)
                        }
                }
            }
        }
        .padding(AetowerDesign.Spacing.sm)
        .background(AetowerDesign.Surface.badge, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private func similarTextDocumentActions(_ group: StorageDuplicateGroupModel) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Button {
                openDuplicateGroupFirstPair(group)
            } label: {
                Label("Open pair", systemImage: "rectangle.split.2x1")
            }
            Button {
                compareDuplicateGroupFirstPair(group)
            } label: {
                Label("Compare", systemImage: "arrow.left.arrow.right")
            }
            Button {
                copy(diffCommand(for: Array(group.paths.prefix(2)).map(\.path)))
            } label: {
                Label("Copy diff command", systemImage: "doc.on.doc")
            }
            Spacer()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .disabled(group.paths.count < 2)
    }

    private func similarDuplicateGroupActions(_ group: StorageDuplicateGroupModel) -> some View {
        let groupKey = duplicateGroupActionKey(group)
        let paths = group.paths.map(\.path)
        return HStack(spacing: AetowerDesign.Spacing.sm) {
            Button {
                recordSimilarityAction(
                    "quick-look-used",
                    duplicateGroup: group,
                    extraFields: [(key: "target_scope", value: "selected")]
                )
                quickLook(path: duplicateGroupCanonicalItem(group)?.path ?? group.paths.first?.path ?? "")
            } label: {
                Label("Quick Look selected", systemImage: "eye")
            }
            .disabled(!group.actions.canQuickLook || group.paths.isEmpty)

            Button {
                recordSimilarityAction(
                    "reveal-used",
                    duplicateGroup: group,
                    extraFields: [
                        (key: "target_scope", value: "group"),
                        (key: "revealed_count", value: String(paths.count)),
                    ]
                )
                reveal(paths: paths)
            } label: {
                Label("Reveal all", systemImage: "arrow.up.forward.square")
            }
            .disabled(!group.actions.canReveal || paths.isEmpty)

            Button {
                recordSimilarityAction(
                    "open-comparison-used",
                    duplicateGroup: group,
                    extraFields: [(key: "compared_count", value: String(min(paths.count, 2)))]
                )
                comparePaths(Array(paths.prefix(2)))
            } label: {
                Label("Open comparison", systemImage: "arrow.left.arrow.right")
            }
            .disabled(paths.count < 2)

            Button {
                toggleDuplicateGroupExpansion(group)
            } label: {
                Label(
                    similarityGroupIsExpanded(groupKey) ? "Collapse details" : "Expand details",
                    systemImage: similarityGroupIsExpanded(groupKey) ? "chevron.up.circle" : "chevron.down.circle"
                )
            }

            Menu {
                Button("Copy paths") {
                    recordSimilarityAction(
                        "copy-paths-used",
                        duplicateGroup: group,
                        extraFields: [(key: "path_count", value: String(paths.count))]
                    )
                    copyPaths(paths)
                }
                .disabled(paths.isEmpty)
                if reviewedSimilarityGroupIDs.contains(groupKey) {
                    Button("Mark unreviewed") {
                        unmarkSimilarityGroupReviewed(groupKey)
                    }
                } else {
                    Button("Mark reviewed") {
                        markSimilarityGroupReviewed(groupKey)
                        recordSimilarityAction("marked-reviewed", duplicateGroup: group)
                    }
                }
                if ignoredSimilarityGroupIDs.contains(groupKey) {
                    Button("Restore group") {
                        restoreSimilarityGroup(groupKey)
                    }
                } else {
                    Button("Ignore group") {
                        ignoreSimilarityGroup(groupKey)
                        recordSimilarityAction("ignored", duplicateGroup: group)
                    }
                }
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
            Spacer()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }

    private func similarRedundancyGroupActions(_ group: StorageRedundancyGroupModel) -> some View {
        let groupKey = redundancyGroupActionKey(group)
        let paths = group.items.map(\.path)
        return HStack(spacing: AetowerDesign.Spacing.sm) {
            Button {
                recordSimilarityAction(
                    "quick-look-used",
                    redundancyGroup: group,
                    extraFields: [(key: "target_scope", value: "selected")]
                )
                quickLook(path: paths.first ?? "")
            } label: {
                Label("Quick Look selected", systemImage: "eye")
            }
            .disabled(!group.actions.canQuickLook || paths.isEmpty)

            Button {
                recordSimilarityAction(
                    "reveal-used",
                    redundancyGroup: group,
                    extraFields: [
                        (key: "target_scope", value: "group"),
                        (key: "revealed_count", value: String(paths.count)),
                    ]
                )
                reveal(paths: paths)
            } label: {
                Label("Reveal all", systemImage: "arrow.up.forward.square")
            }
            .disabled(!group.actions.canReveal || paths.isEmpty)

            Button {
                recordSimilarityAction(
                    "open-comparison-used",
                    redundancyGroup: group,
                    extraFields: [(key: "compared_count", value: String(min(paths.count, 2)))]
                )
                comparePaths(Array(paths.prefix(2)))
            } label: {
                Label("Open comparison", systemImage: "arrow.left.arrow.right")
            }
            .disabled(paths.count < 2)

            Button {
                toggleRedundancyGroupExpansion(group)
            } label: {
                Label(
                    similarityGroupIsExpanded(groupKey) ? "Collapse details" : "Expand details",
                    systemImage: similarityGroupIsExpanded(groupKey) ? "chevron.up.circle" : "chevron.down.circle"
                )
            }

            Menu {
                Button("Copy paths") {
                    recordSimilarityAction(
                        "copy-paths-used",
                        redundancyGroup: group,
                        extraFields: [(key: "path_count", value: String(paths.count))]
                    )
                    copyPaths(paths)
                }
                .disabled(paths.isEmpty)
                if reviewedSimilarityGroupIDs.contains(groupKey) {
                    Button("Mark unreviewed") {
                        unmarkSimilarityGroupReviewed(groupKey)
                    }
                } else {
                    Button("Mark reviewed") {
                        markSimilarityGroupReviewed(groupKey)
                        recordSimilarityAction("marked-reviewed", redundancyGroup: group)
                    }
                }
                if ignoredSimilarityGroupIDs.contains(groupKey) {
                    Button("Restore group") {
                        restoreSimilarityGroup(groupKey)
                    }
                } else {
                    Button("Ignore group") {
                        ignoreSimilarityGroup(groupKey)
                        recordSimilarityAction("ignored", redundancyGroup: group)
                    }
                }
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
            Spacer()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }

    @ViewBuilder
    private func similarDuplicateCleanupControls(_ group: StorageDuplicateGroupModel) -> some View {
        if duplicateGroupCanStageExactCleanup(group), let canonical = duplicateGroupCanonicalItem(group) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                    Label("Canonical kept", systemImage: "checkmark.seal")
                        .font(.caption2)
                        .foregroundStyle(AetowerDesign.Status.ready)
                    Picker("Canonical", selection: duplicateCanonicalSelection(for: group)) {
                        ForEach(group.paths) { item in
                            Text(item.displayName).tag(item.path)
                        }
                    }
                    .labelsHidden()
                    .pickerStyle(.menu)
                    .frame(maxWidth: 240)
                    Spacer(minLength: AetowerDesign.Spacing.sm)
                    Button {
                        stageExactDuplicateGroup(group, canonical: canonical)
                    } label: {
                        Label(
                            "Stage duplicates except canonical",
                            systemImage: "tray.and.arrow.down"
                        )
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .disabled(duplicateGroupStageableItems(group, canonical: canonical).isEmpty)
                }
                Text("Preflight opens before anything moves to Trash. Only non-canonical copies are staged.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .padding(AetowerDesign.Spacing.sm)
            .background(AetowerDesign.Status.ready.opacity(0.10), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        } else {
            Label(
                duplicateGroupReviewOnlyReason(group),
                systemImage: "hand.raised"
            )
            .font(.caption2.weight(.medium))
            .foregroundStyle(AetowerDesign.Status.warning)
            .fixedSize(horizontal: false, vertical: true)
            .padding(AetowerDesign.Spacing.sm)
            .background(AetowerDesign.Status.warning.opacity(0.10), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
    }

    private func similarDuplicateItemRow(
        _ item: StorageDuplicateItemModel,
        group: StorageDuplicateGroupModel
    ) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(item.displayName)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                Text(item.path)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(similarDuplicateItemMetadata(item))
                    .font(.system(size: 9))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: AetowerDesign.Spacing.sm)
            VStack(alignment: .trailing, spacing: 2) {
                Text(formatBytes(item.sizeBytes))
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                Text(item.safety)
                    .font(.system(size: 9))
                    .foregroundStyle(.tertiary)
            }
            Button {
                openPath(path: item.path)
            } label: {
                Image(systemName: "arrow.up.right.square")
            }
            .help("Open")
            Button {
                recordSimilarityAction(
                    "quick-look-used",
                    duplicateGroup: group,
                    extraFields: [(key: "target_scope", value: "item")]
                )
                quickLook(path: item.path)
            } label: {
                Image(systemName: "eye")
            }
            .help("Quick Look")
            Button {
                recordSimilarityAction(
                    "reveal-used",
                    duplicateGroup: group,
                    extraFields: [(key: "target_scope", value: "item")]
                )
                reveal(path: item.path)
            } label: {
                Image(systemName: "arrow.up.forward.square")
            }
            .help("Reveal in Finder")
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
        .padding(.vertical, 4)
    }

    private func similarRedundancyItemRow(
        _ item: StorageRedundancyItemModel,
        group: StorageRedundancyGroupModel
    ) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(item.displayName)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                Text(item.path)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(similarRedundancyItemMetadata(item))
                    .font(.system(size: 9))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: AetowerDesign.Spacing.sm)
            VStack(alignment: .trailing, spacing: 2) {
                Text(formatBytes(item.physicalBytes))
                    .font(.caption2.weight(.semibold))
                Text("\(formatBytes(item.logicalBytes)) logical")
                    .font(.system(size: 9))
                    .foregroundStyle(.tertiary)
            }
            Button {
                openPath(path: item.path)
            } label: {
                Image(systemName: "arrow.up.right.square")
            }
            .help("Open")
            Button {
                recordSimilarityAction(
                    "quick-look-used",
                    redundancyGroup: group,
                    extraFields: [(key: "target_scope", value: "item")]
                )
                quickLook(path: item.path)
            } label: {
                Image(systemName: "eye")
            }
            .help("Quick Look")
            Button {
                recordSimilarityAction(
                    "reveal-used",
                    redundancyGroup: group,
                    extraFields: [(key: "target_scope", value: "item")]
                )
                reveal(path: item.path)
            } label: {
                Image(systemName: "arrow.up.forward.square")
            }
            .help("Reveal in Finder")
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
        .padding(.vertical, 4)
    }

    private func similarFilesEmptyState(_ filter: StorageSimilarityFilter) -> some View {
        Label("No \(filter.emptyStateLabel) in the current scan.", systemImage: "checkmark.circle")
            .font(.caption)
            .foregroundStyle(.tertiary)
            .padding(.vertical, AetowerDesign.Spacing.sm)
    }

    private func appFootprintsCard(_ footprints: [StorageAppFootprintModel]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text("App footprints")
                .font(.subheadline.weight(.semibold))
            Text("Uninstall view: app bundle, caches, preferences, receipts, containers, support data, and launch items when visible to the scan.")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if footprints.isEmpty {
                Label("No app footprints in the retained scan set.", systemImage: "app.dashed")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            } else {
                ForEach(footprints.prefix(3)) { footprint in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(footprint.appName)
                                .font(.caption.weight(.semibold))
                                .lineLimit(1)
                            Spacer()
                            Text(formatBytes(footprint.totalBytes))
                                .font(.caption2.weight(.semibold))
                        }
                        Text("\(footprint.componentCount) component\(footprint.componentCount == 1 ? "" : "s") · \(footprint.confidenceScore)% confidence")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        ForEach(footprint.components.prefix(2)) { component in
                            HStack(spacing: AetowerDesign.Spacing.xs) {
                                Text(component.component)
                                    .font(.caption2.weight(.semibold))
                                Text(component.path)
                                    .font(.system(size: 10, design: .monospaced))
                                    .foregroundStyle(.tertiary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                Spacer()
                                Button("Reveal") { reveal(path: component.path) }
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.mini)
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 210, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func systemDataBucketsCard(_ buckets: [StorageSystemDataBucketModel]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text("System Data explainer")
                .font(.subheadline.weight(.semibold))
            Text("Common macOS System Data buckets with plain-English cleanup guidance.")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            ForEach(buckets.prefix(4)) { bucket in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(bucket.title)
                            .font(.caption.weight(.semibold))
                        Spacer()
                        Text(formatBytes(bucket.sizeBytes))
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(bucket.sizeBytes > 0 ? tone(forCleanupTier: bucket.cleanupTier) : .secondary)
                    }
                    Text(bucket.sizeBytes > 0 ? bucket.recommendedAction : bucket.explanation)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                    if let path = bucket.paths.first {
                        HStack(spacing: AetowerDesign.Spacing.xs) {
                            Text(path)
                                .font(.system(size: 10, design: .monospaced))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Spacer()
                            Button("Reveal") { reveal(path: path) }
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.mini)
                    }
                }
                .padding(.vertical, 4)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 210, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func compactStorageItemActionRow(_ item: StorageHygieneItemModel) -> some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            Image(systemName: icon(for: item))
                .foregroundStyle(tone(for: item))
                .frame(width: 15)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.displayName)
                    .font(.caption2.weight(.semibold))
                    .lineLimit(1)
                Text(item.path)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: AetowerDesign.Spacing.xs)
            Text(formatBytes(item.sizeBytes))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
            if storageItemIsTrashActionable(item) {
                storageItemPrimaryAction(item)
            }
            Button("Reveal") { reveal(path: item.path) }
            storageItemActionMenu(item)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
    }

    // MARK: - Disk pressure header

    private func primaryVolume(_ report: StorageHygieneReportModel) -> StorageVolumeStateModel? {
        report.volumeStates.first { $0.path == "/" }
            ?? report.volumeStates.max { $0.totalBytes < $1.totalBytes }
    }

    /// The hero for the whole tab: how full the disk is, how much this scan can
    /// give back, and the one button that does it. Absorbs the old separate
    /// "Reclaim safely" panel so the primary number and CTA appear once.
    private func storageDiskPressureHeader(_ report: StorageHygieneReportModel) -> some View {
        let volume = primaryVolume(report)
        let reclaimable = report.summary.totalReclaimableBytes
        let safeBundle = report.cleanupBundles.first
        let hasCTA = safeBundle.map(cleanupBundleHasActionableCommands) ?? false

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            if let volume, volume.totalBytes > 0 {
                let free = volume.availableBytes > 0 ? volume.availableBytes : volume.freeNowBytes
                let cappedReclaim = min(reclaimable, volume.totalBytes)
                let freeRatio = Double(free) / Double(volume.totalBytes)
                let tone: Color = freeRatio < 0.05 ? AetowerDesign.Status.error
                    : freeRatio < 0.12 ? AetowerDesign.Status.warning
                    : AetowerDesign.Status.ready
                let pressureLabel = freeRatio < 0.05 ? "Critically low space"
                    : freeRatio < 0.12 ? "Low space" : "Healthy"

                HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                    Image(systemName: "internaldrive.fill").foregroundStyle(tone)
                    Text(volumeDisplayName(volume)).font(.headline)
                    Text(pressureLabel)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(tone)
                        .padding(.horizontal, 8).padding(.vertical, 3)
                        .background(tone.opacity(0.14), in: Capsule())
                    Spacer()
                    (Text(formatBytes(free)).font(.system(size: 26, weight: .bold, design: .rounded))
                        + Text("  free").font(.callout).foregroundColor(.secondary))
                }

                diskCapacityBar(total: volume.totalBytes, free: free, reclaimable: cappedReclaim, tone: tone)

                HStack(spacing: AetowerDesign.Spacing.md) {
                    Text("\(formatBytes(volume.totalBytes - free)) used of \(formatBytes(volume.totalBytes))")
                        .font(.caption).foregroundStyle(.secondary)
                    if cappedReclaim > 0 {
                        Text("up to \(formatBytes(free + cappedReclaim)) free after cleanup")
                            .font(.caption.weight(.medium))
                            .foregroundStyle(AetowerDesign.Tone.disk)
                    }
                    Spacer()
                }
            }

            if reclaimable > 0 {
                Divider().padding(.vertical, AetowerDesign.Spacing.xxs)
                HStack(spacing: AetowerDesign.Spacing.md) {
                    if hasCTA, let safeBundle {
                        Button {
                            stageCleanupBundle(safeBundle)
                        } label: {
                            Label("Reclaim \(formatBytes(safeBundle.estimatedReclaimableBytes)) safely", systemImage: "sparkles")
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                    }
                    heroStat("\(report.summary.safeCandidateCount)", "safe", AetowerDesign.Status.ready)
                    heroStat("\(report.summary.reviewCandidateCount)", "to review", AetowerDesign.Status.warning)
                    Spacer()
                    heroStat(formatBytes(reclaimable), "total reclaimable", AetowerDesign.Tone.disk, trailing: true)
                }
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func heroStat(_ value: String, _ label: String, _ tone: Color, trailing: Bool = false) -> some View {
        VStack(alignment: trailing ? .trailing : .leading, spacing: 1) {
            Text(value)
                .font(.system(size: 15, weight: .semibold, design: .rounded))
                .foregroundStyle(tone)
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
    }

    private func volumeDisplayName(_ volume: StorageVolumeStateModel) -> String {
        if volume.path == "/" { return "Macintosh HD" }
        let last = (volume.path as NSString).lastPathComponent
        return last.isEmpty ? volume.path : last
    }

    /// Three-segment capacity bar: used (neutral) · reclaimable (disk-tinted,
    /// the recoverable slice) · free (track). Proportional to total bytes.
    private func diskCapacityBar(total: UInt64, free: UInt64, reclaimable: UInt64, tone: Color) -> some View {
        GeometryReader { geo in
            let width = geo.size.width
            let usedBytes = total - free
            let hardUsed = usedBytes > reclaimable ? usedBytes - reclaimable : 0
            let hardWidth = width * CGFloat(Double(hardUsed) / Double(total))
            let reclaimWidth = width * CGFloat(Double(reclaimable) / Double(total))
            HStack(spacing: 1.5) {
                Rectangle()
                    .fill(AetowerDesign.Ink.secondary.opacity(0.55))
                    .frame(width: max(0, hardWidth))
                if reclaimWidth > 0 {
                    Rectangle()
                        .fill(AetowerDesign.Tone.disk)
                        .frame(width: max(2, reclaimWidth))
                }
                Rectangle()
                    .fill(AetowerDesign.Surface.badgeStrong)
            }
            .clipShape(Capsule())
        }
        .frame(height: 12)
    }

    private func storageActionPanel(_ report: StorageHygieneReportModel) -> some View {
        let primaryBundle = report.cleanupBundles.first
        let hasCandidateCommands = primaryBundle.map(cleanupBundleHasActionableCommands) ?? false

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: primaryBundle.map { cleanupBundleIcon($0) } ?? "externaldrive")
                    .foregroundStyle(primaryBundle.map { cleanupBundleTone($0) } ?? AetowerDesign.Tone.disk)
                    .frame(width: 24)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text(primaryBundle?.title ?? "No cleanup plan yet")
                        .font(.title3.weight(.semibold))
                    Text(primaryBundle?.subtitle ?? "Run or narrow a scan to build a cleanup plan with Trash actions, reveal targets, verification commands, and manual command references.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(primaryBundle.map { formatBytes($0.estimatedReclaimableBytes) } ?? formatBytes(report.summary.totalReclaimableBytes))
                        .font(.system(size: 26, weight: .semibold, design: .rounded))
                    Text(primaryBundle.map { "\($0.confidenceScore)% confidence" } ?? "estimated reclaimable")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 160), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "Reclaimable",
                    value: formatBytes(report.summary.totalReclaimableBytes),
                    detail: "\(report.summary.itemCount) candidate\(report.summary.itemCount == 1 ? "" : "s")"
                )
                footprintMetric(
                    "Safe items",
                    value: "\(report.summary.safeCandidateCount)",
                    detail: "expected artifacts"
                )
                footprintMetric(
                    "Needs review",
                    value: "\(report.summary.reviewCandidateCount)",
                    detail: "operator decision"
                )
                footprintMetric(
                    "Scan",
                    value: "\(report.scanDurationMillis) ms",
                    // Index-served modes walk nothing; "0 folders" reads as a
                    // failure when it actually means "answered from the index".
                    detail: report.summary.scannedDirectoryCount > 0
                        ? "\(report.summary.scannedDirectoryCount) folders"
                        : "from index"
                )
            }

            if let primaryBundle {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    if hasCandidateCommands {
                        Button {
                            stageCleanupBundle(primaryBundle)
                        } label: {
                            Label("Stage cleanup", systemImage: "tray.and.arrow.down")
                        }
                        .buttonStyle(.borderedProminent)
                    }

                    // Reference-only actions (nothing here executes commands)
                    // collapsed into one menu so the primary action stands out.
                    Menu {
                        Button("Copy plan") {
                            copy(cleanupBundleManifest(primaryBundle))
                            copiedCleanupBundleID = primaryBundle.id
                        }
                        Button("Copy verify commands") {
                            copy(primaryBundle.dryRunCommands.joined(separator: "\n"))
                            copiedCleanupBundleID = primaryBundle.id
                        }
                        .disabled(primaryBundle.dryRunCommands.isEmpty)
                        if hasCandidateCommands {
                            Button("Review command references") {
                                candidateCommandPreviewBundle = primaryBundle
                            }
                        }
                    } label: {
                        Label("References", systemImage: "doc.on.doc")
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()

                    Spacer()

                    Text(copiedCleanupBundleID == primaryBundle.id ? "Copied" : basketSummaryLabel)
                        .font(.caption2)
                        .foregroundStyle(copiedCleanupBundleID == primaryBundle.id ? AetowerDesign.Status.ready : .secondary)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            } else {
                Label("No copyable cleanup bundle is available for this scan.", systemImage: "info.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func storageCoverageOverview(_ report: StorageHygieneReportModel) -> some View {
        let sources = report.sourceCoverage
        let scanned = sources.filter(\.scanned).count
        let blocked = sources.filter { $0.permissionState == "needs_full_disk_access" }.count
        let cloud = sources.filter { $0.kind == "cloud" || $0.cloudPlaceholder }.count
        let protected = sources.filter(\.protected).count
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Computer coverage")
                        .font(.headline)
                    Text("Every default source Aetower considered, including unreadable, cloud-backed, and external locations.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                AetowerBadge("\(scanned)/\(sources.count) scanned", tone: blocked == 0 ? AetowerDesign.Status.ready : AetowerDesign.Status.warning)
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 190), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric("Scanned", value: "\(scanned)", detail: "\(sources.count) source\(sources.count == 1 ? "" : "s")")
                footprintMetric("Needs access", value: "\(blocked)", detail: "Full Disk Access or unavailable")
                footprintMetric("Cloud roots", value: "\(cloud)", detail: "local bytes are separated")
                footprintMetric("Protected", value: "\(protected)", detail: "blocked from unattended cleanup")
                footprintMetric("Volumes", value: "\(report.volumeStates.count)", detail: "capacity sources")
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 250), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                ForEach(sources.prefix(8)) { source in
                    storageSourceCard(source)
                }
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func volumeStateSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            advancedSectionLabel(
                title: "Volume capacity",
                detail: "\(report.volumeStates.count) volume\(report.volumeStates.count == 1 ? "" : "s") with free, available, and macOS capacity signals",
                systemImage: "internaldrive"
            )
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 260), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                ForEach(report.volumeStates) { volume in
                    volumeStateCard(volume)
                }
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func storageInvestigationSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "magnifyingglass.circle")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                    .frame(width: 24)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Investigation")
                        .font(.headline)
                    Text("Where storage pressure comes from, why Aetower classified it that way, and what to inspect before cleanup.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                Button("Advanced details") {
                    selectedSection = .insights
                    showRawArtifacts = true
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "Caches/builds",
                    value: formatBytes(report.investigation.knownCacheBytes),
                    detail: "known patterns"
                )
                footprintMetric(
                    "Rebuildable",
                    value: formatBytes(report.investigation.rebuildableBytes),
                    detail: "toolchain-owned"
                )
                footprintMetric(
                    "Expensive",
                    value: formatBytes(report.investigation.expensiveBytes),
                    detail: "slow to restore"
                )
                footprintMetric(
                    "Risky",
                    value: formatBytes(report.investigation.riskyBytes),
                    detail: "\(report.investigation.largeFileCount) large file\(report.investigation.largeFileCount == 1 ? "" : "s")"
                )
                footprintMetric(
                    "Cold",
                    value: formatBytes(report.investigation.coldFileBytes),
                    detail: "\(report.investigation.coldFileCount) over 1y"
                )
                footprintMetric(
                    "Review",
                    value: "\(report.investigation.reviewItemCount)",
                    detail: "manual decision"
                )
            }

            if report.investigation.topFindings.isEmpty {
                Label("No storage pressure findings were produced for this scan.", systemImage: "checkmark.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 280), spacing: AetowerDesign.Spacing.sm)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.sm
                ) {
                    ForEach(report.investigation.topFindings) { finding in
                        storageInvestigationFindingCard(finding)
                    }
                }
            }

            if !report.investigation.recommendedNextSteps.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(report.investigation.recommendedNextSteps.prefix(3), id: \.self) { step in
                        Label(step, systemImage: "checklist")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func storageInvestigationFindingCard(_ finding: StorageInvestigationFindingModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: cleanupTierIcon(finding.cleanupTier))
                    .foregroundStyle(tone(forCleanupTier: finding.cleanupTier))
                    .frame(width: 20)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        Text(finding.title)
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(1)
                        AetowerBadge(storageRoleLabel(finding.storageRole), tone: tone(forCleanupTier: finding.cleanupTier))
                    }
                    Text(finding.detail)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: AetowerDesign.Spacing.sm)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(formatBytes(finding.sizeBytes))
                        .font(.caption.weight(.semibold))
                    Text("\(finding.confidenceScore)%")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            Text(finding.path)
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)

            VStack(alignment: .leading, spacing: 3) {
                ForEach(finding.evidence.prefix(3), id: \.self) { evidence in
                    Label(evidence, systemImage: "smallcircle.filled.circle")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(2)
                }
            }

            Text(finding.recommendedAction)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Quick Look") { quickLook(path: finding.path) }
                Button("Reveal") { reveal(path: finding.path) }
                Button("Explain") { classificationExplanation = explanation(for: finding) }
                Button("Copy path") { copy(finding.path) }
                Spacer()
                Text(finding.safety == "safe" ? "actionable finding" : "manual review")
                    .font(.caption2)
                    .foregroundStyle(finding.safety == "safe" ? .secondary : AetowerDesign.Status.warning)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func reclaimSpaceSection(_ report: StorageHygieneReportModel) -> some View {
        let recipes = overviewReclaimRecipes(report)
        let visibleBytes = recipes.reduce(UInt64(0)) { total, recipe in
            let sum = total.addingReportingOverflow(recipe.estimatedReclaimableBytes)
            return sum.overflow ? UInt64.max : sum.partialValue
        }

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "bolt.horizontal.circle")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Reclaim space")
                        .font(.headline)
                    Text("Concrete cleanup actions for the largest high-confidence local artifacts. Reveal targets, verify candidates, then copy the exact command you choose to run.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(visibleBytes))
                        .font(.system(size: 22, weight: .semibold, design: .rounded))
                    Text("\(recipes.count) visible action\(recipes.count == 1 ? "" : "s")")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            if recipes.isEmpty {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                    Label("No direct reclaim actions were generated for this scan.", systemImage: "checkmark.circle")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Inspect details") {
                        selectedSection = .insights
                    }
                }
            } else {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 250), spacing: AetowerDesign.Spacing.sm)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.sm
                ) {
                    ForEach(recipes) { recipe in
                        reclaimActionCard(recipe)
                    }
                }

                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Button {
                        stageCleanupRecipes(recipes)
                    } label: {
                        Label("Stage visible", systemImage: "tray.and.arrow.down")
                    }
                    .buttonStyle(.borderedProminent)

                    Button {
                        copy(recipes.map(\.command).joined(separator: "\n"))
                        copiedCleanupRecipeID = "overview-visible"
                    } label: {
                        Label("Copy visible commands", systemImage: "doc.on.doc")
                    }

                    Button("Show all actions") {
                        selectedSection = .reclaim
                        showCleanupRecipes = true
                    }

                    Spacer()

                    Text(copiedCleanupRecipeID == "overview-visible" ? "Copied" : "Review before running")
                        .font(.caption2)
                        .foregroundStyle(copiedCleanupRecipeID == "overview-visible" ? AetowerDesign.Status.ready : .secondary)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func reclaimActionCard(_ recipe: StorageCleanupRecipeModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: cleanupRecipeIcon(recipe))
                    .foregroundStyle(cleanupRecipeTone(recipe))
                    .frame(width: 20)
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        Text(recipe.title)
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(1)
                        Text(recipe.requiresReview ? "Review" : "Ready")
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(cleanupRecipeTone(recipe))
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(cleanupRecipeTone(recipe).opacity(0.12), in: Capsule())
                    }
                    Text(recipe.reason)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: AetowerDesign.Spacing.sm)
                Text(formatBytes(recipe.estimatedReclaimableBytes))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }

            recommendationDecisionGrid(recipeRecommendationDecisions(recipe))

            Text(recipe.command)
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Copy command") {
                    copy(recipe.command)
                    copiedCleanupRecipeID = recipe.id
                }
                Button("Stage") {
                    stageCleanupRecipe(recipe)
                }
                Button("Quick Look") {
                    quickLook(path: recipe.affectedPath)
                }
                Button("Reveal") {
                    reveal(path: recipe.affectedPath)
                }
                Button("Explain") {
                    classificationExplanation = explanation(for: recipe)
                }
                Spacer()
                Text(copiedCleanupRecipeID == recipe.id ? "Copied" : "Ready")
                    .font(.caption2)
                    .foregroundStyle(copiedCleanupRecipeID == recipe.id ? AetowerDesign.Status.ready : .secondary)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 154, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func overviewReclaimRecipes(_ report: StorageHygieneReportModel) -> [StorageCleanupRecipeModel] {
        Array(
            report.cleanupRecipes
                .filter { !$0.command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
                .sorted { left, right in
                    let leftRank = left.requiresReview ? 1 : 0
                    let rightRank = right.requiresReview ? 1 : 0
                    if leftRank != rightRank {
                        return leftRank < rightRank
                    }
                    if left.estimatedReclaimableBytes != right.estimatedReclaimableBytes {
                        return left.estimatedReclaimableBytes > right.estimatedReclaimableBytes
                    }
                    return left.title < right.title
                }
                .prefix(4)
        )
    }

    private func shouldShowAgentHygieneOverview(_ report: StorageHygieneReportModel) -> Bool {
        report.agentHygiene.agentCount > 0 || report.agentHygiene.totalAgentArtifactBytes > 0
    }

    private func summaryGrid(_ report: StorageHygieneReportModel) -> some View {
        let similaritySummary = similarityReviewSummary(for: report)
        return LazyVGrid(
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
                "Similar",
                value: "\(similaritySummary.groupCount)",
                detail: formatBytes(similaritySummary.reviewableBytes),
                systemImage: "doc.on.doc",
                tone: similaritySummary.groupCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
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
            summaryCard(
                "Attributed",
                value: "\(report.summary.attributedRepoCount)",
                detail: "repo/branch-linked artifacts",
                systemImage: "point.3.connected.trianglepath.dotted",
                tone: report.summary.attributedRepoCount > 0 ? AetowerDesign.Status.ready : .secondary
            )
        }
    }

    private func topOffenderCallout(_ report: StorageHygieneReportModel) -> some View {
        let offenders = storageTopOffenders(report)
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Top storage pressure")
                    .font(.headline)
                Text("The repo, agent, and folder most likely to explain current growth or cleanup impact.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if offenders.isEmpty {
                Label("No top offender can be determined from this scan yet.", systemImage: "questionmark.folder")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 210), spacing: AetowerDesign.Spacing.sm)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.sm
                ) {
                    ForEach(offenders) { offender in
                        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                            Image(systemName: offender.systemImage)
                                .foregroundStyle(offender.tone)
                                .frame(width: 20)
                            VStack(alignment: .leading, spacing: 3) {
                                Text(offender.title.uppercased())
                                    .font(.caption2.weight(.semibold))
                                    .foregroundStyle(.tertiary)
                                Text(offender.value)
                                    .font(.system(size: 18, weight: .semibold, design: .rounded))
                                    .lineLimit(1)
                                    .minimumScaleFactor(0.75)
                                Text(offender.detail)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(2)
                            }
                            Spacer(minLength: AetowerDesign.Spacing.sm)
                        }
                        .padding(AetowerDesign.Spacing.sm)
                        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func budgetGuardrailsSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: budgetGuardrailIcon(report.budgetGuardrails.status))
                    .foregroundStyle(budgetGuardrailTone(report.budgetGuardrails.status))
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Budget guardrails")
                        .font(.headline)
                    Text(budgetGuardrailSummary(report.budgetGuardrails))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer()

                Text(report.budgetGuardrails.status.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(budgetGuardrailTone(report.budgetGuardrails.status))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(budgetGuardrailTone(report.budgetGuardrails.status).opacity(0.12), in: Capsule())
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 190), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "Total artifacts",
                    value: formatBytes(report.summary.totalReclaimableBytes),
                    detail: "budget \(formatBytes(report.budgetGuardrails.totalArtifactBudgetBytes))"
                )
                footprintMetric(
                    "Per repo",
                    value: formatBytes(report.budgetGuardrails.repoArtifactBudgetBytes),
                    detail: "artifact footprint limit"
                )
                footprintMetric(
                    "Growth",
                    value: formatBytes(report.budgetGuardrails.repoGrowthBudgetBytesPerDay),
                    detail: "per repo per day"
                )
                footprintMetric(
                    "Free floor",
                    value: formatBytes(report.budgetGuardrails.freeSpaceFloorBytes),
                    detail: "\(report.budgetGuardrails.volumePressureFloorPercent)% volume floor"
                )
                footprintMetric(
                    "Policy mode",
                    value: report.budgetGuardrails.warningOnlyByDefault ? "Warn" : "Active",
                    detail: report.budgetGuardrails.autoTrashSafeTierEnabled ? "safe auto-trash opt-in" : "no auto-trash"
                )
            }

            if !report.budgetGuardrails.preventionSuggestions.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    Text("Prevention suggestions")
                        .font(AetowerDesign.Typography.controlLabel)
                    ForEach(report.budgetGuardrails.preventionSuggestions) { suggestion in
                        preventionSuggestionRow(suggestion, report: report)
                    }
                }
            }

            if report.budgetGuardrails.violations.isEmpty {
                Label("All storage budgets are currently within limits.", systemImage: "checkmark.circle")
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.ready)
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(report.budgetGuardrails.violations) { violation in
                        budgetViolationRow(violation)
                    }
                }
            }

            if !report.budgetGuardrails.policies.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    Text("Policy boundaries")
                        .font(AetowerDesign.Typography.controlLabel)
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 240), spacing: AetowerDesign.Spacing.sm)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.sm
                    ) {
                        ForEach(report.budgetGuardrails.policies) { policy in
                            preventionPolicyCard(policy)
                        }
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func preventionSuggestionRow(
        _ suggestion: StoragePreventionSuggestionModel,
        report: StorageHygieneReportModel
    ) -> some View {
        AetowerSurface(level: .card, padding: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: preventionSuggestionIcon(suggestion))
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        Text(suggestion.title)
                            .font(AetowerDesign.Typography.controlLabel)
                        AetowerBadge(
                            suggestion.safety.uppercased(),
                            tone: preventionSuggestionTone(suggestion)
                        )
                    }
                    Text(suggestion.detail)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    if suggestion.estimatedReclaimableBytes > 0 {
                        Text(formatBytes(suggestion.estimatedReclaimableBytes))
                            .font(AetowerDesign.Typography.data)
                    }
                    Button {
                        stagePreventionSuggestion(suggestion, report: report)
                    } label: {
                        Text(suggestion.actionLabel)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }
            }
        }
    }

    private func preventionPolicyCard(_ policy: StoragePreventionPolicyModel) -> some View {
        AetowerSurface(level: .card, padding: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: policy.enabled ? "bell.badge" : "hand.raised")
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                    Text(policy.title)
                        .font(AetowerDesign.Typography.metadataStrong)
                        .lineLimit(1)
                    Spacer()
                    AetowerBadge(
                        policy.mode.uppercased(),
                        tone: policy.enabled ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
                    )
                }
                Text(policy.detail)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text(policy.nextStep)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func budgetViolationRow(_ violation: StorageBudgetViolationModel) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
            Image(systemName: budgetGuardrailIcon(violation.severity))
                .foregroundStyle(budgetGuardrailTone(violation.severity))
                .frame(width: 20)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text(violation.title)
                    .font(.subheadline.weight(.semibold))
                Text(violation.detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text(violation.recommendation)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                Text(formatBytes(violation.observedBytes))
                    .font(.caption.weight(.semibold))
                Text("limit \(formatBytes(violation.limitBytes))")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.sm)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func agentHygieneSection(_ report: StorageHygieneReportModel) -> some View {
        let hygiene = report.agentHygiene
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "sparkles.rectangle.stack")
                    .foregroundStyle(AetowerDesign.Tone.energy)
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Agent-aware hygiene")
                        .font(.headline)
                    Text("Per-agent storage cost for artifacts Aetower can directly tie to AI sessions, commands, process trees, or known local agent directories.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 180), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "This week",
                    value: formatBytes(hygiene.weekAgentArtifactBytes),
                    detail: "\(formatPercent(hygiene.weekRebuildableAgentPercent)) rebuildable"
                )
                footprintMetric(
                    "All agent artifacts",
                    value: formatBytes(hygiene.totalAgentArtifactBytes),
                    detail: "\(formatPercent(hygiene.rebuildableAgentPercent)) rebuildable"
                )
                footprintMetric(
                    "Attributed",
                    value: "\(hygiene.agentCount)",
                    detail: "\(hygiene.attributedItemCount) item\(hygiene.attributedItemCount == 1 ? "" : "s")"
                )
            }

            if hygiene.agents.isEmpty {
                Label("No agent-attributed storage artifacts were found in this scan.", systemImage: "person.crop.circle.badge.questionmark")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(hygiene.agents) { agent in
                        agentHygieneCard(agent)
                    }
                }
            }

            ForEach(hygiene.caveats.prefix(2), id: \.self) { caveat in
                Label(caveat, systemImage: "info.circle")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func agentHygieneCard(_ agent: StorageAgentArtifactSummaryModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "person.crop.circle.badge.gearshape")
                    .foregroundStyle(AetowerDesign.agentColor(agent.provider))
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(agent.displayName)
                            .font(.subheadline.weight(.semibold))
                        Text(agent.confidence.capitalized)
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(agentConfidenceTone(agent.confidence))
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(agentConfidenceTone(agent.confidence).opacity(0.12), in: Capsule())
                    }
                    Text(agentSourceSummary(agent))
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                    if let sessionId = agent.sessionId {
                        Text(sessionId)
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(agent.weekArtifactBytes))
                        .font(.system(size: 20, weight: .semibold, design: .rounded))
                    Text("this week")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 145), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "Total",
                    value: formatBytes(agent.artifactBytes),
                    detail: "\(agent.itemCount) item\(agent.itemCount == 1 ? "" : "s")"
                )
                footprintMetric(
                    "Rebuildable",
                    value: formatBytes(agent.rebuildableBytes),
                    detail: "\(formatPercent(agent.rebuildablePercent)) of total"
                )
                footprintMetric(
                    "Week rebuildable",
                    value: formatBytes(agent.weekRebuildableBytes),
                    detail: "\(formatPercent(agent.weekRebuildablePercent)) this week"
                )
                footprintMetric(
                    "Repos",
                    value: "\(agent.repoCount)",
                    detail: agent.topRepositories.first?.repoName ?? "no repo link"
                )
            }

            if !agent.topRepositories.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Top repositories")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ForEach(Array(agent.topRepositories.prefix(3))) { repo in
                        HStack(spacing: AetowerDesign.Spacing.sm) {
                            Text(repo.repoName)
                                .font(.caption.weight(.semibold))
                                .lineLimit(1)
                            Spacer()
                            Text("\(formatBytes(repo.artifactBytes)) · \(repo.itemCount) item\(repo.itemCount == 1 ? "" : "s")")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }

            if !agent.topItems.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Top artifacts")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ForEach(Array(agent.topItems.prefix(3))) { item in
                        HStack(spacing: AetowerDesign.Spacing.sm) {
                            Image(systemName: cleanupTierIcon(item.cleanupTier))
                                .foregroundStyle(tone(forCleanupTier: item.cleanupTier))
                                .frame(width: 16)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.displayName)
                                    .font(.caption.weight(.semibold))
                                Text(item.path)
                                    .font(.system(size: 10, design: .monospaced))
                                    .foregroundStyle(.tertiary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }
                            Spacer()
                            Text(formatBytes(item.sizeBytes))
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.secondary)
                            Button("Quick Look") { quickLook(path: item.path) }
                            Button("Reveal") { reveal(path: item.path) }
                            Button("Explain") { classificationExplanation = explanation(for: item) }
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.mini)
                    }
                }
            }

            Text(agent.recommendation)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func cleanupPreviewSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Safe cleanup preview")
                    .font(.headline)
                Text("Aetower classifies candidates by cleanup risk. This view never deletes files; it only explains what should be safe, rebuildable, expensive, or risky.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 210), spacing: AetowerDesign.Spacing.md)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.md
            ) {
                ForEach(report.cleanupTiers) { tier in
                    cleanupTierCard(tier)
                }
            }
        }
    }

    private func cleanupBundlesSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Dry-run cleanup bundles")
                    .font(.headline)
                Text("One-click cleanup bundles with a full manifest, confidence score, verification commands, candidate commands, and rollback notes.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if report.cleanupBundles.isEmpty {
                Label("No cleanup bundle can be built from the current scan.", systemImage: "shippingbox.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(report.cleanupBundles) { bundle in
                        cleanupBundleCard(bundle)
                    }
                }
            }
        }
    }

    private func cleanupBundleCard(_ bundle: StorageCleanupBundleModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: cleanupBundleIcon(bundle))
                    .foregroundStyle(cleanupBundleTone(bundle))
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(bundle.title)
                            .font(.subheadline.weight(.semibold))
                        Text("\(bundle.confidenceScore)% confidence")
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(cleanupBundleTone(bundle))
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(cleanupBundleTone(bundle).opacity(0.12), in: Capsule())
                        if bundle.dryRunOnly {
                            Text("Dry-run")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Surface.badge, in: Capsule())
                        }
                    }
                    Text(bundle.subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(bundle.estimatedReclaimableBytes))
                        .font(.system(size: 20, weight: .semibold, design: .rounded))
                    Text("\(bundle.itemCount) item\(bundle.itemCount == 1 ? "" : "s")")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            recommendationDecisionGrid(bundleRecommendationDecisions(bundle))

            if !bundle.manifest.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Manifest preview")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ForEach(Array(bundle.manifest.prefix(4))) { item in
                        HStack(spacing: AetowerDesign.Spacing.sm) {
                            Image(systemName: cleanupTierIcon(item.cleanupTier))
                                .foregroundStyle(tone(forCleanupTier: item.cleanupTier))
                                .frame(width: 16)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.displayName)
                                    .font(.caption.weight(.semibold))
                                Text(item.path)
                                    .font(.system(size: 10, design: .monospaced))
                                    .foregroundStyle(.tertiary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }
                            Spacer()
                            Text(formatBytes(item.sizeBytes))
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }

            if !bundle.rollbackNotes.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Array(bundle.rollbackNotes.prefix(3)), id: \.self) { note in
                        Label(note, systemImage: "arrow.uturn.backward.circle")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                if cleanupBundleHasActionableCommands(bundle) {
                    Button {
                        stageCleanupBundle(bundle)
                    } label: {
                        Label("Stage bundle", systemImage: "tray.and.arrow.down")
                    }
                    .buttonStyle(.borderedProminent)
                }
                // Command text is a manual reference only (execution is always
                // Finder Trash), so the copy/reference actions live in one
                // overflow menu instead of competing with the real action.
                Menu {
                    Button("Copy plan") {
                        copy(cleanupBundleManifest(bundle))
                        copiedCleanupBundleID = bundle.id
                    }
                    Button("Copy verify commands") {
                        copy(bundle.dryRunCommands.joined(separator: "\n"))
                        copiedCleanupBundleID = bundle.id
                    }
                    .disabled(bundle.dryRunCommands.isEmpty)
                    if cleanupBundleHasActionableCommands(bundle) {
                        Button("Review command references") {
                            candidateCommandPreviewBundle = bundle
                        }
                    }
                } label: {
                    Label("References", systemImage: "doc.on.doc")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                Spacer()
                Text(copiedCleanupBundleID == bundle.id ? "Copied" : "Ready")
                    .font(.caption2)
                    .foregroundStyle(copiedCleanupBundleID == bundle.id ? AetowerDesign.Status.ready : .secondary)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func cleanupCommandPreviewSheet(_ bundle: StorageCleanupBundleModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "exclamationmark.triangle")
                    .foregroundStyle(AetowerDesign.Status.warning)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Review command references")
                        .font(.title3.weight(.semibold))
                    Text("These commands are references only. Aetower cleanup execution moves staged targets to Finder Trash.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
            }

            ScrollView {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    footprintMetric(
                        "Planned reclaim",
                        value: formatBytes(bundle.estimatedReclaimableBytes),
                        detail: "\(bundle.confidenceScore)% confidence · \(bundle.itemCount) item\(bundle.itemCount == 1 ? "" : "s")"
                    )

                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Command references")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        Text(cleanupBundleCleanupCommands(bundle))
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .padding(AetowerDesign.Spacing.sm)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    }

                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Manifest preview")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        ForEach(Array(bundle.manifest.prefix(8))) { item in
                            HStack(spacing: AetowerDesign.Spacing.sm) {
                                Image(systemName: cleanupTierIcon(item.cleanupTier))
                                    .foregroundStyle(tone(forCleanupTier: item.cleanupTier))
                                    .frame(width: 16)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(item.path)
                                        .font(.system(size: 10, design: .monospaced))
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                    Text(item.rollbackNote)
                                        .font(.caption2)
                                        .foregroundStyle(.tertiary)
                                        .lineLimit(2)
                                }
                                Spacer()
                                Text(formatBytes(item.sizeBytes))
                                    .font(.caption2.weight(.semibold))
                                    .foregroundStyle(.secondary)
                                Button("Quick Look") { quickLook(path: item.path) }
                                Button("Reveal") { reveal(path: item.path) }
                                Button("Explain") { classificationExplanation = explanation(for: item) }
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.mini)
                            .padding(.vertical, 2)
                        }
                    }
                }
                .padding(.trailing, AetowerDesign.Spacing.sm)
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Cancel") {
                    candidateCommandPreviewBundle = nil
                }
                Spacer()
                Button("Stage bundle") {
                    candidateCommandPreviewBundle = nil
                    stageCleanupBundle(bundle)
                }
                .buttonStyle(.borderedProminent)

                Button("Copy commands") {
                    copy(cleanupBundleCleanupCommands(bundle))
                    copiedCleanupBundleID = bundle.id
                    candidateCommandPreviewBundle = nil
                }
                .disabled(cleanupBundleCleanupCommandList(bundle).isEmpty)
            }
        }
        .padding(AetowerDesign.Spacing.xl)
        .frame(width: 720, height: 560, alignment: .topLeading)
    }

    private func cleanupExecutionSheet(_ request: StorageCleanupExecutionRequest) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "trash")
                    .foregroundStyle(AetowerDesign.Status.warning)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text(request.title)
                        .font(.title3.weight(.semibold))
                    Text(request.subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
                if cleanupExecutionIsRunning {
                    ProgressView()
                        .controlSize(.small)
                }
            }

            ScrollView {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 160), spacing: AetowerDesign.Spacing.sm)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.sm
                    ) {
                        footprintMetric(
                            "Potential reclaim",
                            value: formatBytes(request.estimatedBytes),
                            detail: request.requiresReview ? "manual review" : "ready"
                        )
                        footprintMetric(
                            "Action",
                            value: "Trash",
                            detail: cleanupExecutionIsRunning ? "running" : "waiting"
                        )
                        if let result = cleanupExecutionResult {
                            footprintMetric(
                                "Moved",
                                value: "\(result.movedPaths.count)/\(result.movedPaths.count + result.failedPaths.count)",
                                detail: result.succeeded
                                    ? "success"
                                    : result.partiallySucceeded ? "partial" : "failed"
                            )
                        }
                    }

                    if !request.prerequisites.isEmpty {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                            Text("Before running")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                            ForEach(request.prerequisites.prefix(8), id: \.self) { prerequisite in
                                Label(prerequisite, systemImage: "checklist")
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }

                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Targets")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        Text(targetPathList(request.targetPaths))
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .padding(AetowerDesign.Spacing.sm)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    }

                    if let result = cleanupExecutionResult {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                            Text(
                                result.succeeded
                                    ? "Result: \(result.movedPaths.count) moved to Trash"
                                    : result.partiallySucceeded
                                        ? "Result: \(result.movedPaths.count) moved to Trash · \(result.failedPaths.count) failed"
                                        : "Result: needs attention"
                            )
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(
                                result.succeeded
                                    ? AetowerDesign.Status.ready
                                    : result.partiallySucceeded
                                        ? AetowerDesign.Status.warning
                                        : AetowerDesign.Status.error
                            )
                            Text(result.output.isEmpty ? "Cleanup completed with no output." : result.output)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                                .padding(AetowerDesign.Spacing.sm)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                            Text(String(format: "%.1fs", result.durationSeconds))
                                .font(.caption2)
                                .foregroundStyle(.tertiary)

                            if !result.movedPaths.isEmpty {
                                // The step users miss: items in the Trash still
                                // occupy disk until the Trash is emptied.
                                HStack(spacing: AetowerDesign.Spacing.sm) {
                                    Label(
                                        "Items are in the Trash — disk space frees when you empty it.",
                                        systemImage: "info.circle"
                                    )
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    Button("Open Trash") { openTrash() }
                                        .buttonStyle(.bordered)
                                        .controlSize(.small)
                                    Button("Empty Trash…") { requestEmptyTrashConfirmation() }
                                        .buttonStyle(.borderedProminent)
                                        .controlSize(.small)
                                        .disabled(emptyTrashInFlight)
                                }
                                .padding(AetowerDesign.Spacing.sm)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(
                                    AetowerDesign.Status.warning.opacity(0.08),
                                    in: RoundedRectangle(cornerRadius: 10, style: .continuous)
                                )
                            }
                        }
                    }
                }
                .padding(.trailing, AetowerDesign.Spacing.sm)
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button(cleanupExecutionResult == nil ? "Cancel" : "Close") {
                    pendingCleanupExecutionRequest = nil
                    cleanupExecutionResult = nil
                    cleanupExecutionIsRunning = false
                }
                .disabled(cleanupExecutionIsRunning)

                if let targetPath = request.targetPath {
                    Button("Reveal target") {
                        reveal(path: targetPath)
                    }
                }

                Button("Copy targets") {
                    copy(request.targetPaths.joined(separator: "\n"))
                }

                Spacer()

                Button(cleanupExecutionIsRunning ? "Running..." : cleanupExecutionButtonTitle(request)) {
                    runCleanupExecution(request)
                }
                .buttonStyle(.borderedProminent)
                .disabled(cleanupExecutionIsRunning || !cleanupExecutionCanRun(request))
            }
        }
        .padding(AetowerDesign.Spacing.xl)
        .frame(width: 760, height: 620, alignment: .topLeading)
        .confirmationDialog(
            "Delete Aetower-tracked Trash items?",
            isPresented: $confirmEmptyTrash,
            titleVisibility: .visible
        ) {
            Button("Delete Aetower-tracked items", role: .destructive) { emptyTrash() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This permanently deletes only items Aetower moved to the Trash and still tracks. Unrelated Finder Trash contents are left alone.")
        }
    }

    private func cleanupRecipesSection(_ report: StorageHygieneReportModel) -> some View {
        DisclosureGroup(isExpanded: $showCleanupRecipes) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                Text("Exact commands for common cleanup tasks. Copy and execute only after reviewing prerequisites, active processes, and rollback notes.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                if report.cleanupRecipes.isEmpty {
                    Label("No cleanup recipes match the current scan.", systemImage: "wand.and.stars.inverse")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(AetowerDesign.Spacing.md)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                } else {
                    LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        ForEach(report.cleanupRecipes) { recipe in
                            cleanupRecipeCard(recipe)
                        }
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        } label: {
            advancedSectionLabel(
                title: "Cleanup recipes",
                detail: "\(report.cleanupRecipes.count) command recipe\(report.cleanupRecipes.count == 1 ? "" : "s")",
                systemImage: "terminal"
            )
        }
    }

    private var cleanupAuditSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Cleanup audit")
                    .font(.headline)
                Text("Local record of staged, blocked, trashed, override, and failed cleanup actions.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if cleanupAuditEvents.isEmpty {
                Label("No cleanup actions have been recorded yet.", systemImage: "doc.text.magnifyingglass")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    ForEach(Array(cleanupAuditEvents.prefix(12))) { event in
                        HStack(spacing: AetowerDesign.Spacing.sm) {
                            Image(systemName: event.succeeded == false ? "exclamationmark.triangle" : "checkmark.circle")
                                .foregroundStyle(event.succeeded == false ? AetowerDesign.Status.error : AetowerDesign.Status.ready)
                                .frame(width: 18)
                            VStack(alignment: .leading, spacing: 2) {
                                HStack(spacing: AetowerDesign.Spacing.xs) {
                                    Text(event.action)
                                        .font(.caption.weight(.semibold))
                                    Text(formatBytes(event.bytes))
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                }
                                Text(event.path)
                                    .font(.system(size: 10, design: .monospaced))
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                Text(event.detail)
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                                    .lineLimit(2)
                            }
                            Spacer()
                        }
                        .padding(AetowerDesign.Spacing.sm)
                        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                }
            }
        }
    }

    private func cleanupRecipeCard(_ recipe: StorageCleanupRecipeModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: cleanupRecipeIcon(recipe))
                    .foregroundStyle(cleanupRecipeTone(recipe))
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(recipe.title)
                            .font(.subheadline.weight(.semibold))
                        cleanupRecipeBadge(recipe)
                        if recipe.requiresReview {
                            Text("Review")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Status.warning)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Status.warning.opacity(0.12), in: Capsule())
                        }
                    }

                    Text(recipe.reason)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)

                    Text(recipe.command)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .lineLimit(2)
                        .truncationMode(.middle)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(recipe.estimatedReclaimableBytes))
                        .font(.system(size: 18, weight: .semibold, design: .rounded))
                    Text(recipe.category)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            if !recipe.prerequisites.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(recipe.prerequisites, id: \.self) { prerequisite in
                        Label(prerequisite, systemImage: "checklist")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Copy command") { copy(recipe.command) }
                Button("Stage") { stageCleanupRecipe(recipe) }
                Button("Quick Look") { quickLook(path: recipe.affectedPath) }
                Button("Reveal target") { reveal(path: recipe.affectedPath) }
                Button("Explain") { classificationExplanation = explanation(for: recipe) }
                Spacer()
                Text(recipe.destructive ? "command reference" : "verification command")
                    .font(.caption2)
                    .foregroundStyle(recipe.destructive ? AetowerDesign.Status.warning : .secondary)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func repoFootprintDashboard(_ report: StorageHygieneReportModel) -> some View {
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Repo footprint dashboard")
                    .font(.headline)
                Text("Per repository artifact footprint, top growth sources, branch context, last writer evidence, and estimated rebuild cost.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if report.repoFootprints.isEmpty {
                Label("No artifacts could be tied to an enclosing Git repository.", systemImage: "questionmark.folder")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(report.repoFootprints) { footprint in
                        repoFootprintCard(footprint)
                    }
                }
            }
        }
    }

    private func repoFootprintCard(_ footprint: StorageRepoFootprintModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "folder.badge.gearshape")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(footprint.repoName)
                            .font(.subheadline.weight(.semibold))
                        if let branch = footprint.lastBranchTouched {
                            Text(branch)
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Status.ready)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Status.ready.opacity(0.12), in: Capsule())
                        }
                    }
                    Text(footprint.repoRoot)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Text(footprint.optimizationSummary)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(footprint.currentSizeBytes))
                        .font(.system(size: 20, weight: .semibold, design: .rounded))
                    Text(storageGrowthLabel(for: footprint))
                        .font(.caption2)
                        .foregroundStyle(storageGrowthTone(for: footprint))
                }
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "Artifacts",
                    value: "\(footprint.itemCount)",
                    detail: formatBytes(footprint.artifactBytes)
                )
                footprintMetric(
                    "Rebuild cost",
                    value: footprint.estimatedRebuildCost,
                    detail: rebuildTimeLabel(footprint.estimatedRebuildSeconds)
                )
                footprintMetric(
                    "Rebuildable",
                    value: formatBytes(footprint.rebuildableBytes),
                    detail: "\(formatPercent(footprint.rebuildablePercent)) of artifacts"
                )
                footprintMetric(
                    "Costly/Risky",
                    value: formatBytes(sumBytes(footprint.expensiveBytes, footprint.riskyBytes)),
                    detail: "\(formatBytes(footprint.expensiveBytes)) expensive · \(formatBytes(footprint.riskyBytes)) risky"
                )
                footprintMetric(
                    "Last writer",
                    value: footprint.lastWriterProcess ?? "Unknown",
                    detail: footprint.lastWriterPid.map { "pid \($0)" } ?? "needs file-event journal"
                )
                footprintMetric(
                    "Growth",
                    value: storageGrowthCompactValue(for: footprint),
                    detail: storageGrowthWindow(for: footprint)
                )
                footprintMetric(
                    "Clone group",
                    value: footprint.duplicateCloneCount > 1 ? "\(footprint.duplicateCloneCount) clones" : "Unique",
                    detail: footprint.duplicateCloneRoots.prefix(2).joined(separator: " · ")
                )
            }

            if !footprint.artifactMix.isEmpty {
                StorageArtifactMixList(artifactMix: footprint.artifactMix)
            }

            if !footprint.topArtifactFolders.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Top artifact folders")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ForEach(footprint.topArtifactFolders.prefix(3)) { folder in
                        HStack(spacing: AetowerDesign.Spacing.sm) {
                            Image(systemName: cleanupTierIcon(folder.cleanupTier))
                                .foregroundStyle(tone(forCleanupTier: folder.cleanupTier))
                                .frame(width: 16)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(folder.displayName)
                                    .font(.caption.weight(.semibold))
                                Text(folder.path)
                                    .font(.system(size: 10, design: .monospaced))
                                    .foregroundStyle(.tertiary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }
                            Spacer()
                            Text(formatBytes(folder.sizeBytes))
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func storageGrowthTimeline(_ report: StorageHygieneReportModel) -> some View {
        let events = storageGrowthEvents(from: report)
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Why did disk usage jump?")
                    .font(.headline)
                Text("Positive artifact-size deltas since the previous scan, correlated with repo, branch, command, process tree, and AI session when the scan has direct evidence.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if state.previousStorageHygieneReport == nil, state.persistedStorageHygieneBaseline == nil {
                Label("Run a second scan to establish a growth timeline. Aetower will persist a compact baseline for future launches.", systemImage: "timeline.selection")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else if events.isEmpty {
                Label("No meaningful storage jumps were detected since the last baseline.", systemImage: "checkmark.circle")
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.ready)
                    .padding(AetowerDesign.Spacing.md)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(events) { event in
                        storageGrowthTimelineRow(event)
                    }
                }
            }
        }
    }

    private func storageGrowthTimelineRow(_ event: StorageGrowthTimelineEvent) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
            VStack(spacing: 4) {
                Circle()
                    .fill(tone(forCleanupTier: event.cleanupTier))
                    .frame(width: 9, height: 9)
                Rectangle()
                    .fill(Color.secondary.opacity(0.18))
                    .frame(width: 1)
            }
            .frame(width: 18)

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Text(storageGrowthEventTime(event))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text("+\(formatBytes(UInt64(event.deltaBytes)))")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(AetowerDesign.Status.warning)
                    if let branch = event.branch {
                        Text(branch)
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(AetowerDesign.Status.ready)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 3)
                            .background(AetowerDesign.Status.ready.opacity(0.12), in: Capsule())
                    }
                    AetowerBadge(
                        storageGrowthConfidenceLabel(event),
                        tone: storageGrowthConfidenceTone(event)
                    )
                }

                Text(storageGrowthEventTitle(event))
                    .font(.subheadline.weight(.semibold))
                    .fixedSize(horizontal: false, vertical: true)

                Text(event.path)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .lineLimit(1)
                    .truncationMode(.middle)

                Text(storageGrowthCorrelationDetail(event))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
                if !event.attributionEvidence.isEmpty {
                    Text(event.attributionEvidence.prefix(2).joined(separator: " "))
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            Spacer(minLength: AetowerDesign.Spacing.md)

            VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                Text(formatBytes(event.currentBytes))
                    .font(.caption.weight(.semibold))
                Text("was \(formatBytes(event.previousBytes))")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func storageGrowthInsightsSection(_ insights: StorageGrowthInsightsModel) -> some View {
        let topRepo = insights.perRepoRates.first
        let topRoot = insights.perRootRates.first
        let forecast = insights.volumeForecasts.first
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text("Growth rate and forecast")
                    .font(.headline)
                Text("Daily rates aggregated from the full \(insights.windowDays)-day growth history in the persistent index, with a half-window trend and a days-to-full forecast once three days of history exist.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 190), spacing: AetowerDesign.Spacing.md)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.md
            ) {
                AetowerMetricTile(
                    "Top repo growth",
                    value: topRepo.map { "\(storageSignedBytes($0.dailyRateBytes))/day" } ?? "No data",
                    detail: topRepo.map { "\($0.repoName ?? $0.scope) · \($0.trend)" } ?? "no repo growth recorded",
                    systemImage: topRepo.map { storageGrowthTrendIcon($0.trend) } ?? "chart.line.flattrend.xyaxis",
                    tone: topRepo.map { storageGrowthTrendTone($0.trend) } ?? AetowerDesign.Status.neutral
                )
                AetowerMetricTile(
                    "Top root growth",
                    value: topRoot.map { "\(storageSignedBytes($0.dailyRateBytes))/day" } ?? "No data",
                    detail: topRoot.map { "\(lastPathComponent($0.scope)) · \($0.trend)" } ?? "no root growth recorded",
                    systemImage: topRoot.map { storageGrowthTrendIcon($0.trend) } ?? "chart.line.flattrend.xyaxis",
                    tone: topRoot.map { storageGrowthTrendTone($0.trend) } ?? AetowerDesign.Status.neutral
                )
                AetowerMetricTile(
                    "Disk-full forecast",
                    value: forecast.map { "~\(Int($0.daysToFull.rounded())) days" } ?? "Pending",
                    detail: forecast.map(storageForecastDetail(_:))
                        ?? "needs 3+ days of growth history and a positive rate",
                    systemImage: "externaldrive.badge.exclamationmark",
                    tone: forecast.map { $0.daysToFull < 30 ? AetowerDesign.Status.warning : AetowerDesign.Tone.disk }
                        ?? AetowerDesign.Status.neutral
                )
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func storageSinceLastScanSection(_ diff: StorageScanDiffModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Since last scan")
                        .font(.headline)
                    Text("Newly appeared paths and cleanup-tier transitions from the latest scan generation.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                AetowerBadge(
                    "\(diff.appearedCount) appeared · \(formatBytes(diff.appearedTotalBytes))",
                    tone: diff.appearedCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
                )
                AetowerBadge(
                    "\(diff.tierChangedCount) tier changed",
                    tone: diff.tierChangedCount > 0 ? AetowerDesign.Tone.memory : AetowerDesign.Status.ready
                )
            }

            if diff.appeared.isEmpty && diff.tierChanged.isEmpty {
                Label("No new paths or tier transitions in the latest scan generation.", systemImage: "checkmark.circle")
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.ready)
            } else {
                if !diff.appeared.isEmpty {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Appeared")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        ForEach(diff.appeared.prefix(6)) { entry in
                            storageScanDiffRow(entry, badge: "+\(formatBytes(UInt64(max(0, entry.deltaBytes))))")
                        }
                    }
                }
                if !diff.tierChanged.isEmpty {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Tier changed")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        ForEach(diff.tierChanged.prefix(6)) { entry in
                            storageScanDiffRow(
                                entry,
                                badge: "\(cleanupTierLabel(entry.previousCleanupTier)) → \(cleanupTierLabel(entry.cleanupTier))"
                            )
                        }
                    }
                }
            }

            if !diff.disappearedNote.isEmpty {
                Text(diff.disappearedNote)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func storageScanDiffRow(_ entry: StorageScanDiffEntryModel, badge: String) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: cleanupTierIcon(entry.cleanupTier))
                .foregroundStyle(tone(forCleanupTier: entry.cleanupTier))
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.displayName)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                Text(entry.path)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: AetowerDesign.Spacing.sm)
            AetowerBadge(badge, tone: tone(forCleanupTier: entry.cleanupTier))
            Text(formatBytes(entry.physicalBytes))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
                .frame(width: 72, alignment: .trailing)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func storageGrowthTrendIcon(_ trend: String) -> String {
        switch trend {
        case "accelerating": return "arrow.up.right"
        case "slowing": return "arrow.down.right"
        case "shrinking": return "arrow.down"
        default: return "arrow.right"
        }
    }

    private func storageGrowthTrendTone(_ trend: String) -> Color {
        switch trend {
        case "accelerating": return AetowerDesign.Status.warning
        case "shrinking": return AetowerDesign.Status.ready
        default: return AetowerDesign.Tone.disk
        }
    }

    private func storageForecastDetail(_ forecast: StorageGrowthForecastModel) -> String {
        var parts = [
            "\(forecast.volumePath) \(Int(forecast.daysToFullLowerBound.rounded()))-\(Int(forecast.daysToFullUpperBound.rounded()))d",
            "\(storageSignedBytes(forecast.dailyRateBytes))/day",
            forecast.seasonalPattern,
            forecast.confidence,
        ]
        if forecast.cloudGrowthSharePercent > 0 {
            parts.append("cloud \(forecast.cloudGrowthSharePercent)%")
        }
        if forecast.purgeableBytesEstimate > 0 {
            parts.append("purgeable \(formatBytes(forecast.purgeableBytesEstimate))")
        }
        return parts.joined(separator: " · ")
    }

    private func coldDataLaneSection(_ coldData: StorageColdDataModel) -> some View {
        let totalBytes = coldData.bands.reduce(UInt64(0)) { total, band in
            sumBytes(total, band.totalBytes)
        }
        let totalCount = coldData.bands.reduce(UInt64(0)) { $0 + $1.itemCount }

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "snowflake")
                    .foregroundStyle(AetowerDesign.Tone.memory)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Cold data")
                        .font(.headline)
                    Text("Safe and rebuildable items untouched for months, ranked by size within each band. \(coldData.caveat)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(totalBytes))
                        .font(.system(size: 22, weight: .semibold, design: .rounded))
                    Text("\(totalCount) cold item\(totalCount == 1 ? "" : "s")")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Text("Sort top items")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Picker("", selection: $coldDataSort) {
                    ForEach(StorageColdDataSort.allCases) { sort in
                        Text(sort.label).tag(sort)
                    }
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .frame(width: 220)
                Spacer()
            }

            ForEach(coldData.bands) { band in
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(band.label)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        AetowerBadge("\(band.itemCount) item\(band.itemCount == 1 ? "" : "s")", tone: AetowerDesign.Tone.memory)
                        AetowerBadge(formatBytes(band.totalBytes), tone: AetowerDesign.Tone.disk)
                        Spacer()
                    }
                    if band.topItems.isEmpty {
                        Text("No items in this band.")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    } else {
                        ForEach(coldDataSort.sorted(band.topItems)) { item in
                            storageExplorerTableRow(item, showScore: coldDataSort == .recommended)
                        }
                    }
                }
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func visualExplorationSection(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                Label("Full disk map and advanced explorer", systemImage: "square.grid.3x3.topleft.filled")
                    .font(.headline)
                Spacer()
                Picker("", selection: $storageVisualExplorerMode) {
                    ForEach(StorageVisualExplorerMode.allCases) { mode in
                        Text(mode.label).tag(mode)
                    }
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .frame(width: 220)
            }

            Text("The table opens first for responsiveness. Switch to Full disk or Treemap when you want the heavier visual map.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if storageVisualExplorerMode == .fullDisk {
                storageFullDiskExplorer(report)
            } else if storageVisualExplorerMode == .treemap {
                storageTreemapExplorer(report)
            } else {
                storageExplorerTable(report)
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func storageFullDiskExplorer(_ report: StorageHygieneReportModel) -> some View {
        let breadcrumbs = storageTreemapBreadcrumbs(in: report.treemapRoots)
        let selectedNode = storageTreemapSelectedNode(in: report.treemapRoots)
        let nodes = selectedNode?.children ?? report.treemapRoots
        let totalBytes = nodes.reduce(UInt64(0)) { total, node in
            sumBytes(total, node.sizeBytes)
        }
        let preferredUnitBytes = storageCubePreferredUnitBytes(for: selectedNode)
        let projection = storageCubeProjection(for: nodes, preferredUnitBytes: preferredUnitBytes)
        let cubeBinsByNodeID = Dictionary(
            uniqueKeysWithValues: storageCubeNodeBins(nodes: nodes, projection: projection).map { ($0.id, $0) }
        )

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            storageTreemapNavigationBar(
                breadcrumbs: breadcrumbs,
                blockCount: nodes.count,
                totalBytes: totalBytes,
                totalLabel: selectedNode?.label ?? "Full disk"
            )

            if nodes.isEmpty {
                ContentUnavailableView(
                    "No full disk map yet",
                    systemImage: "square.grid.3x3",
                    description: Text("Run a scan with storage candidates, then return to Full disk.")
                )
                .frame(maxWidth: .infinity)
            } else {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        AetowerBadge("1 cube = \(formatBytes(projection.unitBytes))", tone: AetowerDesign.Tone.disk)
                        if projection.unitBytes > preferredUnitBytes {
                            AetowerBadge("scaled from \(formatBytes(preferredUnitBytes))", tone: AetowerDesign.Status.warning)
                        }
                        AetowerBadge("\(projection.totalCubes) cube\(projection.totalCubes == 1 ? "" : "s")", tone: AetowerDesign.Tone.memory)
                        AetowerBadge("rounded up", tone: AetowerDesign.Status.warning)
                        Spacer()
                    }

                    GeometryReader { proxy in
                        let bounds = CGRect(origin: .zero, size: proxy.size)
                        let layouts = storageProportionalTreemapLayouts(for: nodes, in: bounds)
                        ZStack(alignment: .topLeading) {
                            ForEach(layouts) { layout in
                                storageFullDiskBlock(
                                    layout,
                                    displayedTotalBytes: totalBytes,
                                    cubeBin: cubeBinsByNodeID[layout.node.id]
                                )
                            }
                        }
                        .frame(width: proxy.size.width, height: proxy.size.height)
                    }
                    .frame(minHeight: 360, idealHeight: 460)
                    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))

                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Label("Area is proportional to displayed bytes", systemImage: "ruler")
                        Text("Cubes show rounded byte units inside each folder. Click a folder to drill down; click a leaf to reveal it in Finder.")
                    }
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                }
            }
        }
    }

    private func storageCubeExplorer(_ report: StorageHygieneReportModel) -> some View {
        let breadcrumbs = storageTreemapBreadcrumbs(in: report.treemapRoots)
        let selectedNode = storageTreemapSelectedNode(in: report.treemapRoots)
        let nodes = selectedNode?.children ?? report.treemapRoots
        let preferredUnitBytes = storageCubePreferredUnitBytes(for: selectedNode)
        let projection = storageCubeProjection(for: nodes, preferredUnitBytes: preferredUnitBytes)
        let bins = storageCubeNodeBins(nodes: nodes, projection: projection)

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            storageTreemapNavigationBar(
                breadcrumbs: breadcrumbs,
                blockCount: bins.count,
                totalBytes: projection.totalBytes,
                totalLabel: selectedNode?.label ?? "Full disk"
            )

            if bins.isEmpty {
                ContentUnavailableView(
                    "No cube map yet",
                    systemImage: "square.grid.3x3",
                    description: Text("Run a scan with storage candidates, then return to Cubes.")
                )
                .frame(maxWidth: .infinity)
            } else {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        AetowerBadge("1 cube = \(formatBytes(projection.unitBytes))", tone: AetowerDesign.Tone.disk)
                        if projection.unitBytes > preferredUnitBytes {
                            AetowerBadge("scaled from \(formatBytes(preferredUnitBytes))", tone: AetowerDesign.Status.warning)
                        }
                        AetowerBadge("\(projection.totalCubes) cube\(projection.totalCubes == 1 ? "" : "s")", tone: AetowerDesign.Tone.memory)
                        AetowerBadge("rounded up", tone: AetowerDesign.Status.warning)
                        Spacer()
                    }

                    GeometryReader { proxy in
                        let layout = storageCubeGridLayout(
                            cubeCount: projection.totalCubes,
                            in: proxy.size
                        )
                        Canvas { context, _ in
                            for bin in bins {
                                let color = storageTreemapColor(bin.node.colorKey)
                                for index in bin.startIndex..<bin.endIndex {
                                    let rect = storageCubeRect(index: index, layout: layout)
                                    guard rect.width > 0, rect.height > 0 else { continue }
                                    context.fill(
                                        Path(roundedRect: rect, cornerRadius: min(3, rect.width / 4)),
                                        with: .color(color.opacity(0.82))
                                    )
                                }
                            }
                        }
                        .background(
                            AetowerDesign.Surface.rowIdle,
                            in: RoundedRectangle(cornerRadius: 18, style: .continuous)
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: 18, style: .continuous)
                                .stroke(AetowerDesign.Surface.divider, lineWidth: AetowerDesign.Stroke.hairline)
                        )
                        .gesture(
                            DragGesture(minimumDistance: 0)
                                .onEnded { value in
                                    guard let index = storageCubeIndex(at: value.location, layout: layout),
                                          let bin = bins.first(where: { $0.contains(index) })
                                    else { return }
                                    if !bin.node.children.isEmpty {
                                        selectedTreemapNodeID = bin.node.id
                                    } else {
                                        reveal(path: bin.node.path)
                                    }
                                }
                        )
                        .help("Click a cube group to drill into folders; leaf cubes reveal in Finder.")
                    }
                    .frame(minHeight: 360, idealHeight: 460)

                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 180), spacing: AetowerDesign.Spacing.sm)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.sm
                    ) {
                        ForEach(bins.prefix(12)) { bin in
                            Button {
                                if !bin.node.children.isEmpty {
                                    selectedTreemapNodeID = bin.node.id
                                } else {
                                    reveal(path: bin.node.path)
                                }
                            } label: {
                                HStack(spacing: AetowerDesign.Spacing.xs) {
                                    Circle()
                                        .fill(storageTreemapColor(bin.node.colorKey))
                                        .frame(width: 8, height: 8)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(bin.node.label)
                                            .font(AetowerDesign.Typography.caption.weight(.semibold))
                                            .lineLimit(1)
                                        Text("\(bin.cubeCount) cube\(bin.cubeCount == 1 ? "" : "s") · \(formatBytes(bin.node.sizeBytes))")
                                            .font(AetowerDesign.Typography.caption)
                                            .foregroundStyle(AetowerDesign.Ink.secondary)
                                            .lineLimit(1)
                                    }
                                    Spacer(minLength: AetowerDesign.Spacing.xs)
                                }
                                .padding(.horizontal, AetowerDesign.Spacing.sm)
                                .padding(.vertical, AetowerDesign.Spacing.xs)
                                .background(AetowerDesign.Surface.badge, in: Capsule())
                            }
                            .buttonStyle(.plain)
                        }
                    }

                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Label("Cube counts are rounded up", systemImage: "cube")
                        Text("Root starts around 50 MB per cube; drill down for 10 MB and smaller units. Very large scopes auto-scale up to stay responsive.")
                    }
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                }
            }
        }
    }

    private func storageTreemapExplorer(_ report: StorageHygieneReportModel) -> some View {
        let breadcrumbs = storageTreemapBreadcrumbs(in: report.treemapRoots)
        let selectedNode = storageTreemapSelectedNode(in: report.treemapRoots)
        let nodes = selectedNode?.children ?? report.treemapRoots
        let totalBytes = nodes.reduce(UInt64(0)) { total, node in
            sumBytes(total, node.sizeBytes)
        }

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                if showStorageTreemap {
                    storageTreemapNavigationBar(
                        breadcrumbs: breadcrumbs,
                        blockCount: nodes.count,
                        totalBytes: totalBytes,
                        totalLabel: selectedNode?.label ?? "All roots"
                    )
                } else {
                    Label("Treemap is idle until requested.", systemImage: "pause.circle")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button {
                        showStorageTreemap = true
                    } label: {
                        Label("Load visual map", systemImage: "square.grid.3x3")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(report.treemapRoots.isEmpty)
                }
            }

            if showStorageTreemap {
                if nodes.isEmpty {
                    ContentUnavailableView(
                        "No visual map yet",
                        systemImage: "square.grid.3x3",
                        description: Text("Run a scan with storage candidates, then load the map again.")
                    )
                    .frame(maxWidth: .infinity)
                } else {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.sm
                    ) {
                        ForEach(nodes) { node in
                            storageTreemapBlock(node, totalBytes: totalBytes)
                        }
                    }
                }
            }
        }
    }

    private func storageTreemapNavigationBar(
        breadcrumbs: [StorageTreemapNodeModel],
        blockCount: Int,
        totalBytes: UInt64,
        totalLabel: String
    ) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Button("All roots") {
                selectedTreemapNodeID = nil
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            ForEach(breadcrumbs) { node in
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                Button(node.label) {
                    selectedTreemapNodeID = node.id
                }
                .buttonStyle(.borderless)
                .controlSize(.small)
            }

            Spacer()
            AetowerBadge(totalLabel, tone: AetowerDesign.Tone.disk)
            AetowerBadge("\(blockCount) block\(blockCount == 1 ? "" : "s")", tone: AetowerDesign.Tone.disk)
            AetowerBadge(formatBytes(totalBytes), tone: AetowerDesign.Tone.memory)
        }
    }

    private func storageFullDiskBlock(
        _ layout: StorageTreemapLayout,
        displayedTotalBytes: UInt64,
        cubeBin: StorageCubeNodeBin?
    ) -> some View {
        let node = layout.node
        let color = storageTreemapColor(node.colorKey)
        let rect = layout.rect
        let share = displayedTotalBytes == 0 ? 0 : Double(node.sizeBytes) / Double(displayedTotalBytes)
        let isTiny = rect.width < 72 || rect.height < 58
        let isCompact = rect.width < 150 || rect.height < 104
        let insetRect = rect.insetBy(dx: 2, dy: 2)
        let usableRect = insetRect.width > 0 && insetRect.height > 0 ? insetRect : rect

        return Button {
            if !node.children.isEmpty {
                selectedTreemapNodeID = node.id
            } else {
                reveal(path: node.path)
            }
        } label: {
            ZStack(alignment: .topLeading) {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(
                        LinearGradient(
                            colors: [color.opacity(0.22), color.opacity(0.06)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )
                storageFullDiskCubeCanvas(
                    cubeCount: cubeBin?.cubeCount ?? 0,
                    color: color,
                    isTiny: isTiny
                )
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .stroke(color.opacity(0.28), lineWidth: AetowerDesign.Stroke.hairline)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(alignment: .top, spacing: AetowerDesign.Spacing.xs) {
                        Image(systemName: storageTreemapIcon(node))
                            .foregroundStyle(color)
                        if !isTiny {
                            Text(node.label)
                                .font(AetowerDesign.Typography.caption.weight(.semibold))
                                .lineLimit(isCompact ? 1 : 2)
                        }
                    }

                    if !isTiny {
                        Spacer(minLength: AetowerDesign.Spacing.xs)
                        Text(formatBytes(node.sizeBytes))
                            .font(.system(size: isCompact ? 14 : 18, weight: .semibold, design: .rounded))
                            .lineLimit(1)
                        Text("\(Int((share * 100).rounded()))% of displayed total")
                            .font(AetowerDesign.Typography.caption)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                            .lineLimit(1)
                        if let cubeBin {
                            Text("\(cubeBin.cubeCount) cube\(cubeBin.cubeCount == 1 ? "" : "s")")
                                .font(AetowerDesign.Typography.caption)
                                .foregroundStyle(AetowerDesign.Ink.tertiary)
                                .lineLimit(1)
                        }
                    }

                    if !isCompact {
                        Text("\(node.itemCount) item\(node.itemCount == 1 ? "" : "s") · \(node.fileType)")
                            .font(AetowerDesign.Typography.caption)
                            .foregroundStyle(AetowerDesign.Ink.tertiary)
                            .lineLimit(1)
                        if node.hasMore {
                            Text("Grouped overflow")
                                .font(AetowerDesign.Typography.caption.weight(.medium))
                                .foregroundStyle(AetowerDesign.Ink.secondary)
                        }
                    }
                }
                .padding(isTiny ? AetowerDesign.Spacing.xs : AetowerDesign.Spacing.sm)
            }
            .frame(width: max(0, usableRect.width), height: max(0, usableRect.height))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .position(x: usableRect.midX, y: usableRect.midY)
        .help("\(node.path) · \(formatBytes(node.sizeBytes)) · \(Int((share * 100).rounded()))% of displayed total")
    }

    private func storageFullDiskCubeCanvas(
        cubeCount: Int,
        color: Color,
        isTiny: Bool
    ) -> some View {
        Canvas(opaque: false, colorMode: .nonLinear, rendersAsynchronously: true) { context, size in
            guard cubeCount > 0, size.width > 0, size.height > 0 else { return }
            let layout = storageCubeGridLayout(cubeCount: cubeCount, in: size)
            guard layout.cubeSize > 0 else { return }
            for index in 0..<cubeCount {
                var rect = storageCubeRect(index: index, layout: layout)
                rect = rect.insetBy(dx: max(0, min(0.35, rect.width * 0.08)), dy: max(0, min(0.35, rect.height * 0.08)))
                guard rect.width > 0, rect.height > 0 else { continue }
                context.fill(
                    Path(roundedRect: rect, cornerRadius: min(3, rect.width / 4)),
                    with: .color(color.opacity(isTiny ? 0.34 : 0.50))
                )
            }
        }
        .allowsHitTesting(false)
    }

    private func storageTreemapBlock(_ node: StorageTreemapNodeModel, totalBytes: UInt64) -> some View {
        let ratio = totalBytes == 0 ? 0 : Double(node.sizeBytes) / Double(totalBytes)
        let height = min(210, max(86, 86 + ratio * 320))
        let color = storageTreemapColor(node.colorKey)
        return Button {
            if !node.children.isEmpty {
                selectedTreemapNodeID = node.id
            } else {
                reveal(path: node.path)
            }
        } label: {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: storageTreemapIcon(node))
                        .foregroundStyle(color)
                    Text(node.label)
                        .font(.caption.weight(.semibold))
                        .lineLimit(2)
                    Spacer(minLength: AetowerDesign.Spacing.xs)
                }
                Spacer(minLength: AetowerDesign.Spacing.sm)
                Text(formatBytes(node.sizeBytes))
                    .font(.system(size: 18, weight: .semibold, design: .rounded))
                Text("\(node.itemCount) item\(node.itemCount == 1 ? "" : "s") · \(node.fileType)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if node.hasMore {
                    Text("Grouped overflow")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.secondary)
                }
            }
            .padding(AetowerDesign.Spacing.md)
            .frame(maxWidth: .infinity, minHeight: height, alignment: .topLeading)
            .background(
                LinearGradient(
                    colors: [color.opacity(0.24), color.opacity(0.08)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                ),
                in: RoundedRectangle(cornerRadius: 16, style: .continuous)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .stroke(color.opacity(0.22), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
        .help(node.path)
    }

    private func storageCubeProjection(
        for nodes: [StorageTreemapNodeModel],
        preferredUnitBytes: UInt64
    ) -> StorageCubeProjection {
        StorageCubeProjectionBuilder.build(
            inputs: nodes
                .filter { $0.sizeBytes > 0 }
                .sorted { left, right in
                    left.sizeBytes == right.sizeBytes ? left.label < right.label : left.sizeBytes > right.sizeBytes
                }
                .map { StorageCubeProjectionInput(id: $0.id, sizeBytes: $0.sizeBytes) },
            maxCubes: 2_400,
            preferredUnitBytes: preferredUnitBytes
        )
    }

    private func storageCubePreferredUnitBytes(for selectedNode: StorageTreemapNodeModel?) -> UInt64 {
        let mib = UInt64(1_024 * 1_024)
        guard let selectedNode else { return 50 * mib }
        switch selectedNode.depth {
        case 0: return 10 * mib
        case 1: return 5 * mib
        case 2: return 1 * mib
        default: return 256 * 1_024
        }
    }

    private func storageCubeNodeBins(
        nodes: [StorageTreemapNodeModel],
        projection: StorageCubeProjection
    ) -> [StorageCubeNodeBin] {
        let nodesByID = Dictionary(uniqueKeysWithValues: nodes.map { ($0.id, $0) })
        return projection.bins.compactMap { bin in
            guard let node = nodesByID[bin.id] else { return nil }
            return StorageCubeNodeBin(node: node, bin: bin)
        }
    }

    private func storageCubeGridLayout(
        cubeCount: Int,
        in size: CGSize
    ) -> StorageCubeGridLayout {
        guard cubeCount > 0, size.width > 0, size.height > 0 else {
            return StorageCubeGridLayout(columns: 1, rows: 0, cubeSize: 0, gap: 0, origin: .zero, cubeCount: 0)
        }

        let aspect = max(0.25, min(4.0, Double(size.width / max(1, size.height))))
        let columns = max(1, Int(ceil(sqrt(Double(cubeCount) * aspect))))
        let rows = max(1, Int(ceil(Double(cubeCount) / Double(columns))))
        let coarseGap = CGFloat(2)
        let coarseCubeSize = min(
            (size.width - coarseGap * CGFloat(max(0, columns - 1))) / CGFloat(columns),
            (size.height - coarseGap * CGFloat(max(0, rows - 1))) / CGFloat(rows)
        )
        let gap = coarseCubeSize < 6 ? CGFloat(1) : coarseGap
        let cubeSize = max(
            1,
            min(
                (size.width - gap * CGFloat(max(0, columns - 1))) / CGFloat(columns),
                (size.height - gap * CGFloat(max(0, rows - 1))) / CGFloat(rows)
            )
        )
        let gridWidth = cubeSize * CGFloat(columns) + gap * CGFloat(max(0, columns - 1))
        let gridHeight = cubeSize * CGFloat(rows) + gap * CGFloat(max(0, rows - 1))
        return StorageCubeGridLayout(
            columns: columns,
            rows: rows,
            cubeSize: cubeSize,
            gap: gap,
            origin: CGPoint(
                x: max(0, (size.width - gridWidth) / 2),
                y: max(0, (size.height - gridHeight) / 2)
            ),
            cubeCount: cubeCount
        )
    }

    private func storageCubeRect(index: Int, layout: StorageCubeGridLayout) -> CGRect {
        guard layout.columns > 0, index >= 0, index < layout.cubeCount else { return .zero }
        let row = index / layout.columns
        let column = index % layout.columns
        return CGRect(
            x: layout.origin.x + CGFloat(column) * (layout.cubeSize + layout.gap),
            y: layout.origin.y + CGFloat(row) * (layout.cubeSize + layout.gap),
            width: layout.cubeSize,
            height: layout.cubeSize
        )
    }

    private func storageCubeIndex(
        at location: CGPoint,
        layout: StorageCubeGridLayout
    ) -> Int? {
        guard layout.columns > 0, layout.cubeSize > 0, layout.cubeCount > 0 else { return nil }
        let stride = layout.cubeSize + layout.gap
        guard stride > 0 else { return nil }
        let x = location.x - layout.origin.x
        let y = location.y - layout.origin.y
        guard x >= 0, y >= 0 else { return nil }
        let column = Int(floor(x / stride))
        let row = Int(floor(y / stride))
        guard column >= 0, column < layout.columns, row >= 0, row < layout.rows else { return nil }
        let localX = x - CGFloat(column) * stride
        let localY = y - CGFloat(row) * stride
        guard localX <= layout.cubeSize, localY <= layout.cubeSize else { return nil }
        let index = row * layout.columns + column
        return index < layout.cubeCount ? index : nil
    }

    private func storageProportionalTreemapLayouts(
        for nodes: [StorageTreemapNodeModel],
        in bounds: CGRect
    ) -> [StorageTreemapLayout] {
        let visibleNodes = nodes
            .filter { $0.sizeBytes > 0 }
            .sorted { left, right in
                if left.sizeBytes == right.sizeBytes {
                    return left.label < right.label
                }
                return left.sizeBytes > right.sizeBytes
            }
        let totalBytes = visibleNodes.reduce(UInt64(0)) { total, node in
            sumBytes(total, node.sizeBytes)
        }
        guard totalBytes > 0, bounds.width > 0, bounds.height > 0 else {
            return []
        }
        return storageSliceTreemapLayouts(
            for: visibleNodes,
            totalBytes: totalBytes,
            in: bounds
        )
    }

    private func storageSliceTreemapLayouts(
        for nodes: [StorageTreemapNodeModel],
        totalBytes: UInt64,
        in bounds: CGRect
    ) -> [StorageTreemapLayout] {
        guard !nodes.isEmpty, totalBytes > 0 else { return [] }
        if nodes.count == 1 {
            return [StorageTreemapLayout(node: nodes[0], rect: bounds)]
        }

        let splitIndex = storageTreemapSplitIndex(for: nodes, targetBytes: totalBytes / 2)
        let firstNodes = Array(nodes.prefix(splitIndex))
        let secondNodes = Array(nodes.dropFirst(splitIndex))
        let firstBytes = firstNodes.reduce(UInt64(0)) { total, node in
            sumBytes(total, node.sizeBytes)
        }
        let firstRatio = CGFloat(Double(firstBytes) / Double(totalBytes))

        let firstRect: CGRect
        let secondRect: CGRect
        if bounds.width >= bounds.height {
            let firstWidth = bounds.width * firstRatio
            firstRect = CGRect(x: bounds.minX, y: bounds.minY, width: firstWidth, height: bounds.height)
            secondRect = CGRect(
                x: bounds.minX + firstWidth,
                y: bounds.minY,
                width: max(0, bounds.width - firstWidth),
                height: bounds.height
            )
        } else {
            let firstHeight = bounds.height * firstRatio
            firstRect = CGRect(x: bounds.minX, y: bounds.minY, width: bounds.width, height: firstHeight)
            secondRect = CGRect(
                x: bounds.minX,
                y: bounds.minY + firstHeight,
                width: bounds.width,
                height: max(0, bounds.height - firstHeight)
            )
        }

        return storageSliceTreemapLayouts(for: firstNodes, totalBytes: firstBytes, in: firstRect)
            + storageSliceTreemapLayouts(
                for: secondNodes,
                totalBytes: totalBytes >= firstBytes ? totalBytes - firstBytes : 0,
                in: secondRect
            )
    }

    private func storageTreemapSplitIndex(
        for nodes: [StorageTreemapNodeModel],
        targetBytes: UInt64
    ) -> Int {
        var accumulated: UInt64 = 0
        for index in nodes.indices.dropLast() {
            let next = sumBytes(accumulated, nodes[index].sizeBytes)
            if next >= targetBytes {
                return max(1, index + 1)
            }
            accumulated = next
        }
        return max(1, nodes.count / 2)
    }

    private func storageExplorerTable(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            if let error = state.storageItemsPageError {
                storageItemsPageErrorBanner(error)
            }
            if let page = state.storageItemsPage {
                storageServerExplorerTable(page)
            } else if state.storageItemsPageIsLoading || state.storageItemsPageError == nil {
                ContentUnavailableView(
                    "Loading indexed storage page",
                    systemImage: "list.bullet.rectangle",
                    description: Text("Aetower is fetching a small server-side page instead of sorting the full scan on the UI thread.")
                )
            } else {
                // Fallback for the first paint and for older engines that do
                // not serve the items-page endpoint: the in-memory top-K
                // report slice with client-side sort/filter/paging.
                storageLocalExplorerTable(filteredItems(from: report))
            }
        }
        .overlay(alignment: .center) {
            if state.storageItemsPageIsLoading {
                ProgressView("Loading page…")
                    .controlSize(.small)
                    .padding(AetowerDesign.Spacing.md)
                    .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            }
        }
        .task(id: state.storageHygieneReport?.capturedAtMillis) {
            guard state.storageHygieneReport != nil else { return }
            state.loadStorageItemsPage(
                offset: state.storageItemsPage?.offset ?? 0,
                sortKey: state.storageItemsPageSortKey,
                sortDescending: state.storageItemsPageSortDescending
            )
        }
    }

    private func storageItemsPageErrorBanner(_ message: String) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(AetowerDesign.Status.warning)
            Text(message)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Spacer()
            Button("Retry") {
                state.loadStorageItemsPage(
                    offset: state.storageItemsPage?.offset ?? 0,
                    sortKey: state.storageItemsPageSortKey,
                    sortDescending: state.storageItemsPageSortDescending,
                    force: true
                )
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .disabled(state.storageItemsPageIsLoading)
        }
        .padding(AetowerDesign.Spacing.sm)
        .background(AetowerDesign.Status.warning.opacity(0.1), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func storageServerExplorerTable(_ page: StorageHygieneItemsPageModel) -> some View {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let visibleItems = query.isEmpty
            ? page.items
            : page.items.filter { storageItemMatchesSearch($0, query: query) }
        let pageLimit = max(1, page.limit)
        let firstRow = page.items.isEmpty ? page.offset : page.offset + 1
        let lastRow = page.offset + page.items.count

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                if let total = page.totalAvailable {
                    AetowerBadge("rows \(firstRow)-\(lastRow) of \(total)", tone: AetowerDesign.Tone.disk)
                } else {
                    AetowerBadge("rows \(firstRow)-\(lastRow)", tone: AetowerDesign.Tone.disk)
                }
                if !query.isEmpty {
                    AetowerBadge("\(visibleItems.count) match on this page", tone: AetowerDesign.Tone.memory)
                }
                Text("Sorted server-side from the storage index; click a column to re-sort. Search filters current page.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Previous") {
                    state.loadStorageItemsPage(
                        offset: max(0, page.offset - pageLimit),
                        limit: pageLimit,
                        sortKey: state.storageItemsPageSortKey,
                        sortDescending: state.storageItemsPageSortDescending
                    )
                }
                .disabled(page.offset == 0 || state.storageItemsPageIsLoading)
                Button("Next") {
                    state.loadStorageItemsPage(
                        offset: page.offset + pageLimit,
                        limit: pageLimit,
                        sortKey: state.storageItemsPageSortKey,
                        sortDescending: state.storageItemsPageSortDescending
                    )
                }
                .disabled(!page.hasMore || state.storageItemsPageIsLoading)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            if visibleItems.isEmpty {
                ContentUnavailableView(
                    "No matching items",
                    systemImage: "list.bullet.rectangle",
                    description: Text(
                        query.isEmpty
                            ? "This page is empty. Move to another page or run a scan."
                            : "No rows on this page match the search; it filters the current page only."
                    )
                )
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    storageServerExplorerTableHeader(pageLimit: pageLimit)
                    ForEach(visibleItems) { item in
                        storageExplorerTableRow(item, showScore: true)
                    }
                }
            }
        }
    }

    private func storageServerExplorerTableHeader(pageLimit: Int) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            storageExplorerSortButton("Item", key: "path", pageLimit: pageLimit)
                .frame(maxWidth: .infinity, alignment: .leading)
            storageExplorerSortButton("Tier", key: "tier", pageLimit: pageLimit)
                .frame(width: 88, alignment: .center)
            storageExplorerSortButton("Kind", key: "kind", pageLimit: pageLimit)
                .frame(width: 92, alignment: .center)
            storageExplorerSortButton("Score", key: "score", pageLimit: pageLimit)
                .frame(width: 56, alignment: .trailing)
            storageExplorerSortButton("Size", key: "size", pageLimit: pageLimit)
                .frame(width: 80, alignment: .trailing)
            Text("Actions")
                .frame(width: 52, alignment: .trailing)
        }
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.secondary)
        .padding(.horizontal, AetowerDesign.Spacing.sm)
    }

    private func storageExplorerSortButton(_ label: String, key: String, pageLimit: Int) -> some View {
        let isActive = state.storageItemsPageSortKey == key
        return Button {
            let descending = isActive
                ? !state.storageItemsPageSortDescending
                : key == "size" || key == "score"
            state.loadStorageItemsPage(
                offset: 0,
                limit: pageLimit,
                sortKey: key,
                sortDescending: descending
            )
        } label: {
            HStack(spacing: 2) {
                Text(label)
                if isActive {
                    Image(systemName: state.storageItemsPageSortDescending ? "arrow.down" : "arrow.up")
                        .font(.system(size: 8, weight: .bold))
                }
            }
        }
        .buttonStyle(.plain)
        .disabled(state.storageItemsPageIsLoading)
        .help("Sort by \(label.lowercased()) (server-side)")
    }

    private func storageLocalExplorerTable(_ visibleItems: [StorageHygieneItemModel]) -> some View {
        let pageSize = 25
        let pageCount = max(1, Int(ceil(Double(visibleItems.count) / Double(pageSize))))
        let page = min(storageExplorerPage, pageCount - 1)
        let pageItems = Array(visibleItems.dropFirst(page * pageSize).prefix(pageSize))

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerBadge("\(visibleItems.count) visible", tone: AetowerDesign.Tone.disk)
                AetowerBadge("Page \(page + 1)/\(pageCount)", tone: AetowerDesign.Tone.memory)
                Text("Sort/filter uses the same controls as Raw artifacts; MCP item pages now expose sort + offset for agent clients.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Previous") {
                    storageExplorerPage = max(0, page - 1)
                }
                .disabled(page == 0)
                Button("Next") {
                    storageExplorerPage = min(pageCount - 1, page + 1)
                }
                .disabled(page >= pageCount - 1)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            if pageItems.isEmpty {
                ContentUnavailableView(
                    "No matching items",
                    systemImage: "list.bullet.rectangle",
                    description: Text("Change the filter, search text, or scan root.")
                )
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    storageExplorerTableHeader
                    ForEach(pageItems) { item in
                        storageExplorerTableRow(item)
                    }
                }
            }
        }
    }

    private var storageExplorerTableHeader: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Text("Item")
                .frame(maxWidth: .infinity, alignment: .leading)
            Text("Tier")
                .frame(width: 88, alignment: .center)
            Text("Kind")
                .frame(width: 92, alignment: .center)
            Text("Size")
                .frame(width: 80, alignment: .trailing)
            Text("Actions")
                .frame(width: 52, alignment: .trailing)
        }
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.secondary)
        .padding(.horizontal, AetowerDesign.Spacing.sm)
    }

    private func storageExplorerTableRow(
        _ item: StorageHygieneItemModel,
        showScore: Bool = false
    ) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(item.displayName)
                    .font(.caption.weight(.semibold))
                    .lineLimit(1)
                Text(item.path)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let accounting = storageByteAccountingSummary(item) {
                    Text(accounting)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            cleanupTierBadge(item)
                .frame(width: 88)
            Text(item.kind)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 92)
            if showScore {
                Text(storageRecommendationScoreLabel(item.recommendationScore))
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 56, alignment: .trailing)
                    .help("Composite reclaim-recommendation score (size x tier x staleness)")
            }
            Text(formatBytes(item.sizeBytes))
                .font(.caption.weight(.semibold))
                .frame(width: 80, alignment: .trailing)
            storageItemPrimaryAction(item)
                .buttonStyle(.bordered)
                .controlSize(.mini)
            storageItemActionMenu(item)
                .frame(width: 32, alignment: .trailing)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func storageRecommendationScoreLabel(_ score: Double) -> String {
        score <= 0 ? "–" : String(format: "%.1f", score)
    }

    private func storageByteAccountingSummary(_ item: StorageHygieneItemModel) -> String? {
        var cues: [String] = []
        if item.logicalBytes != item.physicalBytes {
            cues.append("logical \(formatBytes(item.logicalBytes))")
            cues.append("physical \(formatBytes(item.physicalBytes))")
        }
        if item.sparseOrShared {
            cues.append("APFS/shared")
        }
        if item.hasHardlinks {
            cues.append("\(item.hardlinkCount)x hardlink")
        }
        if item.cloudPlaceholder {
            cues.append("cloud-only/local sparse")
        }
        if item.protectedPath {
            cues.append("protected")
        }
        return cues.isEmpty ? nil : cues.joined(separator: " · ")
    }

    private func storageTreemapSelectedNode(
        in roots: [StorageTreemapNodeModel]
    ) -> StorageTreemapNodeModel? {
        guard let selectedTreemapNodeID else { return nil }
        return storageTreemapNode(id: selectedTreemapNodeID, in: roots)
    }

    private func storageTreemapNode(
        id: String,
        in nodes: [StorageTreemapNodeModel]
    ) -> StorageTreemapNodeModel? {
        for node in nodes {
            if node.id == id {
                return node
            }
            if let match = storageTreemapNode(id: id, in: node.children) {
                return match
            }
        }
        return nil
    }

    private func storageTreemapBreadcrumbs(
        in roots: [StorageTreemapNodeModel]
    ) -> [StorageTreemapNodeModel] {
        guard let selectedTreemapNodeID else { return [] }
        return storageTreemapPath(to: selectedTreemapNodeID, in: roots) ?? []
    }

    private func storageTreemapPath(
        to id: String,
        in nodes: [StorageTreemapNodeModel]
    ) -> [StorageTreemapNodeModel]? {
        for node in nodes {
            if node.id == id {
                return [node]
            }
            if let childPath = storageTreemapPath(to: id, in: node.children) {
                return [node] + childPath
            }
        }
        return nil
    }

    private func storageTreemapIcon(_ node: StorageTreemapNodeModel) -> String {
        if node.nodeType == "root" { return "externaldrive" }
        if !node.children.isEmpty { return "folder" }
        switch node.colorKey {
        case "xcode": return "hammer"
        case "rust": return "gearshape.2"
        case "node": return "network"
        case "docker": return "shippingbox"
        case "app": return "app.dashed"
        case "system": return "internaldrive"
        case "log": return "doc.text"
        case "risky": return "exclamationmark.triangle"
        default: return "doc"
        }
    }

    private func storageTreemapColor(_ key: String) -> Color {
        switch key {
        case "xcode": return .blue
        case "rust": return .orange
        case "node": return .green
        case "docker": return .cyan
        case "app": return .pink
        case "system": return .indigo
        case "log": return .gray
        case "file": return AetowerDesign.Tone.disk
        case "expensive": return AetowerDesign.Status.warning
        case "risky": return AetowerDesign.Status.error
        default: return AetowerDesign.Tone.memory
        }
    }

    private func itemSection(_ report: StorageHygieneReportModel) -> some View {
        DisclosureGroup(isExpanded: $showRawArtifacts) {
            let visibleItems = filteredItems(from: report)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Picker("Artifact scope", selection: $artifactScope) {
                        ForEach(StorageArtifactScope.allCases) { scope in
                            Text(scope.label).tag(scope)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(maxWidth: 180)

                    Picker("Sort artifacts", selection: $artifactSort) {
                        ForEach(StorageArtifactSort.allCases) { sort in
                            Text(sort.label).tag(sort)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(maxWidth: 180)

                    Spacer()

                    Text("\(visibleItems.count) visible of \(visibleStorageItems(from: report).count)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if visibleItems.isEmpty {
                    ContentUnavailableView(
                        "No matching artifacts",
                        systemImage: "line.3.horizontal.decrease.circle",
                        description: Text("Change the filter, search text, root, or depth and scan again.")
                    )
                } else {
                    LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        ForEach(visibleItems.prefix(120)) { item in
                            artifactRow(item)
                        }
                    }
                    if visibleItems.count > 120 {
                        AetowerInfoBanner(
                            "Showing the first 120 matching artifacts. Use the Explore table for paged server-side browsing.",
                            systemImage: "line.3.horizontal.decrease.circle",
                            tone: AetowerDesign.Status.neutral,
                            level: .card
                        )
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        } label: {
            let visibleCandidateCount = visibleStorageItems(from: report).count
            advancedSectionLabel(
                title: "Raw artifacts",
                detail: "\(visibleCandidateCount) candidate\(visibleCandidateCount == 1 ? "" : "s")",
                systemImage: "list.bullet.rectangle"
            )
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
                        cleanupTierBadge(item)
                        safetyBadge(item)
                        AetowerBadge(storageRoleLabel(item.storageRole), tone: tone(forCleanupTier: item.cleanupTier))
                        AetowerBadge(gitStatusLabel(item.gitStatus), tone: gitStatusTone(item.gitStatus))
                        if item.stale {
                            Text("Stale")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Status.warning)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Status.warning.opacity(0.12), in: Capsule())
                        }
                        if item.cold {
                            Text("Cold")
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
                        if !storageItemIsTrashActionable(item) {
                            Text("Blocked")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Status.error)
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .background(AetowerDesign.Status.error.opacity(0.12), in: Capsule())
                        }
                    }

                    Text(item.path)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Text(accessSummary(for: item))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)

                    if !item.cleanupBlockers.isEmpty {
                        Text("Cleanup blocked: \(item.cleanupBlockers.joined(separator: "; "))")
                            .font(.caption2)
                            .foregroundStyle(AetowerDesign.Status.error)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    Text(item.reason)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(item.recommendation)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                    Text(item.nextStep)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    Label(attributionSummary(for: item), systemImage: "point.3.connected.trianglepath.dotted")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(2)
                    if !item.evidence.isEmpty {
                        VStack(alignment: .leading, spacing: 3) {
                            ForEach(item.evidence.prefix(4), id: \.self) { evidence in
                                Label(evidence, systemImage: "smallcircle.filled.circle")
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                                    .lineLimit(2)
                            }
                        }
                    }
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
                storageItemPrimaryAction(item)
                Button("Reveal") { reveal(path: item.path) }
                Button("Explain") { classificationExplanation = explanation(for: item) }
                storageItemActionMenu(item)
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
        DisclosureGroup(isExpanded: $showScannedRoots) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                if report.sourceCoverage.isEmpty {
                    ForEach(report.roots, id: \.self) { root in
                        rootLine(root, detail: "scanned", systemImage: "checkmark.circle")
                    }
                    ForEach(report.skippedRoots) { root in
                        rootLine(root.path, detail: root.reason, systemImage: "exclamationmark.triangle")
                    }
                } else {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 260), spacing: AetowerDesign.Spacing.sm)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.sm
                    ) {
                        ForEach(report.sourceCoverage) { source in
                            storageSourceCard(source)
                        }
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        } label: {
            advancedSectionLabel(
                title: "Source coverage",
                detail: "\(report.roots.count) scanned · \(report.skippedRoots.count) skipped · \(report.sourceCoverage.count) considered",
                systemImage: "folder.badge.questionmark"
            )
        }
        .font(.caption)
    }

    private func caveatsSection(_ report: StorageHygieneReportModel) -> some View {
        DisclosureGroup(isExpanded: $showCaveats) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(report.caveats, id: \.self) { caveat in
                    Label(caveat, systemImage: "info.circle")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        } label: {
            advancedSectionLabel(
                title: "Caveats",
                detail: "\(report.caveats.count) scan note\(report.caveats.count == 1 ? "" : "s")",
                systemImage: "info.circle"
            )
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private var loadingSection: some View {
        VStack(spacing: AetowerDesign.Spacing.md) {
            ProgressView()
            Text(storageScanLoadingTitle)
                .font(AetowerDesign.Typography.body)
            Text(storageScanLoadingDetail)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
                .truncationMode(.middle)
            if let reason = state.storageScanJob?.progress.throttleReason, !reason.isEmpty {
                Text("Throttled: \(reason)")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(.secondary)
            }
            if let recovery = storageScanRecoveryDetail {
                Text(recovery)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
            }
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button {
                    if state.storageScanJob?.isPaused == true {
                        state.resumeStorageHygieneScan()
                    } else {
                        state.pauseStorageHygieneScan()
                    }
                } label: {
                    Label(
                        state.storageScanJob?.isPaused == true ? "Resume" : "Pause",
                        systemImage: state.storageScanJob?.isPaused == true ? "play.fill" : "pause.fill"
                    )
                }
                .buttonStyle(.bordered)
                Button(role: .cancel) {
                    state.cancelStorageHygieneScan()
                } label: {
                    Label("Cancel", systemImage: "xmark.circle")
                }
                .buttonStyle(.bordered)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 220)
    }

    private var storageScanLoadingTitle: String {
        guard let job = state.storageScanJob else {
            return "Starting storage scan..."
        }
        return "\(job.status.capitalized) \(job.progress.phase)"
    }

    private var storageScanLoadingDetail: String {
        guard let job = state.storageScanJob else {
            return "Preparing scan job."
        }
        let progress = job.progress
        let path = progress.currentPathHint ?? "discovering roots"
        return "\(formatBytes(progress.scannedBytes)) scanned · \(progress.scannedDirectories) dirs · \(progress.scannedFiles) files · \(path)"
    }

    private var storageScanRecoveryDetail: String? {
        guard let job = state.storageScanJob else { return nil }
        if job.resumedFromPartial == true {
            let bytes = formatBytes(job.recoveredBytes ?? 0)
            let files = job.recoveredFiles ?? 0
            let directories = job.recoveredDirectories ?? 0
            return "Recovered partial state: \(bytes), \(directories) dirs, \(files) files. Indexed rows are reused where still valid."
        }
        guard job.partialStateAvailable == true, let persistedAt = job.persistedAtMillis else { return nil }
        let date = Date(timeIntervalSince1970: Double(persistedAt) / 1000.0)
        return "Progress persisted \(date.formatted(date: .omitted, time: .shortened)); a matching relaunch can resume from the partial index."
    }

    private var emptySection: some View {
        ContentUnavailableView(
            "No storage report yet",
            systemImage: "externaldrive",
            description: Text("Run a scan to find storage pressure and prepare cleanup actions.")
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
        AetowerMetricTile(
            title,
            value: value,
            detail: detail,
            systemImage: systemImage,
            tone: tone,
            minHeight: 112,
            valueSize: 22
        )
    }

    private func cleanupTierCard(_ tier: StorageCleanupTierModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: cleanupTierIcon(tier.tier))
                    .foregroundStyle(tone(forCleanupTier: tier.tier))
                Text(tier.label.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            HStack(alignment: .firstTextBaseline) {
                Text(formatBytes(tier.bytes))
                    .font(.system(size: 21, weight: .semibold, design: .rounded))
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                Spacer()
                Text("\(tier.itemCount)")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(tone(forCleanupTier: tier.tier))
            }
            Text(tier.description)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(3)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 126, alignment: .topLeading)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func footprintMetric(
        _ title: String,
        value: String,
        detail: String
    ) -> some View {
        AetowerMetricTile(
            title,
            value: value,
            detail: detail,
            tone: AetowerDesign.Ink.primary,
            minHeight: 72,
            valueSize: 13
        )
    }

    private func storageBudgetTone(_ status: String?) -> Color {
        switch status?.lowercased() {
        case "critical":
            return AetowerDesign.Status.error
        case "warn", "warning":
            return AetowerDesign.Status.warning
        case "ok":
            return AetowerDesign.Status.ready
        default:
            return AetowerDesign.Status.neutral
        }
    }

    private func recommendationDecisionGrid(_ decisions: [StorageRecommendationDecision]) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 122), spacing: AetowerDesign.Spacing.xs)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.xs
        ) {
            ForEach(decisions) { decision in
                recommendationDecisionTile(decision)
            }
        }
    }

    private func recommendationDecisionTile(_ decision: StorageRecommendationDecision) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.xs) {
            Image(systemName: decision.systemImage)
                .font(.caption.weight(.semibold))
                .foregroundStyle(decision.tone)
                .frame(width: 14)
            VStack(alignment: .leading, spacing: 2) {
                Text(decision.title)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                Text(decision.value)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(AetowerDesign.Ink.primary)
                    .lineLimit(1)
                Text(decision.detail)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .lineLimit(2)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .frame(maxWidth: .infinity, minHeight: 68, alignment: .topLeading)
        .background(AetowerDesign.Surface.badge, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func actionRecommendationDecisions(_ action: StorageHomeAction) -> [StorageRecommendationDecision] {
        let relevantItems = action.stageItems.isEmpty ? action.sampleItems : action.stageItems
        return [
            StorageRecommendationDecision(
                id: "what",
                title: "What",
                value: action.title,
                detail: action.detail,
                systemImage: action.systemImage,
                tone: action.tone
            ),
            StorageRecommendationDecision(
                id: "why",
                title: "Why",
                value: action.itemCount == 0 ? "No candidates" : "\(action.itemCount) matched",
                detail: action.growthEvents.isEmpty ? firstReason(in: relevantItems) : "Recent growth explains current pressure.",
                systemImage: "questionmark.circle",
                tone: AetowerDesign.Status.neutral
            ),
            StorageRecommendationDecision(
                id: "safe",
                title: "Safe?",
                value: cleanupSafetySummary(for: action.stageItems, fallbackItems: action.sampleItems),
                detail: action.hasStageableItems ? "Eligible items move to Finder Trash." : "No unattended cleanup in this lane.",
                systemImage: action.hasStageableItems ? "checkmark.shield" : "exclamationmark.triangle",
                tone: action.hasStageableItems ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
            ),
            StorageRecommendationDecision(
                id: "reclaim",
                title: "Reclaim",
                value: formatBytes(action.bytes),
                detail: "Current bounded scan estimate.",
                systemImage: "externaldrive.badge.minus",
                tone: AetowerDesign.Tone.disk
            ),
            StorageRecommendationDecision(
                id: "rebuild",
                title: "Rebuild Cost",
                value: rebuildCostSummary(for: relevantItems),
                detail: rebuildCommandSummary(for: relevantItems),
                systemImage: "hammer",
                tone: AetowerDesign.Tone.cpu
            ),
            StorageRecommendationDecision(
                id: "undo",
                title: "Undo Path",
                value: action.hasStageableItems ? "Finder Trash" : "Manual review",
                detail: action.hasStageableItems ? "Restore from Trash or rebuild artifacts." : "Aetower will not auto-stage risky items.",
                systemImage: "arrow.uturn.backward.circle",
                tone: AetowerDesign.Status.neutral
            ),
        ]
    }

    private func bundleRecommendationDecisions(_ bundle: StorageCleanupBundleModel) -> [StorageRecommendationDecision] {
        [
            StorageRecommendationDecision(
                id: "what",
                title: "What",
                value: bundle.title,
                detail: bundle.subtitle,
                systemImage: cleanupBundleIcon(bundle),
                tone: cleanupBundleTone(bundle)
            ),
            StorageRecommendationDecision(
                id: "why",
                title: "Why",
                value: "\(bundle.itemCount) item\(bundle.itemCount == 1 ? "" : "s")",
                detail: bundle.manifest.first?.reason ?? "Bundle built from cleanup classifier output.",
                systemImage: "questionmark.circle",
                tone: AetowerDesign.Status.neutral
            ),
            StorageRecommendationDecision(
                id: "safe",
                title: "Safe?",
                value: cleanupTierLabel(bundle.safety),
                detail: bundle.dryRunOnly ? "Dry-run only. Review before cleanup." : "\(bundle.confidenceScore)% confidence.",
                systemImage: bundle.dryRunOnly ? "eye" : "checkmark.shield",
                tone: cleanupBundleTone(bundle)
            ),
            StorageRecommendationDecision(
                id: "reclaim",
                title: "Reclaim",
                value: formatBytes(bundle.estimatedReclaimableBytes),
                detail: "Bundle manifest estimate.",
                systemImage: "externaldrive.badge.minus",
                tone: AetowerDesign.Tone.disk
            ),
            StorageRecommendationDecision(
                id: "rebuild",
                title: "Rebuild Cost",
                value: bundleRebuildCostSummary(bundle),
                detail: bundleRebuildDetail(bundle),
                systemImage: "hammer",
                tone: AetowerDesign.Tone.cpu
            ),
            StorageRecommendationDecision(
                id: "undo",
                title: "Undo Path",
                value: "Finder Trash",
                detail: bundle.rollbackNotes.first ?? "Trash-first cleanup; restore from Trash if needed.",
                systemImage: "arrow.uturn.backward.circle",
                tone: AetowerDesign.Status.neutral
            ),
        ]
    }

    private func recipeRecommendationDecisions(_ recipe: StorageCleanupRecipeModel) -> [StorageRecommendationDecision] {
        [
            StorageRecommendationDecision(
                id: "what",
                title: "What",
                value: recipe.title,
                detail: recipe.affectedPath,
                systemImage: cleanupRecipeIcon(recipe),
                tone: cleanupRecipeTone(recipe)
            ),
            StorageRecommendationDecision(
                id: "why",
                title: "Why",
                value: recipe.category,
                detail: recipe.reason,
                systemImage: "questionmark.circle",
                tone: AetowerDesign.Status.neutral
            ),
            StorageRecommendationDecision(
                id: "safe",
                title: "Safe?",
                value: recipe.requiresReview ? "Review" : cleanupTierLabel(recipe.safety),
                detail: recipe.destructive ? "Destructive command reference." : "Staged cleanup uses Trash path.",
                systemImage: recipe.requiresReview ? "exclamationmark.triangle" : "checkmark.shield",
                tone: cleanupRecipeTone(recipe)
            ),
            StorageRecommendationDecision(
                id: "reclaim",
                title: "Reclaim",
                value: formatBytes(recipe.estimatedReclaimableBytes),
                detail: "Recipe estimate.",
                systemImage: "externaldrive.badge.minus",
                tone: AetowerDesign.Tone.disk
            ),
            StorageRecommendationDecision(
                id: "rebuild",
                title: "Rebuild Cost",
                value: recipe.requiresReview ? "Unknown" : "Depends",
                detail: recipe.prerequisites.first ?? "Verify the command and owning toolchain first.",
                systemImage: "hammer",
                tone: AetowerDesign.Tone.cpu
            ),
            StorageRecommendationDecision(
                id: "undo",
                title: "Undo Path",
                value: recipe.destructive ? "Manual" : "Finder Trash",
                detail: recipe.destructive ? "Copy plan only; Aetower does not execute shell deletion here." : "Stage path, then move through Finder Trash.",
                systemImage: "arrow.uturn.backward.circle",
                tone: AetowerDesign.Status.neutral
            ),
        ]
    }

    private func itemRecommendationDecisions(_ item: StorageHygieneItemModel) -> [StorageRecommendationDecision] {
        [
            StorageRecommendationDecision(
                id: "what",
                title: "What",
                value: item.displayName,
                detail: storageRoleLabel(item.storageRole),
                systemImage: icon(for: item),
                tone: tone(for: item)
            ),
            StorageRecommendationDecision(
                id: "why",
                title: "Why",
                value: item.kind,
                detail: item.reason,
                systemImage: "questionmark.circle",
                tone: AetowerDesign.Status.neutral
            ),
            StorageRecommendationDecision(
                id: "safe",
                title: "Safe?",
                value: storageItemIsTrashActionable(item) ? cleanupTierLabel(item.cleanupTier) : "Review",
                detail: item.cleanupBlockers.first ?? item.safety,
                systemImage: storageItemIsTrashActionable(item) ? "checkmark.shield" : "exclamationmark.triangle",
                tone: tone(for: item)
            ),
            StorageRecommendationDecision(
                id: "reclaim",
                title: "Reclaim",
                value: formatBytes(item.sizeBytes),
                detail: item.byteAccounting,
                systemImage: "externaldrive.badge.minus",
                tone: AetowerDesign.Tone.disk
            ),
            StorageRecommendationDecision(
                id: "rebuild",
                title: "Rebuild Cost",
                value: item.estimatedRebuildCost,
                detail: item.rebuildCommand ?? rebuildTimeLabel(item.estimatedRebuildSeconds),
                systemImage: "hammer",
                tone: AetowerDesign.Tone.cpu
            ),
            StorageRecommendationDecision(
                id: "undo",
                title: "Undo Path",
                value: item.defaultCleanupAction == "trash" ? "Finder Trash" : "Manual review",
                detail: item.cleanupConsequence,
                systemImage: "arrow.uturn.backward.circle",
                tone: AetowerDesign.Status.neutral
            ),
        ]
    }

    private func firstReason(in items: [StorageHygieneItemModel]) -> String {
        items.first?.reason ?? "No item-level evidence in this lane yet."
    }

    private func cleanupSafetySummary(
        for stageItems: [StorageHygieneItemModel],
        fallbackItems: [StorageHygieneItemModel]
    ) -> String {
        let items = stageItems.isEmpty ? fallbackItems : stageItems
        guard !items.isEmpty else { return "No action" }
        if stageItems.isEmpty { return "Review" }
        if stageItems.allSatisfy({ $0.cleanupTier == "safe" || $0.cleanupTier == "rebuildable" }) {
            return "Trash-ready"
        }
        return "Mixed"
    }

    private func rebuildCostSummary(for items: [StorageHygieneItemModel]) -> String {
        guard !items.isEmpty else { return "n/a" }
        let totalSeconds = items.compactMap(\.estimatedRebuildSeconds).reduce(UInt64(0), +)
        if totalSeconds > 0 {
            return rebuildTimeLabel(totalSeconds)
        }
        if let cost = items.first(where: { !$0.estimatedRebuildCost.isEmpty })?.estimatedRebuildCost {
            return cost
        }
        if items.allSatisfy({ $0.cleanupTier == "safe" }) {
            return "None expected"
        }
        return "Unknown"
    }

    private func rebuildCommandSummary(for items: [StorageHygieneItemModel]) -> String {
        let commands = uniqueStrings(items.compactMap(\.rebuildCommand))
        if commands.isEmpty {
            return "No exact rebuild command known."
        }
        return commands.prefix(2).joined(separator: " · ")
    }

    private func bundleRebuildCostSummary(_ bundle: StorageCleanupBundleModel) -> String {
        if bundle.manifest.contains(where: { $0.cleanupTier == "expensive" }) {
            return "Expensive"
        }
        if bundle.manifest.contains(where: { $0.cleanupTier == "rebuildable" }) {
            return "Rebuildable"
        }
        if bundle.manifest.contains(where: { $0.cleanupTier == "risky" }) {
            return "Manual"
        }
        return "Low"
    }

    private func bundleRebuildDetail(_ bundle: StorageCleanupBundleModel) -> String {
        bundle.manifest.first?.consequence
            ?? bundle.prerequisites.first
            ?? "Review generated artifacts and rebuild cost before committing."
    }

    private func cleanupRecipeBadge(_ recipe: StorageCleanupRecipeModel) -> some View {
        AetowerBadge(cleanupTierLabel(recipe.safety), tone: cleanupRecipeTone(recipe))
    }

    private func cleanupTierBadge(_ item: StorageHygieneItemModel) -> some View {
        AetowerBadge(cleanupTierLabel(item.cleanupTier), tone: tone(forCleanupTier: item.cleanupTier))
    }

    private func safetyBadge(_ item: StorageHygieneItemModel) -> some View {
        AetowerBadge(item.safety == "safe" ? "Expected artifact" : "Review", tone: tone(for: item))
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

    private func storageSourceCard(_ source: StorageSourceCoverageModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: sourceIcon(source))
                    .foregroundStyle(sourceTone(source))
                    .frame(width: 18)
                VStack(alignment: .leading, spacing: 2) {
                    Text(source.label)
                        .font(.caption.weight(.semibold))
                        .lineLimit(1)
                    Text(source.path)
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer()
                AetowerBadge(source.gapKind.replacingOccurrences(of: "-", with: " "), tone: sourceTone(source))
            }
            Text(source.detail)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(2)
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Text("reclaim \(source.reclaimableBytes.map(formatBytes) ?? "none")")
                if let local = source.localBytes, let logical = source.logicalBytes, logical > local {
                    Text("local \(formatBytes(local)) / logical \(formatBytes(logical))")
                }
                if source.protected {
                    Text("protected")
                }
            }
            .font(.caption2)
            .foregroundStyle(.tertiary)
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func volumeStateCard(_ volume: StorageVolumeStateModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack {
                Image(systemName: "internaldrive")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                VStack(alignment: .leading, spacing: 2) {
                    Text(volume.path)
                        .font(.caption.weight(.semibold))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Text(volume.filesystemType)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text(formatBytes(volume.totalBytes))
                    .font(.caption.weight(.semibold))
            }
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 110), spacing: AetowerDesign.Spacing.xs)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.xs
            ) {
                miniCapacityMetric("free now", bytes: volume.freeNowBytes)
                miniCapacityMetric("available", bytes: volume.availableBytes)
                miniCapacityMetric("important", bytes: volume.importantUsageAvailableBytes)
                miniCapacityMetric("opportunistic", bytes: volume.opportunisticUsageAvailableBytes)
            }
            Text(volume.detail)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(2)
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func miniCapacityMetric(_ label: String, bytes: UInt64?) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(label.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.tertiary)
            Text(bytes.map(formatBytes) ?? "n/a")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func sourceIcon(_ source: StorageSourceCoverageModel) -> String {
        if source.permissionState == "needs_full_disk_access" { return "lock.trianglebadge.exclamationmark" }
        if source.protected { return "shield.lefthalf.filled" }
        if source.cloudPlaceholder || source.kind == "cloud" { return "icloud" }
        if source.network { return "externaldrive.connected.to.line.below" }
        if source.kind == "applications" { return "app.dashed" }
        if source.kind == "package-cache" { return "shippingbox" }
        if source.kind == "docker" { return "square.stack.3d.up" }
        return source.scanned ? "checkmark.circle" : "folder.badge.questionmark"
    }

    private func sourceTone(_ source: StorageSourceCoverageModel) -> Color {
        if source.permissionState == "needs_full_disk_access" { return AetowerDesign.Status.warning }
        if source.status == "unavailable" || source.status == "skipped" { return AetowerDesign.Status.error }
        if source.status == "partial" { return AetowerDesign.Status.warning }
        if source.protected || source.gapKind == "protected" { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.ready
    }

    private func advancedSectionLabel(title: String, detail: String, systemImage: String) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: systemImage)
                .foregroundStyle(.secondary)
                .frame(width: 18)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.headline)
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .contentShape(Rectangle())
    }

    private func warningBanner(_ text: String) -> some View {
        AetowerInfoBanner(
            text,
            systemImage: "exclamationmark.triangle",
            tone: AetowerDesign.Status.warning,
            level: .warning
        )
    }

    private func filteredItems(from report: StorageHygieneReportModel) -> [StorageHygieneItemModel] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return visibleStorageItems(from: report).filter { item in
            selectedFilter.matches(item)
                && artifactScope.matches(item)
                && (query.isEmpty || storageItemMatchesSearch(item, query: query))
        }
        .sorted(by: artifactSort.areInIncreasingOrder)
    }

    private func visibleStorageItems(from report: StorageHygieneReportModel) -> [StorageHygieneItemModel] {
        report.items.filter { !state.storagePathWasMovedToTrash($0.path) }
    }

    /// Shared text-search predicate for hygiene items. `query` must already be
    /// trimmed and lowercased.
    private func storageItemMatchesSearch(_ item: StorageHygieneItemModel, query: String) -> Bool {
        item.displayName.lowercased().contains(query)
            || item.path.lowercased().contains(query)
            || item.kind.lowercased().contains(query)
            || item.storageRole.lowercased().contains(query)
            || item.gitStatus.lowercased().contains(query)
            || item.reason.lowercased().contains(query)
            || item.recommendation.lowercased().contains(query)
            || item.nextStep.lowercased().contains(query)
            || item.cleanupTier.lowercased().contains(query)
            || item.evidence.contains(where: { $0.lowercased().contains(query) })
            || (item.attribution.repoName?.lowercased().contains(query) ?? false)
            || (item.attribution.gitBranch?.lowercased().contains(query) ?? false)
            || (item.attribution.command?.lowercased().contains(query) ?? false)
            || (item.attribution.processTree?.lowercased().contains(query) ?? false)
            || (item.attribution.aiAgentSession?.lowercased().contains(query) ?? false)
    }

    private func runScan() {
        let root = customRoot.trimmingCharacters(in: .whitespacesAndNewlines)
        state.runStorageHygieneScan(
            roots: root.isEmpty ? [] : [root],
            maxDepth: UInt32(maxDepth),
            limit: scanMode.resultLimit,
            mode: scanMode.rawValue
        )
    }

    private func runQuickScan() {
        selectedFilter = .attention
        state.runStorageHygieneScan(
            roots: [],
            maxDepth: 5,
            limit: StorageScanModeSelection.fast.resultLimit,
            mode: StorageScanModeSelection.fast.rawValue
        )
    }

    private func attributionSummary(for item: StorageHygieneItemModel) -> String {
        var parts: [String] = []
        if let repoName = item.attribution.repoName {
            parts.append("repo \(repoName)")
        }
        if let branch = item.attribution.gitBranch {
            parts.append("branch \(branch)")
        } else if let head = item.attribution.gitHead {
            parts.append("head \(head)")
        }
        if let command = item.attribution.command {
            parts.append("command \(command)")
        }
        if let session = item.attribution.aiAgentSession {
            parts.append("session \(session)")
        }
        if let writerDisplay = item.attribution.writerDisplay {
            parts.append("written by \(writerDisplay)")
        }
        if parts.isEmpty {
            return "No repo/session attribution available"
        }
        if item.attribution.command == nil && item.attribution.aiAgentSession == nil {
            parts.append("runtime link unavailable")
        }
        return parts.joined(separator: " · ")
    }

    private func storageTopOffenders(_ report: StorageHygieneReportModel) -> [StorageTopOffender] {
        var offenders: [StorageTopOffender] = []

        if let repo = report.repoFootprints.max(by: {
            storageGrowthRank(for: $0) < storageGrowthRank(for: $1)
        }) {
            let delta = storageGrowthDelta(for: repo)
            offenders.append(
                StorageTopOffender(
                    id: "repo",
                    title: "Repo",
                    value: delta.map(storageSignedBytes) ?? formatBytes(repo.currentSizeBytes),
                    detail: delta == nil
                        ? "\(repo.repoName) · baseline pending"
                        : "\(repo.repoName) · \(storageGrowthWindow(for: repo))",
                    systemImage: "folder.badge.gearshape",
                    tone: (delta ?? 0) > 0 ? AetowerDesign.Status.warning : AetowerDesign.Tone.disk
                )
            )
        }

        if let agent = report.agentHygiene.agents.first {
            let bytes = agent.weekArtifactBytes > 0 ? agent.weekArtifactBytes : agent.artifactBytes
            offenders.append(
                StorageTopOffender(
                    id: "agent",
                    title: "Agent",
                    value: formatBytes(bytes),
                    detail: "\(agent.displayName) · \(formatPercent(agent.weekRebuildablePercent)) rebuildable this week",
                    systemImage: "sparkles.rectangle.stack",
                    tone: AetowerDesign.agentColor(agent.provider)
                )
            )
        }

        if let growthEvent = storageGrowthEvents(from: report).first {
            offenders.append(
                StorageTopOffender(
                    id: "folder",
                    title: "Folder",
                    value: "+\(formatBytes(UInt64(growthEvent.deltaBytes)))",
                    detail: "\(growthEvent.displayName) · \(growthEvent.repoName ?? "unattributed")",
                    systemImage: cleanupTierIcon(growthEvent.cleanupTier),
                    tone: tone(forCleanupTier: growthEvent.cleanupTier)
                )
            )
        } else if let item = visibleStorageItems(from: report).max(by: { $0.sizeBytes < $1.sizeBytes }) {
            offenders.append(
                StorageTopOffender(
                    id: "folder",
                    title: "Folder",
                    value: formatBytes(item.sizeBytes),
                    detail: "\(item.displayName) · baseline pending",
                    systemImage: cleanupTierIcon(item.cleanupTier),
                    tone: tone(forCleanupTier: item.cleanupTier)
                )
            )
        }

        return offenders
    }

    private func storageGrowthRank(for footprint: StorageRepoFootprintModel) -> Int64 {
        storageGrowthDelta(for: footprint) ?? Int64(clamping: footprint.currentSizeBytes)
    }

    private func storageSignedBytes(_ bytes: Int64) -> String {
        if bytes == 0 {
            return "Flat"
        }
        let absolute = formatBytes(UInt64(abs(bytes)))
        return bytes > 0 ? "+\(absolute)" : "-\(absolute)"
    }

    private func storageGrowthEvents(
        from report: StorageHygieneReportModel
    ) -> [StorageGrowthTimelineEvent] {
        let ledgerEvents = report.growthDeltas
            .filter { $0.deltaBytes > 0 }
            .sorted {
                if $0.scanMillis == $1.scanMillis {
                    return $0.deltaBytes > $1.deltaBytes
                }
                return $0.scanMillis > $1.scanMillis
            }
            .prefix(12)
            .map { delta in
                StorageGrowthTimelineEvent(
                    id: delta.id,
                    timestampMillis: delta.scanMillis,
                    repoName: delta.repoName,
                    repoRoot: delta.repoRoot,
                    branch: delta.gitBranch ?? delta.gitHead,
                    displayName: lastPathComponent(delta.path),
                    path: delta.path,
                    cleanupTier: delta.cleanupTier,
                    deltaBytes: delta.deltaBytes,
                    previousBytes: delta.previousPhysicalBytes,
                    currentBytes: delta.currentPhysicalBytes,
                    command: delta.command,
                    processTree: delta.processTree,
                    aiAgentSession: delta.aiAgentSession,
                    writerSource: delta.writerSource,
                    writerDisplay: delta.writerDisplay,
                    matchedWriterCount: delta.matchedWriterCount ?? 0,
                    matchedFilesystemEventCount: delta.matchedFilesystemEventCount ?? 0,
                    attributionSources: delta.attributionSources ?? [],
                    confidence: delta.attributionConfidence,
                    confidenceScore: delta.attributionConfidenceScore,
                    ambiguous: delta.attributionAmbiguous,
                    attributionSummary: delta.attributionSummary,
                    attributionEvidence: delta.attributionEvidence
                )
            }
        if !ledgerEvents.isEmpty {
            return Array(ledgerEvents)
        }

        let previousItemsByID: [String: UInt64]
        if let previousReport = state.previousStorageHygieneReport {
            previousItemsByID = previousReport.items.reduce(into: [String: UInt64]()) {
                $0[$1.id] = $1.sizeBytes
            }
        } else if let baseline = state.persistedStorageHygieneBaseline {
            previousItemsByID = baseline.items.reduce(into: [String: UInt64]()) {
                $0[$1.id] = $1.sizeBytes
            }
        } else {
            return []
        }
        let minimumDeltaBytes: Int64 = 8 * 1_024 * 1_024
        return visibleStorageItems(from: report).compactMap { item in
            let previousBytes = previousItemsByID[item.id] ?? 0
            let delta = Int64(clamping: item.sizeBytes) - Int64(clamping: previousBytes)
            guard delta >= minimumDeltaBytes else {
                return nil
            }
            return StorageGrowthTimelineEvent(
                id: item.id,
                timestampMillis: item.modifiedMillis ?? report.capturedAtMillis,
                repoName: item.attribution.repoName,
                repoRoot: item.attribution.repoRoot,
                branch: item.attribution.gitBranch ?? item.attribution.gitHead,
                displayName: item.displayName,
                path: item.path,
                cleanupTier: item.cleanupTier,
                deltaBytes: delta,
                previousBytes: previousBytes,
                currentBytes: item.sizeBytes,
                command: item.attribution.command,
                processTree: item.attribution.processTree,
                aiAgentSession: item.attribution.aiAgentSession,
                writerSource: nil,
                writerDisplay: item.attribution.writerDisplay,
                matchedWriterCount: 0,
                matchedFilesystemEventCount: 0,
                attributionSources: ["ui_baseline_diff"],
                confidence: item.attribution.confidence,
                confidenceScore: storageAttributionConfidenceScore(item.attribution.confidence),
                ambiguous: false,
                attributionSummary: "Baseline diff from visible Storage items; indexed writer ledger was not available.",
                attributionEvidence: item.attribution.notes
            )
        }
        .sorted {
            if ($0.timestampMillis ?? 0) == ($1.timestampMillis ?? 0) {
                return $0.deltaBytes > $1.deltaBytes
            }
            return ($0.timestampMillis ?? 0) > ($1.timestampMillis ?? 0)
        }
        .prefix(10)
        .map { $0 }
    }

    private func storageGrowthEventTitle(_ event: StorageGrowthTimelineEvent) -> String {
        let repo = event.repoName ?? event.repoRoot.map(lastPathComponent) ?? "unattributed workspace"
        return "\(repo) added \(formatBytes(UInt64(event.deltaBytes))) to \(event.displayName)"
    }

    private func storageGrowthCorrelationDetail(_ event: StorageGrowthTimelineEvent) -> String {
        var parts: [String] = []
        if event.ambiguous {
            parts.append("ambiguous writer attribution")
        }
        if let writerDisplay = event.writerDisplay {
            parts.append("written by \(writerDisplay)")
        }
        if let command = event.command {
            parts.append("command \(command)")
        }
        if let processTree = event.processTree {
            parts.append("process tree \(processTree)")
        }
        if let session = event.aiAgentSession {
            parts.append("AI session \(session)")
        }
        if let writerSource = event.writerSource {
            parts.append("source \(writerSource)")
        }
        if event.matchedWriterCount > 1 {
            parts.append("\(event.matchedWriterCount) matched writers")
        } else if event.matchedWriterCount == 1 {
            parts.append("1 matched writer")
        }
        if event.matchedFilesystemEventCount > 1 {
            parts.append("\(event.matchedFilesystemEventCount) filesystem events")
        } else if event.matchedFilesystemEventCount == 1 {
            parts.append("1 filesystem event")
        }
        if parts.isEmpty {
            parts.append(event.attributionSummary.isEmpty ? "writer unknown: no command/process/session matched" : event.attributionSummary)
        } else if !event.attributionSummary.isEmpty {
            parts.append(event.attributionSummary)
        }
        return parts.joined(separator: " · ")
    }

    private func storageGrowthEventTime(_ event: StorageGrowthTimelineEvent) -> String {
        guard let timestampMillis = event.timestampMillis else {
            return "time unknown"
        }
        let date = Date(timeIntervalSince1970: Double(timestampMillis) / 1000.0)
        return date.formatted(date: .omitted, time: .shortened)
    }

    private func storageGrowthConfidenceLabel(_ event: StorageGrowthTimelineEvent) -> String {
        if event.ambiguous {
            return "ambiguous \(event.confidenceScore)%"
        }
        return "\(event.confidence) \(event.confidenceScore)%"
    }

    private func storageGrowthConfidenceTone(_ event: StorageGrowthTimelineEvent) -> Color {
        if event.ambiguous {
            return AetowerDesign.Status.warning
        }
        if event.confidenceScore >= 80 {
            return AetowerDesign.Status.ready
        }
        if event.confidenceScore >= 50 {
            return AetowerDesign.Status.warning
        }
        return AetowerDesign.Status.neutral
    }

    private func storageAttributionConfidenceScore(_ confidence: String) -> UInt8 {
        switch confidence.lowercased() {
        case "high": return 82
        case "medium": return 62
        case "ambiguous": return 45
        default: return 25
        }
    }

    private func storageGrowthDelta(for footprint: StorageRepoFootprintModel) -> Int64? {
        if let backendGrowth = footprint.growthBytes {
            return backendGrowth
        }
        if let previous = state.previousStorageHygieneReport?.repoFootprints.first(where: {
            $0.repoRoot == footprint.repoRoot
        }) {
            return Int64(clamping: footprint.currentSizeBytes) - Int64(clamping: previous.currentSizeBytes)
        }
        if let baseline = state.persistedStorageHygieneBaseline?.repoFootprints.first(where: {
            $0.repoRoot == footprint.repoRoot
        }) {
            return Int64(clamping: footprint.currentSizeBytes) - Int64(clamping: baseline.currentSizeBytes)
        }
        return nil
    }

    private func storageGrowthLabel(for footprint: StorageRepoFootprintModel) -> String {
        guard let delta = storageGrowthDelta(for: footprint) else {
            return "baseline pending"
        }
        if delta == 0 {
            return "no growth since last scan"
        }
        let absolute = formatBytes(UInt64(abs(delta)))
        return delta > 0 ? "+\(absolute) since last scan" : "-\(absolute) since last scan"
    }

    private func storageGrowthCompactValue(for footprint: StorageRepoFootprintModel) -> String {
        guard let delta = storageGrowthDelta(for: footprint) else {
            return "Pending"
        }
        if delta == 0 {
            return "Flat"
        }
        let absolute = formatBytes(UInt64(abs(delta)))
        return delta > 0 ? "+\(absolute)" : "-\(absolute)"
    }

    private func storageGrowthWindow(for footprint: StorageRepoFootprintModel) -> String {
        guard storageGrowthDelta(for: footprint) != nil else {
            return footprint.growthWindow
        }
        return state.previousStorageHygieneReport == nil ? "since saved baseline" : "since previous scan"
    }

    private func storageGrowthTone(for footprint: StorageRepoFootprintModel) -> Color {
        guard let delta = storageGrowthDelta(for: footprint) else {
            return .secondary
        }
        if delta > 0 {
            return AetowerDesign.Status.warning
        }
        if delta < 0 {
            return AetowerDesign.Status.ready
        }
        return .secondary
    }

    private func rebuildTimeLabel(_ seconds: UInt64?) -> String {
        guard let seconds else {
            return "manual review"
        }
        if seconds == 0 {
            return "no rebuild expected"
        }
        if seconds < 60 {
            return "~\(seconds)s"
        }
        return "~\(seconds / 60)m"
    }

    private func tone(for item: StorageHygieneItemModel) -> Color {
        item.safety == "safe" ? tone(forCleanupTier: item.cleanupTier) : AetowerDesign.Status.warning
    }

    private func tone(forCleanupTier tier: String) -> Color {
        storageCleanupTierTone(tier)
    }

    private func storageRoleLabel(_ role: String) -> String {
        switch role {
        case "build-artifact":
            return "Build artifact"
        case "dependency-tree":
            return "Dependency tree"
        case "large-file":
            return "Large file"
        case "cold-file":
            return "Cold file"
        case "cache":
            return "Cache"
        case "environment":
            return "Environment"
        case "temporary":
            return "Temporary"
        case "log":
            return "Log"
        default:
            return role.replacingOccurrences(of: "-", with: " ").capitalized
        }
    }

    private func gitStatusLabel(_ status: String) -> String {
        switch status {
        case "tracked":
            return "Tracked"
        case "ignored":
            return "Ignored"
        case "generated-or-cache":
            return "Generated"
        case "untracked":
            return "Untracked"
        case "outside-git":
            return "Outside Git"
        case "repo-linked", "repo-linked-unchecked":
            return "Repo-linked"
        default:
            return "Git unknown"
        }
    }

    private func gitStatusTone(_ status: String) -> Color {
        switch status {
        case "tracked":
            return AetowerDesign.Status.error
        case "untracked":
            return AetowerDesign.Status.warning
        case "ignored", "generated-or-cache":
            return AetowerDesign.Status.ready
        default:
            return .secondary
        }
    }

    private func cleanupTierLabel(_ tier: String) -> String {
        switch tier {
        case "safe":
            return "Safe"
        case "rebuildable":
            return "Rebuildable"
        case "expensive":
            return "Expensive"
        case "risky":
            return "Risky"
        default:
            return tier.capitalized
        }
    }

    private func cleanupTierIcon(_ tier: String) -> String {
        storageCleanupTierIcon(tier)
    }

    private func cleanupRecipeIcon(_ recipe: StorageCleanupRecipeModel) -> String {
        switch recipe.category {
        case "rust":
            return "shippingbox"
        case "swiftpm":
            return "swift"
        case "xcode":
            return "hammer"
        case "python":
            return "curlybraces"
        case "node":
            return "shippingbox.fill"
        case "frontend":
            return "sparkles.rectangle.stack"
        case "tools":
            return "wrench.and.screwdriver"
        case "tests":
            return "checklist"
        case "temporary":
            return "timer"
        case "logs":
            return "doc.text"
        case "release":
            return "archivebox"
        default:
            return cleanupTierIcon(recipe.safety)
        }
    }

    private func cleanupRecipeTone(_ recipe: StorageCleanupRecipeModel) -> Color {
        if recipe.requiresReview {
            return AetowerDesign.Status.warning
        }
        return tone(forCleanupTier: recipe.safety)
    }

    private func cleanupBundleIcon(_ bundle: StorageCleanupBundleModel) -> String {
        if bundle.confidenceScore >= 90 {
            return "checkmark.shield"
        }
        if bundle.safety == "review" {
            return "checklist.checked"
        }
        return "shippingbox"
    }

    private func cleanupBundleTone(_ bundle: StorageCleanupBundleModel) -> Color {
        if bundle.confidenceScore >= 90 {
            return AetowerDesign.Status.ready
        }
        if bundle.confidenceScore >= 70 {
            return AetowerDesign.Status.warning
        }
        return AetowerDesign.Status.error
    }

    private func cleanupBundleManifest(_ bundle: StorageCleanupBundleModel) -> String {
        var lines: [String] = [
            "# Aetower dry-run cleanup bundle",
            "",
            "- Bundle: \(bundle.title)",
            "- Safety: \(bundle.safety)",
            "- Confidence: \(bundle.confidenceScore)%",
            "- Estimated reclaimable: \(formatBytes(bundle.estimatedReclaimableBytes))",
            "- Items: \(bundle.itemCount)",
            "- Dry-run only: \(bundle.dryRunOnly ? "yes" : "no")",
            "",
            "## Prerequisites",
        ]
        lines.append(contentsOf: bundle.prerequisites.map { "- \($0)" })
        lines.append(contentsOf: ["", "## Dry-run verification commands"])
        if bundle.dryRunCommands.isEmpty {
            lines.append("- No verification commands were generated.")
        } else {
            lines.append(contentsOf: bundle.dryRunCommands.map { "- `\($0)`" })
        }

        let cleanupCommands = cleanupBundleCleanupCommandList(bundle)
        lines.append(contentsOf: ["", "## Candidate cleanup command references"])
        if cleanupCommands.isEmpty {
            lines.append("- No command references were generated for this bundle.")
        } else {
            lines.append("- Aetower does not run these commands in-app. In-app cleanup moves staged paths to Finder Trash.")
            lines.append(contentsOf: cleanupCommands.map { "- `\($0)`" })
        }

        lines.append(contentsOf: ["", "## Full manifest"])
        for item in bundle.manifest {
            lines.append("- \(formatBytes(item.sizeBytes)) | \(item.confidenceScore)% | \(item.cleanupTier) | \(item.path)")
            lines.append("  - Cleanup: \(cleanupBundleItemIsActionable(item) ? "Trash-actionable" : "blocked/manual review")")
            if !item.cleanupBlockers.isEmpty {
                lines.append("  - Blockers: \(item.cleanupBlockers.joined(separator: "; "))")
            }
            lines.append("  - Reason: \(item.reason)")
            lines.append("  - Consequence: \(item.consequence)")
            if !item.evidence.isEmpty {
                lines.append("  - Evidence: \(item.evidence.prefix(4).joined(separator: "; "))")
            }
            lines.append("  - Rollback: \(item.rollbackNote)")
        }

        lines.append(contentsOf: ["", "## Rollback notes"])
        lines.append(contentsOf: bundle.rollbackNotes.map { "- \($0)" })
        lines.append(contentsOf: ["", "## Caveats"])
        lines.append(contentsOf: bundle.caveats.map { "- \($0)" })
        return lines.joined(separator: "\n")
    }

    private func cleanupBundleCleanupCommands(_ bundle: StorageCleanupBundleModel) -> String {
        cleanupBundleCleanupCommandList(bundle).joined(separator: "\n")
    }

    private func cleanupBundleCleanupCommandList(_ bundle: StorageCleanupBundleModel) -> [String] {
        var seen = Set<String>()
        var commands: [String] = []
        for item in actionableManifestItems(bundle) {
            guard let command = item.cleanupCommand, seen.insert(command).inserted else {
                continue
            }
            commands.append(command)
        }
        return commands
    }

    private func actionableManifestItems(_ bundle: StorageCleanupBundleModel) -> [StorageCleanupBundleItemModel] {
        bundle.manifest.filter(cleanupBundleItemIsActionable)
    }

    private func cleanupBundleHasActionableCommands(_ bundle: StorageCleanupBundleModel) -> Bool {
        actionableManifestItems(bundle).contains { $0.cleanupCommand != nil }
    }

    private func cleanupBundleItemIsActionable(_ item: StorageCleanupBundleItemModel) -> Bool {
        item.cleanupAllowed
            && item.defaultCleanupAction == "trash"
            && item.cleanupBlockers.isEmpty
            && item.cleanupTier != "risky"
    }

    private func storageItemIsTrashActionable(_ item: StorageHygieneItemModel) -> Bool {
        item.cleanupAllowed
            && item.defaultCleanupAction == "trash"
            && item.cleanupBlockers.isEmpty
            && item.cleanupTier != "risky"
            && !item.sizeTruncated
            && storageCleanupPathExists(item.path)
            && storagePrivilegedCleanupBlocker(for: item.path) == nil
    }

    private var basketSummaryLabel: String {
        cleanupBasket.isEmpty
            ? "Ready"
            : "\(cleanupBasket.count) staged · \(formatBytes(cleanupBasketTotalBytes()))"
    }

    private func cleanupBasketTotalBytes() -> UInt64 {
        cleanupBasket.reduce(UInt64(0)) { total, item in
            let sum = total.addingReportingOverflow(item.estimatedBytes)
            return sum.overflow ? UInt64.max : sum.partialValue
        }
    }

    private var cleanupBasketSheet: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "trash")
                    .foregroundStyle(AetowerDesign.Status.warning)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Cleanup basket")
                        .font(.title3.weight(.semibold))
                    Text("Review staged targets. Aetower moves files to Finder Trash by default and records every action in the local audit log.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 160), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric("Staged", value: "\(cleanupBasket.count)", detail: "target\(cleanupBasket.count == 1 ? "" : "s")")
                footprintMetric("Potential reclaim", value: formatBytes(cleanupBasketTotalBytes()), detail: "Trash first")
                footprintMetric(
                    "Review",
                    value: cleanupBasket.contains(where: \.requiresReview) ? "Needed" : "Ready",
                    detail: "single confirmation"
                )
            }

            ScrollView {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    if cleanupBasket.isEmpty {
                        Label("No cleanup targets are staged.", systemImage: "tray")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(AetowerDesign.Spacing.md)
                            .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    } else {
                        ForEach(cleanupBasket) { item in
                            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                                Image(systemName: cleanupTierIcon(item.cleanupTier))
                                    .foregroundStyle(tone(forCleanupTier: item.cleanupTier))
                                    .frame(width: 18)
                                VStack(alignment: .leading, spacing: 3) {
                                    HStack(spacing: AetowerDesign.Spacing.xs) {
                                        Text(item.title)
                                            .font(.caption.weight(.semibold))
                                        Text(item.source)
                                            .font(.caption2.weight(.semibold))
                                            .foregroundStyle(.secondary)
                                            .padding(.horizontal, 6)
                                            .padding(.vertical, 2)
                                            .background(AetowerDesign.Surface.badge, in: Capsule())
                                    }
                                    Text(item.path)
                                        .font(.system(size: 10, design: .monospaced))
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                    Text(item.reason)
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(2)
                                    Text(item.consequence)
                                        .font(.caption2)
                                        .foregroundStyle(.tertiary)
                                        .lineLimit(2)
                                    if !item.blockers.isEmpty {
                                        Text(item.blockers.joined(separator: "; "))
                                            .font(.caption2)
                                            .foregroundStyle(AetowerDesign.Status.error)
                                            .fixedSize(horizontal: false, vertical: true)
                                    }
                                }
                                Spacer(minLength: AetowerDesign.Spacing.sm)
                                Text(formatBytes(item.estimatedBytes))
                                    .font(.caption2.weight(.semibold))
                                    .foregroundStyle(.secondary)
                                Button("Quick Look") {
                                    quickLook(path: item.path)
                                }
                                Button("Reveal") {
                                    reveal(path: item.path)
                                }
                                Button("Explain") {
                                    classificationExplanation = explanation(for: item)
                                }
                                Button("Remove") {
                                    cleanupBasket.removeAll { $0.id == item.id }
                                    appendCleanupAudit(
                                        action: "unstage",
                                        path: item.path,
                                        detail: "Removed \(item.title) from cleanup basket.",
                                        bytes: item.estimatedBytes,
                                        cleanupTier: item.cleanupTier,
                                        safety: item.safety,
                                        blockers: item.blockers,
                                        succeeded: true
                                    )
                                }
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                            .padding(AetowerDesign.Spacing.sm)
                            .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        }
                    }

                    if !cleanupAuditEvents.isEmpty {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                            Text("Recent audit trail")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                            ForEach(Array(cleanupAuditEvents.prefix(8))) { event in
                                HStack(spacing: AetowerDesign.Spacing.sm) {
                                    Text(event.action)
                                        .font(.caption2.weight(.semibold))
                                    if let cleanupTier = event.cleanupTier {
                                        Text(cleanupTier)
                                            .font(.caption2)
                                            .foregroundStyle(tone(forCleanupTier: cleanupTier))
                                    }
                                    if let safety = event.safety {
                                        Text(safety)
                                            .font(.caption2)
                                            .foregroundStyle(.secondary)
                                    }
                                    Text(event.path)
                                        .font(.system(size: 10, design: .monospaced))
                                        .foregroundStyle(.tertiary)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                    Spacer()
                                    if let succeeded = event.succeeded {
                                        Text(succeeded ? "ok" : "failed")
                                            .font(.caption2)
                                            .foregroundStyle(succeeded ? AetowerDesign.Status.ready : AetowerDesign.Status.error)
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(.trailing, AetowerDesign.Spacing.sm)
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Close") {
                    showCleanupBasket = false
                }
                Button("Clear basket") {
                    for item in cleanupBasket {
                        appendCleanupAudit(
                            action: "unstage",
                            path: item.path,
                            detail: "Cleared \(item.title) from cleanup basket.",
                            bytes: item.estimatedBytes,
                            cleanupTier: item.cleanupTier,
                            safety: item.safety,
                            blockers: item.blockers,
                            succeeded: true
                        )
                    }
                    cleanupBasket.removeAll()
                }
                .disabled(cleanupBasket.isEmpty)
                Spacer()
                Button(cleanupBasket.count == 1 ? "Move staged item to Trash" : "Move \(cleanupBasket.count) staged items to Trash") {
                    let request = basketTrashExecutionRequest()
                    showCleanupBasket = false
                    presentCleanupExecution(request)
                }
                .buttonStyle(.borderedProminent)
                .disabled(cleanupBasket.isEmpty)
            }
        }
        .padding(AetowerDesign.Spacing.xl)
        .frame(width: 760, height: 620, alignment: .topLeading)
    }

    private func stageCleanupBundle(_ bundle: StorageCleanupBundleModel) {
        var staged = 0
        for item in bundle.manifest {
            guard cleanupBundleItemIsActionable(item) else {
                appendCleanupAudit(
                    action: "policy-blocked",
                    path: item.path,
                    detail: item.cleanupBlockers.isEmpty ? "Bundle item requires manual review." : item.cleanupBlockers.joined(separator: "; "),
                    bytes: item.sizeBytes,
                    cleanupTier: item.cleanupTier,
                    safety: item.safety,
                    blockers: item.cleanupBlockers,
                    succeeded: false
                )
                continue
            }
            let basketItem = StorageCleanupBasketItem(
                id: "bundle|\(bundle.id)|\(item.path)",
                title: item.displayName,
                path: item.path,
                source: bundle.title,
                cleanupTier: item.cleanupTier,
                safety: item.safety,
                estimatedBytes: item.sizeBytes,
                reason: item.reason,
                consequence: item.consequence,
                evidence: item.evidence,
                requiresReview: bundle.safety != "safe" || bundle.confidenceScore < 90,
                blockers: item.cleanupBlockers,
                prerequisites: bundle.prerequisites + bundle.caveats
            )
            if stageBasketItem(basketItem) {
                staged += 1
            }
        }
    }

    private func stageCleanupRecipes(_ recipes: [StorageCleanupRecipeModel]) {
        var staged = 0
        for recipe in recipes where stageCleanupRecipe(recipe, showBasket: false) {
            staged += 1
        }
    }

    private func stageStorageHomeAction(_ action: StorageHomeAction, presentExecution: Bool = false) {
        if let bundle = action.cleanupBundle {
            let stagedBefore = cleanupBasket.count
            stageCleanupBundle(bundle)
            if presentExecution, cleanupBasket.count > stagedBefore {
                presentCleanupExecution(basketTrashExecutionRequest())
            } else if cleanupBasket.count == stagedBefore {
                copy(storageHomeActionPlan(action))
            }
            return
        }

        var staged = 0
        for item in uniqueStorageItems(action.stageItems).prefix(80) {
            if stageCleanupItem(item, showBasket: false) {
                staged += 1
            }
        }
        if presentExecution, !cleanupBasket.isEmpty {
            presentCleanupExecution(basketTrashExecutionRequest())
        } else if staged == 0 {
            copy(storageHomeActionPlan(action))
        }
    }

    private func directCleanItems(from items: [StorageHygieneItemModel]) -> [StorageHygieneItemModel] {
        uniqueStorageItems(items).filter {
            storageItemIsTrashActionable($0)
                && $0.safety == "safe"
                && !directTrashInFlightPaths.contains($0.path)
        }
    }

    private func trashStorageItemsDirectly(
        _ items: [StorageHygieneItemModel],
        sourceTitle: String
    ) {
        let candidates = directCleanItems(from: items)
        guard !candidates.isEmpty else { return }

        let limited = Array(candidates.prefix(80))
        let paths = limited.map(\.path)
        for path in paths {
            directTrashInFlightPaths.insert(path)
        }

        let activeWriterProbe = state.cleanupActiveWriterProbe()
        Task.detached(priority: .utility) {
            let result = Self.movePathsToTrash(paths, activeWriterProbe: activeWriterProbe)
            await MainActor.run {
                for path in paths {
                    directTrashInFlightPaths.remove(path)
                }
                recordDirectTrashResult(
                    items: limited,
                    result: result,
                    sourceTitle: sourceTitle
                )
            }
        }
    }

    private func recordDirectTrashResult(
        items: [StorageHygieneItemModel],
        result: StorageCleanupExecutionResult,
        sourceTitle: String
    ) {
        var metadataByPath: [String: StorageHygieneItemModel] = [:]
        for item in items where metadataByPath[item.path] == nil {
            metadataByPath[item.path] = item
        }
        let moved = Set(result.movedPaths)
        let alreadyReclaimed = Set(result.failedPaths.compactMap { path, reason in
            reason == "Path no longer exists" ? path : nil
        })
        let resolved = moved.union(alreadyReclaimed)

        cleanupBasket.removeAll { resolved.contains($0.path) }
        state.markStoragePathsMovedToTrash(Array(resolved))

        for (path, trashURL) in result.movedTrashURLs {
            guard let metadata = metadataByPath[path] else { continue }
            trashedItemURLsByOriginalPath[path] = trashURL
            StorageTrackedTrashStore.upsert(
                originalPath: path,
                trashURL: trashURL,
                bytes: metadata.sizeBytes
            )
        }

        trashPendingBytes = result.movedPaths.reduce(trashPendingBytes) { acc, path in
            let (sum, overflow) = acc.addingReportingOverflow(metadataByPath[path]?.sizeBytes ?? 0)
            return overflow ? UInt64.max : sum
        }

        for path in metadataByPath.keys.sorted() {
            let metadata = metadataByPath[path]
            let pathSucceeded = moved.contains(path)
            let pathAlreadyReclaimed = alreadyReclaimed.contains(path)
            appendCleanupAudit(
                action: pathSucceeded ? "direct-trash" : pathAlreadyReclaimed ? "already-reclaimed" : "failed-direct-trash",
                path: path,
                detail: pathSucceeded
                    ? "One-click Clean moved \(sourceTitle) target to Finder Trash."
                    : pathAlreadyReclaimed
                        ? "Path no longer exists; treating it as already reclaimed."
                        : (result.failedPaths[path] ?? "Not attempted."),
                bytes: metadata?.sizeBytes ?? 0,
                cleanupTier: metadata?.cleanupTier,
                safety: metadata?.safety,
                blockers: metadata?.cleanupBlockers ?? [],
                succeeded: pathSucceeded || pathAlreadyReclaimed
            )
        }

        let movedCount = result.movedPaths.count
        let failedCount = result.failedPaths.count
        let bytes = result.movedPaths.reduce(UInt64(0)) { total, path in
            let (sum, overflow) = total.addingReportingOverflow(metadataByPath[path]?.sizeBytes ?? 0)
            return overflow ? UInt64.max : sum
        }
        presentDirectTrashUndo(
            StorageDirectTrashUndo(
                message: movedCount > 0
                    ? "\(sourceTitle): moved \(movedCount) item\(movedCount == 1 ? "" : "s") (\(formatBytes(bytes))) to Trash"
                    : "\(sourceTitle): no items moved\(failedCount > 0 ? " (\(failedCount) issue\(failedCount == 1 ? "" : "s"))" : "")",
                originalPath: result.movedPaths.first ?? sourceTitle,
                trashURL: result.movedTrashURLs[result.movedPaths.first ?? ""],
                bytes: bytes,
                succeeded: movedCount > 0
            )
        )
    }

    private func stageReclaimFolder(_ folder: StorageReclaimFolderRow) {
        if storageReclaimFolderIsTrashActionable(folder) {
            let item = StorageCleanupBasketItem(
                id: "folder|\(folder.path)",
                title: folder.displayName,
                path: folder.path,
                source: folder.source,
                cleanupTier: folder.cleanupTier,
                safety: folder.safety,
                estimatedBytes: folder.sizeBytes,
                reason: "Folder-level cleanup candidate from \(folder.source).",
                consequence: "Moves the folder to Finder Trash. Restore from Trash if needed; rebuildable contents may be regenerated by their tools.",
                evidence: [
                    "Folder contains \(folder.itemCount) known cleanup candidate\(folder.itemCount == 1 ? "" : "s").",
                    "Policy action: \(folder.defaultCleanupAction).",
                ],
                requiresReview: folder.safety != "safe",
                blockers: folder.cleanupBlockers,
                prerequisites: []
            )
            if !stageBasketItem(item) {
                copy(storageReclaimFolderPlan(folder))
            }
            return
        }

        var staged = 0
        for item in uniqueStorageItems(folder.stageItems).prefix(80) where stageCleanupItem(item, showBasket: false) {
            staged += 1
        }
        if staged == 0 {
            copy(storageReclaimFolderPlan(folder))
        }
    }

    private func storageHomeActionCanMoveToTrash(_ action: StorageHomeAction) -> Bool {
        if let bundle = action.cleanupBundle {
            return bundle.safety == "safe"
                && bundle.confidenceScore >= 90
                && cleanupBundleHasActionableCommands(bundle)
        }
        return !action.stageItems.isEmpty
            && action.stageItems.allSatisfy { item in
                storageItemIsTrashActionable(item) && item.safety == "safe"
            }
    }

    private func storageReclaimActionDecision(for action: StorageHomeAction) -> StorageReclaimActionDecision {
        StorageReclaimPolicy.primaryActionDecision(
            hasStageableContent: action.hasStageableItems,
            canMoveToTrash: storageHomeActionCanMoveToTrash(action)
        )
    }

    private func stagePreventionSuggestion(
        _ suggestion: StoragePreventionSuggestionModel,
        report: StorageHygieneReportModel
    ) {
        let candidates: [StorageHygieneItemModel]
        let visibleItems = visibleStorageItems(from: report)
        switch suggestion.trigger {
        case "safe-reclaim":
            candidates = visibleItems.filter {
                $0.cleanupTier == "safe" && storageItemIsTrashActionable($0)
            }
        case "post-build":
            let repoRoot = suggestion.id.replacingOccurrences(of: "post-build-cleanup|", with: "")
            candidates = visibleItems.filter {
                $0.cleanupTier == "rebuildable"
                    && $0.attribution.repoRoot == repoRoot
                    && storageItemIsTrashActionable($0)
            }
        case "artifact-budget":
            candidates = visibleItems.filter {
                ($0.cleanupTier == "safe" || $0.cleanupTier == "rebuildable")
                    && storageItemIsTrashActionable($0)
            }
        default:
            candidates = []
        }

        var staged = 0
        for item in uniqueStorageItems(candidates).prefix(80) {
            if stageCleanupItem(item, showBasket: false) {
                staged += 1
            }
        }
        if staged == 0 {
            copy(preventionSuggestionPlan(suggestion, report: report))
        }
    }

    private func preventionSuggestionPlan(
        _ suggestion: StoragePreventionSuggestionModel,
        report: StorageHygieneReportModel
    ) -> String {
        [
            "# Aetower storage prevention suggestion",
            "",
            "- Trigger: \(suggestion.trigger)",
            "- Suggestion: \(suggestion.title)",
            "- Safety: \(suggestion.safety)",
            "- Estimated reclaimable: \(formatBytes(suggestion.estimatedReclaimableBytes))",
            "- Requires approval: \(suggestion.requiresApproval ? "yes" : "no")",
            "",
            "## Detail",
            suggestion.detail,
            "",
            "## Policy",
            "Aetower is warning-only by default. Auto-trash is \(report.budgetGuardrails.autoTrashSafeTierEnabled ? "enabled for Safe-tier only" : "disabled").",
        ].joined(separator: "\n")
    }

    private func storageHomeActionPlan(_ action: StorageHomeAction) -> String {
        var lines = [
            "# Aetower storage action",
            "",
            "- Action: \(action.title)",
            "- Estimated bytes: \(formatBytes(action.bytes))",
            "- Items: \(action.itemCount)",
            "- Confidence: \(action.confidence)%",
            "- Consequence: \(action.consequence)",
            "",
            "## Recommended path",
            action.hasStageableItems
                ? "Stage eligible items into the cleanup basket, review once, then move them to Finder Trash."
                : "Review classification evidence first. Aetower did not find unattended cleanup candidates for this lane.",
        ]

        if !action.sampleItems.isEmpty {
            lines.append(contentsOf: ["", "## Sample items"])
            for item in action.sampleItems.prefix(12) {
                lines.append("- \(formatBytes(item.sizeBytes)) | \(item.cleanupTier) | \(item.path)")
                if !item.cleanupBlockers.isEmpty {
                    lines.append("  - Blocked: \(item.cleanupBlockers.joined(separator: "; "))")
                }
                lines.append("  - Why: \(item.reason)")
            }
        }

        if let bundle = action.cleanupBundle {
            lines.append(contentsOf: ["", "## Engine cleanup bundle"])
            lines.append("- Bundle: \(bundle.title)")
            lines.append("- Manifest bytes: \(formatBytes(bundle.estimatedReclaimableBytes))")
            lines.append("- Manifest items: \(bundle.itemCount)")
            lines.append("- Confidence: \(bundle.confidenceScore)%")
            lines.append(contentsOf: ["", "## Bundle manifest"])
            for item in bundle.manifest.prefix(16) {
                lines.append("- \(formatBytes(item.sizeBytes)) | \(item.cleanupTier) | \(item.path)")
                if !item.cleanupBlockers.isEmpty {
                    lines.append("  - Blocked: \(item.cleanupBlockers.joined(separator: "; "))")
                }
                lines.append("  - Why: \(item.reason)")
            }
        }

        if !action.growthEvents.isEmpty {
            lines.append(contentsOf: ["", "## Growth evidence"])
            for event in action.growthEvents.prefix(12) {
                lines.append("- +\(formatBytes(UInt64(event.deltaBytes))) | \(event.path)")
                lines.append("  - \(storageGrowthCorrelationDetail(event))")
            }
        }

        return lines.joined(separator: "\n")
    }

    private func storageReclaimFolderPlan(_ folder: StorageReclaimFolderRow) -> String {
        var lines = [
            "# Aetower folder cleanup plan",
            "",
            "- Folder: \(folder.path)",
            "- Source: \(folder.source)",
            "- Estimated bytes: \(formatBytes(folder.sizeBytes))",
            "- Known candidates: \(folder.itemCount)",
            "- Cleanup tier: \(cleanupTierLabel(folder.cleanupTier))",
            "- Direct Trash allowed: \(storageReclaimFolderIsTrashActionable(folder) ? "yes" : "no")",
            "",
            "## Recommended path",
            storageReclaimFolderIsTrashActionable(folder)
                ? "Stage this folder, review the basket once, then move it to Finder Trash."
                : "Stage the safe child items or review the folder manually in Finder. Aetower will not delete the whole folder unattended.",
        ]

        if !folder.cleanupBlockers.isEmpty {
            lines.append(contentsOf: ["", "## Blockers"])
            lines.append(contentsOf: folder.cleanupBlockers.map { "- \($0)" })
        }

        if !folder.stageItems.isEmpty {
            lines.append(contentsOf: ["", "## Stageable child items"])
            for item in folder.stageItems.prefix(16) {
                lines.append("- \(formatBytes(item.sizeBytes)) | \(item.cleanupTier) | \(item.path)")
            }
        }

        return lines.joined(separator: "\n")
    }

    private func uniqueStorageItems(_ items: [StorageHygieneItemModel]) -> [StorageHygieneItemModel] {
        var seen = Set<String>()
        var result: [StorageHygieneItemModel] = []
        for item in items where seen.insert(item.path).inserted {
            result.append(item)
        }
        return result
    }

    private func storageCleanupPathExists(_ path: String) -> Bool {
        let normalized = URL(fileURLWithPath: path).standardizedFileURL.path
        return FileManager.default.fileExists(atPath: normalized)
    }

    private func storagePrivilegedCleanupBlocker(for path: String) -> String? {
        TrashService.privilegedCleanupBlocker(for: path)
    }

    private func recordAlreadyReclaimedPath(
        _ path: String,
        bytes: UInt64,
        cleanupTier: String?,
        safety: String?
    ) {
        appendCleanupAudit(
            action: "already-reclaimed",
            path: path,
            detail: "Path no longer exists; treating it as already reclaimed and refreshing storage estimates.",
            bytes: bytes,
            cleanupTier: cleanupTier,
            safety: safety,
            succeeded: true
        )
        cleanupBasket.removeAll { $0.path == path }
        state.markStoragePathsMovedToTrash([path])
    }

    @discardableResult
    private func stageCleanupItem(_ item: StorageHygieneItemModel, showBasket: Bool = false) -> Bool {
        guard storageCleanupPathExists(item.path) else {
            recordAlreadyReclaimedPath(
                item.path,
                bytes: item.sizeBytes,
                cleanupTier: item.cleanupTier,
                safety: item.safety
            )
            return false
        }
        if let blocker = storagePrivilegedCleanupBlocker(for: item.path) {
            appendCleanupAudit(
                action: "policy-blocked",
                path: item.path,
                detail: blocker,
                bytes: item.sizeBytes,
                cleanupTier: item.cleanupTier,
                safety: item.safety,
                blockers: [blocker],
                succeeded: false
            )
            return false
        }
        guard storageItemIsTrashActionable(item) else {
            appendCleanupAudit(
                action: "policy-blocked",
                path: item.path,
                detail: item.cleanupBlockers.isEmpty ? "Item is not eligible for Trash cleanup." : item.cleanupBlockers.joined(separator: "; "),
                bytes: item.sizeBytes,
                cleanupTier: item.cleanupTier,
                safety: item.safety,
                blockers: item.cleanupBlockers,
                succeeded: false
            )
            return false
        }
        let basketItem = StorageCleanupBasketItem(
            id: "item|\(item.id)",
            title: item.displayName,
            path: item.path,
            source: "artifact",
            cleanupTier: item.cleanupTier,
            safety: item.safety,
            estimatedBytes: item.sizeBytes,
            reason: item.reason,
            consequence: item.cleanupConsequence,
            evidence: item.evidence,
            requiresReview: item.safety != "safe",
            blockers: item.cleanupBlockers,
            prerequisites: ["Reveal and inspect the target before moving it to Trash."]
        )
        let staged = stageBasketItem(basketItem)
        if staged && showBasket {
            showCleanupBasket = true
        }
        return staged
    }

    @discardableResult
    private func stageCleanupRecipe(_ recipe: StorageCleanupRecipeModel, showBasket: Bool = false) -> Bool {
        let item = StorageCleanupBasketItem(
            id: "recipe|\(recipe.id)",
            title: recipe.title,
            path: recipe.affectedPath,
            source: recipe.category,
            cleanupTier: recipe.safety,
            safety: recipe.safety,
            estimatedBytes: recipe.estimatedReclaimableBytes,
            reason: recipe.reason,
            consequence: "Aetower will move the affected path to Finder Trash. The command is retained as a manual reference only.",
            evidence: recipe.prerequisites.isEmpty ? [recipe.command] : recipe.prerequisites,
            requiresReview: recipe.requiresReview,
            blockers: [],
            prerequisites: recipe.prerequisites
        )
        let staged = stageBasketItem(item)
        if staged && showBasket {
            showCleanupBasket = true
        }
        return staged
    }

    @discardableResult
    private func stageBasketItem(_ item: StorageCleanupBasketItem) -> Bool {
        guard storageCleanupPathExists(item.path) else {
            recordAlreadyReclaimedPath(
                item.path,
                bytes: item.estimatedBytes,
                cleanupTier: item.cleanupTier,
                safety: item.safety
            )
            return false
        }
        if let blocker = storagePrivilegedCleanupBlocker(for: item.path) {
            appendCleanupAudit(
                action: "policy-blocked",
                path: item.path,
                detail: blocker,
                bytes: item.estimatedBytes,
                cleanupTier: item.cleanupTier,
                safety: item.safety,
                blockers: [blocker],
                succeeded: false
            )
            return false
        }
        guard item.blockers.isEmpty else {
            appendCleanupAudit(
                action: "policy-blocked",
                path: item.path,
                detail: item.blockers.joined(separator: "; "),
                bytes: item.estimatedBytes,
                cleanupTier: item.cleanupTier,
                safety: item.safety,
                blockers: item.blockers,
                succeeded: false
            )
            return false
        }
        guard !cleanupBasket.contains(where: { $0.path == item.path }) else {
            return false
        }
        cleanupBasket.append(item)
        appendCleanupAudit(
            action: "stage",
            path: item.path,
            detail: "Staged \(item.title) for Finder Trash cleanup. \(item.reason) Consequence: \(item.consequence)",
            bytes: item.estimatedBytes,
            cleanupTier: item.cleanupTier,
            safety: item.safety,
            blockers: item.blockers,
            succeeded: true
        )
        return true
    }

    private func basketTrashExecutionRequest() -> StorageCleanupExecutionRequest {
        let prerequisites = uniqueStrings(cleanupBasket.flatMap(\.prerequisites))
        return StorageCleanupExecutionRequest(
            title: "Move cleanup basket to Trash",
            subtitle: "Move staged cleanup targets to Finder Trash. Nothing is permanently deleted by this action.",
            targetPaths: uniquePaths(cleanupBasket.map(\.path)),
            estimatedBytes: cleanupBasketTotalBytes(),
            requiresReview: cleanupBasket.contains(where: \.requiresReview),
            prerequisites: prerequisites
        )
    }

    // MARK: - Sticky batch bar + one-click trash

    /// Persistent bar shown whenever the basket is non-empty: staging is now
    /// silent, so this is the standing affordance for reviewing/executing the
    /// batch (previously the basket sheet popped over the UI on every stage).
    /// Nimble floating pill (replaces the old full-width bottom bar). Hugs its
    /// content, hovers over the workspace, and shows only the state and the one
    /// or two actions that matter for the current step.
    private var cleanupActionPill: some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            if !cleanupBasket.isEmpty {
                Image(systemName: "tray.full.fill")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                Text("\(cleanupBasket.count) staged")
                    .font(.callout.weight(.semibold))
                Text(formatBytes(cleanupBasketTotalBytes()))
                    .font(.callout).foregroundStyle(.secondary)

                Button {
                    cleanupBasket.removeAll()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.tertiary)
                .help("Clear the basket")

                Divider().frame(height: 16)

                Button("Review") { showCleanupBasket = true }
                    .buttonStyle(.bordered)
                Button {
                    presentCleanupExecution(basketTrashExecutionRequest())
                } label: {
                    Label("Move to Trash", systemImage: "trash")
                }
                .buttonStyle(.borderedProminent)
            }

            if trashPendingBytes > 0 {
                if !cleanupBasket.isEmpty {
                    Divider().frame(height: 16)
                }
                Image(systemName: "arrow.down.circle.fill")
                    .foregroundStyle(.secondary)
                Text("\(formatBytes(trashPendingBytes)) in Trash")
                    .font(.callout).foregroundStyle(.secondary)
                Button("Open") { openTrash() }
                    .buttonStyle(.plain)
                    .foregroundStyle(AetowerDesign.Status.ready)
                // Prominent only when emptying is the sole remaining step; a
                // non-empty basket keeps Move to Trash as the primary action.
                if cleanupBasket.isEmpty {
                    Button {
                        requestEmptyTrashConfirmation()
                    } label: {
                        Label(emptyTrashInFlight ? "Emptying…" : "Empty Trash", systemImage: "trash.slash")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(emptyTrashInFlight)
                } else {
                    Button {
                        requestEmptyTrashConfirmation()
                    } label: {
                        Label(emptyTrashInFlight ? "Emptying…" : "Empty Trash", systemImage: "trash.slash")
                    }
                    .buttonStyle(.bordered)
                    .disabled(emptyTrashInFlight)
                }
            }
        }
        .controlSize(.small)
        .padding(.horizontal, AetowerDesign.Spacing.lg)
        .padding(.vertical, AetowerDesign.Spacing.sm)
        .background(.regularMaterial, in: Capsule())
        .overlay(Capsule().strokeBorder(AetowerDesign.Surface.divider, lineWidth: 1))
        .shadow(color: .black.opacity(0.18), radius: 14, y: 5)
        .fixedSize()
    }

    private func directTrashUndoToast(_ undo: StorageDirectTrashUndo) -> some View {
        HStack(spacing: AetowerDesign.Spacing.md) {
            Image(systemName: undo.succeeded ? "trash" : "exclamationmark.triangle")
            Text(undo.message)
                .font(.callout)
                .lineLimit(1)
            if undo.trashURL != nil {
                Button("Undo") { undoDirectTrash(undo) }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
            }
            Button {
                dismissDirectTrashUndo()
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal, AetowerDesign.Spacing.lg)
        .padding(.vertical, AetowerDesign.Spacing.md)
        .background(.regularMaterial, in: Capsule())
        .shadow(radius: 8, y: 2)
    }

    /// One-click Finder Trash for safe-tier items; non-safe items fall back to
    /// silent staging so the review flow is preserved for anything risky.
    private func trashItemDirectly(_ item: StorageHygieneItemModel) {
        guard storageItemIsTrashActionable(item), item.safety == "safe" else {
            _ = stageCleanupItem(item)
            return
        }
        let path = item.path
        guard !directTrashInFlightPaths.contains(path) else { return }
        directTrashInFlightPaths.insert(path)
        let title = item.displayName
        let bytes = item.sizeBytes
        let tier = item.cleanupTier
        let safety = item.safety
        let activeWriterProbe = state.cleanupActiveWriterProbe()
        Task.detached(priority: .utility) {
            let outcome = Self.trashSingleItem(path, activeWriterProbe: activeWriterProbe)
            await MainActor.run {
                directTrashInFlightPaths.remove(path)
                appendCleanupAudit(
                    action: outcome.trashURL != nil ? "trash" : "failed-trash",
                    path: path,
                    detail: outcome.message,
                    bytes: bytes,
                    cleanupTier: tier,
                    safety: safety,
                    succeeded: outcome.trashURL != nil
                )
                cleanupBasket.removeAll { $0.path == path }
                if outcome.trashURL != nil {
                    let (sum, overflow) = trashPendingBytes.addingReportingOverflow(bytes)
                    trashPendingBytes = overflow ? UInt64.max : sum
                    trashedItemURLsByOriginalPath[path] = outcome.trashURL
                    if let trashURL = outcome.trashURL {
                        StorageTrackedTrashStore.upsert(
                            originalPath: path,
                            trashURL: trashURL,
                            bytes: bytes
                        )
                    }
                    state.markStoragePathsMovedToTrash([path])
                }
                presentDirectTrashUndo(
                    StorageDirectTrashUndo(
                        message: outcome.trashURL != nil
                            ? "\(title) (\(formatBytes(bytes))) moved to Trash"
                            : "Could not trash \(title): \(outcome.message)",
                        originalPath: path,
                        trashURL: outcome.trashURL,
                        bytes: bytes,
                        succeeded: outcome.trashURL != nil
                    )
                )
            }
        }
    }

    nonisolated private static func trashSingleItem(
        _ path: String,
        activeWriterProbe: TrashService.ActiveWriterProbe?
    ) -> (trashURL: URL?, message: String) {
        let outcome = TrashService.trash(path, activeWriterProbe: activeWriterProbe)
        return (outcome.trashURL, outcome.message)
    }

    private func presentDirectTrashUndo(_ undo: StorageDirectTrashUndo) {
        directTrashUndoDismissTask?.cancel()
        withAnimation(.easeOut(duration: 0.2)) { directTrashUndo = undo }
        directTrashUndoDismissTask = Task {
            try? await Task.sleep(nanoseconds: 8_000_000_000)
            guard !Task.isCancelled else { return }
            await MainActor.run { dismissDirectTrashUndo() }
        }
    }

    private func dismissDirectTrashUndo() {
        directTrashUndoDismissTask?.cancel()
        directTrashUndoDismissTask = nil
        withAnimation(.easeIn(duration: 0.15)) { directTrashUndo = nil }
    }

    private func undoDirectTrash(_ undo: StorageDirectTrashUndo) {
        guard let trashURL = undo.trashURL else {
            dismissDirectTrashUndo()
            return
        }
        let destination = undo.originalPath
        Task.detached(priority: .utility) {
            var message: String
            var succeeded: Bool
            do {
                try FileManager.default.moveItem(
                    at: trashURL,
                    to: URL(fileURLWithPath: destination)
                )
                message = "Restored from Trash"
                succeeded = true
            } catch {
                message = "Restore failed: \(error.localizedDescription)"
                succeeded = false
            }
            await MainActor.run {
                appendCleanupAudit(
                    action: succeeded ? "restore" : "failed-restore",
                    path: destination,
                    detail: message,
                    bytes: undo.bytes,
                    succeeded: succeeded
                )
                if succeeded {
                    trashPendingBytes = trashPendingBytes >= undo.bytes
                        ? trashPendingBytes - undo.bytes
                        : 0
                    trashedItemURLsByOriginalPath.removeValue(forKey: destination)
                    StorageTrackedTrashStore.remove(originalPath: destination)
                }
                dismissDirectTrashUndo()
            }
        }
    }

    private func openTrash() {
        if let trashURL = trashedItemURLsByOriginalPath.values.first {
            NSWorkspace.shared.open(trashURL.deletingLastPathComponent())
        } else {
            NSWorkspace.shared.open(URL(fileURLWithPath: NSHomeDirectory() + "/.Trash"))
        }
    }

    private func requestEmptyTrashConfirmation() {
        refreshTrackedTrashState()
        guard !trashedItemURLsByOriginalPath.isEmpty else {
            emptyTrash()
            return
        }
        appendCleanupAudit(
            action: "empty-trash-confirmation-requested",
            path: "Aetower tracked Trash",
            detail: "User requested permanent deletion of \(trashedItemURLsByOriginalPath.count) Aetower-tracked Trash item(s).",
            bytes: trashPendingBytes,
            succeeded: true
        )
        confirmEmptyTrash = true
    }

    private func refreshTrackedTrashState() {
        let snapshot = StorageTrackedTrashStore.reconcileExisting()
        trashedItemURLsByOriginalPath = snapshot.urls
        trashPendingBytes = snapshot.pendingBytes
    }

    /// Permanently delete only the Trash items Aetower moved and still tracks.
    /// This avoids Finder's all-or-nothing "empty trash", avoids deleting the
    /// user's unrelated Trash contents, and works for per-volume Trash URLs.
    private func emptyTrash() {
        let trashURLsByOriginalPath = trashedItemURLsByOriginalPath
        guard !trashURLsByOriginalPath.isEmpty else {
            appendCleanupAudit(
                action: "failed-empty-trash",
                path: "Aetower tracked Trash",
                detail: "No Aetower-tracked Trash URLs are available to empty.",
                bytes: trashPendingBytes,
                succeeded: false
            )
            trashPendingBytes = 0
            StorageTrackedTrashStore.clear()
            return
        }
        emptyTrashInFlight = true
        let pending = trashPendingBytes
        let trashURLs = Array(trashURLsByOriginalPath.values)
        Task.detached(priority: .userInitiated) {
            let outcome = Self.emptyTrackedTrashItems(trashURLs)
            await MainActor.run {
                emptyTrashInFlight = false
                let succeeded = outcome.failed == 0
                appendCleanupAudit(
                    action: succeeded ? "empty-trash" : "failed-empty-trash",
                    path: "Aetower tracked Trash",
                    detail: emptyTrashDetail(outcome),
                    bytes: pending,
                    succeeded: succeeded
                )
                if outcome.failed == 0 {
                    trashPendingBytes = 0
                    trashedItemURLsByOriginalPath.removeAll()
                    StorageTrackedTrashStore.clear()
                } else if outcome.removed > 0 || outcome.missing > 0 {
                    reconcileTrackedTrashAfterPartialEmpty()
                }
            }
        }
    }

    nonisolated private static func emptyTrackedTrashItems(
        _ urls: [URL]
    ) -> (removed: Int, missing: Int, failed: Int, firstError: String?) {
        let outcome = TrashService.emptyTrashItems(urls)
        return (outcome.removed, outcome.missing, outcome.failed, outcome.firstError)
    }

    private func emptyTrashDetail(
        _ outcome: (removed: Int, missing: Int, failed: Int, firstError: String?)
    ) -> String {
        if outcome.removed == 0, outcome.missing == 0, outcome.failed == 0 {
            return "No Aetower-tracked Trash items were present."
        }
        var parts = [
            "Deleted \(outcome.removed) Aetower-tracked item\(outcome.removed == 1 ? "" : "s") from Trash",
        ]
        if outcome.missing > 0 {
            parts.append("\(outcome.missing) already missing")
        }
        if outcome.failed > 0 {
            parts.append("\(outcome.failed) failed (\(outcome.firstError ?? "in use or protected"))")
        }
        return parts.joined(separator: "; ") + "."
    }

    private func reconcileTrackedTrashAfterPartialEmpty() {
        refreshTrackedTrashState()
    }

    private func uniquePaths(_ paths: [String]) -> [String] {
        var seen = Set<String>()
        var result: [String] = []
        for path in paths where seen.insert(path).inserted {
            result.append(path)
        }
        return result
    }

    private func uniqueStrings(_ values: [String]) -> [String] {
        var seen = Set<String>()
        var result: [String] = []
        for value in values {
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty, seen.insert(trimmed).inserted {
                result.append(trimmed)
            }
        }
        return result
    }

    private func targetPathList(_ paths: [String]) -> String {
        if paths.isEmpty {
            return "No targets available."
        }
        let visible = paths.prefix(80).joined(separator: "\n")
        if paths.count > 80 {
            return "\(visible)\n... \(paths.count - 80) more target(s) ..."
        }
        return visible
    }

    private func cleanupExecutionButtonTitle(_ request: StorageCleanupExecutionRequest) -> String {
        request.targetPaths.count == 1 ? "Move to Trash" : "Move \(request.targetPaths.count) to Trash"
    }

    private func cleanupExecutionCanRun(_ request: StorageCleanupExecutionRequest) -> Bool {
        !request.targetPaths.isEmpty
    }

    private func presentCleanupExecution(_ request: StorageCleanupExecutionRequest) {
        cleanupExecutionResult = nil
        cleanupExecutionIsRunning = false
        pendingCleanupExecutionRequest = request
    }

    private func runCleanupExecution(_ request: StorageCleanupExecutionRequest) {
        cleanupExecutionIsRunning = true
        cleanupExecutionResult = nil
        let targetPaths = request.targetPaths
        let activeWriterProbe = state.cleanupActiveWriterProbe()
        Task.detached(priority: .utility) {
            let result = Self.movePathsToTrash(targetPaths, activeWriterProbe: activeWriterProbe)
            await MainActor.run {
                cleanupExecutionResult = result
                cleanupExecutionIsRunning = false
                recordCleanupExecutionResult(request: request, result: result)
            }
        }
    }

    private func recordCleanupExecutionResult(
        request: StorageCleanupExecutionRequest,
        result: StorageCleanupExecutionResult
    ) {
        let bytesByPath = Dictionary(
            uniqueKeysWithValues: cleanupBasket.map { ($0.path, $0.estimatedBytes) }
        )
        let metadataByPath = Dictionary(
            uniqueKeysWithValues: cleanupBasket.map { ($0.path, $0) }
        )
        let fallbackBytes = request.targetPaths.count == 1 ? request.estimatedBytes : 0
        // Outcomes are per-path: items that reached the Trash leave the basket
        // and audit as "trash" even when a sibling in the same batch failed.
        // The old batch-level bookkeeping marked all 42 paths failed (and kept
        // them staged for a doomed re-run) because 1 of 42 was root-owned.
        let moved = Set(result.movedPaths)
        let alreadyReclaimed = Set(result.failedPaths.compactMap { path, reason in
            reason == "Path no longer exists" ? path : nil
        })
        let resolved = moved.union(alreadyReclaimed)
        cleanupBasket.removeAll { resolved.contains($0.path) }
        state.markStoragePathsMovedToTrash(Array(resolved))
        for (path, trashURL) in result.movedTrashURLs {
            trashedItemURLsByOriginalPath[path] = trashURL
            StorageTrackedTrashStore.upsert(
                originalPath: path,
                trashURL: trashURL,
                bytes: bytesByPath[path] ?? fallbackBytes
            )
        }
        trashPendingBytes = result.movedPaths.reduce(trashPendingBytes) { acc, path in
            let (sum, overflow) = acc.addingReportingOverflow(bytesByPath[path] ?? fallbackBytes)
            return overflow ? UInt64.max : sum
        }

        for path in request.targetPaths {
            let metadata = metadataByPath[path]
            let pathSucceeded = moved.contains(path)
            let pathAlreadyReclaimed = alreadyReclaimed.contains(path)
            appendCleanupAudit(
                action: pathSucceeded ? "trash" : pathAlreadyReclaimed ? "already-reclaimed" : "failed-trash",
                path: path,
                detail: pathSucceeded ? "Moved to Finder Trash."
                    : pathAlreadyReclaimed
                        ? "Path no longer exists; treating it as already reclaimed."
                        : (result.failedPaths[path] ?? "Not attempted."),
                bytes: bytesByPath[path] ?? fallbackBytes,
                cleanupTier: metadata?.cleanupTier,
                safety: metadata?.safety,
                blockers: metadata?.blockers ?? [],
                succeeded: pathSucceeded || pathAlreadyReclaimed
            )
        }
    }

    private func appendCleanupAudit(
        action: String,
        path: String,
        detail: String,
        bytes: UInt64,
        cleanupTier: String? = nil,
        safety: String? = nil,
        blockers: [String] = [],
        succeeded: Bool?
    ) {
        let event = StorageCleanupAuditEvent(
            id: UUID().uuidString,
            timestampMillis: UInt64(Date().timeIntervalSince1970 * 1000),
            action: action,
            path: path,
            detail: detail,
            bytes: bytes,
            cleanupTier: cleanupTier,
            safety: safety,
            blockers: blockers.isEmpty ? nil : blockers,
            succeeded: succeeded
        )
        let auditPersisted = StorageCleanupAuditLog.append(event)
        state.recordStorageCleanupDiagnostics(
            action: action,
            path: path,
            detail: detail,
            bytes: bytes,
            cleanupTier: cleanupTier,
            safety: safety,
            blockerCount: blockers.count,
            succeeded: succeeded,
            auditPersisted: auditPersisted
        )
        cleanupAuditEvents = StorageCleanupAuditLog.loadRecent()
    }

    nonisolated private static func movePathsToTrash(
        _ paths: [String],
        activeWriterProbe: TrashService.ActiveWriterProbe?
    ) -> StorageCleanupExecutionResult {
        let started = Date()
        let outcome = TrashService.trash(paths: paths, activeWriterProbe: activeWriterProbe)
        let lines = outcome.movedPaths.map { "Moved: \($0) -> Trash" }
            + outcome.failedPaths.map { "Failed: \($0.key) - \($0.value)" }
        let movedTrashURLs = Dictionary(
            uniqueKeysWithValues: outcome.movedItems.map { ($0.originalPath, $0.trashURL) }
        )
        return StorageCleanupExecutionResult(
            exitCode: outcome.succeeded ? 0 : 1,
            output: ([outcome.summaryLine] + lines).joined(separator: "\n"),
            durationSeconds: Date().timeIntervalSince(started),
            movedPaths: outcome.movedPaths,
            movedTrashURLs: movedTrashURLs,
            failedPaths: outcome.failedPaths
        )
    }

    private func budgetGuardrailSummary(_ guardrails: StorageBudgetGuardrailsModel) -> String {
        if guardrails.violations.isEmpty {
            return "Warning-only by default: repo artifact, repo growth, total artifact, and volume pressure budgets are watched before storage becomes an emergency."
        }
        return "\(guardrails.violations.count) budget warning\(guardrails.violations.count == 1 ? "" : "s") need review before the machine slows down."
    }

    private func budgetGuardrailTone(_ status: String) -> Color {
        switch status {
        case "critical", "error":
            return AetowerDesign.Status.error
        case "warning", "warn":
            return AetowerDesign.Status.warning
        case "ok":
            return AetowerDesign.Status.ready
        default:
            return .secondary
        }
    }

    private func budgetGuardrailIcon(_ status: String) -> String {
        switch status {
        case "critical", "error":
            return "exclamationmark.octagon"
        case "warning", "warn":
            return "exclamationmark.triangle"
        case "ok":
            return "checkmark.shield"
        default:
            return "gauge.with.dots.needle.67percent"
        }
    }

    private func preventionSuggestionTone(_ suggestion: StoragePreventionSuggestionModel) -> Color {
        switch suggestion.safety {
        case "safe", "non-destructive":
            return AetowerDesign.Status.ready
        case "rebuildable":
            return AetowerDesign.Tone.disk
        case "review":
            return AetowerDesign.Status.warning
        default:
            return .secondary
        }
    }

    private func preventionSuggestionIcon(_ suggestion: StoragePreventionSuggestionModel) -> String {
        switch suggestion.trigger {
        case "safe-reclaim":
            return "trash.circle"
        case "post-build":
            return "hammer.circle"
        case "artifact-budget":
            return "shippingbox.circle"
        case "policy-violation":
            return "calendar.badge.clock"
        default:
            return "bell.badge"
        }
    }

    private func agentSourceSummary(_ agent: StorageAgentArtifactSummaryModel) -> String {
        let sources = agent.attributionSources
            .map(agentAttributionSourceLabel)
            .joined(separator: ", ")
        if sources.isEmpty {
            return "No attribution source reported"
        }
        return "Source: \(sources)"
    }

    private func agentAttributionSourceLabel(_ source: String) -> String {
        switch source {
        case "ai_agent_session":
            return "AI session"
        case "command":
            return "command"
        case "process_tree":
            return "process tree"
        case "known_agent_directory":
            return "local agent directory"
        default:
            return source.replacingOccurrences(of: "_", with: " ")
        }
    }

    private func agentConfidenceTone(_ confidence: String) -> Color {
        switch confidence {
        case "high":
            return AetowerDesign.Status.ready
        case "medium":
            return AetowerDesign.Status.warning
        default:
            return .secondary
        }
    }

    private func icon(for item: StorageHygieneItemModel) -> String {
        switch item.kind {
        case "log-file", "logs": return "doc.text"
        case "cold-file": return "snowflake"
        case "large-file": return "doc.badge.exclamationmark"
        case "release-artifact": return "archivebox"
        case "macos-app-bundle": return "app"
        case "app-support-data", "app-container": return "shippingbox"
        case "app-preferences": return "slider.horizontal.3"
        case "app-receipt": return "doc.text.magnifyingglass"
        case "app-launch-item": return "powerplug"
        case "ios-backup": return "iphone"
        case "mail-attachments", "message-attachments": return "paperclip"
        case "local-snapshot": return "clock.arrow.circlepath"
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

    private func accessSummary(for item: StorageHygieneItemModel) -> String {
        var parts: [String] = []
        if let accessAgeDays = item.accessAgeDays {
            parts.append(accessAgeDays == 0 ? "accessed today" : "last accessed \(accessAgeDays)d ago")
        } else if let accessedMillis = item.accessedMillis {
            let date = Date(timeIntervalSince1970: Double(accessedMillis) / 1000.0)
            parts.append("accessed \(date.formatted(date: .abbreviated, time: .omitted))")
        } else {
            parts.append("access time unavailable")
        }
        if let ageDays = item.ageDays {
            parts.append(ageDays == 0 ? "modified today" : "modified \(ageDays)d ago")
        }
        return parts.joined(separator: " · ")
    }

    private func formatBytes(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
    }

    private func formatPercent(_ value: Double) -> String {
        storageFormatPercent(value)
    }

    private func lastPathComponent(_ path: String) -> String {
        URL(fileURLWithPath: path).lastPathComponent
    }

    private func reveal(path: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
    }

    private func openPath(path: String) {
        let expanded = NSString(string: path).expandingTildeInPath
        NSWorkspace.shared.open(URL(fileURLWithPath: expanded))
    }

    private func comparePaths(_ paths: [String]) {
        guard paths.count >= 2 else { return }
        let left = NSString(string: paths[0]).expandingTildeInPath
        let right = NSString(string: paths[1]).expandingTildeInPath
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/opendiff")
        process.arguments = [left, right]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            copy(diffCommand(for: [left, right]))
        }
    }

    private func shellQuotedPath(_ path: String) -> String {
        "'\(path.replacingOccurrences(of: "'", with: "'\\''"))'"
    }

    private func quickLook(path: String) {
        let expanded = NSString(string: path).expandingTildeInPath
        let url = URL(fileURLWithPath: expanded)
        guard FileManager.default.fileExists(atPath: url.path) else {
            reveal(path: expanded)
            return
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/qlmanage")
        process.arguments = ["-p", url.path]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try? process.run()
    }

    private func copy(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }
}

private struct StorageSimilarityImageThumbnail: View {
    let item: StorageDuplicateItemModel

    private var image: NSImage? {
        let expanded = NSString(string: item.path).expandingTildeInPath
        return NSImage(contentsOfFile: expanded)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ZStack {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(AetowerDesign.Surface.rowIdle)
                if let image {
                    Image(nsImage: image)
                        .resizable()
                        .scaledToFill()
                        .frame(maxWidth: .infinity, minHeight: 76, maxHeight: 76)
                        .clipped()
                } else {
                    Image(systemName: "photo")
                        .font(.title3)
                        .foregroundStyle(AetowerDesign.Status.neutral)
                }
            }
            .frame(height: 76)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))

            Text(item.displayName)
                .font(.system(size: 10, weight: .semibold))
                .lineLimit(1)
            Text(ByteCountFormatter.string(fromByteCount: Int64(item.sizeBytes), countStyle: .file))
                .font(.system(size: 9))
                .foregroundStyle(.secondary)
        }
        .padding(6)
        .frame(maxWidth: .infinity, minHeight: 118, alignment: .topLeading)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

private enum StorageSimilarityFilter: String, CaseIterable, Identifiable {
    case exactDuplicates
    case similarImages
    case similarDocumentsText
    case similarVideos
    case similarBinaries
    case otherRedundancy

    var id: String { rawValue }

    var label: String {
        switch self {
        case .exactDuplicates:
            return "Exact duplicates"
        case .similarImages:
            return "Images"
        case .similarDocumentsText:
            return "Docs/Text"
        case .similarVideos:
            return "Videos"
        case .similarBinaries:
            return "Binaries"
        case .otherRedundancy:
            return "Other"
        }
    }

    var emptyStateLabel: String {
        switch self {
        case .exactDuplicates:
            return "exact duplicate groups"
        case .similarImages:
            return "similar image groups"
        case .similarDocumentsText:
            return "similar document or text groups"
        case .similarVideos:
            return "similar video groups"
        case .similarBinaries:
            return "similar binary groups"
        case .otherRedundancy:
            return "other redundancy groups"
        }
    }
}

private struct StorageSimilarityReviewSummary {
    let duplicateGroupCount: Int
    let otherRedundancyGroupCount: Int
    let exactGroupCount: Int
    let fuzzyGroupCount: Int
    let exactBytes: UInt64
    let otherRedundancyBytes: UInt64
    let reviewableBytes: UInt64

    var groupCount: Int {
        duplicateGroupCount + otherRedundancyGroupCount
    }
}

private enum StorageSection: String, CaseIterable, Identifiable {
    case reclaim
    case explore
    case insights

    var id: String { rawValue }

    var label: String {
        switch self {
        case .reclaim: return "Reclaim"
        case .explore: return "Explore"
        case .insights: return "Insights"
        }
    }

    var role: String {
        switch self {
        case .reclaim: return "Free up space"
        case .explore: return "Find what's big"
        case .insights: return "Trends & coverage"
        }
    }

    var summary: String {
        switch self {
        case .reclaim:
            return "Disk pressure, safe cleanup opportunities, and the staged basket."
        case .explore:
            return "Treemap, similar files review, and the full item list."
        case .insights:
            return "Growth over time, volume coverage, budgets, and raw diagnostics."
        }
    }

    var systemImage: String {
        switch self {
        case .reclaim: return "sparkles"
        case .explore: return "square.grid.3x3.topleft.filled"
        case .insights: return "chart.line.uptrend.xyaxis"
        }
    }
}

private enum StorageExplorePane: String, CaseIterable, Identifiable {
    case browse
    case optimize
    case similar
    case raw

    var id: String { rawValue }

    var label: String {
        switch self {
        case .browse: return "Browse"
        case .optimize: return "Optimize"
        case .similar: return "Similar"
        case .raw: return "Raw"
        }
    }

    var detail: String {
        switch self {
        case .browse:
            return "Open the indexed table first, then opt into heavier full-disk and treemap views."
        case .optimize:
            return "Large-file, cold-data, app-footprint, System Data, and investigation leads for the current scan."
        case .similar:
            return "Exact duplicates, fuzzy media/document matches, and redundancy groups with review controls."
        case .raw:
            return "The raw artifact list stays collapsed until opened; use it when you need every retained candidate."
        }
    }
}

private struct StorageTreemapLayout: Identifiable {
    let node: StorageTreemapNodeModel
    let rect: CGRect

    var id: String { node.id }
}

private struct StorageCubeNodeBin: Identifiable {
    let node: StorageTreemapNodeModel
    let bin: StorageCubeProjectionBin

    var id: String { node.id }
    var cubeCount: Int { bin.cubeCount }
    var startIndex: Int { bin.startIndex }
    var endIndex: Int { bin.endIndex }

    func contains(_ index: Int) -> Bool {
        index >= startIndex && index < endIndex
    }
}

private struct StorageCubeGridLayout {
    let columns: Int
    let rows: Int
    let cubeSize: CGFloat
    let gap: CGFloat
    let origin: CGPoint
    let cubeCount: Int
}

private enum StorageVisualExplorerMode: String, CaseIterable, Identifiable {
    case fullDisk
    case treemap
    case table

    var id: String { rawValue }

    var label: String {
        switch self {
        case .fullDisk: return "Full disk"
        case .treemap: return "Treemap"
        case .table: return "Table"
        }
    }
}

private enum StorageScanModeSelection: String, CaseIterable, Identifiable {
    case fast = "fast_changed_only"
    case deep = "deep_native"
    case forensic = "forensic_verified"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .fast: return "Fast"
        case .deep: return "Deep"
        case .forensic: return "Forensic"
        }
    }

    var resultLimit: UInt32 {
        switch self {
        case .fast: return 120
        case .deep: return 160
        case .forensic: return 200
        }
    }
}

private enum StorageArtifactScope: String, CaseIterable, Identifiable {
    case all
    case large
    case cold
    case stale
    case repoLinked
    case agentLinked
    case partial

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: return "All scopes"
        case .large: return "Large"
        case .cold: return "Cold"
        case .stale: return "Stale"
        case .repoLinked: return "Repo-linked"
        case .agentLinked: return "Agent-linked"
        case .partial: return "Partial sizes"
        }
    }

    func matches(_ item: StorageHygieneItemModel) -> Bool {
        switch self {
        case .all:
            return true
        case .large:
            return item.sizeBytes >= 100 * 1024 * 1024
        case .cold:
            return item.cold
        case .stale:
            return item.stale
        case .repoLinked:
            return item.attribution.repoRoot != nil
        case .agentLinked:
            return item.attribution.aiAgentSession != nil
        case .partial:
            return item.sizeTruncated
        }
    }
}

private enum StorageColdDataSort: String, CaseIterable, Identifiable {
    case recommended
    case largest

    var id: String { rawValue }

    var label: String {
        switch self {
        case .recommended: return "Recommended (score)"
        case .largest: return "Largest"
        }
    }

    func sorted(_ items: [StorageHygieneItemModel]) -> [StorageHygieneItemModel] {
        items.sorted { left, right in
            switch self {
            case .recommended:
                return left.recommendationScore == right.recommendationScore
                    ? left.path < right.path
                    : left.recommendationScore > right.recommendationScore
            case .largest:
                return left.sizeBytes == right.sizeBytes
                    ? left.path < right.path
                    : left.sizeBytes > right.sizeBytes
            }
        }
    }
}

private enum StorageArtifactSort: String, CaseIterable, Identifiable {
    case largest
    case recommended
    case smallest
    case newest
    case oldest
    case path
    case tier

    var id: String { rawValue }

    var label: String {
        switch self {
        case .largest: return "Largest first"
        case .recommended: return "Recommended (score)"
        case .smallest: return "Smallest first"
        case .newest: return "Newest first"
        case .oldest: return "Oldest first"
        case .path: return "Path"
        case .tier: return "Cleanup tier"
        }
    }

    func areInIncreasingOrder(
        _ left: StorageHygieneItemModel,
        _ right: StorageHygieneItemModel
    ) -> Bool {
        switch self {
        case .recommended:
            return left.recommendationScore == right.recommendationScore
                ? left.path < right.path
                : left.recommendationScore > right.recommendationScore
        case .largest:
            return left.sizeBytes == right.sizeBytes
                ? left.path < right.path
                : left.sizeBytes > right.sizeBytes
        case .smallest:
            return left.sizeBytes == right.sizeBytes
                ? left.path < right.path
                : left.sizeBytes < right.sizeBytes
        case .newest:
            return (left.modifiedMillis ?? 0) == (right.modifiedMillis ?? 0)
                ? left.path < right.path
                : (left.modifiedMillis ?? 0) > (right.modifiedMillis ?? 0)
        case .oldest:
            return (left.modifiedMillis ?? UInt64.max) == (right.modifiedMillis ?? UInt64.max)
                ? left.path < right.path
                : (left.modifiedMillis ?? UInt64.max) < (right.modifiedMillis ?? UInt64.max)
        case .path:
            return left.path.localizedStandardCompare(right.path) == .orderedAscending
        case .tier:
            let leftKey = "\(left.cleanupTier)|\(left.safety)|\(left.path)"
            let rightKey = "\(right.cleanupTier)|\(right.safety)|\(right.path)"
            return leftKey < rightKey
        }
    }
}

private enum StorageFilter: String, CaseIterable, Identifiable {
    case attention
    case safe
    case rebuildable
    case expensive
    case risky
    case all

    var id: String { rawValue }

    var label: String {
        switch self {
        case .attention: return "Attention"
        case .safe: return "Safe"
        case .rebuildable: return "Rebuildable"
        case .expensive: return "Expensive"
        case .risky: return "Risky"
        case .all: return "All"
        }
    }

    func matches(_ item: StorageHygieneItemModel) -> Bool {
        switch self {
        case .attention:
            item.cleanupTier == "risky"
                || item.cleanupTier == "expensive"
                || item.safety != "safe"
                || item.sizeTruncated
                || item.cold
                || item.sizeBytes >= 100 * 1024 * 1024
        case .safe:
            item.cleanupTier == "safe"
        case .rebuildable:
            item.cleanupTier == "rebuildable"
        case .expensive:
            item.cleanupTier == "expensive"
        case .risky:
            item.cleanupTier == "risky"
        case .all:
            true
        }
    }
}
