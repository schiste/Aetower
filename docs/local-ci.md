# Aetower local CI

The local verification path is intentionally strict. It is layered so that:

- `pre-commit` is a hard gate on changed code and staged diff quality
- `pre-push` is the full repository validation path
- `full` is the same as `pre-push`, plus optional dependency audit when `cargo-audit` is installed

## Install commit hooks

```sh
sh scripts/install-hooks.sh
```

This configures Git to use the repository `.githooks` directory:

- `pre-commit` runs the fast local gate
- `pre-push` runs the full local gate

## Run manually

Hard gate for every commit:

```sh
sh scripts/ci-local.sh --mode pre-commit
```

Full local push gate:

```sh
sh scripts/ci-local.sh --mode pre-push
```

Full local CI, including optional dependency audit:

```sh
sh scripts/ci-local.sh --mode full
```

## What the gates enforce

`pre-commit`:

- diff-based `quality-guard` for:
  - secrets
  - placeholder markers
  - unsafe Swift patterns (`AnyView`, `as!`, `try!`, `fatalError`)
  - hardcoded SwiftUI colors outside `DesignTokens`
  - missing `.utilityTextInput()` on newly added `TextField`s
  - production Rust `unwrap` / `expect`
  - risky dependency additions in `Cargo.toml` and `Package.swift`
  - suspicious new dumping-ground filenames
  - shell syntax checks on changed hook/script files
- `cargo fmt --check` when Rust files change
- targeted Rust `clippy` and `test` runs for changed crates, or full workspace when core crates/manifests change
- Rust bridge rebuild when FFI/bridge surfaces change
- `swift build` when Swift package files change
- light benchmark smoke when hot-path Rust crates change

`pre-push`:

- full-repo `quality-guard`, including duplicate-block detection against changed files
- `cargo fmt --check`
- workspace `cargo clippy --all-targets -- -D warnings`
- workspace `cargo test`
- Rust bridge rebuild
- `swift build`
- benchmark budget enforcement
- loopback OTLP telemetry smoke verification
- full app packaging
- package smoke verification

`full`:

- everything in `pre-push`
- `cargo audit` when `cargo-audit` is installed locally

## Packaging smoke

You can run package verification directly:

```sh
sh scripts/smoke-package.sh --rebuild
```

To also launch the packaged app briefly and verify the process comes up:

```sh
sh scripts/smoke-package.sh --rebuild --launch
```

## Release packaging

For Developer ID signing and optional notarization, see [distribution.md](distribution.md).

## Runtime profiling

For live packaged-app CPU / memory profiling outside the synthetic harness, see [runtime-profiling.md](runtime-profiling.md).

For a long-running enforced soak with telemetry smoke and optional release preflight:

```sh
sh scripts/soak-local.sh --rebuild --launch
```

## Telemetry smoke

To exercise OTLP export against a real local loopback collector:

```sh
sh scripts/telemetry-smoke.sh
```

This starts a temporary in-process HTTP receiver through `aetower-bench`, verifies that Aetower sends a real OTLP/HTTP payload to `/v1/metrics`, and fails if the exporter cannot deliver or the payload is malformed.

## What the new guard is trying to stop

`scripts/quality-guard.py` is intentionally anti-slop. It is designed to catch common low-signal patterns before they enter the repo:

- duplicated blocks copied into changed files
- SwiftUI design-token bypasses
- unsafe Swift escape hatches
- production Rust panic shortcuts
- remote or floating dependencies
- suspicious utility dumping grounds
