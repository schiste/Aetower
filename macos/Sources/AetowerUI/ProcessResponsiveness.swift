import AppKit
import Darwin
import Observation

enum ProcessResponsiveness: Equatable, Sendable {
    case responding
    case notResponding
    case unknown
}

struct ProcessResponsivenessObservation: Equatable, Sendable {
    let pid: UInt32
    let startTimeMicros: UInt64
    let executablePath: String
    let status: ProcessResponsiveness
}

struct ProcessResponsivenessSnapshot: Equatable, Sendable {
    // nil means that no scan has completed yet, not that apps are healthy.
    var isAvailable: Bool?
    var observations: [UInt32: ProcessResponsivenessObservation] = [:]

    func status(
        pid: UInt32?,
        startTimeMillis: UInt64? = nil,
        executablePath: String? = nil
    ) -> ProcessResponsiveness {
        guard let pid, let observation = observations[pid] else { return .unknown }
        if let startTimeMillis, startTimeMillis > 0 {
            // The Rust collector exposes start times at whole-second precision.
            guard startTimeMillis / 1_000 == observation.startTimeMicros / 1_000_000 else {
                return .unknown
            }
        }
        if let executablePath, !executablePath.isEmpty {
            guard executablePath == observation.executablePath else { return .unknown }
        } else if startTimeMillis == nil || startTimeMillis == 0 {
            // Compact rows have no start time: require an executable match.
            return .unknown
        }
        return observation.status
    }
}

/// Reads WindowServer's app event-response state, never a CPU/sleep heuristic.
/// This SPI is optional: missing symbols, unregistered processes and races stay
/// unknown. No private framework is linked and no input or AppleEvent is sent.
final class WindowServerResponsivenessProbe: @unchecked Sendable {
    private typealias Connection = @convention(c) () -> Int32
    private typealias ProcessNumber = @convention(c) (Int32, UnsafeMutableRawPointer) -> Int32
    private typealias IsUnresponsive = @convention(c) (Int32, UnsafeRawPointer) -> Bool

    static let shared = WindowServerResponsivenessProbe()

    private let connection: Connection?
    private let processNumber: ProcessNumber?
    private let isUnresponsive: IsUnresponsive?

    init(symbolLookup: (String) -> UnsafeMutableRawPointer? = {
        dlsym(UnsafeMutableRawPointer(bitPattern: -2), $0)
    }) {
        connection = symbolLookup("CGSMainConnectionID").map { unsafeBitCast($0, to: Connection.self) }
        processNumber = symbolLookup("GetProcessForPID").map { unsafeBitCast($0, to: ProcessNumber.self) }
        isUnresponsive = symbolLookup("CGSEventIsAppUnresponsive").map { unsafeBitCast($0, to: IsUnresponsive.self) }
    }

    func scan(pids: [Int32]) -> ProcessResponsivenessSnapshot {
        guard let connection, let processNumber, let isUnresponsive else {
            return ProcessResponsivenessSnapshot(isAvailable: false)
        }
        let connectionID = connection()
        guard connectionID > 0 else { return ProcessResponsivenessSnapshot(isAvailable: false) }
        var observations: [UInt32: ProcessResponsivenessObservation] = [:]
        for pid in Set(pids) where pid > 0 {
            if Task.isCancelled { break }
            guard let start = Self.startTimeMicros(pid: pid) else { continue }
            var number = (UInt32(0), UInt32(0))
            let result = withUnsafeMutablePointer(to: &number) {
                processNumber(pid, UnsafeMutableRawPointer($0))
            }
            guard result == 0, number != (0, 0) else { continue }
            let unresponsive = withUnsafePointer(to: &number) {
                isUnresponsive(connectionID, UnsafeRawPointer($0))
            }
            // PROC_PIDPATHINFO_MAXSIZE is a C expression macro, not imported by Swift.
            var path = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
            guard proc_pidpath(pid, &path, UInt32(path.count)) > 0,
                  Self.startTimeMicros(pid: pid) == start else { continue }
            let processID = UInt32(pid)
            observations[processID] = ProcessResponsivenessObservation(
                pid: processID,
                startTimeMicros: start,
                executablePath: path.withUnsafeBufferPointer { String(cString: $0.baseAddress!) },
                status: unresponsive ? .notResponding : .responding
            )
        }
        return ProcessResponsivenessSnapshot(isAvailable: true, observations: observations)
    }

