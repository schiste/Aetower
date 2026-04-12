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
    let state: AppState
    @State private var range: HistoryRangePreset = .lastHour
    @State private var showClearHistoryConfirmation = false

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
                    .disabled(state.historyIsLoading || state.historyIsLoadingMore)

                    Button("Clear persisted history", role: .destructive) {
                        showClearHistoryConfirmation = true
                    }
                    .buttonStyle(.bordered)
                    .disabled(state.historyIsLoading || state.historyIsLoadingMore)
                }

                if state.historyIsLoading && state.historySnapshots.isEmpty {
                    historyLoadingState
                } else if let historyLoadError = state.historyLoadError, state.historySnapshots.isEmpty {
                    ContentUnavailableView(
                        "No persisted history yet",
                        systemImage: "clock.badge.questionmark",
                        description: Text(historyLoadError)
                    )
                    .frame(maxWidth: .infinity)
                } else {
                    // Compute sample arrays once per render to avoid
                    // the double-evaluation pattern where latestHostXxx
                    // re-traversed the same array that body passed
                    // separately. Each hostXxxSamples was O(n) over all
                    // loaded snapshots; calling it twice per card doubled
                    // the cost for no benefit.
                    let frictionSamples = hostFrictionSamples
                    let cpuSamples = hostCPUSamples
                    let memorySamples = hostMemorySamples
                    let networkSamples = hostNetworkSamples
                    let gpuSamples = hostGPUSamples
                    let lastHost = state.historySnapshots.last?.host

                    GroupBox("Host trend") {
                        LazyVGrid(columns: columns, alignment: .leading, spacing: 12) {
                            TrendMetricCard(
                                title: "Friction",
                                value: String(format: "%.1f", frictionSamples.last ?? 0),
                                subtitle: range.detail,
                                samples: frictionSamples,
                                style: .friction
                            )
                            TrendMetricCard(
                                title: "CPU",
                                value: String(format: "%.1f%%", cpuSamples.last ?? 0),
                                subtitle: "\(state.historySnapshots.count) persisted samples",
                                samples: cpuSamples,
                                style: .cpu
                            )
                            TrendMetricCard(
                                title: "Memory",
                                value: formatBytes(UInt64(memorySamples.last ?? 0)),
                                subtitle: "peak \(formatBytes(memorySamples.max().map(UInt64.init) ?? 0))",
                                samples: memorySamples,
                                style: .memory
                            )
                            TrendMetricCard(
                                title: "Network",
                                value: formatRate(UInt64(networkSamples.last ?? 0)),
                                subtitle: "peak \(formatRate(networkSamples.max().map(UInt64.init) ?? 0))",
                                samples: networkSamples,
                                style: .network
                            )
                            TrendMetricCard(
                                title: "GPU",
                                value: String(format: "%.1f%%", gpuSamples.last ?? 0),
                                subtitle: "ANE \(String(format: "%.1f%%", lastHost.map { Double($0.anePercent) } ?? 0)) · \(formatBytes(lastHost?.gpuMemoryBytes ?? 0)) memory",
                                samples: gpuSamples,
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
                                            "\(entity.sightings) sightings · peak friction \(String(format: "%.1f", entity.peakFriction)) · peak CPU \(String(format: "%.1f%%", entity.peakCPU)) · peak net \(formatRate(entity.peakNetworkBps))"
                                        )
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    }
                                }
                            }
                            .padding(.top, 4)
                        }
                    }

                    GroupBox("Latest notable changes") {
                        if latestChangeSummaries.isEmpty {
                            Text("No persisted recent-change summaries are available in this range yet.")
                                .foregroundStyle(.secondary)
                        } else {
                            VStack(alignment: .leading, spacing: 10) {
                                ForEach(latestChangeSummaries, id: \.id) { item in
                                    VStack(alignment: .leading, spacing: 4) {
                                        HStack {
                                            Text(item.name)
                                                .font(.headline)
                                            Spacer()
                                            Text(historyTimestamp(item.capturedAtMillis))
                                                .font(.caption2)
                                                .foregroundStyle(.tertiary)
                                        }
                                        Text(item.summary)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                            .fixedSize(horizontal: false, vertical: true)
                                    }
                                }
                            }
                            .padding(.top, 4)
                        }
                    }

                    GroupBox("Store coverage") {
                        VStack(alignment: .leading, spacing: 8) {
                            LabeledContent("Range", value: range.detail)
                            LabeledContent(
                                "Loaded samples",
                                value: "\(state.historySnapshots.count)\(state.historyRangeSummary.map { " of \($0.rangeCount)" } ?? "")"
                            )
                            LabeledContent(
                                "Store size",
                                value: formatBytes(state.historyRangeSummary?.storeBytes ?? 0)
                            )
                            LabeledContent(
                                "WAL size",
                                value: formatBytes(state.historyRangeSummary?.walBytes ?? 0)
                            )
                            LabeledContent(
                                "Persisted samples",
                                value: "\(state.historyRangeSummary?.snapshotCount ?? 0)"
                            )
                            LabeledContent(
                                "Quarantine rows",
                                value: "\(state.historyRangeSummary?.quarantineCount ?? 0)"
                            )
                            LabeledContent(
                                "First sample",
                                value: historyTimestamp(state.historyRangeSummary?.oldestMillis ?? state.historySnapshots.first?.capturedAtMillis)
                            )
                            LabeledContent(
                                "Last sample",
                                value: historyTimestamp(state.historyRangeSummary?.newestMillis ?? state.historySnapshots.last?.capturedAtMillis)
                            )
                            LabeledContent(
                                "Last load",
                                value: String(format: "%.0f ms", state.historyLastLoadDurationMillis)
                            )
                            if let historyLoadStatus = state.historyLoadStatus {
                                Text(historyLoadStatus)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                            Text("Aetower now keeps persisted history on a bounded local budget by default: a shorter retention window, WAL checkpoints, and automatic hard-cap trims when the store grows too large.")
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .fixedSize(horizontal: false, vertical: true)
                            if state.historyIsLoadingMore {
                                ProgressView("Loading older samples…")
                                    .progressViewStyle(.linear)
                            } else if state.historyHasMore {
                                Button("Load more persisted samples") {
                                    state.loadMoreHistory()
                                }
                                .buttonStyle(.bordered)
                            }
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
            state.setHistoryVisible(true)
        }
        .onDisappear {
            state.setHistoryVisible(false)
        }
        .onChange(of: range) { _, newValue in
            state.setHistoryWindow(seconds: newValue.rawValue)
        }
        .alert("Clear persisted history?", isPresented: $showClearHistoryConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Clear", role: .destructive) {
                state.clearHistory()
            }
        } message: {
            Text("This removes persisted history and quarantine rows from local storage. Live monitoring continues.")
        }
    }


    private var historyLoadingState: some View {
        ContentUnavailableView {
            Label("Loading persisted history", systemImage: "clock.arrow.trianglehead.counterclockwise.rotate.90")
        } description: {
            VStack(alignment: .center, spacing: 8) {
                Text(state.historyLoadStatus ?? "Preparing the local history store and loading recent samples first.")
                if let summary = state.historyRangeSummary {
                    Text(
                        "\(summary.rangeCount) samples in range · store \(formatBytes(summary.storeBytes)) · WAL \(formatBytes(summary.walBytes))"
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                ProgressView()
                    .controlSize(.large)
            }
        }
        .frame(maxWidth: .infinity)
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

    private var latestChangeSummaries: [(id: String, name: String, summary: String, capturedAtMillis: UInt64)] {
        guard let latestSnapshot = state.historySnapshots.last else {
            return []
        }
        return latestSnapshot.entities.compactMap { entity in
            guard let summary = entity.recentChangeSummary, !summary.isEmpty else {
                return nil
            }
            return (
                id: entity.entityId,
                name: entity.displayName,
                summary: summary,
                capturedAtMillis: latestSnapshot.capturedAtMillis
            )
        }
        .prefix(8)
        .map { $0 }
    }
}

private func historyMachineFriction(_ snapshot: SystemSnapshot) -> Double {
    machineFrictionScore(for: snapshot.host)
}

private func historyTimestamp(_ millis: UInt64?) -> String {
    guard let millis else {
        return "n/a"
    }
    let date = Date(timeIntervalSince1970: TimeInterval(millis) / 1000)
    return date.formatted(date: .abbreviated, time: .shortened)
}
