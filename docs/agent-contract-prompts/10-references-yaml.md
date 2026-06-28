# Prompt: Generate `.agents/references.yaml`

```text
You are generating or updating `.agents/references.yaml`.

Target file:
`.agents/references.yaml`

Scope:
Edit only `.agents/references.yaml`.

Schema:
`.agents/schema-v1/references.schema.json`

Purpose:
Point agents to deeper context that should be loaded only when relevant, without
turning `AGENTS.md` into a long documentation dump.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect README, CONTRIBUTING, docs, ADRs, runbooks, architecture notes, API
  docs, setup docs, release docs, security/privacy docs, schema docs, and nested
  `AGENTS.md` files.
- Do not list stale or speculative docs.
- Prefer canonical local docs over duplicate or older docs.

Recommended content:
- Reference ID, kind, path or URI, title, summary, load triggers, applicable
  paths, task triggers, stale-after window, canonical flag, superseded docs, and
  network requirement for URI references.
- Separate onboarding, workflow, architecture, runbook, schema, contract,
  configuration, security, release, and agent-specific references.
- Mark docs that should not be loaded by default.

Validation:
- Validate YAML against `.agents/schema-v1/references.schema.json` if available.
- Check every local path exists.
- Check canonical references do not contradict `AGENTS.md`.
- Report changed files, validation performed, and residual risk.
```

