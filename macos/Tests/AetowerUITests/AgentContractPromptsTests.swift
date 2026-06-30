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
        XCTAssertTrue(prompt.contains("Do not assume this repository contains Aethyme docs"))
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

        XCTAssertTrue(prompt.contains("Do not spend time searching for Aethyme prompt guides or schemas"))
        XCTAssertTrue(prompt.contains("Use only local repository evidence"))
        XCTAssertFalse(prompt.contains("docs/agent-contract-prompts"))
        XCTAssertFalse(prompt.contains("Validate YAML against .agents/schema-v1"))
    }

    func testLaunchPromptCanPointAtInstalledAethymeKit() {
        let kit = AgentContractKitPromptContext(
            kitID: "agent-contracts",
            version: "v1",
            relativePath: ".aethyme/agent-contracts/v1",
            absolutePath: "/tmp/Target/.aethyme/agent-contracts/v1",
            fingerprint: "abc123"
        )
        let prompt = AgentContractPrompts.generationPrompt(
            repository: AgentContractPromptContext(
                name: "Target",
                root: "/tmp/Target",
                branch: "main",
                head: "abc123",
                dirtyDetail: nil
            ),
            contract: manifestContract,
            issues: [],
            kit: kit
        )

        XCTAssertTrue(prompt.contains("Aethyme local reference kit"))
        XCTAssertTrue(prompt.contains(".aethyme/agent-contracts/v1/templates/manifest.yaml"))
        XCTAssertTrue(prompt.contains(".aethyme/agent-contracts/v1/schemas/manifest.schema.json"))
        XCTAssertTrue(prompt.contains("do not edit or commit `.aethyme/`"))
        XCTAssertTrue(prompt.contains("Write the actual repository contract only to .agents/manifest.yaml"))
    }

    func testAethymeKitInstallerCopiesResourcesAndExcludesLocalFolder() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("AethymeKitTests-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try FileManager.default.createDirectory(
            at: root.appendingPathComponent(".git/info", isDirectory: true),
            withIntermediateDirectories: true
        )

        let install = try AethymeAgentContractKitInstaller.install(
            in: root.path,
            now: Date(timeIntervalSince1970: 1_782_691_200)
        )

        XCTAssertEqual(install.relativePath, ".aethyme/agent-contracts/v1")
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: root.appendingPathComponent(".aethyme/agent-contracts/v1/templates/manifest.yaml").path
        ))
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: root.appendingPathComponent(".aethyme/agent-contracts/v1/schemas/manifest.schema.json").path
        ))
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: root.appendingPathComponent(".aethyme/manifest.json").path
        ))
        let requiredFiles = AethymeAgentContractKitInstaller.requiredRelativePaths(
            forContractID: "manifest",
            contractPath: ".agents/manifest.yaml",
            install: install
        )
        try AethymeAgentContractKitInstaller.waitForInstalledFiles(
            in: root.path,
            relativePaths: requiredFiles
        )
        XCTAssertTrue(requiredFiles.contains(".aethyme/agent-contracts/v1/templates/manifest.yaml"))
        XCTAssertTrue(requiredFiles.contains(".aethyme/agent-contracts/v1/schemas/manifest.schema.json"))

        let exclude = try String(
            contentsOf: root.appendingPathComponent(".git/info/exclude"),
            encoding: .utf8
        )
        XCTAssertTrue(exclude.contains(".aethyme/"))

        let secondInstall = try AethymeAgentContractKitInstaller.install(in: root.path)
        XCTAssertFalse(secondInstall.installed)
        XCTAssertFalse(secondInstall.excludeUpdated)
    }

    func testAethymeKitInstallerFailsClosedWhenAlternateKitConflicts() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("AethymeKitConflictTests-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try FileManager.default.createDirectory(
            at: root.appendingPathComponent(".git/info", isDirectory: true),
            withIntermediateDirectories: true
        )

        let install = try AethymeAgentContractKitInstaller.install(in: root.path)
        try "changed\n".write(
            to: root.appendingPathComponent(".aethyme/agent-contracts/v1/README.md"),
            atomically: true,
            encoding: .utf8
        )
        let alternate = root.appendingPathComponent(
            ".aethyme/agent-contracts/v1-\(install.fingerprint.prefix(12))",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: alternate, withIntermediateDirectories: true)
        try "conflicting\n".write(
            to: alternate.appendingPathComponent("README.md"),
            atomically: true,
            encoding: .utf8
        )

        XCTAssertThrowsError(try AethymeAgentContractKitInstaller.install(in: root.path)) { error in
            XCTAssertEqual(
                error as? AethymeAgentContractKitError,
                .destinationConflict(alternate.path)
            )
        }
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
