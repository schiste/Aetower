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
}

private struct StorageTopOffender: Identifiable {
    let id: String
    let title: String
    let value: String
    let detail: String
    let systemImage: String
    let tone: Color
}

public struct StorageView: View {
    let state: AppState
    @State private var selectedFilter: StorageFilter = .attention
    @State private var artifactScope: StorageArtifactScope = .all
    @State private var artifactSort: StorageArtifactSort = .largest
    @State private var searchText = ""
    @State private var customRoot = ""
    @State private var maxDepth = 5.0
    @State private var copiedCleanupBundleID: String?
    @State private var copiedCleanupRecipeID: String?
    @State private var candidateCommandPreviewBundle: StorageCleanupBundleModel?
    @State private var selectedMode: StorageMode = .overview
    @State private var showCleanupRecipes = false
    @State private var showRawArtifacts = false
    @State private var showScannedRoots = false
    @State private var showCaveats = false

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        VStack(spacing: AetowerDesign.Spacing.none) {
            storageTabToolBand
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                    safetyBanner

                    if let error = state.storageHygieneError {
                        warningBanner(error)
                    }

                    if let report = state.storageHygieneReport {
                        if selectedMode == .overview {
                            storageOverview(report)
                        } else {
                            storageAdvanced(report)
                        }
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
        .task {
            state.ensureStorageHygieneScan()
        }
        .sheet(item: $candidateCommandPreviewBundle) { bundle in
            cleanupCommandPreviewSheet(bundle)
        }
    }

    private var storageTabToolBand: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search storage artifacts and paths",
            searchWidth: 300
        ) {
            Picker("", selection: $selectedMode) {
                ForEach(StorageMode.allCases) { mode in
                    Text(mode.label).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityLabel("Storage view")
            .frame(width: 220)
        } filterTools: {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                if selectedMode == .advanced {
                    Picker("", selection: $selectedFilter) {
                        ForEach(StorageFilter.allCases) { filter in
                            Text(filter.label).tag(filter)
                        }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .accessibilityLabel("Storage filter")
                    .frame(width: 430)
                }
                TextField("Optional root, for example ~/Repositories", text: $customRoot)
                    .aetowerUtilityTextInput()
                    .textFieldStyle(.plain)
                    .font(AetowerDesign.Typography.caption)
                    .padding(.horizontal, AetowerDesign.Spacing.sm)
                    .padding(.vertical, AetowerDesign.Spacing.xs)
                    .frame(width: 230)
                    .aetowerControlChrome()
                Stepper(
                    "Depth \(Int(maxDepth))",
                    value: $maxDepth,
                    in: 1...12,
                    step: 1
                )
                .font(AetowerDesign.Typography.caption)
                .frame(width: 118)
            }
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
                AetowerToolBadge(
                    "Scan",
                    value: storageScanStatusLabel,
                    systemImage: state.storageHygieneIsLoading ? "arrow.triangle.2.circlepath" : "shield.checkered",
                    tone: state.storageHygieneError == nil ? AetowerDesign.Status.ready : AetowerDesign.Status.error
                )
            }
        } actions: {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button {
                    runScan()
                } label: {
                    Label("Scan", systemImage: "arrow.clockwise")
                }
                .buttonStyle(.borderedProminent)
                .disabled(state.storageHygieneIsLoading)
            }
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

    private var safetyBanner: some View {
        AetowerInfoBanner(
            "Aetower does not delete files from this view. It estimates size, age, and cleanup confidence, then gives reveal/copy actions so operators stay in control.",
            title: "Read-only inventory",
            systemImage: "shield.checkered",
            tone: AetowerDesign.Status.ready,
            level: .card
        )
    }

    private func storageOverview(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            storageActionPanel(report)
            reclaimSpaceSection(report)
            topOffenderCallout(report)
            budgetGuardrailsSection(report)
            if shouldShowAgentHygieneOverview(report) {
                agentHygieneSection(report)
            }
            if report.truncated {
                warningBanner("The scan hit a cap or time budget. Results are partial; use Advanced to inspect raw artifacts or narrow the root.")
            }
        }
    }

