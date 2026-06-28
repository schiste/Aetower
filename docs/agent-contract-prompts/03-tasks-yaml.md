# Prompt: Generate `.agents/tasks.yaml`

```text
You are generating or updating `.agents/tasks.yaml`.

Target file:
`.agents/tasks.yaml`

Scope:
Edit only `.agents/tasks.yaml`.

Schema:
`.agents/schema-v1/tasks.schema.json`

Purpose:
Define the agent decision layer: classify incoming work, route it to likely
domains/files, require the right contracts and safety checks, and define the
completion report expected before final response.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect `AGENTS.md`, `.agents/repo-map.yaml`, `.agents/contracts.yaml`,
  `.agents/validation.yaml`, `.agents/boundaries.yaml`, `.agents/risks.yaml`,
  package manifests, test layout, hooks, and recurring task patterns in docs.
- Do not invent task types. Use only task classes that match real repository
  workflows or obvious recurring maintenance categories.
- Reference only existing validation rule IDs, contract IDs, risk IDs, boundary
  IDs, and command IDs. If a target file does not exist yet, mark the reference
  as review-needed rather than pretending it is wired.
- Do not mark `reviewed_by` or `reviewed_at` unless a human reviewed the routing.

Recommended content:
- Classification policy: fallback task, highest-risk behavior, uncertainty
  reporting, and whether multiple task matches are allowed.
- Task entries with stable IDs, compact descriptions, signals, strategy,
  required reads, likely paths, domain IDs, required contract IDs, risk surface
  IDs, boundary rule IDs, validation rule IDs, approval requirement, and
  completion-report requirements.
- Prefer task IDs such as `fix_bug`, `add_feature`, `modify_api`,
  `security_sensitive_change`, `storage_cleanup`, `release_change`,
  `docs_only`, or repository-specific equivalents when locally justified.

Validation:
- Validate YAML against `.agents/schema-v1/tasks.schema.json` if available.
- Check every referenced ID exists in the matching contract file.
- Check path globs are repository-relative and do not include parent traversal.
- Report changed files, validation performed, and residual risk.
```

