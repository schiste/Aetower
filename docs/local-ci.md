---
type: how-to
status: active
review_cycle: 180
last_reviewed: 2026-05-28
---

# Pre-commit and Pre-push Quality Gates

Aetower uses Git's `core.hooksPath` as the hook entry point. The hooks are thin
shell wrappers under `.githooks/`; the real quality system is the unified local
runner at `scripts/ci-local.sh`.

The contract is:

- `pre-commit` protects the staged file set before the commit is created.
- `pre-push` protects the full local integration surface before work leaves the
  machine.
- `full` adds packaged-app operator/MCP smoke and optional `cargo-audit`
  cross-checking.

Aetower does **not** use GitHub Actions for CI. The source of truth is the local
hook-driven runner, so the same gates run before code leaves the machine.

## Installation

```sh
sh scripts/install-hooks.sh
```

This runs:

```sh
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit .githooks/pre-push scripts/ci-local.sh scripts/install-hooks.sh scripts/smoke-package.sh
```

## Entry Points

| Action | Command path | Runner mode |
| --- | --- | --- |
| Commit | `.githooks/pre-commit` -> `sh scripts/ci-local.sh --mode pre-commit` | `pre-commit` |
| Push | `.githooks/pre-push` -> `sh scripts/ci-local.sh --mode pre-push --push-stdin-file <tmp>` | `pre-push` |
| Manual pre-commit | `sh scripts/ci-local.sh --mode pre-commit` | `pre-commit` |
| Manual pre-push | `sh scripts/ci-local.sh --mode pre-push` | `pre-push` |
| Full local battery | `sh scripts/ci-local.sh --mode full` | `full` |

Compatibility aliases:

- `--mode prepush` maps to `pre-push`
- `--mode prepush-full`, `--mode local`, and `--mode cloud-parity` map to
  `full`
- `--full` forces `full`

## Scope Model

### Pre-commit scope

`pre-commit` uses:

```sh
git diff --cached --name-only --diff-filter=ACMR
```

Only added, copied, modified, or renamed staged files are considered for
changed-file gates. Generated bindings, local caches, `dist/`, SwiftPM build
output, and Rust target output are excluded by a mix of `.gitignore`,
path-specific gate filters, and `quality-guard`.

### Pre-push scope

`.githooks/pre-push` captures the update lines Git provides on stdin and passes
them to the runner through `--push-stdin-file`. The runner uses the pushed
remote SHA as the commit-range base for commit-range checks such as `gitleaks`.

When push stdin is unavailable, the runner resolves a fallback base in this
order:

1. The current upstream branch.
2. `origin/main`.
3. `main`.
4. `HEAD~1`.
5. `HEAD`.

Pre-push intentionally runs the full local integration surface. Aetower is a
native system monitor with Swift, Rust, FFI, packaging, Sparkle, local MCP, and
runtime observability surfaces; small changes can break integration behavior
outside the directly changed file set.

### Dirty worktree policy

Before `pre-push` and `full`, the runner checks whether the working tree or
index is dirty.

- If clean, the run proceeds.
- If dirty and an interactive TTY is available, the runner asks before
  continuing.
- If dirty and no TTY is available, the run fails unless
  `AETOWER_SKIP_DIRTY_WORKTREE_CONFIRM=1` is set.

This prevents a push gate from validating a state that is not actually what is
being pushed.

## What Pre-commit Checks

Pre-commit is intentionally strict and staged-file focused. Warnings block the
commit.