    private static func startTimeMicros(pid: Int32) -> UInt64? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size,
              info.pbi_start_tvsec > 0 else { return nil }
        return info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec
    }
}

/// One background scan at a time, driven by AppState's existing refresh loop.
/// Results expire even if a native call stalls; stop/restart rejects old work.
@MainActor
@Observable
final class ProcessResponsivenessMonitor {
    private(set) var snapshot = ProcessResponsivenessSnapshot()
    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var generation: UInt64 = 0
    @ObservationIgnored private var clients: Set<UUID> = []
    @ObservationIgnored private var lastSuccess: TimeInterval?
    @ObservationIgnored private let scan: @Sendable ([Int32]) -> ProcessResponsivenessSnapshot
    @ObservationIgnored private let candidates: () -> [Int32]
    @ObservationIgnored private let clock: @Sendable () -> TimeInterval
    private static let maximumAge: TimeInterval = 15

    init(
        scan: @escaping @Sendable ([Int32]) -> ProcessResponsivenessSnapshot = {
            WindowServerResponsivenessProbe.shared.scan(pids: $0)
        },
        candidates: @escaping () -> [Int32] = {
            guard NSApp?.isRunning == true else { return [] }
            return NSWorkspace.shared.runningApplications
                .filter { !$0.isTerminated }
                .map(\.processIdentifier)
        },
        clock: @escaping @Sendable () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }
    ) {
        self.scan = scan
        self.candidates = candidates
        self.clock = clock
    }

    func beginObserving(client: UUID) {
        clients.insert(client)
        refresh()
    }

    func endObserving(client: UUID) {
        clients.remove(client)
        if clients.isEmpty { stop() }
    }

    func refresh(now: TimeInterval? = nil) {
        guard !clients.isEmpty else { return }
        expire(now: now ?? clock())
        guard task == nil else { return }
        let pids = candidates()
        guard !pids.isEmpty else {
            if !snapshot.observations.isEmpty { snapshot.observations = [:] }
            return
        }
        let scan = self.scan
        let clock = self.clock
        let currentGeneration = generation
        task = Task.detached(priority: .utility) { [weak self] in
            let startedAt = clock()
            let result = scan(pids)
            await self?.publish(
                Task.isCancelled ? nil : result,
                startedAt: startedAt,
                generation: currentGeneration
            )
        }
    }

    func stop() {
        generation &+= 1
        task?.cancel()
        // Keep the in-flight slot until the worker returns, even across a
        // stop/restart. Cancellation cannot interrupt a native system call.
        lastSuccess = nil
        if snapshot != ProcessResponsivenessSnapshot() { snapshot = ProcessResponsivenessSnapshot() }
    }

    func expire(now: TimeInterval) {
        if let lastSuccess, now - lastSuccess >= Self.maximumAge {
            let expired = ProcessResponsivenessSnapshot(isAvailable: false)
            if snapshot != expired { snapshot = expired }
        }
    }

    private func publish(
        _ result: ProcessResponsivenessSnapshot?,
        startedAt: TimeInterval,
        generation: UInt64
    ) {
        task = nil
        guard self.generation == generation, let result else { return }
        let age = clock() - startedAt
        guard age >= 0, age < Self.maximumAge else {
            lastSuccess = nil
            let unavailable = ProcessResponsivenessSnapshot(isAvailable: false)
            if snapshot != unavailable { snapshot = unavailable }
            return
        }
        // Age the oldest observation from scan start, not publication time.
        lastSuccess = startedAt
        if snapshot != result { snapshot = result }
    }
}
