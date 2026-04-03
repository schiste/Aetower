import Foundation
import SwiftUI
import AetowerBridge

private enum HistoryRangePreset: Double, CaseIterable, Identifiable {
    case lastHour = 3_600
    case last6Hours = 21_600
    case last24Hours = 86_400
    case last7Days = 604_800

    var id: Double { rawValue }

    var label: String {
        switch self {
        case .lastHour: return "1h"
        case .last6Hours: return "6h"
        case .last24Hours: return "24h"
        case .last7Days: return "7d"
        }
    }

    var detail: String {
        switch self {
        case .lastHour: return "last hour"
        case .last6Hours: return "last 6 hours"
        case .last24Hours: return "last 24 hours"
        case .last7Days: return "last 7 days"
        }
    }
}

private struct HistoricalEntitySummary: Identifiable {
    let id: String
    let name: String
    let sightings: Int
    let averageFriction: Double
    let peakFriction: Double
    let peakCPU: Double
    let peakNetworkBps: UInt64
}

public struct HistoryView: View {
    @ObservedObject private var state: AppState
    @State private var range: HistoryRangePreset = .lastHour

    private let columns = [GridItem(.adaptive(minimum: 180), spacing: 12)]

    public init(state: AppState) {
        self.state = state
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Persisted history")
                        .font(.system(size: 28, weight: .semibold, design: .rounded))
                    Text("This view comes from the persisted snapshot store, not just the in-memory trend buffer. Use it to see what Aetower has been observing over a wider time range.")
                        .foregroundStyle(.secondary)
                }

                HStack {
                    Picker("Range", selection: $range) {
                        ForEach(HistoryRangePreset.allCases) { preset in
                            Text(preset.label).tag(preset)
                        }
                    }
                    .pickerStyle(.segmented)
                    .frame(maxWidth: 320)

                    Spacer()

                    Button("Reload") {
                        state.loadHistory(force: true)
                    }
                    .buttonStyle(.bordered)
                }

