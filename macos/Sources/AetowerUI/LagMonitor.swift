import AppKit
import CoreVideo
import Foundation

struct LocalLagSample {
    var displayFrameIntervalMillis: Double = 0
    var displayRefreshHz: Double = 0
    var displayDroppedFrames: UInt64 = 0
    var inputAvgLatencyMillis: Double = 0
    var inputMaxLatencyMillis: Double = 0
    var inputSampleCount: UInt32 = 0
}

final class LagMonitor {
    private let lock = NSLock()
    private var displayLink: CVDisplayLink?
    private var localMonitors: [Any] = []
    private var lastDisplaySeconds: Double?
    private var nominalFrameIntervalMillis: Double = 0
    private var smoothedFrameIntervalMillis: Double = 0
    private var displayDroppedFrames: UInt64 = 0
    private var inputLatenciesMillis: [Double] = []
    private let maxInputSamples = 64

    func start() {
        startDisplayLink()
        startInputMonitoring()
    }

    func stop() {
        stopDisplayLink()
        stopInputMonitoring()
    }

    func sample() -> LocalLagSample {
        lock.lock()
        defer { lock.unlock() }

        let count = inputLatenciesMillis.count
        let inputAvgLatencyMillis = count > 0
            ? inputLatenciesMillis.reduce(0, +) / Double(count)
            : 0
        let inputMaxLatencyMillis = inputLatenciesMillis.max() ?? 0

        return LocalLagSample(
            displayFrameIntervalMillis: smoothedFrameIntervalMillis,
            displayRefreshHz: smoothedFrameIntervalMillis > 0
                ? 1000.0 / smoothedFrameIntervalMillis
                : (nominalFrameIntervalMillis > 0 ? 1000.0 / nominalFrameIntervalMillis : 0),
            displayDroppedFrames: displayDroppedFrames,
            inputAvgLatencyMillis: inputAvgLatencyMillis,
            inputMaxLatencyMillis: inputMaxLatencyMillis,
            inputSampleCount: UInt32(count)
        )
    }

    private func startDisplayLink() {
        guard displayLink == nil else { return }
        var candidate: CVDisplayLink?
        guard CVDisplayLinkCreateWithActiveCGDisplays(&candidate) == kCVReturnSuccess,
              let candidate else {
            return
        }
        let callback: CVDisplayLinkOutputCallback = { _, _, outputTime, _, _, userInfo in
            guard let userInfo else { return kCVReturnSuccess }
            let monitor = Unmanaged<LagMonitor>.fromOpaque(userInfo).takeUnretainedValue()
            monitor.recordDisplay(outputTime.pointee)
            return kCVReturnSuccess
        }
        CVDisplayLinkSetOutputCallback(
            candidate,
            callback,
            UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        )
        if CVDisplayLinkStart(candidate) == kCVReturnSuccess {
            displayLink = candidate
        }
    }

    private func stopDisplayLink() {
        guard let displayLink else { return }
        CVDisplayLinkStop(displayLink)
        self.displayLink = nil
    }

    private func startInputMonitoring() {
        guard localMonitors.isEmpty else { return }
        let mask: NSEvent.EventTypeMask = [
            .keyDown,
            .leftMouseDown,
            .rightMouseDown,
            .otherMouseDown,
            .scrollWheel
        ]
        if let monitor = NSEvent.addLocalMonitorForEvents(matching: mask, handler: { [weak self] event in
            self?.recordInput(event)
            return event
        }) {
            localMonitors.append(monitor)
        }
    }

    private func stopInputMonitoring() {
        localMonitors.forEach(NSEvent.removeMonitor)
        localMonitors.removeAll(keepingCapacity: false)
    }

    private func recordDisplay(_ timestamp: CVTimeStamp) {
        let scale = Double(timestamp.videoTimeScale)
        guard scale > 0 else { return }
        let displaySeconds = Double(timestamp.videoTime) / scale
        let refreshPeriodSeconds = timestamp.videoRefreshPeriod > 0
            ? Double(timestamp.videoRefreshPeriod) / scale
            : 0

        lock.lock()
        defer { lock.unlock() }

        if refreshPeriodSeconds > 0 {
            nominalFrameIntervalMillis = refreshPeriodSeconds * 1000.0
        }

        if let lastDisplaySeconds {
            let deltaMillis = max(0, (displaySeconds - lastDisplaySeconds) * 1000.0)
            if deltaMillis > 0 {
                if smoothedFrameIntervalMillis == 0 {
                    smoothedFrameIntervalMillis = deltaMillis
                } else {
                    smoothedFrameIntervalMillis =
                        (smoothedFrameIntervalMillis * 0.8) + (deltaMillis * 0.2)
                }
                let expectedMillis = nominalFrameIntervalMillis > 0
                    ? nominalFrameIntervalMillis
                    : deltaMillis
                if expectedMillis > 0 {
                    let observedFrames = max(1, Int((deltaMillis / expectedMillis).rounded(.down)))
                    if observedFrames > 1 {
                        displayDroppedFrames = min(
                            UInt64.max,
                            displayDroppedFrames + UInt64(observedFrames - 1)
                        )
                    }
                }
            }
        }

        lastDisplaySeconds = displaySeconds
    }

    private func recordInput(_ event: NSEvent) {
        let latencyMillis = max(
            0,
            (ProcessInfo.processInfo.systemUptime - event.timestamp) * 1000.0
        )
        lock.lock()
        defer { lock.unlock() }
        inputLatenciesMillis.append(latencyMillis)
        if inputLatenciesMillis.count > maxInputSamples {
            inputLatenciesMillis.removeFirst(inputLatenciesMillis.count - maxInputSamples)
        }
    }
}
