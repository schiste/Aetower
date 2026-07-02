use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct AgentGuidanceAudit {
    pub(super) status: String,
    pub(super) issues: Vec<StorageAgentGuidanceIssue>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AgentContractDefinition {
    id: &'static str,
    label: &'static str,
    path: &'static str,
    kind: &'static str,
    pub(super) weight: u64,
    missing_severity: &'static str,
    requires_schema: bool,
    requires_review: bool,
    detail: &'static str,
}

#[derive(Clone, Debug, Default)]
pub(super) struct AgentContractCoverageAudit {
    pub(super) score: u8,
    pub(super) status: String,
    pub(super) missing_count: u64,
    pub(super) coverage: Vec<StorageAgentContractCoverage>,
    pub(super) issues: Vec<StorageAgentGuidanceIssue>,
}

pub(super) fn agent_guidance_audit(
    repo_root: &Path,
    quality: &RepositoryQuality,
    tracked_agent_paths: &BTreeSet<String>,
) -> AgentGuidanceAudit {
    let mut issues = Vec::new();
    let agents_path = repo_root.join("AGENTS.md");

    if !quality.has_agents_md {
        push_guidance_issue(
            &mut issues,
            "agents.root.missing",
            "error",
            "Missing root AGENTS.md",
            "Every repository should have a tracked root AGENTS.md as the canonical agent contract.",
            "AGENTS.md",
        );
    } else {
        if !tracked_agent_paths.contains("AGENTS.md") {
            push_guidance_issue(
                &mut issues,
                "agents.root.untracked",
                "error",
                "Root AGENTS.md is not tracked",
                "A referenced agent contract must be committed, not only present in the local worktree.",
                "AGENTS.md",
            );
        }
        if let Ok(content) = fs::read_to_string(&agents_path) {
            audit_agents_markdown(&mut issues, repo_root, "AGENTS.md", &content);
        }
    }

    if quality.has_claude_md && !quality.claude_md_delegates_to_agents_md {
        let detail = if quality
            .claude_md_bytes
            .is_some_and(|bytes| bytes > CLAUDE_MD_DELEGATION_MAX_BYTES)
        {
            "CLAUDE.md is too large to be treated as a delegating adapter."
        } else {
            "CLAUDE.md should delegate to AGENTS.md with @AGENTS.md as its first non-empty line."
        };
        push_guidance_issue(
            &mut issues,
            "agents.adapter.claude_not_delegated",
            "warning",
            "CLAUDE.md does not delegate cleanly",
            detail,
            "CLAUDE.md",
        );
    }

    audit_canonical_agent_links(&mut issues, repo_root, "README.md");
    audit_canonical_agent_links(&mut issues, repo_root, "CONTRIBUTING.md");

    let status = guidance_status(&issues);
    AgentGuidanceAudit { status, issues }
}

pub(super) fn agent_contract_definitions() -> &'static [AgentContractDefinition] {
    &[
        AgentContractDefinition {
            id: "agents_md",
            label: "AGENTS.md",
            path: "AGENTS.md",
            kind: "human-contract",
            weight: 22,
            missing_severity: "error",
            requires_schema: false,
            requires_review: false,
            detail: "Human-readable operating contract: precedence, workflow, git discipline, approvals, and completion rules.",
        },
        AgentContractDefinition {
            id: "manifest",
            label: "Manifest",
            path: ".agents/manifest.yaml",
            kind: "machine-contract",
            weight: 6,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Root machine contract: expected files, schema ids, generator/check commands, cross-file integrity, and freshness policy.",
        },
        AgentContractDefinition {
            id: "tasks",
            label: "Tasks",
            path: ".agents/tasks.yaml",
            kind: "reviewed-contract",
            weight: 12,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Decision layer: task classification, required reads, likely paths, strategy, validation routing, and completion reporting.",
        },
        AgentContractDefinition {
            id: "repo_map",
            label: "Repo map",
            path: ".agents/repo-map.yaml",
            kind: "machine-contract",
            weight: 9,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Machine-readable topology: roots, packages, services, entrypoints, generated folders, and ignored roots.",
        },
        AgentContractDefinition {
            id: "contracts",
            label: "Contracts",
            path: ".agents/contracts.yaml",
            kind: "reviewed-contract",
            weight: 14,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Invariant layer: API, auth, data, release, storage, process-control, performance, UI, and security contracts agents must not break.",
        },
        AgentContractDefinition {
            id: "commands",
            label: "Commands",
            path: ".agents/commands.yaml",
            kind: "machine-contract",
            weight: 10,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Exact command registry with cwd, cost, mutation, approval, breadth, and expected runtime metadata.",
        },
        AgentContractDefinition {
            id: "validation",
            label: "Validation",
            path: ".agents/validation.yaml",
            kind: "reviewed-contract",
            weight: 12,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Touched-path to validation mapping so agents know which checks prove a change safe.",
        },
        AgentContractDefinition {
            id: "boundaries",
            label: "Boundaries",
            path: ".agents/boundaries.yaml",
            kind: "reviewed-contract",
            weight: 7,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "Architecture rules agents and tools can check: layers, forbidden imports, generated-code ownership.",
        },
        AgentContractDefinition {
            id: "risks",
            label: "Risks",
            path: ".agents/risks.yaml",
            kind: "reviewed-contract",
            weight: 6,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: true,
            detail: "High-risk surfaces: auth, billing, permissions, migrations, secrets, deploy, tenants, webhooks.",
        },
        AgentContractDefinition {
            id: "references",
            label: "References",
            path: ".agents/references.yaml",
            kind: "machine-contract",
            weight: 2,
            missing_severity: "warning",
            requires_schema: true,
            requires_review: false,
            detail: "Pointers to deeper context without loading it by default: architecture, runbooks, standards, API docs.",
        },
    ]
}

