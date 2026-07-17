import AppKit
import Foundation
import Observation
import SwiftUI
import AetowerBridge

public struct SettingsView: View {
    let state: AppState
    let settings: SettingsStore
    var nav: NavigationModel?
    @Environment(UpdaterController.self) private var updater
    @FocusState private var focusedField: SettingsField?
    @State private var selectedSection: SettingsSection = .setup
    @State private var appliedIntegrationSnapshot: SettingsIntegrationSnapshot?
    @State private var applyConfirmation: String?
    @State private var showAdvancedCollectionControls = false
    @State private var showResetLocalDataConfirmation = false
    @State private var editingAutomationRules: [AutomationRule] = []
    @State private var hasLoadedIntegrationDraft = false
    @State private var searchText = ""
    @State private var repositoryRootDraft = ""
    /// In-memory integration draft. Values are written to SettingsStore/Keychain
    /// only when the user applies them, so endpoint edits are genuinely staged.
    @State private var integrationDraft = SettingsIntegrationDraft()
    @State private var browserAttributionState: BrowserAttributionViewState = .notConfigured

    public init(state: AppState, settings: SettingsStore, nav: NavigationModel? = nil) {
        self.state = state
        self.settings = settings
        self.nav = nav
    }

    private enum SettingsField: Hashable {
        case chromiumEndpoint
        case dockerSocketPath
        case privilegedHelperPath
        case chau7Endpoint
        case chau7AgentCommand
        case telemetryEndpoint
        case githubOAuthClientID
        case githubOAuthScopes
        case githubToken
        case cloudflareToken
        case repositoryRoot
    }

    private enum SettingsSection: String, CaseIterable, Identifiable {
        case setup
        case general
        case collection
        case repositories
        case integrations
        case aiClients
        case notifications
        case automation
        case privacy
        case updates
        case advanced

        var id: Self { self }

        var title: String {
            switch self {
            case .setup: return "Setup"
            case .general: return "General"
            case .collection: return "Collection"
            case .repositories: return "Repositories"
            case .integrations: return "Integrations"
            case .aiClients: return "AI Clients"
            case .notifications: return "Notifications"
            case .automation: return "Automation"
            case .privacy: return "Privacy"
            case .updates: return "Updates"
            case .advanced: return "Advanced"
            }
        }

        var systemImage: String {
            switch self {
            case .setup: return "checklist"
            case .general: return "slider.horizontal.2.square"
            case .collection: return "gauge.with.needle"
            case .repositories: return "folder.badge.gearshape"
            case .integrations: return "point.3.connected.trianglepath.dotted"
            case .aiClients: return "cpu"
            case .notifications: return "bell.badge"
            case .automation: return "bolt.badge.automatic"
            case .privacy: return "hand.raised"
            case .updates: return "sparkles"
            case .advanced: return "wrench.and.screwdriver"
            }
        }
    }

    private struct IntegrationValidation {
        let issues: [SettingsValidationIssue]

        @MainActor
        init(draft: SettingsIntegrationDraft) {
            self.issues = SettingsValidationIssue.integrationIssues(for: draft)
        }

        var hasBlockingIssues: Bool {
            issues.contains { $0.severity == .error }
        }

        func issues(for target: SettingsValidationTarget) -> [SettingsValidationIssue] {
            issues.filter { $0.target == target }
        }
    }

    public var body: some View {
        @Bindable var settings = settings
        VStack(spacing: AetowerDesign.Spacing.none) {
            settingsTabToolBand
            Divider()
            HStack(spacing: AetowerDesign.Spacing.none) {
                settingsSidebar
                Divider()
                ScrollView {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xl) {
                        if settingsSearchQuery.isEmpty || visibleSettingsSections.contains(selectedSection) {
                            selectedSectionContent
                        } else {
                            settingsSearchEmpty
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(AetowerDesign.Spacing.xxl)
                }
            }
        }
        .navigationTitle("Settings")
        .onAppear {
            loadIntegrationDraftIfNeeded()
            if appliedIntegrationSnapshot == nil {
                appliedIntegrationSnapshot = currentIntegrationSnapshot
            }
            refreshBrowserAttributionStatusIfConfigured()
        }
        .onDisappear {
            focusedField = nil
        }
        .alert("Reset Aetower local data?", isPresented: $showResetLocalDataConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Reset local data", role: .destructive) {
                resetLocalAetowerData()
            }
        } message: {
            Text("This clears persisted history and diagnostics, restores conservative defaults, and reapplies runtime settings. It does not delete exported support bundles or rewrite existing AI-client configuration files.")
        }
    }

    private var settingsTabToolBand: some View {
        AetowerTabToolBand(
            searchText: $searchText,
            searchPrompt: "Search settings sections",
            searchWidth: 280
        ) {
            AetowerSelectionMenu(
                selection: $selectedSection,
                options: SettingsSection.allCases,
                accessibilityLabel: "Settings section",
                title: { $0.title },
                systemImage: { $0.systemImage }
            )
        } badges: {
            AetowerToolBadgeGroup(settingsHeaderBadges, visibleCount: 2)
        }
    }

    private var settingsHeaderBadges: [AetowerToolBadgeItem] {
        let progress = setupChecklistProgress
        return [
            AetowerToolBadgeItem(
                "Setup",
                value: "\(progress.completed)/\(progress.total)",
                systemImage: "checklist",
                tone: progress.completed == progress.total ? AetowerDesign.Status.success : AetowerDesign.Status.warning
            ),
            AetowerToolBadgeItem(
                "Integrations",
                value: hasPendingIntegrationChanges ? "Pending" : "Applied",
                systemImage: "point.3.connected.trianglepath.dotted",
                tone: hasPendingIntegrationChanges ? AetowerDesign.Status.warning : AetowerDesign.Status.success
            ),
            AetowerToolBadgeItem(
                "Safe mode",
                value: settings.operatorSafeModeEnabled ? "On" : "Off",
                systemImage: "shield.lefthalf.filled",
                tone: settings.operatorSafeModeEnabled ? AetowerDesign.Status.success : AetowerDesign.Status.warning
            ),
        ]
    }

    private var settingsSidebar: some View {
        AetowerNavigationRail(width: 238) {
            ForEach(visibleSettingsSections) { section in
                let status = status(for: section)
                AetowerSettingsSidebarButton(
                    title: section.title,
                    systemImage: section.systemImage,
                    status: status.label,
                    statusTone: status.color,
                    isSelected: selectedSection == section
                ) {
                    selectedSection = section
                }
            }

            if visibleSettingsSections.isEmpty {
                AetowerEmptyState(
                    title: "No section",
                    detail: "Clear search to show all settings.",
                    systemImage: "magnifyingglass"
                )
            }
        }
    }

    private var settingsSearchQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var visibleSettingsSections: [SettingsSection] {
        let query = settingsSearchQuery
        guard !query.isEmpty else {
            return SettingsSection.allCases
        }
        return SettingsSection.allCases.filter { section in
            settingsSectionSearchText(section).localizedCaseInsensitiveContains(query)
        }
    }

    private func settingsSectionSearchText(_ section: SettingsSection) -> String {
        let status = status(for: section)
        switch section {
        case .setup:
            return "\(section.title) checklist readiness first run \(status.label)"
        case .general:
            return "\(section.title) launch window app behavior appearance menu bar \(status.label)"
        case .collection:
            return "\(section.title) sampling refresh cadence sensors metrics profile \(status.label)"
        case .repositories:
            return "\(section.title) repository roots git inventory scans folders paths \(status.label)"
        case .integrations:
            return "\(section.title) Chau7 Docker Chromium telemetry browser helper \(status.label)"
        case .aiClients:
            return "\(section.title) MCP Claude Codex clients registration \(status.label)"
        case .notifications:
            return "\(section.title) alerts warnings friction thermal network \(status.label)"
        case .automation:
            return "\(section.title) shortcuts rules budget actions \(status.label)"
        case .privacy:
            return "\(section.title) outbound data local first export redaction telemetry fleet virustotal provider tokens mcp privacy \(status.label)"
        case .updates:
            return "\(section.title) Sparkle release update download \(status.label)"
        case .advanced:
            return "\(section.title) reset diagnostics capabilities support \(status.label)"
        }
    }

    private var settingsSearchEmpty: some View {
        AetowerEmptyState(
            title: "No settings section selected",
            detail: "The current section is hidden by the header search. Pick a visible section or clear search.",
            systemImage: "magnifyingglass",
            tone: AetowerDesign.Status.warning
        )
    }

    private func status(for section: SettingsSection) -> SettingsStatus {
        switch section {
        case .setup:
            let progress = setupChecklistProgress
            if progress.completed == progress.total {
                return SettingsStatus("Ready", AetowerDesign.Status.success)
            }
            return SettingsStatus("\(progress.completed)/\(progress.total)", AetowerDesign.Status.warning)
        case .general:
            return SettingsStatus("Live", AetowerDesign.Status.success)
        case .collection:
            return SettingsStatus("Live", AetowerDesign.Status.success)
        case .repositories:
            return SettingsStatus(
                "\(settings.repositoryRoots.count) roots",
                settings.repositoryRoots.contains { !SettingsStore.repositoryRootExists($0) }
                    ? AetowerDesign.Status.warning
                    : AetowerDesign.Status.success
            )
        case .integrations:
            return hasPendingIntegrationChanges
                ? SettingsStatus("Pending Apply", AetowerDesign.Status.warning)
                : SettingsStatus("Applied", AetowerDesign.Status.success)
        case .aiClients:
            if settings.localMcpOperatorActionsEnabled {
                return SettingsStatus("Operator", AetowerDesign.Status.warning)
            }
            return settings.autoRegisterLocalMcpClientsEnabled
                ? SettingsStatus("Auto", AetowerDesign.Status.ready)
                : SettingsStatus("Read-only", AetowerDesign.Status.success)
        case .notifications:
            return settings.notificationsEnabled
                ? SettingsStatus("Enabled", AetowerDesign.Status.success)
                : SettingsStatus("Off", AetowerDesign.Status.neutral)
        case .automation:
            let active = state.automationRules.filter(\.enabled).count
            return active > 0
                ? SettingsStatus("\(active) active", AetowerDesign.Status.success)
                : SettingsStatus("Off", AetowerDesign.Status.neutral)
        case .privacy:
            return SettingsStatus(settings.exportPrivacyTier.rawValue.capitalized, AetowerDesign.Status.ready)
        case .updates:
            return updater.isConfigured
                ? SettingsStatus("Ready", AetowerDesign.Status.success)
                : SettingsStatus("Disabled", AetowerDesign.Status.neutral)
        case .advanced:
            return SettingsStatus("Advanced", AetowerDesign.Status.warning)
        }
    }

    private var hasPendingIntegrationChanges: Bool {
        guard let appliedIntegrationSnapshot else {
            return false
        }
        return appliedIntegrationSnapshot != currentIntegrationSnapshot
    }

