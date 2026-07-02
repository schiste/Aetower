import SwiftUI

public struct ProjectsView: View {
    let state: AppState
    let settings: SettingsStore

    @State private var githubToken = ""
    @State private var cloudflareToken = ""
    @State private var githubCredentialSource: ProviderCredentialSource = .none
    @State private var cloudflareCredentialSource: ProviderCredentialSource = .none
    @State private var tokenStatusMessage: String?
    @State private var showAdvancedTokenSetup = false
    @State private var cloudflareLinkRequest: ProjectCloudflareLinkRequest?

    public init(state: AppState, settings: SettingsStore) {
        self.state = state
        self.settings = settings
    }

    public var body: some View {
        VStack(spacing: AetowerDesign.Spacing.none) {
            projectsToolBand
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.lg) {
                    connectorSetup
                    projectsContent
                    repositoryProjectCreation
                }
                .padding(AetowerDesign.Spacing.xxl)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .onAppear(perform: loadTokens)
        .sheet(item: $cloudflareLinkRequest) { request in
            ProjectCloudflareLinkSheet(
                projectName: request.project.name,
                onCancel: { cloudflareLinkRequest = nil },
                onSave: { link, environmentName, rank in
                    var project = request.project
                    project.upsertCloudflareLink(
                        link,
                        environmentName: environmentName,
                        rank: rank
                    )
                    state.upsertRepositoryProject(project)
                    state.refreshRepositoryCloudflareStatus(
                        repoRoot: project.primaryRepoRoot,
                        link: link,
                        force: true
                    )
                    cloudflareLinkRequest = nil
                }
            )
        }
    }

    private var projectsToolBand: some View {
        AetowerTabToolBand(
            searchText: .constant(""),
            searchPrompt: "Projects",
            searchWidth: 240
        ) {
            EmptyView()
        } badges: {
            AetowerToolBadgeGroup([
                AetowerToolBadgeItem(
                    "Projects",
                    value: "\(state.repositoryProjects.count)",
                    systemImage: "shippingbox",
                    tone: state.repositoryProjects.isEmpty
                        ? AetowerDesign.Status.neutral
                        : AetowerDesign.Status.ready
                ),
                AetowerToolBadgeItem(
                    "GitHub",
                    value: providerConfiguredLabel(source: githubCredentialSource),
                    systemImage: "chevron.left.forwardslash.chevron.right",
                    tone: providerConfiguredTone(source: githubCredentialSource)
                ),
                AetowerToolBadgeItem(
                    "Cloudflare",
                    value: providerConfiguredLabel(source: cloudflareCredentialSource),
                    systemImage: "cloud",
                    tone: providerConfiguredTone(source: cloudflareCredentialSource)
                ),
            ], visibleCount: 3)
        }
    }