pub(super) fn agent_contract_candidate_paths() -> Vec<&'static str> {
    let mut paths = Vec::with_capacity(agent_contract_definitions().len());
    for definition in agent_contract_definitions() {
        paths.push(definition.path);
    }
    paths
}

pub(super) fn agent_contract_coverage_audit(
    repo_root: &Path,
    quality: &RepositoryQuality,
    guidance: &AgentGuidanceAudit,
    tracked_agent_paths: &BTreeSet<String>,
) -> AgentContractCoverageAudit {
    let mut audit = AgentContractCoverageAudit::default();
    let mut earned_score = 0_u64;

    for definition in agent_contract_definitions() {
        let evaluated = evaluate_agent_contract(
            repo_root,
            quality,
            guidance,
            definition,
            tracked_agent_paths,
        );
        earned_score = earned_score.saturating_add(evaluated.earned_weight);
        if !evaluated.present {
            audit.missing_count = audit.missing_count.saturating_add(1);
        }
        if let Some(issue) = agent_contract_issue(definition, &evaluated) {
            audit.issues.push(issue);
        }
        audit.coverage.push(evaluated);
    }

    audit.score = earned_score.min(AGENT_READINESS_MAX_SCORE) as u8;
    audit.status = agent_readiness_status(audit.score, guidance, &audit.coverage);
    audit
}

