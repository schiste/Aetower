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

private enum RepositoryDetailTab: String, CaseIterable, Identifiable {
    case actions
    case storage
    case contracts
    case scorecard
    case git
    case live

    var id: String { rawValue }

    var label: String {
        switch self {
        case .actions: return "Actions"
        case .storage: return "Storage"
        case .contracts: return "Contracts"
        case .scorecard: return "Scorecard"
        case .git: return "Git"
        case .live: return "Live"
        }
    }

    var systemImage: String {
        switch self {
        case .actions: return "bolt.circle"
        case .storage: return "internaldrive"
        case .contracts: return "doc.text"
        case .scorecard: return "shield.lefthalf.filled"
        case .git: return "arrow.triangle.branch"
        case .live: return "waveform.path.ecg"
        }
    }
}

private enum RepositorySort: String, CaseIterable, Identifiable {
    case attention
    case size
    case growth
    case artifacts
    case aiSpend
    case name

    var id: String { rawValue }

    var label: String {
        switch self {
        case .attention: return "Attention"
        case .size: return "Size"
        case .growth: return "Growth"
        case .artifacts: return "Artifacts"
        case .aiSpend: return "AI spend"
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

private struct RepositoryPrimaryAction {
    let title: String
    let detail: String
    let systemImage: String
    let tone: Color
    let kind: RepositoryOptimizationActionKind
}

private enum RepositoryRowLayout {
    static let height: CGFloat = 34
    static let rowSpacing: CGFloat = AetowerDesign.Spacing.xxs
    static let columnSpacing: CGFloat = AetowerDesign.Spacing.xs + AetowerDesign.Spacing.xxs
    static let selectionWidth: CGFloat = 18
    static let iconWidth: CGFloat = 16
    static let statusWidth: CGFloat = 92
    static let gitWidth: CGFloat = 116
    static let inventoryWidth: CGFloat = 122
    static let attentionWidth: CGFloat = 220
    static let actionWidth: CGFloat = 166
}

/// Precomputed, value-only display data for one cockpit row. The parent builds
/// this from its existing helpers so the row view stays a pure function of its
/// inputs — no reach-back into AppState.
private struct RepositoryRowModel {
    let id: String
    let name: String
    let shortPath: String
    let statusLabel: String
    let statusTone: Color
    let gitOverview: String
    let gitDetail: String
    let inventoryLabel: String
    let inventoryTone: Color
    let inventoryDetail: String
    let attentionTitle: String
    let attentionDetail: String
    let attentionTone: Color
    let aiUsageLabel: String?
    let aiUsageHelp: String
    let projectTone: Color?
    let projectName: String?
    let primaryAction: RepositoryPrimaryAction
    let inventoryNeedsAttention: Bool
    let isSelected: Bool
}

/// One repository row, extracted from the 4,200-line RepositoryView so the
/// cockpit list is composed of child View structs rather than methods on one
/// god-object. Renders from a value model and reports interactions through
/// closures; it never touches AppState directly.
private struct RepositoryRow: View {
    let model: RepositoryRowModel
    let onToggleSelect: () -> Void
    let onPrimaryAction: () -> Void
    let onReveal: () -> Void
    let onCopyPath: () -> Void
    let onCopyBrief: () -> Void

    var body: some View {
        AetowerOperationalListRow(
            tone: rowTone,
            isSelected: model.isSelected,
            minHeight: RepositoryRowLayout.height
        ) {
            ViewThatFits(in: .horizontal) {
                wideRow
                compactRow
            }
        }
        .contextMenu {
            Button("Reveal in Finder", action: onReveal)
            Button("Copy path", action: onCopyPath)
            Button("Copy optimization brief", action: onCopyBrief)
        }
        .help(model.attentionDetail)
    }

    private var wideRow: some View {
        HStack(alignment: .center, spacing: RepositoryRowLayout.columnSpacing) {
            selectionToggle
                .frame(width: RepositoryRowLayout.selectionWidth)
            Image(systemName: "folder.badge.gearshape")
                .font(AetowerDesign.Typography.compactData(size: 11, weight: .medium))
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .frame(width: RepositoryRowLayout.iconWidth)
            nameCell
                .frame(maxWidth: .infinity, alignment: .leading)
            AetowerBadge(model.statusLabel, tone: model.statusTone)
                .frame(width: RepositoryRowLayout.statusWidth, alignment: .leading)
            metricText(model.gitOverview)
                .frame(width: RepositoryRowLayout.gitWidth, alignment: .center)
            inventoryCell
                .frame(width: RepositoryRowLayout.inventoryWidth, alignment: .center)
            attentionCell
                .frame(width: RepositoryRowLayout.attentionWidth, alignment: .leading)
            primaryButton
                .frame(width: RepositoryRowLayout.actionWidth, alignment: .trailing)
        }
    }

    private var compactRow: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                selectionToggle
                nameCell
                Spacer(minLength: AetowerDesign.Spacing.sm)
                AetowerBadge(model.statusLabel, tone: model.statusTone)
            }
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                cellText("Git", detail: model.gitOverview)
                inventoryCell
                Spacer(minLength: AetowerDesign.Spacing.sm)
                primaryButton
            }
            attentionCell
        }
    }

    private var selectionToggle: some View {
        Button(action: onToggleSelect) {
            Image(systemName: model.isSelected ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(model.isSelected ? AetowerDesign.Tone.cpu : AetowerDesign.Ink.tertiary)
        }
        .buttonStyle(.plain)
        .help(model.isSelected ? "Deselect" : "Select for a bulk action")
    }

    private var nameCell: some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            NavigationLink(value: model.id) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text(model.name)
                        .font(AetowerDesign.Typography.caption.weight(.medium))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Text(model.shortPath)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .buttonStyle(.plain)

            if let projectTone = model.projectTone {
                miniSignal("Project", tone: projectTone)
                    .help(model.projectName ?? "")
            }
            if let aiUsageLabel = model.aiUsageLabel {
                miniSignal(aiUsageLabel, tone: AetowerDesign.Tone.energy)
                    .help(model.aiUsageHelp)
            }
        }
    }

    private var inventoryCell: some View {
        compactSignal(
            model.inventoryLabel,
            systemImage: model.inventoryNeedsAttention ? "exclamationmark.triangle.fill" : "checkmark.circle",
            tone: model.inventoryTone
        )
        .help(model.inventoryDetail)
    }

    private var attentionCell: some View {
        AetowerBadge(
            model.attentionTitle,
            systemImage: model.inventoryNeedsAttention ? "exclamationmark.triangle.fill" : "target",
            tone: model.attentionTone,
            style: .outline
        )
        .help(model.attentionDetail)
    }

    private var primaryButton: some View {
        Button(action: onPrimaryAction) {
            Label(model.primaryAction.title, systemImage: model.primaryAction.systemImage)
                .lineLimit(1)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .tint(model.primaryAction.tone)
        .help(model.primaryAction.detail)
    }

    private var rowTone: Color {
        if model.inventoryNeedsAttention { return model.attentionTone }
        return model.statusTone
    }

    private func compactSignal(_ label: String, systemImage: String, tone: Color) -> some View {
        AetowerBadge(label, systemImage: systemImage, tone: tone, style: .outline)
    }

    private func miniSignal(_ label: String, tone: Color) -> some View {
        AetowerBadge(label, tone: tone, style: .soft)
    }

    private func metricText(_ value: String) -> some View {
        Text(value)
            .font(AetowerDesign.Typography.compactData(size: 11))
            .foregroundStyle(AetowerDesign.Ink.secondary)
            .lineLimit(1)
            .truncationMode(.middle)
    }

    private func cellText(_ value: String, detail: String) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            Text(value)
                .font(AetowerDesign.Typography.caption.weight(.semibold))
                .foregroundStyle(AetowerDesign.Ink.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            if !detail.isEmpty {
                Text(detail)
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
    }
}

/// Shared provider metric tile (GitHub + Cloudflare cards, scorecard). File
/// scope so both RepositoryView and the extracted provider cards call it.
private func repositoryProjectProviderMetric(_ label: String, _ value: String) -> some View {
    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
        Text(label.uppercased())
            .font(AetowerDesign.Typography.metadata)
            .foregroundStyle(AetowerDesign.Ink.tertiary)
        Text(value)
            .font(AetowerDesign.Typography.caption.weight(.semibold))
            .foregroundStyle(AetowerDesign.Ink.primary)
            .lineLimit(1)
            .truncationMode(.middle)
    }
}

/// GitHub provider status tone — pure function of the status/error/loading
/// state. Shared between the extracted card and RepositoryView's nav rail.
private func repositoryGitHubProviderTone(
    _ status: RepositoryGitHubProviderStatusModel?,
    error: String? = nil,
    isLoading: Bool = false
) -> Color {
    if isLoading { return AetowerDesign.Tone.cpu }
    if error != nil { return AetowerDesign.Status.warning }
    guard let status else { return AetowerDesign.Tone.cpu }
    if status.failedLatestCIOnDefaultBranch {
        return AetowerDesign.Status.error
    }
    if status.staleOpenPullRequestCount() > 0 { return AetowerDesign.Status.warning }
    switch status.status {
    case "ok":
        return AetowerDesign.Status.ready
    case "warning", "auth_needed":
        return AetowerDesign.Status.warning
    case "failed":
        return AetowerDesign.Status.error
    default:
        return AetowerDesign.Status.neutral
    }
}

/// GitHub provider status card, extracted from the RepositoryView god-object.
/// Renders from the status model plus the loading/error flags the parent reads
/// from AppState, and reports the workflow re-run through a closure — it never
/// touches AppState directly.
private struct GitHubProviderCard: View {
    let status: RepositoryGitHubProviderStatusModel?
    let isLoading: Bool
    let error: String?
    let onRerunWorkflow: (UInt64) -> Void

