# Changelog

All notable public changes to Aetower should be documented here.

## Unreleased

Cost model:

- Added unified resource-cost rollups for repositories, sessions, entities,
  and machine-level activity, with shared estimates for energy, battery,
  dollars, carbon, disk growth, thermal contribution, source, and
  confidence.
- Repository rollups include estimated energy and carbon only when
  attribution is strong enough (known repo path, matching Chau7
  session/workspace, session linked to an entity with accumulated session
  energy), tagged with the `chau7-repo+estimated-energy` source. The Repos
  UI carries dollars, tokens, estimated energy, carbon, source, and
  confidence, and row/detail help text explains the estimates where
  available.

Privacy:

- Added an Outbound Data panel under Settings → Privacy that shows every
  channel that can leave the machine and its live state: telemetry on/off
  with endpoint and cadence, Fleet on/off with local-network advertising
  behavior, VirusTotal on/off/key state with its exact hash-only behavior,
  provider credentials configured versus actually usable for outbound API
  calls, and registered MCP clients with the local-socket/client-forwarding
  caveat.

CLI install consistency:

- Homebrew casks now declare a `binary` artifact for the bundled
  `Aetower.app/Contents/Helpers/aetower` helper, so `brew install --cask`
  links `aetower` into Homebrew's `bin` directory automatically.
- Settings → Setup now tracks CLI readiness, and Settings → AI Clients shows
  the command-line tool status before MCP registration, with explicit smoke
  commands for `aetower top`, `aetower storage`, and `aetower repos`.
- Release matrix verification now fails if the generated Homebrew cask stops
  linking the CLI or stops documenting the three smoke commands.

Homebrew tap:

- Published Aetower through the real `Aeptus/homebrew-aetower` tap so users can
  install with `brew tap aeptus/aetower` and `brew install --cask aetower`.
- Added an explicit tap publisher to the release pipeline; generated cask files
  remain build artifacts until `--publish-homebrew-tap` pushes the tap.
- Updated docs and website copy to distinguish the downloadable cask artifact
  from the brew-installable tap distribution.

Repository operations:

- Added explicit Repos operator-truth coverage for immediate cached inventory,
  Git/index/config fingerprint changes, stale-but-visible repositories, and the
  background new-repo scan toast/state.

Release validation:

- Added a public-claims validation gate that checks release-facing claims
  against source defaults, local release artifacts, the Sparkle appcast, source
  archives, bundled CLI, Fleet/outbound defaults, and published release URLs
  before website deploys.

Storage:

- Added explicit APFS-aware storage coverage for sparse files, hardlinks,
  cloud placeholders, purgeable capacity, and clone-lineage caveats, with docs
  that frame the feature as local reclaim estimates rather than exact
  filesystem ownership forensics.

## 0.8 (build 800) - 2026-07-08

Accessibility & automation:

- The main window's tabs are now addressable, not just clickable. Every
  workspace has a stable slug and four ways to reach it: the
  `aetower://tab/<slug>` URL scheme (e.g. `open "aetower://tab/system"`,
  sub-tabs via `aetower://tab/activity/timeline`), an `aetower tab <slug>`
  CLI verb, Cmd+1…8 in a new **Navigate** menu, and matching
  accessibility identifiers (`tab.system`, `rail.activity.timeline`). None
  of these need Accessibility trust or synthetic clicks, so headless agents
  and scripts can drive the UI.
