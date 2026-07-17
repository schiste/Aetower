@testable import AetowerUI
import XCTest

final class ProcessFormattingTests: XCTestCase {
    func testProcessPIDLabelUsesUngroupedDigits() {
        XCTAssertEqual(processPIDLabel(5_877), "PID 5877")
    }
}
