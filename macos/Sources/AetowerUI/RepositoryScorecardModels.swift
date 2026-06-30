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
}
