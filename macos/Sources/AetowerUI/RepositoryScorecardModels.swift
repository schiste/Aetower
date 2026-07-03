import Foundation

struct RepositoryScorecardReportModel: Decodable, Sendable {
    let repoRoot: String
    let remoteUrl: String
    let provider: String
    let owner: String
    let repo: String
    let mode: String
    let requestedMode: String
    let status: String
    let score: Double?
    let checks: [RepositoryScorecardCheckModel]
    let failedChecks: [RepositoryScorecardCheckModel]
    let unavailableChecks: [RepositoryScorecardCheckModel]
    let recommendations: [RepositoryScorecardRecommendationModel]
    let commitSha: String?
    let scorecardVersion: String?
    let scorecardCommit: String?
    let sourceTimestamp: String?
    let cacheStatus: String
    let cacheKey: String?
    let cacheHit: Bool
    let cachedAtMillis: UInt64?
    let capturedAtMillis: UInt64
    let warnings: [String]

    private enum CodingKeys: String, CodingKey {
        case repoRoot
        case remoteUrl
        case provider
        case owner
        case repo
        case mode
        case requestedMode
        case status
        case score
        case checks
        case failedChecks
        case unavailableChecks
        case recommendations
        case commitSha
        case scorecardVersion
        case scorecardCommit
        case sourceTimestamp
        case cacheStatus
        case cacheKey
        case cacheHit
        case cachedAtMillis
        case capturedAtMillis
        case warnings
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        repoRoot = try container.decode(String.self, forKey: .repoRoot)
        remoteUrl = try container.decodeIfPresent(String.self, forKey: .remoteUrl) ?? ""
        provider = try container.decodeIfPresent(String.self, forKey: .provider) ?? ""
        owner = try container.decodeIfPresent(String.self, forKey: .owner) ?? ""
        repo = try container.decodeIfPresent(String.self, forKey: .repo) ?? ""
        mode = try container.decodeIfPresent(String.self, forKey: .mode) ?? "auto"
        requestedMode = try container.decodeIfPresent(String.self, forKey: .requestedMode) ?? mode
        status = try container.decodeIfPresent(String.self, forKey: .status) ?? "unknown"
        score = try container.decodeIfPresent(Double.self, forKey: .score)
        checks = try container.decodeIfPresent([RepositoryScorecardCheckModel].self, forKey: .checks) ?? []
        failedChecks =
            try container.decodeIfPresent([RepositoryScorecardCheckModel].self, forKey: .failedChecks) ?? []
        unavailableChecks =
            try container.decodeIfPresent([RepositoryScorecardCheckModel].self, forKey: .unavailableChecks) ?? []
        recommendations =
            try container.decodeIfPresent(
                [RepositoryScorecardRecommendationModel].self,
                forKey: .recommendations
            ) ?? []
        commitSha = try container.decodeIfPresent(String.self, forKey: .commitSha)
        scorecardVersion = try container.decodeIfPresent(String.self, forKey: .scorecardVersion)
        scorecardCommit = try container.decodeIfPresent(String.self, forKey: .scorecardCommit)
        sourceTimestamp = try container.decodeIfPresent(String.self, forKey: .sourceTimestamp)
        cacheStatus = try container.decodeIfPresent(String.self, forKey: .cacheStatus) ?? "not_used"
        cacheKey = try container.decodeIfPresent(String.self, forKey: .cacheKey)
        cacheHit = try container.decodeIfPresent(Bool.self, forKey: .cacheHit) ?? false
        cachedAtMillis = try container.decodeIfPresent(UInt64.self, forKey: .cachedAtMillis)
        capturedAtMillis = try container.decodeIfPresent(UInt64.self, forKey: .capturedAtMillis) ?? 0
        warnings = try container.decodeIfPresent([String].self, forKey: .warnings) ?? []
    }

    /// The engine flags a cache older than its fresh window as "stale" while
    /// still serving it. A supply-chain score can drift under a fixed commit,
    /// so the UI must show freshness rather than imply a fresh evaluation.
    var isStale: Bool { cacheStatus == "stale" }

    /// Relative "as of" label derived from when the score was cached; nil when
    /// this is a live (uncached) evaluation.
    func freshnessLabel(now: Date = Date()) -> String? {
        guard let cachedAtMillis, cachedAtMillis > 0 else { return nil }
        let age = now.timeIntervalSince1970 - Double(cachedAtMillis) / 1000.0
        return "as of \(Self.relativeAge(age))"
    }

    private static func relativeAge(_ seconds: TimeInterval) -> String {
        if seconds < 90 { return "just now" }
        if seconds < 3600 { return "\(Int(seconds / 60)) min ago" }
        if seconds < 86_400 { return "\(Int(seconds / 3600)) h ago" }
        return "\(Int(seconds / 86_400)) d ago"
    }
}

struct RepositoryScorecardCheckModel: Decodable, Identifiable, Sendable {
    let name: String
    let score: Double?
    let outcome: String
    let reason: String
    let details: [String]
    let documentationUrl: String?
    let documentationShort: String?

    var id: String {
        "\(name)-\(outcome)-\(score.map { String($0) } ?? "none")"
    }
}

struct RepositoryScorecardRecommendationModel: Decodable, Identifiable, Sendable {
    let category: String
    let checkName: String
    let severity: String
    let actionability: String
    let title: String
    let detail: String

    var id: String {
        "\(category)-\(checkName)-\(title)"
    }

    var severityRank: Int {
        switch severity.lowercased() {
        case "critical": return 3
        case "high": return 2
        case "medium": return 1
        default: return 0
        }
    }

    /// "local" recommendations are fixable in the working tree; "remote" ones
    /// need repository-host settings (branch protection, tokens).
    var isLocallyActionable: Bool {
        actionability.lowercased() == "local"
    }
}

/// Severity first (critical -> low), locally-actionable before remote within
/// a severity band, stable by title.
func orderedScorecardRecommendations(
    _ recommendations: [RepositoryScorecardRecommendationModel]
) -> [RepositoryScorecardRecommendationModel] {
    recommendations.sorted { left, right in
        if left.severityRank != right.severityRank {
            return left.severityRank > right.severityRank
        }
        if left.isLocallyActionable != right.isLocallyActionable {
            return left.isLocallyActionable
        }
        return left.title.localizedCaseInsensitiveCompare(right.title) == .orderedAscending
    }
}
