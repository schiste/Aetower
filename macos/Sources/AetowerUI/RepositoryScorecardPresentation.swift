import Foundation

enum RepositoryScorecardPresentation {
    static func scoreLabel(_ report: RepositoryScorecardReportModel?) -> String {
        guard let score = report?.score, score >= 0 else { return "N/A" }
        return String(format: "%.1f", score)
    }

    static func sourceLabel(_ report: RepositoryScorecardReportModel?) -> String {
        guard let report else { return "Not run" }
        switch report.mode {
        case "public_api":
            return "API"
        case "live_cli":
            return "CLI"
        case "auto":
            return "Auto"
        default:
            return report.mode.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    static func stateLabel(
        report: RepositoryScorecardReportModel?,
        isLoading: Bool,
        error: String?
    ) -> String {
        if isLoading { return "Scanning" }
        if let error, !error.isEmpty { return "Failed" }
        guard let report else { return "Not scanned" }
        switch report.status {
        case "ok":
            return "Cached"
        case "auth_required":
            return "Needs GitHub auth"
        case "unsupported":
            return "Unsupported remote"
        case "timeout":
            return "Timed out"
        default:
            return "Failed"
        }
    }

    static func statusDetail(
        report: RepositoryScorecardReportModel?,
        isLoading: Bool,
        error: String?
    ) -> String {
        if isLoading, report != nil {
            return "Refreshing Scorecard; the previous result remains visible."
        }
        if isLoading {
            return "Running Scorecard for this repository."
        }
        if let error, !error.isEmpty {
            return error
        }
        guard let report else {
            return "Run Scorecard on demand; the default repository scan does not call the API or CLI."
        }
        if let message = reportStateMessage(report) {
            return message
        }

        var parts: [String] = []
        if !report.owner.isEmpty, !report.repo.isEmpty {
            parts.append("\(report.owner)/\(report.repo)")
        }
        parts.append("\(report.failedChecks.count) failed")
        parts.append("\(report.unavailableChecks.count) unavailable")
        parts.append("\(report.recommendations.count) recommendation\(report.recommendations.count == 1 ? "" : "s")")
        return parts.joined(separator: " · ")
    }

    static func reportStateMessage(_ report: RepositoryScorecardReportModel) -> String? {
        switch report.status {
        case "ok":
            return nil
        case "auth_required":
            return "The Scorecard source requires GitHub authentication or a token with enough access for this repository."
        case "unsupported":
            return "Scorecard integration currently supports GitHub remotes; this repository remote is not supported."
        case "timeout":
            return "The Scorecard request did not finish before the configured timeout."
        case "invalid_repo":
            return "The repository root could not be validated as a local Git repository."
        case "rate_limited":
            return "The public Scorecard API rate limit was reached. Refresh later or use the local CLI mode."
        case "source_unavailable":
            return "No Scorecard source was available. Install the Scorecard CLI or use a public GitHub repository."
        case "parse_error":
            return "Scorecard returned data that could not be parsed into the repository report."
        default:
            return "Scorecard finished with status \(report.status.replacingOccurrences(of: "_", with: " "))."
        }
    }

    static func checkScoreLabel(_ check: RepositoryScorecardCheckModel) -> String {
        guard let score = check.score, score >= 0 else {
            return check.outcome.isEmpty ? "N/A" : check.outcome.capitalized
        }
        return String(format: "%.1f", score)
    }
}
