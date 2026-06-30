import XCTest
@testable import AetowerUI

final class RepositoryScorecardModelsTests: XCTestCase {
    func testRepositoryScorecardReportDecodesSnakeCaseJSON() throws {
        let json = """
        {
          "repo_root": "/tmp/Repo",
          "remote_url": "https://github.com/example/repo.git",
          "provider": "github",
          "owner": "example",
          "repo": "repo",
          "mode": "public_api",
          "requested_mode": "auto",
          "status": "ok",
          "score": 6.4,
          "checks": [
            {
              "name": "Token-Permissions",
              "score": 0,
              "outcome": "failed",
              "reason": "workflow tokens are overly broad",
              "details": ["permissions are not set"],
              "documentation_url": "https://example.com/token",
              "documentation_short": "Token permissions"
            }
          ],
          "failed_checks": [
            {
              "name": "Token-Permissions",
              "score": 0,
              "outcome": "failed",
              "reason": "workflow tokens are overly broad",
              "details": [],
              "documentation_url": null,
              "documentation_short": null
            }
          ],
          "unavailable_checks": [],
          "recommendations": [
            {
              "category": "Token-Permissions",
              "check_name": "Token-Permissions",
              "severity": "high",
              "actionability": "mixed",
              "title": "Restrict automation token permissions",
              "detail": "Set least-privilege permissions."
            }
          ],
          "commit_sha": "abc123",
          "scorecard_version": "v5.0.0",
          "scorecard_commit": "def456",
          "source_timestamp": "2026-06-29",
          "cache_status": "hit",
          "cache_key": "cache-key",
          "cache_hit": true,
          "cached_at_millis": 1782720000000,
          "captured_at_millis": 1782720000100,
          "warnings": []
        }
        """

        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let report = try decoder.decode(RepositoryScorecardReportModel.self, from: Data(json.utf8))

        XCTAssertEqual(report.repoRoot, "/tmp/Repo")
        XCTAssertEqual(report.remoteUrl, "https://github.com/example/repo.git")
        XCTAssertEqual(report.mode, "public_api")
        XCTAssertEqual(report.requestedMode, "auto")
        XCTAssertEqual(report.status, "ok")
        XCTAssertEqual(report.score, 6.4)
        XCTAssertEqual(report.checks.first?.documentationUrl, "https://example.com/token")
        XCTAssertEqual(report.failedChecks.first?.outcome, "failed")
        XCTAssertEqual(report.recommendations.first?.category, "Token-Permissions")
        XCTAssertEqual(report.cacheStatus, "hit")
        XCTAssertTrue(report.cacheHit)
    }

    func testScorecardChecksMapToAethymeRemediationCategories() {
        let expectations: [(String, RepositoryScorecardRemediationCategory)] = [
            ("Security-Policy", .securityPolicy),
            ("Token_Permissions", .tokenPermissions),
            ("Branch Protection", .branchProtection),
            ("Code-Review", .codeReview),
            ("Pinned-Dependencies", .pinnedDependencies),
            ("Dangerous-Workflow", .dangerousWorkflow),
            ("Vulnerabilities", .vulnerabilities),
            ("SAST", .sast),
            ("Fuzzing", .fuzzing),
            ("Maintained", .maintained),
        ]

        for (check, category) in expectations {
            XCTAssertEqual(RepositoryScorecardRemediationCategory.category(for: check), category)
        }
    }

    func testScorecardPresentationStateLabelsCoverLifecycleStates() throws {
        let cached = try decodeScorecardReport(status: "ok", mode: "public_api", score: 8.2)
        let authRequired = try decodeScorecardReport(status: "auth_required", mode: "public_api", score: nil)
        let unsupported = try decodeScorecardReport(status: "unsupported", mode: "auto", score: nil)
        let timedOut = try decodeScorecardReport(status: "timeout", mode: "live_cli", score: nil)
        let failed = try decodeScorecardReport(status: "parse_error", mode: "auto", score: nil)

        XCTAssertEqual(
            RepositoryScorecardPresentation.stateLabel(report: nil, isLoading: false, error: nil),
            "Not scanned"
        )
        XCTAssertEqual(
            RepositoryScorecardPresentation.stateLabel(report: nil, isLoading: true, error: nil),
            "Scanning"
        )
        XCTAssertEqual(
            RepositoryScorecardPresentation.stateLabel(report: cached, isLoading: false, error: nil),
            "Cached"
        )
        XCTAssertEqual(
            RepositoryScorecardPresentation.stateLabel(report: authRequired, isLoading: false, error: nil),
            "Needs GitHub auth"
        )
        XCTAssertEqual(
            RepositoryScorecardPresentation.stateLabel(report: unsupported, isLoading: false, error: nil),
            "Unsupported remote"
        )
        XCTAssertEqual(
            RepositoryScorecardPresentation.stateLabel(report: timedOut, isLoading: false, error: nil),
            "Timed out"
        )
        XCTAssertEqual(
            RepositoryScorecardPresentation.stateLabel(report: failed, isLoading: false, error: nil),
            "Failed"
        )
        XCTAssertEqual(
            RepositoryScorecardPresentation.stateLabel(report: cached, isLoading: false, error: "boom"),
            "Failed"
        )
    }

