import XCTest
@testable import AetowerUI

final class StorageReclaimPolicyTests: XCTestCase {
    private let gigabyte: UInt64 = 1_073_741_824

    func testExistingCleanupPathsDeduplicatesAndRejectsMissingTargets() {
        let existing = StorageReclaimPolicy.existingCleanupPaths(
            ["/repo/target", "/repo/missing", "/repo/target"]
        ) { $0 == "/repo/target" }

        XCTAssertEqual(existing, ["/repo/target"])
    }

    func testMissingCleanupPolicyFailsClosed() throws {
        let item = try storageItem(
            kind: "rust-build",
            path: "/repo/target",
            sizeGB: 6,
            includeCleanupPolicy: false
        )

        XCTAssertFalse(item.cleanupAllowed)
        XCTAssertEqual(item.defaultCleanupAction, "manual_review")
        XCTAssertTrue(item.cleanupBlockers.contains("Missing cleanup policy evidence."))
        XCTAssertFalse(StorageReclaimPolicy.itemIsTrashActionable(item))
    }

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

    func testBulkCleanupPlanKeepsSafeOneClickStrictAndAggressiveReviewBounded() throws {
        let safeBuild = try storageItem(
            kind: "rust-build",
            path: "/repo/target",
            sizeGB: 6,
            cleanupConsequence: "Moves generated output to Finder Trash; emptying Trash is separate."
        )
        let oldDeviceSupport = try storageItem(
            kind: "xcode-device-support",
            path: "/Users/me/Library/Developer/Xcode/iOS DeviceSupport/17.0",
            sizeGB: 11,
            safety: "review",
            cleanupTier: "rebuildable"
        )
        let codexSession = try storageItem(
            kind: "ai-session-data",
            path: "/Users/me/.codex/sessions/old.jsonl",
            sizeGB: 9,
            safety: "review",
            cleanupTier: "rebuildable",
            provider: "codex",
            aiAgentSession: "codex"
        )
        let hardlinkedBuild = try storageItem(
            kind: "rust-build",
            path: "/repo/shared-target",
            sizeGB: 8,
            hasHardlinks: true
        )
        let protectedBuild = try storageItem(
            kind: "test-output",
            path: "/repo/protected-results",
            sizeGB: 7,
            protectedPath: true
        )
        let unknownSafety = try storageItem(
            kind: "test-output",
            path: "/repo/unknown-safety",
            sizeGB: 12,
            safety: "unknown"
        )

        let plan = StorageReclaimPolicy.bulkCleanupPlan(
            items: [
                safeBuild,
                oldDeviceSupport,
                codexSession,
                hardlinkedBuild,
                protectedBuild,
                unknownSafety,
                safeBuild,
            ]
        )

        XCTAssertEqual(plan.safeItems.map(\.path), ["/repo/target"])
        XCTAssertEqual(
            plan.additionalReviewItems.map(\.path),
            ["/Users/me/Library/Developer/Xcode/iOS DeviceSupport/17.0"]
        )
        XCTAssertEqual(plan.items.count, 2)
        XCTAssertEqual(plan.items.map(\.path), [
            "/Users/me/Library/Developer/Xcode/iOS DeviceSupport/17.0",
            "/repo/target",
        ])
        XCTAssertEqual(plan.safeBytes, 6 * gigabyte)
        XCTAssertEqual(plan.additionalReviewBytes, 11 * gigabyte)
        XCTAssertEqual(plan.totalBytes, 17 * gigabyte)
        XCTAssertEqual(
            plan.safeItems.first?.cleanupConsequence,
            "Moves generated output to Finder Trash; emptying Trash is separate."
        )
    }

    func testBulkCleanupPlanUsesVerifiedCachedRepositoryArtifactsWithoutAHygieneReport() throws {
        let plan = StorageReclaimPolicy.bulkCleanupPlan(
            items: [],
            repositoryArtifacts: [
                try repositoryArtifact(
                    path: "/repo/target",
                    sizeGB: 6
                ),
                try repositoryArtifact(
                    path: "/repo/checked-in-results",
                    sizeGB: 9,
                    gitIgnored: false,
                    gitTracked: true
                ),
                try repositoryArtifact(
                    path: "/repo/blocked-cache",
                    sizeGB: 5,
                    cleanupBlockers: ["active-writer"]
                ),
            ]
        )

        XCTAssertEqual(plan.safeItems.map(\.path), ["/repo/target"])
        XCTAssertTrue(plan.additionalReviewItems.isEmpty)
        XCTAssertEqual(plan.safeBytes, 6 * gigabyte)
    }

    func testBulkCleanupPlanSurfacesBoundedRepositoryReviewCandidatesOnlyAggressively() throws {
        let missingMarker = try repositoryArtifact(
            path: "/repo/old-target",
            sizeGB: 7,
            confidence: "likely",
            cleanupTier: "review",
            cleanupAllowed: false,
            defaultCleanupAction: "review",
            cleanupBlockers: ["No build-tool marker confirms this artifact type."]
        )
        let recentBuild = try repositoryArtifact(
            path: "/repo/recent-dist",
            sizeGB: 3,
            cleanupTier: "review",
            cleanupAllowed: false,
            defaultCleanupAction: "review",
            cleanupBlockers: ["Modified within the last hour; it may belong to active work."]
        )
        let unknownBlocker = try repositoryArtifact(
            path: "/repo/unsafe-target",
            sizeGB: 11,
            confidence: "likely",
            cleanupTier: "review",
            cleanupAllowed: false,
            defaultCleanupAction: "review",
            cleanupBlockers: ["Active writer could not be ruled out."]
        )

        let plan = StorageReclaimPolicy.bulkCleanupPlan(
            items: [],
            repositoryArtifacts: [missingMarker, recentBuild, unknownBlocker]
        )

        XCTAssertTrue(plan.safeItems.isEmpty)
        XCTAssertEqual(
            plan.additionalReviewItems.map(\.path),
            ["/repo/old-target", "/repo/recent-dist"]
        )
        XCTAssertEqual(plan.additionalReviewBytes, 10 * gigabyte)
        XCTAssertTrue(
            plan.additionalReviewItems[0].cleanupConsequence.contains("No build-tool marker")
        )
    }

