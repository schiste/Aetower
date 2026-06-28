# Prompt: Generate `.agents/contracts.yaml`

```text
You are generating or updating `.agents/contracts.yaml`.

Target file:
`.agents/contracts.yaml`

Scope:
Edit only `.agents/contracts.yaml`.

Schema:
`.agents/schema-v1/contracts.schema.json`

Purpose:
Declare the repository invariants agents must not break: API shapes,
authentication model, authorization rules, data isolation, database guarantees,
release/update guarantees, storage semantics, performance budgets, process
control behavior, UI contracts, and security constraints.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect `AGENTS.md`, architecture docs, API docs, security/privacy docs,
  release docs, migrations, tests, `.agents/validation.yaml`,
  `.agents/risks.yaml`, `.agents/boundaries.yaml`, and source files that encode
  invariant behavior.
- Keep contracts compact and checkable. Do not write essays.
- Do not invent product or security guarantees. If an invariant seems likely but
  is not locally proven, add a review note.
- Do not mark `reviewed_by` or `reviewed_at` unless a human reviewed the
  invariants.

Recommended content:
- Stable contract IDs with category, rule, severity, affected paths, source of
  truth, owner when known, forbidden operations, required tests/validation rule
  IDs, structured shape for API/error contracts, safe alternatives, and short
  examples only when useful.
- Typical categories: `api`, `auth`, `authorization`, `database`,
  `data-integrity`, `data-privacy`, `error-shape`, `observability`,
  `performance`, `process-control`, `release`, `security`, `storage`,
  `tenant-isolation`, `ui`.

Validation:
- Validate YAML against `.agents/schema-v1/contracts.schema.json` if available.
- Check every referenced path, command ID, and validation rule ID exists.
- Check contracts align with risks and boundaries rather than duplicating them.
- Report changed files, validation performed, and residual risk.
```

