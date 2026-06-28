# Prompt: Generate `.agents/commands.yaml`

```text
You are generating or updating `.agents/commands.yaml`.

Target file:
`.agents/commands.yaml`

Scope:
Edit only `.agents/commands.yaml`.

Schema:
`.agents/schema-v1/commands.schema.json`

Purpose:
Create the exact command registry agents can use without guessing. Each command
must describe cost, scope, mutation, approval, shell usage, network behavior,
secrets, output expectations, and runtime bounds.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect package manifests, Makefile, justfile, scripts, hook files, CI files,
  release scripts, and existing docs.
- Prefer `argv` arrays for deterministic commands. Use `shell_command` only when
  shell semantics are required.
- Do not invent scripts or binaries.
- If a command may write outside the repo, use network, require secrets, deploy,
  mutate remote state, or need elevated permissions, encode that explicitly.

Recommended fields per command:
- `id`
- `purpose`
- `cwd`
- `argv` or `shell_command`
- `cost`
- `mutates_files`
- `needs_approval`
- `approval_reason` when approval is needed
- `scope`
- `timeout_seconds`
- `uses_shell`
- `interactive`
- `requires_network`
- `requires_secrets`
- `writes_outside_repo`
- `external_effects`
- `allowed_in_hooks`
- `parameters` for safe command variables
- `rerun` or equivalent operator-facing recovery instruction if supported by
  the schema

Validation:
- Validate YAML against `.agents/schema-v1/commands.schema.json` if available.
- Check every listed script exists.
- Check every `pnpm <script>`, npm script, Cargo command, Make target, just
  recipe, or direct script path is real.
- Check command IDs are unique.
- Report changed files, validation performed, and residual risk.
```

