# Prompt: Generate `.agents/manifest.yaml`

```text
You are generating or updating `.agents/manifest.yaml`.

Target file:
`.agents/manifest.yaml`

Scope:
Edit only `.agents/manifest.yaml`.

Schema:
`.agents/schema-v1/manifest.schema.json`

Purpose:
Declare the agent contract file set, schema locations, generator/check commands,
freshness policy, and cross-file integrity expectations.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect `.agents/schema-v1/`, `.agents/*.yaml`, package manifests, hook files,
  scripts, and docs that mention agent contracts.
- Do not invent command IDs. Reference only IDs that exist in
  `.agents/commands.yaml`, or mark the command field absent until commands are
  defined.
- Do not claim hashes or freshness guarantees unless a checker actually enforces
  them.

Required content:
- `schema_version: 1`
- `contracts` entries for:
  - `AGENTS.md`
  - `.agents/manifest.yaml`
  - `.agents/tasks.yaml`
  - `.agents/repo-map.yaml`
  - `.agents/contracts.yaml`
  - `.agents/commands.yaml`
  - `.agents/validation.yaml`
  - `.agents/boundaries.yaml`
  - `.agents/risks.yaml`
  - `.agents/references.yaml`
- `integrity` rules for unique IDs, command ID references, source-file
  existence, local reference existence, and generated-file freshness.
- `freshness` rules if the repo has a real scan/generation process.
- `source_files` containing only files used to derive the manifest.

Review rules:
- Mark `reviewed_by` and `reviewed_at` only when a human has reviewed the
  manifest.
- If no human review happened, omit those fields and add a clear note.

Validation:
- Validate YAML against `.agents/schema-v1/manifest.schema.json` if available.
- Check every listed contract path is intentional.
- Check every referenced command ID exists, or explicitly report it as pending.
- Report changed files, validation performed, and residual risk.
```
