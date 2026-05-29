# Changelog

All notable public changes to Aetower should be documented here.

## 0.1.0-developer-preview.1 (build 405) - 2026-05-29

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
