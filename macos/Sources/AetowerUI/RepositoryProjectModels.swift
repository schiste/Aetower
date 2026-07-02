import Foundation

struct RepositoryProjectModel: Codable, Identifiable, Equatable, Sendable {
    let id: String
    var name: String
    var primaryRepoRoot: String
    var repoRemote: String
    var links: [RepositoryProjectLinkModel]
    var environments: [RepositoryProjectEnvironmentModel]
    var githubStatus: RepositoryGitHubProviderStatusModel?
    var cloudflareStatuses: [RepositoryCloudflareProviderStatusModel]?
    let createdAtMillis: UInt64
    var updatedAtMillis: UInt64

    init(
        id: String = UUID().uuidString,
        name: String,
        primaryRepoRoot: String,
        repoRemote: String = "",
        links: [RepositoryProjectLinkModel] = [],
        environments: [RepositoryProjectEnvironmentModel] = [],
        githubStatus: RepositoryGitHubProviderStatusModel? = nil,
        cloudflareStatuses: [RepositoryCloudflareProviderStatusModel]? = nil,
        createdAtMillis: UInt64 = Self.currentMillis(),
        updatedAtMillis: UInt64 = Self.currentMillis()
    ) {
        self.id = id
        self.name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        self.primaryRepoRoot = Self.normalizedRepoRoot(primaryRepoRoot)
        self.repoRemote = repoRemote.trimmingCharacters(in: .whitespacesAndNewlines)
        self.links = Self.deduplicatedLinks(links)
        self.environments = Self.deduplicatedEnvironments(environments)
        self.githubStatus = githubStatus
        self.cloudflareStatuses = cloudflareStatuses
        self.createdAtMillis = createdAtMillis
        self.updatedAtMillis = updatedAtMillis
    }

    var isConfigured: Bool {
        !name.isEmpty && !primaryRepoRoot.isEmpty
    }

    var githubRepositoryLink: RepositoryProjectLinkModel? {
        links.first { $0.provider == .github && $0.kind == .repository }
    }

    var cloudflareLinks: [RepositoryProjectLinkModel] {
        Self.deduplicatedLinks(
            links.filter { $0.provider == .cloudflare }
                + environments.flatMap(\.cloudflareLinks)
        )
    }

    var cloudflareEnvironmentGroups: [RepositoryProjectCloudflareEnvironmentGroup] {
        var groups = environments.compactMap { environment -> RepositoryProjectCloudflareEnvironmentGroup? in
            let cloudflareLinks = environment.cloudflareLinks
            guard !cloudflareLinks.isEmpty else { return nil }
            return RepositoryProjectCloudflareEnvironmentGroup(
                id: environment.id,
                name: environment.name,
                rank: environment.rank,
                links: cloudflareLinks
            )
        }
        let legacyLinks = links.filter { $0.provider == .cloudflare }
        if !legacyLinks.isEmpty {
            groups.append(
                RepositoryProjectCloudflareEnvironmentGroup(
                    id: "unassigned",
                    name: "Unassigned",
                    rank: 50,
                    links: Self.deduplicatedLinks(legacyLinks)
                )
            )
        }
        return groups.sorted { left, right in
            if left.rank == right.rank {
                return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
            }
            return left.rank > right.rank
        }
    }

    func cloudflareStatus(for link: RepositoryProjectLinkModel) -> RepositoryCloudflareProviderStatusModel? {
        cloudflareStatuses?.first { $0.linkIdentityKey == link.identityKey }
    }

    func containsCloudflareLink(_ link: RepositoryProjectLinkModel) -> Bool {
        cloudflareLinks.contains { $0.identityKey == link.identityKey }
    }

    mutating func upsertCloudflareLink(
        _ link: RepositoryProjectLinkModel,
        environmentName: String,
        rank: Int
    ) {
        let environment = RepositoryProjectEnvironmentModel(
            name: environmentName,
            rank: rank,
            links: [link]
        )
        if let index = environments.firstIndex(where: { $0.id == environment.id }) {
            environments[index].name = environment.name
            environments[index].rank = environment.rank
            environments[index].links = Self.deduplicatedLinks(environments[index].links + [link])
        } else {
            environments.append(environment)
        }
        links.removeAll { $0.identityKey == link.identityKey && $0.provider == .cloudflare }
        environments = Self.deduplicatedEnvironments(environments)
    }

