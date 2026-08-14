import XCTest
@testable import AetowerUI

final class MemoryPerformanceImpactTests: XCTestCase {
    private let gib: UInt64 = 1_073_741_824
    private let mib: UInt64 = 1_048_576

    func testRetainedButIdleMemoryHasLowImpact() {
        let score = score()
        XCTAssertLessThan(score, 25)
    }

    func testLiveSwapAndCompressorChurnRaisesImpact() {
        let score = score(
            swapinBps: 16 * mib,
            swapoutBps: 8 * mib,
            compressionBps: 64 * mib,
            decompressionBps: 128 * mib
        )
        XCTAssertGreaterThanOrEqual(score, 60)
    }

    private func score(
        swapinBps: UInt64 = 0,
        swapoutBps: UInt64 = 0,
        compressionBps: UInt64 = 0,
        decompressionBps: UInt64 = 0
    ) -> Double {
        memoryPerformanceImpactScore(
            memoryUsedBytes: 14 * gib,
            memoryTotalBytes: 16 * gib,
            compressedMemoryBytes: 7 * gib,
            swapUsedBytes: 20 * gib,
            pageinBps: 0,
            pageoutBps: 0,
            swapinBps: swapinBps,
            swapoutBps: swapoutBps,
            compressionBps: compressionBps,
            decompressionBps: decompressionBps
        )
    }
}
