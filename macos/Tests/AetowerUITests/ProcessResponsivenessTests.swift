import AetowerBridge
import Darwin
import XCTest
@testable import AetowerUI

final class ProcessResponsivenessTests: XCTestCase {
    private func report(_ status: ProcessResponsiveness, start: UInt64 = 20_123_456) -> ProcessResponsivenessSnapshot {
        ProcessResponsivenessSnapshot(isAvailable: true, observations: [
            42: ProcessResponsivenessObservation(
                pid: 42, startTimeMicros: start, executablePath: "/Example.app/Example", status: status
            ),
        ])
    }

    func testUnknownIsDistinctFromRespondingAndNotResponding() {
        let empty = ProcessResponsivenessSnapshot()
        XCTAssertNil(empty.isAvailable)
        XCTAssertEqual(empty.status(pid: nil), .unknown)
        XCTAssertEqual(empty.status(pid: 42, startTimeMillis: 20_000), .unknown)
        XCTAssertEqual(report(.responding).status(pid: 42, startTimeMillis: 20_000), .responding)
        XCTAssertEqual(report(.notResponding).status(pid: 42, startTimeMillis: 20_000), .notResponding)
    }

    func testReusedPIDAndDifferentExecutableDoNotInheritHang() {
        let sample = report(.notResponding)
        XCTAssertEqual(sample.status(pid: 42, startTimeMillis: 19_000), .unknown)
        XCTAssertEqual(sample.status(pid: 42, startTimeMillis: 21_000), .unknown)
        XCTAssertEqual(sample.status(pid: 42, startTimeMillis: 20_000, executablePath: "/other"), .unknown)
        XCTAssertEqual(sample.status(pid: 43, startTimeMillis: 20_000), .unknown)
    }

    func testCompactRowsRequireMatchingExecutableWhenStartTimeIsMissing() {
        let sample = report(.notResponding)
        XCTAssertEqual(sample.status(pid: 42), .unknown)
        XCTAssertEqual(sample.status(pid: 42, startTimeMillis: 0), .unknown)
        XCTAssertEqual(sample.status(pid: 42, executablePath: ""), .unknown)
        XCTAssertEqual(sample.status(pid: 42, executablePath: "/other"), .unknown)
        XCTAssertEqual(sample.status(pid: 42, executablePath: "/Example.app/Example"), .notResponding)
    }

    func testAppAggregateFindsHungMainAppWhenPrimaryPIDIsAHelper() {
        let sample = report(.notResponding)
        var row = processRow(primaryPID: 99, kind: .app)
        XCTAssertTrue(sample.isNotResponding(row: row))
        row.entityKind = .browser
        XCTAssertTrue(sample.isNotResponding(row: row))
        row.entityKind = .daemon
        XCTAssertFalse(sample.isNotResponding(row: row))
        row.entityKind = .app
        row.executablePath = "/Example.app/Helper"
        XCTAssertFalse(sample.isNotResponding(row: row))
    }

    func testResponsiveAndUnavailableReportsDoNotShowHangBadges() {
        let row = processRow(primaryPID: 42, kind: .app)
        XCTAssertFalse(report(.responding).isNotResponding(row: row))
        XCTAssertFalse(ProcessResponsivenessSnapshot(isAvailable: false).isNotResponding(row: row))
    }

    func testMissingNativeAPIStaysUnavailableInsteadOfReportingHealthy() {
        let probe = WindowServerResponsivenessProbe(symbolLookup: { _ in nil })
        let result = probe.scan(pids: [getpid()])
        XCTAssertEqual(result.isAvailable, false)
        XCTAssertTrue(result.observations.isEmpty)
    }

