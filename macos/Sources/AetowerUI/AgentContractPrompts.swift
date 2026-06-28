import Foundation

struct AgentContractPromptContext {
    let name: String
    let root: String
    let branch: String
    let head: String
    let dirtyDetail: String?
}

enum AgentContractPrompts {
    static func key(
        repositoryID: String,
        contract: StorageAgentContractCoverageModel,
        kind: String
    ) -> String {
        "\(repositoryID)::\(contract.id)::\(kind)"
    }

    static func generationPrompt(
        repository: AgentContractPromptContext,
        contract: StorageAgentContractCoverageModel,
        issues: [StorageAgentGuidanceIssueModel]
    ) -> String {
        """
        You are generating or updating one Aetower-compatible agent contract file.

        Repository:
        - Name: \(repository.name)
        - Root: \(repository.root)
        - Branch: \(repository.branch)
        - HEAD: \(repository.head)

        Target:
        - File: \(contract.path)
        - Contract: \(contract.label)
        - Current status: \(status(repository: repository, contract: contract))
        - Prompt guide: \(guide(for: contract))
        - Schema: \(schemaPath(for: contract))

        Scope:
        - Edit only \(contract.path).
        - Run `git status --short` first.
        - Preserve unrelated user changes.
        - Stay on the current branch and worktree.
        - Do not commit or push unless explicitly asked.

        Required evidence:
        \(evidence(for: contract))

        File requirements:
        \(checklist(for: contract))

        Current Aetower findings for this file:
        \(issueSummary(issues, contract: contract))

        Validation:
        \(validation(for: contract))
        """
    }

    static func reconcilePrompt(
        repository: AgentContractPromptContext,
        contract: StorageAgentContractCoverageModel,
        issues: [StorageAgentGuidanceIssueModel]
    ) -> String {
        """
        You are reconciling one Aetower-compatible agent contract file against the local repository.

        Repository root:
        \(repository.root)

        Target file:
        \(contract.path)

        Current Aetower status:
        \(status(repository: repository, contract: contract))

        Current Aetower findings for this file:
        \(issueSummary(issues, contract: contract))

        Instructions:
        - Run `git status --short` first.
        - Edit only \(contract.path).
        - Preserve unrelated user changes.
        - Compare the file against real local evidence. Do not invent paths, commands, risks, boundaries, references, or owners.
        - Remove stale claims that no longer match the repository.
        - Keep review-only fields unset unless a human actually reviewed the contract.
        - If uncertainty remains, leave a short review note rather than encoding a guess as fact.
        - Run the applicable validation:
        \(validation(for: contract))
        """
    }

    static func guide(for contract: StorageAgentContractCoverageModel) -> String {
        switch contract.id {
        case "agents_md":
            return "docs/agent-contract-prompts/01-agents-md.md"
        case "manifest":
            return "docs/agent-contract-prompts/02-manifest-yaml.md"
        case "tasks":
            return "docs/agent-contract-prompts/03-tasks-yaml.md"
        case "repo_map":
            return "docs/agent-contract-prompts/04-repo-map-yaml.md"
        case "contracts":
            return "docs/agent-contract-prompts/05-contracts-yaml.md"
        case "commands":
            return "docs/agent-contract-prompts/06-commands-yaml.md"
        case "validation":
            return "docs/agent-contract-prompts/07-validation-yaml.md"
        case "boundaries":
            return "docs/agent-contract-prompts/08-boundaries-yaml.md"
        case "risks":
            return "docs/agent-contract-prompts/09-risks-yaml.md"
        case "references":
            return "docs/agent-contract-prompts/10-references-yaml.md"
        default:
            return "docs/agent-contract-prompts/README.md"
        }
    }

    static func schemaPath(for contract: StorageAgentContractCoverageModel) -> String {
        switch contract.id {
        case "manifest":
            return ".agents/schema-v1/manifest.schema.json"
        case "tasks":
            return ".agents/schema-v1/tasks.schema.json"
        case "repo_map":
            return ".agents/schema-v1/repo-map.schema.json"
        case "contracts":
            return ".agents/schema-v1/contracts.schema.json"
        case "commands":
            return ".agents/schema-v1/commands.schema.json"
        case "validation":
            return ".agents/schema-v1/validation.schema.json"
        case "boundaries":
            return ".agents/schema-v1/boundaries.schema.json"
        case "risks":
            return ".agents/schema-v1/risks.schema.json"
        case "references":
            return ".agents/schema-v1/references.schema.json"
        default:
            return "Not required"
        }
    }

