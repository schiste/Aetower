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

private struct RepositoryAttentionItem: Identifiable {
    let id: String
    let title: String
    let detail: String
    let systemImage: String
    let tone: Color
    let level: AetowerSurfaceLevel
}

private struct ScorecardWorkflowPreview: Identifiable {
    let id: String
    let repositoryName: String
    let repositoryRoot: String
    let relativePath: String
    let absolutePath: String
    let contents: String
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
    let scorecardReport: RepositoryScorecardReportModel?

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
        return storageScore + runtimeScore + duplicateScore + dirtyScore + scorecardAttentionScore
    }

    var scorecardAttentionScore: Double {
        guard let scorecardReport, scorecardReport.status == "ok" else { return 0 }
        let aggregateScore: Double
        if let score = scorecardReport.score {
            if score < 5 {
                aggregateScore = 12
            } else if score <= 7 {
                aggregateScore = 6
            } else {
                aggregateScore = 1
            }
        } else {
            aggregateScore = 0
        }
        let criticalFailureScore = scorecardCriticalFailureCount > 0 ? 10.0 : 0.0
        return max(aggregateScore, criticalFailureScore)
    }

    var scorecardCriticalFailureCount: Int {
        guard let scorecardReport else { return 0 }
        let highSeverityChecks = Set(scorecardReport.recommendations.compactMap { recommendation in
            ["critical", "high"].contains(recommendation.severity.lowercased())
                ? Self.normalizedScorecardCheckName(recommendation.checkName)
                : nil
        })
        return scorecardReport.failedChecks.filter { check in
            if let score = check.score, score <= 0 { return true }
            return highSeverityChecks.contains(Self.normalizedScorecardCheckName(check.name))
        }.count
    }

    var scorecardCaveat: String? {
        guard let scorecardReport else {
            return "OpenSSF Scorecard has not been run for this repository; supply-chain attention is unchanged."
        }
        guard scorecardReport.status == "ok" else {
            return "OpenSSF Scorecard is unavailable for this repository (\(scorecardReport.status)); supply-chain attention is unchanged."
        }
        guard scorecardReport.score != nil else {
            return "OpenSSF Scorecard completed without an aggregate score; supply-chain attention is unchanged."
        }
        return nil
    }

    var hasScorecardAttention: Bool {
        scorecardAttentionScore >= 5 || scorecardCriticalFailureCount > 0
    }

    var requiresAttention: Bool {
        attentionScore >= 8
            || violationCount > 0
            || reviewItemCount > 0
            || qualityIssueCount > 0
            || agentGuidanceIssueCount > 0
            || agentReadinessStatus == "blocked"
            || agentReadinessStatus == "weak"
            || cloneGroupCount > 1
            || gitDirtyStatus == "dirty"
            || hasScorecardAttention
    }

    private static func normalizedScorecardCheckName(_ name: String) -> String {
        name
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
            .replacingOccurrences(of: "_", with: "-")
            .replacingOccurrences(of: " ", with: "-")
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
        if scorecardAttentionScore >= 10 { return "Scorecard" }
        if cloneGroupCount > 1 { return "Cloned" }
        if (growthBytes ?? 0) > 0 { return "Growing" }
        if gitDirtyStatus == "dirty" { return "Dirty" }
        if scorecardAttentionScore >= 5 { return "Scorecard" }
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
        if scorecardAttentionScore >= 10 { return AetowerDesign.Status.error }
        if cloneGroupCount > 1 { return AetowerDesign.Status.warning }
        if (growthBytes ?? 0) > 512 * 1024 * 1024 { return AetowerDesign.Status.warning }
        if gitDirtyStatus == "dirty" { return AetowerDesign.Tone.energy }
        if scorecardAttentionScore >= 5 { return AetowerDesign.Status.warning }
        if reviewItemCount > 0 { return AetowerDesign.Status.warning }
        if liveSessionCount > 0 || liveEntityCount > 0 { return AetowerDesign.Status.ready }
        return AetowerDesign.Status.neutral
    }
}

private enum Chau7ContractLaunchState: Equatable {
    case preparingKit
    case verifyingKit(String)
    case kitReady(String)
    case openingChau7(String)
    case launched(String, warning: Bool)
    case failed(String)

    var label: String {
        switch self {
        case .preparingKit:
            return "Preparing kit"
        case .verifyingKit:
            return "Verifying kit"
        case .kitReady:
            return "Kit ready"
        case .openingChau7:
            return "Opening Chau7"
        case let .launched(_, warning):
            return warning ? "Prompt pending" : "Launched"
        case .failed:
            return "Failed"
        }
    }

    var icon: String {
        switch self {
        case .preparingKit, .verifyingKit, .kitReady, .openingChau7:
            return "arrow.triangle.2.circlepath"
        case let .launched(_, warning):
            return warning ? "exclamationmark.triangle" : "terminal"
        case .failed:
            return "xmark.octagon"
        }
    }

    var tone: Color {
        switch self {
        case .preparingKit, .verifyingKit, .kitReady, .openingChau7:
            return AetowerDesign.Status.neutral
        case let .launched(_, warning):
            return warning ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
        case .failed:
            return AetowerDesign.Status.error
        }
    }

    var detail: String {
        switch self {
        case .preparingKit:
            return "Copying the local Aethyme template/schema kit into the target repository."
        case let .verifyingKit(detail), let .kitReady(detail), let .openingChau7(detail):
            return detail
        case let .launched(detail, _), let .failed(detail):
            return detail
        }
    }

    var isLaunching: Bool {
        switch self {
        case .preparingKit, .verifyingKit, .kitReady, .openingChau7:
            return true
        case .launched, .failed:
            return false
        }
    }
}

private enum AgentContractLaunchPromptKind: Equatable {
    case generation
    case reconcile
}