    private var connectorSetup: some View {
        AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                    Label("Project Connectors", systemImage: "point.3.connected.trianglepath.dotted")
                        .font(AetowerDesign.Typography.sectionTitle)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Spacer(minLength: AetowerDesign.Spacing.md)
                    AetowerBadge(
                        "GitHub \(providerConfiguredLabel(source: githubCredentialSource))",
                        tone: providerConfiguredTone(source: githubCredentialSource)
                    )
                    AetowerBadge(
                        "Cloudflare \(providerConfiguredLabel(source: cloudflareCredentialSource))",
                        tone: providerConfiguredTone(source: cloudflareCredentialSource)
                    )
                }

                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 320), spacing: AetowerDesign.Spacing.md)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.md
                ) {
                    githubConnectorCard
                    cloudflareConnectorCard
                }

                advancedTokenSetup
            }
        }
    }

    private var githubConnectorCard: some View {
        providerConnectorFrame(
            title: "GitHub",
            systemImage: "chevron.left.forwardslash.chevron.right",
            provider: .github,
            source: githubCredentialSource
        ) {
            GitHubOAuthConnectorView(
                clientID: githubOAuthClientIDBinding,
                scopesText: githubOAuthScopesBinding,
                showsConfigurationFields: false,
                showsHelpText: false,
                showsDisconnectButton: false,
                onConfigurationApplied: persistGitHubOAuthConfiguration,
                onCredentialChanged: loadTokens
            )
            connectorActionRow(provider: .github) {
                refreshGitHubConnector()
            }
        }
    }

    private var cloudflareConnectorCard: some View {
        providerConnectorFrame(
            title: "Cloudflare",
            systemImage: "cloud",
            provider: .cloudflare,
            source: cloudflareCredentialSource
        ) {
            CloudflareOAuthConnectorView(
                accountID: cloudflareOAuthAccountIDBinding,
                clientID: cloudflareOAuthClientIDBinding,
                scopesText: cloudflareOAuthScopesBinding,
                redirectURI: cloudflareOAuthRedirectURIBinding,
                showsConfigurationFields: false,
                showsRegistrationButton: false,
                showsHelpText: false,
                showsDisconnectButton: false,
                apiTokenProvider: { cloudflareToken },
                onConfigurationApplied: persistCloudflareOAuthConfiguration,
                onCredentialChanged: loadTokens
            )
            connectorActionRow(provider: .cloudflare) {
                refreshCloudflareConnector()
            }
        }
    }

    private func providerConnectorFrame<Content: View>(
        title: String,
        systemImage: String,
        provider: ProjectProvider,
        source: ProviderCredentialSource,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.sm) {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                Label(title, systemImage: systemImage)
                    .font(AetowerDesign.Typography.controlLabel)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                Spacer(minLength: AetowerDesign.Spacing.sm)
                AetowerBadge(
                    providerConnectionBadgeLabel(source: source),
                    tone: providerConfiguredTone(source: source)
                )
            }

            Text(providerConnectionSummary(provider: provider, source: source))
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)
                .lineLimit(2)

            content()
        }
        .padding(AetowerDesign.Spacing.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            AetowerDesign.Surface.card,
            in: RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm)
        )
        .overlay(
            RoundedRectangle(cornerRadius: AetowerDesign.Radius.sm)
                .stroke(AetowerDesign.Surface.divider, lineWidth: AetowerDesign.Stroke.hairline)
        )
    }

    private func connectorActionRow(
        provider: ProjectProvider,
        refresh: @escaping () -> Void
    ) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            Button {
                disconnectProvider(provider)
            } label: {
                Label("Disconnect", systemImage: "xmark.circle")
            }
            .disabled(providerCredentialSource(provider) == .none)

            Button {
                refresh()
            } label: {
                Label("Refresh", systemImage: "arrow.clockwise")
            }
        }
        .controlSize(.small)
    }

    private var advancedTokenSetup: some View {
        DisclosureGroup(isExpanded: $showAdvancedTokenSetup) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 320), spacing: AetowerDesign.Spacing.md)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.md
                ) {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("GitHub")
                            .font(AetowerDesign.Typography.controlLabel)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                        GitHubOAuthConnectorView(
                            clientID: githubOAuthClientIDBinding,
                            scopesText: githubOAuthScopesBinding,
                            onConfigurationApplied: persistGitHubOAuthConfiguration,
                            onCredentialChanged: loadTokens
                        )
                        SecureField("Manual GitHub token", text: $githubToken)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                    }

                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                        Text("Cloudflare")
                            .font(AetowerDesign.Typography.controlLabel)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                        CloudflareOAuthConnectorView(
                            accountID: cloudflareOAuthAccountIDBinding,
                            clientID: cloudflareOAuthClientIDBinding,
                            scopesText: cloudflareOAuthScopesBinding,
                            redirectURI: cloudflareOAuthRedirectURIBinding,
                            apiTokenProvider: { cloudflareToken },
                            onConfigurationApplied: persistCloudflareOAuthConfiguration,
                            onCredentialChanged: loadTokens
                        )
                        SecureField("Read-only API token", text: $cloudflareToken)
                            .textFieldStyle(.roundedBorder)
                            .aetowerUtilityTextInput()
                        Text("Use account-scoped read access for Pages and Workers Scripts.")
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                    }
                }

                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Button {
                        saveTokens()
                    } label: {
                        Label("Save connectors", systemImage: "key")
                    }
                    .buttonStyle(.borderedProminent)

                    Button {
                        loadTokens()
                    } label: {
                        Label("Reload", systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(.bordered)

                    if let tokenStatusMessage {
                        Text(tokenStatusMessage)
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                    }
                }
                .controlSize(.small)
            }
            .padding(.top, AetowerDesign.Spacing.sm)
        } label: {
            Label("Advanced token setup", systemImage: "key")
                .font(AetowerDesign.Typography.controlLabel)
                .foregroundStyle(AetowerDesign.Ink.primary)
        }
    }

    @ViewBuilder
    private var projectsContent: some View {
        if state.repositoryProjects.isEmpty {
            AetowerEmptyState(
                title: "No projects yet",
                detail: "Create one from a discovered repository below, then link GitHub and Cloudflare resources.",
                systemImage: "shippingbox",
                tone: AetowerDesign.Status.neutral
            )
        } else {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                Text("Projects")
                    .font(AetowerDesign.Typography.sectionTitle)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 360), spacing: AetowerDesign.Spacing.md)],
                    alignment: .leading,
                    spacing: AetowerDesign.Spacing.md
                ) {
                    ForEach(state.repositoryProjects) { project in
                        projectCard(project)
                    }
                }
            }
        }
    }

    private var repositoryProjectCreation: some View {
        AetowerSurface(level: .quiet, padding: AetowerDesign.Spacing.md) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
                    Text("Create From Repositories")
                        .font(AetowerDesign.Typography.sectionTitle)
                        .foregroundStyle(AetowerDesign.Ink.primary)
                    Spacer(minLength: AetowerDesign.Spacing.md)
                    AetowerBadge("\(availableRepositories.count) available",
                                 tone: AetowerDesign.Status.neutral)
                }

                if availableRepositories.isEmpty {
                    Text("Run a repository scan in the Repos tab to populate project candidates.")
                        .font(AetowerDesign.Typography.caption)
                        .foregroundStyle(AetowerDesign.Ink.secondary)
                } else {
                    VStack(spacing: AetowerDesign.Spacing.xs) {
                        ForEach(availableRepositories.prefix(12)) { repository in
                            repositoryCandidateRow(repository)
                        }
                    }
                }
            }
        }
    }

    private func projectCard(_ project: RepositoryProjectModel) -> some View {
        AetowerSurface(
            level: projectSurfaceLevel(project),
            padding: AetowerDesign.Spacing.md
        ) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.md) {
                HStack(alignment: .top, spacing: AetowerDesign.Spacing.sm) {
                    VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                        Text(project.name)
                            .font(AetowerDesign.Typography.controlLabel)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                            .lineLimit(1)
                        Text(shortPath(project.primaryRepoRoot))
                            .font(AetowerDesign.Typography.metadata)
                            .foregroundStyle(AetowerDesign.Ink.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer(minLength: AetowerDesign.Spacing.sm)
                    AetowerBadge(projectHealthLabel(project), tone: projectHealthTone(project))
                }

                projectGitHubRow(project)
                projectCloudflareGroups(project)

                HStack(spacing: AetowerDesign.Spacing.sm) {
                    Button {
                        linkGitHub(project)
                    } label: {
                        Label(project.githubRepositoryLink == nil ? "Link GitHub" : "Update GitHub",
                              systemImage: "chevron.left.forwardslash.chevron.right")
                    }
                    .disabled(githubLink(for: project) == nil)

                    Button {
                        refreshGitHub(project)
                    } label: {
                        Label("Refresh GitHub", systemImage: "arrow.clockwise")
                    }
                    .disabled(project.githubRepositoryLink == nil)

                    Button {
                        cloudflareLinkRequest = ProjectCloudflareLinkRequest(project: project)
                    } label: {
                        Label("Add Cloudflare", systemImage: "cloud")
                    }
                }
                .controlSize(.small)

                if let error = state.repositoryProjectGitHubErrorsByRoot[project.primaryRepoRoot] {
                    projectProviderWarning(error, systemImage: "exclamationmark.triangle")
                }
            }
        }
    }

    private func projectGitHubRow(_ project: RepositoryProjectModel) -> some View {
        HStack(alignment: .center, spacing: AetowerDesign.Spacing.sm) {
            AetowerBadge("GitHub", systemImage: "chevron.left.forwardslash.chevron.right",
                         tone: AetowerDesign.Tone.cpu)
            if let status = project.githubStatus {
                Text(githubStatusSummary(status))
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
            } else if project.githubRepositoryLink != nil {
                Text("Linked, not refreshed")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            } else {
                Text("Not linked")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }
            Spacer(minLength: AetowerDesign.Spacing.sm)
            if state.repositoryProjectGitHubLoadingRoots.contains(project.primaryRepoRoot) {
                ProgressView()
                    .controlSize(.small)
            }
        }
    }

    @ViewBuilder
    private func projectCloudflareGroups(_ project: RepositoryProjectModel) -> some View {
        if project.cloudflareEnvironmentGroups.isEmpty {
            HStack(spacing: AetowerDesign.Spacing.sm) {
                AetowerBadge("Cloudflare", systemImage: "cloud", tone: AetowerDesign.Tone.network)
                Text("No environments linked")
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
            }
        } else {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
                ForEach(project.cloudflareEnvironmentGroups) { group in
                    projectCloudflareGroup(group, project: project)
                }
            }
        }
    }

    private func projectCloudflareGroup(
        _ group: RepositoryProjectCloudflareEnvironmentGroup,
        project: RepositoryProjectModel
    ) -> some View {
        VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xs) {
            HStack(spacing: AetowerDesign.Spacing.xs) {
                AetowerBadge(group.name, systemImage: "server.rack",
                             tone: cloudflareGroupTone(group, project: project))
                Text(cloudflareGroupSummary(group, project: project))
                    .font(AetowerDesign.Typography.caption)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                Spacer(minLength: AetowerDesign.Spacing.sm)
            }

            ForEach(group.links) { link in
                VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                    HStack(spacing: AetowerDesign.Spacing.sm) {
                        Text(cloudflareLinkTitle(link))
                            .font(AetowerDesign.Typography.metadataStrong)
                            .foregroundStyle(AetowerDesign.Ink.primary)
                            .lineLimit(1)
                        if let status = project.cloudflareStatus(for: link) {
                            Text(cloudflareStatusSummary(status))
                                .font(AetowerDesign.Typography.metadata)
                                .foregroundStyle(AetowerDesign.Ink.secondary)
                                .lineLimit(1)
                        } else {
                            Text("Not refreshed")
                                .font(AetowerDesign.Typography.metadata)
                                .foregroundStyle(AetowerDesign.Ink.secondary)
                        }
                        Spacer(minLength: AetowerDesign.Spacing.sm)
                        Button {
                            state.refreshRepositoryCloudflareStatus(
                                repoRoot: project.primaryRepoRoot,
                                link: link,
                                force: true
                            )
                        } label: {
                            Image(systemName: "checkmark.circle")
                        }
                        .help("Test connection")
                        Button {
                            state.refreshRepositoryCloudflareStatus(
                                repoRoot: project.primaryRepoRoot,
                                link: link,
                                force: false
                            )
                        } label: {
                            Image(systemName: "arrow.clockwise")
                        }
                        .help("Refresh deployment")
                    }
                    if let error = state.repositoryProjectCloudflareErrorsByKey[
                        state.repositoryCloudflareProviderKey(
                            repoRoot: project.primaryRepoRoot,
                            link: link
                        )
                    ] {
                        projectProviderWarning(error, systemImage: "exclamationmark.triangle")
                    }
                }
                .controlSize(.small)
            }
        }
    }

    private func repositoryCandidateRow(_ repository: StorageRepositoryInventoryModel) -> some View {
        HStack(spacing: AetowerDesign.Spacing.sm) {
            VStack(alignment: .leading, spacing: AetowerDesign.Spacing.xxs) {
                Text(repository.repoName)
                    .font(AetowerDesign.Typography.caption.weight(.semibold))
                    .foregroundStyle(AetowerDesign.Ink.primary)
                    .lineLimit(1)
                Text(shortPath(repository.repoRoot))
                    .font(AetowerDesign.Typography.metadata)
                    .foregroundStyle(AetowerDesign.Ink.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: AetowerDesign.Spacing.md)
            if repositoryRemoteIsGitHub(repository) {
                AetowerBadge("GitHub", tone: AetowerDesign.Tone.cpu)
            }
            Button {
                createProject(from: repository)
            } label: {
                Label("Create", systemImage: "plus")
            }
            .controlSize(.small)
        }
    }

    private var availableRepositories: [StorageRepositoryInventoryModel] {
        let existingRoots = Set(state.repositoryProjects.map(\.primaryRepoRoot))
        return (state.storageHygieneReport?.repositoryInventory ?? [])
            .filter { !existingRoots.contains(RepositoryProjectModel.normalizedRepoRoot($0.repoRoot)) }
            .sorted { $0.repoName.localizedCaseInsensitiveCompare($1.repoName) == .orderedAscending }
    }

    private func createProject(from repository: StorageRepositoryInventoryModel) {
        var links: [RepositoryProjectLinkModel] = []
        if let link = githubLink(for: repository) {
            links.append(link)
        }
        let project = RepositoryProjectModel(
            name: repository.repoName,
            primaryRepoRoot: repository.repoRoot,
            repoRemote: repository.gitRemoteOriginUrl ?? repository.gitRemoteKey ?? "",
            links: links
        )
        state.upsertRepositoryProject(project)
        if !links.isEmpty {
            state.refreshRepositoryGitHubStatus(
                repoRoot: project.primaryRepoRoot,
                currentBranch: repository.gitBranch,
                currentHead: repository.gitHead,
                force: true
            )
        }
    }

    private func linkGitHub(_ project: RepositoryProjectModel) {
        guard let link = githubLink(for: project) else { return }
        var updated = project
        if !updated.links.contains(where: { $0.identityKey == link.identityKey }) {
            updated.links.append(link)
        }
        state.upsertRepositoryProject(updated)
        refreshGitHub(updated)
    }

    private func refreshGitHub(_ project: RepositoryProjectModel) {
        let repository = inventory(for: project)
        state.refreshRepositoryGitHubStatus(
            repoRoot: project.primaryRepoRoot,
            currentBranch: repository?.gitBranch,
            currentHead: repository?.gitHead,
            force: true
        )
    }

    private var githubOAuthClientIDBinding: Binding<String> {
        Binding(
            get: { settings.githubOAuthClientID },
            set: { settings.githubOAuthClientID = $0 }
        )
    }

    private var githubOAuthScopesBinding: Binding<String> {
        Binding(
            get: { settings.githubOAuthScopes },
            set: { settings.githubOAuthScopes = $0 }
        )
    }

    private var cloudflareOAuthClientIDBinding: Binding<String> {
        Binding(
            get: { settings.cloudflareOAuthClientID },
            set: { settings.cloudflareOAuthClientID = $0 }
        )
    }

    private var cloudflareOAuthAccountIDBinding: Binding<String> {
        Binding(
            get: { settings.cloudflareOAuthAccountID },
            set: { settings.cloudflareOAuthAccountID = $0 }
        )
    }

    private var cloudflareOAuthScopesBinding: Binding<String> {
        Binding(
            get: { settings.cloudflareOAuthScopes },
            set: { settings.cloudflareOAuthScopes = $0 }
        )
    }

    private var cloudflareOAuthRedirectURIBinding: Binding<String> {
        Binding(
            get: { settings.cloudflareOAuthRedirectURI },
            set: { settings.cloudflareOAuthRedirectURI = $0 }
        )
    }

    private func persistGitHubOAuthConfiguration(clientID: String, scopesText: String) {
        settings.githubOAuthClientID = clientID
        settings.githubOAuthScopes = scopesText
    }

    private func persistCloudflareOAuthConfiguration(
        accountID: String,
        clientID: String,
        scopesText: String,
        redirectURI: String
    ) {
        settings.cloudflareOAuthAccountID = accountID
        settings.cloudflareOAuthClientID = clientID
        settings.cloudflareOAuthScopes = scopesText
        settings.cloudflareOAuthRedirectURI = redirectURI
    }

    private func loadTokens() {
        let credentialStore = ProviderCredentialStore()
        githubToken = credentialStore.manualToken(for: .github) ?? ""
        cloudflareToken = credentialStore.manualToken(for: .cloudflare) ?? ""
        githubCredentialSource = credentialStore.credentialSource(for: .github)
        cloudflareCredentialSource = credentialStore.credentialSource(for: .cloudflare)
    }

    private func saveTokens() {
        let credentialStore = ProviderCredentialStore()
        let githubSaved = credentialStore.storeManualToken(githubToken, for: .github)
        let cloudflareSaved = credentialStore.storeManualToken(cloudflareToken, for: .cloudflare)
        githubCredentialSource = credentialStore.credentialSource(for: .github)
        cloudflareCredentialSource = credentialStore.credentialSource(for: .cloudflare)
        tokenStatusMessage = githubSaved && cloudflareSaved
            ? "Saved"
            : "Could not update keychain"
    }

    private func providerCredentialSource(_ provider: ProjectProvider) -> ProviderCredentialSource {
        switch provider {
        case .github:
            return githubCredentialSource
        case .cloudflare:
            return cloudflareCredentialSource
        }
    }

    private func disconnectProvider(_ provider: ProjectProvider) {
        let credentialStore = ProviderCredentialStore()
        let source = credentialStore.credentialSource(for: provider)
        switch source {
        case .oauth:
            disconnectOAuthProvider(provider, credentialStore: credentialStore)
        case .manualToken:
            credentialStore.storeManualToken("", for: provider)
            if provider == .github {
                githubToken = ""
            } else {
                cloudflareToken = ""
            }
            loadTokens()
            tokenStatusMessage = "\(providerDisplayName(provider)) disconnected."
        case .none:
            tokenStatusMessage = "\(providerDisplayName(provider)) is not connected."
        }
    }

    private func disconnectOAuthProvider(
        _ provider: ProjectProvider,
        credentialStore: ProviderCredentialStore
    ) {
        if provider == .cloudflare,
           let accessToken = credentialStore.oauthCredential(for: .cloudflare)?.accessToken,
           !settings.cloudflareOAuthClientID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            tokenStatusMessage = "Disconnecting Cloudflare."
            let clientID = settings.cloudflareOAuthClientID.trimmingCharacters(in: .whitespacesAndNewlines)
            Task { @MainActor in
                _ = try? await CloudflareOAuthPKCEClient.live.revoke(
                    accessToken: accessToken,
                    clientID: clientID
                )
                ProviderCredentialStore().clearOAuthCredential(for: .cloudflare)
                loadTokens()
                tokenStatusMessage = "Cloudflare disconnected."
            }
            return
        }

        credentialStore.clearOAuthCredential(for: provider)
        loadTokens()
        tokenStatusMessage = "\(providerDisplayName(provider)) disconnected."
    }

    private func refreshGitHubConnector() {
        loadTokens()
        let projects = state.repositoryProjects.filter { $0.githubRepositoryLink != nil }
        for project in projects {
            refreshGitHub(project)
        }
        tokenStatusMessage = projects.isEmpty
            ? "GitHub connection refreshed."
            : "Refreshing GitHub for \(projects.count) project\(projects.count == 1 ? "" : "s")."
    }

    private func refreshCloudflareConnector() {
        loadTokens()
        var refreshCount = 0
        for project in state.repositoryProjects {
            for link in project.cloudflareLinks {
                state.refreshRepositoryCloudflareStatus(
                    repoRoot: project.primaryRepoRoot,
                    link: link,
                    force: true
                )
                refreshCount += 1
            }
        }
        tokenStatusMessage = refreshCount == 0
            ? "Cloudflare connection refreshed."
            : "Refreshing Cloudflare for \(refreshCount) resource\(refreshCount == 1 ? "" : "s")."
    }

    private func providerConnectionBadgeLabel(source: ProviderCredentialSource) -> String {
        switch source {
        case .oauth:
            return "OAuth connected"
        case .manualToken:
            return "Manual token"
        case .none:
            return "Not connected"
        }
    }

    private func providerConnectionSummary(
        provider: ProjectProvider,
        source: ProviderCredentialSource
    ) -> String {
        switch source {
        case .oauth:
            if let identity = ProviderCredentialStore()
                .metadata(for: provider)
                .oauth?
                .accountIdentity
            {
                return "Connected as \(identity)"
            }
            return "Connected with OAuth."
        case .manualToken:
            return "Manual token configured."
        case .none:
            return "Connect OAuth or add a token in Advanced token setup."
        }
    }

    private func providerDisplayName(_ provider: ProjectProvider) -> String {
        switch provider {
        case .github:
            return "GitHub"
        case .cloudflare:
            return "Cloudflare"
        }
    }

    private func githubLink(for project: RepositoryProjectModel) -> RepositoryProjectLinkModel? {
        if let link = project.githubRepositoryLink {
            return link
        }
        if let repository = inventory(for: project) {
            return githubLink(for: repository)
        }
        return Self.githubLink(remote: project.repoRemote)
    }

    private func githubLink(for repository: StorageRepositoryInventoryModel) -> RepositoryProjectLinkModel? {
        if repositoryRemoteIsGitHub(repository),
           let owner = repository.gitRemoteOwner,
           let repo = repository.gitRemoteName,
           !owner.isEmpty,
           !repo.isEmpty
        {
            return .githubRepository(owner: owner, repo: repo)
        }
        return Self.githubLink(remote: repository.gitRemoteOriginUrl ?? repository.gitRemoteKey ?? "")
    }

    private func repositoryRemoteIsGitHub(_ repository: StorageRepositoryInventoryModel) -> Bool {
        let host = repository.gitRemoteHost?.lowercased() ?? ""
        let remote = (repository.gitRemoteOriginUrl ?? repository.gitRemoteKey ?? "").lowercased()
        return host == "github.com" || remote.contains("github.com")
    }

    private func inventory(for project: RepositoryProjectModel) -> StorageRepositoryInventoryModel? {
        state.storageHygieneReport?.repositoryInventory.first {
            RepositoryProjectModel.normalizedRepoRoot($0.repoRoot) == project.primaryRepoRoot
        }
    }

    private static func githubLink(remote: String) -> RepositoryProjectLinkModel? {
        let trimmed = remote.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let path: String
        if trimmed.hasPrefix("git@github.com:") {
            path = String(trimmed.dropFirst("git@github.com:".count))
        } else if trimmed.hasPrefix("https://github.com/") {
            path = String(trimmed.dropFirst("https://github.com/".count))
        } else if trimmed.hasPrefix("http://github.com/") {
            path = String(trimmed.dropFirst("http://github.com/".count))
        } else {
            return nil
        }
        let cleanPath = path.hasSuffix(".git") ? String(path.dropLast(4)) : path
        let parts = cleanPath.split(separator: "/", maxSplits: 1).map(String.init)
        guard parts.count == 2 else { return nil }
        return .githubRepository(owner: parts[0], repo: parts[1])
    }

    private func projectSurfaceLevel(_ project: RepositoryProjectModel) -> AetowerSurfaceLevel {
        if projectHealthLabel(project) == "Failed" { return .critical }
        if projectHealthLabel(project) == "Check" { return .warning }
        return .quiet
    }

    private func projectHealthLabel(_ project: RepositoryProjectModel) -> String {
        if project.cloudflareStatuses?.contains(where: \.hasFailedDeployment) == true {
            return "Failed"
        }
        if project.githubStatus?.failedLatestCIOnDefaultBranch == true {
            return "Failed"
        }
        if (project.githubStatus?.staleOpenPullRequestCount() ?? 0) > 0 {
            return "Check"
        }
        if project.githubStatus != nil || project.cloudflareStatuses?.isEmpty == false {
            return "OK"
        }
        return "Unscanned"
    }

    private func projectHealthTone(_ project: RepositoryProjectModel) -> Color {
        switch projectHealthLabel(project) {
        case "Failed": return AetowerDesign.Status.error
        case "Check": return AetowerDesign.Status.warning
        case "OK": return AetowerDesign.Status.ready
        default: return AetowerDesign.Status.neutral
        }
    }

    private func githubStatusSummary(_ status: RepositoryGitHubProviderStatusModel) -> String {
        if status.status == "auth_needed" {
            return status.warnings.first ?? "Needs GitHub token"
        }
        if status.status == "failed" || status.status == "unavailable" {
            return status.warnings.first ?? "GitHub status unavailable"
        }
        if status.failedLatestCIOnDefaultBranch {
            return "CI failed on \(status.defaultBranch ?? "default branch")"
        }
        let stale = status.staleOpenPullRequestCount()
        if stale > 0 {
            return "\(stale) stale PR\(stale == 1 ? "" : "s")"
        }
        return "\(status.openPrCount) open PR\(status.openPrCount == 1 ? "" : "s") · checks \(status.latestCheckState)"
    }

    private func cloudflareGroupSummary(
        _ group: RepositoryProjectCloudflareEnvironmentGroup,
        project: RepositoryProjectModel
    ) -> String {
        let statuses = group.links.compactMap { project.cloudflareStatus(for: $0) }
        if statuses.isEmpty { return "Not refreshed" }
        if statuses.contains(where: \.hasFailedDeployment) { return "Deployment failed" }
        if statuses.contains(where: { $0.status == "auth_needed" }) { return "Needs token" }
        return "Deployment healthy"
    }

    private func cloudflareGroupTone(
        _ group: RepositoryProjectCloudflareEnvironmentGroup,
        project: RepositoryProjectModel
    ) -> Color {
        let statuses = group.links.compactMap { project.cloudflareStatus(for: $0) }
        if statuses.contains(where: \.hasFailedDeployment) { return AetowerDesign.Status.error }
        if statuses.contains(where: { ["auth_needed", "failed", "unavailable", "warning"].contains($0.status) }) {
            return AetowerDesign.Status.warning
        }
        return statuses.isEmpty ? AetowerDesign.Status.neutral : AetowerDesign.Status.ready
    }

    private func cloudflareStatusSummary(_ status: RepositoryCloudflareProviderStatusModel) -> String {
        if status.status == "auth_needed" {
            return status.warnings.first ?? "Needs Cloudflare token"
        }
        if status.status == "failed" || status.status == "unavailable" {
            return status.warnings.first ?? "Cloudflare status unavailable"
        }
        return [
            status.deploymentStatus?.capitalized ?? status.status.capitalized,
            status.environment?.capitalized,
            status.branch,
            status.commit.map { String($0.prefix(8)) },
        ]
        .compactMap { $0 }
        .joined(separator: " · ")
    }

    private func cloudflareLinkTitle(_ link: RepositoryProjectLinkModel) -> String {
        switch link.kind {
        case .pages:
            return "Pages \(link.projectName ?? "project")"
        case .worker:
            return "Worker \(link.scriptName ?? "script")"
        default:
            return link.provider.rawValue.capitalized
        }
    }

    private func providerConfiguredLabel(source: ProviderCredentialSource) -> String {
        providerConnectionBadgeLabel(source: source)
    }

    private func providerConfiguredTone(source: ProviderCredentialSource) -> Color {
        source == .none ? AetowerDesign.Status.neutral : AetowerDesign.Status.ready
    }

    private func shortPath(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        if path == home { return "~" }
        if path.hasPrefix(home + "/") {
            return "~" + path.dropFirst(home.count)
        }
        return path
    }

    private func projectProviderWarning(_ message: String, systemImage: String) -> some View {
        Label(message, systemImage: systemImage)
            .font(AetowerDesign.Typography.metadata)
            .foregroundStyle(AetowerDesign.Status.warning)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
    }
}

