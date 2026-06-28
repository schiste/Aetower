# Prompt: Generate `AGENTS.md`

```text
You are generating or updating the root `AGENTS.md` for this repository.

Target file:
`AGENTS.md`

Scope:
Edit only `AGENTS.md`.

Purpose:
Create the short human-readable operating contract that every coding agent must
read before changing this repository.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect existing `AGENTS.md`, `.agents/`, `README.md`, `CONTRIBUTING.md`,
  hook files, command manifests, and validation docs before editing.
- Keep `AGENTS.md` concise. Prefer pointers to `.agents/*.yaml` and docs over
  duplicating machine-readable contracts.
- Do not invent commands, paths, policies, or architecture boundaries.
- If a fact is uncertain, add a short "Needs Review" item instead of pretending
  it is confirmed.
- Express the boot sequence clearly: read manifest, classify via tasks, use repo
  map, check contracts/boundaries/risks, run validation, report outcome.

Required sections, in this order:
1. Scope And Precedence
2. Repository Map
3. Standard Workflow
4. Commands
5. Approval Required
6. Validation Matrix
7. Architecture Boundaries
8. Code Rules
9. Security Rules
10. Completion Checklist
11. References

Content rules:
- State that agents must preserve unrelated user changes.
- State that commits and pushes happen only when explicitly requested.
- State that staging must be targeted.
- State that branch/worktree switching requires explicit instruction.
- State which setup, deploy, destructive, network-mutating, or privileged actions
  require approval.
- Reference `.agents/*.yaml` as the machine-checkable source when those files
  exist.
- Require final responses to report task type, files changed, contracts touched,
  validation run, skipped checks, and residual risks when applicable.
- Avoid broad whole-tree staging or commit shortcuts; require targeted staging.
- Keep tool-specific adapter docs out of the root file unless they are part of
  the repository standard.

Validation:
- Check markdown fences are balanced.
- Check referenced local paths exist or are explicitly marked pending.
- Check README/CONTRIBUTING references do not contradict this file.
- Report changed files, validation performed, and residual risk.
```
