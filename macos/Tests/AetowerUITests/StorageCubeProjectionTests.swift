import XCTest
@testable import AetowerUI

final class StorageCubeProjectionTests: XCTestCase {
    func testTinyFilesRoundUpToOneCubeEach() {
        let projection = StorageCubeProjectionBuilder.build(
            inputs: [
                StorageCubeProjectionInput(id: "a", sizeBytes: 1),
                StorageCubeProjectionInput(id: "b", sizeBytes: 32),
            ],
            maxCubes: 100
        )

        XCTAssertEqual(projection.unitBytes, 1_024)
        XCTAssertEqual(projection.totalCubes, 2)
        XCTAssertEqual(projection.bins.map(\.cubeCount), [1, 1])
    }

    func testCubeRangesAreContiguous() {
        let projection = StorageCubeProjectionBuilder.build(
            inputs: [
                StorageCubeProjectionInput(id: "a", sizeBytes: 1_024),
                StorageCubeProjectionInput(id: "b", sizeBytes: 2_048),
                StorageCubeProjectionInput(id: "c", sizeBytes: 3_072),
            ],
            maxCubes: 100
        )

        XCTAssertEqual(projection.bins.map(\.startIndex), [0, 1, 3])
        XCTAssertEqual(projection.bins.map(\.endIndex), [1, 3, 6])
        XCTAssertEqual(projection.totalCubes, 6)
    }

    func testProjectionRaisesUnitToRespectBudget() {
        let projection = StorageCubeProjectionBuilder.build(
            inputs: [
                StorageCubeProjectionInput(id: "a", sizeBytes: 64 * 1_024 * 1_024),
                StorageCubeProjectionInput(id: "b", sizeBytes: 64 * 1_024 * 1_024),
            ],
            maxCubes: 8
        )

        XCTAssertLessThanOrEqual(projection.totalCubes, 8)
        XCTAssertGreaterThan(projection.unitBytes, 1_024)
    }

    func testPreferredUnitIsUsedWhenWithinBudget() {
        let mib = UInt64(1_024 * 1_024)
        let projection = StorageCubeProjectionBuilder.build(
            inputs: [
                StorageCubeProjectionInput(id: "a", sizeBytes: 50 * mib),
                StorageCubeProjectionInput(id: "b", sizeBytes: 10 * mib),
            ],
            maxCubes: 100,
            preferredUnitBytes: 10 * mib
        )

        XCTAssertEqual(projection.unitBytes, 10 * mib)
        XCTAssertEqual(projection.totalCubes, 6)
    }

    func testPreferredUnitScalesUpWhenOverBudget() {
        let mib = UInt64(1_024 * 1_024)
        let projection = StorageCubeProjectionBuilder.build(
            inputs: [
                StorageCubeProjectionInput(id: "a", sizeBytes: 500 * mib),
                StorageCubeProjectionInput(id: "b", sizeBytes: 500 * mib),
            ],
            maxCubes: 8,
            preferredUnitBytes: 10 * mib
        )

        XCTAssertGreaterThan(projection.unitBytes, 10 * mib)
        XCTAssertLessThanOrEqual(projection.totalCubes, 8)
    }
}