    mutating func touch(nowMillis: UInt64 = Self.currentMillis()) {
        updatedAtMillis = nowMillis
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        primaryRepoRoot = Self.normalizedRepoRoot(
            try container.decode(String.self, forKey: .primaryRepoRoot)
        )
        repoRemote = try container.decodeIfPresent(String.self, forKey: .repoRemote)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        links = Self.deduplicatedLinks(
            try container.decodeIfPresent([RepositoryProjectLinkModel].self, forKey: .links) ?? []
        )
        environments = Self.deduplicatedEnvironments(
            try container.decodeIfPresent(
                [RepositoryProjectEnvironmentModel].self,
                forKey: .environments
            ) ?? []
        )
        githubStatus = try container.decodeIfPresent(
            RepositoryGitHubProviderStatusModel.self,
            forKey: .githubStatus
        )
        cloudflareStatuses = try container.decodeIfPresent(
            [RepositoryCloudflareProviderStatusModel].self,
            forKey: .cloudflareStatuses
        )
        createdAtMillis = try container.decodeIfPresent(UInt64.self, forKey: .createdAtMillis)
            ?? Self.currentMillis()
        updatedAtMillis = try container.decodeIfPresent(UInt64.self, forKey: .updatedAtMillis)
            ?? createdAtMillis
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(name, forKey: .name)
        try container.encode(primaryRepoRoot, forKey: .primaryRepoRoot)
        try container.encode(repoRemote, forKey: .repoRemote)
        try container.encode(links, forKey: .links)
        try container.encode(environments, forKey: .environments)
        try container.encodeIfPresent(githubStatus, forKey: .githubStatus)
        try container.encodeIfPresent(cloudflareStatuses, forKey: .cloudflareStatuses)
        try container.encode(createdAtMillis, forKey: .createdAtMillis)
        try container.encode(updatedAtMillis, forKey: .updatedAtMillis)
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case primaryRepoRoot = "primary_repo_root"
        case repoRemote = "repo_remote"
        case links
        case environments
        case githubStatus = "github_status"
        case cloudflareStatuses = "cloudflare_statuses"
        case createdAtMillis = "created_at_millis"
        case updatedAtMillis = "updated_at_millis"
    }

    private static func deduplicatedLinks(
        _ links: [RepositoryProjectLinkModel]
    ) -> [RepositoryProjectLinkModel] {
        var seen = Set<String>()
        var retained: [RepositoryProjectLinkModel] = []
        for link in links where seen.insert(link.identityKey).inserted {
            retained.append(link)
        }
        return retained
    }

    private static func deduplicatedEnvironments(
        _ environments: [RepositoryProjectEnvironmentModel]
    ) -> [RepositoryProjectEnvironmentModel] {
        var retained: [RepositoryProjectEnvironmentModel] = []
        for environment in environments where !environment.id.isEmpty {
            if let index = retained.firstIndex(where: { $0.id == environment.id }) {
                retained[index].rank = max(retained[index].rank, environment.rank)
                retained[index].links = deduplicatedLinks(retained[index].links + environment.links)
            } else {
                retained.append(environment)
            }
        }
        return retained.sorted { left, right in
            if left.rank == right.rank {
                return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
            }
            return left.rank > right.rank
        }
    }

    static func normalizedRepoRoot(_ path: String) -> String {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "" }
        let expanded: String
        if trimmed == "~" {
            expanded = FileManager.default.homeDirectoryForCurrentUser.path
        } else if trimmed.hasPrefix("~/") {
            expanded = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(trimmed.dropFirst(2)))
                .path
        } else {
            expanded = trimmed
        }
        return URL(fileURLWithPath: expanded, isDirectory: true).standardizedFileURL.path
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}

