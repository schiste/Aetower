# Prompt: Check And Reconcile Agent Contracts

```text
You are checking the repository's agent operating contracts for consistency.

Scope:
Do not generate new files unless explicitly asked. Prefer reporting and small
targeted fixes.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Validate contract YAML against `.agents/schema-v1/*.schema.json` when present.
- Check cross-file integrity that JSON Schema cannot enforce:
  - Unique IDs inside each contract.
  - Task references to contracts, risks, boundaries, validation rules, and
    commands exist.
  - Contract references to validation rules and commands exist.
  - Command IDs referenced from manifest, validation, boundaries, and risks
    exist in `.agents/commands.yaml`.
  - Risk IDs and boundary rule IDs referenced from validation exist.
  - Source files exist.
  - Local references exist.
  - Generated files are fresh when a freshness policy exists.
  - Hook commands match the command registry.
  - README/CONTRIBUTING point to canonical `AGENTS.md` and do not duplicate stale
    agent workflow rules.
- Do not auto-write generated files from hooks. If regeneration is needed,
  report the explicit command to run.

Output:
- Findings ordered by severity.
- Exact file/path references.
- Suggested targeted fixes.
- Validation commands run.
- Residual risk.
```