fn evaluate_agent_contract(
    repo_root: &Path,
    quality: &RepositoryQuality,
    guidance: &AgentGuidanceAudit,
    definition: &AgentContractDefinition,
    tracked_agent_paths: &BTreeSet<String>,
) -> StorageAgentContractCoverage {
    let path = repo_root.join(definition.path);
    let present = path.is_file();
    let tracked = present && tracked_agent_paths.contains(definition.path);
    let mut status = "missing".to_owned();
    let mut severity = definition.missing_severity.to_owned();
    let mut detail = format!("Missing {}. {}", definition.path, definition.detail);
    let mut earned_weight = 0_u64;
    let mut schema_version = None;
    let mut generated = false;
    let mut reviewed = false;

    if present {
        let content = fs::read_to_string(&path).unwrap_or_default();
        schema_version =
            yaml_scalar(&content, "schema_version").or_else(|| yaml_scalar(&content, "version"));
        let contract_shape_issues = if definition.requires_schema {
            yaml_contract_shape_issues(repo_root, definition, &content)
        } else {
            Vec::new()
        };
        generated = yaml_boolish(&content, "generated")
            || yaml_scalar(&content, "generated_by").is_some()
            || content.contains("source_files:");
        reviewed = yaml_boolish(&content, "reviewed")
            || has_completed_human_review(&content)
            || yaml_scalar(&content, "last_reviewed").is_some();

        if !tracked {
            status = "untracked".to_owned();
            severity = if definition.path == "AGENTS.md" {
                "error".to_owned()
            } else {
                "warning".to_owned()
            };
            detail = format!(
                "{} exists but is not tracked by git. Agent contracts must be committed to be reliable.",
                definition.path
            );
            earned_weight = definition.weight / 3;
        } else if definition.path == "AGENTS.md" {
            let has_errors = guidance
                .issues
                .iter()
                .any(|issue| issue.severity == "error" && issue.path == "AGENTS.md");
            let has_warnings = guidance
                .issues
                .iter()
                .any(|issue| issue.severity == "warning" && issue.path == "AGENTS.md");
            if !quality.has_agents_md || has_errors {
                status = "error".to_owned();
                severity = "error".to_owned();
                detail = "AGENTS.md exists but has blocking contract issues.".to_owned();
                earned_weight = definition.weight / 3;
            } else if has_warnings {
                status = "partial".to_owned();
                severity = "warning".to_owned();
                detail = "AGENTS.md is tracked but has shape, reference, or workflow warnings."
                    .to_owned();
                earned_weight = definition.weight.saturating_mul(3) / 4;
            } else {
                status = "ok".to_owned();
                severity = "ok".to_owned();
                detail =
                    "AGENTS.md is present, tracked, and passes current portable checks.".to_owned();
                earned_weight = definition.weight;
            }
        } else {
            let schema_missing = definition.requires_schema && schema_version.is_none();
            let review_missing = definition.requires_review && !reviewed;
            let shape_invalid = !contract_shape_issues.is_empty();
            if schema_missing || review_missing || shape_invalid {
                status = "partial".to_owned();
                severity = "warning".to_owned();
                let mut gaps = Vec::new();
                if schema_missing {
                    gaps.push("schema_version/version".to_owned());
                }
                if review_missing {
                    gaps.push("reviewed_by/reviewed_at".to_owned());
                }
                if shape_invalid {
                    gaps.push(format!(
                        "shape checks ({})",
                        contract_shape_issues.join("; ")
                    ));
                }
                detail = format!(
                    "{} is present and tracked but has contract gaps: {}.",
                    definition.path,
                    gaps.join("; ")
                );
                earned_weight = definition.weight.saturating_mul(2) / 3;
            } else {
                status = "ok".to_owned();
                severity = "ok".to_owned();
                detail = format!(
                    "{} is present, tracked, and machine-checkable.",
                    definition.path
                );
                earned_weight = definition.weight;
            }
        }
    }

    let coverage_percent = if definition.weight == 0 {
        0
    } else {
        (earned_weight.saturating_mul(100) / definition.weight).min(100) as u8
    };

    StorageAgentContractCoverage {
        id: definition.id.to_owned(),
        label: definition.label.to_owned(),
        path: definition.path.to_owned(),
        kind: definition.kind.to_owned(),
        status,
        severity,
        detail,
        weight: definition.weight,
        earned_weight,
        coverage_percent,
        present,
        tracked,
        schema_version,
        generated,
        reviewed,
    }
}

fn agent_contract_issue(
    definition: &AgentContractDefinition,
    coverage: &StorageAgentContractCoverage,
) -> Option<StorageAgentGuidanceIssue> {
    if coverage.status == "ok" || definition.path == "AGENTS.md" {
        return None;
    }

    let id = format!("agents.contract.{}.{}", coverage.status, definition.id);
    Some(StorageAgentGuidanceIssue {
        id,
        severity: coverage.severity.clone(),
        title: match coverage.status.as_str() {
            "missing" => format!("Missing {}", definition.label),
            "untracked" => format!("{} is not tracked", definition.label),
            "partial" => format!("{} coverage is partial", definition.label),
            _ => format!("{} needs review", definition.label),
        },
        detail: coverage.detail.clone(),
        path: definition.path.to_owned(),
    })
}

