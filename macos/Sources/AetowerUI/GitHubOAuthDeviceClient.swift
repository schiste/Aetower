import Foundation

struct GitHubOAuthDeviceCode: Equatable, Sendable {
    let deviceCode: String
    let userCode: String
    let verificationURI: String
    let expiresIn: Int
    let interval: Int

    var verificationURL: URL? {
        URL(string: verificationURI)
    }
}

struct GitHubOAuthAccessToken: Equatable, Sendable {
    let accessToken: String
    let scopes: [String]
    let tokenType: String
}

enum GitHubOAuthDevicePollResult: Equatable, Sendable {
    case pending
    case slowDown(interval: Int?)
    case authorized(GitHubOAuthAccessToken)
    case denied(String)
    case expired(String)
    case failed(String)
}

struct GitHubOAuthDeviceClient: Sendable {
    typealias Transport = @Sendable (URLRequest) async throws -> (Data, HTTPURLResponse)

    static let live = GitHubOAuthDeviceClient()

    private let githubBaseURL: URL
    private let apiBaseURL: URL
    private let transport: Transport
    private let decoder: JSONDecoder

    init(
        githubBaseURL: URL = URL(string: "https://github.com")!,
        apiBaseURL: URL = URL(string: "https://api.github.com")!,
        transport: @escaping Transport = Self.urlSessionTransport
    ) {
        self.githubBaseURL = githubBaseURL
        self.apiBaseURL = apiBaseURL
        self.transport = transport
        self.decoder = JSONDecoder()
    }

    func requestDeviceCode(
        clientID: String,
        scopes: [String]
    ) async throws -> GitHubOAuthDeviceCode {
        let response = try await postForm(
            GitHubOAuthDeviceCodeResponse.self,
            url: githubBaseURL.appending(path: "login/device/code"),
            parameters: [
                "client_id": clientID,
                "scope": scopes.joined(separator: " "),
            ]
        )
        return GitHubOAuthDeviceCode(
            deviceCode: response.deviceCode,
            userCode: response.userCode,
            verificationURI: response.verificationURI,
            expiresIn: response.expiresIn,
            interval: max(1, response.interval)
        )
    }

    func pollAccessToken(
        clientID: String,
        deviceCode: String
    ) async throws -> GitHubOAuthDevicePollResult {
        let response = try await postForm(
            GitHubOAuthAccessTokenResponse.self,
            url: githubBaseURL.appending(path: "login/oauth/access_token"),
            parameters: [
                "client_id": clientID,
                "device_code": deviceCode,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            ]
        )
        if let accessToken = response.accessToken {
            return .authorized(
                GitHubOAuthAccessToken(
                    accessToken: accessToken,
                    scopes: Self.scopes(from: response.scope),
                    tokenType: response.tokenType ?? "bearer"
                )
            )
        }
        switch response.error {
        case "authorization_pending":
            return .pending
        case "slow_down":
            return .slowDown(interval: response.interval)
        case "expired_token", "token_expired":
            return .expired(response.errorDescription ?? "GitHub device code expired.")
        case "access_denied":
            return .denied(response.errorDescription ?? "GitHub authorization was denied.")
        case let error?:
            return .failed(response.errorDescription ?? "GitHub OAuth failed: \(error).")
        case nil:
            return .failed("GitHub OAuth returned no token.")
        }
    }

    func fetchUserIdentity(accessToken: String) async throws -> String? {
        let url = apiBaseURL.appending(path: "user")
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("Aetower", forHTTPHeaderField: "User-Agent")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("2022-11-28", forHTTPHeaderField: "X-GitHub-Api-Version")
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let (data, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            throw GitHubOAuthDeviceClientError.httpStatus(response.statusCode)
        }
        let user = try decoder.decode(GitHubOAuthUserResponse.self, from: data)
        return user.login
    }

    private func postForm<T: Decodable>(
        _ type: T.Type,
        url: URL,
        parameters: [String: String]
    ) async throws -> T {
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Aetower", forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(
            "application/x-www-form-urlencoded",
            forHTTPHeaderField: "Content-Type"
        )
        request.httpBody = Self.formBody(parameters)

        let (data, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            throw GitHubOAuthDeviceClientError.httpStatus(response.statusCode)
        }
        return try decoder.decode(type, from: data)
    }

    private static func formBody(_ parameters: [String: String]) -> Data {
        var components = URLComponents()
        components.queryItems = parameters
            .filter { !$0.value.isEmpty }
            .map { URLQueryItem(name: $0.key, value: $0.value) }
            .sorted { $0.name < $1.name }
        return (components.percentEncodedQuery ?? "").data(using: .utf8) ?? Data()
    }

    private static func scopes(from value: String?) -> [String] {
        let rawScopes = value?.split(separator: ",").map(String.init) ?? []
        var seen = Set<String>()
        return rawScopes.compactMap { rawScope in
            let scope = rawScope.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !scope.isEmpty, seen.insert(scope).inserted else { return nil }
            return scope
        }
    }

    private static func urlSessionTransport(
        request: URLRequest
    ) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw GitHubOAuthDeviceClientError.invalidResponse
        }
        return (data, http)
    }
}

private struct GitHubOAuthDeviceCodeResponse: Decodable {
    let deviceCode: String
    let userCode: String
    let verificationURI: String
    let expiresIn: Int
    let interval: Int

    private enum CodingKeys: String, CodingKey {
        case deviceCode = "device_code"
        case userCode = "user_code"
        case verificationURI = "verification_uri"
        case expiresIn = "expires_in"
        case interval
    }
}

private struct GitHubOAuthAccessTokenResponse: Decodable {
    let accessToken: String?
    let tokenType: String?
    let scope: String?
    let error: String?
    let errorDescription: String?
    let interval: Int?

    private enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case tokenType = "token_type"
        case scope
        case error
        case errorDescription = "error_description"
        case interval
    }
}

private struct GitHubOAuthUserResponse: Decodable {
    let login: String?
}

private enum GitHubOAuthDeviceClientError: LocalizedError {
    case invalidResponse
    case httpStatus(Int)

    var errorDescription: String? {
        switch self {
        case .invalidResponse:
            return "GitHub OAuth returned a non-HTTP response."
        case let .httpStatus(status):
            return "GitHub OAuth failed with HTTP \(status)."
        }
    }
}
