import Foundation
import XCTest
@testable import AetowerUI

final class CloudflareOAuthPKCEClientTests: XCTestCase {
    func testPKCECodeChallengeUsesSHA256Base64URL() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        XCTAssertEqual(
            CloudflareOAuthPKCEGenerator.codeChallenge(for: verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        )
    }

    func testBuildsAuthorizationURLWithPKCEAndReadScopes() throws {
        let client = CloudflareOAuthPKCEClient(
            authorizationEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/auth")),
            tokenEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/token")),
            revokeEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/revoke"))
        )

        let url = try client.authorizationURL(
            clientID: "cf-client",
            redirectURI: "aetower://oauth/cloudflare/callback",
            scopes: ["pages.read", "workers.scripts.read"],
            state: "state-123",
            codeChallenge: "challenge-123"
        )

        let components = try XCTUnwrap(URLComponents(url: url, resolvingAgainstBaseURL: false))
        let items = Self.queryItems(components)
        XCTAssertEqual(components.scheme, "https")
        XCTAssertEqual(components.host, "dash.cloudflare.test")
        XCTAssertEqual(components.path, "/oauth2/auth")
        XCTAssertEqual(items["client_id"], "cf-client")
        XCTAssertEqual(items["redirect_uri"], "aetower://oauth/cloudflare/callback")
        XCTAssertEqual(items["response_type"], "code")
        XCTAssertEqual(items["scope"], "pages.read workers.scripts.read")
        XCTAssertEqual(items["state"], "state-123")
        XCTAssertEqual(items["code_challenge"], "challenge-123")
        XCTAssertEqual(items["code_challenge_method"], "S256")
        XCTAssertNil(items["client_secret"])
    }

