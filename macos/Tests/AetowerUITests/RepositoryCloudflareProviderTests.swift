import Foundation
import XCTest
@testable import AetowerUI

final class RepositoryCloudflareProviderTests: XCTestCase {
    func testCloudflareProviderStatusEncodesSnakeCaseAndFreshness() throws {
        let link = RepositoryProjectLinkModel.cloudflarePages(
            accountId: "account-123",
            projectName: "aetower"
        )
        let status = RepositoryCloudflareProviderStatusModel(
            status: "ok",
            kind: "pages",
            accountId: "account-123",
            resourceName: "aetower",
            linkIdentityKey: link.identityKey,
            deploymentId: "deploy-1",
            deploymentStatus: "success",
            deploymentSource: "github:push",
            branch: "main",
            commit: "abcdef123456",
            url: "https://aetower.pages.dev",
            environment: "production",
            createdAt: "2026-07-01T10:00:00Z",
            modifiedAt: "2026-07-01T10:01:00Z",
            capturedAtMillis: 1_000
        )

        let data = try JSONEncoder().encode(status)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])

        XCTAssertEqual(object["provider"] as? String, "cloudflare")
        XCTAssertEqual(object["status"] as? String, "ok")
        XCTAssertEqual(object["account_id"] as? String, "account-123")
        XCTAssertEqual(object["resource_name"] as? String, "aetower")
        XCTAssertEqual(object["link_identity_key"] as? String, link.identityKey)
        XCTAssertEqual(object["deployment_status"] as? String, "success")
        XCTAssertEqual(object["deployment_source"] as? String, "github:push")
        XCTAssertEqual((object["captured_at_millis"] as? NSNumber)?.uint64Value, 1_000)
        XCTAssertTrue(status.isFresh(nowMillis: 2_000, ttlMillis: 10_000))
        XCTAssertFalse(status.isFresh(nowMillis: 20_000, ttlMillis: 10_000))
    }

    func testCloudflareProviderClientBuildsPagesStatus() async throws {
        let link = RepositoryProjectLinkModel.cloudflarePages(
            accountId: "account-123",
            projectName: "aetower",
            deploymentEnvironment: "production",
            branch: "main"
        )
        let client = RepositoryCloudflareProviderClient(
            baseURL: try XCTUnwrap(URL(string: "https://api.cloudflare.test/client/v4")),
            transport: { request in
                XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer cf_test")
                let components = try XCTUnwrap(
                    URLComponents(url: try XCTUnwrap(request.url), resolvingAgainstBaseURL: false)
                )
                let queryItems = components.queryItems ?? []
                XCTAssertEqual(queryItems.first { $0.name == "env" }?.value, "production")
                XCTAssertEqual(queryItems.first { $0.name == "per_page" }?.value, "10")
                return try Self.mockResponse(for: request)
            }
        )

        let status = await client.fetchStatus(
            RepositoryCloudflareStatusRequest(link: link, token: "cf_test"),
            nowMillis: 123
        )

        XCTAssertEqual(status.provider, "cloudflare")
        XCTAssertEqual(status.status, "ok")
        XCTAssertEqual(status.kind, "pages")
        XCTAssertEqual(status.deploymentId, "pages-deploy-1")
        XCTAssertEqual(status.deploymentStatus, "success")
        XCTAssertEqual(status.deploymentSource, "github:push")
        XCTAssertEqual(status.branch, "main")
        XCTAssertEqual(status.commit, "abcdef123456")
        XCTAssertEqual(status.url, "https://aetower.pages.dev")
        XCTAssertEqual(status.environment, "production")
        XCTAssertEqual(status.capturedAtMillis, 123)
        XCTAssertFalse(status.hasRealIssue)
    }

    func testCloudflareProviderClientBuildsWorkerStatus() async throws {
        let link = RepositoryProjectLinkModel.cloudflareWorker(
            accountId: "account-123",
            scriptName: "aetower-worker"
        )
        let client = RepositoryCloudflareProviderClient(
            baseURL: try XCTUnwrap(URL(string: "https://api.cloudflare.test/client/v4")),
            transport: { request in
                try Self.mockResponse(for: request)
            }
        )

        let status = await client.fetchStatus(
            RepositoryCloudflareStatusRequest(link: link, token: "cf_test"),
            nowMillis: 456
        )

        XCTAssertEqual(status.status, "ok")
        XCTAssertEqual(status.kind, "worker")
        XCTAssertEqual(status.deploymentId, "worker-deploy-1")
        XCTAssertEqual(status.deploymentStatus, "success")
        XCTAssertEqual(status.deploymentSource, "api")
        XCTAssertEqual(status.commit, "worker-version-1")
        XCTAssertEqual(status.warnings, ["Deploy bug fix."])
    }

    func testCloudflareProviderClientReportsAuthNeededWithoutToken() async throws {
        let link = RepositoryProjectLinkModel.cloudflarePages(
            accountId: "account-123",
            projectName: "aetower"
        )
        let client = RepositoryCloudflareProviderClient(
            baseURL: try XCTUnwrap(URL(string: "https://api.cloudflare.test/client/v4")),
            transport: { request in
                XCTFail("Cloudflare request should not run without a token: \(request)")
                return try Self.httpResponse(request: request, body: #"{"success":true,"result":[]}"#)
            }
        )

        let status = await client.fetchStatus(
            RepositoryCloudflareStatusRequest(link: link, token: nil),
            nowMillis: 789
        )

        XCTAssertEqual(status.status, "auth_needed")
        XCTAssertEqual(status.capturedAtMillis, 789)
        XCTAssertTrue(status.warnings.first?.contains("API token") == true)
    }

    private static func mockResponse(for request: URLRequest) throws -> (Data, HTTPURLResponse) {
        switch request.url?.path {
        case "/client/v4/accounts/account-123/pages/projects/aetower/deployments":
            return try httpResponse(
                request: request,
                body: """
                {
                  "success": true,
                  "errors": [],
                  "messages": [],
                  "result": [
                    {
                      "id": "pages-deploy-1",
                      "aliases": ["https://main.aetower.pages.dev"],
                      "created_on": "2026-07-01T10:00:00Z",
                      "deployment_trigger": {
                        "type": "github:push",
                        "metadata": {
                          "branch": "main",
                          "commit_hash": "abcdef123456"
                        }
                      },
                      "environment": "production",
                      "latest_stage": { "status": "success" },
                      "modified_on": "2026-07-01T10:01:00Z",
                      "url": "https://aetower.pages.dev"
                    }
                  ]
                }
                """
            )
        case "/client/v4/accounts/account-123/workers/scripts/aetower-worker/deployments":
            return try httpResponse(
                request: request,
                body: """
                {
                  "success": true,
                  "errors": [],
                  "messages": [],
                  "result": {
                    "deployments": [
                      {
                        "id": "worker-deploy-1",
                        "created_on": "2026-07-01T09:00:00Z",
                        "source": "api",
                        "versions": [
                          {
                            "percentage": 100,
                            "version_id": "worker-version-1"
                          }
                        ],
                        "annotations": {
                          "workers/message": "Deploy bug fix."
                        },
                        "success": true
                      }
                    ]
                  }
                }
                """
            )
        default:
            XCTFail("Unexpected Cloudflare provider request: \(request.url?.absoluteString ?? "nil")")
            return try httpResponse(
                request: request,
                statusCode: 404,
                body: #"{"success":false,"errors":[{"message":"Unexpected"}],"result":[]}"#
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