struct RepositoryProjectEnvironmentModel: Codable, Identifiable, Equatable, Sendable {
    let id: String
    var name: String
    var rank: Int
    var links: [RepositoryProjectLinkModel]

    init(
        id: String? = nil,
        name: String,
        rank: Int,
        links: [RepositoryProjectLinkModel] = []
    ) {
        let sanitizedName = Self.sanitizedName(name)
        self.name = sanitizedName.isEmpty ? "Environment" : sanitizedName
        self.id = Self.normalizedID(id ?? self.name)
        self.rank = max(0, rank)
        self.links = Self.deduplicatedLinks(links)
    }

    var cloudflareLinks: [RepositoryProjectLinkModel] {
        links.filter { $0.provider == .cloudflare }
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case rank
        case links
    }

    private static func sanitizedName(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func normalizedID(_ value: String) -> String {
        let sanitized = sanitizedName(value).lowercased()
        let scalars = sanitized.unicodeScalars.map { scalar -> String in
            CharacterSet.alphanumerics.contains(scalar) ? String(scalar) : "-"
        }
        let collapsed = scalars.joined()
            .split(separator: "-")
            .joined(separator: "-")
        return collapsed.isEmpty ? "environment" : collapsed
    }

    private static func deduplicatedLinks(
        _ links: [RepositoryProjectLinkModel]
    ) -> [RepositoryProjectLinkModel] {
        var seen = Set<String>()
        var retained: [RepositoryProjectLinkModel] = []
        for link in links where seen.insert(link.identityKey).inserted {
            retained.append(link)
        }
        return retained
    }
}

struct RepositoryProjectCloudflareEnvironmentGroup: Identifiable, Equatable, Sendable {
    let id: String
    let name: String
    let rank: Int
    let links: [RepositoryProjectLinkModel]
}

struct RepositoryGitHubProviderStatusModel: Codable, Equatable, Sendable {
    static let provider = "github"
    static let defaultTTLMillis: UInt64 = 10 * 60 * 1000
    static let stalePullRequestDays = 14

    let provider: String
    let status: String
    let openPrCount: Int
    let latestPrs: [RepositoryGitHubPullRequestModel]
    let latestWorkflowRuns: [RepositoryGitHubWorkflowRunModel]
    let latestChecks: [RepositoryGitHubCheckModel]
    let latestWorkflowConclusion: String?
    let latestWorkflowStatus: String?
    let latestCheckState: String
    let defaultBranch: String?
    let checkedRef: String?
    let capturedAtMillis: UInt64
    let warnings: [String]

    init(
        provider: String = Self.provider,
        status: String,
        openPrCount: Int,
        latestPrs: [RepositoryGitHubPullRequestModel] = [],
        latestWorkflowRuns: [RepositoryGitHubWorkflowRunModel] = [],
        latestChecks: [RepositoryGitHubCheckModel] = [],
        latestWorkflowConclusion: String? = nil,
        latestWorkflowStatus: String? = nil,
        latestCheckState: String,
        defaultBranch: String? = nil,
        checkedRef: String? = nil,
        capturedAtMillis: UInt64,
        warnings: [String] = []
    ) {
        self.provider = provider
        self.status = status
        self.openPrCount = max(0, openPrCount)
        self.latestPrs = latestPrs
        self.latestWorkflowRuns = latestWorkflowRuns
        self.latestChecks = latestChecks
        self.latestWorkflowConclusion = latestWorkflowConclusion
        self.latestWorkflowStatus = latestWorkflowStatus
        self.latestCheckState = latestCheckState
        self.defaultBranch = defaultBranch
        self.checkedRef = checkedRef
        self.capturedAtMillis = capturedAtMillis
        self.warnings = warnings
    }

    var hasRealIssue: Bool {
        failedLatestCIOnDefaultBranch || staleOpenPullRequestCount() > 0
    }

    var hasAuthCaveat: Bool {
        status == "auth_needed"
    }

    var failedLatestCIOnDefaultBranch: Bool {
        guard latestCITargetsDefaultBranch else { return false }
        return latestCheckState == "failing"
            || Self.failedWorkflowConclusions.contains(latestWorkflowConclusion ?? "")
    }