| Gate | Runs when | What it checks |
| --- | --- | --- |
| `quality guard` | Always | Diff-level anti-slop and safety checks: secrets, placeholder markers, unsafe Swift patterns, hardcoded SwiftUI colors outside design tokens, production Rust panic shortcuts, risky dependency additions, suspicious dumping-ground filenames, and shell syntax for changed scripts/hooks. |
| `gitleaks (staged)` | Always | Staged secret scan with redacted output. |
| `swiftlint (<file>)` | Swift source files are staged | Strict SwiftLint on changed Swift files, excluding generated bindings. |
| `semgrep (changed files)` | Swift/Rust/script/hook files are staged and Semgrep is healthy | Local Semgrep quality rules against changed files. |
| `cargo fmt --check` | Rust files or Rust formatting/toolchain files are staged | Workspace Rust formatting. |
| `cargo clippy (<pkg>)` | A Rust crate is affected | Targeted crate clippy with warnings denied. |
| `cargo test (<pkg>)` | A Rust crate is affected | Targeted crate tests. |
| `cargo clippy (workspace)` | Rust manifests, lockfile, toolchain, or bridge scripts change | Full workspace clippy with warnings denied. |
| `cargo test (workspace)` | High-impact Rust files change | Full Rust workspace tests excluding helper first. |
| `cargo test (aetower-helper)` | High-impact Rust files change | Helper tests serialized because helper behavior can touch system-facing assumptions. |
| `build Rust bridge` | FFI, generated bindings, bridge code, or Rust build scripts change | Rebuilds Rust FFI and Swift bindings. |
| `swift build` | Swift package files are staged | SwiftPM build against the macOS package. |
| `benchmark smoke` | Hot-path runtime crates change | Short enforced performance budget smoke. |
| `shell hook checks` | Scripts or hooks are staged | Placeholder for shell-specific checks; currently covered by `quality-guard` syntax checks. |

## What Pre-push Checks

Pre-push is intentionally full-surface. It is the local equivalent of CI.

| Gate | What it checks |
| --- | --- |
| `quality guard` | Full-repo quality guard, including duplicate-block detection against changed files. |
| `gitleaks (commits)` | Commit-range secret scan from push base to `HEAD`. |
| `swiftlint` | Full Swift source lint, excluding generated bindings. |
| `semgrep` | Full Semgrep scan across Swift UI/app/bridge code, Rust crates, and scripts when Semgrep is healthy. |
| `cargo fmt --check` | Workspace Rust formatting. |
| `cargo clippy` | Workspace Rust clippy with warnings denied. |
| `cargo test (workspace)` | Workspace Rust tests excluding helper. |
| `cargo test (aetower-helper)` | Helper tests with one test thread. |
| `build Rust bridge` | Rust bridge and generated Swift binding rebuild. |
| `swift build` | SwiftPM debug build. |
| `benchmark budget` | Enforced synthetic runtime budget. |
| `telemetry smoke` | Real OTLP/HTTP loopback export verification. |
| `package smoke` | Rebuilds and verifies packaged `dist/Aetower.app`. |
| `cargo deny` | Rust dependency advisories, bans, and source policy. |

## What Full Adds

`full` runs everything in `pre-push`, then adds:

| Gate | What it checks |
| --- | --- |
| `local operator smoke` | Packaged-app MCP/operator smoke against the bundled helper. |
| `cargo audit` | Advisory cross-check when `cargo-audit` is installed. |

## Techniques That Keep It Fast, Strong, and Predictable

### Single runner

All gates live in `scripts/ci-local.sh`. New checks should be added there
rather than hidden inside hook files. The hook files should stay thin and only
select the correct runner mode.

### Explicit scope decisions

Pre-commit is scoped to staged files. Pre-push is full-surface by design.

That trade-off is intentional: pre-commit should stay fast enough to run often,
while pre-push should catch native integration failures that only appear after
Rust, Swift, FFI, packaging, Sparkle, and MCP surfaces are assembled together.

### Fail-fast execution

The runner executes gates sequentially and stops at the first failure. Later
gates depend on earlier contracts; for example, package smoke should not run if
the Rust bridge cannot be rebuilt.

### Verbose command logging

Every executed gate prints:

- stable gate label
- exact command line
- pass/fail result
- duration

Skipped scope gates print an explicit skip reason.

### Summary table

Every run ends with a quality summary:

- gate status
- gate duration
- percentage of measured runtime
- skip or failure detail when relevant
- total runtime
- ran, passed, skipped, and failed counts
- log path for non-commit runs

### Forensic logs

`pre-push` and `full` tee output to:

```text
logs/quality-<mode>-<timestamp>.log
```

`pre-commit` does not create logs to avoid flooding the repository during
frequent commit cycles. The `logs/` directory is ignored by Git.

