# Aetower

Aetower is a macOS performance and runtime observability app for operators,
developers, and local AI agents. It combines process pressure, memory,
wakeups, energy friction, history, diagnostics, and optional app integrations
into one local control surface.

## Release status

Aetower is currently a **Developer Preview**.

This means:

- the app is intended for technical users who understand macOS permissions and
  local observability tooling
- the default build does not require the optional Endpoint Security helper
- public builds should be Developer ID signed, notarized, and distributed
  through the direct-download release pipeline
- data collection is local by default; optional exports must be explicitly
  configured

Do not present Developer Preview builds as production-ready or App Store-ready.

## What Aetower does

- Monitors host pressure, process groups, wakeups, memory, CPU, disk, network,
  GPU-related signals, and friction trends.
- Persists local history for incident reconstruction and before/after
  comparisons.
- Exposes a local read-only MCP server so AI coding agents can inspect live
  Aetower data without starting a second collection engine.
- Integrates optionally with Chau7, Chromium-compatible debug endpoints,
  Docker, local OTLP/HTTP metrics collectors, and an advanced signed helper.
- Provides diagnostics, support-bundle previews, runtime lag checks, and
  process action planning.

## Install and first run

For Developer Preview builds, use the direct-download release path:

1. Download the signed and notarized `Aetower.zip`.
2. Unzip and move `Aetower.app` to `/Applications`.
3. Launch Aetower.
4. Open **Settings -> Setup** and complete the readiness checklist.

See [Getting Started](docs/getting-started.md) for the first-run flow.
See [Download Aetower Developer Preview](docs/download.md) for the public
download-page copy, privacy summary, update expectations, and support/reset
guidance.

## Local development

Install local hooks once:

```sh
sh scripts/install-hooks.sh
```

Run the same gates manually:

```sh
sh scripts/ci-local.sh --mode pre-commit
sh scripts/ci-local.sh --mode pre-push
sh scripts/ci-local.sh --mode full
```

Package a local app:

```sh
sh scripts/package-macos.sh
```

Cut a signed Developer Preview release:

```sh
sh scripts/release-candidate.sh
```

Release configuration is documented in
[Direct Download Release](docs/direct-download-release.md). Public-preview
verification is documented in
[Public Preview Validation](docs/public-preview-validation.md).
Brand asset import is documented in [Brand Assets](docs/brand-assets.md).
Cloudflare Pages hosting is documented in
[Cloudflare Release Hosting](docs/cloudflare-release-hosting.md).

## Privacy and safety

Aetower observes local system and process metadata. Review
[Privacy](PRIVACY.md) before sharing builds publicly.

Review [Known Limitations](docs/known-limitations.md) before publishing or
installing Developer Preview builds outside your own machine.

The default direct-download build excludes the optional privileged Endpoint
Security helper. Keep that default for Developer Preview distribution unless
you are shipping a separately approved enterprise build.

## Security

Report vulnerabilities through the process in [Security](SECURITY.md).

## License

See [License](LICENSE.md).
