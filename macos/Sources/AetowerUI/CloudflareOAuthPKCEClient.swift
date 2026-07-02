import CryptoKit
import Foundation
import Security

struct CloudflareOAuthPKCE: Equatable, Sendable {
    let state: String
    let codeVerifier: String
    let codeChallenge: String
}

struct CloudflareOAuthAccessToken: Equatable, Sendable {
    let accessToken: String
    let refreshToken: String?
    let expiresIn: Int?
    let scopes: [String]
    let tokenType: String

    func expiresAtMillis(capturedAtMillis: UInt64) -> UInt64? {
        guard let expiresIn else { return nil }
        return capturedAtMillis + UInt64(max(0, expiresIn)) * 1000
    }
}

struct CloudflareOAuthCallback: Equatable, Sendable {
    let code: String?
    let state: String?
    let error: String?
    let errorDescription: String?
}

struct CloudflareOAuthPKCEGenerator {
    static func make() throws -> CloudflareOAuthPKCE {
        let verifier = try randomURLSafeString(byteCount: 32)
        return CloudflareOAuthPKCE(
            state: try randomURLSafeString(byteCount: 24),
            codeVerifier: verifier,
            codeChallenge: codeChallenge(for: verifier)
        )
    }

    static func codeChallenge(for verifier: String) -> String {
        let digest = SHA256.hash(data: Data(verifier.utf8))
        return base64URLEncoded(Data(digest))
    }

    static func randomURLSafeString(byteCount: Int) throws -> String {
        var bytes = [UInt8](repeating: 0, count: byteCount)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        guard status == errSecSuccess else {
            throw CloudflareOAuthClientError.randomGenerationFailed
        }
        return base64URLEncoded(Data(bytes))
    }