fn agent_readiness_status(
    score: u8,
    guidance: &AgentGuidanceAudit,
    coverage: &[StorageAgentContractCoverage],
) -> String {
    if guidance
        .issues
        .iter()
        .any(|issue| issue.severity == "error")
        || coverage
            .iter()
            .any(|item| item.path == "AGENTS.md" && item.severity == "error")
    {
        return "blocked".to_owned();
    }
    if score >= 90 {
        "ready".to_owned()
    } else if score >= 60 {
        "partial".to_owned()
    } else {
        "weak".to_owned()
    }
}

fn yaml_scalar(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix(&prefix)?.trim();
        if value.is_empty() || value == "|" || value == ">" {
            return None;
        }
        Some(value.trim_matches('"').trim_matches('\'').to_owned())
    })
}

fn yaml_boolish(content: &str, key: &str) -> bool {
    yaml_scalar(content, key)
        .map(|value| {
            let normalized = value.to_lowercase();
            matches!(normalized.as_str(), "true" | "yes" | "1")
        })
        .unwrap_or(false)
}

fn has_completed_human_review(content: &str) -> bool {
    let reviewed_by = yaml_scalar(content, "reviewed_by")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let reviewed_at = yaml_scalar(content, "reviewed_at")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    !reviewed_by.is_empty()
        && reviewed_by != "pending-human-review"
        && reviewed_by != "pending_human_review"
        && reviewed_by != "pending"
        && !reviewed_at.is_empty()
        && reviewed_at != "pending-human-review"
        && reviewed_at != "pending_human_review"
        && reviewed_at != "pending"
}

fn yaml_contract_shape_issues(
    repo_root: &Path,
    definition: &AgentContractDefinition,
    content: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    if content.trim().is_empty() {
        issues.push("empty file".to_owned());
        return issues;
    }
    if content.lines().any(|line| line.starts_with('\t')) {
        issues.push("tab indentation".to_owned());
    }
    for key in yaml_contract_required_keys(definition.id) {
        if !yaml_has_top_level_key(content, key) {
            issues.push(format!("missing top-level `{key}`"));
        }
    }
    for path in local_markdown_paths(content) {
        if !repo_root.join(&path).exists() {
            issues.push(format!("missing local reference `{path}`"));
        }
        if issues.len() >= 6 {
            break;
        }
    }
    unique_limited(issues, 6)
}

fn yaml_contract_required_keys(contract_id: &str) -> &'static [&'static str] {
    match contract_id {
        "manifest" => &["schema_version", "contracts", "integrity"],
        "tasks" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "tasks",
        ],
        "repo_map" => &["schema_version", "generated_by", "source_files", "roots"],
        "contracts" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "contracts",
        ],
        "commands" => &["schema_version", "generated_by", "source_files", "commands"],
        "validation" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "rules",
        ],
        "boundaries" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "layers",
            "rules",
        ],
        "risks" => &[
            "schema_version",
            "reviewed_by",
            "reviewed_at",
            "source_files",
            "surfaces",
        ],
        "references" => &[
            "schema_version",
            "generated_by",
            "source_files",
            "references",
        ],
        _ => &["schema_version"],
    }
}

fn yaml_has_top_level_key(content: &str, key: &str) -> bool {
    let prefix = format!("{key}:");
    content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && line.trim_start() == line
            && trimmed.starts_with(&prefix)
    })
}

