import XCTest
@testable import AetowerUI

final class AgentContractPromptsTests: XCTestCase {
    func testManifestGenerationPromptIsSelfContainedForUnpreparedRepos() {
        let prompt = AgentContractPrompts.generationPrompt(
            repository: AgentContractPromptContext(
                name: "Target",
                root: "/tmp/Target",
                branch: "main",
                head: "abc123",
                dirtyDetail: nil
            ),
            contract: manifestContract,
            issues: []
        )

        XCTAssertTrue(prompt.contains("This prompt is self-contained"))
        XCTAssertTrue(prompt.contains("Do not assume this repository contains Aetower docs"))
        XCTAssertTrue(prompt.contains("Embedded contract spec"))
        XCTAssertTrue(prompt.contains("Required top-level keys: schema_version, contracts, integrity"))
        XCTAssertTrue(prompt.contains(".agents/manifest.yaml"))
        XCTAssertFalse(prompt.contains("Prompt guide:"))
        XCTAssertFalse(prompt.contains("Schema: .agents/schema-v1"))
        XCTAssertFalse(prompt.contains("docs/agent-contract-prompts"))
    }

    func testReconcilePromptDoesNotRequireAetowerLocalArtifacts() {
        let prompt = AgentContractPrompts.reconcilePrompt(
            repository: AgentContractPromptContext(
                name: "Target",
                root: "/tmp/Target",
                branch: "main",
                head: "abc123",
                dirtyDetail: nil
            ),
            contract: manifestContract,
            issues: []
        )

        XCTAssertTrue(prompt.contains("Do not spend time searching for Aetower prompt guides or schemas"))
        XCTAssertTrue(prompt.contains("Use only local repository evidence"))
        XCTAssertFalse(prompt.contains("docs/agent-contract-prompts"))
        XCTAssertFalse(prompt.contains("Validate YAML against .agents/schema-v1"))
    }

    private var manifestContract: StorageAgentContractCoverageModel {
        StorageAgentContractCoverageModel(
            id: "manifest",
            label: "Manifest",
            path: ".agents/manifest.yaml",
            kind: "yaml",
            status: "missing",
            severity: "error",
            detail: "Missing",
            weight: 10,
            earnedWeight: 0,
            coveragePercent: 0,
            present: false,
            tracked: false,
            schemaVersion: nil,
            generated: false,
            reviewed: false
        )
    }
}