    var body: some View {
        AetowerSurface(level: statusLevel, padding: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                    AetowerBadge(
                        statusLabel,
                        systemImage: "chevron.left.forwardslash.chevron.right",
                        tone: repositoryGitHubProviderTone(status, error: error, isLoading: isLoading)
                    )
                    if isLoading {
                        ProgressView()
                            .controlSize(.small)
                    }
                    Spacer(minLength: AetowerDesign.Spacing.md)
                    Text(capturedLabel)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }

                if let status {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 130), spacing: AetowerDesign.Spacing.sm)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.sm
                    ) {
                        repositoryProjectProviderMetric("Open PRs", "\(status.openPrCount)")
                        repositoryProjectProviderMetric("Workflow", workflowLabel(status))
                        repositoryProjectProviderMetric("Checks", status.latestCheckState.capitalized)
                    }

                    lists(status)

                    ForEach(status.warnings.prefix(2), id: \.self) { warning in
                        Text(warning)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Status.warning)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                } else if let error {
                    Text(error)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Status.warning)
                        .fixedSize(horizontal: false, vertical: true)
                } else {
                    Text("Not refreshed")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }
            }
        }
    }

    private var statusLevel: AetowerSurfaceLevel {
        if error != nil { return .warning }
        guard let status else { return .quiet }
        if status.failedLatestCIOnDefaultBranch { return .critical }
        if status.staleOpenPullRequestCount() > 0 { return .warning }
        switch status.status {
        case "failed":
            return .critical
        case "warning", "auth_needed", "unavailable":
            return .warning
        default:
            return .quiet
        }
    }

    private var statusLabel: String {
        if isLoading { return "Refreshing" }
        if error != nil { return "GitHub issue" }
        guard let status else { return "GitHub not refreshed" }
        if status.failedLatestCIOnDefaultBranch { return "CI failed" }
        if status.staleOpenPullRequestCount() > 0 { return "Stale PRs" }
        switch status.status {
        case "ok":
            return "GitHub ok"
        case "auth_needed":
            return "Needs auth"
        case "unavailable":
            return "Unavailable"
        case "failed":
            return "Failed"
        default:
            return "Warning"
        }
    }

    private var capturedLabel: String {
        guard let status, status.capturedAtMillis > 0 else { return "Never" }
        let date = Date(timeIntervalSince1970: Double(status.capturedAtMillis) / 1000.0)
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    private func workflowLabel(_ status: RepositoryGitHubProviderStatusModel) -> String {
        if let conclusion = status.latestWorkflowConclusion, !conclusion.isEmpty {
            return conclusion.capitalized
        }
        if let workflowStatus = status.latestWorkflowStatus, !workflowStatus.isEmpty {
            return workflowStatus.capitalized
        }
        return "Unavailable"
    }

    private func workflowRunIsFailed(_ run: RepositoryGitHubWorkflowRunModel) -> Bool {
        ["failure", "timed_out", "cancelled", "action_required", "startup_failure"]
            .contains(run.conclusion ?? "")
    }

    @ViewBuilder
    private func lists(_ status: RepositoryGitHubProviderStatusModel) -> some View {
        let latestPrs = Array(status.latestPrs.prefix(3))
        let latestRuns = Array(status.latestWorkflowRuns.prefix(2))
        if !latestPrs.isEmpty || !latestRuns.isEmpty {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                ForEach(latestPrs) { pullRequest in
                    Text("#\(pullRequest.number) \(pullRequest.title)")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                ForEach(latestRuns) { run in
                    HStack(spacing: AetowerDesign.Spacing.xs) {
                        Text("\(run.name): \(run.conclusion ?? run.status ?? "unknown")")
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                            .lineLimit(1)
                            .truncationMode(.tail)
                        if workflowRunIsFailed(run) {
                            Button {
                                onRerunWorkflow(run.id)
                            } label: {
                                Label("Re-run", systemImage: "arrow.clockwise")
                                    .labelStyle(.iconOnly)
                            }
                            .buttonStyle(.borderless)
                            .controlSize(.mini)
                            .help("Re-run this failed workflow on GitHub.")
                        }
                    }
                }
            }
        }
    }
}

private struct ScorecardWorkflowPreview: Identifiable {
    let id: String
    let repositoryName: String
    let repositoryRoot: String
    let relativePath: String
    let absolutePath: String
    let contents: String
}

private enum RepositoryCloudflareLinkKind: String, CaseIterable, Identifiable {
    case pages
    case worker

    var id: String { rawValue }

    var label: String {
        switch self {
        case .pages: return "Pages"
        case .worker: return "Worker"
        }
    }
}

private enum RepositoryCloudflareEnvironmentPreset: String, CaseIterable, Identifiable {
    case production
    case staging
    case development
    case custom

    var id: String { rawValue }

    var label: String {
        switch self {
        case .production: return "Production"
        case .staging: return "Staging"
        case .development: return "Development"
        case .custom: return "Custom"
        }
    }

    var defaultName: String {
        switch self {
        case .production: return "Production"
        case .staging: return "Staging"
        case .development: return "Development"
        case .custom: return ""
        }
    }

    var rank: Int {
        switch self {
        case .production: return 100
        case .staging: return 60
        case .development: return 20
        case .custom: return 40
        }
    }

    var pagesDeploymentEnvironment: String? {
        switch self {
        case .production:
            return "production"
        case .staging, .development:
            return "preview"
        case .custom:
            return nil
        }
    }
}

private enum RepositoryCloudflarePagesDeploymentEnvironment: String, CaseIterable, Identifiable {
    case any
    case production
    case preview

    var id: String { rawValue }

    var label: String {
        switch self {
        case .any: return "Any"
        case .production: return "Production"
        case .preview: return "Preview"
        }
    }

    var apiValue: String? {
        switch self {
        case .any: return nil
        case .production, .preview: return rawValue
        }
    }

    static func selection(for value: String?) -> Self {
        switch value {
        case "production": return .production
        case "preview": return .preview
        default: return .any
        }
    }
}

