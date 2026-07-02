import Foundation

struct RepositoryCloudflareStatusRequest: Sendable {
    let link: RepositoryProjectLinkModel
    let token: String?
}

struct RepositoryCloudflareProviderClient: Sendable {
    typealias Transport = @Sendable (URLRequest) async throws -> (Data, HTTPURLResponse)

    static let live = RepositoryCloudflareProviderClient()

    private let baseURL: URL
    private let transport: Transport
    private let decoder: JSONDecoder

    init(
        baseURL: URL = URL(string: "https://api.cloudflare.com/client/v4")!,
        transport: @escaping Transport = Self.urlSessionTransport
    ) {
        self.baseURL = baseURL
        self.transport = transport
        self.decoder = JSONDecoder()
    }

    func fetchStatus(
        _ request: RepositoryCloudflareStatusRequest,
        nowMillis: UInt64 = currentMillis()
    ) async -> RepositoryCloudflareProviderStatusModel {
        let token = request.token?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard token?.isEmpty == false else {
            return authNeededStatus(link: request.link, nowMillis: nowMillis)
        }
        guard let accountID = request.link.accountId,
              let resourceName = request.link.cloudflareResourceName
        else {
            return unavailableStatus(
                link: request.link,
                nowMillis: nowMillis,
                warning: "Cloudflare link is missing account ID or resource name."
            )
        }

        do {
            switch request.link.kind {
            case .pages:
                return try await fetchPagesStatus(
                    link: request.link,
                    accountID: accountID,
                    projectName: resourceName,
                    token: token,
                    nowMillis: nowMillis
                )
            case .worker:
                return try await fetchWorkerStatus(
                    link: request.link,
                    accountID: accountID,
                    scriptName: resourceName,
                    token: token,
                    nowMillis: nowMillis
                )
            default:
                return unavailableStatus(
                    link: request.link,
                    nowMillis: nowMillis,
                    warning: "Cloudflare provider supports Pages and Workers links only."
                )
            }
        } catch let error as RepositoryCloudflareHTTPFailure {
            return failureStatus(error, link: request.link, nowMillis: nowMillis)
        } catch {
            return failedStatus(
                link: request.link,
                nowMillis: nowMillis,
                warning: "Cloudflare refresh failed: \(error.localizedDescription)"
            )
        }
    }

    private func fetchPagesStatus(
        link: RepositoryProjectLinkModel,
        accountID: String,
        projectName: String,
        token: String?,
        nowMillis: UInt64
    ) async throws -> RepositoryCloudflareProviderStatusModel {
        let branch = normalizedOptional(link.branch)
        var queryItems = [
            URLQueryItem(name: "per_page", value: branch == nil ? "1" : "10"),
        ]
        if let deploymentEnvironment = normalizedOptional(link.deploymentEnvironment) {
            queryItems.append(URLQueryItem(name: "env", value: deploymentEnvironment))
        }
        let response = try await get(
            CloudflarePagesDeploymentsResponse.self,
            path: ["accounts", accountID, "pages", "projects", projectName, "deployments"],
            queryItems: queryItems,
            token: token
        )
        guard response.success else {
            return failedStatus(
                link: link,
                nowMillis: nowMillis,
                warning: response.errorMessage ?? "Cloudflare Pages deployment request failed."
            )
        }
        guard !response.result.isEmpty else {
            return unavailableStatus(
                link: link,
                nowMillis: nowMillis,
                warning: "Cloudflare Pages project has no deployments."
            )
        }
        let deployment: CloudflarePagesDeploymentResponse
        if let branch {
            guard let branchDeployment = response.result.first(where: {
                normalizedOptional($0.deploymentTrigger?.metadata?.branch) == branch
            }) else {
                return unavailableStatus(
                    link: link,
                    nowMillis: nowMillis,
                    warning: "Cloudflare Pages has no recent deployment for branch \(branch)."
                )
            }
            deployment = branchDeployment
        } else {
            deployment = response.result[0]
        }
        let deploymentStatus = normalizedOptional(deployment.latestStage?.status)
        return RepositoryCloudflareProviderStatusModel(
            status: providerStatus(deploymentStatus: deploymentStatus, warnings: []),
            kind: link.kind.rawValue,
            accountId: accountID,
            resourceName: projectName,
            linkIdentityKey: link.identityKey,
            deploymentId: deployment.id,
            deploymentStatus: deploymentStatus,
            deploymentSource: deployment.deploymentTrigger?.type,
            branch: deployment.deploymentTrigger?.metadata?.branch,
            commit: deployment.deploymentTrigger?.metadata?.commitHash,
            url: deployment.url ?? deployment.aliases?.first,
            environment: deployment.environment ?? link.deploymentEnvironment,
            createdAt: deployment.createdOn,
            modifiedAt: deployment.modifiedOn,
            capturedAtMillis: nowMillis
        )
    }

