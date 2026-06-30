# Agent Operating Contract

## Scope And Precedence

State which paths this contract covers and how nested `AGENTS.md` files override
or refine these instructions.

## Repository Map

Summarize the main source, test, docs, generated, and tooling roots. Link to
`.agents/repo-map.yaml` for machine-readable detail when present.

## Standard Workflow

- Run `git status --short` before editing.
- Preserve unrelated user changes.
- Keep edits scoped to the requested task.
- Do not switch branches, commit, or push unless explicitly asked.

## Commands

List the exact commands agents should prefer, or point to
`.agents/commands.yaml`.

## Approval Required

List actions requiring explicit operator approval: destructive commands, network
or dependency installs, production deploys, migrations, secrets, or writes
outside the repository.

## Validation Matrix

List required checks by change type, or point to `.agents/validation.yaml`.

## Architecture Boundaries

Summarize import, ownership, generated-code, and layer constraints. Point to
`.agents/boundaries.yaml` when present.

## Code Rules

State repository-specific coding rules that must be preserved.

## Security Rules

State security, privacy, credential, and data-handling rules.

## Completion Checklist

- Report changed files.
- Report validation performed.
- Report skipped checks and why.
- Report residual risk or uncertainty.

## References

List canonical docs only. Prefer `.agents/references.yaml` for deeper context.

