import Foundation

enum RepositoryScorecardRemediationCategory: String, CaseIterable {
    case securityPolicy = "Security-Policy"
    case tokenPermissions = "Token-Permissions"
    case branchProtection = "Branch-Protection"
    case codeReview = "Code-Review"
    case pinnedDependencies = "Pinned-Dependencies"
    case dangerousWorkflow = "Dangerous-Workflow"
    case vulnerabilities = "Vulnerabilities"
    case sast = "SAST"
    case fuzzing = "Fuzzing"
    case maintained = "Maintained"

    static func category(for checkName: String) -> RepositoryScorecardRemediationCategory? {
        switch normalized(checkName) {
        case "security-policy":
            return .securityPolicy
        case "token-permissions":
            return .tokenPermissions
        case "branch-protection":
            return .branchProtection
        case "code-review":
            return .codeReview
        case "pinned-dependencies":
            return .pinnedDependencies
        case "dangerous-workflow":
            return .dangerousWorkflow
        case "vulnerabilities":
            return .vulnerabilities
        case "sast":
            return .sast
        case "fuzzing":
            return .fuzzing
        case "maintained":
            return .maintained
        default:
            return nil
        }
    }

    static func normalized(_ value: String) -> String {
        value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
            .replacingOccurrences(of: "_", with: "-")
            .replacingOccurrences(of: " ", with: "-")
    }

    var remediationSurface: String {
        switch self {
        case .securityPolicy:
            return "local SECURITY.md edits first; remote GitHub security settings only as manual checklist"
        case .tokenPermissions:
            return "local workflow permission edits plus remote GitHub default-token settings checklist"
        case .branchProtection:
            return "remote GitHub branch protection or ruleset checklist"
        case .codeReview:
            return "remote GitHub review settings checklist; local CODEOWNERS or contribution docs only when evidence supports it"
        case .pinnedDependencies:
            return "local dependency, workflow, or action pinning edits"
        case .dangerousWorkflow:
            return "local GitHub Actions workflow edits"
        case .vulnerabilities:
            return "local dependency updates or audit follow-up edits"
        case .sast:
            return "local static-analysis config or CI edits"
        case .fuzzing:
            return "local fuzz target, CI, or documentation edits"
        case .maintained:
            return "local maintenance docs/release hygiene plus remote repository maintenance checklist"
        }
    }
}

enum RepositoryScorecardRemediationPrompts {
    static func remediationPrompt(
        repository: AgentContractPromptContext,
        remote: String?,
        report: RepositoryScorecardReportModel,
        kit: AgentContractKitPromptContext? = nil
    ) -> String {
        """
        You are improving this repository's OpenSSF Scorecard posture through Aethyme remediation categories.

        Repository:
        - Name: \(repository.name)
        - Root: \(repository.root)
        - Remote: \(remote ?? "unavailable")
        - Branch: \(repository.branch)
        - HEAD: \(repository.head)
        - Worktree: \(repository.dirtyDetail ?? "unknown")

        Scorecard source:
        - Provider: \(report.provider.isEmpty ? "unknown" : report.provider)
        - Owner/repo: \(ownerRepoLabel(report))
        - Mode: \(report.mode)
        - Status: \(report.status)
        - Score: \(scoreLabel(report.score))
        - Failed checks: \(report.failedChecks.count)
        - Unavailable checks: \(report.unavailableChecks.count)

        Aethyme local reference kit:
        \(kitInstructions(kit))

        Prompt rules:
        - Use failed checks as evidence.
        - Do not fabricate GitHub settings.
        - Separate local file edits from remote GitHub settings.
        - Write PR-ready edits where possible.
        - Produce a manual checklist for settings that cannot be changed locally.

        Operating constraints:
        - Run `git status --short` first.
        - Preserve unrelated user changes.
        - Do not commit, push, open PRs, or change remote GitHub settings unless explicitly asked.
        - Do not claim branch protection, review requirements, repo rulesets, security advisories, or default workflow token settings are enabled unless local evidence proves it.
        - If a setting can only be changed in GitHub, leave it in the manual checklist with exact navigation/setting guidance and evidence still needed.
        - For local fixes, make focused PR-ready edits and include validation commands you ran or could not run.

        Aethyme remediation category map:
        \(categoryMap(report))

        Failed checks evidence:
        \(failedCheckEvidence(report))

        Unavailable checks:
        \(unavailableCheckEvidence(report))

        Required output:
        - Summary of the failed Scorecard evidence used.
        - Local PR-ready edits made, with files changed and rationale.
        - Remote GitHub settings checklist, separated from local edits.
        - Validation run and results.
        - Remaining risk or follow-up when evidence is incomplete.
        """
    }

