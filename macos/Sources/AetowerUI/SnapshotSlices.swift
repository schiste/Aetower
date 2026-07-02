import AetowerBridge
import Foundation
import SwiftUI

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
