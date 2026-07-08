import Foundation

enum RepositoryOptimizationSignalKind: String, CaseIterable, Codable, Sendable {
    case inventoryFreshness = "inventory-freshness"
    case budget
    case contractReadiness = "contract-readiness"
    case guidance
    case scorecard
    case githubProvider = "github-provider"
    case cloudflareProvider = "cloudflare-provider"
    case cloneGroup = "clone-group"
    case dirtyWorktree = "dirty-worktree"
    case storageGrowth = "storage-growth"
    case reviewItems = "review-items"
}

enum RepositoryOptimizationSeverity: Int, Comparable, Codable, Sendable {
    case warning = 1
    case critical = 2

    static func < (left: RepositoryOptimizationSeverity, right: RepositoryOptimizationSeverity) -> Bool {
        left.rawValue < right.rawValue
    }
}

enum RepositoryOptimizationActionKind: String, Codable, Equatable, Sendable {
    case refreshInventory
    case prepareContract
    case improveScorecard
    case runScorecard
    case reviewCleanup
    case reviewClones
    case reveal
}

struct RepositoryOptimizationInput: Codable, Equatable, Sendable {
    struct Identity: Codable, Equatable, Sendable {
        var id: String = ""
        var root: String = ""
        var name: String = ""
        var source: String? = nil
    }

    struct Inventory: Codable, Equatable, Sendable {
        var status: String = "unknown"
        var fingerprintChanged: Bool = false
        var notSeenInLatestScan: Bool = false

        var needsAttention: Bool {
            notSeenInLatestScan
                || fingerprintChanged
                || status == "changed"
                || status == "missing"
                || status == "legacy"
        }
    }

    struct Storage: Codable, Equatable, Sendable {
        var artifactBytes: UInt64 = 0
        var growthBytes: Int64? = nil
        var reviewItemCount: Int = 0
    }

    struct Git: Codable, Equatable, Sendable {
        var dirtyStatus: String = "unknown"
        var cloneGroupCount: UInt64 = 1
    }

    struct AgentGuidance: Codable, Equatable, Sendable {
        var readinessStatus: String = "unknown"
        var guidanceStatus: String = "unknown"
        var guidanceIssueCount: UInt64 = 0
        var qualityIssueCount: Int = 0

        var readinessNeedsAttention: Bool {
            readinessStatus == "blocked" || readinessStatus == "weak"
        }

        var guidanceNeedsAttention: Bool {
            guidanceIssueCount > 0 || qualityIssueCount > 0
        }
    }

    struct ProviderHealth: Codable, Equatable, Sendable {
        var scanned: Bool = false
        var attentionScore: Double = 0

        var hasAttention: Bool {
            attentionScore >= 5
        }
    }

    var identity: Identity = .init()
    var aggregateAttentionScore: Double = 0
    var budgetViolationCount: Int = 0
    var inventory: Inventory = .init()
    var storage: Storage = .init()
    var git: Git = .init()
    var agentGuidance: AgentGuidance = .init()
    var scorecard: ProviderHealth = .init()
    var github: ProviderHealth = .init()
    var cloudflare: ProviderHealth = .init()
}

struct RepositoryOptimizationSignal: Identifiable, Codable, Equatable, Sendable {
    let kind: RepositoryOptimizationSignalKind
    let severity: RepositoryOptimizationSeverity
    let priority: Int

    var id: String { kind.rawValue }
}

struct RepositoryOptimizationProfile: Codable, Equatable, Sendable {
    let signals: [RepositoryOptimizationSignal]
    let primaryActionKind: RepositoryOptimizationActionKind
    let requiresAttention: Bool
}

enum RepositoryOptimizationPlanner {
    static func profile(for input: RepositoryOptimizationInput) -> RepositoryOptimizationProfile {
        RepositoryOptimizationProfile(
            signals: signals(for: input),
            primaryActionKind: primaryActionKind(for: input),
            requiresAttention: requiresAttention(input)
        )
    }

