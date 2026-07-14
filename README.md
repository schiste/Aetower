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
- Exposes a local MCP server so AI coding agents can inspect live Aetower data
  and, by default, see guarded operator actions without starting a second
  collection engine.
- Integrates optionally with Chau7, Chromium-compatible debug endpoints,
  Docker, local OTLP/HTTP metrics collectors, and an advanced signed helper.
- Provides diagnostics, support-bundle previews, runtime lag checks, and
  process action planning.

## Install and first run

Latest Developer Preview: **0.8.1 build 801**.

Download the release from
[aetower.dev](https://aetower.dev/).
The release pipeline now produces a signed/notarized drag-and-drop DMG,
installer PKG, Sparkle ZIP, matching source archive, Homebrew tap cask,
Sparkle appcast, third-party notices, and checksums.

Canonical Sparkle update metadata is configured for:

```text
https://aetower.dev/releases/appcast.xml
```

The release payload is served separately from the website so Sparkle update
metadata stays available even when the marketing site changes:

```text
https://aetower.dev/releases/appcast.xml
```

For Developer Preview builds:

1. Download the signed and notarized `Aetower.pkg` for the default installer
   path, or `Aetower.dmg` for drag-and-drop installs.
2. Run the PKG installer, or open the DMG and drag `Aetower.app` to
   Applications.
3. Launch Aetower.
4. Open **Settings -> Setup** and complete the readiness checklist.
5. Confirm the CLI path with `aetower top`, `aetower storage`, and
   `aetower repos`. PKG and Homebrew tap installs link the command
   automatically; DMG/ZIP installs can use **Settings -> AI Clients ->
   Install Command Line Tool**.

Homebrew install:

```sh
brew tap aeptus/aetower
brew install --cask aetower
```

See [Getting Started](docs/getting-started.md) for the first-run flow.
See [Feature List](docs/features.md) for a structured technical and non-technical overview of Aetower capabilities.
See [Download Aetower Developer Preview](docs/download.md) for the public
download-page copy, privacy summary, update expectations, and support/reset
guidance.
See [Observability](docs/observability.md) for OTLP metric names, a
Prometheus/OTel collector setup, and a ready-made Grafana dashboard.

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

When `.env.release.local` exists, local packaging loads it automatically so
rebuilds use the stable Developer ID signature. Use
`AETOWER_SIGN_IDENTITY=- sh scripts/package-macos.sh` only for explicit ad-hoc
testing. Direct local packaging does not automatically notarize; release
wrappers opt into notarization and stapling.

Cut a signed Developer Preview release:

```sh
sh scripts/release-public-preview.sh
```

Release configuration is documented in
[Direct Download Release](docs/direct-download-release.md). Public-preview
verification is documented in
[Public Preview Validation](docs/public-preview-validation.md).
Brand asset import is documented in [Brand Assets](docs/brand-assets.md).
Cloudflare Pages hosting is documented in
[Cloudflare Release Hosting](docs/cloudflare-release-hosting.md).
Homebrew tap publishing is documented in
[Homebrew Release](docs/homebrew-release.md).

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

Aetower is licensed under the GNU Affero General Public License v3.0 only
(`AGPL-3.0-only`). See [License](LICENSE.md).

Copyright (C) 2026 Aetower contributors.

Public binary releases should be paired with the corresponding source archive
for that exact version and build.
