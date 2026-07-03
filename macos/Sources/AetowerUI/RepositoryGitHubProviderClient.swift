import Foundation

struct RepositoryGitHubStatusRequest: Sendable {
    let owner: String
    let repo: String
    let currentBranch: String?
    let currentHead: String?
    let token: String?
}

struct RepositoryGitHubProviderClient: Sendable {
    typealias Transport = @Sendable (URLRequest) async throws -> (Data, HTTPURLResponse)

    static let live = RepositoryGitHubProviderClient()

    private let baseURL: URL
    private let transport: Transport
    private let decoder: JSONDecoder

    init(
        baseURL: URL = URL(string: "https://api.github.com")!,
        transport: @escaping Transport = Self.urlSessionTransport
    ) {
        self.baseURL = baseURL
        self.transport = transport
        self.decoder = JSONDecoder()
    }

    /// Re-run a GitHub Actions workflow run. Turns the read-only CI tile into a
    /// control surface: the caller already has the run's database id.
    /// Returns nil on success, or a human-readable error string on failure.
    func rerunWorkflow(
        owner: String,
        repo: String,
        runId: UInt64,
        token: String?
    ) async -> String? {
        do {
            try await post(
                path: ["repos", owner, repo, "actions", "runs", String(runId), "rerun"],
                token: token?.trimmingCharacters(in: .whitespacesAndNewlines)
            )
            return nil
        } catch let failure as RepositoryGitHubHTTPFailure {
            return "Re-run failed: HTTP \(failure.statusCode)\(failure.detailSuffix)."
        } catch {
            return "Re-run failed: \(error.localizedDescription)"
        }
    }

