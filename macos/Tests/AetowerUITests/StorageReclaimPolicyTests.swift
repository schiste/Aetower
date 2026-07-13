import XCTest
@testable import AetowerUI

final class StorageReclaimPolicyTests: XCTestCase {
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
        XCTAssertEqual(StorageScanModeSelection.allCases.map(\.label), ["Fast", "Complete", "Forensic"])
        XCTAssertEqual(StorageScanModeSelection.label(for: "deep_native"), "Complete")
        XCTAssertEqual(StorageScanModeSelection.label(for: "unknown_mode"), "unknown_mode")
    }

    func testCompleteScanUsesFullDepthAndResultBudget() {
        XCTAssertEqual(StorageScanModeSelection.fast.defaultMaxDepth, 5)
        XCTAssertEqual(StorageScanModeSelection.complete.defaultMaxDepth, 12)
        XCTAssertEqual(StorageScanModeSelection.complete.resultLimit, 200)
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
}
