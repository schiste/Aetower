import AppKit
import SwiftUI

private let defaultClaudeMdDelegationMaxBytes: UInt64 = 1_024

private enum RepositoryMode: String, CaseIterable, Identifiable {
    case overview
    case attention

    var id: String { rawValue }

    var label: String {
        switch self {
        case .overview: return "Overview"
        case .attention: return "Attention"
        }
    }
}

private enum RepositorySort: String, CaseIterable, Identifiable {
    case attention
    case size
    case growth
    case artifacts
    case name

    var id: String { rawValue }

    var label: String {
        switch self {
        case .attention: return "Attention"
        case .size: return "Size"
        case .growth: return "Growth"
        case .artifacts: return "Artifacts"
        case .name: return "Name"
        }
    }
}

private struct RepositorySummary: Identifiable {
    let id: String
    let root: String
    let name: String
    let gitBranch: String?
    let gitHead: String?
    let gitRef: String?
    let gitDetachedHead: Bool
    let gitRemoteOriginUrl: String?
    let gitRemoteKey: String?
    let gitRemoteHost: String?
    let gitRemoteOwner: String?
    let gitRemoteName: String?
    let gitDirtyStatus: String
    let gitDirtyFileCount: UInt64?
    let gitDirtyTruncated: Bool
    let cloneGroupCount: UInt64
    let cloneGroupRoots: [String]
    let discoveredRoot: String?
    let hasAgentsMd: Bool
    let hasClaudeMd: Bool
    let claudeMdBytes: UInt64?
    let claudeMdDelegationMaxBytes: UInt64
    let claudeMdDelegatesToAgentsMd: Bool
    let agentReadinessScore: UInt8
    let agentReadinessStatus: String
    let agentContractMissingCount: UInt64
    let agentContractCoverage: [StorageAgentContractCoverageModel]
    let agentGuidanceStatus: String
    let agentGuidanceIssueCount: UInt64
    let agentGuidanceIssues: [StorageAgentGuidanceIssueModel]
    let hasStorageFootprint: Bool
    let currentSizeBytes: UInt64
    let artifactBytes: UInt64
    let itemCount: Int
    let growthBytes: Int64?
    let growthWindow: String
    let estimatedRebuildCost: String
    let estimatedRebuildSeconds: UInt64?
    let lastWriterProcess: String?
    let lastWriterPid: UInt32?
    let lastBranchTouched: String?
    let topArtifactFolders: [StorageRepoArtifactFolderModel]
    let caveats: [String]
    let violationCount: Int
    let reviewItemCount: Int
    let safeItemCount: Int
    let staleItemCount: Int
    let liveSessionCount: Int
    let liveEntityCount: Int
    let liveMemoryBytes: UInt64
    let liveCPUPercent: Float
    let agentArtifactBytes: UInt64
    let agentCount: Int

    var attentionScore: Double {
        let artifactScore = min(Double(artifactBytes) / Double(512 * 1024 * 1024), 18)
        let growthScore = growthBytes.map { max(0, Double($0) / Double(256 * 1024 * 1024)) } ?? 0
        let reviewScore = Double(reviewItemCount) * 2.2
        let staleScore = Double(staleItemCount) * 0.7
        let violationScore = Double(violationCount) * 8
        let liveScore = Double(liveSessionCount + liveEntityCount) * 1.4
        let readinessPenalty = Double(100 - Int(agentReadinessScore)) / 8.0
        let qualityScore = Double(qualityIssueCount + Int(agentGuidanceIssueCount)) * 2.5 + readinessPenalty
        let duplicateScore = cloneGroupCount > 1 ? Double(cloneGroupCount) * 3 : 0
        let dirtyScore = gitDirtyStatus == "dirty" ? 2.0 : 0.0
        let storageScore = artifactScore + growthScore + reviewScore + staleScore
        let runtimeScore = violationScore + liveScore + qualityScore
        return storageScore + runtimeScore + duplicateScore + dirtyScore
    }

    var qualityIssueCount: Int {
        [
            !hasAgentsMd,
            !hasClaudeMd,
            hasClaudeMd && !claudeMdDelegatesToAgentsMd,
        ].filter { $0 }.count
    }

    var qualityStatusLabel: String {
        let count = max(UInt64(qualityIssueCount), agentGuidanceIssueCount)
        return count == 0 ? "Guidance ok" : "\(count) guidance gap\(count == 1 ? "" : "s")"
    }

    var qualityStatusTone: Color {
        if agentGuidanceStatus == "error" { return AetowerDesign.Status.error }
        if agentGuidanceStatus == "warning" || qualityIssueCount > 0 { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.ready
    }

    var statusLabel: String {
        if violationCount > 0 { return "Budget" }
        if agentReadinessStatus == "blocked" { return "Agent blocked" }
        if agentReadinessStatus == "weak" { return "Agent weak" }
        if agentGuidanceStatus == "error" { return "Guidance" }
        if cloneGroupCount > 1 { return "Cloned" }
        if (growthBytes ?? 0) > 0 { return "Growing" }
        if gitDirtyStatus == "dirty" { return "Dirty" }
        if reviewItemCount > 0 { return "Review" }
        if liveSessionCount > 0 || liveEntityCount > 0 { return "Active" }
        if !hasStorageFootprint { return "Indexed" }
        return "Stable"
    }

    var statusTone: Color {
        if violationCount > 0 { return AetowerDesign.Status.error }
        if agentReadinessStatus == "blocked" { return AetowerDesign.Status.error }
        if agentReadinessStatus == "weak" { return AetowerDesign.Status.warning }
        if agentGuidanceStatus == "error" { return AetowerDesign.Status.error }
        if cloneGroupCount > 1 { return AetowerDesign.Status.warning }
        if (growthBytes ?? 0) > 512 * 1024 * 1024 { return AetowerDesign.Status.warning }
        if gitDirtyStatus == "dirty" { return AetowerDesign.Tone.energy }
        if reviewItemCount > 0 { return AetowerDesign.Status.warning }
        if liveSessionCount > 0 || liveEntityCount > 0 { return AetowerDesign.Status.ready }
        return AetowerDesign.Status.neutral
    }
}

public struct RepositoryView: View {
    let state: AppState
    @State private var mode: RepositoryMode = .overview
    @State private var sort: RepositorySort = .attention
    @State private var searchText = ""
    @State private var repositoryPath: [String] = []
    @State private var copiedRepositoryID: String?
    @State private var copiedAgentPromptKey: String?
    @State private var selectedAgentContractByRepository: [String: String] = [:]

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        NavigationStack(path: $repositoryPath) {
            VStack(spacing: AetowerDesign.Spacing.none) {
                repositoryToolBand
                Divider()
                content
            }
            .navigationDestination(for: String.self) { repositoryID in
                repositoryDestination(repositoryID)
            }
        }
        .task {
            state.ensureStorageHygieneScan()
        }
    }