    func testBulkCleanupPlanNeverReviewsTrackedOrUnignoredRepositoryArtifacts() throws {
        let tracked = try repositoryArtifact(
            path: "/repo/tracked-target",
            sizeGB: 7,
            gitTracked: true,
            confidence: "likely",
            cleanupTier: "review",
            cleanupAllowed: false,
            defaultCleanupAction: "review",
            cleanupBlockers: ["No build-tool marker confirms this artifact type."]
        )
        let unignored = try repositoryArtifact(
            path: "/repo/unignored-target",
            sizeGB: 8,
            gitIgnored: false,
            confidence: "likely",
            cleanupTier: "review",
            cleanupAllowed: false,
            defaultCleanupAction: "review",
            cleanupBlockers: ["No build-tool marker confirms this artifact type."]
        )

        let plan = StorageReclaimPolicy.bulkCleanupPlan(
            items: [],
            repositoryArtifacts: [tracked, unignored]
        )

        XCTAssertTrue(plan.items.isEmpty)
    }

    private func storageItem(
        kind: String,
        path: String,
        sizeGB: UInt64,
        safety: String = "safe",
        cleanupTier: String = "rebuildable",
        cleanupAllowed: Bool = true,
        defaultCleanupAction: String = "trash",
        cleanupConsequence: String? = nil,
        storageRole: String? = nil,
        provider: String? = nil,
        aiAgentSession: String? = nil,
        hasHardlinks: Bool = false,
        protectedPath: Bool = false,
        includeCleanupPolicy: Bool = true
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
            "identity": [
                "device": 1,
                "inode": 1,
                "sizeBytes": sizeGB * gigabyte,
                "isDirectory": true,
                "modifiedMillis": 1_600_000_000_000 as UInt64,
                "changedMillis": 1_600_000_000_000 as UInt64,
            ],
            "scanGenerationId": 1,
            "displayName": URL(fileURLWithPath: path).lastPathComponent,
            "kind": kind,
            "safety": safety,
            "cleanupTier": cleanupTier,
            "sizeBytes": sizeGB * gigabyte,
            "sizeTruncated": false,
            "hasHardlinks": hasHardlinks,
            "protectedPath": protectedPath,
            "stale": false,
            "reason": "Unit test fixture.",
            "recommendation": "Review this fixture.",
            "commandHint": "",
            "attribution": attribution,
        ]
        if includeCleanupPolicy {
            payload["cleanupAllowed"] = cleanupAllowed
            payload["cleanupBlockers"] = []
            payload["defaultCleanupAction"] = defaultCleanupAction
        }
        if let cleanupConsequence {
            payload["cleanupConsequence"] = cleanupConsequence
        }
        if let storageRole {
            payload["storageRole"] = storageRole
        }

        let data = try JSONSerialization.data(withJSONObject: payload)
        return try JSONDecoder().decode(StorageHygieneItemModel.self, from: data)
    }

    private func repositoryArtifact(
        path: String,
        sizeGB: UInt64,
        gitIgnored: Bool = true,
        gitTracked: Bool = false,
        confidence: String = "confirmed",
        cleanupTier: String = "rebuildable",
        cleanupAllowed: Bool = true,
        defaultCleanupAction: String = "trash",
        cleanupBlockers: [String] = []
    ) throws -> StorageRepositoryArtifactModel {
        let payload: [String: Any] = [
            "id": path,
            "path": path,
            "identity": [
                "device": 1,
                "inode": 1,
                "sizeBytes": sizeGB * gigabyte,
                "isDirectory": true,
                "modifiedMillis": 1_600_000_000_000 as UInt64,
                "changedMillis": 1_600_000_000_000 as UInt64,
            ],
            "scanGenerationId": 1,
            "relativePath": URL(fileURLWithPath: path).lastPathComponent,
            "repositoryRoot": "/repo",
            "repositoryFamilyId": "/repo",
            "repositoryFamilyRoot": "/repo",
            "repositoryFamilyLabel": "repo",
            "worktree": false,
            "kind": "rust-build",
            "label": URL(fileURLWithPath: path).lastPathComponent,
            "physicalBytes": sizeGB * gigabyte,
            "fileCount": 100,
            "newestModifiedMillis": 1_700_000_000_000 as UInt64,
            "lastActivityMillis": 1_700_000_000_000 as UInt64,
            "activityBasis": "modified",
            "inactivityDays": 365,
            "staleness": "stale",
            "stalenessScore": 100,
            "staleCandidate": true,
            "reclaimPriority": 100,
            "evidence": ["ignored generated output"],
            "confidence": confidence,
            "gitIgnored": gitIgnored,
            "gitTracked": gitTracked,
            "cleanupTier": cleanupTier,
            "cleanupAllowed": cleanupAllowed,
            "cleanupBlockers": cleanupBlockers,
            "defaultCleanupAction": defaultCleanupAction,
            "rebuildInstruction": "cargo build",
            "estimatedRebuildCost": "moderate",
        ]
        let data = try JSONSerialization.data(withJSONObject: payload)
        return try JSONDecoder().decode(StorageRepositoryArtifactModel.self, from: data)
    }
}