    private var repositoryRootDraftCanBeAdded: Bool {
        let trimmed = repositoryRootDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return false }
        return SettingsStore.normalizedRepositoryRoots(settings.repositoryRoots + [trimmed]).count
            > settings.repositoryRoots.count
    }

    private func addRepositoryRootDraft() {
        guard repositoryRootDraftCanBeAdded else { return }
        settings.addRepositoryRoot(repositoryRootDraft)
        repositoryRootDraft = ""
        focusedField = .repositoryRoot
    }

    private var currentIntegrationSnapshot: SettingsIntegrationSnapshot {
        SettingsIntegrationSnapshot(integrationDraft)
    }

    private var persistedIntegrationSnapshot: SettingsIntegrationSnapshot {
        SettingsIntegrationSnapshot(settings)
    }

    private var browserAttributionEndpoint: String {
        integrationDraft.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var browserAttributionIsBusy: Bool {
        switch browserAttributionState {
        case .checking, .launching:
            return true
        case .notConfigured, .connected, .failed:
            return false
        }
    }

    private var browserAttributionPrimaryActionTitle: String {
        switch browserAttributionState {
        case .launching:
            return "Opening Chrome..."
        case .checking:
            return "Checking..."
        case .connected:
            return "Open Dedicated Chrome"
        case .failed, .notConfigured:
            return browserAttributionEndpoint.isEmpty ? "Enable Browser Attribution" : "Open Dedicated Chrome"
        }
    }

    private var browserAttributionPresentation: (badge: String, color: Color, detail: String) {
        switch browserAttributionState {
        case .launching:
            return ("Opening", AetowerDesign.Status.ready, "Opening a dedicated Chrome profile and waiting for the local debug endpoint.")
        case .checking:
            return ("Checking", AetowerDesign.Status.ready, "Testing the configured browser debug endpoint.")
        case .connected(let summary) where summary.endpoint == browserAttributionEndpoint:
            let pageLabel = summary.pageTargetCount == 1 ? "tab" : "tabs"
            return (
                "Connected",
                AetowerDesign.Status.success,
                "Connected to \(summary.pageTargetCount) \(pageLabel) across \(summary.totalTargetCount) debug targets."
            )
        case .failed(let message):
            return ("Attention", AetowerDesign.Status.warning, message)
        case .connected, .notConfigured:
            if browserAttributionEndpoint.isEmpty {
                return (
                    "Not enabled",
                    AetowerDesign.Status.neutral,
                    "Browser tab attribution is off. Use the button above to open a dedicated Chrome window."
                )
            }
            return (
                "Configured",
                AetowerDesign.Status.ready,
                "An endpoint is configured. Test it, or apply the staged settings to use it."
            )
        }
    }

    private var setupChecklistProgress: (completed: Int, total: Int) {
        let states = [
            settings.operatorSafeModeEnabled,
            true,
            isCapabilityReady(.chau7),
            hasInstalledCommandLineTool,
            hasRegisteredLocalMcpClient,
            settings.exportPrivacyTier != .full,
            updater.isConfigured,
        ]
        return (states.filter { $0 }.count, states.count)
    }

    private var hasRegisteredLocalMcpClient: Bool {
        return state.localMcpClientStatuses.contains { status in
            if case .registered = status.state {
                return true
            }
            return false
        }
    }

    private var hasInstalledCommandLineTool: Bool {
        CommandLineToolInstaller.currentState() == .installed
    }

    private var oneClickMcpClientStatuses: [LocalMcpClientRegistrationStatus] {
        state.localMcpClientStatuses.filter { status in
            status.supportsAutomaticRegistration && status.isInstalled
        }
    }

    private var oneClickMcpPendingClientStatuses: [LocalMcpClientRegistrationStatus] {
        oneClickMcpClientStatuses.filter { status in
            status.state == .availableForAutomaticRegistration
        }
    }

    private var oneClickMcpDetectedClientLabel: String {
        let names = oneClickMcpClientStatuses.map(\.displayName)
        guard !names.isEmpty else { return "No supported Claude or Codex client detected yet." }
        return "Detected: \(names.joined(separator: ", "))"
    }

    private var virusTotalKeyConfigured: Bool {
        KeychainHelper.exists(account: KeychainHelper.binaryReputationAccount)
    }

    private var providerCredentialSnapshots: [ProviderCredentialSnapshot] {
        let store = ProviderCredentialStore()
        return ProjectProvider.allCases.map { provider in
            let source = store.credentialSource(for: provider)
            return ProviderCredentialSnapshot(
                provider: provider,
                source: source,
                canAuthenticate: source != .none
                    && (
                        store.resolvedAccessToken(for: provider) != nil
                            || store.credentialNeedsRefresh(for: provider)
                    )
            )
        }
    }

    private var configuredProviderCredentials: [ProviderCredentialSnapshot] {
        providerCredentialSnapshots.filter { $0.source != .none }
    }

    private var activeProviderCredentials: [ProviderCredentialSnapshot] {
        configuredProviderCredentials.filter(\.canAuthenticate)
    }

    private var registeredMcpClientNames: [String] {
        state.localMcpClientStatuses.compactMap { status in
            status.state == .registered ? status.displayName : nil
        }
    }

    private var outboundDataSnapshot: OutboundDataSnapshot {
        let rows = outboundDataRows
        return OutboundDataSnapshot(
            rows: rows,
            outboundRows: rows.filter(\.canLeaveMac),
            registeredMcpClientNames: registeredMcpClientNames
        )
    }

    private var outboundDataRows: [OutboundDataRow] {
        let telemetryEndpoint = SettingsStore.normalizedTelemetryEndpoint(settings.telemetryEndpoint)
        let telemetryInterval = SettingsStore.normalizedTelemetryExportIntervalSeconds(
            settings.telemetryExportIntervalSeconds
        )
        let configuredProviders = configuredProviderCredentials
        let activeProviders = activeProviderCredentials
        let providerDetail: String
        if configuredProviders.isEmpty {
            providerDetail = "No GitHub or Cloudflare token is configured. Provider refreshes and provider actions cannot authenticate."
        } else if activeProviders.isEmpty {
            providerDetail = "\(configuredProviders.map(providerCredentialSummary).joined(separator: ", ")) configured, but no usable provider credential is currently available. Provider refreshes and actions will prompt for reconnection before contacting remote APIs."
        } else if activeProviders.count == configuredProviders.count {
            providerDetail = "\(configuredProviders.map(providerCredentialSummary).joined(separator: ", ")) configured. Aetower contacts provider APIs only for linked project/deployment refreshes and explicit provider actions."
        } else {
            providerDetail = "\(configuredProviders.map(providerCredentialSummary).joined(separator: ", ")) configured; ready: \(activeProviders.map(providerCredentialSummary).joined(separator: ", ")). Aetower contacts provider APIs only when a usable credential exists."
        }
        let mcpNames = registeredMcpClientNames
        let mcpDetail = mcpNames.isEmpty
            ? "No supported local AI client is registered. Aetower still runs its local MCP server for explicit local use."
            : "\(mcpNames.joined(separator: ", ")) can launch Aetower's local MCP proxy. Data stays on this Mac unless that local client forwards it elsewhere; operator actions are \(settings.localMcpOperatorActionsEnabled ? "visible with approval gates" : "hidden")."

        return [
            OutboundDataRow(
                id: "telemetry",
                title: "Telemetry",
                badge: settings.telemetryEnabled ? "On" : "Off",
                badgeColor: settings.telemetryEnabled ? AetowerDesign.Status.warning : AetowerDesign.Status.success,
                detail: settings.telemetryEnabled
                    ? "Exports host and entity metrics to \(telemetryEndpoint) every \(telemetryInterval) seconds."
                    : "No observability metrics are exported.",
                systemImage: "arrow.up.forward.circle",
                tone: .network,
                canLeaveMac: settings.telemetryEnabled
            ),
            OutboundDataRow(
                id: "fleet",
                title: "Fleet",
                badge: settings.fleetEnabled ? "On" : "Off",
                badgeColor: settings.fleetEnabled ? AetowerDesign.Status.warning : AetowerDesign.Status.success,
                detail: settings.fleetEnabled
                    ? "Advertises this Mac on the trusted local network and can serve current snapshots to nearby Aetower peers with Fleet enabled."
                    : "This Mac is not advertising Fleet snapshots on the local network.",
                systemImage: "network",
                tone: .network,
                canLeaveMac: settings.fleetEnabled
            ),
            OutboundDataRow(
                id: "virustotal",
                title: "VirusTotal",
                badge: virusTotalBadge,
                badgeColor: virusTotalBadgeColor,
                detail: virusTotalDetail,
                systemImage: "checkmark.shield",
                tone: .warning,
                canLeaveMac: settings.binaryReputationEnabled && virusTotalKeyConfigured
            ),
            OutboundDataRow(
                id: "provider-tokens",
                title: "Provider tokens",
                badge: configuredProviders.isEmpty ? "None" : "\(configuredProviders.count) configured",
                badgeColor: configuredProviders.isEmpty ? AetowerDesign.Status.success : AetowerDesign.Status.ready,
                detail: providerDetail,
                systemImage: "key",
                tone: .ready,
                canLeaveMac: !activeProviders.isEmpty
            ),
            OutboundDataRow(
                id: "mcp-clients",
                title: "MCP registered clients",
                badge: mcpNames.isEmpty ? "None" : "\(mcpNames.count) registered",
                badgeColor: mcpNames.isEmpty ? AetowerDesign.Status.success : AetowerDesign.Status.ready,
                detail: mcpDetail,
                systemImage: "cpu",
                tone: .cpu,
                canLeaveMac: false
            ),
        ]
    }

    private var virusTotalBadge: String {
        guard settings.binaryReputationEnabled else { return "Off" }
        return virusTotalKeyConfigured ? "On" : "Needs key"
    }

    private var virusTotalBadgeColor: Color {
        guard settings.binaryReputationEnabled else { return AetowerDesign.Status.success }
        return virusTotalKeyConfigured ? AetowerDesign.Status.warning : AetowerDesign.Status.ready
    }

    private var virusTotalDetail: String {
        guard settings.binaryReputationEnabled else {
            return "No binary hashes are sent to VirusTotal."
        }
        guard virusTotalKeyConfigured else {
            return "The lookup consent is enabled, but no API key is saved. Aetower cannot perform VirusTotal lookups until a key is added."
        }
        return "For unsigned or ad-hoc binaries with network activity, Aetower sends only the executable SHA-256 hash to VirusTotal; it never uploads the file, path, or host context."
    }

    private func isCapabilityReady(_ kind: CapabilityKind) -> Bool {
        guard let capability = state.capabilitiesState.first(where: { $0.kind == kind }) else {
            return false
        }
        if case .granted = capability.state {
            return true
        }
        switch capability.health {
        case .live, .configured:
            return true
        case .cached, .degraded:
            return false
        }
    }

    @ViewBuilder
    private var selectedSectionContent: some View {
        switch selectedSection {
        case .setup:
            setupSection
        case .general:
            generalSection
        case .collection:
            collectionSection
        case .repositories:
            repositoriesSection
        case .integrations:
            integrationsSection
        case .aiClients:
            aiClientsSection
        case .notifications:
            notificationsSection
        case .automation:
            automationSection
        case .privacy:
            privacySection
        case .updates:
            updatesSection
        case .advanced:
            advancedSection
        }
    }

    @ViewBuilder
    private var setupSection: some View {
        let progress = setupChecklistProgress
        SettingsCard(
            title: "First-run setup",
            subtitle: "A short checklist for making Aetower useful without forcing every advanced option.",
            status: status(for: .setup)
        ) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                Text("\(progress.completed) of \(progress.total) recommended steps are ready.")
                    .font(.headline)
                ProgressView(value: Double(progress.completed), total: Double(progress.total))
            }

            localMcpFirstRunConsentCard

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                SettingsChecklistRow(
                    title: "Keep heavy views safe",
                    detail: "Operator-safe mode keeps History and Timeline summary-first so large payloads do not freeze the UI.",
                    isComplete: settings.operatorSafeModeEnabled,
                    actionTitle: "Review General"
                ) {
                    selectedSection = .general
                }

                SettingsChecklistRow(
                    title: "Choose collection behavior",
                    detail: "Collection has a live preset. Tune it only when you need more detail or lower overhead.",
                    isComplete: true,
                    actionTitle: "Review Collection"
                ) {
                    selectedSection = .collection
                }

                SettingsChecklistRow(
                    title: "Connect Chau7",
                    detail: "Chau7 data enriches sessions with terminal tabs, repositories, branches, and AI-agent context.",
                    isComplete: isCapabilityReady(.chau7),
                    actionTitle: "Review Integrations"
                ) {
                    selectedSection = .integrations
                }

                SettingsChecklistRow(
                    title: "Install the aetower CLI",
                    detail: "PKG and Homebrew installs should put aetower on PATH automatically; DMG/ZIP installs can use the in-app installer.",
                    isComplete: hasInstalledCommandLineTool,
                    actionTitle: "Review AI Clients"
                ) {
                    selectedSection = .aiClients
                }

                SettingsChecklistRow(
                    title: "Register MCP with local AI clients",
                    detail: "One-click registration makes Aetower discoverable from supported Claude and Codex clients after explicit consent.",
                    isComplete: hasRegisteredLocalMcpClient,
                    actionTitle: "Review AI Clients"
                ) {
                    selectedSection = .aiClients
                }

                SettingsChecklistRow(
                    title: "Keep exports safe",
                    detail: "Redacted or operator privacy tiers are safer defaults for support bundles and JSON exports.",
                    isComplete: settings.exportPrivacyTier != .full,
                    actionTitle: "Review Privacy"
                ) {
                    selectedSection = .privacy
                }

                SettingsChecklistRow(
                    title: "Confirm direct-download updates",
                    detail: "Sparkle must be configured in packaged builds before users can receive signed updates.",
                    isComplete: updater.isConfigured,
                    actionTitle: "Review Updates"
                ) {
                    selectedSection = .updates
                }
            }
        }
    }

    private var localMcpFirstRunConsentCard: some View {
        let pendingTargets = oneClickMcpPendingClientStatuses
        let isRegistered = hasRegisteredLocalMcpClient
        return SettingsRowCard {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: isRegistered ? "checkmark.shield.fill" : "person.crop.circle.badge.plus")
                    .foregroundStyle(isRegistered ? AetowerDesign.Status.success : AetowerDesign.Status.ready)
                    .frame(width: 24)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text("Register Aetower MCP with Claude/Codex")
                            .font(AetowerDesign.Typography.sectionTitle)
                        SettingsBadge(
                            isRegistered ? "Registered" : "Consent required",
                            color: isRegistered ? AetowerDesign.Status.success : AetowerDesign.Status.ready
                        )
                    }
                    Text("One-click registration writes Aetower's local MCP proxy into supported Claude and Codex config files. Operator actions are visible by default, can be hidden in Settings, and remain preview- and approval-gated.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }
            }

            Text(oneClickMcpDetectedClientLabel)
                .font(AetowerDesign.Typography.caption.monospaced())
                .foregroundStyle(AetowerDesign.Ink.tertiary)
                .padding(.leading, AetowerDesign.Spacing.xxl + AetowerDesign.Spacing.md)

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button {
                    focusedField = nil
                    state.registerSupportedLocalMcpClients()
                } label: {
                    Label("Register Aetower MCP", systemImage: "checkmark.circle")
                }
                .buttonStyle(.borderedProminent)
                .disabled(pendingTargets.isEmpty)

                Button("Review AI Clients") {
                    selectedSection = .aiClients
                }
                .buttonStyle(.bordered)
            }
            .padding(.leading, AetowerDesign.Spacing.xxl + AetowerDesign.Spacing.md)

            if let message = state.localMcpRegistrationStatusMessage {
                Text(message)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(message.contains("Registered")
                        ? AetowerDesign.Status.neutral
                        : AetowerDesign.Status.warning)
                    .padding(.leading, AetowerDesign.Spacing.xxl + AetowerDesign.Spacing.md)
            }
        }
    }

    @ViewBuilder
    private var generalSection: some View {
        @Bindable var settings = settings
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            SettingsCard(
                title: "Appearance",
                subtitle: "Controls the app chrome immediately.",
                status: status(for: .general)
            ) {
                Picker("Appearance", selection: $settings.appearanceMode) {
                    Text("System").tag("system")
                    Text("Light").tag("light")
                    Text("Dark").tag("dark")
                }
                .pickerStyle(.segmented)
            }

            SettingsCard(title: "Window and launch behavior", subtitle: "Small controls that change how Aetower shows up on macOS.") {
                if let nav {
                    @Bindable var nav = nav
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                        Text("Startup tab")
                            .font(.headline)
                        Picker(
                            "Startup tab",
                            selection: Binding(
                                get: { nav.defaultTab ?? .monitor },
                                set: { nav.defaultTab = ($0 == .monitor ? nil : $0) }
                            )
                        ) {
                            Text("Monitor (default)").tag(WorkspaceTab.monitor)
                            ForEach(WorkspaceTab.allCases.filter { $0 != .monitor }) { tab in
                                Text(tab.title).tag(tab)
                            }
                        }
                        .accessibilityIdentifier("settings.startupTab")
                        Text("Which tab Aetower opens on. Agents and scripts can also set this with `defaults write com.aeptus.aetower nav.defaultWorkspaceTab <slug>` or `aetower tab <slug> --default`.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    SettingDivider()
                }

                Toggle("Show menu bar extra", isOn: $settings.showMenuBarExtra)
                Toggle("Operator-safe mode for History and Timeline", isOn: $settings.operatorSafeModeEnabled)
                Text("Heavy History and Timeline views start from summaries first, smaller visible windows, and manual expansion for large detail lists.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    Text("UI refresh interval")
                        .font(.headline)
                    Picker("UI refresh interval", selection: $settings.refreshIntervalSeconds) {
                        Text("1.0s").tag(1.0)
                        Text("2.0s").tag(2.0)
                        Text("5.0s").tag(5.0)
                    }
                    .pickerStyle(.segmented)
                    Text("How often the macOS UI asks the engine for a newer snapshot. Engine collection cadence is controlled separately.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                SettingDivider()

                Toggle(
                    "Launch at login",
                    isOn: Binding(
                        get: { settings.launchAtLoginEnabled },
                        set: { settings.setLaunchAtLogin($0) }
                    )
                )
                if let error = settings.launchAtLoginError {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(AetowerDesign.Status.warning)
                }
                Text("Uses macOS Login Items. If macOS requires approval, finish it in System Settings.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            SettingsCard(
                title: "Energy & sustainability",
                subtitle: "Translate per-process power into running cost and carbon."
            ) {
                HStack {
                    Text("Currency symbol")
                        .font(.headline)
                    Spacer()
                    TextField("$", text: $settings.energyCurrencySymbol)
                        .textFieldStyle(.roundedBorder)
                        .aetowerUtilityTextInput()
                        .frame(width: 60)
                        .multilineTextAlignment(.center)
                }

                SettingDivider()

                intervalSlider(
                    title: "Electricity price (per kWh)",
                    value: $settings.electricityPricePerKwh,
                    range: 0...1,
                    step: 0.01,
                    format: "%.2f",
                    valueWidth: 44,
                    note: "Your electricity rate. Used for the $/hr cost translation."
                )

                SettingDivider()

                intervalSlider(
                    title: "Grid carbon intensity (gCO₂ per kWh)",
                    value: $settings.gridCarbonIntensityGramsPerKwh,
                    range: 0...1000,
                    step: 10,
                    format: "%.0f",
                    valueWidth: 44,
                    note: "Carbon intensity of your local grid. Defaults to the world average (~480). France ≈ 60; US ≈ 370."
                )
            }
        }
    }

    @ViewBuilder
    private var collectionSection: some View {
        @Bindable var settings = settings
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            SettingsCard(
                title: "Runtime collection",
                subtitle: "Choose an intent first. Detailed cadence tuning is available when needed.",
                status: status(for: .collection)
            ) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    Text("Collection presets")
                        .font(.headline)
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 238), spacing: AetowerDesign.Spacing.md)],
                        alignment: .leading,
                        spacing: AetowerDesign.Spacing.md
                    ) {
                        ForEach(SettingsCollectionPreset.allCases) { preset in
                            Button {
                                preset.apply(to: settings)
                            } label: {
                                SettingsPresetCard(preset: preset)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    Text("Presets update the running engine through the existing live collection settings path.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                SettingDivider()

                DisclosureGroup("Advanced cadence controls", isExpanded: $showAdvancedCollectionControls) {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                            Text("Collection mode")
                                .font(.headline)
                            Picker("Collection mode", selection: $settings.collectionProfile) {
                                Text("Balanced").tag(CollectionProfile.balanced)
                                Text("Full detail").tag(CollectionProfile.full)
                            }
                            .pickerStyle(.segmented)
                            Text("Balanced lowers CPU and battery cost by sampling expensive per-process signals less often. Full detail is intended for short diagnostic sessions.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }

                        Toggle("Adaptive engine cadence", isOn: $settings.adaptiveCadenceEnabled)
                        Text("When enabled, Aetower stays active during hotspots and slows down when the machine is quiet or on battery.")
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        intervalSlider(
                            title: "Engine active interval",
                            value: $settings.engineActiveIntervalSeconds,
                            range: 1...5,
                            step: 0.5,
                            format: "%.1fs"
                        )
                        intervalSlider(
                            title: "Engine idle interval",
                            value: $settings.engineIdleIntervalSeconds,
                            range: 2...30,
                            step: 1,
                            format: "%.0fs",
                            note: "Used when adaptive cadence is on and the machine is quiet."
                        )
                        intervalSlider(
                            title: "Engine low-power interval",
                            value: $settings.engineLowPowerIntervalSeconds,
                            range: 3...45,
                            step: 1,
                            format: "%.0fs",
                            note: "Used on battery or Low Power Mode."
                        )
                        intervalSlider(
                            title: "GPU sample interval",
                            value: $settings.gpuSampleIntervalSeconds,
                            range: 5...120,
                            step: 5,
                            format: "%.0fs"
                        )
                        intervalSlider(
                            title: "GPU sample interval on battery",
                            value: $settings.gpuSampleLowPowerIntervalSeconds,
                            range: 10...180,
                            step: 5,
                            format: "%.0fs",
                            note: "Longer GPU intervals reduce system-service activity and save power."
                        )
                    }
                    .padding(.top, AetowerDesign.Spacing.md)
                }
            }

            SettingsCard(
                title: "Monitor rings",
                subtitle: "Placement, focus, and graph scale for the seven metric cards."
            ) {
                monitorMetricRingsControls
            }

            SettingsCard(
                title: "Storage prevention",
                subtitle: "Optional background scans that feed warning-only storage budgets."
            ) {
                Toggle("Run scheduled storage scans", isOn: $settings.storageScheduledScansEnabled)
                Text("Off by default. When enabled, Aetower uses the same non-blocking Storage scan job path and never deletes files automatically.")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)

                intervalSlider(
                    title: "Scheduled scan interval",
                    value: $settings.storageScheduledScanIntervalHours,
                    range: 1...168,
                    step: 1,
                    format: "%.0fh",
                    valueWidth: 46,
                    note: "Storage budgets remain warning-only. Safe-tier auto-trash still requires a separate explicit opt-in policy."
                )
                .disabled(!settings.storageScheduledScansEnabled)
            }
        }
    }

    @ViewBuilder
    private var repositoriesSection: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            SettingsCard(
                title: "Repository roots",
                subtitle: "Controls repository inventory and repository-tab scans. Storage cleanup roots stay separate.",
                status: status(for: .repositories)
            ) {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                    ForEach(settings.repositoryRoots, id: \.self) { root in
                        SettingsRowCard {
                            HStack(alignment: .center, spacing: AetowerDesign.Spacing.md) {
                                let exists = SettingsStore.repositoryRootExists(root)
                                Image(systemName: exists ? "folder" : "folder.badge.questionmark")
                                    .foregroundStyle(exists ? AetowerDesign.Status.ready : AetowerDesign.Status.warning)
                                    .frame(width: 18)
                                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                                    Text(root)
                                        .font(AetowerDesign.Typography.compactData(size: 12))
                                        .textSelection(.enabled)
                                    Text(exists ? "Available for repository inventory" : "Path is not currently available")
                                        .font(AetowerDesign.Typography.caption)
                                        .foregroundStyle(AetowerDesign.Ink.secondary)
                                }
                                Spacer(minLength: AetowerDesign.Spacing.md)
                                Button {
                                    settings.removeRepositoryRoot(root)
                                } label: {
                                    Label("Remove", systemImage: "minus.circle")
                                }
                                .buttonStyle(.bordered)
                            }
                        }
                    }
                }

                HStack(spacing: AetowerDesign.Spacing.sm) {
                    TextField("Add root, for example ~/Wikimedia", text: $repositoryRootDraft)
                        .textFieldStyle(.roundedBorder)
                        .aetowerUtilityTextInput()
                        .focused($focusedField, equals: .repositoryRoot)
                        .onSubmit(addRepositoryRootDraft)
                    Button {
                        addRepositoryRootDraft()
                    } label: {
                        Label("Add", systemImage: "plus.circle")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(!repositoryRootDraftCanBeAdded)
                }

                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Button {
                        settings.resetRepositoryRootsToDefaults()
                        repositoryRootDraft = ""
                    } label: {
                        Label("Reset defaults", systemImage: "arrow.counterclockwise")
                    }
                    .buttonStyle(.bordered)

                    Text("Defaults: \(SettingsStore.defaultRepositoryRoots.joined(separator: ", "))")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }
            }
        }
    }

    private var monitorMetricRingsControls: some View {
        @Bindable var settings = settings
        return VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Picker("Ring placement", selection: $settings.monitorMetricCardPlacement) {
                ForEach(MonitorMetricCardPlacement.allCases) { placement in
                    Text(placement.title).tag(placement)
                }
            }
            .pickerStyle(.segmented)

            Picker("Focused ring", selection: $settings.monitorMetricCardFocus) {
                ForEach(MonitorMetricCardFocus.allCases) { focus in
                    Text(focus.title).tag(focus)
                }
            }
            .pickerStyle(.menu)
            .disabled(!settings.monitorMetricCardPlacement.supportsFocusedMetric)

            Text(settings.monitorMetricCardPlacement.supportsFocusedMetric
                ? "Top and bottom placements can pin one full-width ring, or switch back to all rings."
                : "Left and right placements use a compact vertical rail and always show all rings.")
                .font(.caption)
                .foregroundStyle(.secondary)

            SettingDivider()

            Toggle("Use fixed 0-100 ring scale (%)", isOn: $settings.metricRingsFixedScaling)
            Text("CPU, memory, GPU, and friction draw against 0-100. Disk, network, and wakeups use fixed danger-threshold axes. Turn this off to auto-scale each ring to its recent range.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var integrationsSection: some View {
        let validation = IntegrationValidation(draft: integrationDraft)
        let browserPresentation = browserAttributionPresentation
        SettingsCard(
            title: "Integration endpoints",
            subtitle: "Endpoint fields are staged until you apply them.",
            status: status(for: .integrations)
        ) {
            if hasPendingIntegrationChanges {
                SettingsNotice(
                    title: "Integration changes are pending",
                    detail: "Apply these staged endpoint and telemetry changes before trusting capability status.",
                    color: AetowerDesign.Status.warning
                )
            }

            if validation.hasBlockingIssues {
                SettingsNotice(
                    title: "Some integration settings need attention",
                    detail: "Fix the highlighted values before applying this page.",
                    color: AetowerDesign.Status.error
                )
            }

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                SettingsSetupCard(
                    title: "Browser tabs",
                    subtitle: "Open a dedicated Chrome profile so Aetower can attribute browser load to tabs.",
                    systemImage: "globe",
                    badge: browserPresentation.badge,
                    color: browserPresentation.color
                ) {
                    Text("Aetower uses a separate Chrome profile for this integration. When enabled, local tab titles, URLs, JS heap, DOM size, and network activity can be shown in Monitor without exposing your normal Chrome profile.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)

                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Button(browserAttributionPrimaryActionTitle) {
                            focusedField = nil
                            enableDedicatedBrowserAttribution()
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(browserAttributionIsBusy)

                        Button("Test endpoint") {
                            focusedField = nil
                            testBrowserAttributionEndpoint()
                        }
                        .buttonStyle(.bordered)
                        .disabled(browserAttributionEndpoint.isEmpty || browserAttributionIsBusy)

                        if !browserAttributionEndpoint.isEmpty {
                            Button("Disable") {
                                focusedField = nil
                                disableBrowserAttribution()
                            }
                            .buttonStyle(.bordered)
                            .disabled(browserAttributionIsBusy)
                        }
                    }

                    Text(browserPresentation.detail)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)

                    Text("Dedicated profile: \(BrowserAttributionSetup.dedicatedProfileDisplayPath)")
                        .font(AetowerDesign.Typography.caption.monospaced())
                        .foregroundStyle(AetowerDesign.Ink.tertiary)
                        .textSelection(.enabled)

                    TextField(
                        "Browser debug endpoint",
                        text: $integrationDraft.chromiumEndpoint,
                        prompt: Text("http://127.0.0.1:9222/json/list")
                    )
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .focused($focusedField, equals: .chromiumEndpoint)
                    Text("Advanced: use a local `/json` or `/json/list` endpoint for Chrome, Edge, Brave, Arc, or Chromium. Leave blank to disable browser attribution.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    SettingsValidationList(issues: validation.issues(for: .chromiumEndpoint))
                }

                SettingsSetupCard(
                    title: "Docker",
                    subtitle: "Lets Aetower group container activity with host processes.",
                    systemImage: "shippingbox",
                    badge: "Optional",
                    color: AetowerDesign.Status.ready
                ) {
                    TextField(
                        "Docker socket",
                        text: $integrationDraft.dockerSocketPath,
                        prompt: Text(SettingsStore.defaultDockerSocketPath)
                    )
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .focused($focusedField, equals: .dockerSocketPath)
                    Text("Leave blank to use the default Docker socket path.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    SettingsValidationList(issues: validation.issues(for: .dockerSocketPath))
                }

                SettingsSetupCard(
                    title: "Signed helper",
                    subtitle: "Enterprise-only access for capabilities that require a separately installed helper.",
                    systemImage: "lock.shield",
                    badge: "Advanced",
                    color: AetowerDesign.Status.warning
                ) {
                    Toggle("Enable signed helper", isOn: $integrationDraft.privilegedHelperEnabled)
                    TextField("Signed helper path", text: $integrationDraft.privilegedHelperPath)
                        .textFieldStyle(.roundedBorder)
                        .aetowerUtilityTextInput()
                        .focused($focusedField, equals: .privilegedHelperPath)
                    Text("Enable only after installing and signing the helper outside Aetower.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    SettingsValidationList(issues: validation.issues(for: .privilegedHelperPath))
                }

                SettingsSetupCard(
                    title: "Chau7",
                    subtitle: "Adds terminal tabs, repositories, branches, sessions, and AI-agent context.",
                    systemImage: "terminal",
                    badge: "Recommended",
                    color: AetowerDesign.Status.success
                ) {
                    TextField(
                        "Chau7 session socket",
                        text: $integrationDraft.chau7Endpoint,
                        prompt: Text("Leave blank for auto-detect")
                    )
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .focused($focusedField, equals: .chau7Endpoint)
                    Text("Leave blank for the default local Chau7 socket. If set manually, use an absolute path.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    SettingsValidationList(issues: validation.issues(for: .chau7Endpoint))

                    TextField(
                        "Default Chau7 agent command",
                        text: $integrationDraft.chau7AgentCommand,
                        prompt: Text(SettingsStore.defaultChau7AgentCommand)
                    )
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .focused($focusedField, equals: .chau7AgentCommand)
                    Text("Used by Repository contract actions when Aetower asks Chau7 to open a shell and launch an agent.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }

                SettingsSetupCard(
                    title: "Observability export",
                    subtitle: "Sends Aetower host and process metrics to a local or enterprise collector.",
                    systemImage: "arrow.up.forward.circle",
                    badge: "OTLP/HTTP",
                    color: AetowerDesign.Status.ready
                ) {
                    Toggle("Enable observability export", isOn: $integrationDraft.telemetryEnabled)
                    TextField(
                        "Metrics collector endpoint",
                        text: $integrationDraft.telemetryEndpoint,
                        prompt: Text(SettingsStore.defaultTelemetryEndpoint)
                    )
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .focused($focusedField, equals: .telemetryEndpoint)
                    SettingsValidationList(issues: validation.issues(for: .telemetryEndpoint))

                    intervalSlider(
                        title: "Metrics export interval",
                        value: $integrationDraft.telemetryExportIntervalSeconds,
                        range: 5...120,
                        step: 5,
                        format: "%.0fs",
                        note: "Exports host and entity gauges to an OTLP/HTTP metrics collector."
                    )
                }

                SettingsSetupCard(
                    title: "GitHub",
                    subtitle: "Enables optional project status refresh for linked repositories.",
                    systemImage: "chevron.left.forwardslash.chevron.right",
                    badge: "Optional",
                    color: AetowerDesign.Status.ready
                ) {
                    GitHubOAuthConnectorView(
                        clientID: $integrationDraft.githubOAuthClientID,
                        scopesText: $integrationDraft.githubOAuthScopes,
                        onConfigurationApplied: persistGitHubOAuthConfiguration
                    )

                    SecureField(
                        "GitHub token",
                        text: $integrationDraft.githubToken,
                        prompt: Text("Optional fine-grained token")
                    )
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .focused($focusedField, equals: .githubToken)
                    Text("OAuth device flow requires a GitHub OAuth App client ID with device flow enabled. Manual fine-grained tokens remain available for selected private repositories.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                SettingsSetupCard(
                    title: "Cloudflare",
                    subtitle: "Enables optional deployment refresh for manually linked Pages and Workers.",
                    systemImage: "cloud",
                    badge: "Optional",
                    color: AetowerDesign.Tone.network
                ) {
                    CloudflareOAuthConnectorView(
                        accountID: $integrationDraft.cloudflareOAuthAccountID,
                        clientID: $integrationDraft.cloudflareOAuthClientID,
                        scopesText: $integrationDraft.cloudflareOAuthScopes,
                        redirectURI: $integrationDraft.cloudflareOAuthRedirectURI,
                        apiTokenProvider: { integrationDraft.cloudflareToken },
                        onConfigurationApplied: persistCloudflareOAuthConfiguration
                    )

                    SecureField(
                        "Cloudflare API token",
                        text: $integrationDraft.cloudflareToken,
                        prompt: Text("Optional read-only token")
                    )
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .focused($focusedField, equals: .cloudflareToken)
                    Text("Use an API token scoped to the selected account with read access for Cloudflare Pages and Workers Scripts. Global API keys are not needed.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                SettingsSetupCard(
                    title: "Binary reputation",
                    subtitle:
                        "Checks unsigned or ad-hoc apps with network activity against VirusTotal.",
                    systemImage: "checkmark.shield",
                    badge: "VirusTotal",
                    color: AetowerDesign.Status.warning
                ) {
                    Toggle("Enable VirusTotal reputation", isOn: $integrationDraft.binaryReputationEnabled)
                    SecureField(
                        "VirusTotal API key",
                        text: $integrationDraft.virusTotalKey,
                        prompt: Text("Paste your VirusTotal API key")
                    )
                    .textFieldStyle(.roundedBorder)
                    .aetowerUtilityTextInput()
                    .disabled(!integrationDraft.binaryReputationEnabled)
                    Text(
                        "Only the file's SHA-256 is sent — never the file, its path, or any host context. Signed Apple/Developer-ID apps are never checked. The key is stored in your Keychain."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    Link(
                        "Get a free VirusTotal API key",
                        destination: URL(string: "https://www.virustotal.com/gui/join-us")!
                    )
                    .font(.caption)
                }
            }

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button("Apply integration settings") {
                    focusedField = nil
                    applyIntegrationAndTelemetrySettings()
                }
                .buttonStyle(.borderedProminent)
                .disabled(validation.hasBlockingIssues)

                Button("Send test metrics") {
                    focusedField = nil
                    applyIntegrationAndTelemetrySettings(
                        confirmation: "Integration settings applied before telemetry verification."
                    )
                    state.verifyTelemetryExport(settings)
                }
                .buttonStyle(.bordered)
                .disabled(validation.hasBlockingIssues)
            }

            if let applyConfirmation {
                Text(applyConfirmation)
                    .font(.caption)
                    .foregroundStyle(AetowerDesign.Status.success)
            }
            if let telemetryVerificationStatus = state.telemetryVerificationStatus {
                Text(telemetryVerificationStatus)
                    .font(.caption)
                    .foregroundStyle(telemetryVerificationStatus.contains("failed") ? .orange : .secondary)
            }
        }
        .onAppear {
            loadIntegrationDraftIfNeeded()
        }
    }

    @ViewBuilder
    private var aiClientsSection: some View {
        @Bindable var settings = settings
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            CommandLineToolCard()

            SettingsCard(
                title: "Local AI client MCP access",
                subtitle: "Aetower can register its bundled MCP proxy for supported local agents.",
                status: status(for: .aiClients)
            ) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Text("MCP mode")
                        .font(AetowerDesign.Typography.controlLabel)
                    SettingsBadge(
                        settings.localMcpOperatorActionsEnabled ? "Operator actions visible" : "Read-only tools",
                        color: settings.localMcpOperatorActionsEnabled
                            ? AetowerDesign.Status.warning
                            : AetowerDesign.Status.success
                    )
                    Spacer()
                }

                Toggle(
                    "Automatically register supported AI clients on launch",
                    isOn: $settings.autoRegisterLocalMcpClientsEnabled
                )
                Text("Advanced opt-in. Off by default so Aetower does not modify Claude or Codex config files without an explicit setup action.")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)

                Toggle(
                    "Show MCP operator actions",
                    isOn: $settings.localMcpOperatorActionsEnabled
                )
                if settings.localMcpOperatorActionsEnabled {
                    Text("Visible by default. Local MCP exposes guarded process-action tools; execution still requires preview and operator confirmation.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Status.warning)
                } else {
                    Text("Hidden. Local MCP exposes only read-only tools until operator actions are shown again.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                }

                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Button {
                        focusedField = nil
                        state.registerSupportedLocalMcpClients()
                    } label: {
                        Label("Register Aetower MCP with Claude/Codex", systemImage: "checkmark.circle")
                    }
                    .buttonStyle(.borderedProminent)

                    Button("Refresh client status") {
                        state.refreshLocalMcpClientStatuses()
                    }
                    .buttonStyle(.bordered)
                }

                if let message = state.localMcpRegistrationStatusMessage {
                    Text(message)
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(message.contains("Registered")
                            ? AetowerDesign.Status.neutral
                            : AetowerDesign.Status.warning)
                }

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                    ForEach(state.localMcpClientStatuses) { status in
                        SettingsRowCard {
                            HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                                Text(status.displayName)
                                    .font(AetowerDesign.Typography.sectionTitle)
                                SettingsBadge(
                                    registrationLabel(status.state),
                                    color: registrationColor(status.state)
                                )
                                Spacer()
                            }

                            Text(status.detail)
                                .font(AetowerDesign.Typography.caption)
                                .foregroundStyle(AetowerDesign.Ink.secondary)

                            if let configPath = status.configPath {
                                Text(configPath)
                                    .font(AetowerDesign.Typography.caption.monospaced())
                                    .foregroundStyle(AetowerDesign.Ink.tertiary)
                                    .textSelection(.enabled)
                            }

                            HStack(spacing: AetowerDesign.Spacing.sm) {
                                if status.supportsAutomaticRegistration,
                                   status.isInstalled,
                                   status.state != .registered {
                                    Button("Register") {
                                        focusedField = nil
                                        state.registerSupportedLocalMcpClients()
                                    }
                                    .buttonStyle(.bordered)
                                }

                                if status.manualSnippet != nil {
                                    Button("Copy MCP snippet") {
                                        focusedField = nil
                                        state.copyLocalMcpConfigSnippet(providerId: status.id)
                                    }
                                    .buttonStyle(.bordered)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var automationSection: some View {
        SettingsCard(
            title: "Event automations",
            subtitle: "Run a macOS Shortcut or shell command when a matching timeline event appears.",
            status: status(for: .automation)
        ) {
            if editingAutomationRules.isEmpty {
                Text("No automation rules yet. Add one to react to launches, exits, anomalies, thermal/friction shifts, or an unsigned process opening a network connection.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            ForEach($editingAutomationRules) { $rule in
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Toggle("", isOn: $rule.enabled).labelsHidden()
                        TextField("Rule name", text: $rule.name)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                        Button(role: .destructive) {
                            editingAutomationRules.removeAll { $0.id == rule.id }
                        } label: {
                            Image(systemName: "trash")
                        }
                        .buttonStyle(.borderless)
                    }
                    HStack {
                        Picker("When", selection: $rule.event) {
                            ForEach(AutomationEvent.allCases) { event in
                                Text(event.label).tag(event)
                            }
                        }
                        .frame(maxWidth: 220)
                        TextField("title contains (optional)", text: $rule.titleContains)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                    }
                    HStack {
                        Picker("Action", selection: $rule.actionKind) {
                            ForEach(AutomationActionKind.allCases) { kind in
                                Text(kind.label).tag(kind)
                            }
                        }
                        .frame(maxWidth: 200)
                        TextField(
                            rule.actionKind == .shortcut ? "Shortcut name" : "shell command",
                            text: $rule.actionValue
                        )
                        .textFieldStyle(.roundedBorder)
                        .aetowerUtilityTextInput()
                        .font(.system(size: 11, design: .monospaced))
                    }
                    if rule.event.supportsConnectionPredicates {
                        HStack {
                            Menu {
                                ForEach(SigningClass.allCases) { signingClass in
                                    Button {
                                        toggleSigningClass($rule, signingClass)
                                    } label: {
                                        Label(
                                            signingClass.label,
                                            systemImage: rule.signingClasses.contains(
                                                signingClass.rawValue) ? "checkmark" : ""
                                        )
                                    }
                                }
                            } label: {
                                Text("Signing: \(signingClassSummary(rule.signingClasses))")
                            }
                            .frame(maxWidth: 220)
                            TextField(
                                "remote host contains (optional)",
                                text: $rule.remoteHostContains
                            )
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                        }
                        Text(
                            "Fires when a process whose signing class matches opens a connection to a matching host. Requires the privileged helper; connections are sampled, not blocked."
                        )
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                    }
                    if rule.event.supportsBudgetFields {
                        HStack {
                            Picker("Metric", selection: $rule.budgetMetric) {
                                ForEach(AgentBudgetMetric.allCases) { metric in
                                    Text(metric.label).tag(metric.rawValue)
                                }
                            }
                            .frame(maxWidth: 220)
                            TextField(
                                "threshold",
                                value: $rule.budgetThreshold,
                                format: .number
                            )
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                            .frame(maxWidth: 120)
                        }
                        Text(
                            "Fires when an AI agent's rate exceeds the threshold (the rate is derived from recent persisted history). Use the title field above to scope to an agent name (blank = any agent)."
                        )
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                    }
                }
                .padding(10)
                .background(Color.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
            }
            HStack {
                Button {
                    editingAutomationRules.append(AutomationRule())
                } label: {
                    Label("Add rule", systemImage: "plus")
                }
                .buttonStyle(.bordered)
                Spacer()
                Button("Save rules") {
                    state.updateAutomationRules(editingAutomationRules)
                }
                .buttonStyle(.borderedProminent)
            }
            Text("Rules evaluate on each refresh while Aetower is running; background-only execution is not yet supported.")
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
        .onAppear {
            if editingAutomationRules.isEmpty {
                editingAutomationRules = state.automationRules
            }
        }
    }

    /// Toggle a signing class in/out of a network-connection rule's filter set.
    private func toggleSigningClass(_ rule: Binding<AutomationRule>, _ signingClass: SigningClass) {
        var classes = rule.wrappedValue.signingClasses
        if let index = classes.firstIndex(of: signingClass.rawValue) {
            classes.remove(at: index)
        } else {
            classes.append(signingClass.rawValue)
        }
        rule.wrappedValue.signingClasses = classes
    }

    /// Human-readable summary of a rule's selected signing classes for the menu
    /// label ("Any signing" when none are selected).
    private func signingClassSummary(_ classes: [String]) -> String {
        if classes.isEmpty { return "Any signing" }
        let labels = classes.compactMap { SigningClass(rawValue: $0)?.label }
        return labels.isEmpty ? "Any signing" : labels.joined(separator: ", ")
    }

    @ViewBuilder
    private var notificationsSection: some View {
        @Bindable var settings = settings
        SettingsCard(
            title: "Friction alerts",
            subtitle: "Notify when a process group crosses your attention threshold.",
            status: status(for: .notifications)
        ) {
            Toggle("Enable notifications", isOn: $settings.notificationsEnabled)
            Text("Authorization: \(state.notificationAuthorizationStatus)")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            intervalSlider(
                title: "Friction notification threshold",
                value: $settings.frictionNotificationThreshold,
                range: 10...100,
                step: 5,
                format: "%.0f",
                valueWidth: 34,
                note: "Notify when an app's friction score exceeds this threshold."
            )

            SettingDivider()

            Text("Alert categories")
                .font(.headline)
            Toggle("Thermal & throttle", isOn: $settings.notifyThermal)
            Toggle("Regressions", isOn: $settings.notifyRegression)
            Toggle("Restart loops", isOn: $settings.notifyRestartLoop)
            Toggle("New network connections", isOn: $settings.notifyNetwork)
            Toggle("AI agent budgets", isOn: $settings.notifyAgentBudget)
            Text("Categories require notifications to be enabled above. Each app is rate-limited; use \"Snooze alerts\" in an app's detail view to mute it temporarily.")
                .font(.caption)
                .foregroundStyle(.secondary)

            if !state.activeNotificationSnoozes.isEmpty {
                SettingDivider()
                Text("Snoozed apps")
                    .font(.headline)
                ForEach(state.activeNotificationSnoozes) { snooze in
                    HStack {
                        Text(snooze.displayName.isEmpty ? snooze.bundleId : snooze.displayName)
                            .font(.callout)
                        Spacer()
                        Button("Clear") {
                            state.clearSnooze(bundleId: snooze.bundleId)
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }

            Button("Re-check notification permission") {
                state.applyNotificationSettings(settings)
            }
            .buttonStyle(.bordered)
        }
    }

    @ViewBuilder
    private var privacySection: some View {
        @Bindable var settings = settings
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            outboundDataSection

            SettingsCard(
                title: "Export privacy",
                subtitle: "Controls what leaves the machine in JSON exports and support bundles.",
                status: status(for: .privacy)
            ) {
                Picker("Export privacy tier", selection: $settings.exportPrivacyTier) {
                    Text("Redacted").tag(ExportPrivacyTier.redacted)
                    Text("Operator").tag(ExportPrivacyTier.operatorMode)
                    Text("Full").tag(ExportPrivacyTier.full)
                }
                .pickerStyle(.segmented)
                Text("Redacted strips sensitive titles, paths, URLs, and commands. Operator keeps structural context while still hiding secrets. Full exports everything and is intended only for explicit troubleshooting.")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }
        }
    }

    private var outboundDataSection: some View {
        let snapshot = outboundDataSnapshot
        return SettingsCard(
            title: "Outbound Data",
            subtitle: "Applied settings that determine when Aetower can send data outside the app.",
            status: snapshot.status
        ) {
            SettingsNotice(
                title: snapshot.noticeTitle,
                detail: snapshot.noticeDetail,
                color: snapshot.noticeColor
            )

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.none) {
                ForEach(snapshot.rows) { row in
                    SettingsRowCard {
                        HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                            outboundDataIcon(row)
                            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                                HStack(spacing: AetowerDesign.Spacing.sm) {
                                    Text(row.title)
                                        .font(AetowerDesign.Typography.sectionTitle)
                                        .foregroundStyle(AetowerDesign.Ink.primary)
                                    SettingsBadge(row.badge, color: row.badgeColor)
                                }
                                Text(row.detail)
                                    .font(AetowerDesign.Typography.caption)
                                    .foregroundStyle(AetowerDesign.Ink.secondary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                }
            }

            Text("Manual exports and support bundles still use the Export privacy tier below. Provider and VirusTotal secrets are stored in the macOS Keychain, not in UserDefaults.")
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.tertiary)
        }
    }

    @ViewBuilder
    private func outboundDataIcon(_ row: OutboundDataRow) -> some View {
        switch row.tone {
        case .network:
            Image(systemName: row.systemImage)
                .foregroundStyle(AetowerDesign.Tone.network)
                .frame(width: 24)
        case .warning:
            Image(systemName: row.systemImage)
                .foregroundStyle(AetowerDesign.Status.warning)
                .frame(width: 24)
        case .ready:
            Image(systemName: row.systemImage)
                .foregroundStyle(AetowerDesign.Status.ready)
                .frame(width: 24)
        case .cpu:
            Image(systemName: row.systemImage)
                .foregroundStyle(AetowerDesign.Tone.cpu)
                .frame(width: 24)
        }
    }

    private var updatesSection: some View {
        SettingsCard(
            title: "Direct-download updates",
            subtitle: "Sparkle handles signed release checks outside the Mac App Store.",
            status: status(for: .updates)
        ) {
            Text(updater.statusMessage)
                .font(.caption)
                .foregroundStyle(.secondary)

            Button("Check for Updates") {
                updater.checkForUpdates()
            }
            .buttonStyle(.bordered)
            .disabled(!updater.isConfigured)

            Text("Local development builds stay disabled until the packaged app includes an appcast URL and Sparkle public key.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var advancedSection: some View {
        @Bindable var settings = settings
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            SettingsCard(
                title: "Capabilities",
                subtitle: "Richer integrations stay behind explicit capability gates.",
                status: status(for: .advanced)
            ) {
                ForEach(state.capabilitiesState, id: \.kind) { capability in
                    SettingsRowCard {
                        HStack {
                            Text(capabilityKindDisplayName(capability.kind))
                                .font(.headline)
                            Spacer()
                            SettingsBadge(capabilityStateLabel(capability), color: capabilityStateColor(capability))
                            SettingsBadge(capabilityHealthLabel(capability.health), color: capabilityHealthColor(capability.health))
                        }
                        Text(capability.detail)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(capabilityConsequenceText(capability))
                            .font(.caption)
                        Text(capabilityRemediationText(capability))
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                        HStack {
                            Button(capabilityActionLabel(capability)) {
                                state.performCapabilityAction(capability, settings: settings)
                                if isAdapterCapability(capability.kind) {
                                    appliedIntegrationSnapshot = persistedIntegrationSnapshot
                                }
                            }
                            .buttonStyle(.borderedProminent)

                            Text(Date(timeIntervalSince1970: TimeInterval(capability.lastUpdatedMillis) / 1000), style: .time)
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
            }

            SettingsCard(title: "Reset", subtitle: "Restore defaults or clear local Aetower data.") {
                Text("Restores defaults immediately, stops launch-at-login, disables automatic AI-client registration, and applies runtime, integration, notification, and telemetry defaults.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button("Reset to defaults", role: .destructive) {
                    settings.resetToDefaults()
                    integrationDraft = SettingsIntegrationDraft(
                        settings: settings,
                        virusTotalKey: "",
                        githubOAuthClientID: "",
                        githubOAuthScopes: "",
                        cloudflareOAuthAccountID: "",
                        cloudflareOAuthClientID: "",
                        cloudflareOAuthScopes: SettingsStore.defaultCloudflareOAuthScopes,
                        cloudflareOAuthRedirectURI: SettingsStore.defaultCloudflareOAuthRedirectURI,
                        githubToken: "",
                        cloudflareToken: ""
                    )
                    hasLoadedIntegrationDraft = true
                    state.applyNotificationSettings(settings)
                    state.applyRuntimeCollectionSettings(settings)
                    state.applyStoragePolicySettings(settings)
                    state.applyIntegrationSettings(settings)
                    state.applyLocalMcpClientRegistrationSettings(settings)
                    appliedIntegrationSnapshot = currentIntegrationSnapshot
                    applyConfirmation = "All settings reset and applied."
                    clearApplyConfirmationLater()
                }
                .buttonStyle(.bordered)

                Divider()

                Text("Use this before sharing a support machine or after a heavy preview run. It clears persisted history and diagnostics, resets settings, and leaves exported files and third-party MCP client configuration untouched.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button("Reset Aetower local data", role: .destructive) {
                    showResetLocalDataConfirmation = true
                }
                .buttonStyle(.borderedProminent)

                if let applyConfirmation {
                    Text(applyConfirmation)
                        .font(.caption)
                        .foregroundStyle(AetowerDesign.Status.success)
                }
            }
        }
    }

    private func intervalSlider(
        title: String,
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double,
        format: String,
        valueWidth: CGFloat = 44,
        note: String? = nil
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            Text(title)
                .font(.headline)
            HStack {
                Slider(value: value, in: range, step: step)
                Text(String(format: format, value.wrappedValue))
                    .font(.caption.monospacedDigit())
                    .frame(width: valueWidth, alignment: .trailing)
            }
            if let note {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func clearApplyConfirmationLater() {
        Task {
            try? await Task.sleep(nanoseconds: 4_000_000_000)
            applyConfirmation = nil
        }
    }

    private func enableDedicatedBrowserAttribution() {
        browserAttributionState = .launching
        Task { @MainActor in
            do {
                let summary = try await BrowserAttributionSetup.enableDedicatedChrome()
                persistBrowserAttributionEndpoint(
                    summary.endpoint,
                    confirmation: "Browser attribution enabled with a dedicated Chrome profile."
                )
                browserAttributionState = .connected(summary)
            } catch {
                browserAttributionState = .failed(error.localizedDescription)
            }
        }
    }

    private func testBrowserAttributionEndpoint() {
        let endpoint = browserAttributionEndpoint
        guard !endpoint.isEmpty else {
            browserAttributionState = .notConfigured
            return
        }
        browserAttributionState = .checking
        Task { @MainActor in
            do {
                let summary = try await BrowserAttributionSetup.probeEndpoint(endpoint)
                browserAttributionState = .connected(summary)
            } catch {
                browserAttributionState = .failed(error.localizedDescription)
            }
        }
    }

    private func disableBrowserAttribution() {
        persistBrowserAttributionEndpoint(
            "",
            confirmation: "Browser attribution disabled."
        )
        browserAttributionState = .notConfigured
    }

    private func persistBrowserAttributionEndpoint(_ endpoint: String, confirmation: String) {
        integrationDraft.chromiumEndpoint = endpoint
        settings.chromiumEndpoint = endpoint
        state.applyIntegrationSettings(settings)
        appliedIntegrationSnapshot = currentIntegrationSnapshot
        applyConfirmation = confirmation
        clearApplyConfirmationLater()
    }

    private func refreshBrowserAttributionStatusIfConfigured() {
        guard !browserAttributionEndpoint.isEmpty else {
            browserAttributionState = .notConfigured
            return
        }
        guard browserAttributionState == .notConfigured else { return }
        testBrowserAttributionEndpoint()
    }

    private func applyIntegrationAndTelemetrySettings(
        confirmation: String = "Integration and telemetry settings applied."
    ) {
        var normalizedDraft = integrationDraft
        normalizedDraft.normalize()
        integrationDraft = normalizedDraft
        normalizedDraft.apply(to: settings)
        // Persist the VirusTotal key to the Keychain before applying — an empty
        // draft clears it. applyIntegrationSettings reads it back from there.
        KeychainHelper.store(normalizedDraft.virusTotalKey, account: KeychainHelper.binaryReputationAccount)
        let credentialStore = ProviderCredentialStore()
        credentialStore.storeManualToken(normalizedDraft.githubToken, for: .github)
        credentialStore.storeManualToken(normalizedDraft.cloudflareToken, for: .cloudflare)
        state.applyIntegrationSettings(settings)
        appliedIntegrationSnapshot = currentIntegrationSnapshot
        applyConfirmation = confirmation
        clearApplyConfirmationLater()
    }

    private func persistGitHubOAuthConfiguration(clientID: String, scopesText: String) {
        integrationDraft.githubOAuthClientID = clientID
        integrationDraft.githubOAuthScopes = scopesText
        settings.applyGitHubOAuthConfiguration(clientID: clientID, scopesText: scopesText)
    }

    private func persistCloudflareOAuthConfiguration(
        accountID: String,
        clientID: String,
        scopesText: String,
        redirectURI: String
    ) {
        integrationDraft.cloudflareOAuthAccountID = accountID
        integrationDraft.cloudflareOAuthClientID = clientID
        integrationDraft.cloudflareOAuthScopes = scopesText
        integrationDraft.cloudflareOAuthRedirectURI = redirectURI
        settings.applyCloudflareOAuthConfiguration(
            accountID: accountID,
            clientID: clientID,
            scopesText: scopesText,
            redirectURI: redirectURI
        )
    }

    private func resetLocalAetowerData() {
        settings.resetToDefaults()
        integrationDraft = SettingsIntegrationDraft(
            settings: settings,
            virusTotalKey: "",
            githubOAuthClientID: "",
            githubOAuthScopes: "",
            cloudflareOAuthAccountID: "",
            cloudflareOAuthClientID: "",
            cloudflareOAuthScopes: SettingsStore.defaultCloudflareOAuthScopes,
            cloudflareOAuthRedirectURI: SettingsStore.defaultCloudflareOAuthRedirectURI,
            githubToken: "",
            cloudflareToken: ""
        )
        hasLoadedIntegrationDraft = true
        state.clearHistory()
        state.clearDiagnostics()
        state.applyNotificationSettings(settings)
        state.applyRuntimeCollectionSettings(settings)
        state.applyStoragePolicySettings(settings)
        state.applyIntegrationSettings(settings)
        state.applyLocalMcpClientRegistrationSettings(settings)
        appliedIntegrationSnapshot = currentIntegrationSnapshot
        applyConfirmation = "Local Aetower data cleared and defaults restored."
        clearApplyConfirmationLater()
    }

    private func loadIntegrationDraftIfNeeded() {
        guard !hasLoadedIntegrationDraft else { return }
        integrationDraft = SettingsIntegrationDraft(
            settings: settings,
            virusTotalKey: KeychainHelper.retrieve(account: KeychainHelper.binaryReputationAccount) ?? "",
            githubOAuthClientID: settings.githubOAuthClientID,
            githubOAuthScopes: settings.githubOAuthScopes,
            cloudflareOAuthAccountID: settings.cloudflareOAuthAccountID,
            cloudflareOAuthClientID: settings.cloudflareOAuthClientID,
            cloudflareOAuthScopes: settings.cloudflareOAuthScopes,
            cloudflareOAuthRedirectURI: settings.cloudflareOAuthRedirectURI,
            githubToken: ProviderCredentialStore().manualToken(for: .github) ?? "",
            cloudflareToken: ProviderCredentialStore().manualToken(for: .cloudflare) ?? ""
        )
        hasLoadedIntegrationDraft = true
    }

    private func isAdapterCapability(_ kind: CapabilityKind) -> Bool {
        switch kind {
        case .accessibility, .fullDiskAccess, .appleAutomation:
            return false
        case .chromiumDebug, .dockerSocket, .privilegedHelper, .endpointSecurity, .chau7:
            return true
        }
    }
}

private func providerCredentialSummary(_ credential: ProviderCredentialSnapshot) -> String {
    "\(projectProviderDisplayName(credential.provider)) \(providerCredentialSourceLabel(credential.source))"
}

private func projectProviderDisplayName(_ provider: ProjectProvider) -> String {
    switch provider {
    case .github:
        return "GitHub"
    case .cloudflare:
        return "Cloudflare"
    }
}

private func providerCredentialSourceLabel(_ source: ProviderCredentialSource) -> String {
    switch source {
    case .none:
        return "not configured"
    case .manualToken:
        return "manual token"
    case .oauth:
        return "OAuth"
    }
}

private func registrationLabel(_ state: LocalMcpClientRegistrationState) -> String {
    switch state {
    case .registered:
        return "Registered"
    case .availableForAutomaticRegistration:
        return "Available"
    case .manualConfigurationRequired:
        return "Manual"
    case .unavailable:
        return "Unavailable"
    case .notInstalled:
        return "Not installed"
    }
}

private func registrationColor(_ state: LocalMcpClientRegistrationState) -> Color {
    switch state {
    case .registered:
        return AetowerDesign.Status.success
    case .availableForAutomaticRegistration:
        return AetowerDesign.Status.ready
    case .manualConfigurationRequired:
        return AetowerDesign.Status.warning
    case .unavailable:
        return AetowerDesign.Status.error
    case .notInstalled:
        return AetowerDesign.Status.neutral
    }
}

private func capabilityHealthLabel(_ health: CapabilityHealth) -> String {
    switch health {
    case .configured:
        return "Configured"
    case .live:
        return "Live"
    case .cached:
        return "Cached"
    case .degraded:
        return "Degraded"
    }
}

private func capabilityHealthColor(_ health: CapabilityHealth) -> Color {
    switch health {
    case .configured:
        return .secondary
    case .live:
        return .green
    case .cached:
        return .orange
    case .degraded:
        return .red
    }
}

private func capabilityStateLabel(_ capability: CapabilitySnapshot) -> String {
    switch capability.state {
    case .granted:
        return "Granted"
    case .denied:
        return "Denied"
    case .requested:
        return "Pending"
    case .unknown:
        return "Not checked"
    case .unavailable:
        let detail = capability.detail.localizedLowercase
        if detail.contains("disabled") || detail.contains("configure") || detail.contains("set ") {
            return "Not configured"
        }
        if detail.contains("missing") || detail.contains("not found") || detail.contains("not detected") {
            return "Missing"
        }
        if detail.contains("requires") || detail.contains("unavailable on this system") {
            return "Not available"
        }
        return "Unavailable"
    }
}

private func capabilityStateColor(_ capability: CapabilitySnapshot) -> Color {
    switch capability.state {
    case .granted:
        return AetowerDesign.Status.success
    case .denied:
        return AetowerDesign.Status.error
    case .requested:
        return AetowerDesign.Status.ready
    case .unknown:
        return AetowerDesign.Status.neutral
    case .unavailable:
        return capabilityStateLabel(capability) == "Not configured"
            ? AetowerDesign.Status.warning
            : AetowerDesign.Status.neutral
    }
}

private func capabilityActionLabel(_ capability: CapabilitySnapshot) -> String {
    switch capability.kind {
    case .accessibility, .fullDiskAccess, .appleAutomation:
        return "Request Access"
    default:
        return "Refresh Status"
    }
}

private struct SettingsStatus {
    let label: String
    let color: Color

    init(_ label: String, _ color: Color) {
        self.label = label
        self.color = color
    }
}

private struct OutboundDataRow: Identifiable {
    let id: String
    let title: String
    let badge: String
    let badgeColor: Color
    let detail: String
    let systemImage: String
    let tone: OutboundDataTone
    let canLeaveMac: Bool
}

private enum OutboundDataTone {
    case network
    case warning
    case ready
    case cpu
}

private struct OutboundDataSnapshot {
    let rows: [OutboundDataRow]
    let outboundRows: [OutboundDataRow]
    let registeredMcpClientNames: [String]

    var status: SettingsStatus {
        if outboundRows.isEmpty {
            return registeredMcpClientNames.isEmpty
                ? SettingsStatus("Local-only", AetowerDesign.Status.success)
                : SettingsStatus("Local MCP", AetowerDesign.Status.ready)
        }
        return SettingsStatus(
            "\(outboundRows.count) path\(outboundRows.count == 1 ? "" : "s")",
            AetowerDesign.Status.warning
        )
    }

    var noticeTitle: String {
        if outboundRows.isEmpty {
            return registeredMcpClientNames.isEmpty
                ? "No outbound data paths are enabled"
                : "Only local MCP client access is configured"
        }
        return "Outbound data paths are configured"
    }

    var noticeDetail: String {
        if outboundRows.isEmpty {
            if registeredMcpClientNames.isEmpty {
                return "Telemetry, Fleet, VirusTotal, provider tokens, and MCP client registrations are off or absent. Aetower stays local unless you manually export and share data."
            }
            return "No enabled path sends data off this Mac. Registered MCP clients can read Aetower over a local socket on this Mac."
        }
        return "Data can leave this Mac through: \(outboundRows.map(\.title).joined(separator: ", ")). Review the rows below for the exact trigger and destination."
    }

    var noticeColor: Color {
        outboundRows.isEmpty ? AetowerDesign.Status.success : AetowerDesign.Status.warning
    }
}

private struct ProviderCredentialSnapshot {
    let provider: ProjectProvider
    let source: ProviderCredentialSource
    let canAuthenticate: Bool
}

private enum BrowserAttributionViewState: Equatable {
    case notConfigured
    case checking
    case launching
    case connected(BrowserAttributionEndpointSummary)
    case failed(String)
}

private struct SettingsIntegrationDraft: Equatable {
    var chromiumEndpoint = ""
    var dockerSocketPath = SettingsStore.defaultDockerSocketPath
    var privilegedHelperEnabled = false
    var privilegedHelperPath = ""
    var chau7Endpoint = ""
    var chau7AgentCommand = SettingsStore.defaultChau7AgentCommand
    var telemetryEnabled = false
    var telemetryEndpoint = SettingsStore.defaultTelemetryEndpoint
    var telemetryExportIntervalSeconds = 30.0
    var binaryReputationEnabled = false
    var virusTotalKey = ""
    var githubOAuthClientID = ""
    var githubOAuthScopes = ""
    var cloudflareOAuthAccountID = ""
    var cloudflareOAuthClientID = ""
    var cloudflareOAuthScopes = SettingsStore.defaultCloudflareOAuthScopes
    var cloudflareOAuthRedirectURI = SettingsStore.defaultCloudflareOAuthRedirectURI
    var githubToken = ""
    var cloudflareToken = ""

    @MainActor
    init(
        settings: SettingsStore,
        virusTotalKey: String,
        githubOAuthClientID: String,
        githubOAuthScopes: String,
        cloudflareOAuthAccountID: String,
        cloudflareOAuthClientID: String,
        cloudflareOAuthScopes: String,
        cloudflareOAuthRedirectURI: String,
        githubToken: String,
        cloudflareToken: String
    ) {
        chromiumEndpoint = settings.chromiumEndpoint
        dockerSocketPath = settings.dockerSocketPath
        privilegedHelperEnabled = settings.privilegedHelperEnabled
        privilegedHelperPath = settings.privilegedHelperPath
        chau7Endpoint = settings.chau7Endpoint
        chau7AgentCommand = settings.chau7AgentCommand
        telemetryEnabled = settings.telemetryEnabled
        telemetryEndpoint = settings.telemetryEndpoint
        telemetryExportIntervalSeconds = settings.telemetryExportIntervalSeconds
        binaryReputationEnabled = settings.binaryReputationEnabled
        self.virusTotalKey = virusTotalKey
        self.githubOAuthClientID = githubOAuthClientID
        self.githubOAuthScopes = githubOAuthScopes
        self.cloudflareOAuthAccountID = cloudflareOAuthAccountID
        self.cloudflareOAuthClientID = cloudflareOAuthClientID
        self.cloudflareOAuthScopes = cloudflareOAuthScopes
        self.cloudflareOAuthRedirectURI = cloudflareOAuthRedirectURI
        self.githubToken = githubToken
        self.cloudflareToken = cloudflareToken
        normalize()
    }

    init() {}

    mutating func normalize() {
        chromiumEndpoint = chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        dockerSocketPath = dockerSocketPath.trimmingCharacters(in: .whitespacesAndNewlines)
        privilegedHelperPath = privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines)
        chau7Endpoint = chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        chau7AgentCommand = SettingsStore.normalizedChau7AgentCommand(chau7AgentCommand)
        telemetryEndpoint = telemetryEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        telemetryExportIntervalSeconds = max(
            SettingsStore.minimumTelemetryExportIntervalSeconds,
            telemetryExportIntervalSeconds
        )
        virusTotalKey = virusTotalKey.trimmingCharacters(in: .whitespacesAndNewlines)
        githubOAuthClientID = githubOAuthClientID.trimmingCharacters(in: .whitespacesAndNewlines)
        githubOAuthScopes = SettingsStore.normalizedOAuthScopes(githubOAuthScopes)
        cloudflareOAuthAccountID = cloudflareOAuthAccountID.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        cloudflareOAuthClientID = cloudflareOAuthClientID.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        cloudflareOAuthScopes = SettingsStore.normalizedOAuthScopes(cloudflareOAuthScopes)
        cloudflareOAuthRedirectURI = SettingsStore.normalizedCloudflareOAuthRedirectURI(
            cloudflareOAuthRedirectURI
        )
        githubToken = githubToken.trimmingCharacters(in: .whitespacesAndNewlines)
        cloudflareToken = cloudflareToken.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    @MainActor
    func apply(to settings: SettingsStore) {
        settings.chromiumEndpoint = chromiumEndpoint
        settings.dockerSocketPath = dockerSocketPath
        settings.privilegedHelperEnabled = privilegedHelperEnabled
        settings.privilegedHelperPath = privilegedHelperPath
        settings.chau7Endpoint = chau7Endpoint
        settings.chau7AgentCommand = chau7AgentCommand
        settings.telemetryEnabled = telemetryEnabled
        settings.telemetryEndpoint = telemetryEndpoint
        settings.telemetryExportIntervalSeconds = telemetryExportIntervalSeconds
        settings.binaryReputationEnabled = binaryReputationEnabled
        settings.githubOAuthClientID = githubOAuthClientID
        settings.githubOAuthScopes = githubOAuthScopes
        settings.cloudflareOAuthAccountID = cloudflareOAuthAccountID
        settings.cloudflareOAuthClientID = cloudflareOAuthClientID
        settings.cloudflareOAuthScopes = cloudflareOAuthScopes
        settings.cloudflareOAuthRedirectURI = cloudflareOAuthRedirectURI
    }
}

private struct SettingsIntegrationSnapshot: Equatable {
    let chromiumEndpoint: String
    let dockerSocketPath: String
    let privilegedHelperEnabled: Bool
    let privilegedHelperPath: String
    let chau7Endpoint: String
    let chau7AgentCommand: String
    let telemetryEnabled: Bool
    let telemetryEndpoint: String
    let telemetryExportIntervalSeconds: UInt32
    let binaryReputationEnabled: Bool
    let githubOAuthClientID: String
    let githubOAuthScopes: String
    let cloudflareOAuthAccountID: String
    let cloudflareOAuthClientID: String
    let cloudflareOAuthScopes: String
    let cloudflareOAuthRedirectURI: String
    let virusTotalKeySignature: String
    let githubTokenSignature: String
    let cloudflareTokenSignature: String

    @MainActor
    init(_ settings: SettingsStore, virusTotalKeyDraft: String? = nil) {
        chromiumEndpoint = settings.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        dockerSocketPath = SettingsStore.normalizedDockerSocketPath(settings.dockerSocketPath)
        privilegedHelperEnabled = settings.privilegedHelperEnabled
        privilegedHelperPath = settings.privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines)
        chau7Endpoint = settings.chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        chau7AgentCommand = SettingsStore.normalizedChau7AgentCommand(settings.chau7AgentCommand)
        telemetryEnabled = settings.telemetryEnabled
        telemetryEndpoint = SettingsStore.normalizedTelemetryEndpoint(settings.telemetryEndpoint)
        telemetryExportIntervalSeconds = SettingsStore.normalizedTelemetryExportIntervalSeconds(
            settings.telemetryExportIntervalSeconds
        )
        binaryReputationEnabled = settings.binaryReputationEnabled
        githubOAuthClientID = settings.githubOAuthClientID.trimmingCharacters(in: .whitespacesAndNewlines)
        githubOAuthScopes = SettingsStore.normalizedOAuthScopes(settings.githubOAuthScopes)
        cloudflareOAuthAccountID = settings.cloudflareOAuthAccountID.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        cloudflareOAuthClientID = settings.cloudflareOAuthClientID.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        cloudflareOAuthScopes = SettingsStore.normalizedOAuthScopes(settings.cloudflareOAuthScopes)
        cloudflareOAuthRedirectURI = SettingsStore.normalizedCloudflareOAuthRedirectURI(
            settings.cloudflareOAuthRedirectURI
        )
        let key = virusTotalKeyDraft
            ?? KeychainHelper.retrieve(account: KeychainHelper.binaryReputationAccount)
            ?? ""
        virusTotalKeySignature = Self.redactedKeySignature(key)
        let githubToken = ProviderCredentialStore().manualToken(for: .github) ?? ""
        githubTokenSignature = Self.redactedKeySignature(githubToken)
        let cloudflareToken = ProviderCredentialStore().manualToken(for: .cloudflare) ?? ""
        cloudflareTokenSignature = Self.redactedKeySignature(cloudflareToken)
    }

    init(_ draft: SettingsIntegrationDraft) {
        chromiumEndpoint = draft.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        dockerSocketPath = SettingsStore.normalizedDockerSocketPath(draft.dockerSocketPath)
        privilegedHelperEnabled = draft.privilegedHelperEnabled
        privilegedHelperPath = draft.privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines)
        chau7Endpoint = draft.chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        chau7AgentCommand = SettingsStore.normalizedChau7AgentCommand(draft.chau7AgentCommand)
        telemetryEnabled = draft.telemetryEnabled
        telemetryEndpoint = SettingsStore.normalizedTelemetryEndpoint(draft.telemetryEndpoint)
        telemetryExportIntervalSeconds = SettingsStore.normalizedTelemetryExportIntervalSeconds(
            draft.telemetryExportIntervalSeconds
        )
        binaryReputationEnabled = draft.binaryReputationEnabled
        githubOAuthClientID = draft.githubOAuthClientID.trimmingCharacters(in: .whitespacesAndNewlines)
        githubOAuthScopes = SettingsStore.normalizedOAuthScopes(draft.githubOAuthScopes)
        cloudflareOAuthAccountID = draft.cloudflareOAuthAccountID.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        cloudflareOAuthClientID = draft.cloudflareOAuthClientID.trimmingCharacters(
            in: .whitespacesAndNewlines
        )
        cloudflareOAuthScopes = SettingsStore.normalizedOAuthScopes(draft.cloudflareOAuthScopes)
        cloudflareOAuthRedirectURI = SettingsStore.normalizedCloudflareOAuthRedirectURI(
            draft.cloudflareOAuthRedirectURI
        )
        virusTotalKeySignature = Self.redactedKeySignature(draft.virusTotalKey)
        githubTokenSignature = Self.redactedKeySignature(draft.githubToken)
        cloudflareTokenSignature = Self.redactedKeySignature(draft.cloudflareToken)
    }

    private static func redactedKeySignature(_ value: String) -> String {
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return "empty" }
        var hasher = Hasher()
        hasher.combine(normalized)
        return "\(normalized.count):\(hasher.finalize())"
    }
}

private enum SettingsCollectionPreset: String, CaseIterable, Identifiable {
    case batterySaver
    case balanced
    case diagnostic
    case fullDetail

    var id: String { rawValue }

    var title: String {
        switch self {
        case .batterySaver:
            return "Battery Saver"
        case .balanced:
            return "Balanced"
        case .diagnostic:
            return "Diagnostic"
        case .fullDetail:
            return "Full Detail"
        }
    }

    var subtitle: String {
        switch self {
        case .batterySaver:
            return "Slow sampling for long-running observation on battery."
        case .balanced:
            return "Default profile for daily monitoring."
        case .diagnostic:
            return "Sharper sampling for short performance investigations."
        case .fullDetail:
            return "Maximum detail for brief, active debugging sessions."
        }
    }

    var systemImage: String {
        switch self {
        case .batterySaver:
            return "battery.75"
        case .balanced:
            return "dial.medium"
        case .diagnostic:
            return "waveform.path.ecg"
        case .fullDetail:
            return "scope"
        }
    }

    var color: Color {
        switch self {
        case .batterySaver:
            return AetowerDesign.Status.success
        case .balanced:
            return AetowerDesign.Status.ready
        case .diagnostic:
            return AetowerDesign.Status.warning
        case .fullDetail:
            return AetowerDesign.Status.error
        }
    }

    @MainActor
    func apply(to settings: SettingsStore) {
        switch self {
        case .batterySaver:
            settings.collectionProfile = .balanced
            settings.adaptiveCadenceEnabled = true
            settings.engineActiveIntervalSeconds = 3.0
            settings.engineIdleIntervalSeconds = 12.0
            settings.engineLowPowerIntervalSeconds = 20.0
            settings.gpuSampleIntervalSeconds = 90.0
            settings.gpuSampleLowPowerIntervalSeconds = 150.0
        case .balanced:
            settings.collectionProfile = .balanced
            settings.adaptiveCadenceEnabled = true
            settings.engineActiveIntervalSeconds = 2.0
            settings.engineIdleIntervalSeconds = 5.0
            settings.engineLowPowerIntervalSeconds = 8.0
            settings.gpuSampleIntervalSeconds = 30.0
            settings.gpuSampleLowPowerIntervalSeconds = 60.0
        case .diagnostic:
            settings.collectionProfile = .full
            settings.adaptiveCadenceEnabled = true
            settings.engineActiveIntervalSeconds = 1.0
            settings.engineIdleIntervalSeconds = 2.0
            settings.engineLowPowerIntervalSeconds = 5.0
            settings.gpuSampleIntervalSeconds = 10.0
            settings.gpuSampleLowPowerIntervalSeconds = 20.0
        case .fullDetail:
            settings.collectionProfile = .full
            settings.adaptiveCadenceEnabled = false
            settings.engineActiveIntervalSeconds = 1.0
            settings.engineIdleIntervalSeconds = 1.0
            settings.engineLowPowerIntervalSeconds = 3.0
            settings.gpuSampleIntervalSeconds = 5.0
            settings.gpuSampleLowPowerIntervalSeconds = 10.0
        }
    }
}

private enum SettingsValidationSeverity {
    case warning
    case error

    var color: Color {
        switch self {
        case .warning:
            return AetowerDesign.Status.warning
        case .error:
            return AetowerDesign.Status.error
        }
    }

    var systemImage: String {
        switch self {
        case .warning:
            return "exclamationmark.triangle.fill"
        case .error:
            return "xmark.octagon.fill"
        }
    }
}

private enum SettingsValidationTarget {
    case chromiumEndpoint
    case dockerSocketPath
    case privilegedHelperPath
    case chau7Endpoint
    case telemetryEndpoint
}

private struct SettingsValidationIssue: Identifiable {
    let target: SettingsValidationTarget
    let severity: SettingsValidationSeverity
    let message: String

    var id: String {
        "\(target)-\(severity)-\(message)"
    }

    @MainActor
    static func integrationIssues(for draft: SettingsIntegrationDraft) -> [SettingsValidationIssue] {
        var issues: [SettingsValidationIssue] = []

        let chromiumEndpoint = draft.chromiumEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        if !chromiumEndpoint.isEmpty {
            if !isHTTPURL(chromiumEndpoint) {
                issues.append(SettingsValidationIssue(
                    target: .chromiumEndpoint,
                    severity: .error,
                    message: "Use a full http:// or https:// browser debug endpoint."
                ))
            } else if let path = URLComponents(string: chromiumEndpoint)?.path,
                      !path.localizedLowercase.contains("json") {
                issues.append(SettingsValidationIssue(
                    target: .chromiumEndpoint,
                    severity: .warning,
                    message: "Most Chromium debug endpoints end with /json or /json/list."
                ))
            }
        }

        let dockerSocketPath = draft.dockerSocketPath.trimmingCharacters(in: .whitespacesAndNewlines)
        if !dockerSocketPath.isEmpty && !isAbsolutePath(dockerSocketPath) {
            issues.append(SettingsValidationIssue(
                target: .dockerSocketPath,
                severity: .error,
                message: "Use an absolute socket path, or leave this blank for the default."
            ))
        }

        let helperPath = draft.privilegedHelperPath.trimmingCharacters(in: .whitespacesAndNewlines)
        if draft.privilegedHelperEnabled && helperPath.isEmpty {
            issues.append(SettingsValidationIssue(
                target: .privilegedHelperPath,
                severity: .error,
                message: "Choose the installed helper path, or disable the signed helper."
            ))
        } else if !helperPath.isEmpty && !isAbsolutePath(helperPath) {
            issues.append(SettingsValidationIssue(
                target: .privilegedHelperPath,
                severity: .error,
                message: "Use an absolute helper path."
            ))
        }

        let chau7Endpoint = draft.chau7Endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        if !chau7Endpoint.isEmpty && !isAbsolutePath(chau7Endpoint) {
            issues.append(SettingsValidationIssue(
                target: .chau7Endpoint,
                severity: .error,
                message: "Use an absolute socket path, or leave this blank for auto-detect."
            ))
        }

        let telemetryEndpoint = SettingsStore.normalizedTelemetryEndpoint(draft.telemetryEndpoint)
        if draft.telemetryEnabled && !isHTTPURL(telemetryEndpoint) {
            issues.append(SettingsValidationIssue(
                target: .telemetryEndpoint,
                severity: .error,
                message: "Use a full http:// or https:// metrics collector endpoint."
            ))
        } else if !draft.telemetryEnabled,
                  !draft.telemetryEndpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !isHTTPURL(telemetryEndpoint) {
            issues.append(SettingsValidationIssue(
                target: .telemetryEndpoint,
                severity: .warning,
                message: "This endpoint will need a full http:// or https:// URL before export is enabled."
            ))
        }

        return issues
    }

    private static func isHTTPURL(_ value: String) -> Bool {
        guard let components = URLComponents(string: value),
              let scheme = components.scheme?.localizedLowercase,
              scheme == "http" || scheme == "https",
              let host = components.host,
              !host.isEmpty else {
            return false
        }
        return true
    }

    private static func isAbsolutePath(_ value: String) -> Bool {
        value.hasPrefix("/")
    }
}

private struct SettingsCard<Content: View>: View {
    let title: String
    let subtitle: String
    let status: SettingsStatus?
    let content: Content

    init(
        title: String,
        subtitle: String,
        status: SettingsStatus? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.subtitle = subtitle
        self.status = status
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Text(title)
                        .font(.headline)
                    if let status {
                        SettingsBadge(status.label, color: status.color)
                    }
                }
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.top, AetowerDesign.Spacing.md)
    }
}

private struct SettingsPresetCard: View {
    let preset: SettingsCollectionPreset

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Image(systemName: preset.systemImage)
                    .foregroundStyle(preset.color)
                    .frame(width: 20)
                Text(preset.title)
                    .font(.headline)
                Spacer()
            }
            Text(preset.subtitle)
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.leading)
        }
        .frame(maxWidth: .infinity, minHeight: 82, alignment: .topLeading)
        .padding(AetowerDesign.Spacing.md)
        .background(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                .fill(preset.color.opacity(0.08))
        )
        .overlay(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.md)
                .stroke(preset.color.opacity(0.20), lineWidth: 1)
        )
    }
}

private struct SettingsSetupCard<Content: View>: View {
    let title: String
    let subtitle: String
    let systemImage: String
    let badge: String
    let color: Color
    let content: Content

    init(
        title: String,
        subtitle: String,
        systemImage: String,
        badge: String,
        color: Color,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.subtitle = subtitle
        self.systemImage = systemImage
        self.badge = badge
        self.color = color
        self.content = content()
    }

    var body: some View {
        SettingsRowCard {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: systemImage)
                    .foregroundStyle(color)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(title)
                            .font(.headline)
                        SettingsBadge(badge, color: color)
                    }
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
                content
            }
            .padding(.leading, 36)
        }
    }
}

private struct SettingsValidationList: View {
    let issues: [SettingsValidationIssue]

    var body: some View {
        if !issues.isEmpty {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                ForEach(issues) { issue in
                    HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.xs) {
                        Image(systemName: issue.severity.systemImage)
                            .foregroundStyle(issue.severity.color)
                        Text(issue.message)
                            .font(.caption)
                            .foregroundStyle(issue.severity.color)
                    }
                }
            }
        }
    }
}

private struct SettingsChecklistRow: View {
    let title: String
    let detail: String
    let isComplete: Bool
    let actionTitle: String
    let action: () -> Void

    var body: some View {
        SettingsRowCard {
            HStack(alignment: .top, spacing: AetowerDesign.Spacing.md) {
                Image(systemName: isComplete ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(isComplete ? AetowerDesign.Status.success : AetowerDesign.Status.warning)
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(title)
                            .font(.headline)
                        SettingsBadge(
                            isComplete ? "Ready" : "Review",
                            color: isComplete ? AetowerDesign.Status.success : AetowerDesign.Status.warning
                        )
                    }
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer(minLength: AetowerDesign.Spacing.md)

                Button(actionTitle, action: action)
                    .buttonStyle(.bordered)
            }
        }
    }
}

/// The "Install Command Line Tool" card: symlinks the bundled `aetower` CLI
/// onto `$PATH` so it can be run from any shell. Self-contained state so it can
/// drop into the AI-clients section without threading through AppState.
private struct CommandLineToolCard: View {
    @State private var state: CommandLineToolInstaller.State = .notInstalled
    @State private var message: String?
    @State private var recoveryCommand: String?
    @State private var isError = false

    var body: some View {
        SettingsCard(
            title: "Command line tool",
            subtitle: "Run Aetower from any shell. PKG and Homebrew installs should link it automatically; DMG and ZIP installs can use this installer.",
            status: SettingsStatus(statusLabel, statusColor)
        ) {
            HStack(alignment: .firstTextBaseline, spacing: AetowerDesign.Spacing.sm) {
                Text(CommandLineToolInstaller.linkPath)
                    .font(AetowerDesign.Typography.caption.monospaced())
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .textSelection(.enabled)
                SettingsBadge(statusLabel, color: statusColor)
                Spacer()
            }

            Text("Smoke path: aetower top · aetower storage · aetower repos. The app must be running because the CLI reads the app-owned local MCP socket.")
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Button(state == .installed ? "Reinstall" : "Install Command Line Tool") {
                    run(CommandLineToolInstaller.install)
                }
                .buttonStyle(.borderedProminent)

                if state == .installed {
                    Button("Uninstall") {
                        run(CommandLineToolInstaller.uninstall)
                    }
                    .buttonStyle(.bordered)
                }

                if let recoveryCommand {
                    Button("Copy sudo command") {
                        copyToPasteboard(recoveryCommand)
                    }
                    .buttonStyle(.bordered)
                }
            }

            if let message {
                Text(message)
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(isError ? AetowerDesign.Status.warning : AetowerDesign.Ink.secondary)
                    .textSelection(.enabled)
            }
        }
        .onAppear { state = CommandLineToolInstaller.currentState() }
    }

    private var statusLabel: String {
        switch state {
        case .installed: return "Installed"
        case .notInstalled: return "Not installed"
        case .conflict: return "Conflict"
        case .unavailable: return "Unavailable"
        }
    }

    private var statusColor: Color {
        switch state {
        case .installed: return AetowerDesign.Status.success
        case .notInstalled: return AetowerDesign.Status.warning
        case .conflict, .unavailable: return AetowerDesign.Status.warning
        }
    }

    private func run(_ action: () throws -> String) {
        recoveryCommand = nil
        do {
            message = try action()
            isError = false
        } catch {
            isError = true
            message = error.localizedDescription
            if let installError = error as? CommandLineToolInstaller.InstallError {
                recoveryCommand = installError.recoveryCommand
            }
        }
        state = CommandLineToolInstaller.currentState()
    }

    private func copyToPasteboard(_ value: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
    }
}

private struct SettingsRowCard<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, AetowerDesign.Spacing.md)
    }
}

private struct SettingsBadge: View {
    let label: String
    let color: Color

    init(_ label: String, color: Color) {
        self.label = label
        self.color = color
    }

    var body: some View {
        Text(label)
            .font(.caption.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, AetowerDesign.Spacing.sm)
            .padding(.vertical, AetowerDesign.Spacing.xs)
            .background(color.opacity(0.12), in: Capsule())
    }
}

private struct SettingsNotice: View {
    let title: String
    let detail: String
    let color: Color

    var body: some View {
        HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(color)
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                Text(title)
                    .font(.caption.weight(.semibold))
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AetowerDesign.Spacing.md)
        .background(color.opacity(0.10), in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.md))
    }
}

private struct SettingDivider: View {
    var body: some View {
        Divider()
            .padding(.vertical, AetowerDesign.Spacing.xs)
    }
}