### Runtime filters

The runner supports narrow reproductions:

```sh
sh scripts/ci-local.sh --mode pre-push --include="swift build"
sh scripts/ci-local.sh --mode pre-push --skip="package smoke"
sh scripts/ci-local.sh --mode pre-push --iterations 4
```

Filters match gate labels exactly and are for debugging. The default hook path
should stay strict.

### Rerun hints

When a gate fails, the summary includes the command that can be rerun directly.
This makes hook failures reproducible without searching through shell code.

## Packaging Smoke

Run package verification directly:

```sh
sh scripts/smoke-package.sh --rebuild
```

To also launch the packaged app briefly and verify the process comes up:

```sh
sh scripts/smoke-package.sh --rebuild --launch
```

## Local Operator Smoke

To launch the packaged app if needed and verify the bundled MCP helper end to
end:

```sh
sh scripts/local-operator-smoke.sh --rebuild
```

This validates:

- packaged app startup
- MCP `initialize`
- `aetower_current_snapshot`
- `aetower_diff_snapshots`
- `aetower_entity_process_tree`
- one dynamic profiling call through `aetower_profile_entity`

## Telemetry Smoke

To exercise OTLP export against a real local loopback collector:

```sh
sh scripts/telemetry-smoke.sh
```

This starts a temporary in-process HTTP receiver through `aetower-bench`,
verifies that Aetower sends a real OTLP/HTTP payload to `/v1/metrics`, and
fails if the exporter cannot deliver or the payload is malformed.

## Runtime Profiling

For live packaged-app CPU and memory profiling outside the synthetic harness,
see [Runtime Profiling](runtime-profiling.md).

For a long-running enforced soak with telemetry smoke and optional release
preflight:

```sh
sh scripts/soak-local.sh --rebuild --launch
```

## Release Packaging

For Developer ID signing and optional notarization, see
[Distribution](distribution.md) and
[Direct Download Release](direct-download-release.md).

## Reclaiming Local Disk

Cargo and SwiftPM caches plus the packaged app and profiling tmp dirs can
accumulate several GB. Inspect or delete them with:

```sh
sh scripts/clean-local.sh          # dry-run, reports sizes
sh scripts/clean-local.sh --yes    # actually delete
```

The script targets `tmp/`, `dist/`, `rust/target/`, and `macos/.build/`. It
never touches user state under `~/Library/Application Support/Aetower/`.

## Tooling Expectations

The local gates assume these tools are installed:

- `gitleaks`
- `swiftlint`
- `semgrep`
- `cargo-deny`

Install them with:

```sh
brew install gitleaks swiftlint semgrep
cargo install cargo-deny --locked
```

`cargo-deny` is the blocking Rust dependency-policy gate. Its ignore list in
[deny.toml](../deny.toml) is intentionally short and documents the migration
reason for each allowed advisory.

`semgrep` is wired into the hooks, but the script skips it explicitly if the
local binary cannot start cleanly. That keeps the gate strict when the tool is
healthy without making commits fail for unrelated local certificate/runtime
breakage.

## Operational Guidance

Use the default hooks for normal work:

```sh
git add .
git commit -m "feat: ..."
git push
```

Run gates manually when debugging:

```sh
sh scripts/ci-local.sh --mode pre-commit
sh scripts/ci-local.sh --mode pre-push
sh scripts/ci-local.sh --mode full
```

Bypass Git hooks only for emergencies:

```sh
git commit --no-verify -m "wip: emergency fix"
git push --no-verify
```

The expected policy is to fix the underlying quality failure and rerun the same
gate, not to make `--no-verify` part of the normal workflow.

## Recommendation

Do not add a generic content-hash cache yet. The current timing data shows the
dominant costs are `package smoke` and `swift build`, so the next CI efficiency
step should be targeted:

1. preserve and reuse the SwiftPM package build when inputs allow it
2. split package smoke into fast bundle-structure verification and slower
   rebuild verification
3. keep release/full mode as the place that proves a fresh package from
   scratch

That will reduce local push time without weakening the integration guarantees
that matter for a native macOS system monitor.