- Added a persisted default startup tab (Settings → General → "Startup
  tab"). The app opens on Monitor unless an explicit default is set, which
  agents can also write with `aetower tab <slug> --default` or
  `defaults write com.aeptus.aetower nav.defaultWorkspaceTab <slug>`.
- Sidebar rail buttons and section menus now expose selection state, labels,
  and hints to VoiceOver and UI automation.

Repository operations:

- Reworked the Repos tab into a cache-first operator view: cached repository
  inventory paints immediately, a fingerprint pass decides whether a deeper
  parse is needed, and a small background toast reports new-repo scans without
  blocking the list.
- Split repository optimization planning out of the Aetower-specific view layer
  so future content sources can feed the same optimization decisions.
- Aligned Repos list rows and floating controls with the Monitor tab and shared
  design-system components.

Storage:

- Added the Storage cube explorer and reclaim-card actions for scanning storage
  pressure visually.
- Fixed environment-dependent storage scan policy assertions and preserved
  cached repository rows when an empty signal-only refresh returns.

macOS:

- Disabled AppKit automatic window tabbing so `Cmd+T` no longer creates
  duplicate macOS-level Aetower window tabs.

Website:

- Updated the public site with early-alpha positioning, Chau7 links, and real
  Storage and Repos captures from the current app.

## 0.74 (build 723) - 2026-07-04

Developer Preview feature release.

Release metadata:

- Bundle identifier: `com.aeptus.aetower`
- Sparkle appcast: `https://aetower.dev/releases/appcast.xml`
- Release archive prefix: `https://aetower.dev/releases/`

Command line tool:

- Added `aetower`, an operator CLI that reads live data from a running
  Aetower over its local MCP socket: `aetower top`, `host`, `storage`,
  `repos`, `alerts`, `findings`, and `doctor`, a generic `aetower call`
  reaching every tool, `--json` on any command for pipelines, `--watch`,
  scriptable exit codes, shell completions, and a man page.
- The CLI ships inside the app bundle, so it arrives through every install
  channel. The signed installer package symlinks it onto your `PATH`
  automatically; disk-image, ZIP, and Homebrew installs can add it from
  Settings → AI Clients ("Install Command Line Tool") or with
  `aetower install`.

Distribution:

- The signed, notarized installer package (`.pkg`) and disk image (`.dmg`)
  are built and published again as part of every release, so the direct
  downloads track the current version instead of an older build.

## 0.73 (build 718) - 2026-07-03

Developer Preview feature release.

Release metadata:

- Bundle identifier: `com.aeptus.aetower`
- Sparkle appcast: `https://aetower.dev/releases/appcast.xml`
- Release archive prefix: `https://aetower.dev/releases/`

Storage:

- Redesigned the Storage tab around reclaiming space: a disk-pressure header,
  clearer reclaim opportunities, a nimble cleanup pill, and a reversible
  Finder-Trash cleanup flow that shows how much space frees when the Trash is
  emptied.
- Made storage relaunch paint instantly from a persisted cache and skip the
  full repository inventory re-walk when nothing changed on disk, removing the
  long "starting scan" delay after every launch.
- Added growth, cold-data, and filesystem-event attribution so reclaim cost is
  learned from writer history.

Repository:

- Reworked the Repository page around repository health: a health-header hero,
  a navigation rail with live signals, bulk multi-select operations, and real
  actions — reversible artifact cleanup, project unlinking, and re-running
  failed GitHub workflows / redeploying Cloudflare Pages from the provider tile.
- Expired and labelled the OpenSSF Scorecard cache so a cached supply-chain
  score is never shown as fresh, and banded per-check outcomes so a strong
  check is no longer reported as "failed".

Metrics & correctness:

- Fixed CPU-wakeups, disk, and network rates to divide by the real elapsed
  interval (they were previously stuck at zero or over-reported), kept metric
  rings updating while a visible window is unfocused, carried hardware sensors
  forward between samples, and summed GPU memory across accelerators.

Reliability & security:

- Hardened token storage to be device-bound, surfaced decode errors instead of
  failing silently, backed up corrupt local stores instead of discarding them,
  fenced untrusted external text before it enters agent prompts, and added a CI
  gate against a stale Rust↔Swift FFI binding.

## 0.54 (build 497) - 2026-06-04

Developer Preview maintenance release.

Release metadata:

- Bundle identifier: `com.aeptus.aetower`
- Sparkle appcast: `https://aetower.dev/releases/appcast.xml`
- Release archive prefix: `https://aetower.dev/releases/`

- Added a merged Activity workspace that combines History and Timeline into a
  single section with Overview, History, Timeline, and Storage modes.
- Added hardened process actions with per-click action IDs, PID identity
  safeguards, target blast-radius metadata, richer command results, and
  follow-up verification for terminate flows.
- Added a guarded privileged-helper process action command path that remains
  opt-in and disabled by default for public builds.
- Added clearer process-action UI feedback, including sent-action state,
  verification outcomes, command stderr/stdout, target outcomes, helper status,
  and restore-previous-priority support.
- Fixed Monitor click responsiveness by preventing ordinary entity/PID
  selection from starting eager process inspections or static analysis spinners.

## 0.53 (build 467) - 2026-06-04

Developer Preview feature release.

Release metadata:

- Bundle identifier: `com.aeptus.aetower`
- Sparkle appcast: `https://aetower.dev/releases/appcast.xml`
- Release archive prefix: `https://aetower.dev/releases/`

- Added a fast, operator-oriented Startup & Persistence view with richer risk
  scoring, change detection, runtime correlation, deep audit support, and safe
  reveal/copy/focus actions.
- Added configurable Monitor metric-ring placement for top, bottom, left, or
  right layouts.
- Added Monitor ring focus controls for top and bottom layouts, including
  full-width focused graphs and inline pin/unpin controls.
- Added stable seven-ring Monitor presentation by keeping GPU visible even when
  GPU activity is currently idle.
- Added release/support metadata for Monitor metric-ring placement and focus
  settings.

## 0.52 (build 462) - 2026-06-03

Developer Preview feature release.

Release metadata:

- Bundle identifier: `com.aeptus.aetower`
- Sparkle appcast: `https://aetower.dev/releases/appcast.xml`
- Release archive prefix: `https://aetower.dev/releases/`

- Added a redesigned public website with a cleaner visual system, real Monitor
  screenshot, PKG-first install flow, and project/author links.
- Added reverse resource pivot tools to show which process holds a file, port,
  or socket, including MCP-facing reports and UI filtering for open resources.
- Added startup and persistence inspection for login items, launch agents,
  launch daemons, and related persistence surfaces.
- Added richer code-signing classification with amortized full-coverage scans.
- Added per-agent budget rules, alerting, timeline notifications, per-category
  toggles, and snooze support.
- Added actionable recommendation flows so users and agents can act on concrete
  process guidance rather than generic friction text.
- Added thermal throttle forecasting and contributor attribution.
- Added slow-regression detection foundations, including app-version capture
  and off-tick detector wiring.
- Added energy translation for battery time, estimated cost, and carbon impact.
- Added observability documentation, a metric reference, and a Grafana dashboard.

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