    static func signals(for input: RepositoryOptimizationInput) -> [RepositoryOptimizationSignal] {
        var signals: [RepositoryOptimizationSignal] = []

        if input.inventory.needsAttention {
            signals.append(
                signal(
                    .inventoryFreshness,
                    input.inventory.status == "missing" ? .critical : .warning,
                    priority: 100
                )
            )
        }
        if input.budgetViolationCount > 0 {
            signals.append(signal(.budget, .critical, priority: 95))
        }
        if input.agentGuidance.readinessNeedsAttention {
            signals.append(
                signal(
                    .contractReadiness,
                    input.agentGuidance.readinessStatus == "blocked" ? .critical : .warning,
                    priority: 90
                )
            )
        }
        if input.agentGuidance.guidanceNeedsAttention {
            signals.append(
                signal(
                    .guidance,
                    input.agentGuidance.guidanceStatus == "error" ? .critical : .warning,
                    priority: 85
                )
            )
        }
        if input.scorecard.hasAttention {
            signals.append(
                signal(
                    .scorecard,
                    input.scorecard.attentionScore >= 10 ? .critical : .warning,
                    priority: 80
                )
            )
        }
        if input.github.hasAttention {
            signals.append(
                signal(
                    .githubProvider,
                    input.github.attentionScore >= 8 ? .critical : .warning,
                    priority: 75
                )
            )
        }
        if input.cloudflare.hasAttention {
            signals.append(
                signal(
                    .cloudflareProvider,
                    input.cloudflare.attentionScore >= 8 ? .critical : .warning,
                    priority: 70
                )
            )
        }
        if input.git.cloneGroupCount > 1 {
            signals.append(signal(.cloneGroup, .warning, priority: 65))
        }
        if input.git.dirtyStatus == "dirty" {
            signals.append(signal(.dirtyWorktree, .warning, priority: 60))
        }
        if (input.storage.growthBytes ?? 0) > 0 {
            signals.append(signal(.storageGrowth, .warning, priority: 55))
        }
        if input.storage.reviewItemCount > 0 {
            signals.append(signal(.reviewItems, .warning, priority: 50))
        }

        return signals.sorted { left, right in
            if left.priority != right.priority {
                return left.priority > right.priority
            }
            return left.kind.rawValue < right.kind.rawValue
        }
    }

    static func primaryActionKind(for input: RepositoryOptimizationInput) -> RepositoryOptimizationActionKind {
        if input.inventory.needsAttention {
            return .refreshInventory
        }
        if input.agentGuidance.readinessNeedsAttention || input.agentGuidance.guidanceNeedsAttention {
            return .prepareContract
        }
        if input.scorecard.hasAttention, input.scorecard.scanned {
            return .improveScorecard
        }
        if !input.scorecard.scanned {
            return .runScorecard
        }
        if input.storage.reviewItemCount > 0 || input.storage.artifactBytes > 0 {
            return .reviewCleanup
        }
        if input.git.cloneGroupCount > 1 {
            return .reviewClones
        }
        return .reveal
    }

    static func requiresAttention(_ input: RepositoryOptimizationInput) -> Bool {
        input.aggregateAttentionScore >= 8
            || input.inventory.needsAttention
            || input.budgetViolationCount > 0
            || input.storage.reviewItemCount > 0
            || input.agentGuidance.readinessNeedsAttention
            || input.agentGuidance.guidanceNeedsAttention
            || input.git.cloneGroupCount > 1
            || input.git.dirtyStatus == "dirty"
            || input.scorecard.hasAttention
            || input.github.hasAttention
            || input.cloudflare.hasAttention
    }

    private static func signal(
        _ kind: RepositoryOptimizationSignalKind,
        _ severity: RepositoryOptimizationSeverity,
        priority: Int
    ) -> RepositoryOptimizationSignal {
        RepositoryOptimizationSignal(kind: kind, severity: severity, priority: priority)
    }
}

extension RepositorySummary {
    var optimizationInput: RepositoryOptimizationInput {
        RepositoryOptimizationInput(
            identity: .init(
                id: id,
                root: root,
                name: name,
                source: "aetower"
            ),
            aggregateAttentionScore: attentionScore,
            budgetViolationCount: violationCount,
            inventory: .init(
                status: inventoryCacheStatus,
                fingerprintChanged: inventoryFingerprintChanged,
                notSeenInLatestScan: notSeenInLatestScan
            ),
            storage: .init(
                artifactBytes: artifactBytes,
                growthBytes: growthBytes,
                reviewItemCount: reviewItemCount
            ),
            git: .init(
                dirtyStatus: gitDirtyStatus,
                cloneGroupCount: cloneGroupCount
            ),
            agentGuidance: .init(
                readinessStatus: agentReadinessStatus,
                guidanceStatus: agentGuidanceStatus,
                guidanceIssueCount: agentGuidanceIssueCount,
                qualityIssueCount: qualityIssueCount
            ),
            scorecard: .init(
                scanned: scorecardReport != nil,
                attentionScore: scorecardAttentionScore
            ),
            github: .init(
                scanned: project?.githubStatus != nil,
                attentionScore: githubProviderAttentionScore
            ),
            cloudflare: .init(
                scanned: project?.cloudflareStatuses != nil,
                attentionScore: cloudflareProviderAttentionScore
            )
        )
    }

    var optimizationProfile: RepositoryOptimizationProfile {
        RepositoryOptimizationPlanner.profile(for: optimizationInput)
    }

    var optimizationSignals: [RepositoryOptimizationSignal] {
        optimizationProfile.signals
    }

    var primaryOptimizationActionKind: RepositoryOptimizationActionKind {
        optimizationProfile.primaryActionKind
    }
}