fn audit_agents_markdown(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
    content: &str,
) {
    let line_count = content.lines().count();
    if line_count > AGENTS_MD_MAX_LINES {
        push_guidance_issue(
            issues,
            "agents.root.too_large",
            "warning",
            "AGENTS.md is too large",
            &format!(
                "Root AGENTS.md has {line_count} lines; keep it under {AGENTS_MD_MAX_LINES} lines or move generated/detail content behind references."
            ),
            relative_path,
        );
    }

    audit_required_sections(issues, relative_path, content);
    audit_duplicate_headings(issues, relative_path, content);
    audit_banned_agent_phrases(issues, relative_path, content);
    audit_broad_git_examples(issues, relative_path, content);
    audit_reference_paths(issues, repo_root, relative_path, content);
}

fn audit_required_sections(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const REQUIRED_SECTIONS: &[&str] = &[
        "Scope And Precedence",
        "Repository Map",
        "Standard Workflow",
        "Commands",
        "Approval Required",
        "Validation Matrix",
        "Architecture Boundaries",
        "Code Rules",
        "Security Rules",
        "Completion Checklist",
        "References",
    ];

    let headings: Vec<_> = markdown_headings(content)
        .into_iter()
        .filter(|heading| heading.level == 2)
        .collect();
    let mut previous_index = None;
    for required in REQUIRED_SECTIONS {
        let matches: Vec<_> = headings
            .iter()
            .enumerate()
            .filter(|(_, heading)| heading.title == *required)
            .collect();
        if matches.is_empty() {
            push_guidance_issue(
                issues,
                "agents.sections.missing",
                "warning",
                "Required AGENTS.md section is missing",
                &format!("Missing section: {required}."),
                relative_path,
            );
            continue;
        }
        if matches.len() > 1 {
            push_guidance_issue(
                issues,
                "agents.sections.duplicate_required",
                "warning",
                "Required AGENTS.md section is duplicated",
                &format!("Section appears more than once: {required}."),
                relative_path,
            );
        }
        let index = matches[0].0;
        if let Some(previous_index) = previous_index
            && index < previous_index
        {
            push_guidance_issue(
                issues,
                "agents.sections.order",
                "warning",
                "AGENTS.md section order drifted",
                &format!("Section {required} appears before an earlier required section."),
                relative_path,
            );
            break;
        }
        previous_index = Some(index);
    }
}

fn audit_duplicate_headings(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    let mut counts = BTreeMap::<String, u64>::new();
    for heading in markdown_headings(content) {
        *counts.entry(heading.title).or_default() += 1;
    }
    for (heading, count) in counts {
        if count > 1 {
            push_guidance_issue(
                issues,
                "agents.sections.duplicate_heading",
                "warning",
                "Duplicate AGENTS.md heading",
                &format!("Heading appears {count} times: {heading}."),
                relative_path,
            );
        }
    }
}

fn audit_banned_agent_phrases(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const BANNED_PHRASES: &[&str] = &[
        "agents.md",
        "Agent Playbook",
        "AI/human collaboration playbook",
        "AI Quickstart",
        "ALWAYS RUN FIRST",
        "BEFORE YOU CODE",
    ];
    for phrase in BANNED_PHRASES {
        if content.contains(phrase) {
            push_guidance_issue(
                issues,
                "agents.drift.banned_phrase",
                "warning",
                "Stale agent-guidance phrase",
                &format!("Remove or modernize stale phrase: {phrase}."),
                relative_path,
            );
        }
    }
}

fn audit_broad_git_examples(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    relative_path: &str,
    content: &str,
) {
    const BANNED_GIT_EXAMPLES: &[&str] = &["git add .", "git add -A", "git commit -a"];
    for line in content.lines() {
        for command in BANNED_GIT_EXAMPLES {
            if line.contains(command) && !line_explicitly_prohibits_command(line) {
                push_guidance_issue(
                    issues,
                    "agents.git.broad_command_example",
                    "error",
                    "Broad git command example",
                    &format!("Use targeted staging examples instead of `{command}`."),
                    relative_path,
                );
            }
        }
    }
}

pub(super) fn line_explicitly_prohibits_command(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("never use")
        || lower.contains("do not use")
        || lower.contains("don't use")
        || lower.contains("avoid ")
        || lower.contains("forbid")
        || lower.contains("prohibit")
}

