# The `aetower` command line tool

`aetower` talks to a running Aetower.app over its local MCP socket and prints
live system state: friction, energy, storage, and repositories. It reads the
exact same engine the app renders — no second collector, no separate daemon.

```
$ aetower top
ENTITY                  FRICTION  CPU%      MEM
Chrome · 48 procs             72   119   1.2 GB
Ollama · GPU x                58    12   9.1 GB
Claude Code · repo x          14     8   340 MB
```

Add `--json` to any command for machine-readable output, or `--watch <secs>`
to refresh read commands until interrupted.

## Commands

- `aetower top` — loudest entities right now (friction, CPU, memory)
- `aetower findings` — the recommendation feed: what's straining the machine
  and what to do
- `aetower alerts` — active alerts
- `aetower host` — host summary: CPU, memory, wakeups, energy, thermal
- `aetower storage` — storage pressure and reclaim opportunities
- `aetower repos` — repository health: git state, agent-contract readiness,
  clones
- `aetower tab <slug>` — switch the app's visible tab
  (`aetower tab system`, `aetower tab activity/timeline`)
- `aetower doctor` — self-check: reachability, capabilities, engine health
- `aetower tools` — list every MCP tool the running app exposes
- `aetower call <tool>` — call any MCP tool directly and print its JSON
  payload
- `aetower install` / `aetower uninstall` — manage the `/usr/local/bin`
  symlink
- `aetower completions` — shell completion scripts (zsh, bash, fish, …)
- `aetower man` — the man page

## Options

- `--json` — emit the raw structured JSON payload instead of a formatted view
- `--socket <PATH>` — override the MCP socket path (default:
  `~/.aetower/mcp.sock`)
- `--watch <SECS>` — refresh every SECS seconds until interrupted (read verbs
  only)

## Installation

The CLI ships inside the app bundle, so it arrives through every install
channel:

- **Installer package (.pkg)** — symlinks `aetower` onto your `PATH`
  automatically.
- **Homebrew tap** — `brew tap aeptus/aetower && brew install --cask aetower`
  links the command into Homebrew's `bin` directory.
- **DMG / ZIP** — add it from Settings → AI Clients ("Install Command Line
  Tool") or run the bundled helper once:
  `/Applications/Aetower.app/Contents/Helpers/aetower install`.

## Driving the UI from scripts

Every workspace tab is addressable without Accessibility trust or synthetic
clicks:

```
aetower tab storage
open "aetower://tab/activity/timeline"
aetower tab system --default   # also persist as the startup tab
```

Slugs: `monitor`, `activity`, `storage`, `repos`, `projects`, `agents`,
`system`, `settings` — with sub-tabs like `activity/timeline` and
`system/diagnostics`.

## Exit codes and pipelines

Commands exit non-zero when the app is unreachable or the query fails, so
`aetower doctor && aetower top --json | jq ...` composes the way you'd
expect. If Aetower isn't running, the CLI says so and exits instead of
starting a second engine.