    func fetchStatus(
        _ request: RepositoryGitHubStatusRequest,
        nowMillis: UInt64 = currentMillis()
    ) async -> RepositoryGitHubProviderStatusModel {
        let token = request.token?.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            let repository = try await get(
                GitHubRepositoryResponse.self,
                path: ["repos", request.owner, request.repo],
                token: token
            )
            var warnings: [String] = []
            let defaultBranch = repository.defaultBranch
            let branch = normalizedOptional(request.currentBranch) ?? defaultBranch
            let checkedRef = normalizedOptional(request.currentHead) ?? branch

            let openPrCount = await fetchOpenPrCount(
                owner: request.owner,
                repo: request.repo,
                token: token,
                warnings: &warnings
            )
            let latestPrs = await fetchLatestPullRequests(
                owner: request.owner,
                repo: request.repo,
                token: token,
                warnings: &warnings
            )
            let workflowRuns = await fetchLatestWorkflowRuns(
                owner: request.owner,
                repo: request.repo,
                branch: branch,
                token: token,
                warnings: &warnings
            )
            let checks = await fetchLatestChecks(
                owner: request.owner,
                repo: request.repo,
                ref: checkedRef,
                token: token,
                warnings: &warnings
            )

            let latestRun = workflowRuns.first
            let latestWorkflowConclusion = normalizedOptional(latestRun?.conclusion)
            let latestWorkflowStatus = normalizedOptional(latestRun?.status)
            let latestCheckState = Self.checkState(checks)
            let status = Self.providerStatus(
                workflowConclusion: latestWorkflowConclusion,
                checkState: latestCheckState,
                warnings: warnings
            )

            return RepositoryGitHubProviderStatusModel(
                status: status,
                openPrCount: openPrCount ?? latestPrs.count,
                latestPrs: latestPrs,
                latestWorkflowRuns: workflowRuns,
                latestChecks: checks,
                latestWorkflowConclusion: latestWorkflowConclusion,
                latestWorkflowStatus: latestWorkflowStatus,
                latestCheckState: latestCheckState,
                defaultBranch: defaultBranch,
                checkedRef: checkedRef,
                capturedAtMillis: nowMillis,
                warnings: warnings
            )
        } catch let error as RepositoryGitHubHTTPFailure {
            return Self.failureStatus(
                error,
                token: token,
                nowMillis: nowMillis
            )
        } catch {
            return RepositoryGitHubProviderStatusModel(
                status: "failed",
                openPrCount: 0,
                latestCheckState: "unavailable",
                capturedAtMillis: nowMillis,
                warnings: ["GitHub refresh failed: \(error.localizedDescription)"]
            )
        }
    }

    private func fetchOpenPrCount(
        owner: String,
        repo: String,
        token: String?,
        warnings: inout [String]
    ) async -> Int? {
        do {
            let response = try await get(
                GitHubSearchResponse.self,
                path: ["search", "issues"],
                queryItems: [
                    URLQueryItem(name: "q", value: "repo:\(owner)/\(repo) is:pr is:open"),
                    URLQueryItem(name: "per_page", value: "1"),
                ],
                token: token
            )
            return response.totalCount
        } catch {
            warnings.append(endpointWarning("open PR count", error))
            return nil
        }
    }

    private func fetchLatestPullRequests(
        owner: String,
        repo: String,
        token: String?,
        warnings: inout [String]
    ) async -> [RepositoryGitHubPullRequestModel] {
        do {
            let pulls = try await get(
                [GitHubPullRequestResponse].self,
                path: ["repos", owner, repo, "pulls"],
                queryItems: [
                    URLQueryItem(name: "state", value: "open"),
                    URLQueryItem(name: "sort", value: "updated"),
                    URLQueryItem(name: "direction", value: "desc"),
                    URLQueryItem(name: "per_page", value: "5"),
                ],
                token: token
            )
            return pulls.map {
                RepositoryGitHubPullRequestModel(
                    number: $0.number,
                    title: $0.title,
                    url: $0.htmlUrl,
                    author: $0.user?.login,
                    updatedAt: $0.updatedAt
                )
            }
        } catch {
            warnings.append(endpointWarning("latest PRs", error))
            return []
        }
    }

    private func fetchLatestWorkflowRuns(
        owner: String,
        repo: String,
        branch: String?,
        token: String?,
        warnings: inout [String]
    ) async -> [RepositoryGitHubWorkflowRunModel] {
        do {
            var queryItems = [URLQueryItem(name: "per_page", value: "5")]
            if let branch, !branch.isEmpty {
                queryItems.append(URLQueryItem(name: "branch", value: branch))
            }
            let response = try await get(
                GitHubWorkflowRunsResponse.self,
                path: ["repos", owner, repo, "actions", "runs"],
                queryItems: queryItems,
                token: token
            )
            return response.workflowRuns.map {
                RepositoryGitHubWorkflowRunModel(
                    id: $0.id,
                    name: $0.name ?? $0.displayTitle ?? "Workflow",
                    status: $0.status,
                    conclusion: $0.conclusion,
                    url: $0.htmlUrl,
                    branch: $0.headBranch,
                    event: $0.event,
                    updatedAt: $0.updatedAt
                )
            }
        } catch {
            warnings.append(endpointWarning("workflow runs", error))
            return []
        }
    }

    private func fetchLatestChecks(
        owner: String,
        repo: String,
        ref: String?,
        token: String?,
        warnings: inout [String]
    ) async -> [RepositoryGitHubCheckModel] {
        guard let ref, !ref.isEmpty else {
            warnings.append("GitHub checks unavailable because no branch or commit SHA was detected.")
            return []
        }
        do {
            let response = try await get(
                GitHubCheckRunsResponse.self,
                path: ["repos", owner, repo, "commits", ref, "check-runs"],
                queryItems: [URLQueryItem(name: "per_page", value: "10")],
                token: token
            )
            return response.checkRuns.map {
                RepositoryGitHubCheckModel(
                    id: $0.id,
                    name: $0.name,
                    status: $0.status,
                    conclusion: $0.conclusion,
                    url: $0.htmlUrl
                )
            }
        } catch {
            warnings.append(endpointWarning("checks", error))
            return []
        }
    }

    private func get<T: Decodable>(
        _ type: T.Type,
        path: [String],
        queryItems: [URLQueryItem] = [],
        token: String?
    ) async throws -> T {
        let url = try url(path: path, queryItems: queryItems)
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("Aetower", forHTTPHeaderField: "User-Agent")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("2022-11-28", forHTTPHeaderField: "X-GitHub-Api-Version")
        if let token, !token.isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }

        let (data, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            let message = try? decoder.decode(GitHubErrorMessage.self, from: data).message
            throw RepositoryGitHubHTTPFailure(statusCode: response.statusCode, message: message)
        }
        return try decoder.decode(type, from: data)
    }

    /// POST with no decoded body — for action endpoints (workflow re-run) that
    /// return 201/204. Shares the auth headers and error mapping with `get`.
    private func post(path: [String], token: String?) async throws {
        let url = try url(path: path, queryItems: [])
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Aetower", forHTTPHeaderField: "User-Agent")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("2022-11-28", forHTTPHeaderField: "X-GitHub-Api-Version")
        if let token, !token.isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        let (data, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            let message = try? decoder.decode(GitHubErrorMessage.self, from: data).message
            throw RepositoryGitHubHTTPFailure(statusCode: response.statusCode, message: message)
        }
    }

    private func url(path: [String], queryItems: [URLQueryItem]) throws -> URL {
        guard var components = URLComponents(url: baseURL, resolvingAgainstBaseURL: false) else {
            throw RepositoryGitHubProviderError.invalidBaseURL
        }
        components.percentEncodedPath = "/" + path.map(Self.percentEncodePathSegment).joined(separator: "/")
        components.queryItems = queryItems.isEmpty ? nil : queryItems
        guard let url = components.url else {
            throw RepositoryGitHubProviderError.invalidURL
        }
        return url
    }

    private func endpointWarning(_ endpoint: String, _ error: Error) -> String {
        if let failure = error as? RepositoryGitHubHTTPFailure {
            return "GitHub \(endpoint) unavailable: HTTP \(failure.statusCode)\(failure.detailSuffix)."
        }
        return "GitHub \(endpoint) unavailable: \(error.localizedDescription)."
    }

    private func normalizedOptional(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func providerStatus(
        workflowConclusion: String?,
        checkState: String,
        warnings: [String]
    ) -> String {
        if checkState == "failing" {
            return "warning"
        }
        if let workflowConclusion,
           ["failure", "timed_out", "cancelled", "action_required"].contains(workflowConclusion)
        {
            return "warning"
        }
        return warnings.isEmpty ? "ok" : "warning"
    }

    private static func checkState(_ checks: [RepositoryGitHubCheckModel]) -> String {
        guard !checks.isEmpty else { return "unavailable" }
        if checks.contains(where: { $0.status != "completed" }) {
            return "pending"
        }
        if checks.contains(where: { check in
            ["failure", "timed_out", "cancelled", "action_required"].contains(check.conclusion ?? "")
        }) {
            return "failing"
        }
        return "passing"
    }

    private static func failureStatus(
        _ error: RepositoryGitHubHTTPFailure,
        token: String?,
        nowMillis: UInt64
    ) -> RepositoryGitHubProviderStatusModel {
        let hasToken = token?.isEmpty == false
        if error.statusCode == 401 || error.statusCode == 403 || (error.statusCode == 404 && !hasToken) {
            return RepositoryGitHubProviderStatusModel(
                status: "auth_needed",
                openPrCount: 0,
                latestCheckState: "unavailable",
                capturedAtMillis: nowMillis,
                warnings: [
                    "GitHub repository data needs authentication or a token with enough read access\(error.detailSuffix)."
                ]
            )
        }
        if error.statusCode == 404 {
            return RepositoryGitHubProviderStatusModel(
                status: "unavailable",
                openPrCount: 0,
                latestCheckState: "unavailable",
                capturedAtMillis: nowMillis,
                warnings: ["GitHub repository was not found\(error.detailSuffix)."]
            )
        }
        return RepositoryGitHubProviderStatusModel(
            status: "failed",
            openPrCount: 0,
            latestCheckState: "unavailable",
            capturedAtMillis: nowMillis,
            warnings: ["GitHub refresh failed: HTTP \(error.statusCode)\(error.detailSuffix)."]
        )
    }

    private static func percentEncodePathSegment(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: pathSegmentAllowed) ?? value
    }

    private static var pathSegmentAllowed: CharacterSet {
        var allowed = CharacterSet.urlPathAllowed
        allowed.remove(charactersIn: "/")
        return allowed
    }

    private static func urlSessionTransport(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw RepositoryGitHubProviderError.invalidResponse
        }
        return (data, httpResponse)
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}

