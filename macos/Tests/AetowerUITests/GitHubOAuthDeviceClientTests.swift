import Foundation
import XCTest
@testable import AetowerUI

final class GitHubOAuthDeviceClientTests: XCTestCase {
    func testRequestsDeviceCodeWithClientIDAndScopes() async throws {
        let recorder = OAuthRequestRecorder()
        let client = GitHubOAuthDeviceClient(
            githubBaseURL: try XCTUnwrap(URL(string: "https://github.test")),
            apiBaseURL: try XCTUnwrap(URL(string: "https://api.github.test")),
            transport: { request in
                await recorder.record(request)
                return try Self.httpResponse(
                    request: request,
                    body: """
                    {
                      "device_code": "device-123",
                      "user_code": "ABCD-EFGH",
                      "verification_uri": "https://github.com/login/device",
                      "expires_in": 900,
                      "interval": 5
                    }
                    """
                )
            }
        )

        let code = try await client.requestDeviceCode(
            clientID: "client-123",
            scopes: ["repo", "read:user"]
        )

        let recordedRequest = await recorder.lastRequest
        let capturedRequest = try XCTUnwrap(recordedRequest)
        XCTAssertEqual(capturedRequest.url?.path, "/login/device/code")
        XCTAssertEqual(capturedRequest.httpMethod, "POST")
        XCTAssertEqual(capturedRequest.value(forHTTPHeaderField: "Accept"), "application/json")
        let body = String(data: try XCTUnwrap(capturedRequest.httpBody), encoding: .utf8)
        XCTAssertEqual(body, "client_id=client-123&scope=repo%20read:user")
        XCTAssertEqual(code.deviceCode, "device-123")
        XCTAssertEqual(code.userCode, "ABCD-EFGH")
        XCTAssertEqual(code.interval, 5)
    }

    func testPollsPendingSlowDownAndAuthorizedToken() async throws {
        let responseQueue = OAuthResponseQueue([
            #"{"error":"authorization_pending"}"#,
            #"{"error":"slow_down","interval":10}"#,
            #"{"access_token":"gho_token","token_type":"bearer","scope":"repo,read:user,repo"}"#,
        ])
        let client = GitHubOAuthDeviceClient(
            githubBaseURL: try XCTUnwrap(URL(string: "https://github.test")),
            apiBaseURL: try XCTUnwrap(URL(string: "https://api.github.test")),
            transport: { request in
                try Self.httpResponse(
                    request: request,
                    body: await responseQueue.next()
                )
            }
        )

        let pending = try await client.pollAccessToken(
            clientID: "client-123",
            deviceCode: "device-123"
        )
        let slowDown = try await client.pollAccessToken(
            clientID: "client-123",
            deviceCode: "device-123"
        )
        let authorized = try await client.pollAccessToken(
            clientID: "client-123",
            deviceCode: "device-123"
        )

        XCTAssertEqual(pending, .pending)
        XCTAssertEqual(slowDown, .slowDown(interval: 10))
        XCTAssertEqual(
            authorized,
            .authorized(
                GitHubOAuthAccessToken(
                    accessToken: "gho_token",
                    scopes: ["repo", "read:user"],
                    tokenType: "bearer"
                )
            )
        )
    }

    func testFetchesUserIdentityWithBearerToken() async throws {
        let recorder = OAuthRequestRecorder()
        let client = GitHubOAuthDeviceClient(
            githubBaseURL: try XCTUnwrap(URL(string: "https://github.test")),
            apiBaseURL: try XCTUnwrap(URL(string: "https://api.github.test")),
            transport: { request in
                await recorder.record(request)
                return try Self.httpResponse(
                    request: request,
                    body: #"{"login":"octocat"}"#
                )
            }
        )

        let identity = try await client.fetchUserIdentity(accessToken: "gho_token")

        let recordedRequest = await recorder.lastRequest
        let request = try XCTUnwrap(recordedRequest)
        let authorizationHeader = request.value(forHTTPHeaderField: "Authorization")
        XCTAssertEqual(authorizationHeader, "Bearer gho_token")
        XCTAssertEqual(identity, "octocat")
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

private actor OAuthRequestRecorder {
    private(set) var requests: [URLRequest] = []

    var lastRequest: URLRequest? {
        requests.last
    }

    func record(_ request: URLRequest) {
        requests.append(request)
    }
}

private actor OAuthResponseQueue {
    private var bodies: [String]

    init(_ bodies: [String]) {
        self.bodies = bodies
    }

    func next() -> String {
        bodies.removeFirst()
    }
}