public struct RepositoryView: View {
    let state: AppState
    let settings: SettingsStore
    @State private var mode: RepositoryMode = .overview
    @State private var sort: RepositorySort = .attention
    @State private var searchText = ""
    @State private var repositoryPath: [String] = []
    @State private var copiedRepositoryID: String?
    @State private var copiedAgentPromptKey: String?
    @State private var chau7LaunchStatusByKey: [String: Chau7ContractLaunchState] = [:]
    @State private var scorecardWorkflowWritingRoots: Set<String> = []
    @State private var scorecardWorkflowStatusByRoot: [String: String] = [:]
    @State private var scorecardWorkflowErrorsByRoot: [String: String] = [:]
    @State private var scorecardWorkflowPreview: ScorecardWorkflowPreview?
    @State private var selectedAgentContractByRepository: [String: String] = [:]

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
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
        .sheet(item: $scorecardWorkflowPreview) { preview in
            scorecardWorkflowPreviewSheet(preview)
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
                AetowerToolBadge(
                    "Scan",
                    value: repositoryScanStatusLabel,
                    systemImage: repositoryScanStatusIcon,
                    tone: repositoryScanStatusTone
                )
                .help(repositoryScanStatusHelp)
            }
        } actions: {
            Button {
                state.runStorageHygieneScan()
            } label: {
                Label(state.storageHygieneReport == nil ? "Scan" : "Refresh", systemImage: "arrow.clockwise")
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

    private var repositoryScanProgressBanner: some View {
        AetowerInfoBanner(
            repositoryScanStatusHelp,
            title: repositoryScanStatusLabel,
            systemImage: repositoryScanStatusIcon,
            tone: repositoryScanStatusTone,
            level: .quiet
        )
    }

    private var repositoryScanStatusLabel: String {
        if state.storageHygieneIsLoading {
            guard let job = state.storageScanJob else { return "Scanning" }
            switch job.status {
            case "queued": return "Queued"
            case "running": return job.progress.phase.isEmpty ? "Running" : job.progress.phase.capitalized
            case "paused": return "Paused"
            case "complete": return "Finalizing"
            default: return job.status.replacingOccurrences(of: "_", with: " ").capitalized
            }
        }
        guard let completedAt = state.storageHygieneCompletedAt else { return "Not run" }
        return "Last \(completedAt.formatted(date: .omitted, time: .shortened))"
    }

    private var repositoryScanStatusIcon: String {
        if let job = state.storageScanJob, job.status == "paused" { return "pause.circle" }
        return state.storageHygieneIsLoading ? "arrow.triangle.2.circlepath" : "clock"
    }

    private var repositoryScanStatusTone: Color {
        if let job = state.storageScanJob, job.status == "paused" { return AetowerDesign.Status.warning }
        return state.storageHygieneIsLoading ? AetowerDesign.Tone.cpu : AetowerDesign.Status.neutral
    }

    private var repositoryScanStatusHelp: String {
        guard let job = state.storageScanJob else {
            if state.storageHygieneIsLoading {
                return "Preparing repository inventory refresh."
            }
            if let completedAt = state.storageHygieneCompletedAt {
                return "Last repository scan completed \(completedAt.formatted(date: .abbreviated, time: .shortened))."
            }
            return "Repository inventory has not been scanned yet."
        }
        var parts = [
            "Status \(job.status)",
            "\(job.progress.scannedFiles) files",
            formatBytes(job.progress.scannedBytes),
        ]
        if !job.progress.phase.isEmpty {
            parts.append("phase \(job.progress.phase)")
        }
        if let currentPathHint = job.progress.currentPathHint, !currentPathHint.isEmpty {
            parts.append(shortPath(currentPathHint))
        }
        if let throttleReason = job.progress.throttleReason, !throttleReason.isEmpty {
            parts.append(throttleReason)
        }
        return parts.joined(separator: " · ")
    }

    private func repositoryDashboard(_ report: StorageHygieneReportModel) -> some View {
        let repositories = filteredRepositories(from: report)

        return ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                repositoryOverview(report, repositories: repositories)
                if state.storageHygieneIsLoading {
                    repositoryScanProgressBanner
                }

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
        let reviewCount = allRepositories.filter(\.requiresAttention).count

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
                    repositoryActions(repository)
                    repositoryAttentionSummary(repository)
                    repositorySignals(repository)
                    repositorySupplyChainReadiness(repository)
                    repositoryAgentGuidance(repository)
                    repositoryGitIntelligence(repository)
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

    private func repositoryAttentionSummary(_ repository: RepositorySummary) -> some View {
        let items = Array(repositoryAttentionItems(repository).prefix(4))
        return AetowerSection("Top risks", subtitle: "Immediate reasons this repository needs attention") {
            if items.isEmpty {
                AetowerInfoBanner(
                    "No immediate repository attention signals are active. Git, storage, agents, and supply-chain details remain available below.",
                    title: "No active risks",
                    systemImage: "checkmark.circle",
                    tone: AetowerDesign.Status.ready,
                    level: .quiet
                )
            } else {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 220), spacing: AetowerDesign.Spacing.sm)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.sm
                ) {
                    ForEach(items) { item in
                        repositoryAttentionCard(item)
                    }
                }
            }
        }
    }

    private func repositoryAttentionCard(_ item: RepositoryAttentionItem) -> some View {
        AetowerSurface(level: item.level, padding: AetowerDesign.Spacing.sm) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: item.systemImage)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        Text(item.title)
                            .font(AetowerDesign.Typography.controlLabel)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                        AetowerBadge("Attention", tone: item.tone)
                    }
                    Text(item.detail)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func repositoryAttentionItems(_ repository: RepositorySummary) -> [RepositoryAttentionItem] {
        var items: [RepositoryAttentionItem] = []
        if repository.violationCount > 0 {
            items.append(RepositoryAttentionItem(
                id: "budget",
                title: "Budget guardrail",
                detail: "\(repository.violationCount) repository budget signal\(repository.violationCount == 1 ? "" : "s") need review before cleanup.",
                systemImage: "exclamationmark.triangle",
                tone: AetowerDesign.Status.error,
                level: .critical
            ))
        }
        if repository.agentReadinessStatus == "blocked" || repository.agentReadinessStatus == "weak" {
            items.append(RepositoryAttentionItem(
                id: "contract-readiness",
                title: "Contract readiness",
                detail: agentReadinessDetail(repository),
                systemImage: "checklist.checked",
                tone: agentReadinessTone(repository),
                level: repository.agentReadinessStatus == "blocked" ? .critical : .warning
            ))
        }
        if repository.agentGuidanceIssueCount > 0 || repository.qualityIssueCount > 0 {
            items.append(RepositoryAttentionItem(
                id: "guidance",
                title: agentGuidanceTitle(repository),
                detail: qualityDetail(repository),
                systemImage: "doc.badge.exclamationmark",
                tone: repository.qualityStatusTone,
                level: repository.agentGuidanceStatus == "error" ? .critical : .warning
            ))
        }
        if repository.hasScorecardAttention {
            items.append(RepositoryAttentionItem(
                id: "scorecard",
                title: "Scorecard posture",
                detail: scorecardReadinessDetail(repository),
                systemImage: "shield.lefthalf.filled",
                tone: scorecardReadinessTone(repository),
                level: repository.scorecardAttentionScore >= 10 ? .critical : .warning
            ))
        }
        if repository.cloneGroupCount > 1 {
            items.append(RepositoryAttentionItem(
                id: "clone-group",
                title: "Duplicate clone group",
                detail: cloneGroupDetail(repository),
                systemImage: "square.stack.3d.up",
                tone: AetowerDesign.Status.warning,
                level: .warning
            ))
        }
        if repository.gitDirtyStatus == "dirty" {
            items.append(RepositoryAttentionItem(
                id: "dirty",
                title: "Dirty worktree",
                detail: dirtyDetail(repository),
                systemImage: "pencil.and.scribble",
                tone: AetowerDesign.Tone.energy,
                level: .warning
            ))
        }
        if (repository.growthBytes ?? 0) > 0 {
            items.append(RepositoryAttentionItem(
                id: "growth",
                title: "Storage growth",
                detail: "\(growthLabel(repository)) in \(repository.growthWindow).",
                systemImage: "chart.line.uptrend.xyaxis",
                tone: growthTone(repository),
                level: .warning
            ))
        }
        if repository.reviewItemCount > 0 {
            items.append(RepositoryAttentionItem(
                id: "review-items",
                title: "Reviewable artifacts",
                detail: "\(repository.reviewItemCount) item\(repository.reviewItemCount == 1 ? "" : "s") need human review before cleanup.",
                systemImage: "shippingbox",
                tone: AetowerDesign.Status.warning,
                level: .warning
            ))
        }
        return items
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
                "Supply-chain",
                value: scorecardReadinessLabel(repository),
                detail: scorecardReadinessDetail(repository),
                systemImage: "shield.lefthalf.filled",
                tone: scorecardReadinessTone(repository)
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
                                agentContractListRow(
                                    repository,
                                    contract: contract,
                                    selected: selectedAgentContract(repository)?.id == contract.id
                                )
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
        _ repository: RepositorySummary,
        contract: StorageAgentContractCoverageModel,
        selected: Bool
    ) -> some View {
        let key = AgentContractPrompts.key(repositoryID: repository.id, contract: contract, kind: "chau7")
        let launchState = chau7LaunchStatusByKey[key]
        let promptKind = agentContractLaunchPromptKind(contract)
        return AetowerSurface(
            level: selected ? .selected : contractSurfaceLevel(contract),
            padding: AetowerDesign.Spacing.sm
        ) {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                Button {
                    selectedAgentContractByRepository[repository.id] = contract.id
                } label: {
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
                .buttonStyle(.plain)
                .accessibilityLabel("\(contract.label), \(agentContractStatusLabel(contract))")
                .accessibilityValue(selected ? "Selected" : "Not selected")
                .accessibilityAddTraits(selected ? .isSelected : [])

                Button(promptKind == .reconcile ? "Reconcile" : "Generate") {
                    launchAgentContractPromptInChau7(
                        repository,
                        contract: contract,
                        issues: agentContractIssues(repository, contract: contract),
                        key: key,
                        promptKind: promptKind
                    )
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .disabled(launchState?.isLaunching == true)
                if let launchState {
                    Image(systemName: launchState.icon)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .help(launchState.detail)
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
        let chau7Key = AgentContractPrompts.key(repositoryID: repository.id, contract: contract, kind: "chau7")
        let promptKind = agentContractLaunchPromptKind(contract)
        let copied = copiedAgentPromptKey == generationKey || copiedAgentPromptKey == reconcileKey
        let launchState = chau7LaunchStatusByKey[chau7Key]
        return AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                    Image(systemName: "text.badge.plus")
                        .foregroundStyle(AetowerDesign.Status.neutral)
                    Text("Focused prompts")
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    AetowerBadge("Self-contained", tone: AetowerDesign.Status.ready)
                    AetowerBadge("Portable", tone: AetowerDesign.Status.neutral)
                    Spacer(minLength: AetowerDesign.Spacing.sm)
                    AetowerBadge(
                        copied ? "Copied" : "Ready",
                        tone: copied ? AetowerDesign.Status.ready : AetowerDesign.Status.neutral
                    )
                }
                Text("Copy a self-contained prompt, or launch Chau7 with a local Aethyme template/schema kit installed under `.aethyme/`.")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text("The agent uses `.aethyme/agent-contracts` as reference, writes portable output to `.agents/`, and reports unavailable validation explicitly.")
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
                    Button {
                        launchAgentContractPromptInChau7(
                            repository,
                            contract: contract,
                            issues: issues,
                            key: chau7Key,
                            promptKind: promptKind
                        )
                    } label: {
                        Label(promptKind == .reconcile ? "Chau7 reconcile" : "Chau7 generate", systemImage: "terminal")
                    }
                    .disabled(launchState?.isLaunching == true)
                    Spacer(minLength: AetowerDesign.Spacing.sm)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                if let launchState {
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        Image(systemName: launchState.icon)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                        AetowerBadge(launchState.label, tone: launchState.tone)
                        Text(launchState.detail)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                            .lineLimit(2)
                    }
                }
            }
        }
    }

    private func launchAgentContractPromptInChau7(
        _ repository: RepositorySummary,
        contract: StorageAgentContractCoverageModel,
        issues: [StorageAgentGuidanceIssueModel],
        key: String,
        promptKind: AgentContractLaunchPromptKind
    ) {
        let promptContext = agentContractPromptContext(repository)
        let repositoryRoot = repository.root
        let contractID = contract.id
        let contractPath = contract.path
        let socketPath = settings.chau7Endpoint
        let agentCommand = settings.chau7AgentCommand
        chau7LaunchStatusByKey[key] = .preparingKit

        Task {
            let nextState: Chau7ContractLaunchState
            do {
                let kitInstall = try await Task.detached(priority: .utility) {
                    try AethymeAgentContractKitInstaller.install(in: repositoryRoot)
                }.value
                let requiredKitFiles = AethymeAgentContractKitInstaller.requiredRelativePaths(
                    forContractID: contractID,
                    contractPath: contractPath,
                    install: kitInstall
                )
                await MainActor.run {
                    chau7LaunchStatusByKey[key] = .verifyingKit(
                        "Waiting for \(kitInstall.relativePath) templates and schemas to be readable."
                    )
                }
                try await Task.detached(priority: .utility) {
                    try AethymeAgentContractKitInstaller.waitForInstalledFiles(
                        in: repositoryRoot,
                        relativePaths: requiredKitFiles
                    )
                }.value
                let kitStatus = kitInstall.installed
                    ? "Installed \(kitInstall.relativePath)."
                    : "Reused \(kitInstall.relativePath)."
                await MainActor.run {
                    chau7LaunchStatusByKey[key] = .kitReady(
                        "\(kitStatus) Verified \(requiredKitFiles.count) local kit file\(requiredKitFiles.count == 1 ? "" : "s")."
                    )
                }
                try await Task.sleep(nanoseconds: 150_000_000)

                let prompt: String
                switch promptKind {
                case .generation:
                    prompt = AgentContractPrompts.generationPrompt(
                        repository: promptContext,
                        contract: contract,
                        issues: issues,
                        kit: kitInstall.promptContext
                    )
                case .reconcile:
                    prompt = AgentContractPrompts.reconcilePrompt(
                        repository: promptContext,
                        contract: contract,
                        issues: issues,
                        kit: kitInstall.promptContext
                    )
                }
                await MainActor.run {
                    chau7LaunchStatusByKey[key] = .openingChau7(
                        "Opening Chau7 after local kit verification; the focused prompt will be sent only after Chau7 accepts the launch."
                    )
                }
                let request = Chau7AgentLaunchRequest(
                    socketPath: socketPath,
                    repositoryRoot: repositoryRoot,
                    agentCommand: agentCommand,
                    prompt: prompt
                )
                let result = try await Chau7AgentLauncher.launch(request)
                let promptStatus = result.promptStatus ?? "unknown"
                let submitDetail = result.submitConfirmation.map { " Submit: \($0)." } ?? ""
                let kitDetail = kitInstall.installed
                    ? " Kit installed: \(kitInstall.relativePath)."
                    : " Kit ready: \(kitInstall.relativePath)."
                let excludeDetail = kitInstall.excludeUpdated ? " `.aethyme/` excluded locally." : ""
                let detail = "\(result.summary) Prompt: \(promptStatus).\(kitDetail)\(excludeDetail)\(submitDetail)"
                nextState = .launched(detail, warning: promptStatus != "sent")
            } catch {
                nextState = .failed(error.localizedDescription)
            }
            await MainActor.run {
                chau7LaunchStatusByKey[key] = nextState
            }
        }
    }

    private func launchScorecardRemediationInChau7(
        _ repository: RepositorySummary,
        report: RepositoryScorecardReportModel
    ) {
        let key = scorecardRemediationKey(repository)
        let promptContext = agentContractPromptContext(repository)
        let repositoryRoot = repository.root
        let socketPath = settings.chau7Endpoint
        let agentCommand = settings.chau7AgentCommand
        chau7LaunchStatusByKey[key] = .preparingKit

        Task {
            let nextState: Chau7ContractLaunchState
            do {
                let kitInstall = try await Task.detached(priority: .utility) {
                    try AethymeAgentContractKitInstaller.install(in: repositoryRoot)
                }.value
                let requiredKitFiles = AethymeAgentContractKitInstaller.requiredBaseRelativePaths(
                    install: kitInstall
                )
                await MainActor.run {
                    chau7LaunchStatusByKey[key] = .verifyingKit(
                        "Waiting for \(kitInstall.relativePath) base kit files to be readable."
                    )
                }
                try await Task.detached(priority: .utility) {
                    try AethymeAgentContractKitInstaller.waitForInstalledFiles(
                        in: repositoryRoot,
                        relativePaths: requiredKitFiles
                    )
                }.value
                let kitStatus = kitInstall.installed
                    ? "Installed \(kitInstall.relativePath)."
                    : "Reused \(kitInstall.relativePath)."
                await MainActor.run {
                    chau7LaunchStatusByKey[key] = .kitReady(
                        "\(kitStatus) Verified \(requiredKitFiles.count) local kit file\(requiredKitFiles.count == 1 ? "" : "s")."
                    )
                }
                try await Task.sleep(nanoseconds: 150_000_000)

                let prompt = RepositoryScorecardRemediationPrompts.remediationPrompt(
                    repository: promptContext,
                    remote: repository.gitRemoteKey ?? repository.gitRemoteOriginUrl,
                    report: report,
                    kit: kitInstall.promptContext
                )
                await MainActor.run {
                    chau7LaunchStatusByKey[key] = .openingChau7(
                        "Opening Chau7 after local Aethyme kit verification; the Scorecard remediation prompt will be sent only after Chau7 accepts the launch."
                    )
                }
                let request = Chau7AgentLaunchRequest(
                    socketPath: socketPath,
                    repositoryRoot: repositoryRoot,
                    agentCommand: agentCommand,
                    prompt: prompt
                )
                let result = try await Chau7AgentLauncher.launch(request)
                let promptStatus = result.promptStatus ?? "unknown"
                let submitDetail = result.submitConfirmation.map { " Submit: \($0)." } ?? ""
                let kitDetail = kitInstall.installed
                    ? " Kit installed: \(kitInstall.relativePath)."
                    : " Kit ready: \(kitInstall.relativePath)."
                let excludeDetail = kitInstall.excludeUpdated ? " `.aethyme/` excluded locally." : ""
                let detail = "\(result.summary) Prompt: \(promptStatus).\(kitDetail)\(excludeDetail)\(submitDetail)"
                nextState = .launched(detail, warning: promptStatus != "sent")
            } catch {
                nextState = .failed(error.localizedDescription)
            }
            await MainActor.run {
                chau7LaunchStatusByKey[key] = nextState
            }
        }
    }

    private func showScorecardWorkflowPreview(_ repository: RepositorySummary) {
        let rootURL = URL(fileURLWithPath: repository.root, isDirectory: true)
            .standardizedFileURL
        let workflowURL = rootURL.appendingPathComponent(
            RepositoryScorecardWorkflowWriter.relativePath,
            isDirectory: false
        )
        scorecardWorkflowPreview = ScorecardWorkflowPreview(
            id: repository.root,
            repositoryName: repository.name,
            repositoryRoot: repository.root,
            relativePath: RepositoryScorecardWorkflowWriter.relativePath,
            absolutePath: workflowURL.path,
            contents: RepositoryScorecardWorkflowWriter.workflowContents
        )
    }

    private func confirmScorecardWorkflowWrite(_ preview: ScorecardWorkflowPreview) {
        scorecardWorkflowPreview = nil
        addScorecardGitHubAction(repoRoot: preview.repositoryRoot)
    }

    private func addScorecardGitHubAction(repoRoot root: String) {
        scorecardWorkflowWritingRoots.insert(root)
        scorecardWorkflowStatusByRoot[root] = nil
        scorecardWorkflowErrorsByRoot[root] = nil

        Task {
            do {
                let result = try await Task.detached(priority: .utility) {
                    try RepositoryScorecardWorkflowWriter.write(in: root)
                }.value
                await MainActor.run {
                    scorecardWorkflowWritingRoots.remove(root)
                    scorecardWorkflowStatusByRoot[root] = result.created
                        ? "Created \(result.relativePath) with least-permission Scorecard defaults."
                        : "\(result.relativePath) already exists; left it unchanged."
                    scorecardWorkflowErrorsByRoot[root] = nil
                }
            } catch {
                await MainActor.run {
                    scorecardWorkflowWritingRoots.remove(root)
                    scorecardWorkflowStatusByRoot[root] = nil
                    scorecardWorkflowErrorsByRoot[root] = error.localizedDescription
                }
            }
        }
    }

    private func scorecardWorkflowPreviewSheet(_ preview: ScorecardWorkflowPreview) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: "plus.rectangle.on.folder")
                    .foregroundStyle(AetowerDesign.Status.warning)
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Add Scorecard GitHub Action")
                        .font(AetowerDesign.Typography.sectionTitle)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text("Review the exact workflow file before Aetower writes it into \(preview.repositoryName).")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            AetowerSurface(level: .warning, padding: AetowerDesign.Spacing.md) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text(preview.relativePath)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text(preview.absolutePath)
                        .font(AetowerDesign.Typography.compactData(size: 10))
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .textSelection(.enabled)
                        .lineLimit(2)
                        .truncationMode(.middle)
                    Text("If this file already exists, Aetower leaves it unchanged.")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                }
            }

            AetowerSurface(level: .card, padding: AetowerDesign.Spacing.md) {
                ScrollView {
                    Text(preview.contents)
                        .font(AetowerDesign.Typography.compactData(size: 11))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .frame(minHeight: 260, idealHeight: 320, maxHeight: 420)

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Spacer(minLength: AetowerDesign.Spacing.md)
                Button("Cancel", role: .cancel) {
                    scorecardWorkflowPreview = nil
                }
                Button {
                    confirmScorecardWorkflowWrite(preview)
                } label: {
                    Label("Write workflow", systemImage: "square.and.pencil")
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
            }
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.xxl)
        .frame(minWidth: 640, idealWidth: 720, maxWidth: 820, minHeight: 520, idealHeight: 580)
    }

    private func agentContractLaunchPromptKind(
        _ contract: StorageAgentContractCoverageModel
    ) -> AgentContractLaunchPromptKind {
        contract.present ? .reconcile : .generation
    }

    private func scorecardRemediationKey(_ repository: RepositorySummary) -> String {
        "\(repository.id)::scorecard::remediation"
    }

    private func scorecardRemediationPrompt(
        _ repository: RepositorySummary,
        report: RepositoryScorecardReportModel
    ) -> String {
        RepositoryScorecardRemediationPrompts.remediationPrompt(
            repository: agentContractPromptContext(repository),
            remote: repository.gitRemoteKey ?? repository.gitRemoteOriginUrl,
            report: report
        )
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

    private func repositorySupplyChainReadiness(_ repository: RepositorySummary) -> some View {
        let report = state.repositoryScorecardReportsByRoot[repository.root]
        let isLoading = state.repositoryScorecardLoadingRoots.contains(repository.root)
        let error = state.repositoryScorecardErrorsByRoot[repository.root]
        return AetowerSection("Supply-chain readiness", subtitle: "OpenSSF Scorecard checks for this GitHub repository") {
            AetowerSurface(level: .card, padding: AetowerDesign.Spacing.md) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                        Image(systemName: "shield.lefthalf.filled")
                            .font(AetowerDesign.Typography.sectionTitle)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                            .frame(width: AetowerDesign.Size.iconSlot)
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                            Text("OpenSSF Scorecard")
                                .font(AetowerDesign.Typography.sectionTitle)
                                .foregroundStyle(AetowerDesign.Ink.primary)
                            Text(scorecardStatusDetail(report: report, isLoading: isLoading, error: error))
                                .font(AetowerDesign.Typography.caption)
                                .foregroundStyle(AetowerDesign.Ink.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        Spacer(minLength: AetowerDesign.Spacing.md)
                        if isLoading {
                            ProgressView()
                                .controlSize(.small)
                        }
                        AetowerBadge(
                            scorecardStateLabel(report: report, isLoading: isLoading, error: error),
                            tone: scorecardStateTone(report: report, isLoading: isLoading, error: error)
                        )
                        Button {
                            state.runRepositoryScorecard(repoRoot: repository.root, mode: "auto", refresh: false)
                        } label: {
                            Label("Run Scorecard", systemImage: "play.fill")
                        }
                        Button {
                            state.runRepositoryScorecard(
                                repoRoot: repository.root,
                                mode: report?.requestedMode ?? "auto",
                                refresh: true
                            )
                        } label: {
                            Label("Refresh", systemImage: "arrow.clockwise")
                        }
                        .disabled(report == nil)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(isLoading)

                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 150), spacing: AetowerDesign.Spacing.md)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.md
                    ) {
                        scorecardFact(
                            "Score",
                            value: scorecardScoreLabel(report),
                            detail: "current Scorecard score",
                            icon: "number.circle",
                            tone: scorecardScoreTone(report)
                        )
                        scorecardFact(
                            "Source",
                            value: scorecardSourceLabel(report),
                            detail: "API or CLI mode",
                            icon: "antenna.radiowaves.left.and.right",
                            tone: AetowerDesign.Status.neutral
                        )
                        scorecardFact(
                            "Last scan",
                            value: scorecardLastScanLabel(report),
                            detail: report?.cacheHit == true ? "cached result" : "latest request",
                            icon: "clock",
                            tone: report == nil ? AetowerDesign.Status.neutral : AetowerDesign.Status.ready
                        )
                        scorecardFact(
                            "Failed",
                            value: "\(report?.failedChecks.count ?? 0)",
                            detail: "checks below passing",
                            icon: "xmark.seal",
                            tone: (report?.failedChecks.isEmpty ?? true) ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
                        )
                        scorecardFact(
                            "Unavailable",
                            value: "\(report?.unavailableChecks.count ?? 0)",
                            detail: "checks not measured",
                            icon: "questionmark.diamond",
                            tone: (report?.unavailableChecks.isEmpty ?? true) ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
                        )
                    }

                    if isLoading, report == nil {
                        AetowerInfoBanner(
                            "Scorecard is running for this repository. The default repository scan stays read-only and does not call the Scorecard API or CLI.",
                            title: "Scanning",
                            systemImage: "hourglass",
                            tone: AetowerDesign.Status.neutral,
                            level: .quiet
                        )
                    } else if let error {
                        AetowerInfoBanner(
                            error,
                            title: "Failed",
                            systemImage: "exclamationmark.triangle",
                            tone: AetowerDesign.Status.error,
                            level: .critical
                        )
                    } else if let report {
                        scorecardRemediationTools(repository, report: report)
                        scorecardReportFindings(report)
                    } else {
                        AetowerInfoBanner(
                            "Run Scorecard when you want supply-chain security checks. It supports the public OpenSSF API for public GitHub repos and the local Scorecard CLI when available.",
                            title: "Not scanned",
                            systemImage: "shield",
                            tone: AetowerDesign.Status.neutral,
                            level: .quiet
                        )
                    }
                }
            }
        }
    }

    private func scorecardFact(
        _ label: String,
        value: String,
        detail: String,
        icon: String,
        tone: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: icon)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                Text(label.uppercased())
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
            }
            Text(value)
                .font(AetowerDesign.Typography.metricValue(size: 20, weight: .semibold))
                .foregroundStyle(AetowerDesign.Ink.primary)
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(detail)
                .font(AetowerDesign.Typography.metadata)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(2)
        }
        .frame(maxWidth: .infinity, minHeight: 74, alignment: .topLeading)
    }

    private func scorecardRemediationTools(
        _ repository: RepositorySummary,
        report: RepositoryScorecardReportModel
    ) -> some View {
        let key = scorecardRemediationKey(repository)
        let launchState = chau7LaunchStatusByKey[key]
        let categories = RepositoryScorecardRemediationPrompts.categories(for: report)
        let copied = copiedAgentPromptKey == key
        let workflowIsWriting = scorecardWorkflowWritingRoots.contains(repository.root)
        let workflowStatus = scorecardWorkflowStatusByRoot[repository.root]
        let workflowError = scorecardWorkflowErrorsByRoot[repository.root]
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Divider()
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: "wrench.and.screwdriver")
                    .foregroundStyle(report.failedChecks.isEmpty ? AetowerDesign.Status.neutral : AetowerDesign.Tone.energy)
                    .frame(width: AetowerDesign.Size.iconSlot)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        Text("Aethyme remediation")
                            .font(AetowerDesign.Typography.controlLabel)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                        AetowerBadge(
                            report.failedChecks.isEmpty ? "No failed checks" : "\(report.failedChecks.count) failed",
                            tone: report.failedChecks.isEmpty ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
                        )
                        if copied {
                            AetowerBadge("Copied", tone: AetowerDesign.Status.ready)
                        }
                    }
                    Text("Maps failed checks into local PR-ready edits and remote GitHub settings checklist items.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                Button("Copy prompt") {
                    copy(scorecardRemediationPrompt(repository, report: report))
                    copiedAgentPromptKey = key
                }
                Button {
                    launchScorecardRemediationInChau7(repository, report: report)
                } label: {
                    Label("Ask Chau7 to improve Scorecard posture", systemImage: "terminal")
                }
                .disabled(report.failedChecks.isEmpty || launchState?.isLaunching == true)
                Button {
                    showScorecardWorkflowPreview(repository)
                } label: {
                    Label("Preview Scorecard workflow", systemImage: "plus.rectangle.on.folder")
                }
                .disabled(workflowIsWriting)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            if workflowIsWriting {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Writing \(RepositoryScorecardWorkflowWriter.relativePath)")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }
            } else if let workflowStatus {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: "checkmark.circle")
                        .foregroundStyle(AetowerDesign.Status.ready)
                    AetowerBadge("Workflow ready", tone: AetowerDesign.Status.ready)
                    Text(workflowStatus)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(2)
                }
            } else if let workflowError {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(AetowerDesign.Status.error)
                    AetowerBadge("Workflow failed", tone: AetowerDesign.Status.error)
                    Text(workflowError)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(2)
                }
            }

            if categories.isEmpty {
                Text("No Aethyme remediation category is active because this Scorecard payload has no failed checks.")
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
            } else {
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.xs) {
                    ForEach(categories, id: \.rawValue) { category in
                        AetowerBadge(category.rawValue, tone: AetowerDesign.Status.neutral)
                    }
                }
            }

            if let launchState {
                HStack(spacing: AetowerDesign.Spacing.xs) {
                    Image(systemName: launchState.icon)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                    AetowerBadge(launchState.label, tone: launchState.tone)
                    Text(launchState.detail)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(2)
                }
            }
        }
    }

    private func scorecardReportFindings(_ report: RepositoryScorecardReportModel) -> some View {
        let failed = Array(report.failedChecks.prefix(5))
        let unavailable = Array(report.unavailableChecks.prefix(5))
        let recommendations = Array(report.recommendations.prefix(3))
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            if let message = scorecardReportStateMessage(report) {
                AetowerInfoBanner(
                    message,
                    title: scorecardStateLabel(report: report, isLoading: false, error: nil),
                    systemImage: scorecardStateIcon(report),
                    tone: scorecardStateTone(report: report, isLoading: false, error: nil),
                    level: scorecardReportStateLevel(report)
                )
            }

            scorecardCheckGroup(
                "Failed checks",
                checks: failed,
                empty: "No failed checks reported.",
                tone: AetowerDesign.Status.warning
            )
            scorecardCheckGroup(
                "Unavailable checks",
                checks: unavailable,
                empty: "No unavailable checks reported.",
                tone: AetowerDesign.Status.neutral
            )
            scorecardRecommendationGroup(recommendations)

            if !report.warnings.isEmpty {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("WARNINGS")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                    ForEach(Array(report.warnings.prefix(3)), id: \.self) { warning in
                        HStack(alignment: .top, spacing: AetowerDesign.Spacing.xs) {
                            Image(systemName: "exclamationmark.triangle")
                                .foregroundStyle(AetowerDesign.Status.warning)
                            Text(warning)
                                .font(AetowerDesign.Typography.caption)
                                .foregroundStyle(AetowerDesign.Ink.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
        }
    }

    private func scorecardCheckGroup(
        _ title: String,
        checks: [RepositoryScorecardCheckModel],
        empty: String,
        tone: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Text(title.uppercased())
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                AetowerBadge("\(checks.count)", tone: tone)
            }
            if checks.isEmpty {
                Text(empty)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            } else {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    ForEach(Array(checks.enumerated()), id: \.offset) { pair in
                        scorecardCheckRow(pair.element)
                    }
                }
            }
        }
    }

    private func scorecardCheckRow(_ check: RepositoryScorecardCheckModel) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "seal")
                .foregroundStyle(AetowerDesign.Ink.tertiary)
                .frame(width: AetowerDesign.Size.iconSlot)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text(check.name)
                    .font(AetowerDesign.Typography.controlLabel)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                if !check.reason.isEmpty {
                    Text(check.reason)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: AetowerDesign.Spacing.sm)
            AetowerBadge(scorecardCheckScoreLabel(check), tone: scorecardCheckTone(check))
        }
        .padding(.vertical, AetowerDesign.Spacing.xxs)
    }

    private func scorecardRecommendationGroup(
        _ recommendations: [RepositoryScorecardRecommendationModel]
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Text("TOP RECOMMENDATIONS")
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                AetowerBadge("\(recommendations.count)", tone: AetowerDesign.Tone.energy)
            }
            if recommendations.isEmpty {
                Text("No recommendations reported.")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            } else {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    ForEach(Array(recommendations.enumerated()), id: \.offset) { pair in
                        scorecardRecommendationRow(pair.element)
                    }
                }
            }
        }
    }

    private func scorecardRecommendationRow(
        _ recommendation: RepositoryScorecardRecommendationModel
    ) -> some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "arrow.up.forward.circle")
                .foregroundStyle(AetowerDesign.Ink.tertiary)
                .frame(width: AetowerDesign.Size.iconSlot)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.xs) {
                    Text(recommendation.title)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .fixedSize(horizontal: false, vertical: true)
                    AetowerBadge(
                        recommendation.severity.capitalized,
                        tone: scorecardSeverityTone(recommendation.severity)
                    )
                }
                Text(recommendation.detail)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text(recommendation.checkName)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .padding(.vertical, AetowerDesign.Spacing.xxs)
    }

    private func repositoryActions(_ repository: RepositorySummary) -> some View {
        AetowerSurface {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    Text("Repository actions")
                        .font(AetowerDesign.Typography.sectionTitle)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text("Fast local actions for this repository. Any repository write is shown with a preview before it runs.")
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
        return "\(repositorySummaries(from: report).filter(\.requiresAttention).count)"
    }

    private var repositoryBadgeTone: Color {
        state.storageHygieneReport == nil ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
    }

    private var attentionBadgeTone: Color {
        guard let report = state.storageHygieneReport else { return AetowerDesign.Status.neutral }
        return repositorySummaries(from: report).contains(where: \.requiresAttention)
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
            repositories = repositories.filter(\.requiresAttention)
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
                    || scorecardReadinessLabel(repository).localizedCaseInsensitiveContains(query)
                    || scorecardReadinessDetail(repository).localizedCaseInsensitiveContains(query)
                    || (repository.scorecardReport?.recommendations.contains {
                        $0.title.localizedCaseInsensitiveContains(query)
                            || $0.checkName.localizedCaseInsensitiveContains(query)
                            || $0.severity.localizedCaseInsensitiveContains(query)
                    } ?? false)
                    || (repository.scorecardReport?.failedChecks.contains {
                        $0.name.localizedCaseInsensitiveContains(query)
                            || $0.outcome.localizedCaseInsensitiveContains(query)
                            || $0.reason.localizedCaseInsensitiveContains(query)
                    } ?? false)
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
        let scorecardReport = state.repositoryScorecardReportsByRoot[root]
        let baseCaveats = footprint?.caveats ?? [
            "Indexed from Git repository discovery. No tracked storage artifacts were found in the bounded hygiene scan."
        ]
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
            caveats: repositoryCaveats(base: baseCaveats, scorecardReport: scorecardReport),
            violationCount: violations,
            reviewItemCount: items.filter { $0.safety != "safe" }.count,
            safeItemCount: items.filter { $0.safety == "safe" }.count,
            staleItemCount: items.filter(\.stale).count,
            liveSessionCount: live.sessionCount,
            liveEntityCount: live.entityCount,
            liveMemoryBytes: live.memoryBytes,
            liveCPUPercent: live.cpuPercent,
            agentArtifactBytes: agent.artifactBytes,
            agentCount: agent.agentCount,
            scorecardReport: scorecardReport
        )
    }

    private func repositoryCaveats(
        base: [String],
        scorecardReport: RepositoryScorecardReportModel?
    ) -> [String] {
        var caveats = base
        if let scorecardReport {
            if scorecardReport.status != "ok" {
                caveats.append(
                    "OpenSSF Scorecard is unavailable for this repository (\(scorecardReport.status)); supply-chain attention is unchanged."
                )
            } else if scorecardReport.score == nil {
                caveats.append(
                    "OpenSSF Scorecard completed without an aggregate score; supply-chain attention is unchanged."
                )
            }
        } else {
            caveats.append(
                "OpenSSF Scorecard has not been run for this repository; supply-chain attention is unchanged."
            )
        }
        return caveats
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

    private func scorecardReadinessLabel(_ repository: RepositorySummary) -> String {
        guard let report = repository.scorecardReport else { return "Not scanned" }
        guard report.status == "ok" else { return "Unavailable" }
        if repository.scorecardCriticalFailureCount > 0 {
            return "\(repository.scorecardCriticalFailureCount) critical"
        }
        guard let score = report.score else { return "No score" }
        if score < 5 { return "High attention" }
        if score <= 7 { return "Medium" }
        return "Low"
    }

    private func scorecardReadinessDetail(_ repository: RepositorySummary) -> String {
        guard let report = repository.scorecardReport else {
            return "Scorecard unavailable: attention unchanged"
        }
        guard report.status == "ok" else {
            return "Scorecard unavailable: \(scorecardStateLabel(report: report, isLoading: false, error: nil).lowercased())"
        }
        let score = scorecardScoreLabel(report)
        let failed = report.failedChecks.count
        let unavailable = report.unavailableChecks.count
        if repository.scorecardCriticalFailureCount > 0 {
            return "\(repository.scorecardCriticalFailureCount) critical failed check\(repository.scorecardCriticalFailureCount == 1 ? "" : "s") · score \(score)"
        }
        if let numericScore = report.score {
            if numericScore < 5 {
                return "Score \(score) raises repository attention"
            }
            if numericScore <= 7 {
                return "Score \(score) adds medium repository attention"
            }
            return "Score \(score) adds low repository attention"
        }
        return "\(failed) failed · \(unavailable) unavailable · no aggregate score"
    }

    private func scorecardReadinessTone(_ repository: RepositorySummary) -> Color {
        if repository.scorecardAttentionScore >= 10 { return AetowerDesign.Status.error }
        if repository.scorecardAttentionScore >= 5 { return AetowerDesign.Status.warning }
        if repository.scorecardReport?.status == "ok" { return AetowerDesign.Status.ready }
        return AetowerDesign.Status.neutral
    }

    private func scorecardScoreLabel(_ report: RepositoryScorecardReportModel?) -> String {
        RepositoryScorecardPresentation.scoreLabel(report)
    }

    private func scorecardScoreTone(_ report: RepositoryScorecardReportModel?) -> Color {
        guard let score = report?.score, score >= 0 else { return AetowerDesign.Status.neutral }
        if score >= 7.0 { return AetowerDesign.Status.ready }
        if score >= 5.0 { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.error
    }

    private func scorecardSourceLabel(_ report: RepositoryScorecardReportModel?) -> String {
        RepositoryScorecardPresentation.sourceLabel(report)
    }

    private func scorecardLastScanLabel(_ report: RepositoryScorecardReportModel?) -> String {
        guard let report else { return "Never" }
        let millis: UInt64?
        if let cachedAtMillis = report.cachedAtMillis {
            millis = cachedAtMillis
        } else if report.capturedAtMillis > 0 {
            millis = report.capturedAtMillis
        } else {
            millis = nil
        }
        guard let millis else { return "Unknown" }
        let date = Date(timeIntervalSince1970: Double(millis) / 1000.0)
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    private func scorecardStateLabel(
        report: RepositoryScorecardReportModel?,
        isLoading: Bool,
        error: String?
    ) -> String {
        RepositoryScorecardPresentation.stateLabel(
            report: report,
            isLoading: isLoading,
            error: error
        )
    }

    private func scorecardStateTone(
        report: RepositoryScorecardReportModel?,
        isLoading: Bool,
        error: String?
    ) -> Color {
        if isLoading { return AetowerDesign.Tone.cpu }
        if let error, !error.isEmpty { return AetowerDesign.Status.error }
        guard let report else { return AetowerDesign.Status.neutral }
        switch report.status {
        case "ok":
            return scorecardScoreTone(report)
        case "auth_required", "unsupported", "timeout":
            return AetowerDesign.Status.warning
        default:
            return AetowerDesign.Status.error
        }
    }

    private func scorecardStatusDetail(
        report: RepositoryScorecardReportModel?,
        isLoading: Bool,
        error: String?
    ) -> String {
        RepositoryScorecardPresentation.statusDetail(
            report: report,
            isLoading: isLoading,
            error: error
        )
    }

    private func scorecardReportStateMessage(_ report: RepositoryScorecardReportModel) -> String? {
        RepositoryScorecardPresentation.reportStateMessage(report)
    }

    private func scorecardReportStateLevel(_ report: RepositoryScorecardReportModel) -> AetowerSurfaceLevel {
        switch report.status {
        case "auth_required", "unsupported", "timeout", "rate_limited", "source_unavailable":
            return .warning
        case "ok":
            return .quiet
        default:
            return .critical
        }
    }

    private func scorecardStateIcon(_ report: RepositoryScorecardReportModel) -> String {
        switch report.status {
        case "auth_required":
            return "person.badge.key"
        case "unsupported":
            return "network.slash"
        case "timeout":
            return "timer"
        case "ok":
            return "checkmark.seal"
        default:
            return "exclamationmark.triangle"
        }
    }

    private func scorecardCheckScoreLabel(_ check: RepositoryScorecardCheckModel) -> String {
        RepositoryScorecardPresentation.checkScoreLabel(check)
    }

    private func scorecardCheckTone(_ check: RepositoryScorecardCheckModel) -> Color {
        guard let score = check.score, score >= 0 else { return AetowerDesign.Status.neutral }
        if score >= 7.0 { return AetowerDesign.Status.ready }
        if score >= 5.0 { return AetowerDesign.Status.warning }
        return AetowerDesign.Status.error
    }

    private func scorecardSeverityTone(_ severity: String) -> Color {
        switch severity.lowercased() {
        case "critical", "high", "error":
            return AetowerDesign.Status.error
        case "medium", "warning":
            return AetowerDesign.Status.warning
        case "low", "info":
            return AetowerDesign.Status.ready
        default:
            return AetowerDesign.Status.neutral
        }
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
        let agentContractCoverage = repository.agentContractCoverage.map {
            "- \($0.label): \($0.coveragePercent)% \($0.status) | \($0.path) | \($0.detail)"
        }.joined(separator: "\n")
        let operatingContractIssues = repository.agentGuidanceIssues.prefix(8).map {
            "- \($0.severity.uppercased()) | \($0.path) | \($0.title): \($0.detail)"
        }.joined(separator: "\n")
        let scorecardFailedChecks = repository.scorecardReport?.failedChecks.prefix(8).map {
            "- \(scorecardCheckScoreLabel($0)) | \($0.name): \($0.reason)"
        }.joined(separator: "\n") ?? "- Scorecard not available"
        let scorecardRecommendations = repository.scorecardReport?.recommendations.prefix(5).map {
            "- \($0.severity.uppercased()) | \($0.checkName): \($0.title) - \($0.detail)"
        }.joined(separator: "\n") ?? "- Scorecard not available"
        let caveats = repository.caveats.prefix(8).map {
            "- \($0)"
        }.joined(separator: "\n")
        let topArtifactFolders = repository.topArtifactFolders.prefix(5).map {
            "- \(formatBytes($0.sizeBytes)) | \($0.cleanupTier) | \($0.path)"
        }.joined(separator: "\n")

        var lines: [String] = [
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
            "- Scorecard readiness: \(scorecardReadinessLabel(repository))",
            "- Scorecard detail: \(scorecardReadinessDetail(repository))",
            "- Scorecard attention contribution: \(String(format: "%.1f", repository.scorecardAttentionScore))",
            "",
            "Agent contract coverage:",
            agentContractCoverage,
            "",
            "Operating contract issues:",
            operatingContractIssues,
            "",
            "Scorecard failed checks:",
            scorecardFailedChecks,
            "",
            "Scorecard recommendations:",
            scorecardRecommendations,
            "",
            "Caveats:",
            caveats,
            "",
            "Top artifact folders:",
            topArtifactFolders,
        ]
        lines.append("")
        lines.append(
            "Recommended next step: inspect large rebuildable folders first, then decide whether cleanup cost is acceptable for the current branch/session."
        )
        return lines.filter { !$0.isEmpty }.joined(separator: "\n")
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
