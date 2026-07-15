import AetowerBridge
import Foundation
import SwiftUI

/// Rarely-changing agent context (Chau7 sessions, AI repo summaries, and
/// resource cost rollups) grouped into one change-gated slice so its readers are
/// only invalidated when the agent landscape actually moves, not on every engine
/// tick.
public struct AgentContextSlice: Equatable, Sendable {
    public var chau7Sessions: [Chau7SessionSummary]
    public var aiRepoSummaries: [AiRepoSummary]
    public var resourceCostRollups: [ResourceCostRollup]

    public init(
        chau7Sessions: [Chau7SessionSummary] = [],
        aiRepoSummaries: [AiRepoSummary] = [],
        resourceCostRollups: [ResourceCostRollup] = []
    ) {
        self.chau7Sessions = chau7Sessions
        self.aiRepoSummaries = aiRepoSummaries
        self.resourceCostRollups = resourceCostRollups
    }
}

/// Compact process attribution used by Repos. It keeps the repo-matching
/// fields needed for live badges/cost rollups without making the Repos page
/// observe the full entity/component graph every refresh.
struct RepositoryRuntimeEntityContext: Equatable, Sendable {
    let entityId: String
    let candidateRoots: [String]
    let memoryBytes: UInt64
    let cpuPercent: Float

    init(
        entityId: String,
        candidateRoots: [String],
        memoryBytes: UInt64,
        cpuPercent: Float
    ) {
        self.entityId = entityId
        self.candidateRoots = candidateRoots
        self.memoryBytes = memoryBytes
        self.cpuPercent = cpuPercent
    }

    init?(entity: EntitySnapshot) {
        var roots: [String] = []
        var seen = Set<String>()
        for component in entity.components {
            for candidate in [component.adapterContext?.repoRoot, component.cwd] {
                guard let candidate, !candidate.isEmpty, seen.insert(candidate).inserted else {
                    continue
                }
                roots.append(candidate)
            }
        }
        guard !roots.isEmpty else { return nil }
        self.init(
            entityId: entity.entityId,
            candidateRoots: roots,
            memoryBytes: entityEffectiveMemoryBytes(entity),
            cpuPercent: entity.metrics.cpuPercent
        )
    }
}

/// Repository-specific runtime overlay. Static repo storage/scorecard rows are
/// cached elsewhere; this slice is the smaller live layer that changes when
/// sessions, repo-linked processes, AI usage, or resource-cost rollups change.
struct RepositoryRuntimeContextSlice: Equatable, Sendable {
    var chau7Sessions: [Chau7SessionSummary]
    var entities: [RepositoryRuntimeEntityContext]
    var aiRepoSummaries: [AiRepoSummary]
    var resourceCostRollups: [ResourceCostRollup]

    init(
        chau7Sessions: [Chau7SessionSummary] = [],
        entities: [RepositoryRuntimeEntityContext] = [],
        aiRepoSummaries: [AiRepoSummary] = [],
        resourceCostRollups: [ResourceCostRollup] = []
    ) {
        self.chau7Sessions = chau7Sessions
        self.entities = entities
        self.aiRepoSummaries = aiRepoSummaries
        self.resourceCostRollups = resourceCostRollups
    }
}

/// Sensor page projection. It deliberately includes the complete HostSnapshot
/// because hardware sensors live there, but it isolates Sensors from unrelated
/// entity-array updates by precomputing the one entity-derived value it needs.
struct SensorDashboardPayload: Equatable, Sendable {
    var host: HostSnapshot
    var hostTrend: HostTrend
    var thermalForecast: ThermalForecast?
    var totalAttributedWatts: Double
    var capturedAtMillis: UInt64

    init(snapshot: SystemSnapshot) {
        self.host = snapshot.host
        self.hostTrend = snapshot.hostTrend
        self.thermalForecast = snapshot.thermalForecast
        self.totalAttributedWatts = snapshot.entities
            .compactMap { EnergyTranslation.watts(fromNjPerS: $0.metrics.energyNjPerS) }
            .reduce(0, +)
        self.capturedAtMillis = snapshot.capturedAtMillis
    }
}

struct SnapshotHotSliceSignature: Equatable {
    let hostDigest: Int
    let entityDigest: Int
    let entityCount: Int

    init(snapshot: SystemSnapshot) {
        self.hostDigest = Self.digestHost(snapshot.host)
        self.entityDigest = Self.digestEntities(snapshot.entities)
        self.entityCount = snapshot.entities.count
    }