private struct RepositoryGitHubHTTPFailure: Error {
    let statusCode: Int
    let message: String?

    var detailSuffix: String {
        guard let message, !message.isEmpty else { return "" }
        return ": \(message)"
    }
}

private enum RepositoryGitHubProviderError: LocalizedError {
    case invalidBaseURL
    case invalidURL
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .invalidBaseURL:
            return "GitHub API base URL is invalid."
        case .invalidURL:
            return "GitHub API request URL is invalid."
        case .invalidResponse:
            return "GitHub API returned a non-HTTP response."
        }
    }
}

private struct GitHubErrorMessage: Decodable {
    let message: String?
}

private struct GitHubRepositoryResponse: Decodable {
    let defaultBranch: String?

    private enum CodingKeys: String, CodingKey {
        case defaultBranch = "default_branch"
    }
}

private struct GitHubSearchResponse: Decodable {
    let totalCount: Int

    private enum CodingKeys: String, CodingKey {
        case totalCount = "total_count"
    }
}

private struct GitHubPullRequestResponse: Decodable {
    let number: Int
    let title: String
    let htmlUrl: String?
    let user: GitHubUserResponse?
    let updatedAt: String?

    private enum CodingKeys: String, CodingKey {
        case number
        case title
        case htmlUrl = "html_url"
        case user
        case updatedAt = "updated_at"
    }
}

private struct GitHubUserResponse: Decodable {
    let login: String?
}

private struct GitHubWorkflowRunsResponse: Decodable {
    let workflowRuns: [GitHubWorkflowRunResponse]

    private enum CodingKeys: String, CodingKey {
        case workflowRuns = "workflow_runs"
    }
}

private struct GitHubWorkflowRunResponse: Decodable {
    let id: UInt64
    let name: String?
    let displayTitle: String?
    let status: String?
    let conclusion: String?
    let htmlUrl: String?
    let headBranch: String?
    let event: String?
    let updatedAt: String?

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case displayTitle = "display_title"
        case status
        case conclusion
        case htmlUrl = "html_url"
        case headBranch = "head_branch"
        case event
        case updatedAt = "updated_at"
    }
}

private struct GitHubCheckRunsResponse: Decodable {
    let checkRuns: [GitHubCheckRunResponse]

    private enum CodingKeys: String, CodingKey {
        case checkRuns = "check_runs"
    }
}

private struct GitHubCheckRunResponse: Decodable {
    let id: UInt64
    let name: String
    let status: String?
    let conclusion: String?
    let htmlUrl: String?

    private enum CodingKeys: String, CodingKey {
        case id
        case name
        case status
        case conclusion
        case htmlUrl = "html_url"
    }
}