    @MainActor
    func testRepositoryScorecardAppStateTransitionsStoreReportsAndErrors() throws {
        let state = AppState()
        let key = "/tmp/Target"

        let failedRunID = state.beginRepositoryScorecardRun(key: key, runID: "failed-run")
        XCTAssertTrue(state.repositoryScorecardLoadingRoots.contains(key))
        XCTAssertNil(state.repositoryScorecardErrorsByRoot[key])

        state.publishRepositoryScorecardJSONResult(
            key: key,
            runID: failedRunID,
            requestedMode: "auto",
            json: nil,
            errorMessage: "scorecard failed"
        )

        XCTAssertFalse(state.repositoryScorecardLoadingRoots.contains(key))
        XCTAssertEqual(state.repositoryScorecardErrorsByRoot[key], "scorecard failed")
        XCTAssertNil(state.repositoryScorecardReportsByRoot[key])

        let successRunID = state.beginRepositoryScorecardRun(key: key, runID: "success-run")
        XCTAssertTrue(state.repositoryScorecardLoadingRoots.contains(key))
        XCTAssertNil(state.repositoryScorecardErrorsByRoot[key])

        state.publishRepositoryScorecardJSONResult(
            key: key,
            runID: "stale-run",
            requestedMode: "auto",
            json: nil,
            errorMessage: "stale failure"
        )

        XCTAssertTrue(state.repositoryScorecardLoadingRoots.contains(key))
        XCTAssertNil(state.repositoryScorecardErrorsByRoot[key])

        state.publishRepositoryScorecardJSONResult(
            key: key,
            runID: successRunID,
            requestedMode: "auto",
            json: scorecardReportJSON(status: "ok", mode: "public_api", score: "7.6"),
            errorMessage: nil
        )

        XCTAssertFalse(state.repositoryScorecardLoadingRoots.contains(key))
        XCTAssertNil(state.repositoryScorecardErrorsByRoot[key])
        XCTAssertEqual(state.repositoryScorecardReportsByRoot[key]?.status, "ok")
        XCTAssertEqual(state.repositoryScorecardReportsByRoot[key]?.mode, "public_api")
        XCTAssertEqual(state.repositoryScorecardReportsByRoot[key]?.score, 7.6)
    }

    func testScorecardPresentationHelperLabels() throws {
        let api = try decodeScorecardReport(status: "ok", mode: "public_api", score: 8.24)
        let cli = try decodeScorecardReport(status: "ok", mode: "live_cli", score: 5.0)
        let custom = try decodeScorecardReport(status: "ok", mode: "custom_mode", score: nil)
        let sourceUnavailable = try decodeScorecardReport(
            status: "source_unavailable",
            mode: "auto",
            score: nil
        )
        let reportWithFindings = try decodeScorecardReport(
            failedChecks: """
              {
                "name": "Token-Permissions",
                "score": 0,
                "outcome": "failed",
                "reason": "workflow tokens are overly broad",
                "details": [],
                "documentation_url": null,
                "documentation_short": null
              }
            """,
            recommendations: """
              {
                "category": "Token-Permissions",
                "check_name": "Token-Permissions",
                "severity": "high",
                "actionability": "mixed",
                "title": "Restrict automation token permissions",
                "detail": "Set least-privilege permissions."
              }
            """
        )

        XCTAssertEqual(RepositoryScorecardPresentation.scoreLabel(api), "8.2")
        XCTAssertEqual(RepositoryScorecardPresentation.scoreLabel(custom), "N/A")
        XCTAssertEqual(RepositoryScorecardPresentation.sourceLabel(nil), "Not run")
        XCTAssertEqual(RepositoryScorecardPresentation.sourceLabel(api), "API")
        XCTAssertEqual(RepositoryScorecardPresentation.sourceLabel(cli), "CLI")
        XCTAssertEqual(RepositoryScorecardPresentation.sourceLabel(custom), "Custom Mode")
        XCTAssertTrue(
            RepositoryScorecardPresentation
                .statusDetail(report: nil, isLoading: false, error: nil)
                .contains("Run Scorecard on demand")
        )
        XCTAssertTrue(
            RepositoryScorecardPresentation
                .statusDetail(report: reportWithFindings, isLoading: false, error: nil)
                .contains("example/repo")
        )
        XCTAssertTrue(
            RepositoryScorecardPresentation
                .statusDetail(report: sourceUnavailable, isLoading: false, error: nil)
                .contains("No Scorecard source was available")
        )
        XCTAssertEqual(
            RepositoryScorecardPresentation.checkScoreLabel(reportWithFindings.failedChecks[0]),
            "0.0"
        )
    }