pub(super) fn audit_reference_paths(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
    content: &str,
) {
    let Some(references) = markdown_section(content, "References") else {
        return;
    };
    for candidate in local_markdown_paths(&references) {
        if !repo_root.join(&candidate).exists() {
            push_guidance_issue(
                issues,
                "agents.references.missing_path",
                "warning",
                "Referenced local path is missing",
                &format!("References points at `{candidate}`, but the path does not exist."),
                relative_path,
            );
        }
    }
}

fn audit_canonical_agent_links(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    repo_root: &Path,
    relative_path: &str,
) {
    let path = repo_root.join(relative_path);
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    if content.contains("agents.md") {
        push_guidance_issue(
            issues,
            "agents.links.casing",
            "warning",
            "Non-canonical AGENTS.md casing",
            &format!("{relative_path} should reference AGENTS.md with canonical uppercase casing."),
            relative_path,
        );
    }
}

#[derive(Clone, Debug)]
pub(super) struct MarkdownHeading {
    level: usize,
    pub(super) title: String,
}

pub(super) fn markdown_headings(content: &str) -> Vec<MarkdownHeading> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') {
                return None;
            }
            let level = trimmed
                .chars()
                .take_while(|character| *character == '#')
                .count();
            if level == 0
                || level > 6
                || !trimmed.chars().nth(level).is_some_and(char::is_whitespace)
            {
                return None;
            }
            Some(MarkdownHeading {
                level,
                title: normalize_markdown_heading_title(
                    trimmed[level..].trim().trim_matches('#').trim(),
                ),
            })
        })
        .collect()
}

fn normalize_markdown_heading_title(title: &str) -> String {
    let title = title.trim();
    let mut saw_digit = false;
    for (index, character) in title.char_indices() {
        if character.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        if saw_digit && matches!(character, '.' | ')') {
            let rest = title[index + character.len_utf8()..].trim_start();
            if !rest.is_empty() {
                return rest.to_owned();
            }
        }
        break;
    }
    title.to_owned()
}

fn markdown_section(content: &str, title: &str) -> Option<String> {
    let mut in_section = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            let heading = trimmed
                .trim_start_matches('#')
                .trim()
                .trim_matches('#')
                .trim();
            let heading = normalize_markdown_heading_title(heading);
            if in_section {
                break;
            }
            in_section = heading == title;
            continue;
        }
        if in_section {
            lines.push(line);
        }
    }
    if in_section {
        Some(lines.join("\n"))
    } else {
        None
    }
}

pub(super) fn local_markdown_paths(content: &str) -> Vec<String> {
    let mut paths = BTreeSet::<String>::new();
    for token in content
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '(' | ')' | '[' | ']' | ',' | '`' | '"' | '\'')
        })
        .map(|token| token.trim_end_matches(['.', ':', ';']))
    {
        if token.is_empty()
            || token.starts_with("http://")
            || token.starts_with("https://")
            || token.starts_with('#')
            || token.starts_with('/')
        {
            continue;
        }
        if token.contains("..") {
            continue;
        }
        let lower = token.to_lowercase();
        let has_known_reference_extension = lower.ends_with(".md")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml")
            || lower.ends_with(".json");
        if has_known_reference_extension || token.starts_with("./") {
            paths.insert(token.trim_start_matches("./").to_owned());
        }
    }
    paths.into_iter().collect()
}

pub(super) fn guidance_status(issues: &[StorageAgentGuidanceIssue]) -> String {
    if issues.iter().any(|issue| issue.severity == "error") {
        "error".to_owned()
    } else if !issues.is_empty() {
        "warning".to_owned()
    } else {
        "ok".to_owned()
    }
}

fn push_guidance_issue(
    issues: &mut Vec<StorageAgentGuidanceIssue>,
    id: &str,
    severity: &str,
    title: &str,
    detail: &str,
    path: &str,
) {
    issues.push(StorageAgentGuidanceIssue {
        id: id.to_owned(),
        severity: severity.to_owned(),
        title: title.to_owned(),
        detail: detail.to_owned(),
        path: path.to_owned(),
    });
}