private struct ProjectCloudflareLinkRequest: Identifiable {
    let project: RepositoryProjectModel

    var id: String { project.id }
}

private struct ProjectCloudflareLinkSheet: View {
    let projectName: String
    let onCancel: () -> Void
    let onSave: (RepositoryProjectLinkModel, String, Int) -> Void

    @State private var kind: CloudflareKind = .pages
    @State private var environmentPreset: EnvironmentPreset = .production
    @State private var environmentName = EnvironmentPreset.production.defaultName
    @State private var pagesDeploymentEnvironment: PagesDeploymentEnvironment = .production
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
                Label("Add Cloudflare", systemImage: "cloud")
                    .font(AetowerDesign.Typography.sectionTitle)
                    .foregroundStyle(AetowerDesign.Ink.primary)
                Spacer(minLength: AetowerDesign.Spacing.md)
                Button(action: onCancel) {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.borderless)
                .keyboardShortcut(.cancelAction)
            }

            Text(projectName)
                .font(AetowerDesign.Typography.caption)
                .foregroundStyle(AetowerDesign.Ink.secondary)

            Picker("Environment", selection: $environmentPreset) {
                ForEach(EnvironmentPreset.allCases) { preset in
                    Text(preset.label).tag(preset)
                }
            }
            .pickerStyle(.segmented)
            .onChange(of: environmentPreset) { _, preset in
                guard preset != .custom else { return }
                environmentName = preset.defaultName
                pagesDeploymentEnvironment = preset.pagesEnvironment
            }

            TextField("Environment name", text: $environmentName)
                .textFieldStyle(.roundedBorder)
                .aetowerUtilityTextInput()
                .onChange(of: environmentName) { _, value in
                    if environmentPreset != .custom, value != environmentPreset.defaultName {
                        environmentPreset = .custom
                    }
                }

            Picker("Cloudflare kind", selection: $kind) {
                ForEach(CloudflareKind.allCases) { option in
                    Text(option.label).tag(option)
                }
            }
            .pickerStyle(.segmented)

            if kind == .pages {
                Picker("Pages deployment", selection: $pagesDeploymentEnvironment) {
                    ForEach(PagesDeploymentEnvironment.allCases) { option in
                        Text(option.label).tag(option)
                    }
                }
                .pickerStyle(.segmented)
            }

            TextField("Account ID", text: $accountID)
                .textFieldStyle(.roundedBorder)
                .aetowerUtilityTextInput()
            TextField(kind == .pages ? "Pages project name" : "Worker script name", text: $resourceName)
                .textFieldStyle(.roundedBorder)
                .aetowerUtilityTextInput()
            TextField("Branch or ref", text: $branch)
                .textFieldStyle(.roundedBorder)
                .aetowerUtilityTextInput()

            HStack(spacing: AetowerDesign.Spacing.sm) {
                Spacer(minLength: AetowerDesign.Spacing.md)
                Button("Cancel", role: .cancel, action: onCancel)
                Button {
                    onSave(link, environmentName, environmentPreset.rank)
                } label: {
                    Label("Add", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(!canSave)
            }
            .controlSize(.small)
        }
        .padding(AetowerDesign.Spacing.xxl)
        .frame(width: 440)
    }

    private var link: RepositoryProjectLinkModel {
        switch kind {
        case .pages:
            return .cloudflarePages(
                accountId: accountID,
                projectName: resourceName,
                deploymentEnvironment: pagesDeploymentEnvironment.apiValue,
                branch: sanitizedOptional(branch)
            )
        case .worker:
            return .cloudflareWorker(
                accountId: accountID,
                scriptName: resourceName,
                branch: sanitizedOptional(branch)
            )
        }
    }

    private func sanitizedOptional(_ value: String) -> String? {
        let sanitized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return sanitized.isEmpty ? nil : sanitized
    }

    private enum CloudflareKind: String, CaseIterable, Identifiable {
        case pages
        case worker

        var id: Self { self }
        var label: String { self == .pages ? "Pages" : "Worker" }
    }

    private enum EnvironmentPreset: String, CaseIterable, Identifiable {
        case production
        case staging
        case development
        case custom

        var id: Self { self }

        var label: String {
            switch self {
            case .production: return "Production"
            case .staging: return "Staging"
            case .development: return "Dev"
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

        var pagesEnvironment: PagesDeploymentEnvironment {
            switch self {
            case .production: return .production
            case .staging, .development: return .preview
            case .custom: return .any
            }
        }
    }

    private enum PagesDeploymentEnvironment: String, CaseIterable, Identifiable {
        case any
        case production
        case preview

        var id: Self { self }

        var label: String {
            switch self {
            case .any: return "Any"
            case .production: return "Production"
            case .preview: return "Preview"
            }
        }

        var apiValue: String? {
            self == .any ? nil : rawValue
        }
    }
}
