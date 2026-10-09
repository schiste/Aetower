---
name: repo-onboarding
description: Use when starting work in an unfamiliar repository, when the task asks for repo overview, setup, architecture, entrypoints, test commands, or where to begin. Skip for narrow file-scoped edits once the relevant paths are already known.
---

# Repo Onboarding: Aetower

## When to Use

- Load this skill first when the repository is unfamiliar or the request is broad.
- Recommended when: first task in repo, repo overview, setup or run instructions, architecture or entrypoints, where should I start, broad debugging or feature-localization request.
- Skip when: known file-scoped edit, follow-up inside already identified area, task already localized to concrete files.
- Use `.codex/skills/aethyme/SKILL.md` or `.claude/skills/aethyme/SKILL.md` for Aethyme's short operating contract after orientation; load its `references/` files only when needed.

## Repo Identity

- Kind: `repository`
- Languages: `rust`
- Package manager: `cargo`
- Key manifests: `rust/Cargo.toml, rust/crates/aetower-attribution/Cargo.toml, rust/crates/aetower-bench/Cargo.toml, rust/crates/aetower-cli/Cargo.toml, rust/crates/aetower-collector/Cargo.toml, rust/crates/aetower-core/Cargo.toml, rust/crates/aetower-diagnostics/Cargo.toml, rust/crates/aetower-ffi/Cargo.toml, rust/crates/aetower-friction/Cargo.toml, rust/crates/aetower-gpu/Cargo.toml, rust/crates/aetower-helper/Cargo.toml, rust/crates/aetower-history/Cargo.toml, rust/crates/aetower-identity/Cargo.toml, rust/crates/aetower-mcp/Cargo.toml, rust/crates/aetower-model/Cargo.toml, rust/crates/aetower-persistence/Cargo.toml, rust/crates/aetower-policy/Cargo.toml, rust/crates/aetower-regression/Cargo.toml, rust/crates/aetower-reputation/Cargo.toml, rust/crates/aetower-sensors/Cargo.toml, rust/crates/aetower-telemetry/Cargo.toml, rust/crates/aetower-time/Cargo.toml, rust/crates/uniffi-bindgen-swift/Cargo.toml`

## Workspaces

- `rust` (primary; cargo; manifest `rust/Cargo.toml`; high confidence)

## Start Here

- `fast_test`: `cargo test --manifest-path rust/Cargo.toml --workspace`
- `build`: `cargo build --manifest-path rust/Cargo.toml --workspace`

## Supporting Commands

- `cargo test --manifest-path rust/Cargo.toml --workspace` (fast_test; high confidence from `rust/Cargo.toml`)
  Workspace: `rust`
- `cargo build --manifest-path rust/Cargo.toml --workspace` (build; high confidence from `rust/Cargo.toml`)
  Workspace: `rust`

## Entrypoints

- `cli`: `rust/crates/aetower-cli/src/main.rs` (tracked Rust binary entrypoint in `rust`; high confidence)

## Additional Entrypoints

- `rust/crates/aetower-cli/src/main.rs` (file; role=cli; tracked Rust binary entrypoint in `rust`; high confidence)
  Executable: `aetower`
- `rust/crates/aetower-bench/src/main.rs` (file; role=cli; tracked Rust binary entrypoint in `rust`; high confidence)
  Executable: `aetower-bench`
- `rust/crates/aetower-helper/src/main.rs` (file; role=cli; tracked Rust binary entrypoint in `rust`; high confidence)
  Executable: `aetower-helper`
- `rust/crates/aetower-mcp/src/main.rs` (file; role=cli; tracked Rust binary entrypoint in `rust`; high confidence)
  Executable: `aetower-mcp`
- `rust/crates/uniffi-bindgen-swift/src/main.rs` (file; role=cli; tracked Rust binary entrypoint in `rust`; high confidence)
  Executable: `uniffi-bindgen-swift`

## Repo Map

- `.github` (automation; automation and CI configuration; high confidence)
- `assets` (assets; public assets or static files; high confidence)
- `docs` (docs; documentation area; high confidence)
- `infra` (infrastructure; deployment or infrastructure configuration; high confidence)
- `scripts` (tooling; developer tooling or scripts; high confidence)

## Aethyme Recipes

- `aethyme explore --repo "$PWD" --request "<task>" --format answer-json`
  Purpose: Broad repository orientation for a user request
- `aethyme repo inspect "$PWD" --mode brief --json-output`
  Purpose: Quick deterministic repo summary
- `aethyme graph callers "$PWD" "<symbol-or-file>" --json-output`
  Purpose: Trace likely impact before editing

## Generated and Dangerous Paths

- Generated/vendor `.aethyme/generated`: tracked generated or vendored surface; verify ownership before editing
- Sensitive `.github/workflows`: repository automation; changes can affect publication or shared CI

## Freshness

- Source digest: `476bbb3a2b860c88f7ded2fe2a69df2c7f0ed01f2392c8e957d4a80634ff9888`
- Tracked source files: `402`
- Overrides applied: `False`
- Sections generated: `repo, workspaces, primary_workspace, commands, areas, entrypoints, caution_zones, generated_paths, dangerous_paths, navigation_recipes, summon, freshness`