    private static func digestHost(_ host: HostSnapshot) -> Int {
        var hasher = Hasher()
        hasher.combine(bucket(host.cpuPercent, scale: 2))
        hasher.combine(bytesBucket(host.memoryUsedBytes))
        hasher.combine(bytesBucket(host.compressedMemoryBytes))
        hasher.combine(bytesBucket(host.swapUsedBytes))
        hasher.combine(bytesBucket(host.diskReadBps, quantum: 512 * 1_024))
        hasher.combine(bytesBucket(host.diskWriteBps, quantum: 512 * 1_024))
        hasher.combine(bytesBucket(host.networkReceiveBps, quantum: 512 * 1_024))
        hasher.combine(bytesBucket(host.networkSendBps, quantum: 512 * 1_024))
        hasher.combine(bucket(host.wakeupsPerSecond, scale: 1))
        hasher.combine(host.thermalState)
        hasher.combine(host.onBattery)
        hasher.combine(host.lowPowerMode)
        hasher.combine(host.frontmostAppName)
        hasher.combine(host.frontmostWindowTitle)
        hasher.combine(bucket(host.aiAgentFriction, scale: 2))
        hasher.combine(host.aiAgentCount)
        hasher.combine(bucket(host.gpuPercent, scale: 2))
        hasher.combine(bytesBucket(host.gpuMemoryBytes))
        hasher.combine(host.fans.count)
        hasher.combine(host.cpuTemperatures.count)
        hasher.combine(host.powerReadings.count)
        hasher.combine(host.disks.count)
        hasher.combine(host.bluetoothDevices.count)
        hasher.combine(host.perCoreCpu.count)
        return hasher.finalize()
    }

    private static func digestEntities(_ entities: [EntitySnapshot]) -> Int {
        var hasher = Hasher()
        for entity in entities {
            hasher.combine(entity.entityId)
            hasher.combine(entity.displayName)
            hasher.combine(entity.entityKind)
            hasher.combine(bucket(entity.metrics.cpuPercent, scale: 2))
            hasher.combine(bytesBucket(entity.metrics.memoryResidentBytes))
            hasher.combine(bytesBucket(entity.metrics.memoryPhysicalFootprintBytes))
            hasher.combine(bytesBucket(entity.metrics.diskReadBps, quantum: 512 * 1_024))
            hasher.combine(bytesBucket(entity.metrics.diskWriteBps, quantum: 512 * 1_024))
            hasher.combine(bytesBucket(entity.metrics.networkReceiveBps, quantum: 512 * 1_024))
            hasher.combine(bytesBucket(entity.metrics.networkSendBps, quantum: 512 * 1_024))
            hasher.combine(bucket(entity.metrics.wakeupsPerSecond, scale: 1))
            hasher.combine(bucket(Float(entity.metrics.energyNjPerS / 1_000_000_000), scale: 2))
            hasher.combine(bucket(entity.metrics.estimatedGpuPercent, scale: 2))
            hasher.combine(entity.metrics.processCount)
            hasher.combine(entity.metrics.threadCount)
            hasher.combine(entity.metrics.isForeground)
            hasher.combine(bucket(entity.friction.totalScore, scale: 2))
            hasher.combine(entity.components.count)
            hasher.combine(entity.processLineage.count)
            hasher.combine(entity.badges)
            hasher.combine(entity.activeWindowTitle)
            hasher.combine(entity.recentChangeSummary)
            hasher.combine(entity.anomalyDetected)
            hasher.combine(entity.groupingSuggestion)
            hasher.combine(entity.sessionMarkers.count)
            hasher.combine(entity.recommendations.count)
            hasher.combine(entity.networkConnections.count)
            hasher.combine(entity.signingClassification)
            hasher.combine(entity.isAdhoc)
            hasher.combine(entity.appVersion)
        }
        return hasher.finalize()
    }

    private static func bucket(_ value: Float, scale: Float) -> Int {
        Int((value * scale).rounded())
    }

    private static func bytesBucket(_ bytes: UInt64, quantum: UInt64 = 1_024 * 1_024) -> UInt64 {
        bytes / max(1, quantum)
    }
}

/// Marks a view as a consumer of the full SystemSnapshot decode. While at
/// least one such view is on screen, AppState fetches the full snapshot every
/// tick; otherwise the expensive FFI decode drops to the evaluator floor
/// cadence. Attach to the outermost content of every view that reads
/// entitiesState/timelineState/hostState-derived detail.
private struct FullSnapshotDemandModifier: ViewModifier {
    let state: AppState
    let reason: String
    @State private var token: UUID?

    func body(content: Content) -> some View {
        content
            .onAppear {
                if token == nil {
                    token = state.beginFullSnapshotDemand(reason: reason)
                }
            }
            .onDisappear {
                if let token {
                    state.endFullSnapshotDemand(token)
                }
                token = nil
            }
    }
}

extension View {
    public func demandsFullSnapshot(from state: AppState, reason: String = "unspecified") -> some View {
        modifier(FullSnapshotDemandModifier(state: state, reason: reason))
    }
}
