import AetowerBridge
import Foundation

/// Rarely-changing agent context (Chau7 sessions + AI repo summaries) grouped
/// into one change-gated slice so its readers are only invalidated when the
/// agent landscape actually moves, not on every engine tick.
public struct AgentContextSlice: Equatable, Sendable {
    public var chau7Sessions: [Chau7SessionSummary]
    public var aiRepoSummaries: [AiRepoSummary]

    public init(
        chau7Sessions: [Chau7SessionSummary] = [],
        aiRepoSummaries: [AiRepoSummary] = []
    ) {
        self.chau7Sessions = chau7Sessions
        self.aiRepoSummaries = aiRepoSummaries
    }
}