    private func fetchWorkerStatus(
        link: RepositoryProjectLinkModel,
        accountID: String,
        scriptName: String,
        token: String?,
        nowMillis: UInt64
    ) async throws -> RepositoryCloudflareProviderStatusModel {
        let response = try await get(
            CloudflareWorkerDeploymentsResponse.self,
            path: ["accounts", accountID, "workers", "scripts", scriptName, "deployments"],
            token: token
        )
        guard response.success else {
            return failedStatus(
                link: link,
                nowMillis: nowMillis,
                warning: response.errorMessage ?? "Cloudflare Worker deployment request failed."
            )
        }
        guard let deployment = response.result.deployments.first else {
            return unavailableStatus(
                link: link,
                nowMillis: nowMillis,
                warning: "Cloudflare Worker has no deployments."
            )
        }
        let deploymentStatus = deployment.success == false ? "failure" : "success"
        return RepositoryCloudflareProviderStatusModel(
            status: providerStatus(deploymentStatus: deploymentStatus, warnings: []),
            kind: link.kind.rawValue,
            accountId: accountID,
            resourceName: scriptName,
            linkIdentityKey: link.identityKey,
            deploymentId: deployment.id,
            deploymentStatus: deploymentStatus,
            deploymentSource: deployment.source,
            branch: link.branch,
            commit: deployment.versions.first?.versionId,
            url: nil,
            environment: link.deploymentEnvironment,
            createdAt: deployment.createdOn,
            modifiedAt: deployment.createdOn,
            capturedAtMillis: nowMillis,
            warnings: deployment.annotations?.workersMessage.map { [$0] } ?? []
        )
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
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let token, !token.isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }

        let (data, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            let message = try? decoder.decode(CloudflareErrorResponse.self, from: data).errorMessage
            throw RepositoryCloudflareHTTPFailure(statusCode: response.statusCode, message: message)
        }
        return try decoder.decode(type, from: data)
    }

    private func url(path: [String], queryItems: [URLQueryItem]) throws -> URL {
        guard var components = URLComponents(url: baseURL, resolvingAgainstBaseURL: false) else {
            throw RepositoryCloudflareProviderError.invalidBaseURL
        }
        let basePath = components.percentEncodedPath
        let suffix = path.map(Self.percentEncodePathSegment).joined(separator: "/")
        components.percentEncodedPath = [basePath, suffix]
            .filter { !$0.isEmpty }
            .joined(separator: "/")
        components.queryItems = queryItems.isEmpty ? nil : queryItems
        guard let url = components.url else {
            throw RepositoryCloudflareProviderError.invalidURL
        }
        return url
    }

    private func authNeededStatus(
        link: RepositoryProjectLinkModel,
        nowMillis: UInt64
    ) -> RepositoryCloudflareProviderStatusModel {
        status(
            link: link,
            state: "auth_needed",
            nowMillis: nowMillis,
            warning: "Cloudflare refresh needs an API token with read access to the linked resource."
        )
    }

    private func unavailableStatus(
        link: RepositoryProjectLinkModel,
        nowMillis: UInt64,
        warning: String
    ) -> RepositoryCloudflareProviderStatusModel {
        status(link: link, state: "unavailable", nowMillis: nowMillis, warning: warning)
    }

    private func failedStatus(
        link: RepositoryProjectLinkModel,
        nowMillis: UInt64,
        warning: String
    ) -> RepositoryCloudflareProviderStatusModel {
        status(link: link, state: "failed", nowMillis: nowMillis, warning: warning)
    }

    private func failureStatus(
        _ error: RepositoryCloudflareHTTPFailure,
        link: RepositoryProjectLinkModel,
        nowMillis: UInt64
    ) -> RepositoryCloudflareProviderStatusModel {
        if error.statusCode == 401 || error.statusCode == 403 {
            return status(
                link: link,
                state: "auth_needed",
                nowMillis: nowMillis,
                warning: "Cloudflare token needs read access\(error.detailSuffix)."
            )
        }
        if error.statusCode == 404 {
            return status(
                link: link,
                state: "unavailable",
                nowMillis: nowMillis,
                warning: "Cloudflare resource was not found\(error.detailSuffix)."
            )
        }
        return status(
            link: link,
            state: "failed",
            nowMillis: nowMillis,
            warning: "Cloudflare refresh failed: HTTP \(error.statusCode)\(error.detailSuffix)."
        )
    }

    private func status(
        link: RepositoryProjectLinkModel,
        state: String,
        nowMillis: UInt64,
        warning: String
    ) -> RepositoryCloudflareProviderStatusModel {
        RepositoryCloudflareProviderStatusModel(
            status: state,
            kind: link.kind.rawValue,
            accountId: link.accountId ?? "",
            resourceName: link.cloudflareResourceName ?? "",
            linkIdentityKey: link.identityKey,
            branch: link.branch,
            environment: link.deploymentEnvironment,
            capturedAtMillis: nowMillis,
            warnings: [warning]
        )
    }

    private func providerStatus(deploymentStatus: String?, warnings: [String]) -> String {
        if ["failure", "failed", "canceled", "cancelled"].contains(deploymentStatus ?? "") {
            return "warning"
        }
        return warnings.isEmpty ? "ok" : "warning"
    }

    private func normalizedOptional(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
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
            throw RepositoryCloudflareProviderError.invalidResponse
        }
        return (data, httpResponse)
    }

    private static func currentMillis() -> UInt64 {
        UInt64(Date().timeIntervalSince1970 * 1000)
    }
}