    private var repositoryToolBand: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search repositories, branches, writers",
            searchWidth: 320
        ) {
            Picker("", selection: $mode) {
                ForEach(RepositoryMode.allCases) { mode in
                    Text(mode.label).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityLabel("Repository view")
            .frame(width: 300)
        } filterTools: {
            Picker("", selection: $sort) {
                ForEach(RepositorySort.allCases) { sort in
                    Text(sort.label).tag(sort)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityLabel("Repository sort")
            .frame(width: 360)
        } badges: {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerToolBadge(
                    "Repos",
                    value: repositoryCountLabel,
                    systemImage: "folder",
                    tone: repositoryBadgeTone
                )
                AetowerToolBadge(
                    "Artifacts",
                    value: artifactBytesLabel,
                    systemImage: "shippingbox",
                    tone: AetowerDesign.Tone.disk
                )
                AetowerToolBadge(
                    "Attention",
                    value: attentionCountLabel,
                    systemImage: "exclamationmark.triangle",
                    tone: attentionBadgeTone
                )
            }
        } actions: {
            Button {
                state.runStorageHygieneScan()
            } label: {
                Label("Scan", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.borderedProminent)
            .disabled(state.storageHygieneIsLoading)
        }
    }

    @ViewBuilder
    private var content: some View {
        if let error = state.storageHygieneError {
            ScrollView {
                AetowerInfoBanner(
                    error,
                    title: "Repository scan failed",
                    systemImage: "exclamationmark.triangle",
                    tone: AetowerDesign.Status.error,
                    level: .critical
                )
                .padding(AetowerDesign.Spacing.xxl)
            }
        } else if state.storageHygieneReport == nil, state.storageHygieneIsLoading {
            loadingState
        } else if let report = state.storageHygieneReport {
            repositoryDashboard(report)
        } else {
            emptyState
        }
    }

    private var loadingState: some View {
        VStack(spacing: AetowerDesign.Spacing.sm) {
            ProgressView()
            Text("Scanning repositories and storage footprints")
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var emptyState: some View {
        AetowerEmptyState(
            title: "No repositories indexed yet",
            detail: "Run a read-only scan to build the repository inventory from local Git roots.",
            systemImage: "folder.badge.gearshape",
            tone: AetowerDesign.Tone.disk
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func repositoryDashboard(_ report: StorageHygieneReportModel) -> some View {
        let repositories = filteredRepositories(from: report)

        return ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                repositoryOverview(report, repositories: repositories)

                if repositories.isEmpty {
                    AetowerEmptyState(
                        title: "No repository matches",
                        detail: "Clear search or change the mode to see more repositories.",
                        systemImage: "magnifyingglass",
                        tone: AetowerDesign.Status.neutral
                    )
                } else {
                    repositoryOverviewGrid(repositories)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
    }

    private func repositoryOverview(
        _ report: StorageHygieneReportModel,
        repositories: [RepositorySummary]
    ) -> some View {
        let allRepositories = repositorySummaries(from: report)
        let activeCount = allRepositories.filter { $0.liveSessionCount > 0 || $0.liveEntityCount > 0 }.count
        let growingCount = allRepositories.filter { ($0.growthBytes ?? 0) > 0 }.count
        let duplicateCloneCount = allRepositories.filter { $0.cloneGroupCount > 1 }.count
        let dirtyCount = allRepositories.filter { $0.gitDirtyStatus == "dirty" }.count
        let agentReadyCount = allRepositories.filter { $0.agentReadinessStatus == "ready" }.count
        let reviewCount = allRepositories.filter {
            $0.reviewItemCount > 0
                || $0.violationCount > 0
                || $0.qualityIssueCount > 0
                || $0.agentGuidanceIssueCount > 0
                || $0.agentReadinessStatus == "blocked"
                || $0.agentReadinessStatus == "weak"
                || $0.cloneGroupCount > 1
                || $0.gitDirtyStatus == "dirty"
        }.count

        return LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 180), spacing: AetowerDesign.Spacing.md)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.md
        ) {
            AetowerMetricTile(
                "Indexed",
                value: "\(allRepositories.count)",
                detail: "\(repositories.count) visible · \(report.repoFootprints.count) with artifacts",
                systemImage: "folder",
                tone: AetowerDesign.Tone.cpu
            )
            AetowerMetricTile(
                "Artifact weight",
                value: formatBytes(totalArtifactBytes(allRepositories)),
                detail: "build/log/cache footprint",
                systemImage: "shippingbox",
                tone: AetowerDesign.Tone.disk
            )
            AetowerMetricTile(
                "Attention",
                value: "\(reviewCount)",
                detail: "budget, readiness, or review signals",
                systemImage: "exclamationmark.triangle",
                tone: reviewCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
            AetowerMetricTile(
                "Agent-ready",
                value: "\(agentReadyCount)",
                detail: "repos with >=90% contract coverage",
                systemImage: "checkmark.seal",
                tone: agentReadyCount == allRepositories.count ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
            )
            AetowerMetricTile(
                "Active",
                value: "\(activeCount)",
                detail: "live sessions or attributed processes",
                systemImage: "dot.radiowaves.left.and.right",
                tone: activeCount > 0 ? AetowerDesign.Status.ready : AetowerDesign.Status.neutral
            )
            AetowerMetricTile(
                "Growing",
                value: "\(growingCount)",
                detail: report.summary.attributedRepoCount > 0 ? "baseline-backed deltas" : "waiting for baseline",
                systemImage: "chart.line.uptrend.xyaxis",
                tone: growingCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
            AetowerMetricTile(
                "Clone groups",
                value: "\(duplicateCloneCount)",
                detail: "repos sharing a remote",
                systemImage: "square.stack.3d.up",
                tone: duplicateCloneCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
            AetowerMetricTile(
                "Dirty trees",
                value: "\(dirtyCount)",
                detail: "uncommitted work detected",
                systemImage: "pencil.and.scribble",
                tone: dirtyCount > 0 ? AetowerDesign.Tone.energy : AetowerDesign.Status.ready
            )
        }
    }

    private func repositoryOverviewGrid(_ repositories: [RepositorySummary]) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 280), spacing: AetowerDesign.Spacing.md)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.md
        ) {
            ForEach(repositories) { repository in
                repositoryOverviewCard(repository)
            }
        }
    }

    private func repositoryOverviewCard(_ repository: RepositorySummary) -> some View {
        NavigationLink(value: repository.id) {
            AetowerSurface(level: .card, padding: AetowerDesign.Spacing.md) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                        Text(repository.name)
                            .font(AetowerDesign.Typography.controlLabel)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                            .lineLimit(1)
                        Spacer(minLength: AetowerDesign.Spacing.sm)
                        AetowerBadge(repository.statusLabel, tone: repository.statusTone)
                    }
                    Text(shortPath(repository.root))
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                    repositoryOverviewStats(repository)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button("Reveal in Finder") {
                reveal(repository.root)
            }
            Button("Copy path") {
                copy(repository.root)
            }
            Button("Copy optimization brief") {
                copy(optimizationBrief(for: repository))
                copiedRepositoryID = repository.id
            }
        }
    }

    private func repositoryOverviewStats(_ repository: RepositorySummary) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 96), spacing: AetowerDesign.Spacing.xs)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.xs
        ) {
            repositoryMiniStat(
                "Artifacts",
                value: repository.hasStorageFootprint ? formatBytes(repository.artifactBytes) : "Clean",
                tone: repository.hasStorageFootprint ? AetowerDesign.Tone.disk : AetowerDesign.Status.neutral
            )
            repositoryMiniStat("Growth", value: growthLabel(repository), tone: growthTone(repository))
            repositoryMiniStat("Git", value: gitOverviewLabel(repository), tone: gitOverviewTone(repository))
            repositoryMiniStat(
                "Live",
                value: "\(repository.liveSessionCount + repository.liveEntityCount)",
                tone: repository.liveSessionCount + repository.liveEntityCount > 0
                    ? AetowerDesign.Status.ready
                    : AetowerDesign.Status.neutral
            )
            repositoryMiniStat("Agents", value: agentReadinessLabel(repository), tone: agentReadinessTone(repository))
        }
    }

    private func repositoryMiniStat(_ label: String, value: String, tone: Color) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            Text(label.uppercased())
                .font(AetowerDesign.Typography.metadata)
                .foregroundStyle(AetowerDesign.Ink.tertiary)
            AetowerBadge(value, tone: tone)
        }
    }

    @ViewBuilder
    private func repositoryDestination(_ repositoryID: String) -> some View {
        if let report = state.storageHygieneReport,
           let repository = repositorySummaries(from: report).first(where: { $0.id == repositoryID }) {
            repositoryDetailPage(repository, report: report)
        } else {
            AetowerEmptyState(
                title: "Repository no longer available",
                detail: "Run a fresh scan to rebuild the repository inventory.",
                systemImage: "folder.badge.questionmark",
                tone: AetowerDesign.Status.neutral
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .navigationTitle("Repository")
        }
    }

    private func repositoryDetailPage(
        _ repository: RepositorySummary,
        report: StorageHygieneReportModel
    ) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                repositoryHero(repository)
                repositorySignals(repository)
                repositoryAgentGuidance(repository)
                repositoryGitIntelligence(repository)
                repositoryActions(repository)
                topArtifacts(repository)
                liveContext(repository)
                futureOptimizationLanes(repository, report: report)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
        .navigationTitle(repository.name)
    }

    private func repositoryHero(_ repository: RepositorySummary) -> some View {
        AetowerSurface(padding: AetowerDesign.Spacing.lg) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.lg) {
                Image(systemName: "folder.badge.gearshape")
                    .font(AetowerDesign.Typography.metricValue(size: 30, weight: .semibold))
                    .foregroundStyle(repository.statusTone)
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                        Text(repository.name)
                            .font(AetowerDesign.Typography.metricValue(size: 26, weight: .semibold))
                            .foregroundStyle(AetowerDesign.Ink.primary)
                            .lineLimit(1)
                            .minimumScaleFactor(0.8)
                        AetowerBadge(repository.statusLabel, tone: repository.statusTone)
                    }
                    Text(repository.root)
                        .font(AetowerDesign.Typography.compactData(size: 11))
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(2)
                        .textSelection(.enabled)
                    Text(repositorySummarySentence(repository))
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.lg)
                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(repository.currentSizeBytes))
                        .font(AetowerDesign.Typography.metricValue(size: 28, weight: .semibold))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text(repository.hasStorageFootprint ? "current footprint" : "no tracked artifacts")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }
            }
        }
    }

    private func repositorySignals(_ repository: RepositorySummary) -> some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 160), spacing: AetowerDesign.Spacing.md)],
            alignment: .leading,
            spacing: AetowerDesign.Spacing.md
        ) {
            AetowerMetricTile(
                "Artifacts",
                value: formatBytes(repository.artifactBytes),
                detail: "\(repository.itemCount) tracked item\(repository.itemCount == 1 ? "" : "s")",
                systemImage: "shippingbox",
                tone: AetowerDesign.Tone.disk
            )
            AetowerMetricTile(
                "Growth",
                value: growthLabel(repository),
                detail: repository.growthWindow,
                systemImage: "chart.line.uptrend.xyaxis",
                tone: growthTone(repository)
            )
            AetowerMetricTile(
                "Rebuild cost",
                value: repository.estimatedRebuildCost,
                detail: rebuildTimeLabel(repository.estimatedRebuildSeconds),
                systemImage: "hammer",
                tone: AetowerDesign.Tone.energy
            )
            AetowerMetricTile(
                "Live load",
                value: String(format: "%.1f%%", repository.liveCPUPercent),
                detail: "\(formatBytes(repository.liveMemoryBytes)) memory",
                systemImage: "cpu",
                tone: repository.liveCPUPercent > 15 ? AetowerDesign.Status.warning : AetowerDesign.Tone.cpu
            )
            AetowerMetricTile(
                "Agent cost",
                value: formatBytes(repository.agentArtifactBytes),
                detail: "\(repository.agentCount) agent source\(repository.agentCount == 1 ? "" : "s")",
                systemImage: "person.crop.circle.badge.gearshape",
                tone: repository.agentArtifactBytes > 0 ? AetowerDesign.Tone.memory : AetowerDesign.Status.neutral
            )
            AetowerMetricTile(
                "Contract readiness",
                value: agentReadinessLabel(repository),
                detail: agentReadinessDetail(repository),
                systemImage: "checkmark.seal",
                tone: agentReadinessTone(repository)
            )
            AetowerMetricTile(
                "Git state",
                value: gitOverviewLabel(repository),
                detail: gitDetail(repository),
                systemImage: "arrow.triangle.branch",
                tone: gitOverviewTone(repository)
            )
            AetowerMetricTile(
                "Clone group",
                value: cloneGroupLabel(repository),
                detail: repository.gitRemoteKey ?? "remote unavailable",
                systemImage: "square.stack.3d.up",
                tone: repository.cloneGroupCount > 1 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
            )
        }
    }

    private func repositoryAgentGuidance(_ repository: RepositorySummary) -> some View {
        AetowerSection("Operating contracts", subtitle: "Boot, routing, invariants, execution, and safety") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                AetowerInfoBanner(
                    agentGuidanceSummary(repository),
                    title: agentGuidanceTitle(repository),
                    systemImage: "checklist.checked",
                    tone: agentReadinessTone(repository),
                    level: repository.agentReadinessStatus == "blocked" ? .critical : .card
                )
                agentContractReadinessPanel(repository)
            }
        }
    }

    private func agentContractReadinessPanel(_ repository: RepositorySummary) -> some View {
        AetowerSurface(level: .card, padding: AetowerDesign.Spacing.none) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.none) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    Text("EXPECTED FILES")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                    if repository.agentContractCoverage.isEmpty {
                        Text("No contract coverage was reported.")
                            .font(AetowerDesign.Typography.caption)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                    } else {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                            ForEach(repository.agentContractCoverage) { contract in
                                Button {
                                    selectedAgentContractByRepository[repository.id] = contract.id
                                } label: {
                                    agentContractListRow(
                                        contract,
                                        selected: selectedAgentContract(repository)?.id == contract.id
                                    )
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .frame(width: 280, alignment: .topLeading)
                .padding(AetowerDesign.Spacing.md)

                Rectangle()
                    .fill(AetowerDesign.Surface.divider)
                    .frame(width: AetowerDesign.Stroke.hairline)

                Group {
                    if let selected = selectedAgentContract(repository) {
                        agentContractDetailPane(repository, contract: selected)
                    } else {
                        AetowerEmptyState(
                            title: "No contract selected",
                            detail: "Aetower did not receive contract coverage for this repository.",
                            systemImage: "doc.badge.questionmark",
                            tone: AetowerDesign.Status.neutral
                        )
                    }
                }
                .frame(maxWidth: .infinity, alignment: .topLeading)
                .padding(AetowerDesign.Spacing.md)
            }
        }
    }

    private func agentContractListRow(
        _ contract: StorageAgentContractCoverageModel,
        selected: Bool
    ) -> some View {
        AetowerSurface(
            level: selected ? .selected : contractSurfaceLevel(contract),
            padding: AetowerDesign.Spacing.sm
        ) {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                agentContractIconImage(contract)
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text(contract.label)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                    Text(contract.path)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: AetowerDesign.Spacing.xs)
                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xxs) {
                    AetowerBadge(agentContractStatusLabel(contract), tone: agentContractTone(contract))
                    Text("\(contract.coveragePercent)%")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }
            }
        }
    }

    private func agentContractDetailPane(
        _ repository: RepositorySummary,
        contract: StorageAgentContractCoverageModel
    ) -> some View {
        let issues = agentContractIssues(repository, contract: contract)
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                agentContractIconImage(contract)
                Text(contract.label)
                    .font(AetowerDesign.Typography.sectionTitle)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                AetowerBadge(agentContractStatusLabel(contract), tone: agentContractTone(contract))
                AetowerBadge("\(contract.coveragePercent)%", tone: agentContractTone(contract))
                Spacer(minLength: AetowerDesign.Spacing.md)
            }

            Text(contract.path)
                .font(AetowerDesign.Typography.compactData(size: 11))
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .textSelection(.enabled)

            Text(contract.detail)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .fixedSize(horizontal: false, vertical: true)

            agentContractPromptTools(repository, contract: contract)

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.sm)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.sm
            ) {
                contractFact("Kind", contract.kind)
                contractFact("Weight", "\(contract.earnedWeight)/\(contract.weight)")
                contractFact("Present", contract.present ? "Yes" : "No")
                contractFact("Tracked", contract.tracked ? "Yes" : "No")
                contractFact("Schema", contract.schemaVersion ?? "Missing")
                contractFact("Generated", contract.generated ? "Yes" : "No")
                contractFact("Reviewed", contract.reviewed ? "Yes" : "No")
            }

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                Text("ISSUES FOR THIS FILE")
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                if issues.isEmpty {
                    AetowerInfoBanner(
                        "No issue is currently attached to \(contract.path).",
                        systemImage: "checkmark.circle",
                        tone: AetowerDesign.Status.ready,
                        level: .quiet
                    )
                } else {
                    LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        ForEach(issues) { issue in
                            agentGuidanceIssueRow(issue)
                        }
                    }
                }
            }
        }
    }

    private func agentContractPromptTools(
        _ repository: RepositorySummary,
        contract: StorageAgentContractCoverageModel
    ) -> some View {
        let promptContext = agentContractPromptContext(repository)
        let issues = agentContractIssues(repository, contract: contract)
        let generationKey = AgentContractPrompts.key(repositoryID: repository.id, contract: contract, kind: "generate")
        let reconcileKey = AgentContractPrompts.key(repositoryID: repository.id, contract: contract, kind: "reconcile")
        let copied = copiedAgentPromptKey == generationKey || copiedAgentPromptKey == reconcileKey
        return AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                    Image(systemName: "text.badge.plus")
                        .foregroundStyle(AetowerDesign.Status.neutral)
                    Text("Focused prompts")
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    AetowerBadge("Prompt guide", tone: AetowerDesign.Status.neutral)
                    AetowerBadge(
                        AgentContractPrompts.schemaPath(for: contract) == "Not required" ? "No schema" : "Schema",
                        tone: AetowerDesign.Status.neutral
                    )
                    Spacer(minLength: AetowerDesign.Spacing.sm)
                    AetowerBadge(
                        copied ? "Copied" : "Ready",
                        tone: copied ? AetowerDesign.Status.ready : AetowerDesign.Status.neutral
                    )
                }
                Text("Copy a one-file prompt when you want an agent to generate or repair this contract without touching unrelated files.")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text("Guide: \(AgentContractPrompts.guide(for: contract)) · Schema: \(AgentContractPrompts.schemaPath(for: contract))")
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Button("Copy generation prompt") {
                        copy(AgentContractPrompts.generationPrompt(
                            repository: promptContext,
                            contract: contract,
                            issues: issues
                        ))
                        copiedAgentPromptKey = generationKey
                    }
                    Button("Copy reconcile prompt") {
                        copy(AgentContractPrompts.reconcilePrompt(
                            repository: promptContext,
                            contract: contract,
                            issues: issues
                        ))
                        copiedAgentPromptKey = reconcileKey
                    }
                    Spacer(minLength: AetowerDesign.Spacing.sm)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
        }
    }

    private func contractFact(_ label: String, _ value: String) -> some View {
        AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text(label.uppercased())
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                Text(value)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            }
        }
    }

    private func selectedAgentContract(_ repository: RepositorySummary) -> StorageAgentContractCoverageModel? {
        if let selectedID = selectedAgentContractByRepository[repository.id],
           let selected = repository.agentContractCoverage.first(where: { $0.id == selectedID }) {
            return selected
        }
        return repository.agentContractCoverage.first(where: { $0.severity == "error" })
            ?? repository.agentContractCoverage.first(where: { $0.severity == "warning" })
            ?? repository.agentContractCoverage.first
    }

    private func agentContractPromptContext(_ repository: RepositorySummary) -> AgentContractPromptContext {
        AgentContractPromptContext(
            name: repository.name,
            root: repository.root,
            branch: repository.gitBranch ?? detachedLabel(repository),
            head: repository.gitHead ?? "unknown",
            dirtyDetail: repository.gitDirtyStatus == "dirty" ? dirtyDetail(repository) : nil
        )
    }

    private func agentContractIssues(
        _ repository: RepositorySummary,
        contract: StorageAgentContractCoverageModel
    ) -> [StorageAgentGuidanceIssueModel] {
        sortedAgentGuidanceIssues(repository).filter { issue in
            issue.path == contract.path
        }
    }

    private func agentContractIcon(_ contract: StorageAgentContractCoverageModel) -> String {
        switch contract.status {
        case "ok":
            return "checkmark.seal"
        case "partial":
            return "circle.lefthalf.filled"
        case "missing":
            return "doc.badge.plus"
        case "untracked":
            return "doc.badge.exclamationmark"
        case "error":
            return "xmark.octagon"
        default:
            return contract.present ? "doc.text" : "doc.badge.questionmark"
        }
    }

    private func agentGuidanceIssueRow(_ issue: StorageAgentGuidanceIssueModel) -> some View {
        AetowerSurface(level: agentIssueIsCritical(issue) ? .critical : .warning, padding: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: agentIssueIsCritical(issue) ? "xmark.octagon" : "exclamationmark.triangle")
                    .foregroundStyle(agentIssueIsCritical(issue) ? AetowerDesign.Status.error : AetowerDesign.Status.warning)
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        AetowerBadge(
                            agentIssueIsCritical(issue) ? "ERROR" : issue.severity.uppercased(),
                            tone: agentIssueIsCritical(issue) ? AetowerDesign.Status.error : AetowerDesign.Status.warning
                        )
                        Text(issue.path)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.tertiary)
                        Text(issue.id)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.tertiary)
                            .lineLimit(1)
                    }
                    Text(issue.title)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text(issue.detail)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func agentIssueIsCritical(_ issue: StorageAgentGuidanceIssueModel) -> Bool {
        issue.severity == "error" || issue.id.contains(".missing")
    }

    private func sortedAgentGuidanceIssues(
        _ repository: RepositorySummary
    ) -> [StorageAgentGuidanceIssueModel] {
        repository.agentGuidanceIssues.sorted { left, right in
            let leftRank = left.severity == "error" ? 0 : 1
            let rightRank = right.severity == "error" ? 0 : 1
            if leftRank != rightRank { return leftRank < rightRank }
            if left.path != right.path { return left.path < right.path }
            return left.title < right.title
        }
    }

    private func repositoryGitIntelligence(_ repository: RepositorySummary) -> some View {
        AetowerSection("Git intelligence", subtitle: "Remote identity, working tree state, and duplicate clone detection") {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 220), spacing: AetowerDesign.Spacing.md)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.md
                ) {
                    gitFact("Remote", value: repository.gitRemoteKey ?? "No remote", icon: "network")
                    gitFact("Branch", value: repository.gitBranch ?? detachedLabel(repository), icon: "arrow.triangle.branch")
                    gitFact("HEAD", value: repository.gitHead ?? "Unknown", icon: "number")
                    gitFact("Worktree", value: dirtyDetail(repository), icon: "pencil.and.scribble")
                }

                if repository.cloneGroupCount > 1 {
                    AetowerInfoBanner(
                        cloneGroupDetail(repository),
                        title: "Duplicate clone group",
                        systemImage: "square.stack.3d.up",
                        tone: AetowerDesign.Status.warning,
                        level: .warning
                    )
                    LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        ForEach(repository.cloneGroupRoots, id: \.self) { root in
                            cloneRootRow(root, isCurrent: root == repository.root)
                        }
                    }
                }
            }
        }
    }

    private func gitFact(_ label: String, value: String, icon: String) -> some View {
        AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: icon)
                    .foregroundStyle(AetowerDesign.Tone.cpu)
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text(label.uppercased())
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                    Text(value)
                        .font(AetowerDesign.Typography.compactData(size: 11))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(2)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                }
            }
        }
    }

    private func cloneRootRow(_ root: String, isCurrent: Bool) -> some View {
        AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerBadge(isCurrent ? "Current" : "Clone", tone: isCurrent ? AetowerDesign.Status.ready : AetowerDesign.Status.warning)
                Text(shortPath(root))
                    .font(AetowerDesign.Typography.compactData(size: 10))
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                Spacer(minLength: AetowerDesign.Spacing.sm)
                Button("Reveal") {
                    reveal(root)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
        }
    }

    private func repositoryActions(_ repository: RepositorySummary) -> some View {
        AetowerSurface {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Repository actions")
                        .font(AetowerDesign.Typography.sectionTitle)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text("Read-only today. This is where optimization, cleanup, branch hygiene, and build-cache actions will attach later.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                Button("Reveal") {
                    reveal(repository.root)
                }
                Button("Copy path") {
                    copy(repository.root)
                }
                Button("Copy brief") {
                    copy(optimizationBrief(for: repository))
                    copiedRepositoryID = repository.id
                }
                .buttonStyle(.borderedProminent)
                AetowerBadge(
                    copiedRepositoryID == repository.id ? "Copied" : "Ready",
                    tone: copiedRepositoryID == repository.id ? AetowerDesign.Status.ready : AetowerDesign.Status.neutral
                )
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
    }

    private func topArtifacts(_ repository: RepositorySummary) -> some View {
        AetowerSection("Top artifact folders", subtitle: "Highest-value optimization targets") {
            if repository.topArtifactFolders.isEmpty {
                AetowerInfoBanner(
                    repository.hasStorageFootprint
                        ? "No top artifact folder is available for this repository yet."
                        : "This Git repository is indexed, but no tracked build, log, cache, dependency, or release artifact exceeded the storage scan threshold.",
                    systemImage: "folder",
                    tone: AetowerDesign.Status.neutral
                )
            } else {
                LazyVStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(repository.topArtifactFolders.prefix(5)) { folder in
                        artifactFolderRow(folder)
                    }
                }
            }
        }
    }

    private func artifactFolderRow(_ folder: StorageRepoArtifactFolderModel) -> some View {
        AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: cleanupTierIcon(folder.cleanupTier))
                    .foregroundStyle(cleanupTierTone(folder.cleanupTier))
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text(folder.displayName)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text(shortPath(folder.path))
                        .font(AetowerDesign.Typography.compactData(size: 10))
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: AetowerDesign.Spacing.sm)
                VStack(alignment: .trailing, spacing: AetowerDesign.Spacing.xs) {
                    Text(formatBytes(folder.sizeBytes))
                        .font(AetowerDesign.Typography.data)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    AetowerBadge(folder.cleanupTier, tone: cleanupTierTone(folder.cleanupTier))
                }
            }
        }
    }

    private func liveContext(_ repository: RepositorySummary) -> some View {
        AetowerSection("Live context", subtitle: "Current agents, terminal sessions, and attributed processes") {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 170), spacing: AetowerDesign.Spacing.md)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.md
            ) {
                AetowerMetricTile(
                    "Chau7 sessions",
                    value: "\(repository.liveSessionCount)",
                    detail: repository.lastBranchTouched ?? "branch unavailable",
                    systemImage: "terminal",
                    tone: repository.liveSessionCount > 0 ? AetowerDesign.Status.ready : AetowerDesign.Status.neutral
                )
                AetowerMetricTile(
                    "Entities",
                    value: "\(repository.liveEntityCount)",
                    detail: "adapter-attributed live processes",
                    systemImage: "point.3.connected.trianglepath.dotted",
                    tone: repository.liveEntityCount > 0 ? AetowerDesign.Tone.cpu : AetowerDesign.Status.neutral
                )
                AetowerMetricTile(
                    "Last writer",
                    value: repository.lastWriterProcess ?? "Unknown",
                    detail: repository.lastWriterPid.map { "pid \($0)" } ?? "waiting for file-event journal",
                    systemImage: "pencil.and.list.clipboard",
                    tone: repository.lastWriterProcess == nil ? AetowerDesign.Status.neutral : AetowerDesign.Status.ready
                )
            }
        }
    }

    private func futureOptimizationLanes(
        _ repository: RepositorySummary,
        report: StorageHygieneReportModel
    ) -> some View {
        AetowerSection("Optimization lanes", subtitle: "Future repository-management surface") {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 220), spacing: AetowerDesign.Spacing.md)],
                alignment: .leading,
                spacing: AetowerDesign.Spacing.md
            ) {
                laneCard(
                    title: "Artifact hygiene",
                    detail: "\(formatBytes(repository.artifactBytes)) can be explained by build/log/cache folders before cleanup actions are enabled.",
                    icon: "externaldrive.badge.minus",
                    tone: AetowerDesign.Tone.disk
                )
                laneCard(
                    title: "Growth budget",
                    detail: "Track repo deltas against \(formatBytes(report.budgetGuardrails.repoGrowthBudgetBytesPerDay)) per day.",
                    icon: "speedometer",
                    tone: growthTone(repository)
                )
                laneCard(
                    title: "Agent attribution",
                    detail: "\(formatBytes(repository.agentArtifactBytes)) attributed to AI-agent sessions for this repo.",
                    icon: "person.crop.circle.badge.gearshape",
                    tone: AetowerDesign.Tone.memory
                )
                laneCard(
                    title: "Build cost",
                    detail: "\(repository.estimatedRebuildCost) rebuild class; defer expensive cleanup until confirmed.",
                    icon: "hammer",
                    tone: AetowerDesign.Tone.energy
                )
            }
        }
    }

    private func laneCard(
        title: String,
        detail: String,
        icon: String,
        tone: Color
    ) -> some View {
        AetowerSurface(level: .card) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: icon)
                    .foregroundStyle(tone)
                Text(title)
                    .font(AetowerDesign.Typography.controlLabel)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                Text(detail)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var repositoryCountLabel: String {
        guard let report = state.storageHygieneReport else {
            return state.storageHygieneIsLoading ? "Loading" : "0"
        }
        return "\(repositorySummaries(from: report).count)"
    }

    private var artifactBytesLabel: String {
        guard let report = state.storageHygieneReport else { return "0 MB" }
        return formatBytes(totalArtifactBytes(repositorySummaries(from: report)))
    }

    private var attentionCountLabel: String {
        guard let report = state.storageHygieneReport else { return "0" }
        return "\(repositorySummaries(from: report).filter { $0.attentionScore >= 8 }.count)"
    }

    private var repositoryBadgeTone: Color {
        state.storageHygieneReport == nil ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
    }

    private var attentionBadgeTone: Color {
        guard let report = state.storageHygieneReport else { return AetowerDesign.Status.neutral }
        return repositorySummaries(from: report).contains { $0.attentionScore >= 8 }
            ? AetowerDesign.Status.warning
            : AetowerDesign.Status.ready
    }

    private func filteredRepositories(from report: StorageHygieneReportModel) -> [RepositorySummary] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        var repositories = repositorySummaries(from: report)

        switch mode {
        case .overview:
            break
        case .attention:
            repositories = repositories.filter {
                $0.attentionScore >= 8
                    || $0.violationCount > 0
                || $0.reviewItemCount > 0
                || $0.qualityIssueCount > 0
                || $0.agentGuidanceIssueCount > 0
                || $0.agentReadinessStatus == "blocked"
                || $0.agentReadinessStatus == "weak"
                || $0.cloneGroupCount > 1
                || $0.gitDirtyStatus == "dirty"
            }
        }

        if !query.isEmpty {
            repositories = repositories.filter { repository in
                repository.name.localizedCaseInsensitiveContains(query)
                    || repository.root.localizedCaseInsensitiveContains(query)
                    || (repository.gitRemoteKey?.localizedCaseInsensitiveContains(query) ?? false)
                    || (repository.gitRemoteOriginUrl?.localizedCaseInsensitiveContains(query) ?? false)
                    || (repository.gitBranch?.localizedCaseInsensitiveContains(query) ?? false)
                    || (repository.gitHead?.localizedCaseInsensitiveContains(query) ?? false)
                    || repository.agentReadinessStatus.localizedCaseInsensitiveContains(query)
                    || repository.agentContractCoverage.contains {
                        $0.label.localizedCaseInsensitiveContains(query)
                            || $0.path.localizedCaseInsensitiveContains(query)
                            || $0.status.localizedCaseInsensitiveContains(query)
                    }
                    || (repository.lastBranchTouched?.localizedCaseInsensitiveContains(query) ?? false)
                    || (repository.lastWriterProcess?.localizedCaseInsensitiveContains(query) ?? false)
                    || repository.topArtifactFolders.contains {
                        $0.displayName.localizedCaseInsensitiveContains(query)
                            || $0.path.localizedCaseInsensitiveContains(query)
                            || $0.kind.localizedCaseInsensitiveContains(query)
                    }
            }
        }

        return repositories.sorted(by: repositoryComparator)
    }

    private func repositoryComparator(_ left: RepositorySummary, _ right: RepositorySummary) -> Bool {
        switch sort {
        case .attention:
            if left.attentionScore != right.attentionScore {
                return left.attentionScore > right.attentionScore
            }
            return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
        case .size:
            return left.currentSizeBytes == right.currentSizeBytes
                ? left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
                : left.currentSizeBytes > right.currentSizeBytes
        case .growth:
            return (left.growthBytes ?? 0) == (right.growthBytes ?? 0)
                ? left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
                : (left.growthBytes ?? 0) > (right.growthBytes ?? 0)
        case .artifacts:
            return left.artifactBytes == right.artifactBytes
                ? left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
                : left.artifactBytes > right.artifactBytes
        case .name:
            return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
        }
    }

    private func repositorySummaries(from report: StorageHygieneReportModel) -> [RepositorySummary] {
        let violationsByRoot = Dictionary(grouping: report.budgetGuardrails.violations.compactMap { violation -> (String, StorageBudgetViolationModel)? in
            guard let repoRoot = violation.repoRoot else { return nil }
            return (repoRoot, violation)
        }, by: \.0)
        let itemsByRoot = Dictionary(grouping: report.items.compactMap { item -> (String, StorageHygieneItemModel)? in
            guard let repoRoot = item.attribution.repoRoot else { return nil }
            return (repoRoot, item)
        }, by: \.0)
        let footprintsByRoot = Dictionary(uniqueKeysWithValues: report.repoFootprints.map { ($0.repoRoot, $0) })

        var seenRoots = Set<String>()
        var summaries = report.repositoryInventory.map { inventory in
            seenRoots.insert(inventory.repoRoot)
            let footprint = footprintsByRoot[inventory.repoRoot]
            return repositorySummary(
                root: inventory.repoRoot,
                id: inventory.id,
                name: inventory.repoName,
                gitBranch: inventory.gitBranch,
                gitHead: inventory.gitHead,
                gitRef: inventory.gitRef,
                gitDetachedHead: inventory.gitDetachedHead,
                gitRemoteOriginUrl: inventory.gitRemoteOriginUrl,
                gitRemoteKey: inventory.gitRemoteKey,
                gitRemoteHost: inventory.gitRemoteHost,
                gitRemoteOwner: inventory.gitRemoteOwner,
                gitRemoteName: inventory.gitRemoteName,
                gitDirtyStatus: inventory.gitDirtyStatus,
                gitDirtyFileCount: inventory.gitDirtyFileCount,
                gitDirtyTruncated: inventory.gitDirtyTruncated,
                cloneGroupCount: inventory.cloneGroupCount,
                cloneGroupRoots: inventory.cloneGroupRoots,
                discoveredRoot: inventory.discoveredRoot,
                hasAgentsMd: inventory.hasAgentsMd,
                hasClaudeMd: inventory.hasClaudeMd,
                claudeMdBytes: inventory.claudeMdBytes,
                claudeMdDelegationMaxBytes: inventory.claudeMdDelegationMaxBytes,
                claudeMdDelegatesToAgentsMd: inventory.claudeMdDelegatesToAgentsMd,
                agentReadinessScore: inventory.agentReadinessScore,
                agentReadinessStatus: inventory.agentReadinessStatus,
                agentContractMissingCount: inventory.agentContractMissingCount,
                agentContractCoverage: inventory.agentContractCoverage,
                agentGuidanceStatus: inventory.agentGuidanceStatus,
                agentGuidanceIssueCount: inventory.agentGuidanceIssueCount,
                agentGuidanceIssues: inventory.agentGuidanceIssues,
                footprint: footprint,
                items: itemsByRoot[inventory.repoRoot]?.map(\.1) ?? [],
                violations: violationsByRoot[inventory.repoRoot]?.count ?? 0,
                report: report
            )
        }

        for footprint in report.repoFootprints where !seenRoots.contains(footprint.repoRoot) {
            summaries.append(
                repositorySummary(
                    root: footprint.repoRoot,
                    id: footprint.id,
                    name: footprint.repoName,
                    gitBranch: footprint.lastBranchTouched,
                    gitHead: nil,
                    gitRef: nil,
                    gitDetachedHead: false,
                    gitRemoteOriginUrl: nil,
                    gitRemoteKey: nil,
                    gitRemoteHost: nil,
                    gitRemoteOwner: nil,
                    gitRemoteName: nil,
                    gitDirtyStatus: "unknown",
                    gitDirtyFileCount: nil,
                    gitDirtyTruncated: false,
                    cloneGroupCount: 1,
                    cloneGroupRoots: [],
                    discoveredRoot: nil,
                    hasAgentsMd: false,
                    hasClaudeMd: false,
                    claudeMdBytes: nil,
                    claudeMdDelegationMaxBytes: defaultClaudeMdDelegationMaxBytes,
                    claudeMdDelegatesToAgentsMd: false,
                    agentReadinessScore: 0,
                    agentReadinessStatus: "unknown",
                    agentContractMissingCount: 0,
                    agentContractCoverage: [],
                    agentGuidanceStatus: "unknown",
                    agentGuidanceIssueCount: 0,
                    agentGuidanceIssues: [],
                    footprint: footprint,
                    items: itemsByRoot[footprint.repoRoot]?.map(\.1) ?? [],
                    violations: violationsByRoot[footprint.repoRoot]?.count ?? 0,
                    report: report
                )
            )
        }

        return summaries
    }

    private func repositorySummary(
        root: String,
        id: String,
        name: String,
        gitBranch: String?,
        gitHead: String?,
        gitRef: String?,
        gitDetachedHead: Bool,
        gitRemoteOriginUrl: String?,
        gitRemoteKey: String?,
        gitRemoteHost: String?,
        gitRemoteOwner: String?,
        gitRemoteName: String?,
        gitDirtyStatus: String,
        gitDirtyFileCount: UInt64?,
        gitDirtyTruncated: Bool,
        cloneGroupCount: UInt64,
        cloneGroupRoots: [String],
        discoveredRoot: String?,
        hasAgentsMd: Bool,
        hasClaudeMd: Bool,
        claudeMdBytes: UInt64?,
        claudeMdDelegationMaxBytes: UInt64,
        claudeMdDelegatesToAgentsMd: Bool,
        agentReadinessScore: UInt8,
        agentReadinessStatus: String,
        agentContractMissingCount: UInt64,
        agentContractCoverage: [StorageAgentContractCoverageModel],
        agentGuidanceStatus: String,
        agentGuidanceIssueCount: UInt64,
        agentGuidanceIssues: [StorageAgentGuidanceIssueModel],
        footprint: StorageRepoFootprintModel?,
        items: [StorageHygieneItemModel],
        violations: Int,
        report: StorageHygieneReportModel
    ) -> RepositorySummary {
        let live = liveContext(for: root)
        let agent = agentContext(for: root, report: report)
        return RepositorySummary(
            id: id,
            root: root,
            name: name,
            gitBranch: gitBranch,
            gitHead: gitHead,
            gitRef: gitRef,
            gitDetachedHead: gitDetachedHead,
            gitRemoteOriginUrl: gitRemoteOriginUrl,
            gitRemoteKey: gitRemoteKey,
            gitRemoteHost: gitRemoteHost,
            gitRemoteOwner: gitRemoteOwner,
            gitRemoteName: gitRemoteName,
            gitDirtyStatus: gitDirtyStatus,
            gitDirtyFileCount: gitDirtyFileCount,
            gitDirtyTruncated: gitDirtyTruncated,
            cloneGroupCount: cloneGroupCount,
            cloneGroupRoots: cloneGroupRoots,
            discoveredRoot: discoveredRoot,
            hasAgentsMd: hasAgentsMd,
            hasClaudeMd: hasClaudeMd,
            claudeMdBytes: claudeMdBytes,
            claudeMdDelegationMaxBytes: claudeMdDelegationMaxBytes,
            claudeMdDelegatesToAgentsMd: claudeMdDelegatesToAgentsMd,
            agentReadinessScore: agentReadinessScore,
            agentReadinessStatus: agentReadinessStatus,
            agentContractMissingCount: agentContractMissingCount,
            agentContractCoverage: agentContractCoverage,
            agentGuidanceStatus: agentGuidanceStatus,
            agentGuidanceIssueCount: agentGuidanceIssueCount,
            agentGuidanceIssues: agentGuidanceIssues,
            hasStorageFootprint: footprint != nil,
            currentSizeBytes: footprint?.currentSizeBytes ?? 0,
            artifactBytes: footprint?.artifactBytes ?? 0,
            itemCount: footprint?.itemCount ?? 0,
            growthBytes: footprint.flatMap { storageGrowthDelta(for: $0) },
            growthWindow: footprint.map { storageGrowthWindow(for: $0) } ?? "no storage footprint baseline",
            estimatedRebuildCost: footprint?.estimatedRebuildCost ?? "None",
            estimatedRebuildSeconds: footprint?.estimatedRebuildSeconds ?? 0,
            lastWriterProcess: footprint?.lastWriterProcess,
            lastWriterPid: footprint?.lastWriterPid,
            lastBranchTouched: footprint?.lastBranchTouched ?? gitBranch ?? gitHead,
            topArtifactFolders: footprint?.topArtifactFolders ?? [],
            caveats: footprint?.caveats ?? [
                "Indexed from Git repository discovery. No tracked storage artifacts were found in the bounded hygiene scan."
            ],
            violationCount: violations,
            reviewItemCount: items.filter { $0.safety != "safe" }.count,
            safeItemCount: items.filter { $0.safety == "safe" }.count,
            staleItemCount: items.filter(\.stale).count,
            liveSessionCount: live.sessionCount,
            liveEntityCount: live.entityCount,
            liveMemoryBytes: live.memoryBytes,
            liveCPUPercent: live.cpuPercent,
            agentArtifactBytes: agent.artifactBytes,
            agentCount: agent.agentCount
        )
    }

    private func liveContext(for repoRoot: String) -> (sessionCount: Int, entityCount: Int, memoryBytes: UInt64, cpuPercent: Float) {
        let sessions = state.snapshot.chau7Sessions.filter { session in
            session.repoRoot == repoRoot || session.workspacePath == repoRoot
        }
        let entities = state.snapshot.entities.filter { entity in
            entity.components.contains { component in
                component.adapterContext?.repoRoot == repoRoot || component.cwd == repoRoot
            }
        }
        let memory = entities.reduce(UInt64(0)) { total, entity in
            total.addingReportingOverflow(entityEffectiveMemoryBytes(entity)).partialValue
        }
        let cpu = entities.reduce(Float(0)) { $0 + $1.metrics.cpuPercent }
        return (sessions.count, entities.count, memory, cpu)
    }

    private func agentContext(
        for repoRoot: String,
        report: StorageHygieneReportModel
    ) -> (artifactBytes: UInt64, agentCount: Int) {
        let agents = report.agentHygiene.agents.filter { agent in
            agent.topRepositories.contains { $0.repoRoot == repoRoot }
        }
        let artifactBytes = agents.reduce(UInt64(0)) { total, agent in
            let repoBytes = agent.topRepositories
                .filter { $0.repoRoot == repoRoot }
                .reduce(UInt64(0)) { subtotal, repo in
                    subtotal.addingReportingOverflow(repo.artifactBytes).partialValue
                }
            return total.addingReportingOverflow(repoBytes).partialValue
        }
        return (artifactBytes, agents.count)
    }

    private func totalArtifactBytes(_ repositories: [RepositorySummary]) -> UInt64 {
        repositories.reduce(UInt64(0)) { total, repository in
            total.addingReportingOverflow(repository.artifactBytes).partialValue
        }
    }

    private func storageGrowthDelta(for footprint: StorageRepoFootprintModel) -> Int64? {
        if let growth = footprint.growthBytes {
            return growth
        }
        if let previous = state.previousStorageHygieneReport?.repoFootprints.first(where: {
            $0.repoRoot == footprint.repoRoot
        }) {
            return Int64(footprint.currentSizeBytes) - Int64(previous.currentSizeBytes)
        }
        if let baseline = state.persistedStorageHygieneBaseline?.repoFootprints.first(where: {
            $0.repoRoot == footprint.repoRoot
        }) {
            return Int64(footprint.currentSizeBytes) - Int64(baseline.currentSizeBytes)
        }
        return nil
    }

    private func storageGrowthWindow(for footprint: StorageRepoFootprintModel) -> String {
        if !footprint.growthWindow.isEmpty {
            return footprint.growthWindow
        }
        return state.persistedStorageHygieneBaseline == nil ? "no baseline yet" : "since saved baseline"
    }

    private func growthLabel(_ repository: RepositorySummary) -> String {
        guard let growth = repository.growthBytes else { return "No baseline" }
        if growth == 0 { return "Flat" }
        let absolute = formatBytes(UInt64(abs(growth)))
        return growth > 0 ? "+\(absolute)" : "-\(absolute)"
    }

    private func growthTone(_ repository: RepositorySummary) -> Color {
        guard let growth = repository.growthBytes else { return AetowerDesign.Status.neutral }
        if growth > 512 * 1024 * 1024 { return AetowerDesign.Status.warning }
        if growth > 0 { return AetowerDesign.Tone.energy }
        return AetowerDesign.Status.ready
    }

    private func repositorySummarySentence(_ repository: RepositorySummary) -> String {
        if !repository.hasStorageFootprint {
            var parts = [
                "Git repository indexed",
                "no tracked storage artifacts above threshold",
            ]
            if repository.liveSessionCount > 0 {
                parts.append("\(repository.liveSessionCount) live Chau7 session\(repository.liveSessionCount == 1 ? "" : "s")")
            }
            if repository.agentReadinessStatus != "ready" && repository.agentReadinessStatus != "unknown" {
                parts.append("operating contract \(repository.agentReadinessScore)%")
            }
            return parts.joined(separator: " · ")
        }

        var parts = [
            "\(formatBytes(repository.artifactBytes)) in rebuildable or reviewable artifacts",
            "\(repository.itemCount) tracked item\(repository.itemCount == 1 ? "" : "s")",
        ]
        if repository.liveSessionCount > 0 {
            parts.append("\(repository.liveSessionCount) live Chau7 session\(repository.liveSessionCount == 1 ? "" : "s")")
        }
        if repository.violationCount > 0 {
            parts.append("\(repository.violationCount) budget signal\(repository.violationCount == 1 ? "" : "s")")
        }
        if repository.agentReadinessStatus != "ready" && repository.agentReadinessStatus != "unknown" {
            parts.append("operating contract \(repository.agentReadinessScore)%")
        }
        if repository.qualityIssueCount > 0 {
            parts.append(repository.qualityStatusLabel)
        }
        return parts.joined(separator: " · ")
    }

    private func claudeMdOversized(_ repository: RepositorySummary) -> Bool {
        guard let bytes = repository.claudeMdBytes else { return false }
        return bytes > repository.claudeMdDelegationMaxBytes
    }

    private func claudeDelegationLabel(_ repository: RepositorySummary) -> String {
        if repository.claudeMdDelegatesToAgentsMd { return "Claude -> Agents" }
        if claudeMdOversized(repository) { return "Claude too large" }
        return "Claude not delegated"
    }

    private func qualityOverviewLabel(_ repository: RepositorySummary) -> String {
        if repository.agentGuidanceIssueCount > 0 {
            return agentGuidanceShortLabel(repository)
        }
        if repository.qualityIssueCount == 0 { return "Ok" }
        if !repository.hasAgentsMd { return "No AGENTS" }
        if !repository.hasClaudeMd { return "No CLAUDE" }
        return claudeDelegationLabel(repository)
    }

    private func agentReadinessLabel(_ repository: RepositorySummary) -> String {
        switch repository.agentReadinessStatus {
        case "ready":
            return "\(repository.agentReadinessScore)% ready"
        case "partial":
            return "\(repository.agentReadinessScore)% partial"
        case "weak":
            return "\(repository.agentReadinessScore)% weak"
        case "blocked":
            return "\(repository.agentReadinessScore)% blocked"
        default:
            return repository.agentContractCoverage.isEmpty ? "Unknown" : "\(repository.agentReadinessScore)%"
        }
    }

    private func agentReadinessTone(_ repository: RepositorySummary) -> Color {
        switch repository.agentReadinessStatus {
        case "ready":
            return AetowerDesign.Status.ready
        case "partial":
            return AetowerDesign.Tone.cpu
        case "weak":
            return AetowerDesign.Status.warning
        case "blocked":
            return AetowerDesign.Status.error
        default:
            return AetowerDesign.Status.neutral
        }
    }

    private func agentReadinessDetail(_ repository: RepositorySummary) -> String {
        let coverageCount = repository.agentContractCoverage.count
        let missing = repository.agentContractMissingCount
        if coverageCount == 0 {
            return "No agent contract coverage was reported for this repository."
        }
        return [
            "\(coverageCount) operating contract\(coverageCount == 1 ? "" : "s") checked",
            "\(missing) missing",
            "\(repository.agentGuidanceIssueCount) issue\(repository.agentGuidanceIssueCount == 1 ? "" : "s")",
        ].joined(separator: " · ")
    }

    private func agentContractStatusLabel(_ contract: StorageAgentContractCoverageModel) -> String {
        switch contract.status {
        case "ok":
            return "Ok"
        case "partial":
            return "Partial"
        case "missing":
            return "Missing"
        case "untracked":
            return "Untracked"
        case "error":
            return "Error"
        default:
            return contract.status.capitalized
        }
    }

    @ViewBuilder
    private func agentContractIconImage(_ contract: StorageAgentContractCoverageModel) -> some View {
        if contract.status == "missing" {
            Image(systemName: agentContractIcon(contract))
                .foregroundStyle(AetowerDesign.Status.error)
        } else {
            switch contract.severity {
            case "error":
                Image(systemName: agentContractIcon(contract))
                    .foregroundStyle(AetowerDesign.Status.error)
            case "warning":
                Image(systemName: agentContractIcon(contract))
                    .foregroundStyle(AetowerDesign.Status.warning)
            case "ok":
                Image(systemName: agentContractIcon(contract))
                    .foregroundStyle(AetowerDesign.Status.ready)
            default:
                Image(systemName: agentContractIcon(contract))
                    .foregroundStyle(AetowerDesign.Status.neutral)
            }
        }
    }

    private func agentContractTone(_ contract: StorageAgentContractCoverageModel) -> Color {
        if contract.status == "missing" {
            return AetowerDesign.Status.error
        }
        switch contract.severity {
        case "error":
            return AetowerDesign.Status.error
        case "warning":
            return AetowerDesign.Status.warning
        case "ok":
            return AetowerDesign.Status.ready
        default:
            return AetowerDesign.Status.neutral
        }
    }

    private func contractSurfaceLevel(_ contract: StorageAgentContractCoverageModel) -> AetowerSurfaceLevel {
        if contract.status == "missing" {
            return .critical
        }
        switch contract.severity {
        case "error":
            return .critical
        case "warning":
            return .warning
        default:
            return .quiet
        }
    }

    private func agentGuidanceShortLabel(_ repository: RepositorySummary) -> String {
        switch repository.agentGuidanceStatus {
        case "error":
            return "\(repository.agentGuidanceIssueCount) error\(repository.agentGuidanceIssueCount == 1 ? "" : "s")"
        case "warning":
            return "\(repository.agentGuidanceIssueCount) warning\(repository.agentGuidanceIssueCount == 1 ? "" : "s")"
        case "ok":
            return "Ok"
        default:
            return "Unknown"
        }
    }

    private func agentGuidanceTitle(_ repository: RepositorySummary) -> String {
        switch repository.agentReadinessStatus {
        case "ready":
            return "Operating contract is ready"
        case "partial":
            return "Operating contract is partial"
        case "weak":
            return "Operating contract is weak"
        case "blocked":
            return "Operating contract is blocked"
        default:
            break
        }
        switch repository.agentGuidanceStatus {
        case "error":
            return "Operating contract needs fixes"
        case "warning":
            return "Operating contract has warnings"
        case "ok":
            return "Operating contract is clean"
        default:
            return "Operating contract unavailable"
        }
    }

    private func agentGuidanceSummary(_ repository: RepositorySummary) -> String {
        let readiness = agentReadinessDetail(repository)
        if repository.agentGuidanceIssues.isEmpty {
            return "Operating contract score is \(repository.agentReadinessScore)%. \(readiness)."
        }
        let errorCount = repository.agentGuidanceIssues.filter { $0.severity == "error" }.count
        let warningCount = repository.agentGuidanceIssues.filter { $0.severity == "warning" }.count
        var parts = [
            "Readiness \(repository.agentReadinessScore)%",
            readiness,
            "\(errorCount) error\(errorCount == 1 ? "" : "s")",
            "\(warningCount) warning\(warningCount == 1 ? "" : "s")",
            "Rules cover AGENTS.md plus manifest, tasks, repo-map, contracts, commands, validation, boundaries, risks, and references.",
        ]
        if let firstIssue = sortedAgentGuidanceIssues(repository).first {
            parts.append("First issue: \(firstIssue.title) in \(firstIssue.path).")
        }
        return parts.joined(separator: " · ")
    }

    private func gitOverviewLabel(_ repository: RepositorySummary) -> String {
        if repository.cloneGroupCount > 1 { return "\(repository.cloneGroupCount)x clone" }
        if repository.gitDirtyStatus == "dirty" { return dirtyOverviewLabel(repository) }
        if repository.gitDirtyStatus == "timeout" { return "Slow status" }
        if repository.gitDetachedHead { return "Detached" }
        return repository.gitBranch ?? "No branch"
    }

    private func gitOverviewTone(_ repository: RepositorySummary) -> Color {
        if repository.cloneGroupCount > 1 { return AetowerDesign.Status.warning }
        if repository.gitDirtyStatus == "dirty" { return AetowerDesign.Tone.energy }
        if repository.gitDirtyStatus == "timeout" { return AetowerDesign.Status.warning }
        if repository.gitDetachedHead { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.ready
    }

    private func gitDetail(_ repository: RepositorySummary) -> String {
        [
            repository.gitRemoteKey ?? "remote unavailable",
            repository.gitBranch ?? detachedLabel(repository),
            dirtyDetail(repository),
        ].joined(separator: " · ")
    }

    private func dirtyOverviewLabel(_ repository: RepositorySummary) -> String {
        guard let count = repository.gitDirtyFileCount else { return "Dirty" }
        let suffix = repository.gitDirtyTruncated ? "+" : ""
        return "\(count)\(suffix) dirty"
    }

    private func dirtyDetail(_ repository: RepositorySummary) -> String {
        switch repository.gitDirtyStatus {
        case "clean":
            return "Clean"
        case "dirty":
            return dirtyOverviewLabel(repository)
        case "timeout":
            return "Status timed out"
        case "unavailable":
            return "Status unavailable"
        default:
            return "Status unknown"
        }
    }

    private func detachedLabel(_ repository: RepositorySummary) -> String {
        repository.gitDetachedHead ? "Detached HEAD" : "Branch unavailable"
    }

    private func cloneGroupLabel(_ repository: RepositorySummary) -> String {
        repository.cloneGroupCount > 1 ? "\(repository.cloneGroupCount) clones" : "Unique"
    }

    private func cloneGroupDetail(_ repository: RepositorySummary) -> String {
        let remote = repository.gitRemoteKey ?? "this remote"
        return [
            "\(repository.cloneGroupCount) local folders point at \(remote).",
            "Keep separate only for active branches or isolated experiments.",
            "Otherwise one canonical clone avoids duplicate build artifacts.",
        ].joined(separator: " ")
    }

    private func qualityDetail(_ repository: RepositorySummary) -> String {
        if repository.agentGuidanceIssueCount > 0 {
            return agentGuidanceSummary(repository)
        }
        let claudeDetail: String
        if !repository.hasClaudeMd {
            claudeDetail = "CLAUDE.md missing"
        } else if repository.claudeMdDelegatesToAgentsMd {
            claudeDetail = "CLAUDE.md delegates"
        } else if claudeMdOversized(repository), let bytes = repository.claudeMdBytes {
            claudeDetail =
                "CLAUDE.md too large (\(formatBytes(bytes)) > \(formatBytes(repository.claudeMdDelegationMaxBytes)))"
        } else {
            claudeDetail = "CLAUDE.md first non-empty line must be @AGENTS.md"
        }

        return [
            repository.hasAgentsMd ? "AGENTS.md present" : "AGENTS.md missing",
            repository.hasClaudeMd ? "CLAUDE.md present" : "CLAUDE.md missing",
            claudeDetail,
        ].joined(separator: " · ")
    }

    private func optimizationBrief(for repository: RepositorySummary) -> String {
        [
            "# Repository optimization brief",
            "",
            "- Repository: \(repository.name)",
            "- Root: \(repository.root)",
            "- Remote: \(repository.gitRemoteKey ?? "unavailable")",
            "- Branch: \(repository.gitBranch ?? detachedLabel(repository))",
            "- HEAD: \(repository.gitHead ?? "unknown")",
            "- Worktree: \(dirtyDetail(repository))",
            "- Clone group: \(cloneGroupLabel(repository))",
            "- Current footprint: \(formatBytes(repository.currentSizeBytes))",
            "- Artifact footprint: \(formatBytes(repository.artifactBytes)) across \(repository.itemCount) item(s)",
            "- Growth: \(growthLabel(repository)) (\(repository.growthWindow))",
            "- Rebuild cost: \(repository.estimatedRebuildCost) (\(rebuildTimeLabel(repository.estimatedRebuildSeconds)))",
            "- Review items: \(repository.reviewItemCount)",
            "- Stale items: \(repository.staleItemCount)",
            "- Live sessions: \(repository.liveSessionCount)",
            "- Live attributed entities: \(repository.liveEntityCount)",
            "- Agent-attributed artifacts: \(formatBytes(repository.agentArtifactBytes))",
            "- Operating contract readiness: \(agentReadinessLabel(repository))",
            "- Agent contracts missing: \(repository.agentContractMissingCount)",
            "- Operating contract guidance: \(agentGuidanceTitle(repository))",
            "- Quality: \(qualityDetail(repository))",
            "",
            "Agent contract coverage:",
            repository.agentContractCoverage.map {
                "- \($0.label): \($0.coveragePercent)% \($0.status) | \($0.path) | \($0.detail)"
            }.joined(separator: "\n"),
            "",
            "Operating contract issues:",
            repository.agentGuidanceIssues.prefix(8).map {
                "- \($0.severity.uppercased()) | \($0.path) | \($0.title): \($0.detail)"
            }.joined(separator: "\n"),
            "",
            "Top artifact folders:",
            repository.topArtifactFolders.prefix(5).map {
                "- \(formatBytes($0.sizeBytes)) | \($0.cleanupTier) | \($0.path)"
            }.joined(separator: "\n"),
            "",
            "Recommended next step: inspect large rebuildable folders first, then decide whether cleanup cost is acceptable for the current branch/session."
        ]
        .filter { !$0.isEmpty }
        .joined(separator: "\n")
    }

    private func rebuildTimeLabel(_ seconds: UInt64?) -> String {
        guard let seconds else { return "duration unknown" }
        if seconds < 60 { return "\(seconds)s estimated" }
        if seconds < 3_600 { return "\(seconds / 60)m estimated" }
        return "\(seconds / 3_600)h estimated"
    }

    private func cleanupTierIcon(_ tier: String) -> String {
        switch tier.lowercased() {
        case "safe": return "checkmark.shield"
        case "rebuildable": return "hammer"
        case "expensive": return "clock.badge.exclamationmark"
        case "risky": return "exclamationmark.triangle"
        default: return "shippingbox"
        }
    }

    private func cleanupTierTone(_ tier: String) -> Color {
        switch tier.lowercased() {
        case "safe": return AetowerDesign.Status.ready
        case "rebuildable": return AetowerDesign.Tone.cpu
        case "expensive": return AetowerDesign.Status.warning
        case "risky": return AetowerDesign.Status.error
        default: return AetowerDesign.Status.neutral
        }
    }

    private func shortPath(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        if path == home { return "~" }
        if path.hasPrefix(home + "/") {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }

    private func reveal(_ path: String) {
        let url = URL(fileURLWithPath: path)
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    private func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}