    static func base64URLEncoded(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

struct CloudflareOAuthPKCEClient: Sendable {
    typealias Transport = @Sendable (URLRequest) async throws -> (Data, HTTPURLResponse)

    static let live = CloudflareOAuthPKCEClient()

    private let authorizationEndpoint: URL
    private let tokenEndpoint: URL
    private let revokeEndpoint: URL
    private let apiBaseURL: URL
    private let transport: Transport
    private let decoder: JSONDecoder

    init(
        authorizationEndpoint: URL = URL(string: "https://dash.cloudflare.com/oauth2/auth")!,
        tokenEndpoint: URL = URL(string: "https://dash.cloudflare.com/oauth2/token")!,
        revokeEndpoint: URL = URL(string: "https://dash.cloudflare.com/oauth2/revoke")!,
        apiBaseURL: URL = URL(string: "https://api.cloudflare.com/client/v4")!,
        transport: @escaping Transport = Self.urlSessionTransport
    ) {
        self.authorizationEndpoint = authorizationEndpoint
        self.tokenEndpoint = tokenEndpoint
        self.revokeEndpoint = revokeEndpoint
        self.apiBaseURL = apiBaseURL
        self.transport = transport
        self.decoder = JSONDecoder()
    }

    func authorizationURL(
        clientID: String,
        redirectURI: String,
        scopes: [String],
        state: String,
        codeChallenge: String
    ) throws -> URL {
        guard !clientID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw CloudflareOAuthClientError.missingClientID
        }
        guard URL(string: redirectURI)?.scheme?.isEmpty == false else {
            throw CloudflareOAuthClientError.invalidRedirectURI
        }

        var components = URLComponents(
            url: authorizationEndpoint,
            resolvingAgainstBaseURL: false
        )
        components?.queryItems = [
            URLQueryItem(name: "client_id", value: clientID),
            URLQueryItem(name: "redirect_uri", value: redirectURI),
            URLQueryItem(name: "response_type", value: "code"),
            URLQueryItem(name: "scope", value: scopes.joined(separator: " ")),
            URLQueryItem(name: "state", value: state),
            URLQueryItem(name: "code_challenge", value: codeChallenge),
            URLQueryItem(name: "code_challenge_method", value: "S256"),
        ].filter { !($0.value ?? "").isEmpty }

        guard let url = components?.url else {
            throw CloudflareOAuthClientError.invalidAuthorizationURL
        }
        return url
    }

    func exchangeCode(
        clientID: String,
        code: String,
        redirectURI: String,
        codeVerifier: String
    ) async throws -> CloudflareOAuthAccessToken {
        let response = try await postForm(
            CloudflareOAuthAccessTokenResponse.self,
            url: tokenEndpoint,
            parameters: [
                "client_id": clientID,
                "code": code,
                "code_verifier": codeVerifier,
                "grant_type": "authorization_code",
                "redirect_uri": redirectURI,
            ]
        )
        guard let accessToken = response.accessToken, !accessToken.isEmpty else {
            throw CloudflareOAuthClientError.oauthFailed(
                response.errorDescription ?? response.error ?? "Cloudflare OAuth returned no token."
            )
        }
        return CloudflareOAuthAccessToken(
            accessToken: accessToken,
            refreshToken: response.refreshToken,
            expiresIn: response.expiresIn,
            scopes: Self.scopes(from: response.scope),
            tokenType: response.tokenType ?? "bearer"
        )
    }

    func revoke(accessToken: String, clientID: String) async throws {
        let request = formRequest(
            url: revokeEndpoint,
            parameters: [
                "client_id": clientID,
                "token": accessToken,
                "token_type_hint": "access_token",
            ]
        )
        let (_, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            throw CloudflareOAuthClientError.httpStatus(response.statusCode)
        }
    }

    func registerOAuthClient(
        accountID: String,
        apiToken: String,
        redirectURI: String,
        scopes: [String]
    ) async throws -> String {
        let normalizedAccountID = accountID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedAccountID.isEmpty else {
            throw CloudflareOAuthClientError.missingAccountID
        }
        let url = apiBaseURL
            .appending(path: "accounts")
            .appending(path: normalizedAccountID)
            .appending(path: "oauth_clients")
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Aetower", forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(apiToken)", forHTTPHeaderField: "Authorization")
        request.httpBody = try JSONEncoder().encode(
            CloudflareOAuthClientCreateRequest(
                clientName: "Aetower Project Health",
                grantTypes: ["authorization_code", "refresh_token"],
                redirectURIs: [redirectURI],
                responseTypes: ["code"],
                scopes: scopes,
                tokenEndpointAuthMethod: "none",
                visibility: "private"
            )
        )
        let (data, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            throw CloudflareOAuthClientError.httpStatus(response.statusCode)
        }
        let decoded = try decoder.decode(CloudflareOAuthClientCreateResponse.self, from: data)
        guard let clientID = decoded.result?.clientID, !clientID.isEmpty else {
            throw CloudflareOAuthClientError.oauthFailed(
                decoded.errors.first?.message ?? "Cloudflare returned no OAuth client ID."
            )
        }
        return clientID
    }

