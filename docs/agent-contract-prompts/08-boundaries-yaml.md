# Prompt: Generate `.agents/boundaries.yaml`

```text
You are generating or updating `.agents/boundaries.yaml`.

Target file:
`.agents/boundaries.yaml`

Scope:
Edit only `.agents/boundaries.yaml`.

Schema:
`.agents/schema-v1/boundaries.schema.json`

Purpose:
Declare architecture boundaries agents can check before making changes:
layers, forbidden imports, generated-code ownership, app neutrality, domain
separation, and safe alternatives.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect source tree, import/dependency configs, lint rules, architecture docs,
  package boundaries, backend app boundaries, generated-code markers,
  `.agents/repo-map.yaml`, and `.agents/contracts.yaml`.
- Do not invent architecture intent. If intent is not locally proven, mark the
  rule as review-needed.
- Do not mark `reviewed_by` or `reviewed_at` unless a human reviewed the
  boundaries.

Recommended content:
- Layer definitions with IDs and default policy.
- Boundary rules with from-layer and to-layer IDs where possible.
- Forbidden imports or dependency directions.
- Non-import rules such as "models do not import views", "shared UI is
  app-neutral", or "generated code is edited only through generators" when the
  repository proves those rules.
- Enforcing command IDs per rule when available.
- Rationale, good examples, and bad examples.
- Safe alternatives for blocked dependencies.

Validation:
- Validate YAML against `.agents/schema-v1/boundaries.schema.json` if available.
- Check every referenced layer ID and command ID exists.
- Check local paths and globs are repository-relative.
- Report changed files, validation performed, and residual risk.
```