    @MainActor
    func testMonitorUpdatesOnRecoveryAndExitWithoutSnapshotChanges() async throws {
        let script = Script(report(.notResponding))
        let monitor = ProcessResponsivenessMonitor(scan: { script.scan($0) }, candidates: { [42] })
        defer { monitor.stop() }
        monitor.beginObserving(client: UUID())
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .notResponding }
        script.set(report(.responding))
        monitor.refresh()
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .responding }
        script.set(ProcessResponsivenessSnapshot(isAvailable: true))
        monitor.refresh()
        try await wait { monitor.snapshot.observations.isEmpty }
    }

    @MainActor
    func testMonitorReplacesLifetimeAndNeverCarriesOldHangForward() async throws {
        let script = Script(report(.notResponding))
        let monitor = ProcessResponsivenessMonitor(scan: { script.scan($0) }, candidates: { [42] })
        defer { monitor.stop() }
        monitor.beginObserving(client: UUID())
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .notResponding }
        script.set(report(.responding, start: 21_123_456))
        monitor.refresh()
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 21_000) == .responding }
        XCTAssertEqual(monitor.snapshot.status(pid: 42, startTimeMillis: 20_000), .unknown)
    }

    @MainActor
    func testSlowScanDoesNotOverlapAndStaleHangExpires() async throws {
        let script = Script(report(.notResponding))
        let monitor = ProcessResponsivenessMonitor(scan: { script.scan($0) }, candidates: { [42] })
        defer { script.release(); monitor.stop() }
        monitor.beginObserving(client: UUID())
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .notResponding }
        script.block()
        monitor.refresh()
        try await wait { script.callCount == 2 }
        monitor.refresh()
        XCTAssertEqual(script.callCount, 2)
        monitor.expire(now: ProcessInfo.processInfo.systemUptime + 16)
        XCTAssertEqual(monitor.snapshot.isAvailable, false)
        XCTAssertTrue(monitor.snapshot.observations.isEmpty)
    }

    @MainActor
    func testLateScanCannotRefreshAnOldHang() async throws {
        let script = Script(report(.notResponding))
        let clock = TestClock(10)
        script.block()
        let monitor = ProcessResponsivenessMonitor(
            scan: { script.scan($0) }, candidates: { [42] }, clock: { clock.now }
        )
        defer { script.release(); monitor.stop() }
        monitor.beginObserving(client: UUID())
        try await wait { script.callCount == 1 }
        clock.set(30)
        script.release()
        try await wait { monitor.snapshot.isAvailable == false }
        XCTAssertTrue(monitor.snapshot.observations.isEmpty)
        script.set(report(.responding))
        monitor.refresh()
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .responding }
        clock.set(45)
        monitor.expire(now: clock.now)
        XCTAssertEqual(monitor.snapshot.isAvailable, false)
    }

    @MainActor
    func testStopAndRestartCannotOverlapANativeScan() async throws {
        let script = Script(report(.notResponding))
        script.block()
        let monitor = ProcessResponsivenessMonitor(scan: { script.scan($0) }, candidates: { [42] })
        defer { script.release(); monitor.stop() }
        monitor.beginObserving(client: UUID())
        try await wait { script.callCount == 1 }
        monitor.stop()
        monitor.beginObserving(client: UUID())
        monitor.refresh()
        XCTAssertEqual(script.callCount, 1)
        script.release()
        try await wait { script.returnCount == 1 }
        for _ in 0..<10 { await Task.yield() }
        XCTAssertEqual(monitor.snapshot, ProcessResponsivenessSnapshot())
        script.set(report(.responding))
        try await wait {
            monitor.refresh()
            return monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .responding
        }
    }

    @MainActor
    func testStopRejectsLateResultsAndClearsAllStatus() async throws {
        let script = Script(report(.notResponding))
        script.block()
        let monitor = ProcessResponsivenessMonitor(scan: { script.scan($0) }, candidates: { [42] })
        defer { script.release(); monitor.stop() }
        monitor.beginObserving(client: UUID())
        try await wait { script.callCount == 1 }
        monitor.stop()
        script.release()
        try await wait { script.returnCount == 1 }
        for _ in 0..<10 { await Task.yield() }
        XCTAssertEqual(monitor.snapshot, ProcessResponsivenessSnapshot())
    }

    @MainActor
    func testEmptyApplicationCatalogClearsExitedProcess() async throws {
        let script = Script(report(.notResponding))
        var pids: [Int32] = [42]
        let monitor = ProcessResponsivenessMonitor(scan: { script.scan($0) }, candidates: { pids })
        defer { monitor.stop() }
        monitor.beginObserving(client: UUID())
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .notResponding }
        pids = []
        monitor.refresh()
        XCTAssertTrue(monitor.snapshot.observations.isEmpty)
        XCTAssertEqual(script.callCount, 1)
    }

    @MainActor
    func testMultipleMonitorWindowsKeepDemandUntilTheLastOneCloses() async throws {
        let script = Script(report(.notResponding))
        let monitor = ProcessResponsivenessMonitor(scan: { script.scan($0) }, candidates: { [42] })
        defer { monitor.stop() }
        monitor.refresh()
        XCTAssertEqual(script.callCount, 0)
        let first = UUID(), second = UUID()
        monitor.beginObserving(client: first)
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .notResponding }
        monitor.beginObserving(client: second)
        monitor.endObserving(client: first)
        script.set(report(.responding))
        try await wait { script.returnCount >= 2 }
        // Wait for the prior publication before asking for another scan.
        for _ in 0..<10 { await Task.yield() }
        monitor.refresh()
        try await wait { monitor.snapshot.status(pid: 42, startTimeMillis: 20_000) == .responding }
        monitor.endObserving(client: second)
        XCTAssertEqual(monitor.snapshot, ProcessResponsivenessSnapshot())
        let calls = script.callCount
        monitor.refresh()
        XCTAssertEqual(script.callCount, calls)
    }

    @MainActor
    private func wait(_ condition: () -> Bool) async throws {
        for _ in 0..<250 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTFail("Responsiveness worker did not reach the expected state")
    }

    private func processRow(primaryPID: UInt32, kind: EntityKind) -> UiProcessRow {
        UiProcessRow(
            entityId: "bundle:example", displayName: "Example", entityKind: kind,
            bundleId: "example", executablePath: "/Example.app/Example", primaryPid: primaryPID,
            processCount: 2, threadCount: 2, cpuPercent: 0, memoryResidentBytes: 0,
            memoryPhysicalFootprintBytes: 0, diskReadBps: 0, diskWriteBps: 0,
            networkReceiveBps: 0, networkSendBps: 0, wakeupsPerSecond: 0, energyNjPerS: 0,
            estimatedGpuPercent: 0, frictionScore: 0, isForeground: false, anomalyDetected: false,
            activeWindowTitle: nil, recentChangeSummary: nil, signingClassification: "unknown",
            isAdhoc: false, appVersion: nil
        )
    }
}

private final class TestClock: @unchecked Sendable {
    private let lock = NSLock()
    private var value: TimeInterval
    init(_ value: TimeInterval) { self.value = value }
    var now: TimeInterval { lock.withLock { value } }
    func set(_ value: TimeInterval) { lock.withLock { self.value = value } }
}

private final class Script: @unchecked Sendable {
    private let condition = NSCondition()
    private var result: ProcessResponsivenessSnapshot
    private var blocked = false
    private var calls = 0
    private var returns = 0

    init(_ result: ProcessResponsivenessSnapshot) { self.result = result }
    var callCount: Int { condition.withLock { calls } }
    var returnCount: Int { condition.withLock { returns } }
    func set(_ result: ProcessResponsivenessSnapshot) { condition.withLock { self.result = result } }
    func block() { condition.withLock { blocked = true } }
    func release() { condition.withLock { blocked = false; condition.broadcast() } }

    func scan(_ pids: [Int32]) -> ProcessResponsivenessSnapshot {
        condition.lock()
        defer { condition.unlock() }
        calls += 1
        while blocked { condition.wait() }
        returns += 1
        return result
    }
}