    func testScorecardRemediationPromptSeparatesLocalEditsFromRemoteSettings() throws {
        let report = try decodeScorecardReport(
            failedChecks: """
              {
                "name": "Token-Permissions",
                "score": 0,
                "outcome": "failed",
                "reason": "workflow tokens are overly broad",
                "details": ["permissions are not set"],
                "documentation_url": null,
                "documentation_short": null
              },
              {
                "name": "Branch-Protection",
                "score": 3,
                "outcome": "failed",
                "reason": "branch protection is not enforced",
                "details": [],
                "documentation_url": null,
                "documentation_short": null
              }
            """,
            recommendations: """
              {
                "category": "Token-Permissions",
                "check_name": "Token-Permissions",
                "severity": "high",
                "actionability": "mixed",
                "title": "Restrict automation token permissions",
                "detail": "Set least-privilege permissions."
              },
              {
                "category": "Branch-Protection",
                "check_name": "Branch-Protection",
                "severity": "medium",
                "actionability": "remote",
                "title": "Require protected default-branch workflows",
                "detail": "Configure branch protection."
              }
            """
        )

        let prompt = RepositoryScorecardRemediationPrompts.remediationPrompt(
            repository: AgentContractPromptContext(
                name: "Target",
                root: "/tmp/Target",
                branch: "main",
                head: "abc123",
                dirtyDetail: "Clean"
            ),
            remote: "github.com/example/repo",
            report: report
        )

        XCTAssertTrue(prompt.contains("Use failed checks as evidence."))
        XCTAssertTrue(prompt.contains("Do not fabricate GitHub settings."))
        XCTAssertTrue(prompt.contains("Separate local file edits from remote GitHub settings."))
        XCTAssertTrue(prompt.contains("Write PR-ready edits where possible."))
        XCTAssertTrue(prompt.contains("Produce a manual checklist for settings that cannot be changed locally."))
        XCTAssertTrue(prompt.contains("Token-Permissions -> Aethyme remediation category: Token-Permissions"))
        XCTAssertTrue(prompt.contains("Branch-Protection -> Aethyme remediation category: Branch-Protection"))
        XCTAssertTrue(prompt.contains("remote GitHub branch protection or ruleset checklist"))
        XCTAssertTrue(prompt.contains("Local PR-ready edits"))
        XCTAssertTrue(prompt.contains("Remote GitHub settings checklist"))
        XCTAssertTrue(prompt.contains("workflow tokens are overly broad"))
    }

    func testScorecardRemediationPromptUsesFailedChecksForChau7Instructions() throws {
        let report = try decodeScorecardReport(
            failedChecks: """
              {
                "name": "Dangerous-Workflow",
                "score": 0,
                "outcome": "failed",
                "reason": "pull_request_target is used with untrusted checkout",
                "details": ["workflow .github/workflows/ci.yml needs review"],
                "documentation_url": "https://example.com/dangerous-workflow",
                "documentation_short": "Dangerous workflow"
              }
            """,
            recommendations: """
              {
                "category": "Dangerous-Workflow",
                "check_name": "Dangerous-Workflow",
                "severity": "high",
                "actionability": "local",
                "title": "Remove dangerous workflow patterns",
                "detail": "Audit GitHub Actions for unsafe pull_request_target."
              }
            """
        )
        let kit = AgentContractKitPromptContext(
            kitID: "agent-contracts",
            version: "v1",
            relativePath: ".aethyme/agent-contracts/v1",
            absolutePath: "/tmp/Target/.aethyme/agent-contracts/v1",
            fingerprint: "abc123"
        )

        let prompt = RepositoryScorecardRemediationPrompts.remediationPrompt(
            repository: AgentContractPromptContext(
                name: "Target",
                root: "/tmp/Target",
                branch: "main",
                head: "abc123",
                dirtyDetail: "Clean"
            ),
            remote: "github.com/example/repo",
            report: report,
            kit: kit
        )

        XCTAssertTrue(prompt.contains("Dangerous-Workflow -> Aethyme remediation category: Dangerous-Workflow"))
        XCTAssertTrue(prompt.contains("pull_request_target is used with untrusted checkout"))
        XCTAssertTrue(prompt.contains("workflow .github/workflows/ci.yml needs review"))
        XCTAssertTrue(prompt.contains("Remove dangerous workflow patterns"))
        XCTAssertTrue(prompt.contains("local GitHub Actions workflow edits"))
        XCTAssertTrue(prompt.contains("Relative path: .aethyme/agent-contracts/v1"))
        XCTAssertTrue(prompt.contains("Do not claim branch protection"))
        XCTAssertTrue(prompt.contains("manual checklist"))
    }

