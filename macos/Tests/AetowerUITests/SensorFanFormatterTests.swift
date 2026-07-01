import XCTest
@testable import AetowerUI

final class SensorFanFormatterTests: XCTestCase {
    func testRpmLabelRejectsNonFiniteAndImplausibleValues() {
        XCTAssertEqual(SensorFanFormatter.rpmLabel(.infinity), "rpm n/a")
        XCTAssertEqual(SensorFanFormatter.rpmLabel(.nan), "rpm n/a")
        XCTAssertEqual(SensorFanFormatter.rpmLabel(-1), "rpm n/a")
        XCTAssertEqual(SensorFanFormatter.rpmLabel(1_000_000), "rpm n/a")
    }

    func testRpmLabelRoundsPlausibleValues() {
        XCTAssertEqual(SensorFanFormatter.rpmLabel(1_234.4), "1234 rpm")
        XCTAssertEqual(SensorFanFormatter.rpmLabel(1_234.6), "1235 rpm")
    }

    func testProgressNeverPublishesNonFiniteValues() {
        let progress = SensorFanFormatter.progress(
            currentRpm: .infinity,
            minRpm: .nan,
            maxRpm: 1_000_000
        )

        XCTAssertTrue(progress.value.isFinite)
        XCTAssertTrue(progress.total.isFinite)
        XCTAssertGreaterThanOrEqual(progress.value, 0)
        XCTAssertGreaterThan(progress.total, 0)
    }

    func testNormalizedLoadRejectsInvalidRanges() {
        XCTAssertNil(
            SensorFanFormatter.normalizedLoad(
                currentRpm: .infinity,
                minRpm: 1_000,
                maxRpm: 6_000
            )
        )
        XCTAssertNil(
            SensorFanFormatter.normalizedLoad(
                currentRpm: 3_000,
                minRpm: 6_000,
                maxRpm: 1_000
            )
        )
    }
}
