import XCTest
@testable import AetowerUI

final class RepositoryProjectModelsTests: XCTestCase {
    func testRepositoryProjectEncodesSnakeCaseLocalJSON() throws {
        let project = RepositoryProjectModel(
            id: "project-aetower",
            name: " Aetower ",
            primaryRepoRoot: "/Users/example/Repositories/Aetower",
            repoRemote: " git@github.com:owner/aetower.git ",
            links: [
                .githubRepository(owner: " owner ", repo: " aetower "),
                .cloudflarePages(accountId: " account-123 ", projectName: " aetower-dev "),
            ],
            environments: [
                RepositoryProjectEnvironmentModel(
                    name: " Production ",
                    rank: 100,
                    links: [
                        .cloudflarePages(
                            accountId: "account-123",
                            projectName: "aetower",
                            deploymentEnvironment: "production",
                            branch: "main"
                        ),
                    ]
                ),
            ],
            createdAtMillis: 123,
            updatedAtMillis: 456
        )

        let data = try JSONEncoder().encode(project)
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data) as? [String: Any]
        )
        let links = try XCTUnwrap(object["links"] as? [[String: Any]])
        let environments = try XCTUnwrap(object["environments"] as? [[String: Any]])
        let environmentLinks = try XCTUnwrap(environments[0]["links"] as? [[String: Any]])

        XCTAssertEqual(object["id"] as? String, "project-aetower")
        XCTAssertEqual(object["name"] as? String, "Aetower")
        XCTAssertEqual(object["primary_repo_root"] as? String, "/Users/example/Repositories/Aetower")
        XCTAssertEqual(object["repo_remote"] as? String, "git@github.com:owner/aetower.git")
        XCTAssertEqual((object["created_at_millis"] as? NSNumber)?.uint64Value, 123)
        XCTAssertEqual((object["updated_at_millis"] as? NSNumber)?.uint64Value, 456)
        XCTAssertEqual(links[0]["provider"] as? String, "github")
        XCTAssertEqual(links[0]["kind"] as? String, "repository")
        XCTAssertEqual(links[0]["owner"] as? String, "owner")
        XCTAssertEqual(links[0]["repo"] as? String, "aetower")
        XCTAssertEqual(links[1]["provider"] as? String, "cloudflare")
        XCTAssertEqual(links[1]["kind"] as? String, "pages")
        XCTAssertEqual(links[1]["account_id"] as? String, "account-123")
        XCTAssertEqual(links[1]["project_name"] as? String, "aetower-dev")
        XCTAssertEqual(environments[0]["id"] as? String, "production")
        XCTAssertEqual(environments[0]["name"] as? String, "Production")
        XCTAssertEqual(environments[0]["rank"] as? Int, 100)
        XCTAssertEqual(environmentLinks[0]["deployment_environment"] as? String, "production")
        XCTAssertEqual(environmentLinks[0]["branch"] as? String, "main")
    }

    func testRepositoryProjectStorePersistsProjectsLocally() throws {
        let url = temporaryProjectStoreURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }

        let project = RepositoryProjectModel(
            id: "project-a",
            name: "Project A",
            primaryRepoRoot: "/tmp/project-a",
            repoRemote: "https://github.com/example/project-a.git",
            links: [.githubRepository(owner: "example", repo: "project-a")],
            createdAtMillis: 100,
            updatedAtMillis: 100
        )

        RepositoryProjectStore.save([project], fileURL: url)
        let loaded = RepositoryProjectStore.load(fileURL: url)

        XCTAssertEqual(loaded, [project])
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
    }

    func testRepositoryProjectStoreUpsertRemoveAndLookup() throws {
        let first = RepositoryProjectModel(
            id: "first",
            name: "First",
            primaryRepoRoot: "/tmp/first",
            createdAtMillis: 100,
            updatedAtMillis: 100
        )
        let replacement = RepositoryProjectModel(
            id: "first",
            name: "First renamed",
            primaryRepoRoot: "/tmp/first",
            links: [
                .githubRepository(owner: "example", repo: "first"),
                .githubRepository(owner: "example", repo: "first"),
            ],
            createdAtMillis: 100,
            updatedAtMillis: 100
        )

        let upserted = RepositoryProjectStore.upsert(first, into: [], nowMillis: 200)
        let replaced = RepositoryProjectStore.upsert(replacement, into: upserted, nowMillis: 300)

        XCTAssertEqual(replaced.count, 1)
        XCTAssertEqual(replaced[0].name, "First renamed")
        XCTAssertEqual(replaced[0].updatedAtMillis, 300)
        XCTAssertEqual(replaced[0].links.count, 1)
        XCTAssertEqual(
            RepositoryProjectStore.project(forRepoRoot: "/tmp/first", in: replaced)?.id,
            "first"
        )
        XCTAssertTrue(RepositoryProjectStore.remove(id: "first", from: replaced).isEmpty)
    }

    func testRepositoryProjectEnvironmentsAggregateCloudflareLinks() {
        let productionLink = RepositoryProjectLinkModel.cloudflarePages(
            accountId: "account-123",
            projectName: "aetower",
            deploymentEnvironment: "production",
            branch: "main"
        )
        let stagingLink = RepositoryProjectLinkModel.cloudflarePages(
            accountId: "account-123",
            projectName: "aetower",
            deploymentEnvironment: "preview",
            branch: "staging"
        )
        var project = RepositoryProjectModel(
            name: "Aetower",
            primaryRepoRoot: "/tmp/aetower",
            environments: [
                RepositoryProjectEnvironmentModel(
                    name: "Production",
                    rank: 100,
                    links: [productionLink]
                ),
            ]
        )

        project.upsertCloudflareLink(stagingLink, environmentName: "Staging", rank: 60)

        XCTAssertEqual(project.cloudflareLinks.map(\.identityKey).count, 2)
        XCTAssertTrue(project.containsCloudflareLink(productionLink))
        XCTAssertTrue(project.containsCloudflareLink(stagingLink))
        XCTAssertEqual(project.cloudflareEnvironmentGroups.map(\.name), ["Production", "Staging"])
        XCTAssertEqual(project.cloudflareEnvironmentGroups.map(\.rank), [100, 60])
    }

    func testRepositoryProjectDecodesLegacyJSONWithoutEnvironments() throws {
        let data = Data(
            """
            {
              "id": "legacy-project",
              "name": "Legacy",
              "primary_repo_root": "/tmp/legacy",
              "repo_remote": "",
              "links": [],
              "created_at_millis": 100,
              "updated_at_millis": 100
            }
            """.utf8
        )

        let project = try JSONDecoder().decode(RepositoryProjectModel.self, from: data)

        XCTAssertEqual(project.id, "legacy-project")
        XCTAssertTrue(project.environments.isEmpty)
    }

    func testGitHubAuthNeededIsCaveatNotAttentionIssue() {
        let status = RepositoryGitHubProviderStatusModel(
            status: "auth_needed",
            openPrCount: 0,
            latestCheckState: "unavailable",
            capturedAtMillis: 1_000,
            warnings: ["Needs authentication"]
        )

        XCTAssertTrue(status.hasAuthCaveat)
        XCTAssertFalse(status.hasRealIssue)
        XCTAssertFalse(status.failedLatestCIOnDefaultBranch)
        XCTAssertEqual(status.staleOpenPullRequestCount(), 0)
    }

    func testGitHubFailedCIRequiresDefaultBranch() {
        let defaultBranchFailure = RepositoryGitHubProviderStatusModel(
            status: "warning",
            openPrCount: 0,
            latestWorkflowRuns: [
                RepositoryGitHubWorkflowRunModel(
                    id: 1,
                    name: "CI",
                    status: "completed",
                    conclusion: "failure",
                    url: nil,
                    branch: "main",
                    event: "push",
                    updatedAt: nil
                ),
            ],
            latestWorkflowConclusion: "failure",
            latestWorkflowStatus: "completed",
            latestCheckState: "passing",
            defaultBranch: "main",
            checkedRef: "main",
            capturedAtMillis: 1_000
        )
        let featureBranchFailure = RepositoryGitHubProviderStatusModel(
            status: "warning",
            openPrCount: 0,
            latestWorkflowRuns: [
                RepositoryGitHubWorkflowRunModel(
                    id: 2,
                    name: "CI",
                    status: "completed",
                    conclusion: "failure",
                    url: nil,
                    branch: "feature",
                    event: "push",
                    updatedAt: nil
                ),
            ],
            latestWorkflowConclusion: "failure",
            latestWorkflowStatus: "completed",
            latestCheckState: "passing",
            defaultBranch: "main",
            checkedRef: "feature",
            capturedAtMillis: 1_000
        )

        XCTAssertTrue(defaultBranchFailure.failedLatestCIOnDefaultBranch)
        XCTAssertTrue(defaultBranchFailure.hasRealIssue)
        XCTAssertFalse(featureBranchFailure.failedLatestCIOnDefaultBranch)
        XCTAssertFalse(featureBranchFailure.hasRealIssue)
    }

    func testGitHubStalePullRequestsAreAttentionIssue() throws {
        let status = RepositoryGitHubProviderStatusModel(
            status: "ok",
            openPrCount: 2,
            latestPrs: [
                RepositoryGitHubPullRequestModel(
                    number: 1,
                    title: "Old PR",
                    url: nil,
                    author: nil,
                    updatedAt: "2020-06-01T10:00:00Z"
                ),
                RepositoryGitHubPullRequestModel(
                    number: 2,
                    title: "Fresh PR",
                    url: nil,
                    author: nil,
                    updatedAt: "2026-06-29T10:00:00Z"
                ),
            ],
            latestWorkflowConclusion: "success",
            latestWorkflowStatus: "completed",
            latestCheckState: "passing",
            defaultBranch: "main",
            checkedRef: "main",
            capturedAtMillis: 1_000
        )
        let now = try XCTUnwrap(ISO8601DateFormatter().date(from: "2026-07-01T10:00:00Z"))

        XCTAssertEqual(status.staleOpenPullRequestCount(now: now, staleDays: 14), 1)
        XCTAssertTrue(status.hasRealIssue)
    }

    func testCloudflareAuthNeededIsCaveatAndFailedDeploymentIsAttentionIssue() {
        let link = RepositoryProjectLinkModel.cloudflarePages(
            accountId: "account-123",
            projectName: "aetower"
        )
        let authNeeded = RepositoryCloudflareProviderStatusModel(
            status: "auth_needed",
            kind: "pages",
            accountId: "account-123",
            resourceName: "aetower",
            linkIdentityKey: link.identityKey,
            capturedAtMillis: 1_000,
            warnings: ["Needs API token"]
        )
        let failedDeployment = RepositoryCloudflareProviderStatusModel(
            status: "ok",
            kind: "pages",
            accountId: "account-123",
            resourceName: "aetower",
            linkIdentityKey: link.identityKey,
            deploymentStatus: "failure",
            capturedAtMillis: 1_000
        )

        XCTAssertTrue(authNeeded.hasAuthCaveat)
        XCTAssertFalse(authNeeded.hasRealIssue)
        XCTAssertFalse(authNeeded.hasFailedDeployment)
        XCTAssertTrue(failedDeployment.hasRealIssue)
        XCTAssertTrue(failedDeployment.hasFailedDeployment)
    }

    private func temporaryProjectStoreURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
            .appendingPathComponent("repository-projects-v1.json", isDirectory: false)
    }
}