private struct RepositoryCloudflareLinkRequest: Identifiable {
    let id: String
    let repository: RepositorySummary
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
    @State private var summaryCache = RepositorySummaryCacheStore()
    @State private var sort: RepositorySort = .attention
    @State private var detailTab: RepositoryDetailTab = .actions
    @State private var searchText = ""
    @State private var debouncedSearchText = ""
    @State private var searchDebounceTask: Task<Void, Never>?
    @State private var repositoryPath: [String] = []
    @State private var copiedRepositoryID: String?
    @State private var pendingArtifactCleanup: RepositorySummary?
    @State private var selectedRepoRoots: Set<String> = []
    @State private var copiedAgentPromptKey: String?
    @State private var chau7LaunchStatusByKey: [String: Chau7ContractLaunchState] = [:]
    @State private var scorecardWorkflowWritingRoots: Set<String> = []
    @State private var scorecardWorkflowStatusByRoot: [String: String] = [:]
    @State private var scorecardWorkflowErrorsByRoot: [String: String] = [:]
    @State private var scorecardWorkflowPreview: ScorecardWorkflowPreview?
    @State private var selectedAgentContractByRepository: [String: String] = [:]
    @State private var expandedProjectSectionRepositoryID: String?
    @State private var cloudflareLinkRequest: RepositoryCloudflareLinkRequest?

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
            .overlay(alignment: .bottom) {
                VStack(spacing: AetowerDesign.Spacing.sm) {
                    if let refreshState = state.repositoryInventoryRefreshState {
                        repositoryInventoryRefreshToast(refreshState)
                            .transition(.move(edge: .bottom).combined(with: .opacity))
                    }
                    repositoryBulkPill
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                .padding(.bottom, AetowerDesign.Spacing.xl)
                .animation(AetowerDesign.Motion.smooth, value: selectedRepoRoots.count)
                .animation(AetowerDesign.Motion.quick, value: state.repositoryInventoryRefreshState)
            }
        }
        .task {
            state.ensureRepositoryInventoryResponsiveLoad(roots: repositoryScanRoots)
        }
        .onChange(of: settings.repositoryRoots) { _, roots in
            state.ensureRepositoryInventoryResponsiveLoad(roots: roots)
        }
        .onChange(of: searchText) { _, newValue in
            // Debounce so the O(n · fields) localized filter+sort runs once the
            // user pauses, not on every keystroke.
            searchDebounceTask?.cancel()
            searchDebounceTask = Task {
                try? await Task.sleep(nanoseconds: 200_000_000)
                guard !Task.isCancelled else { return }
                debouncedSearchText = newValue
            }
        }
        .sheet(item: $scorecardWorkflowPreview) { preview in
            scorecardWorkflowPreviewSheet(preview)
        }
        .sheet(item: $cloudflareLinkRequest) { request in
            RepositoryCloudflareLinkSheet(
                repositoryName: request.repository.name,
                onCancel: { cloudflareLinkRequest = nil },
                onSave: { kind, environmentName, rank, accountID, resourceName, deploymentEnvironment, branch in
                    linkCloudflareProject(
                        request.repository,
                        kind: kind,
                        environmentName: environmentName,
                        rank: rank,
                        accountID: accountID,
                        resourceName: resourceName,
                        deploymentEnvironment: deploymentEnvironment,
                        branch: branch
                    )
                    cloudflareLinkRequest = nil
                }
            )
        }
        .confirmationDialog(
            "Move artifacts to Trash?",
            isPresented: Binding(
                get: { pendingArtifactCleanup != nil },
                set: { if !$0 { pendingArtifactCleanup = nil } }
            ),
            titleVisibility: .visible,
            presenting: pendingArtifactCleanup
        ) { repository in
            let folders = repositoryCleanableFolders(repository)
            Button("Move \(folders.count) folder\(folders.count == 1 ? "" : "s") to Trash") {
                state.trashRepositoryArtifacts(repoRoot: repository.root, folders: folders)
                pendingArtifactCleanup = nil
            }
            Button("Cancel", role: .cancel) { pendingArtifactCleanup = nil }
        } message: { repository in
            let folders = repositoryCleanableFolders(repository)
            let bytes = folders.reduce(UInt64(0)) { $0 + $1.sizeBytes }
            Text("Moves \(folders.count) rebuildable artifact folder\(folders.count == 1 ? "" : "s") (\(formatBytes(bytes))) to the Finder Trash. Reversible with Put Back; nothing is permanently deleted.")
        }
    }

    /// Trash-eligible artifact folders for a repo: only safe/rebuildable tiers,
    /// mirroring the Storage tab's cleanup eligibility.
    private func repositoryCleanableFolders(_ repository: RepositorySummary) -> [StorageRepoArtifactFolderModel] {
        repository.topArtifactFolders.filter(repositoryArtifactFolderIsTrashActionable)
    }

    private func repositoryArtifactFolderIsTrashActionable(_ folder: StorageRepoArtifactFolderModel) -> Bool {
        ["safe", "rebuildable"].contains(folder.cleanupTier)
            && folder.cleanupAllowed
            && folder.defaultCleanupAction == "trash"
            && folder.cleanupBlockers.isEmpty
            && !folder.sizeTruncated
            && !folder.cloudPlaceholder
            && !folder.hasHardlinks
    }

    private var repositoryToolBand: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search repositories, branches, writers",
            searchWidth: 320
        ) {
            repositoryModeMenu
        } filterTools: {
            repositorySortMenu
        } badges: {
            AetowerToolBadgeGroup(repositoryHeaderBadges, visibleCount: 2)
        } actions: {
            AetowerScanButton(
                state.storageHygieneReport == nil ? "Scan" : "Refresh",
                isRunning: state.storageHygieneIsLoading
            ) {
                state.runStorageHygieneScan(roots: repositoryScanRoots)
            }
        }
    }

    private var repositoryHeaderBadges: [AetowerToolBadgeItem] {
        [
            AetowerToolBadgeItem(
                "Roots",
                value: "\(settings.repositoryRoots.count)",
                systemImage: "folder.badge.gearshape",
                tone: repositoryRootsBadgeTone
            ),
            AetowerToolBadgeItem(
                "Repos",
                value: repositoryCountLabel,
                systemImage: "folder",
                tone: repositoryBadgeTone
            ),
            AetowerToolBadgeItem(
                "Artifacts",
                value: artifactBytesLabel,
                systemImage: "shippingbox",
                tone: AetowerDesign.Tone.disk
            ),
            AetowerToolBadgeItem(
                "Attention",
                value: attentionCountLabel,
                systemImage: "exclamationmark.triangle",
                tone: attentionBadgeTone
            ),
        ]
    }

    private var repositoryModeMenu: some View {
        // Visible segmented control instead of a grid-icon dropdown: the old
        // menu hid that the list was filtered to attention-only, so users
        // expecting "all my repos" silently saw a subset. Default is Overview.
        Picker("Repository view", selection: $mode) {
            ForEach(RepositoryMode.allCases) { candidate in
                Text(candidate.label).tag(candidate)
            }
        }
        .pickerStyle(.segmented)
        .fixedSize()
        .accessibilityLabel("Repository view")
    }

    private var repositorySortMenu: some View {
        Menu {
            ForEach(RepositorySort.allCases) { candidate in
                Button {
                    sort = candidate
                } label: {
                    HStack {
                        Text(candidate.label)
                        if sort == candidate {
                            Spacer()
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                Image(systemName: "arrow.up.arrow.down")
                Text(sort.label)
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
        .accessibilityLabel("Repository sort")
    }

    private var repositoryScanRoots: [String] {
        SettingsStore.normalizedRepositoryRoots(settings.repositoryRoots)
    }

    private var repositoryRootsBadgeTone: Color {
        repositoryScanRoots.contains { !SettingsStore.repositoryRootExists($0) }
            ? AetowerDesign.Status.warning
            : AetowerDesign.Status.ready
    }

    private var repositoryRootsHelp: String {
        let missingRoots = repositoryScanRoots.filter { !SettingsStore.repositoryRootExists($0) }
        let visibleRoots = repositoryScanRoots.prefix(5).joined(separator: ", ")
        let remainingCount = repositoryScanRoots.count - min(repositoryScanRoots.count, 5)
        let suffix = remainingCount > 0 ? ", +\(remainingCount) more" : ""
        if missingRoots.isEmpty {
            return "Repository inventory roots: \(visibleRoots)\(suffix)."
        }
        return "Repository inventory roots: \(visibleRoots)\(suffix). Missing: \(missingRoots.joined(separator: ", "))."
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
        if state.storageHygieneIsVerifyingCache {
            return "Verifying"
        }
        if state.storageHygieneIsLoading {
            guard let job = state.storageScanJob else { return "Scanning" }
            switch job.status {
            case "queued": return "Queued"
            case "running": return repositoryScanPhaseLabel(job.progress.phase)
            case "paused": return "Paused"
            case "complete": return "Finalizing"
            default: return job.status.replacingOccurrences(of: "_", with: " ").capitalized
            }
        }
        if let report = state.storageHygieneReport, repositoryInventoryIsIncomplete(report) {
            return "Inventory partial"
        }
        guard let completedAt = state.storageHygieneCompletedAt else { return "Not run" }
        return "Last \(completedAt.formatted(date: .omitted, time: .shortened))"
    }

    private var repositoryScanStatusIcon: String {
        if let job = state.storageScanJob, job.status == "paused" { return "pause.circle" }
        if state.storageHygieneIsVerifyingCache { return "arrow.triangle.2.circlepath" }
        if let report = state.storageHygieneReport, !state.storageHygieneIsLoading,
           repositoryInventoryIsIncomplete(report)
        {
            return "exclamationmark.triangle"
        }
        return state.storageHygieneIsLoading ? "arrow.triangle.2.circlepath" : "clock"
    }

    private var repositoryScanStatusTone: Color {
        if let job = state.storageScanJob, job.status == "paused" { return AetowerDesign.Status.warning }
        if state.storageHygieneIsVerifyingCache { return AetowerDesign.Tone.cpu }
        if let report = state.storageHygieneReport, !state.storageHygieneIsLoading,
           repositoryInventoryIsIncomplete(report)
        {
            return AetowerDesign.Status.warning
        }
        return state.storageHygieneIsLoading ? AetowerDesign.Tone.cpu : AetowerDesign.Status.neutral
    }

    private var repositoryScanStatusHelp: String {
        guard let job = state.storageScanJob else {
            if state.storageHygieneIsVerifyingCache {
                return "Showing cached repository data while refreshing Git-root inventory in the background."
            }
            if state.storageHygieneIsLoading {
                return "Preparing repository inventory refresh."
            }
            if let report = state.storageHygieneReport, repositoryInventoryIsIncomplete(report) {
                return repositoryInventoryIncompleteHelp(report)
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
            parts.append("phase \(repositoryScanPhaseLabel(job.progress.phase))")
        }
        if let currentPathHint = job.progress.currentPathHint, !currentPathHint.isEmpty {
            parts.append(shortPath(currentPathHint))
        }
        if let throttleReason = job.progress.throttleReason, !throttleReason.isEmpty {
            parts.append(throttleReason)
        }
        return parts.joined(separator: " · ")
    }

    private func repositoryScanPhaseLabel(_ phase: String) -> String {
        switch phase {
        case "repository_inventory":
            return "Repository inventory"
        case "artifact_sizing":
            return "Artifact sizing"
        case "scorecard_overlay":
            return "Scorecard overlay"
        case "finalizing":
            return "Finalizing"
        case "queued":
            return "Queued"
        case "complete":
            return "Complete"
        case "failed":
            return "Failed"
        case "cancelled":
            return "Cancelled"
        case "paused":
            return "Paused"
        case "":
            return "Running"
        default:
            return phase.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    private func repositoryDashboard(_ report: StorageHygieneReportModel) -> some View {
        let repositories = filteredRepositories(from: report)

        return ScrollView {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                repositoryStatusStrip(report, repositories: repositories)
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
                    repositoryCockpitList(repositories)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
    }

    private func repositoryStatusStrip(
        _ report: StorageHygieneReportModel,
        repositories: [RepositorySummary]
    ) -> some View {
        let allRepositories = repositorySummaries(from: report)
        let attentionCount = allRepositories.filter(\.requiresAttention).count
        let changedInventoryCount = allRepositories.filter(\.inventoryNeedsAttention).count
        let completedLabel = state.storageHygieneCompletedAt.map {
            $0.formatted(date: .omitted, time: .shortened)
        } ?? "Not run"

        return AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerBadge(
                    "\(allRepositories.count)\(repositoryInventoryIsIncomplete(report) ? "+" : "") repos",
                    tone: repositoryInventoryIsIncomplete(report) ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
                )
                AetowerBadge(
                    "\(attentionCount)\(repositoryInventoryIsIncomplete(report) ? "+" : "") attention",
                    tone: attentionCount > 0 || repositoryInventoryIsIncomplete(report)
                        ? AetowerDesign.Status.warning
                        : AetowerDesign.Status.ready
                )
                AetowerBadge(
                    repositoryInventoryIsIncomplete(report) ? "Inventory partial" : "Inventory complete",
                    tone: repositoryInventoryIsIncomplete(report) ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
                )
                if changedInventoryCount > 0 {
                    AetowerBadge(
                        "\(changedInventoryCount) stale",
                        tone: AetowerDesign.Status.warning
                    )
                }
                Text("\(repositories.count) visible · \(completedLabel)")
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                Spacer(minLength: AetowerDesign.Spacing.sm)
                if repositoryInventoryIsIncomplete(report) {
                    Text(repositoryInventoryPartialRootsDetail(report))
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
        }
    }

    private func repositoryCockpitList(_ repositories: [RepositorySummary]) -> some View {
        AetowerSurface(level: .card, padding: AetowerDesign.Spacing.sm) {
            LazyVStack(alignment: .leading, spacing: RepositoryRowLayout.rowSpacing) {
                repositoryListHeader(visible: repositories)
                ForEach(repositories) { repository in
                    repositoryListRow(repository)
                }
            }
        }
    }

    private func repositoryListHeader(visible: [RepositorySummary]) -> some View {
        HStack(spacing: RepositoryRowLayout.columnSpacing) {
            repositorySelectAllToggle(visible: visible)
                .frame(width: RepositoryRowLayout.selectionWidth)
            Text("")
                .frame(width: RepositoryRowLayout.iconWidth)
            tableHeader("Repository")
                .frame(maxWidth: .infinity, alignment: .leading)
            tableHeader("Status", width: RepositoryRowLayout.statusWidth)
            tableHeader("Git", width: RepositoryRowLayout.gitWidth, alignment: .center)
            tableHeader("Inventory", width: RepositoryRowLayout.inventoryWidth, alignment: .center)
            tableHeader("Attention", width: RepositoryRowLayout.attentionWidth)
            tableHeader("Next action", width: RepositoryRowLayout.actionWidth, alignment: .trailing)
        }
        .font(AetowerDesign.Typography.metadataStrong)
        .foregroundStyle(AetowerDesign.Ink.tertiary)
        .padding(.horizontal, AetowerDesign.Spacing.sm)
        .padding(.vertical, AetowerDesign.Spacing.xxs)
    }

    private func repositorySelectAllToggle(visible visibleRepos: [RepositorySummary]) -> some View {
        let visible = Set(visibleRepos.map(\.root))
        let allSelected = !visible.isEmpty && visible.isSubset(of: selectedRepoRoots)
        return Button {
            if allSelected { selectedRepoRoots.subtract(visible) }
            else { selectedRepoRoots.formUnion(visible) }
        } label: {
            Image(systemName: allSelected ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(allSelected ? AetowerDesign.Tone.cpu : AetowerDesign.Ink.tertiary)
        }
        .buttonStyle(.plain)
        .help(allSelected ? "Deselect all visible" : "Select all visible")
    }

    /// Floating batch pill — the fleet-scale action surface. Mirrors the
    /// Storage cleanup pill: content-width, hovers at the bottom, shows the
    /// selection and the bulk actions, plus live progress while a batch drains.
    @ViewBuilder
    private var repositoryBulkPill: some View {
        if !selectedRepoRoots.isEmpty {
            let roots = Array(selectedRepoRoots)
            repositoryFloatingSurface(maxWidth: 760) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    AetowerBadge(
                        "\(selectedRepoRoots.count) selected",
                        systemImage: "checklist",
                        tone: AetowerDesign.Tone.cpu
                    )

                    if let progress = state.repositoryBulkProgress, let label = state.repositoryBulkLabel {
                        Divider().frame(height: 16)
                        ProgressView().controlSize(.small)
                        Text("\(label): \(progress.completed)/\(progress.total)")
                            .font(AetowerDesign.Typography.caption)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                    }

                    Divider().frame(height: 16)
                    Button("Run Scorecard") { state.bulkRunScorecard(roots: roots) }
                        .buttonStyle(.bordered)
                    Button("Refresh providers") { state.bulkRefreshProviders(roots: roots) }
                        .buttonStyle(.bordered)
                    Button {
                        selectedRepoRoots.removeAll()
                        state.clearRepositoryBulk()
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                    .help("Clear selection")
                }
                .controlSize(.small)
            }
        }
    }

    private func repositoryInventoryRefreshToast(_ refreshState: RepositoryInventoryRefreshState) -> some View {
        repositoryFloatingSurface(maxWidth: 420) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                ProgressView()
                    .controlSize(.small)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text(refreshState.title)
                        .font(AetowerDesign.Typography.caption.weight(.semibold))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                    Text(refreshState.detail)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
        }
    }

    private func repositoryFloatingSurface<Content: View>(
        maxWidth: CGFloat,
        @ViewBuilder content: () -> Content
    ) -> some View {
        AetowerSurface(
            level: .card,
            padding: AetowerDesign.Spacing.sm,
            cornerRadius: AetowerDesign.Radius.md
        ) {
            content()
        }
        .frame(maxWidth: maxWidth)
        .fixedSize(horizontal: false, vertical: true)
    }

    @ViewBuilder
    private func tableHeader(
        _ label: String,
        width: CGFloat? = nil,
        alignment: Alignment = .leading
    ) -> some View {
        let text = Text(label.uppercased())
            .lineLimit(1)
        if let width {
            text.frame(width: width, alignment: alignment)
        } else {
            text
        }
    }

    private func repositoryListRow(_ repository: RepositorySummary) -> some View {
        let project = repositoryProject(for: repository)
        let attention = repositoryAttentionItems(repository).first
        let action = primaryRepositoryAction(repository)
        let model = RepositoryRowModel(
            id: repository.id,
            name: repository.name,
            shortPath: shortPath(repository.root),
            statusLabel: repository.statusLabel,
            statusTone: repository.statusTone,
            gitOverview: gitOverviewLabel(repository),
            gitDetail: repository.gitBranch ?? repository.gitHead ?? "",
            inventoryLabel: inventoryFreshnessLabel(repository),
            inventoryTone: inventoryFreshnessTone(repository),
            inventoryDetail: inventoryFreshnessDetail(repository),
            attentionTitle: attention?.title ?? "Ready",
            attentionDetail: attention?.detail ?? "No immediate action required.",
            attentionTone: attention?.tone ?? AetowerDesign.Status.ready,
            aiUsageLabel: repository.aiRunCount > 0 ? repositoryAiUsageLabel(repository) : nil,
            aiUsageHelp: repositoryAiUsageHelp(repository),
            projectTone: project.map { repositoryProjectTone($0) },
            projectName: project?.name,
            primaryAction: action,
            inventoryNeedsAttention: repository.inventoryNeedsAttention,
            isSelected: selectedRepoRoots.contains(repository.root)
        )
        return RepositoryRow(
            model: model,
            onToggleSelect: { toggleRepoSelection(repository.root) },
            onPrimaryAction: { performPrimaryRepositoryAction(action, repository: repository) },
            onReveal: { reveal(repository.root) },
            onCopyPath: { copy(repository.root) },
            onCopyBrief: {
                copy(optimizationBrief(for: repository))
                copiedRepositoryID = repository.id
            }
        )
    }

    private func toggleRepoSelection(_ root: String) {
        if selectedRepoRoots.contains(root) {
            selectedRepoRoots.remove(root)
        } else {
            selectedRepoRoots.insert(root)
        }
    }

    private func repositoryProjectSection(_ repository: RepositorySummary) -> some View {
        let project = repositoryProject(for: repository)
        return AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.md) {
            DisclosureGroup(isExpanded: projectSectionExpansionBinding(for: repository)) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    if let project {
                        repositoryProjectLinkedContent(project, repository: repository)
                    } else {
                        repositoryProjectUnlinkedContent(repository)
                    }
                }
                .padding(.top, AetowerDesign.Spacing.sm)
            } label: {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                    Label("Project", systemImage: "link")
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Spacer(minLength: AetowerDesign.Spacing.md)
                    if let project {
                        Text(project.name)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                            .lineLimit(1)
                            .truncationMode(.tail)
                        AetowerBadge("Linked", tone: AetowerDesign.Tone.cpu)
                    } else {
                        AetowerBadge("Not linked", tone: AetowerDesign.Status.neutral)
                    }
                }
            }
        }
    }

    private func repositoryProjectLinkedContent(
        _ project: RepositoryProjectModel,
        repository: RepositorySummary
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    Text(project.name)
                        .font(AetowerDesign.Typography.controlLabel)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text(shortPath(project.primaryRepoRoot))
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                repositoryProjectLinkBadges(project)
            }

            repositoryProjectEnvironmentHealthStrip(project)
            repositoryProjectGitHubStatus(project, repository: repository)
            repositoryProjectCloudflareStatuses(project, repository: repository)
            repositoryProjectActions(repository, project: project)
        }
    }

    private func repositoryProjectUnlinkedContent(_ repository: RepositorySummary) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            AetowerBadge("Not linked", tone: AetowerDesign.Status.neutral)
            repositoryProjectActions(repository, project: nil)
        }
    }

    private func repositoryProjectLinkBadges(_ project: RepositoryProjectModel) -> some View {
        HStack(spacing: AetowerDesign.Spacing.xs) {
            ForEach(project.links) { link in
                AetowerBadge(
                    repositoryProjectLinkLabel(link),
                    systemImage: repositoryProjectLinkIcon(link),
                    tone: repositoryProjectLinkTone(link)
                )
            }
            ForEach(project.cloudflareEnvironmentGroups) { group in
                AetowerBadge(
                    group.name,
                    systemImage: "server.rack",
                    tone: repositoryProjectCloudflareGroupTone(group, project: project)
                )
            }
        }
    }

    @ViewBuilder
    private func repositoryProjectEnvironmentHealthStrip(_ project: RepositoryProjectModel) -> some View {
        let groups = project.cloudflareEnvironmentGroups
        if !groups.isEmpty {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                ForEach(groups) { group in
                    AetowerBadge(
                        "\(group.name): \(repositoryProjectCloudflareGroupLabel(group, project: project))",
                        systemImage: "server.rack",
                        tone: repositoryProjectCloudflareGroupTone(group, project: project)
                    )
                }
            }
        }
    }

    @ViewBuilder
    private func repositoryProjectGitHubStatus(
        _ project: RepositoryProjectModel,
        repository: RepositorySummary
    ) -> some View {
        if project.githubRepositoryLink != nil {
            GitHubProviderCard(
                status: project.githubStatus,
                isLoading: state.repositoryProjectGitHubLoadingRoots.contains(repository.root),
                error: state.repositoryProjectGitHubErrorsByRoot[repository.root],
                onRerunWorkflow: { runId in
                    state.rerunRepositoryWorkflow(repoRoot: repository.root, runId: runId)
                }
            )
        }
    }

    @ViewBuilder
    private func repositoryProjectCloudflareStatuses(
        _ project: RepositoryProjectModel,
        repository: RepositorySummary
    ) -> some View {
        let groups = project.cloudflareEnvironmentGroups
        if !groups.isEmpty {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                ForEach(groups) { group in
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        HStack(spacing: AetowerDesign.Spacing.xs) {
                            Text(group.name)
                                .font(AetowerDesign.Typography.caption.weight(.semibold))
                                .foregroundStyle(AetowerDesign.Ink.primary)
                                .lineLimit(1)
                                .truncationMode(.tail)
                            AetowerBadge(
                                repositoryProjectCloudflareGroupLabel(group, project: project),
                                tone: repositoryProjectCloudflareGroupTone(group, project: project)
                            )
                        }
                        ForEach(group.links) { link in
                            repositoryProjectCloudflareStatus(
                                link,
                                project: project,
                                repository: repository
                            )
                        }
                    }
                }
            }
        }
    }

    private func repositoryProjectCloudflareStatus(
        _ link: RepositoryProjectLinkModel,
        project: RepositoryProjectModel,
        repository: RepositorySummary
    ) -> some View {
        let status = project.cloudflareStatus(for: link)
        let loadingKey = state.repositoryCloudflareProviderKey(repoRoot: repository.root, link: link)
        let isLoading = state.repositoryProjectCloudflareLoadingKeys.contains(loadingKey)
        let error = state.repositoryProjectCloudflareErrorsByKey[loadingKey]
        return AetowerSurface(
            level: repositoryProjectCloudflareStatusLevel(status: status, error: error),
            padding: AetowerDesign.Spacing.sm
        ) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                    AetowerBadge(
                        repositoryProjectCloudflareStatusLabel(status, isLoading: isLoading, error: error),
                        systemImage: "cloud",
                        tone: repositoryCloudflareProviderTone(status, error: error, isLoading: isLoading)
                    )
                    Text(repositoryProjectCloudflareLinkTitle(link))
                        .font(AetowerDesign.Typography.caption.weight(.semibold))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: AetowerDesign.Spacing.md)
                    Text(repositoryProjectCloudflareCapturedLabel(status))
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                    // Pages deployments can be retried in place; the read-only
                    // deploy tile becomes a control surface.
                    if link.kind == .pages, let deploymentId = status?.deploymentId, !deploymentId.isEmpty {
                        Button {
                            state.redeployRepositoryCloudflare(
                                repoRoot: repository.root,
                                link: link,
                                deploymentId: deploymentId
                            )
                        } label: {
                            Label("Redeploy", systemImage: "arrow.clockwise")
                                .labelStyle(.iconOnly)
                        }
                        .buttonStyle(.borderless)
                        .controlSize(.mini)
                        .disabled(isLoading)
                        .help("Retry this Cloudflare Pages deployment.")
                    }
                }

                if let status {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 130), spacing: AetowerDesign.Spacing.sm)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.sm
                    ) {
                        repositoryProjectProviderMetric(
                            "Deployment",
                            status.deploymentStatus?.capitalized ?? "Unavailable"
                        )
                        repositoryProjectProviderMetric(
                            "Environment",
                            status.environment?.capitalized ?? link.deploymentEnvironment?.capitalized ?? "Any"
                        )
                        repositoryProjectProviderMetric("Branch", status.branch ?? "Unavailable")
                        repositoryProjectProviderMetric("Commit", shortCommitLabel(status.commit))
                    }
                    if let url = status.url, !url.isEmpty {
                        Text(url)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                            .textSelection(.enabled)
                    }
                    ForEach(status.warnings.prefix(2), id: \.self) { warning in
                        Text(warning)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Status.warning)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                } else if let error {
                    Text(error)
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Status.warning)
                } else {
                    Text("Not tested")
                        .font(AetowerDesign.Typography.metadata)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }

                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Button {
                        state.refreshRepositoryCloudflareStatus(
                            repoRoot: repository.root,
                            link: link,
                            force: true
                        )
                    } label: {
                        Label(isLoading ? "Testing" : "Test connection", systemImage: "checkmark.circle")
                    }
                    .disabled(isLoading)

                    Button {
                        state.refreshRepositoryCloudflareStatus(
                            repoRoot: repository.root,
                            link: link,
                            force: false
                        )
                    } label: {
                        Label(isLoading ? "Refreshing" : "Refresh", systemImage: "arrow.clockwise")
                    }
                    .disabled(isLoading)
                }
                .controlSize(.small)
            }
        }
    }

    private func repositoryProjectActions(
        _ repository: RepositorySummary,
        project: RepositoryProjectModel?
    ) -> some View {
        let githubLink = githubProjectLink(for: repository)
        let linkedGithub = project?.githubRepositoryLink != nil
        let githubIsLoading = state.repositoryProjectGitHubLoadingRoots.contains(repository.root)
        return HStack(spacing: AetowerDesign.Spacing.sm) {
            if project == nil {
                Button {
                    createProject(from: repository)
                } label: {
                    Label("Create project from this repo", systemImage: "plus")
                }
            }
            Button {
                linkGitHubProject(repository)
            } label: {
                Label("Link GitHub", systemImage: "chevron.left.forwardslash.chevron.right")
            }
            .disabled(githubLink == nil)
            .help(githubLink == nil ? "No GitHub remote was detected for this repository." : "Link the detected GitHub repository.")

            Button {
                cloudflareLinkRequest = RepositoryCloudflareLinkRequest(
                    id: repository.id,
                    repository: repository
                )
            } label: {
                Label("Link Cloudflare", systemImage: "cloud")
            }

            Button {
                state.refreshRepositoryGitHubStatus(
                    repoRoot: repository.root,
                    currentBranch: repository.gitBranch,
                    currentHead: repository.gitHead,
                    force: false
                )
            } label: {
                Label(githubIsLoading ? "Refreshing" : "Refresh status", systemImage: "arrow.clockwise")
            }
            .disabled(!linkedGithub || githubIsLoading)
            .help(linkedGithub ? "Refresh GitHub project status; cached values are reused briefly." : "Link GitHub before refreshing status.")

            if let project {
                Spacer(minLength: AetowerDesign.Spacing.sm)
                // Linking was one-way before: removeRepositoryProject existed but
                // had no call site, so a wrong Cloudflare account or GitHub
                // remote could never be undone from the page.
                Button(role: .destructive) {
                    state.removeRepositoryProject(id: project.id)
                } label: {
                    Label("Unlink", systemImage: "link.badge.plus")
                }
                .help("Remove this project link. The repository itself is untouched.")
            }
        }
        .controlSize(.small)
    }

    private func projectSectionExpansionBinding(for repository: RepositorySummary) -> Binding<Bool> {
        Binding {
            expandedProjectSectionRepositoryID == repository.id
        } set: { isExpanded in
            expandedProjectSectionRepositoryID = isExpanded ? repository.id : nil
        }
    }

    private func repositoryPrimaryActionButton(_ repository: RepositorySummary) -> some View {
        let action = primaryRepositoryAction(repository)
        return Button {
            performPrimaryRepositoryAction(action, repository: repository)
        } label: {
            Label(action.title, systemImage: action.systemImage)
                .lineLimit(1)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .tint(action.tone)
        .help(action.detail)
    }

    private func primaryRepositoryAction(_ repository: RepositorySummary) -> RepositoryPrimaryAction {
        switch repository.primaryOptimizationActionKind {
        case .refreshInventory:
            return RepositoryPrimaryAction(
                title: "Refresh repo",
                detail: inventoryFreshnessDetail(repository),
                systemImage: "arrow.clockwise",
                tone: AetowerDesign.Status.warning,
                kind: .refreshInventory
            )
        case .prepareContract:
            return RepositoryPrimaryAction(
                title: "Prepare with Aethyme",
                detail: agentGuidanceTitle(repository),
                systemImage: "terminal",
                tone: AetowerDesign.Tone.energy,
                kind: .prepareContract
            )
        case .improveScorecard:
            return RepositoryPrimaryAction(
                title: "Ask Chau7",
                detail: "Improve Scorecard posture with local edits and a remote settings checklist.",
                systemImage: "terminal",
                tone: AetowerDesign.Tone.energy,
                kind: .improveScorecard
            )
        case .runScorecard:
            return RepositoryPrimaryAction(
                title: "Run Scorecard",
                detail: "Run OpenSSF Scorecard on demand for this repository.",
                systemImage: "shield.lefthalf.filled",
                tone: AetowerDesign.Tone.cpu,
                kind: .runScorecard
            )
        case .reviewCleanup:
            return RepositoryPrimaryAction(
                title: "Review cleanup",
                detail: "Copy a focused optimization brief for artifact cleanup review.",
                systemImage: "shippingbox",
                tone: AetowerDesign.Tone.disk,
                kind: .reviewCleanup
            )
        case .reviewClones:
            return RepositoryPrimaryAction(
                title: "Review clones",
                detail: cloneGroupDetail(repository),
                systemImage: "square.stack.3d.up",
                tone: AetowerDesign.Status.warning,
                kind: .reviewClones
            )
        case .reveal:
            return RepositoryPrimaryAction(
                title: "Reveal",
                detail: "Reveal this repository in Finder.",
                systemImage: "folder",
                tone: AetowerDesign.Status.neutral,
                kind: .reveal
            )
        }
    }

    private func performPrimaryRepositoryAction(
        _ action: RepositoryPrimaryAction,
        repository: RepositorySummary
    ) {
        switch action.kind {
        case .refreshInventory:
            state.runStorageHygieneScan(roots: repositoryScanRoots)
        case .prepareContract:
            launchPrimaryAethymeContractAction(repository)
        case .improveScorecard:
            if let report = repository.scorecardReport {
                launchScorecardRemediationInChau7(repository, report: report)
            } else {
                state.runRepositoryScorecard(repoRoot: repository.root, mode: "auto", refresh: false)
            }
        case .runScorecard:
            state.runRepositoryScorecard(repoRoot: repository.root, mode: "auto", refresh: false)
        case .reviewCleanup:
            // If there are trash-eligible artifact folders, offer the real
            // (reversible) cleanup; otherwise fall back to the copyable brief.
            if repositoryCleanableFolders(repository).isEmpty {
                copy(optimizationBrief(for: repository))
                copiedRepositoryID = repository.id
            } else {
                pendingArtifactCleanup = repository
            }
        case .reviewClones:
            repositoryPath.append(repository.id)
            detailTab = .git
        case .reveal:
            reveal(repository.root)
        }
    }

    private func launchPrimaryAethymeContractAction(_ repository: RepositorySummary) {
        guard let contract = selectedAgentContract(repository) else {
            copy(optimizationBrief(for: repository))
            copiedRepositoryID = repository.id
            return
        }
        let key = AgentContractPrompts.key(repositoryID: repository.id, contract: contract, kind: "chau7")
        launchAgentContractPromptInChau7(
            repository,
            contract: contract,
            issues: agentContractIssues(repository, contract: contract),
            key: key,
            promptKind: agentContractLaunchPromptKind(contract)
        )
    }

    private func inventoryFreshnessLabel(_ repository: RepositorySummary) -> String {
        if repository.inventoryFingerprintChanged { return "Changed" }
        switch repository.inventoryCacheStatus {
        case "scanned":
            return "Fresh"
        case "current":
            return "Current"
        case "changed":
            return "Changed"
        case "missing":
            return "Missing"
        case "legacy":
            return "Legacy"
        case "uncached":
            return "Uncached"
        default:
            return repository.notSeenInLatestScan ? "Cached" : "Unknown"
        }
    }

    private func inventoryFreshnessTone(_ repository: RepositorySummary) -> Color {
        if repository.inventoryCacheStatus == "missing" { return AetowerDesign.Status.error }
        if repository.inventoryNeedsAttention { return AetowerDesign.Status.warning }
        if repository.inventoryCacheStatus == "scanned" || repository.inventoryCacheStatus == "current" {
            return AetowerDesign.Status.ready
        }
        return AetowerDesign.Status.neutral
    }

    private func inventoryFreshnessDetail(_ repository: RepositorySummary) -> String {
        let seen = repository.inventoryLastSeenMillis.map(repositoryMillisLabel) ?? "not seen"
        switch repository.inventoryCacheStatus {
        case "scanned":
            return "latest scan"
        case "current":
            return "cache verified · \(seen)"
        case "changed":
            return "Git metadata changed"
        case "missing":
            return "cached path missing"
        case "legacy":
            return "refresh fingerprint"
        case "uncached":
            return "not cached"
        default:
            return repository.notSeenInLatestScan ? "cached · \(seen)" : seen
        }
    }

    private func repositoryMillisLabel(_ millis: UInt64) -> String {
        guard millis > 0 else { return "unknown" }
        let date = Date(timeIntervalSince1970: Double(millis) / 1000.0)
        return date.formatted(date: .omitted, time: .shortened)
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
                if let result = state.repositoryCleanupResultByRoot[repository.root] {
                    repositoryCleanupResultBanner(result, repoRoot: repository.root)
                }
                repositoryProjectSection(repository)
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.lg) {
                    repositoryDetailRail(repository)
                    Divider()
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                        repositoryDetailTabContent(repository, report: report)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(AetowerDesign.Spacing.xxl)
        }
        .navigationTitle(repository.name)
    }

    /// Vertical rail replacing the 6-tab segmented picker: each destination
    /// carries a live signal (score, coverage, dirty/clone count) so the user
    /// sees which panel matters, mirroring the Storage tab's rail.
    private func repositoryDetailRail(_ repository: RepositorySummary) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
            ForEach(RepositoryDetailTab.allCases) { tab in
                AetowerRailButton(
                    title: tab.label,
                    role: "",
                    signal: repositoryDetailTabSignal(tab, repository: repository),
                    systemImage: tab.systemImage,
                    signalTone: repositoryDetailTabTone(tab, repository: repository),
                    isSelected: detailTab == tab
                ) {
                    detailTab = tab
                }
            }
        }
        .frame(width: 172)
    }

    private func repositoryDetailTabSignal(
        _ tab: RepositoryDetailTab,
        repository: RepositorySummary
    ) -> String? {
        switch tab {
        case .storage:
            return repository.hasStorageFootprint ? formatBytes(repository.currentSizeBytes) : nil
        case .scorecard:
            guard let report = repository.scorecardReport, let score = report.score else { return nil }
            return String(format: "%.1f", score)
        case .contracts:
            return repository.agentGuidanceIssueCount > 0 ? "\(repository.agentGuidanceIssueCount)" : nil
        case .git:
            if repository.gitDirtyStatus == "dirty" { return "dirty" }
            return repository.cloneGroupCount > 1 ? "\(repository.cloneGroupCount) clones" : nil
        default:
            return nil
        }
    }

    private func repositoryDetailTabTone(
        _ tab: RepositoryDetailTab,
        repository: RepositorySummary
    ) -> Color {
        switch tab {
        case .scorecard:
            return repository.hasScorecardAttention ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
        case .contracts:
            return repository.agentGuidanceIssueCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
        case .git:
            return repository.gitDirtyStatus == "dirty" ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
        default:
            return AetowerDesign.Status.neutral
        }
    }

    private func repositoryCleanupResultBanner(
        _ result: RepositoryArtifactCleanupResult,
        repoRoot: String
    ) -> some View {
        AetowerInfoBanner(
            result.succeeded
                ? "Moved \(result.movedCount) folder\(result.movedCount == 1 ? "" : "s") (\(formatBytes(result.reclaimedBytes))) to the Trash. Empty the Trash to free the space."
                : "Moved \(result.movedCount) folder\(result.movedCount == 1 ? "" : "s"); \(result.failedCount) could not be removed (\(result.firstError ?? "in use or protected")).",
            systemImage: result.succeeded ? "checkmark.circle" : "exclamationmark.triangle",
            tone: result.succeeded ? AetowerDesign.Status.ready : AetowerDesign.Status.warning
        )
        .onTapGesture { state.clearRepositoryCleanupResult(repoRoot: repoRoot) }
    }

    @ViewBuilder
    private func repositoryDetailTabContent(
        _ repository: RepositorySummary,
        report: StorageHygieneReportModel
    ) -> some View {
        switch detailTab {
        case .actions:
            repositoryActions(repository)
            repositoryAttentionSummary(repository)
        case .storage:
            repositoryStorageSignals(repository)
                .task(id: repository.root) {
                    state.loadRepositoryStorageDetail(repoRoot: repository.root)
                }
            repositoryStorageDetailSection(repository)
            topArtifacts(repository)
        case .contracts:
            repositoryAgentGuidance(repository)
        case .scorecard:
            repositorySupplyChainReadiness(repository)
        case .git:
            repositoryGitIntelligence(repository)
        case .live:
            liveContext(repository)
        }
    }

    /// Health header, modelled on the Storage tab's disk header: a prominent
    /// status verdict, an attention meter proportional to severity, the one
    /// recommended action as a prominent CTA (promoted from a tab down), and
    /// footprint demoted to a small stat rather than the focal number.
    private func repositoryHero(_ repository: RepositorySummary) -> some View {
        // attentionScore accumulates unbounded; ~40 is already a heavy repo, so
        // clamp there for a readable meter.
        let severity = min(repository.attentionScore / 40.0, 1.0)
        return AetowerSurface(padding: AetowerDesign.Spacing.lg) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                    Image(systemName: "folder.badge.gearshape")
                        .foregroundStyle(repository.statusTone)
                    Text(repository.name)
                        .font(AetowerDesign.Typography.metricValue(size: 24, weight: .semibold))
                        .foregroundStyle(AetowerDesign.Ink.primary)
                        .lineLimit(1)
                        .minimumScaleFactor(0.8)
                    AetowerBadge(repository.statusLabel, tone: repository.statusTone)
                    Spacer(minLength: AetowerDesign.Spacing.md)
                    repositoryPrimaryActionButton(repository)
                }

                Text(repositorySummarySentence(repository))
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                // Attention meter — proportional to severity, tone-matched.
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        Capsule().fill(AetowerDesign.Surface.badgeStrong)
                        Capsule().fill(repository.statusTone.opacity(0.85))
                            .frame(width: max(2, geo.size.width * severity))
                    }
                }
                .frame(height: 6)

                HStack(spacing: AetowerDesign.Spacing.lg) {
                    repositoryHeroStat(
                        repository.hasStorageFootprint ? formatBytes(repository.currentSizeBytes) : "—",
                        "footprint"
                    )
                    repositoryHeroStat(gitOverviewLabel(repository), "git")
                    if repository.reviewItemCount > 0 {
                        repositoryHeroStat("\(repository.reviewItemCount)", "to review")
                    }
                    Spacer()
                    Text(repository.root)
                        .font(AetowerDesign.Typography.compactData(size: 10))
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                }
            }
        }
    }

    private func repositoryHeroStat(_ value: String, _ label: String) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(value)
                .font(.system(size: 14, weight: .semibold, design: .rounded))
                .foregroundStyle(AetowerDesign.Ink.primary)
            Text(label)
                .font(AetowerDesign.Typography.metadata)
                .foregroundStyle(AetowerDesign.Ink.secondary)
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
        repository.optimizationSignals.map { signal in
            repositoryAttentionItem(signal, repository: repository)
        }
    }

    private func repositoryAttentionItem(
        _ signal: RepositoryOptimizationSignal,
        repository: RepositorySummary
    ) -> RepositoryAttentionItem {
        switch signal.kind {
        case .inventoryFreshness:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Inventory freshness",
                detail: inventoryFreshnessDetail(repository),
                systemImage: "arrow.clockwise",
                tone: inventoryFreshnessTone(repository),
                level: repositoryAttentionLevel(signal)
            )
        case .budget:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Budget guardrail",
                detail: "\(repository.violationCount) repository budget signal\(repository.violationCount == 1 ? "" : "s") need review before cleanup.",
                systemImage: "exclamationmark.triangle",
                tone: AetowerDesign.Status.error,
                level: .critical
            )
        case .contractReadiness:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Contract readiness",
                detail: agentReadinessDetail(repository),
                systemImage: "checklist.checked",
                tone: agentReadinessTone(repository),
                level: repositoryAttentionLevel(signal)
            )
        case .guidance:
            return RepositoryAttentionItem(
                id: signal.id,
                title: agentGuidanceTitle(repository),
                detail: qualityDetail(repository),
                systemImage: "doc.badge.exclamationmark",
                tone: repository.qualityStatusTone,
                level: repositoryAttentionLevel(signal)
            )
        case .scorecard:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Scorecard posture",
                detail: scorecardReadinessDetail(repository),
                systemImage: "shield.lefthalf.filled",
                tone: scorecardReadinessTone(repository),
                level: repositoryAttentionLevel(signal)
            )
        case .githubProvider:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "GitHub status",
                detail: repositoryGitHubProviderDetail(repository),
                systemImage: "chevron.left.forwardslash.chevron.right",
                tone: repositoryGitHubProviderTone(repository.project?.githubStatus),
                level: repositoryAttentionLevel(signal)
            )
        case .cloudflareProvider:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Cloudflare deployment",
                detail: repositoryCloudflareProviderDetail(repository),
                systemImage: "cloud",
                tone: repositoryCloudflareProviderTone(repository),
                level: repositoryAttentionLevel(signal)
            )
        case .cloneGroup:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Duplicate clone group",
                detail: cloneGroupDetail(repository),
                systemImage: "square.stack.3d.up",
                tone: AetowerDesign.Status.warning,
                level: .warning
            )
        case .dirtyWorktree:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Dirty worktree",
                detail: dirtyDetail(repository),
                systemImage: "pencil.and.scribble",
                tone: AetowerDesign.Tone.energy,
                level: .warning
            )
        case .storageGrowth:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Storage growth",
                detail: "\(growthLabel(repository)) in \(repository.growthWindow).",
                systemImage: "chart.line.uptrend.xyaxis",
                tone: growthTone(repository),
                level: .warning
            )
        case .reviewItems:
            return RepositoryAttentionItem(
                id: signal.id,
                title: "Reviewable artifacts",
                detail: "\(repository.reviewItemCount) item\(repository.reviewItemCount == 1 ? "" : "s") need human review before cleanup.",
                systemImage: "shippingbox",
                tone: AetowerDesign.Status.warning,
                level: .warning
            )
        }
    }

    private func repositoryAttentionLevel(_ signal: RepositoryOptimizationSignal) -> AetowerSurfaceLevel {
        signal.severity == .critical ? .critical : .warning
    }

    private func repositoryStorageSignals(_ repository: RepositorySummary) -> some View {
        AetowerSection("Storage", subtitle: "Artifact footprint, growth, and cleanup review signals") {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 170), spacing: AetowerDesign.Spacing.md)],
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
                    "Review",
                    value: "\(repository.reviewItemCount)",
                    detail: "\(repository.safeItemCount) safe · \(repository.staleItemCount) stale",
                    systemImage: "checklist",
                    tone: repository.reviewItemCount > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
                )
                AetowerMetricTile(
                    "Agent cost",
                    value: formatBytes(repository.agentArtifactBytes),
                    detail: "\(repository.agentCount) agent source\(repository.agentCount == 1 ? "" : "s")",
                    systemImage: "person.crop.circle.badge.gearshape",
                    tone: repository.agentArtifactBytes > 0 ? AetowerDesign.Tone.memory : AetowerDesign.Status.neutral
                )
                if let footprint = repository.footprint {
                    AetowerMetricTile(
                        "Rebuildable",
                        value: formatBytes(footprint.rebuildableBytes),
                        detail: "\(storageFormatPercent(footprint.rebuildablePercent)) of artifacts regenerate",
                        systemImage: "arrow.triangle.2.circlepath",
                        tone: AetowerDesign.Tone.disk
                    )
                    AetowerMetricTile(
                        "Costly/Risky",
                        value: formatBytes(
                            footprint.expensiveBytes.addingReportingOverflow(footprint.riskyBytes).partialValue
                        ),
                        detail: "\(formatBytes(footprint.expensiveBytes)) expensive · \(formatBytes(footprint.riskyBytes)) risky",
                        systemImage: "exclamationmark.triangle",
                        tone: footprint.riskyBytes > 0 ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
                    )
                }
            }
            if let footprint = repository.footprint, !footprint.artifactMix.isEmpty {
                StorageArtifactMixList(artifactMix: footprint.artifactMix)
            }
        }
    }

    @ViewBuilder
    private func repositoryStorageDetailSection(_ repository: RepositorySummary) -> some View {
        let detail = state.repositoryDetailReportsByRoot[repository.root]
        AetowerSection(
            "Storage detail",
            subtitle: "On-demand per-repository drill-down from the storage index"
        ) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                if state.repositoryDetailLoadingRoots.contains(repository.root) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        ProgressView().controlSize(.small)
                        Text("Loading repository storage detail…")
                            .font(AetowerDesign.Typography.caption)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                    }
                } else if let error = state.repositoryDetailErrorsByRoot[repository.root] {
                    AetowerInfoBanner(
                        error,
                        title: "Detail unavailable",
                        systemImage: "exclamationmark.triangle",
                        tone: AetowerDesign.Status.warning,
                        level: .card
                    )
                } else if let detail {
                    if detail.items.isEmpty {
                        Text("No tracked storage items attributed to this repository in the index.")
                            .font(AetowerDesign.Typography.caption)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                    } else {
                        ForEach(detail.items.prefix(8)) { item in
                            HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                                Image(systemName: storageCleanupTierIcon(item.cleanupTier))
                                    .foregroundStyle(storageCleanupTierTone(item.cleanupTier))
                                    .frame(width: 16)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(item.displayName)
                                        .font(AetowerDesign.Typography.controlLabel)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                    Text(item.recommendation)
                                        .font(AetowerDesign.Typography.caption)
                                        .foregroundStyle(AetowerDesign.Ink.secondary)
                                        .lineLimit(2)
                                }
                                Spacer()
                                Text(formatBytes(item.sizeBytes))
                                    .font(AetowerDesign.Typography.caption)
                                    .foregroundStyle(AetowerDesign.Ink.primary)
                            }
                        }
                        if detail.items.count > 8 {
                            Text("+\(detail.items.count - 8) more in the Storage tab's explorer, scoped to this root.")
                                .font(AetowerDesign.Typography.metadata)
                                .foregroundStyle(AetowerDesign.Ink.tertiary)
                        }
                    }
                    ForEach(detail.caveats.prefix(2), id: \.self) { caveat in
                        Text(caveat)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.tertiary)
                    }
                }
                HStack {
                    Button("Refresh detail") {
                        state.loadRepositoryStorageDetail(
                            repoRoot: repository.root,
                            mode: "fast_changed_only",
                            force: true
                        )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(state.repositoryDetailLoadingRoots.contains(repository.root))
                    if let detail {
                        Text("mode: \(detail.scanMode)")
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.tertiary)
                    }
                    Spacer()
                }
            }
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

                Divider()

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

                chau7LaunchStatusRow(launchState)
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
                        if let report, let freshness = report.freshnessLabel() {
                            // Never let a cached supply-chain score read as a
                            // fresh evaluation: show when it was taken, and warn
                            // when the engine flagged it past its fresh window.
                            AetowerBadge(
                                report.isStale ? "Stale · \(freshness)" : freshness,
                                tone: report.isStale ? AetowerDesign.Status.warning : AetowerDesign.Status.neutral
                            )
                        }
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

    /// Shared Chau7 launch-status row (was copy-pasted across the contract and
    /// scorecard remediation panels).
    @ViewBuilder
    private func chau7LaunchStatusRow(_ launchState: Chau7ContractLaunchState?) -> some View {
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

    // Was a hand-rolled duplicate of AetowerMetricTile; delegate to the shared
    // component so scorecard facts match every other metric tile in the app.
    private func scorecardFact(
        _ label: String,
        value: String,
        detail: String,
        icon: String,
        tone: Color
    ) -> some View {
        AetowerMetricTile(label, value: value, detail: detail, systemImage: icon, tone: tone)
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

            chau7LaunchStatusRow(launchState)
        }
    }

    private func scorecardReportFindings(_ report: RepositoryScorecardReportModel) -> some View {
        let failed = report.failedChecks
        let unavailable = report.unavailableChecks
        let recommendations = orderedScorecardRecommendations(report.recommendations)
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
                    ForEach(Array(checks.prefix(5).enumerated()), id: \.offset) { pair in
                        scorecardCheckRow(pair.element)
                    }
                    if checks.count > 5 {
                        DisclosureGroup("Show all \(checks.count)") {
                            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                                ForEach(Array(checks.dropFirst(5).enumerated()), id: \.offset) { pair in
                                    scorecardCheckRow(pair.element)
                                }
                            }
                        }
                        .font(AetowerDesign.Typography.caption)
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
                    ForEach(Array(recommendations.prefix(3).enumerated()), id: \.offset) { pair in
                        scorecardRecommendationRow(pair.element)
                    }
                    if recommendations.count > 3 {
                        DisclosureGroup("Show all \(recommendations.count)") {
                            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                                ForEach(Array(recommendations.dropFirst(3).enumerated()), id: \.offset) { pair in
                                    scorecardRecommendationRow(pair.element)
                                }
                            }
                        }
                        .font(AetowerDesign.Typography.caption)
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
                    AetowerBadge(
                        recommendation.isLocallyActionable ? "Local fix" : "Remote",
                        tone: recommendation.isLocallyActionable
                            ? AetowerDesign.Status.ready
                            : AetowerDesign.Status.neutral
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
                    Text("Next action")
                        .font(AetowerDesign.Typography.sectionTitle)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Text(primaryRepositoryAction(repository).detail)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: AetowerDesign.Spacing.md)
                Menu {
                    Button("Prepare operating contracts in Chau7") {
                        launchPrimaryAethymeContractAction(repository)
                    }
                    .disabled(repository.agentContractCoverage.isEmpty)
                    if let report = repository.scorecardReport {
                        Button("Improve Scorecard posture in Chau7") {
                            launchScorecardRemediationInChau7(repository, report: report)
                        }
                        .disabled(report.failedChecks.isEmpty)
                    } else {
                        Button("Run Scorecard first") {
                            state.runRepositoryScorecard(repoRoot: repository.root, mode: "auto", refresh: false)
                        }
                    }
                    Button("Copy optimization brief") {
                        copy(optimizationBrief(for: repository))
                        copiedRepositoryID = repository.id
                    }
                } label: {
                    Label("Optimize with Aethyme", systemImage: "terminal")
                }
                .buttonStyle(.borderedProminent)
                Button("Reveal") {
                    reveal(repository.root)
                }
                Button("Copy path") {
                    copy(repository.root)
                }
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
                AetowerMetricTile(
                    "AI usage",
                    value: repository.aiRunCount > 0
                        ? String(format: "$%.2f", repository.aiCostUsd)
                        : "None",
                    detail: repository.aiRunCount > 0
                        ? "\(repository.aiRunCount) runs · \(formatTokenCount(repository.aiTotalTokens)) tokens"
                            + (repository.aiProviders.isEmpty ? "" : " · \(repository.aiProviders.joined(separator: ", "))")
                        : "no recorded agent runs in this repository",
                    systemImage: "brain",
                    tone: repository.aiRunCount > 0 ? AetowerDesign.Tone.energy : AetowerDesign.Status.neutral
                )
            }
        }
    }

    private func repositoryAiUsageLabel(_ repository: RepositorySummary) -> String {
        String(format: "$%.2f · %d run%@", repository.aiCostUsd, repository.aiRunCount, repository.aiRunCount == 1 ? "" : "s")
    }

    private func repositoryAiUsageHelp(_ repository: RepositorySummary) -> String {
        var parts = [
            "AI agent usage attributed to this repository:",
            String(format: "$%.2f estimated cost", repository.aiCostUsd),
            "\(repository.aiRunCount) runs",
            "\(formatTokenCount(repository.aiTotalTokens)) tokens",
        ]
        if !repository.aiProviders.isEmpty {
            parts.append("providers: \(repository.aiProviders.joined(separator: ", "))")
        }
        return parts.joined(separator: " · ")
    }

    private var repositoryCountLabel: String {
        guard let report = state.storageHygieneReport else {
            return state.storageHygieneIsLoading ? "Loading" : "0"
        }
        let count = repositorySummaries(from: report).count
        return repositoryInventoryIsIncomplete(report) ? "\(count)+" : "\(count)"
    }

    private var repositoryCountHelp: String {
        guard let report = state.storageHygieneReport else {
            return state.storageHygieneIsLoading
                ? "Repository inventory is currently scanning."
                : "Repository inventory has not been scanned yet."
        }
        let count = repositorySummaries(from: report).count
        if repositoryInventoryIsIncomplete(report) {
            return "\(count) found, scan incomplete. \(repositoryInventoryPartialRootsDetail(report))"
        }
        return "\(count) repositories found."
    }

    private var artifactBytesLabel: String {
        guard let report = state.storageHygieneReport else { return "0 MB" }
        return formatBytes(totalArtifactBytes(repositorySummaries(from: report)))
    }

    private var attentionCountLabel: String {
        guard let report = state.storageHygieneReport else { return "0" }
        let count = repositorySummaries(from: report).filter(\.requiresAttention).count
        return repositoryInventoryIsIncomplete(report) ? "\(count)+" : "\(count)"
    }

    private var attentionCountHelp: String {
        guard let report = state.storageHygieneReport else { return "No repository attention scan has run yet." }
        let count = repositorySummaries(from: report).filter(\.requiresAttention).count
        if repositoryInventoryIsIncomplete(report) {
            return "\(count) attention item\(count == 1 ? "" : "s") found, scan incomplete. Additional repositories may need attention."
        }
        return "\(count) attention item\(count == 1 ? "" : "s") found."
    }

    private var repositoryBadgeTone: Color {
        guard let report = state.storageHygieneReport else { return AetowerDesign.Status.warning }
        return repositoryInventoryIsIncomplete(report) ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
    }

    private var attentionBadgeTone: Color {
        guard let report = state.storageHygieneReport else { return AetowerDesign.Status.neutral }
        if repositoryInventoryIsIncomplete(report) { return AetowerDesign.Status.warning }
        return repositorySummaries(from: report).contains(where: \.requiresAttention)
            ? AetowerDesign.Status.warning
            : AetowerDesign.Status.ready
    }

    private func repositoryInventoryIsIncomplete(_ report: StorageHygieneReportModel) -> Bool {
        !report.repositoryInventoryComplete || report.repositoryInventoryTruncated
    }

    private func repositoryInventoryIncompleteHelp(_ report: StorageHygieneReportModel) -> String {
        let count = repositorySummaries(from: report).count
        return "\(count) found, scan incomplete. \(repositoryInventoryPartialRootsDetail(report))"
    }

    private func repositoryInventoryPartialRootsDetail(_ report: StorageHygieneReportModel) -> String {
        let roots = report.repositoryInventoryPartialRoots
        guard !roots.isEmpty else {
            let staleCount = report.repositoryInventory.filter(\.notSeenInLatestScan).count
            if staleCount > 0 {
                return "\(staleCount) cached repositor\(staleCount == 1 ? "y was" : "ies were") not seen in the latest scan."
            }
            return "Some requested roots were not fully inventoried."
        }
        let visibleRoots = roots.prefix(3).map(shortPath).joined(separator: ", ")
        let remainingCount = roots.count - min(roots.count, 3)
        if remainingCount > 0 {
            return "Partial roots: \(visibleRoots), +\(remainingCount) more."
        }
        return "Partial roots: \(visibleRoots)."
    }

    private func filteredRepositories(from report: StorageHygieneReportModel) -> [RepositorySummary] {
        let query = debouncedSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
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
                    || repository.inventoryCacheStatus.localizedCaseInsensitiveContains(query)
                    || inventoryFreshnessLabel(repository).localizedCaseInsensitiveContains(query)
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
        case .aiSpend:
            if left.aiCostUsd != right.aiCostUsd {
                return left.aiCostUsd > right.aiCostUsd
            }
            if left.aiTotalTokens != right.aiTotalTokens {
                return left.aiTotalTokens > right.aiTotalTokens
            }
            return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
        case .name:
            return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
        }
    }

    private func repositorySummaries(from report: StorageHygieneReportModel) -> [RepositorySummary] {
        summaryCache.summaries(
            inputsGeneration: state.repositorySummaryInputsGeneration,
            snapshotSequence: state.snapshotSequence
        ) {
            let previousSizeByRoot = Dictionary(
                uniqueKeysWithValues: (state.previousStorageHygieneReport?.repoFootprints ?? [])
                    .map { ($0.repoRoot, $0.currentSizeBytes) }
            )
            let baselineSizeByRoot = Dictionary(
                uniqueKeysWithValues: (state.persistedStorageHygieneBaseline?.repoFootprints ?? [])
                    .map { ($0.repoRoot, $0.currentSizeBytes) }
            )
            return RepositorySummaryBuilder.staticSummaries(
                report: report,
                scorecardsByRoot: state.repositoryScorecardReportsByRoot,
                projectForRoot: { state.repositoryProject(forRepoRoot: $0) },
                previousSizeByRoot: previousSizeByRoot,
                baselineSizeByRoot: baselineSizeByRoot,
                hasBaseline: state.persistedStorageHygieneBaseline != nil
            )
        } buildLive: { staticSummaries in
            let live = RepositorySummaryBuilder.liveContexts(
                roots: staticSummaries.map(\.root),
                sessions: state.agentContextState.chau7Sessions,
                entities: state.entitiesState
            )
            return RepositorySummaryBuilder.applyingLive(
                staticSummaries,
                live: live,
                aiUsageByRoot: RepositorySummaryBuilder.aiUsage(byRoot: state.agentContextState.aiRepoSummaries)
            )
        }
    }

    private func totalArtifactBytes(_ repositories: [RepositorySummary]) -> UInt64 {
        repositories.reduce(UInt64(0)) { total, repository in
            total.addingReportingOverflow(repository.artifactBytes).partialValue
        }
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
            let top = orderedScorecardRecommendations(report.recommendations).first
            let headline = top.map { " · \($0.title)" } ?? ""
            return "\(repository.scorecardCriticalFailureCount) critical failed check\(repository.scorecardCriticalFailureCount == 1 ? "" : "s") · score \(score)\(headline)"
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

    private func repositoryGitHubProviderDetail(_ repository: RepositorySummary) -> String {
        guard let status = repository.project?.githubStatus else {
            return "GitHub project status has not been refreshed."
        }
        if status.failedLatestCIOnDefaultBranch {
            return "Latest CI is failing on \(status.defaultBranch ?? "the default branch")."
        }
        let stalePullRequests = status.staleOpenPullRequestCount()
        if stalePullRequests > 0 {
            return "\(stalePullRequests) open PR\(stalePullRequests == 1 ? "" : "s") have not been updated in more than \(RepositoryGitHubProviderStatusModel.stalePullRequestDays) days."
        }
        if status.status == "auth_needed" {
            return "GitHub needs a token with read access; repository attention is unchanged."
        }
        if let warning = status.warnings.first {
            return warning
        }
        return "\(status.openPrCount) open PR\(status.openPrCount == 1 ? "" : "s"); checks \(status.latestCheckState)."
    }

    private func repositoryCloudflareProviderDetail(_ repository: RepositorySummary) -> String {
        guard let project = repository.project else {
            return "Cloudflare deployment status has not been refreshed."
        }
        if let failed = repositoryProjectFailedCloudflareDeployment(project) {
            let deploymentStatus = failed.status.deploymentStatus ?? "failing"
            return "\(failed.environmentName): \(failed.status.resourceName) deployment is \(deploymentStatus)."
        }
        let statuses = project.cloudflareLinks.compactMap { project.cloudflareStatus(for: $0) }
        if let auth = statuses.first(where: { $0.status == "auth_needed" }) {
            return auth.warnings.first ?? "Cloudflare needs an API token with read access; repository attention is unchanged."
        }
        if let warning = statuses.first(where: \.hasRealIssue)?.warnings.first {
            return warning
        }
        return "Cloudflare deployment status has no active issue."
    }

    private func repositoryProjectTone(_ project: RepositoryProjectModel) -> Color {
        if project.githubStatus?.failedLatestCIOnDefaultBranch == true
            || repositoryProjectCloudflareAttentionScore(project) >= 8 {
            return AetowerDesign.Status.error
        }
        if (project.githubStatus?.staleOpenPullRequestCount() ?? 0) > 0
            || repositoryProjectCloudflareAttentionScore(project) >= 5
        {
            return AetowerDesign.Status.warning
        }
        return AetowerDesign.Tone.cpu
    }

    private func repositoryProjectFailedCloudflareDeployment(
        _ project: RepositoryProjectModel
    ) -> (environmentName: String, status: RepositoryCloudflareProviderStatusModel)? {
        for group in project.cloudflareEnvironmentGroups {
            for link in group.links {
                if let status = project.cloudflareStatus(for: link), status.hasFailedDeployment {
                    return (group.name, status)
                }
            }
        }
        return nil
    }

    private func repositoryProjectCloudflareAttentionScore(
        _ project: RepositoryProjectModel
    ) -> Double {
        let failedRank = project.cloudflareEnvironmentGroups.compactMap { group -> Int? in
            group.links.contains {
                project.cloudflareStatus(for: $0)?.hasFailedDeployment == true
            } ? group.rank : nil
        }.max() ?? 0
        if failedRank >= 80 { return 10 }
        if failedRank >= 50 { return 6 }
        if failedRank > 0 { return 3 }
        return 0
    }

    private func repositoryProjectCloudflareGroupLabel(
        _ group: RepositoryProjectCloudflareEnvironmentGroup,
        project: RepositoryProjectModel
    ) -> String {
        let statuses = group.links.compactMap { project.cloudflareStatus(for: $0) }
        if statuses.isEmpty { return "Not tested" }
        if statuses.contains(where: \.hasFailedDeployment) { return "Failed" }
        if statuses.contains(where: { $0.status == "auth_needed" }) { return "Auth" }
        if statuses.contains(where: { repositoryProjectProviderStatusNeedsReview($0.status) }) {
            return "Check"
        }
        return "OK"
    }

    private func repositoryProjectCloudflareGroupTone(
        _ group: RepositoryProjectCloudflareEnvironmentGroup,
        project: RepositoryProjectModel
    ) -> Color {
        let statuses = group.links.compactMap { project.cloudflareStatus(for: $0) }
        if statuses.contains(where: \.hasFailedDeployment) {
            return group.rank >= 80 ? AetowerDesign.Status.error : AetowerDesign.Status.warning
        }
        if statuses.contains(where: { repositoryProjectProviderStatusNeedsReview($0.status) }) {
            return AetowerDesign.Status.warning
        }
        return statuses.isEmpty ? AetowerDesign.Status.neutral : AetowerDesign.Status.ready
    }

    private func repositoryProjectProviderStatusNeedsReview(_ status: String) -> Bool {
        ["auth_needed", "failed", "unavailable", "warning"].contains(status)
    }

    private func repositoryCloudflareProviderTone(_ repository: RepositorySummary) -> Color {
        guard let project = repository.project else { return AetowerDesign.Status.neutral }
        if repository.cloudflareProviderAttentionScore >= 8 {
            return AetowerDesign.Status.error
        }
        if repository.cloudflareProviderAttentionScore >= 5 {
            return AetowerDesign.Status.warning
        }
        let statuses = project.cloudflareLinks.compactMap { project.cloudflareStatus(for: $0) }
        if statuses.contains(where: \.hasFailedDeployment) {
            return AetowerDesign.Status.warning
        }
        if statuses.contains(where: { repositoryProjectProviderStatusNeedsReview($0.status) }) {
            return AetowerDesign.Status.warning
        }
        return AetowerDesign.Status.ready
    }

    private func repositoryCloudflareProviderTone(
        _ status: RepositoryCloudflareProviderStatusModel?,
        error: String? = nil,
        isLoading: Bool = false
    ) -> Color {
        if isLoading { return AetowerDesign.Tone.network }
        if error != nil { return AetowerDesign.Status.warning }
        guard let status else { return AetowerDesign.Tone.network }
        if status.hasFailedDeployment || status.status == "failed" {
            return AetowerDesign.Status.error
        }
        if ["auth_needed", "unavailable", "warning"].contains(status.status) {
            return AetowerDesign.Status.warning
        }
        return AetowerDesign.Status.ready
    }

    private func repositoryProjectCloudflareStatusLevel(
        status: RepositoryCloudflareProviderStatusModel?,
        error: String?
    ) -> AetowerSurfaceLevel {
        if error != nil { return .warning }
        guard let status else { return .quiet }
        if status.hasFailedDeployment {
            return .critical
        }
        if ["auth_needed", "failed", "unavailable", "warning"].contains(status.status) {
            return .warning
        }
        return .quiet
    }

    private func repositoryProjectCloudflareStatusLabel(
        _ status: RepositoryCloudflareProviderStatusModel?,
        isLoading: Bool,
        error: String?
    ) -> String {
        if isLoading { return "Cloudflare refreshing" }
        if error != nil { return "Cloudflare issue" }
        guard let status else { return "Cloudflare not tested" }
        switch status.status {
        case "ok":
            return "Cloudflare ok"
        case "auth_needed":
            return "Needs auth"
        case "unavailable":
            return "Unavailable"
        case "failed":
            return "Failed"
        default:
            return "Warning"
        }
    }

    private func repositoryProjectCloudflareCapturedLabel(
        _ status: RepositoryCloudflareProviderStatusModel?
    ) -> String {
        guard let status, status.capturedAtMillis > 0 else { return "Never" }
        let date = Date(timeIntervalSince1970: Double(status.capturedAtMillis) / 1000.0)
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    private func repositoryProjectCloudflareLinkTitle(_ link: RepositoryProjectLinkModel) -> String {
        let resource = link.cloudflareResourceName ?? "resource"
        switch link.kind {
        case .pages:
            return "Pages \(resource)"
        case .worker:
            return "Worker \(resource)"
        default:
            return resource
        }
    }

    private func shortCommitLabel(_ commit: String?) -> String {
        guard let commit, !commit.isEmpty else { return "Unavailable" }
        return String(commit.prefix(8))
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

    private func repositoryProject(for repository: RepositorySummary) -> RepositoryProjectModel? {
        repository.project ?? state.repositoryProject(forRepoRoot: repository.root)
    }

    private func createProject(from repository: RepositorySummary) {
        state.upsertRepositoryProject(baseRepositoryProject(for: repository))
        expandedProjectSectionRepositoryID = repository.id
    }

    private func linkGitHubProject(_ repository: RepositorySummary) {
        guard let link = githubProjectLink(for: repository) else { return }
        upsertRepositoryProject(for: repository, adding: link)
    }

    private func linkCloudflareProject(
        _ repository: RepositorySummary,
        kind: RepositoryCloudflareLinkKind,
        environmentName: String,
        rank: Int,
        accountID: String,
        resourceName: String,
        deploymentEnvironment: String?,
        branch: String?
    ) {
        let link: RepositoryProjectLinkModel
        switch kind {
        case .pages:
            link = .cloudflarePages(
                accountId: accountID,
                projectName: resourceName,
                deploymentEnvironment: deploymentEnvironment,
                branch: branch
            )
        case .worker:
            link = .cloudflareWorker(
                accountId: accountID,
                scriptName: resourceName,
                branch: branch
            )
        }
        upsertRepositoryProject(
            for: repository,
            adding: link,
            environmentName: environmentName,
            rank: rank
        )
    }

    private func upsertRepositoryProject(
        for repository: RepositorySummary,
        adding link: RepositoryProjectLinkModel,
        environmentName: String? = nil,
        rank: Int = 0
    ) {
        var project = repositoryProject(for: repository) ?? baseRepositoryProject(for: repository)
        if project.repoRemote.isEmpty {
            project.repoRemote = repositoryRemoteURL(repository)
        }
        if link.provider == .cloudflare, let environmentName {
            project.upsertCloudflareLink(link, environmentName: environmentName, rank: rank)
        } else if !project.links.contains(where: { $0.identityKey == link.identityKey }) {
            project.links.append(link)
        }
        state.upsertRepositoryProject(project)
        expandedProjectSectionRepositoryID = repository.id
    }

    private func baseRepositoryProject(for repository: RepositorySummary) -> RepositoryProjectModel {
        RepositoryProjectModel(
            name: repository.name,
            primaryRepoRoot: repository.root,
            repoRemote: repositoryRemoteURL(repository)
        )
    }

    private func githubProjectLink(for repository: RepositorySummary) -> RepositoryProjectLinkModel? {
        guard let owner = repository.gitRemoteOwner,
              let repo = repository.gitRemoteName,
              !owner.isEmpty,
              !repo.isEmpty,
              repositoryRemoteIsGitHub(repository)
        else {
            return nil
        }
        return .githubRepository(owner: owner, repo: repo)
    }

    private func repositoryRemoteIsGitHub(_ repository: RepositorySummary) -> Bool {
        let host = repository.gitRemoteHost?.lowercased() ?? ""
        let remote = repositoryRemoteURL(repository).lowercased()
        return host == "github.com" || remote.contains("github.com")
    }

    private func repositoryRemoteURL(_ repository: RepositorySummary) -> String {
        repository.gitRemoteOriginUrl ?? repository.gitRemoteKey ?? ""
    }

    private func repositoryProjectLinkLabel(_ link: RepositoryProjectLinkModel) -> String {
        switch (link.provider, link.kind) {
        case (.github, .repository):
            return "GitHub"
        case (.cloudflare, .pages):
            return "Pages"
        case (.cloudflare, .worker):
            return "Worker"
        default:
            return link.provider.rawValue.capitalized
        }
    }

    private func repositoryProjectLinkIcon(_ link: RepositoryProjectLinkModel) -> String {
        switch link.provider {
        case .github:
            return "chevron.left.forwardslash.chevron.right"
        case .cloudflare:
            return "cloud"
        }
    }

    private func repositoryProjectLinkTone(_ link: RepositoryProjectLinkModel) -> Color {
        switch link.provider {
        case .github:
            return AetowerDesign.Tone.cpu
        case .cloudflare:
            return AetowerDesign.Tone.network
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

private struct RepositoryCloudflareLinkSheet: View {
    typealias SaveHandler = (
        RepositoryCloudflareLinkKind,
        String,
        Int,
        String,
        String,
        String?,
        String?
    ) -> Void

    let repositoryName: String
    let onCancel: () -> Void
    let onSave: SaveHandler

    @State private var kind: RepositoryCloudflareLinkKind = .pages
    @State private var preset: RepositoryCloudflareEnvironmentPreset = .production
    @State private var environmentName = RepositoryCloudflareEnvironmentPreset.production.defaultName
    @State private var pagesDeploymentEnvironment: RepositoryCloudflarePagesDeploymentEnvironment = .production
    @State private var accountID = ""
    @State private var resourceName = ""
    @State private var branch = ""

    private var canSave: Bool {
        !environmentName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !accountID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !resourceName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
            HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                Label("Link Cloudflare", systemImage: "cloud")
                    .font(AetowerDesign.Typography.sectionTitle)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                Spacer(minLength: AetowerDesign.Spacing.md)
                Button(action: onCancel) {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.borderless)
                .keyboardShortcut(.cancelAction)
            }

            Text(repositoryName)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(1)
                .truncationMode(.tail)

            Picker("Environment preset", selection: $preset) {
                ForEach(RepositoryCloudflareEnvironmentPreset.allCases) { option in
                    Text(option.label).tag(option)
                }
            }
            .pickerStyle(.segmented)
            .onChange(of: preset) { _, value in
                applyPreset(value)
            }

            TextField("Environment name", text: $environmentName)
                .aetowerUtilityTextInput()
                .onChange(of: environmentName) { _, value in
                    if preset != .custom, value != preset.defaultName {
                        preset = .custom
                    }
                }

            Picker("Cloudflare kind", selection: $kind) {
                ForEach(RepositoryCloudflareLinkKind.allCases) { option in
                    Text(option.label).tag(option)
                }
            }
            .pickerStyle(.segmented)

            if kind == .pages {
                Picker("Pages deployment type", selection: $pagesDeploymentEnvironment) {
                    ForEach(RepositoryCloudflarePagesDeploymentEnvironment.allCases) { option in
                        Text(option.label).tag(option)
                    }
                }
                .pickerStyle(.segmented)
            }

            TextField("Account ID", text: $accountID)
                .aetowerUtilityTextInput()
            TextField(resourceNamePrompt, text: $resourceName)
                .aetowerUtilityTextInput()
            TextField("Branch or ref", text: $branch)
                .aetowerUtilityTextInput()

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Spacer(minLength: AetowerDesign.Spacing.md)
                Button("Cancel", role: .cancel, action: onCancel)
                Button {
                    onSave(
                        kind,
                        environmentName,
                        preset.rank,
                        accountID,
                        resourceName,
                        kind == .pages ? pagesDeploymentEnvironment.apiValue : nil,
                        sanitizedOptional(branch)
                    )
                } label: {
                    Label("Link", systemImage: "link")
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(!canSave)
            }
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.xxl)
        .frame(width: 420)
    }

    private func applyPreset(_ preset: RepositoryCloudflareEnvironmentPreset) {
        guard preset != .custom else { return }
        environmentName = preset.defaultName
        pagesDeploymentEnvironment = .selection(for: preset.pagesDeploymentEnvironment)
    }

    private func sanitizedOptional(_ value: String) -> String? {
        let sanitized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return sanitized.isEmpty ? nil : sanitized
    }

    private var resourceNamePrompt: String {
        switch kind {
        case .pages:
            return "Pages project name"
        case .worker:
            return "Worker script name"
        }
    }
}