private struct RepositoryCloudflareHTTPFailure: Error {
    let statusCode: Int
    let message: String?

    var detailSuffix: String {
        guard let message, !message.isEmpty else { return "" }
        return ": \(message)"
    }
}

private enum RepositoryCloudflareProviderError: LocalizedError {
    case invalidBaseURL
    case invalidURL
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .invalidBaseURL:
            return "Cloudflare API base URL is invalid."
        case .invalidURL:
            return "Cloudflare API request URL is invalid."
        case .invalidResponse:
            return "Cloudflare API returned a non-HTTP response."
        }
    }
}

private struct CloudflareErrorResponse: Decodable {
    let errors: [CloudflareMessage]

    var errorMessage: String? {
        errors.first?.message
    }
}

private struct CloudflareMessage: Decodable {
    let message: String?
}

private struct CloudflarePagesDeploymentsResponse: Decodable {
    let result: [CloudflarePagesDeploymentResponse]
    let success: Bool
    let errors: [CloudflareMessage]?

    var errorMessage: String? {
        errors?.first?.message
    }
}

private struct CloudflarePagesDeploymentResponse: Decodable {
    let id: String
    let aliases: [String]?
    let createdOn: String?
    let deploymentTrigger: CloudflarePagesDeploymentTrigger?
    let environment: String?
    let latestStage: CloudflarePagesDeploymentStage?
    let modifiedOn: String?
    let url: String?

    private enum CodingKeys: String, CodingKey {
        case id
        case aliases
        case createdOn = "created_on"
        case deploymentTrigger = "deployment_trigger"
        case environment
        case latestStage = "latest_stage"
        case modifiedOn = "modified_on"
        case url
    }
}

private struct CloudflarePagesDeploymentTrigger: Decodable {
    let metadata: CloudflarePagesDeploymentMetadata?
    let type: String?
}

private struct CloudflarePagesDeploymentMetadata: Decodable {
    let branch: String?
    let commitHash: String?

    private enum CodingKeys: String, CodingKey {
        case branch
        case commitHash = "commit_hash"
    }
}

private struct CloudflarePagesDeploymentStage: Decodable {
    let status: String?
}

private struct CloudflareWorkerDeploymentsResponse: Decodable {
    let result: CloudflareWorkerDeploymentResult
    let success: Bool
    let errors: [CloudflareMessage]?

    var errorMessage: String? {
        errors?.first?.message
    }
}

private struct CloudflareWorkerDeploymentResult: Decodable {
    let deployments: [CloudflareWorkerDeploymentResponse]
}

private struct CloudflareWorkerDeploymentResponse: Decodable {
    let id: String
    let createdOn: String?
    let source: String?
    let versions: [CloudflareWorkerDeploymentVersion]
    let annotations: CloudflareWorkerDeploymentAnnotations?
    let success: Bool?

    private enum CodingKeys: String, CodingKey {
        case id
        case createdOn = "created_on"
        case source
        case versions
        case annotations
        case success
    }
}

private struct CloudflareWorkerDeploymentVersion: Decodable {
    let versionId: String?

    private enum CodingKeys: String, CodingKey {
        case versionId = "version_id"
    }
}

private struct CloudflareWorkerDeploymentAnnotations: Decodable {
    let workersMessage: String?

    private enum CodingKeys: String, CodingKey {
        case workersMessage = "workers/message"
    }
}