    private static func status(
        repository: AgentContractPromptContext,
        contract: StorageAgentContractCoverageModel
    ) -> String {
        [
            statusLabel(for: contract),
            "\(contract.coveragePercent)% coverage",
            contract.present ? "present" : "missing",
            contract.tracked ? "tracked" : "not tracked",
            repository.dirtyDetail,
        ]
        .compactMap { $0 }
        .joined(separator: " · ")
    }

    private static func statusLabel(for contract: StorageAgentContractCoverageModel) -> String {
        switch contract.status {
        case "ok":
            return "Ok"
        case "partial":
            return "Partial"
        case "missing":
            return "Missing"
        case "untracked":
            return "Untracked"
        case "error":
            return "Error"
        default:
            return contract.status.capitalized
        }
    }

    private static func issueSummary(
        _ issues: [StorageAgentGuidanceIssueModel],
        contract: StorageAgentContractCoverageModel
    ) -> String {
        if issues.isEmpty {
            return "- No current issue is attached to \(contract.path)."
        }
        return issues.map { issue in
            "- [\(issue.severity.uppercased())] \(issue.id): \(issue.title). \(issue.detail)"
        }
        .joined(separator: "\n")
    }

    private static func evidence(for contract: StorageAgentContractCoverageModel) -> String {
        switch contract.id {
        case "agents_md":
            return bulletList([
                "Inspect existing AGENTS.md, README.md, CONTRIBUTING.md, hooks, commands, validation docs, and `.agents/` files.",
                "Use `.agents/*.yaml` as the machine-checkable source when present.",
                "Keep root guidance short and defer deep context to references.",
            ])
        case "manifest":
            return bulletList([
                "Inspect `.agents/schema-v1/`, `.agents/*.yaml`, package manifests, hooks, scripts, and docs mentioning agent contracts.",
                "Reference only command IDs that exist in `.agents/commands.yaml`.",
                "Declare cross-file integrity checks that a real checker can enforce.",
            ])
        case "tasks":
            return bulletList([
                "Inspect AGENTS.md, repo map, contracts, validation rules, boundaries, risks, package manifests, hooks, tests, and recurring task patterns.",
                "Reference only real task routes, contract IDs, validation rule IDs, risk IDs, boundary IDs, and command IDs.",
                "Keep the file focused on classification, routing, required reads, strategy, validation, and completion reporting.",
            ])
        case "repo_map":
            return bulletList([
                "Inspect tracked top-level directories, workspace manifests, package manifests, app/service roots, scripts, docs, hooks, generated roots, and ignored roots.",
                "Map only roots that are real or intentionally generated.",
                "Connect roots to validation and boundary IDs when those files define them.",
            ])
        case "contracts":
            return bulletList([
                "Inspect architecture, API, security, privacy, release, storage, process-control, and test files that encode invariant behavior.",
                "Capture only invariants that are locally proven or explicitly marked for review.",
                "Keep rules compact, checkable, and connected to validation commands or validation rule IDs where possible.",
            ])
        case "commands":
            return bulletList([
                "Inspect package scripts, Makefile, justfile, direct scripts, hooks, CI, and release scripts.",
                "Prefer deterministic argv arrays; use shell commands only when shell semantics are needed.",
                "Encode approval, secrets, network, mutation, hook-safety, and runtime cost explicitly.",
            ])
        case "validation":
            return bulletList([
                "Inspect `.agents/commands.yaml`, hooks, CI, test layout, package manifests, and validation docs.",
                "Reference only command IDs that exist.",
                "Use manual-review-only rules where no reliable local command exists.",
            ])
        case "boundaries":
            return bulletList([
                "Inspect architecture docs, dependency configs, source imports, package boundaries, generated-code markers, and `.agents/repo-map.yaml`.",
                "Encode only rules that are locally provable or explicitly marked for review.",
                "Include rationale and safe alternatives for blocked dependency directions.",
            ])
        case "risks":
            return bulletList([
                "Inspect security/privacy docs, deploy scripts, migrations, auth, permissions, data export, dependency manifests, observability, CI, and hooks.",
                "Separate proven risks from review-needed candidates.",
                "Reference validation and safe read-only commands when available.",
            ])
        case "references":
            return bulletList([
                "Inspect README, CONTRIBUTING, docs, ADRs, runbooks, architecture notes, setup docs, release docs, security/privacy docs, and nested AGENTS.md files.",
                "Prefer canonical references and omit stale or duplicate docs.",
                "Add machine triggers where possible so agents load context only when relevant.",
            ])
        default:
            return bulletList(["Inspect local repository evidence before editing."])
        }
    }

