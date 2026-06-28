# Prompt: Agent Contract Preflight

Paste this before generating any individual contract file.

```text
You are preparing this repository for Aetower-compatible agent contracts.

Goal:
Build enough local evidence to generate one contract file accurately. Do not
edit files during this preflight unless explicitly asked.

Required behavior:
- Run `git status --short` first and preserve unrelated user changes.
- Stay on the current branch and worktree.
- Do not commit or push unless explicitly asked.
- Identify the package managers, build systems, test runners, hooks, CI files,
  repository roots, generated folders, ignored folders, entrypoints, and docs
  that are relevant to agent operation.
- Prefer local files over assumptions. If a claim cannot be proven locally,
  mark it as uncertain.
- Keep the result compact and actionable.

Inspect, when present:
- `AGENTS.md`
- `.agents/`
- `README.md`
- `CONTRIBUTING.md`
- `.gitignore`
- `.githooks/`
- `.husky/`
- `package.json`
- `pnpm-workspace.yaml`
- `Cargo.toml`
- `go.mod`
- `pyproject.toml`
- `Makefile`
- `justfile`
- `scripts/`
- `.github/workflows/`
- app, package, service, backend, frontend, docs, and tool roots.

Output:
- Repository summary.
- Detected command sources.
- Detected validation sources.
- Detected architecture boundary sources.
- Detected risk surfaces.
- Detected reference documents.
- Unknowns that need human review.
```

