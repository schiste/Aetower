import XCTest
@testable import AetowerUI

final class StorageReclaimPolicyTests: XCTestCase {
    private let gigabyte: UInt64 = 1_073_741_824

    func testPrimaryActionDecisionStagesWhenContentIsNotTrashSafe() {
        XCTAssertEqual(
            StorageReclaimPolicy.primaryActionDecision(
                hasStageableContent: true,
                canMoveToTrash: false
            ),
            .stageOnly
        )
    }

    func testPrimaryActionDecisionMovesOnlyTrashSafeContent() {
        XCTAssertEqual(
            StorageReclaimPolicy.primaryActionDecision(
                hasStageableContent: true,
                canMoveToTrash: true
            ),
            .moveToTrash
        )
    }

    func testPrimaryActionDecisionCopiesPlanWhenNothingCanBeStaged() {
        XCTAssertEqual(
            StorageReclaimPolicy.primaryActionDecision(
                hasStageableContent: false,
                canMoveToTrash: false
            ),
            .copyPlan
        )
    }

    func testReclaimListModesExposeFilesAndFolders() {
        XCTAssertEqual(StorageReclaimListMode.allCases.map(\.label), ["Files", "Folders"])
    }

    func testScanModesExposeExplicitBackendModes() {
        XCTAssertEqual(StorageScanModeSelection.fast.rawValue, "fast_changed_only")
        XCTAssertEqual(StorageScanModeSelection.complete.rawValue, "deep_native")
        XCTAssertEqual(StorageScanModeSelection.forensic.rawValue, "forensic_verified")
    }

    func testScanModesExposeOperatorLabels() {
        XCTAssertEqual(StorageScanModeSelection.allCases.map(\.label), ["Quick", "Complete", "Forensic"])
        XCTAssertEqual(
            StorageScanModeSelection.allCases.map(\.actionTitle),
            ["Quick scan", "Complete scan", "Forensic scan"]
        )
        XCTAssertEqual(StorageScanModeSelection.label(for: "fast_changed_only"), "Quick")
        XCTAssertEqual(StorageScanModeSelection.label(for: "deep_native"), "Complete")
        XCTAssertEqual(StorageScanModeSelection.label(for: "forensic_partial"), "Forensic partial")
        XCTAssertEqual(
            StorageScanModeSelection.resultLabel(for: "forensic_verified", partial: true),
            "Forensic partial"
        )
        XCTAssertEqual(StorageScanModeSelection.label(for: "unknown_mode"), "unknown_mode")
    }

    func testCompleteScanUsesFullDepthAndResultBudget() {
        XCTAssertEqual(StorageScanModeSelection.fast.defaultMaxDepth, 5)
        XCTAssertEqual(StorageScanModeSelection.complete.defaultMaxDepth, 12)
        XCTAssertEqual(StorageScanModeSelection.complete.resultLimit, 200)
        XCTAssertEqual(StorageScanModeSelection.fast.rowLimitLabel, "120 top rows")
        XCTAssertEqual(StorageScanModeSelection.complete.rowLimitLabel, "all normal rows")
    }

    func testDataCardActionsExposeClearOperatorLabels() {
        XCTAssertEqual(StorageDataCardActionKind.review.title, "Review")
        XCTAssertEqual(StorageDataCardActionKind.clean.title, "Clean")
        XCTAssertEqual(StorageDataCardActionKind.scan.title, "Complete scan")
    }

    func testDataCardActionsExposeClearOperatorIcons() {
        XCTAssertEqual(StorageDataCardActionKind.review.systemImage, "magnifyingglass")
        XCTAssertEqual(StorageDataCardActionKind.clean.systemImage, "sparkles")
        XCTAssertEqual(StorageDataCardActionKind.scan.systemImage, "arrow.triangle.2.circlepath")
    }

