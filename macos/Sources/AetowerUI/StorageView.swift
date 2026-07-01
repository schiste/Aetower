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

    var hasStageableItems: Bool {
        !stageItems.isEmpty
    }
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

private struct StorageCleanupExecutionRequest: Identifiable, Sendable {
    let id = UUID()
    let title: String
    let subtitle: String
    let operation: StorageCleanupOperation
    let command: String
    let targetPaths: [String]
    let estimatedBytes: UInt64
    let destructive: Bool
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
    let command: String
    let requiresReview: Bool
    let blockers: [String]
    let prerequisites: [String]
}

private enum StorageCleanupOperation: String, Sendable {
    case moveToTrash
    case shellCommand
}

private struct StorageCleanupExecutionResult: Sendable {
    let exitCode: Int32
    let output: String
    let durationSeconds: Double

    var succeeded: Bool { exitCode == 0 }
}

private struct StorageCleanupAuditEvent: Codable, Identifiable, Sendable {
    let id: String
    let timestampMillis: UInt64
    let action: String
    let path: String
    let detail: String
    let bytes: UInt64
    let succeeded: Bool?
}

private enum StorageCleanupAuditLog {
    private static let fileName = "storage-cleanup-audit.ndjson"

    static func append(_ event: StorageCleanupAuditEvent) {
        guard let url = auditURL(createDirectory: true),
            let data = try? JSONEncoder().encode(event)
        else { return }
        var line = data
        line.append(0x0A)
        if FileManager.default.fileExists(atPath: url.path),
            let handle = try? FileHandle(forWritingTo: url)
        {
            do {
                try handle.seekToEnd()
                try handle.write(contentsOf: line)
                try handle.close()
            } catch {
                try? handle.close()
            }
        } else {
            try? line.write(to: url, options: [.atomic])
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
            .compactMap { line in
                try? decoder.decode(StorageCleanupAuditEvent.self, from: Data(line.utf8))
            }
    }

    private static func auditURL(createDirectory: Bool) -> URL? {
        storageSupportFileURL(fileName: fileName, createDirectory: createDirectory)
    }
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
    @State private var selectedSection: StorageSection = .actions
    @State private var showCleanupRecipes = false
    @State private var showRawArtifacts = false
    @State private var showScannedRoots = false
    @State private var showCaveats = false
    @State private var pendingCleanupExecutionRequest: StorageCleanupExecutionRequest?
    @State private var cleanupExecutionResult: StorageCleanupExecutionResult?
    @State private var cleanupExecutionIsRunning = false
    @State private var cleanupBasket: [StorageCleanupBasketItem] = []
    @State private var showCleanupBasket = false
    @State private var cleanupAuditEvents = StorageCleanupAuditLog.loadRecent()
    @State private var classificationExplanation: StorageClassificationExplanation?
    @State private var storageVisualExplorerMode: StorageVisualExplorerMode = .treemap
    @State private var showStorageTreemap = false
    @State private var selectedTreemapNodeID: String?
    @State private var storageExplorerPage = 0

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
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                        storageScanOptionsCard

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
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerToolBadge(
                    "Reclaim",
                    value: storageReclaimableLabel,
                    systemImage: "externaldrive.badge.minus",
                    tone: AetowerDesign.Tone.disk
                )
                AetowerToolBadge(
                    "Items",
                    value: storageItemCountLabel,
                    systemImage: "shippingbox",
                    tone: AetowerDesign.Tone.memory
                )
            }
        } actions: {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                storageFilterMenu
                AetowerScanButton(isRunning: state.storageHygieneIsLoading) {
                    runScan()
                }
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

    private var storageFilterMenu: some View {
        Menu {
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
        } label: {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: "line.3.horizontal.decrease.circle")
                Text(selectedFilter.label)
                Image(systemName: "chevron.down")
                    .font(AetowerDesign.Typography.compactData(size: 8, weight: .semibold))
            }
            .font(AetowerDesign.Typography.caption.weight(.semibold))
            .foregroundStyle(AetowerDesign.Ink.secondary)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .aetowerControlChrome()
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    private var storageScanOptionsCard: some View {
        AetowerSurface(level: .card, padding: AetowerDesign.Spacing.md) {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                Label("Scan options", systemImage: "slider.horizontal.3")
                    .font(AetowerDesign.Typography.controlLabel)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize()

                TextField("Optional root, for example ~/Repositories", text: $customRoot)
                    .aetowerUtilityTextInput()
                    .textFieldStyle(.plain)
                    .font(AetowerDesign.Typography.caption)
                    .padding(.horizontal, AetowerDesign.Spacing.sm)
                    .padding(.vertical, AetowerDesign.Spacing.xs)
                    .frame(minWidth: 220, idealWidth: 320, maxWidth: 420)
                    .aetowerControlChrome()

                Stepper(
                    "Depth \(Int(maxDepth))",
                    value: $maxDepth,
                    in: 1...12,
                    step: 1
                )
                .font(AetowerDesign.Typography.caption)
                .fixedSize()

                Picker("Mode", selection: $scanMode) {
                    ForEach(StorageScanModeSelection.allCases) { mode in
                        Text(mode.label).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 220)

                Spacer(minLength: AetowerDesign.Spacing.sm)

                if state.storageHygieneIsLoading {
                    Text(storageScanLoadingTitle)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
        }
    }

    private func storageNavigationRail(report: StorageHygieneReportModel?) -> some View {
        AetowerNavigationRail(width: 264) {
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
            return state.storageHygieneIsLoading ? "Scanning" : "No scan"
        }
        switch section {
        case .actions:
            return formatBytes(report.summary.totalReclaimableBytes)
        case .reclaim:
            let actions = report.cleanupBundles.count + report.cleanupRecipes.count
            return "\(actions) action\(actions == 1 ? "" : "s")"
        case .growth:
            let growthCount = report.growthDeltas.filter { $0.deltaBytes > 0 }.count
            return "\(growthCount) growth signal\(growthCount == 1 ? "" : "s")"
        case .explorer:
            return "\(report.summary.itemCount) item\(report.summary.itemCount == 1 ? "" : "s")"
        case .inventory:
            return "\(report.summary.itemCount) item\(report.summary.itemCount == 1 ? "" : "s")"
        case .sources:
            return "\(report.volumeStates.count) volume\(report.volumeStates.count == 1 ? "" : "s")"
        case .policies:
            let violations = report.budgetGuardrails.violations.count
            return violations == 0 ? "No violations" : "\(violations) violation\(violations == 1 ? "" : "s")"
        case .advanced:
            return report.truncated ? "Partial scan" : "\(report.scanDurationMillis) ms"
        }
    }

    private func storageSectionTone(
        _ section: StorageSection,
        report: StorageHygieneReportModel?
    ) -> Color {
        guard let report else {
            return state.storageHygieneIsLoading ? AetowerDesign.Tone.disk : AetowerDesign.Status.neutral
        }
        switch section {
        case .actions:
            return report.summary.totalReclaimableBytes > 0 ? AetowerDesign.Tone.disk : AetowerDesign.Status.ready
        case .reclaim:
            return report.cleanupBundles.isEmpty && report.cleanupRecipes.isEmpty ? AetowerDesign.Status.neutral : AetowerDesign.Status.ready
        case .growth:
            return report.growthDeltas.contains { $0.deltaBytes > 0 } ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
        case .explorer, .inventory:
            return AetowerDesign.Tone.memory
        case .sources:
            return report.skippedRoots.isEmpty ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
        case .policies:
            return report.budgetGuardrails.violations.isEmpty ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
        case .advanced:
            return report.truncated ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
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
        case .actions:
            storageActionHome(report)
        case .reclaim:
            storageReclaimWorkspace(report)
        case .growth:
            storageGrowthWorkspace(report)
        case .explorer:
            storageExplorerWorkspace(report)
        case .inventory:
            storageInventoryWorkspace(report)
        case .sources:
            storageSourcesWorkspace(report)
        case .policies:
            storagePoliciesWorkspace(report)
        case .advanced:
            storageAdvanced(report)
        }
    }

    private func storageActionHome(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            storageHomeActionsSection(report)
            storageActionPanel(report)
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
            cleanupRecipesSection(report)
            cleanupAuditSection
        }
    }

    private func storageGrowthWorkspace(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            topOffenderCallout(report)
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
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Storage actions")
                        .font(.title3.weight(.semibold))
                    Text("Six decisions first: what is safe, what is rebuildable, what grew, what is old, and what must be reviewed.")
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
                columns: [GridItem(.adaptive(minimum: 300), spacing: AetowerDesign.Spacing.md)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.md
            ) {
                ForEach(actions) { action in
                    storageHomeActionCard(action)
                }
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func storageHomeActionCard(_ action: StorageHomeAction) -> some View {
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
                        .font(.system(size: 18, weight: .semibold, design: .rounded))
                    Text("\(action.confidence)% confidence")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 105), spacing: AetowerDesign.Spacing.xs)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.xs
            ) {
                footprintMetric("Items", value: "\(action.itemCount)", detail: "matched")
                footprintMetric("Impact", value: action.bytes == 0 ? "None" : "Visible", detail: "ranked")
            }

            Text(action.consequence)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            storageHomeActionSamples(action)

            HStack(spacing: AetowerDesign.Spacing.sm) {
                if action.hasStageableItems {
                    Button {
                        stageStorageHomeAction(action)
                    } label: {
                        Label("Stage cleanup", systemImage: "tray.and.arrow.down")
                    }
                    .buttonStyle(.borderedProminent)
                } else {
                    Button {
                        copy(storageHomeActionPlan(action))
                    } label: {
                        Label("Copy cleanup plan", systemImage: "doc.on.doc")
                    }
                    .buttonStyle(.borderedProminent)
                }

                Button("Explain") {
                    classificationExplanation = explanation(for: action)
                }
                Spacer()
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, minHeight: 270, alignment: .topLeading)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
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
            }
            Spacer(minLength: AetowerDesign.Spacing.xs)
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
            Button("Quick Look") { quickLook(path: event.path) }
            Button("Reveal") { reveal(path: event.path) }
            Button("Explain") { classificationExplanation = explanation(for: event) }
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
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
            evidence: actionEvidence(action),
            blockers: action.sampleItems.flatMap(\.cleanupBlockers).prefix(8).map { $0 }
        )
    }

    private func explanation(for item: StorageHygieneItemModel) -> StorageClassificationExplanation {
        var evidence = item.evidence.isEmpty ? [item.reason, item.recommendation] : item.evidence
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
                item.reason,
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
            evidence: recipe.prerequisites.isEmpty ? [recipe.command] : recipe.prerequisites,
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
            consequence: item.requiresReview
                ? "This staged target needs manual review before moving it to Trash."
                : "This staged target can be moved to Finder Trash after basket review.",
            evidence: item.prerequisites.isEmpty
                ? ["Estimated reclaim: \(formatBytes(item.estimatedBytes))."]
                : item.prerequisites,
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
        let allItems = report.items.sorted { left, right in
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

        return [
            StorageHomeAction(
                id: "safe-reclaim",
                title: "Safe Reclaim",
                detail: "High-confidence logs, caches, and rebuildable artifacts.",
                consequence: "Moves eligible local artifacts to Finder Trash. Reversal is normally possible from Trash or by rebuilding.",
                systemImage: "checkmark.shield",
                tone: AetowerDesign.Status.ready,
                bytes: sumItemBytes(safe),
                itemCount: safe.count,
                confidence: safe.isEmpty ? 0 : 94,
                sampleItems: Array(safe.prefix(4)),
                stageItems: safe,
                growthEvents: []
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
                growthEvents: []
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
                growthEvents: []
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
                growthEvents: recentlyGrew
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
                growthEvents: []
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
                growthEvents: []
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
            summaryGrid(report)
            cleanupPreviewSection(report)
            cleanupBundlesSection(report)
            cleanupRecipesSection(report)
            cleanupAuditSection
            wholeComputerOptimizationSection(report)
            visualExplorationSection(report)
            repoFootprintDashboard(report)
            storageGrowthTimeline(report)
            volumeStateSection(report)
            if report.truncated {
                warningBanner("The scan hit a cap or time budget. Results are partial; narrow the root or refresh when the machine is idle.")
            }
            itemSection(report)
            rootsSection(report)
            caveatsSection(report)
        }
    }

    private func wholeComputerOptimizationSection(_ report: StorageHygieneReportModel) -> some View {
        let largeFiles = report.items
            .filter { $0.kind == "large-file" || $0.kind == "release-artifact" }
            .sorted(by: storageItemSizeSort)
        let oldUnused = report.items
            .filter(isOldUnusedStorageItem)
            .sorted(by: storageItemSizeSort)
        let systemBytes = report.systemDataBuckets.reduce(UInt64(0)) { total, bucket in
            sumBytes(total, bucket.sizeBytes)
        }

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: "sparkles.square.filled.on.square")
                    .foregroundStyle(AetowerDesign.Tone.disk)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Whole-computer optimization")
                        .font(.headline)
                    Text("Large files, cold data, duplicates, app footprints, and macOS System Data buckets from the current bounded scan.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                AetowerBadge(
                    "\(largeFiles.count + oldUnused.count + report.duplicateGroups.count + report.appFootprints.count) leads",
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
                footprintMetric("Duplicates", value: "\(report.duplicateGroups.count)", detail: formatBytes(report.duplicateGroups.reduce(UInt64(0)) { sumBytes($0, $1.reclaimableBytes) }))
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
                duplicateGroupsCard(report.duplicateGroups)
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

    private func duplicateGroupsCard(_ groups: [StorageDuplicateGroupModel]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text("Duplicate finder")
                .font(.subheadline.weight(.semibold))
            Text("Cheap size/type grouping first; full content hashing only for candidate files within the scan hash budget.")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if groups.isEmpty {
                Label("No duplicate candidates in the retained scan set.", systemImage: "checkmark.circle")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            } else {
                ForEach(groups.prefix(3)) { group in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(group.confirmed ? "Confirmed duplicates" : "Potential duplicates")
                                .font(.caption.weight(.semibold))
                            Spacer()
                            Text(formatBytes(group.reclaimableBytes))
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(group.confirmed ? AetowerDesign.Status.ready : AetowerDesign.Status.warning)
                        }
                        Text("\(group.fileCount) file\(group.fileCount == 1 ? "" : "s") · \(group.confidenceScore)% confidence")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        ForEach(group.paths.prefix(2)) { item in
                            HStack(spacing: AetowerDesign.Spacing.xs) {
                                Text(item.displayName)
                                    .font(.caption2.weight(.semibold))
                                    .lineLimit(1)
                                Spacer()
                                Button("Quick Look") { quickLook(path: item.path) }
                                Button("Reveal") { reveal(path: item.path) }
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

    private func appFootprintsCard(_ footprints: [StorageAppFootprintModel]) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text("App footprints")
                .font(.subheadline.weight(.semibold))
            Text("Uninstall view: app bundle, support data, caches, containers, and launch items when visible to the scan.")
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
                Button("Stage") { _ = stageCleanupItem(item) }
            }
            Button("Quick Look") { quickLook(path: item.path) }
            Button("Reveal") { reveal(path: item.path) }
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
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
                    Text(primaryBundle?.subtitle ?? "Run or narrow a scan to build a cleanup plan with Trash actions, reveal targets, verification commands, and advanced permanent commands.")
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
                    detail: "\(report.summary.scannedDirectoryCount) folders"
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

                        Button("Review permanent commands") {
                            candidateCommandPreviewBundle = primaryBundle
                        }
                    } else {
                        Button {
                            copy(cleanupBundleManifest(primaryBundle))
                            copiedCleanupBundleID = primaryBundle.id
                        } label: {
                            Label("Copy cleanup plan", systemImage: "doc.on.doc")
                        }
                        .buttonStyle(.borderedProminent)
                    }

                    Button("Copy plan") {
                        copy(cleanupBundleManifest(primaryBundle))
                        copiedCleanupBundleID = primaryBundle.id
                    }

                    Button("Copy verify commands") {
                        copy(primaryBundle.dryRunCommands.joined(separator: "\n"))
                        copiedCleanupBundleID = primaryBundle.id
                    }
                    .disabled(primaryBundle.dryRunCommands.isEmpty)

                    if !cleanupBasket.isEmpty {
                        Button("Review basket") {
                            showCleanupBasket = true
                        }
                    }

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
                    selectedSection = .advanced
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
                        selectedSection = .advanced
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

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 145), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                footprintMetric(
                    "Manifest",
                    value: "\(bundle.manifest.count)",
                    detail: "full returned list"
                )
                footprintMetric(
                    "Verify",
                    value: "\(bundle.dryRunCommands.count)",
                    detail: "dry-run command\(bundle.dryRunCommands.count == 1 ? "" : "s")"
                )
                footprintMetric(
                    "Rollback",
                    value: "\(bundle.rollbackNotes.count)",
                    detail: "note\(bundle.rollbackNotes.count == 1 ? "" : "s")"
                )
            }

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
                Button {
                    copy(cleanupBundleManifest(bundle))
                    copiedCleanupBundleID = bundle.id
                } label: {
                    Label("Copy plan", systemImage: "doc.on.doc")
                }
                Button("Copy verify commands") {
                    copy(bundle.dryRunCommands.joined(separator: "\n"))
                    copiedCleanupBundleID = bundle.id
                }
                .disabled(bundle.dryRunCommands.isEmpty)
                if cleanupBundleHasActionableCommands(bundle) {
                    Button("Review permanent commands") {
                        candidateCommandPreviewBundle = bundle
                    }
                }
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
                    Text("Review candidate cleanup commands")
                        .font(.title3.weight(.semibold))
                    Text("Review the exact permanent commands, prerequisites, and manifest before bypassing Trash.")
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
                        Text("Permanent commands")
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

                Button("Run permanent commands") {
                    candidateCommandPreviewBundle = nil
                    presentCleanupExecution(bundleShellExecutionRequest(bundle))
                }
                .disabled(cleanupBundleCleanupCommandList(bundle).isEmpty)

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
                Image(systemName: request.operation == .moveToTrash ? "trash" : "exclamationmark.triangle")
                    .foregroundStyle(request.operation == .moveToTrash ? AetowerDesign.Status.warning : AetowerDesign.Status.error)
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
                            value: request.operation == .moveToTrash ? "Trash" : "Permanent",
                            detail: cleanupExecutionIsRunning ? "running" : "waiting"
                        )
                        if let result = cleanupExecutionResult {
                            footprintMetric(
                                "Exit",
                                value: "\(result.exitCode)",
                                detail: result.succeeded ? "success" : "failed"
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
                        Text(request.operation == .moveToTrash ? "Targets" : "Command")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                        Text(request.operation == .moveToTrash ? targetPathList(request.targetPaths) : request.command)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .padding(AetowerDesign.Spacing.sm)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    }

                    if let result = cleanupExecutionResult {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                            Text(result.succeeded ? "Result" : "Result: needs attention")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(result.succeeded ? AetowerDesign.Status.ready : AetowerDesign.Status.error)
                            Text(result.output.isEmpty ? "Command completed with no output." : result.output)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                                .padding(AetowerDesign.Spacing.sm)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                            Text(String(format: "%.1fs", result.durationSeconds))
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
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

                Button(request.operation == .moveToTrash ? "Copy targets" : "Copy command") {
                    copy(request.operation == .moveToTrash ? request.targetPaths.joined(separator: "\n") : request.command)
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
                Button("Permanent command") { presentCleanupExecution(recipeShellExecutionRequest(recipe)) }
                Button("Quick Look") { quickLook(path: recipe.affectedPath) }
                Button("Reveal target") { reveal(path: recipe.affectedPath) }
                Button("Explain") { classificationExplanation = explanation(for: recipe) }
                Spacer()
                Text(recipe.destructive ? "cleanup command" : "verification command")
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

    private func visualExplorationSection(_ report: StorageHygieneReportModel) -> some View {
        let visibleItems = filteredItems(from: report)
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                Label("Visual exploration", systemImage: "square.grid.3x3.topleft.filled")
                    .font(.headline)
                Spacer()
                Picker("", selection: $storageVisualExplorerMode) {
                    ForEach(StorageVisualExplorerMode.allCases) { mode in
                        Text(mode.label).tag(mode)
                    }
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .frame(width: 210)
            }

            Text("Lazy spatial discovery over Rust projections. The map is loaded only on demand; the list stays paged and sorted from the current bounded projection.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if storageVisualExplorerMode == .treemap {
                storageTreemapExplorer(report)
            } else {
                storageExplorerTable(visibleItems)
            }
        }
        .padding(AetowerDesign.Spacing.lg)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
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
                    AetowerBadge("\(nodes.count) block\(nodes.count == 1 ? "" : "s")", tone: AetowerDesign.Tone.disk)
                    AetowerBadge(formatBytes(totalBytes), tone: AetowerDesign.Tone.memory)
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

    private func storageExplorerTable(_ visibleItems: [StorageHygieneItemModel]) -> some View {
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
                .frame(width: 105, alignment: .center)
            Text("Kind")
                .frame(width: 120, alignment: .center)
            Text("Size")
                .frame(width: 95, alignment: .trailing)
            Text("Actions")
                .frame(width: 180, alignment: .trailing)
        }
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.secondary)
        .padding(.horizontal, AetowerDesign.Spacing.sm)
    }

    private func storageExplorerTableRow(_ item: StorageHygieneItemModel) -> some View {
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
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            cleanupTierBadge(item)
                .frame(width: 105)
            Text(item.kind)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 120)
            Text(formatBytes(item.sizeBytes))
                .font(.caption.weight(.semibold))
                .frame(width: 95, alignment: .trailing)
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Button("Look") { quickLook(path: item.path) }
                Button("Reveal") { reveal(path: item.path) }
                Button("Stage") { stageCleanupItem(item) }
                    .disabled(!storageItemIsTrashActionable(item))
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
            .frame(width: 180, alignment: .trailing)
        }
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xs)
        .background(AetowerDesign.Surface.rowIdle, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
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
        let visibleItems = filteredItems(from: report)
        return DisclosureGroup(isExpanded: $showRawArtifacts) {
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

                    Text("\(visibleItems.count) visible of \(report.items.count)")
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
                        ForEach(visibleItems) { item in
                            artifactRow(item)
                        }
                    }
                }
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        } label: {
            advancedSectionLabel(
                title: "Raw artifacts",
                detail: "\(visibleItems.count) visible of \(report.items.count) candidate\(report.items.count == 1 ? "" : "s")",
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
                Button("Quick Look") { quickLook(path: item.path) }
                Button("Reveal") { reveal(path: item.path) }
                Button("Explain") { classificationExplanation = explanation(for: item) }
                Button("Copy path") { copy(item.path) }
                Button("Copy command") { copy(item.commandHint) }
                Button("Stage") { stageCleanupItem(item) }
                    .disabled(!storageItemIsTrashActionable(item))
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
                AetowerBadge(source.permissionState.replacingOccurrences(of: "_", with: " "), tone: sourceTone(source))
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
        return report.items.filter { item in
            selectedFilter.matches(item)
                && artifactScope.matches(item)
                && (
                    query.isEmpty
                        || item.displayName.lowercased().contains(query)
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
                )
        }
        .sorted(by: artifactSort.areInIncreasingOrder)
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
        } else if let item = report.items.max(by: { $0.sizeBytes < $1.sizeBytes }) {
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
        return report.items.compactMap { item in
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
        if let command = event.command {
            parts.append("command \(command)")
        }
        if let processTree = event.processTree {
            parts.append("process tree \(processTree)")
        }
        if let session = event.aiAgentSession {
            parts.append("AI session \(session)")
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
        switch tier {
        case "safe":
            return AetowerDesign.Status.ready
        case "rebuildable":
            return AetowerDesign.Tone.disk
        case "expensive":
            return AetowerDesign.Status.warning
        case "risky":
            return AetowerDesign.Status.error
        default:
            return .secondary
        }
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
        switch tier {
        case "safe":
            return "checkmark.shield"
        case "rebuildable":
            return "hammer"
        case "expensive":
            return "clock.badge.exclamationmark"
        case "risky":
            return "exclamationmark.triangle"
        default:
            return "folder"
        }
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
        lines.append(contentsOf: ["", "## Candidate cleanup commands"])
        if cleanupCommands.isEmpty {
            lines.append("- No cleanup commands were generated for this bundle.")
        } else {
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
                command: item.cleanupCommand ?? "",
                requiresReview: bundle.safety != "safe" || bundle.confidenceScore < 90,
                blockers: item.cleanupBlockers,
                prerequisites: bundle.prerequisites + bundle.caveats
            )
            if stageBasketItem(basketItem) {
                staged += 1
            }
        }
        if staged > 0 {
            showCleanupBasket = true
        }
    }

    private func stageCleanupRecipes(_ recipes: [StorageCleanupRecipeModel]) {
        var staged = 0
        for recipe in recipes where stageCleanupRecipe(recipe, showBasket: false) {
            staged += 1
        }
        if staged > 0 {
            showCleanupBasket = true
        }
    }

    private func stageStorageHomeAction(_ action: StorageHomeAction) {
        var staged = 0
        for item in uniqueStorageItems(action.stageItems).prefix(80) {
            if stageCleanupItem(item, showBasket: false) {
                staged += 1
            }
        }
        if staged > 0 {
            showCleanupBasket = true
        } else {
            copy(storageHomeActionPlan(action))
        }
    }

    private func stagePreventionSuggestion(
        _ suggestion: StoragePreventionSuggestionModel,
        report: StorageHygieneReportModel
    ) {
        let candidates: [StorageHygieneItemModel]
        switch suggestion.trigger {
        case "safe-reclaim":
            candidates = report.items.filter {
                $0.cleanupTier == "safe" && storageItemIsTrashActionable($0)
            }
        case "post-build":
            let repoRoot = suggestion.id.replacingOccurrences(of: "post-build-cleanup|", with: "")
            candidates = report.items.filter {
                $0.cleanupTier == "rebuildable"
                    && $0.attribution.repoRoot == repoRoot
                    && storageItemIsTrashActionable($0)
            }
        case "artifact-budget":
            candidates = report.items.filter {
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
        if staged > 0 {
            showCleanupBasket = true
        } else {
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

        if !action.growthEvents.isEmpty {
            lines.append(contentsOf: ["", "## Growth evidence"])
            for event in action.growthEvents.prefix(12) {
                lines.append("- +\(formatBytes(UInt64(event.deltaBytes))) | \(event.path)")
                lines.append("  - \(storageGrowthCorrelationDetail(event))")
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

    @discardableResult
    private func stageCleanupItem(_ item: StorageHygieneItemModel, showBasket: Bool = true) -> Bool {
        guard storageItemIsTrashActionable(item) else {
            appendCleanupAudit(
                action: "policy-blocked",
                path: item.path,
                detail: item.cleanupBlockers.isEmpty ? "Item is not eligible for Trash cleanup." : item.cleanupBlockers.joined(separator: "; "),
                bytes: item.sizeBytes,
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
            command: "",
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
    private func stageCleanupRecipe(_ recipe: StorageCleanupRecipeModel, showBasket: Bool = true) -> Bool {
        let item = StorageCleanupBasketItem(
            id: "recipe|\(recipe.id)",
            title: recipe.title,
            path: recipe.affectedPath,
            source: recipe.category,
            cleanupTier: recipe.safety,
            safety: recipe.safety,
            estimatedBytes: recipe.estimatedReclaimableBytes,
            command: recipe.command,
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
        guard item.blockers.isEmpty else {
            appendCleanupAudit(
                action: "policy-blocked",
                path: item.path,
                detail: item.blockers.joined(separator: "; "),
                bytes: item.estimatedBytes,
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
            detail: "Staged \(item.title) for Finder Trash cleanup.",
            bytes: item.estimatedBytes,
            succeeded: true
        )
        return true
    }

    private func basketTrashExecutionRequest() -> StorageCleanupExecutionRequest {
        let prerequisites = uniqueStrings(cleanupBasket.flatMap(\.prerequisites))
        return StorageCleanupExecutionRequest(
            title: "Move cleanup basket to Trash",
            subtitle: "Move staged cleanup targets to Finder Trash. Nothing is permanently deleted by this action.",
            operation: .moveToTrash,
            command: uniqueStrings(cleanupBasket.map(\.command).filter { !$0.isEmpty }).joined(separator: "\n"),
            targetPaths: uniquePaths(cleanupBasket.map(\.path)),
            estimatedBytes: cleanupBasketTotalBytes(),
            destructive: true,
            requiresReview: cleanupBasket.contains(where: \.requiresReview),
            prerequisites: prerequisites
        )
    }

    private func recipeShellExecutionRequest(_ recipe: StorageCleanupRecipeModel) -> StorageCleanupExecutionRequest {
        return StorageCleanupExecutionRequest(
            title: "Run permanent command: \(recipe.title)",
            subtitle: recipe.reason,
            operation: .shellCommand,
            command: recipe.command,
            targetPaths: [recipe.affectedPath],
            estimatedBytes: recipe.estimatedReclaimableBytes,
            destructive: recipe.destructive,
            requiresReview: true,
            prerequisites: recipe.prerequisites
        )
    }

    private func bundleShellExecutionRequest(_ bundle: StorageCleanupBundleModel) -> StorageCleanupExecutionRequest {
        StorageCleanupExecutionRequest(
            title: "Run permanent cleanup commands",
            subtitle: bundle.subtitle,
            operation: .shellCommand,
            command: cleanupBundleCleanupCommands(bundle),
            targetPaths: uniquePaths(actionableManifestItems(bundle).map(\.path)),
            estimatedBytes: bundle.estimatedReclaimableBytes,
            destructive: true,
            requiresReview: true,
            prerequisites: bundle.prerequisites + bundle.caveats
        )
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
        switch request.operation {
        case .moveToTrash:
            return request.targetPaths.count == 1 ? "Move to Trash" : "Move \(request.targetPaths.count) to Trash"
        case .shellCommand:
            return "Run permanent command"
        }
    }

    private func cleanupExecutionCanRun(_ request: StorageCleanupExecutionRequest) -> Bool {
        switch request.operation {
        case .moveToTrash:
            return !request.targetPaths.isEmpty
        case .shellCommand:
            return !request.command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
    }

    private func presentCleanupExecution(_ request: StorageCleanupExecutionRequest) {
        cleanupExecutionResult = nil
        cleanupExecutionIsRunning = false
        pendingCleanupExecutionRequest = request
    }

    private func runCleanupExecution(_ request: StorageCleanupExecutionRequest) {
        cleanupExecutionIsRunning = true
        cleanupExecutionResult = nil
        let operation = request.operation
        let command = request.command
        let targetPaths = request.targetPaths
        Task.detached(priority: .utility) {
            let result: StorageCleanupExecutionResult = switch operation {
            case .moveToTrash:
                Self.movePathsToTrash(targetPaths)
            case .shellCommand:
                Self.runShellCommand(command)
            }
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
        let fallbackBytes = request.targetPaths.count == 1 ? request.estimatedBytes : 0
        let action: String
        if request.operation == .moveToTrash {
            action = result.succeeded ? "trash" : "failed-trash"
            if result.succeeded {
                let removed = Set(request.targetPaths)
                cleanupBasket.removeAll { removed.contains($0.path) }
            }
        } else {
            action = result.succeeded ? "override-permanent-command" : "failed-permanent-command"
        }

        for path in request.targetPaths {
            appendCleanupAudit(
                action: action,
                path: path,
                detail: result.output,
                bytes: bytesByPath[path] ?? fallbackBytes,
                succeeded: result.succeeded
            )
        }
    }

    private func appendCleanupAudit(
        action: String,
        path: String,
        detail: String,
        bytes: UInt64,
        succeeded: Bool?
    ) {
        let event = StorageCleanupAuditEvent(
            id: UUID().uuidString,
            timestampMillis: UInt64(Date().timeIntervalSince1970 * 1000),
            action: action,
            path: path,
            detail: detail,
            bytes: bytes,
            succeeded: succeeded
        )
        StorageCleanupAuditLog.append(event)
        cleanupAuditEvents = StorageCleanupAuditLog.loadRecent()
    }

    nonisolated private static func movePathsToTrash(_ paths: [String]) -> StorageCleanupExecutionResult {
        let started = Date()
        var moved = 0
        var output: [String] = []
        for path in paths {
            let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { continue }
            let url = URL(fileURLWithPath: trimmed)
            guard FileManager.default.fileExists(atPath: url.path) else {
                output.append("Missing: \(trimmed)")
                continue
            }
            do {
                var resultingURL: NSURL?
                try FileManager.default.trashItem(
                    at: url,
                    resultingItemURL: &resultingURL
                )
                moved += 1
                if let resultingURL {
                    output.append("Moved: \(trimmed) -> \(resultingURL.path ?? "Trash")")
                } else {
                    output.append("Moved: \(trimmed) -> Trash")
                }
            } catch {
                output.append("Failed: \(trimmed) - \(error.localizedDescription)")
            }
        }

        let failed = output.filter { $0.hasPrefix("Failed:") || $0.hasPrefix("Missing:") }.count
        let summary = "Moved \(moved) item\(moved == 1 ? "" : "s") to Trash; \(failed) issue\(failed == 1 ? "" : "s")."
        return StorageCleanupExecutionResult(
            exitCode: failed == 0 ? 0 : 1,
            output: ([summary] + output).joined(separator: "\n"),
            durationSeconds: Date().timeIntervalSince(started)
        )
    }

    nonisolated private static func runShellCommand(_ command: String) -> StorageCleanupExecutionResult {
        let started = Date()
        let process = Process()
        let outputPipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]
        process.standardOutput = outputPipe
        process.standardError = outputPipe

        do {
            try process.run()
        } catch {
            return StorageCleanupExecutionResult(
                exitCode: -1,
                output: "Failed to launch command: \(error.localizedDescription)",
                durationSeconds: Date().timeIntervalSince(started)
            )
        }

        let waitGroup = DispatchGroup()
        waitGroup.enter()
        DispatchQueue.global(qos: .utility).async {
            process.waitUntilExit()
            waitGroup.leave()
        }

        var timedOut = false
        if waitGroup.wait(timeout: .now() + 120) == .timedOut {
            timedOut = true
            process.terminate()
            _ = waitGroup.wait(timeout: .now() + 5)
        }

        let data = outputPipe.fileHandleForReading.readDataToEndOfFile()
        var output = String(data: data, encoding: .utf8) ?? ""
        if output.count > 12_000 {
            output = String(output.prefix(12_000)) + "\n... output truncated ..."
        }
        if timedOut {
            output = output.isEmpty
                ? "Command timed out after 120 seconds."
                : "\(output)\nCommand timed out after 120 seconds."
        }

        return StorageCleanupExecutionResult(
            exitCode: timedOut ? 124 : process.terminationStatus,
            output: output.trimmingCharacters(in: .whitespacesAndNewlines),
            durationSeconds: Date().timeIntervalSince(started)
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
        if value.rounded() == value {
            return "\(Int(value))%"
        }
        return String(format: "%.1f%%", value)
    }

    private func lastPathComponent(_ path: String) -> String {
        URL(fileURLWithPath: path).lastPathComponent
    }

    private func reveal(path: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
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

private enum StorageSection: String, CaseIterable, Identifiable {
    case actions
    case reclaim
    case growth
    case explorer
    case inventory
    case sources
    case policies
    case advanced

    var id: String { rawValue }

    var label: String {
        switch self {
        case .actions: return "Actions"
        case .reclaim: return "Reclaim"
        case .growth: return "Growth"
        case .explorer: return "Explorer"
        case .inventory: return "Inventory"
        case .sources: return "Sources"
        case .policies: return "Policies"
        case .advanced: return "Advanced"
        }
    }

    var role: String {
        switch self {
        case .actions: return "Start here"
        case .reclaim: return "Cleanup workflow"
        case .growth: return "What changed"
        case .explorer: return "Visual + list"
        case .inventory: return "Classified data"
        case .sources: return "Coverage"
        case .policies: return "Prevention"
        case .advanced: return "Raw detail"
        }
    }

    var summary: String {
        switch self {
        case .actions:
            return "Recommended cleanup moves and current pressure."
        case .reclaim:
            return "Bundles, recipes, staged basket, and audit trail."
        case .growth:
            return "Storage jumps, repo footprint, and top offender context."
        case .explorer:
            return "Lazy treemap, table view, and filtered artifacts."
        case .inventory:
            return "Large files, old data, duplicates, apps, system data, and agent artifacts."
        case .sources:
            return "Volumes, roots, permissions, skipped paths, and caveats."
        case .policies:
            return "Budgets, guardrails, warning-only policies, and cleanup safety."
        case .advanced:
            return "Complete diagnostic composition for power review."
        }
    }

    var systemImage: String {
        switch self {
        case .actions: return "bolt.circle"
        case .reclaim: return "trash.circle"
        case .growth: return "chart.line.uptrend.xyaxis"
        case .explorer: return "square.grid.3x3.topleft.filled"
        case .inventory: return "shippingbox"
        case .sources: return "externaldrive.connected.to.line.below"
        case .policies: return "shield.lefthalf.filled"
        case .advanced: return "slider.horizontal.3"
        }
    }
}

private enum StorageVisualExplorerMode: String, CaseIterable, Identifiable {
    case treemap
    case table

    var id: String { rawValue }

    var label: String {
        switch self {
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

private enum StorageArtifactSort: String, CaseIterable, Identifiable {
    case largest
    case smallest
    case newest
    case oldest
    case path
    case tier

    var id: String { rawValue }

    var label: String {
        switch self {
        case .largest: return "Largest first"
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