    private static func checklist(for contract: StorageAgentContractCoverageModel) -> String {
        switch contract.id {
        case "agents_md":
            return bulletList([
                "Use the required AGENTS.md section order.",
                "State git workflow, approval rules, validation expectations, security rules, completion checklist, and references.",
                "Avoid broad git examples and stale local agent docs.",
            ])
        case "manifest":
            return bulletList([
                "List all expected contract files and schemas, including tasks and contracts.",
                "Define integrity rules for IDs, command references, source files, local references, and freshness.",
                "Avoid claiming generator/check command IDs before they exist.",
            ])
        case "tasks":
            return bulletList([
                "Define compact task classes with signals, strategy, required reads, likely paths, referenced contracts, risks, boundaries, validation rules, and completion report requirements.",
                "Prefer highest-risk task routing when signals overlap.",
                "Require uncertainty reporting rather than confident misclassification.",
            ])
        case "repo_map":
            return bulletList([
                "Describe roots, workspaces, entrypoints, generated roots, ignored roots, constraints, and owners where known.",
                "Flag agent-critical roots such as AGENTS.md, `.agents/`, hooks, CI, and release scripts.",
                "Keep cache/build folders classified as generated or ignored, not required source.",
            ])
        case "contracts":
            return bulletList([
                "Declare invariant IDs with category, rule, severity, paths, source of truth, forbidden operations, required tests, and safe alternatives where useful.",
                "Cover API/auth/data/security/release/storage/performance/process-control invariants that are real for this repo.",
                "Avoid long prose; agents need constraints and checks.",
            ])
        case "commands":
            return bulletList([
                "Give every command a stable ID, purpose, cwd, cost, mutation, approval, scope, timeout, shell, network, and hook-safety metadata.",
                "Describe secrets, outside-repo writes, remote effects, and interactive behavior.",
                "Check scripts and binaries exist.",
            ])
        case "validation":
            return bulletList([
                "Map path globs to exact command IDs.",
                "Include exclude paths, changed-files mode, risk IDs, boundary IDs, skip reporting, and full-suite triggers when applicable.",
                "Require downstream validation for shared surfaces.",
            ])
        case "boundaries":
            return bulletList([
                "Define layers, default policies, import/dependency rules, generated-code ownership, and app-neutrality rules when proven.",
                "Attach enforcing command IDs where available.",
                "Include rationale plus good and bad examples.",
            ])
        case "risks":
            return bulletList([
                "Declare high-risk surfaces, approval type, forbidden operations without approval, safe read-only commands, and required validation.",
                "Use manual review when local validation cannot prove safety.",
                "Do not overstate risk without evidence.",
            ])
        case "references":
            return bulletList([
                "List deeper context with kind, path or URI, title, summary, load triggers, applicable paths, task triggers, freshness, and canonical/supersedes metadata.",
                "Mark URI references that require network.",
                "Keep references actionable and avoid duplicating AGENTS.md.",
            ])
        default:
            return bulletList(["Follow the selected contract guide and schema."])
        }
    }

    private static func validation(for contract: StorageAgentContractCoverageModel) -> String {
        if schemaPath(for: contract) == "Not required" {
            return bulletList([
                "Check markdown fences are balanced.",
                "Check referenced local paths exist or are explicitly marked for review.",
                "Report changed files, validation performed, and residual risk.",
            ])
        }
        return bulletList([
            "Validate YAML against \(schemaPath(for: contract)) when available.",
            "Check referenced local paths and command IDs exist when the target file references them.",
            "Report changed files, validation performed, and residual risk.",
        ])
    }

    private static func bulletList(_ items: [String]) -> String {
        items.map { "- \($0)" }.joined(separator: "\n")
    }
}
