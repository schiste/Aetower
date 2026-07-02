import Foundation
import XCTest
@testable import AetowerUI

final class RepositoryGitHubProviderTests: XCTestCase {
    func testGitHubProviderStatusEncodesSnakeCaseAndFreshness() throws {
        let status = RepositoryGitHubProviderStatusModel(
            status: "ok",
            openPrCount: 3,
            latestPrs: [
                RepositoryGitHubPullRequestModel(
                    number: 42,
                    title: "Ship provider",
                    url: "https://github.com/owner/repo/pull/42",
                    author: "octocat",
                    updatedAt: "2026-07-01T10:00:00Z"
                ),
            ],
            latestWorkflowConclusion: "success",
            latestWorkflowStatus: "completed",
            latestCheckState: "passing",
            defaultBranch: "main",
            checkedRef: "abc123",
            capturedAtMillis: 1_000,
            warnings: []
        )

        let data = try JSONEncoder().encode(status)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])

        XCTAssertEqual(object["provider"] as? String, "github")
        XCTAssertEqual(object["status"] as? String, "ok")
        XCTAssertEqual(object["open_pr_count"] as? Int, 3)
        XCTAssertEqual(object["latest_workflow_conclusion"] as? String, "success")
        XCTAssertEqual(object["latest_workflow_status"] as? String, "completed")
        XCTAssertEqual(object["latest_check_state"] as? String, "passing")
        XCTAssertEqual(object["default_branch"] as? String, "main")
        XCTAssertEqual(object["checked_ref"] as? String, "abc123")
        XCTAssertTrue(status.isFresh(nowMillis: 2_000, ttlMillis: 10_000))
        XCTAssertFalse(status.isFresh(nowMillis: 20_000, ttlMillis: 10_000))
    }

    func testGitHubProviderClientBuildsStatusFromResponses() async throws {
        let client = RepositoryGitHubProviderClient(
            baseURL: try XCTUnwrap(URL(string: "https://api.github.test")),
            transport: { request in
                try Self.mockResponse(for: request)
            }
        )

        let status = await client.fetchStatus(
            RepositoryGitHubStatusRequest(
                owner: "owner",
                repo: "repo",
                currentBranch: "main",
                currentHead: "abc123",
                token: "ghp_test"
            ),
            nowMillis: 123
        )

        XCTAssertEqual(status.provider, "github")
        XCTAssertEqual(status.status, "ok")
        XCTAssertEqual(status.openPrCount, 3)
        XCTAssertEqual(status.latestPrs.map(\.number), [42, 41])
        XCTAssertEqual(status.latestWorkflowConclusion, "success")
        XCTAssertEqual(status.latestWorkflowStatus, "completed")
        XCTAssertEqual(status.latestCheckState, "passing")
        XCTAssertEqual(status.latestWorkflowRuns.first?.name, "CI")
        XCTAssertEqual(status.latestChecks.first?.name, "unit")
        XCTAssertEqual(status.defaultBranch, "main")
        XCTAssertEqual(status.checkedRef, "abc123")
        XCTAssertEqual(status.capturedAtMillis, 123)
        XCTAssertTrue(status.warnings.isEmpty)
    }

    func testGitHubProviderClientReportsAuthNeededWhenRepositoryIsHidden() async throws {
        let client = RepositoryGitHubProviderClient(
            baseURL: try XCTUnwrap(URL(string: "https://api.github.test")),
            transport: { request in
                try Self.httpResponse(
                    request: request,
                    statusCode: 404,
                    body: #"{"message":"Not Found"}"#
                )
            }
        )

        let status = await client.fetchStatus(
            RepositoryGitHubStatusRequest(
                owner: "owner",
                repo: "private",
                currentBranch: nil,
                currentHead: nil,
                token: nil
            ),
            nowMillis: 456
        )

        XCTAssertEqual(status.status, "auth_needed")
        XCTAssertEqual(status.latestCheckState, "unavailable")
        XCTAssertEqual(status.capturedAtMillis, 456)
        XCTAssertTrue(status.warnings.first?.contains("authentication") == true)
    }

    private static func mockResponse(for request: URLRequest) throws -> (Data, HTTPURLResponse) {
        switch request.url?.path {
        case "/repos/owner/repo":
            return try httpResponse(
                request: request,
                body: #"{"default_branch":"main"}"#
            )
        case "/search/issues":
            return try httpResponse(
                request: request,
                body: #"{"total_count":3}"#
            )
        case "/repos/owner/repo/pulls":
            return try httpResponse(
                request: request,
                body: """
                [
                  {
                    "number": 42,
                    "title": "Ship provider",
                    "html_url": "https://github.com/owner/repo/pull/42",
                    "updated_at": "2026-07-01T10:00:00Z",
                    "user": { "login": "octocat" }
                  },
                  {
                    "number": 41,
                    "title": "Prepare cache",
                    "html_url": "https://github.com/owner/repo/pull/41",
                    "updated_at": "2026-07-01T09:00:00Z",
                    "user": { "login": "hubot" }
                  }
                ]
                """
            )
        case "/repos/owner/repo/actions/runs":
            return try httpResponse(
                request: request,
                body: """
                {
                  "workflow_runs": [
                    {
                      "id": 100,
                      "name": "CI",
                      "status": "completed",
                      "conclusion": "success",
                      "html_url": "https://github.com/owner/repo/actions/runs/100",
                      "head_branch": "main",
                      "event": "push",
                      "updated_at": "2026-07-01T11:00:00Z"
                    }
                  ]
                }
                """
            )
        case "/repos/owner/repo/commits/abc123/check-runs":
            return try httpResponse(
                request: request,
                body: """
                {
                  "check_runs": [
                    {
                      "id": 200,
                      "name": "unit",
                      "status": "completed",
                      "conclusion": "success",
                      "html_url": "https://github.com/owner/repo/runs/200"
                    }
                  ]
                }
                """
            )
        default:
            XCTFail("Unexpected GitHub provider request: \(request.url?.absoluteString ?? "nil")")
            return try httpResponse(
                request: request,
                statusCode: 404,
                body: #"{"message":"Unexpected"}"#
            )
        }
    }

    private static func httpResponse(
        request: URLRequest,
        statusCode: Int = 200,
        body: String
    ) throws -> (Data, HTTPURLResponse) {
        let url = try XCTUnwrap(request.url)
        let response = try XCTUnwrap(
            HTTPURLResponse(
                url: url,
                statusCode: statusCode,
                httpVersion: nil,
                headerFields: nil
            )
        )
        return (Data(body.utf8), response)
    }
}