    private func storageAdvanced(_ report: StorageHygieneReportModel) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
            summaryGrid(report)
            cleanupPreviewSection(report)
            cleanupBundlesSection(report)
            cleanupRecipesSection(report)
            repoFootprintDashboard(report)
            storageGrowthTimeline(report)
            if report.truncated {
                warningBanner("The scan hit a cap or time budget. Results are partial; narrow the root or refresh when the machine is idle.")
            }
            itemSection(report)
            rootsSection(report)
            caveatsSection(report)
        }
    }

    private func storageActionPanel(_ report: StorageHygieneReportModel) -> some View {
        let primaryBundle = report.cleanupBundles.first
        let hasCandidateCommands = primaryBundle?.manifest.contains(where: { $0.cleanupCommand != nil }) ?? false

        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: primaryBundle.map { cleanupBundleIcon($0) } ?? "externaldrive")
                    .foregroundStyle(primaryBundle.map { cleanupBundleTone($0) } ?? AetowerDesign.Tone.disk)
                    .frame(width: 24)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text(primaryBundle?.title ?? "No cleanup plan yet")
                        .font(.title3.weight(.semibold))
                    Text(primaryBundle?.subtitle ?? "Run or narrow a scan to build a read-only cleanup plan. Aetower copies instructions only; it does not delete files.")
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
                    Button {
                        copy(cleanupBundleManifest(primaryBundle))
                        copiedCleanupBundleID = primaryBundle.id
                    } label: {
                        Label("Copy cleanup plan", systemImage: "doc.on.doc")
                    }
                    .buttonStyle(.borderedProminent)

                    Button("Copy verify commands") {
                        copy(primaryBundle.dryRunCommands.joined(separator: "\n"))
                        copiedCleanupBundleID = primaryBundle.id
                    }
                    .disabled(primaryBundle.dryRunCommands.isEmpty)

                    if hasCandidateCommands {
                        Button("Review candidate commands") {
                            candidateCommandPreviewBundle = primaryBundle
                        }
                    }

                    Spacer()

                    Text(copiedCleanupBundleID == primaryBundle.id ? "Copied" : "No files changed")
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
                    Text("Concrete cleanup actions for the largest high-confidence local artifacts. Aetower copies commands and reveals targets; it does not run deletion commands.")
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
                        selectedMode = .advanced
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
                        copy(recipes.map(\.command).joined(separator: "\n"))
                        copiedCleanupRecipeID = "overview-visible"
                    } label: {
                        Label("Copy visible commands", systemImage: "doc.on.doc")
                    }
                    .buttonStyle(.borderedProminent)

                    Button("Show all actions") {
                        selectedMode = .advanced
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
                Button("Reveal") {
                    reveal(path: recipe.affectedPath)
                }
                Spacer()
                Text(copiedCleanupRecipeID == recipe.id ? "Copied" : "No files changed")
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
        }
        .padding(AetowerDesign.Spacing.md)
        .background(AetowerDesign.Surface.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
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
                        }
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
                Text("One-click planning bundles with a full manifest, confidence score, verification commands, and rollback notes. Aetower copies plans only; it does not delete files.")
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
                Button {
                    copy(cleanupBundleManifest(bundle))
                    copiedCleanupBundleID = bundle.id
                } label: {
                    Label("Copy cleanup plan", systemImage: "doc.on.doc")
                }
                Button("Copy verify commands") {
                    copy(bundle.dryRunCommands.joined(separator: "\n"))
                    copiedCleanupBundleID = bundle.id
                }
                .disabled(bundle.dryRunCommands.isEmpty)
                if bundle.manifest.contains(where: { $0.cleanupCommand != nil }) {
                    Button("Copy candidate commands") {
                        candidateCommandPreviewBundle = bundle
                    }
                }
                Spacer()
                Text(copiedCleanupBundleID == bundle.id ? "Copied" : "no files changed")
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
                    Text("Aetower will only copy these commands. Files are not changed unless you paste and run them in a shell.")
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
                        Text("Commands to copy")
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
                            }
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
                Button("Copy candidate commands") {
                    copy(cleanupBundleCleanupCommands(bundle))
                    copiedCleanupBundleID = bundle.id
                    candidateCommandPreviewBundle = nil
                }
                .buttonStyle(.borderedProminent)
                .disabled(cleanupBundleCleanupCommandList(bundle).isEmpty)
            }
        }
        .padding(AetowerDesign.Spacing.xl)
        .frame(width: 720, height: 560, alignment: .topLeading)
    }

    private func cleanupRecipesSection(_ report: StorageHygieneReportModel) -> some View {
        DisclosureGroup(isExpanded: $showCleanupRecipes) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                Text("Exact commands for common cleanup tasks. Aetower does not run these; copy and execute only after reviewing the prerequisites.")
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
                Button("Reveal target") { reveal(path: recipe.affectedPath) }
                Spacer()
                Text(recipe.destructive ? "destructive command" : "read-only command")
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
                    Label(attributionSummary(for: item), systemImage: "point.3.connected.trianglepath.dotted")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(2)
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
        DisclosureGroup(isExpanded: $showScannedRoots) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(report.roots, id: \.self) { root in
                    rootLine(root, detail: "scanned", systemImage: "checkmark.circle")
                }
                ForEach(report.skippedRoots) { root in
                    rootLine(root.path, detail: root.reason, systemImage: "exclamationmark.triangle")
                }
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        } label: {
            advancedSectionLabel(
                title: "Scanned and skipped roots",
                detail: "\(report.roots.count) scanned · \(report.skippedRoots.count) skipped",
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
                        || item.reason.lowercased().contains(query)
                        || item.cleanupTier.lowercased().contains(query)
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
            limit: 120
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
                aiAgentSession: item.attribution.aiAgentSession
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
            parts.append("writer unknown: Aetower needs a file-event journal to tie this jump to a command/process/session")
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
        for command in bundle.manifest.compactMap(\.cleanupCommand) where seen.insert(command).inserted {
            commands.append(command)
        }
        return commands
    }

    private func budgetGuardrailSummary(_ guardrails: StorageBudgetGuardrailsModel) -> String {
        if guardrails.violations.isEmpty {
            return "Warn when a repo exceeds \(formatBytes(guardrails.repoArtifactBudgetBytes)), grows more than \(formatBytes(guardrails.repoGrowthBudgetBytesPerDay)) per day, or total local dev artifacts exceed \(formatBytes(guardrails.totalArtifactBudgetBytes))."
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

    private func copy(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }
}

private enum StorageMode: String, CaseIterable, Identifiable {
    case overview
    case advanced

    var id: String { rawValue }

    var label: String {
        switch self {
        case .overview: return "Overview"
        case .advanced: return "Advanced"
        }
    }

    var helperText: String {
        switch self {
        case .overview:
            return "Overview keeps the recommended cleanup action, current pressure, and guardrails visible."
        case .advanced:
            return "Advanced shows detailed tiers, cleanup bundles, recipes, repo growth, raw artifacts, roots, and caveats."
        }
    }
}

private enum StorageArtifactScope: String, CaseIterable, Identifiable {
    case all
    case large
    case stale
    case repoLinked
    case agentLinked
    case partial

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: return "All scopes"
        case .large: return "Large"
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