    func testScorecardWorkflowWriterCreatesLeastPermissionWorkflow() throws {
        let root = try temporaryRepositoryRoot()
        defer { try? FileManager.default.removeItem(at: root) }

        let result = try RepositoryScorecardWorkflowWriter.write(in: root.path)
        let workflowURL = root.appendingPathComponent(RepositoryScorecardWorkflowWriter.relativePath)
        let workflow = try String(contentsOf: workflowURL, encoding: .utf8)

        XCTAssertTrue(result.created)
        XCTAssertEqual(result.relativePath, ".github/workflows/scorecard.yml")
        XCTAssertTrue(FileManager.default.fileExists(atPath: workflowURL.path))
        XCTAssertTrue(workflow.contains("name: Scorecard"))
        XCTAssertTrue(workflow.contains("permissions: {}"))
        XCTAssertTrue(workflow.contains("contents: read"))
        XCTAssertTrue(workflow.contains("actions: read"))
        XCTAssertTrue(workflow.contains("security-events: write"))
        XCTAssertTrue(workflow.contains("id-token: write"))
        XCTAssertTrue(workflow.contains("persist-credentials: false"))
        XCTAssertTrue(workflow.contains("ossf/scorecard-action@v2.4.0"))
        XCTAssertFalse(workflow.contains("contents: write"))
        XCTAssertFalse(workflow.contains("pull-requests: write"))
        XCTAssertFalse(workflow.contains("issues: write"))
    }

    func testScorecardWorkflowWriterDoesNotOverwriteExistingWorkflow() throws {
        let root = try temporaryRepositoryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let workflowURL = root.appendingPathComponent(RepositoryScorecardWorkflowWriter.relativePath)
        try FileManager.default.createDirectory(
            at: workflowURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try "existing workflow\n".write(to: workflowURL, atomically: true, encoding: .utf8)

        let result = try RepositoryScorecardWorkflowWriter.write(in: root.path)
        let workflow = try String(contentsOf: workflowURL, encoding: .utf8)

        XCTAssertFalse(result.created)
        XCTAssertEqual(workflow, "existing workflow\n")
    }

    private func decodeScorecardReport(
        failedChecks: String,
        recommendations: String
    ) throws -> RepositoryScorecardReportModel {
        try decodeScorecardReportJSON(
            status: "ok",
            mode: "public_api",
            score: "4.8",
            failedChecks: failedChecks,
            unavailableChecks: "",
            recommendations: recommendations
        )
    }

    private func decodeScorecardReport(
        status: String,
        mode: String,
        score: Double?
    ) throws -> RepositoryScorecardReportModel {
        try decodeScorecardReportJSON(
            status: status,
            mode: mode,
            score: score.map { String($0) } ?? "null",
            failedChecks: "",
            unavailableChecks: "",
            recommendations: ""
        )
    }

    private func decodeScorecardReportJSON(
        status: String,
        mode: String,
        score: String,
        failedChecks: String,
        unavailableChecks: String,
        recommendations: String
    ) throws -> RepositoryScorecardReportModel {
        let json = scorecardReportJSON(
            status: status,
            mode: mode,
            score: score,
            failedChecks: failedChecks,
            unavailableChecks: unavailableChecks,
            recommendations: recommendations
        )

        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(RepositoryScorecardReportModel.self, from: Data(json.utf8))
    }

    private func scorecardReportJSON(
        status: String,
        mode: String,
        score: String,
        failedChecks: String = "",
        unavailableChecks: String = "",
        recommendations: String = ""
    ) -> String {
        """
        {
          "repo_root": "/tmp/Target",
          "remote_url": "https://github.com/example/repo.git",
          "provider": "github",
          "owner": "example",
          "repo": "repo",
          "mode": "\(mode)",
          "requested_mode": "auto",
          "status": "\(status)",
          "score": \(score),
          "checks": [],
          "failed_checks": [
        \(failedChecks)
          ],
          "unavailable_checks": [
        \(unavailableChecks)
          ],
          "recommendations": [
        \(recommendations)
          ],
          "commit_sha": "abc123",
          "scorecard_version": "v5.0.0",
          "scorecard_commit": "def456",
          "source_timestamp": "2026-06-30",
          "cache_status": "miss",
          "cache_key": "cache-key",
          "cache_hit": false,
          "cached_at_millis": null,
          "captured_at_millis": 1782806400000,
          "warnings": []
        }
        """
    }

    private func temporaryRepositoryRoot() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("aetower-scorecard-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: url.appendingPathComponent(".git", isDirectory: true),
            withIntermediateDirectories: true
        )
        return url
    }
}
