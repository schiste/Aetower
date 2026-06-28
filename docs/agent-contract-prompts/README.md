# Agent Contract Prompt Pack

This directory contains paste-ready prompts for creating and maintaining
Aetower-compatible agent contracts one file at a time.

Use the prompts in this order:

1. `00-preflight.md`
2. `01-agents-md.md`
3. `02-manifest-yaml.md`
4. `03-tasks-yaml.md`
5. `04-repo-map-yaml.md`
6. `05-contracts-yaml.md`
7. `06-commands-yaml.md`
8. `07-validation-yaml.md`
9. `08-boundaries-yaml.md`
10. `09-risks-yaml.md`
11. `10-references-yaml.md`
12. `11-check-and-reconcile.md`

The intended workflow is granular. One prompt updates one contract file, using
local repository evidence instead of guessing. Hooks and CI should run checks;
they should not auto-generate contract files.

Core rules for every prompt:

- Start with `git status --short`.
- Preserve unrelated user changes.
- Inspect the local files that prove each claim.
- Do not invent paths, commands, packages, owners, risks, or architecture rules.
- Update only the target file unless the operator explicitly asks for more.
- Validate YAML shape against `.agents/schema-v1/*.schema.json` when present.
- Leave uncertain claims as review notes rather than presenting guesses as facts.
- Keep files compact: routing, constraints, commands, and invariants beat prose.