    var latestCITargetsDefaultBranch: Bool {
        guard let defaultBranch = Self.normalizedBranch(defaultBranch) else {
            return true
        }
        if let workflowBranch = Self.normalizedBranch(latestWorkflowRuns.first?.branch) {
            return workflowBranch == defaultBranch
        }
        if let checkedRef = Self.normalizedBranch(checkedRef) {
            return checkedRef == defaultBranch
        }
        return false
    }

    func staleOpenPullRequestCount(
        now: Date = Date(),
        staleDays: Int = Self.stalePullRequestDays
    ) -> Int {
        guard staleDays > 0 else { return 0 }
        let threshold = TimeInterval(staleDays) * 24 * 60 * 60
        return latestPrs.filter { pullRequest in
            guard let updatedAt = Self.date(fromGitHubTimestamp: pullRequest.updatedAt) else {
                return false
            }
            return now.timeIntervalSince(updatedAt) > threshold
        }.count
    }

    func isFresh(nowMillis: UInt64 = Self.currentMillis(), ttlMillis: UInt64 = Self.defaultTTLMillis) -> Bool {
        guard capturedAtMillis > 0 else { return false }
        return nowMillis >= capturedAtMillis
            ? nowMillis - capturedAtMillis < ttlMillis
            : false
    }

    private enum CodingKeys: String, CodingKey {
        case provider
        case status
        case openPrCount = "open_pr_count"
        case latestPrs = "latest_prs"
        case latestWorkflowRuns = "latest_workflow_runs"
        case latestChecks = "latest_checks"
        case latestWorkflowConclusion = "latest_workflow_conclusion"
        case latestWorkflowStatus = "latest_workflow_status"
        case latestCheckState = "latest_check_state"
        case defaultBranch = "default_branch"
        case checkedRef = "checked_ref"
        case capturedAtMillis = "captured_at_millis"
        case warnings
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }

    private static let failedWorkflowConclusions = Set([
        "failure",
        "timed_out",
        "cancelled",
        "action_required",
    ])

    private static func normalizedBranch(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }

    // Cached: this runs per timestamp during list decodes; ISO8601DateFormatter
    // is thread-safe but mutating a shared one is not, hence two instances.
    nonisolated(unsafe) private static let fractionalTimestampFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    nonisolated(unsafe) private static let plainTimestampFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter
    }()

    private static func date(fromGitHubTimestamp value: String?) -> Date? {
        guard let value, !value.isEmpty else { return nil }
        if let date = fractionalTimestampFormatter.date(from: value) {
            return date
        }
        return plainTimestampFormatter.date(from: value)
    }
}

struct RepositoryGitHubPullRequestModel: Codable, Equatable, Identifiable, Sendable {
    let number: Int
    let title: String
    let url: String?
    let author: String?
    let updatedAt: String?

    var id: Int { number }

    private enum CodingKeys: String, CodingKey {
        case number
        case title
        case url
        case author
        case updatedAt = "updated_at"
    }
}

struct RepositoryGitHubWorkflowRunModel: Codable, Equatable, Identifiable, Sendable {
    let id: UInt64
    let name: String
    let status: String?
    let conclusion: String?
    let url: String?
    let branch: String?
    let event: String?
    let updatedAt: String?

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case status
        case conclusion
        case url
        case branch
        case event
        case updatedAt = "updated_at"
    }
}

struct RepositoryGitHubCheckModel: Codable, Equatable, Identifiable, Sendable {
    let id: UInt64
    let name: String
    let status: String?
    let conclusion: String?
    let url: String?

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case status
        case conclusion
        case url
    }
}

struct RepositoryCloudflareProviderStatusModel: Codable, Equatable, Sendable {
    static let provider = "cloudflare"
    static let defaultTTLMillis: UInt64 = 10 * 60 * 1000

    let provider: String
    let status: String
    let kind: String
    let accountId: String
    let resourceName: String
    let linkIdentityKey: String
    let deploymentId: String?
    let deploymentStatus: String?
    let deploymentSource: String?
    let branch: String?
    let commit: String?
    let url: String?
    let environment: String?
    let createdAt: String?
    let modifiedAt: String?
    let capturedAtMillis: UInt64
    let warnings: [String]

