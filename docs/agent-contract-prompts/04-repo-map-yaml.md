# Prompt: Generate `.agents/repo-map.yaml`

```text
You are generating or updating `.agents/repo-map.yaml`.

Target file:
`.agents/repo-map.yaml`

Scope:
Edit only `.agents/repo-map.yaml`.

Schema:
`.agents/schema-v1/repo-map.schema.json`

Purpose:
Describe the repository topology in a machine-checkable way so agents know where
code, packages, tools, generated output, tests, docs, hooks, and assets live.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect `git ls-tree -d --name-only HEAD`, workspace manifests, package
  manifests, build files, app/service folders, docs, hooks, scripts,
  `.gitignore`, and generated/cache roots.
- Do not invent roots. If a folder is ambiguous, include a review note.
- Keep ignored/cache/build directories out of required topology unless they are
  intentionally documented as generated or ignored roots.

Recommended content:
- Repository roots with kind, description, owner if known, source of truth,
  local agent file if present, validation rule IDs, and boundary layer IDs.
- Workspace/package groupings derived from real manifests.
- Entrypoints for apps, services, CLIs, workers, or libraries.
- Generated roots and ignored roots.
- Agent-critical roots such as `AGENTS.md`, `.agents/`, hooks, CI, and release
  scripts.
- Constraints describing what should or should not be placed in each root.

Validation:
- Validate YAML against `.agents/schema-v1/repo-map.schema.json` if available.
- Check every local path exists unless explicitly marked generated or pending.
- Check workspace roots match actual workspace/package manifests.
- Report changed files, validation performed, and residual risk.
```

