# Prompt: Generate `.agents/validation.yaml`

```text
You are generating or updating `.agents/validation.yaml`.

Target file:
`.agents/validation.yaml`

Scope:
Edit only `.agents/validation.yaml`.

Schema:
`.agents/schema-v1/validation.schema.json`

Purpose:
Map touched paths, task classes, risks, and contract changes to exact validation
commands so agents can run the right checks without over-testing,
under-testing, or guessing.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect `.agents/tasks.yaml`, `.agents/contracts.yaml`,
  `.agents/commands.yaml`, hooks, CI, test layout, package manifests, workspace
  manifests, build scripts, lint configs, typecheck configs, and docs.
- Reference only command IDs that exist in `.agents/commands.yaml`.
- Add manual-review-only rules when no reliable local command exists.
- Do not mark `reviewed_by` or `reviewed_at` unless a human reviewed the mapping.

Recommended content:
- Path glob rules with included paths, excluded paths, command IDs, reason,
  changed-files mode, risk surface IDs, boundary rule IDs, and skip-reporting
  requirements.
- Full-suite triggers that map high-impact files to exact command IDs.
- Downstream validation for shared packages.
- Lightweight docs-only validation rules.
- Superset behavior for multi-area changes.
- Maximum cost allowed before asking the operator.

Validation:
- Validate YAML against `.agents/schema-v1/validation.schema.json` if available.
- Check every command ID exists in `.agents/commands.yaml`.
- Check path globs are repository-relative and do not include parent traversal.
- Report changed files, validation performed, and residual risk.
```

