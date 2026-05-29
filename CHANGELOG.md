# Changelog

All notable public changes to Aetower should be documented here.

## 0.51 (build 418) - 2026-05-29

Developer Preview maintenance release.

Release metadata:

- Bundle identifier: `com.aeptus.aetower`
- Sparkle appcast: `https://aetower.dev/releases/appcast.xml`
- Release archive prefix: `https://aetower.dev/releases/`

- Changed the Aetower project license to `AGPL-3.0-only`.
- Added corresponding source archive generation for public binary releases.
- Added Cloudflare release-site publication for versioned and latest source
  archives.
- Added public release verification for source archive availability.
- Added release preflight protection against dirty release source states.
- Updated package metadata so Rust workspace crates report the AGPL license.
- Embedded the Aetower license file in packaged macOS app bundles.

## 0.5 (build 414) - 2026-05-29

First public `0.5` Developer Preview release.

Release metadata:

- Bundle identifier: `com.aeptus.aetower`
- Sparkle appcast: `https://aetower.dev/releases/appcast.xml`
- Release archive prefix: `https://aetower.dev/releases/`

- Added prominent process actions in Monitor and process detail surfaces,
  including easier access to terminate and force-kill preview flows.
- Added richer monitor-side process controls, quick stop access, and clearer
  action naming.
- Added Chau7 integration guidance when Chau7 is not running or not connected.
- Added public-release publication checks for Sparkle appcast, immutable ZIP,
  direct ZIP, Homebrew cask, optional PKG, and third-party notices.
- Added Cloudflare release-site publication of Homebrew cask artifacts and
  optional signed PKG artifacts, gated so stale local PKGs are never published
  unless the current release run explicitly built a PKG.
- Added release preflight validation that the changelog version/build matches
  the release metadata.

## 0.1.0-developer-preview.1 (build 408) - 2026-05-29

First public Developer Preview baseline.

Release metadata:

- Bundle identifier: `com.aeptus.aetower`
- Sparkle appcast: `https://aetower.dev/releases/appcast.xml`
- Release archive prefix: `https://aetower.dev/releases/`

- Added a direct-download macOS release path with Developer ID signing,
  notarization, Sparkle appcast generation, and package smoke testing.
- Added local MCP support for AI agents through the app-owned live engine and
  a bundled helper.
- Added Monitor, History, Timeline, Chau7, AI Agents, Diagnostics, Fleet, and
  Settings surfaces.
- Added operator-safe History and Timeline behavior for large datasets.
- Added runtime diagnostics, history health, memory breakdowns, profiling
  hooks, wakeup attribution, and support-bundle previews.
- Added richer process inspection with code-signing classification, loaded
  library enumeration, bundle metadata, startup-entry detection, and child-tree
  thread aggregation.
- Added advanced process-list workflows: regex/field search, CSV export,
  recently-finished process visibility, new-process badges, and sandboxed Rhai
  filters.
- Added process-event automation hooks for Shortcuts or shell commands.
- Added release artifacts for Homebrew cask, optional PKG packaging, Sparkle
  distribution matrix verification, and generated third-party dependency
  notices.
- Added hardware-focused views for timestamp hover-scrub history graphs,
  hardware sensors, live menu-bar sparklines, and per-core performance/efficiency
  CPU load.
- Added local quality gates for Swift, Rust, security scans, benchmarks,
  package smoke, telemetry smoke, and dependency policy.
