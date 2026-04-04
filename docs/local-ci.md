# Aetower local CI

The local verification path is intentionally strict.

## Install commit hooks

```sh
sh scripts/install-hooks.sh
```

This configures Git to use the repository `.githooks` directory:

- `pre-commit` runs the fast local gate
- `pre-push` runs the full local gate

## Run manually

Fast gate, suitable for every commit:

```sh
sh scripts/ci-local.sh --mode pre-commit
```

Full gate, suitable before pushing or cutting a build:

```sh
sh scripts/ci-local.sh --mode pre-push
```

## What the gates enforce

`pre-commit`:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- Rust bridge rebuild
- `swift build`
- benchmark budget enforcement

`pre-push`:

- everything in `pre-commit`
- loopback OTLP telemetry smoke verification
- full app packaging
- package smoke verification

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
