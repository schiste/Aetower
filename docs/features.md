# Aetower Feature List

Current shipped product surfaces, technical capabilities, and maturity notes.

Aetower is a local-first macOS observability and operator dashboard. It helps a
technical user, developer, or AI agent understand what is making a Mac slow,
hot, noisy, memory pressured, storage constrained, or expensive to run.

Updated: 2026-07-14
Applies to: Developer Preview 0.8.1

This document is the product feature inventory. For product direction and
priorities, read [Product Direction](product-direction.md). For navigation, read
[Tab-by-Tab Guide](tabs-guide.md).

## Feature Overview

| Area | Human-readable value | Technical capability |
| --- | --- | --- |
| Monitor | See what is hurting the Mac right now. | Groups processes into entities, ranks them by friction, and shows host pressure. |
| Friction scoring | Replace raw counters with one triage score. | Combines CPU, memory, disk, network, wakeups, energy, thermal, and anomaly signals. |
| Entity detail and actions | Understand what an item is and what action is safe. | Shows process membership, launcher/origin context, trends, recommendations, and guarded action previews. |
| Activity | Reconstruct incidents over time. | Combines Overview, History, Timeline, and history-store health. |
| Storage | Free space without lying about reclaimability. | Separates Reclaim, Similar, Explore, Audit, and Insights; distinguishes logical bytes from APFS/local estimates. |
| Repos | See local workspace truth. | Uses cached inventory, fingerprints, Git state, storage footprint, agent contracts, AI spend, and provider context. |
| Projects | Link local repos to provider surfaces. | Connects GitHub and Cloudflare project status to local repository records. |
| Agents | Understand local AI work. | Tracks Chau7 sessions, AI-agent/runtime burden, estimated energy/cost, and source/confidence badges. |
| Local MCP and CLI | Let trusted local tools query Aetower. | Ships a socket-backed MCP server, stdio helper, and `aetower` CLI commands. |
| Diagnostics | Debug Aetower itself. | Exposes subsystem events, pipeline timing, MCP state, payload sizes, memory, and support bundles. |
| System and Fleet | Inspect machine, startup, and peer state. | Shows sensors, startup inventory, diagnostics, and opt-in Bonjour peer discovery. |
| Settings and privacy | Make outbound behavior visible and deliberate. | Controls collection, integrations, MCP registration, CLI status, telemetry, Fleet, VirusTotal, and resets. |
| Release validation | Keep public claims honest. | Validates claims against code/config/artifacts before website deploy. |

## 1. Monitor

### What it does for people

Monitor is the first place to look when the machine feels bad. Instead of
forcing the user to scan hundreds of raw processes, Aetower groups related
processes into understandable entities such as an app, daemon family, terminal
session, local AI runtime, or browser group.

### Technical details

- Samples host and process-level runtime signals.
- Groups raw processes into higher-level entities using identity, origin,
  runtime, repository, workspace, and launcher context where available.
- Presents ranked entities and drill-down detail instead of only process IDs.
- Supports guarded process-action previews through explicit operator surfaces.

## 2. Friction Scoring

### What it does for people

Friction is Aetower's shorthand for "how much this thing is costing the machine
right now." A high friction score can come from CPU, memory pressure, wakeups,
disk churn, network activity, energy impact, thermal contribution, or unusual
change from recent behavior.

### Technical details

- Produces a 0-100 score for triage and ranking.
- Separates host pressure from per-entity contribution.
- Feeds recommendations, anomaly explanations, trend cards, timeline events,
  and top-findings surfaces.
- Keeps scoring explainable from collected signals.

## 3. Entity Detail, Provenance, And Action Planning

### What it does for people

When an entity looks suspicious, detail views explain what it is, which
processes belong to it, why it is ranked highly, and which actions are safe or
reasonable.

### Technical details

- Shows entity process membership and tree relationships.
- Uses process origin metadata to explain launch paths and runtime context.
- Provides recommendations based on dominant burden signals and environment
  context such as battery or thermal pressure.
- Keeps actions preview- and approval-gated.

## 4. Activity

### What it does for people

Activity answers time-based questions: what changed, what happened earlier, and
what sequence of events led to an incident.

### Technical details

- Overview summarizes recent changes, event pressure, history coverage, and
  load state.
- History persists seven days of local entity snapshots by default under normal
  budget and exposes paged historical access.
- Timeline correlates lifecycle events, host changes, sensor alerts,
  restart-loop signals, and AI-session markers.
- Storage reports history DB/WAL size, retention coverage, quarantine, and
  maintenance state.

## 5. Storage

### What it does for people

Storage helps the operator reclaim disk space and understand growth without
overstating safety or reclaimable bytes.

### Technical details

- Provides Quick, Complete, Forensic, and Custom scan profiles.
- Reclaim focuses on quick wins, duplicate review, old screenshots, staging,
  and move-to-Trash flows.
- Similar is a first-class redundancy page for exact duplicates, similar
  images, docs/text, videos, binaries, and other redundancy.
- Explore handles indexed browsing, visual full-disk/treemap/table views, cold
  data, optimization leads, and raw artifacts.
- Audit records local cleanup actions.
- Separates logical bytes from local APFS estimates, hardlink dedupe, sparse
  files, cloud placeholders, purgeable capacity, and clone-lineage caveats.