    func testPrimaryActionsSurfaceActionFirstFamiliesInFixedOrder() throws {
        let items = try [
            storageItem(
                kind: "xcode-device-support",
                path: "/Users/me/Library/Developer/Xcode/iOS DeviceSupport/18.0",
                sizeGB: 11,
                safety: "review",
                cleanupTier: "rebuildable"
            ),
            storageItem(
                kind: "docker-storage",
                path: "/Users/me/.docker/buildx/cache",
                sizeGB: 4,
                safety: "review",
                cleanupTier: "expensive",
                cleanupAllowed: false,
                defaultCleanupAction: "manual"
            ),
            storageItem(kind: "rust-build", path: "/repo/target", sizeGB: 6),
            storageItem(kind: "swift-build", path: "/repo/.build", sizeGB: 4),
            storageItem(
                kind: "colima-vm",
                path: "/Users/me/.colima/_lima/default/diffdisk",
                sizeGB: 15,
                safety: "review",
                cleanupTier: "expensive",
                cleanupAllowed: false,
                defaultCleanupAction: "manual"
            ),
            storageItem(
                kind: "ai-session-data",
                path: "/Users/me/.codex/sessions/2026/07/session.jsonl",
                sizeGB: 13,
                safety: "review",
                cleanupTier: "risky",
                cleanupAllowed: false,
                defaultCleanupAction: "manual",
                provider: "codex",
                aiAgentSession: "codex"
            ),
        ]

        let actions = StorageReclaimPolicy.primaryActions(items: items)

        XCTAssertEqual(
            actions.map(\.kind),
            [.xcodeDeviceSupport, .dockerBuildCache, .buildOutputs, .colimaVM, .codexSessions]
        )
        XCTAssertEqual(actions.map(\.verb), [.free, .free, .free, .review, .review])

        let buildOutputs = try XCTUnwrap(actions.first { $0.kind == .buildOutputs })
        XCTAssertEqual(buildOutputs.bytes, 10 * gigabyte)
        XCTAssertTrue(buildOutputs.canMoveToTrash)

        let docker = try XCTUnwrap(actions.first { $0.kind == .dockerBuildCache })
        XCTAssertEqual(docker.safety, .toolCleanup)
        XCTAssertFalse(docker.canStageTrash)

        let codex = try XCTUnwrap(actions.first { $0.kind == .codexSessions })
        XCTAssertEqual(codex.bytes, 13 * gigabyte)
        XCTAssertEqual(codex.safety, .review)
        XCTAssertFalse(codex.canStageTrash)
    }

    func testPrimaryActionsDoNotTreatDockerVolumesAsBuildCache() throws {
        let volume = try storageItem(
            kind: "docker-storage",
            path: "/Users/me/.docker/volumes/postgres/_data",
            sizeGB: 40,
            safety: "review",
            cleanupTier: "expensive",
            cleanupAllowed: false,
            defaultCleanupAction: "manual"
        )

        XCTAssertTrue(StorageReclaimPolicy.primaryActions(items: [volume]).isEmpty)
    }

    func testCodexSessionsStayReviewOnlyEvenWhenAPathIsTrashable() throws {
        let session = try storageItem(
            kind: "ai-session-data",
            path: "/Users/me/.codex/sessions/2026/07/session.jsonl",
            sizeGB: 2,
            safety: "review",
            cleanupTier: "rebuildable",
            cleanupAllowed: true,
            defaultCleanupAction: "trash",
            provider: "codex",
            aiAgentSession: "codex"
        )

        let action = try XCTUnwrap(StorageReclaimPolicy.primaryActions(items: [session]).first)

        XCTAssertEqual(action.kind, .codexSessions)
        XCTAssertEqual(action.verb, .review)
        XCTAssertFalse(action.canStageTrash)
        XCTAssertFalse(action.canMoveToTrash)
    }

    private func storageItem(
        kind: String,
        path: String,
        sizeGB: UInt64,
        safety: String = "safe",
        cleanupTier: String = "rebuildable",
        cleanupAllowed: Bool = true,
        defaultCleanupAction: String = "trash",
        storageRole: String? = nil,
        provider: String? = nil,
        aiAgentSession: String? = nil
    ) throws -> StorageHygieneItemModel {
        var attribution: [String: Any] = [
            "confidence": "high",
            "notes": ["unit-test fixture"],
        ]
        if let provider {
            attribution["provider"] = provider
        }
        if let aiAgentSession {
            attribution["aiAgentSession"] = aiAgentSession
        }

        var payload: [String: Any] = [
            "id": path,
            "path": path,
            "displayName": URL(fileURLWithPath: path).lastPathComponent,
            "kind": kind,
            "safety": safety,
            "cleanupTier": cleanupTier,
            "sizeBytes": sizeGB * gigabyte,
            "sizeTruncated": false,
            "stale": false,
            "reason": "Unit test fixture.",
            "recommendation": "Review this fixture.",
            "commandHint": "",
            "cleanupAllowed": cleanupAllowed,
            "cleanupBlockers": [],
            "defaultCleanupAction": defaultCleanupAction,
            "attribution": attribution,
        ]
        if let storageRole {
            payload["storageRole"] = storageRole
        }

        let data = try JSONSerialization.data(withJSONObject: payload)
        return try JSONDecoder().decode(StorageHygieneItemModel.self, from: data)
    }
}
