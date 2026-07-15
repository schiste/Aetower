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

/// Marks a view as a consumer of the full SystemSnapshot decode. While at
/// least one such view is on screen, AppState fetches the full snapshot every
/// tick; otherwise the expensive FFI decode drops to the evaluator floor
/// cadence. Attach to the outermost content of every view that reads
/// entitiesState/timelineState/hostState-derived detail.
private struct FullSnapshotDemandModifier: ViewModifier {
    let state: AppState
    @State private var token: UUID?

    func body(content: Content) -> some View {
        content
            .onAppear {
                if token == nil {
                    token = state.beginFullSnapshotDemand()
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
    public func demandsFullSnapshot(from state: AppState) -> some View {
        modifier(FullSnapshotDemandModifier(state: state))
    }
}