    static func categories(for report: RepositoryScorecardReportModel) -> [RepositoryScorecardRemediationCategory] {
        var seen = Set<String>()
        var categories: [RepositoryScorecardRemediationCategory] = []
        for check in report.failedChecks {
            guard let category = RepositoryScorecardRemediationCategory.category(for: check.name),
                  seen.insert(category.rawValue).inserted else {
                continue
            }
            categories.append(category)
        }
        return categories
    }

    private static func categoryMap(_ report: RepositoryScorecardReportModel) -> String {
        if report.failedChecks.isEmpty {
            return "- No failed checks were reported. Do not invent remediation work; ask whether Scorecard should be refreshed."
        }
        return report.failedChecks.map { check in
            let category = RepositoryScorecardRemediationCategory.category(for: check.name)
            let categoryLabel = category?.rawValue ?? "Unmapped Scorecard"
            let surface = category?.remediationSurface ?? "inspect the finding and decide whether local edits or a manual checklist apply"
            return "- \(check.name) -> Aethyme remediation category: \(categoryLabel) | \(surface)"
        }
        .joined(separator: "\n")
    }

    private static func failedCheckEvidence(_ report: RepositoryScorecardReportModel) -> String {
        if report.failedChecks.isEmpty {
            return "- No failed checks were reported in this Scorecard payload."
        }
        return report.failedChecks.map { check in
            let recommendation = matchingRecommendation(for: check, in: report)
            let details = check.details.prefix(4).map { "  - Detail: \($0)" }.joined(separator: "\n")
            let documentation = check.documentationUrl.map { "  - Documentation: \($0)" } ?? nil
            return [
                "- Check: \(check.name)",
                "  - Aethyme remediation category: \(RepositoryScorecardRemediationCategory.category(for: check.name)?.rawValue ?? "Unmapped Scorecard")",
                "  - Score: \(scoreLabel(check.score))",
                "  - Outcome: \(check.outcome.isEmpty ? "unknown" : check.outcome)",
                "  - Reason: \(check.reason.isEmpty ? "not provided" : check.reason)",
                details.isEmpty ? nil : details,
                documentation,
                recommendation.map {
                    "  - Recommendation: [\($0.severity.uppercased())] \($0.title). \($0.detail) Actionability: \($0.actionability)."
                },
            ]
            .compactMap { $0 }
            .joined(separator: "\n")
        }
        .joined(separator: "\n")
    }

    private static func unavailableCheckEvidence(_ report: RepositoryScorecardReportModel) -> String {
        if report.unavailableChecks.isEmpty {
            return "- No unavailable checks were reported."
        }
        return report.unavailableChecks.map { check in
            "- \(check.name): \(check.reason.isEmpty ? "unavailable" : check.reason)"
        }
        .joined(separator: "\n")
    }

    private static func matchingRecommendation(
        for check: RepositoryScorecardCheckModel,
        in report: RepositoryScorecardReportModel
    ) -> RepositoryScorecardRecommendationModel? {
        let normalized = RepositoryScorecardRemediationCategory.normalized(check.name)
        return report.recommendations.first {
            RepositoryScorecardRemediationCategory.normalized($0.checkName) == normalized
        }
    }

    private static func kitInstructions(_ kit: AgentContractKitPromptContext?) -> String {
        guard let kit else {
            return "- No local Aethyme kit is attached to this copied prompt."
        }
        return """
        - Kit: \(kit.kitID) \(kit.version)
        - Relative path: \(kit.relativePath)
        - Absolute path: \(kit.absolutePath)
        - Fingerprint: \(kit.fingerprint)
        - Treat `.aethyme/` as local Aethyme tooling/cache, not as portable repository content.
        """
    }

    private static func ownerRepoLabel(_ report: RepositoryScorecardReportModel) -> String {
        if report.owner.isEmpty, report.repo.isEmpty {
            return "unknown"
        }
        return "\(report.owner)/\(report.repo)"
    }

    private static func scoreLabel(_ score: Double?) -> String {
        guard let score else { return "N/A" }
        return String(format: "%.1f", score)
    }
}