    init(
        provider: String = Self.provider,
        status: String,
        kind: String,
        accountId: String,
        resourceName: String,
        linkIdentityKey: String,
        deploymentId: String? = nil,
        deploymentStatus: String? = nil,
        deploymentSource: String? = nil,
        branch: String? = nil,
        commit: String? = nil,
        url: String? = nil,
        environment: String? = nil,
        createdAt: String? = nil,
        modifiedAt: String? = nil,
        capturedAtMillis: UInt64,
        warnings: [String] = []
    ) {
        self.provider = provider
        self.status = status
        self.kind = kind
        self.accountId = accountId
        self.resourceName = resourceName
        self.linkIdentityKey = linkIdentityKey
        self.deploymentId = deploymentId
        self.deploymentStatus = deploymentStatus
        self.deploymentSource = deploymentSource
        self.branch = branch
        self.commit = commit
        self.url = url
        self.environment = environment
        self.createdAt = createdAt
        self.modifiedAt = modifiedAt
        self.capturedAtMillis = capturedAtMillis
        self.warnings = warnings
    }

    var hasRealIssue: Bool {
        hasFailedDeployment
    }

    var hasAuthCaveat: Bool {
        status == "auth_needed"
    }

    var hasFailedDeployment: Bool {
        ["failure", "failed", "canceled", "cancelled"].contains(deploymentStatus ?? "")
    }

    func isFresh(nowMillis: UInt64 = Self.currentMillis(), ttlMillis: UInt64 = Self.defaultTTLMillis) -> Bool {
        guard capturedAtMillis > 0 else { return false }
        return nowMillis >= capturedAtMillis
            ? nowMillis - capturedAtMillis < ttlMillis
            : false
    }

    private enum CodingKeys: String, CodingKey {
        case provider
        case status
        case kind
        case accountId = "account_id"
        case resourceName = "resource_name"
        case linkIdentityKey = "link_identity_key"
        case deploymentId = "deployment_id"
        case deploymentStatus = "deployment_status"
        case deploymentSource = "deployment_source"
        case branch
        case commit
        case url
        case environment
        case createdAt = "created_at"
        case modifiedAt = "modified_at"
        case capturedAtMillis = "captured_at_millis"
        case warnings
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}

struct RepositoryProjectLinkModel: Codable, Identifiable, Equatable, Sendable {
    enum Provider: String, Codable, Sendable {
        case github
        case cloudflare
    }

    enum Kind: String, Codable, Sendable {
        case repository
        case pages
        case worker
    }

    let provider: Provider
    let kind: Kind
    let owner: String?
    let repo: String?
    let accountId: String?
    let projectName: String?
    let scriptName: String?
    let deploymentEnvironment: String?
    let branch: String?

    var id: String { identityKey }

    var cloudflareResourceName: String? {
        switch kind {
        case .pages:
            return projectName
        case .worker:
            return scriptName
        default:
            return nil
        }
    }

    var identityKey: String {
        [
            provider.rawValue,
            kind.rawValue,
            owner ?? "",
            repo ?? "",
            accountId ?? "",
            projectName ?? "",
            scriptName ?? "",
            deploymentEnvironment ?? "",
            branch ?? "",
        ]
        .joined(separator: ":")
    }

    static func githubRepository(owner: String, repo: String) -> Self {
        RepositoryProjectLinkModel(
            provider: .github,
            kind: .repository,
            owner: sanitized(owner),
            repo: sanitized(repo),
            accountId: nil,
            projectName: nil,
            scriptName: nil,
            deploymentEnvironment: nil,
            branch: nil
        )
    }

    static func cloudflarePages(
        accountId: String,
        projectName: String,
        deploymentEnvironment: String? = nil,
        branch: String? = nil
    ) -> Self {
        RepositoryProjectLinkModel(
            provider: .cloudflare,
            kind: .pages,
            owner: nil,
            repo: nil,
            accountId: sanitized(accountId),
            projectName: sanitized(projectName),
            scriptName: nil,
            deploymentEnvironment: sanitizedOptional(deploymentEnvironment),
            branch: sanitizedOptional(branch)
        )
    }

