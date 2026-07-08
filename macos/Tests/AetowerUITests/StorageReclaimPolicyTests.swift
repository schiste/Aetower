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

    func testDataCardActionsExposeClearOperatorLabels() {
        XCTAssertEqual(StorageDataCardActionKind.review.title, "Review")
        XCTAssertEqual(StorageDataCardActionKind.clean.title, "Clean")
        XCTAssertEqual(StorageDataCardActionKind.scan.title, "Start scan")
    }

    func testDataCardActionsExposeClearOperatorIcons() {
        XCTAssertEqual(StorageDataCardActionKind.review.systemImage, "magnifyingglass")
        XCTAssertEqual(StorageDataCardActionKind.clean.systemImage, "sparkles")
        XCTAssertEqual(StorageDataCardActionKind.scan.systemImage, "arrow.triangle.2.circlepath")
    }
}