                if let historyLoadError = state.historyLoadError, state.historySnapshots.isEmpty {
                    ContentUnavailableView(
                        "No persisted history yet",
                        systemImage: "clock.badge.questionmark",
                        description: Text(historyLoadError)
                    )
                    .frame(maxWidth: .infinity)
                } else {
                    GroupBox("Host trend") {
                        LazyVGrid(columns: columns, alignment: .leading, spacing: 12) {
                            TrendMetricCard(
                                title: "Friction",
                                value: String(format: "%.1f", latestHostFriction),
                                subtitle: range.detail,
                                samples: hostFrictionSamples,
                                style: .friction
                            )
                            TrendMetricCard(
                                title: "CPU",
                                value: String(format: "%.1f%%", latestHostCPU),
                                subtitle: "\(state.historySnapshots.count) persisted samples",
                                samples: hostCPUSamples,
                                style: .cpu
                            )
                            TrendMetricCard(
                                title: "Memory",
                                value: historyFormatBytes(latestHostMemory),
                                subtitle: "peak \(historyFormatBytes(hostMemorySamples.max().map(UInt64.init) ?? 0))",
                                samples: hostMemorySamples,
                                style: .memory
                            )
                            TrendMetricCard(
                                title: "Network",
                                value: historyFormatRate(latestHostNetwork),
                                subtitle: "peak \(historyFormatRate(hostNetworkSamples.max().map(UInt64.init) ?? 0))",
                                samples: hostNetworkSamples,
                                style: .disk
                            )
                            TrendMetricCard(
                                title: "GPU",
                                value: String(format: "%.1f%%", latestHostGPU),
                                subtitle: "ANE \(String(format: "%.1f%%", latestHostANE)) · \(historyFormatBytes(latestHostGPUMemory)) memory",
                                samples: hostGPUSamples,
                                style: .energy
                            )
                        }
                        .padding(.top, 4)
                    }

                    GroupBox("Most recurring entities") {
                        if historicalEntities.isEmpty {
                            Text("No entity-level history is persisted for this range yet.")
                                .foregroundStyle(.secondary)
                        } else {
                            VStack(alignment: .leading, spacing: 12) {
                                ForEach(historicalEntities.prefix(12)) { entity in
                                    VStack(alignment: .leading, spacing: 4) {
                                        HStack {
                                            Text(entity.name)
                                                .font(.headline)
                                            Spacer()
                                            Text(String(format: "avg %.1f", entity.averageFriction))
                                                .font(.caption.monospacedDigit())
                                                .foregroundStyle(.secondary)
                                        }
                                        Text(
                                            "\(entity.sightings) sightings · peak friction \(String(format: "%.1f", entity.peakFriction)) · peak CPU \(String(format: "%.1f%%", entity.peakCPU)) · peak net \(historyFormatRate(entity.peakNetworkBps))"
                                        )
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    }
                                }
                            }
                            .padding(.top, 4)
                        }
                    }

                    GroupBox("Store coverage") {
                        VStack(alignment: .leading, spacing: 8) {
                            LabeledContent("Range", value: range.detail)
                            LabeledContent("Persisted samples", value: "\(state.historySnapshots.count)")
                            LabeledContent("First sample", value: historyTimestamp(state.historySnapshots.first?.capturedAtMillis))
                            LabeledContent("Last sample", value: historyTimestamp(state.historySnapshots.last?.capturedAtMillis))
                        }
                        .padding(.top, 4)
                    }
                }
            }
            .padding(24)
        }
        .navigationTitle("History")
        .task {
            state.setHistoryWindow(seconds: range.rawValue)
        }
        .onChange(of: range) { _, newValue in
            state.setHistoryWindow(seconds: newValue.rawValue)
        }
    }

    private var latestHostFriction: Double {
        hostFrictionSamples.last ?? 0
    }

    private var latestHostCPU: Double {
        hostCPUSamples.last ?? 0
    }

    private var latestHostMemory: UInt64 {
        UInt64(hostMemorySamples.last ?? 0)
    }

    private var latestHostNetwork: UInt64 {
        UInt64(hostNetworkSamples.last ?? 0)
    }

    private var latestHostGPU: Double {
        hostGPUSamples.last ?? 0
    }

    private var latestHostANE: Double {
        state.historySnapshots.last.map { Double($0.host.anePercent) } ?? 0
    }

    private var latestHostGPUMemory: UInt64 {
        state.historySnapshots.last?.host.gpuMemoryBytes ?? 0
    }

    private var hostFrictionSamples: [Double] {
        state.historySnapshots.map(historyMachineFriction)
    }

    private var hostCPUSamples: [Double] {
        state.historySnapshots.map { Double($0.host.cpuPercent) }
    }

    private var hostMemorySamples: [Double] {
        state.historySnapshots.map { Double($0.host.memoryUsedBytes) }
    }

    private var hostNetworkSamples: [Double] {
        state.historySnapshots.map {
            Double($0.host.networkReceiveBps + $0.host.networkSendBps)
        }
    }

    private var hostGPUSamples: [Double] {
        state.historySnapshots.map { Double($0.host.gpuPercent) }
    }

    private var historicalEntities: [HistoricalEntitySummary] {
        struct Accumulator {
            var name: String
            var sightings: Int = 0
            var totalFriction: Double = 0
            var peakFriction: Double = 0
            var peakCPU: Double = 0
            var peakNetworkBps: UInt64 = 0
        }

        var grouped: [String: Accumulator] = [:]
        for snapshot in state.historySnapshots {
            for entity in snapshot.entities {
                let network = entity.metrics.networkReceiveBps + entity.metrics.networkSendBps
                var accumulator = grouped[entity.entityId] ?? Accumulator(name: entity.displayName)
                accumulator.sightings += 1
                accumulator.totalFriction += Double(entity.friction.totalScore)
                accumulator.peakFriction = max(accumulator.peakFriction, Double(entity.friction.totalScore))
                accumulator.peakCPU = max(accumulator.peakCPU, Double(entity.metrics.cpuPercent))
                accumulator.peakNetworkBps = max(accumulator.peakNetworkBps, network)
                grouped[entity.entityId] = accumulator
            }
        }

        return grouped.map { id, accumulator in
            HistoricalEntitySummary(
                id: id,
                name: accumulator.name,
                sightings: accumulator.sightings,
                averageFriction: accumulator.sightings == 0 ? 0 : accumulator.totalFriction / Double(accumulator.sightings),
                peakFriction: accumulator.peakFriction,
                peakCPU: accumulator.peakCPU,
                peakNetworkBps: accumulator.peakNetworkBps
            )
        }
        .sorted {
            $0.averageFriction > $1.averageFriction
                || ($0.averageFriction == $1.averageFriction && $0.peakCPU > $1.peakCPU)
        }
    }
}

private func historyMachineFriction(_ snapshot: SystemSnapshot) -> Double {
    let host = snapshot.host
    let cpuScore = min(Double(host.cpuPercent), 100.0) * 0.5
    let memoryRatio = host.memoryTotalBytes == 0 ? 0.0 : Double(host.memoryUsedBytes) / Double(host.memoryTotalBytes)
    let memoryScore = min(memoryRatio, 1.0) * 35.0
    let swapScore = host.swapUsedBytes == 0 ? 0.0 : (min(Double(host.swapUsedBytes) / 1_073_741_824.0, 8.0) / 8.0) * 15.0
    let compressedScore = host.memoryTotalBytes == 0 ? 0.0 : min(Double(host.compressedMemoryBytes) / Double(host.memoryTotalBytes), 1.0) * 12.0
    let networkScore = min(Double(host.networkReceiveBps + host.networkSendBps) / 8_388_608.0, 1.0) * 10.0
    let wakeupsScore = min(Double(host.wakeupsPerSecond) / 500.0, 1.0) * 8.0
    return min(cpuScore + memoryScore + swapScore + compressedScore + networkScore + wakeupsScore, 100.0)
}

private func historyFormatBytes(_ bytes: UInt64) -> String {
    let formatter = ByteCountFormatter()
    formatter.allowedUnits = [.useMB, .useGB]
    formatter.countStyle = .binary
    return formatter.string(fromByteCount: Int64(bytes))
}

private func historyFormatRate(_ bytesPerSecond: UInt64) -> String {
    "\(historyFormatBytes(bytesPerSecond))/s"
}

private func historyTimestamp(_ millis: UInt64?) -> String {
    guard let millis else {
        return "n/a"
    }
    let date = Date(timeIntervalSince1970: TimeInterval(millis) / 1000)
    return date.formatted(date: .abbreviated, time: .shortened)
}