    static func cloudflareWorker(
        accountId: String,
        scriptName: String,
        branch: String? = nil
    ) -> Self {
        RepositoryProjectLinkModel(
            provider: .cloudflare,
            kind: .worker,
            owner: nil,
            repo: nil,
            accountId: sanitized(accountId),
            projectName: nil,
            scriptName: sanitized(scriptName),
            deploymentEnvironment: nil,
            branch: sanitizedOptional(branch)
        )
    }

    private enum CodingKeys: String, CodingKey {
        case provider
        case kind
        case owner
        case repo
        case accountId = "account_id"
        case projectName = "project_name"
        case scriptName = "script_name"
        case deploymentEnvironment = "deployment_environment"
        case branch
    }

    private static func sanitized(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func sanitizedOptional(_ value: String?) -> String? {
        let sanitized = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return sanitized.isEmpty ? nil : sanitized
    }
}

enum RepositoryProjectStore {
    private static let schemaVersion: UInt8 = 1
    private static let fileName = "repository-projects-v1.json"

    static func load(fileURL: URL? = nil) -> [RepositoryProjectModel] {
        guard let url = fileURL ?? defaultURL(createDirectory: false),
              let data = try? Data(contentsOf: url)
        else {
            return []
        }
        let decoder = JSONDecoder()
        if let record = try? decoder.decode(RepositoryProjectStoreRecord.self, from: data),
           record.schemaVersion == schemaVersion
        {
            return sortedProjects(record.projects)
        }
        if let projects = try? decoder.decode([RepositoryProjectModel].self, from: data) {
            return sortedProjects(projects)
        }
        return []
    }

    static func save(
        _ projects: [RepositoryProjectModel],
        fileURL: URL? = nil
    ) {
        guard let url = fileURL ?? defaultURL(createDirectory: true) else { return }
        let record = RepositoryProjectStoreRecord(
            schemaVersion: schemaVersion,
            projects: sortedProjects(projects)
        )
        guard let data = try? JSONEncoder().encode(record) else { return }
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try? data.write(to: url, options: [.atomic])
    }

    static func upsert(
        _ project: RepositoryProjectModel,
        into projects: [RepositoryProjectModel],
        nowMillis: UInt64 = currentMillis()
    ) -> [RepositoryProjectModel] {
        var updatedProject = project
        updatedProject.touch(nowMillis: nowMillis)
        var retained = projects.filter { $0.id != project.id }
        retained.append(updatedProject)
        return sortedProjects(retained)
    }

    static func remove(
        id: String,
        from projects: [RepositoryProjectModel]
    ) -> [RepositoryProjectModel] {
        sortedProjects(projects.filter { $0.id != id })
    }

    static func project(
        forRepoRoot repoRoot: String,
        in projects: [RepositoryProjectModel]
    ) -> RepositoryProjectModel? {
        let normalized = RepositoryProjectModel.normalizedRepoRoot(repoRoot)
        return projects.first { $0.primaryRepoRoot == normalized }
    }

    private static func defaultURL(createDirectory: Bool) -> URL? {
        guard let baseURL = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first else {
            return nil
        }
        let directory = baseURL.appendingPathComponent("Aetower", isDirectory: true)
        if createDirectory {
            try? FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
        }
        return directory.appendingPathComponent(fileName, isDirectory: false)
    }

    private static func sortedProjects(
        _ projects: [RepositoryProjectModel]
    ) -> [RepositoryProjectModel] {
        projects.sorted { left, right in
            if left.name.localizedCaseInsensitiveCompare(right.name) == .orderedSame {
                return left.primaryRepoRoot < right.primaryRepoRoot
            }
            return left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
        }
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}

private struct RepositoryProjectStoreRecord: Codable {
    let schemaVersion: UInt8
    let projects: [RepositoryProjectModel]

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case projects
    }
}