    func fetchUserIdentity(accessToken: String) async throws -> String? {
        let url = apiBaseURL.appending(path: "user")
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("Aetower", forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        let (data, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            throw CloudflareOAuthClientError.httpStatus(response.statusCode)
        }
        let user = try decoder.decode(CloudflareUserResponse.self, from: data)
        return user.result?.email ?? user.result?.username ?? user.result?.id
    }

    static func parseCallbackURL(_ url: URL) -> CloudflareOAuthCallback {
        let components = URLComponents(url: url, resolvingAgainstBaseURL: false)
        let items = components?.queryItems ?? []
        return CloudflareOAuthCallback(
            code: items.first { $0.name == "code" }?.value,
            state: items.first { $0.name == "state" }?.value,
            error: items.first { $0.name == "error" }?.value,
            errorDescription: items.first { $0.name == "error_description" }?.value
        )
    }

    private func postForm<T: Decodable>(
        _ type: T.Type,
        url: URL,
        parameters: [String: String]
    ) async throws -> T {
        let request = formRequest(url: url, parameters: parameters)
        let (data, response) = try await transport(request)
        guard (200..<300).contains(response.statusCode) else {
            throw CloudflareOAuthClientError.httpStatus(response.statusCode)
        }
        return try decoder.decode(type, from: data)
    }

    private func formRequest(url: URL, parameters: [String: String]) -> URLRequest {
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Aetower", forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(
            "application/x-www-form-urlencoded",
            forHTTPHeaderField: "Content-Type"
        )
        request.httpBody = Self.formBody(parameters)
        return request
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
        let rawScopes = value?.split { $0 == " " || $0 == "," }.map(String.init) ?? []
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
            throw CloudflareOAuthClientError.invalidResponse
        }
        return (data, http)
    }
}

private struct CloudflareOAuthAccessTokenResponse: Decodable {
    let accessToken: String?
    let refreshToken: String?
    let expiresIn: Int?
    let scope: String?
    let tokenType: String?
    let error: String?
    let errorDescription: String?

    private enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
        case expiresIn = "expires_in"
        case scope
        case tokenType = "token_type"
        case error
        case errorDescription = "error_description"
    }
}

private struct CloudflareOAuthClientCreateRequest: Encodable {
    let clientName: String
    let grantTypes: [String]
    let redirectURIs: [String]
    let responseTypes: [String]
    let scopes: [String]
    let tokenEndpointAuthMethod: String
    let visibility: String

    private enum CodingKeys: String, CodingKey {
        case clientName = "client_name"
        case grantTypes = "grant_types"
        case redirectURIs = "redirect_uris"
        case responseTypes = "response_types"
        case scopes
        case tokenEndpointAuthMethod = "token_endpoint_auth_method"
        case visibility
    }
}

private struct CloudflareOAuthClientCreateResponse: Decodable {
    let result: CloudflareOAuthClientCreateResult?
    let errors: [CloudflareOAuthAPIMessage]

    private enum CodingKeys: String, CodingKey {
        case result
        case errors
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        result = try container.decodeIfPresent(CloudflareOAuthClientCreateResult.self, forKey: .result)
        errors = try container.decodeIfPresent([CloudflareOAuthAPIMessage].self, forKey: .errors) ?? []
    }
}

private struct CloudflareOAuthClientCreateResult: Decodable {
    let clientID: String?

    private enum CodingKeys: String, CodingKey {
        case clientID = "client_id"
    }
}

private struct CloudflareOAuthAPIMessage: Decodable {
    let message: String
}

private struct CloudflareUserResponse: Decodable {
    let result: CloudflareUser?
}

private struct CloudflareUser: Decodable {
    let id: String?
    let email: String?
    let username: String?
}

private enum CloudflareOAuthClientError: LocalizedError {
    case invalidResponse
    case invalidAuthorizationURL
    case invalidRedirectURI
    case missingAccountID
    case missingClientID
    case randomGenerationFailed
    case httpStatus(Int)
    case oauthFailed(String)

    var errorDescription: String? {
        switch self {
        case .invalidResponse:
            return "Cloudflare OAuth returned a non-HTTP response."
        case .invalidAuthorizationURL:
            return "Cloudflare OAuth authorization URL is invalid."
        case .invalidRedirectURI:
            return "Cloudflare OAuth redirect URI is invalid."
        case .missingAccountID:
            return "Set a Cloudflare account ID first."
        case .missingClientID:
            return "Set a Cloudflare OAuth client ID first."
        case .randomGenerationFailed:
            return "Could not generate Cloudflare OAuth PKCE verifier."
        case let .httpStatus(status):
            return "Cloudflare OAuth failed with HTTP \(status)."
        case let .oauthFailed(message):
            return message
        }
    }
}