    func testExchangesAuthorizationCodeWithoutClientSecret() async throws {
        let recorder = CloudflareOAuthRequestRecorder()
        let client = CloudflareOAuthPKCEClient(
            authorizationEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/auth")),
            tokenEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/token")),
            revokeEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/revoke")),
            transport: { request in
                await recorder.record(request)
                return try Self.httpResponse(
                    request: request,
                    body: """
                    {
                      "access_token": "cf-access",
                      "refresh_token": "cf-refresh",
                      "expires_in": 7200,
                      "scope": "pages.read workers.scripts.read pages.read",
                      "token_type": "bearer"
                    }
                    """
                )
            }
        )

        let token = try await client.exchangeCode(
            clientID: "cf-client",
            code: "code-123",
            redirectURI: "aetower://oauth/cloudflare/callback",
            codeVerifier: "verifier-123"
        )

        let recordedRequest = await recorder.lastRequest
        let request = try XCTUnwrap(recordedRequest)
        XCTAssertEqual(request.url?.path, "/oauth2/token")
        XCTAssertEqual(request.httpMethod, "POST")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "application/x-www-form-urlencoded")
        let items = Self.formItems(request)
        XCTAssertEqual(items["client_id"], "cf-client")
        XCTAssertEqual(items["code"], "code-123")
        XCTAssertEqual(items["code_verifier"], "verifier-123")
        XCTAssertEqual(items["grant_type"], "authorization_code")
        XCTAssertEqual(items["redirect_uri"], "aetower://oauth/cloudflare/callback")
        XCTAssertNil(items["client_secret"])
        XCTAssertEqual(token.accessToken, "cf-access")
        XCTAssertEqual(token.refreshToken, "cf-refresh")
        XCTAssertEqual(token.expiresIn, 7200)
        XCTAssertEqual(token.scopes, ["pages.read", "workers.scripts.read"])
    }

    func testRevokesAccessToken() async throws {
        let recorder = CloudflareOAuthRequestRecorder()
        let client = CloudflareOAuthPKCEClient(
            authorizationEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/auth")),
            tokenEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/token")),
            revokeEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/revoke")),
            transport: { request in
                await recorder.record(request)
                return try Self.httpResponse(request: request, body: "{}")
            }
        )

        try await client.revoke(accessToken: "cf-access", clientID: "cf-client")

        let recordedRequest = await recorder.lastRequest
        let request = try XCTUnwrap(recordedRequest)
        XCTAssertEqual(request.url?.path, "/oauth2/revoke")
        let items = Self.formItems(request)
        XCTAssertEqual(items["client_id"], "cf-client")
        XCTAssertEqual(items["token"], "cf-access")
        XCTAssertEqual(items["token_type_hint"], "access_token")
    }

    func testRegistersPublicPKCEOAuthClient() async throws {
        let recorder = CloudflareOAuthRequestRecorder()
        let client = CloudflareOAuthPKCEClient(
            authorizationEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/auth")),
            tokenEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/token")),
            revokeEndpoint: try XCTUnwrap(URL(string: "https://dash.cloudflare.test/oauth2/revoke")),
            apiBaseURL: try XCTUnwrap(URL(string: "https://api.cloudflare.test/client/v4")),
            transport: { request in
                await recorder.record(request)
                return try Self.httpResponse(
                    request: request,
                    body: #"{"success":true,"result":{"client_id":"registered-client"}}"#
                )
            }
        )

        let clientID = try await client.registerOAuthClient(
            accountID: "account-123",
            apiToken: "cf-token",
            redirectURI: "aetower://oauth/cloudflare/callback",
            scopes: ["pages.read", "workers.scripts.read"]
        )

        let recordedRequest = await recorder.lastRequest
        let request = try XCTUnwrap(recordedRequest)
        XCTAssertEqual(request.url?.path, "/client/v4/accounts/account-123/oauth_clients")
        XCTAssertEqual(request.httpMethod, "POST")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer cf-token")
        let data = try XCTUnwrap(request.httpBody)
        let body = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(body?["client_name"] as? String, "Aetower Project Health")
        XCTAssertEqual(body?["token_endpoint_auth_method"] as? String, "none")
        XCTAssertEqual(body?["visibility"] as? String, "private")
        XCTAssertEqual(body?["redirect_uris"] as? [String], ["aetower://oauth/cloudflare/callback"])
        XCTAssertEqual(body?["response_types"] as? [String], ["code"])
        XCTAssertEqual(body?["grant_types"] as? [String], ["authorization_code", "refresh_token"])
        XCTAssertEqual(body?["scopes"] as? [String], ["pages.read", "workers.scripts.read"])
        XCTAssertEqual(clientID, "registered-client")
    }

    func testParsesCallbackURL() throws {
        let callback = CloudflareOAuthPKCEClient.parseCallbackURL(
            try XCTUnwrap(URL(string: "aetower://oauth/cloudflare/callback?code=abc&state=state-123"))
        )

        XCTAssertEqual(callback.code, "abc")
        XCTAssertEqual(callback.state, "state-123")
        XCTAssertNil(callback.error)
    }

    private static func queryItems(_ components: URLComponents) -> [String: String] {
        Dictionary(uniqueKeysWithValues: (components.queryItems ?? []).compactMap { item in
            item.value.map { (item.name, $0) }
        })
    }

    private static func formItems(_ request: URLRequest) -> [String: String] {
        guard let data = request.httpBody,
              let body = String(data: data, encoding: .utf8),
              let components = URLComponents(string: "?\(body)")
        else {
            return [:]
        }
        return queryItems(components)
    }

    private static func httpResponse(
        request: URLRequest,
        statusCode: Int = 200,
        body: String
    ) throws -> (Data, HTTPURLResponse) {
        let data = try XCTUnwrap(body.data(using: .utf8))
        let response = try XCTUnwrap(
            HTTPURLResponse(
                url: try XCTUnwrap(request.url),
                statusCode: statusCode,
                httpVersion: nil,
                headerFields: nil
            )
        )
        return (data, response)
    }
}

private actor CloudflareOAuthRequestRecorder {
    private(set) var requests: [URLRequest] = []

    var lastRequest: URLRequest? {
        requests.last
    }

    func record(_ request: URLRequest) {
        requests.append(request)
    }
}
