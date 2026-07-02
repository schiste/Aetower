import AetowerBridge
import Foundation

// MARK: - Byte and rate formatters

public func formatBytes(_ bytes: UInt64) -> String {
    MonitorByteFormatters.binary.string(fromByteCount: Int64(bytes))
}

/// KB-granular adaptive variant for compact per-row/tile readouts.
public func formatBytesCompact(_ bytes: UInt64) -> String {
    MonitorByteFormatters.compact.string(fromByteCount: Int64(bytes))
}

func formatRate(_ bytesPerSecond: UInt64) -> String {
    "\(MonitorByteFormatters.rate.string(fromByteCount: Int64(bytesPerSecond)))/s"
}

func formatWakeups(_ wakeupsPerSecond: Float) -> String {
    if wakeupsPerSecond >= 100 {
        return String(format: "%.0f/s", wakeupsPerSecond)
    }
    return String(format: "%.1f/s", wakeupsPerSecond)
}

private enum MonitorByteFormatters {
    nonisolated(unsafe) static let binary: ByteCountFormatter = {
        let formatter = ByteCountFormatter()
        formatter.allowedUnits = [.useMB, .useGB]
        formatter.countStyle = .binary
        return formatter
    }()

    nonisolated(unsafe) static let compact: ByteCountFormatter = {
        let formatter = ByteCountFormatter()
        formatter.allowedUnits = [.useKB, .useMB, .useGB]
        formatter.countStyle = .binary
        formatter.includesUnit = true
        formatter.isAdaptive = true
        return formatter
    }()

    nonisolated(unsafe) static let rate: ByteCountFormatter = {
        let formatter = ByteCountFormatter()
        formatter.allowedUnits = [.useBytes, .useKB, .useMB, .useGB]
        formatter.countStyle = .binary
        formatter.isAdaptive = true
        formatter.includesUnit = true
        formatter.zeroPadsFractionDigits = false
        return formatter
    }()
}

// MARK: - Host summary helpers

func hostGPUSummary(_ host: HostSnapshot) -> String {
    if host.anePercent > 0 {
        return "ANE \(String(format: "%.1f%%", host.anePercent)) · \(formatBytes(host.gpuMemoryBytes)) GPU memory"
    }
    if host.gpuMemoryBytes > 0 {
        return "\(formatBytes(host.gpuMemoryBytes)) GPU memory"
    }
    return "render/compute activity"
}

func thermalStateLabel(_ state: ThermalState) -> String {
    switch state {
    case .nominal: return "nominal"
    case .fair: return "fair"
    case .serious: return "serious"
    case .critical: return "critical"
    }
}

// MARK: - Trend helpers

func trendLabel(samples: [Double], stableText: String) -> String {
    guard let first = samples.first, let last = samples.last, samples.count >= 2 else {
        return stableText
    }
    let baseline = max(abs(first), 1.0)
    let deltaRatio = (last - first) / baseline
    if deltaRatio > 0.12 { return "rising" }
    if deltaRatio < -0.12 { return "falling" }
    return stableText
}

func memoryMetricNote(residentBytes: UInt64, footprintBytes: UInt64) -> String {
    if footprintBytes > 0 {
        if footprintBytes == residentBytes {
            return "Resident and footprint currently align."
        }
        return "Resident is the live in-memory set. Footprint follows macOS task footprint and can include graphics and other charged memory."
    }
    return "Resident is available. Footprint is unavailable for this sample."
}

func trendWindowLabel(sampleCount: Int) -> String {
    let seconds = min(sampleCount * 2, 300)
    return "last \(seconds)s"
}

// MARK: - Host band classification

enum HostBand {
    case nominal
    case elevated
    case severe
}

func hostPressureBand(_ host: HostSnapshot) -> HostBand {
    let totalBytes = max(Double(host.memoryTotalBytes), 1)
    let usedRatio = Double(host.memoryUsedBytes) / totalBytes
    let compressedRatio = Double(host.compressedMemoryBytes) / totalBytes
    let swapRatio = Double(host.swapUsedBytes) / totalBytes
    if usedRatio >= 0.88 || compressedRatio >= 0.12 || swapRatio >= 0.08 { return .severe }
    if usedRatio >= 0.75 || compressedRatio >= 0.05 || swapRatio >= 0.02 { return .elevated }
    return .nominal
}

func hostWakeupBand(_ wakeupsPerSecond: Float) -> HostBand {
    if wakeupsPerSecond >= 3_000 { return .severe }
    if wakeupsPerSecond >= 1_500 { return .elevated }
    return .nominal
}

// MARK: - Memory helpers

func memoryLoadPercent(bytes: UInt64, totalBytes: UInt64) -> Double {
    guard totalBytes > 0 else { return 0 }
    return (Double(bytes) / Double(totalBytes)) * 100.0
}

func hostMemoryPressureScore(_ host: HostSnapshot) -> Double {
    let totalBytes = max(Double(host.memoryTotalBytes), 1)
    let usedRatio = Double(host.memoryUsedBytes) / totalBytes
    let compressedRatio = Double(host.compressedMemoryBytes) / totalBytes
    let swapRatio = Double(host.swapUsedBytes) / totalBytes
    return min(usedRatio * 55.0 + min(compressedRatio, 1.0) * 25.0 + min(swapRatio, 1.0) * 20.0, 100.0)
}

func effectiveMemoryBytes(residentBytes: UInt64, footprintBytes: UInt64) -> UInt64 {
    max(residentBytes, footprintBytes)
}

func entityEffectiveMemoryBytes(_ entity: EntitySnapshot) -> UInt64 {
    effectiveMemoryBytes(
        residentBytes: entity.metrics.memoryResidentBytes,
        footprintBytes: entity.metrics.memoryPhysicalFootprintBytes
    )
}

func entityMemoryLoadPercent(_ entity: EntitySnapshot, totalBytes: UInt64) -> Double {
    memoryLoadPercent(bytes: entityEffectiveMemoryBytes(entity), totalBytes: totalBytes)
}

// MARK: - Age label

func ageLabel(from startMillis: UInt64, now capturedAtMillis: UInt64) -> String {
    guard startMillis > 0, capturedAtMillis >= startMillis else { return "n/a" }
    let elapsedSeconds = Int((capturedAtMillis - startMillis) / 1_000)
    if elapsedSeconds < 60 { return "\(elapsedSeconds)s" }
    if elapsedSeconds < 3_600 { return "\(elapsedSeconds / 60)m" }
    if elapsedSeconds < 86_400 { return "\(elapsedSeconds / 3_600)h" }
    return "\(elapsedSeconds / 86_400)d"
}
