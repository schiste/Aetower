import Foundation

struct AgentContractPromptContext {
    let name: String
    let root: String
    let branch: String
    let head: String
    let dirtyDetail: String?
}

struct AgentContractKitPromptContext: Sendable, Equatable {
    let kitID: String
    let version: String
    let relativePath: String
    let absolutePath: String
    let fingerprint: String

    func templatePath(for contract: StorageAgentContractCoverageModel) -> String {
        "\(relativePath)/templates/\(templateFilename(for: contract))"
    }

    func schemaPath(for contract: StorageAgentContractCoverageModel) -> String? {
        guard let filename = schemaFilename(for: contract) else {
            return nil
        }
        return "\(relativePath)/schemas/\(filename)"
    }

    private func templateFilename(for contract: StorageAgentContractCoverageModel) -> String {
        switch contract.id {
        case "agents_md":
            return "AGENTS.md"
        case "repo_map":
            return "repo-map.yaml"
        default:
            return contract.path.split(separator: "/").last.map(String.init) ?? "\(contract.id).yaml"
        }
    }

    private func schemaFilename(for contract: StorageAgentContractCoverageModel) -> String? {
        switch contract.id {
        case "manifest":
            return "manifest.schema.json"
        case "tasks":
            return "tasks.schema.json"
        case "repo_map":
            return "repo-map.schema.json"
        case "contracts":
            return "contracts.schema.json"
        case "commands":
            return "commands.schema.json"
        case "validation":
            return "validation.schema.json"
        case "boundaries":
            return "boundaries.schema.json"
        case "risks":
            return "risks.schema.json"
        case "references":
            return "references.schema.json"
        default:
            return nil
        }
    }
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
        issues: [StorageAgentGuidanceIssueModel],
        kit: AgentContractKitPromptContext? = nil
    ) -> String {
        """
        You are generating or updating one portable agent operating contract file.

        Repository:
        - Name: \(repository.name)
        - Root: \(repository.root)
        - Branch: \(repository.branch)
        - HEAD: \(repository.head)

        Target:
        - File: \(contract.path)
        - Contract: \(contract.label)
        - Current status: \(status(repository: repository, contract: contract))

        This prompt is self-contained:
        - Do not assume this repository contains Aethyme docs, Aethyme schemas, or any prior `.agents/` files unless a local Aethyme kit is listed below.
        - Do not spend time searching for Aethyme prompt guides or schemas in this repository.
        - If the target file is under `.agents/`, create the `.agents/` directory if needed.
        - Use only local repository evidence, the Aethyme kit when listed, and the embedded contract spec below.

        Aethyme local reference kit:
        \(kitInstructions(kit, contract: contract))

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

        Embedded contract spec:
        \(embeddedSpec(for: contract))

        Current Aethyme findings for this file:
        \(issueSummary(issues, contract: contract))

        Validation:
        \(validation(for: contract, kit: kit))
        """
    }

    static func reconcilePrompt(
        repository: AgentContractPromptContext,
        contract: StorageAgentContractCoverageModel,
        issues: [StorageAgentGuidanceIssueModel],
        kit: AgentContractKitPromptContext? = nil
    ) -> String {
        """
        You are reconciling one portable agent operating contract file against the local repository.

        Repository root:
        \(repository.root)

        Target file:
        \(contract.path)

        Current Aethyme status:
        \(status(repository: repository, contract: contract))

        Current Aethyme findings for this file:
        \(issueSummary(issues, contract: contract))

        Aethyme local reference kit:
        \(kitInstructions(kit, contract: contract))

        Embedded contract spec:
        \(embeddedSpec(for: contract))

        Instructions:
        - Run `git status --short` first.
        - Edit only \(contract.path).
        - Do not assume this repository contains Aethyme docs, Aethyme schemas, or any prior `.agents/` files unless a local Aethyme kit is listed above.
        - Do not spend time searching for Aethyme prompt guides or schemas in this repository.
        - Preserve unrelated user changes.
        - Use only local repository evidence, the Aethyme kit when listed, and the embedded contract spec below.
        - Compare the file against real local evidence. Do not invent paths, commands, risks, boundaries, references, or owners.
        - Remove stale claims that no longer match the repository.
        - Do not claim completed human review. If the schema requires review fields and no human reviewed the contract, use `reviewed_by: pending-human-review`, a current ISO-8601 `reviewed_at`, and a note that review is pending.
        - If uncertainty remains, leave a short review note rather than encoding a guess as fact.
        - Run the applicable validation:
        \(validation(for: contract, kit: kit))
        """
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
                "Inspect existing `.agents/*.yaml` files if present, plus package manifests, hooks, scripts, README/CONTRIBUTING, CI, and docs that describe agent workflow.",
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

    private static func kitInstructions(
        _ kit: AgentContractKitPromptContext?,
        contract: StorageAgentContractCoverageModel
    ) -> String {
        guard let kit else {
            return bulletList([
                "No local Aethyme kit is attached to this copied prompt.",
                "Use the embedded spec below as the fallback contract shape.",
            ])
        }

        var items = [
            "Installed reference path: \(kit.relativePath).",
            "Template for this file: \(kit.templatePath(for: contract)).",
            "Read files under \(kit.relativePath) when useful, but do not edit or commit `.aethyme/`.",
            "Write the actual repository contract only to \(contract.path).",
            "Treat `.aethyme/` as local Aethyme tooling/cache, not as portable contract content.",
        ]
        if let schemaPath = kit.schemaPath(for: contract) {
            items.insert("Reference schema for this file: \(schemaPath).", at: 2)
        }
        return bulletList(items)
    }

    private static func validation(
        for contract: StorageAgentContractCoverageModel,
        kit: AgentContractKitPromptContext?
    ) -> String {
        if schemaFilename(for: contract) == nil {
            return bulletList([
                "Check markdown fences are balanced.",
                "Check referenced local paths exist or are explicitly marked for review.",
                "Report changed files, validation performed, and residual risk.",
            ])
        }
        var items = [
            "Check YAML parses and uses the embedded shape above.",
            "Compare shape against the Aethyme reference schema at \(kit?.schemaPath(for: contract) ?? "the local Aethyme kit schema") when useful.",
            "If this repo already has a local contract checker command, run it. Otherwise report that schema validation was unavailable.",
            "Check referenced local paths and command IDs exist when the target file references them.",
            "Report changed files, validation performed, and residual risk.",
        ]
        if kit == nil {
            items.insert("No local Aethyme schema path is attached to this copied prompt.", at: 1)
        }
        return bulletList(items)
    }

    private static func schemaFilename(for contract: StorageAgentContractCoverageModel) -> String? {
        switch contract.id {
        case "manifest":
            return "manifest.schema.json"
        case "tasks":
            return "tasks.schema.json"
        case "repo_map":
            return "repo-map.schema.json"
        case "contracts":
            return "contracts.schema.json"
        case "commands":
            return "commands.schema.json"
        case "validation":
            return "validation.schema.json"
        case "boundaries":
            return "boundaries.schema.json"
        case "risks":
            return "risks.schema.json"
        case "references":
            return "references.schema.json"
        default:
            return nil
        }
    }

    private static func embeddedSpec(for contract: StorageAgentContractCoverageModel) -> String {
        switch contract.id {
        case "agents_md":
            return bulletList([
                "Create a concise root AGENTS.md operating contract for coding agents.",
                "Use these sections in order: Scope And Precedence, Repository Map, Standard Workflow, Commands, Approval Required, Validation Matrix, Architecture Boundaries, Code Rules, Security Rules, Completion Checklist, References.",
                "Keep it short enough to read every session. Prefer links to deeper docs over copying long explanations.",
                "State exact git behavior: check `git status --short`, preserve unrelated user changes, use targeted staging, do not switch branches, do not commit or push unless explicitly asked.",
                "List exact validation commands that actually exist, or mark unknown validation as pending review.",
                "Do not include broad git examples such as `git add .`, `git add -A`, or `git commit -a`.",
            ])
        case "manifest":
            return yamlSpec(
                purpose: "Root index for the agent operating contract files, schema/version metadata, freshness policy, and cross-file integrity expectations.",
                requiredTopLevel: ["schema_version", "contracts", "integrity"],
                fields: [
                    "`schema_version: 1`.",
                    "`contracts`: list entries for AGENTS.md, .agents/manifest.yaml, .agents/tasks.yaml, .agents/repo-map.yaml, .agents/contracts.yaml, .agents/commands.yaml, .agents/validation.yaml, .agents/boundaries.yaml, .agents/risks.yaml, and .agents/references.yaml.",
                    "Each contract entry should include `id`, `path`, `schema`, `required`, and `generated`; add `review_required`, `source_files`, `owner`, or `description` only when supported by evidence.",
                    "`integrity`: include booleans or lists for unique IDs, command references, source-file existence, local reference existence, generated freshness, schema validation, and stale-reference policy.",
                    "`freshness`: optional; include only if this repo has real freshness/checking behavior.",
                    "Command IDs such as `generate_command_id`, `check_command_id`, and `explain_command_id` must be omitted unless those command IDs already exist in `.agents/commands.yaml`.",
                    "Use `notes` for pending human review, missing generators, or intentionally absent commands.",
                ]
            )
        case "tasks":
            return yamlSpec(
                purpose: "Task classification and routing contract for agents.",
                requiredTopLevel: ["schema_version", "reviewed_by", "reviewed_at", "source_files", "tasks"],
                fields: [
                    "`tasks`: list task objects with stable `id`, `description`, `signals`, `strategy`, and `validation_rule_ids`.",
                    "Signals should be structured around phrases, path globs, command IDs, risk IDs, contract IDs, or domain IDs.",
                    "Strategies should be compact values such as classify_first, reproduce_first, smallest_correct_patch, prefer_existing_patterns, add_regression_test, avoid_broad_refactor, ask_before_architecture_change, or manual_review_required.",
                    "Add `required_reads`, `likely_paths`, `required_contract_ids`, `risk_surface_ids`, `boundary_rule_ids`, and `required_command_ids` only when real local evidence supports them.",
                    "Add a classification policy that prefers highest-risk matches and requires uncertainty reporting when task type is unclear.",
                    "If no human review happened, use `reviewed_by: pending-human-review`, a current ISO-8601 `reviewed_at`, and a note explaining review is pending.",
                ]
            )
        case "repo_map":
            return yamlSpec(
                purpose: "Machine-readable repository topology for agents.",
                requiredTopLevel: ["schema_version", "generated_by", "source_files", "roots"],
                fields: [
                    "Describe major roots, workspaces, packages, services, entrypoints, generated roots, ignored roots, hooks, CI, assets, migrations, fixtures, and agent-critical files.",
                    "Each root should have a stable ID or path, kind, description, paths/globs, ownership if known, constraints, source-of-truth notes, local AGENTS.md if present, validation rule IDs, and boundary layer ID when known.",
                    "Use only paths that exist, or explicitly label intentionally generated/ignored paths.",
                    "Do not require cache/build output directories as source roots.",
                ]
            )
        case "contracts":
            return yamlSpec(
                purpose: "Repository invariants agents must preserve.",
                requiredTopLevel: ["schema_version", "reviewed_by", "reviewed_at", "source_files", "contracts"],
                fields: [
                    "`contracts`: list invariant objects with `id`, `category`, `rule`, `paths`, and `severity`.",
                    "Use categories such as api, auth, authorization, database, data-integrity, data-privacy, error-shape, observability, performance, process-control, release, security, storage, tenant-isolation, ui, or other.",
                    "Add source of truth, owner, forbidden operations, required tests, validation rule IDs, field shape, safe alternatives, and good/bad examples only when backed by local evidence.",
                    "Focus on invariants that would cause real regressions if broken.",
                    "If no human review happened, use `reviewed_by: pending-human-review`, a current ISO-8601 `reviewed_at`, and a note explaining review is pending.",
                ]
            )
        case "commands":
            return yamlSpec(
                purpose: "Canonical command registry for agents.",
                requiredTopLevel: ["schema_version", "generated_by", "source_files", "commands"],
                fields: [
                    "`commands`: list commands with stable `id`, `purpose`, `cwd`, `cost`, `mutates_files`, `needs_approval`, `scope`, and `timeout_seconds`.",
                    "Prefer `argv` arrays. Use `shell_command` only when shell semantics are required, and set `uses_shell` accordingly.",
                    "Encode `interactive`, `requires_network`, `requires_secrets`, `writes_outside_repo`, `external_effects`, `allowed_in_hooks`, and `approval_reason` when relevant.",
                    "Only list commands that actually exist in package manifests, Makefiles, justfiles, scripts, hooks, CI, or documented local workflows.",
                    "Do not invent validation commands to make another contract look complete.",
                ]
            )
        case "validation":
            return yamlSpec(
                purpose: "Map touched paths, task types, and risks to required checks.",
                requiredTopLevel: ["schema_version", "reviewed_by", "reviewed_at", "source_files", "rules"],
                fields: [
                    "`rules`: list validation rules with stable `id`, description, path globs or task/risk triggers, command IDs, and skip/reporting requirements.",
                    "Use `exclude_paths`, changed-files mode, max cost without approval, risk IDs, boundary IDs, and manual-review-only flags when applicable.",
                    "`full_suite_triggers` should include both trigger globs and the command IDs to run.",
                    "Reference only command IDs that exist in `.agents/commands.yaml`; if commands are missing, create manual-review-only rules and note the gap.",
                    "If no human review happened, use `reviewed_by: pending-human-review`, a current ISO-8601 `reviewed_at`, and a note explaining review is pending.",
                ]
            )
        case "boundaries":
            return yamlSpec(
                purpose: "Architecture and ownership boundaries agents must not violate.",
                requiredTopLevel: ["schema_version", "reviewed_by", "reviewed_at", "source_files", "layers", "rules"],
                fields: [
                    "Define layers/domains with IDs, path globs, default policy, and descriptions.",
                    "Rules should cover forbidden imports/dependencies, generated-code ownership, layer direction, subtree ownership, and non-import boundaries proven by local evidence.",
                    "Each rule should include rationale, severity, path globs or layers, safe alternatives, enforcing command IDs when available, and good/bad examples where useful.",
                    "Do not invent architecture policy. Mark uncertain boundaries as review notes.",
                    "If no human review happened, use `reviewed_by: pending-human-review`, a current ISO-8601 `reviewed_at`, and a note explaining review is pending.",
                ]
            )
        case "risks":
            return yamlSpec(
                purpose: "High-risk surfaces and failure modes agents must slow down for.",
                requiredTopLevel: ["schema_version", "reviewed_by", "reviewed_at", "source_files", "surfaces"],
                fields: [
                    "`surfaces`: list risk surfaces with stable `id`, category, risk, paths, approval type, forbidden operations, safe read-only commands, validation command IDs, and manual-review requirements when applicable.",
                    "Use categories such as auth, authorization, tenant-isolation, pii, data-export, ai-llm, dependencies, ci, hooks, infra, observability, generated-code, migration, deploy, billing, webhooks, or other.",
                    "Do not overstate risk without evidence; explain common failure modes only when visible in this repo.",
                    "Validation commands are optional when no reliable local command exists; require manual review instead.",
                    "If no human review happened, use `reviewed_by: pending-human-review`, a current ISO-8601 `reviewed_at`, and a note explaining review is pending.",
                ]
            )
        case "references":
            return yamlSpec(
                purpose: "Pointers to deeper context that agents should load only when relevant.",
                requiredTopLevel: ["schema_version", "generated_by", "source_files", "references"],
                fields: [
                    "`references`: list docs/specs/runbooks with stable `id`, kind, title, summary, path or URI, load triggers, applicable paths, task triggers, freshness, canonical status, and supersedes metadata where useful.",
                    "Use kinds such as agent, workflow, architecture, security, privacy, api, runbook, adr, configuration, schema, contract, onboarding, product, or external.",
                    "Mark URI references that require network.",
                    "Prefer canonical docs and omit stale duplicates.",
                    "Do not duplicate AGENTS.md; reference it as the operating contract.",
                ]
            )
        default:
            return bulletList([
                "Use local evidence only.",
                "Keep the file compact, machine-checkable where possible, and explicit about uncertainty.",
            ])
        }
    }

    private static func yamlSpec(
        purpose: String,
        requiredTopLevel: [String],
        fields: [String]
    ) -> String {
        bulletList([
            "YAML file purpose: \(purpose)",
            "Required top-level keys: \(requiredTopLevel.joined(separator: ", ")).",
            "Use `schema_version: 1` when this is a `.agents/*.yaml` contract.",
            "Allow repo-specific extension keys only when prefixed with `x_`.",
            "Use relative repo paths. Do not use absolute paths, `~`, or parent traversal.",
        ] + fields)
    }

    private static func bulletList(_ items: [String]) -> String {
        items.map { "- \($0)" }.joined(separator: "\n")
    }
}