## 6. Repos

### What it does for people

Repos shows which local repositories are active, stale, dirty, duplicated,
growing, missing agent contracts, linked to AI sessions, or carrying large
rebuildable artifacts.

### Technical details

- Opens from cached repository inventory.
- Uses Git/index/config fingerprints to mark stale repos without hiding them.
- Sorts by attention, size, growth, artifacts, AI spend, or name.
- Shows detail tabs for Actions, Storage, Contracts, Scorecard, Git, and Live.
- Surfaces agent-contract readiness for `AGENTS.md` and `.agents/*.yaml`.
- Includes estimated per-repo cost, energy, and carbon only when attribution is
  strong enough, with source/confidence labels.

## 7. Projects

### What it does for people

Projects maps local repositories to provider status so a user can see the
outside-world context for a local workspace.

### Technical details

- Stores local project records keyed to repository roots.
- Supports GitHub and Cloudflare connectors.
- Surfaces pull requests, workflow runs, checks, and Cloudflare deployment
  status when credentials and links are configured.
- Keeps provider configuration deliberate through Settings and connector cards.

## 8. Agents

### What it does for people

Agents makes the cost of local AI work visible: which session is active, which
runtime is heavy, what repo is involved, and how much energy or money was
estimated.

### Technical details

- Detects and groups common local AI tools and model runtimes.
- Integrates optional Chau7 context for sessions, terminal tabs, repositories,
  branches, active apps, and cost data.
- Shows GPU and unified-memory source badges such as "GPU inferred" rather than
  claiming exact per-process VRAM.
- Accumulates session and repo cost through the shared resource-cost model when
  attribution is available.

## 9. Local MCP And CLI

### What it does for people

Trusted local AI agents and shell scripts can ask Aetower what is happening on
the machine without starting a second monitor.

### Technical details

- Runs an app-owned local MCP server when Aetower is active.
- Uses a local Unix socket and packaged stdio helper for supported clients.
- Offers one-click registration for supported Claude and Codex clients;
  automatic registration remains off by default.
- Exposes snapshot, host, entity, history, diagnostics, storage, repository,
  recommendation, cost, and support-bundle tools.
- Operator actions are visible by default for trusted local clients, hideable in
  Settings, and still preview- and approval-gated.
- Ships an `aetower` CLI with `top`, `storage`, `repos`, `alerts`, `findings`,
  `doctor`, `tab`, and `call` workflows.

## 10. Diagnostics

### What it does for people

Diagnostics explains whether Aetower itself is healthy before the user files an
issue, exports a support bundle, or investigates why a sensor, adapter,
database, or collection path is unavailable.

### Technical details

- Tracks subsystem events for engine, collector, adapters, persistence, history,
  MCP, UI payloads, and export paths.
- Surfaces pipeline timing and payload pressure.
- Reports capability state and permission/adapter availability.
- Provides support-bundle previews and diagnostics summaries.

## 11. System And Fleet

### What it does for people

System covers machine state, startup state, Aetower health, and optional local
peer discovery.

### Technical details

- Sensors show thermals, fans, power, storage, battery, Bluetooth, and per-core
  load.
- Startup scans login items, launch agents/daemons, active-now links, scanned
  locations, and attention items.
- Fleet uses remembered opt-in local-network discovery and shows nearby Macs
  running Aetower with Fleet enabled.
- Peer data is local-network oriented and not a cloud fleet service.

## 12. Settings, Privacy, And Runtime Tuning

### What it does for people

Settings lets users choose how much detail Aetower collects and exactly when
data can leave the Mac.

### Technical details

- Controls startup checklist, appearance, collection cadence, repository roots,
  integrations, AI-client registration, notifications, automation, updates, and
  advanced reset/support operations.
- Shows Outbound Data status for telemetry, Fleet, VirusTotal, provider tokens,
  and MCP clients.
- Keeps telemetry, Fleet, VirusTotal, automatic MCP registration, scheduled
  storage scans, and privileged helper behavior off by default.
- Makes CLI readiness visible in setup and AI-client flows.

## 13. Export And External Observability

### What it does for people

Teams with OpenTelemetry, Prometheus, Grafana, or internal observability systems
can optionally export selected metrics.

### Technical details

- Supports optional OTLP/HTTP metric export.
- Documents collector and dashboard setup.
- Keeps export opt-in so local machine metadata is not sent accidentally.

## 14. Release Validation

### What it does for people

The public site should not claim a feature that the app, release scripts, or
published artifacts cannot support.

### Technical details

- Validates history retention days, MCP operator visibility/hiding, outbound
  defaults, Fleet defaults, appcast metadata, release artifacts, source
  archives, CLI bundling, Homebrew cask, and published URLs.
- Runs before website deploys.
- Keeps changelog and website language tied to release facts.

## Feature Maturity Notes

Aetower is a Developer Preview. The Monitor, Activity, Storage, Repos,
Diagnostics, Settings, local MCP, CLI, and release-validation workflows are
primary product surfaces. Some integrations depend on local permissions,
provider credentials, Chau7 availability, supported AI clients, or release build
configuration. Optional helpers, exports, Fleet, VirusTotal, and provider
lookups are deliberately configurable instead of always-on behavior.
